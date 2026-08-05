use sukaku_forge_core::{
    CandidateMask, CandidateRemovalsBuilder, CellId, CellMask, Digit, Grid, PositionMask,
    REGION_TYPE_COUNT, RegionId,
};

use crate::{CellSequence, EngineConfig, Evidence, Inference, Rating, RatingMode, Technique};

/// Find the first ordinary or visibility-generalized Naked Set.
#[must_use]
pub fn find_naked_set(
    grid: &Grid,
    config: EngineConfig,
    degree: u8,
    generalized: bool,
) -> Option<Inference> {
    assert!(
        matches!(degree, 2..=4),
        "only pair through quad layers are currently registered"
    );
    let families = if generalized {
        extended_family_order(grid, config)
    } else {
        naked_set_family_order(grid)
    };
    for type_index in families {
        for region_index in 0..grid.topology().region_count(type_index) {
            let region = region_id(type_index, region_index);
            if empty_cell_count(grid, region) < usize::from(degree * 2) {
                continue;
            }
            for subset in combination_masks(degree) {
                let tuple_positions = PositionMask::from_bits(subset);
                let mut tuple_digits = CandidateMask::EMPTY;
                let mut valid = true;
                for position in tuple_positions.iter() {
                    let candidates = grid.candidates(region_cell(grid, region, position));
                    if candidates.count() <= 1 {
                        valid = false;
                        break;
                    }
                    tuple_digits = tuple_digits.union(candidates);
                }
                if !valid || tuple_digits.count() != u32::from(degree) {
                    continue;
                }
                let removals = if generalized {
                    generalized_naked_removals(grid, region, tuple_positions, tuple_digits)
                } else {
                    region_naked_removals(grid, region, tuple_positions, tuple_digits)
                };
                if removals.is_empty() {
                    continue;
                }
                return Some(Inference::elimination(
                    naked_set_technique(degree, generalized),
                    naked_set_rating(degree),
                    removals,
                    Evidence::NakedSet {
                        degree,
                        region,
                        tuple_digits,
                        tuple_positions,
                        generalized,
                    },
                ));
            }
        }
    }
    None
}

/// Find the first indirect Hidden Set.
#[must_use]
pub fn find_hidden_set(grid: &Grid, config: EngineConfig, degree: u8) -> Option<Inference> {
    assert!(
        matches!(degree, 2..=4),
        "only pair through quad layers are currently registered"
    );
    for type_index in extended_family_order(grid, config) {
        for region_index in 0..grid.topology().region_count(type_index) {
            let region = region_id(type_index, region_index);
            if empty_cell_count(grid, region) <= usize::from(degree * 2) {
                continue;
            }
            for subset in combination_masks(degree) {
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
                let mut builder = CandidateRemovalsBuilder::with_capacity(usize::from(degree));
                for position in tuple_positions.iter() {
                    let cell = region_cell(grid, region, position);
                    builder.add(cell, grid.candidates(cell).without(tuple_digits));
                }
                let removals = builder.build();
                if removals.is_empty() {
                    continue;
                }
                return Some(Inference::elimination(
                    hidden_set_technique(degree),
                    hidden_set_rating(degree, config.rating_mode),
                    removals,
                    Evidence::HiddenSet {
                        degree,
                        region,
                        tuple_digits,
                        tuple_positions,
                    },
                ));
            }
        }
    }
    None
}

/// Find the first row/column fish of the requested degree.
#[must_use]
pub fn find_fish(grid: &Grid, config: EngineConfig, degree: u8) -> Option<Inference> {
    assert!(
        matches!(degree, 2..=4),
        "only X-Wing through Jellyfish are currently registered"
    );
    let mut occurrences = [0_u8; 10];
    for raw_cell in 0_u8..81 {
        let value = grid.value(CellId::new(raw_cell).expect("cell loop"));
        if value != 0 {
            occurrences[usize::from(value)] += 1;
        }
    }
    for (base_type, cover_type) in [(2_usize, 1_usize), (1, 2)] {
        for subset in combination_masks(degree) {
            let base_indexes = PositionMask::from_bits(subset);
            for value in 1_u8..=9 {
                if occurrences[usize::from(value)] + degree * 2 > 9 {
                    continue;
                }
                let digit = Digit::new(value).expect("digit loop");
                let mut cover_indexes = PositionMask::EMPTY;
                let mut valid = true;
                for base_index in base_indexes.iter() {
                    let positions = grid.region_candidate_positions(
                        region_id(base_type, usize::from(base_index)),
                        digit,
                    );
                    if positions.count() <= 1 {
                        valid = false;
                        break;
                    }
                    cover_indexes = cover_indexes.union(positions);
                }
                if !valid || cover_indexes.count() != u32::from(degree) {
                    continue;
                }
                let mut selected_cells = CellSequence::new();
                let mut builder = CandidateRemovalsBuilder::with_capacity(
                    usize::from(degree) * usize::from(9 - degree),
                );
                for cover_index in cover_indexes.iter() {
                    let cover = region_id(cover_type, usize::from(cover_index));
                    for base_index in base_indexes.iter() {
                        let cell = region_cell(grid, cover, base_index);
                        if grid.candidates(cell).contains(digit) {
                            selected_cells.push(cell);
                        }
                    }
                    let removable_positions = grid
                        .region_candidate_positions(cover, digit)
                        .without(base_indexes);
                    for position in removable_positions.iter() {
                        builder.add(region_cell(grid, cover, position), CandidateMask::of(digit));
                    }
                }
                let removals = builder.build();
                if removals.is_empty() {
                    continue;
                }
                return Some(Inference::elimination(
                    fish_technique(degree),
                    fish_rating(degree, config.rating_mode),
                    removals,
                    Evidence::Fish {
                        degree,
                        digit,
                        base_type: base_type as u8,
                        cover_type: cover_type as u8,
                        selected_cells,
                    },
                ));
            }
        }
    }
    None
}

fn naked_set_technique(degree: u8, generalized: bool) -> Technique {
    match (degree, generalized) {
        (2, false) => Technique::NakedPair,
        (2, true) => Technique::GeneralizedNakedPair,
        (3, false) => Technique::NakedTriplet,
        (3, true) => Technique::GeneralizedNakedTriplet,
        (4, false) => Technique::NakedQuad,
        (4, true) => Technique::GeneralizedNakedQuad,
        _ => unreachable!("registered naked-set degree"),
    }
}

fn naked_set_rating(degree: u8) -> Rating {
    match degree {
        2 => Rating::from_tenths(30),
        3 => Rating::from_tenths(36),
        4 => Rating::from_tenths(50),
        _ => unreachable!("registered naked-set degree"),
    }
}

fn hidden_set_technique(degree: u8) -> Technique {
    match degree {
        2 => Technique::HiddenPair,
        3 => Technique::HiddenTriplet,
        4 => Technique::HiddenQuad,
        _ => unreachable!("registered hidden-set degree"),
    }
}

fn hidden_set_rating(degree: u8, mode: RatingMode) -> Rating {
    match (degree, mode) {
        (2, RatingMode::Original) => Rating::from_tenths(34),
        (2, RatingMode::Revised) => Rating::from_tenths(29),
        (3, RatingMode::Original) => Rating::from_tenths(40),
        (3, RatingMode::Revised) => Rating::from_tenths(38),
        (4, RatingMode::Original) => Rating::from_tenths(54),
        (4, RatingMode::Revised) => Rating::from_tenths(52),
        _ => unreachable!("registered hidden-set degree"),
    }
}

fn fish_technique(degree: u8) -> Technique {
    match degree {
        2 => Technique::XWing,
        3 => Technique::Swordfish,
        4 => Technique::Jellyfish,
        _ => unreachable!("registered fish degree"),
    }
}

fn fish_rating(degree: u8, mode: RatingMode) -> Rating {
    match (degree, mode) {
        (2, _) => Rating::from_tenths(32),
        (3, RatingMode::Original) => Rating::from_tenths(38),
        (3, RatingMode::Revised) => Rating::from_tenths(40),
        (4, RatingMode::Original) => Rating::from_tenths(52),
        (4, RatingMode::Revised) => Rating::from_tenths(54),
        _ => unreachable!("registered fish degree"),
    }
}

fn region_naked_removals(
    grid: &Grid,
    region: RegionId,
    tuple_positions: PositionMask,
    tuple_digits: CandidateMask,
) -> sukaku_forge_core::CandidateRemovals {
    let mut builder = CandidateRemovalsBuilder::with_capacity(7);
    for position in PositionMask::ALL.without(tuple_positions).iter() {
        let cell = region_cell(grid, region, position);
        builder.add(cell, grid.candidates(cell).intersect(tuple_digits));
    }
    builder.build()
}

fn generalized_naked_removals(
    grid: &Grid,
    region: RegionId,
    tuple_positions: PositionMask,
    tuple_digits: CandidateMask,
) -> sukaku_forge_core::CandidateRemovals {
    let mut builder = CandidateRemovalsBuilder::with_capacity(8);
    let tuple_cells = tuple_cell_mask(grid, region, tuple_positions);
    for digit in tuple_digits.iter() {
        let mut supports = tuple_positions.iter().filter(|&position| {
            grid.candidates(region_cell(grid, region, position))
                .contains(digit)
        });
        let first = supports.next().expect("tuple digit has a supporting cell");
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
            builder.add(victim, CandidateMask::of(digit));
        }
    }
    builder.build()
}

fn tuple_cell_mask(grid: &Grid, region: RegionId, positions: PositionMask) -> CellMask {
    let mut result = CellMask::EMPTY;
    for position in positions.iter() {
        result.insert(region_cell(grid, region, position));
    }
    result
}

fn naked_set_family_order(grid: &Grid) -> Vec<usize> {
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

fn extended_family_order(grid: &Grid, config: EngineConfig) -> Vec<usize> {
    let mut result = Vec::with_capacity(REGION_TYPE_COUNT);
    if grid.topology().config().blocks {
        result.push(0);
    }
    result.extend([2, 1]);
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

fn combination_masks(degree: u8) -> CombinationMasks {
    CombinationMasks {
        next: (1_u16 << degree) - 1,
    }
}

/// Increasing fixed-cardinality nine-bit masks without scanning rejected masks.
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

fn empty_cell_count(grid: &Grid, region: RegionId) -> usize {
    grid.topology()
        .region_cells(region)
        .iter()
        .filter(|&&raw| grid.value(CellId::new(raw).expect("region cell")) == 0)
        .count()
}

fn region_cell(grid: &Grid, region: RegionId, position: u8) -> CellId {
    CellId::new(grid.topology().region_cells(region)[usize::from(position)]).expect("region cell")
}

fn region_id(type_index: usize, region_index: usize) -> RegionId {
    RegionId::new(type_index as u8, region_index as u8).expect("topology region identity")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{ConstraintTopology, Grid, Puzzle, VariantConfig};

    use super::{combination_masks, find_fish, find_hidden_set, find_naked_set};
    use crate::{EngineConfig, Rating, RatingMode, Technique};

    fn sparse_snapshot(config: VariantConfig, entries: &[(usize, &[u8])]) -> Grid {
        sparse_snapshot_with_values(config, &[], entries)
    }

    fn sparse_snapshot_with_values(
        config: VariantConfig,
        placed: &[(usize, u8)],
        entries: &[(usize, &[u8])],
    ) -> Grid {
        let mut value_display = ['.'; 81];
        for &(cell, value) in placed {
            value_display[cell] = char::from(b'0' + value);
        }
        let values = Puzzle::parse(&value_display.iter().collect::<String>()).unwrap();
        let mut display = ['.'; 729];
        for &(cell, digits) in entries {
            for &digit in digits {
                display[cell * 9 + usize::from(digit - 1)] = char::from(b'0' + digit);
            }
        }
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(config)),
            &values,
            &candidates,
        )
        .unwrap()
    }

    #[test]
    fn triplet_combinations_follow_java_numeric_mask_order() {
        let combinations = combination_masks(3).collect::<Vec<_>>();
        assert_eq!(&combinations[..5], &[7, 11, 13, 14, 19]);
        assert_eq!(combinations.len(), 84);
        assert_eq!(combinations.last(), Some(&448));
    }

    #[test]
    fn naked_pair_removes_both_digits_from_a_region_victim() {
        let mut grid = sparse_snapshot(
            VariantConfig::default(),
            &[(0, &[1, 2]), (1, &[1, 2]), (2, &[1, 2, 3])],
        );
        let inference = find_naked_set(&grid, EngineConfig::default(), 2, false).unwrap();
        assert_eq!(inference.technique(), Technique::NakedPair);
        assert_eq!(inference.rating(), Rating::from_tenths(30));
        assert_eq!(
            inference.description(grid.topology()),
            "Naked Pair: Cells r1c1,r1c2: 1,2 in block"
        );
        assert_eq!(inference.removals().elimination_count(), 2);
        inference.apply(&mut grid);
        assert_eq!(
            grid.candidates(sukaku_forge_core::CellId::new(2).unwrap())
                .single()
                .unwrap()
                .get(),
            3
        );
    }

    #[test]
    fn generalized_pair_uses_anti_knight_common_visibility() {
        let entries = [(0, &[1, 2][..]), (10, &[1, 2]), (3, &[1, 3])];
        let classic = sparse_snapshot(VariantConfig::default(), &entries);
        assert!(find_naked_set(&classic, EngineConfig::default(), 2, true).is_none());

        let mut anti_knight = sparse_snapshot(
            VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            },
            &entries,
        );
        let inference = find_naked_set(&anti_knight, EngineConfig::default(), 2, true).unwrap();
        assert_eq!(inference.technique(), Technique::GeneralizedNakedPair);
        assert_eq!(
            inference.description(anti_knight.topology()),
            "Generalized Naked Pair: Cells r1c1,r2c2: 1,2 in block"
        );
        inference.apply(&mut anti_knight);
        assert_eq!(
            anti_knight
                .candidates(sukaku_forge_core::CellId::new(3).unwrap())
                .single()
                .unwrap()
                .get(),
            3
        );
    }

    #[test]
    fn x_wing_preserves_cover_then_base_cell_order() {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = "123456789".repeat(81).chars().collect::<Vec<_>>();
        for row in 0..9 {
            if !matches!(row, 1 | 6) {
                for column in [0, 3] {
                    display[(row * 9 + column) * 9 + 4] = '.';
                }
            }
        }
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        let grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &values,
            &candidates,
        )
        .unwrap();
        let inference = find_fish(&grid, EngineConfig::default(), 2).unwrap();
        assert_eq!(inference.technique(), Technique::XWing);
        assert_eq!(inference.rating(), Rating::from_tenths(32));
        assert_eq!(
            inference.description(grid.topology()),
            "X-Wing: Cells r2c1,r2c4,r7c1,r7c4: 5 in 2 columns and 2 rows"
        );
        assert_eq!(inference.removals().elimination_count(), 14);
    }

    #[test]
    fn hidden_pair_rating_changes_but_effect_and_text_do_not() {
        let grid = sparse_snapshot(
            VariantConfig::default(),
            &[(3, &[1, 5, 9]), (4, &[2, 5, 9])],
        );
        let original = find_hidden_set(&grid, EngineConfig::default(), 2).unwrap();
        assert_eq!(original.technique(), Technique::HiddenPair);
        assert_eq!(original.rating(), Rating::from_tenths(34));
        assert_eq!(
            original.description(grid.topology()),
            "Hidden Pair: Cells r1c4,r1c5: 5,9 in block"
        );
        let revised = find_hidden_set(
            &grid,
            EngineConfig {
                rating_mode: RatingMode::Revised,
                ..EngineConfig::default()
            },
            2,
        )
        .unwrap();
        assert_eq!(revised.rating(), Rating::from_tenths(29));
        assert_eq!(revised.removals(), original.removals());
    }

    #[test]
    fn naked_triplet_aggregates_three_digit_removals() {
        let entries = [
            (0, &[1, 2][..]),
            (1, &[1, 3]),
            (2, &[2, 3]),
            (9, &[1, 2, 3, 4]),
        ];
        let mut grid = sparse_snapshot_with_values(
            VariantConfig::default(),
            &[(10, 5), (11, 6), (18, 7)],
            &entries,
        );
        let inference = find_naked_set(&grid, EngineConfig::default(), 3, false).unwrap();
        assert_eq!(inference.technique(), Technique::NakedTriplet);
        assert_eq!(inference.rating(), Rating::from_tenths(36));
        assert_eq!(
            inference.description(grid.topology()),
            "Naked Triplet: Cells r1c1,r1c2,r1c3: 1,2,3 in block"
        );
        assert_eq!(inference.removals().elimination_count(), 3);
        inference.apply(&mut grid);
        assert_eq!(
            grid.candidates(sukaku_forge_core::CellId::new(9).unwrap())
                .single()
                .unwrap()
                .get(),
            4
        );

        let five_empty_cells = sparse_snapshot_with_values(
            VariantConfig::default(),
            &[(10, 5), (11, 6), (18, 7), (19, 8)],
            &entries,
        );
        assert!(find_naked_set(&five_empty_cells, EngineConfig::default(), 3, false).is_none());
    }

    #[test]
    fn generalized_triplet_intersects_only_each_digits_support_cells() {
        let mut grid = sparse_snapshot(
            VariantConfig::default(),
            &[(0, &[1, 2]), (1, &[1, 3]), (9, &[2, 3]), (3, &[1, 4])],
        );
        assert!(find_naked_set(&grid, EngineConfig::default(), 3, false).is_none());
        let inference = find_naked_set(&grid, EngineConfig::default(), 3, true).unwrap();
        assert_eq!(inference.technique(), Technique::GeneralizedNakedTriplet);
        assert_eq!(inference.rating(), Rating::from_tenths(36));
        assert_eq!(
            inference.description(grid.topology()),
            "Generalized Naked Triplet: Cells r1c1,r1c2,r2c1: 1,2,3 in block"
        );
        inference.apply(&mut grid);
        assert_eq!(
            grid.candidates(sukaku_forge_core::CellId::new(3).unwrap())
                .single()
                .unwrap()
                .get(),
            4
        );
    }

    #[test]
    fn swordfish_preserves_cover_then_base_order_and_rating_mode() {
        let grid = sparse_snapshot(
            VariantConfig::default(),
            &[
                (0, &[1]),
                (9, &[1]),
                (10, &[1]),
                (19, &[1]),
                (2, &[1]),
                (20, &[1]),
                (3, &[1]),
            ],
        );
        let original = find_fish(&grid, EngineConfig::default(), 3).unwrap();
        assert_eq!(original.technique(), Technique::Swordfish);
        assert_eq!(original.rating(), Rating::from_tenths(38));
        assert_eq!(
            original.description(grid.topology()),
            "Swordfish: Cells r1c1,r1c3,r2c1,r2c2,r3c2,r3c3: 1 in 3 columns and 3 rows"
        );
        assert_eq!(original.removals().elimination_count(), 1);

        let revised = find_fish(
            &grid,
            EngineConfig {
                rating_mode: RatingMode::Revised,
                ..EngineConfig::default()
            },
            3,
        )
        .unwrap();
        assert_eq!(revised.rating(), Rating::from_tenths(40));
        assert_eq!(revised.removals(), original.removals());
    }

    #[test]
    fn hidden_triplet_rating_changes_but_effect_and_text_do_not() {
        let entries = [
            (0, &[1, 2, 4][..]),
            (3, &[1, 3]),
            (6, &[2, 3]),
            (8, &[4, 5]),
        ];
        let grid =
            sparse_snapshot_with_values(VariantConfig::default(), &[(1, 7), (2, 8)], &entries);
        let original = find_hidden_set(&grid, EngineConfig::default(), 3).unwrap();
        assert_eq!(original.technique(), Technique::HiddenTriplet);
        assert_eq!(original.rating(), Rating::from_tenths(40));
        assert_eq!(
            original.description(grid.topology()),
            "Hidden Triplet: Cells r1c1,r1c4,r1c7: 1,2,3 in row"
        );
        assert_eq!(original.removals().elimination_count(), 1);

        let revised = find_hidden_set(
            &grid,
            EngineConfig {
                rating_mode: RatingMode::Revised,
                ..EngineConfig::default()
            },
            3,
        )
        .unwrap();
        assert_eq!(revised.rating(), Rating::from_tenths(38));
        assert_eq!(revised.removals(), original.removals());

        let six_empty_cells = sparse_snapshot_with_values(
            VariantConfig::default(),
            &[(1, 7), (2, 8), (4, 9)],
            &entries,
        );
        assert!(find_hidden_set(&six_empty_cells, EngineConfig::default(), 3).is_none());
    }

    #[test]
    fn naked_quad_preserves_numeric_position_order_and_eight_empty_gate() {
        let entries = [
            (0, &[1, 2][..]),
            (1, &[1, 3]),
            (2, &[2, 4]),
            (9, &[3, 4]),
            (10, &[1, 2, 3, 4, 5]),
        ];
        let mut grid = sparse_snapshot_with_values(VariantConfig::default(), &[(18, 6)], &entries);
        let inference = find_naked_set(&grid, EngineConfig::default(), 4, false).unwrap();
        assert_eq!(inference.technique(), Technique::NakedQuad);
        assert_eq!(inference.rating(), Rating::from_tenths(50));
        assert_eq!(
            inference.description(grid.topology()),
            "Naked Quad: Cells r1c1,r1c2,r1c3,r2c1: 1,2,3,4 in block"
        );
        assert_eq!(inference.removals().elimination_count(), 4);
        inference.apply(&mut grid);
        assert_eq!(
            grid.candidates(sukaku_forge_core::CellId::new(10).unwrap())
                .single()
                .unwrap()
                .get(),
            5
        );

        let seven_empty_cells =
            sparse_snapshot_with_values(VariantConfig::default(), &[(18, 6), (19, 7)], &entries);
        assert!(find_naked_set(&seven_empty_cells, EngineConfig::default(), 4, false).is_none());
    }

    #[test]
    fn generalized_quad_can_eliminate_outside_its_source_region() {
        let entries = [
            (0, &[1, 2][..]),
            (1, &[1, 3]),
            (9, &[2, 4]),
            (10, &[3, 4]),
            (3, &[1, 5]),
        ];
        let mut grid = sparse_snapshot(VariantConfig::default(), &entries);
        assert!(find_naked_set(&grid, EngineConfig::default(), 4, false).is_none());
        let inference = find_naked_set(&grid, EngineConfig::default(), 4, true).unwrap();
        assert_eq!(inference.technique(), Technique::GeneralizedNakedQuad);
        assert_eq!(inference.rating(), Rating::from_tenths(50));
        assert_eq!(
            inference.description(grid.topology()),
            "Generalized Naked Quad: Cells r1c1,r1c2,r2c1,r2c2: 1,2,3,4 in block"
        );
        inference.apply(&mut grid);
        assert_eq!(
            grid.candidates(sukaku_forge_core::CellId::new(3).unwrap())
                .single()
                .unwrap()
                .get(),
            5
        );
    }

    #[test]
    fn jellyfish_preserves_cover_major_pattern_order_and_rating_mode() {
        let mut grid = sparse_snapshot(
            VariantConfig::default(),
            &[
                (0, &[1]),
                (9, &[1]),
                (10, &[1]),
                (19, &[1]),
                (20, &[1]),
                (29, &[1]),
                (3, &[1]),
                (30, &[1]),
                (4, &[1, 7]),
            ],
        );
        let original = find_fish(&grid, EngineConfig::default(), 4).unwrap();
        assert_eq!(original.technique(), Technique::Jellyfish);
        assert_eq!(original.rating(), Rating::from_tenths(52));
        assert_eq!(
            original.description(grid.topology()),
            "Jellyfish: Cells r1c1,r1c4,r2c1,r2c2,r3c2,r3c3,r4c3,r4c4: 1 in 4 columns and 4 rows"
        );
        assert_eq!(original.removals().elimination_count(), 1);
        original.apply(&mut grid);
        assert_eq!(
            grid.candidates(sukaku_forge_core::CellId::new(4).unwrap())
                .single()
                .unwrap()
                .get(),
            7
        );

        let revised_grid = sparse_snapshot(
            VariantConfig::default(),
            &[
                (0, &[1]),
                (9, &[1]),
                (10, &[1]),
                (19, &[1]),
                (20, &[1]),
                (29, &[1]),
                (3, &[1]),
                (30, &[1]),
                (4, &[1, 7]),
            ],
        );
        let revised = find_fish(
            &revised_grid,
            EngineConfig {
                rating_mode: RatingMode::Revised,
                ..EngineConfig::default()
            },
            4,
        )
        .unwrap();
        assert_eq!(revised.rating(), Rating::from_tenths(54));
        assert_eq!(revised.removals().elimination_count(), 1);

        let one_placed = sparse_snapshot_with_values(
            VariantConfig::default(),
            &[(70, 1)],
            &[
                (0, &[1]),
                (9, &[1]),
                (10, &[1]),
                (19, &[1]),
                (20, &[1]),
                (29, &[1]),
                (3, &[1]),
                (30, &[1]),
                (4, &[1, 7]),
            ],
        );
        assert!(find_fish(&one_placed, EngineConfig::default(), 4).is_some());

        let two_placed = sparse_snapshot_with_values(
            VariantConfig::default(),
            &[(70, 1), (77, 1)],
            &[
                (0, &[1]),
                (9, &[1]),
                (10, &[1]),
                (19, &[1]),
                (20, &[1]),
                (29, &[1]),
                (3, &[1]),
                (30, &[1]),
                (4, &[1, 7]),
            ],
        );
        assert!(find_fish(&two_placed, EngineConfig::default(), 4).is_none());
    }

    #[test]
    fn jellyfish_falls_back_to_row_base_with_cover_major_evidence() {
        let mut grid = sparse_snapshot(
            VariantConfig::default(),
            &[
                (0, &[1]),
                (1, &[1]),
                (10, &[1]),
                (11, &[1]),
                (20, &[1]),
                (21, &[1]),
                (27, &[1]),
                (30, &[1]),
                (36, &[1, 7]),
            ],
        );
        let inference = find_fish(&grid, EngineConfig::default(), 4).unwrap();
        assert_eq!(inference.rating(), Rating::from_tenths(52));
        assert_eq!(
            inference.description(grid.topology()),
            "Jellyfish: Cells r1c1,r4c1,r1c2,r2c2,r2c3,r3c3,r3c4,r4c4: 1 in 4 rows and 4 columns"
        );
        inference.apply(&mut grid);
        assert_eq!(
            grid.candidates(sukaku_forge_core::CellId::new(36).unwrap())
                .single()
                .unwrap()
                .get(),
            7
        );
    }

    #[test]
    fn hidden_quad_requires_nine_empty_cells_and_uses_rating_mode() {
        let entries = [
            (0, &[1, 2, 5][..]),
            (1, &[1, 3]),
            (9, &[2, 4]),
            (10, &[3, 4]),
        ];
        let mut grid = sparse_snapshot(VariantConfig::default(), &entries);
        let original = find_hidden_set(&grid, EngineConfig::default(), 4).unwrap();
        assert_eq!(original.technique(), Technique::HiddenQuad);
        assert_eq!(original.rating(), Rating::from_tenths(54));
        assert_eq!(
            original.description(grid.topology()),
            "Hidden Quad: Cells r1c1,r1c2,r2c1,r2c2: 1,2,3,4 in block"
        );
        assert_eq!(original.removals().elimination_count(), 1);
        original.apply(&mut grid);
        assert_eq!(
            grid.candidates(sukaku_forge_core::CellId::new(0).unwrap())
                .count(),
            2
        );

        let revised_grid = sparse_snapshot(VariantConfig::default(), &entries);
        let revised = find_hidden_set(
            &revised_grid,
            EngineConfig {
                rating_mode: RatingMode::Revised,
                ..EngineConfig::default()
            },
            4,
        )
        .unwrap();
        assert_eq!(revised.rating(), Rating::from_tenths(52));

        let eight_empty_cells =
            sparse_snapshot_with_values(VariantConfig::default(), &[(20, 9)], &entries);
        assert!(find_hidden_set(&eight_empty_cells, EngineConfig::default(), 4).is_none());
    }
}
