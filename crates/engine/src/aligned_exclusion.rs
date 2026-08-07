use sukaku_forge_core::{
    CandidateMask, CandidateRemovalsBuilder, CellId, CellMask, ConstraintTopology, Digit, Grid,
    se121_classic_peers,
};

use crate::{
    AlignedPairCombinationSequence, AlignedTripletCombinationSequence, Evidence, Inference, Rating,
    Technique,
};

const UNCOMPUTED: i8 = i8::MIN;
const ALLOWED: i8 = -2;
const DUPLICATE_VALUE: i8 = -1;

/// Find the first Java-ordered Aligned Pair Exclusion.
///
/// Candidate bases and their pairs retain raw-cell order. Common excluders are
/// filtered from the first base cell's ordered peer catalog, which is exactly
/// the observable order of Java's retained excluder list without maintaining
/// its 81-by-81 byte workspace.
#[must_use]
pub fn find_aligned_pair_exclusion(grid: &Grid) -> Option<Inference> {
    find_aligned_pair_exclusion_with_order(grid, false)
}

/// Find an Aligned Pair Exclusion in SE 1.2.1 peer insertion order.
#[must_use]
pub(crate) fn find_aligned_pair_exclusion_se121(grid: &Grid) -> Option<Inference> {
    find_aligned_pair_exclusion_with_order(grid, true)
}

fn find_aligned_pair_exclusion_with_order(grid: &Grid, se121_order: bool) -> Option<Inference> {
    let mut first = None;
    visit_aligned_pair_exclusions(grid, se121_order, &mut |inference| {
        first = Some(inference);
        false
    });
    first
}

/// Collect every Java-ordered Aligned Pair Exclusion inference.
#[must_use]
pub fn collect_aligned_pair_exclusions(grid: &Grid) -> Vec<Inference> {
    let mut keys = Vec::new();
    let mut inferences = Vec::new();
    visit_aligned_pair_exclusions(grid, false, &mut |inference| {
        let key = aligned_exclusion_equality_key(&inference);
        if !keys.contains(&key) {
            keys.push(key);
            inferences.push(inference);
        }
        true
    });
    inferences
}

fn visit_aligned_pair_exclusions(
    grid: &Grid,
    se121_order: bool,
    emit: &mut dyn FnMut(Inference) -> bool,
) {
    let mut naked_singles = CellMask::EMPTY;
    let mut bivalue_cells = CellMask::EMPTY;
    for raw in 0_u8..CellId::COUNT as u8 {
        let current = cell(raw);
        match grid.candidates(current).count() {
            1 => naked_singles.insert(current),
            2 => bivalue_cells.insert(current),
            _ => {}
        }
    }

    let mut candidate_cells = [0_u8; CellId::COUNT];
    let mut candidate_count = 0_usize;
    for raw in 0_u8..CellId::COUNT as u8 {
        let base_cell = cell(raw);
        if grid.candidates(base_cell).count() < 2 {
            continue;
        }
        let visible = grid.topology().visible_mask(base_cell);
        if visible.intersect(naked_singles).is_empty()
            && !visible.intersect(bivalue_cells).is_empty()
        {
            candidate_cells[candidate_count] = raw;
            candidate_count += 1;
        }
    }

    for second in 1..candidate_count {
        for first in 0..second {
            let bases = [cell(candidate_cells[first]), cell(candidate_cells[second])];
            if let Some(inference) = evaluate_pair(grid, bases, bivalue_cells, se121_order) {
                if !emit(inference) {
                    return;
                }
            }
        }
    }
}

/// Find the first Java-ordered Aligned Triplet Exclusion.
///
/// Java stores an ordered excluder list for every candidate base cell. The
/// same order is reconstructed here from the topology's peer catalog while
/// compact `CellMask`s make twin-area and common-excluder intersections cheap.
/// This avoids both the Java workspace's 81-by-81 byte table and allocation on
/// unsuccessful base sets without changing any observable traversal order.
#[must_use]
pub fn find_aligned_triplet_exclusion(grid: &Grid) -> Option<Inference> {
    find_aligned_triplet_exclusion_with_order(grid, false)
}

/// Find an Aligned Triplet Exclusion in SE 1.2.1 peer insertion order.
#[must_use]
pub(crate) fn find_aligned_triplet_exclusion_se121(grid: &Grid) -> Option<Inference> {
    find_aligned_triplet_exclusion_with_order(grid, true)
}

fn find_aligned_triplet_exclusion_with_order(grid: &Grid, se121_order: bool) -> Option<Inference> {
    let mut first = None;
    visit_aligned_triplet_exclusions(grid, se121_order, &mut |inference| {
        first = Some(inference);
        false
    });
    first
}

/// Collect every Java-ordered Aligned Triplet Exclusion inference.
#[must_use]
pub fn collect_aligned_triplet_exclusions(grid: &Grid) -> Vec<Inference> {
    let mut keys = Vec::new();
    let mut inferences = Vec::new();
    visit_aligned_triplet_exclusions(grid, false, &mut |inference| {
        let key = aligned_exclusion_equality_key(&inference);
        if !keys.contains(&key) {
            keys.push(key);
            inferences.push(inference);
        }
        true
    });
    inferences
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AlignedExclusionEqualityKey {
    // AlignedExclusionHint compares an unordered base-cell set and Java Map
    // equality for removable potentials; the dense array is order-neutral.
    base_cells: CellMask,
    removal_masks: [u16; CellId::COUNT],
}

fn aligned_exclusion_equality_key(inference: &Inference) -> AlignedExclusionEqualityKey {
    let mut base_cells = CellMask::EMPTY;
    match inference.evidence() {
        Evidence::AlignedPairExclusion { cells, .. } => {
            for cell in cells {
                base_cells.insert(cell);
            }
        }
        Evidence::AlignedTripletExclusion { cells, .. } => {
            for cell in cells {
                base_cells.insert(cell);
            }
        }
        _ => unreachable!("aligned-exclusion equality key evidence"),
    }
    let mut removal_masks = [0_u16; CellId::COUNT];
    for removal in inference.removals().iter() {
        removal_masks[usize::from(removal.cell().raw())] = removal.digits().bits();
    }
    AlignedExclusionEqualityKey {
        base_cells,
        removal_masks,
    }
}

fn visit_aligned_triplet_exclusions(
    grid: &Grid,
    se121_order: bool,
    emit: &mut dyn FnMut(Inference) -> bool,
) {
    let topology = grid.topology();
    let mut excluder_masks = [CellMask::EMPTY; CellId::COUNT];
    let mut candidate_bases = CellMask::EMPTY;
    let mut candidate_cells = [0_u8; CellId::COUNT];
    let mut candidate_count = 0_usize;

    for raw in 0_u8..CellId::COUNT as u8 {
        let base = cell(raw);
        if grid.candidates(base).count() < 2 {
            continue;
        }

        let mut has_naked_single = false;
        for &raw_peer in ordered_peers(topology, base, se121_order) {
            let peer = cell(raw_peer);
            match grid.candidates(peer).count() {
                1 => has_naked_single = true,
                2 | 3 => excluder_masks[usize::from(raw)].insert(peer),
                _ => {}
            }
        }
        if !has_naked_single && !excluder_masks[usize::from(raw)].is_empty() {
            candidate_bases.insert(base);
            candidate_cells[candidate_count] = raw;
            candidate_count += 1;
        }
    }

    if candidate_count < 3 {
        return;
    }

    // Java's Twomutations order: (0,1), (0,2), (1,2), (0,3), ...
    for second in 1..candidate_count {
        for first in 0..second {
            let first_base = cell(candidate_cells[first]);
            let second_base = cell(candidate_cells[second]);
            let mut twin_cells = [0_u8; CellId::COUNT];
            let mut twin_count = 0_usize;
            let mut seen = CellMask::EMPTY;

            for source in [first_base, second_base] {
                let source_excluders = excluder_masks[usize::from(source.raw())];
                for &raw_peer in ordered_peers(topology, source, se121_order) {
                    let peer = cell(raw_peer);
                    if peer != first_base
                        && peer != second_base
                        && source_excluders.contains(peer)
                        && candidate_bases.contains(peer)
                        && !seen.contains(peer)
                    {
                        seen.insert(peer);
                        twin_cells[twin_count] = raw_peer;
                        twin_count += 1;
                    }
                }
            }

            // Degree three selects one tail, so LinkedHashSet order is the
            // complete Java tail-combination order.
            for &raw_tail in &twin_cells[..twin_count] {
                let bases = [first_base, second_base, cell(raw_tail)];
                if let Some(inference) = evaluate_triplet(grid, bases, &excluder_masks, se121_order)
                {
                    if !emit(inference) {
                        return;
                    }
                }
            }
        }
    }
}

fn evaluate_triplet(
    grid: &Grid,
    bases: [CellId; 3],
    excluder_masks: &[CellMask; CellId::COUNT],
    se121_order: bool,
) -> Option<Inference> {
    let topology = grid.topology();
    let common_mask = excluder_masks[usize::from(bases[0].raw())]
        .intersect(excluder_masks[usize::from(bases[1].raw())])
        .intersect(excluder_masks[usize::from(bases[2].raw())]);
    let mut common_excluders = [0_u8; CellId::COUNT];
    let mut common_count = 0_usize;
    for &raw_peer in ordered_peers(topology, bases[0], se121_order) {
        if common_mask.contains(cell(raw_peer)) {
            common_excluders[common_count] = raw_peer;
            common_count += 1;
        }
    }
    if common_count < 2 {
        return None;
    }

    let values = bases.map(|base| grid.candidates(base));
    let mut visible_pairs = 0_u8;
    if topology.visible_mask(bases[0]).contains(bases[1]) {
        visible_pairs |= 0b001;
    }
    if topology.visible_mask(bases[0]).contains(bases[2]) {
        visible_pairs |= 0b010;
    }
    if topology.visible_mask(bases[1]).contains(bases[2]) {
        visible_pairs |= 0b100;
    }

    let mut locking_cache = [UNCOMPUTED; 1 << 10];
    let mut allowed = [CandidateMask::EMPTY; 3];
    let mut remaining = values.iter().map(|mask| mask.count()).sum::<u32>();

    // For degree >= 3 Java's mixed-radix odometer starts at the highest
    // digit in every cell and decrements position zero fastest.
    for third_raw in (1_u8..=9).rev() {
        let third = Digit::new(third_raw).expect("candidate digit");
        if !values[2].contains(third) {
            continue;
        }
        for second_raw in (1_u8..=9).rev() {
            let second = Digit::new(second_raw).expect("candidate digit");
            if !values[1].contains(second) {
                continue;
            }
            for first_raw in (1_u8..=9).rev() {
                let first = Digit::new(first_raw).expect("candidate digit");
                if !values[0].contains(first) {
                    continue;
                }
                let digits = [first, second, third];
                if triplet_locking_code(
                    grid,
                    digits,
                    visible_pairs,
                    &common_excluders[..common_count],
                    &mut locking_cache,
                ) == ALLOWED
                {
                    for position in 0..3 {
                        if !allowed[position].contains(digits[position]) {
                            allowed[position].insert(digits[position]);
                            remaining -= 1;
                        }
                    }
                    if remaining == 0 {
                        return None;
                    }
                }
            }
        }
    }

    let removal_masks = [
        values[0].without(allowed[0]),
        values[1].without(allowed[1]),
        values[2].without(allowed[2]),
    ];
    if removal_masks.iter().all(|mask| mask.is_empty()) {
        return None;
    }
    let mut removals = CandidateRemovalsBuilder::with_capacity(3);
    for position in 0..3 {
        removals.add(bases[position], removal_masks[position]);
    }

    let mut locked_combinations = AlignedTripletCombinationSequence::new(values, visible_pairs);
    for &raw_excluder in &common_excluders[..common_count] {
        let excluder = cell(raw_excluder);
        locked_combinations.push_excluder(excluder, grid.candidates(excluder));
    }

    Some(Inference::elimination(
        Technique::AlignedTripletExclusion,
        Rating::from_tenths(75),
        removals.build(),
        Evidence::AlignedTripletExclusion {
            cells: bases,
            locked_combinations,
        },
    ))
}

fn triplet_locking_code(
    grid: &Grid,
    digits: [Digit; 3],
    visible_pairs: u8,
    common_excluders: &[u8],
    cache: &mut [i8; 1 << 10],
) -> i8 {
    if (visible_pairs & 0b001 != 0 && digits[0] == digits[1])
        || (visible_pairs & 0b010 != 0 && digits[0] == digits[2])
        || (visible_pairs & 0b100 != 0 && digits[1] == digits[2])
    {
        return DUPLICATE_VALUE;
    }

    let selected = CandidateMask::of(digits[0])
        .union(CandidateMask::of(digits[1]))
        .union(CandidateMask::of(digits[2]));
    let index = usize::from(selected.bits());
    if cache[index] != UNCOMPUTED {
        return cache[index];
    }
    let result = common_excluders
        .iter()
        .copied()
        .find(|raw| grid.candidates(cell(*raw)).without(selected).is_empty())
        .map_or(ALLOWED, |raw| raw as i8);
    cache[index] = result;
    result
}

fn evaluate_pair(
    grid: &Grid,
    bases: [CellId; 2],
    bivalue_cells: CellMask,
    se121_order: bool,
) -> Option<Inference> {
    let mut common_excluders = [0_u8; CellId::COUNT];
    let mut common_count = 0_usize;
    let common_mask = grid
        .topology()
        .visible_mask(bases[0])
        .intersect(grid.topology().visible_mask(bases[1]))
        .intersect(bivalue_cells);
    for &raw_peer in ordered_peers(grid.topology(), bases[0], se121_order) {
        let peer = cell(raw_peer);
        if common_mask.contains(peer) {
            common_excluders[common_count] = raw_peer;
            common_count += 1;
        }
    }
    if common_count < 2 {
        return None;
    }

    let values = [grid.candidates(bases[0]), grid.candidates(bases[1])];
    let bases_are_visible = grid.topology().visible_mask(bases[0]).contains(bases[1]);
    let mut locking_cache = [UNCOMPUTED; 1 << 10];
    let mut allowed = [CandidateMask::EMPTY; 2];
    let mut remaining = values[0].count() + values[1].count();

    'combinations: for first_digit in values[0].iter() {
        for second_digit in values[1].iter() {
            let code = locking_code(
                grid,
                [first_digit, second_digit],
                bases_are_visible,
                &common_excluders[..common_count],
                &mut locking_cache,
            );
            if code == ALLOWED {
                for (position, digit) in [first_digit, second_digit].into_iter().enumerate() {
                    if !allowed[position].contains(digit) {
                        allowed[position].insert(digit);
                        remaining -= 1;
                    }
                }
                if remaining == 0 {
                    break 'combinations;
                }
            }
        }
    }
    if remaining == 0 {
        return None;
    }

    let removal_masks = [values[0].without(allowed[0]), values[1].without(allowed[1])];
    if removal_masks[0].is_empty() && removal_masks[1].is_empty() {
        return None;
    }
    let mut removal_builder = CandidateRemovalsBuilder::with_capacity(2);
    removal_builder.add(bases[0], removal_masks[0]);
    removal_builder.add(bases[1], removal_masks[1]);

    let mut locked_combinations = AlignedPairCombinationSequence::new();
    for first_digit in values[0].iter() {
        for second_digit in values[1].iter() {
            let code = locking_code(
                grid,
                [first_digit, second_digit],
                bases_are_visible,
                &common_excluders[..common_count],
                &mut locking_cache,
            );
            match code {
                ALLOWED => {}
                DUPLICATE_VALUE => {
                    locked_combinations.push(first_digit, second_digit, None);
                }
                raw if raw >= 0 => {
                    locked_combinations.push(first_digit, second_digit, Some(cell(raw as u8)))
                }
                _ => unreachable!("locking cache code"),
            }
        }
    }

    Some(Inference::elimination(
        Technique::AlignedPairExclusion,
        Rating::from_tenths(62),
        removal_builder.build(),
        Evidence::AlignedPairExclusion {
            cells: bases,
            locked_combinations,
        },
    ))
}

fn locking_code(
    grid: &Grid,
    digits: [Digit; 2],
    bases_are_visible: bool,
    common_excluders: &[u8],
    cache: &mut [i8; 1 << 10],
) -> i8 {
    if bases_are_visible && digits[0] == digits[1] {
        return DUPLICATE_VALUE;
    }
    let selected = CandidateMask::of(digits[0]).union(CandidateMask::of(digits[1]));
    let index = usize::from(selected.bits());
    if cache[index] != UNCOMPUTED {
        return cache[index];
    }
    let result = common_excluders
        .iter()
        .copied()
        .find(|raw| grid.candidates(cell(*raw)).without(selected).is_empty())
        .map_or(ALLOWED, |raw| raw as i8);
    cache[index] = result;
    result
}

fn cell(raw: u8) -> CellId {
    CellId::new(raw).expect("cell index")
}

fn ordered_peers(topology: &ConstraintTopology, cell: CellId, se121_order: bool) -> &[u8] {
    if se121_order {
        se121_classic_peers(cell)
    } else {
        topology.visible_peers(cell)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{
        CandidateMask, CellId, ConstraintTopology, Grid, Puzzle, VariantConfig,
    };

    use super::{
        cell, collect_aligned_pair_exclusions, collect_aligned_triplet_exclusions,
        find_aligned_pair_exclusion, find_aligned_triplet_exclusion,
    };
    use crate::{Evidence, Rating, Technique};

    fn sparse_snapshot(entries: &[(usize, &str)], variant: VariantConfig) -> Grid {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = ['.'; 729];
        for &(cell, candidates) in entries {
            for digit in candidates.bytes() {
                display[cell * 9 + usize::from(digit - b'1')] = char::from(digit);
            }
        }
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(variant)),
            &values,
            &candidates,
        )
        .unwrap()
    }

    #[test]
    fn primitive_pair_fixture_matches_java_order_and_effect() {
        let mut grid = sparse_snapshot(
            &[(0, "12"), (10, "34"), (1, "13"), (9, "14")],
            VariantConfig::default(),
        );
        let inference = find_aligned_pair_exclusion(&grid).expect("Java APE fixture");
        assert_eq!(
            Some(inference.clone()),
            collect_aligned_pair_exclusions(&grid).first().cloned()
        );
        assert_eq!(inference.technique(), Technique::AlignedPairExclusion);
        assert_eq!(inference.rating(), Rating::from_tenths(62));
        assert_eq!(inference.short_name(), "APE");
        assert_eq!(
            inference.description(grid.topology()),
            "Aligned Pair Exclusion: r1c1,r2c2"
        );
        let Evidence::AlignedPairExclusion {
            cells,
            locked_combinations,
        } = inference.evidence()
        else {
            panic!("APE evidence");
        };
        assert_eq!(cells.map(CellId::raw), [0, 10]);
        assert_eq!(
            locked_combinations
                .iter()
                .map(|(first, second, locking)| {
                    (first.get(), second.get(), locking.map(CellId::raw))
                })
                .collect::<Vec<_>>(),
            [(1, 3, Some(1)), (1, 4, Some(9))]
        );
        inference.apply(&mut grid);
        assert_eq!(
            grid.candidates(cell(0)).bits(),
            CandidateMask::from_bits(1 << 2).bits()
        );
    }

    #[test]
    fn visible_naked_single_suppresses_a_candidate_base() {
        let grid = sparse_snapshot(
            &[(0, "12"), (10, "34"), (1, "13"), (9, "14"), (2, "9")],
            VariantConfig::default(),
        );
        assert!(find_aligned_pair_exclusion(&grid).is_none());

        let distant_single = sparse_snapshot(
            &[(0, "12"), (10, "34"), (1, "13"), (9, "14"), (80, "9")],
            VariantConfig::default(),
        );
        assert!(find_aligned_pair_exclusion(&distant_single).is_some());
    }

    #[test]
    fn anti_knight_visibility_can_form_the_aligned_pair() {
        let mut grid = sparse_snapshot(
            &[(30, "12"), (32, "34"), (13, "13"), (49, "14")],
            VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            },
        );
        let inference = find_aligned_pair_exclusion(&grid).expect("anti-knight APE fixture");
        assert_eq!(
            inference.description(grid.topology()),
            "Aligned Pair Exclusion: r4c4,r4c6"
        );
        inference.apply(&mut grid);
        assert_eq!(grid.candidates(cell(30)), CandidateMask::from_bits(1 << 2));

        let classic = sparse_snapshot(
            &[(30, "12"), (32, "34"), (13, "13"), (49, "14")],
            VariantConfig::default(),
        );
        assert!(find_aligned_pair_exclusion(&classic).is_none());
    }

    #[test]
    fn common_excluders_retain_the_first_bases_topology_order() {
        let mut grid = sparse_snapshot(
            &[(30, "12"), (32, "13"), (13, "13"), (49, "13")],
            VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            },
        );
        let inference = find_aligned_pair_exclusion(&grid).expect("ordered AK APE fixture");
        assert_eq!(
            inference.description(grid.topology()),
            "Aligned Pair Exclusion: r2c5,r4c4"
        );
        let Evidence::AlignedPairExclusion {
            locked_combinations,
            ..
        } = inference.evidence()
        else {
            panic!("APE evidence");
        };
        let locking_cell = locked_combinations
            .iter()
            .find_map(|(first, second, locking)| {
                (first.get() == 3 && second.get() == 1).then_some(locking)
            })
            .flatten();
        assert_eq!(locking_cell.map(CellId::raw), Some(49));
        assert!(locked_combinations.iter().any(|(first, second, locking)| {
            first.get() == 1 && second.get() == 1 && locking.is_none()
        }));
        inference.apply(&mut grid);
        assert_eq!(grid.candidates(cell(30)), CandidateMask::from_bits(1 << 2));
    }

    #[test]
    fn base_cardinality_is_uncapped_but_two_common_excluders_are_required() {
        let mut grid = sparse_snapshot(
            &[(0, "125"), (10, "34"), (1, "13"), (9, "14")],
            VariantConfig::default(),
        );
        let inference = find_aligned_pair_exclusion(&grid).expect("three-candidate APE base");
        assert_eq!(
            inference.description(grid.topology()),
            "Aligned Pair Exclusion: r1c1,r2c2"
        );
        inference.apply(&mut grid);
        assert_eq!(
            grid.candidates(cell(0)),
            CandidateMask::from_bits((1 << 2) | (1 << 5))
        );

        let one_common = sparse_snapshot(
            &[(0, "12"), (10, "34"), (1, "13")],
            VariantConfig::default(),
        );
        assert!(find_aligned_pair_exclusion(&one_common).is_none());
    }

    #[test]
    fn classic_oracle_initial_state_matches_the_java_direct_producer() {
        let puzzle = Puzzle::parse(
            "100000002520070049009000500000689000000703000090105030640010025010000070900000008",
        )
        .unwrap();
        let mut grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &puzzle,
        );
        let before = grid.candidates(cell(43));
        let inference = find_aligned_pair_exclusion(&grid).expect("Java oracle APE state");
        assert_eq!(
            inference.description(grid.topology()),
            "Aligned Pair Exclusion: r3c8,r5c8"
        );
        inference.apply(&mut grid);
        let six = CandidateMask::from_bits(1 << 6);
        assert_eq!(grid.candidates(cell(43)), before.without(six));
        assert_eq!(inference.removals().elimination_count(), 1);
    }

    #[test]
    fn aligned_triplet_fixture_preserves_descending_java_proof_rows() {
        let mut grid = sparse_snapshot(
            &[(0, "12"), (1, "13"), (2, "14"), (3, "123"), (4, "124")],
            VariantConfig::default(),
        );
        let inference = find_aligned_triplet_exclusion(&grid).expect("Java ATE fixture");
        let mut raw = Vec::new();
        super::visit_aligned_triplet_exclusions(&grid, false, &mut |inference| {
            raw.push(inference);
            true
        });
        let retained = collect_aligned_triplet_exclusions(&grid);
        assert!(
            raw.len() > retained.len(),
            "alternate Java base-pair traversals must collapse"
        );
        assert_eq!(Some(inference.clone()), retained.first().cloned());
        assert_eq!(inference.technique(), Technique::AlignedTripletExclusion);
        assert_eq!(inference.rating(), Rating::from_tenths(75));
        assert_eq!(inference.name(), "Aligned Triplet Exclusion");
        assert_eq!(inference.short_name(), "ATE");
        assert_eq!(
            inference.description(grid.topology()),
            "Aligned Triplet Exclusion: r1c1,r1c2,r1c3"
        );

        let Evidence::AlignedTripletExclusion {
            cells,
            locked_combinations,
        } = inference.evidence()
        else {
            panic!("ATE evidence");
        };
        assert_eq!(cells.map(CellId::raw), [0, 1, 2]);
        assert_eq!(locked_combinations.common_excluder_count(), 2);
        assert_eq!(
            locked_combinations
                .iter()
                .map(|(digits, locking)| {
                    (digits.map(|digit| digit.get()), locking.map(CellId::raw))
                })
                .collect::<Vec<_>>(),
            [
                ([2, 1, 4], Some(4)),
                ([1, 1, 4], None),
                ([2, 3, 1], Some(3)),
                ([1, 3, 1], None),
                ([2, 1, 1], None),
                ([1, 1, 1], None),
            ]
        );

        inference.apply(&mut grid);
        assert_eq!(grid.candidates(cell(0)), CandidateMask::from_bits(0b110));
        assert_eq!(grid.candidates(cell(1)), CandidateMask::from_bits(1 << 3));
        assert_eq!(grid.candidates(cell(2)), CandidateMask::from_bits(1 << 4));
    }

    #[test]
    fn aligned_triplet_retains_variant_twin_and_excluder_order() {
        let variant = VariantConfig {
            anti_knight: true,
            ..VariantConfig::default()
        };
        let mut grid = sparse_snapshot(
            &[
                (6, "245"),
                (11, "45"),
                (16, "34"),
                (22, "12"),
                (23, "124"),
                (25, "34"),
            ],
            variant,
        );
        let inference = find_aligned_triplet_exclusion(&grid).expect("anti-knight ATE fixture");
        assert_eq!(
            inference.description(grid.topology()),
            "Aligned Triplet Exclusion: r1c7,r3c5,r2c8"
        );
        let Evidence::AlignedTripletExclusion {
            cells,
            locked_combinations,
        } = inference.evidence()
        else {
            panic!("ATE evidence");
        };
        assert_eq!(cells.map(CellId::raw), [6, 22, 16]);
        assert_eq!(
            locked_combinations
                .iter()
                .map(|(digits, locking)| {
                    (digits.map(|digit| digit.get()), locking.map(CellId::raw))
                })
                .collect::<Vec<_>>(),
            [
                ([4, 2, 4], None),
                ([4, 1, 4], None),
                ([2, 1, 4], Some(23)),
                ([4, 2, 3], Some(25)),
                ([4, 1, 3], Some(25)),
            ]
        );
        inference.apply(&mut grid);
        assert_eq!(
            grid.candidates(cell(6)),
            CandidateMask::from_bits((1 << 2) | (1 << 5))
        );

        let classic = sparse_snapshot(
            &[
                (6, "245"),
                (11, "45"),
                (16, "34"),
                (22, "12"),
                (23, "124"),
                (25, "34"),
            ],
            VariantConfig::default(),
        );
        assert!(find_aligned_triplet_exclusion(&classic).is_none());
    }
}
