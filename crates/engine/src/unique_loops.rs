use std::collections::HashSet;

use sukaku_forge_core::{
    CandidateMask, CandidateRemovals, CandidateRemovalsBuilder, CellId, CellMask, Digit, Grid,
    NonConsecutiveMode, PositionMask, REGION_TYPE_COUNT, RegionId,
};

use crate::{
    CellSequence, EngineConfig, Evidence, Inference, Rating, RatingMode, Technique, UniqueLoopKind,
};

/// Find the first Unique Rectangle/Loop after Java's global difficulty/type sort.
#[must_use]
pub fn find_unique_loop(grid: &Grid, config: EngineConfig) -> Option<Inference> {
    run_search(grid, config, false)
        .best
        .map(|best| best.inference)
}

/// Collect every Java-compatible Unique Rectangle/Loop after the producer's
/// concrete-hint deduplication and stable difficulty/type sort.
#[must_use]
pub fn collect_unique_loop(grid: &Grid, config: EngineConfig) -> Vec<Inference> {
    let mut search = run_search(grid, config, true);
    let mut hints = search.all.take().expect("unique-loop collector");
    hints.sort_by(|left, right| {
        left.java_difficulty
            .partial_cmp(&right.java_difficulty)
            .expect("finite unique-loop difficulty")
            .then_with(|| left.hint_type.cmp(&right.hint_type))
    });
    hints.into_iter().map(|hint| hint.inference).collect()
}

fn run_search(grid: &Grid, config: EngineConfig, collect_all: bool) -> Search<'_> {
    let mut search = Search {
        grid,
        config,
        active_types: active_region_types(grid, config),
        best: None,
        all: collect_all.then(Vec::new),
        seen: collect_all.then(HashSet::new),
        stop: false,
    };
    for raw_start in 0_u8..81 {
        if search.stop {
            break;
        }
        let start = cell(raw_start);
        let loop_values = grid.candidates(start);
        if loop_values.count() != 2 {
            continue;
        }
        let mut digits = loop_values.iter();
        let first_digit = digits.next().expect("bivalue cell");
        let second_digit = digits.next().expect("bivalue cell");
        let mut loop_cells = LoopPath::new();
        search.walk(
            start,
            [first_digit, second_digit],
            loop_values,
            &mut loop_cells,
            CellMask::EMPTY,
            2,
            CandidateMask::EMPTY,
            None,
            0,
            0,
        );
    }
    search
}

struct Search<'a> {
    grid: &'a Grid,
    config: EngineConfig,
    active_types: Vec<usize>,
    best: Option<Best>,
    all: Option<Vec<Best>>,
    seen: Option<HashSet<JavaUniqueLoopKey>>,
    stop: bool,
}

struct Best {
    java_difficulty: f64,
    hint_type: u8,
    inference: Inference,
}

/// Incremental parity pruning bounds every surviving path at 18 cells, so the
/// hot DFS needs no per-start allocation.
struct LoopPath {
    cells: [CellId; 18],
    len: usize,
}

impl LoopPath {
    fn new() -> Self {
        Self {
            cells: [cell(0); 18],
            len: 0,
        }
    }

    fn push(&mut self, value: CellId) {
        self.cells[self.len] = value;
        self.len += 1;
    }

    fn pop(&mut self) {
        self.len -= 1;
    }

    fn first(&self) -> CellId {
        self.cells[0]
    }

    fn as_slice(&self) -> &[CellId] {
        &self.cells[..self.len]
    }
}

impl Search<'_> {
    #[allow(clippy::too_many_arguments)]
    fn walk(
        &mut self,
        current: CellId,
        digits: [Digit; 2],
        loop_values: CandidateMask,
        loop_cells: &mut LoopPath,
        mut visited_cells: CellMask,
        allowed_extra_cells: i8,
        extra_values: CandidateMask,
        last_region_type: Option<usize>,
        mut odd_regions: u128,
        mut even_regions: u128,
    ) {
        if self.stop {
            return;
        }

        let odd_position = loop_cells.len % 2 == 1;
        for &type_index in &self.active_types {
            let Some(region_index) = self.grid.topology().cell_region_index(current, type_index)
            else {
                continue;
            };
            let bit = 1_u128 << (type_index * 9 + usize::from(region_index));
            let visited = if odd_position {
                &mut odd_regions
            } else {
                &mut even_regions
            };
            if *visited & bit != 0 {
                return;
            }
            *visited |= bit;
        }

        visited_cells.insert(current);
        loop_cells.push(current);
        let mut rolling_extra_values = extra_values;
        for type_offset in 0..self.active_types.len() {
            if self.stop {
                break;
            }
            let type_index = self.active_types[type_offset];
            if Some(type_index) == last_region_type {
                continue;
            }
            let Some(region_index) = self.grid.topology().cell_region_index(current, type_index)
            else {
                continue;
            };
            let region = region(type_index, usize::from(region_index));
            let region_cells = *self.grid.topology().region_cells(region);
            for raw_next in region_cells {
                if self.stop {
                    break;
                }
                let next = cell(raw_next);
                if next == loop_cells.first() && loop_cells.len >= 4 {
                    if odd_regions == even_regions {
                        self.evaluate_loop(loop_cells.as_slice(), digits, loop_values);
                    }
                    continue;
                }
                if visited_cells.contains(next) {
                    continue;
                }
                let candidates = self.grid.candidates(next);
                if candidates.intersect(loop_values) != loop_values {
                    continue;
                }

                let cell_extra_values = candidates.without(loop_values);
                let new_extra_values = if self.config.unique_loop_fix {
                    extra_values.union(cell_extra_values)
                } else {
                    rolling_extra_values = rolling_extra_values.union(cell_extra_values);
                    rolling_extra_values
                };
                if candidates.count() != 2
                    && new_extra_values.count() != 1
                    && allowed_extra_cells <= 0
                {
                    continue;
                }
                let new_allowed = allowed_extra_cells - if candidates.count() > 2 { 1 } else { 0 };
                self.walk(
                    next,
                    digits,
                    loop_values,
                    loop_cells,
                    visited_cells,
                    new_allowed,
                    new_extra_values,
                    Some(type_index),
                    odd_regions,
                    even_regions,
                );
            }
        }
        loop_cells.pop();
    }

    fn evaluate_loop(
        &mut self,
        loop_cells: &[CellId],
        digits: [Digit; 2],
        loop_values: CandidateMask,
    ) {
        if self.is_restricted(loop_cells, digits) {
            return;
        }
        let mut extra_cells = [cell(0); 18];
        let mut extra_count = 0;
        for &loop_cell in loop_cells {
            if self.grid.candidates(loop_cell).count() > 2 {
                extra_cells[extra_count] = loop_cell;
                extra_count += 1;
            }
        }
        let extra_cells = &extra_cells[..extra_count];
        match extra_count {
            0 => {}
            1 => {
                let rescue = extra_cells[0];
                let mut builder = CandidateRemovalsBuilder::with_capacity(1);
                builder.add(rescue, loop_values);
                self.consider(
                    loop_cells,
                    digits,
                    UniqueLoopKind::Type1 { rescue },
                    self.base_rating(loop_cells.len()),
                    self.base_java_difficulty(loop_cells.len()),
                    builder.build(),
                );
            }
            2 => {
                let rescue_cells = [extra_cells[0], extra_cells[1]];
                let extras = self
                    .grid
                    .candidates(rescue_cells[0])
                    .union(self.grid.candidates(rescue_cells[1]))
                    .without(loop_values);
                if extras.count() == 1 {
                    self.create_type2(loop_cells, digits, extra_cells, extras.single().unwrap());
                } else if extras.count() >= 2 {
                    self.create_type3(loop_cells, digits, rescue_cells, extras);
                }
                self.create_type4(loop_cells, digits, rescue_cells);
            }
            _ => {
                // With assertions disabled, Java's legacy recursion can admit
                // inconsistent later rescues. Type 2 still takes the lowest
                // extra digit from the first rescue cell.
                if let Some(extra_digit) = self
                    .grid
                    .candidates(extra_cells[0])
                    .without(loop_values)
                    .iter()
                    .next()
                {
                    self.create_type2(loop_cells, digits, extra_cells, extra_digit);
                }
            }
        }
    }

    fn create_type3(
        &mut self,
        loop_cells: &[CellId],
        digits: [Digit; 2],
        rescue_cells: [CellId; 2],
        extra_values: CandidateMask,
    ) {
        let loop_values = CandidateMask::of(digits[0]).union(CandidateMask::of(digits[1]));
        let extra_count = extra_values.count() as u8;
        let first_degree = if self.config.unique_loop_fix {
            2
        } else {
            extra_count
        };
        for degree in first_degree..=7 {
            for type_offset in 0..self.active_types.len() {
                let type_index = self.active_types[type_offset];
                let Some(region_index) = self
                    .grid
                    .topology()
                    .cell_region_index(rescue_cells[0], type_index)
                else {
                    continue;
                };
                if self
                    .grid
                    .topology()
                    .cell_region_index(rescue_cells[1], type_index)
                    != Some(region_index)
                {
                    continue;
                }
                let shared = region(type_index, usize::from(region_index));
                let empty_count = self.empty_cell_count(shared);
                let first_position = self
                    .grid
                    .topology()
                    .cell_position_in_region(rescue_cells[0], type_index)
                    .expect("rescue belongs to shared region");
                let second_position = self
                    .grid
                    .topology()
                    .cell_position_in_region(rescue_cells[1], type_index)
                    .expect("rescue belongs to shared region");

                if usize::from(degree) * 2 <= empty_count
                    && (!self.config.unique_loop_fix || degree >= extra_count)
                {
                    self.search_type3_naked(
                        loop_cells,
                        digits,
                        rescue_cells,
                        extra_values,
                        shared,
                        first_position,
                        second_position,
                        degree,
                    );
                }
                if usize::from(degree) * 2 < empty_count {
                    self.search_type3_hidden(
                        loop_cells,
                        digits,
                        rescue_cells,
                        extra_values,
                        loop_values,
                        shared,
                        first_position,
                        second_position,
                        degree,
                    );
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn search_type3_naked(
        &mut self,
        loop_cells: &[CellId],
        digits: [Digit; 2],
        rescue_cells: [CellId; 2],
        extra_values: CandidateMask,
        shared: RegionId,
        first_position: u8,
        second_position: u8,
        degree: u8,
    ) {
        for subset in CombinationMasks::new(degree, 9) {
            let positions = PositionMask::from_bits(subset);
            if !positions.contains(first_position) || positions.contains(second_position) {
                continue;
            }
            let mut tuple_values = CandidateMask::EMPTY;
            let mut naked_gate = extra_values
                .intersect(self.grid.candidates(rescue_cells[0]))
                .intersect(self.grid.candidates(rescue_cells[1]));
            let mut helper_cells = [cell(0); 6];
            let mut helper_count = 0;
            let mut valid = true;
            for position in positions.iter() {
                let values = if position == first_position {
                    extra_values
                } else {
                    let helper = self.region_cell(shared, position);
                    helper_cells[helper_count] = helper;
                    helper_count += 1;
                    let values = self.grid.candidates(helper);
                    naked_gate = naked_gate.union(values);
                    values
                };
                if values.count() <= 1 {
                    valid = false;
                    break;
                }
                tuple_values = tuple_values.union(values);
            }
            if !valid
                || naked_gate.count() != u32::from(degree)
                || tuple_values.count() != u32::from(degree)
            {
                continue;
            }
            let helpers = &helper_cells[..helper_count];
            let removals = self.type3_naked_removals(shared, rescue_cells, helpers, tuple_values);
            if removals.is_empty() {
                continue;
            }
            self.consider(
                loop_cells,
                digits,
                UniqueLoopKind::Type3Naked {
                    rescue_cells,
                    region: shared,
                    extra_values,
                    set_cells: sequence(helpers),
                    set_values: tuple_values,
                },
                Rating::from_tenths(
                    self.base_rating_tenths(loop_cells.len()) + u16::from(degree - 1),
                ),
                self.base_java_difficulty(loop_cells.len()) + f64::from(degree - 1) * 0.1,
                removals,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn search_type3_hidden(
        &mut self,
        loop_cells: &[CellId],
        digits: [Digit; 2],
        rescue_cells: [CellId; 2],
        extra_values: CandidateMask,
        loop_values: CandidateMask,
        shared: RegionId,
        first_position: u8,
        second_position: u8,
        degree: u8,
    ) {
        let mut remaining = [digits[0]; 7];
        let mut remaining_count = 0;
        for digit in CandidateMask::ALL
            .without(loop_values)
            .without(extra_values)
            .iter()
        {
            remaining[remaining_count] = digit;
            remaining_count += 1;
        }
        let selected_count = degree - 2;
        if usize::from(selected_count) > remaining_count {
            return;
        }
        for subset in CombinationMasks::new(selected_count, remaining_count as u8) {
            let mut hidden_values = loop_values;
            let mut position_union = PositionMask::EMPTY;
            let mut valid = true;
            for index in PositionMask::from_bits(subset).iter() {
                let digit = remaining[usize::from(index)];
                hidden_values = hidden_values.union(CandidateMask::of(digit));
                let mut positions = self.grid.region_candidate_positions(shared, digit);
                positions.remove(second_position);
                if positions.is_empty() {
                    valid = false;
                    break;
                }
                position_union = position_union.union(positions);
            }
            if !valid {
                continue;
            }
            for digit in digits {
                let mut positions = self.grid.region_candidate_positions(shared, digit);
                positions.remove(second_position);
                if positions.is_empty() {
                    valid = false;
                    break;
                }
                position_union = position_union.union(positions);
            }
            if !valid || position_union.count() != u32::from(degree) {
                continue;
            }
            let mut hidden_positions = position_union;
            hidden_positions.remove(first_position);
            hidden_positions.remove(second_position);
            let mut builder =
                CandidateRemovalsBuilder::with_capacity(hidden_positions.count() as usize);
            for position in hidden_positions.iter() {
                let target = self.region_cell(shared, position);
                builder.add(target, self.grid.candidates(target).without(hidden_values));
            }
            let removals = builder.build();
            if removals.is_empty() {
                continue;
            }
            let hidden_count = hidden_positions.count() as u16;
            let rating_addition = match self.config.rating_mode {
                RatingMode::Revised => hidden_count,
                RatingMode::Original => hidden_count - 1,
            };
            let java_addition = match self.config.rating_mode {
                RatingMode::Revised => f64::from(hidden_count) * 0.1,
                RatingMode::Original => f64::from(hidden_count - 1) * 0.1,
            };
            self.consider(
                loop_cells,
                digits,
                UniqueLoopKind::Type3Hidden {
                    rescue_cells,
                    region: shared,
                    extra_values,
                    hidden_positions,
                    hidden_values,
                },
                Rating::from_tenths(self.base_rating_tenths(loop_cells.len()) + rating_addition),
                self.base_java_difficulty(loop_cells.len()) + java_addition,
                removals,
            );
        }
    }

    fn type3_naked_removals(
        &self,
        shared: RegionId,
        rescue_cells: [CellId; 2],
        helpers: &[CellId],
        tuple_values: CandidateMask,
    ) -> CandidateRemovals {
        let mut builder = CandidateRemovalsBuilder::with_capacity(16);
        if effective_variant_latin(self.grid, self.config) {
            for position in 0_u8..9 {
                let target = self.region_cell(shared, position);
                if target == rescue_cells[0]
                    || target == rescue_cells[1]
                    || helpers.contains(&target)
                {
                    continue;
                }
                builder.add(target, self.grid.candidates(target).intersect(tuple_values));
            }
        } else {
            for digit in tuple_values.iter() {
                let mut victims = None;
                for support in helpers
                    .iter()
                    .copied()
                    .chain(rescue_cells)
                    .filter(|&support| self.grid.candidates(support).contains(digit))
                {
                    victims = Some(victims.map_or_else(
                        || self.grid.topology().visible_mask(support),
                        |current: CellMask| {
                            current.intersect(self.grid.topology().visible_mask(support))
                        },
                    ));
                }
                let Some(victims) = victims else {
                    continue;
                };
                for victim in victims.intersect(self.grid.candidate_cells(digit)).iter() {
                    builder.add(victim, CandidateMask::of(digit));
                }
            }
        }
        builder.build()
    }

    fn empty_cell_count(&self, shared: RegionId) -> usize {
        self.grid
            .topology()
            .region_cells(shared)
            .iter()
            .filter(|&&raw| self.grid.value(cell(raw)) == 0)
            .count()
    }

    fn region_cell(&self, shared: RegionId, position: u8) -> CellId {
        cell(self.grid.topology().region_cells(shared)[usize::from(position)])
    }

    fn create_type2(
        &mut self,
        loop_cells: &[CellId],
        digits: [Digit; 2],
        extra_cells: &[CellId],
        digit: Digit,
    ) {
        let mut victims = self.grid.topology().visible_mask(extra_cells[0]);
        for &extra_cell in &extra_cells[1..] {
            victims = victims.intersect(self.grid.topology().visible_mask(extra_cell));
        }
        for &extra_cell in extra_cells {
            victims.remove(extra_cell);
        }
        victims = victims.intersect(self.grid.candidate_cells(digit));
        let mut builder = CandidateRemovalsBuilder::with_capacity(victims.count() as usize);
        for victim in victims.iter() {
            builder.add(victim, CandidateMask::of(digit));
        }
        let removals = builder.build();
        if removals.is_empty() {
            return;
        }
        self.consider(
            loop_cells,
            digits,
            UniqueLoopKind::Type2 {
                extra_cells: sequence(extra_cells),
                digit,
            },
            self.base_rating(loop_cells.len()),
            self.base_java_difficulty(loop_cells.len()),
            removals,
        );
    }

    fn create_type4(
        &mut self,
        loop_cells: &[CellId],
        digits: [Digit; 2],
        rescue_cells: [CellId; 2],
    ) {
        let mut first_region = None;
        let mut second_region = None;
        for &type_index in &self.active_types {
            let Some(first_index) = self
                .grid
                .topology()
                .cell_region_index(rescue_cells[0], type_index)
            else {
                continue;
            };
            if self
                .grid
                .topology()
                .cell_region_index(rescue_cells[1], type_index)
                != Some(first_index)
            {
                continue;
            }
            let shared = region(type_index, usize::from(first_index));
            let mut has_first = false;
            let mut has_second = false;
            for &raw_cell in self.grid.topology().region_cells(shared) {
                let other = cell(raw_cell);
                if rescue_cells.contains(&other) {
                    continue;
                }
                let candidates = self.grid.candidates(other);
                has_first |= candidates.contains(digits[0]);
                has_second |= candidates.contains(digits[1]);
            }
            if !has_first {
                first_region = Some(shared);
            }
            if !has_second {
                second_region = Some(shared);
            }
        }
        let (shared, lock_digit, remove_digit) = if let Some(shared) = first_region {
            (shared, digits[0], digits[1])
        } else if let Some(shared) = second_region {
            (shared, digits[1], digits[0])
        } else {
            return;
        };
        let mut builder = CandidateRemovalsBuilder::with_capacity(2);
        builder.add(rescue_cells[0], CandidateMask::of(remove_digit));
        builder.add(rescue_cells[1], CandidateMask::of(remove_digit));
        self.consider(
            loop_cells,
            [lock_digit, remove_digit],
            UniqueLoopKind::Type4 {
                rescue_cells,
                region: shared,
                lock_digit,
                remove_digit,
            },
            self.base_rating(loop_cells.len()),
            self.base_java_difficulty(loop_cells.len()),
            builder.build(),
        );
    }

    fn consider(
        &mut self,
        loop_cells: &[CellId],
        digits: [Digit; 2],
        kind: UniqueLoopKind,
        rating: Rating,
        java_difficulty: f64,
        removals: CandidateRemovals,
    ) {
        if removals.is_empty() {
            return;
        }
        let hint_type = kind.hint_type();
        if self.all.is_none()
            && let Some(best) = &self.best
            && (best.java_difficulty < java_difficulty
                || (best.java_difficulty == java_difficulty && best.hint_type <= hint_type))
        {
            return;
        }
        let inference = Inference::elimination(
            Technique::UniqueLoop,
            rating,
            removals,
            Evidence::UniqueLoop {
                loop_cells: sequence(loop_cells),
                first_digit: digits[0],
                second_digit: digits[1],
                kind,
            },
        );
        let candidate = Best {
            java_difficulty,
            hint_type,
            inference,
        };
        if let Some(all) = &mut self.all {
            // Java performs this concrete-hint equality check in discovery
            // order before its stable rank sort, retaining the first proof.
            let key = java_unique_loop_key(&candidate.inference);
            if self
                .seen
                .as_mut()
                .expect("unique-loop collector keys")
                .insert(key)
            {
                all.push(candidate);
            }
            return;
        }
        self.best = Some(candidate);
        self.stop = java_difficulty == 4.5 && hint_type == 1;
    }

    fn base_rating(&self, loop_size: usize) -> Rating {
        Rating::from_tenths(self.base_rating_tenths(loop_size))
    }

    fn base_rating_tenths(&self, loop_size: usize) -> u16 {
        let addition = match self.config.rating_mode {
            RatingMode::Revised => loop_size / 2 - 2,
            RatingMode::Original if loop_size >= 10 => 5,
            RatingMode::Original if loop_size >= 8 => 2,
            RatingMode::Original if loop_size >= 6 => 1,
            RatingMode::Original => 0,
        };
        45 + addition as u16
    }

    /// Reproduce Java's raw `double` operation order for sorting. Nominally
    /// equal one-decimal ratings can have different IEEE representations when
    /// Type 3 adds its increment in a second operation.
    fn base_java_difficulty(&self, loop_size: usize) -> f64 {
        let mut result = 4.5_f64;
        match self.config.rating_mode {
            RatingMode::Revised => result += (loop_size / 2 - 2) as f64 * 0.1,
            RatingMode::Original => {
                if loop_size >= 10 {
                    result += 0.3;
                }
                if loop_size >= 8 {
                    result += 0.2;
                } else if loop_size >= 6 {
                    result += 0.1;
                }
            }
        }
        result
    }

    fn is_restricted(&self, loop_cells: &[CellId], digits: [Digit; 2]) -> bool {
        let variant = self.grid.topology().config();
        for &loop_cell in loop_cells {
            if variant.anti_ferz || variant.anti_knight {
                for &raw_peer in self.grid.topology().chess_only_peers(loop_cell) {
                    let candidates = self.grid.candidates(cell(raw_peer));
                    if candidates.contains(digits[0]) || candidates.contains(digits[1]) {
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
                    .orthogonal_neighbors(loop_cell, variant.toroidal)
            } else {
                self.grid
                    .topology()
                    .diagonal_neighbors(loop_cell, variant.toroidal)
            };
            for &raw_neighbor in neighbors {
                let candidates = self.grid.candidates(cell(raw_neighbor));
                for digit in digits {
                    let value = digit.get();
                    if (mode.is_cyclic() || value < 9)
                        && candidates.contains(digit_wrapped(value + 1))
                    {
                        return true;
                    }
                    if (mode.is_cyclic() || value > 1)
                        && candidates.contains(digit_wrapped(value.wrapping_sub(1)))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }
}

fn active_region_types(grid: &Grid, config: EngineConfig) -> Vec<usize> {
    let mut result = Vec::with_capacity(REGION_TYPE_COUNT);
    if grid.topology().config().blocks {
        result.push(0);
    }
    result.extend([1, 2]);
    if !effective_variant_latin(grid, config) {
        for type_index in 3..REGION_TYPE_COUNT {
            if grid.topology().is_region_type_active(type_index) {
                result.push(type_index);
            }
        }
    }
    result
}

#[derive(Eq, Hash, PartialEq)]
enum JavaUniqueLoopKey {
    Type1 {
        loop_cells: CellMask,
    },
    Type2 {
        loop_cells: CellMask,
    },
    Type3Naked {
        loop_cells: CellMask,
        region: RegionId,
        set_cells: OrderedCells,
        set_values: CandidateMask,
    },
    Type3Hidden {
        loop_cells: CellMask,
        region: RegionId,
        hidden_positions: PositionMask,
        hidden_values: CandidateMask,
    },
    Type4 {
        loop_cells: CellMask,
    },
}

#[derive(Eq, Hash, PartialEq)]
struct OrderedCells {
    cells: [u8; 18],
    len: u8,
}

fn java_unique_loop_key(inference: &Inference) -> JavaUniqueLoopKey {
    let Evidence::UniqueLoop {
        loop_cells, kind, ..
    } = inference.evidence()
    else {
        unreachable!("unique-loop collector evidence")
    };
    let loop_cells = cell_sequence_mask(loop_cells);
    match kind {
        UniqueLoopKind::Type1 { .. } => JavaUniqueLoopKey::Type1 { loop_cells },
        UniqueLoopKind::Type2 { .. } => JavaUniqueLoopKey::Type2 { loop_cells },
        UniqueLoopKind::Type3Naked {
            region,
            set_cells,
            set_values,
            ..
        } => JavaUniqueLoopKey::Type3Naked {
            loop_cells,
            region,
            set_cells: ordered_cells(set_cells),
            set_values,
        },
        UniqueLoopKind::Type3Hidden {
            region,
            hidden_positions,
            hidden_values,
            ..
        } => JavaUniqueLoopKey::Type3Hidden {
            loop_cells,
            region,
            hidden_positions,
            hidden_values,
        },
        UniqueLoopKind::Type4 { .. } => JavaUniqueLoopKey::Type4 { loop_cells },
    }
}

fn cell_sequence_mask(cells: CellSequence) -> CellMask {
    let mut result = CellMask::EMPTY;
    for cell in cells.iter() {
        result.insert(cell);
    }
    result
}

fn ordered_cells(sequence: CellSequence) -> OrderedCells {
    let mut cells = [0; 18];
    for (index, cell) in sequence.iter().enumerate() {
        cells[index] = cell.raw();
    }
    OrderedCells {
        cells,
        len: sequence.len() as u8,
    }
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

fn sequence(cells: &[CellId]) -> CellSequence {
    let mut result = CellSequence::new();
    for &cell in cells {
        result.push(cell);
    }
    result
}

/// Java `Permutations` order, including its observable zero-degree corner:
/// `(0, n>0)` yields one empty subset while `(0, 0)` yields none.
struct CombinationMasks {
    next: u16,
    limit: u16,
    zero_pending: bool,
}

impl CombinationMasks {
    fn new(degree: u8, count: u8) -> Self {
        let limit = 1_u16 << count;
        if degree > count || (degree == 0 && count == 0) {
            return Self {
                next: limit,
                limit,
                zero_pending: false,
            };
        }
        if degree == 0 {
            return Self {
                next: 0,
                limit,
                zero_pending: true,
            };
        }
        Self {
            next: (1_u16 << degree) - 1,
            limit,
            zero_pending: false,
        }
    }
}

impl Iterator for CombinationMasks {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        if self.zero_pending {
            self.zero_pending = false;
            self.next = self.limit;
            return Some(0);
        }
        let current = self.next;
        if current >= self.limit {
            return None;
        }
        let smallest = current & current.wrapping_neg();
        let ripple = current + smallest;
        let shifted_ones = ((current ^ ripple) >> 2) / smallest;
        self.next = ripple | shifted_ones;
        Some(current)
    }
}

fn cell(raw: u8) -> CellId {
    CellId::new(raw).expect("cell index")
}

fn region(type_index: usize, region_index: usize) -> RegionId {
    RegionId::new(type_index as u8, region_index as u8).expect("region identity")
}

fn digit_wrapped(value: u8) -> Digit {
    Digit::new(if value == 0 {
        9
    } else if value == 10 {
        1
    } else {
        value
    })
    .expect("wrapped digit")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{
        CandidateMask, ConstraintTopology, Grid, NonConsecutiveMode, Puzzle, VariantConfig,
    };

    use super::{
        CombinationMasks, Search, active_region_types, cell, collect_unique_loop, find_unique_loop,
    };
    use crate::{EngineConfig, Evidence, Inference, Rating, RatingMode, Technique, UniqueLoopKind};

    fn sparse_snapshot(config: VariantConfig, entries: &[(usize, &[u8])]) -> Grid {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
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
    fn type1_preserves_java_loop_order_and_removes_the_pair() {
        let mut grid = sparse_snapshot(
            VariantConfig::default(),
            &[(0, &[1, 2]), (3, &[1, 2]), (9, &[1, 2]), (12, &[1, 2, 3])],
        );
        let inference = find_unique_loop(&grid, EngineConfig::default()).unwrap();
        assert_eq!(inference.technique(), Technique::UniqueLoop);
        assert_eq!(inference.rating(), Rating::from_tenths(45));
        assert_eq!(inference.name(), "Unique Rectangle type 1");
        assert_eq!(inference.short_name(), "UR1");
        assert_eq!(
            inference.description(grid.topology()),
            "Unique Rectangle type 1: Cells r1c1,r2c1,r2c4,r1c4 on 1, 2"
        );
        assert!(matches!(
            inference.evidence(),
            Evidence::UniqueLoop {
                kind: UniqueLoopKind::Type1 { .. },
                ..
            }
        ));
        inference.apply(&mut grid);
        assert_eq!(grid.candidates(cell(12)).single().unwrap().get(), 3);
    }

    #[test]
    fn full_collector_deduplicates_sorts_and_matches_compact_winner() {
        let grid = sparse_snapshot(
            VariantConfig::default(),
            &[
                (0, &[1, 2]),
                (3, &[1, 2]),
                (9, &[1, 2]),
                (12, &[1, 2, 3]),
                (54, &[4, 5]),
                (57, &[4, 5]),
                (63, &[4, 5]),
                (66, &[4, 5, 6]),
            ],
        );
        let hints = collect_unique_loop(&grid, EngineConfig::default());
        assert_eq!(hints.len(), 2);
        assert_eq!(
            find_unique_loop(&grid, EngineConfig::default()).as_ref(),
            hints.first()
        );
        assert_eq!(
            hints
                .iter()
                .map(|hint| hint.description(grid.topology()))
                .collect::<Vec<_>>(),
            [
                "Unique Rectangle type 1: Cells r1c1,r2c1,r2c4,r1c4 on 1, 2",
                "Unique Rectangle type 1: Cells r7c1,r8c1,r8c4,r7c4 on 4, 5",
            ]
        );

        for (expected_name, entries) in [
            (
                "Unique Rectangle type 2",
                &[
                    (0, &[1, 2][..]),
                    (3, &[1, 2]),
                    (9, &[1, 2, 3]),
                    (12, &[1, 2, 3]),
                    (10, &[1, 2, 3]),
                ][..],
            ),
            (
                "Unique Rectangle type 3",
                &[
                    (0, &[1, 2][..]),
                    (3, &[1, 2]),
                    (9, &[1, 2, 3]),
                    (12, &[1, 2, 4]),
                    (10, &[3, 4]),
                    (11, &[3]),
                    (16, &[1, 2]),
                ],
            ),
            (
                "Unique Rectangle type 4",
                &[
                    (0, &[1, 2][..]),
                    (3, &[1, 2]),
                    (9, &[1, 2, 3]),
                    (12, &[1, 2, 4]),
                ],
            ),
        ] {
            let case = sparse_snapshot(VariantConfig::default(), entries);
            let collected = collect_unique_loop(&case, EngineConfig::default());
            assert_eq!(
                collected.first().map(Inference::name).as_deref(),
                Some(expected_name)
            );
            assert_eq!(
                find_unique_loop(&case, EngineConfig::default()).as_ref(),
                collected.first()
            );
        }
    }

    #[test]
    fn type2_intersects_every_rescue_visibility_set() {
        let mut grid = sparse_snapshot(
            VariantConfig::default(),
            &[
                (0, &[1, 2]),
                (3, &[1, 2]),
                (9, &[1, 2, 3]),
                (12, &[1, 2, 3]),
                (10, &[1, 2, 3]),
            ],
        );
        let inference = find_unique_loop(&grid, EngineConfig::default()).unwrap();
        assert_eq!(inference.name(), "Unique Rectangle type 2");
        assert_eq!(inference.rating(), Rating::from_tenths(45));
        assert_eq!(inference.removals().elimination_count(), 1);
        inference.apply(&mut grid);
        assert_eq!(
            grid.candidates(cell(10)).bits(),
            CandidateMask::from_bits(0b110).bits()
        );
    }

    #[test]
    fn type3_naked_uses_the_rescue_extra_union_as_a_pseudo_cell() {
        let mut grid = sparse_snapshot(
            VariantConfig::default(),
            &[
                (0, &[1, 2]),
                (3, &[1, 2]),
                (9, &[1, 2, 3]),
                (12, &[1, 2, 4]),
                (10, &[3, 4]),
                (11, &[3]),
                (16, &[1, 2]),
            ],
        );
        let inference = find_unique_loop(&grid, EngineConfig::default()).unwrap();
        assert_eq!(inference.name(), "Unique Rectangle type 3");
        assert_eq!(inference.rating(), Rating::from_tenths(46));
        assert!(matches!(
            inference.evidence(),
            Evidence::UniqueLoop {
                kind: UniqueLoopKind::Type3Naked { .. },
                ..
            }
        ));
        inference.apply(&mut grid);
        assert!(grid.candidates(cell(11)).is_empty());
    }

    #[test]
    fn type3_hidden_keeps_java_original_and_revised_rating_difference() {
        let entries = [
            (0, &[1, 2][..]),
            (3, &[1, 2]),
            (9, &[1, 2, 4]),
            (12, &[1, 2, 5]),
            (10, &[1, 2, 3]),
            (11, &[4, 5]),
        ];
        let mut original_grid = sparse_snapshot(VariantConfig::default(), &entries);
        let original = find_unique_loop(&original_grid, EngineConfig::default()).unwrap();
        assert_eq!(original.name(), "Unique Rectangle type 3");
        assert_eq!(original.rating(), Rating::from_tenths(45));
        assert!(matches!(
            original.evidence(),
            Evidence::UniqueLoop {
                kind: UniqueLoopKind::Type3Hidden { .. },
                ..
            }
        ));
        original.apply(&mut original_grid);
        assert_eq!(original_grid.candidates(cell(10)).bits(), 0b110);

        let revised_grid = sparse_snapshot(VariantConfig::default(), &entries);
        let revised = find_unique_loop(
            &revised_grid,
            EngineConfig {
                rating_mode: RatingMode::Revised,
                ..EngineConfig::default()
            },
        )
        .unwrap();
        assert_eq!(revised.rating(), Rating::from_tenths(46));
    }

    #[test]
    fn type3_naked_generalizes_removals_through_anti_knight_visibility() {
        let entries = [
            (0, &[1, 2][..]),
            (3, &[1, 2]),
            (9, &[1, 2, 3]),
            (12, &[1, 2, 4]),
            (10, &[3, 4]),
            (16, &[1, 2]),
            (27, &[3, 5]),
        ];
        let classic = sparse_snapshot(VariantConfig::default(), &entries);
        assert!(find_unique_loop(&classic, EngineConfig::default()).is_none());

        let mut anti_knight = sparse_snapshot(
            VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            },
            &entries,
        );
        let inference = find_unique_loop(&anti_knight, EngineConfig::default()).unwrap();
        assert_eq!(inference.name(), "Unique Rectangle type 3");
        assert_eq!(inference.rating(), Rating::from_tenths(46));
        inference.apply(&mut anti_knight);
        assert_eq!(anti_knight.candidates(cell(27)).single().unwrap().get(), 5);
    }

    #[test]
    fn type4_prefers_the_first_digit_but_can_reverse_description_digits() {
        let entries = [
            (0, &[1, 2][..]),
            (3, &[1, 2]),
            (9, &[1, 2, 3]),
            (12, &[1, 2, 4]),
        ];
        let grid = sparse_snapshot(VariantConfig::default(), &entries);
        let inference = find_unique_loop(&grid, EngineConfig::default()).unwrap();
        assert_eq!(inference.name(), "Unique Rectangle type 4");
        assert!(inference.description(grid.topology()).ends_with("on 1, 2"));

        let reversed_entries = [
            (0, &[1, 2][..]),
            (3, &[1, 2]),
            (9, &[1, 2, 3]),
            (12, &[1, 2, 4]),
            (10, &[1]),
        ];
        let reversed = sparse_snapshot(VariantConfig::default(), &reversed_entries);
        let inference = find_unique_loop(&reversed, EngineConfig::default()).unwrap();
        assert!(
            inference
                .description(reversed.topology())
                .ends_with("on 2, 1")
        );
    }

    #[test]
    fn variant_restrictions_do_not_depend_on_the_forbidden_pairs_flag() {
        let entries = [
            (0, &[1, 2][..]),
            (3, &[1, 2]),
            (9, &[1, 2]),
            (12, &[1, 2, 3]),
        ];
        let non_consecutive = sparse_snapshot(
            VariantConfig {
                non_consecutive: NonConsecutiveMode::Orthogonal,
                forbidden_pairs: false,
                ..VariantConfig::default()
            },
            &entries,
        );
        assert!(find_unique_loop(&non_consecutive, EngineConfig::default()).is_none());

        let mut anti_entries = entries.to_vec();
        anti_entries.push((28, &[1]));
        let anti_knight = sparse_snapshot(
            VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            },
            &anti_entries,
        );
        assert!(find_unique_loop(&anti_knight, EngineConfig::default()).is_none());
    }

    #[test]
    fn raw_java_double_order_is_not_collapsed_to_display_tenths() {
        let grid = sparse_snapshot(VariantConfig::default(), &[]);
        let config = EngineConfig::default();
        let search = Search {
            grid: &grid,
            config,
            active_types: active_region_types(&grid, config),
            best: None,
            all: None,
            seen: None,
            stop: false,
        };
        assert_eq!(search.base_rating_tenths(8), 47);
        assert_eq!(search.base_rating_tenths(6) + 1, 47);
        assert!(search.base_java_difficulty(6) + 0.1 < search.base_java_difficulty(8));

        let revised = EngineConfig {
            rating_mode: RatingMode::Revised,
            ..EngineConfig::default()
        };
        let search = Search {
            grid: &grid,
            config: revised,
            active_types: active_region_types(&grid, revised),
            best: None,
            all: None,
            seen: None,
            stop: false,
        };
        assert_eq!(search.base_rating_tenths(18), 52);
    }

    #[test]
    fn zero_degree_combinations_match_java_corner_case() {
        assert_eq!(CombinationMasks::new(0, 0).collect::<Vec<_>>(), []);
        assert_eq!(CombinationMasks::new(0, 3).collect::<Vec<_>>(), [0]);
        assert_eq!(
            CombinationMasks::new(3, 5).collect::<Vec<_>>(),
            [7, 11, 13, 14, 19, 21, 22, 25, 26, 28]
        );
    }
}
