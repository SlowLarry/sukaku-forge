use std::cell::OnceCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use sukaku_forge_core::{
    CandidateMask, CandidateRemovals, CandidateRemovalsBuilder, CellId, CellMask, Digit, Grid,
    PositionMask, REGION_TYPE_COUNT, RegionId,
};

use crate::aligned_exclusion::find_aligned_triplet_exclusion;
use crate::alphabet_wings::collect_alphabet_wing_advanced;
use crate::bug::find_bivalue_universal_grave;
use crate::forcing_chains::{
    Implications, KEY_COUNT, active_region_types, collect_forcing_chain_proofs, decode_candidate,
    is_on, potential_key,
};
use crate::nested_chains::{
    ChainProof, FullChainFingerprint, NestedHint, NestedHintCollector, OnCause, ProofArena,
    ProofKind, ProofNode, ProofTarget,
};
use crate::unique_loops::find_unique_loop;
use crate::{EngineConfig, Evidence, Inference, MultipleChainKind, Rating, Technique};

const NO_NODE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MultiMode {
    Static,
    Dynamic,
    DynamicPlus,
    Nested { level: u8, nesting_limit: u8 },
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
    multiple: HashMap<GridStateKey, Arc<[NestedHint]>>,
    dynamic: HashMap<GridStateKey, CachedInnerResult>,
    dynamic_plus: HashMap<GridStateKey, CachedInnerResult>,
    nested_two: HashMap<GridStateKey, CachedInnerResult>,
    nested_three: HashMap<GridStateKey, CachedInnerResult>,
}

impl InnerChainCache {
    fn forcing(&mut self, grid: &Grid, config: EngineConfig) -> Arc<[NestedHint]> {
        let key = GridStateKey::new(grid);
        if let Some(hints) = self.forcing.get(&key) {
            return Arc::clone(hints);
        }
        let hints = Arc::<[NestedHint]>::from(collect_forcing_chain_proofs(grid, config));
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
            MultiMode::Nested { level: 2, .. } => &mut self.nested_two,
            MultiMode::Nested { level: 3, .. } => &mut self.nested_three,
            MultiMode::Static | MultiMode::Nested { .. } => {
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
            Self::DynamicPlus => Technique::DynamicForcingChainPlus,
            Self::Nested { .. } => Technique::NestedForcingChain,
        }
    }

    fn base_rating(self) -> (u16, f64) {
        match self {
            Self::Static => (80, 8.0),
            Self::Dynamic => (85, 8.5),
            Self::DynamicPlus => (90, 9.0),
            Self::Nested { level, .. } => {
                debug_assert!((2..=4).contains(&level));
                (85 + u16::from(level) * 5, 8.5 + f64::from(level) * 0.5)
            }
        }
    }

    fn level(self) -> u8 {
        match self {
            Self::Static | Self::Dynamic => 0,
            Self::DynamicPlus => 1,
            Self::Nested { level, .. } => level,
        }
    }

    fn nesting_limit(self) -> u8 {
        match self {
            Self::Nested { nesting_limit, .. } => nesting_limit,
            Self::Static | Self::Dynamic | Self::DynamicPlus => 0,
        }
    }
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

    fn parent_range(&self, node: u32) -> std::ops::Range<usize> {
        let entry = &self.nodes[usize::try_from(node).expect("multiple-chain node index")];
        let start = usize::try_from(entry.parent_start).expect("parent start");
        start..start + usize::from(entry.parent_count)
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

/// Candidate-removal rollback journal for one dynamic branch closure.
struct DynamicState {
    changed_cells: Vec<CellId>,
    original_masks: [CandidateMask; 81],
    changed: [bool; 81],
    removed_nodes: [u32; 81 * 9],
}

impl DynamicState {
    fn new() -> Self {
        Self {
            changed_cells: Vec::with_capacity(32),
            original_masks: [CandidateMask::EMPTY; 81],
            changed: [false; 81],
            removed_nodes: [NO_NODE; 81 * 9],
        }
    }

    fn begin(&mut self) {
        debug_assert!(self.changed_cells.is_empty());
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
    }
}

#[derive(Clone, Copy)]
struct Contradiction {
    on: u32,
    off: u32,
}

/// Java's chaining hints historically expose their removals through a
/// default-constructed HashMap. Preserve its capacity-16 bucket order even
/// when the optimized Java implementation stores a compact removal payload
/// before lazily materializing that compatibility map.
fn legacy_hash_map_cell_order(cells: &[CellId], result: &mut Vec<CellId>) {
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
    advanced_nested: Option<Arc<ChainProof>>,
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
            advanced_nested: None,
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
        self.clear();
        state.begin();
        let source_key = potential_key(source_cell, source_digit, source_on);
        let source = self.arena.root(source_key);
        if source_on {
            self.to_on.add_if_absent(&self.arena, source);
            self.pending_on.push_back(source);
        } else {
            self.to_off.add_if_absent(&self.arena, source);
            self.pending_off.push_back(source);
        }

        let contradiction = self.propagate(
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
    fn propagate(
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
                self.collect_weak_keys(grid, implications, mode, parent);
                for index in 0..self.generated_keys.len() {
                    let target_key = self.generated_keys[index];
                    let target = self.arena.child(target_key, parent);
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

    fn collect_weak_keys(
        &mut self,
        grid: &Grid,
        implications: &Implications,
        mode: MultiMode,
        parent: u32,
    ) {
        self.generated_keys.clear();
        let parent_key = self.arena.key(parent);
        if mode == MultiMode::Static {
            implications.for_each_off(parent_key, true, |key| self.generated_keys.push(key));
            return;
        }

        let (source_cell, source_digit) = decode_candidate(parent_key);
        for digit in grid.candidates(source_cell).iter() {
            if digit != source_digit {
                self.generated_keys
                    .push(potential_key(source_cell, digit, false));
            }
        }
        implications.for_each_weak_off(parent_key, |key| {
            let (cell, digit) = decode_candidate(key);
            if grid.candidates(cell).contains(digit) {
                self.generated_keys.push(key);
            }
        });
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

    /// Java stores an indirect hint's removals in a `HashMap<Cell, BitSet>`.
    /// Cell hashes are their raw indexes, so reproduce the table's bucket
    /// iteration before appending the advanced OFF nodes.
    fn commit_advanced_hint(&mut self) -> AdvancedScan {
        if self.advanced_parents.is_empty() || self.advanced_target_cells.is_empty() {
            self.clear_advanced_hint();
            return AdvancedScan::default();
        }

        legacy_hash_map_cell_order(&self.advanced_target_cells, &mut self.advanced_target_order);

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
        let base = self.collect_level_one_advanced(grid, state, config);
        if base.productive || mode.level() == 1 {
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
                if scan.productive {
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
            MultiMode::Static
            | MultiMode::Dynamic
            | MultiMode::DynamicPlus
            | MultiMode::Nested { .. } => AdvancedScan::default(),
        }
    }

    fn collect_level_one_advanced(
        &mut self,
        grid: &Grid,
        state: &DynamicState,
        config: EngineConfig,
    ) -> AdvancedScan {
        let variant_latin = effective_variant_latin(grid, config);
        let first = if variant_latin {
            self.scan_locking(grid, state)
        } else {
            self.scan_generalized_intersections(grid, state)
        };
        if first.productive {
            return first;
        }
        let hidden = self.scan_hidden_sets(grid, state, config, 2);
        if hidden.productive {
            return hidden;
        }
        let naked = self.scan_naked_sets(grid, state, config, !variant_latin, 2);
        if naked.productive {
            return naked;
        }
        let fish = self.scan_fish(grid, state, 2);
        if fish.productive {
            return fish;
        }

        // TurbotFish is present at this point in Java's FCPlus > 0 schedule,
        // but its legacy getRuleParents implementation compares initialGrid
        // with initialGrid.  It therefore never contributes an implication.
        if config.forcing_chain_plus == 0 {
            return AdvancedScan::default();
        }
        let xy = self.scan_xy_wings(grid, state, false);
        if xy.productive {
            return xy;
        }
        let xyz = self.scan_xy_wings(grid, state, true);
        if xyz.productive || config.forcing_chain_plus == 1 {
            return xyz;
        }

        let hidden = self.scan_hidden_sets(grid, state, config, 3);
        if hidden.productive {
            return hidden;
        }
        let naked = self.scan_naked_sets(grid, state, config, !variant_latin, 3);
        if naked.productive {
            return naked;
        }
        let fish = self.scan_fish(grid, state, 3);
        if fish.productive {
            return fish;
        }

        // StrongLinks(3), scheduled only for classic/Latin configurations,
        // has the same inert initialGrid/initialGrid parent bug as TurbotFish.
        let wxyz = self.scan_alphabet_wings(grid, state, 4);
        if wxyz.productive {
            return wxyz;
        }
        if variant_latin {
            let vwxyz = self.scan_alphabet_wings(grid, state, 5);
            if vwxyz.productive {
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
        let mut family = AdvancedScan::default();
        for type_index in extended_family_order(grid, config) {
            for region_index in 0..grid.topology().region_count(type_index) {
                let region = region_id(type_index, region_index);
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
        let mut family = AdvancedScan::default();
        let families = if generalized {
            extended_family_order(grid, config)
        } else {
            naked_family_order(grid)
        };
        for type_index in families {
            for region_index in 0..grid.topology().region_count(type_index) {
                let region = region_id(type_index, region_index);
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
}

struct RankedMulti {
    inference: Inference,
    java_difficulty: f64,
    complexity: u32,
    sort_key: u8,
}

impl RankedMulti {
    fn precedes(&self, other: &Self) -> bool {
        if self.java_difficulty < other.java_difficulty {
            return true;
        }
        if self.java_difficulty > other.java_difficulty {
            return false;
        }
        (self.complexity, self.sort_key) < (other.complexity, other.sort_key)
    }
}

enum MultiSink<'a> {
    First(&'a mut Option<RankedMulti>),
    All(&'a mut NestedHintCollector),
}

impl MultiSink<'_> {
    fn needs_proof(&self) -> bool {
        matches!(self, Self::All(_))
    }

    fn needs_complete_complexity(&self, mode: MultiMode) -> bool {
        matches!(self, Self::First(_)) && mode.level() >= 2
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
                    keep_best(best, candidate);
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

/// Find Java's first ranked level-0 Dynamic Forcing Chain.
#[must_use]
pub fn find_dynamic_forcing_chain(grid: &Grid, config: EngineConfig) -> Option<Inference> {
    find_multi_chain_checked(grid, config, MultiMode::Dynamic)
        .expect("level-zero dynamic chains cannot reach an FCPlus boundary")
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

/// Checked DFC+ entry point for Java's historically broken FCPlus=2 tail.
pub fn find_dynamic_forcing_chain_plus_checked(
    grid: &Grid,
    config: EngineConfig,
) -> Result<Option<Inference>, LegacyFcPlusBoundary> {
    find_multi_chain_checked(grid, config, MultiMode::DynamicPlus)
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
    Ok(best.map(|candidate| candidate.inference))
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
                &mut working,
                &implications,
                &region_types,
                &mut workspace.state,
                &mut inner_cache,
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
                    &mut working,
                    &implications,
                    &region_types,
                    &mut workspace.state,
                    &mut inner_cache,
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
                &mut working,
                &implications,
                &region_types,
                mode,
                config,
                cell,
                digit,
                on_branch,
                &mut workspace.region_branches,
                &mut workspace.state,
                &mut inner_cache,
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

fn keep_best(best: &mut Option<RankedMulti>, candidate: RankedMulti) {
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
        CandidateMask, CellId, ConstraintTopology, Digit, Grid, Puzzle, VariantConfig,
    };

    use super::{
        Branch, DynamicState, GridStateKey, Implications, InnerChainCache, LegacyFcPlusBoundary,
        MultiMode, active_region_types, collect_multiple_chain_proofs, find_dynamic_forcing_chain,
        find_dynamic_forcing_chain_plus, find_dynamic_forcing_chain_plus_checked,
        find_multiple_forcing_chain, find_nested_forcing_chain, first_broken_fcplus_two_family,
        legacy_hash_map_cell_order,
    };
    use crate::{
        EngineConfig, Evidence, MultipleChainKind, Rating, RatingMode, SearchOutcome, Solver,
    };

    fn sparse_snapshot(entries: &[(u8, &str)]) -> Grid {
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
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
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

    #[test]
    fn nested_advanced_targets_preserve_legacy_hash_map_order() {
        let first_touch =
            [31_u8, 49, 39, 41].map(|raw| CellId::new(raw).expect("advanced target cell"));
        assert_eq!(
            first_touch.map(CellId::raw),
            [31_u8, 49, 39, 41],
            "compact removal first-touch order"
        );
        let mut ordered = Vec::new();
        legacy_hash_map_cell_order(&first_touch, &mut ordered);
        assert_eq!(
            ordered.into_iter().map(CellId::raw).collect::<Vec<_>>(),
            [49_u8, 39, 41, 31],
            "default-capacity Java HashMap bucket order"
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
            let scan = branch.collect_level_one_advanced(&grid, &state, config);
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
            branch.collect_level_one_advanced(&grid, &state, EngineConfig::default()),
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
        assert_eq!(inference.rating(), Rating::from_tenths(87));
        assert_eq!(inference.name(), "Dynamic Contradiction Forcing Chains");
        assert_eq!(inference.short_name(), "DCFC");
        assert_eq!(
            inference.description(grid.topology()),
            "Contradiction Forcing Chain: r1c1.1 on ==> r3c4.1 both on & off"
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
