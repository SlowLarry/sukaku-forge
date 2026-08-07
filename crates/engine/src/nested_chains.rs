use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use sukaku_forge_core::{
    CandidateMask, CandidateRemovals, CandidateRemovalsBuilder, CellId, Digit, Grid,
};

use crate::Inference;
use crate::forcing_chains::{KEY_COUNT, decode_candidate, potential_key};

/// A complete inner chaining hint after Java's own worth check. The public
/// result kind stays in the producing module; a containing chain needs only
/// the effect, rank, and proof graph.
pub(crate) struct NestedHint {
    pub(crate) proof: Arc<ChainProof>,
    pub(crate) removals: CandidateRemovals,
    pub(crate) java_difficulty: f64,
    pub(crate) complexity: u32,
    pub(crate) sort_key: u8,
}

impl NestedHint {
    pub(crate) fn precedes(&self, other: &Self) -> bool {
        if self.java_difficulty < other.java_difficulty {
            return true;
        }
        if self.java_difficulty > other.java_difficulty {
            return false;
        }
        (self.complexity, self.sort_key) < (other.complexity, other.sort_key)
    }
}

struct RankedNestedHint {
    hint: NestedHint,
    ordinal: u64,
}

/// Exact online equivalent of Java's stable rank sort followed by an
/// effect-only LinkedHashSet. Losing proofs are dropped immediately instead
/// of retaining every combinatorial Cell/Region-chain duplicate.
pub(crate) struct NestedHintCollector {
    by_effect: HashMap<EffectKey, usize>,
    winners: Vec<RankedNestedHint>,
    next_ordinal: u64,
}

impl NestedHintCollector {
    pub(crate) fn new() -> Self {
        Self {
            by_effect: HashMap::new(),
            winners: Vec::new(),
            next_ordinal: 0,
        }
    }

    pub(crate) fn offer(&mut self, hint: NestedHint) {
        let ordinal = self.next_ordinal;
        self.next_ordinal = self
            .next_ordinal
            .checked_add(1)
            .expect("nested hint discovery ordinal");
        let effect = EffectKey::new(&hint.removals);
        if let Some(&index) = self.by_effect.get(&effect) {
            if hint.precedes(&self.winners[index].hint) {
                self.winners[index] = RankedNestedHint { hint, ordinal };
            }
        } else {
            let index = self.winners.len();
            self.by_effect.insert(effect, index);
            self.winners.push(RankedNestedHint { hint, ordinal });
        }
    }

    pub(crate) fn finish(mut self) -> Vec<NestedHint> {
        self.winners.sort_by(|left, right| {
            left.hint
                .java_difficulty
                .partial_cmp(&right.hint.java_difficulty)
                .expect("finite chaining difficulty")
                .then_with(|| left.hint.complexity.cmp(&right.hint.complexity))
                .then_with(|| left.hint.sort_key.cmp(&right.hint.sort_key))
                .then_with(|| left.ordinal.cmp(&right.ordinal))
        });
        self.winners.into_iter().map(|ranked| ranked.hint).collect()
    }
}

struct RankedPublicInference {
    inference: Inference,
    java_difficulty: f64,
    complexity: u32,
    sort_key: u8,
    ordinal: u64,
}

impl RankedPublicInference {
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

/// Compact public all-hints analogue of [`NestedHintCollector`].
///
/// Java stably sorts chaining hints by difficulty, complexity, and family
/// sort key, then keeps the first hint for each removable-potentials map.
/// For a placement, Java's chain hint populates that map with the other
/// candidates in the target cell. This online form retains only the winning
/// compact [`Inference`] for a Java equality key; proof graphs are deliberately
/// replayed later for the selected inference.
pub(crate) struct InferenceCollector {
    by_effect: HashMap<EffectKey, usize>,
    winners: Vec<RankedPublicInference>,
    next_ordinal: u64,
}

impl InferenceCollector {
    pub(crate) fn new() -> Self {
        Self {
            by_effect: HashMap::new(),
            winners: Vec::new(),
            next_ordinal: 0,
        }
    }

    pub(crate) fn offer(
        &mut self,
        grid: &Grid,
        inference: Inference,
        java_difficulty: f64,
        complexity: u32,
        sort_key: u8,
    ) {
        let ordinal = self.next_ordinal;
        self.next_ordinal = self
            .next_ordinal
            .checked_add(1)
            .expect("public hint discovery ordinal");
        let effect = chaining_effect_key(grid, &inference);
        let candidate = RankedPublicInference {
            inference,
            java_difficulty,
            complexity,
            sort_key,
            ordinal,
        };
        if let Some(&index) = self.by_effect.get(&effect) {
            if candidate.precedes(&self.winners[index]) {
                self.winners[index] = candidate;
            }
        } else {
            let index = self.winners.len();
            self.by_effect.insert(effect, index);
            self.winners.push(candidate);
        }
    }

    pub(crate) fn finish(mut self) -> Vec<Inference> {
        self.winners.sort_by(|left, right| {
            left.java_difficulty
                .partial_cmp(&right.java_difficulty)
                .expect("finite chaining difficulty")
                .then_with(|| left.complexity.cmp(&right.complexity))
                .then_with(|| left.sort_key.cmp(&right.sort_key))
                .then_with(|| left.ordinal.cmp(&right.ordinal))
        });
        self.winners
            .into_iter()
            .map(|ranked| ranked.inference)
            .collect()
    }
}

/// Canonical Java removable-potentials key for a public chain inference.
///
/// Elimination entry order is ignored. A placement reconstructs the map Java
/// stores for chaining hints: all other candidates in the target cell.
pub(crate) fn chaining_effect_key(grid: &Grid, inference: &Inference) -> EffectKey {
    if let (Some(cell), Some(digit)) = (inference.placement_cell(), inference.placement_digit()) {
        let mut removals = CandidateRemovalsBuilder::with_capacity(1);
        removals.add(
            cell,
            grid.candidates(cell).without(CandidateMask::of(digit)),
        );
        EffectKey::new(&removals.build())
    } else {
        EffectKey::new(inference.removals())
    }
}

/// Why an implication edge exists. Java normally consults causes on ON nodes;
/// cycle reversal can turn a caused weak/OFF node into an ON parent as well.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OnCause {
    None,
    NakedSingle,
    HiddenRegion(u8),
}

/// Java synthesizes a cause for the root assumptions of Cell and Region
/// forcing hints while collecting an inner hint's outer parents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProofKind {
    Other,
    Cell,
    Region(u8),
}

#[derive(Clone)]
pub(crate) struct ProofNode {
    pub(crate) key: u16,
    pub(crate) parent_start: u32,
    pub(crate) parent_count: u16,
    pub(crate) on_cause: OnCause,
    pub(crate) nested: Option<Arc<ChainProof>>,
}

/// Immutable node storage shared by all hints harvested from one completed
/// branch. Node identity is retained: two nodes may have the same potential
/// key but different parents, just as in Java's implication arenas.
pub(crate) struct ProofArena {
    nodes: Box<[ProofNode]>,
    parents: Box<[u32]>,
}

impl ProofArena {
    pub(crate) fn new(nodes: Vec<ProofNode>, parents: Vec<u32>) -> Self {
        Self {
            nodes: nodes.into_boxed_slice(),
            parents: parents.into_boxed_slice(),
        }
    }

    pub(crate) fn node(&self, node: u32) -> &ProofNode {
        &self.nodes[usize::try_from(node).expect("proof node index")]
    }

    fn parents(&self, node: u32) -> &[u32] {
        let entry = self.node(node);
        let start = usize::try_from(entry.parent_start).expect("proof parent start");
        &self.parents[start..start + usize::from(entry.parent_count)]
    }
}

#[derive(Clone)]
pub(crate) struct ProofTarget {
    pub(crate) arena: Arc<ProofArena>,
    pub(crate) node: u32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct FingerprintTarget {
    target_key: u16,
    breadth_first_keys: Box<[u16]>,
}

/// Exact analogue of Java's `FullChain`: ordered targets followed by each
/// target's breadth-first, parent-ordered potential-key sequence.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct FullChainFingerprint(Box<[FingerprintTarget]>);

#[derive(Clone, Copy)]
enum ParentEvent {
    NakedSingle {
        cell: CellId,
    },
    HiddenRegion {
        type_index: u8,
        cell: CellId,
        digit: Digit,
    },
}

/// Eager, self-contained proof identity. Completed inner hints must not retain
/// their producer's whole branch arenas: recursive L4 can produce thousands
/// of ranked hints per closure. Java semantics need only FullChain identity,
/// recursive complexity, and the ordered cause events replayed for outer
/// parent extraction.
pub(crate) struct ChainProof {
    fingerprint: FullChainFingerprint,
    complexity: u32,
    parent_events: Box<[ParentEvent]>,
}

impl ChainProof {
    pub(crate) fn new(kind: ProofKind, targets: Vec<ProofTarget>) -> Self {
        let complexity_target_count = targets.len();
        Self::with_complexity_target_count(kind, targets, complexity_target_count)
    }

    /// Most chaining hints use every ordered target for both identity and
    /// complexity. Java's CycleHint is the exception: its FullChain and rule
    /// parents contain the forward and reversed targets, while its flat and
    /// nested complexity inspect only the forward target.
    pub(crate) fn with_complexity_target_count(
        kind: ProofKind,
        targets: Vec<ProofTarget>,
        complexity_target_count: usize,
    ) -> Self {
        debug_assert!(!targets.is_empty());
        debug_assert!(complexity_target_count <= targets.len());
        let fingerprint = full_chain_fingerprint(&targets);

        let complexity_targets = &targets[..complexity_target_count];
        let mut complexity = 0_u32;
        for target in complexity_targets {
            complexity = complexity
                .checked_add(distinct_ancestor_count(&target.arena, target.node))
                .expect("nested flat complexity");
        }
        let mut processed = HashSet::new();
        for target in complexity_targets {
            collect_nested_complexity(&target.arena, target.node, &mut processed, &mut complexity);
        }

        let mut parent_events = Vec::new();
        for target in &targets {
            collect_parent_events(kind, target, &mut parent_events);
        }
        Self {
            fingerprint,
            complexity,
            parent_events: parent_events.into_boxed_slice(),
        }
    }

    pub(crate) fn fingerprint(&self) -> &FullChainFingerprint {
        &self.fingerprint
    }

    /// Java sums a flat, per-target distinct-key ancestor count and then one
    /// recursively complete complexity for every distinct directly nested
    /// FullChain reachable from the selected targets.
    pub(crate) fn complexity(&self) -> u32 {
        self.complexity
    }

    /// Collect the dynamically removed candidates on which this complete
    /// inner hint depends. Target traversals have separate visited sets and
    /// feed one insertion-ordered, key-deduplicated parent list.
    pub(crate) fn outer_parent_keys(
        &self,
        grid: &Grid,
        mut original_mask: impl FnMut(CellId) -> CandidateMask,
    ) -> Vec<u16> {
        let mut result = Vec::new();
        let mut emitted = [false; KEY_COUNT];
        for event in &self.parent_events {
            match *event {
                ParentEvent::NakedSingle { cell } => {
                    let removed = original_mask(cell).without(grid.candidates(cell));
                    for parent_digit in removed.iter() {
                        append_parent_key(&mut result, &mut emitted, cell, parent_digit);
                    }
                }
                ParentEvent::HiddenRegion {
                    type_index,
                    cell,
                    digit,
                } => {
                    if let Some(region_index) = grid
                        .topology()
                        .cell_region_index(cell, usize::from(type_index))
                    {
                        let region = sukaku_forge_core::RegionId::new(type_index, region_index)
                            .expect("proof hidden region");
                        for &raw_cell in grid.topology().region_cells(region) {
                            let parent_cell =
                                CellId::new(raw_cell).expect("proof hidden parent cell");
                            if original_mask(parent_cell).contains(digit)
                                && !grid.candidates(parent_cell).contains(digit)
                            {
                                append_parent_key(&mut result, &mut emitted, parent_cell, digit);
                            }
                        }
                    }
                }
            }
        }
        result
    }
}

fn full_chain_fingerprint(targets: &[ProofTarget]) -> FullChainFingerprint {
    let mut result = Vec::with_capacity(targets.len());
    for target in targets {
        let arena = &target.arena;
        let mut seen = [false; KEY_COUNT];
        let mut pending = VecDeque::new();
        let mut keys = Vec::new();
        pending.push_back(target.node);
        while let Some(node) = pending.pop_front() {
            let key = arena.node(node).key;
            if seen[usize::from(key)] {
                continue;
            }
            seen[usize::from(key)] = true;
            keys.push(key);
            pending.extend(arena.parents(node));
        }
        result.push(FingerprintTarget {
            target_key: arena.node(target.node).key,
            breadth_first_keys: keys.into_boxed_slice(),
        });
    }
    FullChainFingerprint(result.into_boxed_slice())
}

fn collect_parent_events(kind: ProofKind, target: &ProofTarget, result: &mut Vec<ParentEvent>) {
    let arena = &target.arena;
    let mut seen = [false; KEY_COUNT];
    let mut pending = VecDeque::new();
    pending.push_back(target.node);
    while let Some(node) = pending.pop_front() {
        let proof_node = arena.node(node);
        if seen[usize::from(proof_node.key)] {
            continue;
        }
        seen[usize::from(proof_node.key)] = true;
        if proof_node.key & 1 != 0 {
            let (cell, digit) = decode_candidate(proof_node.key);
            let cause = if proof_node.on_cause == OnCause::None && arena.parents(node).is_empty() {
                match kind {
                    ProofKind::Cell => OnCause::NakedSingle,
                    ProofKind::Region(type_index) => OnCause::HiddenRegion(type_index),
                    ProofKind::Other => OnCause::None,
                }
            } else {
                proof_node.on_cause
            };
            match cause {
                OnCause::None => {}
                OnCause::NakedSingle => result.push(ParentEvent::NakedSingle { cell }),
                OnCause::HiddenRegion(type_index) => result.push(ParentEvent::HiddenRegion {
                    type_index,
                    cell,
                    digit,
                }),
            }
        }
        pending.extend(arena.parents(node));
    }
}

fn append_parent_key(
    result: &mut Vec<u16>,
    emitted: &mut [bool; KEY_COUNT],
    cell: CellId,
    digit: Digit,
) {
    let key = potential_key(cell, digit, false);
    if !emitted[usize::from(key)] {
        emitted[usize::from(key)] = true;
        result.push(key);
    }
}

fn distinct_ancestor_count(arena: &ProofArena, target: u32) -> u32 {
    let mut seen = [false; KEY_COUNT];
    let mut pending = vec![target];
    let mut result = 0_u32;
    while let Some(node) = pending.pop() {
        let key = arena.node(node).key;
        if seen[usize::from(key)] {
            continue;
        }
        seen[usize::from(key)] = true;
        result = result.checked_add(1).expect("proof ancestor count");
        pending.extend(arena.parents(node));
    }
    result
}

fn collect_nested_complexity(
    arena: &ProofArena,
    target: u32,
    processed: &mut HashSet<FullChainFingerprint>,
    complexity: &mut u32,
) {
    let mut seen = [false; KEY_COUNT];
    let mut pending = vec![target];
    while let Some(node) = pending.pop() {
        let proof_node = arena.node(node);
        if seen[usize::from(proof_node.key)] {
            continue;
        }
        seen[usize::from(proof_node.key)] = true;
        if let Some(nested) = &proof_node.nested {
            let fingerprint = nested.fingerprint().clone();
            if processed.insert(fingerprint) {
                *complexity = complexity
                    .checked_add(nested.complexity())
                    .expect("nested proof complexity");
            }
        }
        // Java pushes parents in stored order and pops the last one first.
        pending.extend(arena.parents(node));
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct EffectKey(Box<[u16; 81]>);

impl EffectKey {
    pub(crate) fn new(removals: &CandidateRemovals) -> Self {
        let mut masks = Box::new([0_u16; 81]);
        for entry in removals.iter() {
            masks[entry.cell().index()] = entry.digits().bits();
        }
        Self(masks)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{
        CandidateMask, CandidateRemovalsBuilder, CellId, ConstraintTopology, Digit, Grid, Puzzle,
        VariantConfig,
    };

    use super::{
        ChainProof, FullChainFingerprint, InferenceCollector, OnCause, ProofArena, ProofKind,
        ProofNode, ProofTarget, chaining_effect_key,
    };
    use crate::forcing_chains::potential_key;
    use crate::{Evidence, Inference, Rating, Technique};

    fn arena(nodes: &[(u16, &[u32])]) -> Arc<ProofArena> {
        let mut proof_nodes = Vec::new();
        let mut parents = Vec::new();
        for &(key, node_parents) in nodes {
            proof_nodes.push(ProofNode {
                key,
                parent_start: u32::try_from(parents.len()).unwrap(),
                parent_count: u16::try_from(node_parents.len()).unwrap(),
                on_cause: OnCause::None,
                nested: None,
            });
            parents.extend_from_slice(node_parents);
        }
        Arc::new(ProofArena::new(proof_nodes, parents))
    }

    #[test]
    fn full_chain_identity_uses_parent_ordered_breadth_first_keys() {
        let first = arena(&[(1, &[]), (2, &[]), (3, &[0, 1])]);
        let reversed = arena(&[(1, &[]), (2, &[]), (3, &[1, 0])]);
        let proof = ChainProof::new(
            ProofKind::Other,
            vec![ProofTarget {
                arena: first,
                node: 2,
            }],
        );
        let other = ChainProof::new(
            ProofKind::Other,
            vec![ProofTarget {
                arena: reversed,
                node: 2,
            }],
        );
        let _: &FullChainFingerprint = proof.fingerprint();
        assert_ne!(proof.fingerprint(), other.fingerprint());
        assert_eq!(proof.complexity(), 3);
    }

    #[test]
    fn duplicate_logical_nodes_are_retained_but_counted_once_per_target() {
        let graph = arena(&[(1, &[]), (1, &[]), (3, &[0, 1])]);
        let proof = ChainProof::new(
            ProofKind::Other,
            vec![ProofTarget {
                arena: graph,
                node: 2,
            }],
        );
        assert_eq!(proof.complexity(), 2);
    }

    #[test]
    fn public_collector_deduplicates_equivalent_placement_and_elimination_effects() {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut slots = vec![".........".to_owned(); 81];
        slots[0] = "12.......".to_owned();
        let grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &values,
            &Puzzle::parse(&slots.concat()).unwrap(),
        )
        .unwrap();
        let cell = CellId::new(0).unwrap();
        let one = Digit::new(1).unwrap();
        let two = Digit::new(2).unwrap();
        let placement = Inference::placement(
            Technique::NakedSingle,
            Rating::from_tenths(80),
            cell,
            one,
            Evidence::NakedSingle,
        );
        let mut removals = CandidateRemovalsBuilder::with_capacity(1);
        removals.add(cell, CandidateMask::of(two));
        let elimination = Inference::elimination(
            Technique::NakedSingle,
            Rating::from_tenths(70),
            removals.build(),
            Evidence::NakedSingle,
        );
        assert_eq!(
            chaining_effect_key(&grid, &placement),
            chaining_effect_key(&grid, &elimination)
        );

        let mut collector = InferenceCollector::new();
        collector.offer(&grid, placement.clone(), 8.0, 8, 0);
        collector.offer(&grid, elimination.clone(), 7.0, 7, 0);
        assert_eq!(collector.finish(), vec![elimination.clone()]);

        let mut placement_wins = InferenceCollector::new();
        placement_wins.offer(&grid, elimination, 8.0, 8, 0);
        placement_wins.offer(&grid, placement.clone(), 7.0, 7, 0);
        let retained = placement_wins.finish();
        assert_eq!(retained, vec![placement]);
        assert!(retained[0].is_placement());
    }

    #[test]
    fn cycle_identity_keeps_reverse_target_but_complexity_uses_forward_only() {
        let forward = arena(&[(1, &[]), (2, &[0]), (1, &[1])]);
        let reversed = arena(&[(0, &[1]), (3, &[2]), (0, &[])]);
        let proof = ChainProof::with_complexity_target_count(
            ProofKind::Other,
            vec![
                ProofTarget {
                    arena: Arc::clone(&forward),
                    node: 2,
                },
                ProofTarget {
                    arena: reversed,
                    node: 0,
                },
            ],
            1,
        );
        let forward_only = ChainProof::new(
            ProofKind::Other,
            vec![ProofTarget {
                arena: forward,
                node: 2,
            }],
        );

        assert_eq!(proof.complexity(), 2);
        assert_eq!(proof.complexity(), forward_only.complexity());
        assert_ne!(proof.fingerprint(), forward_only.fingerprint());
    }

    #[test]
    fn compact_proof_drops_arena_and_replays_ordered_parent_events() {
        let cell = CellId::new(0).unwrap();
        let one = Digit::new(1).unwrap();
        let two = Digit::new(2).unwrap();
        let graph = arena(&[(potential_key(cell, one, true), &[])]);
        let weak = Arc::downgrade(&graph);
        let proof = ChainProof::new(
            ProofKind::Cell,
            vec![ProofTarget {
                arena: graph,
                node: 0,
            }],
        );
        assert!(weak.upgrade().is_none());

        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut slots = vec![".........".to_owned(); 81];
        slots[0] = "1........".to_owned();
        let candidates = Puzzle::parse(&slots.concat()).unwrap();
        let grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &values,
            &candidates,
        )
        .unwrap();
        let parents = proof.outer_parent_keys(&grid, |candidate_cell| {
            if candidate_cell == cell {
                CandidateMask::of(one).union(CandidateMask::of(two))
            } else {
                grid.candidates(candidate_cell)
            }
        });
        assert_eq!(parents, vec![potential_key(cell, two, false)]);
    }
}
