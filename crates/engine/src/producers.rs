use sukaku_forge_core::{
    CandidateMask, CandidateRemovalsBuilder, CellId, CellMask, Digit, Grid, PositionMask,
    REGION_TYPE_COUNT, RegionId,
};

use crate::{EngineConfig, Evidence, Inference, Rating, RatingMode, Technique};

/// Find the first Java-compatible Hidden Single inference.
#[must_use]
pub fn find_hidden_single(grid: &Grid, config: EngineConfig) -> Option<Inference> {
    first_inference(|emit| visit_hidden_singles(grid, config, emit))
}

/// Collect every Java-compatible Hidden Single inference in discovery order.
#[must_use]
pub fn collect_hidden_singles(grid: &Grid, config: EngineConfig) -> Vec<Inference> {
    collect_unique_inferences(
        |emit| visit_hidden_singles(grid, config, emit),
        direct_hint_equality_key,
    )
}

fn visit_hidden_singles(
    grid: &Grid,
    config: EngineConfig,
    emit: &mut dyn FnMut(Inference) -> bool,
) {
    for alone_only in [true, false] {
        for type_index in hidden_set_family_order(grid, config) {
            for region_index in 0..grid.topology().region_count(type_index) {
                let region = region_id(type_index, region_index);
                let empty_count = grid
                    .topology()
                    .region_cells(region)
                    .iter()
                    .filter(|&&raw_cell| grid.value(cell_id(raw_cell)) == 0)
                    .count();
                let alone = empty_count == 1;
                if alone != alone_only {
                    continue;
                }
                for value in 1_u8..=9 {
                    let digit = digit(value);
                    let Some(position) = grid.region_candidate_positions(region, digit).single()
                    else {
                        continue;
                    };
                    let cell = cell_id(grid.topology().region_cells(region)[usize::from(position)]);
                    let rating = if alone {
                        Rating::from_tenths(10)
                    } else if type_index == 0 {
                        Rating::from_tenths(12)
                    } else {
                        Rating::from_tenths(15)
                    };
                    if !emit(Inference::placement(
                        Technique::HiddenSingle,
                        rating,
                        cell,
                        digit,
                        Evidence::HiddenSingle { region, alone },
                    )) {
                        return;
                    }
                }
            }
        }
    }
}

/// Find the first unresolved cell with exactly one candidate.
#[must_use]
pub fn find_naked_single(grid: &Grid, config: EngineConfig) -> Option<Inference> {
    first_inference(|emit| visit_naked_singles(grid, config, emit))
}

/// Collect every unresolved cell with exactly one candidate, in cell order.
#[must_use]
pub fn collect_naked_singles(grid: &Grid, config: EngineConfig) -> Vec<Inference> {
    collect_unique_inferences(
        |emit| visit_naked_singles(grid, config, emit),
        direct_hint_equality_key,
    )
}

fn visit_naked_singles(grid: &Grid, config: EngineConfig, emit: &mut dyn FnMut(Inference) -> bool) {
    for raw_cell in 0_u8..81 {
        let cell = cell_id(raw_cell);
        if grid.value(cell) != 0 {
            continue;
        }
        let Some(digit) = grid.candidates(cell).single() else {
            continue;
        };
        let rating = match config.rating_mode {
            RatingMode::Original => Rating::from_tenths(23),
            RatingMode::Revised => Rating::from_tenths(16),
        };
        if !emit(Inference::placement(
            Technique::NakedSingle,
            rating,
            cell,
            digit,
            Evidence::NakedSingle,
        )) {
            return;
        }
    }
}

/// Find the first direct Hidden Pair or Triplet.
#[must_use]
pub fn find_direct_hidden_set(grid: &Grid, config: EngineConfig, degree: u8) -> Option<Inference> {
    first_inference(|emit| visit_direct_hidden_sets(grid, config, degree, emit))
}

/// Collect every direct Hidden Pair or Triplet in Java discovery order.
#[must_use]
pub fn collect_direct_hidden_sets(grid: &Grid, config: EngineConfig, degree: u8) -> Vec<Inference> {
    // DirectHiddenSetHint inherits Object identity: Java retains structurally
    // equal discoveries, so this collector deliberately performs no dedup.
    collect_inferences(|emit| visit_direct_hidden_sets(grid, config, degree, emit))
}

fn visit_direct_hidden_sets(
    grid: &Grid,
    config: EngineConfig,
    degree: u8,
    emit: &mut dyn FnMut(Inference) -> bool,
) {
    assert!(matches!(degree, 2 | 3));
    for type_index in hidden_set_family_order(grid, config) {
        for region_index in 0..grid.topology().region_count(type_index) {
            let region = region_id(type_index, region_index);
            let empty_count = grid
                .topology()
                .region_cells(region)
                .iter()
                .filter(|&&raw_cell| grid.value(cell_id(raw_cell)) == 0)
                .count();
            if empty_count <= usize::from(degree) {
                continue;
            }
            for subset in 0_u16..512 {
                if subset.count_ones() != u32::from(degree) {
                    continue;
                }
                let tuple_digits = CandidateMask::from_bits(subset << 1);
                let mut tuple_positions = PositionMask::EMPTY;
                let mut valid = true;
                for digit in tuple_digits.iter() {
                    let positions = grid.region_candidate_positions(region, digit);
                    if positions.count() <= 1 {
                        valid = false;
                        break;
                    }
                    tuple_positions = tuple_positions.union(positions);
                }
                if !valid || tuple_positions.count() != u32::from(degree) {
                    continue;
                }
                for value in 1_u8..=9 {
                    let target_digit = digit(value);
                    if tuple_digits.contains(target_digit) {
                        continue;
                    }
                    let original = grid.region_candidate_positions(region, target_digit);
                    if original.count() <= 1 {
                        continue;
                    }
                    let Some(target_position) = original.without(tuple_positions).single() else {
                        continue;
                    };
                    let target =
                        cell_id(grid.topology().region_cells(region)[usize::from(target_position)]);
                    let (technique, rating) = match (degree, config.rating_mode) {
                        (2, _) => (Technique::DirectHiddenPair, Rating::from_tenths(20)),
                        (3, RatingMode::Original) => {
                            (Technique::DirectHiddenTriplet, Rating::from_tenths(25))
                        }
                        (3, RatingMode::Revised) => {
                            (Technique::DirectHiddenTriplet, Rating::from_tenths(31))
                        }
                        _ => unreachable!("degree is checked above"),
                    };
                    if !emit(Inference::placement(
                        technique,
                        rating,
                        target,
                        target_digit,
                        Evidence::HiddenSet {
                            degree,
                            region,
                            tuple_digits,
                            tuple_positions,
                        },
                    )) {
                        return;
                    }
                }
            }
        }
    }
}

/// Find the first direct Pointing/Claiming placement in Java producer order.
#[must_use]
pub fn find_direct_locking(grid: &Grid) -> Option<Inference> {
    first_inference(|emit| visit_direct_locking(grid, emit))
}

/// Collect every direct Pointing/Claiming placement in Java discovery order.
#[must_use]
pub fn collect_direct_locking(grid: &Grid) -> Vec<Inference> {
    collect_unique_inferences(
        |emit| visit_direct_locking(grid, emit),
        |inference| locking_hint_equality_key(grid, inference),
    )
}

fn visit_direct_locking(grid: &Grid, emit: &mut dyn FnMut(Inference) -> bool) {
    if !grid.topology().config().blocks {
        return;
    }
    for (primary_type, secondary_type) in locking_family_pairs(grid) {
        for value in 1_u8..=9 {
            let candidate = digit(value);
            for primary_index in 0..grid.topology().region_count(primary_type) {
                let primary = region_id(primary_type, primary_index);
                let primary_positions = grid.region_candidate_positions(primary, candidate);
                if primary_positions.count() < 2 {
                    continue;
                }
                for secondary_index in 0..grid.topology().region_count(secondary_type) {
                    let secondary = region_id(secondary_type, secondary_index);
                    let primary_overlap = grid.topology().overlap_positions(primary, secondary);
                    if primary_overlap.is_empty()
                        || !primary_positions.without(primary_overlap).is_empty()
                    {
                        continue;
                    }
                    for following_index in 0..grid.topology().region_count(primary_type) {
                        if following_index == primary_index {
                            continue;
                        }
                        let following = region_id(primary_type, following_index);
                        let following_overlap =
                            grid.topology().overlap_positions(following, secondary);
                        if following_overlap.is_empty() {
                            continue;
                        }
                        let following_positions =
                            grid.region_candidate_positions(following, candidate);
                        if following_positions.count() <= 1 {
                            continue;
                        }
                        let Some(target_position) =
                            following_positions.without(following_overlap).single()
                        else {
                            continue;
                        };
                        let target = cell_id(
                            grid.topology().region_cells(following)[usize::from(target_position)],
                        );
                        let secondary_overlap =
                            grid.topology().overlap_positions(secondary, primary);
                        let pattern_positions = grid
                            .region_candidate_positions(secondary, candidate)
                            .intersect(secondary_overlap);
                        let (technique, rating) = if primary_type == 0 {
                            (Technique::DirectPointing, Rating::from_tenths(17))
                        } else {
                            (Technique::DirectClaiming, Rating::from_tenths(19))
                        };
                        if !emit(Inference::placement(
                            technique,
                            rating,
                            target,
                            candidate,
                            Evidence::DirectLocking {
                                primary,
                                secondary,
                                pattern_positions,
                            },
                        )) {
                            return;
                        }
                    }
                }
            }
        }
    }
}

/// Find the first ordinary Pointing/Claiming elimination.
#[must_use]
pub fn find_locking(grid: &Grid) -> Option<Inference> {
    first_inference(|emit| visit_locking(grid, emit))
}

/// Collect every ordinary Pointing/Claiming elimination in discovery order.
#[must_use]
pub fn collect_locking(grid: &Grid) -> Vec<Inference> {
    collect_unique_inferences(
        |emit| visit_locking(grid, emit),
        |inference| locking_hint_equality_key(grid, inference),
    )
}

fn visit_locking(grid: &Grid, emit: &mut dyn FnMut(Inference) -> bool) {
    if !grid.topology().config().blocks {
        return;
    }
    for (primary_type, secondary_type) in locking_family_pairs(grid) {
        for value in 1_u8..=9 {
            let candidate = digit(value);
            for primary_index in 0..grid.topology().region_count(primary_type) {
                let primary = region_id(primary_type, primary_index);
                let primary_positions = grid.region_candidate_positions(primary, candidate);
                if primary_positions.count() < 2 {
                    continue;
                }
                for secondary_index in 0..grid.topology().region_count(secondary_type) {
                    let secondary = region_id(secondary_type, secondary_index);
                    let primary_overlap = grid.topology().overlap_positions(primary, secondary);
                    if primary_overlap.is_empty()
                        || !primary_positions.without(primary_overlap).is_empty()
                    {
                        continue;
                    }
                    let secondary_overlap = grid.topology().overlap_positions(secondary, primary);
                    let secondary_positions = grid.region_candidate_positions(secondary, candidate);
                    let pattern_positions = secondary_positions.intersect(secondary_overlap);
                    let removable_positions = secondary_positions.without(secondary_overlap);
                    let mut builder = CandidateRemovalsBuilder::with_capacity(
                        removable_positions.count() as usize,
                    );
                    for position in removable_positions.iter() {
                        builder.add(
                            cell_id(grid.topology().region_cells(secondary)[usize::from(position)]),
                            CandidateMask::of(candidate),
                        );
                    }
                    let removals = builder.build();
                    if removals.is_empty() {
                        continue;
                    }
                    let (technique, rating) = if matches!(secondary_type, 1 | 2) {
                        (Technique::Pointing, Rating::from_tenths(26))
                    } else {
                        (Technique::Claiming, Rating::from_tenths(28))
                    };
                    if !emit(Inference::elimination(
                        technique,
                        rating,
                        removals,
                        Evidence::Locking {
                            primary,
                            secondary,
                            digit: candidate,
                            pattern_positions,
                        },
                    )) {
                        return;
                    }
                }
            }
        }
    }
}

/// Find the first Generalized Intersections elimination.
#[must_use]
pub fn find_generalized_intersections(grid: &Grid) -> Option<Inference> {
    first_inference(|emit| visit_generalized_intersections(grid, emit))
}

/// Collect every Generalized Intersections elimination in discovery order.
#[must_use]
pub fn collect_generalized_intersections(grid: &Grid) -> Vec<Inference> {
    // VLockingHint compares its freshly allocated Cell[] by reference, so no
    // two producer discoveries compare equal in released Java.
    collect_inferences(|emit| visit_generalized_intersections(grid, emit))
}

fn visit_generalized_intersections(grid: &Grid, emit: &mut dyn FnMut(Inference) -> bool) {
    for type_index in generalized_intersection_family_order(grid) {
        for value in 1_u8..=9 {
            let candidate = digit(value);
            for region_index in 0..grid.topology().region_count(type_index) {
                let region = region_id(type_index, region_index);
                let locked_positions = grid.region_candidate_positions(region, candidate);
                if !(2..=6).contains(&locked_positions.count()) {
                    continue;
                }
                let mut positions = locked_positions.iter();
                let first_position = positions.next().expect("at least two locked positions");
                let first_cell =
                    cell_id(grid.topology().region_cells(region)[usize::from(first_position)]);
                let mut victims = grid.topology().visible_mask(first_cell);
                for position in positions {
                    let locked_cell =
                        cell_id(grid.topology().region_cells(region)[usize::from(position)]);
                    victims = victims.intersect(grid.topology().visible_mask(locked_cell));
                }
                for position in locked_positions.iter() {
                    victims.remove(cell_id(
                        grid.topology().region_cells(region)[usize::from(position)],
                    ));
                }
                victims = victims.intersect(grid.candidate_cells(candidate));
                let mut builder = CandidateRemovalsBuilder::with_capacity(victims.count() as usize);
                for victim in victims.iter() {
                    builder.add(victim, CandidateMask::of(candidate));
                }
                let removals = builder.build();
                if removals.is_empty() {
                    continue;
                }
                if !emit(Inference::elimination(
                    Technique::GeneralizedIntersections,
                    Rating::from_tenths(29),
                    removals,
                    Evidence::GeneralizedIntersections {
                        region,
                        digit: candidate,
                        locked_positions,
                    },
                )) {
                    return;
                }
            }
        }
    }
}

fn first_inference(visit: impl FnOnce(&mut dyn FnMut(Inference) -> bool)) -> Option<Inference> {
    let mut first = None;
    visit(&mut |inference| {
        first = Some(inference);
        false
    });
    first
}

fn collect_inferences(visit: impl FnOnce(&mut dyn FnMut(Inference) -> bool)) -> Vec<Inference> {
    let mut inferences = Vec::new();
    visit(&mut |inference| {
        inferences.push(inference);
        true
    });
    inferences
}

fn collect_unique_inferences<K: Eq>(
    visit: impl FnOnce(&mut dyn FnMut(Inference) -> bool),
    equality_key: impl Fn(&Inference) -> K,
) -> Vec<Inference> {
    let mut keys = Vec::new();
    let mut inferences = Vec::new();
    visit(&mut |inference| {
        let key = equality_key(&inference);
        if !keys.contains(&key) {
            keys.push(key);
            inferences.push(inference);
        }
        true
    });
    inferences
}

fn direct_hint_equality_key(inference: &Inference) -> (CellId, Digit) {
    // DirectHint.equals also compares the producer instance; it is constant
    // within either visitor, leaving cell and value as the local key.
    (
        inference.placement_cell().expect("direct hint placement"),
        inference
            .placement_digit()
            .expect("direct hint placement digit"),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LockingHintEqualityKey {
    // DirectLockingHint and LockingHint both compare only the candidate value
    // and the unordered highlighted-cell set.
    digit: Digit,
    pattern_cell_count: u8,
    pattern_cells: CellMask,
}

fn locking_hint_equality_key(grid: &Grid, inference: &Inference) -> LockingHintEqualityKey {
    let (secondary, pattern_positions, digit) = match inference.evidence() {
        Evidence::DirectLocking {
            secondary,
            pattern_positions,
            ..
        } => (
            secondary,
            pattern_positions,
            inference
                .placement_digit()
                .expect("direct locking placement"),
        ),
        Evidence::Locking {
            secondary,
            digit,
            pattern_positions,
            ..
        } => (secondary, pattern_positions, digit),
        _ => unreachable!("locking equality key evidence"),
    };
    let mut pattern_cells = CellMask::EMPTY;
    for position in pattern_positions.iter() {
        pattern_cells.insert(cell_id(
            grid.topology().region_cells(secondary)[usize::from(position)],
        ));
    }
    LockingHintEqualityKey {
        digit,
        pattern_cell_count: pattern_positions.count() as u8,
        pattern_cells,
    }
}

fn hidden_set_family_order(grid: &Grid, config: EngineConfig) -> Vec<usize> {
    let mut result = Vec::with_capacity(REGION_TYPE_COUNT);
    if grid.topology().config().blocks {
        result.push(0);
    }
    result.push(2);
    result.push(1);
    if config.variant_latin {
        return result;
    }
    for type_index in [3, 4, 5, 6, 7, 8, 9] {
        if grid.topology().is_region_type_active(type_index) {
            result.push(type_index);
        }
    }
    result
}

fn locking_family_pairs(grid: &Grid) -> Vec<(usize, usize)> {
    let topology = grid.topology();
    let mut pairs = vec![(0, 2), (0, 1), (2, 0), (1, 0)];
    if topology.config().disjoint_groups {
        pairs.extend([(3, 2), (3, 1), (2, 3), (1, 3), (0, 3), (3, 0)]);
    }
    if topology.config().windows {
        pairs.extend([(4, 2), (4, 1), (2, 4), (1, 4), (0, 4), (4, 0)]);
    }
    if topology.config().windows && topology.config().disjoint_groups {
        pairs.extend([(4, 3), (3, 4)]);
    }
    pairs
}

fn generalized_intersection_family_order(grid: &Grid) -> Vec<usize> {
    let mut result = Vec::with_capacity(REGION_TYPE_COUNT);
    if grid.topology().config().blocks {
        result.push(0);
    }
    result.extend([1, 2]);
    for type_index in [3, 4, 5, 6, 7, 8, 9] {
        if grid.topology().is_region_type_active(type_index) {
            result.push(type_index);
        }
    }
    result
}

fn cell_id(raw: u8) -> CellId {
    CellId::new(raw).expect("cell index")
}

fn digit(value: u8) -> Digit {
    Digit::new(value).expect("digit loop")
}

fn region_id(type_index: usize, region_index: usize) -> RegionId {
    RegionId::new(type_index as u8, region_index as u8).expect("topology region identity")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{ConstraintTopology, Grid, Puzzle, VariantConfig};

    use super::{
        collect_direct_hidden_sets, collect_direct_locking, collect_generalized_intersections,
        collect_hidden_singles, collect_locking, collect_naked_singles, find_direct_hidden_set,
        find_direct_locking, find_generalized_intersections, find_hidden_single, find_locking,
        find_naked_single,
    };
    use crate::{EngineConfig, Evidence, Rating, RatingMode, Technique};

    #[test]
    fn alone_cell_precedes_hidden_position() {
        let puzzle = Puzzle::parse(
            "12345678.........................................................................",
        )
        .unwrap();
        let grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &puzzle,
        );
        let inference = find_hidden_single(&grid, EngineConfig::default()).unwrap();
        assert_eq!(
            Some(inference.clone()),
            collect_hidden_singles(&grid, EngineConfig::default())
                .first()
                .cloned()
        );
        assert_eq!(inference.placement_cell().unwrap().raw(), 8);
        assert_eq!(inference.placement_digit().unwrap().get(), 9);
        assert_eq!(inference.rating(), Rating::from_tenths(10));
        assert!(matches!(
            inference.evidence(),
            Evidence::HiddenSingle { alone: true, .. }
        ));
    }

    #[test]
    fn hidden_single_dedup_matches_direct_hint_cell_value_equality() {
        let mut solved =
            "534678912672195348198342567859761423426853791713924856961537284287419635345286179"
                .chars()
                .collect::<Vec<_>>();
        solved[0] = '.';
        let puzzle = Puzzle::parse(&solved.into_iter().collect::<String>()).unwrap();
        let grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &puzzle,
        );

        let mut raw = Vec::new();
        super::visit_hidden_singles(&grid, EngineConfig::default(), &mut |inference| {
            raw.push(inference);
            true
        });
        assert_eq!(raw.len(), 3, "block, column, and row discoveries");

        let retained = collect_hidden_singles(&grid, EngineConfig::default());
        assert_eq!(retained.len(), 1);
        assert_eq!(
            find_hidden_single(&grid, EngineConfig::default()),
            retained.first().cloned()
        );
        assert_eq!(retained[0].placement_cell().unwrap().raw(), 0);
        assert_eq!(retained[0].placement_digit().unwrap().get(), 5);
    }

    #[test]
    fn naked_single_uses_cell_order_and_rating_mode() {
        let values = Puzzle::parse(
            "1................................................................................",
        )
        .unwrap();
        let mut display = ['.'; 729];
        display[0] = '1';
        display[9 + 4] = '5';
        display[18 + 5] = '6';
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        let grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &values,
            &candidates,
        )
        .unwrap();
        let original = find_naked_single(&grid, EngineConfig::default()).unwrap();
        let all = collect_naked_singles(&grid, EngineConfig::default());
        assert_eq!(Some(original.clone()), all.first().cloned());
        assert_eq!(
            all.iter()
                .map(|inference| inference.placement_cell().unwrap().raw())
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(original.placement_cell().unwrap().raw(), 1);
        assert_eq!(original.rating(), Rating::from_tenths(23));
        assert_eq!(
            original.description(grid.topology()),
            "Naked Single: r1c2: 5"
        );
        let revised = find_naked_single(
            &grid,
            EngineConfig {
                rating_mode: RatingMode::Revised,
                ..EngineConfig::default()
            },
        )
        .unwrap();
        assert_eq!(revised.rating(), Rating::from_tenths(16));
    }

    #[test]
    fn direct_hidden_triplet_keeps_explanatory_candidates_out_of_effects() {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = ['.'; 729];
        for (cell, digits) in [
            (0, &[1, 2, 4, 5][..]),
            (3, &[1, 3]),
            (6, &[2, 3]),
            (8, &[4, 5]),
        ] {
            for &value in digits {
                display[cell * 9 + value - 1] = char::from(b'0' + value as u8);
            }
        }
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        let mut grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig {
                blocks: false,
                ..VariantConfig::default()
            })),
            &values,
            &candidates,
        )
        .unwrap();
        let inference = find_direct_hidden_set(&grid, EngineConfig::default(), 3).unwrap();
        assert_eq!(
            Some(inference.clone()),
            collect_direct_hidden_sets(&grid, EngineConfig::default(), 3)
                .first()
                .cloned()
        );
        assert_eq!(inference.technique(), Technique::DirectHiddenTriplet);
        assert_eq!(inference.rating(), Rating::from_tenths(25));
        assert_eq!(inference.placement_cell().unwrap().raw(), 8);
        assert_eq!(inference.placement_digit().unwrap().get(), 4);
        assert_eq!(
            inference.description(grid.topology()),
            "Direct Hidden Triplet: Cells r1c1,r1c4,r1c7: 1,2,3 in row"
        );
        assert!(inference.removals().is_empty());
        inference.apply(&mut grid);
        assert!(
            grid.candidates(sukaku_forge_core::CellId::new(0).unwrap())
                .contains(sukaku_forge_core::Digit::new(5).unwrap())
        );
    }

    #[test]
    fn direct_hidden_set_retains_identity_distinct_region_explanations() {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = ['.'; 729];
        for (cell, digits) in [(0, &[1, 2, 3][..]), (1, &[1, 2]), (2, &[3])] {
            for &value in digits {
                display[cell * 9 + value - 1] = char::from(b'0' + value as u8);
            }
        }
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        let grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &values,
            &candidates,
        )
        .unwrap();

        let retained = collect_direct_hidden_sets(&grid, EngineConfig::default(), 2);
        assert_eq!(
            retained
                .iter()
                .map(|inference| inference.description(grid.topology()))
                .collect::<Vec<_>>(),
            [
                "Direct Hidden Pair: Cells r1c1,r1c2: 1,2 in block",
                "Direct Hidden Pair: Cells r1c1,r1c2: 1,2 in row",
            ]
        );
        assert_eq!(
            find_direct_hidden_set(&grid, EngineConfig::default(), 2),
            retained.first().cloned()
        );
    }

    #[test]
    fn direct_pointing_uses_secondary_region_cell_order_and_places_only() {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = ['.'; 729];
        for cell in [0, 9, 27, 28, 54] {
            display[cell * 9] = '1';
        }
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        let mut grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &values,
            &candidates,
        )
        .unwrap();
        let inference = find_direct_locking(&grid).unwrap();
        assert_eq!(
            Some(inference.clone()),
            collect_direct_locking(&grid).first().cloned()
        );
        assert_eq!(inference.technique(), Technique::DirectPointing);
        assert_eq!(inference.rating(), Rating::from_tenths(17));
        assert_eq!(inference.placement_cell().unwrap().raw(), 28);
        assert_eq!(
            inference.description(grid.topology()),
            "Direct Pointing: Cells r1c1,r2c1: 1 of block in column"
        );
        assert!(inference.removals().is_empty());
        inference.apply(&mut grid);
        assert!(
            grid.candidates(sukaku_forge_core::CellId::new(54).unwrap())
                .contains(sukaku_forge_core::Digit::new(1).unwrap())
        );
    }

    #[test]
    fn direct_locking_dedup_ignores_the_alternate_target() {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = ['.'; 729];
        for cell in [0, 9, 27, 28, 54, 55] {
            display[cell * 9] = '1';
        }
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        let grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &values,
            &candidates,
        )
        .unwrap();

        let mut raw = Vec::new();
        super::visit_direct_locking(&grid, &mut |inference| {
            raw.push(inference);
            true
        });
        assert_eq!(raw.len(), 2);
        assert_eq!(
            raw.iter()
                .map(|inference| inference.placement_cell().unwrap().raw())
                .collect::<Vec<_>>(),
            [28, 55]
        );

        let retained = collect_direct_locking(&grid);
        assert_eq!(retained.len(), 1);
        assert_eq!(find_direct_locking(&grid), retained.first().cloned());
        assert_eq!(retained[0].placement_cell().unwrap().raw(), 28);
    }

    #[test]
    fn direct_claiming_is_classified_from_the_primary_family() {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = ['.'; 729];
        for cell in [0, 9, 1, 28] {
            display[cell * 9] = '1';
        }
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        let grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &values,
            &candidates,
        )
        .unwrap();
        let inference = find_direct_locking(&grid).unwrap();
        assert_eq!(inference.technique(), Technique::DirectClaiming);
        assert_eq!(inference.rating(), Rating::from_tenths(19));
        assert_eq!(
            inference.description(grid.topology()),
            "Direct Claiming: Cells r1c1,r2c1: 1 of column in block"
        );
    }

    #[test]
    fn pointing_removes_all_cover_candidates_outside_the_source_region() {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = ['.'; 729];
        for cell in [0, 9, 27, 54] {
            display[cell * 9] = '1';
        }
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        let mut grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &values,
            &candidates,
        )
        .unwrap();
        let inference = find_locking(&grid).unwrap();
        assert_eq!(
            Some(inference.clone()),
            collect_locking(&grid).first().cloned()
        );
        assert_eq!(inference.technique(), Technique::Pointing);
        assert_eq!(inference.rating(), Rating::from_tenths(26));
        assert_eq!(
            inference.description(grid.topology()),
            "Pointing: Cells r1c1,r2c1: 1 in block and column"
        );
        assert!(!inference.is_placement());
        assert_eq!(
            inference
                .removals()
                .iter()
                .map(|entry| entry.cell().raw())
                .collect::<Vec<_>>(),
            [27, 54]
        );
        inference.apply(&mut grid);
        for raw_cell in [27, 54] {
            assert!(
                !grid
                    .candidates(sukaku_forge_core::CellId::new(raw_cell).unwrap())
                    .contains(sukaku_forge_core::Digit::new(1).unwrap())
            );
        }
    }

    #[test]
    fn generalized_intersections_uses_anti_knight_common_visibility() {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = ['.'; 729];
        for cell in [57, 67, 75, 74] {
            display[cell * 9] = '1';
        }
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        let mut grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            })),
            &values,
            &candidates,
        )
        .unwrap();
        let inference = find_generalized_intersections(&grid).unwrap();
        assert_eq!(
            Some(inference.clone()),
            collect_generalized_intersections(&grid).first().cloned()
        );
        assert_eq!(inference.technique(), Technique::GeneralizedIntersections);
        assert_eq!(inference.rating(), Rating::from_tenths(29));
        assert_eq!(
            inference.description(grid.topology()),
            "Cells r7c4,r8c5,r9c4 on value 1 in block 8"
        );
        let removal = inference.removals().iter().next().unwrap();
        assert_eq!(removal.cell().raw(), 74);
        assert_eq!(inference.removals().elimination_count(), 1);
        inference.apply(&mut grid);
        assert!(
            !grid
                .candidates(sukaku_forge_core::CellId::new(74).unwrap())
                .contains(sukaku_forge_core::Digit::new(1).unwrap())
        );
    }
}
