//! Versioned primitive projection of the semantic presentation contract.
//!
//! These DTOs deliberately carry no transport dependency. Their format-neutral
//! Serde projection lets native commands, WebAssembly workers and test oracles
//! serialize the same ordered data without exposing core or engine types.

use serde::Serialize;
use sukaku_forge_engine::Technique;

use crate::{
    CandidateLink, CandidateMark, ExplanationBlock, ExplanationDoc, ExplanationInline,
    HighlightRoles, HintIdentity, HintPresentation, HintView, LinkCause, LinkEndpoint, LinkKind,
    RegionMark,
};

/// Current application-port presentation protocol.
pub const PROTOCOL_VERSION: u16 = 2;

pub const ROLE_SELECTED: u16 = HighlightRoles::SELECTED.bits();
pub const ROLE_PATTERN: u16 = HighlightRoles::PATTERN.bits();
pub const ROLE_POSITIVE: u16 = HighlightRoles::POSITIVE.bits();
pub const ROLE_NEGATIVE: u16 = HighlightRoles::NEGATIVE.bits();
pub const ROLE_AUXILIARY: u16 = HighlightRoles::AUXILIARY.bits();
pub const ROLE_CONCLUSION: u16 = HighlightRoles::CONCLUSION.bits();
pub const ROLE_PRIMARY: u16 = HighlightRoles::PRIMARY.bits();
pub const ROLE_SECONDARY: u16 = HighlightRoles::SECONDARY.bits();

/// One presentation tied to the session revision whose grid produced it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HintPresentationEnvelope {
    pub protocol_version: u16,
    /// Decimal text keeps the full session counter exact in JavaScript.
    pub revision: String,
    pub presentation: HintPresentationDto,
}

impl HintPresentationEnvelope {
    #[must_use]
    pub fn new(revision: u64, presentation: &HintPresentation) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            revision: revision.to_string(),
            presentation: presentation.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HintPresentationDto {
    pub identity: HintIdentityDto,
    pub views: Vec<HintViewDto>,
    pub explanation: ExplanationDocDto,
}

impl From<&HintPresentation> for HintPresentationDto {
    fn from(presentation: &HintPresentation) -> Self {
        Self {
            identity: (&presentation.identity).into(),
            views: presentation.views.iter().map(Into::into).collect(),
            explanation: (&presentation.explanation).into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HintIdentityDto {
    pub technique_key: String,
    pub name: String,
    pub short_name: String,
    pub rating_tenths: u16,
}

impl From<&HintIdentity> for HintIdentityDto {
    fn from(identity: &HintIdentity) -> Self {
        Self {
            technique_key: technique_key(identity.technique).to_owned(),
            name: identity.name.clone(),
            short_name: identity.short_name.clone(),
            rating_tenths: identity.rating.tenths(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HintViewDto {
    pub key: String,
    pub label: String,
    pub cell_marks: Vec<CellMarkDto>,
    pub region_marks: Vec<RegionMarkDto>,
    pub candidate_marks: Vec<CandidateMarkDto>,
    pub links: Vec<CandidateLinkDto>,
}

impl From<&HintView> for HintViewDto {
    fn from(view: &HintView) -> Self {
        Self {
            key: view.key.clone(),
            label: view.label.clone(),
            cell_marks: view
                .cell_marks
                .iter()
                .map(|mark| CellMarkDto {
                    cell: mark.cell.raw(),
                    roles: mark.roles.bits(),
                })
                .collect(),
            region_marks: view.region_marks.iter().map(Into::into).collect(),
            candidate_marks: view.candidate_marks.iter().map(Into::into).collect(),
            links: view.links.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CellMarkDto {
    pub cell: u8,
    pub roles: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct RegionMarkDto {
    pub region_type: u8,
    pub region_index: u8,
    pub roles: u16,
}

impl From<&RegionMark> for RegionMarkDto {
    fn from(mark: &RegionMark) -> Self {
        Self {
            region_type: mark.region.type_index() as u8,
            region_index: mark.region.region_index() as u8,
            roles: mark.roles.bits(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CandidateRefDto {
    pub cell: u8,
    pub digit: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CandidateMarkDto {
    pub candidate: CandidateRefDto,
    pub roles: u16,
}

impl From<&CandidateMark> for CandidateMarkDto {
    fn from(mark: &CandidateMark) -> Self {
        Self {
            candidate: CandidateRefDto {
                cell: mark.candidate.cell.raw(),
                digit: mark.candidate.digit.get(),
            },
            roles: mark.roles.bits(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CandidateLinkDto {
    pub from: LinkEndpointDto,
    pub to: LinkEndpointDto,
    pub kind: LinkKindDto,
    pub cause: LinkCauseDto,
    pub directed: bool,
}

impl From<&CandidateLink> for CandidateLinkDto {
    fn from(link: &CandidateLink) -> Self {
        Self {
            from: link.from.into(),
            to: link.to.into(),
            kind: link.kind.into(),
            cause: link.cause.into(),
            directed: link.directed,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LinkEndpointDto {
    Candidate {
        cell: u8,
        digit: u8,
    },
    CandidateGroup {
        representative: CandidateRefDto,
        members: Vec<CandidateRefDto>,
    },
    CellCenter {
        cell: u8,
    },
}

impl From<LinkEndpoint> for LinkEndpointDto {
    fn from(endpoint: LinkEndpoint) -> Self {
        match endpoint {
            LinkEndpoint::Candidate(candidate) => Self::Candidate {
                cell: candidate.cell.raw(),
                digit: candidate.digit.get(),
            },
            LinkEndpoint::CandidateGroup {
                representative,
                members,
            } => Self::CandidateGroup {
                representative: CandidateRefDto {
                    cell: representative.cell.raw(),
                    digit: representative.digit.get(),
                },
                members: members
                    .iter()
                    .map(|cell| CandidateRefDto {
                        cell: cell.raw(),
                        digit: representative.digit.get(),
                    })
                    .collect(),
            },
            LinkEndpoint::CellCenter(cell) => Self::CellCenter { cell: cell.raw() },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKindDto {
    Strong,
    GroupedStrong,
    Weak,
    Implication,
}

impl From<LinkKind> for LinkKindDto {
    fn from(kind: LinkKind) -> Self {
        match kind {
            LinkKind::Strong => Self::Strong,
            LinkKind::GroupedStrong => Self::GroupedStrong,
            LinkKind::Weak => Self::Weak,
            LinkKind::Implication => Self::Implication,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LinkCauseDto {
    Cell,
    Region { region_type: u8, region_index: u8 },
    Visibility,
    Derived,
}

impl From<LinkCause> for LinkCauseDto {
    fn from(cause: LinkCause) -> Self {
        match cause {
            LinkCause::Cell => Self::Cell,
            LinkCause::Region(region) => Self::Region {
                region_type: region.type_index() as u8,
                region_index: region.region_index() as u8,
            },
            LinkCause::Visibility => Self::Visibility,
            LinkCause::Derived => Self::Derived,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExplanationDocDto {
    pub blocks: Vec<ExplanationBlockDto>,
}

impl From<&ExplanationDoc> for ExplanationDocDto {
    fn from(document: &ExplanationDoc) -> Self {
        Self {
            blocks: document.blocks.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExplanationBlockDto {
    Paragraph {
        inlines: Vec<ExplanationInlineDto>,
    },
    UnorderedList {
        items: Vec<Vec<ExplanationInlineDto>>,
    },
}

impl From<&ExplanationBlock> for ExplanationBlockDto {
    fn from(block: &ExplanationBlock) -> Self {
        match block {
            ExplanationBlock::Paragraph(inlines) => Self::Paragraph {
                inlines: inlines.iter().map(Into::into).collect(),
            },
            ExplanationBlock::UnorderedList(items) => Self::UnorderedList {
                items: items
                    .iter()
                    .map(|item| item.iter().map(Into::into).collect())
                    .collect(),
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExplanationInlineDto {
    Text { text: String },
    Technique { technique_key: String },
    Cell { cell: u8 },
    Digit { digit: u8 },
    Region { region_type: u8, region_index: u8 },
    Candidate { cell: u8, digit: u8 },
}

impl From<&ExplanationInline> for ExplanationInlineDto {
    fn from(inline: &ExplanationInline) -> Self {
        match inline {
            ExplanationInline::Text(text) => Self::Text { text: text.clone() },
            ExplanationInline::Technique(technique) => Self::Technique {
                technique_key: technique_key(*technique).to_owned(),
            },
            ExplanationInline::Cell(cell) => Self::Cell { cell: cell.raw() },
            ExplanationInline::Digit(digit) => Self::Digit { digit: digit.get() },
            ExplanationInline::Region(region) => Self::Region {
                region_type: region.type_index() as u8,
                region_index: region.region_index() as u8,
            },
            ExplanationInline::Candidate(candidate) => Self::Candidate {
                cell: candidate.cell.raw(),
                digit: candidate.digit.get(),
            },
        }
    }
}

/// Stable machine key for a technique, independent of its display name.
#[must_use]
pub const fn technique_key(technique: Technique) -> &'static str {
    match technique {
        Technique::HiddenSingle => "hidden_single",
        Technique::NakedSingle => "naked_single",
        Technique::NonConsecutiveForcingCell => "non_consecutive_forcing_cell",
        Technique::LockedNonConsecutive => "locked_non_consecutive",
        Technique::DirectPointing => "direct_pointing",
        Technique::DirectClaiming => "direct_claiming",
        Technique::DirectHiddenPair => "direct_hidden_pair",
        Technique::DirectHiddenTriplet => "direct_hidden_triplet",
        Technique::Pointing => "pointing",
        Technique::Claiming => "claiming",
        Technique::GeneralizedIntersections => "generalized_intersections",
        Technique::NakedPair => "naked_pair",
        Technique::GeneralizedNakedPair => "generalized_naked_pair",
        Technique::XWing => "x_wing",
        Technique::HiddenPair => "hidden_pair",
        Technique::NakedTriplet => "naked_triplet",
        Technique::GeneralizedNakedTriplet => "generalized_naked_triplet",
        Technique::Swordfish => "swordfish",
        Technique::HiddenTriplet => "hidden_triplet",
        Technique::TurbotFish => "turbot_fish",
        Technique::XYWing => "xy_wing",
        Technique::XYZWing => "xyz_wing",
        Technique::UniqueLoop => "unique_loop",
        Technique::NakedQuad => "naked_quad",
        Technique::GeneralizedNakedQuad => "generalized_naked_quad",
        Technique::Jellyfish => "jellyfish",
        Technique::HiddenQuad => "hidden_quad",
        Technique::ThreeStrongLinks => "three_strong_links",
        Technique::FourStrongLinks => "four_strong_links",
        Technique::WXYZWing => "wxyz_wing",
        Technique::VWXYZWing => "vwxyz_wing",
        Technique::UVWXYZWing => "uvwxyz_wing",
        Technique::TUVWXYZWing => "tuvwxyz_wing",
        Technique::BivalueUniversalGrave => "bivalue_universal_grave",
        Technique::AlignedPairExclusion => "aligned_pair_exclusion",
        Technique::ForcingChainCycle => "forcing_chain_cycle",
        Technique::AlignedTripletExclusion => "aligned_triplet_exclusion",
        Technique::NishioForcingChain => "nishio_forcing_chain",
        Technique::MultipleForcingChain => "multiple_forcing_chain",
        Technique::DynamicForcingChain => "dynamic_forcing_chain",
        Technique::DynamicForcingChainPlus => "dynamic_forcing_chain_plus",
        Technique::NestedForcingChain => "nested_forcing_chain",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use sukaku_forge_core::{ConstraintTopology, Grid, Puzzle, VariantConfig};
    use sukaku_forge_engine::{EngineConfig, find_fish, find_wing};

    use super::{
        CandidateLinkDto, ExplanationBlockDto, ExplanationInlineDto, HintPresentationEnvelope,
        LinkCauseDto, LinkEndpointDto, LinkKindDto, PROTOCOL_VERSION, ROLE_CONCLUSION,
        ROLE_NEGATIVE, ROLE_PATTERN, ROLE_POSITIVE, ROLE_PRIMARY, ROLE_SECONDARY, ROLE_SELECTED,
    };
    use crate::present;

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

    #[test]
    fn xy_wing_projection_pins_envelope_order_and_role_masks() {
        let grid = sparse_snapshot(&[(0, "12"), (3, "13"), (27, "23"), (30, "3")]);
        let inference = find_wing(&grid, false).unwrap();
        let presentation = present(&grid, &inference).unwrap();
        let envelope = HintPresentationEnvelope::new(37, &presentation);

        assert_eq!(envelope.protocol_version, PROTOCOL_VERSION);
        assert_eq!(envelope.protocol_version, 2);
        assert_eq!(envelope.revision, "37");
        assert_eq!(envelope.presentation.identity.technique_key, "xy_wing");
        assert_eq!(envelope.presentation.identity.name, "XY-Wing");
        assert_eq!(envelope.presentation.identity.short_name, "XYW");
        assert_eq!(envelope.presentation.identity.rating_tenths, 42);

        let view = &envelope.presentation.views[0];
        assert_eq!(view.key, "main");
        assert_eq!(view.label, "View 1");
        assert_eq!(
            view.cell_marks
                .iter()
                .map(|mark| (mark.cell, mark.roles))
                .collect::<Vec<_>>(),
            [
                (0, ROLE_SELECTED | ROLE_PATTERN),
                (3, ROLE_SELECTED | ROLE_PATTERN),
                (27, ROLE_SELECTED | ROLE_PATTERN),
            ]
        );
        assert_eq!(
            view.candidate_marks
                .iter()
                .map(|mark| (mark.candidate.cell, mark.candidate.digit, mark.roles))
                .collect::<Vec<_>>(),
            [
                (0, 1, ROLE_POSITIVE | ROLE_NEGATIVE | ROLE_PATTERN),
                (0, 2, ROLE_POSITIVE | ROLE_NEGATIVE | ROLE_PATTERN),
                (3, 3, ROLE_POSITIVE | ROLE_PATTERN),
                (27, 3, ROLE_POSITIVE | ROLE_PATTERN),
                (30, 3, ROLE_NEGATIVE | ROLE_CONCLUSION),
            ]
        );
        assert_eq!(
            view.links,
            [
                CandidateLinkDto {
                    from: LinkEndpointDto::Candidate { cell: 0, digit: 1 },
                    to: LinkEndpointDto::Candidate { cell: 3, digit: 1 },
                    kind: LinkKindDto::Weak,
                    cause: LinkCauseDto::Visibility,
                    directed: true,
                },
                CandidateLinkDto {
                    from: LinkEndpointDto::Candidate { cell: 0, digit: 2 },
                    to: LinkEndpointDto::Candidate { cell: 27, digit: 2 },
                    kind: LinkKindDto::Weak,
                    cause: LinkCauseDto::Visibility,
                    directed: true,
                },
            ]
        );

        assert_eq!(
            envelope.presentation.explanation.blocks[0],
            ExplanationBlockDto::Paragraph {
                inlines: vec![ExplanationInlineDto::Text {
                    text: "XY-Wing: Cells r1c1,r1c4,r4c1 on value 3".to_owned(),
                }],
            }
        );
    }

    #[test]
    fn fish_projection_preserves_interleaved_region_order_and_masks() {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = "123456789".repeat(81).chars().collect::<Vec<_>>();
        for row in 0..9 {
            if !matches!(row, 1 | 6) {
                for column in [0, 3] {
                    display[(row * 9 + column) * 9 + 4] = '.';
                }
            }
        }
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        let grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &values,
            &candidates,
        )
        .unwrap();
        let inference = find_fish(&grid, EngineConfig::default(), 2).unwrap();
        let presentation = present(&grid, &inference).unwrap();
        let envelope = HintPresentationEnvelope::new(8, &presentation);

        assert_eq!(envelope.revision, "8");
        assert_eq!(
            envelope.presentation.views[0]
                .region_marks
                .iter()
                .map(|mark| (mark.region_type, mark.region_index, mark.roles))
                .collect::<Vec<_>>(),
            [
                (2, 0, ROLE_PRIMARY | ROLE_PATTERN),
                (1, 1, ROLE_SECONDARY | ROLE_PATTERN),
                (2, 3, ROLE_PRIMARY | ROLE_PATTERN),
                (1, 6, ROLE_SECONDARY | ROLE_PATTERN),
            ]
        );
    }

    #[test]
    fn v2_json_tags_group_members_and_js_safe_revision_are_frozen() {
        let grid = sparse_snapshot(&[(0, "12"), (3, "13"), (27, "23"), (30, "3")]);
        let inference = find_wing(&grid, false).unwrap();
        let presentation = present(&grid, &inference).unwrap();
        let envelope = HintPresentationEnvelope::new(u64::MAX, &presentation);
        let encoded = serde_json::to_value(&envelope).unwrap();

        assert_eq!(encoded["protocol_version"], 2);
        assert_eq!(encoded["revision"], u64::MAX.to_string());
        assert_eq!(encoded["presentation"]["views"][0]["key"], "main");
        assert_eq!(
            serde_json::to_value(CandidateLinkDto {
                from: LinkEndpointDto::CandidateGroup {
                    representative: super::CandidateRefDto { cell: 1, digit: 2 },
                    members: vec![
                        super::CandidateRefDto { cell: 1, digit: 2 },
                        super::CandidateRefDto { cell: 2, digit: 2 },
                    ],
                },
                to: LinkEndpointDto::Candidate { cell: 9, digit: 2 },
                kind: LinkKindDto::GroupedStrong,
                cause: LinkCauseDto::Region {
                    region_type: 1,
                    region_index: 0,
                },
                directed: true,
            })
            .unwrap(),
            json!({
                "from": {
                    "type": "candidate_group",
                    "representative": {"cell": 1, "digit": 2},
                    "members": [
                        {"cell": 1, "digit": 2},
                        {"cell": 2, "digit": 2}
                    ]
                },
                "to": {"type": "candidate", "cell": 9, "digit": 2},
                "kind": "grouped_strong",
                "cause": {"type": "region", "region_type": 1, "region_index": 0},
                "directed": true
            })
        );
        assert_eq!(
            serde_json::to_value(ExplanationBlockDto::Paragraph {
                inlines: vec![ExplanationInlineDto::Text {
                    text: "proof".to_owned(),
                }],
            })
            .unwrap(),
            json!({
                "type": "paragraph",
                "inlines": [{"type": "text", "text": "proof"}]
            })
        );
    }
}
