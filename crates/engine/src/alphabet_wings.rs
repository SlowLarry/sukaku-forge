use std::cmp::Ordering;

use sukaku_forge_core::{
    CandidateMask, CandidateRemovals, CandidateRemovalsBuilder, CellId, CellMask, Digit, Grid,
};

use crate::{CellSequence, Evidence, Inference, Rating, Technique};

/// Find the Java-compatible generalized ALS wing of size `degree`.
///
/// Degrees four through seven are WXYZ-, VWXYZ-, UVWXYZ- and TUVWXYZ-Wing.
/// The Java implementation spells these as four deeply nested searches.  This
/// port keeps the same observable traversal in one allocation-free clique DFS:
/// the first peer uses topology order, while every later intersection is in
/// ascending cell order, exactly like Java's sorted scratch arrays.
#[must_use]
pub fn find_alphabet_wing(grid: &Grid, degree: u8) -> Option<Inference> {
    assert!((4..=7).contains(&degree));
    let mut search = AlphabetWingSearch {
        grid,
        degree,
        cells: [0; 6],
        values: [CandidateMask::EMPTY; 6],
        best: None,
        all: None,
    };
    search.run();
    search.best.map(|draft| draft.inference)
}

/// Java runs every hint from the first productive advanced-rule family before
/// returning to dynamic propagation.  The public producer above needs only
/// the globally ranked winner; nested forcing chains need this complete,
/// stably sorted payload instead.
pub(crate) fn collect_alphabet_wing_advanced(grid: &Grid, degree: u8) -> Vec<AdvancedAlphabetWing> {
    assert!((4..=5).contains(&degree));
    let mut search = AlphabetWingSearch {
        grid,
        degree,
        cells: [0; 6],
        values: [CandidateMask::EMPTY; 6],
        best: None,
        all: Some(Vec::new()),
    };
    search.run();
    let mut result = search.all.expect("advanced alphabet-wing collector");
    // `Collections.sort` is stable.  Its comparator uses ascending difficulty,
    // descending elimination count, then reverse-lexicographic suffix order.
    result.sort_by(AdvancedAlphabetWing::java_compare);
    result
}

pub(crate) struct AdvancedAlphabetWing {
    pub(crate) selected_cells: CellSequence,
    pub(crate) removals: CandidateRemovals,
    rating: u16,
    eliminations: u16,
    suffix: String,
}

impl AdvancedAlphabetWing {
    fn java_compare(left: &Self, right: &Self) -> Ordering {
        left.rating
            .cmp(&right.rating)
            .then_with(|| right.eliminations.cmp(&left.eliminations))
            .then_with(|| right.suffix.cmp(&left.suffix))
    }
}

struct AlphabetWingSearch<'a> {
    grid: &'a Grid,
    degree: u8,
    cells: [u8; 6],
    values: [CandidateMask; 6],
    best: Option<AlphabetWingDraft>,
    all: Option<Vec<AdvancedAlphabetWing>>,
}

struct AlphabetWingDraft {
    inference: Inference,
    rating: u16,
    eliminations: u16,
    suffix: String,
}

impl AlphabetWingSearch<'_> {
    fn run(&mut self) {
        for raw in 0_u8..81 {
            let cell = cell(raw);
            let values = self.grid.candidates(cell);
            let cardinality = values.count() as u8;
            if !(2..=self.degree).contains(&cardinality) {
                continue;
            }
            self.cells[0] = raw;
            self.values[0] = values;
            self.extend(1, values, cardinality, cardinality);
        }
    }

    fn extend(
        &mut self,
        depth: usize,
        union: CandidateMask,
        biggest_cardinality: u8,
        wing_size: u8,
    ) {
        let required_cells = usize::from(self.degree - 1);
        if depth == required_cells {
            self.evaluate(union, biggest_cardinality, wing_size);
            return;
        }

        let last = cell(self.cells[depth - 1]);
        if depth == 1 {
            // Java deliberately retains the topology's ordered forward-peer
            // catalog for this first edge.
            for &raw in self.grid.topology().forward_visible_peers(last) {
                self.try_cell(raw, depth, union, biggest_cardinality, wing_size);
            }
        } else {
            // Every deeper Java level filters the last cell's forward peers,
            // then Arrays.sorts the live prefix.  The visibility mask already
            // iterates actual peers in that ascending order.
            for candidate in self.grid.topology().visible_mask(last).iter() {
                let raw = candidate.raw();
                if raw <= self.cells[depth - 1] {
                    continue;
                }
                self.try_cell(raw, depth, union, biggest_cardinality, wing_size);
            }
        }
    }

    fn try_cell(
        &mut self,
        raw: u8,
        depth: usize,
        union: CandidateMask,
        biggest_cardinality: u8,
        wing_size: u8,
    ) {
        let candidate = cell(raw);
        if (0..depth.saturating_sub(1)).any(|index| {
            !self
                .grid
                .topology()
                .visible_mask(cell(self.cells[index]))
                .contains(candidate)
        }) {
            return;
        }
        let values = self.grid.candidates(candidate);
        let cardinality = values.count() as u8;
        if cardinality <= 1 {
            return;
        }
        let next_union = union.union(values);
        if next_union.count() > u32::from(self.degree) {
            return;
        }
        let final_cell = depth + 1 == usize::from(self.degree - 1);
        if final_cell && next_union.count() != u32::from(self.degree) {
            return;
        }

        self.cells[depth] = raw;
        self.values[depth] = values;
        self.extend(
            depth + 1,
            next_union,
            biggest_cardinality.max(cardinality),
            wing_size + cardinality,
        );
    }

    fn evaluate(&mut self, wing_set: CandidateMask, biggest_cardinality: u8, wing_size: u8) {
        let pattern_len = usize::from(self.degree - 1);
        let mut yz_range = CellMask::EMPTY;
        for index in 0..pattern_len {
            yz_range = union_cells(
                yz_range,
                self.grid.topology().visible_mask(cell(self.cells[index])),
            );
        }
        for index in 0..pattern_len {
            yz_range.remove(cell(self.cells[index]));
        }

        for yz_cell in yz_range.iter() {
            let yz_values = self.grid.candidates(yz_cell);
            if yz_values.count() != 2 || yz_values.intersect(wing_set).count() != 2 {
                continue;
            }
            let mut digits = yz_values.iter();
            let x_value = digits.next().expect("bivalue cell");
            let z_value = digits.next().expect("bivalue cell");
            let double_link = self.is_linked(yz_cell, z_value, pattern_len);
            if self.is_linked(yz_cell, x_value, pattern_len) {
                self.create_draft(
                    yz_cell,
                    x_value,
                    z_value,
                    double_link,
                    wing_set,
                    biggest_cardinality,
                    wing_size,
                    pattern_len,
                );
            } else if double_link {
                // Only the higher digit is linked.  Java swaps the two formal
                // X/Z arguments before constructing a single-linked draft.
                self.create_draft(
                    yz_cell,
                    z_value,
                    x_value,
                    false,
                    wing_set,
                    biggest_cardinality,
                    wing_size,
                    pattern_len,
                );
            }
        }
    }

    fn is_linked(&self, yz_cell: CellId, digit: Digit, pattern_len: usize) -> bool {
        let visible = self.grid.topology().visible_mask(yz_cell);
        (0..pattern_len).all(|index| {
            !self.values[index].contains(digit) || visible.contains(cell(self.cells[index]))
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn create_draft(
        &mut self,
        yz_cell: CellId,
        x_value: Digit,
        z_value: Digit,
        mut double_link: bool,
        wing_set: CandidateMask,
        biggest_cardinality: u8,
        wing_size: u8,
        pattern_len: usize,
    ) {
        let mut removals = CandidateRemovalsBuilder::with_capacity(16);
        let mut eliminations = 0_u16;
        let mut weak_potentials = false;
        let mut strong_potentials_x = false;

        if double_link {
            for value in wing_set.without(self.grid.candidates(yz_cell)).iter() {
                let count =
                    self.add_eliminations(yz_cell, value, false, pattern_len, &mut removals);
                eliminations += count;
                weak_potentials |= count != 0;
            }
            let count = self.add_eliminations(yz_cell, x_value, true, pattern_len, &mut removals);
            eliminations += count;
            strong_potentials_x = count != 0;
        }

        let count = self.add_eliminations(yz_cell, z_value, true, pattern_len, &mut removals);
        eliminations += count;
        let strong_potentials_z = count != 0;

        let (display_x, display_z) = if double_link && !weak_potentials {
            if !strong_potentials_z {
                double_link = false;
                (z_value, x_value)
            } else {
                if !strong_potentials_x {
                    double_link = false;
                }
                (x_value, z_value)
            }
        } else {
            (x_value, z_value)
        };

        let removals = removals.build();
        if removals.is_empty() {
            return;
        }

        let suffix = format!(
            "{}{}{}",
            if double_link { 2 } else { 1 },
            biggest_cardinality,
            wing_size
        );
        let rating = rating_tenths(self.degree, biggest_cardinality);
        let mut pattern_cells = CellSequence::new();
        for index in 0..pattern_len {
            pattern_cells.push(cell(self.cells[index]));
        }
        pattern_cells.push(yz_cell);
        let inference = Inference::elimination(
            technique(self.degree),
            Rating::from_tenths(rating),
            removals.clone(),
            Evidence::AlphabetWing {
                pattern_cells,
                x_digit: display_x,
                z_digit: display_z,
                double_link,
                biggest_cardinality,
                wing_size,
                wing_set,
            },
        );
        let candidate = AlphabetWingDraft {
            inference,
            rating,
            eliminations,
            suffix,
        };
        if let Some(all) = &mut self.all {
            all.push(AdvancedAlphabetWing {
                selected_cells: pattern_cells,
                removals,
                rating,
                eliminations,
                suffix: candidate.suffix.clone(),
            });
        }
        if self
            .best
            .as_ref()
            .is_none_or(|best| candidate.precedes(best))
        {
            self.best = Some(candidate);
        }
    }

    fn add_eliminations(
        &self,
        yz_cell: CellId,
        digit: Digit,
        include_yz: bool,
        pattern_len: usize,
        removals: &mut CandidateRemovalsBuilder,
    ) -> u16 {
        let mut victims = include_yz.then(|| self.grid.topology().visible_mask(yz_cell));
        for index in 0..pattern_len {
            if self.values[index].contains(digit) {
                let visible = self.grid.topology().visible_mask(cell(self.cells[index]));
                victims = Some(victims.map_or(visible, |current| current.intersect(visible)));
            }
        }
        let mut victims = victims.expect("wing digit occurs in the pattern");
        for index in 0..pattern_len {
            victims.remove(cell(self.cells[index]));
        }
        victims.remove(yz_cell);
        victims = victims.intersect(self.grid.candidate_cells(digit));
        let count = victims.count() as u16;
        let value = CandidateMask::of(digit);
        for victim in victims.iter() {
            removals.add(victim, value);
        }
        count
    }
}

impl AlphabetWingDraft {
    fn precedes(&self, other: &Self) -> bool {
        self.rating < other.rating
            || (self.rating == other.rating
                && (self.eliminations > other.eliminations
                    || (self.eliminations == other.eliminations && self.suffix > other.suffix)))
    }
}

const fn technique(degree: u8) -> Technique {
    match degree {
        4 => Technique::WXYZWing,
        5 => Technique::VWXYZWing,
        6 => Technique::UVWXYZWing,
        7 => Technique::TUVWXYZWing,
        _ => unreachable!(),
    }
}

const fn rating_tenths(degree: u8, biggest_cardinality: u8) -> u16 {
    match (degree, biggest_cardinality) {
        (4, 3) => 56,
        (4, _) => 55,
        (5, 2 | 4) => 63,
        (5, 3) => 64,
        (5, _) => 62,
        (6, _) => 66,
        (7, _) => 75,
        _ => unreachable!(),
    }
}

const fn union_cells(left: CellMask, right: CellMask) -> CellMask {
    CellMask::from_words(left.low() | right.low(), left.high() | right.high())
}

fn cell(raw: u8) -> CellId {
    CellId::new(raw).expect("cell index")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{ConstraintTopology, Grid, Puzzle, VariantConfig};

    use super::find_alphabet_wing;
    use crate::{Rating, Technique};

    fn sparse_snapshot(entries: &[(usize, &[u8])]) -> Grid {
        sparse_snapshot_with_config(VariantConfig::default(), entries)
    }

    fn sparse_snapshot_with_config(config: VariantConfig, entries: &[(usize, &[u8])]) -> Grid {
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

    #[allow(clippy::too_many_arguments)]
    fn assert_wing(
        degree: u8,
        entries: &[(usize, &[u8])],
        technique: Technique,
        rating: u16,
        name: &str,
        short_name: &str,
        description: &str,
        victim: usize,
        survivor: u8,
    ) {
        let mut grid = sparse_snapshot(entries);
        let inference = find_alphabet_wing(&grid, degree).expect("alphabet wing");
        assert_eq!(inference.technique(), technique);
        assert_eq!(inference.rating(), Rating::from_tenths(rating));
        assert_eq!(inference.name(), name);
        assert_eq!(inference.short_name(), short_name);
        assert_eq!(inference.description(grid.topology()), description);
        assert_eq!(inference.removals().elimination_count(), 1);
        inference.apply(&mut grid);
        assert_eq!(
            grid.candidates(CellId::new(victim as u8).unwrap()),
            CandidateMask::from_bits(1_u16 << survivor)
        );
    }

    use sukaku_forge_core::{CandidateMask, CellId};

    #[test]
    fn wxyz_single_and_double_links_match_java() {
        let mut single = sparse_snapshot(&[
            (0, &[1, 2]),
            (1, &[1, 3]),
            (2, &[2]),
            (3, &[1, 2]),
            (9, &[2, 4]),
        ]);
        let inference = find_alphabet_wing(&single, 4).unwrap();
        assert_eq!(inference.rating(), Rating::from_tenths(55));
        assert_eq!(inference.name(), "WXYZ-Wing 126");
        assert_eq!(inference.short_name(), "WXY126");
        assert_eq!(
            inference.description(single.topology()),
            "WXYZ-Wing 126: Cells r1c1,r1c2,r2c1,r1c4 on value 2"
        );
        inference.apply(&mut single);
        assert!(single.candidates(CellId::new(2).unwrap()).is_empty());

        let mut double = sparse_snapshot(&[
            (0, &[1, 2]),
            (1, &[1, 3]),
            (9, &[2, 4]),
            (10, &[1, 2]),
            (11, &[1, 2, 3, 4, 9]),
        ]);
        let inference = find_alphabet_wing(&double, 4).unwrap();
        assert_eq!(inference.name(), "WXYZ-Wing 226");
        assert_eq!(
            inference.description(double.topology()),
            "WXYZ-Wing 226: Cells r1c1,r1c2,r2c1,r2c2 on values 1,2"
        );
        assert_eq!(inference.removals().elimination_count(), 4);
        inference.apply(&mut double);
        assert_eq!(
            double.candidates(CellId::new(11).unwrap()),
            CandidateMask::from_bits(1_u16 << 9)
        );
    }

    #[test]
    fn wxyz_selects_java_global_sort_winner() {
        let grid = sparse_snapshot(&[
            (0, &[1, 2, 3]),
            (1, &[1, 3]),
            (2, &[2]),
            (3, &[1, 2]),
            (9, &[2, 4]),
            (57, &[1, 2]),
            (58, &[1, 3]),
            (59, &[2]),
            (60, &[1, 2]),
            (66, &[2, 4]),
        ]);
        let inference = find_alphabet_wing(&grid, 4).unwrap();
        assert_eq!(inference.rating(), Rating::from_tenths(55));
        assert_eq!(
            inference.description(grid.topology()),
            "WXYZ-Wing 126: Cells r7c4,r7c5,r8c4,r7c7 on value 2"
        );
        assert_eq!(inference.removals().iter().next().unwrap().cell().raw(), 59);
    }

    #[test]
    fn wxyz_uses_anti_knight_visibility() {
        let entries = [
            (2, &[1, 2][..]),
            (3, &[1, 3]),
            (4, &[2]),
            (7, &[1, 2]),
            (13, &[2, 4]),
        ];
        assert!(find_alphabet_wing(&sparse_snapshot(&entries), 4).is_none());
        let grid = sparse_snapshot_with_config(
            VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            },
            &entries,
        );
        let inference = find_alphabet_wing(&grid, 4).unwrap();
        assert_eq!(
            inference.description(grid.topology()),
            "WXYZ-Wing 126: Cells r1c3,r1c4,r2c5,r1c8 on value 2"
        );
        assert_eq!(inference.removals().iter().next().unwrap().cell().raw(), 4);
    }

    #[test]
    fn nominal_double_link_downgrades_to_the_eliminating_digit() {
        let grid = sparse_snapshot(&[
            (0, &[1, 2]),
            (3, &[3, 4]),
            (4, &[3, 5]),
            (5, &[4, 5]),
            (9, &[1, 2]),
            (18, &[1]),
        ]);
        let inference = find_alphabet_wing(&grid, 5).unwrap();
        assert_eq!(inference.name(), "VWXYZ-Wing 128");
        assert_eq!(
            inference.description(grid.topology()),
            "VWXYZ-Wing 128: Cells r1c1,r1c4,r1c5,r1c6,r2c1 on value 1"
        );
    }

    #[test]
    fn vwxyz_single_and_double_links_match_java() {
        assert_wing(
            5,
            &[
                (0, &[1, 3]),
                (1, &[1, 4]),
                (2, &[3, 5]),
                (3, &[2, 5]),
                (9, &[1, 2]),
                (12, &[2, 8]),
            ],
            Technique::VWXYZWing,
            63,
            "VWXYZ-Wing 128",
            "VXY128",
            "VWXYZ-Wing 128: Cells r1c1,r1c2,r1c3,r1c4,r2c1 on value 2",
            12,
            8,
        );
        assert_wing(
            5,
            &[
                (0, &[1, 3]),
                (1, &[1, 4]),
                (2, &[2, 5]),
                (3, &[3, 5]),
                (4, &[3, 8]),
                (9, &[1, 2]),
            ],
            Technique::VWXYZWing,
            63,
            "VWXYZ-Wing 228",
            "VXY228",
            "VWXYZ-Wing 228: Cells r1c1,r1c2,r1c3,r1c4,r2c1 on values 1,2",
            4,
            8,
        );
    }

    #[test]
    fn uvwxyz_single_and_double_links_match_java() {
        assert_wing(
            6,
            &[
                (0, &[1, 3]),
                (1, &[1, 4]),
                (2, &[3, 5]),
                (3, &[2, 6]),
                (4, &[5, 6]),
                (9, &[1, 2]),
                (12, &[2, 8]),
            ],
            Technique::UVWXYZWing,
            66,
            "UVWXYZ-Wing 1210",
            "UXY1210",
            "UVWXYZ-Wing 1210: Cells r1c1,r1c2,r1c3,r1c4,r1c5,r2c1 on value 2",
            12,
            8,
        );
        assert_wing(
            6,
            &[
                (0, &[1, 3]),
                (1, &[1, 4]),
                (2, &[2, 5]),
                (3, &[3, 6]),
                (4, &[5, 6]),
                (5, &[3, 8]),
                (9, &[1, 2]),
            ],
            Technique::UVWXYZWing,
            66,
            "UVWXYZ-Wing 2210",
            "UXY2210",
            "UVWXYZ-Wing 2210: Cells r1c1,r1c2,r1c3,r1c4,r1c5,r2c1 on values 1,2",
            5,
            8,
        );
    }

    #[test]
    fn tuvwxyz_single_and_double_links_match_java() {
        assert_wing(
            7,
            &[
                (0, &[1, 3]),
                (1, &[1, 4]),
                (2, &[3, 5]),
                (3, &[2, 6]),
                (4, &[5, 7]),
                (5, &[6, 7]),
                (9, &[1, 2]),
                (12, &[2, 8]),
            ],
            Technique::TUVWXYZWing,
            75,
            "TUVWXYZ-Wing 1212",
            "TXY1212",
            "TUVWXYZ-Wing 1212: Cells r1c1,r1c2,r1c3,r1c4,r1c5,r1c6,r2c1 on value 2",
            12,
            8,
        );
        assert_wing(
            7,
            &[
                (0, &[1, 3]),
                (1, &[1, 4]),
                (2, &[2, 5]),
                (3, &[3, 6]),
                (4, &[5, 7]),
                (5, &[6, 7]),
                (6, &[3, 8]),
                (9, &[1, 2]),
            ],
            Technique::TUVWXYZWing,
            75,
            "TUVWXYZ-Wing 2212",
            "TXY2212",
            "TUVWXYZ-Wing 2212: Cells r1c1,r1c2,r1c3,r1c4,r1c5,r1c6,r2c1 on values 1,2",
            6,
            8,
        );
    }
}
