use std::array;

use sukaku_forge_core::{
    CandidateMask, CandidateRemovals, CandidateRemovalsBuilder, CellId, CellMask, Digit, Grid,
    NonConsecutiveMode, REGION_TYPE_COUNT, RegionId, se121_classic_peers,
};

use crate::{
    BugCellSequence, BugKind, CellSequence, EngineConfig, Evidence, Inference, Rating, Technique,
};

/// Find the first Java-compatible Bivalue Universal Grave hint.
///
/// Java detects a BUG by cloning a `Grid` and stripping its extra candidates.
/// This port keeps the same ordered search but validates a primitive mask
/// snapshot, avoiding a topology/cache clone on every unsuccessful probe.
#[must_use]
pub fn find_bivalue_universal_grave(grid: &Grid, config: EngineConfig) -> Option<Inference> {
    BugSearch::new(grid, config, false).find()
}

/// Find the first BUG using pristine SE 1.2.1 common-cell insertion order.
#[must_use]
pub(crate) fn find_bivalue_universal_grave_se121(
    grid: &Grid,
    config: EngineConfig,
) -> Option<Inference> {
    BugSearch::new(grid, config, true).find()
}

/// Collect every Java-compatible Bivalue Universal Grave hint in producer
/// order, including all shared-region Type 4 and naked-set Type 3 variants.
#[must_use]
pub fn collect_bivalue_universal_grave(grid: &Grid, config: EngineConfig) -> Vec<Inference> {
    BugSearch::new(grid, config, false).collect()
}

struct BugSearch<'a> {
    grid: &'a Grid,
    config: EngineConfig,
    se121_order: bool,
    variant_latin: bool,
    stripped: [CandidateMask; CellId::COUNT],
    bug_cells: [CellId; CellId::COUNT],
    bug_count: usize,
    bug_cell_mask: CellMask,
    bug_values: [CandidateMask; CellId::COUNT],
    all_bug_values: CandidateMask,
    common_cells: Option<CellMask>,
}

impl<'a> BugSearch<'a> {
    fn new(grid: &'a Grid, config: EngineConfig, se121_order: bool) -> Self {
        Self {
            grid,
            config,
            se121_order,
            variant_latin: effective_variant_latin(grid, config),
            stripped: array::from_fn(|index| grid.candidates(cell(index as u8))),
            bug_cells: [cell(0); CellId::COUNT],
            bug_count: 0,
            bug_cell_mask: CellMask::EMPTY,
            bug_values: [CandidateMask::EMPTY; CellId::COUNT],
            all_bug_values: CandidateMask::EMPTY,
            common_cells: None,
        }
    }

    fn prepare(mut self) -> Option<Self> {
        if !self.discover_bug_cells()
            || !self.validate_stripped_grid()
            || self.is_restricted()
            || self.bug_count == 0
        {
            return None;
        }
        Some(self)
    }

    fn find(self) -> Option<Inference> {
        let self_ = self.prepare()?;

        if self_.bug_count == 1 {
            return self_.type_1();
        }
        if self_.all_bug_values.count() == 1 {
            return self_.type_2().or_else(|| {
                if self_.bug_count == 2 {
                    self_.type_4()
                } else {
                    None
                }
            });
        }
        if self_.common_cells.is_some_and(|cells| !cells.is_empty()) {
            if self_.bug_count == 2
                && let Some(inference) = self_.type_4()
            {
                return Some(inference);
            }
            return self_.type_3();
        }
        None
    }

    fn collect(self) -> Vec<Inference> {
        let Some(self_) = self.prepare() else {
            return Vec::new();
        };
        let mut result = Vec::new();
        if self_.bug_count == 1 {
            if let Some(inference) = self_.type_1() {
                result.push(inference);
            }
        } else if self_.all_bug_values.count() == 1 {
            if let Some(inference) = self_.type_2() {
                result.push(inference);
            }
            if self_.bug_count == 2 {
                self_.visit_type_4(&mut |inference| {
                    result.push(inference);
                    true
                });
            }
        } else if self_.common_cells.is_some_and(|cells| !cells.is_empty()) {
            if self_.bug_count == 2 {
                self_.visit_type_4(&mut |inference| {
                    result.push(inference);
                    true
                });
            }
            self_.visit_type_3(&mut |inference| {
                result.push(inference);
                true
            });
        }
        result
    }

    fn discover_bug_cells(&mut self) -> bool {
        let mut all_extra_cells: Option<CellMask> = None;
        let mut only_value: Option<Digit> = None;
        let mut one_value = true;

        for type_index in self.type_range() {
            if !self.region_type_enabled(type_index) {
                continue;
            }
            for region_index in 0..self.grid.topology().region_count(type_index) {
                let region = region(type_index, region_index);
                let region_cells = *self.grid.topology().region_cells(region);
                for digit in digits() {
                    let positions = self.grid.region_candidate_positions(region, digit);
                    let count = positions.count();
                    if count == 0 || count == 2 {
                        continue;
                    }

                    let mut new_bug_cells = CellMask::EMPTY;
                    let mut sole_bug_cell = None;
                    for position in positions.iter() {
                        let candidate = cell(region_cells[usize::from(position)]);
                        if self.grid.candidates(candidate).count() >= 3 {
                            new_bug_cells.insert(candidate);
                            sole_bug_cell = Some(candidate);
                        }
                    }

                    if self.config.bug_fix {
                        match all_extra_cells {
                            None => {
                                all_extra_cells = Some(new_bug_cells);
                                only_value = Some(digit);
                            }
                            Some(current) if one_value && only_value == Some(digit) => {
                                all_extra_cells = Some(union_cells(current, new_bug_cells));
                            }
                            Some(_) if one_value => one_value = false,
                            Some(_) => {}
                        }
                    }

                    let new_bug_count = new_bug_cells.count();
                    if new_bug_count == 0 {
                        return false;
                    }
                    if new_bug_count == 1
                        && !self.record_bug_cell(sole_bug_cell.expect("one BUG cell"), digit, true)
                    {
                        return false;
                    }
                    // With multiple cells another region may identify which
                    // one carries this extra candidate.
                }
            }
        }

        if self.config.bug_fix
            && one_value
            && let (Some(all_extra_cells), Some(only_value)) = (all_extra_cells, only_value)
            && all_extra_cells.count() as usize > self.bug_count
        {
            for extra_cell in all_extra_cells.without(self.bug_cell_mask).iter() {
                // Compatibility quirk: Java gives tail cells their per-cell
                // extra mask but does not OR the digit into allBugValues.
                if !self.record_bug_cell(extra_cell, only_value, false) {
                    return false;
                }
            }
        }
        true
    }

    fn record_bug_cell(&mut self, bug_cell: CellId, digit: Digit, aggregate: bool) -> bool {
        if !self.bug_cell_mask.contains(bug_cell) {
            self.bug_cells[self.bug_count] = bug_cell;
            self.bug_count += 1;
            self.bug_cell_mask.insert(bug_cell);
        }
        self.bug_values[bug_cell.index()].insert(digit);
        if aggregate {
            self.all_bug_values.insert(digit);
        }
        self.stripped[bug_cell.index()].remove(digit);

        let visible = self.grid.topology().visible_mask(bug_cell);
        let common = self
            .common_cells
            .map_or(visible, |current| current.intersect(visible))
            .without(self.bug_cell_mask);
        self.common_cells = Some(common);

        !(self.bug_count > 1 && self.all_bug_values.count() > 1 && common.is_empty())
    }

    fn validate_stripped_grid(&self) -> bool {
        for raw_cell in 0_u8..81 {
            let cell = cell(raw_cell);
            if self.grid.value(cell) == 0 && self.stripped[cell.index()].count() != 2 {
                return false;
            }
        }

        for type_index in self.type_range() {
            if !self.region_type_enabled(type_index) {
                continue;
            }
            for region_index in 0..self.grid.topology().region_count(type_index) {
                let region = region(type_index, region_index);
                for digit in digits() {
                    let count = self
                        .grid
                        .topology()
                        .region_cells(region)
                        .iter()
                        .filter(|&&raw| self.stripped[usize::from(raw)].contains(digit))
                        .count();
                    if count != 0 && count != 2 {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn is_restricted(&self) -> bool {
        let variant = self.grid.topology().config();
        if !variant.anti_ferz
            && !variant.anti_knight
            && variant.non_consecutive == NonConsecutiveMode::Off
        {
            return false;
        }
        for raw_cell in 0_u8..81 {
            let source = cell(raw_cell);
            if self.grid.value(source) != 0 || self.stripped[source.index()].count() != 2 {
                continue;
            }
            let deadly_values = self.stripped[source.index()];
            if variant.anti_ferz || variant.anti_knight {
                for &raw_peer in self.grid.topology().chess_only_peers(source) {
                    if !self
                        .grid
                        .candidates(cell(raw_peer))
                        .intersect(deadly_values)
                        .is_empty()
                    {
                        return true;
                    }
                }
            }

            let mode = variant.non_consecutive;
            if mode == NonConsecutiveMode::Off {
                continue;
            }
            let neighbors = if mode.is_orthogonal() {
                self.grid
                    .topology()
                    .orthogonal_neighbors(source, variant.toroidal)
            } else {
                self.grid
                    .topology()
                    .diagonal_neighbors(source, variant.toroidal)
            };
            for &raw_neighbor in neighbors {
                let neighbor_values = self.grid.candidates(cell(raw_neighbor));
                for digit in deadly_values.iter() {
                    let value = digit.get();
                    if (mode.is_cyclic() || value < 9)
                        && neighbor_values.contains(wrapped_digit(value + 1))
                    {
                        return true;
                    }
                    if (mode.is_cyclic() || value > 1)
                        && neighbor_values.contains(wrapped_digit(value.wrapping_sub(1)))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn type_1(&self) -> Option<Inference> {
        let bug_cell = self.bug_cells[0];
        let mut builder = CandidateRemovalsBuilder::with_capacity(1);
        builder.add(
            bug_cell,
            self.grid.candidates(bug_cell).without(self.all_bug_values),
        );
        inference(
            56,
            builder.build(),
            BugKind::Type1 {
                cell: bug_cell,
                extra_values: self.all_bug_values,
            },
        )
    }

    fn type_2(&self) -> Option<Inference> {
        let digit = self.all_bug_values.single()?;
        let common = self.common_cells?;
        let mut builder = CandidateRemovalsBuilder::with_capacity(common.count() as usize);
        if self.se121_order {
            for &raw_peer in se121_classic_peers(self.bug_cells[0]) {
                let victim = cell(raw_peer);
                if common.contains(victim) && self.grid.candidates(victim).contains(digit) {
                    builder.add(victim, CandidateMask::of(digit));
                }
            }
        } else {
            for victim in common.iter() {
                if self.grid.candidates(victim).contains(digit) {
                    builder.add(victim, CandidateMask::of(digit));
                }
            }
        }
        inference(
            57,
            builder.build(),
            BugKind::Type2 {
                bug_cells: self.bug_sequence(),
                digit,
            },
        )
    }

    fn type_4(&self) -> Option<Inference> {
        let mut result = None;
        self.visit_type_4(&mut |inference| {
            result = Some(inference);
            false
        });
        result
    }

    fn visit_type_4(&self, emit: &mut dyn FnMut(Inference) -> bool) {
        let bug_cells = [self.bug_cells[0], self.bug_cells[1]];
        let Some(locked) = self
            .grid
            .candidates(bug_cells[0])
            .intersect(self.grid.candidates(bug_cells[1]))
            .without(self.all_bug_values)
            .single()
        else {
            return;
        };

        for type_index in self.type_range() {
            if !self.region_type_enabled(type_index) {
                continue;
            }
            let Some(region) = self.shared_region(type_index) else {
                continue;
            };
            let mut builder = CandidateRemovalsBuilder::with_capacity(2);
            for bug_cell in bug_cells {
                builder.add(
                    bug_cell,
                    self.grid
                        .candidates(bug_cell)
                        .without(self.bug_values[bug_cell.index()])
                        .without(CandidateMask::of(locked)),
                );
            }
            if let Some(inference) = inference(
                57,
                builder.build(),
                BugKind::Type4 {
                    bug_cells,
                    extra_values: [
                        self.bug_values[bug_cells[0].index()],
                        self.bug_values[bug_cells[1].index()],
                    ],
                    region,
                    locked_digit: locked,
                    all_extra_values: self.all_bug_values,
                },
            ) {
                if !emit(inference) {
                    return;
                }
            }
        }
    }

    fn type_3(&self) -> Option<Inference> {
        let mut result = None;
        self.visit_type_3(&mut |inference| {
            result = Some(inference);
            false
        });
        result
    }

    fn visit_type_3(&self, emit: &mut dyn FnMut(Inference) -> bool) {
        if self.config.bug_fix {
            for degree in 2_u8..=6 {
                for type_index in self.type_range() {
                    if !self.visit_type_3_in_family(degree, type_index, true, emit) {
                        return;
                    }
                }
            }
        } else {
            for type_index in self.type_range() {
                for degree in 2_u8..=6 {
                    if !self.visit_type_3_in_family(degree, type_index, false, emit) {
                        return;
                    }
                }
            }
        }
    }

    fn visit_type_3_in_family(
        &self,
        degree: u8,
        type_index: usize,
        fixed_order: bool,
        emit: &mut dyn FnMut(Inference) -> bool,
    ) -> bool {
        if !self.region_type_enabled(type_index) {
            return true;
        }
        let Some(region) = self.shared_region(type_index) else {
            return true;
        };
        let Some(common) = self.common_cells else {
            return true;
        };
        let mut region_cells = [cell(0); 9];
        let mut region_cell_count = 0;
        let mut add_if_in_region = |common_cell| {
            if self
                .grid
                .topology()
                .cell_region_index(common_cell, type_index)
                == Some(region.region_index() as u8)
            {
                region_cells[region_cell_count] = common_cell;
                region_cell_count += 1;
            }
        };
        if self.se121_order {
            for &raw_peer in se121_classic_peers(self.bug_cells[0]) {
                let common_cell = cell(raw_peer);
                if common.contains(common_cell) {
                    add_if_in_region(common_cell);
                }
            }
        } else {
            for common_cell in common.iter() {
                add_if_in_region(common_cell);
            }
        }
        if region_cell_count < usize::from(degree) {
            return true;
        }

        let choose = degree - 1;
        for selection in combination_masks(choose, region_cell_count as u8) {
            let mut helper_cells = CellSequence::new();
            let mut helper_mask = CellMask::EMPTY;
            let mut other_common = CandidateMask::EMPTY;
            let mut all_masks_valid = true;
            for (index, &helper) in region_cells[..region_cell_count].iter().enumerate() {
                if selection & (1_u16 << index) == 0 {
                    continue;
                }
                helper_cells.push(helper);
                helper_mask.insert(helper);
                let values = self.grid.candidates(helper);
                all_masks_valid &= values.count() > 1;
                other_common = other_common.union(values);
            }
            if other_common.count() != u32::from(degree)
                || !all_masks_valid
                || self.all_bug_values.count() <= 1
            {
                continue;
            }
            let set_values = other_common.union(self.all_bug_values);
            if set_values.count() != u32::from(degree) {
                continue;
            }

            let generalized = fixed_order && !self.variant_latin;
            let removals = if generalized {
                self.generalized_type_3_removals(helper_cells, set_values)
            } else {
                self.regional_type_3_removals(
                    &region_cells[..region_cell_count],
                    helper_mask,
                    set_values,
                )
            };
            if let Some(inference) = inference(
                57 + u16::from(degree - 1),
                removals,
                BugKind::Type3 {
                    bug_cells: self.bug_sequence(),
                    set_cells: helper_cells,
                    region,
                    set_values,
                    all_extra_values: self.all_bug_values,
                    generalized,
                },
            ) {
                if !emit(inference) {
                    return false;
                }
            }
        }
        true
    }

    fn regional_type_3_removals(
        &self,
        region_cells: &[CellId],
        helper_cells: CellMask,
        set_values: CandidateMask,
    ) -> CandidateRemovals {
        let mut builder = CandidateRemovalsBuilder::with_capacity(region_cells.len());
        for &victim in region_cells {
            if helper_cells.contains(victim) || self.bug_cell_mask.contains(victim) {
                continue;
            }
            builder.add(victim, self.grid.candidates(victim).intersect(set_values));
        }
        builder.build()
    }

    fn generalized_type_3_removals(
        &self,
        helper_cells: CellSequence,
        set_values: CandidateMask,
    ) -> CandidateRemovals {
        let mut builder = CandidateRemovalsBuilder::with_capacity(9);
        for digit in set_values.iter() {
            let mut victims: Option<CellMask> = None;
            for source in helper_cells
                .iter()
                .chain(self.bug_cells[..self.bug_count].iter().copied())
            {
                if !self.grid.candidates(source).contains(digit) {
                    continue;
                }
                let visible = self.grid.topology().visible_mask(source);
                victims = Some(victims.map_or(visible, |current| current.intersect(visible)));
            }
            for victim in victims.expect("tuple digit has at least one source").iter() {
                if self.grid.candidates(victim).contains(digit) {
                    builder.add(victim, CandidateMask::of(digit));
                }
            }
        }
        builder.build()
    }

    fn shared_region(&self, type_index: usize) -> Option<RegionId> {
        let first = *self.bug_cells[..self.bug_count].first()?;
        let region_index = self.grid.topology().cell_region_index(first, type_index)?;
        for &bug_cell in &self.bug_cells[1..self.bug_count] {
            if self.grid.topology().cell_region_index(bug_cell, type_index) != Some(region_index) {
                return None;
            }
        }
        Some(region(type_index, usize::from(region_index)))
    }

    fn bug_sequence(&self) -> BugCellSequence {
        let mut result = BugCellSequence::new();
        for &bug_cell in &self.bug_cells[..self.bug_count] {
            result.push_with_values(bug_cell, self.bug_values[bug_cell.index()]);
        }
        result
    }

    fn type_range(&self) -> std::ops::Range<usize> {
        let start = usize::from(!self.grid.topology().config().blocks);
        start..if self.variant_latin {
            3
        } else {
            REGION_TYPE_COUNT
        }
    }

    fn region_type_enabled(&self, type_index: usize) -> bool {
        type_index < 3 || self.grid.topology().is_region_type_active(type_index)
    }
}

fn inference(rating: u16, removals: CandidateRemovals, kind: BugKind) -> Option<Inference> {
    if removals.is_empty() {
        return None;
    }
    Some(Inference::elimination(
        Technique::BivalueUniversalGrave,
        Rating::from_tenths(rating),
        removals,
        Evidence::Bug { kind },
    ))
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

fn union_cells(first: CellMask, second: CellMask) -> CellMask {
    CellMask::from_words(first.low() | second.low(), first.high() | second.high())
}

fn combination_masks(degree: u8, count: u8) -> impl Iterator<Item = u16> {
    (0_u16..(1_u16 << count)).filter(move |mask| mask.count_ones() == u32::from(degree))
}

fn digits() -> impl Iterator<Item = Digit> {
    (1_u8..=9).map(|value| Digit::new(value).expect("digit loop"))
}

fn wrapped_digit(value: u8) -> Digit {
    Digit::new(if value == 0 {
        9
    } else if value == 10 {
        1
    } else {
        value
    })
    .expect("wrapped digit")
}

fn cell(raw: u8) -> CellId {
    CellId::new(raw).expect("cell index")
}

fn region(type_index: usize, region_index: usize) -> RegionId {
    RegionId::new(type_index as u8, region_index as u8).expect("region index")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{
        CandidateMask, ConstraintTopology, Digit, Grid, Puzzle, VariantConfig,
    };

    use super::{cell, collect_bivalue_universal_grave, find_bivalue_universal_grave};
    use crate::{BugKind, EngineConfig, Evidence, Inference, Rating, RatingMode};

    const SOLUTION: &str =
        "534678912672195348198342567859761423426853791713924856961537284287419635345286179";

    fn mask(text: &str) -> CandidateMask {
        text.bytes().fold(CandidateMask::EMPTY, |mut result, byte| {
            result.insert(Digit::new(byte - b'0').expect("fixture digit"));
            result
        })
    }

    fn bug_grid(overrides: &[(usize, &str)]) -> Grid {
        let puzzle = Puzzle::parse(&".".repeat(729)).expect("empty pencilmarks");
        let mut grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &puzzle,
        );
        for (index, byte) in SOLUTION.bytes().enumerate() {
            let value = byte - b'0';
            let next = if value == 9 { 1 } else { value + 1 };
            grid.set_candidates(
                cell(index as u8),
                CandidateMask::of(Digit::new(value).unwrap())
                    .union(CandidateMask::of(Digit::new(next).unwrap())),
            );
        }
        for &(index, values) in overrides {
            grid.set_candidates(cell(index as u8), mask(values));
        }
        grid
    }

    fn effects(inference: &Inference) -> Vec<(u8, u16)> {
        inference
            .removals()
            .iter()
            .map(|entry| (entry.cell().raw(), entry.digits().bits()))
            .collect()
    }

    #[test]
    fn bug_type_1_matches_released_java_in_both_rating_modes() {
        for rating_mode in [RatingMode::Original, RatingMode::Revised] {
            let grid = bug_grid(&[(0, "567")]);
            let inference = find_bivalue_universal_grave(
                &grid,
                EngineConfig {
                    rating_mode,
                    ..EngineConfig::default()
                },
            )
            .expect("BUG+1");
            assert_eq!(inference.rating(), Rating::from_tenths(56));
            assert_eq!(inference.name(), "BUG type 1");
            assert_eq!(inference.short_name(), "BUG1");
            assert_eq!(inference.description(grid.topology()), "BUG type 1: r1c1");
            assert_eq!(effects(&inference), vec![(0, mask("56").bits())]);
            let mut applied = grid.clone();
            inference.apply(&mut applied);
            assert_eq!(applied.candidates(cell(0)), mask("7"));
        }
    }

    #[test]
    fn bug_type_2_preserves_discovery_and_victim_order() {
        let grid = bug_grid(&[(0, "567"), (1, "347")]);
        let inference =
            find_bivalue_universal_grave(&grid, EngineConfig::default()).expect("BUG type 2");
        assert_eq!(inference.rating(), Rating::from_tenths(57));
        assert_eq!(inference.name(), "BUG type 2");
        assert_eq!(inference.short_name(), "BUG2");
        assert_eq!(
            inference.description(grid.topology()),
            "BUG type 2: r1c1,r1c2 on 7"
        );
        assert_eq!(
            effects(&inference),
            vec![
                (3, mask("7").bits()),
                (4, mask("7").bits()),
                (9, mask("7").bits()),
                (10, mask("7").bits()),
            ]
        );
    }

    #[test]
    fn bug_type_3_uses_degree_then_block_order() {
        let grid = bug_grid(&[(0, "568"), (1, "349")]);
        let inference =
            find_bivalue_universal_grave(&grid, EngineConfig::default()).expect("BUG type 3");
        assert_eq!(inference.rating(), Rating::from_tenths(58));
        assert_eq!(inference.name(), "BUG type 3");
        assert_eq!(inference.short_name(), "BUG3");
        assert_eq!(
            inference.description(grid.topology()),
            "BUG type 3: r1c1,r1c2 on 8, 9"
        );
        assert_eq!(
            effects(&inference),
            vec![(10, mask("8").bits()), (19, mask("9").bits())]
        );
        let Evidence::Bug {
            kind: BugKind::Type3 { bug_cells, .. },
        } = inference.evidence()
        else {
            panic!("BUG3 evidence");
        };
        assert_eq!(
            bug_cells
                .iter_with_values()
                .map(|(cell, values)| (cell.raw(), values.bits()))
                .collect::<Vec<_>>(),
            vec![(0, mask("8").bits()), (1, mask("9").bits())]
        );
    }

    #[test]
    fn bug_type_4_precedes_type_3() {
        let grid = bug_grid(&[(0, "568"), (3, "679")]);
        let inference =
            find_bivalue_universal_grave(&grid, EngineConfig::default()).expect("BUG type 4");
        assert_eq!(inference.rating(), Rating::from_tenths(57));
        assert_eq!(inference.name(), "BUG type 4");
        assert_eq!(inference.short_name(), "BUG4");
        assert_eq!(
            inference.description(grid.topology()),
            "BUG type 4: r1c1,r1c4 on 6"
        );
        assert_eq!(
            effects(&inference),
            vec![(0, mask("5").bits()), (3, mask("7").bits())]
        );
        let Evidence::Bug {
            kind: BugKind::Type4 { extra_values, .. },
        } = inference.evidence()
        else {
            panic!("BUG4 evidence");
        };
        assert_eq!(extra_values, [mask("8"), mask("9")]);
    }

    #[test]
    fn full_collector_keeps_java_type_and_degree_order_and_compact_winner() {
        for grid in [bug_grid(&[(0, "567")]), bug_grid(&[(0, "567"), (1, "347")])] {
            let collected = collect_bivalue_universal_grave(&grid, EngineConfig::default());
            assert_eq!(collected.len(), 1);
            assert_eq!(
                find_bivalue_universal_grave(&grid, EngineConfig::default()).as_ref(),
                collected.first()
            );
        }

        let type_four_and_three = bug_grid(&[(0, "568"), (3, "679")]);
        let collected =
            collect_bivalue_universal_grave(&type_four_and_three, EngineConfig::default());
        assert_eq!(collected.len(), 10);
        assert_eq!(
            find_bivalue_universal_grave(&type_four_and_three, EngineConfig::default()).as_ref(),
            collected.first()
        );
        assert_eq!(
            collected
                .iter()
                .map(|hint| (hint.rating().tenths(), hint.name()))
                .collect::<Vec<_>>(),
            [
                (57, "BUG type 4".to_owned()),
                (58, "BUG type 3".to_owned()),
                (59, "BUG type 3".to_owned()),
                (59, "BUG type 3".to_owned()),
                (60, "BUG type 3".to_owned()),
                (60, "BUG type 3".to_owned()),
                (61, "BUG type 3".to_owned()),
                (61, "BUG type 3".to_owned()),
                (62, "BUG type 3".to_owned()),
                (62, "BUG type 3".to_owned()),
            ]
        );

        let type_three = bug_grid(&[(0, "568"), (1, "349")]);
        let collected = collect_bivalue_universal_grave(&type_three, EngineConfig::default());
        assert_eq!(collected.len(), 22);
        assert_eq!(
            find_bivalue_universal_grave(&type_three, EngineConfig::default()).as_ref(),
            collected.first()
        );
        assert_eq!(
            collected[0].description(type_three.topology()),
            collected[1].description(type_three.topology()),
            "BUG hints retain Java identity equality rather than effect deduplication"
        );
    }

    #[test]
    fn lk_fix_adds_same_digit_tail_cells_in_java_order() {
        let grid = bug_grid(&[(0, "256"), (1, "234"), (9, "267"), (10, "278"), (13, "129")]);
        let fixed = find_bivalue_universal_grave(&grid, EngineConfig::default())
            .expect("lkSudoku BUG2 fix");
        assert_eq!(
            fixed.description(grid.topology()),
            "BUG type 2: r2c5,r1c1,r1c2,r2c1,r2c2 on 2"
        );
        assert_eq!(effects(&fixed), vec![(11, mask("2").bits())]);

        assert!(
            find_bivalue_universal_grave(
                &grid,
                EngineConfig {
                    bug_fix: false,
                    ..EngineConfig::default()
                }
            )
            .is_none()
        );
    }

    #[test]
    fn chess_restrictions_reject_an_otherwise_valid_bug() {
        let puzzle = Puzzle::parse(&".".repeat(729)).expect("empty pencilmarks");
        let topology = Arc::new(ConstraintTopology::new(VariantConfig {
            anti_knight: true,
            ..VariantConfig::default()
        }));
        let mut grid = Grid::from_puzzle(topology, &puzzle);
        for (index, byte) in SOLUTION.bytes().enumerate() {
            let value = byte - b'0';
            let next = if value == 9 { 1 } else { value + 1 };
            grid.set_candidates(
                cell(index as u8),
                CandidateMask::of(Digit::new(value).unwrap())
                    .union(CandidateMask::of(Digit::new(next).unwrap())),
            );
        }
        grid.set_candidates(cell(0), mask("567"));
        assert!(find_bivalue_universal_grave(&grid, EngineConfig::default()).is_none());
    }
}
