use sukaku_forge_core::{
    CandidateMask, CandidateRemovals, CandidateRemovalsBuilder, CellId, CellMask, Digit, Grid,
    NonConsecutiveMode, RegionId,
};

/// The two neighbor geometries used by the legacy NC direct producers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NonConsecutiveGeometry {
    Orthogonal,
    Ferz,
}

/// Java-ordered one- or two-digit explanation payload.
///
/// A mask is not sufficient here: cyclic locked-NC deliberately displays
/// `9,2` for a locked 1 and `8,1` for a locked 9.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NonConsecutiveDigitSequence {
    digits: [u8; 2],
    len: u8,
}

impl NonConsecutiveDigitSequence {
    fn one(digit: Digit) -> Self {
        Self {
            digits: [digit.get(), 0],
            len: 1,
        }
    }

    fn two(first: Digit, second: Digit) -> Self {
        Self {
            digits: [first.get(), second.get()],
            len: 2,
        }
    }

    pub fn iter(self) -> impl ExactSizeIterator<Item = Digit> {
        self.digits
            .into_iter()
            .take(usize::from(self.len))
            .map(|raw| Digit::new(raw).expect("stored NC digit"))
    }

    #[must_use]
    pub const fn len(self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn mask(self) -> CandidateMask {
        self.iter().fold(CandidateMask::EMPTY, |mask, digit| {
            mask.union(CandidateMask::of(digit))
        })
    }
}

/// Candidate cells retained in a locked-NC explanation, in region order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NonConsecutiveCellSequence {
    cells: [u8; 5],
    len: u8,
}

impl NonConsecutiveCellSequence {
    fn from_region_cells(cells: &[u8; 9], positions: impl Iterator<Item = u8>) -> Self {
        let mut result = Self {
            cells: [0; 5],
            len: 0,
        };
        for position in positions {
            result.cells[usize::from(result.len)] = cells[usize::from(position)];
            result.len += 1;
        }
        result
    }

    pub fn iter(self) -> impl ExactSizeIterator<Item = CellId> {
        self.cells
            .into_iter()
            .take(usize::from(self.len))
            .map(|raw| CellId::new(raw).expect("stored NC cell"))
    }

    #[must_use]
    pub const fn len(self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }
}

/// Presentation metadata shared by the orthogonal and Ferz producers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NonConsecutiveHintKind {
    ForcingCell {
        cell: CellId,
        values: NonConsecutiveDigitSequence,
    },
    Locked {
        cells: NonConsecutiveCellSequence,
        values: NonConsecutiveDigitSequence,
        region: RegionId,
        digit: Digit,
    },
}

/// Compact result of one of Java's four direct NC producers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NonConsecutiveHint {
    geometry: NonConsecutiveGeometry,
    kind: NonConsecutiveHintKind,
    removals: CandidateRemovals,
}

impl NonConsecutiveHint {
    #[must_use]
    pub const fn geometry(&self) -> NonConsecutiveGeometry {
        self.geometry
    }

    #[must_use]
    pub const fn kind(&self) -> NonConsecutiveHintKind {
        self.kind
    }

    #[must_use]
    pub const fn rating_tenths(&self) -> u8 {
        match self.kind {
            NonConsecutiveHintKind::ForcingCell { .. } => 24,
            NonConsecutiveHintKind::Locked { .. } => 25,
        }
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self.kind {
            NonConsecutiveHintKind::ForcingCell { .. } => "Non-Consecutive Forcing Cell",
            NonConsecutiveHintKind::Locked { .. } => "Locked Non Consecutive",
        }
    }

    #[must_use]
    pub const fn short_name(&self) -> &'static str {
        match self.kind {
            NonConsecutiveHintKind::ForcingCell { .. } => "kNC",
            NonConsecutiveHintKind::Locked { .. } => "lNC",
        }
    }

    #[must_use]
    pub fn removals(&self) -> &CandidateRemovals {
        &self.removals
    }

    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        NonConsecutiveGeometry,
        NonConsecutiveHintKind,
        CandidateRemovals,
    ) {
        (self.geometry, self.kind, self.removals)
    }

    pub fn apply(&self, grid: &mut Grid) {
        self.removals.apply(grid);
    }

    /// Exact legacy `Hint.toString()` text (without the technique prefix).
    #[must_use]
    pub fn description(&self) -> String {
        match self.kind {
            NonConsecutiveHintKind::ForcingCell { cell, values } => {
                format!("Cell {cell} on value(s) {}", digit_list(values))
            }
            NonConsecutiveHintKind::Locked {
                cells,
                values,
                digit,
                ..
            } => {
                let label = if cells.len() == 1 { "Cell" } else { "Cells" };
                let cell_list = cells
                    .iter()
                    .map(|cell| cell.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "{digit}: {label} {cell_list} on value(s) {}",
                    digit_list(values)
                )
            }
        }
    }
}

/// Find the first Java-ordered orthogonal NC forcing-cell hint.
#[must_use]
pub fn find_forcing_cell_non_consecutive(grid: &Grid) -> Option<NonConsecutiveHint> {
    let mode = grid.topology().config().non_consecutive;
    matches!(
        mode,
        NonConsecutiveMode::Orthogonal | NonConsecutiveMode::OrthogonalCyclic
    )
    .then(|| find_forcing_cell(grid, NonConsecutiveGeometry::Orthogonal))
    .flatten()
}

/// Find the first Java-ordered orthogonal locked-NC hint.
#[must_use]
pub fn find_locked_non_consecutive(grid: &Grid) -> Option<NonConsecutiveHint> {
    let mode = grid.topology().config().non_consecutive;
    matches!(
        mode,
        NonConsecutiveMode::Orthogonal | NonConsecutiveMode::OrthogonalCyclic
    )
    .then(|| find_locked(grid, NonConsecutiveGeometry::Orthogonal))
    .flatten()
}

/// Find the first Java-ordered diagonal/Ferz NC forcing-cell hint.
#[must_use]
pub fn find_forcing_cell_ferz_non_consecutive(grid: &Grid) -> Option<NonConsecutiveHint> {
    let mode = grid.topology().config().non_consecutive;
    matches!(
        mode,
        NonConsecutiveMode::Diagonal | NonConsecutiveMode::DiagonalCyclic
    )
    .then(|| find_forcing_cell(grid, NonConsecutiveGeometry::Ferz))
    .flatten()
}

/// Find the first Java-ordered diagonal/Ferz locked-NC hint.
#[must_use]
pub fn find_locked_ferz_non_consecutive(grid: &Grid) -> Option<NonConsecutiveHint> {
    let mode = grid.topology().config().non_consecutive;
    matches!(
        mode,
        NonConsecutiveMode::Diagonal | NonConsecutiveMode::DiagonalCyclic
    )
    .then(|| find_locked(grid, NonConsecutiveGeometry::Ferz))
    .flatten()
}

fn find_forcing_cell(grid: &Grid, geometry: NonConsecutiveGeometry) -> Option<NonConsecutiveHint> {
    let cyclic = grid.topology().config().non_consecutive.is_cyclic();
    for raw_cell in 0_u8..81 {
        let cell = CellId::new(raw_cell).expect("cell loop");
        let candidates = grid.candidates(cell);
        let cardinality = candidates.count();
        if cardinality != 2 && cardinality != 3 {
            continue;
        }

        let first = candidates.iter().next().expect("nonempty NC cell");
        let last = candidates.iter().last().expect("nonempty NC cell");
        let range = last.get() - first.get();

        if range == cardinality as u8 - 1 || cyclic && range == 8 {
            if cardinality == 2 {
                let values = NonConsecutiveDigitSequence::two(first, last);
                if let Some(hint) = forcing_hint(grid, geometry, cell, values, false) {
                    return Some(hint);
                }
            } else if cyclic && range == 8 {
                let middle = candidates.iter().nth(1).expect("three candidates");
                if middle.get() == first.get() + 1 {
                    let values = NonConsecutiveDigitSequence::one(first);
                    if let Some(hint) = forcing_hint(grid, geometry, cell, values, true) {
                        return Some(hint);
                    }
                }
                if middle.get() == last.get() - 1 {
                    let values = NonConsecutiveDigitSequence::one(last);
                    if let Some(hint) = forcing_hint(grid, geometry, cell, values, true) {
                        return Some(hint);
                    }
                }
                // Java continues to the next cell even when neither cyclic
                // endpoint pattern produced a worth hint.
                continue;
            } else {
                let middle = Digit::new(first.get() + 1).expect("middle NC digit");
                let values = NonConsecutiveDigitSequence::one(middle);
                if let Some(hint) = forcing_hint(grid, geometry, cell, values, true) {
                    return Some(hint);
                }
            }
        }

        if range == 2 || cyclic && range == 7 && cardinality == 2 {
            let middle = if cyclic && range == 7 {
                if candidates.contains(Digit::new(8).expect("digit 8")) {
                    Digit::new(9).expect("digit 9")
                } else {
                    Digit::new(1).expect("digit 1")
                }
            } else {
                Digit::new(first.get() + 1).expect("middle NC digit")
            };
            let values = NonConsecutiveDigitSequence::one(middle);
            if let Some(hint) = forcing_hint(grid, geometry, cell, values, cardinality == 3) {
                return Some(hint);
            }
        }
    }
    None
}

fn forcing_hint(
    grid: &Grid,
    geometry: NonConsecutiveGeometry,
    forcing_cell: CellId,
    values: NonConsecutiveDigitSequence,
    three_candidate_pattern: bool,
) -> Option<NonConsecutiveHint> {
    let topology = grid.topology();
    let toroidal = topology.config().toroidal;
    let raw_neighbors = match geometry {
        NonConsecutiveGeometry::Orthogonal => topology.orthogonal_neighbors(forcing_cell, toroidal),
        // Released Java quirk: the two-value toroidal Ferz producer reads
        // the toroidal Wazir table, while every single-value branch reads
        // the toroidal Ferz table.
        NonConsecutiveGeometry::Ferz if values.len() == 2 && toroidal => {
            topology.orthogonal_neighbors(forcing_cell, true)
        }
        NonConsecutiveGeometry::Ferz => topology.diagonal_neighbors(forcing_cell, toroidal),
    };
    let mut victims = mask_of(raw_neighbors);

    // Ferz double-consecutive hints always retain visible cells. Ferz
    // single-value hints do so only for a three-candidate forcing cell.
    if geometry == NonConsecutiveGeometry::Ferz && (values.len() == 2 || three_candidate_pattern) {
        victims = victims.intersect(topology.visible_mask(forcing_cell));
    }

    let removals = removals_from_victims(grid, victims, values.mask());
    (!removals.is_empty()).then_some(NonConsecutiveHint {
        geometry,
        kind: NonConsecutiveHintKind::ForcingCell {
            cell: forcing_cell,
            values,
        },
        removals,
    })
}

fn find_locked(grid: &Grid, geometry: NonConsecutiveGeometry) -> Option<NonConsecutiveHint> {
    let topology = grid.topology();
    let variant = topology.config();
    let cyclic = variant.non_consecutive.is_cyclic();
    let lowest_type = usize::from(!variant.blocks);

    for raw_digit in 1_u8..=9 {
        let digit = Digit::new(raw_digit).expect("digit loop");
        for type_index in (lowest_type..=4).rev() {
            if type_index == 3 || type_index == 4 && !variant.windows {
                continue;
            }
            let region_count = if type_index == 4 {
                topology.region_count(type_index).min(4)
            } else {
                topology.region_count(type_index)
            };
            let max_cardinality = if type_index == 0 || type_index == 4 {
                5
            } else {
                3
            };

            for region_index in 0..region_count {
                let region =
                    RegionId::new(type_index as u8, region_index as u8).expect("legacy NC region");
                let positions = grid.region_candidate_positions(region, digit);
                if positions.is_empty() || positions.count() > max_cardinality {
                    continue;
                }
                let cells = NonConsecutiveCellSequence::from_region_cells(
                    topology.region_cells(region),
                    positions.iter(),
                );
                let values = adjacent_values(digit, cyclic);
                if let Some(hint) = locked_hint(grid, geometry, cells, values, region, digit) {
                    return Some(hint);
                }
            }
        }
    }
    None
}

fn locked_hint(
    grid: &Grid,
    geometry: NonConsecutiveGeometry,
    cells: NonConsecutiveCellSequence,
    values: NonConsecutiveDigitSequence,
    region: RegionId,
    digit: Digit,
) -> Option<NonConsecutiveHint> {
    let topology = grid.topology();
    let toroidal = topology.config().toroidal;
    let mut cells_iter = cells.iter();
    let first = cells_iter.next().expect("nonempty locked NC cells");
    let mut victims = neighbor_mask(topology, geometry, first, toroidal);
    victims.insert(first);
    for cell in cells_iter {
        let mut current = neighbor_mask(topology, geometry, cell, toroidal);
        current.insert(cell);
        victims = victims.intersect(current);
        if victims.is_empty() {
            return None;
        }
    }

    let removals = removals_from_victims(grid, victims, values.mask());
    (!removals.is_empty()).then_some(NonConsecutiveHint {
        geometry,
        kind: NonConsecutiveHintKind::Locked {
            cells,
            values,
            region,
            digit,
        },
        removals,
    })
}

fn adjacent_values(digit: Digit, cyclic: bool) -> NonConsecutiveDigitSequence {
    let raw = digit.get();
    let previous = if raw > 1 {
        Some(Digit::new(raw - 1).expect("previous digit"))
    } else if cyclic {
        Some(Digit::new(9).expect("wrapped previous digit"))
    } else {
        None
    };
    let next = if raw < 9 {
        Some(Digit::new(raw + 1).expect("next digit"))
    } else if cyclic {
        Some(Digit::new(1).expect("wrapped next digit"))
    } else {
        None
    };
    match (previous, next) {
        (Some(previous), Some(next)) => NonConsecutiveDigitSequence::two(previous, next),
        (Some(only), None) | (None, Some(only)) => NonConsecutiveDigitSequence::one(only),
        (None, None) => unreachable!("digits 1 through 9 have an adjacent digit"),
    }
}

fn removals_from_victims(
    grid: &Grid,
    victims: CellMask,
    values: CandidateMask,
) -> CandidateRemovals {
    let mut builder = CandidateRemovalsBuilder::with_capacity(victims.count() as usize);
    for victim in victims.iter() {
        builder.add(victim, grid.candidates(victim).intersect(values));
    }
    builder.build()
}

fn neighbor_mask(
    topology: &sukaku_forge_core::ConstraintTopology,
    geometry: NonConsecutiveGeometry,
    cell: CellId,
    toroidal: bool,
) -> CellMask {
    let raw = match geometry {
        NonConsecutiveGeometry::Orthogonal => topology.orthogonal_neighbors(cell, toroidal),
        NonConsecutiveGeometry::Ferz => topology.diagonal_neighbors(cell, toroidal),
    };
    mask_of(raw)
}

fn mask_of(raw_cells: &[u8]) -> CellMask {
    let mut result = CellMask::EMPTY;
    for &raw in raw_cells {
        result.insert(CellId::new(raw).expect("topology cell"));
    }
    result
}

fn digit_list(values: NonConsecutiveDigitSequence) -> String {
    values
        .iter()
        .map(|digit| digit.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{
        ConstraintTopology, Digit, Grid, NonConsecutiveMode, Puzzle, RegionId, VariantConfig,
    };

    use super::{
        NonConsecutiveGeometry, NonConsecutiveHintKind, find_forcing_cell_ferz_non_consecutive,
        find_forcing_cell_non_consecutive, find_locked_ferz_non_consecutive,
        find_locked_non_consecutive,
    };

    fn sparse_snapshot(
        mode: NonConsecutiveMode,
        toroidal: bool,
        entries: &[(usize, &str)],
    ) -> Grid {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = ['.'; 729];
        for &(cell, candidates) in entries {
            for digit in candidates.bytes() {
                display[cell * 9 + usize::from(digit - b'1')] = char::from(digit);
            }
        }
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig {
                toroidal,
                non_consecutive: mode,
                forbidden_pairs: true,
                ..VariantConfig::default()
            })),
            &values,
            &candidates,
        )
        .unwrap()
    }

    fn removal_entries(hint: &super::NonConsecutiveHint) -> Vec<(u8, u16)> {
        hint.removals()
            .iter()
            .map(|removal| (removal.cell().raw(), removal.digits().bits()))
            .collect()
    }

    fn puzzle_grid(mode: NonConsecutiveMode, puzzle: &str) -> Grid {
        Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig {
                non_consecutive: mode,
                forbidden_pairs: true,
                ..VariantConfig::default()
            })),
            &Puzzle::parse(puzzle).unwrap(),
        )
    }

    fn direct_path(mut grid: Grid, ferz: bool) -> (Vec<String>, Grid) {
        let mut descriptions = Vec::new();
        loop {
            let hint = if ferz {
                find_forcing_cell_ferz_non_consecutive(&grid)
                    .or_else(|| find_locked_ferz_non_consecutive(&grid))
            } else {
                find_forcing_cell_non_consecutive(&grid)
                    .or_else(|| find_locked_non_consecutive(&grid))
            };
            let Some(hint) = hint else {
                break;
            };
            descriptions.push(hint.description());
            hint.apply(&mut grid);
            assert!(descriptions.len() < 100, "NC direct path must terminate");
        }
        (descriptions, grid)
    }

    fn fnv1a64(text: &str) -> u64 {
        text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    #[test]
    fn orthogonal_forcing_cell_covers_double_triple_middle_and_cyclic_edges() {
        let double = sparse_snapshot(
            NonConsecutiveMode::Orthogonal,
            false,
            &[(0, "45"), (1, "45"), (9, "45")],
        );
        let hint = find_forcing_cell_non_consecutive(&double).expect("double consecutive");
        assert_eq!(hint.geometry(), NonConsecutiveGeometry::Orthogonal);
        assert_eq!(hint.description(), "Cell r1c1 on value(s) 4,5");
        assert_eq!(removal_entries(&hint), vec![(1, 0x30), (9, 0x30)]);

        let triple = sparse_snapshot(
            NonConsecutiveMode::Orthogonal,
            false,
            &[(0, "456"), (1, "5")],
        );
        assert_eq!(
            find_forcing_cell_non_consecutive(&triple)
                .expect("triple consecutive")
                .description(),
            "Cell r1c1 on value(s) 5"
        );

        let middle = sparse_snapshot(
            NonConsecutiveMode::Orthogonal,
            false,
            &[(0, "46"), (1, "5")],
        );
        assert_eq!(
            find_forcing_cell_non_consecutive(&middle)
                .expect("double middle")
                .description(),
            "Cell r1c1 on value(s) 5"
        );

        for (forcing, victim, description) in [
            ("19", "19", "Cell r1c1 on value(s) 1,9"),
            ("18", "9", "Cell r1c1 on value(s) 9"),
            ("29", "1", "Cell r1c1 on value(s) 1"),
        ] {
            let grid = sparse_snapshot(
                NonConsecutiveMode::OrthogonalCyclic,
                false,
                &[(0, forcing), (1, victim)],
            );
            assert_eq!(
                find_forcing_cell_non_consecutive(&grid)
                    .expect("cyclic forcing-cell edge")
                    .description(),
                description
            );
        }
    }

    #[test]
    fn ferz_forcing_cell_preserves_visibility_and_toroidal_wazir_quirks() {
        let regular_double = sparse_snapshot(
            NonConsecutiveMode::Diagonal,
            false,
            &[(3, "45"), (11, "45"), (13, "45")],
        );
        let hint = find_forcing_cell_ferz_non_consecutive(&regular_double)
            .expect("regular Ferz double consecutive");
        // r2c3 is diagonal but does not see the forcing cell; r2c5 does.
        assert_eq!(removal_entries(&hint), vec![(13, 0x30)]);

        let nonvisible_middle =
            sparse_snapshot(NonConsecutiveMode::Diagonal, false, &[(3, "46"), (11, "5")]);
        assert_eq!(
            removal_entries(
                &find_forcing_cell_ferz_non_consecutive(&nonvisible_middle)
                    .expect("Ferz double-middle does not retain buddies")
            ),
            vec![(11, 1 << 5)]
        );

        let toroidal_double = sparse_snapshot(
            NonConsecutiveMode::DiagonalCyclic,
            true,
            &[(0, "19"), (8, "19"), (72, "19"), (71, "19"), (80, "19")],
        );
        let hint = find_forcing_cell_ferz_non_consecutive(&toroidal_double)
            .expect("toroidal Ferz Wazir quirk");
        assert_eq!(removal_entries(&hint), vec![(8, 0x202), (72, 0x202)]);
    }

    #[test]
    fn locked_search_uses_digit_then_column_order_and_keeps_cyclic_value_order() {
        let orthogonal = sparse_snapshot(
            NonConsecutiveMode::Orthogonal,
            false,
            &[(0, "12"), (9, "12")],
        );
        let hint = find_locked_non_consecutive(&orthogonal).expect("orthogonal locked NC");
        assert_eq!(hint.description(), "1: Cells r1c1,r2c1 on value(s) 2");
        assert_eq!(removal_entries(&hint), vec![(0, 1 << 2), (9, 1 << 2)]);
        let NonConsecutiveHintKind::Locked { region, .. } = hint.kind() else {
            panic!("locked hint");
        };
        assert_eq!((region.type_index(), region.region_index()), (2, 0));

        let ferz_cyclic = sparse_snapshot(
            NonConsecutiveMode::DiagonalCyclic,
            false,
            &[(0, "89"), (10, "1")],
        );
        let hint = super::locked_hint(
            &ferz_cyclic,
            NonConsecutiveGeometry::Ferz,
            super::NonConsecutiveCellSequence {
                cells: [0; 5],
                len: 1,
            },
            super::adjacent_values(Digit::new(9).unwrap(), true),
            RegionId::new(2, 0).unwrap(),
            Digit::new(9).unwrap(),
        )
        .expect("cyclic locked Ferz NC");
        assert_eq!(hint.description(), "9: Cell r1c1 on value(s) 8,1");
        assert_eq!(removal_entries(&hint), vec![(0, 1 << 8), (10, 1 << 1)]);
    }

    #[test]
    fn mode_specific_entry_points_reject_the_other_geometry() {
        let orthogonal = sparse_snapshot(
            NonConsecutiveMode::Orthogonal,
            false,
            &[(40, "45"), (41, "45")],
        );
        assert!(find_forcing_cell_ferz_non_consecutive(&orthogonal).is_none());
        assert!(find_locked_ferz_non_consecutive(&orthogonal).is_none());

        let ferz = sparse_snapshot(
            NonConsecutiveMode::Diagonal,
            false,
            &[(40, "45"), (50, "45")],
        );
        assert!(find_forcing_cell_non_consecutive(&ferz).is_none());
        assert!(find_locked_non_consecutive(&ferz).is_none());
    }

    #[test]
    fn orthogonal_java_oracle_path_matches_all_29_direct_hints() {
        let grid = puzzle_grid(
            NonConsecutiveMode::Orthogonal,
            "005279460460500092002040100006080070079020003000057006351092004000130000027000301",
        );
        let (descriptions, final_grid) = direct_path(grid, false);
        assert_eq!(
            descriptions,
            [
                "Cell r3c1 on value(s) 8",
                "Cell r4c4 on value(s) 3,4",
                "Cell r6c3 on value(s) 3,4",
                "Cell r7c7 on value(s) 7",
                "Cell r8c2 on value(s) 8,9",
                "Cell r8c9 on value(s) 8",
                "1: Cells r5c8,r6c8 on value(s) 2",
                "2: Cell r4c1 on value(s) 1,3",
                "2: Cell r6c7 on value(s) 1,3",
                "3: Cell r6c3 on value(s) 2,4",
                "3: Cell r4c4 on value(s) 2,4",
                "4: Cell r4c2 on value(s) 3,5",
                "4: Cells r4c6,r5c6 on value(s) 3,5",
                "4: Cell r6c8 on value(s) 3,5",
                "4: Cell r5c6 on value(s) 3,5",
                "5: Cells r8c6,r9c6 on value(s) 4,6",
                "5: Cells r8c6,r8c7,r8c8 on value(s) 4,6",
                "6: Cell r9c5 on value(s) 5,7",
                "6: Cell r7c7 on value(s) 5,7",
                "Cell r8c7 on value(s) 8,9",
                "5: Cell r4c7 on value(s) 4,6",
                "7: Cell r3c1 on value(s) 6,8",
                "7: Cell r7c4 on value(s) 6,8",
                "7: Cell r8c9 on value(s) 6,8",
                "8: Cell r5c7 on value(s) 7,9",
                "9: Cell r8c7 on value(s) 8",
                "8: Cell r8c2 on value(s) 7,9",
                "9: Cell r4c9 on value(s) 8",
                "9: Cell r9c1 on value(s) 8",
            ]
        );
        assert_eq!(
            fnv1a64(&final_grid.candidate_string()),
            0xabe2_0531_3859_8ceb
        );
    }

    #[test]
    fn cyclic_ferz_java_oracle_path_matches_all_19_direct_hints() {
        let grid = puzzle_grid(
            NonConsecutiveMode::DiagonalCyclic,
            "000450780006700000789103000200000801060890030800200000345070010600900340010340600",
        );
        let (descriptions, final_grid) = direct_path(grid, true);
        assert_eq!(
            descriptions,
            [
                "Cell r1c3 on value(s) 2",
                "Cell r3c8 on value(s) 5,6",
                "Cell r7c7 on value(s) 1",
                "1: Cell r2c7 on value(s) 9,2",
                "2: Cell r1c2 on value(s) 1,3",
                "2: Cell r2c8 on value(s) 1,3",
                "3: Cells r4c5,r6c5 on value(s) 2,4",
                "3: Cell r2c9 on value(s) 2,4",
                "3: Cell r1c3 on value(s) 2,4",
                "4: Cells r4c6,r6c6 on value(s) 3,5",
                "4: Cells r3c7,r5c7 on value(s) 3,5",
                "4: Cell r2c1 on value(s) 3,5",
                "4: Cells r3c7,r3c9 on value(s) 3,5",
                "4: Cells r5c7,r5c9 on value(s) 3,5",
                "5: Cell r5c1 on value(s) 4,6",
                "5: Cell r4c4 on value(s) 4,6",
                "6: Cell r7c4 on value(s) 5,7",
                "6: Cells r4c5,r6c5 on value(s) 5,7",
                "9: Cells r4c2,r6c2 on value(s) 8,1",
            ]
        );
        assert_eq!(
            fnv1a64(&final_grid.candidate_string()),
            0x486c_a6af_4d0f_a6bd
        );
    }
}
