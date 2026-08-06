//! Ordered, semantic presentation data for graphical Sukaku Forge clients.
//!
//! The types in this crate deliberately describe meaning rather than paint.
//! A frontend theme decides how a selected region, a positive candidate, or
//! an elimination is rendered. Producer order is retained in every `Vec`.

pub mod wire;

use sukaku_forge_core::{CandidateMask, CellId, Digit, Grid, PositionMask, RegionId};
use sukaku_forge_engine::{
    BugKind, CellSequence, ChainCause, ChainProofView, ChainProofViewKind, ChainState, Evidence,
    Inference, NonConsecutiveGeometry, NonConsecutiveHintKind, Rating, RatingMode,
    SelectedChainProof, Technique, UniqueLoopKind,
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
        Evidence::NonConsecutive { kind, .. } => {
            add_non_consecutive(&mut view, pre_grid, kind);
            add_removals(&mut view, inference);
        }
        Evidence::GeneralizedIntersections {
            region,
            digit,
            locked_positions,
        } => {
            add_region(
                &mut view,
                region,
                HighlightRoles::PRIMARY | HighlightRoles::PATTERN,
            );
            for position in locked_positions.iter() {
                let cell = region_cell(pre_grid, region, position);
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
            }
            add_removals(&mut view, inference);
        }
        Evidence::AlphabetWing {
            pattern_cells,
            wing_set,
            ..
        } => {
            add_alphabet_wing(&mut view, pre_grid, pattern_cells, wing_set);
            add_removals(&mut view, inference);
        }
        Evidence::UniqueLoop {
            loop_cells,
            first_digit,
            second_digit,
            kind,
        } => {
            add_unique_loop(
                &mut view,
                pre_grid,
                loop_cells,
                [first_digit, second_digit],
                kind,
            );
            add_removals(&mut view, inference);
        }
        Evidence::Bug { kind } => {
            add_bug(&mut view, pre_grid, kind);
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

/// Present a selected chaining inference with its ordered outer proof
/// materialized by the engine's opt-in GUI search.
///
/// Compact chain evidence deliberately cannot recreate these views; callers
/// should use [`present`] when no selected proof accompanies an inference.
pub fn present_with_selected_chain_proof(
    pre_grid: &Grid,
    inference: &Inference,
    selected_proof: &SelectedChainProof,
) -> Result<HintPresentation, UnsupportedPresentation> {
    if !matches!(
        inference.evidence(),
        Evidence::ForcingChainCycle { .. }
            | Evidence::NishioForcingChain { .. }
            | Evidence::MultipleForcingChain { .. }
    ) {
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
        ChainProofViewKind::Forcing => ("forcing".to_owned(), "Forcing chain".to_owned()),
        ChainProofViewKind::CycleForward => {
            ("cycle-forward".to_owned(), "Cycle forward".to_owned())
        }
        ChainProofViewKind::CycleReverse => {
            ("cycle-reverse".to_owned(), "Cycle reverse".to_owned())
        }
        ChainProofViewKind::NishioOn => (
            "nishio-on".to_owned(),
            "Contradiction: target true".to_owned(),
        ),
        ChainProofViewKind::NishioOff => (
            "nishio-off".to_owned(),
            "Contradiction: target false".to_owned(),
        ),
        ChainProofViewKind::CellBranch { branch } => (
            format!("cell-branch-{branch}"),
            format!("Cell branch {}", u16::from(branch) + 1),
        ),
        ChainProofViewKind::RegionBranch { branch } => (
            format!("region-branch-{branch}"),
            format!("Region branch {}", u16::from(branch) + 1),
        ),
        ChainProofViewKind::AssumptionOn => {
            ("assumption-on".to_owned(), "Assumption true".to_owned())
        }
        ChainProofViewKind::AssumptionOff => {
            ("assumption-off".to_owned(), "Assumption false".to_owned())
        }
        ChainProofViewKind::ContradictionOn => (
            "contradiction-on".to_owned(),
            "Contradiction: target true".to_owned(),
        ),
        ChainProofViewKind::ContradictionOff => (
            "contradiction-off".to_owned(),
            "Contradiction: target false".to_owned(),
        ),
    };
    let mut view = HintView {
        key,
        label,
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
            let (kind, cause) = match parent_edge.cause() {
                ChainCause::None | ChainCause::Derived => {
                    (LinkKind::Implication, LinkCause::Derived)
                }
                ChainCause::Cell => (
                    match node.state() {
                        ChainState::On => LinkKind::Strong,
                        ChainState::Off => LinkKind::Weak,
                    },
                    LinkCause::Cell,
                ),
                ChainCause::Region(region) => (
                    match node.state() {
                        ChainState::On => LinkKind::Strong,
                        ChainState::Off => LinkKind::Weak,
                    },
                    LinkCause::Region(region),
                ),
                ChainCause::Visibility => (
                    match node.state() {
                        ChainState::On => LinkKind::Strong,
                        ChainState::Off => LinkKind::Weak,
                    },
                    LinkCause::Visibility,
                ),
            };
            view.links.push(CandidateLink {
                from: LinkEndpoint::Candidate(CandidateRef {
                    cell: parent.cell(),
                    digit: parent.digit(),
                }),
                to: LinkEndpoint::Candidate(CandidateRef {
                    cell: node.cell(),
                    digit: node.digit(),
                }),
                kind,
                cause,
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

fn add_non_consecutive(view: &mut HintView, grid: &Grid, kind: NonConsecutiveHintKind) {
    match kind {
        NonConsecutiveHintKind::ForcingCell { cell, .. } => {
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
        NonConsecutiveHintKind::Locked {
            cells,
            region,
            digit,
            ..
        } => {
            add_region(
                view,
                region,
                HighlightRoles::PRIMARY | HighlightRoles::PATTERN,
            );
            for cell in cells.iter() {
                add_cell(
                    view,
                    cell,
                    HighlightRoles::SELECTED | HighlightRoles::PATTERN,
                );
                add_candidate(
                    view,
                    CandidateRef { cell, digit },
                    HighlightRoles::POSITIVE | HighlightRoles::PATTERN,
                );
            }
        }
    }
}

fn add_alphabet_wing(
    view: &mut HintView,
    grid: &Grid,
    pattern_cells: CellSequence,
    wing_set: CandidateMask,
) {
    let last_index = pattern_cells.len().saturating_sub(1);
    for (index, cell) in pattern_cells.iter().enumerate() {
        let side = if index == last_index {
            HighlightRoles::SECONDARY
        } else {
            HighlightRoles::PRIMARY
        };
        add_cell(
            view,
            cell,
            HighlightRoles::SELECTED | HighlightRoles::PATTERN | side,
        );
        for digit in grid.candidates(cell).intersect(wing_set).iter() {
            add_candidate(
                view,
                CandidateRef { cell, digit },
                HighlightRoles::POSITIVE | HighlightRoles::PATTERN | side,
            );
        }
    }
}

fn add_unique_loop(
    view: &mut HintView,
    grid: &Grid,
    loop_cells: CellSequence,
    loop_digits: [Digit; 2],
    kind: UniqueLoopKind,
) {
    let loop_values = CandidateMask::of(loop_digits[0]).union(CandidateMask::of(loop_digits[1]));
    for cell in loop_cells.iter() {
        add_cell(
            view,
            cell,
            HighlightRoles::SELECTED | HighlightRoles::PATTERN,
        );
        add_cell_candidates(
            view,
            grid,
            cell,
            loop_values,
            HighlightRoles::POSITIVE | HighlightRoles::PATTERN,
        );
    }

    match kind {
        UniqueLoopKind::Type1 { rescue } => {
            add_auxiliary_candidates(
                view,
                grid,
                rescue,
                grid.candidates(rescue).without(loop_values),
            );
        }
        UniqueLoopKind::Type2 { extra_cells, digit } => {
            for cell in extra_cells.iter() {
                add_auxiliary_candidates(view, grid, cell, CandidateMask::of(digit));
            }
        }
        UniqueLoopKind::Type3Naked {
            rescue_cells,
            region,
            extra_values,
            set_cells,
            set_values,
        } => {
            add_region(
                view,
                region,
                HighlightRoles::PRIMARY | HighlightRoles::PATTERN,
            );
            for cell in rescue_cells {
                add_auxiliary_candidates(view, grid, cell, extra_values);
            }
            for cell in set_cells.iter() {
                add_auxiliary_candidates(view, grid, cell, set_values);
            }
        }
        UniqueLoopKind::Type3Hidden {
            rescue_cells,
            region,
            extra_values,
            hidden_positions,
            hidden_values,
        } => {
            add_region(
                view,
                region,
                HighlightRoles::PRIMARY | HighlightRoles::PATTERN,
            );
            for cell in rescue_cells {
                add_auxiliary_candidates(view, grid, cell, extra_values);
            }
            for position in hidden_positions.iter() {
                let cell = region_cell(grid, region, position);
                add_auxiliary_candidates(view, grid, cell, hidden_values);
            }
        }
        UniqueLoopKind::Type4 {
            rescue_cells,
            region,
            lock_digit,
            ..
        } => {
            add_region(
                view,
                region,
                HighlightRoles::PRIMARY | HighlightRoles::PATTERN,
            );
            for cell in rescue_cells {
                add_auxiliary_candidates(view, grid, cell, CandidateMask::of(lock_digit));
            }
        }
    }
}

fn add_bug(view: &mut HintView, grid: &Grid, kind: BugKind) {
    match kind {
        BugKind::Type1 { cell, extra_values } => {
            add_cell(
                view,
                cell,
                HighlightRoles::SELECTED | HighlightRoles::PATTERN,
            );
            add_cell_candidates(
                view,
                grid,
                cell,
                extra_values,
                HighlightRoles::POSITIVE | HighlightRoles::PATTERN,
            );
        }
        BugKind::Type2 {
            bug_cells,
            digit: _,
        } => {
            for (cell, values) in bug_cells.iter_with_values() {
                add_cell(
                    view,
                    cell,
                    HighlightRoles::SELECTED | HighlightRoles::PATTERN,
                );
                add_cell_candidates(
                    view,
                    grid,
                    cell,
                    values,
                    HighlightRoles::POSITIVE | HighlightRoles::PATTERN,
                );
            }
        }
        BugKind::Type3 {
            bug_cells,
            set_cells,
            region,
            set_values,
            ..
        } => {
            add_region(
                view,
                region,
                HighlightRoles::PRIMARY | HighlightRoles::PATTERN,
            );
            for (cell, values) in bug_cells.iter_with_values() {
                add_cell(
                    view,
                    cell,
                    HighlightRoles::SELECTED | HighlightRoles::PATTERN,
                );
                add_cell_candidates(
                    view,
                    grid,
                    cell,
                    values,
                    HighlightRoles::POSITIVE | HighlightRoles::PATTERN,
                );
            }
            for cell in set_cells.iter() {
                add_auxiliary_candidates(view, grid, cell, set_values);
            }
        }
        BugKind::Type4 {
            bug_cells,
            extra_values,
            region,
            locked_digit,
            ..
        } => {
            add_region(
                view,
                region,
                HighlightRoles::PRIMARY | HighlightRoles::PATTERN,
            );
            for (index, cell) in bug_cells.into_iter().enumerate() {
                add_cell(
                    view,
                    cell,
                    HighlightRoles::SELECTED | HighlightRoles::PATTERN,
                );
                add_cell_candidates(
                    view,
                    grid,
                    cell,
                    extra_values[index].union(CandidateMask::of(locked_digit)),
                    HighlightRoles::POSITIVE | HighlightRoles::PATTERN,
                );
            }
        }
    }
}

fn add_auxiliary_candidates(view: &mut HintView, grid: &Grid, cell: CellId, digits: CandidateMask) {
    add_cell(
        view,
        cell,
        HighlightRoles::AUXILIARY | HighlightRoles::PATTERN,
    );
    add_cell_candidates(
        view,
        grid,
        cell,
        digits,
        HighlightRoles::AUXILIARY | HighlightRoles::POSITIVE | HighlightRoles::PATTERN,
    );
}

fn add_cell_candidates(
    view: &mut HintView,
    grid: &Grid,
    cell: CellId,
    digits: CandidateMask,
    roles: HighlightRoles,
) {
    for digit in grid.candidates(cell).intersect(digits).iter() {
        add_candidate(view, CandidateRef { cell, digit }, roles);
    }
}

fn explanation(pre_grid: &Grid, inference: &Inference) -> ExplanationDoc {
    if let Some(mut blocks) = evidence_explanation(pre_grid, inference) {
        blocks.push(effect_conclusion(inference));
        return ExplanationDoc { blocks };
    }

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

fn evidence_explanation(pre_grid: &Grid, inference: &Inference) -> Option<Vec<ExplanationBlock>> {
    let technique = inference.technique();
    match inference.evidence() {
        Evidence::NonConsecutive { geometry, kind } => {
            let geometry = match geometry {
                NonConsecutiveGeometry::Orthogonal => "orthogonal",
                NonConsecutiveGeometry::Ferz => "diagonal",
            };
            let mut blocks = vec![ExplanationBlock::Paragraph(vec![
                ExplanationInline::Technique(technique),
                ExplanationInline::Text(format!(" uses the {geometry} non-consecutive rule.")),
            ])];
            let mut detail = Vec::new();
            match kind {
                NonConsecutiveHintKind::ForcingCell { cell, values } => {
                    detail.push(ExplanationInline::Cell(cell));
                    detail.push(ExplanationInline::Text(
                        " forces affected cells to exclude ".to_owned(),
                    ));
                    append_digits(&mut detail, values.iter());
                }
                NonConsecutiveHintKind::Locked {
                    cells,
                    values,
                    region,
                    digit,
                } => {
                    detail.push(ExplanationInline::Digit(digit));
                    detail.push(ExplanationInline::Text(" is confined in ".to_owned()));
                    detail.push(ExplanationInline::Region(region));
                    detail.push(ExplanationInline::Text(" to ".to_owned()));
                    append_cells(&mut detail, cells.iter());
                    detail.push(ExplanationInline::Text(
                        ", so affected cells must exclude ".to_owned(),
                    ));
                    append_digits(&mut detail, values.iter());
                }
            }
            detail.push(ExplanationInline::Text(".".to_owned()));
            blocks.push(ExplanationBlock::Paragraph(detail));
            Some(blocks)
        }
        Evidence::GeneralizedIntersections {
            region,
            digit,
            locked_positions,
        } => {
            let mut pattern = vec![
                ExplanationInline::Technique(technique),
                ExplanationInline::Text(" confines ".to_owned()),
                ExplanationInline::Digit(digit),
                ExplanationInline::Text(" in ".to_owned()),
                ExplanationInline::Region(region),
                ExplanationInline::Text(" to ".to_owned()),
            ];
            append_candidates(
                &mut pattern,
                locked_positions.iter().map(|position| CandidateRef {
                    cell: region_cell(pre_grid, region, position),
                    digit,
                }),
            );
            pattern.push(ExplanationInline::Text(".".to_owned()));
            Some(vec![
                ExplanationBlock::Paragraph(pattern),
                ExplanationBlock::Paragraph(vec![ExplanationInline::Text(
                    "Each conclusion candidate sees every candidate in the confined set."
                        .to_owned(),
                )]),
            ])
        }
        Evidence::AlphabetWing {
            pattern_cells,
            x_digit,
            z_digit,
            double_link,
            wing_set,
            ..
        } => {
            let mut pattern = vec![
                ExplanationInline::Technique(technique),
                ExplanationInline::Text(" uses the ordered wing cells ".to_owned()),
            ];
            append_cells(&mut pattern, pattern_cells.iter());
            pattern.push(ExplanationInline::Text(" with wing set ".to_owned()));
            append_digits(&mut pattern, wing_set.iter());
            pattern.push(ExplanationInline::Text(".".to_owned()));

            let mut link = vec![ExplanationInline::Text(if double_link {
                "The two linked digits are ".to_owned()
            } else {
                "The linked elimination digit is ".to_owned()
            })];
            if double_link {
                append_digits(&mut link, [x_digit, z_digit]);
            } else {
                link.push(ExplanationInline::Digit(z_digit));
            }
            link.push(ExplanationInline::Text(".".to_owned()));
            Some(vec![
                ExplanationBlock::Paragraph(pattern),
                ExplanationBlock::Paragraph(link),
            ])
        }
        Evidence::UniqueLoop {
            loop_cells,
            first_digit,
            second_digit,
            kind,
        } => {
            let mut loop_block = vec![
                ExplanationInline::Technique(technique),
                ExplanationInline::Text(" forms the ordered loop ".to_owned()),
            ];
            append_cells(&mut loop_block, loop_cells.iter());
            loop_block.push(ExplanationInline::Text(" on ".to_owned()));
            append_digits(&mut loop_block, [first_digit, second_digit]);
            loop_block.push(ExplanationInline::Text(".".to_owned()));

            let mut detail = Vec::new();
            match kind {
                UniqueLoopKind::Type1 { rescue } => {
                    detail.extend([
                        ExplanationInline::Cell(rescue),
                        ExplanationInline::Text(" is the sole rescue cell.".to_owned()),
                    ]);
                }
                UniqueLoopKind::Type2 { extra_cells, digit } => {
                    append_cells(&mut detail, extra_cells.iter());
                    detail.push(ExplanationInline::Text(
                        " share extra candidate ".to_owned(),
                    ));
                    detail.push(ExplanationInline::Digit(digit));
                    detail.push(ExplanationInline::Text(".".to_owned()));
                }
                UniqueLoopKind::Type3Naked {
                    rescue_cells,
                    region,
                    extra_values,
                    set_cells,
                    set_values,
                } => {
                    detail.push(ExplanationInline::Region(region));
                    detail.push(ExplanationInline::Text(
                        " combines rescue cells ".to_owned(),
                    ));
                    append_cells(&mut detail, rescue_cells);
                    detail.push(ExplanationInline::Text(" with helper cells ".to_owned()));
                    append_cells(&mut detail, set_cells.iter());
                    detail.push(ExplanationInline::Text("; rescue values ".to_owned()));
                    append_digits(&mut detail, extra_values.iter());
                    detail.push(ExplanationInline::Text(" form set ".to_owned()));
                    append_digits(&mut detail, set_values.iter());
                    detail.push(ExplanationInline::Text(".".to_owned()));
                }
                UniqueLoopKind::Type3Hidden {
                    rescue_cells,
                    region,
                    extra_values,
                    hidden_positions,
                    hidden_values,
                } => {
                    detail.push(ExplanationInline::Region(region));
                    detail.push(ExplanationInline::Text(
                        " combines rescue cells ".to_owned(),
                    ));
                    append_cells(&mut detail, rescue_cells);
                    detail.push(ExplanationInline::Text(
                        " with hidden-set cells ".to_owned(),
                    ));
                    append_cells(
                        &mut detail,
                        hidden_positions
                            .iter()
                            .map(|position| region_cell(pre_grid, region, position)),
                    );
                    detail.push(ExplanationInline::Text(" on extra values ".to_owned()));
                    append_digits(&mut detail, extra_values.iter());
                    detail.push(ExplanationInline::Text(" and hidden values ".to_owned()));
                    append_digits(&mut detail, hidden_values.iter());
                    detail.push(ExplanationInline::Text(".".to_owned()));
                }
                UniqueLoopKind::Type4 {
                    rescue_cells,
                    region,
                    lock_digit,
                    remove_digit,
                } => {
                    detail.push(ExplanationInline::Region(region));
                    detail.push(ExplanationInline::Text(" locks ".to_owned()));
                    detail.push(ExplanationInline::Digit(lock_digit));
                    detail.push(ExplanationInline::Text(" in ".to_owned()));
                    append_cells(&mut detail, rescue_cells);
                    detail.push(ExplanationInline::Text(
                        ", forcing the removal of ".to_owned(),
                    ));
                    detail.push(ExplanationInline::Digit(remove_digit));
                    detail.push(ExplanationInline::Text(".".to_owned()));
                }
            }
            Some(vec![
                ExplanationBlock::Paragraph(loop_block),
                ExplanationBlock::Paragraph(detail),
            ])
        }
        Evidence::Bug { kind } => {
            let mut detail = vec![ExplanationInline::Technique(technique)];
            match kind {
                BugKind::Type1 { cell, extra_values } => {
                    detail.push(ExplanationInline::Text(" leaves ".to_owned()));
                    detail.push(ExplanationInline::Cell(cell));
                    detail.push(ExplanationInline::Text(
                        " with the BUG value set ".to_owned(),
                    ));
                    append_digits(&mut detail, extra_values.iter());
                }
                BugKind::Type2 { bug_cells, digit } => {
                    detail.push(ExplanationInline::Text(" uses BUG cells ".to_owned()));
                    append_cells(&mut detail, bug_cells.iter());
                    detail.push(ExplanationInline::Text(" on extra digit ".to_owned()));
                    detail.push(ExplanationInline::Digit(digit));
                }
                BugKind::Type3 {
                    bug_cells,
                    set_cells,
                    region,
                    set_values,
                    generalized,
                    ..
                } => {
                    detail.push(ExplanationInline::Text(" combines BUG cells ".to_owned()));
                    append_cells(&mut detail, bug_cells.iter());
                    detail.push(ExplanationInline::Text(" and helper cells ".to_owned()));
                    append_cells(&mut detail, set_cells.iter());
                    detail.push(ExplanationInline::Text(" in ".to_owned()));
                    detail.push(ExplanationInline::Region(region));
                    detail.push(ExplanationInline::Text(" as set ".to_owned()));
                    append_digits(&mut detail, set_values.iter());
                    if generalized {
                        detail.push(ExplanationInline::Text(
                            " using generalized common visibility".to_owned(),
                        ));
                    }
                }
                BugKind::Type4 {
                    bug_cells,
                    region,
                    locked_digit,
                    all_extra_values,
                    ..
                } => {
                    detail.push(ExplanationInline::Text(" locks ".to_owned()));
                    detail.push(ExplanationInline::Digit(locked_digit));
                    detail.push(ExplanationInline::Text(" between ".to_owned()));
                    append_cells(&mut detail, bug_cells);
                    detail.push(ExplanationInline::Text(" in ".to_owned()));
                    detail.push(ExplanationInline::Region(region));
                    detail.push(ExplanationInline::Text(" with extra values ".to_owned()));
                    append_digits(&mut detail, all_extra_values.iter());
                }
            }
            detail.push(ExplanationInline::Text(".".to_owned()));
            Some(vec![ExplanationBlock::Paragraph(detail)])
        }
        _ => None,
    }
}

fn effect_conclusion(inference: &Inference) -> ExplanationBlock {
    if let (Some(cell), Some(digit)) = (inference.placement_cell(), inference.placement_digit()) {
        return ExplanationBlock::Paragraph(vec![
            ExplanationInline::Text("Therefore ".to_owned()),
            ExplanationInline::Cell(cell),
            ExplanationInline::Text(" contains ".to_owned()),
            ExplanationInline::Digit(digit),
            ExplanationInline::Text(".".to_owned()),
        ]);
    }

    let mut conclusion = vec![ExplanationInline::Text("Therefore remove ".to_owned())];
    append_candidates(
        &mut conclusion,
        inference.removals().iter().flat_map(|removal| {
            removal.digits().iter().map(move |digit| CandidateRef {
                cell: removal.cell(),
                digit,
            })
        }),
    );
    conclusion.push(ExplanationInline::Text(".".to_owned()));
    ExplanationBlock::Paragraph(conclusion)
}

fn append_cells(inlines: &mut Vec<ExplanationInline>, cells: impl IntoIterator<Item = CellId>) {
    append_joined(inlines, cells, ExplanationInline::Cell);
}

fn append_digits(inlines: &mut Vec<ExplanationInline>, digits: impl IntoIterator<Item = Digit>) {
    append_joined(inlines, digits, ExplanationInline::Digit);
}

fn append_candidates(
    inlines: &mut Vec<ExplanationInline>,
    candidates: impl IntoIterator<Item = CandidateRef>,
) {
    append_joined(inlines, candidates, ExplanationInline::Candidate);
}

fn append_joined<T>(
    inlines: &mut Vec<ExplanationInline>,
    values: impl IntoIterator<Item = T>,
    inline: impl Fn(T) -> ExplanationInline,
) {
    for (index, value) in values.into_iter().enumerate() {
        if index != 0 {
            inlines.push(ExplanationInline::Text(", ".to_owned()));
        }
        inlines.push(inline(value));
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
        CandidateMask, CellId, ConstraintTopology, Digit, Grid, NonConsecutiveMode, Puzzle,
        RegionId, VariantConfig,
    };
    use sukaku_forge_engine::{
        BugKind, CellSequence, EngineConfig, Evidence, Inference, RatingMode, SearchOutcome,
        TechniqueGate, TechniqueSet, UniqueLoopKind, find_aligned_pair_exclusion,
        find_alphabet_wing, find_bivalue_universal_grave,
        find_dynamic_forcing_chain_plus_with_proof_checked, find_forcing_chain_cycle_with_proof,
        find_four_strong_links, find_generalized_intersections,
        find_multiple_forcing_chain_with_proof, find_nested_forcing_chain_with_proof_checked,
        find_nishio_forcing_chain_with_proof, find_three_strong_links, find_two_strong_links,
        find_unique_loop, find_wing,
    };

    use super::{
        CandidateRef, ExplanationBlock, ExplanationInline, HighlightRoles, LinkCause, LinkEndpoint,
        LinkKind, present, present_with_selected_chain_proof,
    };

    fn sparse_snapshot(entries: &[(usize, &str)]) -> Grid {
        sparse_snapshot_with_config(VariantConfig::default(), entries)
    }

    fn sparse_snapshot_with_config(config: VariantConfig, entries: &[(usize, &str)]) -> Grid {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = ['.'; 729];
        for &(cell, candidates) in entries {
            for digit in candidates.bytes() {
                display[cell * 9 + usize::from(digit - b'1')] = char::from(digit);
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

    fn mask(text: &str) -> CandidateMask {
        text.bytes().fold(CandidateMask::EMPTY, |mut result, byte| {
            result.insert(Digit::new(byte - b'0').expect("fixture digit"));
            result
        })
    }

    const BUG_SOLUTION: &str =
        "534678912672195348198342567859761423426853791713924856961537284287419635345286179";

    fn bug_grid(overrides: &[(usize, &str)]) -> Grid {
        let puzzle = Puzzle::parse(&".".repeat(729)).expect("empty pencilmarks");
        let mut grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &puzzle,
        );
        for (index, byte) in BUG_SOLUTION.bytes().enumerate() {
            let value = byte - b'0';
            let next = if value == 9 { 1 } else { value + 1 };
            grid.set_candidates(
                CellId::new(index as u8).unwrap(),
                CandidateMask::of(Digit::new(value).unwrap())
                    .union(CandidateMask::of(Digit::new(next).unwrap())),
            );
        }
        for &(index, values) in overrides {
            grid.set_candidates(CellId::new(index as u8).unwrap(), mask(values));
        }
        grid
    }

    fn next_non_consecutive(grid: &Grid, locked: bool) -> Inference {
        let mut enabled = TechniqueSet::ALL
            .without(TechniqueGate::HiddenSingle)
            .without(TechniqueGate::DirectPointing)
            .without(TechniqueGate::DirectHiddenPair)
            .without(TechniqueGate::NakedSingle);
        if locked {
            enabled = enabled.without(TechniqueGate::ForcingCellNonConsecutive);
        }
        let solver = sukaku_forge_engine::Solver::new(EngineConfig {
            enabled_techniques: enabled,
            ..EngineConfig::default()
        });
        let SearchOutcome::Found(inference) = solver.next_inference(grid) else {
            panic!("non-consecutive fixture inference");
        };
        inference
    }

    fn candidate_mark_entries(view: &super::HintView) -> Vec<(u8, u8, u16)> {
        view.candidate_marks
            .iter()
            .map(|mark| {
                (
                    mark.candidate.cell.raw(),
                    mark.candidate.digit.get(),
                    mark.roles.bits(),
                )
            })
            .collect()
    }

    fn cell_mark_entries(view: &super::HintView) -> Vec<(u8, u16)> {
        view.cell_marks
            .iter()
            .map(|mark| (mark.cell.raw(), mark.roles.bits()))
            .collect()
    }

    fn conclusion_candidates(presentation: &super::HintPresentation) -> Vec<(u8, u8)> {
        let ExplanationBlock::Paragraph(inlines) = presentation.explanation.blocks.last().unwrap()
        else {
            panic!("conclusion paragraph");
        };
        inlines
            .iter()
            .filter_map(|inline| match inline {
                ExplanationInline::Candidate(candidate) => {
                    Some((candidate.cell.raw(), candidate.digit.get()))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn non_consecutive_views_preserve_source_and_effect_order() {
        let config = VariantConfig {
            non_consecutive: NonConsecutiveMode::Orthogonal,
            forbidden_pairs: true,
            ..VariantConfig::default()
        };
        let forcing_grid = sparse_snapshot_with_config(config, &[(0, "45"), (1, "45"), (9, "45")]);
        let forcing = next_non_consecutive(&forcing_grid, false);
        let forcing_presentation = present(&forcing_grid, &forcing).unwrap();
        let forcing_view = &forcing_presentation.views[0];
        let pattern = (HighlightRoles::SELECTED | HighlightRoles::PATTERN).bits();
        let positive = (HighlightRoles::POSITIVE | HighlightRoles::PATTERN).bits();
        let conclusion = (HighlightRoles::NEGATIVE | HighlightRoles::CONCLUSION).bits();

        assert_eq!(cell_mark_entries(forcing_view), [(0, pattern)]);
        assert!(forcing_view.region_marks.is_empty());
        assert!(forcing_view.links.is_empty());
        assert_eq!(
            candidate_mark_entries(forcing_view),
            [
                (0, 4, positive),
                (0, 5, positive),
                (1, 4, conclusion),
                (1, 5, conclusion),
                (9, 4, conclusion),
                (9, 5, conclusion),
            ]
        );
        assert_eq!(
            forcing_presentation.explanation.blocks[0],
            ExplanationBlock::Paragraph(vec![
                ExplanationInline::Technique(forcing.technique()),
                ExplanationInline::Text(" uses the orthogonal non-consecutive rule.".to_owned(),),
            ])
        );
        assert_eq!(
            conclusion_candidates(&forcing_presentation),
            [(1, 4), (1, 5), (9, 4), (9, 5)]
        );

        let locked_grid = sparse_snapshot_with_config(config, &[(0, "12"), (9, "12")]);
        let locked = next_non_consecutive(&locked_grid, true);
        let locked_presentation = present(&locked_grid, &locked).unwrap();
        let locked_view = &locked_presentation.views[0];

        assert_eq!(cell_mark_entries(locked_view), [(0, pattern), (9, pattern)]);
        assert_eq!(
            locked_view
                .region_marks
                .iter()
                .map(|mark| {
                    (
                        mark.region.type_index(),
                        mark.region.region_index(),
                        mark.roles.bits(),
                    )
                })
                .collect::<Vec<_>>(),
            [(
                2,
                0,
                (HighlightRoles::PRIMARY | HighlightRoles::PATTERN).bits(),
            )]
        );
        assert!(locked_view.links.is_empty());
        assert_eq!(
            candidate_mark_entries(locked_view),
            [
                (0, 1, positive),
                (9, 1, positive),
                (0, 2, conclusion),
                (9, 2, conclusion),
            ]
        );
        assert_eq!(
            conclusion_candidates(&locked_presentation),
            [(0, 2), (9, 2)]
        );

        let diagonal_grid = sparse_snapshot_with_config(
            VariantConfig {
                non_consecutive: NonConsecutiveMode::Diagonal,
                forbidden_pairs: true,
                ..VariantConfig::default()
            },
            &[(3, "45"), (11, "45"), (13, "45")],
        );
        let diagonal = next_non_consecutive(&diagonal_grid, false);
        let diagonal_presentation = present(&diagonal_grid, &diagonal).unwrap();
        assert_eq!(
            candidate_mark_entries(&diagonal_presentation.views[0]),
            [
                (3, 4, positive),
                (3, 5, positive),
                (13, 4, conclusion),
                (13, 5, conclusion),
            ]
        );
        assert_eq!(
            diagonal_presentation.explanation.blocks[0],
            ExplanationBlock::Paragraph(vec![
                ExplanationInline::Technique(diagonal.technique()),
                ExplanationInline::Text(" uses the diagonal non-consecutive rule.".to_owned()),
            ])
        );
        assert!(diagonal_presentation.views[0].links.is_empty());
    }

    #[test]
    fn generalized_intersections_uses_variant_region_order_without_links() {
        let grid = sparse_snapshot_with_config(
            VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            },
            &[(57, "1"), (67, "1"), (75, "1"), (74, "1")],
        );
        let inference = find_generalized_intersections(&grid).unwrap();
        let presentation = present(&grid, &inference).unwrap();
        let view = &presentation.views[0];
        let selected = (HighlightRoles::SELECTED | HighlightRoles::PATTERN).bits();
        let positive = (HighlightRoles::POSITIVE | HighlightRoles::PATTERN).bits();
        let conclusion = (HighlightRoles::NEGATIVE | HighlightRoles::CONCLUSION).bits();

        assert_eq!(
            view.region_marks
                .iter()
                .map(|mark| {
                    (
                        mark.region.type_index(),
                        mark.region.region_index(),
                        mark.roles.bits(),
                    )
                })
                .collect::<Vec<_>>(),
            [(
                0,
                7,
                (HighlightRoles::PRIMARY | HighlightRoles::PATTERN).bits(),
            )]
        );
        assert_eq!(
            cell_mark_entries(view),
            [(57, selected), (67, selected), (75, selected)]
        );
        assert_eq!(
            candidate_mark_entries(view),
            [
                (57, 1, positive),
                (67, 1, positive),
                (75, 1, positive),
                (74, 1, conclusion),
            ]
        );
        assert!(view.links.is_empty());
        assert_eq!(conclusion_candidates(&presentation), [(74, 1)]);
        assert!(matches!(
            &presentation.explanation.blocks[0],
            ExplanationBlock::Paragraph(inlines)
                if inlines.first() == Some(&ExplanationInline::Technique(inference.technique()))
        ));
    }

    #[test]
    fn alphabet_wing_marks_ordered_clique_and_yz_without_invented_links() {
        let grid = sparse_snapshot(&[(0, "12"), (1, "13"), (2, "2"), (3, "12"), (9, "24")]);
        let inference = find_alphabet_wing(&grid, 4).unwrap();
        let presentation = present(&grid, &inference).unwrap();
        let view = &presentation.views[0];
        let primary_cell =
            (HighlightRoles::SELECTED | HighlightRoles::PATTERN | HighlightRoles::PRIMARY).bits();
        let secondary_cell =
            (HighlightRoles::SELECTED | HighlightRoles::PATTERN | HighlightRoles::SECONDARY).bits();
        let primary_candidate =
            (HighlightRoles::POSITIVE | HighlightRoles::PATTERN | HighlightRoles::PRIMARY).bits();
        let secondary_candidate =
            (HighlightRoles::POSITIVE | HighlightRoles::PATTERN | HighlightRoles::SECONDARY).bits();
        let conclusion = (HighlightRoles::NEGATIVE | HighlightRoles::CONCLUSION).bits();

        assert_eq!(
            cell_mark_entries(view),
            [
                (0, primary_cell),
                (1, primary_cell),
                (9, primary_cell),
                (3, secondary_cell),
            ]
        );
        assert_eq!(
            candidate_mark_entries(view),
            [
                (0, 1, primary_candidate),
                (0, 2, primary_candidate),
                (1, 1, primary_candidate),
                (1, 3, primary_candidate),
                (9, 2, primary_candidate),
                (9, 4, primary_candidate),
                (3, 1, secondary_candidate),
                (3, 2, secondary_candidate),
                (2, 2, conclusion),
            ]
        );
        assert!(view.region_marks.is_empty());
        assert!(view.links.is_empty());
        assert_eq!(conclusion_candidates(&presentation), [(2, 2)]);
        assert!(matches!(
            &presentation.explanation.blocks[0],
            ExplanationBlock::Paragraph(inlines)
                if inlines.first() == Some(&ExplanationInline::Technique(inference.technique()))
        ));
    }

    #[test]
    fn unique_loop_type_1_preserves_loop_order_and_rescue_role_overlap() {
        let grid = sparse_snapshot(&[(0, "12"), (3, "12"), (9, "12"), (12, "123")]);
        let inference = find_unique_loop(&grid, EngineConfig::default()).unwrap();
        assert!(matches!(
            inference.evidence(),
            Evidence::UniqueLoop {
                kind: UniqueLoopKind::Type1 { .. },
                ..
            }
        ));
        let presentation = present(&grid, &inference).unwrap();
        let view = &presentation.views[0];
        let selected = (HighlightRoles::SELECTED | HighlightRoles::PATTERN).bits();
        let rescue_cell =
            (HighlightRoles::SELECTED | HighlightRoles::PATTERN | HighlightRoles::AUXILIARY).bits();
        let positive = (HighlightRoles::POSITIVE | HighlightRoles::PATTERN).bits();
        let eliminated_pattern = (HighlightRoles::POSITIVE
            | HighlightRoles::PATTERN
            | HighlightRoles::NEGATIVE
            | HighlightRoles::CONCLUSION)
            .bits();
        let auxiliary =
            (HighlightRoles::AUXILIARY | HighlightRoles::POSITIVE | HighlightRoles::PATTERN).bits();

        assert_eq!(
            cell_mark_entries(view),
            [
                (0, selected),
                (9, selected),
                (12, rescue_cell),
                (3, selected),
            ]
        );
        assert_eq!(
            candidate_mark_entries(view),
            [
                (0, 1, positive),
                (0, 2, positive),
                (9, 1, positive),
                (9, 2, positive),
                (12, 1, eliminated_pattern),
                (12, 2, eliminated_pattern),
                (3, 1, positive),
                (3, 2, positive),
                (12, 3, auxiliary),
            ]
        );
        assert!(view.region_marks.is_empty());
        assert!(view.links.is_empty());
        assert_eq!(conclusion_candidates(&presentation), [(12, 1), (12, 2)]);
    }

    #[test]
    fn unique_loop_remaining_subtypes_retain_only_explicit_auxiliary_evidence() {
        let type_2_grid =
            sparse_snapshot(&[(0, "12"), (3, "12"), (9, "123"), (12, "123"), (10, "123")]);
        let type_2 = find_unique_loop(&type_2_grid, EngineConfig::default()).unwrap();
        assert!(matches!(
            type_2.evidence(),
            Evidence::UniqueLoop {
                kind: UniqueLoopKind::Type2 { .. },
                ..
            }
        ));
        let type_2_presentation = present(&type_2_grid, &type_2).unwrap();
        let type_2_view = &type_2_presentation.views[0];
        assert!(type_2_view.region_marks.is_empty());
        assert!(type_2_view.links.is_empty());
        assert_eq!(conclusion_candidates(&type_2_presentation), [(10, 3)]);
        assert_eq!(
            cell_mark_entries(type_2_view),
            [
                (
                    0,
                    (HighlightRoles::SELECTED | HighlightRoles::PATTERN).bits(),
                ),
                (
                    9,
                    (HighlightRoles::SELECTED
                        | HighlightRoles::PATTERN
                        | HighlightRoles::AUXILIARY)
                        .bits(),
                ),
                (
                    12,
                    (HighlightRoles::SELECTED
                        | HighlightRoles::PATTERN
                        | HighlightRoles::AUXILIARY)
                        .bits(),
                ),
                (
                    3,
                    (HighlightRoles::SELECTED | HighlightRoles::PATTERN).bits(),
                ),
            ]
        );

        let type_3_naked_grid = sparse_snapshot(&[
            (0, "12"),
            (3, "12"),
            (9, "123"),
            (12, "124"),
            (10, "34"),
            (11, "3"),
            (16, "12"),
        ]);
        let type_3_naked = find_unique_loop(&type_3_naked_grid, EngineConfig::default()).unwrap();
        let Evidence::UniqueLoop {
            kind: UniqueLoopKind::Type3Naked {
                region, set_cells, ..
            },
            ..
        } = type_3_naked.evidence()
        else {
            panic!("type 3 naked fixture");
        };
        assert_eq!((region.type_index(), region.region_index()), (1, 1));
        assert_eq!(set_cells.iter().map(CellId::raw).collect::<Vec<_>>(), [10]);
        let type_3_naked_presentation = present(&type_3_naked_grid, &type_3_naked).unwrap();
        let type_3_naked_view = &type_3_naked_presentation.views[0];
        assert!(type_3_naked_view.links.is_empty());
        assert_eq!(conclusion_candidates(&type_3_naked_presentation), [(11, 3)]);
        assert_eq!(
            type_3_naked_view
                .region_marks
                .iter()
                .map(|mark| (mark.region.type_index(), mark.region.region_index()))
                .collect::<Vec<_>>(),
            [(1, 1)]
        );
        assert_eq!(
            cell_mark_entries(type_3_naked_view)
                .into_iter()
                .map(|(cell, _)| cell)
                .collect::<Vec<_>>(),
            [0, 9, 12, 3, 10]
        );

        let type_3_hidden_grid = sparse_snapshot(&[
            (0, "12"),
            (3, "12"),
            (9, "124"),
            (12, "125"),
            (10, "123"),
            (11, "45"),
        ]);
        let type_3_hidden = find_unique_loop(&type_3_hidden_grid, EngineConfig::default()).unwrap();
        assert!(matches!(
            type_3_hidden.evidence(),
            Evidence::UniqueLoop {
                kind: UniqueLoopKind::Type3Hidden { .. },
                ..
            }
        ));
        let type_3_hidden_presentation = present(&type_3_hidden_grid, &type_3_hidden).unwrap();
        let type_3_hidden_view = &type_3_hidden_presentation.views[0];
        assert!(type_3_hidden_view.links.is_empty());
        assert_eq!(
            conclusion_candidates(&type_3_hidden_presentation),
            [(10, 3)]
        );
        let hidden_effect = type_3_hidden_view
            .candidate_marks
            .iter()
            .find(|mark| mark.candidate.cell.raw() == 10 && mark.candidate.digit.get() == 3)
            .unwrap();
        assert!(hidden_effect.roles.contains(HighlightRoles::NEGATIVE));
        assert!(hidden_effect.roles.contains(HighlightRoles::CONCLUSION));
        assert!(
            type_3_hidden_view
                .candidate_marks
                .iter()
                .filter(|mark| {
                    mark.candidate.cell.raw() == 10 && mark.candidate.digit.get() <= 2
                })
                .all(|mark| mark.roles.contains(HighlightRoles::AUXILIARY)
                    && mark.roles.contains(HighlightRoles::POSITIVE))
        );

        let type_4_grid = sparse_snapshot(&[(0, "12"), (3, "12"), (9, "123"), (12, "124")]);
        let type_4 = find_unique_loop(&type_4_grid, EngineConfig::default()).unwrap();
        let Evidence::UniqueLoop {
            kind:
                UniqueLoopKind::Type4 {
                    region,
                    lock_digit,
                    remove_digit,
                    ..
                },
            ..
        } = type_4.evidence()
        else {
            panic!("type 4 fixture");
        };
        assert_eq!((region.type_index(), region.region_index()), (1, 1));
        assert_eq!((lock_digit.get(), remove_digit.get()), (1, 2));
        let type_4_presentation = present(&type_4_grid, &type_4).unwrap();
        let type_4_view = &type_4_presentation.views[0];
        assert!(type_4_view.links.is_empty());
        assert_eq!(
            conclusion_candidates(&type_4_presentation),
            [(9, 2), (12, 2)]
        );
        assert!(
            type_4_view
                .candidate_marks
                .iter()
                .filter(|mark| mark.candidate.cell.raw() == 9 || mark.candidate.cell.raw() == 12)
                .filter(|mark| mark.candidate.digit.get() == 1)
                .all(|mark| mark.roles.contains(HighlightRoles::AUXILIARY))
        );
    }

    #[test]
    fn bug_type_1_marks_only_retained_extra_values_then_effects() {
        let grid = bug_grid(&[(0, "567")]);
        let inference = find_bivalue_universal_grave(&grid, EngineConfig::default()).unwrap();
        assert!(matches!(
            inference.evidence(),
            Evidence::Bug {
                kind: BugKind::Type1 { .. }
            }
        ));
        let presentation = present(&grid, &inference).unwrap();
        let view = &presentation.views[0];
        let selected = (HighlightRoles::SELECTED | HighlightRoles::PATTERN).bits();
        let positive = (HighlightRoles::POSITIVE | HighlightRoles::PATTERN).bits();
        let conclusion = (HighlightRoles::NEGATIVE | HighlightRoles::CONCLUSION).bits();

        assert_eq!(cell_mark_entries(view), [(0, selected)]);
        assert_eq!(
            candidate_mark_entries(view),
            [(0, 7, positive), (0, 5, conclusion), (0, 6, conclusion),]
        );
        assert!(view.region_marks.is_empty());
        assert!(view.links.is_empty());
        assert_eq!(conclusion_candidates(&presentation), [(0, 5), (0, 6)]);
        assert_eq!(
            presentation.explanation.blocks[0],
            ExplanationBlock::Paragraph(vec![
                ExplanationInline::Technique(inference.technique()),
                ExplanationInline::Text(" leaves ".to_owned()),
                ExplanationInline::Cell(CellId::new(0).unwrap()),
                ExplanationInline::Text(" with the BUG value set ".to_owned()),
                ExplanationInline::Digit(Digit::new(7).unwrap()),
                ExplanationInline::Text(".".to_owned()),
            ])
        );
    }

    #[test]
    fn bug_remaining_subtypes_preserve_evidence_and_victim_order_without_links() {
        let type_2_grid = bug_grid(&[(0, "567"), (1, "347")]);
        let type_2 = find_bivalue_universal_grave(&type_2_grid, EngineConfig::default()).unwrap();
        let Evidence::Bug {
            kind: BugKind::Type2 { bug_cells, digit },
        } = type_2.evidence()
        else {
            panic!("BUG2 fixture");
        };
        assert_eq!(digit.get(), 7);
        assert_eq!(
            bug_cells
                .iter_with_values()
                .map(|(cell, values)| (cell.raw(), values.bits()))
                .collect::<Vec<_>>(),
            [(0, mask("7").bits()), (1, mask("7").bits())]
        );
        let type_2_presentation = present(&type_2_grid, &type_2).unwrap();
        let type_2_view = &type_2_presentation.views[0];
        assert!(type_2_view.region_marks.is_empty());
        assert!(type_2_view.links.is_empty());
        assert_eq!(
            conclusion_candidates(&type_2_presentation),
            [(3, 7), (4, 7), (9, 7), (10, 7)]
        );
        assert_eq!(
            candidate_mark_entries(type_2_view)
                .into_iter()
                .map(|(cell, digit, _)| (cell, digit))
                .collect::<Vec<_>>(),
            [(0, 7), (1, 7), (3, 7), (4, 7), (9, 7), (10, 7)]
        );

        let type_3_grid = bug_grid(&[(0, "568"), (1, "349")]);
        let type_3 = find_bivalue_universal_grave(&type_3_grid, EngineConfig::default()).unwrap();
        let Evidence::Bug {
            kind:
                BugKind::Type3 {
                    set_cells,
                    region,
                    set_values,
                    ..
                },
        } = type_3.evidence()
        else {
            panic!("BUG3 fixture");
        };
        assert_eq!((region.type_index(), region.region_index()), (0, 0));
        assert_eq!(set_cells.iter().map(CellId::raw).collect::<Vec<_>>(), [20]);
        assert_eq!(set_values, mask("89"));
        let type_3_presentation = present(&type_3_grid, &type_3).unwrap();
        let type_3_view = &type_3_presentation.views[0];
        assert!(type_3_view.links.is_empty());
        assert_eq!(
            conclusion_candidates(&type_3_presentation),
            [(10, 8), (19, 9)]
        );
        assert_eq!(
            cell_mark_entries(type_3_view)
                .into_iter()
                .map(|(cell, _)| cell)
                .collect::<Vec<_>>(),
            [0, 1, 20]
        );
        assert_eq!(
            candidate_mark_entries(type_3_view)
                .into_iter()
                .map(|(cell, digit, _)| (cell, digit))
                .collect::<Vec<_>>(),
            [(0, 8), (1, 9), (20, 8), (20, 9), (10, 8), (19, 9)]
        );

        let type_4_grid = bug_grid(&[(0, "568"), (3, "679")]);
        let type_4 = find_bivalue_universal_grave(&type_4_grid, EngineConfig::default()).unwrap();
        let Evidence::Bug {
            kind:
                BugKind::Type4 {
                    bug_cells,
                    region,
                    locked_digit,
                    ..
                },
        } = type_4.evidence()
        else {
            panic!("BUG4 fixture");
        };
        assert_eq!(bug_cells.map(CellId::raw), [0, 3]);
        assert_eq!((region.type_index(), region.region_index()), (1, 0));
        assert_eq!(locked_digit.get(), 6);
        let type_4_presentation = present(&type_4_grid, &type_4).unwrap();
        let type_4_view = &type_4_presentation.views[0];
        assert!(type_4_view.links.is_empty());
        assert_eq!(
            conclusion_candidates(&type_4_presentation),
            [(0, 5), (3, 7)]
        );
        assert_eq!(
            candidate_mark_entries(type_4_view)
                .into_iter()
                .map(|(cell, digit, _)| (cell, digit))
                .collect::<Vec<_>>(),
            [(0, 6), (0, 8), (3, 6), (3, 9), (0, 5), (3, 7)]
        );
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

    #[test]
    fn selected_nishio_maps_both_contradictory_target_closures() {
        let grid = sparse_snapshot(&[(0, "78"), (1, "79"), (9, "79")]);
        let detailed =
            find_nishio_forcing_chain_with_proof(&grid, EngineConfig::default()).unwrap();
        let (inference, proof) = detailed.into_parts();
        let presentation = present_with_selected_chain_proof(&grid, &inference, &proof).unwrap();

        assert_eq!(
            presentation
                .views
                .iter()
                .map(|view| (view.key.as_str(), view.label.as_str()))
                .collect::<Vec<_>>(),
            [
                ("nishio-on", "Contradiction: target true"),
                ("nishio-off", "Contradiction: target false"),
            ]
        );
        let target_on = &presentation.views[0];
        let target_off = &presentation.views[1];
        let positive_pattern = (HighlightRoles::POSITIVE | HighlightRoles::PATTERN).bits();
        let source_conclusion = (HighlightRoles::POSITIVE
            | HighlightRoles::NEGATIVE
            | HighlightRoles::PATTERN
            | HighlightRoles::CONCLUSION)
            .bits();

        assert_eq!(
            candidate_mark_entries(target_on),
            [(9, 7, positive_pattern), (0, 7, source_conclusion)]
        );
        assert_eq!(
            target_on.links,
            [super::CandidateLink {
                from: LinkEndpoint::Candidate(CandidateRef {
                    cell: CellId::new(0).unwrap(),
                    digit: Digit::new(7).unwrap(),
                }),
                to: LinkEndpoint::Candidate(CandidateRef {
                    cell: CellId::new(9).unwrap(),
                    digit: Digit::new(7).unwrap(),
                }),
                kind: LinkKind::Strong,
                cause: LinkCause::Region(RegionId::new(2, 0).unwrap()),
                directed: true,
            }]
        );

        assert_eq!(
            candidate_mark_entries(target_off),
            [
                (
                    9,
                    7,
                    (HighlightRoles::NEGATIVE | HighlightRoles::PATTERN).bits(),
                ),
                (1, 7, positive_pattern),
                (0, 7, source_conclusion),
            ]
        );
        assert_eq!(
            target_off
                .links
                .iter()
                .map(|link| (link.kind, link.cause))
                .collect::<Vec<_>>(),
            [
                (
                    LinkKind::Weak,
                    LinkCause::Region(RegionId::new(0, 0).unwrap()),
                ),
                (
                    LinkKind::Strong,
                    LinkCause::Region(RegionId::new(1, 0).unwrap()),
                ),
            ]
        );
        assert!(
            presentation
                .views
                .iter()
                .all(|view| view.links.iter().all(|link| link.directed))
        );
        assert_eq!(
            cell_mark_entries(target_on),
            [(
                0,
                (HighlightRoles::SELECTED | HighlightRoles::CONCLUSION).bits()
            )]
        );
        assert_eq!(
            cell_mark_entries(target_off),
            [(
                0,
                (HighlightRoles::SELECTED | HighlightRoles::CONCLUSION).bits()
            )]
        );
    }

    #[test]
    fn selected_multiple_chain_maps_stable_ordered_region_branch_views() {
        let grid = sparse_snapshot(&[(0, "123"), (1, "24"), (2, "25"), (10, "26")]);
        let detailed =
            find_multiple_forcing_chain_with_proof(&grid, EngineConfig::default()).unwrap();
        let (inference, proof) = detailed.into_parts();
        let presentation = present_with_selected_chain_proof(&grid, &inference, &proof).unwrap();

        assert_eq!(
            presentation
                .views
                .iter()
                .map(|view| (view.key.as_str(), view.label.as_str()))
                .collect::<Vec<_>>(),
            [
                ("region-branch-0", "Region branch 1"),
                ("region-branch-1", "Region branch 2"),
                ("region-branch-2", "Region branch 3"),
            ]
        );
        let target =
            (HighlightRoles::NEGATIVE | HighlightRoles::PATTERN | HighlightRoles::CONCLUSION)
                .bits();
        let source = (HighlightRoles::POSITIVE | HighlightRoles::PATTERN).bits();
        for (view, source_cell) in presentation.views.iter().zip([0_u8, 1, 2]) {
            assert_eq!(
                candidate_mark_entries(view),
                [(10, 2, target), (source_cell, 2, source)]
            );
            assert_eq!(
                view.links,
                [super::CandidateLink {
                    from: LinkEndpoint::Candidate(CandidateRef {
                        cell: CellId::new(source_cell).unwrap(),
                        digit: Digit::new(2).unwrap(),
                    }),
                    to: LinkEndpoint::Candidate(CandidateRef {
                        cell: CellId::new(10).unwrap(),
                        digit: Digit::new(2).unwrap(),
                    }),
                    kind: LinkKind::Weak,
                    cause: LinkCause::Region(RegionId::new(0, 0).unwrap()),
                    directed: true,
                }]
            );
        }
    }

    #[test]
    fn selected_advanced_and_nested_chains_map_collapsed_edges_as_implications() {
        let topology = Arc::new(ConstraintTopology::new(VariantConfig::default()));
        let dfc_plus_grid = Grid::from_puzzle(
            Arc::clone(&topology),
            &Puzzle::parse(
                "........1.....2....34..........5..6...17..3..8....9..4...6...7...8..4..9.2..3.5..",
            )
            .unwrap(),
        );
        let dfc_plus = find_dynamic_forcing_chain_plus_with_proof_checked(
            &dfc_plus_grid,
            EngineConfig::default(),
        )
        .expect("checked DFC+ proof search")
        .expect("selected DFC+ proof");
        let (inference, proof) = dfc_plus.into_parts();
        let presentation =
            present_with_selected_chain_proof(&dfc_plus_grid, &inference, &proof).unwrap();
        let collapsed = presentation
            .views
            .iter()
            .flat_map(|view| &view.links)
            .filter(|link| link.cause == LinkCause::Derived)
            .collect::<Vec<_>>();
        assert!(!collapsed.is_empty());
        assert!(
            collapsed
                .iter()
                .all(|link| link.kind == LinkKind::Implication)
        );

        let nested_grid = Grid::from_puzzle(
            topology,
            &Puzzle::parse(
                "100000002030400050006000700040603000000020000000508090007000100080009030200000006",
            )
            .unwrap(),
        );
        let nested = find_nested_forcing_chain_with_proof_checked(
            &nested_grid,
            EngineConfig::default(),
            2,
            0,
        )
        .expect("checked level-two proof search")
        .expect("selected level-two proof");
        let (inference, proof) = nested.into_parts();
        let presentation =
            present_with_selected_chain_proof(&nested_grid, &inference, &proof).unwrap();
        let collapsed = presentation
            .views
            .iter()
            .flat_map(|view| &view.links)
            .filter(|link| link.cause == LinkCause::Derived)
            .collect::<Vec<_>>();
        assert!(!collapsed.is_empty());
        assert!(
            collapsed
                .iter()
                .all(|link| link.kind == LinkKind::Implication)
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
