use std::array;
use std::fmt;
use std::sync::Arc;

use crate::puzzle::candidate_string;
use crate::{
    CandidateMask, CellId, CellMask, ConstraintTopology, Digit, PositionMask, Puzzle,
    REGION_TYPE_COUNT, RegionId, VariantConfig,
};

type RegionPositionCache = [Vec<[PositionMask; 10]>; REGION_TYPE_COUNT];

const CLASSIC_REGION_COUNT: usize = 27;

/// Region candidates for the three always-present Classic families.
///
/// The general cache mirrors the variant topology as an array of vectors. A
/// rating-only Classic grid never needs that indirection: its nine blocks,
/// rows, and columns have fixed identities and cell positions. Keeping the
/// specialized table behind an opt-in cache variant leaves the general Grid
/// constructor and all variant consumers on their established representation.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // Preserve the general cache's existing inline Vec array.
enum GridRegionPositions {
    General(RegionPositionCache),
    Classic(Box<[[PositionMask; 10]; CLASSIC_REGION_COUNT]>),
}

/// Mutable puzzle state bound to one immutable topology.
#[derive(Debug)]
pub struct Grid {
    topology: Arc<ConstraintTopology>,
    values: [u8; CellId::COUNT],
    candidates: [CandidateMask; CellId::COUNT],
    candidate_cells: [CellMask; 10],
    givens: CellMask,
    region_positions: GridRegionPositions,
}

impl Clone for Grid {
    fn clone(&self) -> Self {
        Self {
            topology: Arc::clone(&self.topology),
            values: self.values,
            candidates: self.candidates,
            candidate_cells: self.candidate_cells,
            givens: self.givens,
            region_positions: self.region_positions.clone(),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        Arc::clone_from(&mut self.topology, &source.topology);
        self.values = source.values;
        self.candidates = source.candidates;
        self.candidate_cells = source.candidate_cells;
        self.givens = source.givens;
        match (&mut self.region_positions, &source.region_positions) {
            (GridRegionPositions::General(target), GridRegionPositions::General(source)) => {
                for (target_family, source_family) in target.iter_mut().zip(source) {
                    target_family.clone_from(source_family);
                }
            }
            (GridRegionPositions::Classic(target), GridRegionPositions::Classic(source)) => {
                // Copy into the existing Box. Long-lived headless chain
                // sessions can reset a working grid without allocating once
                // both grids use the fixed Classic representation.
                **target = **source;
            }
            (target, source) => {
                // A representation change is uncommon and cannot reuse the
                // old storage. Clone the exact source representation rather
                // than assuming that equal topology implies equal cache kind.
                *target = source.clone();
            }
        }
    }
}

impl Grid {
    #[must_use]
    pub fn from_puzzle(topology: Arc<ConstraintTopology>, puzzle: &Puzzle) -> Self {
        Self::from_puzzle_with_region_positions(topology, puzzle, false)
    }

    /// Construct a Classic grid with fixed block/row/column candidate caches.
    ///
    /// This is an optimization seam for focused 9x9 Classic products. A caller
    /// which accidentally supplies a variant topology safely falls back to the
    /// general representation, so variant semantics cannot change.
    #[must_use]
    #[doc(hidden)]
    pub fn from_classic_puzzle(topology: Arc<ConstraintTopology>, puzzle: &Puzzle) -> Self {
        let use_classic_positions = topology.config() == VariantConfig::default();
        Self::from_puzzle_with_region_positions(topology, puzzle, use_classic_positions)
    }

    fn from_puzzle_with_region_positions(
        topology: Arc<ConstraintTopology>,
        puzzle: &Puzzle,
        use_classic_positions: bool,
    ) -> Self {
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
        let region_positions = if use_classic_positions {
            GridRegionPositions::Classic(Box::new(
                [[PositionMask::EMPTY; 10]; CLASSIC_REGION_COUNT],
            ))
        } else {
            GridRegionPositions::General(create_region_cache(&topology))
        };
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
            region_positions: GridRegionPositions::General(region_positions),
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
            return match &self.region_positions {
                GridRegionPositions::General(positions) => {
                    positions[region.type_index()][region.region_index()][usize::from(digit.get())]
                }
                GridRegionPositions::Classic(positions) => {
                    debug_assert!(region.type_index() < 3);
                    positions[classic_region_slot(region.type_index(), region.region_index())]
                        [usize::from(digit.get())]
                }
            };
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
        match &mut self.region_positions {
            GridRegionPositions::General(positions) => {
                for family in positions {
                    for region in family {
                        *region = [PositionMask::EMPTY; 10];
                    }
                }
            }
            GridRegionPositions::Classic(positions) => {
                positions.fill([PositionMask::EMPTY; 10]);
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
        match &mut self.region_positions {
            GridRegionPositions::General(positions) => {
                for (type_index, family) in positions.iter_mut().enumerate() {
                    if !self.topology.is_region_type_active(type_index) {
                        continue;
                    }
                    let Some(region_index) = self.topology.cell_region_index(cell, type_index)
                    else {
                        continue;
                    };
                    let position = self
                        .topology
                        .cell_position_in_region(cell, type_index)
                        .expect("region membership has a position");
                    let positions_by_digit = &mut family[usize::from(region_index)];
                    for digit in removed.iter() {
                        positions_by_digit[usize::from(digit.get())].remove(position);
                    }
                    for digit in added.iter() {
                        positions_by_digit[usize::from(digit.get())].insert(position);
                    }
                }
            }
            GridRegionPositions::Classic(positions) => {
                update_classic_region_positions(positions, cell, removed, false);
                update_classic_region_positions(positions, cell, added, true);
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

#[inline]
const fn classic_region_slot(type_index: usize, region_index: usize) -> usize {
    type_index * 9 + region_index
}

#[inline]
fn update_classic_region_positions(
    positions: &mut [[PositionMask; 10]; CLASSIC_REGION_COUNT],
    cell: CellId,
    digits: CandidateMask,
    add: bool,
) {
    let row = usize::from(cell.row());
    let column = usize::from(cell.column());
    let block = row / 3 * 3 + column / 3;
    let block_slot = classic_region_slot(0, block);
    let block_position = ((row % 3) * 3 + column % 3) as u8;
    let row_slot = classic_region_slot(1, row);
    let row_position = column as u8;
    let column_slot = classic_region_slot(2, column);
    let column_position = row as u8;
    for digit in digits.iter() {
        let digit_index = usize::from(digit.get());
        if add {
            positions[block_slot][digit_index].insert(block_position);
            positions[row_slot][digit_index].insert(row_position);
            positions[column_slot][digit_index].insert(column_position);
        } else {
            positions[block_slot][digit_index].remove(block_position);
            positions[row_slot][digit_index].remove(row_position);
            positions[column_slot][digit_index].remove(column_position);
        }
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

    use super::{Grid, GridRegionPositions};
    use crate::{
        CandidateMask, CellId, CellMask, ConstraintTopology, Digit, Puzzle, REGION_TYPE_COUNT,
        RegionId, VariantConfig,
    };

    fn assert_grid_caches_equal(left: &Grid, right: &Grid) {
        assert_eq!(left.values_string(), right.values_string());
        assert_eq!(left.candidate_string(), right.candidate_string());
        assert_eq!(left.givens(), right.givens());
        for raw_cell in 0_u8..81 {
            let cell = CellId::new(raw_cell).unwrap();
            assert_eq!(left.value(cell), right.value(cell), "value at {cell}");
            assert_eq!(
                left.candidates(cell),
                right.candidates(cell),
                "candidates at {cell}"
            );
        }
        for value in 1_u8..=9 {
            let digit = Digit::new(value).unwrap();
            assert_eq!(
                left.candidate_cells(digit),
                right.candidate_cells(digit),
                "candidate cells for {digit}"
            );
            for type_index in 0..3 {
                for region_index in 0_u8..9 {
                    let region = RegionId::new(type_index, region_index).unwrap();
                    assert_eq!(
                        left.region_candidate_positions(region, digit),
                        right.region_candidate_positions(region, digit),
                        "candidate positions for family {type_index}, region {region_index}, digit {digit}"
                    );
                }
            }
        }
    }

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
    fn fixed_classic_cache_matches_general_cache_through_mutations_and_clones() {
        let puzzle = Puzzle::parse(
            "53..7....6..195....98....6.8...6...34..8.3..17...2...6.6....28....419..5....8..79",
        )
        .unwrap();
        let topology = Arc::new(ConstraintTopology::new(VariantConfig::default()));
        let mut general = Grid::from_puzzle(Arc::clone(&topology), &puzzle);
        let mut fixed = Grid::from_classic_puzzle(Arc::clone(&topology), &puzzle);

        assert!(matches!(
            &general.region_positions,
            GridRegionPositions::General(_)
        ));
        assert!(matches!(
            &fixed.region_positions,
            GridRegionPositions::Classic(_)
        ));
        // A Classic general cache owns one Vec buffer per active family. The
        // fixed representation and each of its clones own just one Box.
        let GridRegionPositions::General(general_positions) = &general.region_positions else {
            unreachable!();
        };
        assert_eq!(
            general_positions
                .iter()
                .filter(|family| !family.is_empty())
                .count(),
            3
        );
        assert_grid_caches_equal(&general, &fixed);

        // Deterministic xorshift coverage combines removals, arbitrary mask
        // replacement, and placements. Every cache entry is compared after
        // each operation rather than sampling only the touched houses.
        let mut random = 0xd1b5_4a32_d192_ed03_u64;
        for step in 0..2_048 {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            let cell = CellId::new((random % 81) as u8).unwrap();
            let digit = Digit::new(((random >> 8) % 9 + 1) as u8).unwrap();
            let mask = CandidateMask::from_bits(((random >> 17) as u16 & 0x01ff) << 1);
            match step % 4 {
                0 => {
                    assert_eq!(
                        general.remove_candidate(cell, digit),
                        fixed.remove_candidate(cell, digit)
                    );
                }
                1 => {
                    general.set_candidates(cell, mask);
                    fixed.set_candidates(cell, mask);
                }
                2 => {
                    general.place(cell, digit);
                    fixed.place(cell, digit);
                }
                _ => {
                    assert_eq!(
                        general.remove_candidates(cell, mask),
                        fixed.remove_candidates(cell, mask)
                    );
                }
            }
            assert_grid_caches_equal(&general, &fixed);

            if step % 127 == 0 {
                let general_clone = general.clone();
                let fixed_clone = fixed.clone();
                assert!(matches!(
                    &fixed_clone.region_positions,
                    GridRegionPositions::Classic(_)
                ));
                assert_grid_caches_equal(&general_clone, &fixed_clone);
            }
        }

        // Dynamic chains journal candidate masks, remove candidates while a
        // branch is open, then restore every touched cell. Exercise that exact
        // mutation shape repeatedly and require bit-for-bit cache restoration.
        for _ in 0..64 {
            let general_before = general.clone();
            let fixed_before = fixed.clone();
            let mut originals = Vec::with_capacity(32);
            let mut touched = [false; 81];
            for _ in 0..32 {
                random ^= random << 13;
                random ^= random >> 7;
                random ^= random << 17;
                let cell = CellId::new((random % 81) as u8).unwrap();
                let digit = Digit::new(((random >> 8) % 9 + 1) as u8).unwrap();
                if !touched[cell.index()] {
                    touched[cell.index()] = true;
                    originals.push((cell, general.candidates(cell)));
                }
                assert_eq!(
                    general.remove_candidate(cell, digit),
                    fixed.remove_candidate(cell, digit)
                );
            }
            assert_grid_caches_equal(&general, &fixed);
            for &(cell, mask) in &originals {
                general.set_candidates(cell, mask);
                fixed.set_candidates(cell, mask);
            }
            assert_grid_caches_equal(&general_before, &general);
            assert_grid_caches_equal(&fixed_before, &fixed);
            assert_grid_caches_equal(&general, &fixed);
        }
    }

    #[test]
    fn clone_from_reuses_matching_cache_storage_and_handles_mismatches() {
        let puzzle = Puzzle::parse(
            "53..7....6..195....98....6.8...6...34..8.3..17...2...6.6....28....419..5....8..79",
        )
        .unwrap();
        let topology = Arc::new(ConstraintTopology::new(VariantConfig::default()));
        let mut general_source = Grid::from_puzzle(Arc::clone(&topology), &puzzle);
        let mut fixed_source = Grid::from_classic_puzzle(Arc::clone(&topology), &puzzle);
        general_source.remove_candidate(CellId::new(2).unwrap(), Digit::new(1).unwrap());
        fixed_source.remove_candidate(CellId::new(2).unwrap(), Digit::new(1).unwrap());

        let mut general_working = Grid::from_puzzle(Arc::clone(&topology), &puzzle);
        let general_buffers = match &general_working.region_positions {
            GridRegionPositions::General(positions) => positions
                .iter()
                .map(|family| (family.as_ptr(), family.capacity()))
                .collect::<Vec<_>>(),
            GridRegionPositions::Classic(_) => unreachable!(),
        };
        general_working.clone_from(&general_source);
        let reused_general_buffers = match &general_working.region_positions {
            GridRegionPositions::General(positions) => positions
                .iter()
                .map(|family| (family.as_ptr(), family.capacity()))
                .collect::<Vec<_>>(),
            GridRegionPositions::Classic(_) => unreachable!(),
        };
        assert_eq!(reused_general_buffers, general_buffers);
        assert_grid_caches_equal(&general_working, &general_source);

        let mut fixed_working = Grid::from_classic_puzzle(Arc::clone(&topology), &puzzle);
        let fixed_buffer = match &fixed_working.region_positions {
            GridRegionPositions::Classic(positions) => positions.as_ptr(),
            GridRegionPositions::General(_) => unreachable!(),
        };
        fixed_working.clone_from(&fixed_source);
        let reused_fixed_buffer = match &fixed_working.region_positions {
            GridRegionPositions::Classic(positions) => positions.as_ptr(),
            GridRegionPositions::General(_) => unreachable!(),
        };
        assert_eq!(reused_fixed_buffer, fixed_buffer);
        assert_grid_caches_equal(&fixed_working, &fixed_source);

        general_working.clone_from(&fixed_source);
        assert!(matches!(
            &general_working.region_positions,
            GridRegionPositions::Classic(_)
        ));
        assert_grid_caches_equal(&general_working, &fixed_source);

        fixed_working.clone_from(&general_source);
        assert!(matches!(
            &fixed_working.region_positions,
            GridRegionPositions::General(_)
        ));
        assert_grid_caches_equal(&fixed_working, &general_source);
    }

    #[test]
    fn classic_constructor_falls_back_for_variant_topologies() {
        let puzzle = Puzzle::parse(&".".repeat(81)).unwrap();
        let topology = Arc::new(ConstraintTopology::new(VariantConfig {
            sudoku_x: true,
            ..VariantConfig::default()
        }));
        let grid = Grid::from_classic_puzzle(topology, &puzzle);
        assert!(matches!(
            &grid.region_positions,
            GridRegionPositions::General(_)
        ));
        assert_eq!(
            (0..REGION_TYPE_COUNT)
                .filter(|&type_index| grid.topology().is_region_type_active(type_index))
                .count(),
            5
        );
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
