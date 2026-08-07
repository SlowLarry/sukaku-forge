use std::collections::VecDeque;
use std::sync::Arc;

use sukaku_forge_core::{
    CandidateMask, CandidateRemovals, CandidateRemovalsBuilder, CellId, Digit, Grid,
    NonConsecutiveMode, REGION_TYPE_COUNT, RegionId,
};

use crate::nested_chains::{
    ChainProof, InferenceCollector, NestedHint, NestedHintCollector, OnCause, ProofArena,
    ProofKind, ProofNode, ProofTarget,
};
use crate::presentation_proof::{
    ChainCause, ChainNodeId, ChainProofNode, ChainProofParent, ChainProofView, ChainProofViewKind,
    ChainState, ForcingChainWithProof, SelectedChainProof,
};
use crate::{ChainCellSequence, ChainKind, EngineConfig, Evidence, Inference, Rating, Technique};

pub(crate) const CANDIDATE_COUNT: usize = 81 * 9;
pub(crate) const KEY_COUNT: usize = CANDIDATE_COUNT * 2;
const NO_NODE: u32 = u32::MAX;

#[derive(Clone, Copy)]
struct Node {
    key: u16,
    parent: u32,
    on_cause: OnCause,
}

struct Arena {
    nodes: Vec<Node>,
}

impl Arena {
    fn new() -> Self {
        Self {
            nodes: Vec::with_capacity(64),
        }
    }

    fn root(&mut self, key: u16) -> u32 {
        self.push(key, NO_NODE)
    }

    fn push(&mut self, key: u16, parent: u32) -> u32 {
        self.push_with_cause(key, parent, OnCause::None)
    }

    fn push_with_cause(&mut self, key: u16, parent: u32, on_cause: OnCause) -> u32 {
        let id = u32::try_from(self.nodes.len()).expect("static implication arena size");
        self.nodes.push(Node {
            key,
            parent,
            on_cause,
        });
        id
    }

    fn clear(&mut self) {
        self.nodes.clear();
    }

    fn node(&self, id: u32) -> Node {
        self.nodes[usize::try_from(id).expect("node index")]
    }

    /// Java's cycle guard compares only the first-parent ancestry of `child`.
    fn has_parent_key(&self, child: u32, parent_key: u16) -> bool {
        let mut current = child;
        loop {
            let parent = self.node(current).parent;
            if parent == NO_NODE {
                return false;
            }
            current = parent;
            if self.node(current).key == parent_key {
                return true;
            }
        }
    }

    fn path_to_root(&self, terminal: u32) -> Vec<u32> {
        let mut result = Vec::new();
        let mut current = terminal;
        loop {
            result.push(current);
            let parent = self.node(current).parent;
            if parent == NO_NODE {
                return result;
            }
            current = parent;
        }
    }

    fn proof_arena(&self) -> Arc<ProofArena> {
        let mut nodes = Vec::with_capacity(self.nodes.len());
        let mut parents = Vec::with_capacity(self.nodes.len().saturating_sub(1));
        for node in &self.nodes {
            let parent_start = u32::try_from(parents.len()).expect("static proof parent storage");
            let parent_count = if node.parent == NO_NODE {
                0
            } else {
                parents.push(node.parent);
                1
            };
            nodes.push(ProofNode {
                key: node.key,
                parent_start,
                parent_count,
                on_cause: node.on_cause,
                nested: None,
            });
        }
        Arc::new(ProofArena::new(nodes, parents))
    }
}

/// Java-compatible insertion order plus latest-graph lookup semantics.
struct ImplicationSet {
    latest_by_key: [u32; KEY_COUNT],
    touched_keys: Vec<u16>,
}

impl ImplicationSet {
    fn new() -> Self {
        Self {
            latest_by_key: [NO_NODE; KEY_COUNT],
            touched_keys: Vec::with_capacity(64),
        }
    }

    fn add(&mut self, arena: &Arena, node: u32) -> bool {
        let key = usize::from(arena.node(node).key);
        let is_new = self.latest_by_key[key] == NO_NODE;
        if is_new {
            self.touched_keys.push(arena.node(node).key);
        }
        self.latest_by_key[key] = node;
        is_new
    }

    fn add_if_absent(&mut self, arena: &Arena, node: u32) -> bool {
        let key = usize::from(arena.node(node).key);
        if self.latest_by_key[key] != NO_NODE {
            return false;
        }
        self.latest_by_key[key] = node;
        self.touched_keys.push(arena.node(node).key);
        true
    }

    fn clear(&mut self) {
        for key in self.touched_keys.drain(..) {
            self.latest_by_key[usize::from(key)] = NO_NODE;
        }
    }
}

/// Reused storage for the many root assumptions examined on one grid.
struct StaticChainWorkspace {
    arena: Arena,
    to_on: ImplicationSet,
    to_off: ImplicationSet,
    pending_on: VecDeque<u32>,
    pending_off: VecDeque<u32>,
    cycles: Vec<u32>,
    visited: [bool; KEY_COUNT],
    visited_keys: Vec<u16>,
}

impl StaticChainWorkspace {
    fn new() -> Self {
        Self {
            arena: Arena::new(),
            to_on: ImplicationSet::new(),
            to_off: ImplicationSet::new(),
            pending_on: VecDeque::with_capacity(64),
            pending_off: VecDeque::with_capacity(64),
            cycles: Vec::with_capacity(8),
            visited: [false; KEY_COUNT],
            visited_keys: Vec::with_capacity(64),
        }
    }

    fn clear(&mut self) {
        self.to_on.clear();
        self.to_off.clear();
        self.arena.clear();
        self.pending_on.clear();
        self.pending_off.clear();
        self.cycles.clear();
        for key in self.visited_keys.drain(..) {
            self.visited[usize::from(key)] = false;
        }
    }
}

/// Candidate-indexed implications in Java discovery order.
///
/// The original port used one `Vec` per candidate and relation. Nested chains
/// rebuild this immutable graph many times, so that layout caused thousands of
/// tiny allocations per build. A CSR-style table keeps the same slices and
/// order in one contiguous allocation.
struct ImplicationTable {
    offsets: Box<[u32; CANDIDATE_COUNT + 1]>,
    entries: Vec<(u16, OnCause)>,
}

impl ImplicationTable {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            offsets: Box::new([0; CANDIDATE_COUNT + 1]),
            entries: Vec::with_capacity(capacity),
        }
    }

    fn begin_candidate(&mut self, candidate: usize) {
        self.offsets[candidate] =
            u32::try_from(self.entries.len()).expect("implication table offset");
    }

    fn finish_candidate(&mut self, candidate: usize) {
        self.offsets[candidate + 1] =
            u32::try_from(self.entries.len()).expect("implication table offset");
    }

    fn push(&mut self, entry: (u16, OnCause)) {
        self.entries.push(entry);
    }

    fn entries(&self, candidate: usize) -> &[(u16, OnCause)] {
        let start = usize::try_from(self.offsets[candidate]).expect("implication table start");
        let end = usize::try_from(self.offsets[candidate + 1]).expect("implication table end");
        &self.entries[start..end]
    }
}

pub(crate) struct Implications {
    /// Same-cell weak implications used when Y links are enabled.
    cell_off: ImplicationTable,
    /// House/chess/non-consecutive weak implications, enabled in every pass.
    weak_off: ImplicationTable,
    /// Other bivalue-cell candidate used when Y links are enabled.
    cell_on: Box<[Option<u16>; CANDIDATE_COUNT]>,
    /// House conjugates used when X links are enabled.
    strong_on: ImplicationTable,
}

impl Implications {
    pub(crate) fn new(grid: &Grid, config: EngineConfig) -> Self {
        Self::build(grid, config, true)
    }

    /// Build only immutable weak links for dynamic chaining. Same-cell weak
    /// links and every strong link are derived from the live branch grid, so
    /// their static counterparts would be dead work.
    pub(crate) fn weak_only(grid: &Grid, config: EngineConfig) -> Self {
        Self::build(grid, config, false)
    }

    fn build(grid: &Grid, config: EngineConfig, include_static_links: bool) -> Self {
        let mut result = Self {
            cell_off: ImplicationTable::with_capacity(if include_static_links { 2_048 } else { 0 }),
            weak_off: ImplicationTable::with_capacity(16_384),
            cell_on: Box::new([None; CANDIDATE_COUNT]),
            strong_on: ImplicationTable::with_capacity(if include_static_links {
                2_048
            } else {
                0
            }),
        };
        let region_types = active_region_types(grid, config);
        let topology = grid.topology();
        let variant = topology.config();

        for raw_cell in 0_u8..81 {
            let cell = CellId::new(raw_cell).expect("cell index loop");
            let values = grid.candidates(cell);
            for raw_digit in 1_u8..=9 {
                let digit = Digit::new(raw_digit).expect("digit index loop");
                let candidate = candidate_index(cell, digit);
                result.cell_off.begin_candidate(candidate);
                result.weak_off.begin_candidate(candidate);
                result.strong_on.begin_candidate(candidate);

                if !values.contains(digit) {
                    result.cell_off.finish_candidate(candidate);
                    result.weak_off.finish_candidate(candidate);
                    result.strong_on.finish_candidate(candidate);
                    continue;
                }

                if include_static_links {
                    for other in values.iter() {
                        if other != digit {
                            result
                                .cell_off
                                .push((potential_key(cell, other, false), OnCause::NakedSingle));
                        }
                    }
                    if values.count() == 2 {
                        result.cell_on[candidate] = values
                            .iter()
                            .find(|other| *other != digit)
                            .map(|other| potential_key(cell, other, true));
                    }
                }

                let mut weak_seen = [false; 81];
                weak_seen[cell.index()] = true;
                let mut strong_seen = [false; 81];
                for &type_index in &region_types {
                    let Some(region_index) = topology.cell_region_index(cell, type_index) else {
                        continue;
                    };
                    let region = RegionId::new(type_index as u8, region_index)
                        .expect("configured region id");
                    let positions = grid.region_candidate_positions(region, digit);
                    let cells = topology.region_cells(region);
                    for position in positions.iter() {
                        let target = CellId::new(cells[usize::from(position)])
                            .expect("region candidate cell");
                        if !weak_seen[target.index()] {
                            weak_seen[target.index()] = true;
                            result.weak_off.push((
                                potential_key(target, digit, false),
                                OnCause::HiddenRegion(
                                    u8::try_from(type_index).expect("region type index"),
                                ),
                            ));
                        }
                    }

                    if include_static_links {
                        let source_position = topology
                            .cell_position_in_region(cell, type_index)
                            .expect("source region position");
                        let mut other_positions = positions;
                        other_positions.remove(source_position);
                        if other_positions.count() == 1 {
                            let position = other_positions.single().expect("one other position");
                            let target = CellId::new(cells[usize::from(position)])
                                .expect("conjugate target cell");
                            if !strong_seen[target.index()] {
                                strong_seen[target.index()] = true;
                                result.strong_on.push((
                                    potential_key(target, digit, true),
                                    OnCause::HiddenRegion(
                                        u8::try_from(type_index).expect("region type index"),
                                    ),
                                ));
                            }
                        }
                    }
                }

                if variant.anti_ferz {
                    for &raw_target in topology.regular_anti_ferz_neighbors(cell) {
                        let target = CellId::new(raw_target).expect("anti-ferz target");
                        if !weak_seen[target.index()] && grid.candidates(target).contains(digit) {
                            weak_seen[target.index()] = true;
                            result
                                .weak_off
                                .push((potential_key(target, digit, false), OnCause::NakedSingle));
                        }
                    }
                }
                if variant.anti_knight {
                    for &raw_target in topology.regular_anti_knight_neighbors(cell) {
                        let target = CellId::new(raw_target).expect("anti-knight target");
                        if !weak_seen[target.index()] && grid.candidates(target).contains(digit) {
                            weak_seen[target.index()] = true;
                            result
                                .weak_off
                                .push((potential_key(target, digit, false), OnCause::NakedSingle));
                        }
                    }
                }

                if variant.forbidden_pairs && variant.non_consecutive != NonConsecutiveMode::Off {
                    let neighbors = topology
                        .forbidden_pair_neighbors(cell)
                        .expect("enabled forbidden-pair neighbors");
                    for &raw_target in neighbors {
                        let target = CellId::new(raw_target).expect("forbidden-pair target");
                        let value = digit.get();
                        if variant.non_consecutive.is_cyclic() || value < 9 {
                            let next = Digit::new(if value == 9 { 1 } else { value + 1 })
                                .expect("next digit");
                            if grid.candidates(target).contains(next) {
                                result.weak_off.push((
                                    potential_key(target, next, false),
                                    OnCause::NakedSingle,
                                ));
                            }
                        }
                        if variant.non_consecutive.is_cyclic() || value > 1 {
                            let previous = Digit::new(if value == 1 { 9 } else { value - 1 })
                                .expect("previous digit");
                            if grid.candidates(target).contains(previous) {
                                result.weak_off.push((
                                    potential_key(target, previous, false),
                                    OnCause::NakedSingle,
                                ));
                            }
                        }
                    }
                }
                result.cell_off.finish_candidate(candidate);
                result.weak_off.finish_candidate(candidate);
                result.strong_on.finish_candidate(candidate);
            }
        }
        result
    }

    pub(crate) fn for_each_off(&self, source_key: u16, y_enabled: bool, mut emit: impl FnMut(u16)) {
        let candidate = candidate_from_key(source_key);
        if y_enabled {
            for &(target, _) in self.cell_off.entries(candidate) {
                emit(target);
            }
        }
        for &(target, _) in self.weak_off.entries(candidate) {
            emit(target);
        }
    }

    pub(crate) fn for_each_off_with_cause(
        &self,
        source_key: u16,
        y_enabled: bool,
        mut emit: impl FnMut(u16, OnCause),
    ) {
        let candidate = candidate_from_key(source_key);
        if y_enabled {
            for &(target, cause) in self.cell_off.entries(candidate) {
                emit(target, cause);
            }
        }
        for &(target, cause) in self.weak_off.entries(candidate) {
            emit(target, cause);
        }
    }

    /// Emit the Java-ordered house/chess/non-consecutive weak implications.
    ///
    /// Dynamic chaining deliberately omits the same-cell Y links in Nishio
    /// mode, but shares this immutable part of the implication topology.
    pub(crate) fn for_each_weak_off(&self, source_key: u16, mut emit: impl FnMut(u16)) {
        let candidate = candidate_from_key(source_key);
        for &(target, _) in self.weak_off.entries(candidate) {
            emit(target);
        }
    }

    /// Emit Java-ordered dynamic weak implications together with the exact
    /// weak-link cause retained by the immutable implication table.
    pub(crate) fn for_each_weak_off_with_cause(
        &self,
        source_key: u16,
        mut emit: impl FnMut(u16, OnCause),
    ) {
        let candidate = candidate_from_key(source_key);
        for &(target, cause) in self.weak_off.entries(candidate) {
            emit(target, cause);
        }
    }

    pub(crate) fn for_each_on_with_cause(
        &self,
        source_key: u16,
        y_enabled: bool,
        x_enabled: bool,
        mut emit: impl FnMut(u16, OnCause),
    ) {
        let candidate = candidate_from_key(source_key);
        if y_enabled {
            if let Some(target) = self.cell_on[candidate] {
                emit(target, OnCause::NakedSingle);
            }
        }
        if x_enabled {
            for &(target, cause) in self.strong_on.entries(candidate) {
                emit(target, cause);
            }
        }
    }
}

/// Compact directed graph used only to reject impossible SE121 static-chain
/// roots. The accepted roots still run through the original Java-ordered
/// searches below, so this graph never chooses a proof or affects ranking.
struct DirectedImplicationGraph {
    offsets: Box<[u32; KEY_COUNT + 1]>,
    targets: Vec<u16>,
}

impl DirectedImplicationGraph {
    fn new(implications: &Implications, y_enabled: bool, x_enabled: bool) -> Self {
        let mut result = Self {
            offsets: Box::new([0; KEY_COUNT + 1]),
            targets: Vec::with_capacity(16_384),
        };
        for raw_key in 0..KEY_COUNT {
            result.offsets[raw_key] =
                u32::try_from(result.targets.len()).expect("directed implication offset");
            let key = u16::try_from(raw_key).expect("potential key");
            if is_on(key) {
                implications.for_each_off(key, y_enabled, |target| {
                    result.targets.push(target);
                });
            } else {
                implications.for_each_on_with_cause(key, y_enabled, x_enabled, |target, _| {
                    result.targets.push(target)
                });
            }
        }
        result.offsets[KEY_COUNT] =
            u32::try_from(result.targets.len()).expect("directed implication offset");
        result
    }

    fn reversed(&self) -> Self {
        let mut offsets = Box::new([0_u32; KEY_COUNT + 1]);
        for &target in &self.targets {
            offsets[usize::from(target) + 1] += 1;
        }
        for key in 0..KEY_COUNT {
            offsets[key + 1] += offsets[key];
        }

        let mut cursors = offsets.clone();
        let mut targets = vec![0_u16; self.targets.len()];
        for raw_source in 0..KEY_COUNT {
            let source = u16::try_from(raw_source).expect("potential key");
            for &target in self.neighbors(source) {
                let cursor = &mut cursors[usize::from(target)];
                targets[usize::try_from(*cursor).expect("reverse implication offset")] = source;
                *cursor += 1;
            }
        }
        Self { offsets, targets }
    }

    fn neighbors(&self, source: u16) -> &[u16] {
        let source = usize::from(source);
        let start = usize::try_from(self.offsets[source]).expect("directed implication start");
        let end = usize::try_from(self.offsets[source + 1]).expect("directed implication end");
        &self.targets[start..end]
    }

    fn is_incident(&self, reverse: &Self, key: u16) -> bool {
        !self.neighbors(key).is_empty() || !reverse.neighbors(key).is_empty()
    }
}

/// Exact reachability of the immutable static implication graph, condensed by
/// strongly connected component. A negative answer is therefore a proof that
/// the much more expensive Java-compatible arena traversal cannot succeed.
struct StaticRootPrefilter {
    may_cycle: Box<[bool; KEY_COUNT]>,
    may_force: Box<[bool; KEY_COUNT]>,
}

impl StaticRootPrefilter {
    const NO_COMPONENT: u16 = u16::MAX;

    fn new(
        implications: &Implications,
        y_enabled: bool,
        x_enabled: bool,
        build_forcing_reachability: bool,
    ) -> Self {
        let graph = DirectedImplicationGraph::new(implications, y_enabled, x_enabled);
        let reverse = graph.reversed();
        let finish_order = Self::finish_order(&graph, &reverse);

        let mut component_by_key = Box::new([Self::NO_COMPONENT; KEY_COUNT]);
        let mut component_on_count = Vec::new();
        let mut component_off_count = Vec::new();
        let mut pending = Vec::with_capacity(KEY_COUNT);
        for &start in finish_order.iter().rev() {
            if component_by_key[usize::from(start)] != Self::NO_COMPONENT {
                continue;
            }
            let component =
                u16::try_from(component_on_count.len()).expect("static implication components");
            component_by_key[usize::from(start)] = component;
            pending.push(start);
            let mut on_count = 0_u16;
            let mut off_count = 0_u16;
            while let Some(source) = pending.pop() {
                if is_on(source) {
                    on_count += 1;
                } else {
                    off_count += 1;
                }
                for &target in reverse.neighbors(source) {
                    let slot = &mut component_by_key[usize::from(target)];
                    if *slot == Self::NO_COMPONENT {
                        *slot = component;
                        pending.push(target);
                    }
                }
            }
            component_on_count.push(on_count);
            component_off_count.push(off_count);
        }

        let mut may_cycle = Box::new([false; KEY_COUNT]);
        for raw_key in 0..KEY_COUNT {
            let component = component_by_key[raw_key];
            if component != Self::NO_COMPONENT {
                let component = usize::from(component);
                may_cycle[raw_key] =
                    component_on_count[component] >= 2 && component_off_count[component] >= 2;
            }
        }
        // Cycle rejection needs only SCC membership. X-forcing roots can use
        // the XY graph's negative answer because XY is a strict edge superset,
        // so only the XY filter needs the condensation reachability closure.
        if !build_forcing_reachability {
            return Self {
                may_cycle,
                may_force: Box::new([false; KEY_COUNT]),
            };
        }

        let component_count = component_on_count.len();
        let mut component_edges = Vec::new();
        for raw_source in 0..KEY_COUNT {
            let source = u16::try_from(raw_source).expect("potential key");
            let source_component = component_by_key[raw_source];
            if source_component == Self::NO_COMPONENT {
                continue;
            }
            for &target in graph.neighbors(source) {
                let target_component = component_by_key[usize::from(target)];
                if source_component != target_component {
                    component_edges.push((source_component, target_component));
                }
            }
        }
        component_edges.sort_unstable();
        component_edges.dedup();

        let mut component_offsets = vec![0_u32; component_count + 1];
        let mut indegree = vec![0_u16; component_count];
        for &(source, target) in &component_edges {
            component_offsets[usize::from(source) + 1] += 1;
            indegree[usize::from(target)] += 1;
        }
        for component in 0..component_count {
            component_offsets[component + 1] += component_offsets[component];
        }

        let component_targets = component_edges
            .iter()
            .map(|&(_, target)| target)
            .collect::<Vec<_>>();
        let mut ready = VecDeque::with_capacity(component_count);
        for (component, &incoming) in indegree.iter().enumerate() {
            if incoming == 0 {
                ready.push_back(u16::try_from(component).expect("static implication component"));
            }
        }
        let mut topological = Vec::with_capacity(component_count);
        while let Some(source) = ready.pop_front() {
            topological.push(source);
            let start = usize::try_from(component_offsets[usize::from(source)])
                .expect("component edge start");
            let end = usize::try_from(component_offsets[usize::from(source) + 1])
                .expect("component edge end");
            for &target in &component_targets[start..end] {
                let incoming = &mut indegree[usize::from(target)];
                *incoming -= 1;
                if *incoming == 0 {
                    ready.push_back(target);
                }
            }
        }
        debug_assert_eq!(topological.len(), component_count);

        let reachability_words = component_count.div_ceil(64);
        let mut reachable_components = vec![0_u64; component_count * reachability_words];
        for &source in topological.iter().rev() {
            let source = usize::from(source);
            let source_base = source * reachability_words;
            let start = usize::try_from(component_offsets[source]).expect("component edge start");
            let end = usize::try_from(component_offsets[source + 1]).expect("component edge end");
            for &target in &component_targets[start..end] {
                let target = usize::from(target);
                reachable_components[source_base + target / 64] |= 1_u64 << (target % 64);
                let target_base = target * reachability_words;
                for word in 0..reachability_words {
                    let inherited = reachable_components[target_base + word];
                    reachable_components[source_base + word] |= inherited;
                }
            }
        }

        let mut may_force = Box::new([false; KEY_COUNT]);
        for raw_key in 0..KEY_COUNT {
            let component = component_by_key[raw_key];
            if component == Self::NO_COMPONENT {
                continue;
            }
            let component = usize::from(component);
            let target = component_by_key[raw_key ^ 1];
            if target == Self::NO_COMPONENT {
                continue;
            }
            let target = usize::from(target);
            may_force[raw_key] = component == target
                || reachable_components[component * reachability_words + target / 64]
                    & (1_u64 << (target % 64))
                    != 0;
        }
        Self {
            may_cycle,
            may_force,
        }
    }

    fn finish_order(
        graph: &DirectedImplicationGraph,
        reverse: &DirectedImplicationGraph,
    ) -> Vec<u16> {
        let mut visited = Box::new([false; KEY_COUNT]);
        let mut result = Vec::with_capacity(KEY_COUNT);
        let mut stack = Vec::with_capacity(KEY_COUNT);
        for raw_start in 0..KEY_COUNT {
            let start = u16::try_from(raw_start).expect("potential key");
            if visited[raw_start] || !graph.is_incident(reverse, start) {
                continue;
            }
            visited[raw_start] = true;
            stack.push((start, 0_usize));
            while let Some((source, next_neighbor)) = stack.last_mut() {
                let neighbors = graph.neighbors(*source);
                if *next_neighbor == neighbors.len() {
                    result.push(*source);
                    stack.pop();
                    continue;
                }
                let target = neighbors[*next_neighbor];
                *next_neighbor += 1;
                if !visited[usize::from(target)] {
                    visited[usize::from(target)] = true;
                    stack.push((target, 0));
                }
            }
        }
        result
    }

    /// A recorded cycle has at least four alternating links. Its simple
    /// first-parent path therefore contains two distinct ON and two distinct
    /// OFF keys in one strongly connected component.
    fn cycle_possible(&self, source_key: u16) -> bool {
        self.may_cycle[usize::from(source_key)]
    }

    /// Forcing succeeds only by reaching the opposite truth state of its root.
    fn forcing_possible(&self, source_key: u16) -> bool {
        self.may_force[usize::from(source_key)]
    }
}

/// The three static link modes used by one FCC invocation. X and Y retain
/// mode-exact SCC cycle flags, while one XY closure safely rejects forcing
/// roots for both X and XY searches: reachability absent from the superset is
/// necessarily absent from its subset.
struct StaticModePrefilters {
    x_cycle: StaticRootPrefilter,
    y_cycle: StaticRootPrefilter,
    xy: StaticRootPrefilter,
}

impl StaticModePrefilters {
    fn new(implications: &Implications) -> Self {
        Self {
            x_cycle: StaticRootPrefilter::new(implications, false, true, false),
            y_cycle: StaticRootPrefilter::new(implications, true, false, false),
            xy: StaticRootPrefilter::new(implications, true, true, true),
        }
    }

    fn cycle(&self, y_enabled: bool, x_enabled: bool) -> &StaticRootPrefilter {
        match (y_enabled, x_enabled) {
            (false, true) => &self.x_cycle,
            (true, false) => &self.y_cycle,
            (true, true) => &self.xy,
            _ => unreachable!("unsupported static chain link mode"),
        }
    }

    fn forcing(&self) -> &StaticRootPrefilter {
        &self.xy
    }
}

struct RankedInference {
    inference: Inference,
    java_difficulty: f64,
    complexity: u16,
    sort_key: u8,
}

impl RankedInference {
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

/// Cheap, arena-independent coordinates used to rerun only the final winner
/// after all static FCC candidates have been ranked.
#[derive(Clone, Copy)]
enum SelectedProofLocator {
    Cycle {
        source_cell: CellId,
        source_digit: Digit,
        y_enabled: bool,
        x_enabled: bool,
        cycle_index: usize,
    },
    Forcing {
        source_cell: CellId,
        source_digit: Digit,
        source_on: bool,
        y_enabled: bool,
        x_enabled: bool,
    },
}

/// Find the first Java-ranked static Forcing Chain or bidirectional Cycle.
#[must_use]
pub fn find_forcing_chain_cycle(grid: &Grid, config: EngineConfig) -> Option<Inference> {
    let implications = Implications::new(grid, config);
    let mut workspace = StaticChainWorkspace::new();
    let mut best: Option<RankedInference> = None;

    for (kind_cycle, kind_forcing, y_enabled, x_enabled, sort_key) in [
        (ChainKind::XCycle, ChainKind::XForcing, false, true, 2),
        (ChainKind::YCycle, ChainKind::XForcing, true, false, 3),
        (ChainKind::XyCycle, ChainKind::XyForcing, true, true, 4),
    ] {
        for raw_cell in 0_u8..81 {
            let cell = CellId::new(raw_cell).expect("cell index loop");
            if grid.value(cell) != 0 {
                continue;
            }
            let values = grid.candidates(cell);
            if values.count() <= 1 || (!x_enabled && values.count() > 2) {
                continue;
            }
            for digit in values.iter() {
                search_cycles(
                    &mut workspace,
                    &implications,
                    cell,
                    digit,
                    y_enabled,
                    x_enabled,
                );
                for &terminal in &workspace.cycles {
                    if let Some(candidate) =
                        cycle_inference(grid, &workspace.arena, terminal, kind_cycle, sort_key)
                    {
                        keep_best(&mut best, candidate);
                    }
                }

                if x_enabled {
                    if let Some(terminal) = search_forcing(
                        &mut workspace,
                        &implications,
                        cell,
                        digit,
                        true,
                        y_enabled,
                        x_enabled,
                    ) {
                        let candidate = forcing_inference(
                            grid,
                            &workspace.arena,
                            terminal,
                            kind_forcing,
                            sort_key,
                        );
                        keep_best(&mut best, candidate);
                    }
                    if let Some(terminal) = search_forcing(
                        &mut workspace,
                        &implications,
                        cell,
                        digit,
                        false,
                        y_enabled,
                        x_enabled,
                    ) {
                        let candidate = forcing_inference(
                            grid,
                            &workspace.arena,
                            terminal,
                            kind_forcing,
                            sort_key,
                        );
                        keep_best(&mut best, candidate);
                    }
                }
            }
        }
    }

    best.map(|candidate| candidate.inference)
}

/// Collect every Java-ranked static Forcing Chain or bidirectional Cycle.
///
/// Results are stably ordered by Java difficulty, recursive complexity, family
/// sort key, and discovery order, then de-duplicated by logical effect.  Only
/// compact inference summaries are retained; call
/// [`replay_forcing_chain_cycle_with_proof`] for the selected row.
#[must_use]
pub fn collect_forcing_chain_cycles(grid: &Grid, config: EngineConfig) -> Vec<Inference> {
    let implications = Implications::new(grid, config);
    let mut workspace = StaticChainWorkspace::new();
    let mut result = InferenceCollector::new();

    for (kind_cycle, kind_forcing, y_enabled, x_enabled, sort_key) in [
        (ChainKind::XCycle, ChainKind::XForcing, false, true, 2),
        (ChainKind::YCycle, ChainKind::XForcing, true, false, 3),
        (ChainKind::XyCycle, ChainKind::XyForcing, true, true, 4),
    ] {
        for raw_cell in 0_u8..81 {
            let cell = CellId::new(raw_cell).expect("cell index loop");
            if grid.value(cell) != 0 {
                continue;
            }
            let values = grid.candidates(cell);
            if values.count() <= 1 || (!x_enabled && values.count() > 2) {
                continue;
            }
            for digit in values.iter() {
                search_cycles(
                    &mut workspace,
                    &implications,
                    cell,
                    digit,
                    y_enabled,
                    x_enabled,
                );
                for &terminal in &workspace.cycles {
                    if let Some(candidate) =
                        cycle_inference(grid, &workspace.arena, terminal, kind_cycle, sort_key)
                    {
                        offer_public_inference(grid, &mut result, candidate);
                    }
                }

                if x_enabled {
                    if let Some(terminal) = search_forcing(
                        &mut workspace,
                        &implications,
                        cell,
                        digit,
                        true,
                        y_enabled,
                        x_enabled,
                    ) {
                        offer_public_inference(
                            grid,
                            &mut result,
                            forcing_inference(
                                grid,
                                &workspace.arena,
                                terminal,
                                kind_forcing,
                                sort_key,
                            ),
                        );
                    }
                    if let Some(terminal) = search_forcing(
                        &mut workspace,
                        &implications,
                        cell,
                        digit,
                        false,
                        y_enabled,
                        x_enabled,
                    ) {
                        offer_public_inference(
                            grid,
                            &mut result,
                            forcing_inference(
                                grid,
                                &workspace.arena,
                                terminal,
                                kind_forcing,
                                sort_key,
                            ),
                        );
                    }
                }
            }
        }
    }

    result.finish()
}

fn offer_public_inference(
    grid: &Grid,
    result: &mut InferenceCollector,
    candidate: RankedInference,
) {
    result.offer(
        grid,
        candidate.inference,
        candidate.java_difficulty,
        u32::from(candidate.complexity),
        candidate.sort_key,
    );
}

/// Find the first Java-ranked static FCC hint and materialize its flat views.
///
/// This is an opt-in GUI path. Candidate discovery and ranking remain compact:
/// the search retains only cheap coordinates for the current winner, then
/// reruns that one root to copy its ordered proof after ranking is complete.
/// The ordinary [`find_forcing_chain_cycle`] path does not call this function
/// and never constructs presentation proof nodes.
#[must_use]
pub fn find_forcing_chain_cycle_with_proof(
    grid: &Grid,
    config: EngineConfig,
) -> Option<ForcingChainWithProof> {
    let implications = Implications::new(grid, config);
    let mut workspace = StaticChainWorkspace::new();
    let mut best: Option<(RankedInference, SelectedProofLocator)> = None;

    for (kind_cycle, kind_forcing, y_enabled, x_enabled, sort_key) in [
        (ChainKind::XCycle, ChainKind::XForcing, false, true, 2),
        (ChainKind::YCycle, ChainKind::XForcing, true, false, 3),
        (ChainKind::XyCycle, ChainKind::XyForcing, true, true, 4),
    ] {
        for raw_cell in 0_u8..81 {
            let cell = CellId::new(raw_cell).expect("cell index loop");
            if grid.value(cell) != 0 {
                continue;
            }
            let values = grid.candidates(cell);
            if values.count() <= 1 || (!x_enabled && values.count() > 2) {
                continue;
            }
            for digit in values.iter() {
                search_cycles(
                    &mut workspace,
                    &implications,
                    cell,
                    digit,
                    y_enabled,
                    x_enabled,
                );
                for (cycle_index, &terminal) in workspace.cycles.iter().enumerate() {
                    if let Some(candidate) =
                        cycle_inference(grid, &workspace.arena, terminal, kind_cycle, sort_key)
                    {
                        keep_best_with_locator(
                            &mut best,
                            candidate,
                            SelectedProofLocator::Cycle {
                                source_cell: cell,
                                source_digit: digit,
                                y_enabled,
                                x_enabled,
                                cycle_index,
                            },
                        );
                    }
                }

                if x_enabled {
                    if let Some(terminal) = search_forcing(
                        &mut workspace,
                        &implications,
                        cell,
                        digit,
                        true,
                        y_enabled,
                        x_enabled,
                    ) {
                        let candidate = forcing_inference(
                            grid,
                            &workspace.arena,
                            terminal,
                            kind_forcing,
                            sort_key,
                        );
                        keep_best_with_locator(
                            &mut best,
                            candidate,
                            SelectedProofLocator::Forcing {
                                source_cell: cell,
                                source_digit: digit,
                                source_on: true,
                                y_enabled,
                                x_enabled,
                            },
                        );
                    }
                    if let Some(terminal) = search_forcing(
                        &mut workspace,
                        &implications,
                        cell,
                        digit,
                        false,
                        y_enabled,
                        x_enabled,
                    ) {
                        let candidate = forcing_inference(
                            grid,
                            &workspace.arena,
                            terminal,
                            kind_forcing,
                            sort_key,
                        );
                        keep_best_with_locator(
                            &mut best,
                            candidate,
                            SelectedProofLocator::Forcing {
                                source_cell: cell,
                                source_digit: digit,
                                source_on: false,
                                y_enabled,
                                x_enabled,
                            },
                        );
                    }
                }
            }
        }
    }

    let (winner, locator) = best?;
    let proof = materialize_selected_proof(&mut workspace, &implications, grid, locator);
    Some(ForcingChainWithProof::new(winner.inference, proof))
}

/// Replay the flat proof for any retained static FCC inference.
///
/// This is the lazy companion to [`collect_forcing_chain_cycles`].  It reruns
/// only the selected root and returns `None` when the inference does not
/// belong to this grid/configuration.
#[must_use]
pub fn replay_forcing_chain_cycle_with_proof(
    grid: &Grid,
    config: EngineConfig,
    inference: &Inference,
) -> Option<ForcingChainWithProof> {
    if inference.technique() != Technique::ForcingChainCycle {
        return None;
    }
    let Evidence::ForcingChainCycle {
        kind,
        target_cell,
        target_digit,
        target_on,
        ..
    } = inference.evidence()
    else {
        return None;
    };
    let (y_enabled, x_enabled, sort_key, cycle) = match kind {
        ChainKind::XCycle => (false, true, 2, true),
        ChainKind::YCycle => (true, false, 3, true),
        ChainKind::XyCycle => (true, true, 4, true),
        ChainKind::XForcing => (false, true, 2, false),
        ChainKind::XyForcing => (true, true, 4, false),
    };
    let implications = Implications::new(grid, config);
    let mut workspace = StaticChainWorkspace::new();

    let locator = if cycle {
        if !target_on {
            return None;
        }
        search_cycles(
            &mut workspace,
            &implications,
            target_cell,
            target_digit,
            y_enabled,
            x_enabled,
        );
        workspace
            .cycles
            .iter()
            .enumerate()
            .find_map(|(cycle_index, &terminal)| {
                let candidate = cycle_inference(grid, &workspace.arena, terminal, kind, sort_key)?;
                (candidate.inference == *inference).then_some(SelectedProofLocator::Cycle {
                    source_cell: target_cell,
                    source_digit: target_digit,
                    y_enabled,
                    x_enabled,
                    cycle_index,
                })
            })?
    } else {
        let source_on = !target_on;
        let terminal = search_forcing(
            &mut workspace,
            &implications,
            target_cell,
            target_digit,
            source_on,
            y_enabled,
            x_enabled,
        )?;
        let candidate = forcing_inference(grid, &workspace.arena, terminal, kind, sort_key);
        if candidate.inference != *inference {
            return None;
        }
        SelectedProofLocator::Forcing {
            source_cell: target_cell,
            source_digit: target_digit,
            source_on,
            y_enabled,
            x_enabled,
        }
    };

    let proof = materialize_selected_proof(&mut workspace, &implications, grid, locator);
    Some(ForcingChainWithProof::new(inference.clone(), proof))
}

/// Materialize only the selected proof for an inference retained by the
/// all-hints session.
#[must_use]
pub fn replay_forcing_chain_cycle_proof(
    grid: &Grid,
    config: EngineConfig,
    inference: &Inference,
) -> SelectedChainProof {
    replay_forcing_chain_cycle_with_proof(grid, config, inference)
        .expect("retained static FCC inference is reproducible")
        .into_parts()
        .1
}

/// Collect every Java-ranked static FCC hint with its complete implication
/// graph. Nested dynamic chains consume this path; the public first-hint path
/// above remains compact and streaming.
pub(crate) fn collect_forcing_chain_proofs(grid: &Grid, config: EngineConfig) -> Vec<NestedHint> {
    collect_forcing_chain_proofs_impl(grid, config, false)
}

/// Corrected SE121 nested-chain collector with exact negative root pruning.
pub(crate) fn collect_forcing_chain_proofs_se121(
    grid: &Grid,
    config: EngineConfig,
) -> Vec<NestedHint> {
    collect_forcing_chain_proofs_impl(grid, config, true)
}

fn collect_forcing_chain_proofs_impl(
    grid: &Grid,
    config: EngineConfig,
    use_root_prefilter: bool,
) -> Vec<NestedHint> {
    let implications = Implications::new(grid, config);
    let mut workspace = StaticChainWorkspace::new();
    let mut result = NestedHintCollector::new();
    let root_prefilters = use_root_prefilter.then(|| StaticModePrefilters::new(&implications));

    for (kind_cycle, kind_forcing, y_enabled, x_enabled, sort_key) in [
        (ChainKind::XCycle, ChainKind::XForcing, false, true, 2),
        (ChainKind::YCycle, ChainKind::XForcing, true, false, 3),
        (ChainKind::XyCycle, ChainKind::XyForcing, true, true, 4),
    ] {
        let cycle_prefilter = root_prefilters
            .as_ref()
            .map(|filters| filters.cycle(y_enabled, x_enabled));
        let forcing_prefilter = root_prefilters.as_ref().map(StaticModePrefilters::forcing);
        for raw_cell in 0_u8..81 {
            let cell = CellId::new(raw_cell).expect("cell index loop");
            if grid.value(cell) != 0 {
                continue;
            }
            let values = grid.candidates(cell);
            if values.count() <= 1 || (!x_enabled && values.count() > 2) {
                continue;
            }
            for digit in values.iter() {
                search_cycles_prefiltered(
                    &mut workspace,
                    &implications,
                    cycle_prefilter,
                    cell,
                    digit,
                    y_enabled,
                    x_enabled,
                );
                if !workspace.cycles.is_empty() {
                    let forward_arena = workspace.arena.proof_arena();
                    for &terminal in &workspace.cycles {
                        if let Some(candidate) = cycle_nested_hint(
                            grid,
                            &workspace.arena,
                            Arc::clone(&forward_arena),
                            terminal,
                            kind_cycle,
                            sort_key,
                        ) {
                            result.offer(candidate);
                        }
                    }
                }

                if x_enabled {
                    if let Some(terminal) = search_forcing_prefiltered(
                        &mut workspace,
                        &implications,
                        forcing_prefilter,
                        cell,
                        digit,
                        true,
                        (y_enabled, x_enabled),
                    ) {
                        result.offer(forcing_nested_hint(
                            grid,
                            &workspace.arena,
                            terminal,
                            kind_forcing,
                            sort_key,
                        ));
                    }
                    if let Some(terminal) = search_forcing_prefiltered(
                        &mut workspace,
                        &implications,
                        forcing_prefilter,
                        cell,
                        digit,
                        false,
                        (y_enabled, x_enabled),
                    ) {
                        result.offer(forcing_nested_hint(
                            grid,
                            &workspace.arena,
                            terminal,
                            kind_forcing,
                            sort_key,
                        ));
                    }
                }
            }
        }
    }

    result.finish()
}

fn cycle_nested_hint(
    grid: &Grid,
    arena: &Arena,
    forward_arena: Arc<ProofArena>,
    terminal: u32,
    kind: ChainKind,
    sort_key: u8,
) -> Option<NestedHint> {
    let removals = cycle_removals(grid, arena, terminal);
    if removals.is_empty() {
        return None;
    }
    let proof = Arc::new(ChainProof::with_complexity_target_count(
        ProofKind::Other,
        vec![
            ProofTarget {
                arena: forward_arena,
                node: terminal,
            },
            reversed_cycle_target(arena, terminal),
        ],
        1,
    ));
    let complexity = proof.complexity();
    let (_, java_difficulty) = chain_rating(
        kind,
        u16::try_from(complexity).expect("static cycle complexity bound"),
    );
    Some(NestedHint {
        proof,
        removals,
        java_difficulty,
        complexity,
        sort_key,
    })
}

fn forcing_nested_hint(
    grid: &Grid,
    arena: &Arena,
    terminal: u32,
    kind: ChainKind,
    sort_key: u8,
) -> NestedHint {
    let removals = forcing_removals(grid, arena, terminal);
    debug_assert!(!removals.is_empty());
    let proof = Arc::new(ChainProof::new(
        ProofKind::Other,
        vec![ProofTarget {
            arena: arena.proof_arena(),
            node: terminal,
        }],
    ));
    let complexity = proof.complexity();
    let (_, java_difficulty) = chain_rating(
        kind,
        u16::try_from(complexity).expect("static forcing complexity bound"),
    );
    NestedHint {
        proof,
        removals,
        java_difficulty,
        complexity,
        sort_key,
    }
}

fn reversed_cycle_target(arena: &Arena, terminal: u32) -> ProofTarget {
    let path = arena.path_to_root(terminal);
    let mut nodes = Vec::with_capacity(path.len());
    let mut parents = Vec::with_capacity(path.len().saturating_sub(1));
    for (index, &forward_node) in path.iter().rev().enumerate() {
        let source = arena.node(forward_node);
        let parent_start = u32::try_from(parents.len()).expect("reversed cycle parent storage");
        let parent_count = if index + 1 == path.len() {
            0
        } else {
            parents.push(u32::try_from(index + 1).expect("reversed cycle node index"));
            1
        };
        nodes.push(ProofNode {
            key: source.key ^ 1,
            parent_start,
            parent_count,
            on_cause: source.on_cause,
            nested: None,
        });
    }
    ProofTarget {
        arena: Arc::new(ProofArena::new(nodes, parents)),
        node: 0,
    }
}

fn keep_best(best: &mut Option<RankedInference>, candidate: RankedInference) {
    if best
        .as_ref()
        .is_none_or(|current| candidate.precedes(current))
    {
        *best = Some(candidate);
    }
}

fn keep_best_with_locator(
    best: &mut Option<(RankedInference, SelectedProofLocator)>,
    candidate: RankedInference,
    locator: SelectedProofLocator,
) {
    if best
        .as_ref()
        .is_none_or(|(current, _)| candidate.precedes(current))
    {
        *best = Some((candidate, locator));
    }
}

fn materialize_selected_proof(
    workspace: &mut StaticChainWorkspace,
    implications: &Implications,
    grid: &Grid,
    locator: SelectedProofLocator,
) -> SelectedChainProof {
    match locator {
        SelectedProofLocator::Cycle {
            source_cell,
            source_digit,
            y_enabled,
            x_enabled,
            cycle_index,
        } => {
            search_cycles(
                workspace,
                implications,
                source_cell,
                source_digit,
                y_enabled,
                x_enabled,
            );
            let terminal = *workspace
                .cycles
                .get(cycle_index)
                .expect("ranked cycle is reproducible");
            let path = workspace.arena.path_to_root(terminal);
            SelectedChainProof::new(vec![
                materialize_chain_view(
                    grid,
                    &workspace.arena,
                    &path,
                    ChainProofViewKind::CycleForward,
                    false,
                ),
                materialize_chain_view(
                    grid,
                    &workspace.arena,
                    &path,
                    ChainProofViewKind::CycleReverse,
                    true,
                ),
            ])
        }
        SelectedProofLocator::Forcing {
            source_cell,
            source_digit,
            source_on,
            y_enabled,
            x_enabled,
        } => {
            let terminal = search_forcing(
                workspace,
                implications,
                source_cell,
                source_digit,
                source_on,
                y_enabled,
                x_enabled,
            )
            .expect("ranked forcing chain is reproducible");
            let path = workspace.arena.path_to_root(terminal);
            SelectedChainProof::new(vec![materialize_chain_view(
                grid,
                &workspace.arena,
                &path,
                ChainProofViewKind::Forcing,
                false,
            )])
        }
    }
}

fn materialize_chain_view(
    grid: &Grid,
    arena: &Arena,
    target_to_root: &[u32],
    kind: ChainProofViewKind,
    reverse: bool,
) -> ChainProofView {
    debug_assert!(!target_to_root.is_empty());
    let mut nodes = Vec::with_capacity(target_to_root.len());

    if reverse {
        for (index, &node_id) in target_to_root.iter().rev().enumerate() {
            let node = arena.node(node_id);
            let (cell, digit) = decode_candidate(node.key);
            let original_index = target_to_root.len() - 1 - index;
            let cause =
                original_index
                    .checked_sub(1)
                    .map_or(ChainCause::None, |parent_original_index| {
                        chain_cause(grid, arena, target_to_root[parent_original_index])
                    });
            let parents = chain_view_parents(index, target_to_root.len(), cause);
            nodes.push(ChainProofNode::new(
                cell,
                digit,
                chain_state(node.key ^ 1),
                parents,
            ));
        }
    } else {
        for (index, &node_id) in target_to_root.iter().enumerate() {
            let node = arena.node(node_id);
            let (cell, digit) = decode_candidate(node.key);
            let parents = chain_view_parents(
                index,
                target_to_root.len(),
                chain_cause(grid, arena, node_id),
            );
            nodes.push(ChainProofNode::new(
                cell,
                digit,
                chain_state(node.key),
                parents,
            ));
        }
    }

    ChainProofView::new(kind, nodes)
}

fn chain_view_parents(
    index: usize,
    node_count: usize,
    cause: ChainCause,
) -> Box<[ChainProofParent]> {
    if index + 1 == node_count {
        Box::new([])
    } else {
        Box::new([ChainProofParent::new(
            ChainNodeId::from_index(index + 1),
            cause,
        )])
    }
}

fn chain_state(key: u16) -> ChainState {
    if is_on(key) {
        ChainState::On
    } else {
        ChainState::Off
    }
}

fn chain_cause(grid: &Grid, arena: &Arena, node_id: u32) -> ChainCause {
    let node = arena.node(node_id);
    match node.on_cause {
        OnCause::None => ChainCause::None,
        OnCause::HiddenRegion(type_index) => {
            let (cell, _) = decode_candidate(node.key);
            let region_index = grid
                .topology()
                .cell_region_index(cell, usize::from(type_index))
                .expect("chain cause region contains its potential");
            ChainCause::Region(
                RegionId::new(type_index, region_index).expect("chain cause region id"),
            )
        }
        OnCause::NakedSingle => {
            if node.parent == NO_NODE {
                return ChainCause::None;
            }
            let (cell, _) = decode_candidate(node.key);
            let (parent_cell, _) = decode_candidate(arena.node(node.parent).key);
            if cell == parent_cell {
                ChainCause::Cell
            } else {
                ChainCause::Visibility
            }
        }
    }
}

fn search_cycles_prefiltered(
    workspace: &mut StaticChainWorkspace,
    implications: &Implications,
    root_prefilter: Option<&StaticRootPrefilter>,
    source_cell: CellId,
    source_digit: Digit,
    y_enabled: bool,
    x_enabled: bool,
) {
    let source_key = potential_key(source_cell, source_digit, true);
    if root_prefilter.is_some_and(|filter| !filter.cycle_possible(source_key)) {
        // Callers inspect `cycles` immediately even when no traversal runs.
        workspace.clear();
        return;
    }
    search_cycles(
        workspace,
        implications,
        source_cell,
        source_digit,
        y_enabled,
        x_enabled,
    );
}

fn search_forcing_prefiltered(
    workspace: &mut StaticChainWorkspace,
    implications: &Implications,
    root_prefilter: Option<&StaticRootPrefilter>,
    source_cell: CellId,
    source_digit: Digit,
    source_on: bool,
    enabled_links: (bool, bool),
) -> Option<u32> {
    let source_key = potential_key(source_cell, source_digit, source_on);
    if root_prefilter.is_some_and(|filter| !filter.forcing_possible(source_key)) {
        // Preserve the same clean-workspace postcondition as a failed search.
        workspace.clear();
        return None;
    }
    search_forcing(
        workspace,
        implications,
        source_cell,
        source_digit,
        source_on,
        enabled_links.0,
        enabled_links.1,
    )
}

fn search_cycles(
    workspace: &mut StaticChainWorkspace,
    implications: &Implications,
    source_cell: CellId,
    source_digit: Digit,
    y_enabled: bool,
    x_enabled: bool,
) {
    workspace.clear();
    let arena = &mut workspace.arena;
    let source_key = potential_key(source_cell, source_digit, true);
    let source = arena.root(source_key);
    workspace.to_on.add(arena, source);
    workspace.pending_on.push_back(source);
    let mut length = 0_u16;

    while !workspace.pending_on.is_empty() || !workspace.pending_off.is_empty() {
        length += 1;
        while let Some(parent) = workspace.pending_on.pop_front() {
            let parent_key = arena.node(parent).key;
            implications.for_each_off_with_cause(parent_key, y_enabled, |target_key, cause| {
                let target = arena.push_with_cause(target_key, parent, cause);
                if !arena.has_parent_key(parent, target_key) {
                    workspace.pending_off.push_back(target);
                    workspace.to_off.add(arena, target);
                }
            });
        }

        length += 1;
        while let Some(parent) = workspace.pending_off.pop_front() {
            let parent_key = arena.node(parent).key;
            implications.for_each_on_with_cause(
                parent_key,
                y_enabled,
                x_enabled,
                |target_key, cause| {
                    let target = arena.push_with_cause(target_key, parent, cause);
                    if length >= 4 && target_key == source_key {
                        workspace.cycles.push(target);
                    }
                    if workspace.to_on.add_if_absent(arena, target) {
                        workspace.pending_on.push_back(target);
                    }
                },
            );
        }
    }
}

fn search_forcing(
    workspace: &mut StaticChainWorkspace,
    implications: &Implications,
    source_cell: CellId,
    source_digit: Digit,
    source_on: bool,
    y_enabled: bool,
    x_enabled: bool,
) -> Option<u32> {
    workspace.clear();
    let arena = &mut workspace.arena;
    let source_key = potential_key(source_cell, source_digit, source_on);
    let opposite_source = source_key ^ 1;
    let source = arena.root(source_key);
    workspace.visited[usize::from(source_key)] = true;
    workspace.visited_keys.push(source_key);
    if source_on {
        workspace.pending_on.push_back(source);
    } else {
        workspace.pending_off.push_back(source);
    }

    while !workspace.pending_on.is_empty() || !workspace.pending_off.is_empty() {
        while let Some(parent) = workspace.pending_on.pop_front() {
            let parent_key = arena.node(parent).key;
            let mut found = None;
            implications.for_each_off_with_cause(parent_key, y_enabled, |target_key, cause| {
                if found.is_some() {
                    return;
                }
                let target = arena.push_with_cause(target_key, parent, cause);
                if target_key == opposite_source {
                    found = Some(target);
                } else if !workspace.visited[usize::from(target_key)] {
                    workspace.visited[usize::from(target_key)] = true;
                    workspace.visited_keys.push(target_key);
                    workspace.pending_off.push_back(target);
                }
            });
            if let Some(target) = found {
                return Some(target);
            }
        }

        while let Some(parent) = workspace.pending_off.pop_front() {
            let parent_key = arena.node(parent).key;
            let mut found = None;
            implications.for_each_on_with_cause(
                parent_key,
                y_enabled,
                x_enabled,
                |target_key, cause| {
                    if found.is_some() {
                        return;
                    }
                    let target = arena.push_with_cause(target_key, parent, cause);
                    if target_key == opposite_source {
                        found = Some(target);
                    } else if !workspace.visited[usize::from(target_key)] {
                        workspace.visited[usize::from(target_key)] = true;
                        workspace.visited_keys.push(target_key);
                        workspace.pending_on.push_back(target);
                    }
                },
            );
            if let Some(target) = found {
                return Some(target);
            }
        }
    }
    None
}

fn cycle_removals(grid: &Grid, arena: &Arena, terminal: u32) -> CandidateRemovals {
    let path = arena.path_to_root(terminal);
    debug_assert!(path.len() >= 2);
    let mut chain_cells = [false; 81];
    for &node in &path[..path.len() - 1] {
        let (cell, _) = decode_candidate(arena.node(node).key);
        chain_cells[cell.index()] = true;
    }

    let mut cancel_forward = [false; CANDIDATE_COUNT];
    let mut cancel_back = [false; CANDIDATE_COUNT];
    let mut forward_order = Vec::new();
    for &node in &path[..path.len() - 1] {
        let node_key = arena.node(node).key;
        let (cell, digit) = decode_candidate(node_key);
        for &raw_peer in grid.topology().visible_peers(cell) {
            let peer = CellId::new(raw_peer).expect("visible peer");
            if chain_cells[peer.index()] || !grid.candidates(peer).contains(digit) {
                continue;
            }
            let candidate = candidate_index(peer, digit);
            if is_on(node_key) {
                if !cancel_forward[candidate] {
                    cancel_forward[candidate] = true;
                    forward_order.push(candidate);
                }
            } else {
                cancel_back[candidate] = true;
            }
        }
    }

    let mut removals = CandidateRemovalsBuilder::with_capacity(forward_order.len());
    for candidate in forward_order {
        if cancel_back[candidate] {
            let (cell, digit) = decode_candidate_index(candidate);
            removals.add(cell, CandidateMask::of(digit));
        }
    }
    removals.build()
}

fn forcing_removals(grid: &Grid, arena: &Arena, terminal: u32) -> CandidateRemovals {
    let target_key = arena.node(terminal).key;
    let (target_cell, target_digit) = decode_candidate(target_key);
    let mut removals = CandidateRemovalsBuilder::with_capacity(1);
    let mask = if is_on(target_key) {
        grid.candidates(target_cell)
            .without(CandidateMask::of(target_digit))
    } else {
        CandidateMask::of(target_digit)
    };
    if !mask.is_empty() {
        removals.add(target_cell, mask);
    }
    removals.build()
}

fn cycle_inference(
    grid: &Grid,
    arena: &Arena,
    terminal: u32,
    kind: ChainKind,
    sort_key: u8,
) -> Option<RankedInference> {
    let path = arena.path_to_root(terminal);
    debug_assert!(path.len() >= 2);
    let mut chain_cells = [false; 81];
    for &node in &path[..path.len() - 1] {
        let (cell, _) = decode_candidate(arena.node(node).key);
        chain_cells[cell.index()] = true;
    }

    let mut cancel_forward = [false; CANDIDATE_COUNT];
    let mut cancel_back = [false; CANDIDATE_COUNT];
    let mut forward_order = Vec::new();
    for &node in &path[..path.len() - 1] {
        let node_key = arena.node(node).key;
        let (cell, digit) = decode_candidate(node_key);
        for &raw_peer in grid.topology().visible_peers(cell) {
            let peer = CellId::new(raw_peer).expect("visible peer");
            if chain_cells[peer.index()] || !grid.candidates(peer).contains(digit) {
                continue;
            }
            let candidate = candidate_index(peer, digit);
            if is_on(node_key) {
                if !cancel_forward[candidate] {
                    cancel_forward[candidate] = true;
                    forward_order.push(candidate);
                }
            } else {
                cancel_back[candidate] = true;
            }
        }
    }

    let mut removals = CandidateRemovalsBuilder::with_capacity(forward_order.len());
    for candidate in forward_order {
        if cancel_back[candidate] {
            let (cell, digit) = decode_candidate_index(candidate);
            removals.add(cell, CandidateMask::of(digit));
        }
    }
    let removals = removals.build();
    if removals.is_empty() {
        return None;
    }

    let mut selected_cells = ChainCellSequence::new();
    let mut selected = [false; 81];
    for &node in path[..path.len() - 1].iter().rev() {
        let (cell, _) = decode_candidate(arena.node(node).key);
        if !selected[cell.index()] {
            selected[cell.index()] = true;
            selected_cells.push(cell);
        }
    }
    let complexity = ancestor_complexity(arena, terminal);
    let (rating, java_difficulty) = chain_rating(kind, complexity);
    let target_key = arena.node(terminal).key;
    let (target_cell, target_digit) = decode_candidate(target_key);
    let inference = Inference::elimination(
        Technique::ForcingChainCycle,
        rating,
        removals,
        Evidence::ForcingChainCycle {
            kind,
            target_cell,
            target_digit,
            target_on: true,
            complexity,
            selected_cells,
        },
    );
    Some(RankedInference {
        inference,
        java_difficulty,
        complexity,
        sort_key,
    })
}

fn forcing_inference(
    grid: &Grid,
    arena: &Arena,
    terminal: u32,
    kind: ChainKind,
    sort_key: u8,
) -> RankedInference {
    let target_key = arena.node(terminal).key;
    let (target_cell, target_digit) = decode_candidate(target_key);
    let target_on = is_on(target_key);
    let complexity = ancestor_complexity(arena, terminal);
    let (rating, java_difficulty) = chain_rating(kind, complexity);
    let evidence = Evidence::ForcingChainCycle {
        kind,
        target_cell,
        target_digit,
        target_on,
        complexity,
        selected_cells: ChainCellSequence::new(),
    };
    let inference = if target_on {
        Inference::placement(
            Technique::ForcingChainCycle,
            rating,
            target_cell,
            target_digit,
            evidence,
        )
    } else {
        let mut removals = CandidateRemovalsBuilder::with_capacity(1);
        debug_assert!(grid.candidates(target_cell).contains(target_digit));
        removals.add(target_cell, CandidateMask::of(target_digit));
        Inference::elimination(
            Technique::ForcingChainCycle,
            rating,
            removals.build(),
            evidence,
        )
    };
    RankedInference {
        inference,
        java_difficulty,
        complexity,
        sort_key,
    }
}

fn ancestor_complexity(arena: &Arena, terminal: u32) -> u16 {
    let mut seen = [false; KEY_COUNT];
    let mut result = 0_u16;
    let mut current = terminal;
    loop {
        let node = arena.node(current);
        let key = usize::from(node.key);
        if !seen[key] {
            seen[key] = true;
            result += 1;
        }
        if node.parent == NO_NODE {
            return result;
        }
        current = node.parent;
    }
}

fn chain_rating(kind: ChainKind, complexity: u16) -> (Rating, f64) {
    let (base_tenths, base) = match kind {
        ChainKind::XCycle | ChainKind::YCycle => (65_u16, 6.5_f64),
        ChainKind::XForcing => (66, 6.6),
        ChainKind::XyCycle | ChainKind::XyForcing => (70, 7.0),
    };
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
    (Rating::from_tenths(base_tenths + increments), base + added)
}

pub(crate) fn active_region_types(grid: &Grid, config: EngineConfig) -> Vec<usize> {
    let topology = grid.topology();
    let mut result = Vec::with_capacity(REGION_TYPE_COUNT);
    if topology.config().blocks {
        result.push(0);
    }
    result.extend([1, 2]);
    if !effective_variant_latin(grid, config) {
        for type_index in 3..REGION_TYPE_COUNT {
            if topology.is_region_type_active(type_index) {
                result.push(type_index);
            }
        }
    }
    result
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

fn candidate_index(cell: CellId, digit: Digit) -> usize {
    cell.index() * 9 + usize::from(digit.get() - 1)
}

pub(crate) fn potential_key(cell: CellId, digit: Digit, on: bool) -> u16 {
    u16::try_from(candidate_index(cell, digit) * 2 + usize::from(on)).expect("potential key")
}

fn candidate_from_key(key: u16) -> usize {
    usize::from(key >> 1)
}

pub(crate) fn is_on(key: u16) -> bool {
    key & 1 != 0
}

pub(crate) fn decode_candidate(key: u16) -> (CellId, Digit) {
    decode_candidate_index(candidate_from_key(key))
}

fn decode_candidate_index(candidate: usize) -> (CellId, Digit) {
    (
        CellId::new(u8::try_from(candidate / 9).expect("candidate cell"))
            .expect("candidate cell id"),
        Digit::new(u8::try_from(candidate % 9 + 1).expect("candidate digit"))
            .expect("candidate digit id"),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{
        CandidateMask, CellId, ConstraintTopology, Digit, Grid, Puzzle, RegionId, VariantConfig,
    };

    use super::{
        Arena, Implications, OnCause, StaticChainWorkspace, StaticModePrefilters,
        StaticRootPrefilter, chain_rating, collect_forcing_chain_cycles,
        collect_forcing_chain_proofs, collect_forcing_chain_proofs_se121, find_forcing_chain_cycle,
        find_forcing_chain_cycle_with_proof, potential_key, replay_forcing_chain_cycle_with_proof,
        reversed_cycle_target, search_cycles, search_cycles_prefiltered, search_forcing,
    };
    use crate::{
        ChainCause, ChainKind, ChainProofView, ChainProofViewKind, ChainState, EngineConfig,
        Evidence, Rating,
    };

    fn sparse_snapshot(entries: &[(u8, &str)], variant: VariantConfig) -> Grid {
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

    fn cell(raw: u8) -> CellId {
        CellId::new(raw).unwrap()
    }

    fn mask(digits: &str) -> CandidateMask {
        let mut bits = 0_u16;
        for byte in digits.bytes() {
            bits |= 1_u16 << (byte - b'0');
        }
        CandidateMask::from_bits(bits)
    }

    fn assert_linear_target_first(view: &ChainProofView) {
        assert_eq!(view.target().raw(), 0);
        for (index, node) in view.nodes().iter().enumerate() {
            if index + 1 == view.nodes().len() {
                assert!(node.parents().is_empty());
            } else {
                assert_eq!(node.parents().len(), 1);
                assert_eq!(
                    node.parents()[0].node(),
                    crate::ChainNodeId::from_index(index + 1)
                );
            }
        }
    }

    fn edge_causes(view: &ChainProofView) -> Vec<ChainCause> {
        view.nodes()
            .iter()
            .flat_map(|node| node.parents().iter().map(|parent| parent.cause()))
            .collect()
    }

    fn assert_static_root_prefilter_equivalent(grid: &Grid) {
        let config = EngineConfig::default();
        let implications = Implications::new(grid, config);
        let filters = StaticModePrefilters::new(&implications);
        let mut workspace = StaticChainWorkspace::new();

        for (y_enabled, x_enabled) in [(false, true), (true, false), (true, true)] {
            let cycle_filter = filters.cycle(y_enabled, x_enabled);
            for raw_cell in 0_u8..81 {
                let cell = CellId::new(raw_cell).unwrap();
                for digit in grid.candidates(cell).iter() {
                    search_cycles(
                        &mut workspace,
                        &implications,
                        cell,
                        digit,
                        y_enabled,
                        x_enabled,
                    );
                    assert!(
                        workspace.cycles.is_empty()
                            || cycle_filter.cycle_possible(potential_key(cell, digit, true)),
                        "cycle false negative at {cell:?}.{digit:?}, Y={y_enabled}, X={x_enabled}"
                    );

                    if x_enabled {
                        for source_on in [true, false] {
                            let found = search_forcing(
                                &mut workspace,
                                &implications,
                                cell,
                                digit,
                                source_on,
                                y_enabled,
                                x_enabled,
                            )
                            .is_some();
                            assert!(
                                !found
                                    || filters
                                        .forcing()
                                        .forcing_possible(potential_key(cell, digit, source_on,)),
                                "forcing false negative at {cell:?}.{digit:?}={source_on}, Y={y_enabled}, X={x_enabled}"
                            );
                        }
                    }
                }
            }
        }

        let unfiltered = collect_forcing_chain_proofs(grid, config);
        let filtered = collect_forcing_chain_proofs_se121(grid, config);
        assert_eq!(filtered.len(), unfiltered.len());
        for (filtered, unfiltered) in filtered.iter().zip(&unfiltered) {
            assert_eq!(filtered.removals, unfiltered.removals);
            assert_eq!(filtered.java_difficulty, unfiltered.java_difficulty);
            assert_eq!(filtered.complexity, unfiltered.complexity);
            assert_eq!(filtered.sort_key, unfiltered.sort_key);
            assert_eq!(filtered.proof.fingerprint(), unfiltered.proof.fingerprint());
        }
    }

    #[test]
    fn se121_root_prefilter_has_no_false_negatives_and_preserves_ordered_results() {
        for grid in [
            sparse_snapshot(
                &[(0, "12"), (3, "13"), (30, "14"), (27, "15"), (6, "16")],
                VariantConfig::default(),
            ),
            sparse_snapshot(
                &[(0, "12"), (3, "23"), (30, "34"), (27, "14"), (54, "1")],
                VariantConfig::default(),
            ),
            sparse_snapshot(
                &[(0, "19"), (3, "12"), (30, "234"), (27, "29")],
                VariantConfig::default(),
            ),
        ] {
            assert_static_root_prefilter_equivalent(&grid);
        }
    }

    #[test]
    fn rejected_root_clears_reused_static_workspace() {
        let grid = sparse_snapshot(
            &[(0, "12"), (3, "13"), (30, "14"), (27, "15"), (6, "16")],
            VariantConfig::default(),
        );
        let implications = Implications::new(&grid, EngineConfig::default());
        let filter = StaticRootPrefilter::new(&implications, false, true, false);
        let mut workspace = StaticChainWorkspace::new();
        search_cycles(
            &mut workspace,
            &implications,
            cell(0),
            Digit::new(1).unwrap(),
            false,
            true,
        );
        assert!(!workspace.arena.nodes.is_empty());

        let rejected = (0_u8..81).find_map(|raw_cell| {
            let cell = cell(raw_cell);
            grid.candidates(cell)
                .iter()
                .find(|&digit| !filter.cycle_possible(potential_key(cell, digit, true)))
                .map(|digit| (cell, digit))
        });
        let (source_cell, source_digit) = rejected.expect("fixture has a rejected root");
        search_cycles_prefiltered(
            &mut workspace,
            &implications,
            Some(&filter),
            source_cell,
            source_digit,
            false,
            true,
        );
        assert!(workspace.arena.nodes.is_empty());
        assert!(workspace.cycles.is_empty());
    }

    #[test]
    fn weak_link_causes_survive_cycle_reversal() {
        let grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &Puzzle::parse(&".".repeat(81)).unwrap(),
        );
        let implications = Implications::new(&grid, EngineConfig::default());
        let source_cell = cell(0);
        let one = Digit::new(1).unwrap();
        let mut links = Vec::new();
        implications.for_each_off_with_cause(
            potential_key(source_cell, one, true),
            true,
            |key, cause| links.push((key, cause)),
        );
        let cause_of = |target_cell, target_digit| {
            links
                .iter()
                .find(|(key, _)| *key == potential_key(target_cell, target_digit, false))
                .map(|(_, cause)| *cause)
        };
        assert_eq!(
            cause_of(source_cell, Digit::new(2).unwrap()),
            Some(OnCause::NakedSingle)
        );
        assert_eq!(cause_of(cell(1), one), Some(OnCause::HiddenRegion(0)));
        assert_eq!(cause_of(cell(3), one), Some(OnCause::HiddenRegion(1)));
        assert_eq!(cause_of(cell(27), one), Some(OnCause::HiddenRegion(2)));

        let mut arena = Arena::new();
        let root = arena.root(potential_key(source_cell, one, true));
        let weak = arena.push_with_cause(
            potential_key(cell(3), one, false),
            root,
            OnCause::HiddenRegion(1),
        );
        let terminal = arena.push_with_cause(
            potential_key(source_cell, one, true),
            weak,
            OnCause::HiddenRegion(0),
        );
        let reversed = reversed_cycle_target(&arena, terminal);
        let reversed_weak = reversed.arena.node(1);
        assert_eq!(reversed_weak.key, potential_key(cell(3), one, true));
        assert_eq!(reversed_weak.on_cause, OnCause::HiddenRegion(1));
    }

    #[test]
    fn generalized_x_wing_matches_the_release_oracle() {
        let mut grid = sparse_snapshot(
            &[(0, "12"), (3, "13"), (30, "14"), (27, "15"), (6, "16")],
            VariantConfig::default(),
        );
        let inference =
            find_forcing_chain_cycle(&grid, EngineConfig::default()).expect("generalized X-Wing");
        assert_eq!(inference.rating(), Rating::from_tenths(65));
        assert_eq!(inference.name(), "Generalized X-Wing");
        assert_eq!(inference.short_name(), "GXW");
        assert_eq!(
            inference.description(grid.topology()),
            "Generalized X-Wing: r1c4,r4c4,r4c1,r1c1"
        );
        let Evidence::ForcingChainCycle {
            kind,
            complexity,
            selected_cells,
            ..
        } = inference.evidence()
        else {
            panic!("chain evidence");
        };
        assert_eq!(kind, ChainKind::XCycle);
        assert_eq!(complexity, 4);
        assert_eq!(
            selected_cells.iter().map(CellId::raw).collect::<Vec<_>>(),
            vec![3, 30, 27, 0]
        );
        inference.apply(&mut grid);
        assert_eq!(grid.candidates(cell(6)), mask("6"));
    }

    #[test]
    fn all_static_chains_keep_rank_order_and_replay_a_nonfirst_proof() {
        let grid = sparse_snapshot(
            &[
                (0, "12"),
                (3, "13"),
                (30, "14"),
                (27, "15"),
                (6, "16"),
                (36, "78"),
                (39, "79"),
                (66, "67"),
                (63, "57"),
                (42, "47"),
            ],
            VariantConfig::default(),
        );
        let config = EngineConfig::default();
        let hints = collect_forcing_chain_cycles(&grid, config);
        assert!(hints.len() > 1, "fixture must expose multiple FCC effects");
        assert_eq!(
            find_forcing_chain_cycle(&grid, config).as_ref(),
            hints.first()
        );

        let selected = &hints[1];
        let first_replay = replay_forcing_chain_cycle_with_proof(&grid, config, selected).unwrap();
        let second_replay = replay_forcing_chain_cycle_with_proof(&grid, config, selected).unwrap();
        assert_eq!(first_replay.inference(), selected);
        assert_eq!(first_replay.proof(), second_replay.proof());
    }

    #[test]
    fn selected_cycle_proof_matches_compact_winner_and_preserves_both_views() {
        let grid = sparse_snapshot(
            &[(0, "12"), (3, "13"), (30, "14"), (27, "15"), (6, "16")],
            VariantConfig::default(),
        );
        let compact =
            find_forcing_chain_cycle(&grid, EngineConfig::default()).expect("compact cycle");
        let detailed = find_forcing_chain_cycle_with_proof(&grid, EngineConfig::default())
            .expect("selected cycle proof");
        assert_eq!(detailed.inference(), &compact);

        let views = detailed.proof().views();
        assert_eq!(views.len(), 2);
        assert_eq!(views[0].kind(), ChainProofViewKind::CycleForward);
        assert_eq!(views[1].kind(), ChainProofViewKind::CycleReverse);
        assert_linear_target_first(&views[0]);
        assert_linear_target_first(&views[1]);

        let forward = views[0]
            .nodes()
            .iter()
            .map(|node| (node.cell().raw(), node.digit().get(), node.state()))
            .collect::<Vec<_>>();
        assert_eq!(
            forward,
            [
                (0, 1, ChainState::On),
                (27, 1, ChainState::Off),
                (30, 1, ChainState::On),
                (3, 1, ChainState::Off),
                (0, 1, ChainState::On),
            ]
        );
        let reverse = views[1]
            .nodes()
            .iter()
            .map(|node| (node.cell().raw(), node.digit().get(), node.state()))
            .collect::<Vec<_>>();
        assert_eq!(
            reverse,
            [
                (0, 1, ChainState::Off),
                (3, 1, ChainState::On),
                (30, 1, ChainState::Off),
                (27, 1, ChainState::On),
                (0, 1, ChainState::Off),
            ]
        );
        assert_eq!(
            edge_causes(&views[0]),
            [
                ChainCause::Region(RegionId::new(2, 0).unwrap()),
                ChainCause::Region(RegionId::new(1, 3).unwrap()),
                ChainCause::Region(RegionId::new(2, 3).unwrap()),
                ChainCause::Region(RegionId::new(1, 0).unwrap()),
            ]
        );
        assert_eq!(
            edge_causes(&views[1]),
            [
                ChainCause::Region(RegionId::new(1, 0).unwrap()),
                ChainCause::Region(RegionId::new(2, 3).unwrap()),
                ChainCause::Region(RegionId::new(1, 3).unwrap()),
                ChainCause::Region(RegionId::new(2, 0).unwrap()),
            ]
        );
    }

    #[test]
    fn nested_static_collector_keeps_the_ranked_cycle_proof_and_effect() {
        let grid = sparse_snapshot(
            &[(0, "12"), (3, "13"), (30, "14"), (27, "15"), (6, "16")],
            VariantConfig::default(),
        );
        let hints = collect_forcing_chain_proofs(&grid, EngineConfig::default());
        let first = hints.first().expect("nested static cycle");

        assert_eq!(first.java_difficulty, 6.5);
        assert_eq!(first.complexity, 4);
        assert_eq!(first.sort_key, 2);
        assert_eq!(first.proof.complexity(), 4);
        let effect = first.removals.iter().collect::<Vec<_>>();
        assert_eq!(effect.len(), 1);
        assert_eq!(effect[0].cell(), cell(6));
        assert_eq!(effect[0].digits(), mask("1"));
    }

    #[test]
    fn y_cycle_uses_the_java_bivalue_and_description_order() {
        let mut grid = sparse_snapshot(
            &[(0, "12"), (3, "23"), (30, "34"), (27, "14"), (54, "1")],
            VariantConfig::default(),
        );
        let inference = find_forcing_chain_cycle(&grid, EngineConfig::default()).expect("Y-cycle");
        assert_eq!(inference.rating(), Rating::from_tenths(66));
        assert_eq!(inference.name(), "Bidirectional Y-Cycle");
        assert_eq!(inference.short_name(), "BiYCy");
        assert_eq!(
            inference.description(grid.topology()),
            "Bidirectional Y-Cycle: r4c1,r4c4,r1c4,r1c1"
        );
        inference.apply(&mut grid);
        assert!(grid.candidates(cell(54)).is_empty());
    }

    #[test]
    fn mixed_forcing_chain_matches_the_release_order() {
        let mut grid = sparse_snapshot(
            &[(0, "19"), (3, "12"), (30, "234"), (27, "29")],
            VariantConfig::default(),
        );
        let inference =
            find_forcing_chain_cycle(&grid, EngineConfig::default()).expect("mixed chain");
        assert_eq!(inference.rating(), Rating::from_tenths(71));
        assert_eq!(inference.name(), "Forcing Chain");
        assert_eq!(inference.short_name(), "FC");
        assert_eq!(
            inference.description(grid.topology()),
            "Forcing Chain: r1c1.1 off"
        );
        inference.apply(&mut grid);
        assert_eq!(grid.candidates(cell(0)), mask("9"));
    }

    #[test]
    fn selected_forcing_proof_matches_compact_winner_and_has_one_view() {
        let grid = sparse_snapshot(
            &[(0, "19"), (3, "12"), (30, "234"), (27, "29")],
            VariantConfig::default(),
        );
        let compact =
            find_forcing_chain_cycle(&grid, EngineConfig::default()).expect("compact forcing");
        let detailed = find_forcing_chain_cycle_with_proof(&grid, EngineConfig::default())
            .expect("selected forcing proof");
        assert_eq!(detailed.inference(), &compact);

        let Evidence::ForcingChainCycle {
            target_cell,
            target_digit,
            target_on,
            complexity,
            ..
        } = compact.evidence()
        else {
            panic!("forcing evidence");
        };
        let views = detailed.proof().views();
        assert_eq!(views.len(), 1);
        let view = &views[0];
        assert_eq!(view.kind(), ChainProofViewKind::Forcing);
        assert_linear_target_first(view);
        assert_eq!(view.nodes().len(), usize::from(complexity));
        assert_eq!(
            view.nodes()
                .iter()
                .map(|node| (node.cell().raw(), node.digit().get(), node.state()))
                .collect::<Vec<_>>(),
            [
                (0, 1, ChainState::Off),
                (3, 1, ChainState::On),
                (3, 2, ChainState::Off),
                (30, 2, ChainState::On),
                (27, 2, ChainState::Off),
                (27, 9, ChainState::On),
                (0, 9, ChainState::Off),
                (0, 1, ChainState::On),
            ]
        );
        assert_eq!(
            edge_causes(view),
            [
                ChainCause::Region(RegionId::new(1, 0).unwrap()),
                ChainCause::Cell,
                ChainCause::Region(RegionId::new(2, 3).unwrap()),
                ChainCause::Region(RegionId::new(1, 3).unwrap()),
                ChainCause::Cell,
                ChainCause::Region(RegionId::new(2, 0).unwrap()),
                ChainCause::Cell,
            ]
        );

        let target = &view.nodes()[view.target().index()];
        assert_eq!(target.cell(), target_cell);
        assert_eq!(target.digit(), target_digit);
        assert_eq!(target.state().is_on(), target_on);
        let assumption = view.nodes().last().expect("forcing assumption");
        assert_eq!(assumption.cell(), target_cell);
        assert_eq!(assumption.digit(), target_digit);
        assert_eq!(assumption.state().is_on(), !target_on);
        assert!(assumption.parents().is_empty());
    }

    #[test]
    fn java_raw_double_rank_is_not_collapsed_to_display_tenths() {
        let (cycle_rating, cycle_raw) = chain_rating(ChainKind::XCycle, 9);
        let (forcing_rating, forcing_raw) = chain_rating(ChainKind::XForcing, 7);
        assert_eq!(cycle_rating, Rating::from_tenths(67));
        assert_eq!(forcing_rating, Rating::from_tenths(67));
        assert!(forcing_raw < cycle_raw);
    }
}
