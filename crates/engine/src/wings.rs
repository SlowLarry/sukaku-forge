use sukaku_forge_core::{
    CandidateMask, CandidateRemovalsBuilder, CellId, Grid, se121_classic_peers,
};

use crate::{Evidence, Inference, Rating, Technique};

/// Find the first Java-compatible XY-Wing or XYZ-Wing elimination.
///
/// Wing cells deliberately follow the topology's ordered peer catalog. Extra
/// region and anti-chess peers are appended in an observable Java order, so a
/// bit-mask iteration would not be equivalent here. Victims, on the other
/// hand, come from Java's `CellSet` and therefore use ascending cell order.
#[must_use]
pub fn find_wing(grid: &Grid, xyz: bool) -> Option<Inference> {
    find_wing_with_order(grid, xyz, false)
}

/// Find an XY-/XYZ-Wing in the block-row-column peer order of SE 1.2.1.
#[must_use]
pub(crate) fn find_wing_se121(grid: &Grid, xyz: bool) -> Option<Inference> {
    find_wing_with_order(grid, xyz, true)
}

fn find_wing_with_order(grid: &Grid, xyz: bool, se121_order: bool) -> Option<Inference> {
    let mut first = None;
    visit_wings(grid, xyz, se121_order, &mut |inference| {
        first = Some(inference);
        false
    });
    first
}

/// Collect every Java-compatible XY-Wing or XYZ-Wing in discovery order.
#[must_use]
pub fn collect_wings(grid: &Grid, xyz: bool) -> Vec<Inference> {
    let mut keys = Vec::new();
    let mut inferences = Vec::new();
    visit_wings(grid, xyz, false, &mut |inference| {
        let key = wing_equality_key(&inference);
        if !keys.contains(&key) {
            keys.push(key);
            inferences.push(inference);
        }
        true
    });
    inferences
}

fn wing_equality_key(inference: &Inference) -> (bool, u8, u8, u8, u8) {
    // XYWingHint treats the two wing cells as an unordered pair but retains
    // the XY/XYZ flag, pivot, and elimination value.
    let Evidence::Wing {
        pivot,
        xz,
        yz,
        digit,
    } = inference.evidence()
    else {
        unreachable!("wing equality key evidence")
    };
    let (first_wing, second_wing) = if xz.raw() <= yz.raw() {
        (xz.raw(), yz.raw())
    } else {
        (yz.raw(), xz.raw())
    };
    (
        inference.technique() == Technique::XYZWing,
        pivot.raw(),
        first_wing,
        second_wing,
        digit.get(),
    )
}

fn visit_wings(grid: &Grid, xyz: bool, se121_order: bool, emit: &mut dyn FnMut(Inference) -> bool) {
    let pivot_cardinality = if xyz { 3 } else { 2 };
    for raw_pivot in 0_u8..81 {
        let pivot = cell(raw_pivot);
        let pivot_values = grid.candidates(pivot);
        if pivot_values.count() != pivot_cardinality {
            continue;
        }

        let peers: &[u8] = if se121_order {
            se121_classic_peers(pivot)
        } else {
            grid.topology().visible_peers(pivot)
        };
        for &raw_xz in peers {
            let xz = cell(raw_xz);
            let xz_values = grid.candidates(xz);
            if xz_values.count() != 2 || pivot_values.without(xz_values).count() != 1 {
                continue;
            }

            for &raw_yz in peers {
                let yz = cell(raw_yz);
                let yz_values = grid.candidates(yz);
                if yz_values.count() != 2 {
                    continue;
                }
                let union = pivot_values.union(xz_values).union(yz_values);
                if union.count() != 3 {
                    continue;
                }
                let common = pivot_values.intersect(xz_values).intersect(yz_values);
                if (!xyz && !common.is_empty()) || (xyz && common.count() != 1) {
                    continue;
                }
                let Some(digit) = xz_values.intersect(yz_values).single() else {
                    continue;
                };

                let mut victims = grid
                    .topology()
                    .visible_mask(xz)
                    .intersect(grid.topology().visible_mask(yz));
                if xyz {
                    victims = victims.intersect(grid.topology().visible_mask(pivot));
                }
                victims.remove(pivot);
                victims.remove(xz);
                victims.remove(yz);
                victims = victims.intersect(grid.candidate_cells(digit));
                if victims.is_empty() {
                    continue;
                }

                let mut removals =
                    CandidateRemovalsBuilder::with_capacity(victims.count() as usize);
                for victim in victims.iter() {
                    removals.add(victim, CandidateMask::of(digit));
                }
                if !emit(Inference::elimination(
                    if xyz {
                        Technique::XYZWing
                    } else {
                        Technique::XYWing
                    },
                    Rating::from_tenths(if xyz { 44 } else { 42 }),
                    removals.build(),
                    Evidence::Wing {
                        pivot,
                        xz,
                        yz,
                        digit,
                    },
                )) {
                    return;
                }
            }
        }
    }
}

fn cell(raw: u8) -> CellId {
    CellId::new(raw).expect("cell index")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{CellId, ConstraintTopology, Grid, Puzzle, VariantConfig};

    use super::{collect_wings, find_wing};
    use crate::{Evidence, Rating, Technique};

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
    fn xy_wing_matches_the_java_classic_fixture() {
        let mut grid = sparse_snapshot(
            VariantConfig::default(),
            &[(0, &[1, 2]), (3, &[1, 3]), (27, &[2, 3]), (30, &[3])],
        );
        let inference = find_wing(&grid, false).unwrap();
        let mut raw = Vec::new();
        super::visit_wings(&grid, false, false, &mut |inference| {
            raw.push(inference);
            true
        });
        assert_eq!(raw.len(), 2, "the two Java wing-cell orientations");
        let all = collect_wings(&grid, false);
        assert_eq!(Some(inference.clone()), all.first().cloned());
        assert_eq!(
            all.iter()
                .map(|inference| inference.description(grid.topology()))
                .collect::<Vec<_>>(),
            ["XY-Wing: Cells r1c1,r1c4,r4c1 on value 3",]
        );
        assert_eq!(inference.technique(), Technique::XYWing);
        assert_eq!(inference.rating(), Rating::from_tenths(42));
        assert_eq!(
            inference.description(grid.topology()),
            "XY-Wing: Cells r1c1,r1c4,r4c1 on value 3"
        );
        assert_eq!(inference.removals().elimination_count(), 1);
        assert!(matches!(
            inference.evidence(),
            Evidence::Wing {
                pivot,
                xz,
                yz,
                digit
            } if pivot.raw() == 0 && xz.raw() == 3 && yz.raw() == 27 && digit.get() == 3
        ));
        inference.apply(&mut grid);
        assert!(grid.candidates(CellId::new(30).unwrap()).is_empty());
        assert_eq!(grid.candidates(CellId::new(0).unwrap()).count(), 2);
    }

    #[test]
    fn xy_wing_uses_extra_region_visibility() {
        let entries = [(0, &[1, 2][..]), (3, &[2, 3]), (30, &[1, 3]), (12, &[3])];
        assert!(find_wing(&sparse_snapshot(VariantConfig::default(), &entries), false).is_none());

        let disjoint = sparse_snapshot(
            VariantConfig {
                disjoint_groups: true,
                ..VariantConfig::default()
            },
            &entries,
        );
        let inference = find_wing(&disjoint, false).unwrap();
        assert_eq!(
            inference.description(disjoint.topology()),
            "XY-Wing: Cells r1c1,r1c4,r4c4 on value 3"
        );
        assert_eq!(inference.removals().iter().next().unwrap().cell().raw(), 12);
    }

    #[test]
    fn xy_wing_uses_anti_knight_visibility_but_classic_does_not() {
        let entries = [(1, &[1, 2][..]), (2, &[1, 3]), (12, &[2, 3]), (3, &[3])];
        assert!(find_wing(&sparse_snapshot(VariantConfig::default(), &entries), false).is_none());

        let anti_knight = sparse_snapshot(
            VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            },
            &entries,
        );
        let inference = find_wing(&anti_knight, false).unwrap();
        assert_eq!(
            inference.description(anti_knight.topology()),
            "XY-Wing: Cells r1c2,r1c3,r2c4 on value 3"
        );
        assert_eq!(inference.removals().iter().next().unwrap().cell().raw(), 3);
    }

    #[test]
    fn xyz_wing_requires_the_victim_to_see_the_pivot() {
        let mut grid = sparse_snapshot(
            VariantConfig::default(),
            &[(0, &[1, 2, 3]), (1, &[1, 3]), (9, &[2, 3]), (10, &[3])],
        );
        let inference = find_wing(&grid, true).unwrap();
        assert_eq!(
            Some(inference.clone()),
            collect_wings(&grid, true).first().cloned()
        );
        assert_eq!(inference.technique(), Technique::XYZWing);
        assert_eq!(inference.rating(), Rating::from_tenths(44));
        assert_eq!(
            inference.description(grid.topology()),
            "XYZ-Wing: Cells r1c1,r1c2,r2c1 on value 3"
        );
        inference.apply(&mut grid);
        assert!(grid.candidates(CellId::new(10).unwrap()).is_empty());

        let no_shared_pivot_peer = sparse_snapshot(
            VariantConfig::default(),
            &[(0, &[1, 2, 3]), (3, &[1, 3]), (27, &[2, 3]), (30, &[3])],
        );
        assert!(find_wing(&no_shared_pivot_peer, true).is_none());
    }
}
