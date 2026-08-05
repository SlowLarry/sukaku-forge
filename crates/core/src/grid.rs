use std::array;
use std::fmt;
use std::sync::Arc;

use crate::puzzle::candidate_string;
use crate::{
    CandidateMask, CellId, CellMask, ConstraintTopology, Digit, PositionMask, Puzzle,
    REGION_TYPE_COUNT, RegionId,
};

type RegionPositionCache = [Vec<[PositionMask; 10]>; REGION_TYPE_COUNT];

/// Mutable puzzle state bound to one immutable topology.
#[derive(Clone, Debug)]
pub struct Grid {
    topology: Arc<ConstraintTopology>,
    values: [u8; CellId::COUNT],
    candidates: [CandidateMask; CellId::COUNT],
    candidate_cells: [CellMask; 10],
    givens: CellMask,
    region_positions: RegionPositionCache,
}

impl Grid {
    #[must_use]
    pub fn from_puzzle(topology: Arc<ConstraintTopology>, puzzle: &Puzzle) -> Self {
        let (values, candidates, givens) = match puzzle {
            Puzzle::Values { values, givens } => {
                let candidates = array::from_fn(|index| {
                    if values[index] == 0 {
                        CandidateMask::ALL
                    } else {
                        CandidateMask::EMPTY
                    }
                });
                (*values, candidates, *givens)
            }
            Puzzle::Pencilmarks(candidates) => ([0; CellId::COUNT], *candidates, CellMask::EMPTY),
        };
        let region_positions = create_region_cache(&topology);
        let mut grid = Self {
            topology,
            values,
            candidates,
            candidate_cells: [CellMask::EMPTY; 10],
            givens,
            region_positions,
        };
        grid.rebuild_region_positions();
        if matches!(puzzle, Puzzle::Values { .. }) {
            for index in 0_u8..81 {
                let value = grid.values[usize::from(index)];
                if let Some(digit) = Digit::new(value) {
                    grid.remove_conflicting_candidates(
                        CellId::new(index).expect("cell index loop"),
                        digit,
                    );
                }
            }
        }
        grid
    }

    /// Restore an exact Java trace snapshot without recomputing candidates.
    ///
    /// The value puzzle distinguishes solved cells from unresolved singleton
    /// pencilmarks. Candidate slots belonging to solved cells are deliberately
    /// discarded from the mutable candidate state.
    pub fn from_snapshot(
        topology: Arc<ConstraintTopology>,
        values_puzzle: &Puzzle,
        candidates_puzzle: &Puzzle,
    ) -> Result<Self, GridStateError> {
        let Puzzle::Values { values, givens } = values_puzzle else {
            return Err(GridStateError::ExpectedValues);
        };
        let Puzzle::Pencilmarks(display_candidates) = candidates_puzzle else {
            return Err(GridStateError::ExpectedPencilmarks);
        };
        let candidates = array::from_fn(|index| {
            if values[index] == 0 {
                display_candidates[index]
            } else {
                CandidateMask::EMPTY
            }
        });
        let region_positions = create_region_cache(&topology);
        let mut grid = Self {
            topology,
            values: *values,
            candidates,
            candidate_cells: [CellMask::EMPTY; 10],
            givens: *givens,
            region_positions,
        };
        grid.rebuild_region_positions();
        Ok(grid)
    }

    #[must_use]
    pub fn topology(&self) -> &Arc<ConstraintTopology> {
        &self.topology
    }

    #[must_use]
    pub const fn value(&self, cell: CellId) -> u8 {
        self.values[cell.index()]
    }

    #[must_use]
    pub const fn candidates(&self, cell: CellId) -> CandidateMask {
        self.candidates[cell.index()]
    }

    #[must_use]
    pub const fn candidate_cells(&self, digit: Digit) -> CellMask {
        self.candidate_cells[digit.get() as usize]
    }

    #[must_use]
    pub const fn givens(&self) -> CellMask {
        self.givens
    }

    pub fn set_candidates(&mut self, cell: CellId, new_mask: CandidateMask) {
        let old_mask = self.candidates[cell.index()];
        if old_mask == new_mask {
            return;
        }
        self.candidates[cell.index()] = new_mask;
        self.update_region_positions(cell, old_mask, new_mask);
    }

    pub fn remove_candidate(&mut self, cell: CellId, digit: Digit) -> bool {
        let old_mask = self.candidates(cell);
        if !old_mask.contains(digit) {
            return false;
        }
        self.set_candidates(cell, old_mask.without(CandidateMask::of(digit)));
        true
    }

    pub fn remove_candidates(&mut self, cell: CellId, digits: CandidateMask) -> CandidateMask {
        let old_mask = self.candidates(cell);
        let removed = old_mask.intersect(digits);
        if !removed.is_empty() {
            self.set_candidates(cell, old_mask.without(digits));
        }
        removed
    }

    pub fn place(&mut self, cell: CellId, digit: Digit) {
        self.values[cell.index()] = digit.get();
        self.set_candidates(cell, CandidateMask::EMPTY);
        self.remove_conflicting_candidates(cell, digit);
    }

    #[must_use]
    pub fn region_candidate_positions(&self, region: RegionId, digit: Digit) -> PositionMask {
        if self.topology.is_region_type_active(region.type_index()) {
            return self.region_positions[region.type_index()][region.region_index()]
                [usize::from(digit.get())];
        }
        self.scan_region_candidate_positions(region, digit)
    }

    #[must_use]
    pub fn values_string(&self) -> String {
        self.values
            .iter()
            .map(|value| {
                if *value == 0 {
                    '.'
                } else {
                    char::from(b'0' + value)
                }
            })
            .collect()
    }

    /// Java-compatible 729-character view, including each solved value in its slot.
    #[must_use]
    pub fn candidate_string(&self) -> String {
        let display_masks = array::from_fn(|index| {
            Digit::new(self.values[index]).map_or(self.candidates[index], CandidateMask::of)
        });
        candidate_string(&display_masks)
    }

    #[must_use]
    pub fn is_solved(&self) -> bool {
        self.values.iter().all(|value| *value != 0)
    }

    fn remove_conflicting_candidates(&mut self, source: CellId, digit: Digit) {
        let topology = Arc::clone(&self.topology);
        for &raw_peer in topology.visible_peers(source) {
            self.remove_candidate(CellId::new(raw_peer).expect("topology peer"), digit);
        }

        let Some(neighbors) = topology.forbidden_pair_neighbors(source) else {
            return;
        };
        let mode = topology.config().non_consecutive;
        let value = digit.get();
        for &raw_neighbor in neighbors {
            let neighbor = CellId::new(raw_neighbor).expect("topology neighbor");
            if mode.is_cyclic() || value < 9 {
                let next = if value == 9 { 1 } else { value + 1 };
                self.remove_candidate(neighbor, Digit::new(next).expect("adjacent digit"));
            }
            if mode.is_cyclic() || value > 1 {
                let previous = if value == 1 { 9 } else { value - 1 };
                self.remove_candidate(neighbor, Digit::new(previous).expect("adjacent digit"));
            }
        }
    }

    fn rebuild_region_positions(&mut self) {
        self.candidate_cells = [CellMask::EMPTY; 10];
        for family in &mut self.region_positions {
            for region in family {
                *region = [PositionMask::EMPTY; 10];
            }
        }
        for raw_cell in 0_u8..81 {
            let cell = CellId::new(raw_cell).expect("cell index loop");
            self.update_region_positions(cell, CandidateMask::EMPTY, self.candidates(cell));
        }
    }

    fn update_region_positions(
        &mut self,
        cell: CellId,
        old_mask: CandidateMask,
        new_mask: CandidateMask,
    ) {
        let removed = old_mask.without(new_mask);
        let added = new_mask.without(old_mask);
        if removed.is_empty() && added.is_empty() {
            return;
        }
        for digit in removed.iter() {
            self.candidate_cells[usize::from(digit.get())].remove(cell);
        }
        for digit in added.iter() {
            self.candidate_cells[usize::from(digit.get())].insert(cell);
        }
        for type_index in 0..REGION_TYPE_COUNT {
            if !self.topology.is_region_type_active(type_index) {
                continue;
            }
            let Some(region_index) = self.topology.cell_region_index(cell, type_index) else {
                continue;
            };
            let position = self
                .topology
                .cell_position_in_region(cell, type_index)
                .expect("region membership has a position");
            let positions_by_digit =
                &mut self.region_positions[type_index][usize::from(region_index)];
            for digit in removed.iter() {
                positions_by_digit[usize::from(digit.get())].remove(position);
            }
            for digit in added.iter() {
                positions_by_digit[usize::from(digit.get())].insert(position);
            }
        }
    }

    fn scan_region_candidate_positions(&self, region: RegionId, digit: Digit) -> PositionMask {
        let mut result = PositionMask::EMPTY;
        for (position, &raw_cell) in self.topology.region_cells(region).iter().enumerate() {
            let cell = CellId::new(raw_cell).expect("region cell");
            if self.candidates(cell).contains(digit) {
                result.insert(position as u8);
            }
        }
        result
    }
}

/// A mismatched pair of value-grid and pencilmark snapshot inputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GridStateError {
    ExpectedValues,
    ExpectedPencilmarks,
}

impl fmt::Display for GridStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedValues => formatter.write_str("snapshot values must be an 81-cell grid"),
            Self::ExpectedPencilmarks => {
                formatter.write_str("snapshot candidates must be a 729-character pencilmark grid")
            }
        }
    }
}

impl std::error::Error for GridStateError {}

fn create_region_cache(topology: &ConstraintTopology) -> RegionPositionCache {
    array::from_fn(|type_index| {
        if topology.is_region_type_active(type_index) {
            vec![[PositionMask::EMPTY; 10]; topology.region_count(type_index)]
        } else {
            Vec::new()
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::Grid;
    use crate::{CellId, CellMask, ConstraintTopology, Digit, Puzzle, RegionId, VariantConfig};

    #[test]
    fn classic_rebuild_removes_row_column_and_block_candidates() {
        let puzzle = Puzzle::parse(
            "5................................................................................",
        )
        .unwrap();
        let grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &puzzle,
        );
        let five = Digit::new(5).unwrap();
        assert!(!grid.candidates(CellId::new(1).unwrap()).contains(five));
        assert!(!grid.candidates(CellId::new(9).unwrap()).contains(five));
        assert!(!grid.candidates(CellId::new(10).unwrap()).contains(five));
        assert!(grid.candidates(CellId::new(40).unwrap()).contains(five));
    }

    #[test]
    fn anti_knight_rebuild_removes_leaper_candidates() {
        let mut text = ['.'; 81];
        text[40] = '5';
        let puzzle = Puzzle::parse(&text.iter().collect::<String>()).unwrap();
        let grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            })),
            &puzzle,
        );
        let five = Digit::new(5).unwrap();
        for raw_peer in [51, 33, 47, 29, 59, 23, 57, 21] {
            assert!(
                !grid
                    .candidates(CellId::new(raw_peer).unwrap())
                    .contains(five)
            );
        }
    }

    #[test]
    fn region_position_cache_matches_independent_scan_after_mutations() {
        let puzzle = Puzzle::parse(
            ".................................................................................",
        )
        .unwrap();
        let mut grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            })),
            &puzzle,
        );
        grid.remove_candidate(CellId::new(0).unwrap(), Digit::new(1).unwrap());
        grid.place(CellId::new(40).unwrap(), Digit::new(5).unwrap());
        let copy = grid.clone();

        for type_index in copy.topology().active_region_types() {
            for region_index in 0..copy.topology().region_count(type_index) {
                let region = RegionId::new(type_index as u8, region_index as u8).unwrap();
                for value in 1_u8..=9 {
                    let digit = Digit::new(value).unwrap();
                    assert_eq!(
                        copy.region_candidate_positions(region, digit),
                        copy.scan_region_candidate_positions(region, digit)
                    );
                }
            }
        }
        for value in 1_u8..=9 {
            let digit = Digit::new(value).unwrap();
            let mut independent = CellMask::EMPTY;
            for raw_cell in 0_u8..81 {
                let cell = CellId::new(raw_cell).unwrap();
                if copy.candidates(cell).contains(digit) {
                    independent.insert(cell);
                }
            }
            assert_eq!(copy.candidate_cells(digit), independent);
        }
    }

    #[test]
    fn snapshot_keeps_unresolved_singletons_distinct_from_values() {
        let values = Puzzle::parse(
            "1................................................................................",
        )
        .unwrap();
        let mut display = ['.'; 729];
        display[0] = '1';
        display[9 + 4] = '5';
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        let grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &values,
            &candidates,
        )
        .unwrap();
        assert!(grid.candidates(CellId::new(0).unwrap()).is_empty());
        assert_eq!(
            grid.candidates(CellId::new(1).unwrap()).single(),
            Digit::new(5)
        );
        assert_eq!(grid.candidate_string(), display.iter().collect::<String>());
    }
}
