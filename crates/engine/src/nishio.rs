use std::collections::VecDeque;

use sukaku_forge_core::{CandidateMask, CandidateRemovalsBuilder, CellId, Digit, Grid, RegionId};

use crate::forcing_chains::{
    Implications, KEY_COUNT, active_region_types, decode_candidate, is_on, potential_key,
};
use crate::nested_chains::{InferenceCollector, OnCause};
use crate::presentation_proof::{
    ChainCause, ChainNodeId, ChainProofNode, ChainProofParent, ChainProofView, ChainProofViewKind,
    ChainState, NishioForcingChainWithProof, SelectedChainProof,
};
use crate::{EngineConfig, Evidence, Inference, Rating, Technique};

const NO_NODE: u32 = u32::MAX;

#[derive(Clone, Copy)]
struct DynamicNode {
    key: u16,
    parent_start: u32,
    parent_count: u16,
}

/// Reusable implication DAG for dynamic chains.
///
/// Unlike the static-chain arena, a dynamically created hidden single has the
/// current OFF potential plus every earlier removal that made it single as
/// parents. Java counts all of those parents when ranking the hint.
struct DynamicArena {
    nodes: Vec<DynamicNode>,
    parents: Vec<u32>,
    /// Populated only by the selected replay path. Compact ranking retains
    /// the original node layout and does no presentation-cause storage.
    causes: Vec<ChainCause>,
    ancestor_stamps: [u16; KEY_COUNT],
    ancestor_generation: u16,
    traversal: Vec<u32>,
}

impl DynamicArena {
    fn new() -> Self {
        Self {
            nodes: Vec::with_capacity(128),
            parents: Vec::with_capacity(192),
            causes: Vec::new(),
            ancestor_stamps: [0; KEY_COUNT],
            ancestor_generation: 0,
            traversal: Vec::with_capacity(64),
        }
    }

    fn clear(&mut self) {
        self.nodes.clear();
        self.parents.clear();
        self.causes.clear();
    }

    fn root<const CAPTURE_CAUSES: bool>(&mut self, key: u16) -> u32 {
        self.push::<CAPTURE_CAUSES>(key, &[], ChainCause::None)
    }

    fn child<const CAPTURE_CAUSES: bool>(
        &mut self,
        key: u16,
        parent: u32,
        cause: ChainCause,
    ) -> u32 {
        self.push::<CAPTURE_CAUSES>(key, &[parent], cause)
    }

    fn push<const CAPTURE_CAUSES: bool>(
        &mut self,
        key: u16,
        parents: &[u32],
        cause: ChainCause,
    ) -> u32 {
        let node = u32::try_from(self.nodes.len()).expect("dynamic implication node index");
        let parent_start =
            u32::try_from(self.parents.len()).expect("dynamic implication parent index");
        let parent_count = u16::try_from(parents.len()).expect("dynamic implication parent count");
        self.parents.extend_from_slice(parents);
        self.nodes.push(DynamicNode {
            key,
            parent_start,
            parent_count,
        });
        if CAPTURE_CAUSES {
            debug_assert_eq!(self.causes.len() + 1, self.nodes.len());
            self.causes.push(cause);
        } else {
            debug_assert!(self.causes.is_empty());
        }
        node
    }

    /// Add an extra parent immediately after constructing the latest node.
    fn add_parent(&mut self, node: u32, parent: u32) {
        let index = usize::try_from(node).expect("dynamic implication node index");
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
            .expect("dynamic implication parent count");
    }

    fn key(&self, node: u32) -> u16 {
        self.nodes[usize::try_from(node).expect("dynamic implication node index")].key
    }

    fn node(&self, node: u32) -> DynamicNode {
        self.nodes[usize::try_from(node).expect("dynamic implication node index")]
    }

    fn parent_range(&self, node: u32) -> std::ops::Range<usize> {
        let entry = self.node(node);
        let start = usize::try_from(entry.parent_start).expect("parent start");
        start..start + usize::from(entry.parent_count)
    }

    fn parents(&self, node: u32) -> &[u32] {
        &self.parents[self.parent_range(node)]
    }

    fn cause(&self, node: u32) -> ChainCause {
        debug_assert_eq!(self.causes.len(), self.nodes.len());
        self.causes[usize::try_from(node).expect("dynamic implication node index")]
    }

    /// Java de-duplicates each terminal's ancestry by potential key, and then
    /// adds the two terminal counts rather than taking their union.
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
            result = result.checked_add(1).expect("dynamic chain complexity");
            let range = self.parent_range(node);
            self.traversal.extend_from_slice(&self.parents[range]);
        }
        result
    }
}

fn nishio_chain_cause(grid: &Grid, source_key: u16, target_key: u16, cause: OnCause) -> ChainCause {
    let (source_cell, source_digit) = decode_candidate(source_key);
    let (target_cell, target_digit) = decode_candidate(target_key);
    match cause {
        OnCause::None => ChainCause::None,
        OnCause::HiddenRegion(type_index) => {
            debug_assert_eq!(source_digit, target_digit);
            let region_index = grid
                .topology()
                .cell_region_index(target_cell, usize::from(type_index))
                .expect("Nishio cause region contains target");
            ChainCause::Region(
                RegionId::new(type_index, region_index).expect("Nishio cause region"),
            )
        }
        OnCause::NakedSingle => {
            if source_cell == target_cell {
                ChainCause::Cell
            } else {
                ChainCause::Visibility
            }
        }
    }
}

fn materialize_nishio_view(
    arena: &DynamicArena,
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
                    .expect("selected Nishio proof node count");
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
                let parent = ChainProofParent::new(
                    ChainNodeId::from_index(
                        usize::try_from(parent_index).expect("selected Nishio parent index"),
                    ),
                    arena.cause(node_id),
                );
                if !parents.contains(&parent) {
                    parents.push(parent);
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

/// Candidate-removal journal for one dynamic implication closure.
struct DynamicState {
    changed_cells: Vec<CellId>,
    original_masks: [CandidateMask; 81],
    changed: [bool; 81],
    removed_nodes: [u32; 81 * 9],
}

impl DynamicState {
    fn new() -> Self {
        Self {
            changed_cells: Vec::with_capacity(24),
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

struct DynamicChainWorkspace {
    arena: DynamicArena,
    node_by_key: [u32; KEY_COUNT],
    touched_keys: Vec<u16>,
    pending_on: VecDeque<u32>,
    pending_off: VecDeque<u32>,
    strong_nodes: Vec<u32>,
    strong_cell_stamps: [u16; 81],
    strong_generation: u16,
    state: DynamicState,
}

impl DynamicChainWorkspace {
    fn new() -> Self {
        Self {
            arena: DynamicArena::new(),
            node_by_key: [NO_NODE; KEY_COUNT],
            touched_keys: Vec::with_capacity(128),
            pending_on: VecDeque::with_capacity(64),
            pending_off: VecDeque::with_capacity(64),
            strong_nodes: Vec::with_capacity(10),
            strong_cell_stamps: [0; 81],
            strong_generation: 0,
            state: DynamicState::new(),
        }
    }

    fn clear(&mut self) {
        self.arena.clear();
        for key in self.touched_keys.drain(..) {
            self.node_by_key[usize::from(key)] = NO_NODE;
        }
        self.pending_on.clear();
        self.pending_off.clear();
        self.strong_nodes.clear();
    }

    fn remember(&mut self, node: u32) -> bool {
        let key = self.arena.key(node);
        let slot = &mut self.node_by_key[usize::from(key)];
        if *slot != NO_NODE {
            return false;
        }
        *slot = node;
        self.touched_keys.push(key);
        true
    }

    fn next_strong_generation(&mut self) -> u16 {
        self.strong_generation = self.strong_generation.wrapping_add(1);
        if self.strong_generation == 0 {
            self.strong_cell_stamps.fill(0);
            self.strong_generation = 1;
        }
        self.strong_generation
    }

    /// Return Java's first contradictory ON/OFF terminal pair.
    fn contradiction(
        &mut self,
        grid: &mut Grid,
        implications: &Implications,
        region_types: &[usize],
        source_cell: CellId,
        source_digit: Digit,
        source_on: bool,
    ) -> Option<(u32, u32)> {
        self.contradiction_impl::<false>(
            grid,
            implications,
            region_types,
            source_cell,
            source_digit,
            source_on,
        )
    }

    fn contradiction_with_proof(
        &mut self,
        grid: &mut Grid,
        implications: &Implications,
        region_types: &[usize],
        source_cell: CellId,
        source_digit: Digit,
        source_on: bool,
    ) -> Option<(u32, u32)> {
        self.contradiction_impl::<true>(
            grid,
            implications,
            region_types,
            source_cell,
            source_digit,
            source_on,
        )
    }

    fn contradiction_impl<const CAPTURE_CAUSES: bool>(
        &mut self,
        grid: &mut Grid,
        implications: &Implications,
        region_types: &[usize],
        source_cell: CellId,
        source_digit: Digit,
        source_on: bool,
    ) -> Option<(u32, u32)> {
        self.clear();
        self.state.begin();
        let source_key = potential_key(source_cell, source_digit, source_on);
        let source = self.arena.root::<CAPTURE_CAUSES>(source_key);
        self.remember(source);
        if source_on {
            self.pending_on.push_back(source);
        } else {
            self.pending_off.push_back(source);
        }

        let result = self.propagate::<CAPTURE_CAUSES>(grid, implications, region_types);
        self.state.restore(grid);
        result
    }

    fn propagate<const CAPTURE_CAUSES: bool>(
        &mut self,
        grid: &mut Grid,
        implications: &Implications,
        region_types: &[usize],
    ) -> Option<(u32, u32)> {
        loop {
            // This precedence is intentional. Java exhausts newly queued ON
            // nodes before processing even the oldest pending OFF node.
            if let Some(parent) = self.pending_on.pop_front() {
                let parent_key = self.arena.key(parent);
                let mut contradiction = None;
                let mut emit = |target_key, cause: Option<OnCause>| {
                    if contradiction.is_some() {
                        return;
                    }
                    let (target_cell, target_digit) = decode_candidate(target_key);
                    if !grid.candidates(target_cell).contains(target_digit) {
                        return;
                    }
                    let cause = if CAPTURE_CAUSES {
                        nishio_chain_cause(
                            grid,
                            parent_key,
                            target_key,
                            cause.expect("selected Nishio weak cause"),
                        )
                    } else {
                        ChainCause::None
                    };
                    let target = self
                        .arena
                        .child::<CAPTURE_CAUSES>(target_key, parent, cause);
                    let opposite = self.node_by_key[usize::from(target_key ^ 1)];
                    if opposite != NO_NODE {
                        contradiction = Some((opposite, target));
                    } else if self.remember(target) {
                        self.pending_off.push_back(target);
                    }
                };
                if CAPTURE_CAUSES {
                    implications.for_each_weak_off_with_cause(parent_key, |target_key, cause| {
                        emit(target_key, Some(cause));
                    });
                } else {
                    implications.for_each_weak_off(parent_key, |target_key| {
                        emit(target_key, None);
                    });
                }
                if contradiction.is_some() {
                    return contradiction;
                }
                continue;
            }

            if let Some(parent) = self.pending_off.pop_front() {
                self.collect_strong_nodes::<CAPTURE_CAUSES>(grid, region_types, parent);
                let parent_key = self.arena.key(parent);
                self.state.remove(grid, parent_key, parent);
                let strong_count = self.strong_nodes.len();
                for index in 0..strong_count {
                    let target = self.strong_nodes[index];
                    let target_key = self.arena.key(target);
                    let opposite = self.node_by_key[usize::from(target_key ^ 1)];
                    if opposite != NO_NODE {
                        return Some((target, opposite));
                    }
                    if self.remember(target) {
                        self.pending_on.push_back(target);
                    }
                }
                continue;
            }

            return None;
        }
    }

    fn collect_strong_nodes<const CAPTURE_CAUSES: bool>(
        &mut self,
        grid: &Grid,
        region_types: &[usize],
        parent: u32,
    ) {
        self.strong_nodes.clear();
        let parent_key = self.arena.key(parent);
        let (source_cell, digit) = decode_candidate(parent_key);
        let generation = self.next_strong_generation();
        let topology = grid.topology();

        for &type_index in region_types {
            let Some(region_index) = topology.cell_region_index(source_cell, type_index) else {
                continue;
            };
            let region = RegionId::new(type_index as u8, region_index)
                .expect("configured dynamic-chain region");
            let mut positions = grid.region_candidate_positions(region, digit);
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

            let target_key = potential_key(target_cell, digit, true);
            let target =
                self.arena
                    .child::<CAPTURE_CAUSES>(target_key, parent, ChainCause::Region(region));
            for &raw_cell in topology.region_cells(region) {
                let cell = CellId::new(raw_cell).expect("dynamic-chain region cell");
                if self.state.original_mask(grid, cell).contains(digit)
                    && !grid.candidates(cell).contains(digit)
                {
                    let hidden_parent = self.state.removed_node(cell, digit);
                    debug_assert_ne!(hidden_parent, NO_NODE);
                    self.arena.add_parent(target, hidden_parent);
                }
            }
            self.strong_nodes.push(target);
        }
    }
}

struct RankedNishio {
    inference: Inference,
    java_difficulty: f64,
    complexity: u16,
}

#[derive(Clone, Copy)]
struct NishioProofLocator {
    source_cell: CellId,
    source_digit: Digit,
    source_on: bool,
}

impl RankedNishio {
    fn precedes(&self, other: &Self) -> bool {
        if self.java_difficulty < other.java_difficulty {
            return true;
        }
        if self.java_difficulty > other.java_difficulty {
            return false;
        }
        self.complexity < other.complexity
    }
}

fn ranked_nishio(
    arena: &mut DynamicArena,
    source_cell: CellId,
    source_digit: Digit,
    source_on: bool,
    terminal_on: u32,
    terminal_off: u32,
) -> RankedNishio {
    let contradiction_key = arena.key(terminal_off);
    debug_assert_eq!(contradiction_key ^ 1, arena.key(terminal_on));
    let (target_cell, target_digit) = decode_candidate(contradiction_key);
    let complexity = arena
        .ancestor_count(terminal_on)
        .checked_add(arena.ancestor_count(terminal_off))
        .expect("Nishio complexity");
    let (rating, java_difficulty) = nishio_rating(complexity);
    let evidence = Evidence::NishioForcingChain {
        source_cell,
        source_digit,
        source_on,
        target_cell,
        target_digit,
        complexity,
    };
    let inference = if source_on {
        let mut removals = CandidateRemovalsBuilder::with_capacity(1);
        removals.add(source_cell, CandidateMask::of(source_digit));
        Inference::elimination(
            Technique::NishioForcingChain,
            rating,
            removals.build(),
            evidence,
        )
    } else {
        Inference::placement(
            Technique::NishioForcingChain,
            rating,
            source_cell,
            source_digit,
            evidence,
        )
    };
    RankedNishio {
        inference,
        java_difficulty,
        complexity,
    }
}

/// Find Java's first ranked Nishio contradiction forcing chain.
#[must_use]
pub fn find_nishio_forcing_chain(grid: &Grid, config: EngineConfig) -> Option<Inference> {
    let implications = Implications::weak_only(grid, config);
    let region_types = active_region_types(grid, config);
    let mut working = grid.clone();
    let mut workspace = DynamicChainWorkspace::new();
    let mut best: Option<RankedNishio> = None;

    for raw_cell in 0_u8..81 {
        let cell = CellId::new(raw_cell).expect("cell index loop");
        if grid.value(cell) != 0 || grid.candidates(cell).count() <= 1 {
            continue;
        }
        for digit in grid.candidates(cell).iter() {
            // Java tests the ON assumption before the OFF assumption.
            for source_on in [true, false] {
                let Some((terminal_on, terminal_off)) = workspace.contradiction(
                    &mut working,
                    &implications,
                    &region_types,
                    cell,
                    digit,
                    source_on,
                ) else {
                    continue;
                };
                let candidate = ranked_nishio(
                    &mut workspace.arena,
                    cell,
                    digit,
                    source_on,
                    terminal_on,
                    terminal_off,
                );
                if best
                    .as_ref()
                    .is_none_or(|current| candidate.precedes(current))
                {
                    best = Some(candidate);
                }
            }
        }
    }

    best.map(|candidate| candidate.inference)
}

/// Collect every Java-ranked Nishio contradiction forcing chain.
///
/// The result is stably ranked and effect-deduplicated while retaining no
/// presentation DAGs.  Use [`replay_nishio_forcing_chain_with_proof`] after a
/// GUI row has been selected.
#[must_use]
pub fn collect_nishio_forcing_chains(grid: &Grid, config: EngineConfig) -> Vec<Inference> {
    let implications = Implications::weak_only(grid, config);
    let region_types = active_region_types(grid, config);
    let mut working = grid.clone();
    let mut workspace = DynamicChainWorkspace::new();
    let mut result = InferenceCollector::new();

    for raw_cell in 0_u8..81 {
        let cell = CellId::new(raw_cell).expect("cell index loop");
        if grid.value(cell) != 0 || grid.candidates(cell).count() <= 1 {
            continue;
        }
        for digit in grid.candidates(cell).iter() {
            for source_on in [true, false] {
                let Some((terminal_on, terminal_off)) = workspace.contradiction(
                    &mut working,
                    &implications,
                    &region_types,
                    cell,
                    digit,
                    source_on,
                ) else {
                    continue;
                };
                let candidate = ranked_nishio(
                    &mut workspace.arena,
                    cell,
                    digit,
                    source_on,
                    terminal_on,
                    terminal_off,
                );
                result.offer(
                    grid,
                    candidate.inference,
                    candidate.java_difficulty,
                    u32::from(candidate.complexity),
                    0,
                );
            }
        }
    }

    result.finish()
}

/// Find Java's first ranked Nishio chain and materialize only its selected
/// contradiction proof.
///
/// The compact finder above remains the rating path. This opt-in GUI finder
/// retains only the winning source coordinates while ranking, then replays
/// that one assumption to copy the target-ON and target-OFF ancestor DAGs.
#[must_use]
pub fn find_nishio_forcing_chain_with_proof(
    grid: &Grid,
    config: EngineConfig,
) -> Option<NishioForcingChainWithProof> {
    let implications = Implications::weak_only(grid, config);
    let region_types = active_region_types(grid, config);
    let mut working = grid.clone();
    let mut workspace = DynamicChainWorkspace::new();
    let mut best: Option<(RankedNishio, NishioProofLocator)> = None;

    for raw_cell in 0_u8..81 {
        let cell = CellId::new(raw_cell).expect("cell index loop");
        if grid.value(cell) != 0 || grid.candidates(cell).count() <= 1 {
            continue;
        }
        for digit in grid.candidates(cell).iter() {
            for source_on in [true, false] {
                let Some((terminal_on, terminal_off)) = workspace.contradiction(
                    &mut working,
                    &implications,
                    &region_types,
                    cell,
                    digit,
                    source_on,
                ) else {
                    continue;
                };
                let candidate = ranked_nishio(
                    &mut workspace.arena,
                    cell,
                    digit,
                    source_on,
                    terminal_on,
                    terminal_off,
                );
                if best
                    .as_ref()
                    .is_none_or(|(current, _)| candidate.precedes(current))
                {
                    best = Some((
                        candidate,
                        NishioProofLocator {
                            source_cell: cell,
                            source_digit: digit,
                            source_on,
                        },
                    ));
                }
            }
        }
    }

    let (winner, locator) = best?;
    let (terminal_on, terminal_off) = workspace
        .contradiction_with_proof(
            &mut working,
            &implications,
            &region_types,
            locator.source_cell,
            locator.source_digit,
            locator.source_on,
        )
        .expect("ranked Nishio contradiction is reproducible");
    let proof = SelectedChainProof::new(vec![
        materialize_nishio_view(&workspace.arena, terminal_on, ChainProofViewKind::NishioOn),
        materialize_nishio_view(
            &workspace.arena,
            terminal_off,
            ChainProofViewKind::NishioOff,
        ),
    ]);
    Some(NishioForcingChainWithProof::new(winner.inference, proof))
}

/// Replay the contradiction DAGs for any retained Nishio inference.
///
/// Returns `None` for a stale inference or one produced by another family.
#[must_use]
pub fn replay_nishio_forcing_chain_with_proof(
    grid: &Grid,
    config: EngineConfig,
    inference: &Inference,
) -> Option<NishioForcingChainWithProof> {
    if inference.technique() != Technique::NishioForcingChain {
        return None;
    }
    let Evidence::NishioForcingChain {
        source_cell,
        source_digit,
        source_on,
        ..
    } = inference.evidence()
    else {
        return None;
    };
    let implications = Implications::weak_only(grid, config);
    let region_types = active_region_types(grid, config);
    let mut working = grid.clone();
    let mut workspace = DynamicChainWorkspace::new();
    let (terminal_on, terminal_off) = workspace.contradiction_with_proof(
        &mut working,
        &implications,
        &region_types,
        source_cell,
        source_digit,
        source_on,
    )?;
    let candidate = ranked_nishio(
        &mut workspace.arena,
        source_cell,
        source_digit,
        source_on,
        terminal_on,
        terminal_off,
    );
    if candidate.inference != *inference {
        return None;
    }
    let proof = SelectedChainProof::new(vec![
        materialize_nishio_view(&workspace.arena, terminal_on, ChainProofViewKind::NishioOn),
        materialize_nishio_view(
            &workspace.arena,
            terminal_off,
            ChainProofViewKind::NishioOff,
        ),
    ]);
    Some(NishioForcingChainWithProof::new(candidate.inference, proof))
}

/// Materialize only the selected proof for an inference retained by the
/// all-hints session.
#[must_use]
pub fn replay_nishio_forcing_chain_proof(
    grid: &Grid,
    config: EngineConfig,
    inference: &Inference,
) -> SelectedChainProof {
    replay_nishio_forcing_chain_with_proof(grid, config, inference)
        .expect("retained Nishio inference is reproducible")
        .into_parts()
        .1
}

fn nishio_rating(complexity: u16) -> (Rating, f64) {
    let length = i32::from(complexity) - 2;
    let mut ceiling = 4_i32;
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
    (Rating::from_tenths(75 + increments), 7.5 + added)
}

fn candidate_index(cell: CellId, digit: Digit) -> usize {
    cell.index() * 9 + usize::from(digit.get() - 1)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{
        CandidateMask, CellId, ConstraintTopology, Digit, Grid, Puzzle, RegionId, VariantConfig,
    };

    use super::{
        DynamicChainWorkspace, Implications, active_region_types, collect_nishio_forcing_chains,
        find_nishio_forcing_chain, find_nishio_forcing_chain_with_proof, nishio_rating,
        replay_nishio_forcing_chain_with_proof,
    };
    use crate::{
        ChainCause, ChainProofView, ChainProofViewKind, ChainState, EngineConfig, Evidence, Rating,
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

    fn mask(digits: &str) -> CandidateMask {
        let mut bits = 0_u16;
        for byte in digits.bytes() {
            bits |= 1_u16 << (byte - b'0');
        }
        CandidateMask::from_bits(bits)
    }

    fn trace_snapshot(values: &str, candidates: &str, variant: VariantConfig) -> Grid {
        Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(variant)),
            &Puzzle::parse(values).unwrap(),
            &Puzzle::parse(candidates).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn source_off_contradiction_places_the_candidate() {
        let mut grid = sparse_snapshot(&[(0, "78"), (1, "79"), (9, "79")]);
        let inference = find_nishio_forcing_chain(&grid, EngineConfig::default())
            .expect("Nishio forcing chain");
        let detailed = find_nishio_forcing_chain_with_proof(&grid, EngineConfig::default())
            .expect("selected Nishio proof");
        assert_eq!(detailed.inference(), &inference);
        let views = detailed.proof().views();
        assert_eq!(views.len(), 2);
        assert_eq!(views[0].kind(), ChainProofViewKind::NishioOn);
        assert_eq!(views[1].kind(), ChainProofViewKind::NishioOff);
        assert_eq!(
            proof_shape(&views[0]),
            [
                (9, 7, ChainState::On, vec![(1, region(2, 0))]),
                (0, 7, ChainState::Off, vec![]),
            ]
        );
        assert_eq!(
            proof_shape(&views[1]),
            [
                (9, 7, ChainState::Off, vec![(1, region(0, 0))]),
                (1, 7, ChainState::On, vec![(2, region(1, 0))]),
                (0, 7, ChainState::Off, vec![]),
            ]
        );
        assert_eq!(inference.rating(), Rating::from_tenths(75));
        assert_eq!(inference.name(), "Nishio Forcing Chains");
        assert_eq!(inference.short_name(), "NFC");
        assert_eq!(
            inference.description(grid.topology()),
            "Nishio Forcing Chain: r1c1.7 off ==> r2c1.7 both on & off"
        );
        let Evidence::NishioForcingChain { complexity, .. } = inference.evidence() else {
            panic!("Nishio evidence");
        };
        assert_eq!(complexity, 5);
        inference.apply(&mut grid);
        assert_eq!(grid.value(CellId::new(0).unwrap()), 7);
        assert_eq!(
            grid.candidates(CellId::new(0).unwrap()),
            CandidateMask::EMPTY
        );
    }

    #[test]
    fn all_nishio_hints_keep_rank_order_and_replay_a_nonfirst_proof() {
        let grid = sparse_snapshot(&[
            (0, "78"),
            (1, "79"),
            (9, "79"),
            (40, "45"),
            (41, "46"),
            (49, "46"),
        ]);
        let config = EngineConfig::default();
        let hints = collect_nishio_forcing_chains(&grid, config);
        assert!(
            hints.len() > 1,
            "fixture must expose multiple Nishio effects"
        );
        assert_eq!(
            find_nishio_forcing_chain(&grid, config).as_ref(),
            hints.first()
        );

        let selected = &hints[1];
        let first_replay = replay_nishio_forcing_chain_with_proof(&grid, config, selected).unwrap();
        let second_replay =
            replay_nishio_forcing_chain_with_proof(&grid, config, selected).unwrap();
        assert_eq!(first_replay.inference(), selected);
        assert_eq!(first_replay.proof(), second_replay.proof());
    }

    #[test]
    fn compact_closure_does_not_capture_presentation_causes() {
        let grid = sparse_snapshot(&[(0, "78"), (1, "79"), (9, "79")]);
        let config = EngineConfig::default();
        let implications = Implications::weak_only(&grid, config);
        let region_types = active_region_types(&grid, config);
        let mut workspace = DynamicChainWorkspace::new();
        let mut working = grid.clone();

        workspace
            .contradiction(
                &mut working,
                &implications,
                &region_types,
                CellId::new(0).unwrap(),
                Digit::new(7).unwrap(),
                false,
            )
            .expect("compact Nishio contradiction");
        assert!(workspace.arena.causes.is_empty());

        workspace
            .contradiction_with_proof(
                &mut working,
                &implications,
                &region_types,
                CellId::new(0).unwrap(),
                Digit::new(7).unwrap(),
                false,
            )
            .expect("selected Nishio contradiction");
        assert_eq!(workspace.arena.causes.len(), workspace.arena.nodes.len());
        assert!(
            workspace
                .arena
                .causes
                .iter()
                .any(|cause| *cause != ChainCause::None)
        );
    }

    #[test]
    fn source_on_contradiction_eliminates_the_candidate() {
        let mut grid = sparse_snapshot(&[(0, "78"), (3, "79"), (4, "79")]);
        let inference = find_nishio_forcing_chain(&grid, EngineConfig::default())
            .expect("Nishio forcing chain");
        assert_eq!(inference.rating(), Rating::from_tenths(75));
        assert_eq!(
            inference.description(grid.topology()),
            "Nishio Forcing Chain: r1c1.7 on ==> r1c5.7 both on & off"
        );
        let Evidence::NishioForcingChain {
            source_on,
            complexity,
            ..
        } = inference.evidence()
        else {
            panic!("Nishio evidence");
        };
        assert!(source_on);
        assert_eq!(complexity, 5);
        inference.apply(&mut grid);
        assert_eq!(grid.candidates(CellId::new(0).unwrap()), mask("8"));
    }

    #[test]
    fn rating_uses_java_length_staircase() {
        assert_eq!(nishio_rating(6).0, Rating::from_tenths(75));
        assert_eq!(nishio_rating(7).0, Rating::from_tenths(76));
        assert_eq!(nishio_rating(9).0, Rating::from_tenths(77));
        assert_eq!(nishio_rating(45).0, Rating::from_tenths(82));
    }

    #[test]
    fn anti_knight_trace_selects_the_java_on_contradiction() {
        let mut grid = trace_snapshot(
            "....4.8.....5.8.14..4.......5....4..4.285.....3.49......5.63.4..4.7.5.6.....84...",
            "1.3.567.912...67.91.3...7.9123..6......4......2...67.9.......8..23.5.7.9.23.567.9.23..67.9.2...67....3..67.9....5.....2....7.........8..23..67.91...........4.....123.5..8.1....6789...4.....1.3..6...123...7........7.9.2..56....2..5.7.9.2..567.91....67.9....5....1....6789.23..6.....3...7..12...67.....4......2....789.2...67.9...4.....1....67.9.2..............8.....5....1.....7..1....67....3...7.91.3..67.91....67....3......1......8....4.............912...67..12....7...2..5.78.12..5.7..12....7.91......89....5....1.......9.....6.....3......12....7.....4.....12....78912.....89...4.....1.......9......7..12...........5....123.....9.....6...123....89123..67.912...6..9..3..67..12......9.......8....4.....1...5.7.9.2....7.912..5.7.9",
            VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            },
        );
        let inference = find_nishio_forcing_chain(&grid, EngineConfig::default())
            .expect("anti-knight Nishio chain");
        let detailed = find_nishio_forcing_chain_with_proof(&grid, EngineConfig::default())
            .expect("selected anti-knight Nishio proof");
        assert_eq!(detailed.inference(), &inference);
        let views = detailed.proof().views();
        assert_eq!(
            views.iter().map(ChainProofView::kind).collect::<Vec<_>>(),
            [ChainProofViewKind::NishioOn, ChainProofViewKind::NishioOff]
        );
        assert_eq!(
            proof_shape(&views[0]),
            [
                (
                    22,
                    3,
                    ChainState::On,
                    vec![(1, region(0, 1)), (2, region(0, 1))],
                ),
                (21, 3, ChainState::Off, vec![(3, ChainCause::Visibility)],),
                (3, 3, ChainState::Off, vec![(3, region(1, 0))]),
                (2, 3, ChainState::On, vec![]),
            ]
        );
        assert_eq!(
            proof_shape(&views[1]),
            [
                (22, 3, ChainState::Off, vec![(1, ChainCause::Visibility)],),
                (
                    15,
                    3,
                    ChainState::On,
                    vec![(2, region(1, 1)), (3, region(1, 1))],
                ),
                (11, 3, ChainState::Off, vec![(4, region(0, 0))]),
                (9, 3, ChainState::Off, vec![(4, region(0, 0))]),
                (2, 3, ChainState::On, vec![]),
            ]
        );
        assert_eq!(inference.rating(), Rating::from_tenths(77));
        assert_eq!(
            inference.description(grid.topology()),
            "Nishio Forcing Chain: r1c3.3 on ==> r3c5.3 both on & off"
        );
        let Evidence::NishioForcingChain {
            source_on,
            complexity,
            ..
        } = inference.evidence()
        else {
            panic!("Nishio evidence");
        };
        assert!(source_on);
        assert_eq!(complexity, 9);
        inference.apply(&mut grid);
        assert!(
            !grid
                .candidates(CellId::new(2).unwrap())
                .contains(sukaku_forge_core::Digit::new(3).unwrap())
        );
    }

    #[test]
    fn long_anti_knight_chain_retains_exact_multi_parent_complexity() {
        let mut grid = trace_snapshot(
            "....4.8.....5.8.14..4.......5....4..4.285.....3.49......5.63.4..4.7.5.6.....84...",
            "1.3.567.912...67.91.....7.9123..6......4......2...67.9.......8..23.5.7.9.23.567.9.23..67.9.2...67....3..67......5.....2....7.........8..23..67.91...........4.....123.5..8.1....6789...4.....1.3..6...123...7........7.9.2..56....2..5.7.9.2..567.91....67.9....5....1....6789.23..6.....3...7..12...67.....4......2....789.2...67.9...4.....1....67.9.2..............8.....5....1.....7..1....67....3.....91.3..67.91....67....3......1......8....4.............912...67..12....7...2..5.78.12..5.7..12....7.91......89....5....1.......9.....6.....3......12....7.....4.....12....78.12.....89...4.....1.......9......7..12...........5....123.....9.....6...123....89123..67.912...6..9..3..67..12......9.......8....4.....1...5.7.9.2....7.912..5.7.9",
            VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            },
        );
        let inference = find_nishio_forcing_chain(&grid, EngineConfig::default())
            .expect("long anti-knight Nishio chain");
        let detailed = find_nishio_forcing_chain_with_proof(&grid, EngineConfig::default())
            .expect("selected long anti-knight Nishio proof");
        assert_eq!(detailed.inference(), &inference);
        let views = detailed.proof().views();
        assert_eq!((views[0].nodes().len(), views[1].nodes().len()), (21, 23));
        assert_eq!(
            views[0].nodes()[12]
                .parents()
                .iter()
                .map(|parent| (parent.node().index(), parent.cause()))
                .collect::<Vec<_>>(),
            [
                (16, region(0, 8)),
                (17, region(0, 8)),
                (18, region(0, 8)),
                (19, region(0, 8)),
                (20, region(0, 8)),
            ]
        );
        assert_eq!(inference.rating(), Rating::from_tenths(82));
        assert_eq!(
            inference.description(grid.topology()),
            "Nishio Forcing Chain: r9c1.2 on ==> r1c4.2 both on & off"
        );
        let Evidence::NishioForcingChain {
            source_cell,
            source_digit,
            source_on,
            complexity,
            ..
        } = inference.evidence()
        else {
            panic!("Nishio evidence");
        };
        assert_eq!(source_cell, CellId::new(72).unwrap());
        assert_eq!(source_digit, sukaku_forge_core::Digit::new(2).unwrap());
        assert!(source_on);
        assert_eq!(complexity, 44);
        inference.apply(&mut grid);
        assert!(
            !grid
                .candidates(CellId::new(72).unwrap())
                .contains(sukaku_forge_core::Digit::new(2).unwrap())
        );
    }
}
