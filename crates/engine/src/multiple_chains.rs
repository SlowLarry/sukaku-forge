use std::cell::OnceCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use sukaku_forge_core::{
    CandidateMask, CandidateRemovals, CandidateRemovalsBuilder, CellId, CellMask, Digit, Grid,
    PositionMask, REGION_TYPE_COUNT, RegionId, VariantConfig,
};

use crate::aligned_exclusion::find_aligned_triplet_exclusion;
use crate::alphabet_wings::collect_alphabet_wing_advanced;
use crate::bug::find_bivalue_universal_grave;
use crate::forcing_chains::{
    Implications, KEY_COUNT, active_region_types, collect_forcing_chain_proofs,
    collect_forcing_chain_proofs_se121, decode_candidate, is_on, potential_key,
};
use crate::nested_chains::{
    ChainProof, FullChainFingerprint, InferenceCollector, NestedHint, NestedHintCollector, OnCause,
    ProofArena, ProofKind, ProofNode, ProofTarget,
};
use crate::presentation_proof::{
    ChainCause, ChainNodeId, ChainProofNode, ChainProofParent, ChainProofView, ChainProofViewKind,
    ChainState, MultipleForcingChainWithProof, SelectedChainProof,
};
use crate::unique_loops::find_unique_loop;
use crate::{EngineConfig, Evidence, Inference, MultipleChainKind, Rating, Technique};

const NO_NODE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MultiMode {
    Static,
    Dynamic,
    DynamicPlus,
    /// SE 1.2.1 level-one dynamic chaining, whose embedded Locking rule uses
    /// the older region-before-digit traversal.
    Se121DynamicPlus,
    Nested {
        level: u8,
        nesting_limit: u8,
    },
    /// SE 1.2.1/serate nested schedule, with selected later search fixes.
    /// Unlike later Sukaku Explainer releases, this has one producer at each
    /// level 2 through 5.
    Se121Nested {
        level: u8,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdvancedTargetOrder {
    CoordinateHash,
    CellThenDigit,
}

#[cfg(test)]
std::thread_local! {
    /// Lets regression tests run the real SE121 search entry points with only
    /// the post-1.2.1 target-order correction disabled.
    static APPLY_SE121_ORDER_CORRECTION: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    /// Full scanning remains available only for exact A/B regression tests.
    /// Production always uses the corrected-SE121 delta path when eligible.
    static APPLY_SE121_DELTA_SCAN: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    static FIRST_DRAFT_OFFERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static RANKED_TARGET_MATERIALIZATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Exact mutable-grid identity for inner-chain memoization. Topology and
/// engine configuration are fixed for the lifetime of one outer search, so
/// each slot needs only its value and current candidate mask.
#[derive(Clone, Eq, Hash, PartialEq)]
struct GridStateKey([u16; 81]);

impl GridStateKey {
    fn new(grid: &Grid) -> Self {
        Self(std::array::from_fn(|index| {
            let cell = CellId::new(u8::try_from(index).expect("grid-state cell index"))
                .expect("grid-state cell");
            (u16::from(grid.value(cell)) << 10) | grid.candidates(cell).bits()
        }))
    }
}

type CachedInnerResult = Result<Arc<[NestedHint]>, LegacyFcPlusBoundary>;

/// Per-outer-search exact-state caches for the expensive inner chaining
/// families. Each family has its own namespace, and cached slices retain the
/// producer's exact discovery/ranking order. Maps are used for lookup only;
/// their iteration order can never affect a published hint.
#[derive(Default)]
struct InnerChainCache {
    forcing: HashMap<GridStateKey, Arc<[NestedHint]>>,
    /// Exact SE121 inner-FCC states known to have no hint. Unlike positive
    /// proof slices, these compact keys are safe to retain for the lifetime
    /// of one rating session and across outer producer scopes.
    se121_forcing_negative: HashSet<GridStateKey>,
    #[cfg(test)]
    se121_forcing_computations: usize,
    #[cfg(test)]
    se121_forcing_negative_hits: usize,
    multiple: HashMap<GridStateKey, Arc<[NestedHint]>>,
    dynamic: HashMap<GridStateKey, CachedInnerResult>,
    dynamic_plus: HashMap<GridStateKey, CachedInnerResult>,
    nested_two: HashMap<GridStateKey, CachedInnerResult>,
    nested_three: HashMap<GridStateKey, CachedInnerResult>,
}

impl InnerChainCache {
    /// Drop proof-bearing results between outer producer scopes while
    /// retaining the maps' allocation and exact-state negative FCC entries.
    fn clear_local_results(&mut self) {
        self.forcing.clear();
        self.multiple.clear();
        self.dynamic.clear();
        self.dynamic_plus.clear();
        self.nested_two.clear();
        self.nested_three.clear();
    }

    fn forcing(&mut self, grid: &Grid, config: EngineConfig) -> Arc<[NestedHint]> {
        self.forcing_impl(grid, config, false)
    }

    fn forcing_se121(&mut self, grid: &Grid, config: EngineConfig) -> Arc<[NestedHint]> {
        self.forcing_impl(grid, config, true)
    }

    fn forcing_impl(
        &mut self,
        grid: &Grid,
        config: EngineConfig,
        use_root_prefilter: bool,
    ) -> Arc<[NestedHint]> {
        let key = GridStateKey::new(grid);
        if use_root_prefilter && self.se121_forcing_negative.contains(&key) {
            #[cfg(test)]
            {
                self.se121_forcing_negative_hits += 1;
            }
            return Arc::default();
        }
        if let Some(hints) = self.forcing.get(&key) {
            return Arc::clone(hints);
        }
        #[cfg(test)]
        if use_root_prefilter {
            self.se121_forcing_computations += 1;
        }
        let collected = if use_root_prefilter {
            collect_forcing_chain_proofs_se121(grid, config)
        } else {
            collect_forcing_chain_proofs(grid, config)
        };
        if use_root_prefilter && collected.is_empty() {
            self.se121_forcing_negative.insert(key);
            return Arc::default();
        }
        let hints = Arc::<[NestedHint]>::from(collected);
        self.forcing.insert(key, Arc::clone(&hints));
        hints
    }

    fn multiple(&mut self, grid: &Grid, config: EngineConfig) -> Arc<[NestedHint]> {
        let key = GridStateKey::new(grid);
        if let Some(hints) = self.multiple.get(&key) {
            return Arc::clone(hints);
        }
        let hints = Arc::<[NestedHint]>::from(collect_multiple_chain_proofs(grid, config));
        self.multiple.insert(key, Arc::clone(&hints));
        hints
    }

    fn multi(&mut self, grid: &Grid, config: EngineConfig, mode: MultiMode) -> CachedInnerResult {
        let cache = match mode {
            MultiMode::Dynamic => &mut self.dynamic,
            MultiMode::DynamicPlus => &mut self.dynamic_plus,
            MultiMode::Nested { level: 2, .. } | MultiMode::Se121Nested { level: 2 } => {
                &mut self.nested_two
            }
            MultiMode::Nested { level: 3, .. } | MultiMode::Se121Nested { level: 3 } => {
                &mut self.nested_three
            }
            MultiMode::Static
            | MultiMode::Se121DynamicPlus
            | MultiMode::Nested { .. }
            | MultiMode::Se121Nested { .. } => {
                unreachable!("unsupported cached inner multi-chain mode")
            }
        };
        let key = GridStateKey::new(grid);
        if let Some(hints) = cache.get(&key) {
            return hints.clone();
        }
        let hints =
            collect_multi_chain_proofs_for_mode(grid, config, mode).map(Arc::<[NestedHint]>::from);
        cache.insert(key, hints.clone());
        hints
    }
}

impl MultiMode {
    fn is_dynamic(self) -> bool {
        !matches!(self, Self::Static)
    }

    fn technique(self) -> Technique {
        match self {
            Self::Static => Technique::MultipleForcingChain,
            Self::Dynamic => Technique::DynamicForcingChain,
            Self::DynamicPlus | Self::Se121DynamicPlus => Technique::DynamicForcingChainPlus,
            Self::Nested { .. } | Self::Se121Nested { .. } => Technique::NestedForcingChain,
        }
    }

    fn base_rating(self) -> (u16, f64) {
        match self {
            Self::Static => (80, 8.0),
            Self::Dynamic => (85, 8.5),
            Self::DynamicPlus | Self::Se121DynamicPlus => (90, 9.0),
            Self::Nested { level, .. } | Self::Se121Nested { level } => {
                debug_assert!((2..=5).contains(&level));
                (85 + u16::from(level) * 5, 8.5 + f64::from(level) * 0.5)
            }
        }
    }

    fn level(self) -> u8 {
        match self {
            Self::Static | Self::Dynamic => 0,
            Self::DynamicPlus | Self::Se121DynamicPlus => 1,
            Self::Nested { level, .. } | Self::Se121Nested { level } => level,
        }
    }

    fn nesting_limit(self) -> u8 {
        match self {
            Self::Nested { nesting_limit, .. } => nesting_limit,
            Self::Static
            | Self::Dynamic
            | Self::DynamicPlus
            | Self::Se121DynamicPlus
            | Self::Se121Nested { .. } => 0,
        }
    }

    const fn uses_se121_locking_order(self) -> bool {
        matches!(self, Self::Se121DynamicPlus | Self::Se121Nested { .. })
    }

    const fn is_se121_search(self) -> bool {
        matches!(self, Self::Se121DynamicPlus | Self::Se121Nested { .. })
    }

    const fn uses_corrected_se121_pruning(self) -> bool {
        self.is_se121_search()
    }

    fn uses_corrected_se121_order(self) -> bool {
        if !self.is_se121_search() {
            return false;
        }
        #[cfg(test)]
        {
            APPLY_SE121_ORDER_CORRECTION.with(std::cell::Cell::get)
        }
        #[cfg(not(test))]
        true
    }

    /// The corrected rater continues past an advanced family whose removals
    /// were already known to the branch. Java and the general compatibility
    /// modes retain the historical "hint found" stopping rule, which can
    /// prematurely prune later inner families.
    fn advanced_family_stops(self, scan: AdvancedScan) -> bool {
        if scan.boundary.is_some() {
            true
        } else if self.uses_corrected_se121_pruning() {
            scan.added
        } else {
            scan.productive
        }
    }

    fn advanced_target_order(self) -> AdvancedTargetOrder {
        if self.uses_corrected_se121_order() {
            AdvancedTargetOrder::CellThenDigit
        } else {
            AdvancedTargetOrder::CoordinateHash
        }
    }
}

/// Delta invalidation is deliberately narrower than the general compatibility
/// engine. Its proof relies on corrected SE121 stopping on newly added OFFs,
/// monotone candidate removal, the four-family FCPlus=0 schedule, and classic
/// row/column/block geometry.
fn uses_se121_delta_advanced_scan(grid: &Grid, config: EngineConfig, mode: MultiMode) -> bool {
    if !mode.is_se121_search()
        || config.forcing_chain_plus != 0
        || grid.topology().config() != VariantConfig::default()
    {
        return false;
    }
    #[cfg(test)]
    {
        APPLY_SE121_DELTA_SCAN.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    true
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct AdvancedScan {
    productive: bool,
    added: bool,
    boundary: Option<LegacyFcPlusBoundary>,
}

impl AdvancedScan {
    const fn at_boundary(boundary: LegacyFcPlusBoundary) -> Self {
        Self {
            productive: true,
            added: false,
            boundary: Some(boundary),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyFcPlusBoundary {
    AlignedTripletExclusion,
    UniqueLoops,
    BivalueUniversalGrave,
}

#[derive(Clone)]
struct Node {
    key: u16,
    parent_start: u32,
    parent_count: u16,
    on_cause: OnCause,
    nested: Option<Arc<ChainProof>>,
}

/// Multi-parent implication graph shared by static Multiple and level-0
/// Dynamic chains. Static nodes simply retain one parent.
struct Arena {
    nodes: Vec<Node>,
    parents: Vec<u32>,
    ancestor_stamps: Box<[u16; KEY_COUNT]>,
    ancestor_generation: u16,
    traversal: Vec<u32>,
    proof_cache: OnceCell<Arc<ProofArena>>,
}

impl Arena {
    fn new() -> Self {
        Self {
            nodes: Vec::with_capacity(192),
            parents: Vec::with_capacity(256),
            ancestor_stamps: Box::new([0; KEY_COUNT]),
            ancestor_generation: 0,
            traversal: Vec::with_capacity(96),
            proof_cache: OnceCell::new(),
        }
    }

    fn clear(&mut self) {
        self.proof_cache.take();
        self.nodes.clear();
        self.parents.clear();
    }

    fn root(&mut self, key: u16) -> u32 {
        self.push(key, &[])
    }

    fn child(&mut self, key: u16, parent: u32) -> u32 {
        self.push(key, &[parent])
    }

    fn child_with_cause(&mut self, key: u16, parent: u32, on_cause: OnCause) -> u32 {
        self.push_detailed(key, &[parent], on_cause, None)
    }

    fn push(&mut self, key: u16, parents: &[u32]) -> u32 {
        self.push_detailed(key, parents, OnCause::None, None)
    }

    fn push_nested(&mut self, key: u16, parents: &[u32], nested: Arc<ChainProof>) -> u32 {
        self.push_detailed(key, parents, OnCause::None, Some(nested))
    }

    fn push_detailed(
        &mut self,
        key: u16,
        parents: &[u32],
        on_cause: OnCause,
        nested: Option<Arc<ChainProof>>,
    ) -> u32 {
        self.proof_cache.take();
        let node = u32::try_from(self.nodes.len()).expect("multiple-chain node index");
        let parent_start = u32::try_from(self.parents.len()).expect("multiple-chain parent index");
        let parent_count = u16::try_from(parents.len()).expect("multiple-chain parent count");
        self.parents.extend_from_slice(parents);
        self.nodes.push(Node {
            key,
            parent_start,
            parent_count,
            on_cause,
            nested,
        });
        node
    }

    /// Hidden parents are appended only to the node most recently built.
    fn add_parent(&mut self, node: u32, parent: u32) {
        self.proof_cache.take();
        let index = usize::try_from(node).expect("multiple-chain node index");
        debug_assert_eq!(index + 1, self.nodes.len());
        let entry = &mut self.nodes[index];
        debug_assert_eq!(
            usize::try_from(entry.parent_start).expect("parent start")
                + usize::from(entry.parent_count),
            self.parents.len()
        );
        self.parents.push(parent);
        entry.parent_count = entry
            .parent_count
            .checked_add(1)
            .expect("multiple-chain parent count");
    }

    fn key(&self, node: u32) -> u16 {
        self.nodes[usize::try_from(node).expect("multiple-chain node index")].key
    }

    fn node(&self, node: u32) -> &Node {
        &self.nodes[usize::try_from(node).expect("multiple-chain node index")]
    }

    fn parent_range(&self, node: u32) -> std::ops::Range<usize> {
        let entry = self.node(node);
        let start = usize::try_from(entry.parent_start).expect("parent start");
        start..start + usize::from(entry.parent_count)
    }

    fn parents(&self, node: u32) -> &[u32] {
        &self.parents[self.parent_range(node)]
    }

    /// Java de-duplicates ancestry by potential key for each branch target.
    fn ancestor_count(&mut self, terminal: u32) -> u16 {
        self.ancestor_generation = self.ancestor_generation.wrapping_add(1);
        if self.ancestor_generation == 0 {
            self.ancestor_stamps.fill(0);
            self.ancestor_generation = 1;
        }
        let generation = self.ancestor_generation;
        self.traversal.clear();
        self.traversal.push(terminal);
        let mut result = 0_u16;
        while let Some(node) = self.traversal.pop() {
            let key = usize::from(self.key(node));
            if self.ancestor_stamps[key] == generation {
                continue;
            }
            self.ancestor_stamps[key] = generation;
            result = result.checked_add(1).expect("multiple-chain complexity");
            let parents = self.parent_range(node);
            self.traversal.extend_from_slice(&self.parents[parents]);
        }
        result
    }

    fn proof_arena(&self) -> Arc<ProofArena> {
        Arc::clone(self.proof_cache.get_or_init(|| {
            Arc::new(ProofArena::new(
                self.nodes
                    .iter()
                    .map(|node| ProofNode {
                        key: node.key,
                        parent_start: node.parent_start,
                        parent_count: node.parent_count,
                        on_cause: node.on_cause,
                        nested: node.nested.clone(),
                    })
                    .collect(),
                self.parents.clone(),
            ))
        }))
    }
}

/// Recursive Java complexity directly over live branch arenas. Public
/// first-hint ranking needs this scalar only; it must not freeze a full proof
/// graph for every outer candidate.
fn complete_proof_complexity(targets: &[(&Arena, u32)]) -> u32 {
    let mut result = 0_u32;
    for &(arena, terminal) in targets {
        let mut seen = [false; KEY_COUNT];
        let mut pending = vec![terminal];
        while let Some(node) = pending.pop() {
            let key = arena.key(node);
            if seen[usize::from(key)] {
                continue;
            }
            seen[usize::from(key)] = true;
            result = result.checked_add(1).expect("complete flat complexity");
            pending.extend_from_slice(&arena.parents[arena.parent_range(node)]);
        }
    }

    let mut processed: HashSet<&FullChainFingerprint> = HashSet::new();
    for &(arena, terminal) in targets {
        let mut seen = [false; KEY_COUNT];
        let mut pending = vec![terminal];
        while let Some(node) = pending.pop() {
            let entry = &arena.nodes[usize::try_from(node).expect("multiple-chain node index")];
            if seen[usize::from(entry.key)] {
                continue;
            }
            seen[usize::from(entry.key)] = true;
            if let Some(nested) = &entry.nested
                && processed.insert(nested.fingerprint())
            {
                result = result
                    .checked_add(nested.complexity())
                    .expect("complete nested complexity");
            }
            pending.extend_from_slice(&arena.parents[arena.parent_range(node)]);
        }
    }
    result
}

/// Insertion order plus direct key lookup for one ON or OFF consequence set.
struct ConsequenceSet {
    node_by_key: Box<[u32; KEY_COUNT]>,
    touched_keys: Vec<u16>,
    order: Vec<u32>,
}

impl ConsequenceSet {
    fn new() -> Self {
        Self {
            node_by_key: Box::new([NO_NODE; KEY_COUNT]),
            touched_keys: Vec::with_capacity(192),
            order: Vec::with_capacity(192),
        }
    }

    fn clear(&mut self) {
        for key in self.touched_keys.drain(..) {
            self.node_by_key[usize::from(key)] = NO_NODE;
        }
        self.order.clear();
    }

    fn add_if_absent(&mut self, arena: &Arena, node: u32) -> bool {
        let key = arena.key(node);
        let slot = &mut self.node_by_key[usize::from(key)];
        if *slot != NO_NODE {
            return false;
        }
        *slot = node;
        self.touched_keys.push(key);
        self.order.push(node);
        true
    }

    fn node(&self, key: u16) -> u32 {
        self.node_by_key[usize::from(key)]
    }

    fn contains(&self, key: u16) -> bool {
        self.node(key) != NO_NODE
    }
}

const SE121_ADVANCED_FAMILY_COUNT: usize = 4;

/// Independent invalidation cursors are required because Java's advanced
/// ladder stops after the first family that adds an implication. A later
/// family must retain every removal made since *it* was last reached, not just
/// the removals made since the preceding advanced pass.
#[derive(Clone, Copy)]
#[repr(usize)]
enum Se121AdvancedFamily {
    Locking = 0,
    HiddenPair = 1,
    NakedPair = 2,
    XWing = 3,
}

/// Compact removal summary for one family's pending event range.
///
/// Scanners still traverse their canonical Java unit order. These masks are
/// predicates only; removal discovery order must never become hint order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct AdvancedDelta {
    all_cells: CellMask,
    cells_by_digit: [CellMask; 10],
}

impl AdvancedDelta {
    fn from_removed_keys(keys: &[u16]) -> Self {
        let mut result = Self::default();
        for &key in keys {
            debug_assert!(!is_on(key));
            let (cell, digit) = decode_candidate(key);
            result.all_cells.insert(cell);
            result.cells_by_digit[usize::from(digit.get())].insert(cell);
        }
        result
    }

    fn is_empty(&self) -> bool {
        self.all_cells.is_empty()
    }

    fn affects_region(&self, grid: &Grid, region: RegionId) -> bool {
        !self
            .all_cells
            .intersect(grid.topology().region_mask(region))
            .is_empty()
    }

    fn affects_region_digit(&self, grid: &Grid, region: RegionId, digit: Digit) -> bool {
        !self.cells_by_digit[usize::from(digit.get())]
            .intersect(grid.topology().region_mask(region))
            .is_empty()
    }
}

/// Candidate-removal rollback journal for one dynamic branch closure.
struct DynamicState {
    changed_cells: Vec<CellId>,
    /// Successful removals in propagation order. Family cursors index this
    /// capacity-reused log; the events themselves are never iterated as scan
    /// units, so they cannot perturb Java discovery order.
    removed_keys: Vec<u16>,
    track_removed_keys: bool,
    original_masks: [CandidateMask; 81],
    changed: [bool; 81],
    removed_nodes: [u32; 81 * 9],
}

impl DynamicState {
    fn new() -> Self {
        Self {
            changed_cells: Vec::with_capacity(32),
            // General compatibility and GUI searches never allocate this
            // headless-only scratch.
            removed_keys: Vec::new(),
            track_removed_keys: false,
            original_masks: [CandidateMask::EMPTY; 81],
            changed: [false; 81],
            removed_nodes: [NO_NODE; 81 * 9],
        }
    }

    fn begin(&mut self, track_removed_keys: bool) {
        debug_assert!(self.changed_cells.is_empty());
        debug_assert!(self.removed_keys.is_empty());
        self.track_removed_keys = track_removed_keys;
        if track_removed_keys && self.removed_keys.capacity() == 0 {
            self.removed_keys.reserve(96);
        }
    }

    fn remove(&mut self, grid: &mut Grid, key: u16, node: u32) {
        debug_assert!(!is_on(key));
        let (cell, digit) = decode_candidate(key);
        if !grid.candidates(cell).contains(digit) {
            return;
        }
        if !self.changed[cell.index()] {
            self.changed[cell.index()] = true;
            self.original_masks[cell.index()] = grid.candidates(cell);
            self.changed_cells.push(cell);
        }
        self.removed_nodes[candidate_index(cell, digit)] = node;
        grid.remove_candidate(cell, digit);
        if self.track_removed_keys {
            self.removed_keys.push(key);
        }
    }

    fn original_mask(&self, grid: &Grid, cell: CellId) -> CandidateMask {
        if self.changed[cell.index()] {
            self.original_masks[cell.index()]
        } else {
            grid.candidates(cell)
        }
    }

    fn removed_mask(&self, grid: &Grid, cell: CellId) -> CandidateMask {
        self.original_mask(grid, cell)
            .without(grid.candidates(cell))
    }

    fn removed_node(&self, cell: CellId, digit: Digit) -> u32 {
        self.removed_nodes[candidate_index(cell, digit)]
    }

    fn restore(&mut self, grid: &mut Grid) {
        for &cell in &self.changed_cells {
            grid.set_candidates(cell, self.original_masks[cell.index()]);
            self.changed[cell.index()] = false;
        }
        self.changed_cells.clear();
        self.removed_keys.clear();
        self.track_removed_keys = false;
    }
}

#[derive(Clone, Copy)]
struct Contradiction {
    on: u32,
    off: u32,
}

/// Coordinate-hash bucket order used by the later compatibility engine.
///
/// The optimized Java implementation hashes `Cell` by its raw index before
/// lazily materializing this compatibility map. General compatibility modes
/// preserve that traversal. Corrected SE121 modes use the later bug-fix order
/// (cell index, then digit) instead.
fn coordinate_hash_map_cell_order(cells: &[CellId], result: &mut Vec<CellId>) {
    let mut capacity = 16_usize;
    while cells.len() > capacity * 3 / 4 {
        capacity *= 2;
    }
    result.clear();
    for bucket in 0..capacity {
        for &cell in cells {
            if cell.index() & (capacity - 1) == bucket {
                result.push(cell);
            }
        }
    }
}

fn order_advanced_target_cells(
    cells: &[CellId],
    order: AdvancedTargetOrder,
    result: &mut Vec<CellId>,
) {
    match order {
        AdvancedTargetOrder::CoordinateHash => coordinate_hash_map_cell_order(cells, result),
        AdvancedTargetOrder::CellThenDigit => {
            result.clear();
            result.extend_from_slice(cells);
            result.sort_unstable_by_key(|cell| cell.index());
        }
    }
}

/// Reusable graph and frontiers for one source assumption.
struct Branch {
    arena: Arena,
    to_on: ConsequenceSet,
    to_off: ConsequenceSet,
    pending_on: VecDeque<u32>,
    pending_off: VecDeque<u32>,
    generated_keys: Vec<u16>,
    generated_causes: Vec<OnCause>,
    generated_nodes: Vec<u32>,
    strong_cell_stamps: [u16; 81],
    strong_generation: u16,
    advanced_parents: Vec<u32>,
    advanced_target_masks: [CandidateMask; 81],
    advanced_target_cells: Vec<CellId>,
    advanced_target_order: Vec<CellId>,
    advanced_target_policy: AdvancedTargetOrder,
    advanced_nested: Option<Arc<ChainProof>>,
    se121_advanced_family_cursors: [usize; SE121_ADVANCED_FAMILY_COUNT],
}

impl Branch {
    fn new() -> Self {
        Self {
            arena: Arena::new(),
            to_on: ConsequenceSet::new(),
            to_off: ConsequenceSet::new(),
            pending_on: VecDeque::with_capacity(96),
            pending_off: VecDeque::with_capacity(96),
            generated_keys: Vec::with_capacity(32),
            generated_causes: Vec::with_capacity(32),
            generated_nodes: Vec::with_capacity(16),
            strong_cell_stamps: [0; 81],
            strong_generation: 0,
            advanced_parents: Vec::with_capacity(24),
            advanced_target_masks: [CandidateMask::EMPTY; 81],
            advanced_target_cells: Vec::with_capacity(32),
            advanced_target_order: Vec::with_capacity(32),
            advanced_target_policy: AdvancedTargetOrder::CoordinateHash,
            advanced_nested: None,
            se121_advanced_family_cursors: [0; SE121_ADVANCED_FAMILY_COUNT],
        }
    }

    fn clear(&mut self) {
        self.arena.clear();
        self.to_on.clear();
        self.to_off.clear();
        self.pending_on.clear();
        self.pending_off.clear();
        self.generated_keys.clear();
        self.generated_causes.clear();
        self.generated_nodes.clear();
        self.clear_advanced_hint();
        self.se121_advanced_family_cursors.fill(0);
    }

    fn take_se121_advanced_delta(
        &mut self,
        state: &DynamicState,
        family: Se121AdvancedFamily,
    ) -> AdvancedDelta {
        let end = state.removed_keys.len();
        let cursor = &mut self.se121_advanced_family_cursors[family as usize];
        debug_assert!(*cursor <= end, "advanced-family removal cursor");
        let result = AdvancedDelta::from_removed_keys(&state.removed_keys[*cursor..end]);
        *cursor = end;
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        &mut self,
        working: &mut Grid,
        implications: &Implications,
        region_types: &[usize],
        state: &mut DynamicState,
        inner_cache: &mut InnerChainCache,
        mode: MultiMode,
        config: EngineConfig,
        source_cell: CellId,
        source_digit: Digit,
        source_on: bool,
    ) -> Result<Option<Contradiction>, LegacyFcPlusBoundary> {
        self.run_impl::<false>(
            working,
            implications,
            region_types,
            state,
            inner_cache,
            mode,
            config,
            source_cell,
            source_digit,
            source_on,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn run_with_proof(
        &mut self,
        working: &mut Grid,
        implications: &Implications,
        region_types: &[usize],
        state: &mut DynamicState,
        inner_cache: &mut InnerChainCache,
        mode: MultiMode,
        config: EngineConfig,
        source_cell: CellId,
        source_digit: Digit,
        source_on: bool,
    ) -> Result<Option<Contradiction>, LegacyFcPlusBoundary> {
        self.run_impl::<true>(
            working,
            implications,
            region_types,
            state,
            inner_cache,
            mode,
            config,
            source_cell,
            source_digit,
            source_on,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn run_impl<const CAPTURE_CAUSES: bool>(
        &mut self,
        working: &mut Grid,
        implications: &Implications,
        region_types: &[usize],
        state: &mut DynamicState,
        inner_cache: &mut InnerChainCache,
        mode: MultiMode,
        config: EngineConfig,
        source_cell: CellId,
        source_digit: Digit,
        source_on: bool,
    ) -> Result<Option<Contradiction>, LegacyFcPlusBoundary> {
        self.clear();
        state.begin(uses_se121_delta_advanced_scan(working, config, mode));
        let source_key = potential_key(source_cell, source_digit, source_on);
        let source = self.arena.root(source_key);
        if source_on {
            self.to_on.add_if_absent(&self.arena, source);
            self.pending_on.push_back(source);
        } else {
            self.to_off.add_if_absent(&self.arena, source);
            self.pending_off.push_back(source);
        }

        let contradiction = self.propagate::<CAPTURE_CAUSES>(
            working,
            implications,
            region_types,
            state,
            inner_cache,
            mode,
            config,
        );
        state.restore(working);
        contradiction
    }

    #[allow(clippy::too_many_arguments)]
    fn propagate<const CAPTURE_CAUSES: bool>(
        &mut self,
        grid: &mut Grid,
        implications: &Implications,
        region_types: &[usize],
        state: &mut DynamicState,
        inner_cache: &mut InnerChainCache,
        mode: MultiMode,
        config: EngineConfig,
    ) -> Result<Option<Contradiction>, LegacyFcPlusBoundary> {
        loop {
            // Java gives every newly queued ON consequence precedence over
            // even the oldest pending OFF consequence.
            if let Some(parent) = self.pending_on.pop_front() {
                self.collect_weak_keys::<CAPTURE_CAUSES>(grid, implications, mode, parent);
                for index in 0..self.generated_keys.len() {
                    let target_key = self.generated_keys[index];
                    let target = if CAPTURE_CAUSES {
                        self.arena.child_with_cause(
                            target_key,
                            parent,
                            self.generated_causes[index],
                        )
                    } else {
                        self.arena.child(target_key, parent)
                    };
                    let opposite = self.to_on.node(target_key ^ 1);
                    if opposite != NO_NODE {
                        return Ok(Some(Contradiction {
                            on: opposite,
                            off: target,
                        }));
                    }
                    if self.to_off.add_if_absent(&self.arena, target) {
                        self.pending_off.push_back(target);
                    }
                }
                continue;
            }

            if let Some(parent) = self.pending_off.pop_front() {
                self.collect_strong_nodes(grid, implications, region_types, state, mode, parent);
                if mode.is_dynamic() {
                    let parent_key = self.arena.key(parent);
                    state.remove(grid, parent_key, parent);
                }
                for index in 0..self.generated_nodes.len() {
                    let target = self.generated_nodes[index];
                    let target_key = self.arena.key(target);
                    let opposite = self.to_off.node(target_key ^ 1);
                    if opposite != NO_NODE {
                        return Ok(Some(Contradiction {
                            on: target,
                            off: opposite,
                        }));
                    }
                    if self.to_on.add_if_absent(&self.arena, target) {
                        self.pending_on.push_back(target);
                    }
                }
                continue;
            }

            if mode.level() > 0 {
                let scan = self.collect_advanced(grid, state, inner_cache, mode, config);
                if let Some(boundary) = scan.boundary {
                    return Err(boundary);
                }
                if scan.productive {
                    if scan.added {
                        continue;
                    }
                    return Ok(None);
                }
            }
            return Ok(None);
        }
    }

    fn collect_weak_keys<const CAPTURE_CAUSES: bool>(
        &mut self,
        grid: &Grid,
        implications: &Implications,
        mode: MultiMode,
        parent: u32,
    ) {
        self.generated_keys.clear();
        if CAPTURE_CAUSES {
            self.generated_causes.clear();
        }
        let parent_key = self.arena.key(parent);
        if mode == MultiMode::Static {
            if CAPTURE_CAUSES {
                implications.for_each_off_with_cause(parent_key, true, |key, cause| {
                    self.generated_keys.push(key);
                    self.generated_causes.push(cause);
                });
            } else {
                implications.for_each_off(parent_key, true, |key| {
                    self.generated_keys.push(key);
                });
            }
            return;
        }

        let (source_cell, source_digit) = decode_candidate(parent_key);
        for digit in grid.candidates(source_cell).iter() {
            if digit != source_digit {
                self.generated_keys
                    .push(potential_key(source_cell, digit, false));
                if CAPTURE_CAUSES {
                    self.generated_causes.push(OnCause::NakedSingle);
                }
            }
        }
        if CAPTURE_CAUSES {
            implications.for_each_weak_off_with_cause(parent_key, |key, cause| {
                let (cell, digit) = decode_candidate(key);
                if grid.candidates(cell).contains(digit) {
                    self.generated_keys.push(key);
                    self.generated_causes.push(cause);
                }
            });
        } else {
            implications.for_each_weak_off(parent_key, |key| {
                let (cell, digit) = decode_candidate(key);
                if grid.candidates(cell).contains(digit) {
                    self.generated_keys.push(key);
                }
            });
        }
    }

    fn collect_strong_nodes(
        &mut self,
        grid: &Grid,
        implications: &Implications,
        region_types: &[usize],
        state: &DynamicState,
        mode: MultiMode,
        parent: u32,
    ) {
        self.generated_nodes.clear();
        let parent_key = self.arena.key(parent);
        if mode == MultiMode::Static {
            self.generated_keys.clear();
            self.generated_causes.clear();
            implications.for_each_on_with_cause(parent_key, true, true, |key, cause| {
                self.generated_keys.push(key);
                self.generated_causes.push(cause);
            });
            for index in 0..self.generated_keys.len() {
                let target = self.arena.child_with_cause(
                    self.generated_keys[index],
                    parent,
                    self.generated_causes[index],
                );
                self.generated_nodes.push(target);
            }
            return;
        }

        let (source_cell, source_digit) = decode_candidate(parent_key);
        let values = grid.candidates(source_cell);
        if values.count() == 2 {
            let other = values
                .iter()
                .find(|digit| *digit != source_digit)
                .expect("other bivalue digit");
            let target = self.arena.child_with_cause(
                potential_key(source_cell, other, true),
                parent,
                OnCause::NakedSingle,
            );
            for removed in state.removed_mask(grid, source_cell).iter() {
                let hidden_parent = state.removed_node(source_cell, removed);
                debug_assert_ne!(hidden_parent, NO_NODE);
                self.arena.add_parent(target, hidden_parent);
            }
            self.generated_nodes.push(target);
        }

        self.strong_generation = self.strong_generation.wrapping_add(1);
        if self.strong_generation == 0 {
            self.strong_cell_stamps.fill(0);
            self.strong_generation = 1;
        }
        let generation = self.strong_generation;
        let topology = grid.topology();
        for &type_index in region_types {
            let Some(region_index) = topology.cell_region_index(source_cell, type_index) else {
                continue;
            };
            let region = RegionId::new(type_index as u8, region_index)
                .expect("configured dynamic-chain region");
            let mut positions = grid.region_candidate_positions(region, source_digit);
            let source_position = topology
                .cell_position_in_region(source_cell, type_index)
                .expect("dynamic-chain source position");
            positions.remove(source_position);
            if positions.count() != 1 {
                continue;
            }
            let target_position = positions.single().expect("one conjugate position");
            let target_cell =
                CellId::new(topology.region_cells(region)[usize::from(target_position)])
                    .expect("dynamic-chain conjugate cell");
            if self.strong_cell_stamps[target_cell.index()] == generation {
                continue;
            }
            self.strong_cell_stamps[target_cell.index()] = generation;

            let target = self.arena.child_with_cause(
                potential_key(target_cell, source_digit, true),
                parent,
                OnCause::HiddenRegion(u8::try_from(type_index).expect("region type index")),
            );
            for &raw_cell in topology.region_cells(region) {
                let cell = CellId::new(raw_cell).expect("dynamic-chain region cell");
                if state.original_mask(grid, cell).contains(source_digit)
                    && !grid.candidates(cell).contains(source_digit)
                {
                    let hidden_parent = state.removed_node(cell, source_digit);
                    debug_assert_ne!(hidden_parent, NO_NODE);
                    self.arena.add_parent(target, hidden_parent);
                }
            }
            self.generated_nodes.push(target);
        }
    }

    fn clear_advanced_hint(&mut self) {
        for cell in self.advanced_target_cells.drain(..) {
            self.advanced_target_masks[cell.index()] = CandidateMask::EMPTY;
        }
        self.advanced_target_order.clear();
        self.advanced_parents.clear();
        self.advanced_nested = None;
    }

    fn advanced_target(&mut self, cell: CellId, digits: CandidateMask) {
        if digits.is_empty() {
            return;
        }
        if self.advanced_target_masks[cell.index()].is_empty() {
            self.advanced_target_cells.push(cell);
        }
        self.advanced_target_masks[cell.index()] =
            self.advanced_target_masks[cell.index()].union(digits);
    }

    fn advanced_parent(&mut self, grid: &Grid, state: &DynamicState, cell: CellId, digit: Digit) {
        if state.original_mask(grid, cell).contains(digit) && !grid.candidates(cell).contains(digit)
        {
            let node = state.removed_node(cell, digit);
            debug_assert_ne!(node, NO_NODE);
            self.advanced_parents.push(node);
        }
    }

    /// Append advanced OFF nodes in the traversal selected for this chain
    /// mode. Corrected SE121 modes use cell index followed by ascending digit;
    /// general compatibility modes retain coordinate-hash bucket traversal.
    fn commit_advanced_hint(&mut self) -> AdvancedScan {
        if self.advanced_parents.is_empty() || self.advanced_target_cells.is_empty() {
            self.clear_advanced_hint();
            return AdvancedScan::default();
        }

        order_advanced_target_cells(
            &self.advanced_target_cells,
            self.advanced_target_policy,
            &mut self.advanced_target_order,
        );

        let mut added = false;
        let nested = self.advanced_nested.clone();
        for index in 0..self.advanced_target_order.len() {
            let cell = self.advanced_target_order[index];
            for digit in self.advanced_target_masks[cell.index()].iter() {
                let key = potential_key(cell, digit, false);
                let node = if let Some(proof) = &nested {
                    self.arena
                        .push_nested(key, &self.advanced_parents, Arc::clone(proof))
                } else {
                    self.arena.push(key, &self.advanced_parents)
                };
                if self.to_off.add_if_absent(&self.arena, node) {
                    self.pending_off.push_back(node);
                    added = true;
                }
            }
        }
        self.clear_advanced_hint();
        AdvancedScan {
            productive: true,
            added,
            boundary: None,
        }
    }

    fn commit_nested_chain_hint(
        &mut self,
        grid: &Grid,
        state: &DynamicState,
        hint: &NestedHint,
    ) -> AdvancedScan {
        self.clear_advanced_hint();
        for key in hint
            .proof
            .outer_parent_keys(grid, |cell| state.original_mask(grid, cell))
        {
            let parent = self.to_off.node(key);
            debug_assert_ne!(
                parent, NO_NODE,
                "nested-chain parent must be in branch OFF set"
            );
            if parent != NO_NODE {
                self.advanced_parents.push(parent);
            }
        }
        if self.advanced_parents.is_empty() {
            self.clear_advanced_hint();
            return AdvancedScan::default();
        }
        self.advanced_nested = Some(Arc::clone(&hint.proof));
        for removal in hint.removals.iter() {
            self.advanced_target(removal.cell(), removal.digits());
        }
        self.commit_advanced_hint()
    }

    fn scan_nested_chain_hints(
        &mut self,
        grid: &Grid,
        state: &DynamicState,
        hints: &[NestedHint],
    ) -> AdvancedScan {
        let mut family = AdvancedScan::default();
        for hint in hints {
            let scan = self.commit_nested_chain_hint(grid, state, hint);
            family.productive |= scan.productive;
            family.added |= scan.added;
        }
        family
    }

    fn collect_advanced(
        &mut self,
        grid: &Grid,
        state: &DynamicState,
        inner_cache: &mut InnerChainCache,
        mode: MultiMode,
        config: EngineConfig,
    ) -> AdvancedScan {
        self.advanced_target_policy = mode.advanced_target_order();
        let base = self.collect_level_one_advanced(grid, state, config, mode);
        if mode.advanced_family_stops(base) || mode.level() == 1 {
            return base;
        }

        match mode {
            MultiMode::Nested { level: 2, .. } => {
                let hints = inner_cache.forcing(grid, config);
                self.scan_nested_chain_hints(grid, state, &hints)
            }
            MultiMode::Nested { level: 3, .. } => {
                let forcing = inner_cache.forcing(grid, config);
                let scan = self.scan_nested_chain_hints(grid, state, &forcing);
                if mode.advanced_family_stops(scan) {
                    return scan;
                }
                let multiple = inner_cache.multiple(grid, config);
                self.scan_nested_chain_hints(grid, state, &multiple)
            }
            MultiMode::Nested { level: 4, .. } => {
                let inner_mode = match mode.nesting_limit() {
                    0 => MultiMode::Dynamic,
                    1 => MultiMode::DynamicPlus,
                    2 => MultiMode::Nested {
                        level: 2,
                        nesting_limit: 0,
                    },
                    3 => MultiMode::Nested {
                        level: 3,
                        nesting_limit: 0,
                    },
                    _ => unreachable!("unsupported nested dynamic-chain cap"),
                };
                match inner_cache.multi(grid, config, inner_mode) {
                    Ok(hints) => self.scan_nested_chain_hints(grid, state, &hints),
                    Err(boundary) => AdvancedScan::at_boundary(boundary),
                }
            }
            MultiMode::Se121Nested { level: 2 } => {
                let hints = inner_cache.forcing_se121(grid, config);
                self.scan_nested_chain_hints(grid, state, &hints)
            }
            MultiMode::Se121Nested { level: 3 } => {
                let forcing = inner_cache.forcing_se121(grid, config);
                let scan = self.scan_nested_chain_hints(grid, state, &forcing);
                if mode.advanced_family_stops(scan) {
                    return scan;
                }
                let multiple = inner_cache.multiple(grid, config);
                self.scan_nested_chain_hints(grid, state, &multiple)
            }
            MultiMode::Se121Nested { level: 4 } => {
                match inner_cache.multi(grid, config, MultiMode::Dynamic) {
                    Ok(hints) => self.scan_nested_chain_hints(grid, state, &hints),
                    Err(boundary) => AdvancedScan::at_boundary(boundary),
                }
            }
            MultiMode::Se121Nested { level: 5 } => {
                let dynamic = match inner_cache.multi(grid, config, MultiMode::Dynamic) {
                    Ok(hints) => hints,
                    Err(boundary) => return AdvancedScan::at_boundary(boundary),
                };
                let scan = self.scan_nested_chain_hints(grid, state, &dynamic);
                if mode.advanced_family_stops(scan) {
                    return scan;
                }
                match inner_cache.multi(grid, config, MultiMode::Se121Nested { level: 2 }) {
                    Ok(hints) => self.scan_nested_chain_hints(grid, state, &hints),
                    Err(boundary) => AdvancedScan::at_boundary(boundary),
                }
            }
            MultiMode::Static
            | MultiMode::Dynamic
            | MultiMode::DynamicPlus
            | MultiMode::Se121DynamicPlus
            | MultiMode::Nested { .. }
            | MultiMode::Se121Nested { .. } => AdvancedScan::default(),
        }
    }

    fn collect_level_one_advanced(
        &mut self,
        grid: &Grid,
        state: &DynamicState,
        config: EngineConfig,
        mode: MultiMode,
    ) -> AdvancedScan {
        let variant_latin = effective_variant_latin(grid, config);
        let use_delta = uses_se121_delta_advanced_scan(grid, config, mode);
        let first = if variant_latin {
            if mode.uses_se121_locking_order() {
                if use_delta {
                    let delta = self.take_se121_advanced_delta(state, Se121AdvancedFamily::Locking);
                    self.scan_locking_se121_scoped(grid, state, Some(&delta))
                } else {
                    self.scan_locking_se121(grid, state)
                }
            } else {
                self.scan_locking(grid, state)
            }
        } else {
            self.scan_generalized_intersections(grid, state)
        };
        if mode.advanced_family_stops(first) {
            return first;
        }
        let hidden = if use_delta {
            let delta = self.take_se121_advanced_delta(state, Se121AdvancedFamily::HiddenPair);
            self.scan_hidden_sets_scoped(grid, state, config, 2, Some(&delta))
        } else {
            self.scan_hidden_sets(grid, state, config, 2)
        };
        if mode.advanced_family_stops(hidden) {
            return hidden;
        }
        let naked = if use_delta {
            let delta = self.take_se121_advanced_delta(state, Se121AdvancedFamily::NakedPair);
            self.scan_naked_sets_scoped(grid, state, config, !variant_latin, 2, Some(&delta))
        } else {
            self.scan_naked_sets(grid, state, config, !variant_latin, 2)
        };
        if mode.advanced_family_stops(naked) {
            return naked;
        }
        let fish = if use_delta {
            let delta = self.take_se121_advanced_delta(state, Se121AdvancedFamily::XWing);
            self.scan_fish_scoped(grid, state, 2, Some(&delta))
        } else {
            self.scan_fish(grid, state, 2)
        };
        if mode.advanced_family_stops(fish) {
            return fish;
        }

        // TurbotFish is present at this point in Java's FCPlus > 0 schedule,
        // but its legacy getRuleParents implementation compares initialGrid
        // with initialGrid.  It therefore never contributes an implication.
        if config.forcing_chain_plus == 0 {
            return AdvancedScan::default();
        }
        let xy = self.scan_xy_wings(grid, state, false);
        if mode.advanced_family_stops(xy) {
            return xy;
        }
        let xyz = self.scan_xy_wings(grid, state, true);
        if mode.advanced_family_stops(xyz) || config.forcing_chain_plus == 1 {
            return xyz;
        }

        let hidden = self.scan_hidden_sets(grid, state, config, 3);
        if mode.advanced_family_stops(hidden) {
            return hidden;
        }
        let naked = self.scan_naked_sets(grid, state, config, !variant_latin, 3);
        if mode.advanced_family_stops(naked) {
            return naked;
        }
        let fish = self.scan_fish(grid, state, 3);
        if mode.advanced_family_stops(fish) {
            return fish;
        }

        // StrongLinks(3), scheduled only for classic/Latin configurations,
        // has the same inert initialGrid/initialGrid parent bug as TurbotFish.
        let wxyz = self.scan_alphabet_wings(grid, state, 4);
        if mode.advanced_family_stops(wxyz) {
            return wxyz;
        }
        if variant_latin {
            let vwxyz = self.scan_alphabet_wings(grid, state, 5);
            if mode.advanced_family_stops(vwxyz) {
                return vwxyz;
            }
            // The remaining legacy-only entries (Aligned Exclusion, Unique
            // Loops and BUG) do not implement HasParentPotentialHint.  Java
            // throws if one becomes productive, so there is no implication
            // contract to port from them.  Stop at that exact boundary rather
            // than silently consulting a later family.
            if let Some(boundary) = first_broken_fcplus_two_family(grid, config) {
                return AdvancedScan::at_boundary(boundary);
            }
        }
        AdvancedScan::default()
    }

    fn scan_locking(&mut self, grid: &Grid, state: &DynamicState) -> AdvancedScan {
        if !grid.topology().config().blocks {
            return AdvancedScan::default();
        }
        let mut family = AdvancedScan::default();
        for (primary_type, secondary_type) in [(0_usize, 2_usize), (0, 1), (2, 0), (1, 0)] {
            for raw_digit in 1_u8..=9 {
                let digit = Digit::new(raw_digit).expect("digit loop");
                for primary_index in 0..grid.topology().region_count(primary_type) {
                    let primary = region_id(primary_type, primary_index);
                    let primary_positions = grid.region_candidate_positions(primary, digit);
                    if primary_positions.count() < 2 {
                        continue;
                    }
                    for secondary_index in 0..grid.topology().region_count(secondary_type) {
                        let secondary = region_id(secondary_type, secondary_index);
                        let overlap = grid.topology().overlap_positions(primary, secondary);
                        if overlap.is_empty() || !primary_positions.without(overlap).is_empty() {
                            continue;
                        }
                        self.clear_advanced_hint();
                        for &raw_cell in grid.topology().region_cells(primary) {
                            let cell = cell_id(raw_cell);
                            let in_secondary = grid
                                .topology()
                                .cell_position_in_region(cell, secondary_type)
                                .is_some_and(|position| {
                                    grid.topology().region_cells(secondary)[usize::from(position)]
                                        == raw_cell
                                });
                            if !in_secondary {
                                self.advanced_parent(grid, state, cell, digit);
                            }
                        }
                        let secondary_overlap =
                            grid.topology().overlap_positions(secondary, primary);
                        let targets = grid
                            .region_candidate_positions(secondary, digit)
                            .without(secondary_overlap);
                        for position in targets.iter() {
                            self.advanced_target(
                                region_cell(grid, secondary, position),
                                CandidateMask::of(digit),
                            );
                        }
                        let hint = self.commit_advanced_hint();
                        family.productive |= hint.productive;
                        family.added |= hint.added;
                    }
                }
            }
        }
        family
    }

    fn scan_locking_se121(&mut self, grid: &Grid, state: &DynamicState) -> AdvancedScan {
        self.scan_locking_se121_scoped(grid, state, None)
    }

    fn scan_locking_se121_scoped(
        &mut self,
        grid: &Grid,
        state: &DynamicState,
        delta: Option<&AdvancedDelta>,
    ) -> AdvancedScan {
        if delta.is_some_and(AdvancedDelta::is_empty) {
            return AdvancedScan::default();
        }
        if !grid.topology().config().blocks {
            return AdvancedScan::default();
        }
        let mut family = AdvancedScan::default();
        for (primary_type, secondary_type) in [(0_usize, 2_usize), (0, 1), (2, 0), (1, 0)] {
            for primary_index in 0..grid.topology().region_count(primary_type) {
                let primary = region_id(primary_type, primary_index);
                for secondary_index in 0..grid.topology().region_count(secondary_type) {
                    let secondary = region_id(secondary_type, secondary_index);
                    let overlap = grid.topology().overlap_positions(primary, secondary);
                    if overlap.is_empty() {
                        continue;
                    }
                    for raw_digit in 1_u8..=9 {
                        let digit = Digit::new(raw_digit).expect("digit loop");
                        if delta.is_some_and(|delta| {
                            !delta.affects_region_digit(grid, primary, digit)
                                && !delta.affects_region_digit(grid, secondary, digit)
                        }) {
                            continue;
                        }
                        let primary_positions = grid.region_candidate_positions(primary, digit);
                        if primary_positions.count() < 2
                            || !primary_positions.without(overlap).is_empty()
                        {
                            continue;
                        }
                        self.clear_advanced_hint();
                        for &raw_cell in grid.topology().region_cells(primary) {
                            let cell = cell_id(raw_cell);
                            let in_secondary = grid
                                .topology()
                                .cell_position_in_region(cell, secondary_type)
                                .is_some_and(|position| {
                                    grid.topology().region_cells(secondary)[usize::from(position)]
                                        == raw_cell
                                });
                            if !in_secondary {
                                self.advanced_parent(grid, state, cell, digit);
                            }
                        }
                        let secondary_overlap =
                            grid.topology().overlap_positions(secondary, primary);
                        let targets = grid
                            .region_candidate_positions(secondary, digit)
                            .without(secondary_overlap);
                        for position in targets.iter() {
                            self.advanced_target(
                                region_cell(grid, secondary, position),
                                CandidateMask::of(digit),
                            );
                        }
                        let hint = self.commit_advanced_hint();
                        family.productive |= hint.productive;
                        family.added |= hint.added;
                    }
                }
            }
        }
        family
    }

    fn scan_generalized_intersections(
        &mut self,
        grid: &Grid,
        state: &DynamicState,
    ) -> AdvancedScan {
        let mut family = AdvancedScan::default();
        for type_index in generalized_family_order(grid) {
            for raw_digit in 1_u8..=9 {
                let digit = Digit::new(raw_digit).expect("digit loop");
                for region_index in 0..grid.topology().region_count(type_index) {
                    let region = region_id(type_index, region_index);
                    let positions = grid.region_candidate_positions(region, digit);
                    if !(2..=6).contains(&positions.count()) {
                        continue;
                    }
                    let mut iter = positions.iter();
                    let first = region_cell(grid, region, iter.next().expect("two positions"));
                    let mut victims = grid.topology().visible_mask(first);
                    for position in iter {
                        victims = victims.intersect(
                            grid.topology()
                                .visible_mask(region_cell(grid, region, position)),
                        );
                    }
                    victims = victims.intersect(grid.candidate_cells(digit));
                    for position in positions.iter() {
                        victims.remove(region_cell(grid, region, position));
                    }
                    if victims.is_empty() {
                        continue;
                    }

                    self.clear_advanced_hint();
                    for &raw_cell in grid.topology().region_cells(region) {
                        self.advanced_parent(grid, state, cell_id(raw_cell), digit);
                    }
                    for cell in victims.iter() {
                        self.advanced_target(cell, CandidateMask::of(digit));
                    }
                    let hint = self.commit_advanced_hint();
                    family.productive |= hint.productive;
                    family.added |= hint.added;
                }
            }
        }
        family
    }

    fn scan_hidden_sets(
        &mut self,
        grid: &Grid,
        state: &DynamicState,
        config: EngineConfig,
        degree: u8,
    ) -> AdvancedScan {
        self.scan_hidden_sets_scoped(grid, state, config, degree, None)
    }

    fn scan_hidden_sets_scoped(
        &mut self,
        grid: &Grid,
        state: &DynamicState,
        config: EngineConfig,
        degree: u8,
        delta: Option<&AdvancedDelta>,
    ) -> AdvancedScan {
        if delta.is_some_and(AdvancedDelta::is_empty) {
            return AdvancedScan::default();
        }
        let mut family = AdvancedScan::default();
        for type_index in extended_family_order(grid, config) {
            for region_index in 0..grid.topology().region_count(type_index) {
                let region = region_id(type_index, region_index);
                if delta.is_some_and(|delta| !delta.affects_region(grid, region)) {
                    continue;
                }
                if empty_cell_count(grid, region) <= usize::from(degree * 2) {
                    continue;
                }
                for subset in combination_masks(degree) {
                    let digits = CandidateMask::from_bits(subset << 1);
                    let mut positions = PositionMask::EMPTY;
                    let mut valid = true;
                    for digit in digits.iter() {
                        let current = grid.region_candidate_positions(region, digit);
                        if current.count() <= 1 {
                            valid = false;
                            break;
                        }
                        positions = positions.union(current);
                    }
                    if !valid || positions.count() != u32::from(degree) {
                        continue;
                    }
                    self.clear_advanced_hint();
                    for position in PositionMask::ALL.without(positions).iter() {
                        let cell = region_cell(grid, region, position);
                        for digit in digits.iter() {
                            self.advanced_parent(grid, state, cell, digit);
                        }
                    }
                    for position in positions.iter() {
                        let cell = region_cell(grid, region, position);
                        self.advanced_target(cell, grid.candidates(cell).without(digits));
                    }
                    let hint = self.commit_advanced_hint();
                    family.productive |= hint.productive;
                    family.added |= hint.added;
                }
            }
        }
        family
    }

    fn scan_naked_sets(
        &mut self,
        grid: &Grid,
        state: &DynamicState,
        config: EngineConfig,
        generalized: bool,
        degree: u8,
    ) -> AdvancedScan {
        self.scan_naked_sets_scoped(grid, state, config, generalized, degree, None)
    }

    fn scan_naked_sets_scoped(
        &mut self,
        grid: &Grid,
        state: &DynamicState,
        config: EngineConfig,
        generalized: bool,
        degree: u8,
        delta: Option<&AdvancedDelta>,
    ) -> AdvancedScan {
        if delta.is_some_and(AdvancedDelta::is_empty) {
            return AdvancedScan::default();
        }
        let mut family = AdvancedScan::default();
        let families = if generalized {
            extended_family_order(grid, config)
        } else {
            naked_family_order(grid)
        };
        for type_index in families {
            for region_index in 0..grid.topology().region_count(type_index) {
                let region = region_id(type_index, region_index);
                if delta.is_some_and(|delta| !delta.affects_region(grid, region)) {
                    continue;
                }
                if empty_cell_count(grid, region) < usize::from(degree * 2) {
                    continue;
                }
                for subset in combination_masks(degree) {
                    let positions = PositionMask::from_bits(subset);
                    let mut digits = CandidateMask::EMPTY;
                    let mut valid = true;
                    for position in positions.iter() {
                        let current = grid.candidates(region_cell(grid, region, position));
                        if current.count() <= 1 {
                            valid = false;
                            break;
                        }
                        digits = digits.union(current);
                    }
                    if !valid || digits.count() != u32::from(degree) {
                        continue;
                    }
                    self.clear_advanced_hint();
                    for position in positions.iter() {
                        let cell = region_cell(grid, region, position);
                        for parent_digit in state.original_mask(grid, cell).without(digits).iter() {
                            self.advanced_parent(grid, state, cell, parent_digit);
                        }
                    }
                    if generalized {
                        let tuple_cells = tuple_cell_mask(grid, region, positions);
                        for digit in digits.iter() {
                            let mut supports = positions.iter().filter(|&position| {
                                grid.candidates(region_cell(grid, region, position))
                                    .contains(digit)
                            });
                            let first = supports.next().expect("pair digit support");
                            let mut victims = grid
                                .topology()
                                .visible_mask(region_cell(grid, region, first));
                            for position in supports {
                                victims = victims.intersect(
                                    grid.topology()
                                        .visible_mask(region_cell(grid, region, position)),
                                );
                            }
                            victims = victims
                                .without(tuple_cells)
                                .intersect(grid.candidate_cells(digit));
                            for victim in victims.iter() {
                                self.advanced_target(victim, CandidateMask::of(digit));
                            }
                        }
                    } else {
                        for position in PositionMask::ALL.without(positions).iter() {
                            let cell = region_cell(grid, region, position);
                            self.advanced_target(cell, grid.candidates(cell).intersect(digits));
                        }
                    }
                    let hint = self.commit_advanced_hint();
                    family.productive |= hint.productive;
                    family.added |= hint.added;
                }
            }
        }
        family
    }

    fn scan_fish(&mut self, grid: &Grid, state: &DynamicState, degree: u8) -> AdvancedScan {
        self.scan_fish_scoped(grid, state, degree, None)
    }

    fn scan_fish_scoped(
        &mut self,
        grid: &Grid,
        state: &DynamicState,
        degree: u8,
        delta: Option<&AdvancedDelta>,
    ) -> AdvancedScan {
        if delta.is_some_and(AdvancedDelta::is_empty) {
            return AdvancedScan::default();
        }
        let mut occurrences = [0_u8; 10];
        for raw_cell in 0_u8..81 {
            let value = grid.value(cell_id(raw_cell));
            if value != 0 {
                occurrences[usize::from(value)] += 1;
            }
        }
        let mut family = AdvancedScan::default();
        for (base_type, cover_type) in [(2_usize, 1_usize), (1, 2)] {
            for subset in combination_masks(degree) {
                let bases = PositionMask::from_bits(subset);
                for raw_digit in 1_u8..=9 {
                    if occurrences[usize::from(raw_digit)] + degree * 2 > 9 {
                        continue;
                    }
                    let digit = Digit::new(raw_digit).expect("digit loop");
                    if delta.is_some_and(|delta| {
                        bases.iter().all(|base_index| {
                            !delta.affects_region_digit(
                                grid,
                                region_id(base_type, usize::from(base_index)),
                                digit,
                            )
                        })
                    }) {
                        continue;
                    }
                    let mut covers = PositionMask::EMPTY;
                    let mut valid = true;
                    for base_index in bases.iter() {
                        let positions = grid.region_candidate_positions(
                            region_id(base_type, usize::from(base_index)),
                            digit,
                        );
                        if positions.count() <= 1 {
                            valid = false;
                            break;
                        }
                        covers = covers.union(positions);
                    }
                    if !valid || covers.count() != u32::from(degree) {
                        continue;
                    }
                    self.clear_advanced_hint();
                    for base_index in bases.iter() {
                        let base = region_id(base_type, usize::from(base_index));
                        for (position, &raw_cell) in
                            grid.topology().region_cells(base).iter().enumerate()
                        {
                            if !covers.contains(position as u8) {
                                self.advanced_parent(grid, state, cell_id(raw_cell), digit);
                            }
                        }
                    }
                    for cover_index in covers.iter() {
                        let cover = region_id(cover_type, usize::from(cover_index));
                        let targets = grid.region_candidate_positions(cover, digit).without(bases);
                        for position in targets.iter() {
                            self.advanced_target(
                                region_cell(grid, cover, position),
                                CandidateMask::of(digit),
                            );
                        }
                    }
                    let hint = self.commit_advanced_hint();
                    family.productive |= hint.productive;
                    family.added |= hint.added;
                }
            }
        }
        family
    }

    fn scan_xy_wings(&mut self, grid: &Grid, state: &DynamicState, xyz: bool) -> AdvancedScan {
        let target_cardinality = if xyz { 3 } else { 2 };
        let mut family = AdvancedScan::default();
        for raw_pivot in 0_u8..81 {
            let pivot = cell_id(raw_pivot);
            let pivot_values = grid.candidates(pivot);
            if pivot_values.count() != target_cardinality {
                continue;
            }
            let peers = grid.topology().visible_peers(pivot);
            for &raw_xz in peers {
                let xz = cell_id(raw_xz);
                let xz_values = grid.candidates(xz);
                if xz_values.count() != 2 || pivot_values.without(xz_values).count() != 1 {
                    continue;
                }
                for &raw_yz in peers {
                    let yz = cell_id(raw_yz);
                    let yz_values = grid.candidates(yz);
                    if yz_values.count() != 2 {
                        continue;
                    }
                    let union = pivot_values.union(xz_values).union(yz_values);
                    if union.count() != 3 {
                        continue;
                    }
                    let common = pivot_values.intersect(xz_values).intersect(yz_values);
                    if (!xyz && !common.is_empty()) || (xyz && common.count() != 1) {
                        continue;
                    }
                    let Some(digit) = xz_values.intersect(yz_values).single() else {
                        continue;
                    };
                    let mut victims = grid
                        .topology()
                        .visible_mask(xz)
                        .intersect(grid.topology().visible_mask(yz));
                    if xyz {
                        victims = victims.intersect(grid.topology().visible_mask(pivot));
                    }
                    victims.remove(pivot);
                    victims.remove(xz);
                    victims.remove(yz);
                    victims = victims.intersect(grid.candidate_cells(digit));
                    if victims.is_empty() {
                        continue;
                    }

                    self.clear_advanced_hint();
                    // XYWingHint.getRuleParents loops digits first and then
                    // the pivot/XZ/YZ cells for each digit.
                    for raw_digit in 1_u8..=9 {
                        let parent_digit = Digit::new(raw_digit).expect("digit loop");
                        for cell in [pivot, xz, yz] {
                            self.advanced_parent(grid, state, cell, parent_digit);
                        }
                    }
                    for victim in victims.iter() {
                        self.advanced_target(victim, CandidateMask::of(digit));
                    }
                    let hint = self.commit_advanced_hint();
                    family.productive |= hint.productive;
                    family.added |= hint.added;
                }
            }
        }
        family
    }

    fn scan_alphabet_wings(
        &mut self,
        grid: &Grid,
        state: &DynamicState,
        degree: u8,
    ) -> AdvancedScan {
        let mut family = AdvancedScan::default();
        for hint in collect_alphabet_wing_advanced(grid, degree) {
            self.clear_advanced_hint();
            // The Java WXYZ/VWXYZ parent methods loop digits first, followed
            // by the selected pattern cells in their discovery order.
            for raw_digit in 1_u8..=9 {
                let digit = Digit::new(raw_digit).expect("digit loop");
                for cell in hint.selected_cells.iter() {
                    self.advanced_parent(grid, state, cell, digit);
                }
            }
            for removal in hint.removals.iter() {
                self.advanced_target(removal.cell(), removal.digits());
            }
            let scan = self.commit_advanced_hint();
            family.productive |= scan.productive;
            family.added |= scan.added;
        }
        family
    }

    fn contains(&self, key: u16, target_on: bool) -> bool {
        if target_on {
            self.to_on.contains(key)
        } else {
            self.to_off.contains(key)
        }
    }

    fn ordered_keys(&self, target_on: bool, result: &mut Vec<u16>) {
        result.clear();
        let set = if target_on { &self.to_on } else { &self.to_off };
        result.extend(set.order.iter().map(|&node| self.arena.key(node)));
    }

    fn complexity(&mut self, key: u16, target_on: bool) -> u16 {
        let node = self.target_node(key, target_on);
        debug_assert_ne!(node, NO_NODE);
        self.arena.ancestor_count(node)
    }

    fn target_node(&self, key: u16, target_on: bool) -> u32 {
        if target_on {
            self.to_on.node(key)
        } else {
            self.to_off.node(key)
        }
    }
}

fn first_broken_fcplus_two_family(
    grid: &Grid,
    config: EngineConfig,
) -> Option<LegacyFcPlusBoundary> {
    if find_aligned_triplet_exclusion(grid).is_some() {
        return Some(LegacyFcPlusBoundary::AlignedTripletExclusion);
    }
    if find_unique_loop(grid, config).is_some() {
        return Some(LegacyFcPlusBoundary::UniqueLoops);
    }
    find_bivalue_universal_grave(grid, config).map(|_| LegacyFcPlusBoundary::BivalueUniversalGrave)
}

struct MultiWorkspace {
    cell_branches: Vec<Branch>,
    region_branches: Vec<Branch>,
    off_branch: Branch,
    state: DynamicState,
    ordered_keys: Vec<u16>,
}

impl MultiWorkspace {
    fn new() -> Self {
        Self {
            cell_branches: (0..9).map(|_| Branch::new()).collect(),
            region_branches: (0..8).map(|_| Branch::new()).collect(),
            off_branch: Branch::new(),
            state: DynamicState::new(),
            ordered_keys: Vec::with_capacity(192),
        }
    }

    fn clear_results(&mut self) {
        for branch in &mut self.cell_branches {
            branch.clear();
        }
        for branch in &mut self.region_branches {
            branch.clear();
        }
        self.off_branch.clear();
        self.ordered_keys.clear();
        debug_assert!(self.state.changed_cells.is_empty());
    }
}

/// Scratch retained only while rating one puzzle with the dedicated SE121
/// headless path. General engine and presentation searches deliberately keep
/// their isolated workspaces.
pub(crate) struct Se121ChainSession {
    workspace: MultiWorkspace,
    working: Option<Grid>,
    inner_cache: InnerChainCache,
    config: Option<EngineConfig>,
}

impl Default for Se121ChainSession {
    fn default() -> Self {
        Self {
            workspace: MultiWorkspace::new(),
            working: None,
            inner_cache: InnerChainCache::default(),
            config: None,
        }
    }
}

struct RankedMulti {
    inference: Inference,
    java_difficulty: f64,
    complexity: u32,
    sort_key: u8,
}

/// Allocation-free first-hint candidate. The effect and evidence are kept as
/// compact scalars until ranking has selected the one published inference.
struct RankedMultiDraft {
    java_difficulty: f64,
    complexity: u32,
    sort_key: u8,
    kind: MultipleChainKind,
    target_cell: CellId,
    target_digit: Digit,
    target_on: bool,
}

impl RankedMultiDraft {
    #[allow(clippy::too_many_arguments)]
    fn new(
        grid: &Grid,
        mode: MultiMode,
        complexity: u32,
        sort_key: u8,
        kind: MultipleChainKind,
        target_cell: CellId,
        target_digit: Digit,
        target_on: bool,
    ) -> Option<Self> {
        if !target_has_removals(grid, target_cell, target_digit, target_on) {
            return None;
        }
        #[cfg(test)]
        FIRST_DRAFT_OFFERS.with(|count| count.set(count.get() + 1));
        let (_, java_difficulty) = multi_rating(mode, complexity);
        Some(Self {
            java_difficulty,
            complexity,
            sort_key,
            kind,
            target_cell,
            target_digit,
            target_on,
        })
    }

    fn precedes(&self, other: &Self) -> bool {
        if self.java_difficulty < other.java_difficulty {
            return true;
        }
        if self.java_difficulty > other.java_difficulty {
            return false;
        }
        (self.complexity, self.sort_key) < (other.complexity, other.sort_key)
    }

    fn materialize(self, grid: &Grid, mode: MultiMode) -> Inference {
        ranked_target(
            grid,
            mode,
            self.complexity,
            self.sort_key,
            self.kind,
            self.target_cell,
            self.target_digit,
            self.target_on,
        )
        .expect("ranked first multi-chain effect remains applicable")
        .inference
    }
}

enum MultiSink<'a> {
    First(&'a mut Option<RankedMultiDraft>),
    Summaries(&'a mut InferenceCollector),
    All(&'a mut NestedHintCollector),
}

impl MultiSink<'_> {
    fn needs_proof(&self) -> bool {
        matches!(self, Self::All(_))
    }

    fn needs_complete_complexity(&self, mode: MultiMode) -> bool {
        matches!(self, Self::First(_) | Self::Summaries(_)) && mode.level() >= 2
    }

    #[allow(clippy::too_many_arguments)]
    fn offer(
        &mut self,
        grid: &Grid,
        mode: MultiMode,
        flat_complexity: u16,
        sort_key: u8,
        kind: MultipleChainKind,
        effect_cell: CellId,
        effect_digit: Digit,
        effect_on: bool,
        complete_complexity: Option<u32>,
        proof: Option<Arc<ChainProof>>,
    ) {
        match self {
            Self::First(best) => {
                let complexity = if mode.level() >= 2 {
                    complete_complexity.expect("nested first-hint complete complexity")
                } else {
                    u32::from(flat_complexity)
                };
                if let Some(candidate) = RankedMultiDraft::new(
                    grid,
                    mode,
                    complexity,
                    sort_key,
                    kind,
                    effect_cell,
                    effect_digit,
                    effect_on,
                ) {
                    keep_best_draft(best, candidate);
                }
            }
            Self::Summaries(result) => {
                let complexity = if mode.level() >= 2 {
                    complete_complexity.expect("nested all-hints complete complexity")
                } else {
                    u32::from(flat_complexity)
                };
                if let Some(candidate) = ranked_target(
                    grid,
                    mode,
                    complexity,
                    sort_key,
                    kind,
                    effect_cell,
                    effect_digit,
                    effect_on,
                ) {
                    result.offer(
                        grid,
                        candidate.inference,
                        candidate.java_difficulty,
                        candidate.complexity,
                        candidate.sort_key,
                    );
                }
            }
            Self::All(result) => {
                let Some(removals) = target_removals(grid, effect_cell, effect_digit, effect_on)
                else {
                    return;
                };
                let proof = proof.expect("all-hints multi-chain proof");
                let complexity = proof.complexity();
                let (_, java_difficulty) = multi_rating(mode, complexity);
                result.offer(NestedHint {
                    proof,
                    removals,
                    java_difficulty,
                    complexity,
                    sort_key,
                });
            }
        }
    }
}

/// Find Java's first ranked static Multiple Forcing Chain.
#[must_use]
pub fn find_multiple_forcing_chain(grid: &Grid, config: EngineConfig) -> Option<Inference> {
    find_multi_chain_checked(grid, config, MultiMode::Static)
        .expect("static multiple chains cannot reach an FCPlus boundary")
}

/// Collect all Java-ranked static Multiple Forcing Chains without retaining
/// their branch proof graphs.
#[must_use]
pub fn collect_multiple_forcing_chains(grid: &Grid, config: EngineConfig) -> Vec<Inference> {
    collect_multi_chain_summaries_checked(grid, config, MultiMode::Static)
        .expect("static multiple chains cannot reach an FCPlus boundary")
}

/// Find Java's first ranked static Multiple Forcing Chain and replay only its
/// selected outer branches into presentation proof views.
#[must_use]
pub fn find_multiple_forcing_chain_with_proof(
    grid: &Grid,
    config: EngineConfig,
) -> Option<MultipleForcingChainWithProof> {
    let inference = find_multiple_forcing_chain(grid, config)?;
    let proof = replay_selected_multi_proof(grid, config, MultiMode::Static, &inference)
        .expect("static selected replay cannot reach an FCPlus boundary");
    Some(MultipleForcingChainWithProof::new(inference, proof))
}

/// Replay the outer branch views for any retained static MFC inference.
#[must_use]
pub fn replay_multiple_forcing_chain_with_proof(
    grid: &Grid,
    config: EngineConfig,
    inference: &Inference,
) -> Option<MultipleForcingChainWithProof> {
    replay_multi_chain_with_proof_checked(grid, config, MultiMode::Static, inference)
        .expect("static selected replay cannot reach an FCPlus boundary")
}

/// Find Java's first ranked level-0 Dynamic Forcing Chain.
#[must_use]
pub fn find_dynamic_forcing_chain(grid: &Grid, config: EngineConfig) -> Option<Inference> {
    find_multi_chain_checked(grid, config, MultiMode::Dynamic)
        .expect("level-zero dynamic chains cannot reach an FCPlus boundary")
}

/// Collect all Java-ranked level-zero Dynamic Forcing Chains without
/// retaining their branch proof graphs.
#[must_use]
pub fn collect_dynamic_forcing_chains(grid: &Grid, config: EngineConfig) -> Vec<Inference> {
    collect_multi_chain_summaries_checked(grid, config, MultiMode::Dynamic)
        .expect("level-zero dynamic chains cannot reach an FCPlus boundary")
}

/// Find Java's first ranked level-zero Dynamic Forcing Chain and replay only
/// its selected outer branches into presentation proof views.
#[must_use]
pub fn find_dynamic_forcing_chain_with_proof(
    grid: &Grid,
    config: EngineConfig,
) -> Option<MultipleForcingChainWithProof> {
    let inference = find_dynamic_forcing_chain(grid, config)?;
    let proof = replay_selected_multi_proof(grid, config, MultiMode::Dynamic, &inference)
        .expect("level-zero selected replay cannot reach an FCPlus boundary");
    Some(MultipleForcingChainWithProof::new(inference, proof))
}

/// Replay the outer branch views for any retained level-zero DFC inference.
#[must_use]
pub fn replay_dynamic_forcing_chain_with_proof(
    grid: &Grid,
    config: EngineConfig,
    inference: &Inference,
) -> Option<MultipleForcingChainWithProof> {
    replay_multi_chain_with_proof_checked(grid, config, MultiMode::Dynamic, inference)
        .expect("level-zero selected replay cannot reach an FCPlus boundary")
}

/// Find Java's first ranked level-1 Dynamic Forcing Chain (+).
///
/// Java itself throws when a productive FCPlus=2 rule lacks the legacy parent
/// interface. Call [`find_dynamic_forcing_chain_plus_checked`] when that
/// setting is accepted from untrusted configuration.
#[must_use]
pub fn find_dynamic_forcing_chain_plus(grid: &Grid, config: EngineConfig) -> Option<Inference> {
    find_dynamic_forcing_chain_plus_checked(grid, config)
        .expect("legacy Java FCPlus=2 boundary; use the checked DFC+ finder")
}

/// Find SE 1.2.1's level-one Dynamic Forcing Chain (+), with the corrected
/// advanced-family pruning and target traversal.
///
/// Its embedded Locking producer retains the old region-before-digit
/// traversal without changing the later compatibility implementation above.
#[must_use]
#[cfg(test)]
pub(crate) fn find_se121_dynamic_forcing_chain_plus(
    grid: &Grid,
    config: EngineConfig,
) -> Option<Inference> {
    find_multi_chain_checked(grid, config, MultiMode::Se121DynamicPlus)
        .expect("SE 1.2.1 FCPlus=0 cannot reach a legacy parent boundary")
}

/// Search the consecutive SE 1.2.1 multiple/dynamic/nested tail with one
/// reusable set of branch arenas and one dynamic implication table.
///
/// The dedicated classic rater is the only caller. General compatibility
/// entry points continue to create isolated workspaces, while this path can
/// safely retain branch capacity and immutable weak links because every
/// producer sees the same unchanged outer grid. Proof-bearing inner results
/// stay producer-local; exact-state negative FCC results survive for one
/// rating session without retaining proof graphs.
pub(crate) fn find_se121_chain_tail_with_session(
    grid: &Grid,
    config: EngineConfig,
    session: &mut Se121ChainSession,
) -> Option<Inference> {
    // The cache has a dedicated inner-FCC namespace, and one session belongs
    // to one puzzle. Explicitly invalidate it if an internal caller changes
    // the engine configuration rather than letting a same-grid key cross
    // configuration profiles.
    if session.config != Some(config) {
        session.inner_cache.clear_local_results();
        session.inner_cache.se121_forcing_negative.clear();
        session.config = Some(config);
    }
    let region_types = active_region_types(grid, config);
    let working = match &mut session.working {
        Some(working) => {
            working.clone_from(grid);
            working
        }
        None => session.working.insert(grid.clone()),
    };

    // Static MFC needs the full implication graph. Drop it before building
    // the weak-only graph used by every later dynamic family.
    let multiple = {
        let implications = Implications::new(grid, config);
        session.inner_cache.clear_local_results();
        find_multi_chain_with_resources(
            grid,
            config,
            MultiMode::Static,
            &implications,
            &region_types,
            &mut *working,
            &mut session.workspace,
            &mut session.inner_cache,
        )
        .expect("static multiple chains cannot reach an FCPlus boundary")
    };
    session.inner_cache.clear_local_results();
    session.workspace.clear_results();
    if multiple.is_some() {
        return multiple;
    }

    let implications = Implications::weak_only(grid, config);
    for mode in [
        MultiMode::Dynamic,
        MultiMode::Se121DynamicPlus,
        MultiMode::Se121Nested { level: 2 },
        MultiMode::Se121Nested { level: 3 },
        MultiMode::Se121Nested { level: 4 },
        MultiMode::Se121Nested { level: 5 },
    ] {
        session.inner_cache.clear_local_results();
        let inference = find_multi_chain_with_resources(
            grid,
            config,
            mode,
            &implications,
            &region_types,
            &mut *working,
            &mut session.workspace,
            &mut session.inner_cache,
        )
        .expect("SE 1.2.1 FCPlus=0 cannot reach a legacy parent boundary");
        session.inner_cache.clear_local_results();
        session.workspace.clear_results();
        if inference.is_some() {
            return inference;
        }
    }
    None
}

/// Checked DFC+ entry point for Java's historically broken FCPlus=2 tail.
pub fn find_dynamic_forcing_chain_plus_checked(
    grid: &Grid,
    config: EngineConfig,
) -> Result<Option<Inference>, LegacyFcPlusBoundary> {
    find_multi_chain_checked(grid, config, MultiMode::DynamicPlus)
}

/// Checked all-hints DFC+ entry point for Java's historically broken
/// FCPlus=2 tail.
pub fn collect_dynamic_forcing_chain_plus_checked(
    grid: &Grid,
    config: EngineConfig,
) -> Result<Vec<Inference>, LegacyFcPlusBoundary> {
    collect_multi_chain_summaries_checked(grid, config, MultiMode::DynamicPlus)
}

/// Find Java's first ranked level-one Dynamic Forcing Chain (+) and replay
/// only its selected outer branches into presentation proof views.
#[must_use]
pub fn find_dynamic_forcing_chain_plus_with_proof(
    grid: &Grid,
    config: EngineConfig,
) -> Option<MultipleForcingChainWithProof> {
    find_dynamic_forcing_chain_plus_with_proof_checked(grid, config)
        .expect("legacy Java FCPlus=2 boundary; use the checked detailed DFC+ finder")
}

/// Checked presentation entry point for Java's historically broken FCPlus=2
/// tail. Discovery and selected replay surface the same legacy boundary.
pub fn find_dynamic_forcing_chain_plus_with_proof_checked(
    grid: &Grid,
    config: EngineConfig,
) -> Result<Option<MultipleForcingChainWithProof>, LegacyFcPlusBoundary> {
    let Some(inference) = find_dynamic_forcing_chain_plus_checked(grid, config)? else {
        return Ok(None);
    };
    let proof = replay_selected_multi_proof(grid, config, MultiMode::DynamicPlus, &inference)?;
    Ok(Some(MultipleForcingChainWithProof::new(inference, proof)))
}

/// Checked lazy DFC+ proof replay for Java's FCPlus=2 boundary.
pub fn replay_dynamic_forcing_chain_plus_with_proof_checked(
    grid: &Grid,
    config: EngineConfig,
    inference: &Inference,
) -> Result<Option<MultipleForcingChainWithProof>, LegacyFcPlusBoundary> {
    replay_multi_chain_with_proof_checked(grid, config, MultiMode::DynamicPlus, inference)
}

/// Find Java's nested dynamic chain family. Levels 2 and 3 ignore
/// `nesting_limit`; level 4 uses caps 0 through 3 for its inner dynamic rule.
///
/// Call [`find_nested_forcing_chain_checked`] for FCPlus=2 configurations that
/// may reach Java's broken parent-provider tail.
#[must_use]
pub fn find_nested_forcing_chain(
    grid: &Grid,
    config: EngineConfig,
    level: u8,
    nesting_limit: u8,
) -> Option<Inference> {
    find_nested_forcing_chain_checked(grid, config, level, nesting_limit)
        .expect("legacy Java FCPlus=2 boundary; use the checked nested finder")
}

/// Checked nested-chain entry point; every nested level first consults the
/// level-one advanced schedule and can therefore reach the FCPlus=2 boundary.
pub fn find_nested_forcing_chain_checked(
    grid: &Grid,
    config: EngineConfig,
    level: u8,
    nesting_limit: u8,
) -> Result<Option<Inference>, LegacyFcPlusBoundary> {
    assert!((2..=4).contains(&level), "nested chain level");
    assert!(level != 4 || nesting_limit <= 3, "nested chain cap");
    find_multi_chain_checked(
        grid,
        config,
        MultiMode::Nested {
            level,
            nesting_limit,
        },
    )
}

/// Checked nested-chain all-hints entry point.
pub fn collect_nested_forcing_chains_checked(
    grid: &Grid,
    config: EngineConfig,
    level: u8,
    nesting_limit: u8,
) -> Result<Vec<Inference>, LegacyFcPlusBoundary> {
    assert!((2..=4).contains(&level), "nested chain level");
    assert!(level != 4 || nesting_limit <= 3, "nested chain cap");
    collect_multi_chain_summaries_checked(
        grid,
        config,
        MultiMode::Nested {
            level,
            nesting_limit,
        },
    )
}

/// Find Java's selected nested Dynamic Forcing Chain and replay only its
/// outer DAG. Inner chaining deductions remain collapsed `Derived` edges.
#[must_use]
pub fn find_nested_forcing_chain_with_proof(
    grid: &Grid,
    config: EngineConfig,
    level: u8,
    nesting_limit: u8,
) -> Option<MultipleForcingChainWithProof> {
    find_nested_forcing_chain_with_proof_checked(grid, config, level, nesting_limit)
        .expect("legacy Java FCPlus=2 boundary; use the checked detailed nested finder")
}

/// Checked nested presentation entry point with the exact producer level and
/// level-four nesting cap required to replay the selected outer branches.
pub fn find_nested_forcing_chain_with_proof_checked(
    grid: &Grid,
    config: EngineConfig,
    level: u8,
    nesting_limit: u8,
) -> Result<Option<MultipleForcingChainWithProof>, LegacyFcPlusBoundary> {
    let Some(inference) = find_nested_forcing_chain_checked(grid, config, level, nesting_limit)?
    else {
        return Ok(None);
    };
    let proof = replay_selected_multi_proof(
        grid,
        config,
        MultiMode::Nested {
            level,
            nesting_limit,
        },
        &inference,
    )?;
    Ok(Some(MultipleForcingChainWithProof::new(inference, proof)))
}

/// Checked lazy proof replay for one exact nested-chain level and cap.
pub fn replay_nested_forcing_chain_with_proof_checked(
    grid: &Grid,
    config: EngineConfig,
    level: u8,
    nesting_limit: u8,
    inference: &Inference,
) -> Result<Option<MultipleForcingChainWithProof>, LegacyFcPlusBoundary> {
    assert!((2..=4).contains(&level), "nested chain level");
    assert!(level != 4 || nesting_limit <= 3, "nested chain cap");
    replay_multi_chain_with_proof_checked(
        grid,
        config,
        MultiMode::Nested {
            level,
            nesting_limit,
        },
        inference,
    )
}

fn replay_multi_chain_with_proof_checked(
    grid: &Grid,
    config: EngineConfig,
    mode: MultiMode,
    inference: &Inference,
) -> Result<Option<MultipleForcingChainWithProof>, LegacyFcPlusBoundary> {
    let Evidence::MultipleForcingChain { dynamic, level, .. } = inference.evidence() else {
        return Ok(None);
    };
    if inference.technique() != mode.technique()
        || dynamic != mode.is_dynamic()
        || level != mode.level()
    {
        return Ok(None);
    }
    let proof = replay_selected_multi_proof(grid, config, mode, inference)?;
    Ok(Some(MultipleForcingChainWithProof::new(
        inference.clone(),
        proof,
    )))
}

/// Materialize only the selected proof for a retained static MFC inference.
#[must_use]
pub fn replay_multiple_forcing_chain_proof(
    grid: &Grid,
    config: EngineConfig,
    inference: &Inference,
) -> SelectedChainProof {
    replay_multiple_forcing_chain_with_proof(grid, config, inference)
        .expect("retained static MFC inference is reproducible")
        .into_parts()
        .1
}

/// Materialize only the selected proof for a retained level-zero DFC
/// inference.
#[must_use]
pub fn replay_dynamic_forcing_chain_proof(
    grid: &Grid,
    config: EngineConfig,
    inference: &Inference,
) -> SelectedChainProof {
    replay_dynamic_forcing_chain_with_proof(grid, config, inference)
        .expect("retained level-zero DFC inference is reproducible")
        .into_parts()
        .1
}

/// Checked selected-proof replay for a retained DFC+ inference.
pub fn replay_dynamic_forcing_chain_plus_proof(
    grid: &Grid,
    config: EngineConfig,
    inference: &Inference,
) -> Result<SelectedChainProof, LegacyFcPlusBoundary> {
    Ok(
        replay_dynamic_forcing_chain_plus_with_proof_checked(grid, config, inference)?
            .expect("retained DFC+ inference is reproducible")
            .into_parts()
            .1,
    )
}

/// Checked selected-proof replay for a retained nested-chain inference.
pub fn replay_nested_forcing_chain_proof_checked(
    grid: &Grid,
    config: EngineConfig,
    level: u8,
    nesting_limit: u8,
    inference: &Inference,
) -> Result<SelectedChainProof, LegacyFcPlusBoundary> {
    Ok(replay_nested_forcing_chain_with_proof_checked(
        grid,
        config,
        level,
        nesting_limit,
        inference,
    )?
    .expect("retained nested-chain inference is reproducible")
    .into_parts()
    .1)
}

#[allow(clippy::too_many_arguments)]
fn run_selected_branch(
    branch: &mut Branch,
    working: &mut Grid,
    implications: &Implications,
    region_types: &[usize],
    state: &mut DynamicState,
    inner_cache: &mut InnerChainCache,
    mode: MultiMode,
    config: EngineConfig,
    source_cell: CellId,
    source_digit: Digit,
    source_on: bool,
) -> Result<Option<Contradiction>, LegacyFcPlusBoundary> {
    branch.run_with_proof(
        working,
        implications,
        region_types,
        state,
        inner_cache,
        mode,
        config,
        source_cell,
        source_digit,
        source_on,
    )
}

fn replay_selected_multi_proof(
    grid: &Grid,
    config: EngineConfig,
    mode: MultiMode,
    inference: &Inference,
) -> Result<SelectedChainProof, LegacyFcPlusBoundary> {
    let Evidence::MultipleForcingChain {
        dynamic,
        level,
        kind,
        ..
    } = inference.evidence()
    else {
        unreachable!("selected multi-chain inference evidence")
    };
    assert_eq!(dynamic, mode.is_dynamic(), "selected multi-chain mode");
    assert_eq!(level, mode.level(), "selected multi-chain level");

    let implications = if mode == MultiMode::Static {
        Implications::new(grid, config)
    } else {
        Implications::weak_only(grid, config)
    };
    let region_types = active_region_types(grid, config);
    let mut working = grid.clone();
    let mut workspace = MultiWorkspace::new();
    let mut inner_cache = InnerChainCache::default();

    let views = match kind {
        MultipleChainKind::Contradiction {
            source_cell,
            source_digit,
            source_on,
            target_cell,
            target_digit,
        } => {
            assert!(
                mode.is_dynamic(),
                "static MFC contradiction is not published"
            );
            let contradiction = run_selected_branch(
                &mut workspace.off_branch,
                &mut working,
                &implications,
                &region_types,
                &mut workspace.state,
                &mut inner_cache,
                mode,
                config,
                source_cell,
                source_digit,
                source_on,
            )?
            .expect("ranked dynamic contradiction is reproducible");
            assert_eq!(
                workspace.off_branch.arena.key(contradiction.on),
                potential_key(target_cell, target_digit, true),
                "selected contradiction ON target",
            );
            assert_eq!(
                workspace.off_branch.arena.key(contradiction.off),
                potential_key(target_cell, target_digit, false),
                "selected contradiction OFF target",
            );
            vec![
                materialize_multi_view(
                    grid,
                    &workspace.off_branch.arena,
                    contradiction.on,
                    ChainProofViewKind::ContradictionOn,
                ),
                materialize_multi_view(
                    grid,
                    &workspace.off_branch.arena,
                    contradiction.off,
                    ChainProofViewKind::ContradictionOff,
                ),
            ]
        }
        MultipleChainKind::Double {
            source_cell,
            source_digit,
            target_cell,
            target_digit,
            target_on,
        } => {
            assert!(
                mode.is_dynamic(),
                "static MFC double chain is not published"
            );
            run_selected_branch(
                &mut workspace.cell_branches[0],
                &mut working,
                &implications,
                &region_types,
                &mut workspace.state,
                &mut inner_cache,
                mode,
                config,
                source_cell,
                source_digit,
                true,
            )?;
            run_selected_branch(
                &mut workspace.off_branch,
                &mut working,
                &implications,
                &region_types,
                &mut workspace.state,
                &mut inner_cache,
                mode,
                config,
                source_cell,
                source_digit,
                false,
            )?;
            let key = potential_key(target_cell, target_digit, target_on);
            let on_target = workspace.cell_branches[0].target_node(key, target_on);
            let off_target = workspace.off_branch.target_node(key, target_on);
            assert_ne!(on_target, NO_NODE, "selected ON-assumption target");
            assert_ne!(off_target, NO_NODE, "selected OFF-assumption target");
            vec![
                materialize_multi_view(
                    grid,
                    &workspace.cell_branches[0].arena,
                    on_target,
                    ChainProofViewKind::AssumptionOn,
                ),
                materialize_multi_view(
                    grid,
                    &workspace.off_branch.arena,
                    off_target,
                    ChainProofViewKind::AssumptionOff,
                ),
            ]
        }
        MultipleChainKind::Cell {
            source_cell,
            target_cell,
            target_digit,
            target_on,
        } => {
            let key = potential_key(target_cell, target_digit, target_on);
            let values = grid.candidates(source_cell);
            let mut result =
                Vec::with_capacity(usize::try_from(values.count()).expect("cell branch count"));
            for (branch_index, source_digit) in values.iter().enumerate() {
                let branch = &mut workspace.cell_branches[branch_index];
                run_selected_branch(
                    branch,
                    &mut working,
                    &implications,
                    &region_types,
                    &mut workspace.state,
                    &mut inner_cache,
                    mode,
                    config,
                    source_cell,
                    source_digit,
                    true,
                )?;
                let target = branch.target_node(key, target_on);
                assert_ne!(target, NO_NODE, "selected cell-branch target");
                result.push(materialize_multi_view(
                    grid,
                    &branch.arena,
                    target,
                    ChainProofViewKind::CellBranch {
                        branch: u8::try_from(branch_index).expect("cell branch index"),
                    },
                ));
            }
            result
        }
        MultipleChainKind::Region {
            source_region,
            source_digit,
            target_cell,
            target_digit,
            target_on,
        } => {
            let key = potential_key(target_cell, target_digit, target_on);
            let positions = grid.region_candidate_positions(source_region, source_digit);
            let region_cells = grid.topology().region_cells(source_region);
            let mut result = Vec::with_capacity(
                usize::try_from(positions.count()).expect("region branch count"),
            );
            for (branch_index, position) in positions.iter().enumerate() {
                let source_cell = CellId::new(region_cells[usize::from(position)])
                    .expect("selected region branch cell");
                let branch = &mut workspace.cell_branches[branch_index];
                run_selected_branch(
                    branch,
                    &mut working,
                    &implications,
                    &region_types,
                    &mut workspace.state,
                    &mut inner_cache,
                    mode,
                    config,
                    source_cell,
                    source_digit,
                    true,
                )?;
                let target = branch.target_node(key, target_on);
                assert_ne!(target, NO_NODE, "selected region-branch target");
                result.push(materialize_multi_view(
                    grid,
                    &branch.arena,
                    target,
                    ChainProofViewKind::RegionBranch {
                        branch: u8::try_from(branch_index).expect("region branch index"),
                    },
                ));
            }
            result
        }
    };
    Ok(SelectedChainProof::new(views))
}

fn materialize_multi_view(
    grid: &Grid,
    arena: &Arena,
    terminal: u32,
    kind: ChainProofViewKind,
) -> ChainProofView {
    let mut view_index_by_key = [NO_NODE; KEY_COUNT];
    let mut ordered_nodes = Vec::new();
    let mut pending = VecDeque::new();
    let terminal_key = arena.key(terminal);
    view_index_by_key[usize::from(terminal_key)] = 0;
    pending.push_back(terminal);
    let mut next_index = 1_u32;

    while let Some(node) = pending.pop_front() {
        ordered_nodes.push(node);
        for &parent in arena.parents(node) {
            let parent_key = arena.key(parent);
            let slot = &mut view_index_by_key[usize::from(parent_key)];
            if *slot == NO_NODE {
                *slot = next_index;
                next_index = next_index
                    .checked_add(1)
                    .expect("selected multi-chain proof node count");
                pending.push_back(parent);
            }
        }
    }

    let nodes = ordered_nodes
        .into_iter()
        .map(|node_id| {
            let node = arena.node(node_id);
            let (cell, digit) = decode_candidate(node.key);
            let mut parents = Vec::with_capacity(usize::from(node.parent_count));
            for &parent in arena.parents(node_id) {
                let parent_index = view_index_by_key[usize::from(arena.key(parent))];
                debug_assert_ne!(parent_index, NO_NODE);
                let proof_parent = ChainProofParent::new(
                    ChainNodeId::from_index(
                        usize::try_from(parent_index).expect("selected multi-chain parent index"),
                    ),
                    multi_chain_cause(grid, arena, node_id, parent),
                );
                if !parents.contains(&proof_parent) {
                    parents.push(proof_parent);
                }
            }
            ChainProofNode::new(
                cell,
                digit,
                if is_on(node.key) {
                    ChainState::On
                } else {
                    ChainState::Off
                },
                parents.into_boxed_slice(),
            )
        })
        .collect();
    ChainProofView::new(kind, nodes)
}

fn multi_chain_cause(grid: &Grid, arena: &Arena, node: u32, parent: u32) -> ChainCause {
    match arena.node(node).on_cause {
        OnCause::None => ChainCause::Derived,
        OnCause::HiddenRegion(type_index) => {
            let (cell, _) = decode_candidate(arena.key(node));
            let region_index = grid
                .topology()
                .cell_region_index(cell, usize::from(type_index))
                .expect("multi-chain cause region contains its potential");
            ChainCause::Region(
                RegionId::new(type_index, region_index).expect("multi-chain cause region id"),
            )
        }
        OnCause::NakedSingle => {
            let (cell, _) = decode_candidate(arena.key(node));
            let (parent_cell, _) = decode_candidate(arena.key(parent));
            if cell == parent_cell {
                ChainCause::Cell
            } else {
                ChainCause::Visibility
            }
        }
    }
}

fn find_multi_chain_checked(
    grid: &Grid,
    config: EngineConfig,
    mode: MultiMode,
) -> Result<Option<Inference>, LegacyFcPlusBoundary> {
    let mut best = None;
    {
        let mut sink = MultiSink::First(&mut best);
        search_multi_chain(grid, config, mode, &mut sink)?;
    }
    Ok(best.map(|candidate| candidate.materialize(grid, mode)))
}

#[allow(clippy::too_many_arguments)]
fn find_multi_chain_with_resources(
    grid: &Grid,
    config: EngineConfig,
    mode: MultiMode,
    implications: &Implications,
    region_types: &[usize],
    working: &mut Grid,
    workspace: &mut MultiWorkspace,
    inner_cache: &mut InnerChainCache,
) -> Result<Option<Inference>, LegacyFcPlusBoundary> {
    let mut best = None;
    {
        let mut sink = MultiSink::First(&mut best);
        search_multi_chain_with_resources(
            grid,
            config,
            mode,
            &mut sink,
            implications,
            region_types,
            working,
            workspace,
            inner_cache,
        )?;
    }
    Ok(best.map(|candidate| candidate.materialize(grid, mode)))
}

fn collect_multi_chain_summaries_checked(
    grid: &Grid,
    config: EngineConfig,
    mode: MultiMode,
) -> Result<Vec<Inference>, LegacyFcPlusBoundary> {
    let mut result = InferenceCollector::new();
    {
        let mut sink = MultiSink::Summaries(&mut result);
        search_multi_chain(grid, config, mode, &mut sink)?;
    }
    Ok(result.finish())
}

pub(crate) fn collect_multiple_chain_proofs(grid: &Grid, config: EngineConfig) -> Vec<NestedHint> {
    collect_multi_chain_proofs_for_mode(grid, config, MultiMode::Static)
        .expect("static multiple chains cannot reach an FCPlus boundary")
}

fn collect_multi_chain_proofs_for_mode(
    grid: &Grid,
    config: EngineConfig,
    mode: MultiMode,
) -> Result<Vec<NestedHint>, LegacyFcPlusBoundary> {
    let mut result = NestedHintCollector::new();
    {
        let mut sink = MultiSink::All(&mut result);
        search_multi_chain(grid, config, mode, &mut sink)?;
    }
    Ok(result.finish())
}

fn search_multi_chain(
    grid: &Grid,
    config: EngineConfig,
    mode: MultiMode,
    sink: &mut MultiSink<'_>,
) -> Result<(), LegacyFcPlusBoundary> {
    let implications = if mode == MultiMode::Static {
        Implications::new(grid, config)
    } else {
        Implications::weak_only(grid, config)
    };
    let region_types = active_region_types(grid, config);
    let mut working = grid.clone();
    let mut workspace = MultiWorkspace::new();
    let mut inner_cache = InnerChainCache::default();

    search_multi_chain_with_resources(
        grid,
        config,
        mode,
        sink,
        &implications,
        &region_types,
        &mut working,
        &mut workspace,
        &mut inner_cache,
    )
}

#[allow(clippy::too_many_arguments)]
fn search_multi_chain_with_resources(
    grid: &Grid,
    config: EngineConfig,
    mode: MultiMode,
    sink: &mut MultiSink<'_>,
    implications: &Implications,
    region_types: &[usize],
    working: &mut Grid,
    workspace: &mut MultiWorkspace,
    inner_cache: &mut InnerChainCache,
) -> Result<(), LegacyFcPlusBoundary> {
    for raw_cell in 0_u8..81 {
        let cell = CellId::new(raw_cell).expect("cell index loop");
        if grid.value(cell) != 0 {
            continue;
        }
        let values = grid.candidates(cell);
        let cardinality = values.count();
        if cardinality <= 2 && mode == MultiMode::Static || cardinality <= 1 {
            continue;
        }

        let mut branch_count = 0_usize;
        for digit in values.iter() {
            let on_branch = &mut workspace.cell_branches[branch_count];
            let contradiction_on = on_branch.run(
                &mut *working,
                implications,
                region_types,
                &mut workspace.state,
                &mut *inner_cache,
                mode,
                config,
                cell,
                digit,
                true,
            )?;
            if mode.is_dynamic() {
                if let Some(contradiction) = contradiction_on {
                    let complexity = on_branch
                        .arena
                        .ancestor_count(contradiction.on)
                        .checked_add(on_branch.arena.ancestor_count(contradiction.off))
                        .expect("dynamic contradiction complexity");
                    let target_key = on_branch.arena.key(contradiction.off);
                    let (target_cell, target_digit) = decode_candidate(target_key);
                    let complete_complexity = sink.needs_complete_complexity(mode).then(|| {
                        complete_proof_complexity(&[
                            (&on_branch.arena, contradiction.on),
                            (&on_branch.arena, contradiction.off),
                        ])
                    });
                    let proof = sink.needs_proof().then(|| {
                        let arena = on_branch.arena.proof_arena();
                        Arc::new(ChainProof::new(
                            ProofKind::Other,
                            vec![
                                ProofTarget {
                                    arena: Arc::clone(&arena),
                                    node: contradiction.on,
                                },
                                ProofTarget {
                                    arena,
                                    node: contradiction.off,
                                },
                            ],
                        ))
                    });
                    sink.offer(
                        grid,
                        mode,
                        complexity,
                        7,
                        MultipleChainKind::Contradiction {
                            source_cell: cell,
                            source_digit: digit,
                            source_on: true,
                            target_cell,
                            target_digit,
                        },
                        cell,
                        digit,
                        false,
                        complete_complexity,
                        proof,
                    );
                }
            }

            // Java also computes this OFF branch in static MFC, but publishes
            // neither its contradictions nor its double reductions and never
            // feeds it into region/cell intersections. Skipping that dead
            // closure nearly halves the static binary work.
            if mode.is_dynamic() {
                let contradiction_off = workspace.off_branch.run(
                    &mut *working,
                    implications,
                    region_types,
                    &mut workspace.state,
                    &mut *inner_cache,
                    mode,
                    config,
                    cell,
                    digit,
                    false,
                )?;
                if let Some(contradiction) = contradiction_off {
                    let complexity = workspace
                        .off_branch
                        .arena
                        .ancestor_count(contradiction.on)
                        .checked_add(workspace.off_branch.arena.ancestor_count(contradiction.off))
                        .expect("dynamic contradiction complexity");
                    let target_key = workspace.off_branch.arena.key(contradiction.off);
                    let (target_cell, target_digit) = decode_candidate(target_key);
                    let complete_complexity = sink.needs_complete_complexity(mode).then(|| {
                        complete_proof_complexity(&[
                            (&workspace.off_branch.arena, contradiction.on),
                            (&workspace.off_branch.arena, contradiction.off),
                        ])
                    });
                    let proof = sink.needs_proof().then(|| {
                        let arena = workspace.off_branch.arena.proof_arena();
                        Arc::new(ChainProof::new(
                            ProofKind::Other,
                            vec![
                                ProofTarget {
                                    arena: Arc::clone(&arena),
                                    node: contradiction.on,
                                },
                                ProofTarget {
                                    arena,
                                    node: contradiction.off,
                                },
                            ],
                        ))
                    });
                    sink.offer(
                        grid,
                        mode,
                        complexity,
                        7,
                        MultipleChainKind::Contradiction {
                            source_cell: cell,
                            source_digit: digit,
                            source_on: false,
                            target_cell,
                            target_digit,
                        },
                        cell,
                        digit,
                        true,
                        complete_complexity,
                        proof,
                    );
                }

                if cardinality >= 3 {
                    collect_double_reductions(
                        grid,
                        mode,
                        cell,
                        digit,
                        on_branch,
                        &mut workspace.off_branch,
                        &mut workspace.ordered_keys,
                        sink,
                    );
                }
            }

            collect_region_reductions(
                grid,
                &mut *working,
                implications,
                region_types,
                mode,
                config,
                cell,
                digit,
                on_branch,
                &mut workspace.region_branches,
                &mut workspace.state,
                &mut *inner_cache,
                &mut workspace.ordered_keys,
                sink,
            )?;
            branch_count += 1;
        }

        collect_cell_reductions(
            grid,
            mode,
            cell,
            &mut workspace.cell_branches[..branch_count],
            &mut workspace.ordered_keys,
            sink,
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn collect_double_reductions(
    grid: &Grid,
    mode: MultiMode,
    source_cell: CellId,
    source_digit: Digit,
    on_branch: &mut Branch,
    off_branch: &mut Branch,
    ordered_keys: &mut Vec<u16>,
    sink: &mut MultiSink<'_>,
) {
    for target_on in [true, false] {
        on_branch.ordered_keys(target_on, ordered_keys);
        for &key in ordered_keys.iter() {
            if !off_branch.contains(key, target_on) {
                continue;
            }
            let complexity = on_branch
                .complexity(key, target_on)
                .checked_add(off_branch.complexity(key, target_on))
                .expect("double-chain complexity");
            let (target_cell, target_digit) = decode_candidate(key);
            let on_target = on_branch.target_node(key, target_on);
            let off_target = off_branch.target_node(key, target_on);
            let complete_complexity = sink.needs_complete_complexity(mode).then(|| {
                complete_proof_complexity(&[
                    (&on_branch.arena, on_target),
                    (&off_branch.arena, off_target),
                ])
            });
            let proof = sink.needs_proof().then(|| {
                Arc::new(ChainProof::new(
                    ProofKind::Other,
                    vec![
                        ProofTarget {
                            arena: on_branch.arena.proof_arena(),
                            node: on_target,
                        },
                        ProofTarget {
                            arena: off_branch.arena.proof_arena(),
                            node: off_target,
                        },
                    ],
                ))
            });
            sink.offer(
                grid,
                mode,
                complexity,
                1,
                MultipleChainKind::Double {
                    source_cell,
                    source_digit,
                    target_cell,
                    target_digit,
                    target_on,
                },
                target_cell,
                target_digit,
                target_on,
                complete_complexity,
                proof,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_region_reductions(
    grid: &Grid,
    working: &mut Grid,
    implications: &Implications,
    region_types: &[usize],
    mode: MultiMode,
    config: EngineConfig,
    source_cell: CellId,
    source_digit: Digit,
    source_branch: &mut Branch,
    other_branches: &mut [Branch],
    state: &mut DynamicState,
    inner_cache: &mut InnerChainCache,
    ordered_keys: &mut Vec<u16>,
    sink: &mut MultiSink<'_>,
) -> Result<(), LegacyFcPlusBoundary> {
    let topology = grid.topology();
    for &type_index in region_types {
        let Some(region_index) = topology.cell_region_index(source_cell, type_index) else {
            continue;
        };
        let region =
            RegionId::new(type_index as u8, region_index).expect("configured chain region");
        let positions = grid.region_candidate_positions(region, source_digit);
        if positions.count() < 2 {
            continue;
        }
        let first_position = positions.iter().next().expect("nonempty positions");
        let first_cell = CellId::new(topology.region_cells(region)[usize::from(first_position)])
            .expect("region source cell");
        if first_cell != source_cell {
            continue;
        }

        let mut other_count = 0_usize;
        for position in positions.iter().skip(1) {
            let branch_cell = CellId::new(topology.region_cells(region)[usize::from(position)])
                .expect("region branch cell");
            other_branches[other_count].run(
                working,
                implications,
                region_types,
                state,
                inner_cache,
                mode,
                config,
                branch_cell,
                source_digit,
                true,
            )?;
            other_count += 1;
        }

        for target_on in [true, false] {
            source_branch.ordered_keys(target_on, ordered_keys);
            for &key in ordered_keys.iter() {
                if !other_branches[..other_count]
                    .iter()
                    .all(|branch| branch.contains(key, target_on))
                {
                    continue;
                }
                let mut complexity = source_branch.complexity(key, target_on);
                for branch in &mut other_branches[..other_count] {
                    complexity = complexity
                        .checked_add(branch.complexity(key, target_on))
                        .expect("region-chain complexity");
                }
                let (target_cell, target_digit) = decode_candidate(key);
                let complete_complexity = if sink.needs_complete_complexity(mode) {
                    let mut targets = Vec::with_capacity(other_count + 1);
                    targets.push((
                        &source_branch.arena,
                        source_branch.target_node(key, target_on),
                    ));
                    for branch in &other_branches[..other_count] {
                        targets.push((&branch.arena, branch.target_node(key, target_on)));
                    }
                    Some(complete_proof_complexity(&targets))
                } else {
                    None
                };
                let proof = sink.needs_proof().then(|| {
                    let mut targets = Vec::with_capacity(other_count + 1);
                    targets.push(ProofTarget {
                        arena: source_branch.arena.proof_arena(),
                        node: source_branch.target_node(key, target_on),
                    });
                    for branch in &other_branches[..other_count] {
                        targets.push(ProofTarget {
                            arena: branch.arena.proof_arena(),
                            node: branch.target_node(key, target_on),
                        });
                    }
                    Arc::new(ChainProof::new(
                        ProofKind::Region(
                            u8::try_from(type_index).expect("region proof type index"),
                        ),
                        targets,
                    ))
                });
                sink.offer(
                    grid,
                    mode,
                    complexity,
                    6,
                    MultipleChainKind::Region {
                        source_region: region,
                        source_digit,
                        target_cell,
                        target_digit,
                        target_on,
                    },
                    target_cell,
                    target_digit,
                    target_on,
                    complete_complexity,
                    proof,
                );
            }
        }
    }
    Ok(())
}

fn collect_cell_reductions(
    grid: &Grid,
    mode: MultiMode,
    source_cell: CellId,
    branches: &mut [Branch],
    ordered_keys: &mut Vec<u16>,
    sink: &mut MultiSink<'_>,
) {
    let Some((first, remaining)) = branches.split_first_mut() else {
        return;
    };
    for target_on in [true, false] {
        first.ordered_keys(target_on, ordered_keys);
        for &key in ordered_keys.iter() {
            if !remaining
                .iter()
                .all(|branch| branch.contains(key, target_on))
            {
                continue;
            }
            let mut complexity = first.complexity(key, target_on);
            for branch in &mut *remaining {
                complexity = complexity
                    .checked_add(branch.complexity(key, target_on))
                    .expect("cell-chain complexity");
            }
            let (target_cell, target_digit) = decode_candidate(key);
            let complete_complexity = if sink.needs_complete_complexity(mode) {
                let mut targets = Vec::with_capacity(remaining.len() + 1);
                targets.push((&first.arena, first.target_node(key, target_on)));
                for branch in &*remaining {
                    targets.push((&branch.arena, branch.target_node(key, target_on)));
                }
                Some(complete_proof_complexity(&targets))
            } else {
                None
            };
            let proof = sink.needs_proof().then(|| {
                let mut targets = Vec::with_capacity(remaining.len() + 1);
                targets.push(ProofTarget {
                    arena: first.arena.proof_arena(),
                    node: first.target_node(key, target_on),
                });
                for branch in &*remaining {
                    targets.push(ProofTarget {
                        arena: branch.arena.proof_arena(),
                        node: branch.target_node(key, target_on),
                    });
                }
                Arc::new(ChainProof::new(ProofKind::Cell, targets))
            });
            sink.offer(
                grid,
                mode,
                complexity,
                5,
                MultipleChainKind::Cell {
                    source_cell,
                    target_cell,
                    target_digit,
                    target_on,
                },
                target_cell,
                target_digit,
                target_on,
                complete_complexity,
                proof,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn ranked_target(
    grid: &Grid,
    mode: MultiMode,
    complexity: u32,
    sort_key: u8,
    kind: MultipleChainKind,
    target_cell: CellId,
    target_digit: Digit,
    target_on: bool,
) -> Option<RankedMulti> {
    #[cfg(test)]
    RANKED_TARGET_MATERIALIZATIONS.with(|count| count.set(count.get() + 1));
    let (rating, java_difficulty) = multi_rating(mode, complexity);
    let evidence = Evidence::MultipleForcingChain {
        dynamic: mode.is_dynamic(),
        level: mode.level(),
        kind,
        complexity,
    };
    let removals = target_removals(grid, target_cell, target_digit, target_on)?;
    let inference = if target_on {
        Inference::placement(
            mode.technique(),
            rating,
            target_cell,
            target_digit,
            evidence,
        )
    } else {
        Inference::elimination(mode.technique(), rating, removals, evidence)
    };
    Some(RankedMulti {
        inference,
        java_difficulty,
        complexity,
        sort_key,
    })
}

fn target_removals(
    grid: &Grid,
    target_cell: CellId,
    target_digit: Digit,
    target_on: bool,
) -> Option<CandidateRemovals> {
    let digits = if target_on {
        grid.candidates(target_cell)
            .without(CandidateMask::of(target_digit))
    } else if grid.candidates(target_cell).contains(target_digit) {
        CandidateMask::of(target_digit)
    } else {
        return None;
    };
    if digits.is_empty() {
        return None;
    }
    let mut removals = CandidateRemovalsBuilder::with_capacity(1);
    removals.add(target_cell, digits);
    Some(removals.build())
}

fn target_has_removals(
    grid: &Grid,
    target_cell: CellId,
    target_digit: Digit,
    target_on: bool,
) -> bool {
    if target_on {
        !grid
            .candidates(target_cell)
            .without(CandidateMask::of(target_digit))
            .is_empty()
    } else {
        grid.candidates(target_cell).contains(target_digit)
    }
}

fn keep_best_draft(best: &mut Option<RankedMultiDraft>, candidate: RankedMultiDraft) {
    if best
        .as_ref()
        .is_none_or(|current| candidate.precedes(current))
    {
        *best = Some(candidate);
    }
}

fn multi_rating(mode: MultiMode, complexity: u32) -> (Rating, f64) {
    let (base_tenths, base) = mode.base_rating();
    let length = i64::from(complexity) - 2;
    let mut ceiling = 4_i64;
    let mut odd = false;
    let mut added = 0.0_f64;
    let mut increments = 0_u16;
    while length > ceiling {
        added += 0.1;
        increments += 1;
        ceiling = if !odd {
            ceiling * 3 / 2
        } else {
            ceiling * 4 / 3
        };
        odd = !odd;
    }
    (Rating::from_tenths(base_tenths + increments), base + added)
}

fn candidate_index(cell: CellId, digit: Digit) -> usize {
    cell.index() * 9 + usize::from(digit.get() - 1)
}

fn effective_variant_latin(grid: &Grid, config: EngineConfig) -> bool {
    let variant = grid.topology().config();
    config.variant_latin
        || !(variant.disjoint_groups
            || variant.windows
            || variant.sudoku_x
            || variant.girandola
            || variant.asterisk
            || variant.center_dot
            || variant.anti_ferz
            || variant.anti_knight)
}

fn generalized_family_order(grid: &Grid) -> Vec<usize> {
    let mut result = Vec::with_capacity(REGION_TYPE_COUNT);
    if grid.topology().config().blocks {
        result.push(0);
    }
    result.extend([1, 2]);
    for type_index in 3..REGION_TYPE_COUNT {
        if grid.topology().is_region_type_active(type_index) {
            result.push(type_index);
        }
    }
    result
}

fn extended_family_order(grid: &Grid, config: EngineConfig) -> Vec<usize> {
    let mut result = Vec::with_capacity(REGION_TYPE_COUNT);
    if grid.topology().config().blocks {
        result.push(0);
    }
    result.extend([2, 1]);
    if config.variant_latin {
        return result;
    }
    for type_index in 3..REGION_TYPE_COUNT {
        if grid.topology().is_region_type_active(type_index) {
            result.push(type_index);
        }
    }
    result
}

fn naked_family_order(grid: &Grid) -> Vec<usize> {
    let mut result = Vec::with_capacity(5);
    if grid.topology().config().blocks {
        result.push(0);
    }
    result.extend([2, 1]);
    if grid.topology().config().disjoint_groups {
        result.push(3);
    }
    if grid.topology().config().windows {
        result.push(4);
    }
    result
}

fn region_id(type_index: usize, region_index: usize) -> RegionId {
    RegionId::new(type_index as u8, region_index as u8).expect("region identity")
}

fn cell_id(raw: u8) -> CellId {
    CellId::new(raw).expect("cell identity")
}

fn region_cell(grid: &Grid, region: RegionId, position: u8) -> CellId {
    cell_id(grid.topology().region_cells(region)[usize::from(position)])
}

fn empty_cell_count(grid: &Grid, region: RegionId) -> usize {
    grid.topology()
        .region_cells(region)
        .iter()
        .filter(|&&raw| grid.value(cell_id(raw)) == 0)
        .count()
}

fn tuple_cell_mask(grid: &Grid, region: RegionId, positions: PositionMask) -> CellMask {
    let mut result = CellMask::EMPTY;
    for position in positions.iter() {
        result.insert(region_cell(grid, region, position));
    }
    result
}

fn combination_masks(degree: u8) -> CombinationMasks {
    CombinationMasks {
        next: (1_u16 << degree) - 1,
    }
}

struct CombinationMasks {
    next: u16,
}

impl Iterator for CombinationMasks {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next;
        if current >= 512 {
            return None;
        }
        let smallest = current & current.wrapping_neg();
        let ripple = current + smallest;
        let shifted_ones = ((current ^ ripple) >> 2) / smallest;
        self.next = ripple | shifted_ones;
        Some(current)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{
        CandidateMask, CellId, ConstraintTopology, Digit, Grid, Puzzle, RegionId, VariantConfig,
    };

    use super::{
        Branch, DynamicState, GridStateKey, Implications, InnerChainCache, LegacyFcPlusBoundary,
        MultiMode, active_region_types, collect_dynamic_forcing_chain_plus_checked,
        collect_dynamic_forcing_chains, collect_multiple_chain_proofs,
        collect_multiple_forcing_chains, collect_nested_forcing_chains_checked,
        coordinate_hash_map_cell_order, find_dynamic_forcing_chain,
        find_dynamic_forcing_chain_plus, find_dynamic_forcing_chain_plus_checked,
        find_dynamic_forcing_chain_plus_with_proof_checked, find_dynamic_forcing_chain_with_proof,
        find_multiple_forcing_chain, find_multiple_forcing_chain_with_proof,
        find_nested_forcing_chain, find_nested_forcing_chain_with_proof_checked,
        first_broken_fcplus_two_family, replay_dynamic_forcing_chain_plus_with_proof_checked,
        replay_dynamic_forcing_chain_with_proof, replay_multiple_forcing_chain_with_proof,
        replay_nested_forcing_chain_with_proof_checked,
    };
    use crate::se121::Se121Solver;
    use crate::{
        ChainCause, ChainProofView, ChainProofViewKind, ChainState, EngineConfig, Evidence,
        MultipleChainKind, Rating, RatingMode, SearchOutcome, Solver, Technique,
    };

    type ProofNodeShape = (u8, u8, ChainState, Vec<(usize, ChainCause)>);

    fn proof_shape(view: &ChainProofView) -> Vec<ProofNodeShape> {
        view.nodes()
            .iter()
            .map(|node| {
                (
                    node.cell().raw(),
                    node.digit().get(),
                    node.state(),
                    node.parents()
                        .iter()
                        .map(|parent| (parent.node().index(), parent.cause()))
                        .collect(),
                )
            })
            .collect()
    }

    fn region(type_index: u8, region_index: u8) -> ChainCause {
        ChainCause::Region(RegionId::new(type_index, region_index).unwrap())
    }

    fn has_derived_edge(proof: &crate::SelectedChainProof) -> bool {
        proof.views().iter().any(|view| {
            view.nodes().iter().any(|node| {
                node.parents()
                    .iter()
                    .any(|parent| parent.cause() == ChainCause::Derived)
            })
        })
    }

    fn sparse_snapshot(entries: &[(u8, &str)]) -> Grid {
        sparse_snapshot_with_variant(entries, VariantConfig::default())
    }

    fn sparse_snapshot_with_variant(entries: &[(u8, &str)], variant: VariantConfig) -> Grid {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut slots = vec![".........".to_owned(); 81];
        for &(raw, digits) in entries {
            let mut slot = ['.'; 9];
            for byte in digits.bytes() {
                slot[usize::from(byte - b'1')] = char::from(byte);
            }
            slots[usize::from(raw)] = slot.into_iter().collect();
        }
        let candidates = Puzzle::parse(&slots.concat()).unwrap();
        Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(variant)),
            &values,
            &candidates,
        )
        .unwrap()
    }

    fn nested_snapshot(candidates: &str) -> Grid {
        Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &Puzzle::parse(
                "12.3.....4.5...6...7.....2.6..1..3....453.........8..9...45.1.........8......2..7",
            )
            .unwrap(),
            &Puzzle::parse(candidates).unwrap(),
        )
        .unwrap()
    }

    const LEVEL_THREE_CANDIDATES: &str = "1.........2............6.89..3.........4.6789...4567.9...45.789...45.7.9...45..8....4.......3....89....5.....2....78912....7891.....7.9.....6...1.3...7.91.3....8...3....89......7....3..6.89.....6.891..4.6.891..456......45..89.2.......1.345..8......6.......5..89.2....7891.........2.4..7.9...4..7.9..3.........45.7...2.45..8..2....7891......89...4.........5......3...........67.9.2....78.1....67..12...6.8..23.5.7..1.3.5....123...7...2...67...2.4.67.........8..2.45.7..1..4567..........9.23...789..3..6.89.23...789...4.........5......3..67.91..........3..6..9.23..6....23.5.7.91..456..9123...7.9.....67.91....67.91.3..67.9.2.45...9.......8..23456.....3.5...91.3456..91.3.....9.....6.891....6.89.2..........45...9..3456..9......7..";

    const LEVEL_FOUR_CANDIDATES: &str = "1.........2............6.89..3.........4.6789...456..9...45.789...45.7.9...45..8....4.......3....89....5.....2....789.2....7891.....7.9.....6...1.3...7.91.3....8...3....89......7....3..6.89.....6.891..4.6.891..456......45..89.2.......1.345.........6.......5..89.2....7891.........2.4..7.9...4..7.9..3.........45.7...2.45..8..2....7891......89...4.........5......3...........67.9.2....78.1....67..12...6.8..23.5.7..1.3.5....123...7...2...67...2.4.67.........8..2.45.7..1..456...........9.23...789..3..6.89.23...789...4.........5......3..67.91..........3..6..9.23..6....23.5.7.9...456...123...7.9.....67.91....67.91.3..67.9.2.45...9.......8..23456.....3.5...91.3456..91.3.....9.....6.891....6.89.2..........45...9..3456..9......7..";

    fn mask(digits: &str) -> CandidateMask {
        let mut bits = 0_u16;
        for byte in digits.bytes() {
            bits |= 1_u16 << (byte - b'0');
        }
        CandidateMask::from_bits(bits)
    }

    fn remove_for_advanced(
        branch: &mut Branch,
        state: &mut DynamicState,
        grid: &mut Grid,
        raw_cell: u8,
        raw_digit: u8,
    ) -> u32 {
        let cell = CellId::new(raw_cell).unwrap();
        let digit = Digit::new(raw_digit).unwrap();
        let key = super::potential_key(cell, digit, false);
        let node = branch.arena.root(key);
        assert!(branch.to_off.add_if_absent(&branch.arena, node));
        state.remove(grid, key, node);
        node
    }

    fn parent_keys(branch: &Branch, node: u32) -> Vec<u16> {
        branch.arena.parents[branch.arena.parent_range(node)]
            .iter()
            .map(|&parent| branch.arena.key(parent))
            .collect()
    }

    type PendingNodeShape = ((u8, u8), Vec<(u8, u8)>);

    fn decoded_key(key: u16) -> (u8, u8) {
        let (cell, digit) = super::decode_candidate(key);
        (cell.raw(), digit.get())
    }

    fn pending_node_shapes(branch: &Branch) -> Vec<PendingNodeShape> {
        branch
            .pending_off
            .iter()
            .map(|&node| {
                (
                    decoded_key(branch.arena.key(node)),
                    parent_keys(branch, node)
                        .into_iter()
                        .map(decoded_key)
                        .collect(),
                )
            })
            .collect()
    }

    fn with_legacy_se121_target_order<T>(run: impl FnOnce() -> T) -> T {
        struct Restore(bool);

        impl Drop for Restore {
            fn drop(&mut self) {
                super::APPLY_SE121_ORDER_CORRECTION.with(|enabled| enabled.set(self.0));
            }
        }

        let previous_order =
            super::APPLY_SE121_ORDER_CORRECTION.with(|enabled| enabled.replace(false));
        let _restore = Restore(previous_order);
        run()
    }

    fn with_se121_delta_scan<T>(enabled: bool, run: impl FnOnce() -> T) -> T {
        struct Restore(bool);

        impl Drop for Restore {
            fn drop(&mut self) {
                super::APPLY_SE121_DELTA_SCAN.with(|setting| setting.set(self.0));
            }
        }

        let previous = super::APPLY_SE121_DELTA_SCAN.with(|setting| setting.replace(enabled));
        let _restore = Restore(previous);
        run()
    }

    fn classic_grid(puzzle: &str) -> Grid {
        Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &Puzzle::parse(puzzle).unwrap(),
        )
    }

    #[test]
    fn corrected_se121_public_finder_uses_cell_then_digit_target_order() {
        let grid = classic_grid(
            "........1.....2....34..........5..6...17..3..8....9..4...6...7...8..4..9.2..3.5..",
        );
        let config = crate::se121::SE121_ENGINE_CONFIG;
        let corrected = super::find_se121_dynamic_forcing_chain_plus(&grid, config)
            .expect("corrected SE121 DFC+ hint");
        let full_scan = with_se121_delta_scan(false, || {
            super::find_se121_dynamic_forcing_chain_plus(&grid, config)
                .expect("full-scan corrected SE121 DFC+ hint")
        });
        assert_eq!(corrected, full_scan, "production delta/full exact A/B");
        let legacy = with_legacy_se121_target_order(|| {
            super::find_se121_dynamic_forcing_chain_plus(&grid, config)
                .expect("coordinate-hash-order SE121 DFC+ hint")
        });

        assert_eq!(corrected.rating(), Rating::from_tenths(95));
        assert_eq!(corrected.removals(), legacy.removals());
        assert_ne!(corrected, legacy);

        let Evidence::MultipleForcingChain {
            dynamic: true,
            level: 1,
            kind:
                MultipleChainKind::Cell {
                    source_cell,
                    target_cell,
                    target_digit,
                    target_on: false,
                },
            complexity: 26,
        } = corrected.evidence()
        else {
            panic!("corrected cell-then-digit winner: {corrected:?}");
        };
        assert_eq!(source_cell, CellId::new(49).unwrap());
        assert_eq!(target_cell, CellId::new(70).unwrap());
        assert_eq!(target_digit, Digit::new(2).unwrap());

        let Evidence::MultipleForcingChain {
            dynamic: true,
            level: 1,
            kind:
                MultipleChainKind::Double {
                    source_cell,
                    source_digit,
                    target_cell,
                    target_digit,
                    target_on: false,
                },
            complexity: 25,
        } = legacy.evidence()
        else {
            panic!("coordinate-hash-order winner: {legacy:?}");
        };
        assert_eq!(source_cell, CellId::new(58).unwrap());
        assert_eq!(source_digit, Digit::new(2).unwrap());
        assert_eq!(target_cell, CellId::new(70).unwrap());
        assert_eq!(target_digit, Digit::new(2).unwrap());
    }

    #[test]
    #[ignore = "slow nonprotected 10.5 nested full-search delta/full differential"]
    fn se121_first_ai_escargot_nested_inference_matches_full_scan_exactly() {
        let mut grid = classic_grid(
            "1....7.9..3..2...8..96..5....53..9...1..8...26....4...3......1..4......7..7...3..",
        );
        let mut compared_nested = false;
        for _ in 0..512 {
            let inference = Se121Solver
                .next_inference(&grid)
                .unwrap()
                .expect("AI Escargot remains solvable by the corrected registry");
            if inference.technique() == Technique::NestedForcingChain {
                let delta = with_se121_delta_scan(true, || {
                    super::find_se121_chain_tail_with_session(
                        &grid,
                        crate::se121::SE121_ENGINE_CONFIG,
                        &mut super::Se121ChainSession::default(),
                    )
                    .expect("delta nested inference")
                });
                let full = with_se121_delta_scan(false, || {
                    super::find_se121_chain_tail_with_session(
                        &grid,
                        crate::se121::SE121_ENGINE_CONFIG,
                        &mut super::Se121ChainSession::default(),
                    )
                    .expect("full-scan nested inference")
                });
                assert_eq!(delta, inference, "fresh delta production entry");
                assert_eq!(delta, full, "nested delta/full exact A/B");
                compared_nested = true;
                break;
            }
            inference.apply(&mut grid);
        }
        assert!(compared_nested, "fixture never reached a nested inference");
    }

    #[test]
    fn se121_modes_select_the_old_locking_traversal() {
        assert!(MultiMode::Se121DynamicPlus.uses_se121_locking_order());
        for level in 2..=5 {
            assert!(MultiMode::Se121Nested { level }.uses_se121_locking_order());
        }
        assert!(!MultiMode::DynamicPlus.uses_se121_locking_order());
        assert!(
            !MultiMode::Nested {
                level: 2,
                nesting_limit: 0,
            }
            .uses_se121_locking_order()
        );
    }

    #[test]
    fn se121_delta_scan_is_gated_to_corrected_classic_fcplus_zero() {
        let classic = sparse_snapshot(&[(0, "12")]);
        let config = crate::se121::SE121_ENGINE_CONFIG;
        assert!(super::uses_se121_delta_advanced_scan(
            &classic,
            config,
            MultiMode::Se121DynamicPlus,
        ));
        assert!(super::uses_se121_delta_advanced_scan(
            &classic,
            config,
            MultiMode::Se121Nested { level: 5 },
        ));
        assert!(!super::uses_se121_delta_advanced_scan(
            &classic,
            config,
            MultiMode::DynamicPlus,
        ));
        assert!(!super::uses_se121_delta_advanced_scan(
            &classic,
            EngineConfig {
                forcing_chain_plus: 1,
                ..config
            },
            MultiMode::Se121DynamicPlus,
        ));

        let variant = sparse_snapshot_with_variant(
            &[(0, "12")],
            VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            },
        );
        assert!(!super::uses_se121_delta_advanced_scan(
            &variant,
            config,
            MultiMode::Se121DynamicPlus,
        ));
        with_se121_delta_scan(false, || {
            assert!(!super::uses_se121_delta_advanced_scan(
                &classic,
                config,
                MultiMode::Se121DynamicPlus,
            ));
        });
    }

    #[test]
    fn corrected_se121_modes_continue_after_a_stale_advanced_family() {
        let stale = super::AdvancedScan {
            productive: true,
            added: false,
            boundary: None,
        };
        let added = super::AdvancedScan {
            productive: true,
            added: true,
            boundary: None,
        };
        let boundary = super::AdvancedScan::at_boundary(LegacyFcPlusBoundary::UniqueLoops);

        for mode in [
            MultiMode::DynamicPlus,
            MultiMode::Nested {
                level: 3,
                nesting_limit: 0,
            },
        ] {
            assert!(mode.advanced_family_stops(stale));
            assert!(mode.advanced_family_stops(added));
            assert!(mode.advanced_family_stops(boundary));
        }

        for mode in [
            MultiMode::Se121DynamicPlus,
            MultiMode::Se121Nested { level: 3 },
        ] {
            assert!(!mode.advanced_family_stops(stale));
            assert!(mode.advanced_family_stops(added));
            assert!(mode.advanced_family_stops(boundary));
        }
    }

    #[test]
    fn corrected_se121_scanner_ladder_continues_from_stale_locking_to_hidden_pair() {
        fn setup() -> (Grid, Branch, DynamicState) {
            let mut grid = sparse_snapshot(&[
                (0, "2"),
                (1, "2"),
                (9, "2"),
                (27, "2"),
                (3, "1"),
                (4, "1"),
                (12, "1"),
                (30, "1"),
                (72, "789"),
                (73, "678"),
                (74, "78"),
            ]);
            let mut branch = Branch::new();
            let mut state = DynamicState::new();
            state.begin(true);

            remove_for_advanced(&mut branch, &mut state, &mut grid, 1, 2);
            remove_for_advanced(&mut branch, &mut state, &mut grid, 4, 1);
            remove_for_advanced(&mut branch, &mut state, &mut grid, 74, 7);
            remove_for_advanced(&mut branch, &mut state, &mut grid, 74, 8);

            // The two genuine Locking effects are already known to the
            // branch, but deliberately remain visible in this scanner-state
            // fixture so the real family reports productive-but-stale.
            for (raw_cell, raw_digit) in [(27, 2), (30, 1)] {
                let key = super::potential_key(
                    CellId::new(raw_cell).unwrap(),
                    Digit::new(raw_digit).unwrap(),
                    false,
                );
                let node = branch.arena.root(key);
                assert!(branch.to_off.add_if_absent(&branch.arena, node));
            }
            (grid, branch, state)
        }

        let (grid, mut stale_branch, stale_state) = setup();
        assert_eq!(
            stale_branch.scan_locking_se121(&grid, &stale_state),
            super::AdvancedScan {
                productive: true,
                added: false,
                boundary: None,
            },
            "the first real family must be productive but entirely stale"
        );

        let config = crate::se121::SE121_ENGINE_CONFIG;
        let (grid, mut corrected, state) = setup();
        let corrected_scan = corrected.collect_advanced(
            &grid,
            &state,
            &mut InnerChainCache::default(),
            MultiMode::Se121DynamicPlus,
            config,
        );
        assert_eq!(
            corrected_scan,
            super::AdvancedScan {
                productive: true,
                added: true,
                boundary: None,
            }
        );
        assert_eq!(
            corrected
                .pending_off
                .iter()
                .map(|&node| {
                    let (cell, digit) = super::decode_candidate(corrected.arena.key(node));
                    (cell.raw(), digit.get())
                })
                .collect::<Vec<_>>(),
            [(72_u8, 9_u8), (73, 6)],
            "the later Hidden Pair family must contribute the new OFFs"
        );

        let (grid, mut legacy, state) = setup();
        let legacy_scan = legacy.collect_advanced(
            &grid,
            &state,
            &mut InnerChainCache::default(),
            MultiMode::DynamicPlus,
            config,
        );
        assert_eq!(
            legacy_scan,
            super::AdvancedScan {
                productive: true,
                added: false,
                boundary: None,
            }
        );
        assert!(
            legacy.pending_off.is_empty(),
            "the historical general policy must still prune after stale Locking"
        );
    }

    #[test]
    fn se121_delta_backlogs_survive_an_earlier_family_stop_and_match_full_ordered_parents() {
        type Pass = (super::AdvancedScan, Vec<PendingNodeShape>);

        fn run(delta_enabled: bool) -> (Pass, Pass) {
            with_se121_delta_scan(delta_enabled, || {
                let mut grid = sparse_snapshot(&[
                    (0, "2"),
                    (1, "2"),
                    (9, "2"),
                    (27, "2"),
                    (3, "1"),
                    (4, "1"),
                    (12, "1"),
                    (30, "1"),
                    (72, "789"),
                    (73, "678"),
                    (74, "78"),
                ]);
                let mut branch = Branch::new();
                let mut state = DynamicState::new();
                state.begin(delta_enabled);
                for (cell, digit) in [(1, 2), (4, 1), (74, 7), (74, 8)] {
                    remove_for_advanced(&mut branch, &mut state, &mut grid, cell, digit);
                }

                let config = crate::se121::SE121_ENGINE_CONFIG;
                let first_scan = branch.collect_advanced(
                    &grid,
                    &state,
                    &mut InnerChainCache::default(),
                    MultiMode::Se121DynamicPlus,
                    config,
                );
                let first_shape = pending_node_shapes(&branch);
                if delta_enabled {
                    assert_eq!(
                        branch.se121_advanced_family_cursors,
                        [4, 0, 0, 0],
                        "later families must retain the removals skipped after Locking"
                    );
                }

                while let Some(node) = branch.pending_off.pop_front() {
                    let key = branch.arena.key(node);
                    state.remove(&mut grid, key, node);
                }

                let second_scan = branch.collect_advanced(
                    &grid,
                    &state,
                    &mut InnerChainCache::default(),
                    MultiMode::Se121DynamicPlus,
                    config,
                );
                let second_shape = pending_node_shapes(&branch);
                if delta_enabled {
                    assert_eq!(
                        branch.se121_advanced_family_cursors,
                        [6, 6, 0, 0],
                        "Hidden Pair must consume the complete six-removal backlog"
                    );
                }
                ((first_scan, first_shape), (second_scan, second_shape))
            })
        }

        let full = run(false);
        let delta = run(true);
        assert_eq!(delta, full, "delta/full advanced passes must be exact");
        assert_eq!(
            delta.0,
            (
                super::AdvancedScan {
                    productive: true,
                    added: true,
                    boundary: None,
                },
                vec![((27, 2), vec![(1, 2)]), ((30, 1), vec![(4, 1)]),],
            )
        );
        assert_eq!(
            delta.1,
            (
                super::AdvancedScan {
                    productive: true,
                    added: true,
                    boundary: None,
                },
                vec![
                    ((72, 9), vec![(74, 7), (74, 8)]),
                    ((73, 6), vec![(74, 7), (74, 8)]),
                ],
            )
        );
    }

    #[test]
    fn se121_delta_naked_pair_and_xwing_match_full_ordered_parents() {
        fn naked_pair(scoped: bool) -> (super::AdvancedScan, Vec<PendingNodeShape>) {
            let mut grid = sparse_snapshot(&[(0, "123"), (1, "12"), (2, "124")]);
            let mut branch = Branch::new();
            let mut state = DynamicState::new();
            state.begin(true);
            remove_for_advanced(&mut branch, &mut state, &mut grid, 0, 3);
            let scan = if scoped {
                let delta = super::AdvancedDelta::from_removed_keys(&state.removed_keys);
                branch.scan_naked_sets_scoped(
                    &grid,
                    &state,
                    crate::se121::SE121_ENGINE_CONFIG,
                    false,
                    2,
                    Some(&delta),
                )
            } else {
                branch.scan_naked_sets(&grid, &state, crate::se121::SE121_ENGINE_CONFIG, false, 2)
            };
            (scan, pending_node_shapes(&branch))
        }

        fn xwing(scoped: bool) -> (super::AdvancedScan, Vec<PendingNodeShape>) {
            let mut grid = sparse_snapshot(&[
                (0, "12"),
                (1, "12"),
                (2, "14"),
                (9, "12"),
                (10, "12"),
                (18, "13"),
            ]);
            let mut branch = Branch::new();
            let mut state = DynamicState::new();
            state.begin(true);
            remove_for_advanced(&mut branch, &mut state, &mut grid, 18, 1);
            let scan = if scoped {
                let delta = super::AdvancedDelta::from_removed_keys(&state.removed_keys);
                branch.scan_fish_scoped(&grid, &state, 2, Some(&delta))
            } else {
                branch.scan_fish(&grid, &state, 2)
            };
            (scan, pending_node_shapes(&branch))
        }

        let naked_full = naked_pair(false);
        assert_eq!(naked_pair(true), naked_full);
        assert_eq!(
            naked_full,
            (
                super::AdvancedScan {
                    productive: true,
                    added: true,
                    boundary: None,
                },
                vec![((2, 1), vec![(0, 3)]), ((2, 2), vec![(0, 3)]),],
            )
        );

        let xwing_full = xwing(false);
        assert_eq!(xwing(true), xwing_full);
        assert_eq!(
            xwing_full,
            (
                super::AdvancedScan {
                    productive: true,
                    added: true,
                    boundary: None,
                },
                vec![((2, 1), vec![(18, 1)])],
            )
        );
    }

    #[test]
    fn se121_advanced_locking_visits_regions_before_digits() {
        fn scan(se121: bool) -> Vec<(u8, u8)> {
            let mut grid = sparse_snapshot(&[
                (0, "2"),
                (1, "2"),
                (9, "2"),
                (27, "2"),
                (3, "1"),
                (4, "1"),
                (12, "1"),
                (30, "1"),
            ]);
            let mut branch = Branch::new();
            let mut state = DynamicState::new();
            remove_for_advanced(&mut branch, &mut state, &mut grid, 1, 2);
            remove_for_advanced(&mut branch, &mut state, &mut grid, 4, 1);

            let result = if se121 {
                branch.scan_locking_se121(&grid, &state)
            } else {
                branch.scan_locking(&grid, &state)
            };
            assert_eq!(
                result,
                super::AdvancedScan {
                    productive: true,
                    added: true,
                    boundary: None,
                }
            );
            branch
                .pending_off
                .iter()
                .map(|&node| {
                    let (cell, digit) = super::decode_candidate(branch.arena.key(node));
                    (cell.raw(), digit.get())
                })
                .collect()
        }

        assert_eq!(scan(false), [(30, 1), (27, 2)]);
        assert_eq!(scan(true), [(27, 2), (30, 1)]);
    }

    #[test]
    fn nested_advanced_targets_preserve_coordinate_hash_bucket_order() {
        let first_touch =
            [31_u8, 49, 39, 41].map(|raw| CellId::new(raw).expect("advanced target cell"));
        assert_eq!(
            MultiMode::DynamicPlus.advanced_target_order(),
            super::AdvancedTargetOrder::CoordinateHash
        );
        assert_eq!(
            first_touch.map(CellId::raw),
            [31_u8, 49, 39, 41],
            "compact removal first-touch order"
        );
        let mut ordered = Vec::new();
        coordinate_hash_map_cell_order(&first_touch, &mut ordered);
        assert_eq!(
            ordered.into_iter().map(CellId::raw).collect::<Vec<_>>(),
            [49_u8, 39, 41, 31],
            "default-capacity coordinate-hash bucket order"
        );
    }

    #[test]
    fn corrected_se121_advanced_targets_are_cell_then_digit_ordered() {
        let mut branch = Branch::new();
        branch.advanced_target_policy = MultiMode::Se121Nested { level: 2 }.advanced_target_order();
        assert_eq!(
            branch.advanced_target_policy,
            super::AdvancedTargetOrder::CellThenDigit
        );

        let parent = branch.arena.root(super::potential_key(
            CellId::new(80).unwrap(),
            Digit::new(9).unwrap(),
            false,
        ));
        branch.advanced_parents.push(parent);
        for (raw_cell, digits) in [(49, "31"), (31, "92"), (41, "4"), (39, "75")] {
            branch.advanced_target(CellId::new(raw_cell).unwrap(), mask(digits));
        }

        assert_eq!(
            branch.commit_advanced_hint(),
            super::AdvancedScan {
                productive: true,
                added: true,
                boundary: None,
            }
        );
        let ordered = branch
            .pending_off
            .iter()
            .map(|&node| {
                let (cell, digit) = super::decode_candidate(branch.arena.key(node));
                (cell.raw(), digit.get())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ordered,
            [
                (31_u8, 2_u8),
                (31, 9),
                (39, 5),
                (39, 7),
                (41, 4),
                (49, 1),
                (49, 3),
            ]
        );
    }

    #[test]
    fn inner_chain_cache_reuses_only_an_exact_grid_state_and_family() {
        let grid = sparse_snapshot(&[(0, "12")]);
        let config = EngineConfig::default();
        let mut cache = InnerChainCache::default();

        let forcing = cache.forcing(&grid, config);
        let forcing_hit = cache.forcing(&grid, config);
        assert!(Arc::ptr_eq(&forcing, &forcing_hit));
        assert_eq!(cache.forcing.len(), 1);

        let multiple = cache.multiple(&grid, config);
        let multiple_hit = cache.multiple(&grid, config);
        assert!(Arc::ptr_eq(&multiple, &multiple_hit));
        assert_eq!(cache.multiple.len(), 1);
        assert!(!Arc::ptr_eq(&forcing, &multiple));

        let empty = sparse_snapshot(&[]);
        let dynamic = cache
            .multi(&empty, config, MultiMode::Dynamic)
            .expect("level-zero cache is infallible");
        let dynamic_hit = cache
            .multi(&empty, config, MultiMode::Dynamic)
            .expect("cached level-zero result");
        assert!(Arc::ptr_eq(&dynamic, &dynamic_hit));
        assert_eq!(cache.dynamic.len(), 1);

        let dynamic_plus = cache
            .multi(&empty, config, MultiMode::DynamicPlus)
            .expect("empty DFC+ state");
        assert!(!Arc::ptr_eq(&dynamic, &dynamic_plus));
        assert_eq!(cache.dynamic_plus.len(), 1);

        let nested_two = cache
            .multi(
                &empty,
                config,
                MultiMode::Nested {
                    level: 2,
                    nesting_limit: 0,
                },
            )
            .expect("empty nested-two state");
        assert!(!Arc::ptr_eq(&dynamic, &nested_two));
        assert_eq!(cache.nested_two.len(), 1);

        let nested_three = cache
            .multi(
                &empty,
                config,
                MultiMode::Nested {
                    level: 3,
                    nesting_limit: 0,
                },
            )
            .expect("empty nested-three state");
        assert!(!Arc::ptr_eq(&nested_two, &nested_three));
        assert_eq!(cache.nested_three.len(), 1);

        let mut changed = grid.clone();
        changed.remove_candidate(CellId::new(0).unwrap(), Digit::new(2).unwrap());
        let changed_forcing = cache.forcing(&changed, config);
        assert!(!Arc::ptr_eq(&forcing, &changed_forcing));
        assert_eq!(cache.forcing.len(), 2);

        let mut solved = grid.clone();
        solved.place(CellId::new(80).unwrap(), Digit::new(9).unwrap());
        assert!(GridStateKey::new(&grid) != GridStateKey::new(&solved));
    }

    #[test]
    fn se121_negative_fcc_cache_survives_scope_clear_and_keys_values_exactly() {
        let solved = classic_grid(
            "123456789456789123789123456214365897365897214897214365531642978642978531978531642",
        );
        let config = crate::se121::SE121_ENGINE_CONFIG;
        let mut cache = InnerChainCache::default();

        assert!(cache.forcing_se121(&solved, config).is_empty());
        assert_eq!(cache.se121_forcing_computations, 1);
        assert_eq!(cache.se121_forcing_negative_hits, 0);
        assert_eq!(cache.se121_forcing_negative.len(), 1);
        assert!(
            cache.forcing.is_empty(),
            "negative results retain no proof slice"
        );

        cache.clear_local_results();
        assert!(cache.forcing_se121(&solved, config).is_empty());
        assert_eq!(
            cache.se121_forcing_computations, 1,
            "the second scope must hit the session negative cache"
        );
        assert_eq!(cache.se121_forcing_negative_hits, 1);

        let mut changed_value = solved.clone();
        changed_value.place(CellId::new(0).unwrap(), Digit::new(2).unwrap());
        assert!(cache.forcing_se121(&changed_value, config).is_empty());
        assert_eq!(cache.se121_forcing_computations, 2);
        assert_eq!(cache.se121_forcing_negative.len(), 2);
    }

    #[test]
    fn first_hint_draft_materializes_the_same_placement_and_elimination() {
        let grid = sparse_snapshot(&[(0, "123")]);
        let mode = MultiMode::Dynamic;
        let target_cell = CellId::new(0).unwrap();
        let target_digit = Digit::new(1).unwrap();

        for target_on in [true, false] {
            let kind = crate::MultipleChainKind::Cell {
                source_cell: target_cell,
                target_cell,
                target_digit,
                target_on,
            };
            let draft = super::RankedMultiDraft::new(
                &grid,
                mode,
                17,
                5,
                kind,
                target_cell,
                target_digit,
                target_on,
            )
            .expect("applicable draft");
            let direct = super::ranked_target(
                &grid,
                mode,
                17,
                5,
                kind,
                target_cell,
                target_digit,
                target_on,
            )
            .expect("applicable materialized candidate")
            .inference;
            assert_eq!(draft.materialize(&grid, mode), direct);
        }
    }

    #[test]
    fn first_hint_search_materializes_only_the_ranked_winner() {
        let grid = sparse_snapshot(&[
            (0, "129"),
            (10, "157"),
            (11, "124"),
            (12, "17"),
            (19, "128"),
            (20, "189"),
            (21, "17"),
            (28, "146"),
            (29, "157"),
            (80, "129"),
            (70, "157"),
            (69, "124"),
            (68, "17"),
            (61, "128"),
            (60, "189"),
            (59, "17"),
            (52, "146"),
            (51, "157"),
        ]);
        super::FIRST_DRAFT_OFFERS.with(|count| count.set(0));
        super::RANKED_TARGET_MATERIALIZATIONS.with(|count| count.set(0));

        assert!(find_dynamic_forcing_chain(&grid, EngineConfig::default()).is_some());
        let offers = super::FIRST_DRAFT_OFFERS.with(std::cell::Cell::get);
        let materializations = super::RANKED_TARGET_MATERIALIZATIONS.with(std::cell::Cell::get);
        assert!(
            offers > 1,
            "fixture must expose losing first-hint candidates"
        );
        assert_eq!(materializations, 1, "only the ranked winner is allocated");
    }

    #[test]
    fn fcplus_one_adds_java_xy_wing_implications_in_both_rating_modes() {
        for rating_mode in [RatingMode::Original, RatingMode::Revised] {
            let mut grid = sparse_snapshot(&[(0, "123"), (3, "13"), (27, "23"), (30, "34")]);
            let mut branch = Branch::new();
            let mut state = DynamicState::new();
            let parent = remove_for_advanced(&mut branch, &mut state, &mut grid, 0, 3);
            let config = EngineConfig {
                forcing_chain_plus: 1,
                rating_mode,
                ..EngineConfig::default()
            };
            let scan =
                branch.collect_level_one_advanced(&grid, &state, config, MultiMode::DynamicPlus);
            assert_eq!(
                scan,
                super::AdvancedScan {
                    productive: true,
                    added: true,
                    boundary: None,
                }
            );
            let target_key =
                super::potential_key(CellId::new(30).unwrap(), Digit::new(3).unwrap(), false);
            let target = branch.to_off.node(target_key);
            assert_ne!(target, super::NO_NODE);
            assert_eq!(parent_keys(&branch, target), vec![branch.arena.key(parent)]);
        }

        let mut grid = sparse_snapshot(&[(0, "123"), (3, "13"), (27, "23"), (30, "34")]);
        let mut branch = Branch::new();
        let mut state = DynamicState::new();
        remove_for_advanced(&mut branch, &mut state, &mut grid, 0, 3);
        assert_eq!(
            branch.collect_level_one_advanced(
                &grid,
                &state,
                EngineConfig::default(),
                MultiMode::DynamicPlus,
            ),
            super::AdvancedScan::default(),
            "FCPlus=0 must retain the pinned legacy schedule"
        );
    }

    #[test]
    fn fcplus_two_hidden_triplet_uses_java_parent_and_effect_order() {
        let mut grid = sparse_snapshot(&[(0, "124"), (1, "235"), (2, "136"), (3, "17")]);
        let mut branch = Branch::new();
        let mut state = DynamicState::new();
        let parent = remove_for_advanced(&mut branch, &mut state, &mut grid, 3, 1);
        let scan = branch.scan_hidden_sets(&grid, &state, EngineConfig::default(), 3);
        assert!(scan.productive && scan.added);
        for (raw_cell, raw_digit) in [(0, 4), (1, 5), (2, 6)] {
            let key = super::potential_key(
                CellId::new(raw_cell).unwrap(),
                Digit::new(raw_digit).unwrap(),
                false,
            );
            let target = branch.to_off.node(key);
            assert_ne!(target, super::NO_NODE);
            assert_eq!(parent_keys(&branch, target), vec![branch.arena.key(parent)]);
        }
    }

    #[test]
    fn fcplus_two_naked_triplet_and_swordfish_match_java_effects() {
        let mut naked_grid = sparse_snapshot(&[(0, "124"), (1, "13"), (2, "23"), (3, "1234")]);
        let mut naked_branch = Branch::new();
        let mut naked_state = DynamicState::new();
        let naked_parent =
            remove_for_advanced(&mut naked_branch, &mut naked_state, &mut naked_grid, 0, 4);
        let naked = naked_branch.scan_naked_sets(
            &naked_grid,
            &naked_state,
            EngineConfig::default(),
            false,
            3,
        );
        assert!(naked.productive && naked.added);
        for raw_digit in 1_u8..=3 {
            let key = super::potential_key(
                CellId::new(3).unwrap(),
                Digit::new(raw_digit).unwrap(),
                false,
            );
            let target = naked_branch.to_off.node(key);
            assert_ne!(target, super::NO_NODE);
            assert_eq!(
                parent_keys(&naked_branch, target),
                vec![naked_branch.arena.key(naked_parent)]
            );
        }

        let mut fish_grid = sparse_snapshot(&[
            (0, "12"),
            (9, "12"),
            (10, "12"),
            (19, "12"),
            (2, "12"),
            (20, "12"),
            (27, "13"),
            (3, "14"),
        ]);
        let mut fish_branch = Branch::new();
        let mut fish_state = DynamicState::new();
        let fish_parent =
            remove_for_advanced(&mut fish_branch, &mut fish_state, &mut fish_grid, 27, 1);
        let fish = fish_branch.scan_fish(&fish_grid, &fish_state, 3);
        assert!(fish.productive && fish.added);
        let victim_key =
            super::potential_key(CellId::new(3).unwrap(), Digit::new(1).unwrap(), false);
        let victim = fish_branch.to_off.node(victim_key);
        assert_ne!(victim, super::NO_NODE);
        assert_eq!(
            parent_keys(&fish_branch, victim),
            vec![fish_branch.arena.key(fish_parent)]
        );
    }

    #[test]
    fn fcplus_two_wxyz_and_vwxyz_keep_sorted_family_payloads() {
        for (degree, entries, parent_cell, parent_digit, victim, victim_digit) in [
            (
                4,
                vec![(0, "125"), (1, "13"), (3, "12"), (9, "24"), (2, "2")],
                0,
                5,
                2,
                2,
            ),
            (
                5,
                vec![
                    (0, "136"),
                    (1, "14"),
                    (2, "35"),
                    (3, "25"),
                    (9, "12"),
                    (12, "28"),
                ],
                0,
                6,
                12,
                2,
            ),
        ] {
            let mut grid = sparse_snapshot(&entries);
            let mut branch = Branch::new();
            let mut state = DynamicState::new();
            let parent = remove_for_advanced(
                &mut branch,
                &mut state,
                &mut grid,
                parent_cell,
                parent_digit,
            );
            let scan = branch.scan_alphabet_wings(&grid, &state, degree);
            assert!(scan.productive && scan.added);
            let key = super::potential_key(
                CellId::new(victim).unwrap(),
                Digit::new(victim_digit).unwrap(),
                false,
            );
            let target = branch.to_off.node(key);
            assert_ne!(target, super::NO_NODE);
            assert_eq!(parent_keys(&branch, target), vec![branch.arena.key(parent)]);
        }
    }

    #[test]
    fn fcplus_two_stops_at_the_java_parent_interface_boundary() {
        let ate = sparse_snapshot(&[(0, "12"), (1, "13"), (2, "14"), (3, "123"), (4, "124")]);
        assert_eq!(
            first_broken_fcplus_two_family(&ate, EngineConfig::default()),
            Some(LegacyFcPlusBoundary::AlignedTripletExclusion)
        );

        let unique_loop = sparse_snapshot(&[(0, "12"), (3, "12"), (9, "12"), (12, "123")]);
        assert_eq!(
            first_broken_fcplus_two_family(&unique_loop, EngineConfig::default()),
            Some(LegacyFcPlusBoundary::UniqueLoops)
        );
    }

    #[test]
    fn checked_fcplus_two_reports_the_pinned_java_hard_case_boundary() {
        let grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &Puzzle::parse(
                "........1.....2....34..........5..6...17..3..8....9..4...6...7...8..4..9.2..3.5..",
            )
            .unwrap(),
        );
        for rating_mode in [RatingMode::Original, RatingMode::Revised] {
            let config = EngineConfig {
                forcing_chain_plus: 2,
                rating_mode,
                ..EngineConfig::default()
            };
            assert_eq!(
                find_dynamic_forcing_chain_plus_checked(&grid, config),
                Err(LegacyFcPlusBoundary::UniqueLoops),
                "both pinned Java engines throw at UniqueLoopType1Hint here"
            );
        }
    }

    fn classic_trace_grid() -> Grid {
        Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &Puzzle::parse(
                "100000002520070049009000500000689000000703000090105030640010025010000070900000008",
            )
            .unwrap(),
        )
    }

    #[test]
    fn static_region_reduction_matches_java_branch_order() {
        let mut grid = sparse_snapshot(&[(0, "123"), (1, "24"), (2, "25"), (10, "26")]);
        let inference = find_multiple_forcing_chain(&grid, EngineConfig::default())
            .expect("region forcing chain");
        let detailed = find_multiple_forcing_chain_with_proof(&grid, EngineConfig::default())
            .expect("selected region forcing proof");
        assert_eq!(detailed.inference(), &inference);
        let views = detailed.proof().views();
        assert_eq!(
            views.iter().map(ChainProofView::kind).collect::<Vec<_>>(),
            [
                ChainProofViewKind::RegionBranch { branch: 0 },
                ChainProofViewKind::RegionBranch { branch: 1 },
                ChainProofViewKind::RegionBranch { branch: 2 },
            ]
        );
        for (branch, source_cell) in [0_u8, 1, 2].into_iter().enumerate() {
            assert_eq!(
                proof_shape(&views[branch]),
                [
                    (10, 2, ChainState::Off, vec![(1, region(0, 0))]),
                    (source_cell, 2, ChainState::On, vec![]),
                ]
            );
        }
        assert_eq!(inference.rating(), Rating::from_tenths(80));
        assert_eq!(inference.name(), "Region Forcing Chains");
        assert_eq!(inference.short_name(), "RFC");
        assert_eq!(
            inference.description(grid.topology()),
            "Region Forcing Chains: 2 in row ==> r2c2.2 off"
        );
        let Evidence::MultipleForcingChain {
            dynamic: false,
            level: 0,
            kind: MultipleChainKind::Region { .. },
            complexity,
        } = inference.evidence()
        else {
            panic!("static region evidence");
        };
        assert_eq!(complexity, 6);
        inference.apply(&mut grid);
        assert_eq!(grid.candidates(CellId::new(10).unwrap()), mask("6"));
    }

    #[test]
    fn all_static_multiple_chains_keep_rank_order_and_replay_a_nonfirst_proof() {
        let grid = sparse_snapshot(&[
            (0, "123"),
            (1, "24"),
            (2, "25"),
            (10, "26"),
            (39, "178"),
            (40, "27"),
            (41, "37"),
            (49, "47"),
        ]);
        let config = EngineConfig::default();
        let hints = collect_multiple_forcing_chains(&grid, config);
        assert!(hints.len() > 1, "fixture must expose multiple MFC effects");
        assert_eq!(
            find_multiple_forcing_chain(&grid, config).as_ref(),
            hints.first()
        );

        let selected = &hints[1];
        let first_replay =
            replay_multiple_forcing_chain_with_proof(&grid, config, selected).unwrap();
        let second_replay =
            replay_multiple_forcing_chain_with_proof(&grid, config, selected).unwrap();
        assert_eq!(first_replay.inference(), selected);
        assert_eq!(first_replay.proof(), second_replay.proof());
    }

    #[test]
    fn nested_static_mfc_collector_keeps_region_branch_order_and_complexity() {
        let grid = sparse_snapshot(&[(0, "123"), (1, "24"), (2, "25"), (10, "26")]);
        let hints = collect_multiple_chain_proofs(&grid, EngineConfig::default());
        let first = hints.first().expect("nested static region chain");

        assert_eq!(first.java_difficulty, 8.0);
        assert_eq!(first.complexity, 6);
        assert_eq!(first.proof.complexity(), 6);
        assert_eq!(first.sort_key, 6);
        let effect = first.removals.iter().collect::<Vec<_>>();
        assert_eq!(effect.len(), 1);
        assert_eq!(effect[0].cell(), CellId::new(10).unwrap());
        assert_eq!(effect[0].digits(), mask("2"));
    }

    #[test]
    fn dynamic_live_removals_create_java_contradiction() {
        let grid = sparse_snapshot(&[
            (0, "129"),
            (10, "157"),
            (11, "124"),
            (12, "17"),
            (19, "128"),
            (20, "189"),
            (21, "17"),
            (28, "146"),
            (29, "157"),
        ]);
        let config = EngineConfig::default();
        let implications = Implications::new(&grid, config);
        let region_types = active_region_types(&grid, config);
        let mut working = grid.clone();
        let mut state = DynamicState::new();
        let mut branch = Branch::new();
        let mut inner_cache = InnerChainCache::default();
        let contradiction = branch
            .run(
                &mut working,
                &implications,
                &region_types,
                &mut state,
                &mut inner_cache,
                MultiMode::Dynamic,
                config,
                CellId::new(0).unwrap(),
                sukaku_forge_core::Digit::new(1).unwrap(),
                true,
            )
            .expect("level zero has no FCPlus boundary")
            .expect("dynamic-only contradiction");
        assert_eq!(
            super::decode_candidate(branch.arena.key(contradiction.off)),
            (
                CellId::new(21).unwrap(),
                sukaku_forge_core::Digit::new(1).unwrap()
            )
        );
        let complexity = branch
            .arena
            .ancestor_count(contradiction.on)
            .checked_add(branch.arena.ancestor_count(contradiction.off))
            .unwrap();
        assert_eq!(complexity, 9);
        assert_eq!(working.candidates(CellId::new(0).unwrap()), mask("129"));

        let inference = find_dynamic_forcing_chain(&grid, EngineConfig::default())
            .expect("first dynamic contradiction");
        let detailed = find_dynamic_forcing_chain_with_proof(&grid, EngineConfig::default())
            .expect("selected dynamic contradiction proof");
        assert_eq!(detailed.inference(), &inference);
        let views = detailed.proof().views();
        assert_eq!(
            views.iter().map(ChainProofView::kind).collect::<Vec<_>>(),
            [
                ChainProofViewKind::ContradictionOn,
                ChainProofViewKind::ContradictionOff,
            ]
        );
        assert_eq!(
            proof_shape(&views[0]),
            [
                (
                    21,
                    1,
                    ChainState::On,
                    vec![(1, region(1, 2)), (2, region(1, 2))],
                ),
                (20, 1, ChainState::Off, vec![(3, region(0, 0))]),
                (19, 1, ChainState::Off, vec![(3, region(0, 0))]),
                (0, 1, ChainState::On, vec![]),
            ]
        );
        assert_eq!(
            proof_shape(&views[1]),
            [
                (21, 1, ChainState::Off, vec![(1, region(0, 1))]),
                (
                    12,
                    1,
                    ChainState::On,
                    vec![(2, region(1, 1)), (3, region(1, 1))],
                ),
                (11, 1, ChainState::Off, vec![(4, region(0, 0))]),
                (10, 1, ChainState::Off, vec![(4, region(0, 0))]),
                (0, 1, ChainState::On, vec![]),
            ]
        );
        assert_eq!(inference.rating(), Rating::from_tenths(87));
        assert_eq!(inference.name(), "Dynamic Contradiction Forcing Chains");
        assert_eq!(inference.short_name(), "DCFC");
        assert_eq!(
            inference.description(grid.topology()),
            "Contradiction Forcing Chain: r1c1.1 on ==> r3c4.1 both on & off"
        );
    }

    #[test]
    fn all_level_zero_dynamic_chains_keep_rank_order_and_replay_nonfirst() {
        let grid = sparse_snapshot(&[
            (0, "129"),
            (10, "157"),
            (11, "124"),
            (12, "17"),
            (19, "128"),
            (20, "189"),
            (21, "17"),
            (28, "146"),
            (29, "157"),
            (80, "129"),
            (70, "157"),
            (69, "124"),
            (68, "17"),
            (61, "128"),
            (60, "189"),
            (59, "17"),
            (52, "146"),
            (51, "157"),
        ]);
        let config = EngineConfig::default();
        let hints = collect_dynamic_forcing_chains(&grid, config);
        assert!(hints.len() > 1, "fixture must expose multiple DFC effects");
        assert_eq!(
            find_dynamic_forcing_chain(&grid, config).as_ref(),
            hints.first()
        );

        let selected = &hints[1];
        let first_replay =
            replay_dynamic_forcing_chain_with_proof(&grid, config, selected).unwrap();
        let second_replay =
            replay_dynamic_forcing_chain_with_proof(&grid, config, selected).unwrap();
        assert_eq!(first_replay.inference(), selected);
        assert_eq!(first_replay.proof(), second_replay.proof());
    }

    #[test]
    fn dynamic_weak_anti_knight_cause_survives_selected_materialization() {
        let grid = sparse_snapshot_with_variant(
            &[(4, "12"), (15, "1")],
            VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            },
        );
        let config = EngineConfig::default();
        let implications = Implications::weak_only(&grid, config);
        let region_types = active_region_types(&grid, config);
        let mut working = grid.clone();
        let mut state = DynamicState::new();
        let mut branch = Branch::new();
        let mut inner_cache = InnerChainCache::default();
        branch
            .run(
                &mut working,
                &implications,
                &region_types,
                &mut state,
                &mut inner_cache,
                MultiMode::Dynamic,
                config,
                CellId::new(4).unwrap(),
                Digit::new(1).unwrap(),
                true,
            )
            .expect("level zero has no FCPlus boundary");
        let key = super::potential_key(CellId::new(15).unwrap(), Digit::new(1).unwrap(), false);
        let compact_target = branch.target_node(key, false);
        assert_ne!(compact_target, super::NO_NODE);
        assert_eq!(
            branch.arena.node(compact_target).on_cause,
            super::OnCause::None
        );

        branch
            .run_with_proof(
                &mut working,
                &implications,
                &region_types,
                &mut state,
                &mut inner_cache,
                MultiMode::Dynamic,
                config,
                CellId::new(4).unwrap(),
                Digit::new(1).unwrap(),
                true,
            )
            .expect("level zero has no FCPlus boundary");

        let target = branch.target_node(key, false);
        assert_ne!(target, super::NO_NODE);
        assert_eq!(
            branch.arena.node(target).on_cause,
            super::OnCause::NakedSingle
        );
        let view = super::materialize_multi_view(
            &grid,
            &branch.arena,
            target,
            ChainProofViewKind::AssumptionOn,
        );
        assert_eq!(
            proof_shape(&view),
            [
                (15, 1, ChainState::Off, vec![(1, ChainCause::Visibility)],),
                (4, 1, ChainState::On, vec![]),
            ]
        );
    }

    #[test]
    fn static_mode_cannot_publish_the_dynamic_only_removal() {
        let grid = sparse_snapshot(&[
            (0, "129"),
            (10, "157"),
            (11, "124"),
            (12, "17"),
            (19, "128"),
            (20, "189"),
            (21, "17"),
            (28, "146"),
            (29, "157"),
        ]);
        let static_result = find_multiple_forcing_chain(&grid, EngineConfig::default());
        assert!(static_result.is_none_or(|inference| {
            !inference.removals().iter().any(|removal| {
                removal.cell() == CellId::new(0).unwrap()
                    && removal
                        .digits()
                        .contains(sukaku_forge_core::Digit::new(1).unwrap())
            })
        }));
    }

    #[test]
    fn classic_trace_matches_java_multiple_then_dynamic_frontier() {
        let mut grid = classic_trace_grid();
        let solver = Solver::default();
        for step in 1..=9 {
            let SearchOutcome::Found(inference) = solver.next_inference(&grid) else {
                panic!("classic trace step {step}");
            };
            if step == 8 {
                assert_eq!(inference.rating(), Rating::from_tenths(83));
                assert_eq!(inference.name(), "Cell Forcing Chains");
                assert_eq!(inference.short_name(), "LFC");
                assert_eq!(
                    inference.description(grid.topology()),
                    "Cell Forcing Chains: r7c4 ==> r3c4.3 off"
                );
                let Evidence::MultipleForcingChain { complexity, .. } = inference.evidence() else {
                    panic!("first MFC trace evidence");
                };
                assert_eq!(complexity, 12);
            } else if step == 9 {
                assert_eq!(
                    inference.description(grid.topology()),
                    "Cell Forcing Chains: r7c4 ==> r9c4.3 off"
                );
                let Evidence::MultipleForcingChain { complexity, .. } = inference.evidence() else {
                    panic!("second MFC trace evidence");
                };
                assert_eq!(complexity, 14);
            }
            inference.apply(&mut grid);
        }

        assert!(find_multiple_forcing_chain(&grid, EngineConfig::default()).is_none());
        let contradiction = find_dynamic_forcing_chain(&grid, EngineConfig::default())
            .expect("classic DFC contradiction");
        assert_eq!(contradiction.rating(), Rating::from_tenths(88));
        assert_eq!(contradiction.name(), "Dynamic Contradiction Forcing Chains");
        assert_eq!(contradiction.short_name(), "DCFC");
        assert_eq!(
            contradiction.description(grid.topology()),
            "Contradiction Forcing Chain: r3c2.3 on ==> r3c1.7 both on & off"
        );
        let Evidence::MultipleForcingChain { complexity, .. } = contradiction.evidence() else {
            panic!("DFC contradiction evidence");
        };
        assert_eq!(complexity, 13);
        contradiction.apply(&mut grid);

        assert!(find_multiple_forcing_chain(&grid, EngineConfig::default()).is_none());
        let cell_reduction = find_dynamic_forcing_chain(&grid, EngineConfig::default())
            .expect("classic DFC cell reduction");
        assert_eq!(cell_reduction.rating(), Rating::from_tenths(89));
        assert_eq!(cell_reduction.name(), "Dynamic Cell Forcing Chains");
        assert_eq!(cell_reduction.short_name(), "DLFC");
        assert_eq!(
            cell_reduction.description(grid.topology()),
            "Cell Forcing Chains: r4c2 ==> r9c2.3 off"
        );
        let Evidence::MultipleForcingChain { complexity, .. } = cell_reduction.evidence() else {
            panic!("DFC cell evidence");
        };
        assert_eq!(complexity, 15);
    }

    #[test]
    fn level_one_plus_matches_java_first_hard_hint() {
        let grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &Puzzle::parse(
                "........1.....2....34..........5..6...17..3..8....9..4...6...7...8..4..9.2..3.5..",
            )
            .unwrap(),
        );
        assert!(find_dynamic_forcing_chain(&grid, EngineConfig::default()).is_none());
        let inference = find_dynamic_forcing_chain_plus(&grid, EngineConfig::default())
            .expect("first level-one DFC+ hint");
        let detailed =
            find_dynamic_forcing_chain_plus_with_proof_checked(&grid, EngineConfig::default())
                .expect("checked DFC+ proof search")
                .expect("selected DFC+ proof");
        assert_eq!(detailed.inference(), &inference);
        assert_eq!(
            detailed
                .proof()
                .views()
                .iter()
                .map(ChainProofView::kind)
                .collect::<Vec<_>>(),
            [
                ChainProofViewKind::AssumptionOn,
                ChainProofViewKind::AssumptionOff,
            ]
        );
        assert!(has_derived_edge(detailed.proof()));
        assert_eq!(inference.rating(), Rating::from_tenths(95));
        assert_eq!(inference.name(), "Dynamic Double Forcing Chains (+)");
        assert_eq!(inference.short_name(), "DdFC+");
        assert_eq!(
            inference.description(grid.topology()),
            "Double Forcing Chain: r7c5.2 on & off ==> r8c8.2 off"
        );
        let Evidence::MultipleForcingChain {
            dynamic: true,
            level: 1,
            kind: MultipleChainKind::Double { .. },
            complexity: 25,
        } = inference.evidence()
        else {
            panic!("level-one DFC+ evidence");
        };
    }

    #[test]
    fn all_level_one_dynamic_chains_keep_rank_order_and_replay_nonfirst() {
        let grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &Puzzle::parse(
                "........1.....2....34..........5..6...17..3..8....9..4...6...7...8..4..9.2..3.5..",
            )
            .unwrap(),
        );
        let config = EngineConfig::default();
        let hints = collect_dynamic_forcing_chain_plus_checked(&grid, config).unwrap();
        assert!(hints.len() > 1, "fixture must expose multiple DFC+ effects");
        assert_eq!(
            find_dynamic_forcing_chain_plus_checked(&grid, config)
                .unwrap()
                .as_ref(),
            hints.first()
        );

        let selected = &hints[1];
        let first_replay =
            replay_dynamic_forcing_chain_plus_with_proof_checked(&grid, config, selected)
                .unwrap()
                .unwrap();
        let second_replay =
            replay_dynamic_forcing_chain_plus_with_proof_checked(&grid, config, selected)
                .unwrap()
                .unwrap();
        assert_eq!(first_replay.inference(), selected);
        assert_eq!(first_replay.proof(), second_replay.proof());
    }

    #[test]
    fn level_two_matches_java_hard_hint_order() {
        let mut grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &Puzzle::parse(
                "100000002030400050006000700040603000000020000000508090007000100080009030200000006",
            )
            .unwrap(),
        );
        let inference = find_nested_forcing_chain(&grid, EngineConfig::default(), 2, 0)
            .expect("first level-two nested DFC hint");
        let detailed =
            find_nested_forcing_chain_with_proof_checked(&grid, EngineConfig::default(), 2, 0)
                .expect("checked level-two proof search")
                .expect("selected level-two proof");
        assert_eq!(detailed.inference(), &inference);
        assert_eq!(
            detailed
                .proof()
                .views()
                .iter()
                .map(ChainProofView::kind)
                .collect::<Vec<_>>(),
            [
                ChainProofViewKind::ContradictionOn,
                ChainProofViewKind::ContradictionOff,
            ]
        );
        assert!(has_derived_edge(detailed.proof()));
        assert_eq!(inference.rating(), Rating::from_tenths(104));
        assert_eq!(
            inference.name(),
            "Dynamic Contradiction Forcing Chains (+ Forcing Chains)"
        );
        assert_eq!(inference.short_name(), "DCFC+FC");
        assert_eq!(
            inference.description(grid.topology()),
            "Contradiction Forcing Chain: r3c5.1 on ==> r4c5.7 both on & off"
        );
        let Evidence::MultipleForcingChain {
            dynamic: true,
            level: 2,
            kind: MultipleChainKind::Contradiction { .. },
            complexity: 71,
        } = inference.evidence()
        else {
            panic!("level-two nested evidence");
        };
        let effect = inference.removals().iter().collect::<Vec<_>>();
        assert_eq!(effect.len(), 1);
        assert_eq!(effect[0].cell(), CellId::new(22).unwrap());
        assert_eq!(effect[0].digits(), mask("1"));

        inference.apply(&mut grid);
        let second = find_nested_forcing_chain(&grid, EngineConfig::default(), 2, 0)
            .expect("second level-two nested DFC hint");
        assert_eq!(second.rating(), Rating::from_tenths(104));
        assert_eq!(
            second.description(grid.topology()),
            "Contradiction Forcing Chain: r5c3.1 on ==> r4c5.7 both on & off"
        );
        let Evidence::MultipleForcingChain {
            dynamic: true,
            level: 2,
            kind:
                MultipleChainKind::Contradiction {
                    source_cell,
                    source_digit,
                    source_on: true,
                    ..
                },
            complexity: 71,
        } = second.evidence()
        else {
            panic!("second level-two nested evidence");
        };
        assert_eq!(source_cell, CellId::new(38).unwrap());
        assert_eq!(source_digit, Digit::new(1).unwrap());

        second.apply(&mut grid);
        let third = find_nested_forcing_chain(&grid, EngineConfig::default(), 2, 0)
            .expect("third level-two nested DFC hint");
        assert_eq!(third.rating(), Rating::from_tenths(104));
        assert_eq!(
            third.description(grid.topology()),
            "Contradiction Forcing Chain: r5c9.1 on ==> r4c5.7 both on & off"
        );
        let Evidence::MultipleForcingChain {
            dynamic: true,
            level: 2,
            kind:
                MultipleChainKind::Contradiction {
                    source_cell,
                    source_digit,
                    source_on: true,
                    ..
                },
            complexity: 71,
        } = third.evidence()
        else {
            panic!("third level-two nested evidence");
        };
        assert_eq!(source_cell, CellId::new(44).unwrap());
        assert_eq!(source_digit, Digit::new(1).unwrap());
    }

    #[test]
    fn all_level_two_nested_chains_keep_rank_order_and_replay_nonfirst() {
        let grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &Puzzle::parse(
                "100000002030400050006000700040603000000020000000508090007000100080009030200000006",
            )
            .unwrap(),
        );
        let config = EngineConfig::default();
        let hints = collect_nested_forcing_chains_checked(&grid, config, 2, 0).unwrap();
        assert!(
            hints.len() > 1,
            "fixture must expose multiple nested DFC effects"
        );
        assert_eq!(
            super::find_nested_forcing_chain_checked(&grid, config, 2, 0)
                .unwrap()
                .as_ref(),
            hints.first()
        );

        let selected = &hints[1];
        let first_replay =
            replay_nested_forcing_chain_with_proof_checked(&grid, config, 2, 0, selected)
                .unwrap()
                .unwrap();
        let second_replay =
            replay_nested_forcing_chain_with_proof_checked(&grid, config, 2, 0, selected)
                .unwrap()
                .unwrap();
        assert_eq!(first_replay.inference(), selected);
        assert_eq!(first_replay.proof(), second_replay.proof());
    }

    #[test]
    fn level_two_matches_legacy_java_10_5_step_seven_and_eight() {
        let mut grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &Puzzle::parse(
                "1.......2.3.4...5...6...7...4.8.3.......1.......5.6.9...7...6...8...9.3.2.......1",
            )
            .unwrap(),
            &Puzzle::parse(
                "1............5.7.9...45..89..3..67.9..3.56.89....5.78...34...89...4.6.8..2.............789..3.......2.....89...4......2...678912....78.1......89....5.........6.89...45..89.2..5...9.....6...123.....9..3.5..8912..5..8.......7..1..4...8...34...89....567.9...4.....12..5...9.......8..2....7.9..3......12..5....12...67......567....3.56.89.2..567.9..3.5..89.2....7.91.........2.4..7....345..8..2.4.678...345678...3...78.12....7..123....8.....5.....2.4..7.......6...1234...8.........9..34..78...345...91...5...9......7..123........345..8.12.45..8......6....2.4...8....45..89...456..........8.1..45....12...67...2.4567..........9.2.45......3.........45.7...2...........56..9..345...9..3..67....345678....45.78....45..89...4..78.1........",
            )
            .unwrap(),
        )
        .unwrap();
        let inference = find_nested_forcing_chain(&grid, EngineConfig::default(), 2, 0)
            .expect("10.5 step seven");
        assert_eq!(inference.rating(), Rating::from_tenths(104));
        assert_eq!(
            inference.description(grid.topology()),
            "Contradiction Forcing Chain: r5c9.7 on ==> r4c5.2 both on & off"
        );
        let Evidence::MultipleForcingChain {
            dynamic: true,
            level: 2,
            kind:
                MultipleChainKind::Contradiction {
                    source_cell,
                    source_digit,
                    source_on: true,
                    target_cell,
                    target_digit,
                },
            complexity: 71,
        } = inference.evidence()
        else {
            panic!("10.5 step-seven nested evidence");
        };
        assert_eq!(source_cell, CellId::new(44).unwrap());
        assert_eq!(source_digit, Digit::new(7).unwrap());
        assert_eq!(target_cell, CellId::new(31).unwrap());
        assert_eq!(target_digit, Digit::new(2).unwrap());

        inference.apply(&mut grid);
        let inference = find_nested_forcing_chain(&grid, EngineConfig::default(), 2, 0)
            .expect("10.5 step eight");
        assert_eq!(inference.rating(), Rating::from_tenths(104));
        assert_eq!(
            inference.description(grid.topology()),
            "Contradiction Forcing Chain: r9c5.7 on ==> r4c5.2 both on & off"
        );
        let Evidence::MultipleForcingChain {
            dynamic: true,
            level: 2,
            kind:
                MultipleChainKind::Contradiction {
                    source_cell,
                    source_digit,
                    source_on: true,
                    target_cell,
                    target_digit,
                },
            complexity: 69,
        } = inference.evidence()
        else {
            panic!("10.5 step-eight nested evidence");
        };
        assert_eq!(source_cell, CellId::new(76).unwrap());
        assert_eq!(source_digit, Digit::new(7).unwrap());
        assert_eq!(target_cell, CellId::new(31).unwrap());
        assert_eq!(target_digit, Digit::new(2).unwrap());
    }

    #[test]
    fn level_three_matches_java_sparse_direct_fixture() {
        let grid = nested_snapshot(LEVEL_THREE_CANDIDATES);
        let inference = find_nested_forcing_chain(&grid, EngineConfig::default(), 3, 0)
            .expect("first level-three nested DFC hint");
        assert_eq!(inference.rating(), Rating::from_tenths(110));
        assert_eq!(
            inference.name(),
            "Dynamic Contradiction Forcing Chains (+ Multiple Forcing Chains)"
        );
        assert_eq!(inference.short_name(), "DCFC+MFC");
        assert_eq!(
            inference.description(grid.topology()),
            "Contradiction Forcing Chain: r1c6.7 on ==> r5c7.7 both on & off"
        );
        let Evidence::MultipleForcingChain {
            dynamic: true,
            level: 3,
            kind: MultipleChainKind::Contradiction { .. },
            complexity: 116,
        } = inference.evidence()
        else {
            panic!("level-three nested evidence");
        };
        let effect = inference.removals().iter().collect::<Vec<_>>();
        assert_eq!(effect.len(), 1);
        assert_eq!(effect[0].cell(), CellId::new(5).unwrap());
        assert_eq!(effect[0].digits(), mask("7"));
    }

    #[test]
    fn level_four_cap_zero_matches_java_sparse_direct_fixture() {
        let grid = nested_snapshot(LEVEL_FOUR_CANDIDATES);
        let inference = find_nested_forcing_chain(&grid, EngineConfig::default(), 4, 0)
            .expect("first level-four nested DFC hint");
        let detailed =
            find_nested_forcing_chain_with_proof_checked(&grid, EngineConfig::default(), 4, 0)
                .expect("checked level-four proof search")
                .expect("selected level-four proof");
        assert_eq!(detailed.inference(), &inference);
        assert!(has_derived_edge(detailed.proof()));
        assert_eq!(inference.rating(), Rating::from_tenths(117));
        assert_eq!(
            inference.name(),
            "Dynamic Contradiction Forcing Chains (+ Dynamic Forcing Chains)"
        );
        assert_eq!(inference.short_name(), "DCFC+DFC");
        assert_eq!(
            inference.description(grid.topology()),
            "Contradiction Forcing Chain: r1c6.9 on ==> r5c7.2 both on & off"
        );
        let Evidence::MultipleForcingChain {
            dynamic: true,
            level: 4,
            kind: MultipleChainKind::Contradiction { .. },
            complexity: 207,
        } = inference.evidence()
        else {
            panic!("level-four nested evidence");
        };
        let effect = inference.removals().iter().collect::<Vec<_>>();
        assert_eq!(effect.len(), 1);
        assert_eq!(effect[0].cell(), CellId::new(5).unwrap());
        assert_eq!(effect[0].digits(), mask("9"));
    }

    #[test]
    fn level_one_plus_uses_pinned_original_candidates_for_advanced_parents() {
        let grid = sparse_snapshot(&[
            (7, "12"),
            (8, "15"),
            (16, "13"),
            (24, "135"),
            (25, "13"),
            (26, "15"),
        ]);
        let level_zero = find_dynamic_forcing_chain(&grid, EngineConfig::default())
            .expect("level-zero comparison hint");
        assert_eq!(level_zero.rating(), Rating::from_tenths(85));
        assert_eq!(level_zero.name(), "Dynamic Region Forcing Chains");
        assert_eq!(
            level_zero.description(grid.topology()),
            "Region Forcing Chains: 1 in row ==> r2c8.1 off"
        );
        let Evidence::MultipleForcingChain {
            level: 0,
            kind: MultipleChainKind::Region { .. },
            complexity: 4,
            ..
        } = level_zero.evidence()
        else {
            panic!("level-zero comparison evidence");
        };
        let inference = find_dynamic_forcing_chain_plus(&grid, EngineConfig::default())
            .expect("advanced parent contract hint");
        assert_eq!(inference.rating(), Rating::from_tenths(90));
        assert_eq!(inference.name(), "Dynamic Double Forcing Chains (+)");
        assert_eq!(inference.short_name(), "DdFC+");
        assert_eq!(
            inference.description(grid.topology()),
            "Double Forcing Chain: r3c7.1 on & off ==> r2c8.1 off"
        );
        let Evidence::MultipleForcingChain {
            dynamic: true,
            level: 1,
            kind:
                MultipleChainKind::Double {
                    source_cell,
                    source_digit,
                    target_cell,
                    target_digit,
                    target_on: false,
                },
            complexity: 4,
        } = inference.evidence()
        else {
            panic!("level-one contract evidence");
        };
        assert_eq!(source_cell, CellId::new(24).unwrap());
        assert_eq!(source_digit, sukaku_forge_core::Digit::new(1).unwrap());
        assert_eq!(target_cell, CellId::new(16).unwrap());
        assert_eq!(target_digit, sukaku_forge_core::Digit::new(1).unwrap());
        let removals = inference.removals().iter().collect::<Vec<_>>();
        assert_eq!(removals.len(), 1);
        assert_eq!(removals[0].cell(), CellId::new(16).unwrap());
        assert_eq!(removals[0].digits(), mask("1"));
    }
}
