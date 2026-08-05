//! Ordered, semantic presentation data for graphical Sukaku Forge clients.
//!
//! The types in this crate deliberately describe meaning rather than paint.
//! A frontend theme decides how a selected region, a positive candidate, or
//! an elimination is rendered. Producer order is retained in every `Vec`.

pub mod wire;

use sukaku_forge_core::{CandidateMask, CellId, Digit, Grid, PositionMask, RegionId};
use sukaku_forge_engine::{
    CellSequence, ChainCause, ChainProofView, ChainProofViewKind, ChainState, Evidence, Inference,
    Rating, RatingMode, SelectedChainProof, Technique,
};

/// Presentation metadata that remains stable across views of one hint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HintIdentity {
    pub technique: Technique,
    pub name: String,
    pub short_name: String,
    pub rating: Rating,
}

/// Complete frontend-neutral presentation of one inference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HintPresentation {
    pub identity: HintIdentity,
    pub views: Vec<HintView>,
    pub explanation: ExplanationDoc,
}

/// One switchable visual view of a hint.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HintView {
    /// Stable machine key used to retain frontend view selection.
    pub key: String,
    /// Human-readable tab label.
    pub label: String,
    pub cell_marks: Vec<CellMark>,
    pub region_marks: Vec<RegionMark>,
    pub candidate_marks: Vec<CandidateMark>,
    pub links: Vec<CandidateLink>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CellMark {
    pub cell: CellId,
    pub roles: HighlightRoles,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionMark {
    pub region: RegionId,
    pub roles: HighlightRoles,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CandidateRef {
    pub cell: CellId,
    pub digit: Digit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandidateMark {
    pub candidate: CandidateRef,
    /// Roles are a set: in particular, `POSITIVE | NEGATIVE` is meaningful.
    /// It says that a pattern candidate is also the hint's eliminated effect.
    pub roles: HighlightRoles,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandidateLink {
    pub from: LinkEndpoint,
    pub to: LinkEndpoint,
    pub kind: LinkKind,
    pub cause: LinkCause,
    pub directed: bool,
}

/// A semantic link endpoint. Group membership is retained instead of being
/// flattened into unrelated selected cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkEndpoint {
    Candidate(CandidateRef),
    /// One logical endpoint made from all candidates for the same digit in a
    /// region. The representative anchors geometry; members preserve the
    /// complete semantic side of the link.
    CandidateGroup {
        representative: CandidateRef,
        members: CellSequence,
    },
    CellCenter(CellId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkKind {
    Strong,
    GroupedStrong,
    Weak,
    Implication,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkCause {
    Cell,
    Region(RegionId),
    Visibility,
    Derived,
}

/// Composable semantic highlight roles.
///
/// This is intentionally a tiny dependency-free bit set rather than a color
/// enum. Themes may map the same role to different colors or non-color cues.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HighlightRoles(u16);

impl HighlightRoles {
    pub const SELECTED: Self = Self(1 << 0);
    pub const PATTERN: Self = Self(1 << 1);
    pub const POSITIVE: Self = Self(1 << 2);
    pub const NEGATIVE: Self = Self(1 << 3);
    pub const AUXILIARY: Self = Self(1 << 4);
    pub const CONCLUSION: Self = Self(1 << 5);
    pub const PRIMARY: Self = Self(1 << 6);
    pub const SECONDARY: Self = Self(1 << 7);

    /// Stable primitive mask used by transport projections.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    #[must_use]
    pub const fn contains(self, role: Self) -> bool {
        self.0 & role.0 == role.0
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl core::ops::BitOr for HighlightRoles {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl core::ops::BitOrAssign for HighlightRoles {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

/// Small typed explanation tree. Renderers can produce HTML, native text, or
/// accessible speech without parsing solver-formatted strings.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExplanationDoc {
    pub blocks: Vec<ExplanationBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExplanationBlock {
    Paragraph(Vec<ExplanationInline>),
    UnorderedList(Vec<Vec<ExplanationInline>>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExplanationInline {
    Text(String),
    Technique(Technique),
    Cell(CellId),
    Digit(Digit),
    Region(RegionId),
    Candidate(CandidateRef),
}

/// Why an inference cannot yet be represented faithfully by this crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsupportedPresentationKind {
    /// Compact rating evidence intentionally omits the ordered proof graph.
    MissingChainProof,
    /// The evidence family has not yet received a presentation adapter.
    EvidenceNotImplemented,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnsupportedPresentation {
    pub technique: Technique,
    pub kind: UnsupportedPresentationKind,
}

/// Convert one inference against the exact grid state on which it was found.
///
/// Chain evidence is rejected explicitly: its compact rating representation
/// does not contain enough information to recreate honest links or views.
pub fn present(
    pre_grid: &Grid,
    inference: &Inference,
) -> Result<HintPresentation, UnsupportedPresentation> {
    let mut view = HintView {
        key: "main".to_owned(),
        label: "View 1".to_owned(),
        ..HintView::default()
    };

    match inference.evidence() {
        Evidence::HiddenSingle { region, .. } => {
            add_region(
                &mut view,
                region,
                HighlightRoles::PRIMARY | HighlightRoles::PATTERN,
            );
            add_placement(&mut view, inference);
        }
        Evidence::NakedSingle => add_placement(&mut view, inference),
        Evidence::DirectLocking {
            primary,
            secondary,
            pattern_positions,
        } => {
            add_region(
                &mut view,
                primary,
                HighlightRoles::PRIMARY | HighlightRoles::PATTERN,
            );
            add_region(
                &mut view,
                secondary,
                HighlightRoles::SECONDARY | HighlightRoles::PATTERN,
            );
            let digit = inference
                .placement_digit()
                .expect("direct locking is a placement");
            add_region_candidates(
                &mut view,
                pre_grid,
                secondary,
                pattern_positions,
                digit,
                HighlightRoles::POSITIVE | HighlightRoles::PATTERN,
            );
            add_placement(&mut view, inference);
        }
        Evidence::Locking {
            primary,
            secondary,
            digit,
            pattern_positions,
        } => {
            add_region(
                &mut view,
                primary,
                HighlightRoles::PRIMARY | HighlightRoles::PATTERN,
            );
            add_region(
                &mut view,
                secondary,
                HighlightRoles::SECONDARY | HighlightRoles::PATTERN,
            );
            add_region_candidates(
                &mut view,
                pre_grid,
                secondary,
                pattern_positions,
                digit,
                HighlightRoles::POSITIVE | HighlightRoles::PATTERN,
            );
            add_removals(&mut view, inference);
        }
        Evidence::HiddenSet {
            region,
            tuple_digits,
            tuple_positions,
            ..
        }
        | Evidence::NakedSet {
            region,
            tuple_digits,
            tuple_positions,
            ..
        } => {
            add_region(
                &mut view,
                region,
                HighlightRoles::PRIMARY | HighlightRoles::PATTERN,
            );
            for position in tuple_positions.iter() {
                let cell = region_cell(pre_grid, region, position);
                add_cell(
                    &mut view,
                    cell,
                    HighlightRoles::SELECTED | HighlightRoles::PATTERN,
                );
                for digit in pre_grid.candidates(cell).intersect(tuple_digits).iter() {
                    add_candidate(
                        &mut view,
                        CandidateRef { cell, digit },
                        HighlightRoles::POSITIVE | HighlightRoles::PATTERN,
                    );
                }
            }
            if inference.is_placement() {
                add_placement(&mut view, inference);
            } else {
                add_removals(&mut view, inference);
            }
        }
        Evidence::Fish {
            digit,
            base_type,
            cover_type,
            selected_cells,
            ..
        } => {
            for cell in selected_cells.iter() {
                add_cell(
                    &mut view,
                    cell,
                    HighlightRoles::SELECTED | HighlightRoles::PATTERN,
                );
                add_candidate(
                    &mut view,
                    CandidateRef { cell, digit },
                    HighlightRoles::POSITIVE | HighlightRoles::PATTERN,
                );
                add_cell_region(
                    &mut view,
                    pre_grid,
                    cell,
                    usize::from(base_type),
                    HighlightRoles::PRIMARY | HighlightRoles::PATTERN,
                );
                add_cell_region(
                    &mut view,
                    pre_grid,
                    cell,
                    usize::from(cover_type),
                    HighlightRoles::SECONDARY | HighlightRoles::PATTERN,
                );
            }
            add_removals(&mut view, inference);
        }
        Evidence::TwoStrongLinks {
            digit,
            pattern_cells,
            link_regions,
            endpoint_groups,
            bridge_region,
            ring_region,
            grouped_links,
            rating_mode,
            ..
        } => {
            add_strong_links(
                &mut view,
                digit,
                pattern_cells,
                &link_regions,
                &endpoint_groups,
                &[bridge_region],
                ring_region,
                &grouped_links,
                &[0, 1],
                rating_mode == RatingMode::Revised,
            );
            add_removals(&mut view, inference);
        }
        Evidence::ThreeStrongLinks {
            digit,
            pattern_cells,
            link_regions,
            endpoint_groups,
            bridge_regions,
            ring_region,
            grouped_links,
            link_order,
            ..
        } => {
            add_strong_links(
                &mut view,
                digit,
                pattern_cells,
                &link_regions,
                &endpoint_groups,
                &bridge_regions,
                ring_region,
                &grouped_links,
                &link_order,
                false,
            );
            add_removals(&mut view, inference);
        }
        Evidence::FourStrongLinks {
            digit,
            pattern_cells,
            link_regions,
            endpoint_groups,
            bridge_regions,
            ring_region,
            grouped_links,
            link_order,
            ..
        } => {
            add_strong_links(
                &mut view,
                digit,
                pattern_cells,
                &link_regions,
                &endpoint_groups,
                &bridge_regions,
                ring_region,
                &grouped_links,
                &link_order,
                false,
            );
            add_removals(&mut view, inference);
        }
        Evidence::Wing {
            pivot,
            xz,
            yz,
            digit,
        } => {
            add_wing(&mut view, pre_grid, pivot, xz, yz, digit);
            add_removals(&mut view, inference);
        }
        Evidence::AlignedPairExclusion {
            cells,
            locked_combinations,
        } => {
            add_aligned_bases(&mut view, pre_grid, &cells);
            for (_, _, locking_cell) in locked_combinations.iter() {
                if let Some(cell) = locking_cell {
                    add_auxiliary_cell(&mut view, pre_grid, cell);
                }
            }
            add_removals(&mut view, inference);
        }
        Evidence::AlignedTripletExclusion {
            cells,
            locked_combinations,
        } => {
            add_aligned_bases(&mut view, pre_grid, &cells);
            for (_, locking_cell) in locked_combinations.iter() {
                if let Some(cell) = locking_cell {
                    add_auxiliary_cell(&mut view, pre_grid, cell);
                }
            }
            add_removals(&mut view, inference);
        }
        Evidence::ForcingChainCycle { .. }
        | Evidence::NishioForcingChain { .. }
        | Evidence::MultipleForcingChain { .. } => {
            return Err(UnsupportedPresentation {
                technique: inference.technique(),
                kind: UnsupportedPresentationKind::MissingChainProof,
            });
        }
        Evidence::NonConsecutive { .. }
        | Evidence::GeneralizedIntersections { .. }
        | Evidence::AlphabetWing { .. }
        | Evidence::UniqueLoop { .. }
        | Evidence::Bug { .. } => {
            return Err(UnsupportedPresentation {
                technique: inference.technique(),
                kind: UnsupportedPresentationKind::EvidenceNotImplemented,
            });
        }
    }

    Ok(HintPresentation {
        identity: HintIdentity {
            technique: inference.technique(),
            name: inference.name(),
            short_name: inference.short_name(),
            rating: inference.rating(),
        },
        views: vec![view],
        explanation: explanation(pre_grid, inference),
    })
}

/// Present a selected static Forcing Chains & Cycles inference with the
/// ordered proof materialized by the engine's opt-in GUI search.
///
/// Compact FCC evidence deliberately cannot recreate these views; callers
/// should use [`present`] when no selected proof accompanies an inference.
pub fn present_with_selected_chain_proof(
    pre_grid: &Grid,
    inference: &Inference,
    selected_proof: &SelectedChainProof,
) -> Result<HintPresentation, UnsupportedPresentation> {
    if !matches!(inference.evidence(), Evidence::ForcingChainCycle { .. }) {
        return present(pre_grid, inference);
    }

    let views = selected_proof
        .views()
        .iter()
        .map(|proof| present_chain_proof_view(inference, proof))
        .collect();
    Ok(HintPresentation {
        identity: HintIdentity {
            technique: inference.technique(),
            name: inference.name(),
            short_name: inference.short_name(),
            rating: inference.rating(),
        },
        views,
        explanation: explanation(pre_grid, inference),
    })
}

fn present_chain_proof_view(inference: &Inference, proof: &ChainProofView) -> HintView {
    let (key, label) = match proof.kind() {
        ChainProofViewKind::Forcing => ("forcing", "Forcing chain"),
        ChainProofViewKind::CycleForward => ("cycle-forward", "Cycle forward"),
        ChainProofViewKind::CycleReverse => ("cycle-reverse", "Cycle reverse"),
    };
    let mut view = HintView {
        key: key.to_owned(),
        label: label.to_owned(),
        ..HintView::default()
    };
    let target = proof.target();

    for (index, node) in proof.nodes().iter().enumerate() {
        let mut roles = HighlightRoles::PATTERN;
        if index == target.index() {
            roles |= if node.state() == ChainState::On {
                HighlightRoles::POSITIVE
            } else {
                HighlightRoles::NEGATIVE
            };
        } else {
            // Java paints every non-target chain potential green, then paints
            // OFF potentials red as well. The overlap is the legacy orange.
            roles |= HighlightRoles::POSITIVE;
            if node.state() == ChainState::Off {
                roles |= HighlightRoles::NEGATIVE;
            }
        }
        add_candidate(
            &mut view,
            CandidateRef {
                cell: node.cell(),
                digit: node.digit(),
            },
            roles,
        );

        for &parent_edge in node.parents() {
            let parent = proof
                .node(parent_edge.node())
                .expect("selected chain parent belongs to its view");
            view.links.push(CandidateLink {
                from: LinkEndpoint::Candidate(CandidateRef {
                    cell: parent.cell(),
                    digit: parent.digit(),
                }),
                to: LinkEndpoint::Candidate(CandidateRef {
                    cell: node.cell(),
                    digit: node.digit(),
                }),
                kind: match node.state() {
                    ChainState::On => LinkKind::Strong,
                    ChainState::Off => LinkKind::Weak,
                },
                cause: match parent_edge.cause() {
                    ChainCause::None => LinkCause::Derived,
                    ChainCause::Cell => LinkCause::Cell,
                    ChainCause::Region(region) => LinkCause::Region(region),
                    ChainCause::Visibility => LinkCause::Visibility,
                },
                directed: true,
            });
        }
    }

    if inference.is_placement() {
        add_placement(&mut view, inference);
    } else {
        add_removals(&mut view, inference);
    }
    view
}

fn explanation(pre_grid: &Grid, inference: &Inference) -> ExplanationDoc {
    let mut conclusion = vec![ExplanationInline::Text("Therefore ".to_owned())];
    if let (Some(cell), Some(digit)) = (inference.placement_cell(), inference.placement_digit()) {
        conclusion.extend([
            ExplanationInline::Cell(cell),
            ExplanationInline::Text(" contains ".to_owned()),
            ExplanationInline::Digit(digit),
            ExplanationInline::Text(".".to_owned()),
        ]);
    } else {
        conclusion.push(ExplanationInline::Text(
            "the marked negative candidates can be removed.".to_owned(),
        ));
    }
    ExplanationDoc {
        blocks: vec![
            ExplanationBlock::Paragraph(vec![ExplanationInline::Text(
                inference.description(pre_grid.topology()),
            )]),
            ExplanationBlock::Paragraph(conclusion),
        ],
    }
}

fn add_wing(view: &mut HintView, grid: &Grid, pivot: CellId, xz: CellId, yz: CellId, z: Digit) {
    for cell in [pivot, xz, yz] {
        add_cell(
            view,
            cell,
            HighlightRoles::SELECTED | HighlightRoles::PATTERN,
        );
    }

    let z_mask = CandidateMask::of(z);
    let x = grid
        .candidates(xz)
        .without(z_mask)
        .single()
        .expect("wing XZ cell has one non-Z candidate");
    let y = grid
        .candidates(yz)
        .without(z_mask)
        .single()
        .expect("wing YZ cell has one non-Z candidate");

    // Java paints every pivot candidate green and paints X/Y red as well.
    // The overlap is intentional orange evidence rather than an elimination.
    for digit in grid.candidates(pivot).iter() {
        add_candidate(
            view,
            CandidateRef { cell: pivot, digit },
            HighlightRoles::POSITIVE | HighlightRoles::PATTERN,
        );
    }
    for digit in [x, y] {
        add_candidate(
            view,
            CandidateRef { cell: pivot, digit },
            HighlightRoles::NEGATIVE | HighlightRoles::PATTERN,
        );
    }
    for cell in [xz, yz] {
        add_candidate(
            view,
            CandidateRef { cell, digit: z },
            HighlightRoles::POSITIVE | HighlightRoles::PATTERN,
        );
    }

    for (digit, cell) in [(x, xz), (y, yz)] {
        view.links.push(CandidateLink {
            from: LinkEndpoint::Candidate(CandidateRef { cell: pivot, digit }),
            to: LinkEndpoint::Candidate(CandidateRef { cell, digit }),
            kind: LinkKind::Weak,
            cause: LinkCause::Visibility,
            directed: true,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn add_strong_links(
    view: &mut HintView,
    digit: Digit,
    pattern_cells: CellSequence,
    link_regions: &[RegionId],
    endpoint_groups: &[CellSequence],
    bridge_regions: &[RegionId],
    ring_region: Option<RegionId>,
    grouped_links: &[bool],
    link_order: &[u8],
    turbot_region_order: bool,
) {
    let representatives = pattern_cells.iter().collect::<Vec<_>>();
    let link_count = link_regions.len();
    debug_assert_eq!(representatives.len(), link_count * 2);
    debug_assert_eq!(endpoint_groups.len(), link_count * 2);
    debug_assert_eq!(bridge_regions.len() + 1, link_count);
    debug_assert_eq!(grouped_links.len(), link_count);
    debug_assert_eq!(link_order.len(), link_count);

    // Java exposes only bridge regions for an open StrongLinks hint. Rings
    // alternate displayed base/bridge regions. Revised Turbot Fish always
    // exposes base, bridge, cover in its displayed chain order.
    if turbot_region_order || ring_region.is_some() {
        for displayed_index in 0..link_count {
            let base_index = usize::from(link_order[displayed_index]);
            add_region(
                view,
                link_regions[base_index],
                HighlightRoles::PRIMARY | HighlightRoles::PATTERN,
            );
            if displayed_index < bridge_regions.len() {
                add_region(
                    view,
                    bridge_regions[displayed_index],
                    HighlightRoles::SECONDARY | HighlightRoles::PATTERN,
                );
            } else if !turbot_region_order {
                add_region(
                    view,
                    ring_region.expect("ring region exists"),
                    HighlightRoles::SECONDARY | HighlightRoles::PATTERN,
                );
            }
        }
    } else {
        for &region in bridge_regions {
            add_region(
                view,
                region,
                HighlightRoles::SECONDARY | HighlightRoles::PATTERN,
            );
        }
    }

    if !grouped_links.iter().any(|&grouped| grouped) {
        for &cell in [
            representatives.first().expect("strong-link start"),
            representatives.last().expect("strong-link end"),
        ] {
            add_cell(
                view,
                cell,
                HighlightRoles::SELECTED | HighlightRoles::PATTERN,
            );
        }
    }

    // Strong links follow the displayed chain permutation; endpoint-group
    // metadata remains in the engine's base-link order.
    for displayed_index in 0..link_count {
        let base_index = usize::from(link_order[displayed_index]);
        let from = CandidateRef {
            cell: representatives[displayed_index * 2],
            digit,
        };
        let to = CandidateRef {
            cell: representatives[displayed_index * 2 + 1],
            digit,
        };
        let grouped = grouped_links[base_index];
        let endpoint_pair = &endpoint_groups[base_index * 2..base_index * 2 + 2];
        let from_group = endpoint_pair
            .iter()
            .copied()
            .find(|group| group.iter().any(|cell| cell == from.cell))
            .unwrap_or(endpoint_pair[0]);
        let to_group = endpoint_pair
            .iter()
            .copied()
            .find(|group| group.iter().any(|cell| cell == to.cell))
            .unwrap_or(endpoint_pair[1]);

        if grouped {
            for cell in from_group.iter().chain(to_group.iter()) {
                add_cell(
                    view,
                    cell,
                    HighlightRoles::SELECTED | HighlightRoles::PATTERN,
                );
            }
        } else {
            for candidate in [from, to] {
                add_candidate(
                    view,
                    candidate,
                    HighlightRoles::POSITIVE | HighlightRoles::PATTERN,
                );
            }
        }
        view.links.push(CandidateLink {
            from: strong_link_endpoint(from, from_group),
            to: strong_link_endpoint(to, to_group),
            kind: if grouped {
                LinkKind::GroupedStrong
            } else {
                LinkKind::Strong
            },
            cause: LinkCause::Region(link_regions[base_index]),
            directed: true,
        });
    }

    // Weak links retain chain order after Java's ordered base-link collection.
    for (bridge_index, &bridge_region) in bridge_regions.iter().enumerate() {
        let weak_from = CandidateRef {
            cell: representatives[bridge_index * 2 + 1],
            digit,
        };
        let weak_to = CandidateRef {
            cell: representatives[bridge_index * 2 + 2],
            digit,
        };
        view.links.push(CandidateLink {
            from: LinkEndpoint::Candidate(weak_from),
            to: LinkEndpoint::Candidate(weak_to),
            kind: LinkKind::Weak,
            cause: LinkCause::Region(bridge_region),
            directed: true,
        });
    }
    if let Some(region) = ring_region {
        view.links.push(CandidateLink {
            from: LinkEndpoint::Candidate(CandidateRef {
                cell: *representatives.last().expect("strong-link ring end"),
                digit,
            }),
            to: LinkEndpoint::Candidate(CandidateRef {
                cell: representatives[0],
                digit,
            }),
            kind: LinkKind::Weak,
            cause: LinkCause::Region(region),
            directed: true,
        });
    }
}

fn strong_link_endpoint(representative: CandidateRef, members: CellSequence) -> LinkEndpoint {
    if members.len() > 1 {
        LinkEndpoint::CandidateGroup {
            representative,
            members,
        }
    } else {
        LinkEndpoint::Candidate(representative)
    }
}

fn add_placement(view: &mut HintView, inference: &Inference) {
    let cell = inference
        .placement_cell()
        .expect("placement presentation requires a cell");
    let digit = inference
        .placement_digit()
        .expect("placement presentation requires a digit");
    add_cell(
        view,
        cell,
        HighlightRoles::SELECTED | HighlightRoles::CONCLUSION,
    );
    add_candidate(
        view,
        CandidateRef { cell, digit },
        HighlightRoles::POSITIVE | HighlightRoles::CONCLUSION,
    );
}

fn add_removals(view: &mut HintView, inference: &Inference) {
    for removal in inference.removals().iter() {
        for digit in removal.digits().iter() {
            add_candidate(
                view,
                CandidateRef {
                    cell: removal.cell(),
                    digit,
                },
                HighlightRoles::NEGATIVE | HighlightRoles::CONCLUSION,
            );
        }
    }
}

fn add_aligned_bases<const N: usize>(view: &mut HintView, grid: &Grid, cells: &[CellId; N]) {
    for &cell in cells {
        add_cell(
            view,
            cell,
            HighlightRoles::SELECTED | HighlightRoles::PATTERN,
        );
        for digit in grid.candidates(cell).iter() {
            add_candidate(
                view,
                CandidateRef { cell, digit },
                HighlightRoles::POSITIVE | HighlightRoles::PATTERN,
            );
        }
    }
}

fn add_auxiliary_cell(view: &mut HintView, grid: &Grid, cell: CellId) {
    add_cell(view, cell, HighlightRoles::AUXILIARY);
    for digit in grid.candidates(cell).iter() {
        add_candidate(
            view,
            CandidateRef { cell, digit },
            HighlightRoles::AUXILIARY,
        );
    }
}

fn add_region_candidates(
    view: &mut HintView,
    grid: &Grid,
    region: RegionId,
    positions: PositionMask,
    digit: Digit,
    roles: HighlightRoles,
) {
    for position in positions.iter() {
        add_candidate(
            view,
            CandidateRef {
                cell: region_cell(grid, region, position),
                digit,
            },
            roles,
        );
    }
}

fn add_cell_region(
    view: &mut HintView,
    grid: &Grid,
    cell: CellId,
    type_index: usize,
    roles: HighlightRoles,
) {
    if let Some(region_index) = grid.topology().cell_region_index(cell, type_index) {
        let region = RegionId::new(type_index as u8, region_index).expect("topology region id");
        add_region(view, region, roles);
    }
}

fn add_cell(view: &mut HintView, cell: CellId, roles: HighlightRoles) {
    if let Some(mark) = view.cell_marks.iter_mut().find(|mark| mark.cell == cell) {
        mark.roles |= roles;
    } else {
        view.cell_marks.push(CellMark { cell, roles });
    }
}

fn add_region(view: &mut HintView, region: RegionId, roles: HighlightRoles) {
    if let Some(mark) = view
        .region_marks
        .iter_mut()
        .find(|mark| mark.region == region)
    {
        mark.roles |= roles;
    } else {
        view.region_marks.push(RegionMark { region, roles });
    }
}

fn add_candidate(view: &mut HintView, candidate: CandidateRef, roles: HighlightRoles) {
    if let Some(mark) = view
        .candidate_marks
        .iter_mut()
        .find(|mark| mark.candidate == candidate)
    {
        mark.roles |= roles;
    } else {
        view.candidate_marks
            .push(CandidateMark { candidate, roles });
    }
}

fn region_cell(grid: &Grid, region: RegionId, position: u8) -> CellId {
    CellId::new(grid.topology().region_cells(region)[usize::from(position)])
        .expect("topology region cell")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{
        CellId, ConstraintTopology, Digit, Grid, Puzzle, RegionId, VariantConfig,
    };
    use sukaku_forge_engine::{
        CellSequence, EngineConfig, Evidence, Inference, RatingMode, find_aligned_pair_exclusion,
        find_forcing_chain_cycle_with_proof, find_four_strong_links, find_three_strong_links,
        find_two_strong_links, find_wing,
    };

    use super::{
        CandidateRef, HighlightRoles, LinkCause, LinkEndpoint, LinkKind, present,
        present_with_selected_chain_proof,
    };

    fn sparse_snapshot(entries: &[(usize, &str)]) -> Grid {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = ['.'; 729];
        for &(cell, candidates) in entries {
            for digit in candidates.bytes() {
                display[cell * 9 + usize::from(digit - b'1')] = char::from(digit);
            }
        }
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &values,
            &candidates,
        )
        .unwrap()
    }

    fn sparse_digit_snapshot(digit: u8, cells: &[usize], variant: VariantConfig) -> Grid {
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

    #[test]
    fn wing_retains_pattern_then_effect_order_and_roles() {
        let grid = sparse_snapshot(&[(0, "12"), (3, "13"), (27, "23"), (30, "3")]);
        let inference = find_wing(&grid, false).unwrap();
        let presentation = present(&grid, &inference).unwrap();
        let view = &presentation.views[0];

        assert_eq!(view.key, "main");
        assert_eq!(view.label, "View 1");

        assert_eq!(
            view.cell_marks
                .iter()
                .map(|mark| mark.cell.raw())
                .collect::<Vec<_>>(),
            [0, 3, 27]
        );
        assert_eq!(
            view.candidate_marks
                .iter()
                .map(|mark| (mark.candidate.cell.raw(), mark.candidate.digit.get()))
                .collect::<Vec<_>>(),
            [(0, 1), (0, 2), (3, 3), (27, 3), (30, 3)]
        );
        let victim = view.candidate_marks.last().unwrap();
        assert!(victim.roles.contains(HighlightRoles::NEGATIVE));
        assert!(victim.roles.contains(HighlightRoles::CONCLUSION));
    }

    #[test]
    fn aligned_effect_keeps_positive_and_negative_overlap() {
        let grid = sparse_snapshot(&[(0, "12"), (10, "34"), (1, "13"), (9, "14")]);
        let inference = find_aligned_pair_exclusion(&grid).unwrap();
        let presentation = present(&grid, &inference).unwrap();
        let view = &presentation.views[0];
        let eliminated_pattern = view
            .candidate_marks
            .iter()
            .find(|mark| {
                mark.candidate
                    == CandidateRef {
                        cell: CellId::new(0).unwrap(),
                        digit: Digit::new(1).unwrap(),
                    }
            })
            .unwrap();

        assert!(eliminated_pattern.roles.contains(HighlightRoles::POSITIVE));
        assert!(eliminated_pattern.roles.contains(HighlightRoles::PATTERN));
        assert!(eliminated_pattern.roles.contains(HighlightRoles::NEGATIVE));
        assert!(
            eliminated_pattern
                .roles
                .contains(HighlightRoles::CONCLUSION)
        );
        assert_eq!(
            view.candidate_marks
                .iter()
                .filter(|mark| mark.candidate.cell.raw() == 0)
                .count(),
            2,
            "overlapping roles must not duplicate the same candidate"
        );
    }

    #[test]
    fn two_three_and_four_strong_links_preserve_ordered_semantics() {
        let two_grid =
            sparse_digit_snapshot(1, &[1, 9, 18, 36, 40, 4, 7], VariantConfig::default());
        let two = find_two_strong_links(&two_grid, EngineConfig::default()).unwrap();
        let Evidence::TwoStrongLinks {
            digit,
            pattern_cells,
            link_regions,
            endpoint_groups,
            bridge_region,
            ring_region,
            grouped_links,
            ..
        } = two.evidence()
        else {
            panic!("two-link fixture evidence");
        };
        assert_strong_link_view(
            &two_grid,
            &two,
            digit,
            pattern_cells,
            &link_regions,
            &endpoint_groups,
            &[bridge_region],
            ring_region,
            &grouped_links,
            &[0, 1],
        );

        let three_cells = [
            1, 2, 6, 11, 12, 15, 18, 22, 26, 27, 28, 29, 56, 57, 60, 63, 65, 67, 69, 71, 74, 76, 78,
        ];
        let three_grid = sparse_digit_snapshot(3, &three_cells, VariantConfig::default());
        let three = find_three_strong_links(&three_grid, EngineConfig::default()).unwrap();
        let Evidence::ThreeStrongLinks {
            digit,
            pattern_cells,
            link_regions,
            endpoint_groups,
            bridge_regions,
            ring_region,
            grouped_links,
            link_order,
            ..
        } = three.evidence()
        else {
            panic!("three-link fixture evidence");
        };
        assert_strong_link_view(
            &three_grid,
            &three,
            digit,
            pattern_cells,
            &link_regions,
            &endpoint_groups,
            &bridge_regions,
            ring_region,
            &grouped_links,
            &link_order,
        );

        let four_cells = [
            2, 5, 7, 8, 9, 15, 23, 25, 26, 27, 29, 34, 35, 37, 43, 49, 55, 57, 63, 65, 71, 72, 73,
            75, 78, 79, 80,
        ];
        let four_grid = sparse_digit_snapshot(
            9,
            &four_cells,
            VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            },
        );
        let four = find_four_strong_links(&four_grid, EngineConfig::default()).unwrap();
        let Evidence::FourStrongLinks {
            digit,
            pattern_cells,
            link_regions,
            endpoint_groups,
            bridge_regions,
            ring_region,
            grouped_links,
            link_order,
            ..
        } = four.evidence()
        else {
            panic!("four-link fixture evidence");
        };
        assert_strong_link_view(
            &four_grid,
            &four,
            digit,
            pattern_cells,
            &link_regions,
            &endpoint_groups,
            &bridge_regions,
            ring_region,
            &grouped_links,
            &link_order,
        );
    }

    #[test]
    fn revised_turbot_preserves_base_bridge_cover_region_order() {
        let grid = sparse_digit_snapshot(1, &[0, 3, 27, 31, 39], VariantConfig::default());
        let inference = find_two_strong_links(
            &grid,
            EngineConfig {
                rating_mode: RatingMode::Revised,
                ..EngineConfig::default()
            },
        )
        .unwrap();
        let Evidence::TwoStrongLinks {
            link_regions,
            bridge_region,
            ring_region,
            grouped_links,
            ..
        } = inference.evidence()
        else {
            panic!("revised Turbot evidence");
        };
        assert!(ring_region.is_none());
        assert_eq!(grouped_links, [false, false]);

        let presentation = present(&grid, &inference).unwrap();
        let view = &presentation.views[0];
        assert_eq!(
            view.region_marks
                .iter()
                .map(|mark| (mark.region, mark.roles))
                .collect::<Vec<_>>(),
            [
                (
                    link_regions[0],
                    HighlightRoles::PRIMARY | HighlightRoles::PATTERN,
                ),
                (
                    bridge_region,
                    HighlightRoles::SECONDARY | HighlightRoles::PATTERN,
                ),
                (
                    link_regions[1],
                    HighlightRoles::PRIMARY | HighlightRoles::PATTERN,
                ),
            ]
        );
        assert_eq!(
            view.links.iter().map(|link| link.kind).collect::<Vec<_>>(),
            [LinkKind::Strong, LinkKind::Strong, LinkKind::Weak]
        );
    }

    #[test]
    fn three_strong_links_ring_closes_after_all_strong_and_bridge_links() {
        let grid = sparse_digit_snapshot(1, &[0, 3, 12, 30, 33, 54, 60], VariantConfig::default());
        let inference = find_three_strong_links(&grid, EngineConfig::default()).unwrap();
        let Evidence::ThreeStrongLinks {
            digit,
            pattern_cells,
            link_regions,
            endpoint_groups,
            bridge_regions,
            ring_region,
            grouped_links,
            link_order,
            ..
        } = inference.evidence()
        else {
            panic!("three-link ring evidence");
        };
        assert!(ring_region.is_some());
        assert_strong_link_view(
            &grid,
            &inference,
            digit,
            pattern_cells,
            &link_regions,
            &endpoint_groups,
            &bridge_regions,
            ring_region,
            &grouped_links,
            &link_order,
        );
    }

    #[test]
    fn selected_forcing_chain_maps_target_first_links_and_causes() {
        let grid = sparse_snapshot(&[(0, "19"), (3, "12"), (30, "234"), (27, "29")]);
        let detailed = find_forcing_chain_cycle_with_proof(&grid, EngineConfig::default()).unwrap();
        let (inference, proof) = detailed.into_parts();
        let presentation = present_with_selected_chain_proof(&grid, &inference, &proof).unwrap();

        assert_eq!(presentation.views.len(), 1);
        let view = &presentation.views[0];
        assert_eq!(
            (view.key.as_str(), view.label.as_str()),
            ("forcing", "Forcing chain")
        );
        assert_eq!(
            view.links
                .iter()
                .map(|link| (link.kind, link.cause))
                .collect::<Vec<_>>(),
            [
                (
                    LinkKind::Weak,
                    LinkCause::Region(RegionId::new(1, 0).unwrap())
                ),
                (LinkKind::Strong, LinkCause::Cell),
                (
                    LinkKind::Weak,
                    LinkCause::Region(RegionId::new(2, 3).unwrap())
                ),
                (
                    LinkKind::Strong,
                    LinkCause::Region(RegionId::new(1, 3).unwrap())
                ),
                (LinkKind::Weak, LinkCause::Cell),
                (
                    LinkKind::Strong,
                    LinkCause::Region(RegionId::new(2, 0).unwrap())
                ),
                (LinkKind::Weak, LinkCause::Cell),
            ]
        );
        assert!(view.links.iter().all(|link| link.directed));
    }

    #[test]
    fn selected_cycle_maps_forward_then_reverse_views_without_reordering_edges() {
        let grid = sparse_snapshot(&[(0, "12"), (3, "13"), (30, "14"), (27, "15"), (6, "16")]);
        let detailed = find_forcing_chain_cycle_with_proof(&grid, EngineConfig::default()).unwrap();
        let (inference, proof) = detailed.into_parts();
        let presentation = present_with_selected_chain_proof(&grid, &inference, &proof).unwrap();

        assert_eq!(
            presentation
                .views
                .iter()
                .map(|view| (view.key.as_str(), view.label.as_str()))
                .collect::<Vec<_>>(),
            [
                ("cycle-forward", "Cycle forward"),
                ("cycle-reverse", "Cycle reverse"),
            ]
        );
        assert_eq!(
            presentation.views[0]
                .links
                .iter()
                .map(|link| (link.kind, link.cause))
                .collect::<Vec<_>>(),
            [
                (
                    LinkKind::Strong,
                    LinkCause::Region(RegionId::new(2, 0).unwrap())
                ),
                (
                    LinkKind::Weak,
                    LinkCause::Region(RegionId::new(1, 3).unwrap())
                ),
                (
                    LinkKind::Strong,
                    LinkCause::Region(RegionId::new(2, 3).unwrap())
                ),
                (
                    LinkKind::Weak,
                    LinkCause::Region(RegionId::new(1, 0).unwrap())
                ),
            ]
        );
        assert_eq!(
            presentation.views[1]
                .links
                .iter()
                .map(|link| (link.kind, link.cause))
                .collect::<Vec<_>>(),
            [
                (
                    LinkKind::Weak,
                    LinkCause::Region(RegionId::new(1, 0).unwrap())
                ),
                (
                    LinkKind::Strong,
                    LinkCause::Region(RegionId::new(2, 3).unwrap())
                ),
                (
                    LinkKind::Weak,
                    LinkCause::Region(RegionId::new(1, 3).unwrap())
                ),
                (
                    LinkKind::Strong,
                    LinkCause::Region(RegionId::new(2, 0).unwrap())
                ),
            ]
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn assert_strong_link_view(
        grid: &Grid,
        inference: &Inference,
        digit: Digit,
        pattern_cells: CellSequence,
        link_regions: &[RegionId],
        endpoint_groups: &[CellSequence],
        bridge_regions: &[RegionId],
        ring_region: Option<RegionId>,
        grouped_links: &[bool],
        link_order: &[u8],
    ) {
        let presentation = present(grid, inference).unwrap();
        let view = &presentation.views[0];
        let representatives = pattern_cells.iter().collect::<Vec<_>>();
        let strong_count = link_regions.len();
        let weak_count = bridge_regions.len() + usize::from(ring_region.is_some());

        assert_eq!(view.key, "main");
        assert_eq!(view.label, "View 1");
        assert_eq!(view.links.len(), strong_count + weak_count);

        for displayed_index in 0..strong_count {
            let base_index = usize::from(link_order[displayed_index]);
            let link = view.links[displayed_index];
            let from = CandidateRef {
                cell: representatives[displayed_index * 2],
                digit,
            };
            let to = CandidateRef {
                cell: representatives[displayed_index * 2 + 1],
                digit,
            };
            let groups = &endpoint_groups[base_index * 2..base_index * 2 + 2];
            assert_strong_endpoint(link.from, from, groups);
            assert_strong_endpoint(link.to, to, groups);
            assert_eq!(
                link.kind,
                if grouped_links[base_index] {
                    LinkKind::GroupedStrong
                } else {
                    LinkKind::Strong
                }
            );
            assert_eq!(link.cause, LinkCause::Region(link_regions[base_index]));
            assert!(link.directed);
        }

        for (bridge_index, &region) in bridge_regions.iter().enumerate() {
            let link = view.links[strong_count + bridge_index];
            assert_eq!(link.kind, LinkKind::Weak);
            assert_eq!(link.cause, LinkCause::Region(region));
            assert_eq!(
                link.from,
                LinkEndpoint::Candidate(CandidateRef {
                    cell: representatives[bridge_index * 2 + 1],
                    digit,
                })
            );
            assert_eq!(
                link.to,
                LinkEndpoint::Candidate(CandidateRef {
                    cell: representatives[bridge_index * 2 + 2],
                    digit,
                })
            );
            assert!(link.directed);
        }

        if let Some(region) = ring_region {
            let link = view.links.last().copied().unwrap();
            assert_eq!(link.kind, LinkKind::Weak);
            assert_eq!(link.cause, LinkCause::Region(region));
            assert_eq!(
                link.from,
                LinkEndpoint::Candidate(CandidateRef {
                    cell: *representatives.last().unwrap(),
                    digit,
                })
            );
            assert_eq!(
                link.to,
                LinkEndpoint::Candidate(CandidateRef {
                    cell: representatives[0],
                    digit,
                })
            );
        }

        let has_group_endpoint = view.links.iter().any(|link| {
            matches!(link.from, LinkEndpoint::CandidateGroup { .. })
                || matches!(link.to, LinkEndpoint::CandidateGroup { .. })
        });
        assert_eq!(
            has_group_endpoint,
            grouped_links.iter().any(|&grouped| grouped),
            "endpoint shape matches retained grouped-link metadata"
        );
    }

    fn assert_strong_endpoint(
        actual: LinkEndpoint,
        representative: CandidateRef,
        groups: &[CellSequence],
    ) {
        let members = groups
            .iter()
            .copied()
            .find(|group| group.iter().any(|cell| cell == representative.cell))
            .expect("representative belongs to a retained endpoint group");
        if members.len() > 1 {
            assert_eq!(
                actual,
                LinkEndpoint::CandidateGroup {
                    representative,
                    members,
                }
            );
        } else {
            assert_eq!(actual, LinkEndpoint::Candidate(representative));
        }
    }
}
