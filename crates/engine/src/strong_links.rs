use std::{array, cmp::Ordering};

use sukaku_forge_core::{
    CandidateMask, CandidateRemovalsBuilder, CellId, CellMask, Digit, Grid, PositionMask,
    REGION_TYPE_COUNT, RegionId,
};

use crate::inference::{
    four_strong_links_suffix, three_strong_links_suffix, two_strong_links_suffix,
};
use crate::{CellSequence, EngineConfig, Evidence, Inference, Rating, RatingMode, Technique};

const SHARED_REGION_ORDER: [usize; REGION_TYPE_COUNT] = [2, 1, 0, 3, 4, 7, 8, 9, 5, 6];

const REVISED_TYPE_PAIRS: [(usize, usize); 50] = [
    (1, 1),
    (2, 2),
    (2, 1),
    (1, 0),
    (2, 0),
    (0, 0),
    (3, 0),
    (3, 1),
    (3, 2),
    (3, 3),
    (3, 4),
    (3, 5),
    (3, 6),
    (3, 7),
    (3, 8),
    (3, 9),
    (4, 0),
    (4, 1),
    (4, 2),
    (4, 4),
    (4, 5),
    (4, 6),
    (4, 7),
    (4, 8),
    (4, 9),
    (5, 0),
    (5, 1),
    (5, 2),
    (5, 6),
    (5, 7),
    (5, 8),
    (5, 9),
    (6, 0),
    (6, 1),
    (6, 2),
    (6, 7),
    (6, 8),
    (6, 9),
    (7, 0),
    (7, 1),
    (7, 2),
    (7, 8),
    (7, 9),
    (8, 0),
    (8, 1),
    (8, 2),
    (8, 9),
    (9, 0),
    (9, 1),
    (9, 2),
];

const fn mask2(a: u8, b: u8) -> u16 {
    1_u16 << a | 1_u16 << b
}

const fn mask3(a: u8, b: u8, c: u8) -> u16 {
    mask2(a, b) | 1_u16 << c
}

const fn mask4(a: u8, b: u8, c: u8, d: u8) -> u16 {
    mask3(a, b, c) | 1_u16 << d
}

const BOX_EMPTY: [u16; 15] = [
    mask4(4, 5, 7, 8),
    mask4(3, 5, 6, 8),
    mask4(3, 4, 6, 7),
    mask4(1, 2, 7, 8),
    mask4(0, 2, 6, 8),
    mask4(0, 1, 6, 7),
    mask4(1, 2, 4, 5),
    mask4(0, 2, 3, 5),
    mask4(0, 1, 3, 4),
    mask3(6, 7, 8),
    mask3(3, 4, 5),
    mask3(0, 1, 2),
    mask3(2, 5, 8),
    mask3(1, 4, 7),
    mask3(0, 3, 6),
];

const BOX_BLADE_1: [u16; 15] = [
    mask2(3, 6),
    mask2(4, 7),
    mask2(5, 8),
    mask2(0, 6),
    mask2(1, 7),
    mask2(2, 8),
    mask2(0, 3),
    mask2(1, 4),
    mask2(2, 5),
    mask3(0, 1, 2),
    mask3(0, 1, 2),
    mask3(3, 4, 5),
    mask3(0, 3, 6),
    mask3(0, 3, 6),
    mask3(1, 4, 7),
];

const BOX_BLADE_2: [u16; 15] = [
    mask2(1, 2),
    mask2(0, 2),
    mask2(0, 1),
    mask2(4, 5),
    mask2(3, 5),
    mask2(3, 4),
    mask2(7, 8),
    mask2(6, 8),
    mask2(6, 7),
    mask3(3, 4, 5),
    mask3(6, 7, 8),
    mask3(6, 7, 8),
    mask3(1, 4, 7),
    mask3(2, 5, 8),
    mask3(2, 5, 8),
];

const LINE_EMPTY: [u16; 3] = [mask3(0, 1, 2), mask3(3, 4, 5), mask3(6, 7, 8)];
const LINE_BLADE_1: [u16; 3] = [mask3(3, 4, 5), mask3(0, 1, 2), mask3(0, 1, 2)];
const LINE_BLADE_2: [u16; 3] = [mask3(6, 7, 8), mask3(6, 7, 8), mask3(3, 4, 5)];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CellGroup {
    cells: [CellId; 3],
    len: u8,
}

impl CellGroup {
    fn from_cells(cells: &[CellId]) -> Self {
        assert!((1..=3).contains(&cells.len()));
        let filler = cells[0];
        let mut stored = [filler; 3];
        stored[..cells.len()].copy_from_slice(cells);
        Self {
            cells: stored,
            len: cells.len() as u8,
        }
    }

    const fn representative(self) -> CellId {
        self.cells[0]
    }

    fn iter(self) -> impl ExactSizeIterator<Item = CellId> {
        self.cells.into_iter().take(usize::from(self.len))
    }

    fn sequence(self) -> CellSequence {
        let mut result = CellSequence::new();
        for cell in self.iter() {
            result.push(cell);
        }
        result
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LinkCandidate {
    region: RegionId,
    positions: PositionMask,
    endpoints: [CellGroup; 2],
    grouped: bool,
    all_cells: CellMask,
}

struct RankedHint {
    inference: Inference,
    raw_eliminations: u16,
    suffix: String,
}

trait RankedHintSink {
    fn offer(&mut self, hint: RankedHint);
}

impl RankedHintSink for Option<RankedHint> {
    fn offer(&mut self, hint: RankedHint) {
        if self
            .as_ref()
            .is_none_or(|current| better_than(&hint, current))
        {
            *self = Some(hint);
        }
    }
}

impl RankedHintSink for Vec<RankedHint> {
    fn offer(&mut self, hint: RankedHint) {
        self.push(hint);
    }
}

/// Find the first Java-ordered two-strong-links hint.
///
/// Original rating mode ports `StrongLinks(2)`. Revised mode deliberately
/// ports the older `TurbotFish` implementation; the Java algorithms are not
/// inference-equivalent, so their catalog and victim rules remain separate.
#[must_use]
pub fn find_two_strong_links(grid: &Grid, config: EngineConfig) -> Option<Inference> {
    let catalogs = build_catalogs(grid, config.rating_mode);
    let mut best = None;
    match config.rating_mode {
        RatingMode::Original => search_original(grid, config, &catalogs, &mut best),
        RatingMode::Revised => search_revised(grid, config, &catalogs, &mut best),
    }
    best.map(|ranked: RankedHint| ranked.inference)
}

/// Collect every Java-compatible two-strong-links hint in the producer's
/// stable rank order.
#[must_use]
pub fn collect_two_strong_links(grid: &Grid, config: EngineConfig) -> Vec<Inference> {
    let catalogs = build_catalogs(grid, config.rating_mode);
    let mut hints = Vec::new();
    match config.rating_mode {
        RatingMode::Original => search_original(grid, config, &catalogs, &mut hints),
        RatingMode::Revised => search_revised(grid, config, &catalogs, &mut hints),
    }
    finish_ranked_hints(hints)
}

const THREE_LINK_ORDERS: [[usize; 3]; 3] = [[0, 1, 2], [1, 0, 2], [0, 2, 1]];
const FOUR_LINK_ORDERS: [[usize; 4]; 12] = [
    [0, 1, 2, 3],
    [1, 0, 2, 3],
    [2, 0, 1, 3],
    [0, 2, 1, 3],
    [1, 2, 0, 3],
    [2, 1, 0, 3],
    [0, 3, 2, 1],
    [0, 2, 3, 1],
    [1, 0, 3, 2],
    [0, 1, 3, 2],
    [1, 3, 0, 2],
    [0, 3, 1, 2],
];

/// Find the first Java-ranked `StrongLinks(3)` hint.
///
/// Unlike the degree-two gate, Java uses this same producer in both rating
/// modes. The fixed-size search below preserves its numeric family-multiset,
/// candidate-tuple, QuickPerm arrangement, and direction order without
/// allocating completed hint lists.
#[must_use]
pub fn find_three_strong_links(grid: &Grid, config: EngineConfig) -> Option<Inference> {
    let catalogs = build_catalogs(grid, RatingMode::Original);
    let active_types = original_active_types(grid, config);
    let mut best = None;
    visit_three_link_type_multisets(&active_types, |types| {
        search_three_link_types(grid, config, &catalogs, types, &mut best);
    });
    best.map(|ranked: RankedHint| ranked.inference)
}

/// Collect every Java-compatible `StrongLinks(3)` hint in stable rank order.
#[must_use]
pub fn collect_three_strong_links(grid: &Grid, config: EngineConfig) -> Vec<Inference> {
    let catalogs = build_catalogs(grid, RatingMode::Original);
    let active_types = original_active_types(grid, config);
    let mut hints = Vec::new();
    visit_three_link_type_multisets(&active_types, |types| {
        search_three_link_types(grid, config, &catalogs, types, &mut hints);
    });
    finish_ranked_hints(hints)
}

/// Find the first Java-ranked `StrongLinks(4)` hint in either rating mode.
#[must_use]
pub fn find_four_strong_links(grid: &Grid, config: EngineConfig) -> Option<Inference> {
    let catalogs = build_catalogs(grid, RatingMode::Original);
    let active_types = original_active_types(grid, config);
    let mut best = None;
    visit_four_link_type_multisets(&active_types, |types| {
        search_four_link_types(grid, config, &catalogs, types, &mut best);
    });
    best.map(|ranked: RankedHint| ranked.inference)
}

/// Collect every Java-compatible `StrongLinks(4)` hint in stable rank order.
#[must_use]
pub fn collect_four_strong_links(grid: &Grid, config: EngineConfig) -> Vec<Inference> {
    let catalogs = build_catalogs(grid, RatingMode::Original);
    let active_types = original_active_types(grid, config);
    let mut hints = Vec::new();
    visit_four_link_type_multisets(&active_types, |types| {
        search_four_link_types(grid, config, &catalogs, types, &mut hints);
    });
    finish_ranked_hints(hints)
}

fn finish_ranked_hints(mut hints: Vec<RankedHint>) -> Vec<Inference> {
    // Java's Collections.sort is stable. StrongLinks and TurbotFish do not
    // implement value equality, so the producer-local accumulator retains
    // all equal-ranked discoveries rather than deduplicating them.
    hints.sort_by(java_compare);
    hints.into_iter().map(|hint| hint.inference).collect()
}

fn java_compare(left: &RankedHint, right: &RankedHint) -> Ordering {
    left.inference
        .rating()
        .cmp(&right.inference.rating())
        .then_with(|| right.raw_eliminations.cmp(&left.raw_eliminations))
        .then_with(|| left.suffix.cmp(&right.suffix))
}

fn visit_three_link_type_multisets(active_types: &[usize], mut visit: impl FnMut([usize; 3])) {
    // This is the accepted subsequence of Java's numeric Permutations(3,n*3):
    // 000,001,011,111,002,012,... . Enumerating it directly avoids testing up
    // to C(30,3) rejected masks while retaining the observable order.
    for third in 0..active_types.len() {
        for second in 0..=third {
            for first in 0..=second {
                visit([
                    active_types[first],
                    active_types[second],
                    active_types[third],
                ]);
            }
        }
    }
}

fn visit_four_link_type_multisets(active_types: &[usize], mut visit: impl FnMut([usize; 4])) {
    // Accepted Java Permutations(4,n*4) values are the nondecreasing family
    // multisets in numeric/colex order.
    for fourth in 0..active_types.len() {
        for third in 0..=fourth {
            for second in 0..=third {
                for first in 0..=second {
                    visit([
                        active_types[first],
                        active_types[second],
                        active_types[third],
                        active_types[fourth],
                    ]);
                }
            }
        }
    }
}

fn search_three_link_types(
    grid: &Grid,
    config: EngineConfig,
    catalogs: &[[Vec<LinkCandidate>; REGION_TYPE_COUNT]; 10],
    types: [usize; 3],
    sink: &mut impl RankedHintSink,
) {
    for value in 1_u8..=9 {
        let digit = Digit::new(value).expect("digit loop");
        let first_catalog = &catalogs[usize::from(value)][types[0]];
        let second_catalog = &catalogs[usize::from(value)][types[1]];
        let third_catalog = &catalogs[usize::from(value)][types[2]];
        for &first in first_catalog {
            for &second in second_catalog {
                if types[0] == types[1]
                    && second.region.region_index() <= first.region.region_index()
                {
                    continue;
                }
                if !first.all_cells.intersect(second.all_cells).is_empty() {
                    continue;
                }
                let first_two_cells = CellMask::from_words(
                    first.all_cells.low() | second.all_cells.low(),
                    first.all_cells.high() | second.all_cells.high(),
                );
                for &third in third_catalog {
                    if types[1] == types[2]
                        && third.region.region_index() <= second.region.region_index()
                    {
                        continue;
                    }
                    if !first_two_cells.intersect(third.all_cells).is_empty() {
                        continue;
                    }
                    evaluate_three_link_tuple(grid, config, digit, [first, second, third], sink);
                }
            }
        }
    }
}

fn search_four_link_types(
    grid: &Grid,
    config: EngineConfig,
    catalogs: &[[Vec<LinkCandidate>; REGION_TYPE_COUNT]; 10],
    types: [usize; 4],
    sink: &mut impl RankedHintSink,
) {
    for value in 1_u8..=9 {
        let digit = Digit::new(value).expect("digit loop");
        let first_catalog = &catalogs[usize::from(value)][types[0]];
        let second_catalog = &catalogs[usize::from(value)][types[1]];
        let third_catalog = &catalogs[usize::from(value)][types[2]];
        let fourth_catalog = &catalogs[usize::from(value)][types[3]];
        for &first in first_catalog {
            for &second in second_catalog {
                if types[0] == types[1]
                    && second.region.region_index() <= first.region.region_index()
                {
                    continue;
                }
                if !first.all_cells.intersect(second.all_cells).is_empty() {
                    continue;
                }
                let first_two_cells = union_cells(first.all_cells, second.all_cells);
                for &third in third_catalog {
                    if types[1] == types[2]
                        && third.region.region_index() <= second.region.region_index()
                    {
                        continue;
                    }
                    if !first_two_cells.intersect(third.all_cells).is_empty() {
                        continue;
                    }
                    let first_three_cells = union_cells(first_two_cells, third.all_cells);
                    for &fourth in fourth_catalog {
                        if types[2] == types[3]
                            && fourth.region.region_index() <= third.region.region_index()
                        {
                            continue;
                        }
                        if !first_three_cells.intersect(fourth.all_cells).is_empty() {
                            continue;
                        }
                        evaluate_four_link_tuple(
                            grid,
                            config,
                            digit,
                            [first, second, third, fourth],
                            sink,
                        );
                    }
                }
            }
        }
    }
}

fn evaluate_three_link_tuple(
    grid: &Grid,
    config: EngineConfig,
    digit: Digit,
    links: [LinkCandidate; 3],
    sink: &mut impl RankedHintSink,
) {
    for order in THREE_LINK_ORDERS {
        for directions in 0_u8..8 {
            let first_bridge_groups = bridge_groups(links, order, directions, 0);
            let Some(first_bridge_region) =
                shared_region(grid, config, first_bridge_groups, RatingMode::Original)
            else {
                continue;
            };
            let second_bridge_groups = bridge_groups(links, order, directions, 1);
            let Some(second_bridge_region) =
                shared_region(grid, config, second_bridge_groups, RatingMode::Original)
            else {
                continue;
            };

            let first_direction = usize::from(directions >> 2 & 1);
            let last_direction = usize::from(directions & 1);
            let end_groups = [
                links[order[0]].endpoints[first_direction],
                links[order[2]].endpoints[1 - last_direction],
            ];
            let ring_region = shared_region(grid, config, end_groups, RatingMode::Original);
            if ring_region.is_some()
                && (first_direction != 0 || !first_region_is_minimum(links, order))
            {
                continue;
            }

            let bridge_groups = [first_bridge_groups, second_bridge_groups];
            let bridge_regions = [first_bridge_region, second_bridge_region];
            let Some((inference, raw_eliminations)) = build_three_link_hint(
                grid,
                digit,
                links,
                order,
                end_groups,
                bridge_groups,
                bridge_regions,
                ring_region,
            ) else {
                continue;
            };
            let link_regions = links.map(|link| link.region);
            let grouped_links = links.map(|link| link.grouped);
            let order_bytes = order.map(|index| index as u8);
            let ranked = RankedHint {
                inference,
                raw_eliminations,
                suffix: three_strong_links_suffix(link_regions, order_bytes, grouped_links),
            };
            sink.offer(ranked);
        }
    }
}

fn evaluate_four_link_tuple(
    grid: &Grid,
    config: EngineConfig,
    digit: Digit,
    links: [LinkCandidate; 4],
    sink: &mut impl RankedHintSink,
) {
    let endpoint_groups: [CellGroup; 8] =
        array::from_fn(|index| links[index / 2].endpoints[index % 2]);
    let mut shared_region_cache = [None; 64];
    for order in FOUR_LINK_ORDERS {
        for directions in 0_u8..16 {
            let first_bridge_indices = four_bridge_group_indices(order, directions, 0);
            let first_bridge_groups = first_bridge_indices.map(|index| endpoint_groups[index]);
            let Some(first_bridge_region) = cached_shared_region(
                grid,
                config,
                &mut shared_region_cache,
                &endpoint_groups,
                first_bridge_indices,
            ) else {
                continue;
            };
            let second_bridge_indices = four_bridge_group_indices(order, directions, 1);
            let second_bridge_groups = second_bridge_indices.map(|index| endpoint_groups[index]);
            let Some(second_bridge_region) = cached_shared_region(
                grid,
                config,
                &mut shared_region_cache,
                &endpoint_groups,
                second_bridge_indices,
            ) else {
                continue;
            };
            let third_bridge_indices = four_bridge_group_indices(order, directions, 2);
            let third_bridge_groups = third_bridge_indices.map(|index| endpoint_groups[index]);
            let Some(third_bridge_region) = cached_shared_region(
                grid,
                config,
                &mut shared_region_cache,
                &endpoint_groups,
                third_bridge_indices,
            ) else {
                continue;
            };

            let first_direction = usize::from(directions >> 3 & 1);
            let last_direction = usize::from(directions & 1);
            let end_indices = [
                order[0] * 2 + first_direction,
                order[3] * 2 + 1 - last_direction,
            ];
            let end_groups = end_indices.map(|index| endpoint_groups[index]);
            let ring_region = cached_shared_region(
                grid,
                config,
                &mut shared_region_cache,
                &endpoint_groups,
                end_indices,
            );
            if ring_region.is_some()
                && (first_direction != 0 || !first_region_is_minimum(links, order))
            {
                continue;
            }

            let bridge_groups = [
                first_bridge_groups,
                second_bridge_groups,
                third_bridge_groups,
            ];
            let bridge_regions = [
                first_bridge_region,
                second_bridge_region,
                third_bridge_region,
            ];
            let Some((inference, raw_eliminations)) = build_four_link_hint(
                grid,
                digit,
                links,
                order,
                end_groups,
                bridge_groups,
                bridge_regions,
                ring_region,
            ) else {
                continue;
            };
            let link_regions = links.map(|link| link.region);
            let grouped_links = links.map(|link| link.grouped);
            let order_bytes = order.map(|index| index as u8);
            let ranked = RankedHint {
                inference,
                raw_eliminations,
                suffix: four_strong_links_suffix(link_regions, order_bytes, grouped_links),
            };
            sink.offer(ranked);
        }
    }
}

fn bridge_groups(
    links: [LinkCandidate; 3],
    order: [usize; 3],
    directions: u8,
    position: usize,
) -> [CellGroup; 2] {
    let first_direction = usize::from(directions >> (2 - position) & 1);
    let second_direction = usize::from(directions >> (1 - position) & 1);
    [
        links[order[position]].endpoints[1 - first_direction],
        links[order[position + 1]].endpoints[second_direction],
    ]
}

fn four_bridge_group_indices(order: [usize; 4], directions: u8, position: usize) -> [usize; 2] {
    let first_direction = usize::from(directions >> (3 - position) & 1);
    let second_direction = usize::from(directions >> (2 - position) & 1);
    [
        order[position] * 2 + 1 - first_direction,
        order[position + 1] * 2 + second_direction,
    ]
}

fn cached_shared_region(
    grid: &Grid,
    config: EngineConfig,
    cache: &mut [Option<Option<RegionId>>; 64],
    endpoint_groups: &[CellGroup; 8],
    indices: [usize; 2],
) -> Option<RegionId> {
    // Shared-region membership is symmetric. Canonicalizing the pair retains
    // Java's search order while avoiding repeat topology scans across the 192
    // order/direction arrangements of one four-link tuple.
    let [first, second] = if indices[0] <= indices[1] {
        indices
    } else {
        [indices[1], indices[0]]
    };
    let entry = &mut cache[first * 8 + second];
    if let Some(region) = *entry {
        return region;
    }
    let region = shared_region(
        grid,
        config,
        [endpoint_groups[first], endpoint_groups[second]],
        RatingMode::Original,
    );
    *entry = Some(region);
    region
}

fn first_region_is_minimum<const N: usize>(links: [LinkCandidate; N], order: [usize; N]) -> bool {
    let first = region_full_number(links[order[0]].region);
    links
        .into_iter()
        .all(|link| region_full_number(link.region) >= first)
}

fn union_cells(first: CellMask, second: CellMask) -> CellMask {
    CellMask::from_words(first.low() | second.low(), first.high() | second.high())
}

#[allow(clippy::too_many_arguments)]
fn build_three_link_hint(
    grid: &Grid,
    digit: Digit,
    links: [LinkCandidate; 3],
    order: [usize; 3],
    end_groups: [CellGroup; 2],
    bridge_groups: [[CellGroup; 2]; 2],
    bridge_regions: [RegionId; 2],
    ring_region: Option<RegionId>,
) -> Option<(Inference, u16)> {
    let mut builder = CandidateRemovalsBuilder::with_capacity(12);
    let link_regions = links.map(|link| link.region);
    let mut first_victims = common_visibility(grid, end_groups);
    for region in link_regions {
        first_victims = first_victims.without(grid.topology().region_mask(region));
    }
    if ring_region.is_none() {
        for region in bridge_regions {
            first_victims = first_victims.without(grid.topology().region_mask(region));
        }
    }
    first_victims = first_victims.intersect(grid.candidate_cells(digit));
    let mut raw_eliminations = first_victims.count() as u16;
    add_victims(&mut builder, first_victims, digit);

    if ring_region.is_some() {
        for groups in bridge_groups {
            let mut victims = common_visibility(grid, groups);
            for region in link_regions {
                victims = victims.without(grid.topology().region_mask(region));
            }
            victims = victims.intersect(grid.candidate_cells(digit));
            raw_eliminations += victims.count() as u16;
            add_victims(&mut builder, victims, digit);
        }
    }

    let removals = builder.build();
    if removals.is_empty() {
        return None;
    }
    let mut pattern_cells = CellSequence::new();
    pattern_cells.push(end_groups[0].representative());
    for groups in bridge_groups {
        pattern_cells.push(groups[0].representative());
        pattern_cells.push(groups[1].representative());
    }
    pattern_cells.push(end_groups[1].representative());

    let endpoint_groups = [
        links[0].endpoints[0].sequence(),
        links[0].endpoints[1].sequence(),
        links[1].endpoints[0].sequence(),
        links[1].endpoints[1].sequence(),
        links[2].endpoints[0].sequence(),
        links[2].endpoints[1].sequence(),
    ];
    let link_positions = links.map(|link| link.positions);
    let grouped_links = links.map(|link| link.grouped);
    let link_order = order.map(|index| index as u8);
    let rating = three_strong_links_rating(link_regions, link_order, grouped_links);
    Some((
        Inference::elimination(
            Technique::ThreeStrongLinks,
            rating,
            removals,
            Evidence::ThreeStrongLinks {
                digit,
                pattern_cells,
                link_regions,
                link_positions,
                endpoint_groups,
                bridge_regions,
                ring_region,
                grouped_links,
                link_order,
            },
        ),
        raw_eliminations,
    ))
}

fn three_strong_links_rating(regions: [RegionId; 3], order: [u8; 3], grouped: [bool; 3]) -> Rating {
    let suffix = three_strong_links_suffix(regions, order, grouped);
    let structure = &suffix[1..];
    if grouped.into_iter().any(|value| value)
        || structure.chars().any(|value| matches!(value, '3'..='9'))
    {
        Rating::from_tenths(57)
    } else {
        match (structure.contains('0'), structure.contains('2')) {
            (false, false) => Rating::from_tenths(54),
            (true, true) => Rating::from_tenths(56),
            _ => Rating::from_tenths(55),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_four_link_hint(
    grid: &Grid,
    digit: Digit,
    links: [LinkCandidate; 4],
    order: [usize; 4],
    end_groups: [CellGroup; 2],
    bridge_groups: [[CellGroup; 2]; 3],
    bridge_regions: [RegionId; 3],
    ring_region: Option<RegionId>,
) -> Option<(Inference, u16)> {
    let mut builder = CandidateRemovalsBuilder::with_capacity(16);
    let link_regions = links.map(|link| link.region);
    let mut first_victims = common_visibility(grid, end_groups);
    for region in link_regions {
        first_victims = first_victims.without(grid.topology().region_mask(region));
    }
    if ring_region.is_none() {
        for region in bridge_regions {
            first_victims = first_victims.without(grid.topology().region_mask(region));
        }
    }
    first_victims = first_victims.intersect(grid.candidate_cells(digit));
    let mut raw_eliminations = first_victims.count() as u16;
    add_victims(&mut builder, first_victims, digit);

    if ring_region.is_some() {
        for groups in bridge_groups {
            let mut victims = common_visibility(grid, groups);
            for region in link_regions {
                victims = victims.without(grid.topology().region_mask(region));
            }
            victims = victims.intersect(grid.candidate_cells(digit));
            raw_eliminations += victims.count() as u16;
            add_victims(&mut builder, victims, digit);
        }
    }

    let removals = builder.build();
    if removals.is_empty() {
        return None;
    }
    let mut pattern_cells = CellSequence::new();
    pattern_cells.push(end_groups[0].representative());
    for groups in bridge_groups {
        pattern_cells.push(groups[0].representative());
        pattern_cells.push(groups[1].representative());
    }
    pattern_cells.push(end_groups[1].representative());

    let endpoint_groups = [
        links[0].endpoints[0].sequence(),
        links[0].endpoints[1].sequence(),
        links[1].endpoints[0].sequence(),
        links[1].endpoints[1].sequence(),
        links[2].endpoints[0].sequence(),
        links[2].endpoints[1].sequence(),
        links[3].endpoints[0].sequence(),
        links[3].endpoints[1].sequence(),
    ];
    let link_positions = links.map(|link| link.positions);
    let grouped_links = links.map(|link| link.grouped);
    let link_order = order.map(|index| index as u8);
    let rating = four_strong_links_rating(link_regions, link_order, grouped_links);
    Some((
        Inference::elimination(
            Technique::FourStrongLinks,
            rating,
            removals,
            Evidence::FourStrongLinks {
                digit,
                pattern_cells,
                link_regions,
                link_positions,
                endpoint_groups,
                bridge_regions,
                ring_region,
                grouped_links,
                link_order,
            },
        ),
        raw_eliminations,
    ))
}

fn four_strong_links_rating(regions: [RegionId; 4], order: [u8; 4], grouped: [bool; 4]) -> Rating {
    let suffix = four_strong_links_suffix(regions, order, grouped);
    let structure = &suffix[1..];
    if grouped.into_iter().any(|value| value)
        || structure.chars().any(|value| matches!(value, '3'..='9'))
    {
        Rating::from_tenths(61)
    } else {
        match (structure.contains('0'), structure.contains('2')) {
            (false, false) => Rating::from_tenths(58),
            (true, true) => Rating::from_tenths(60),
            _ => Rating::from_tenths(59),
        }
    }
}

fn build_catalogs(grid: &Grid, mode: RatingMode) -> [[Vec<LinkCandidate>; REGION_TYPE_COUNT]; 10] {
    array::from_fn(|value| {
        array::from_fn(|type_index| {
            let Some(digit) = Digit::new(value as u8) else {
                return Vec::new();
            };
            if !grid.topology().is_region_type_active(type_index) {
                return Vec::new();
            }
            let mut result = Vec::with_capacity(grid.topology().region_count(type_index) * 2);
            for region_index in 0..grid.topology().region_count(type_index) {
                let region = region_id(type_index, region_index);
                match mode {
                    RatingMode::Original => {
                        add_original_region_candidates(grid, digit, region, &mut result);
                    }
                    RatingMode::Revised => {
                        add_revised_region_candidates(grid, digit, region, &mut result);
                    }
                }
            }
            result
        })
    })
}

fn search_original(
    grid: &Grid,
    config: EngineConfig,
    catalogs: &[[Vec<LinkCandidate>; REGION_TYPE_COUNT]; 10],
    sink: &mut impl RankedHintSink,
) {
    let active_types = original_active_types(grid, config);
    for second_ordinal in 0..active_types.len() {
        for first_ordinal in 0..=second_ordinal {
            let types = [active_types[first_ordinal], active_types[second_ordinal]];
            for value in 1_u8..=9 {
                let digit = Digit::new(value).expect("digit loop");
                let first_catalog = &catalogs[usize::from(value)][types[0]];
                let second_catalog = &catalogs[usize::from(value)][types[1]];
                for &first in first_catalog {
                    for &second in second_catalog {
                        if types[0] == types[1]
                            && second.region.region_index() <= first.region.region_index()
                        {
                            continue;
                        }
                        if !first.all_cells.intersect(second.all_cells).is_empty() {
                            continue;
                        }
                        evaluate_link_pair(
                            grid,
                            config,
                            digit,
                            [first, second],
                            RatingMode::Original,
                            sink,
                        );
                    }
                }
            }
        }
    }
}

fn search_revised(
    grid: &Grid,
    config: EngineConfig,
    catalogs: &[[Vec<LinkCandidate>; REGION_TYPE_COUNT]; 10],
    sink: &mut impl RankedHintSink,
) {
    let pair_count = if effective_variant_latin(grid, config) {
        5
    } else {
        REVISED_TYPE_PAIRS.len()
    };
    for &(base_type, cover_type) in &REVISED_TYPE_PAIRS[..pair_count] {
        if !grid.topology().is_region_type_active(base_type)
            || !grid.topology().is_region_type_active(cover_type)
        {
            continue;
        }
        for value in 1_u8..=9 {
            let digit = Digit::new(value).expect("digit loop");
            let base_catalog = &catalogs[usize::from(value)][base_type];
            let cover_catalog = &catalogs[usize::from(value)][cover_type];
            for &base in base_catalog {
                for &cover in cover_catalog {
                    if base_type == cover_type
                        && cover.region.region_index() <= base.region.region_index()
                    {
                        continue;
                    }
                    if !base.all_cells.intersect(cover.all_cells).is_empty() {
                        continue;
                    }
                    evaluate_link_pair(
                        grid,
                        config,
                        digit,
                        [base, cover],
                        RatingMode::Revised,
                        sink,
                    );
                }
            }
        }
    }
}

fn evaluate_link_pair(
    grid: &Grid,
    config: EngineConfig,
    digit: Digit,
    links: [LinkCandidate; 2],
    mode: RatingMode,
    sink: &mut impl RankedHintSink,
) {
    for first_direction in 0..2 {
        for second_direction in 0..2 {
            let bridge_groups = [
                links[0].endpoints[1 - first_direction],
                links[1].endpoints[second_direction],
            ];
            let Some(bridge_region) = shared_region(grid, config, bridge_groups, mode) else {
                continue;
            };
            if mode == RatingMode::Revised
                && (bridge_region == links[0].region || bridge_region == links[1].region)
            {
                continue;
            }

            let end_groups = [
                links[0].endpoints[first_direction],
                links[1].endpoints[1 - second_direction],
            ];
            let ring_region = shared_region(grid, config, end_groups, mode);
            if mode == RatingMode::Original
                && ring_region.is_some()
                && (first_direction != 0
                    || region_full_number(links[0].region) > region_full_number(links[1].region))
            {
                continue;
            }

            let Some((inference, raw_eliminations)) = build_hint(
                grid,
                digit,
                links,
                end_groups,
                bridge_groups,
                bridge_region,
                ring_region,
                mode,
            ) else {
                continue;
            };
            let suffix = two_strong_links_suffix(
                [links[0].region, links[1].region],
                [links[0].grouped, links[1].grouped],
                mode,
            );
            let ranked = RankedHint {
                inference,
                raw_eliminations,
                suffix,
            };
            sink.offer(ranked);
        }
    }
}

fn better_than(candidate: &RankedHint, current: &RankedHint) -> bool {
    java_compare(candidate, current) == Ordering::Less
}

#[allow(clippy::too_many_arguments)]
fn build_hint(
    grid: &Grid,
    digit: Digit,
    links: [LinkCandidate; 2],
    end_groups: [CellGroup; 2],
    bridge_groups: [CellGroup; 2],
    bridge_region: RegionId,
    ring_region: Option<RegionId>,
    mode: RatingMode,
) -> Option<(Inference, u16)> {
    let mut builder = CandidateRemovalsBuilder::with_capacity(8);
    let link_regions = [links[0].region, links[1].region];
    let mut first_victims = common_visibility(grid, end_groups);
    for region in link_regions {
        first_victims = first_victims.without(grid.topology().region_mask(region));
    }
    if mode == RatingMode::Original && ring_region.is_none() {
        first_victims = first_victims.without(grid.topology().region_mask(bridge_region));
    }
    first_victims = first_victims.intersect(grid.candidate_cells(digit));
    let mut raw_eliminations = first_victims.count() as u16;
    add_victims(&mut builder, first_victims, digit);

    if ring_region.is_some() {
        let mut ring_victims = common_visibility(grid, bridge_groups);
        for region in link_regions {
            ring_victims = ring_victims.without(grid.topology().region_mask(region));
        }
        ring_victims = ring_victims.intersect(grid.candidate_cells(digit));
        raw_eliminations += ring_victims.count() as u16;
        add_victims(&mut builder, ring_victims, digit);
    }

    let removals = builder.build();
    if removals.is_empty() {
        return None;
    }
    let mut pattern_cells = CellSequence::new();
    for cell in [
        end_groups[0].representative(),
        bridge_groups[0].representative(),
        bridge_groups[1].representative(),
        end_groups[1].representative(),
    ] {
        pattern_cells.push(cell);
    }
    let endpoint_groups = [
        links[0].endpoints[0].sequence(),
        links[0].endpoints[1].sequence(),
        links[1].endpoints[0].sequence(),
        links[1].endpoints[1].sequence(),
    ];
    let grouped_links = [links[0].grouped, links[1].grouped];
    let rating = two_strong_links_rating(link_regions, grouped_links, mode);
    Some((
        Inference::elimination(
            Technique::TurbotFish,
            rating,
            removals,
            Evidence::TwoStrongLinks {
                digit,
                pattern_cells,
                link_regions,
                link_positions: [links[0].positions, links[1].positions],
                endpoint_groups,
                bridge_region,
                ring_region,
                grouped_links,
                rating_mode: mode,
            },
        ),
        raw_eliminations,
    ))
}

fn common_visibility(grid: &Grid, groups: [CellGroup; 2]) -> CellMask {
    let mut cells = groups.into_iter().flat_map(CellGroup::iter);
    let first = cells.next().expect("two nonempty endpoint groups");
    let mut result = grid.topology().visible_mask(first);
    for cell in cells {
        result = result.intersect(grid.topology().visible_mask(cell));
    }
    result
}

fn add_victims(builder: &mut CandidateRemovalsBuilder, victims: CellMask, digit: Digit) {
    let mask = CandidateMask::of(digit);
    for cell in victims.iter() {
        builder.add(cell, mask);
    }
}

fn shared_region(
    grid: &Grid,
    config: EngineConfig,
    groups: [CellGroup; 2],
    mode: RatingMode,
) -> Option<RegionId> {
    'families: for type_index in SHARED_REGION_ORDER {
        if !grid.topology().is_region_type_active(type_index) {
            continue;
        }
        if mode == RatingMode::Revised && effective_variant_latin(grid, config) && type_index > 2 {
            continue;
        }
        let first = groups[0].representative();
        let Some(region_index) = grid.topology().cell_region_index(first, type_index) else {
            continue;
        };
        for cell in groups.into_iter().flat_map(CellGroup::iter) {
            if grid.topology().cell_region_index(cell, type_index) != Some(region_index) {
                continue 'families;
            }
        }
        return RegionId::new(type_index as u8, region_index);
    }
    None
}

fn original_active_types(grid: &Grid, config: EngineConfig) -> Vec<usize> {
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

fn two_strong_links_rating(regions: [RegionId; 2], grouped: [bool; 2], mode: RatingMode) -> Rating {
    let types = [regions[0].type_index(), regions[1].type_index()];
    match mode {
        RatingMode::Original => {
            if grouped.into_iter().any(|value| value)
                || types.into_iter().any(|type_index| type_index > 2)
            {
                return Rating::from_tenths(43);
            }
            let suffix = two_strong_links_suffix(regions, grouped, mode);
            let structure = &suffix[1..];
            match (structure.contains('0'), structure.contains('2')) {
                (false, false) => Rating::from_tenths(40),
                (true, true) => Rating::from_tenths(42),
                _ => Rating::from_tenths(41),
            }
        }
        RatingMode::Revised => {
            if grouped.into_iter().any(|value| value)
                || types.into_iter().any(|type_index| type_index > 2)
            {
                return Rating::from_tenths(43);
            }
            match types {
                [1, 1] | [2, 2] => Rating::from_tenths(40),
                [1, 2] | [2, 1] => Rating::from_tenths(41),
                _ => Rating::from_tenths(42),
            }
        }
    }
}

fn add_original_region_candidates(
    grid: &Grid,
    digit: Digit,
    region: RegionId,
    result: &mut Vec<LinkCandidate>,
) {
    let positions = grid.region_candidate_positions(region, digit);
    let count = positions.count();
    if !(2..=6).contains(&count) {
        return;
    }
    let type_index = region.type_index();
    let box_like = is_box_like(type_index);
    if count == 2 {
        let cells = cells_for_positions(grid, region, positions);
        if box_like && same_row_or_column(cells[0], cells[1]) {
            return;
        }
        push_candidate(
            result,
            grid,
            region,
            positions,
            [
                CellGroup::from_cells(&cells[..1]),
                CellGroup::from_cells(&cells[1..]),
            ],
            false,
        );
        return;
    }

    let geometry_count = if type_index == 0 { 15 } else { 3 };
    let Some((geometry, blade1, blade2)) =
        find_geometry(positions.bits(), box_like, geometry_count, true)
    else {
        return;
    };
    let blade1_cells = cells_for_mask(grid, region, blade1);
    let blade2_cells = cells_for_mask(grid, region, blade2);
    let heart = (box_like && geometry < 9).then(|| region_cell(grid, region, geometry as u8));

    if blade1_cells.len() == 1 || blade2_cells.len() == 1 {
        // Compatibility subtlety: mutating the Java `ordinal` loop variable
        // makes the second branch run immediately when both blades are
        // singletons around a candidate heart. The retained orientation starts
        // at blade 2 and keeps that heart as support of the blade-1 endpoint.
        if blade1_cells.len() == 1 && blade2_cells.len() == 1 {
            let heart = heart.expect("two singleton box blades require a heart");
            push_candidate(
                result,
                grid,
                region,
                positions,
                [
                    CellGroup::from_cells(blade2_cells.as_slice()),
                    CellGroup::from_cells(&[blade1_cells[0], heart]),
                ],
                true,
            );
            return;
        }
        if blade1_cells.len() == 1 {
            let endpoints = singleton_blade_endpoints(
                blade1_cells.as_slice(),
                blade2_cells.as_slice(),
                heart,
                box_like,
            );
            push_candidate(result, grid, region, positions, endpoints, true);
        }
        if blade2_cells.len() == 1 {
            let endpoints = singleton_blade_endpoints(
                blade2_cells.as_slice(),
                blade1_cells.as_slice(),
                heart,
                box_like,
            );
            push_candidate(result, grid, region, positions, endpoints, true);
        }
        return;
    }

    let equivalent_line = geometry > 8
        && count == 4
        && same_row_or_column(blade1_cells[1], blade2_cells[1])
        && same_row_or_column(blade1_cells[0], blade2_cells[0]);
    if equivalent_line {
        push_candidate(
            result,
            grid,
            region,
            positions,
            [
                CellGroup::from_cells(blade1_cells.as_slice()),
                CellGroup::from_cells(blade2_cells.as_slice()),
            ],
            true,
        );
        push_candidate(
            result,
            grid,
            region,
            positions,
            [
                CellGroup::from_cells(&[blade1_cells[0], blade2_cells[0]]),
                CellGroup::from_cells(&[blade1_cells[1], blade2_cells[1]]),
            ],
            true,
        );
        return;
    }

    let mut first_group = blade1_cells;
    let mut second_group = blade2_cells;
    if let Some(heart_cell) = heart.filter(|&cell| grid.candidates(cell).contains(digit)) {
        if first_group.len() == 2 {
            first_group.push(heart_cell);
        }
        if second_group.len() == 2 {
            second_group.push(heart_cell);
        }
    }
    push_candidate(
        result,
        grid,
        region,
        positions,
        [
            CellGroup::from_cells(first_group.as_slice()),
            CellGroup::from_cells(second_group.as_slice()),
        ],
        true,
    );
}

fn add_revised_region_candidates(
    grid: &Grid,
    digit: Digit,
    region: RegionId,
    result: &mut Vec<LinkCandidate>,
) {
    let positions = grid.region_candidate_positions(region, digit);
    let count = positions.count();
    let box_like = is_box_like(region.type_index());
    let maximum = if box_like { 5 } else { 6 };
    if count < 2 || count > maximum {
        return;
    }
    if count == 2 {
        let cells = cells_for_positions(grid, region, positions);
        push_candidate(
            result,
            grid,
            region,
            positions,
            [
                CellGroup::from_cells(&cells[..1]),
                CellGroup::from_cells(&cells[1..]),
            ],
            false,
        );
        return;
    }

    let geometry_count = if box_like { 9 } else { 3 };
    let Some((geometry, blade1, blade2)) =
        find_geometry(positions.bits(), box_like, geometry_count, false)
    else {
        return;
    };
    let blade1_cells = cells_for_mask(grid, region, blade1);
    let blade2_cells = cells_for_mask(grid, region, blade2);
    let heart = box_like.then(|| region_cell(grid, region, geometry as u8));
    if blade1_cells.len() == 1 || blade2_cells.len() == 1 {
        if blade1_cells.len() == 1 {
            push_candidate(
                result,
                grid,
                region,
                positions,
                revised_singleton_blade_endpoints(
                    blade1_cells.as_slice(),
                    blade2_cells.as_slice(),
                    heart,
                    box_like,
                ),
                true,
            );
        }
        if blade2_cells.len() == 1 {
            push_candidate(
                result,
                grid,
                region,
                positions,
                revised_singleton_blade_endpoints(
                    blade2_cells.as_slice(),
                    blade1_cells.as_slice(),
                    heart,
                    box_like,
                ),
                true,
            );
        }
        return;
    }
    push_candidate(
        result,
        grid,
        region,
        positions,
        [
            CellGroup::from_cells(blade1_cells.as_slice()),
            CellGroup::from_cells(blade2_cells.as_slice()),
        ],
        true,
    );
}

fn find_geometry(
    potential_mask: u16,
    box_like: bool,
    geometry_count: usize,
    skip_large_geometry_for_three: bool,
) -> Option<(usize, u16, u16)> {
    for geometry in 0..geometry_count {
        if skip_large_geometry_for_three && geometry > 8 && potential_mask.count_ones() < 4 {
            continue;
        }
        let empty = if box_like {
            BOX_EMPTY[geometry]
        } else {
            LINE_EMPTY[geometry]
        };
        if potential_mask & empty != 0 {
            continue;
        }
        let blade1 = potential_mask
            & if box_like {
                BOX_BLADE_1[geometry]
            } else {
                LINE_BLADE_1[geometry]
            };
        let blade2 = potential_mask
            & if box_like {
                BOX_BLADE_2[geometry]
            } else {
                LINE_BLADE_2[geometry]
            };
        return (!matches!((blade1, blade2), (0, _) | (_, 0)))
            .then_some((geometry, blade1, blade2));
    }
    None
}

fn singleton_blade_endpoints(
    singleton: &[CellId],
    other: &[CellId],
    heart: Option<CellId>,
    box_like: bool,
) -> [CellGroup; 2] {
    debug_assert_eq!(singleton.len(), 1);
    if other.len() == 1 {
        return [
            CellGroup::from_cells(singleton),
            CellGroup::from_cells(other),
        ];
    }
    let mut grouped = Vec::with_capacity(3);
    grouped.push(other[0]);
    if box_like {
        grouped.push(heart.expect("box-like geometry heart"));
        grouped.push(other[1]);
    } else {
        grouped.extend_from_slice(&other[1..]);
    }
    [
        CellGroup::from_cells(singleton),
        CellGroup::from_cells(grouped.as_slice()),
    ]
}

fn revised_singleton_blade_endpoints(
    singleton: &[CellId],
    other: &[CellId],
    heart: Option<CellId>,
    box_like: bool,
) -> [CellGroup; 2] {
    debug_assert_eq!(singleton.len(), 1);
    if !box_like {
        return [
            CellGroup::from_cells(singleton),
            CellGroup::from_cells(other),
        ];
    }
    let mut grouped = Vec::with_capacity(3);
    grouped.push(heart.expect("revised box-like geometry heart"));
    grouped.extend_from_slice(other);
    [
        CellGroup::from_cells(singleton),
        CellGroup::from_cells(grouped.as_slice()),
    ]
}

fn push_candidate(
    result: &mut Vec<LinkCandidate>,
    grid: &Grid,
    region: RegionId,
    positions: PositionMask,
    endpoints: [CellGroup; 2],
    grouped: bool,
) {
    let mut all_cells = CellMask::EMPTY;
    for position in positions.iter() {
        all_cells.insert(region_cell(grid, region, position));
    }
    result.push(LinkCandidate {
        region,
        positions,
        endpoints,
        grouped,
        all_cells,
    });
}

fn cells_for_positions(grid: &Grid, region: RegionId, positions: PositionMask) -> Vec<CellId> {
    positions
        .iter()
        .map(|position| region_cell(grid, region, position))
        .collect()
}

fn cells_for_mask(grid: &Grid, region: RegionId, mask: u16) -> Vec<CellId> {
    cells_for_positions(grid, region, PositionMask::from_bits(mask))
}

const fn is_box_like(type_index: usize) -> bool {
    matches!(type_index, 0 | 3 | 4)
}

fn same_row_or_column(first: CellId, second: CellId) -> bool {
    first.raw() / 9 == second.raw() / 9 || first.raw() % 9 == second.raw() % 9
}

fn region_cell(grid: &Grid, region: RegionId, position: u8) -> CellId {
    CellId::new(grid.topology().region_cells(region)[usize::from(position)]).expect("region cell")
}

fn region_id(type_index: usize, region_index: usize) -> RegionId {
    RegionId::new(type_index as u8, region_index as u8).expect("topology region")
}

fn region_full_number(region: RegionId) -> usize {
    region.type_index() * 10 + region.region_index() + usize::from(region.type_index() <= 4)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{
        CellId, ConstraintTopology, Digit, Grid, Puzzle, RegionId, VariantConfig,
    };

    use super::{
        collect_four_strong_links, collect_three_strong_links, collect_two_strong_links,
        find_four_strong_links, find_three_strong_links, find_two_strong_links,
    };
    use crate::{EngineConfig, Inference, Rating, RatingMode, RatingTracker, Technique};

    fn sparse_snapshot(digit: u8, cells: &[usize]) -> Grid {
        sparse_snapshot_with_variant(digit, cells, VariantConfig::default())
    }

    fn sparse_snapshot_with_variant(digit: u8, cells: &[usize], variant: VariantConfig) -> Grid {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = ['.'; 729];
        for &cell in cells {
            display[cell * 9 + usize::from(digit - 1)] = char::from(b'0' + digit);
        }
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(variant)),
            &values,
            &candidates,
        )
        .unwrap()
    }

    fn assert_fixture(
        mode: RatingMode,
        digit: u8,
        cells: &[usize],
        rating: u16,
        description: &str,
        remaining: &[u8],
    ) {
        let mut grid = sparse_snapshot(digit, cells);
        let inference = find_two_strong_links(
            &grid,
            EngineConfig {
                rating_mode: mode,
                ..EngineConfig::default()
            },
        )
        .expect("focused two-strong-links fixture");
        assert_eq!(
            inference.rating(),
            Rating::from_tenths(rating),
            "{}",
            inference.description(grid.topology())
        );
        assert_eq!(inference.description(grid.topology()), description);
        inference.apply(&mut grid);
        let digit = Digit::new(digit).unwrap();
        assert_eq!(
            grid.candidate_cells(digit)
                .iter()
                .map(CellId::raw)
                .collect::<Vec<_>>(),
            remaining
        );
    }

    #[test]
    fn original_strong_links_fixtures_match_java() {
        assert_fixture(
            RatingMode::Original,
            1,
            &[0, 3, 28, 30, 10],
            40,
            "Skyscraper 011: Cell r1c1,r1c4,r4c4,r4c2 on value 1",
            &[0, 3, 28, 30],
        );
        assert_fixture(
            RatingMode::Original,
            1,
            &[27, 30, 40, 49, 46, 47],
            41,
            "2-String Kite 012: Cell r4c1,r4c4,r5c5,r6c5 on value 1",
            &[27, 30, 40, 49],
        );
        assert_fixture(
            RatingMode::Original,
            1,
            &[1, 9, 18, 36, 40, 4, 7],
            43,
            "Grouped 2 Strong links 101: Cell r1c2,r2c1,r5c1,r5c5 on value 1",
            &[1, 7, 9, 18, 36, 40],
        );
        assert_fixture(
            RatingMode::Original,
            1,
            &[27, 30, 40, 49, 46],
            40,
            "(2 Strong Links) X-Loop 011: Cell r4c1,r4c4,r6c5,r6c2 on value 1",
            &[27, 30, 46, 49],
        );
        assert_fixture(
            RatingMode::Original,
            1,
            &[0, 10, 3, 13, 6, 15],
            41,
            "(2 Strong Links) X-Loop 000: Cell r1c1,r2c2,r2c5,r1c4 on value 1",
            &[0, 3, 10, 13],
        );
    }

    #[test]
    fn full_collectors_preserve_java_rank_order_and_compact_winner() {
        let config = EngineConfig::default();
        let two_grid = sparse_snapshot(1, &[0, 3, 28, 30, 10]);
        let two = collect_two_strong_links(&two_grid, config);
        assert_eq!(two.len(), 5);
        assert_eq!(
            find_two_strong_links(&two_grid, config).as_ref(),
            two.first()
        );
        assert_eq!(
            two.iter()
                .map(|hint| hint.description(two_grid.topology()))
                .collect::<Vec<_>>(),
            [
                "Skyscraper 011: Cell r1c1,r1c4,r4c4,r4c2 on value 1",
                "Skyscraper 011: Cell r2c2,r4c2,r4c4,r1c4 on value 1",
                "2 Strong links 001: Cell r1c1,r2c2,r4c2,r4c4 on value 1",
                "2 Strong links 001: Cell r2c2,r1c1,r1c4,r4c4 on value 1",
                "2-String Kite 012: Cell r1c4,r1c1,r2c2,r4c2 on value 1",
            ]
        );

        let three_grid = sparse_snapshot(
            3,
            &[
                1, 2, 6, 11, 12, 15, 18, 22, 26, 27, 28, 29, 56, 57, 60, 63, 65, 67, 69, 71, 74,
                76, 78,
            ],
        );
        let three = collect_three_strong_links(&three_grid, config);
        assert_eq!(three.len(), 3);
        assert_eq!(
            find_three_strong_links(&three_grid, config).as_ref(),
            three.first()
        );
        assert_eq!(
            three.iter().map(Inference::short_name).collect::<Vec<_>>(),
            ["g3SL1010", "g3SL2012", "g3SL3001"]
        );

        let four_grid = sparse_snapshot(1, &[0, 3, 16, 21, 24, 27, 37, 42, 52]);
        let four = collect_four_strong_links(&four_grid, config);
        assert_eq!(four.len(), 9);
        assert_eq!(
            find_four_strong_links(&four_grid, config).as_ref(),
            four.first()
        );
        assert_eq!(
            four.iter()
                .map(|hint| (hint.rating().tenths(), hint.short_name()))
                .collect::<Vec<_>>(),
            [
                (59, "4SL00001".to_owned()),
                (59, "4SL00001".to_owned()),
                (59, "4SL00011".to_owned()),
                (59, "4SL00011".to_owned()),
                (59, "4SL01121".to_owned()),
                (59, "4SL01121".to_owned()),
                (59, "4SL01212".to_owned()),
                (60, "4SL00112".to_owned()),
                (60, "4SL00112".to_owned()),
            ]
        );

        let revised_config = EngineConfig {
            rating_mode: RatingMode::Revised,
            ..config
        };
        let revised_grid = sparse_snapshot(1, &[0, 3, 27, 31, 39]);
        let revised = collect_two_strong_links(&revised_grid, revised_config);
        assert_eq!(
            find_two_strong_links(&revised_grid, revised_config).as_ref(),
            revised.first()
        );
    }

    #[test]
    fn three_link_family_multisets_and_quickperm_orders_match_java() {
        let mut actual = Vec::new();
        super::visit_three_link_type_multisets(&[0, 1, 2], |types| actual.push(types));
        assert_eq!(
            actual,
            [
                [0, 0, 0],
                [0, 0, 1],
                [0, 1, 1],
                [1, 1, 1],
                [0, 0, 2],
                [0, 1, 2],
                [1, 1, 2],
                [0, 2, 2],
                [1, 2, 2],
                [2, 2, 2],
            ]
        );
        assert_eq!(super::THREE_LINK_ORDERS, [[0, 1, 2], [1, 0, 2], [0, 2, 1]]);
    }

    #[test]
    fn four_link_family_multisets_and_quickperm_orders_match_java() {
        let mut actual = Vec::new();
        super::visit_four_link_type_multisets(&[0, 1, 2], |types| actual.push(types));
        assert_eq!(
            actual,
            [
                [0, 0, 0, 0],
                [0, 0, 0, 1],
                [0, 0, 1, 1],
                [0, 1, 1, 1],
                [1, 1, 1, 1],
                [0, 0, 0, 2],
                [0, 0, 1, 2],
                [0, 1, 1, 2],
                [1, 1, 1, 2],
                [0, 0, 2, 2],
                [0, 1, 2, 2],
                [1, 1, 2, 2],
                [0, 2, 2, 2],
                [1, 2, 2, 2],
                [2, 2, 2, 2],
            ]
        );
        assert_eq!(
            super::FOUR_LINK_ORDERS,
            [
                [0, 1, 2, 3],
                [1, 0, 2, 3],
                [2, 0, 1, 3],
                [0, 2, 1, 3],
                [1, 2, 0, 3],
                [2, 1, 0, 3],
                [0, 3, 2, 1],
                [0, 2, 3, 1],
                [1, 0, 3, 2],
                [0, 1, 3, 2],
                [1, 3, 0, 2],
                [0, 3, 1, 2],
            ]
        );
    }

    #[test]
    fn four_link_shared_region_cache_preserves_order_and_misses() {
        let grid = sparse_snapshot(1, &[0, 1, 40]);
        let endpoint_groups = [
            super::CellGroup::from_cells(&[CellId::new(0).unwrap()]),
            super::CellGroup::from_cells(&[CellId::new(1).unwrap()]),
            super::CellGroup::from_cells(&[CellId::new(40).unwrap()]),
            super::CellGroup::from_cells(&[CellId::new(80).unwrap()]),
            super::CellGroup::from_cells(&[CellId::new(8).unwrap()]),
            super::CellGroup::from_cells(&[CellId::new(72).unwrap()]),
            super::CellGroup::from_cells(&[CellId::new(9).unwrap()]),
            super::CellGroup::from_cells(&[CellId::new(71).unwrap()]),
        ];
        let config = EngineConfig::default();
        let mut cache = [None; 64];

        let row = Some(RegionId::new(1, 0).unwrap());
        assert_eq!(
            super::shared_region(
                &grid,
                config,
                [endpoint_groups[0], endpoint_groups[1]],
                RatingMode::Original,
            ),
            row,
            "row must retain precedence over the shared block"
        );
        assert_eq!(
            super::cached_shared_region(&grid, config, &mut cache, &endpoint_groups, [1, 0],),
            row
        );
        assert_eq!(
            super::cached_shared_region(&grid, config, &mut cache, &endpoint_groups, [0, 1],),
            row
        );

        assert_eq!(
            super::cached_shared_region(&grid, config, &mut cache, &endpoint_groups, [0, 2],),
            None
        );
        assert_eq!(
            super::cached_shared_region(&grid, config, &mut cache, &endpoint_groups, [2, 0],),
            None
        );
        assert_eq!(cache.iter().filter(|entry| entry.is_some()).count(), 2);
    }

    #[test]
    fn four_strong_links_open_ring_and_grouped_fixtures_match_java() {
        let fixtures = [
            (
                VariantConfig::default(),
                &[0, 3, 16, 21, 24, 27, 37, 42, 52][..],
                59,
                "4SL00001",
                "4 Strong links 00001: Cell r3c7,r2c8,r6c8,r5c7,r5c2,r4c1,r1c1,r1c4 on value 1",
                &[0, 3, 16, 24, 27, 37, 42, 52][..],
            ),
            (
                VariantConfig::default(),
                &[0, 3, 9, 21, 24, 37, 42, 54, 55],
                58,
                "4XL01111",
                "(4 Strong Links) X-Loop 01111: Cell r1c1,r1c4,r3c4,r3c7,r5c7,r5c2,r7c2,r7c1 on value 1",
                &[0, 3, 21, 24, 37, 42, 54, 55],
            ),
        ];
        for mode in [RatingMode::Original, RatingMode::Revised] {
            for (variant, cells, rating, short_name, description, remaining) in fixtures {
                let mut grid = sparse_snapshot_with_variant(1, cells, variant);
                let inference = find_four_strong_links(
                    &grid,
                    EngineConfig {
                        rating_mode: mode,
                        ..EngineConfig::default()
                    },
                )
                .expect("focused Java StrongLinks(4) fixture");
                assert_eq!(inference.technique(), Technique::FourStrongLinks);
                assert_eq!(inference.rating(), Rating::from_tenths(rating));
                assert_eq!(inference.short_name(), short_name);
                assert_eq!(inference.description(grid.topology()), description);
                inference.apply(&mut grid);
                assert_eq!(
                    grid.candidate_cells(Digit::new(1).unwrap())
                        .iter()
                        .map(CellId::raw)
                        .collect::<Vec<_>>(),
                    remaining.iter().map(|cell| *cell as u8).collect::<Vec<_>>()
                );
            }

            let cells = [
                2, 5, 7, 8, 9, 15, 23, 25, 26, 27, 29, 34, 35, 37, 43, 49, 55, 57, 63, 65, 71, 72,
                73, 75, 78, 79, 80,
            ];
            let mut grid = sparse_snapshot_with_variant(
                9,
                &cells,
                VariantConfig {
                    anti_knight: true,
                    ..VariantConfig::default()
                },
            );
            let inference = find_four_strong_links(
                &grid,
                EngineConfig {
                    rating_mode: mode,
                    ..EngineConfig::default()
                },
            )
            .expect("anti-knight Java StrongLinks(4) fixture");
            assert_eq!(inference.rating(), Rating::from_tenths(61));
            assert_eq!(inference.short_name(), "g4SL20121");
            assert_eq!(
                inference.description(grid.topology()),
                "Grouped 4 Strong links 20121: Cell r1c3,r2c1,r2c7,r9c7,r8c9,r8c1,r7c2,r5c2 on value 9"
            );
            inference.apply(&mut grid);
            assert_eq!(
                grid.candidate_cells(Digit::new(9).unwrap())
                    .iter()
                    .map(CellId::raw)
                    .collect::<Vec<_>>(),
                cells
                    .into_iter()
                    .filter(|cell| *cell != 29)
                    .map(|cell| cell as u8)
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn three_strong_links_matches_java_in_both_rating_modes() {
        let cells = [
            1, 2, 6, 11, 12, 15, 18, 22, 26, 27, 28, 29, 56, 57, 60, 63, 65, 67, 69, 71, 74, 76, 78,
        ];
        for mode in [RatingMode::Original, RatingMode::Revised] {
            let mut grid = sparse_snapshot(3, &cells);
            let inference = find_three_strong_links(
                &grid,
                EngineConfig {
                    rating_mode: mode,
                    ..EngineConfig::default()
                },
            )
            .expect("focused Java StrongLinks(3) fixture");
            assert_eq!(inference.technique(), Technique::ThreeStrongLinks);
            assert_eq!(inference.rating(), Rating::from_tenths(57));
            assert_eq!(inference.short_name(), "g3SL1010");
            assert_eq!(
                inference.description(grid.topology()),
                "Grouped 3 Strong links 1010: Cell r2c4,r3c5,r3c9,r8c9,r8c1,r7c3 on value 3"
            );
            inference.apply(&mut grid);
            assert_eq!(
                grid.candidate_cells(Digit::new(3).unwrap())
                    .iter()
                    .map(CellId::raw)
                    .collect::<Vec<_>>(),
                cells
                    .into_iter()
                    .filter(|cell| *cell != 11)
                    .map(|cell| cell as u8)
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn three_strong_links_open_and_ring_fixtures_match_java() {
        for (cells, short_name, description, remaining) in [
            (
                &[0, 3, 12, 30, 33, 54, 60][..],
                "3XL0111",
                "(3 Strong Links) X-Loop 0111: Cell r1c1,r1c4,r4c4,r4c7,r7c7,r7c1 on value 1",
                &[0, 3, 30, 33, 54, 60][..],
            ),
            (
                &[0, 3, 21, 24, 27, 37, 42],
                "3SS0111",
                "3 Skyscrapers 0111: Cell r1c1,r1c4,r3c4,r3c7,r5c7,r5c2 on value 1",
                &[0, 3, 21, 24, 37, 42],
            ),
        ] {
            let mut grid = sparse_snapshot(1, cells);
            let inference = find_three_strong_links(&grid, EngineConfig::default())
                .expect("Java StrongLinks(3) fixture");
            assert_eq!(inference.rating(), Rating::from_tenths(54));
            assert_eq!(inference.short_name(), short_name);
            assert_eq!(inference.description(grid.topology()), description);
            inference.apply(&mut grid);
            assert_eq!(
                grid.candidate_cells(Digit::new(1).unwrap())
                    .iter()
                    .map(CellId::raw)
                    .collect::<Vec<_>>(),
                remaining.iter().map(|cell| *cell as u8).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn three_strong_links_rating_table_preserves_legacy_suffix_classes() {
        let block = RegionId::new(0, 0).unwrap();
        let rows = [
            RegionId::new(1, 0).unwrap(),
            RegionId::new(1, 1).unwrap(),
            RegionId::new(1, 2).unwrap(),
        ];
        let column = RegionId::new(2, 0).unwrap();
        let disjoint_group = RegionId::new(3, 0).unwrap();
        let order = [0, 1, 2];
        assert_eq!(
            super::three_strong_links_rating(rows, order, [false; 3]),
            Rating::from_tenths(54)
        );
        assert_eq!(
            super::three_strong_links_rating([block, rows[0], rows[1]], order, [false; 3]),
            Rating::from_tenths(55)
        );
        assert_eq!(
            super::three_strong_links_rating([block, rows[0], column], order, [false; 3]),
            Rating::from_tenths(56)
        );
        assert_eq!(
            super::three_strong_links_rating(rows, order, [true, false, false]),
            Rating::from_tenths(57)
        );
        assert_eq!(
            super::three_strong_links_rating([rows[0], rows[1], disjoint_group], order, [false; 3]),
            Rating::from_tenths(57)
        );
    }

    #[test]
    fn four_strong_links_rating_table_preserves_legacy_suffix_classes() {
        let block = RegionId::new(0, 0).unwrap();
        let rows = [
            RegionId::new(1, 0).unwrap(),
            RegionId::new(1, 1).unwrap(),
            RegionId::new(1, 2).unwrap(),
            RegionId::new(1, 3).unwrap(),
        ];
        let column = RegionId::new(2, 0).unwrap();
        let disjoint_group = RegionId::new(3, 0).unwrap();
        let order = [0, 1, 2, 3];
        assert_eq!(
            super::four_strong_links_rating(rows, order, [false; 4]),
            Rating::from_tenths(58)
        );
        assert_eq!(
            super::four_strong_links_rating([block, rows[0], rows[1], rows[2]], order, [false; 4],),
            Rating::from_tenths(59)
        );
        assert_eq!(
            super::four_strong_links_rating([block, rows[0], rows[1], column], order, [false; 4],),
            Rating::from_tenths(60)
        );
        assert_eq!(
            super::four_strong_links_rating(
                [disjoint_group, rows[0], rows[1], rows[2]],
                order,
                [false; 4],
            ),
            Rating::from_tenths(61)
        );
        assert_eq!(
            super::four_strong_links_rating(rows, order, [true, false, false, false]),
            Rating::from_tenths(61)
        );
    }

    #[test]
    fn revised_turbot_fixtures_match_java() {
        assert_fixture(
            RatingMode::Revised,
            1,
            &[0, 3, 27, 31, 39],
            40,
            "Skyscraper: Cells r1c4,r1c1,r4c1,r4c5 on value 1",
            &[0, 3, 27, 31],
        );
        assert_fixture(
            RatingMode::Revised,
            1,
            &[0, 36, 10, 13, 40],
            40,
            "Skyscraper: Cells r2c2,r2c5,r5c5,r5c1 on value 1",
            &[10, 13, 36, 40],
        );
        assert_fixture(
            RatingMode::Revised,
            1,
            &[0, 4, 31, 41, 36],
            40,
            "Skyscraper: Cells r1c5,r1c1,r5c1,r5c6 on value 1",
            &[0, 4, 36, 41],
        );
        assert_fixture(
            RatingMode::Revised,
            6,
            &[22, 76, 79, 7, 25, 26],
            43,
            "Grouped Skyscraper 01: Cells r3c8,r3c5,r9c5,r9c8 on value 6",
            &[22, 25, 26, 76, 79],
        );
        assert_fixture(
            RatingMode::Revised,
            9,
            &[37, 55, 73, 63, 65, 71, 44],
            43,
            "Grouped Skyscraper 01: Cells r5c2,r5c9,r8c9,r8c1 on value 9",
            &[37, 44, 63, 65, 71],
        );
        assert_fixture(
            RatingMode::Revised,
            1,
            &[0, 3, 27, 30, 9, 12],
            40,
            "Skyscraper X-Loop: Cells r1c1,r1c4,r2c4,r2c1 on value 1",
            &[0, 3, 9, 12],
        );
    }

    #[test]
    fn revised_rating_table_keeps_kite_and_crane_distinct() {
        let row = RegionId::new(1, 0).unwrap();
        let column = RegionId::new(2, 0).unwrap();
        let block = RegionId::new(0, 0).unwrap();
        assert_eq!(
            super::two_strong_links_rating([row, column], [false, false], RatingMode::Revised),
            Rating::from_tenths(41)
        );
        assert_eq!(
            super::two_strong_links_rating([row, block], [false, false], RatingMode::Revised),
            Rating::from_tenths(42)
        );
    }

    #[test]
    fn revised_line_catalog_rejects_a_nonempty_partition_without_overrun() {
        let grid = sparse_snapshot(1, &[0, 3, 6]);
        let mut candidates = Vec::new();
        super::add_revised_region_candidates(
            &grid,
            Digit::new(1).unwrap(),
            RegionId::new(1, 0).unwrap(),
            &mut candidates,
        );
        assert!(candidates.is_empty());
    }

    #[test]
    fn revised_box_singletons_use_the_heart_as_group_representative() {
        let grid = sparse_snapshot(1, &[0, 1, 18]);
        let mut candidates = Vec::new();
        super::add_revised_region_candidates(
            &grid,
            Digit::new(1).unwrap(),
            RegionId::new(0, 0).unwrap(),
            &mut candidates,
        );
        assert_eq!(candidates.len(), 2);
        assert_eq!(
            candidates[0].endpoints[0]
                .iter()
                .map(CellId::raw)
                .collect::<Vec<_>>(),
            [18]
        );
        assert_eq!(
            candidates[0].endpoints[1]
                .iter()
                .map(CellId::raw)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        assert_eq!(
            candidates[1].endpoints[0]
                .iter()
                .map(CellId::raw)
                .collect::<Vec<_>>(),
            [1]
        );
        assert_eq!(
            candidates[1].endpoints[1]
                .iter()
                .map(CellId::raw)
                .collect::<Vec<_>>(),
            [0, 18]
        );
    }

    #[test]
    fn rating_tracker_retains_the_pattern_specific_name() {
        let grid = sparse_snapshot(1, &[0, 3, 28, 30, 10]);
        let inference = find_two_strong_links(&grid, EngineConfig::default()).unwrap();
        let mut tracker = RatingTracker::default();
        tracker.observe(&inference);
        let result = tracker.result();
        assert_eq!(result.er().name(), "Skyscraper 011");
        assert_eq!(result.er().short_name(), "SS011");
    }
}
