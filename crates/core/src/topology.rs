use std::array;
use std::io::{self, Write};

use crate::{CellId, CellMask, NonConsecutiveMode, PositionMask, RegionId, VariantConfig};

pub const REGION_TYPE_COUNT: usize = 10;
pub const SE121_CLASSIC_PEER_COUNT: usize = 20;

const SE121_CLASSIC_PEERS: [[u8; SE121_CLASSIC_PEER_COUNT]; CellId::COUNT] =
    build_se121_classic_peers();

type PeerTable = [Vec<u8>; CellId::COUNT];
type Regions = [Vec<[u8; 9]>; REGION_TYPE_COUNT];

/// Classic peer order used by pristine Sudoku Explainer 1.2.1.
///
/// `Cell.getHouseCells()` inserted block cells, then row cells, then column
/// cells into a `LinkedHashSet`. Later Sukaku Explainer releases replaced this
/// with an ascending cell-index catalog. The difference is observable in a
/// few producer tie-breaks, so the dedicated old rater keeps this fixed table
/// rather than changing the general topology contract.
#[must_use]
pub const fn se121_classic_peers(cell: CellId) -> &'static [u8; SE121_CLASSIC_PEER_COUNT] {
    &SE121_CLASSIC_PEERS[cell.index()]
}

const fn build_se121_classic_peers() -> [[u8; SE121_CLASSIC_PEER_COUNT]; CellId::COUNT] {
    let mut result = [[0_u8; SE121_CLASSIC_PEER_COUNT]; CellId::COUNT];
    let mut raw_cell = 0_usize;
    while raw_cell < CellId::COUNT {
        let row = raw_cell / 9;
        let column = raw_cell % 9;
        let block_row = row / 3 * 3;
        let block_column = column / 3 * 3;
        let mut length = 0_usize;

        let mut block_offset = 0_usize;
        while block_offset < 9 {
            let peer_row = block_row + block_offset / 3;
            let peer_column = block_column + block_offset % 3;
            let peer = peer_row * 9 + peer_column;
            if peer != raw_cell {
                result[raw_cell][length] = peer as u8;
                length += 1;
            }
            block_offset += 1;
        }

        let mut peer_column = 0_usize;
        while peer_column < 9 {
            if peer_column / 3 != column / 3 {
                result[raw_cell][length] = (row * 9 + peer_column) as u8;
                length += 1;
            }
            peer_column += 1;
        }

        let mut peer_row = 0_usize;
        while peer_row < 9 {
            if peer_row / 3 != row / 3 {
                result[raw_cell][length] = (peer_row * 9 + column) as u8;
                length += 1;
            }
            peer_row += 1;
        }

        debug_assert!(length == SE121_CLASSIC_PEER_COUNT);
        raw_cell += 1;
    }
    result
}

const ORTHOGONAL_OFFSETS: [(i8, i8); 4] = [(-1, 0), (1, 0), (0, 1), (0, -1)];
const DIAGONAL_OFFSETS: [(i8, i8); 4] = [(-1, -1), (-1, 1), (1, -1), (1, 1)];
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (-1, 1), (-1, -1), (1, -1)];
const KNIGHT_OFFSETS: [(i8, i8); 8] = [
    (1, 2),
    (-1, 2),
    (1, -2),
    (-1, -2),
    (2, 1),
    (-2, 1),
    (2, -1),
    (-2, -1),
];

const WINDOW_REGIONS: [[u8; 9]; 9] = [
    [10, 11, 12, 19, 20, 21, 28, 29, 30],
    [14, 15, 16, 23, 24, 25, 32, 33, 34],
    [46, 47, 48, 55, 56, 57, 64, 65, 66],
    [50, 51, 52, 59, 60, 61, 68, 69, 70],
    [1, 2, 3, 37, 38, 39, 73, 74, 75],
    [5, 6, 7, 41, 42, 43, 77, 78, 79],
    [9, 18, 27, 13, 22, 31, 17, 26, 35],
    [45, 54, 63, 49, 58, 67, 53, 62, 71],
    [0, 4, 8, 36, 40, 44, 72, 76, 80],
];
const MAIN_DIAGONAL: [u8; 9] = [0, 10, 20, 30, 40, 50, 60, 70, 80];
const ANTI_DIAGONAL: [u8; 9] = [8, 16, 24, 32, 40, 48, 56, 64, 72];
const GIRANDOLA: [u8; 9] = [0, 8, 13, 37, 40, 43, 67, 72, 80];
const ASTERISK: [u8; 9] = [13, 20, 24, 37, 40, 43, 56, 60, 67];
const CENTER_DOT: [u8; 9] = [10, 13, 16, 37, 40, 43, 64, 67, 70];

/// Immutable ordered peers, region geometry and neighbor tables.
#[derive(Clone, Debug)]
pub struct ConstraintTopology {
    config: VariantConfig,
    visible: PeerTable,
    forward_visible: PeerTable,
    anti_visible: PeerTable,
    visible_masks: [CellMask; CellId::COUNT],
    regular_orthogonal: PeerTable,
    toroidal_orthogonal: PeerTable,
    regular_diagonal: PeerTable,
    toroidal_diagonal: PeerTable,
    regular_anti_ferz: PeerTable,
    regular_anti_knight: PeerTable,
    regions: Regions,
    region_masks: [Vec<CellMask>; REGION_TYPE_COUNT],
    cell_region_indexes: [[i8; REGION_TYPE_COUNT]; CellId::COUNT],
    cell_positions_in_regions: [[i8; REGION_TYPE_COUNT]; CellId::COUNT],
    active_region_types: [bool; REGION_TYPE_COUNT],
}

impl ConstraintTopology {
    #[must_use]
    pub fn new(config: VariantConfig) -> Self {
        let regions = build_regions();
        let cell_region_indexes = build_cell_region_indexes(&regions);
        let cell_positions_in_regions = build_cell_positions(&regions);
        let (visible, forward_visible, anti_visible) =
            build_peer_indexes(config, &regions, &cell_region_indexes);
        let visible_masks = array::from_fn(|index| mask_of(&visible[index]));
        let region_masks = array::from_fn(|type_index| {
            regions[type_index]
                .iter()
                .map(|region| mask_of(region))
                .collect()
        });

        Self {
            config,
            visible,
            forward_visible,
            anti_visible,
            visible_masks,
            regular_orthogonal: build_grid_neighbors(&ORTHOGONAL_OFFSETS, false),
            toroidal_orthogonal: build_grid_neighbors(&ORTHOGONAL_OFFSETS, true),
            regular_diagonal: build_grid_neighbors(&DIAGONAL_OFFSETS, false),
            toroidal_diagonal: build_linear_wrapped_neighbors(&[-10, -8, 8, 10]),
            regular_anti_ferz: build_regular_leaper_neighbors(&FERZ_OFFSETS),
            regular_anti_knight: build_regular_leaper_neighbors(&KNIGHT_OFFSETS),
            regions,
            region_masks,
            cell_region_indexes,
            cell_positions_in_regions,
            active_region_types: [
                config.blocks,
                true,
                true,
                config.disjoint_groups,
                config.windows,
                config.sudoku_x,
                config.sudoku_x,
                config.girandola,
                config.asterisk,
                config.center_dot,
            ],
        }
    }

    #[must_use]
    pub const fn config(&self) -> VariantConfig {
        self.config
    }

    #[must_use]
    pub fn visible_peers(&self, cell: CellId) -> &[u8] {
        &self.visible[cell.index()]
    }

    #[must_use]
    pub fn forward_visible_peers(&self, cell: CellId) -> &[u8] {
        &self.forward_visible[cell.index()]
    }

    #[must_use]
    pub fn chess_only_peers(&self, cell: CellId) -> &[u8] {
        &self.anti_visible[cell.index()]
    }

    #[must_use]
    pub const fn visible_mask(&self, cell: CellId) -> CellMask {
        self.visible_masks[cell.index()]
    }

    #[must_use]
    pub fn orthogonal_neighbors(&self, cell: CellId, toroidal: bool) -> &[u8] {
        if toroidal {
            &self.toroidal_orthogonal[cell.index()]
        } else {
            &self.regular_orthogonal[cell.index()]
        }
    }

    #[must_use]
    pub fn diagonal_neighbors(&self, cell: CellId, toroidal: bool) -> &[u8] {
        if toroidal {
            &self.toroidal_diagonal[cell.index()]
        } else {
            &self.regular_diagonal[cell.index()]
        }
    }

    #[must_use]
    pub fn regular_anti_ferz_neighbors(&self, cell: CellId) -> &[u8] {
        &self.regular_anti_ferz[cell.index()]
    }

    #[must_use]
    pub fn regular_anti_knight_neighbors(&self, cell: CellId) -> &[u8] {
        &self.regular_anti_knight[cell.index()]
    }

    #[must_use]
    pub fn forbidden_pair_neighbors(&self, cell: CellId) -> Option<&[u8]> {
        if !self.config.forbidden_pairs || self.config.non_consecutive == NonConsecutiveMode::Off {
            return None;
        }
        Some(if self.config.non_consecutive.is_orthogonal() {
            self.orthogonal_neighbors(cell, self.config.toroidal)
        } else {
            self.diagonal_neighbors(cell, self.config.toroidal)
        })
    }

    #[must_use]
    pub const fn is_region_type_active(&self, type_index: usize) -> bool {
        self.active_region_types[type_index]
    }

    pub fn active_region_types(&self) -> impl Iterator<Item = usize> + '_ {
        self.active_region_types
            .iter()
            .enumerate()
            .filter_map(|(index, active)| active.then_some(index))
    }

    #[must_use]
    pub fn region_count(&self, type_index: usize) -> usize {
        self.regions[type_index].len()
    }

    #[must_use]
    pub fn region_cells(&self, region: RegionId) -> &[u8; 9] {
        &self.regions[region.type_index()][region.region_index()]
    }

    #[must_use]
    pub fn region_mask(&self, region: RegionId) -> CellMask {
        self.region_masks[region.type_index()][region.region_index()]
    }

    /// Positions of `subject` that also belong to `other`, in subject order.
    #[must_use]
    pub fn overlap_positions(&self, subject: RegionId, other: RegionId) -> PositionMask {
        let other_mask = self.region_mask(other);
        let mut result = PositionMask::EMPTY;
        for (position, &raw_cell) in self.region_cells(subject).iter().enumerate() {
            if other_mask.contains(CellId::new(raw_cell).expect("region cell")) {
                result.insert(position as u8);
            }
        }
        result
    }

    #[must_use]
    pub fn cell_region_index(&self, cell: CellId, type_index: usize) -> Option<u8> {
        let index = self.cell_region_indexes[cell.index()][type_index];
        u8::try_from(index).ok()
    }

    #[must_use]
    pub fn cell_position_in_region(&self, cell: CellId, type_index: usize) -> Option<u8> {
        let position = self.cell_positions_in_regions[cell.index()][type_index];
        u8::try_from(position).ok()
    }

    /// Write the exact byte sequence hashed by Java's TopologyConstructionSmoke.
    pub fn write_java_compatibility_bytes(&self, output: &mut impl Write) -> io::Result<()> {
        for cell_index in 0..CellId::COUNT {
            write_array(output, &self.visible[cell_index])?;
            write_array(output, &self.forward_visible[cell_index])?;
            write_array(output, &self.anti_visible[cell_index])?;

            for toroidal in [false, true] {
                write_array(
                    output,
                    if toroidal {
                        &self.toroidal_orthogonal[cell_index]
                    } else {
                        &self.regular_orthogonal[cell_index]
                    },
                )?;
                write_array(
                    output,
                    if toroidal {
                        &self.toroidal_diagonal[cell_index]
                    } else {
                        &self.regular_diagonal[cell_index]
                    },
                )?;
            }

            write_array(output, &self.regular_anti_ferz[cell_index])?;
            write_array(output, &self.regular_anti_knight[cell_index])?;
        }

        write_i32(output, REGION_TYPE_COUNT as i32)?;
        for type_index in 0..REGION_TYPE_COUNT {
            write_i32(output, self.regions[type_index].len() as i32)?;
            for region in &self.regions[type_index] {
                write_array(output, region)?;
            }
            for cell_index in 0..CellId::COUNT {
                write_i32(
                    output,
                    i32::from(self.cell_region_indexes[cell_index][type_index]),
                )?;
                write_i32(
                    output,
                    i32::from(self.cell_positions_in_regions[cell_index][type_index]),
                )?;
            }
        }
        Ok(())
    }
}

/// Write every Java topology configuration in the same mask order as the oracle.
pub fn write_all_java_topologies(output: &mut impl Write) -> io::Result<()> {
    for mask in 0_i32..(1_i32 << 10) {
        write_i32(output, mask)?;
        let enabled = |bit: u32| mask & (1_i32 << bit) != 0;
        let topology = ConstraintTopology::new(VariantConfig {
            blocks: enabled(0),
            disjoint_groups: enabled(1),
            windows: enabled(2),
            sudoku_x: enabled(3),
            center_dot: enabled(4),
            girandola: enabled(5),
            asterisk: enabled(6),
            anti_ferz: enabled(7),
            anti_knight: enabled(8),
            toroidal: enabled(9),
            non_consecutive: NonConsecutiveMode::Off,
            forbidden_pairs: false,
        });
        topology.write_java_compatibility_bytes(output)?;
    }
    Ok(())
}

fn build_peer_indexes(
    config: VariantConfig,
    regions: &Regions,
    cell_regions: &[[i8; REGION_TYPE_COUNT]; CellId::COUNT],
) -> (PeerTable, PeerTable, PeerTable) {
    let ferz_peers = build_visibility_leaper_neighbors(&FERZ_OFFSETS, config.toroidal);
    let knight_peers = build_visibility_leaper_neighbors(&KNIGHT_OFFSETS, config.toroidal);
    let mut visible: PeerTable = array::from_fn(|_| Vec::new());
    let mut forward: PeerTable = array::from_fn(|_| Vec::new());
    let mut anti: PeerTable = array::from_fn(|_| Vec::new());

    for source in 0_u8..81 {
        let mut source_visible = OrderedCellIndexes::new(source);
        let mut source_anti = OrderedCellIndexes::new(source);
        let source_row = source / 9;
        let source_column = source % 9;
        let source_block = source_row / 3 * 3 + source_column / 3;

        for peer in 0_u8..81 {
            let peer_row = peer / 9;
            let peer_column = peer % 9;
            let peer_block = peer_row / 3 * 3 + peer_column / 3;
            if peer_row == source_row
                || peer_column == source_column
                || config.blocks && peer_block == source_block
            {
                source_visible.add(peer);
            }
        }

        if config.windows {
            add_region_peers(&mut source_visible, regions, cell_regions, 4, source);
        }
        if config.disjoint_groups {
            add_region_peers(&mut source_visible, regions, cell_regions, 3, source);
        }
        if config.sudoku_x {
            add_diagonal_peers(&mut source_visible, source);
        }
        if config.center_dot {
            add_region_peers(&mut source_visible, regions, cell_regions, 9, source);
        }
        if config.girandola {
            add_region_peers(&mut source_visible, regions, cell_regions, 7, source);
        }
        if config.asterisk {
            add_region_peers(&mut source_visible, regions, cell_regions, 8, source);
        }
        if config.anti_ferz {
            add_chess_peers(
                &mut source_visible,
                &mut source_anti,
                &ferz_peers[usize::from(source)],
            );
        }
        if config.anti_knight {
            add_chess_peers(
                &mut source_visible,
                &mut source_anti,
                &knight_peers[usize::from(source)],
            );
        }

        visible[usize::from(source)] = source_visible.indexes.clone();
        forward[usize::from(source)] = source_visible
            .indexes
            .iter()
            .copied()
            .filter(|peer| *peer > source)
            .collect();
        anti[usize::from(source)] = source_anti.indexes;
    }
    (visible, forward, anti)
}

fn add_region_peers(
    peers: &mut OrderedCellIndexes,
    regions: &Regions,
    cell_regions: &[[i8; REGION_TYPE_COUNT]; CellId::COUNT],
    type_index: usize,
    source: u8,
) {
    let region_index = cell_regions[usize::from(source)][type_index];
    let Ok(region_index) = usize::try_from(region_index) else {
        return;
    };
    for peer in regions[type_index][region_index] {
        peers.add(peer);
    }
}

fn add_diagonal_peers(peers: &mut OrderedCellIndexes, source: u8) {
    let source_row = source / 9;
    let source_column = source % 9;
    let on_main = source_row == source_column;
    let on_anti = source_row + source_column == 8;
    for peer in 0_u8..81 {
        let row = peer / 9;
        let column = peer % 9;
        if on_main && row == column || on_anti && row + column == 8 {
            peers.add(peer);
        }
    }
}

fn add_chess_peers(
    visible: &mut OrderedCellIndexes,
    anti: &mut OrderedCellIndexes,
    chess_peers: &[u8],
) {
    for &peer in chess_peers {
        if visible.add(peer) {
            anti.add(peer);
        }
    }
}

fn build_visibility_leaper_neighbors(offsets: &[(i8, i8)], toroidal: bool) -> PeerTable {
    array::from_fn(|cell_index| {
        let row = (cell_index / 9) as i8;
        let column = (cell_index % 9) as i8;
        offsets
            .iter()
            .filter_map(|&(row_offset, column_offset)| {
                let mut neighbor_row = row + row_offset;
                let mut neighbor_column = column + column_offset;
                if toroidal {
                    neighbor_row = neighbor_row.rem_euclid(9);
                    neighbor_column = neighbor_column.rem_euclid(9);
                } else if !(0..9).contains(&neighbor_row) || !(0..9).contains(&neighbor_column) {
                    return None;
                }
                Some((neighbor_row * 9 + neighbor_column) as u8)
            })
            .collect()
    })
}

fn build_regular_leaper_neighbors(offsets: &[(i8, i8)]) -> PeerTable {
    array::from_fn(|cell_index| {
        let x = (cell_index % 9) as i8;
        let y = (cell_index / 9) as i8;
        offsets
            .iter()
            .filter_map(|&(x_offset, y_offset)| {
                let neighbor_x = x + x_offset;
                let neighbor_y = y + y_offset;
                if (0..9).contains(&neighbor_x) && (0..9).contains(&neighbor_y) {
                    Some((neighbor_y * 9 + neighbor_x) as u8)
                } else {
                    None
                }
            })
            .collect()
    })
}

fn build_grid_neighbors(offsets: &[(i8, i8)], toroidal: bool) -> PeerTable {
    array::from_fn(|cell_index| {
        let row = (cell_index / 9) as i8;
        let column = (cell_index % 9) as i8;
        offsets
            .iter()
            .filter_map(|&(row_offset, column_offset)| {
                let mut neighbor_row = row + row_offset;
                let mut neighbor_column = column + column_offset;
                if toroidal {
                    neighbor_row = neighbor_row.rem_euclid(9);
                    neighbor_column = neighbor_column.rem_euclid(9);
                } else if !(0..9).contains(&neighbor_row) || !(0..9).contains(&neighbor_column) {
                    return None;
                }
                Some((neighbor_row * 9 + neighbor_column) as u8)
            })
            .collect()
    })
}

fn build_linear_wrapped_neighbors(offsets: &[i16]) -> PeerTable {
    array::from_fn(|cell_index| {
        offsets
            .iter()
            .map(|offset| ((cell_index as i16 + offset).rem_euclid(81)) as u8)
            .collect()
    })
}

fn build_regions() -> Regions {
    let mut result: Regions = array::from_fn(|_| Vec::new());
    result[0] = vec![[0; 9]; 9];
    result[1] = vec![[0; 9]; 9];
    result[2] = vec![[0; 9]; 9];
    result[3] = vec![[0; 9]; 9];
    for region in 0_u8..9 {
        let region_row = region / 3;
        let region_column = region % 3;
        for position in 0_u8..9 {
            result[0][usize::from(region)][usize::from(position)] =
                (region_row * 3 + position / 3) * 9 + region_column * 3 + position % 3;
            result[1][usize::from(region)][usize::from(position)] = region * 9 + position;
            result[2][usize::from(region)][usize::from(position)] = position * 9 + region;
            result[3][usize::from(region)][usize::from(position)] =
                region_row * 9 + region_column + (position % 3) * 3 + (position / 3) * 27;
        }
    }
    result[4] = WINDOW_REGIONS.to_vec();
    result[5] = vec![MAIN_DIAGONAL];
    result[6] = vec![ANTI_DIAGONAL];
    result[7] = vec![GIRANDOLA];
    result[8] = vec![ASTERISK];
    result[9] = vec![CENTER_DOT];
    result
}

fn build_cell_region_indexes(regions: &Regions) -> [[i8; REGION_TYPE_COUNT]; CellId::COUNT] {
    let mut result = [[-1_i8; REGION_TYPE_COUNT]; CellId::COUNT];
    for (type_index, family) in regions.iter().enumerate() {
        for (region_index, region) in family.iter().enumerate() {
            for &cell_index in region {
                result[usize::from(cell_index)][type_index] = region_index as i8;
            }
        }
    }
    result
}

fn build_cell_positions(regions: &Regions) -> [[i8; REGION_TYPE_COUNT]; CellId::COUNT] {
    let mut result = [[-1_i8; REGION_TYPE_COUNT]; CellId::COUNT];
    for (type_index, family) in regions.iter().enumerate() {
        for region in family {
            for (position, &cell_index) in region.iter().enumerate() {
                result[usize::from(cell_index)][type_index] = position as i8;
            }
        }
    }
    result
}

fn mask_of(indexes: &[u8]) -> CellMask {
    let mut result = CellMask::EMPTY;
    for &cell_index in indexes {
        result.insert(CellId::new(cell_index).expect("topology cell index"));
    }
    result
}

fn write_array(output: &mut impl Write, values: &[u8]) -> io::Result<()> {
    write_i32(output, values.len() as i32)?;
    for &value in values {
        write_i32(output, i32::from(value))?;
    }
    Ok(())
}

fn write_i32(output: &mut impl Write, value: i32) -> io::Result<()> {
    output.write_all(&value.to_be_bytes())
}

#[derive(Debug)]
struct OrderedCellIndexes {
    present: [bool; CellId::COUNT],
    indexes: Vec<u8>,
}

impl OrderedCellIndexes {
    fn new(excluded: u8) -> Self {
        let mut present = [false; CellId::COUNT];
        present[usize::from(excluded)] = true;
        Self {
            present,
            indexes: Vec::with_capacity(80),
        }
    }

    fn add(&mut self, cell_index: u8) -> bool {
        if self.present[usize::from(cell_index)] {
            return false;
        }
        self.present[usize::from(cell_index)] = true;
        self.indexes.push(cell_index);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{ConstraintTopology, REGION_TYPE_COUNT, se121_classic_peers};
    use crate::{CellId, VariantConfig};

    #[test]
    fn classic_peer_order_matches_java() {
        let topology = ConstraintTopology::new(VariantConfig::default());
        let cell = CellId::new(0).unwrap();
        assert_eq!(
            topology.visible_peers(cell),
            [
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 18, 19, 20, 27, 36, 45, 54, 63, 72
            ]
        );
        assert_eq!(
            topology.forward_visible_peers(cell),
            topology.visible_peers(cell)
        );
        assert!(topology.chess_only_peers(cell).is_empty());
    }

    #[test]
    fn se121_peer_order_keeps_block_then_row_then_column_insertion() {
        assert_eq!(
            *se121_classic_peers(CellId::new(0).unwrap()),
            [
                1, 2, 9, 10, 11, 18, 19, 20, 3, 4, 5, 6, 7, 8, 27, 36, 45, 54, 63, 72
            ]
        );
        assert_eq!(
            *se121_classic_peers(CellId::new(40).unwrap()),
            [
                30, 31, 32, 39, 41, 48, 49, 50, 36, 37, 38, 42, 43, 44, 4, 13, 22, 58, 67, 76
            ]
        );
    }

    #[test]
    fn anti_knight_offset_order_matches_java_visibility_builder() {
        let topology = ConstraintTopology::new(VariantConfig {
            anti_knight: true,
            ..VariantConfig::default()
        });
        let center = CellId::new(40).unwrap();
        assert_eq!(
            topology.chess_only_peers(center),
            [51, 33, 47, 29, 59, 23, 57, 21]
        );
    }

    #[test]
    fn every_region_mapping_is_bidirectional() {
        let topology = ConstraintTopology::new(VariantConfig::default());
        for type_index in 0..REGION_TYPE_COUNT {
            for region_index in 0..topology.region_count(type_index) {
                let region = crate::RegionId::new(type_index as u8, region_index as u8).unwrap();
                for (position, &raw_cell) in topology.region_cells(region).iter().enumerate() {
                    let cell = CellId::new(raw_cell).unwrap();
                    assert_eq!(
                        topology.cell_region_index(cell, type_index),
                        Some(region_index as u8)
                    );
                    assert_eq!(
                        topology.cell_position_in_region(cell, type_index),
                        Some(position as u8)
                    );
                }
            }
        }
    }
}
