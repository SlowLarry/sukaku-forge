//! Versioned, transport-neutral command boundary for graphical clients.
//!
//! Native and WebAssembly adapters own one [`ApplicationPort`] and forward
//! requests here. The dispatcher is the only layer above [`crate::Session`]
//! that parses client IDs or revisions.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sukaku_forge_core::{
    CellId, ConstraintTopology, Digit, Grid, NonConsecutiveMode, Puzzle, RegionId, VariantConfig,
};
use sukaku_forge_engine::{EngineConfig, PortGap, RatingMode, SearchPolicy, Solver};
use sukaku_forge_presentation::wire::{
    CandidateRefDto, HintIdentityDto, HintPresentationDto,
    PROTOCOL_VERSION as PRESENTATION_PROTOCOL_VERSION, technique_key,
};
use sukaku_forge_presentation::{UnsupportedPresentation, UnsupportedPresentationKind};

use crate::{
    AllHintsOutcome, HintEffects, HintId, HintSummary, MaterializedHintOutcome, NextHintOutcome,
    Session, SessionError, SessionSnapshot,
};

/// One protocol revision covers session commands and nested presentation DTOs.
pub const PROTOCOL_VERSION: u16 = PRESENTATION_PROTOCOL_VERSION;

/// A single application-port request. `request_id` is deliberately a `u32`,
/// which every JavaScript runtime can represent exactly.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct RequestDto {
    pub protocol_version: u16,
    pub request_id: u32,
    #[serde(flatten)]
    pub command: CommandDto,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum CommandDto {
    CreateSession {
        puzzle: String,
        #[serde(default)]
        variant: VariantDto,
        #[serde(default)]
        engine: EngineDto,
    },
    NextHint {
        expected_revision: String,
    },
    GetAllHints {
        expected_revision: String,
        #[serde(default)]
        include_expensive: bool,
    },
    GetHint {
        expected_revision: String,
        hint_id: String,
    },
    ApplyHint {
        expected_revision: String,
        hint_id: String,
    },
    PlaceValue {
        expected_revision: String,
        cell: u8,
        digit: u8,
    },
    ToggleCandidate {
        expected_revision: String,
        cell: u8,
        digit: u8,
    },
    Undo {
        expected_revision: String,
    },
    Redo {
        expected_revision: String,
    },
}

/// Full topology configuration accepted by `create_session`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct VariantDto {
    pub blocks: bool,
    pub disjoint_groups: bool,
    pub windows: bool,
    pub sudoku_x: bool,
    pub girandola: bool,
    pub asterisk: bool,
    pub center_dot: bool,
    pub anti_ferz: bool,
    pub anti_knight: bool,
    pub toroidal: bool,
    pub non_consecutive: NonConsecutiveDto,
    pub forbidden_pairs: bool,
}

impl Default for VariantDto {
    fn default() -> Self {
        VariantConfig::default().into()
    }
}

impl From<VariantDto> for VariantConfig {
    fn from(value: VariantDto) -> Self {
        Self {
            blocks: value.blocks,
            disjoint_groups: value.disjoint_groups,
            windows: value.windows,
            sudoku_x: value.sudoku_x,
            girandola: value.girandola,
            asterisk: value.asterisk,
            center_dot: value.center_dot,
            anti_ferz: value.anti_ferz,
            anti_knight: value.anti_knight,
            toroidal: value.toroidal,
            non_consecutive: value.non_consecutive.into(),
            forbidden_pairs: value.forbidden_pairs,
        }
    }
}

impl From<VariantConfig> for VariantDto {
    fn from(value: VariantConfig) -> Self {
        Self {
            blocks: value.blocks,
            disjoint_groups: value.disjoint_groups,
            windows: value.windows,
            sudoku_x: value.sudoku_x,
            girandola: value.girandola,
            asterisk: value.asterisk,
            center_dot: value.center_dot,
            anti_ferz: value.anti_ferz,
            anti_knight: value.anti_knight,
            toroidal: value.toroidal,
            non_consecutive: value.non_consecutive.into(),
            forbidden_pairs: value.forbidden_pairs,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NonConsecutiveDto {
    #[default]
    Off,
    Orthogonal,
    OrthogonalCyclic,
    Diagonal,
    DiagonalCyclic,
}

impl From<NonConsecutiveDto> for NonConsecutiveMode {
    fn from(value: NonConsecutiveDto) -> Self {
        match value {
            NonConsecutiveDto::Off => Self::Off,
            NonConsecutiveDto::Orthogonal => Self::Orthogonal,
            NonConsecutiveDto::OrthogonalCyclic => Self::OrthogonalCyclic,
            NonConsecutiveDto::Diagonal => Self::Diagonal,
            NonConsecutiveDto::DiagonalCyclic => Self::DiagonalCyclic,
        }
    }
}

impl From<NonConsecutiveMode> for NonConsecutiveDto {
    fn from(value: NonConsecutiveMode) -> Self {
        match value {
            NonConsecutiveMode::Off => Self::Off,
            NonConsecutiveMode::Orthogonal => Self::Orthogonal,
            NonConsecutiveMode::OrthogonalCyclic => Self::OrthogonalCyclic,
            NonConsecutiveMode::Diagonal => Self::Diagonal,
            NonConsecutiveMode::DiagonalCyclic => Self::DiagonalCyclic,
        }
    }
}

/// Solver settings that can be represented without exposing internal bitsets.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct EngineDto {
    pub variant_latin: bool,
    pub rating_mode: RatingModeDto,
    pub search_policy: SearchPolicyDto,
    pub forcing_chain_plus: u8,
    pub unique_loop_fix: bool,
    pub bug_fix: bool,
    pub java_default_technique_profile: bool,
}

impl Default for EngineDto {
    fn default() -> Self {
        let value = EngineConfig::default();
        Self {
            variant_latin: value.variant_latin,
            rating_mode: value.rating_mode.into(),
            search_policy: value.search_policy.into(),
            forcing_chain_plus: value.forcing_chain_plus,
            unique_loop_fix: value.unique_loop_fix,
            bug_fix: value.bug_fix,
            java_default_technique_profile: value.java_default_technique_profile,
        }
    }
}

impl TryFrom<EngineDto> for EngineConfig {
    type Error = ErrorDto;

    fn try_from(value: EngineDto) -> Result<Self, Self::Error> {
        if value.forcing_chain_plus > 2 {
            return Err(ErrorDto::new(
                "invalid_engine",
                "forcing_chain_plus must be between 0 and 2",
            ));
        }
        Ok(Self {
            variant_latin: value.variant_latin,
            rating_mode: value.rating_mode.into(),
            search_policy: value.search_policy.into(),
            forcing_chain_plus: value.forcing_chain_plus,
            unique_loop_fix: value.unique_loop_fix,
            bug_fix: value.bug_fix,
            enabled_techniques: Default::default(),
            java_default_technique_profile: value.java_default_technique_profile,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RatingModeDto {
    #[default]
    Original,
    Revised,
}

impl From<RatingModeDto> for RatingMode {
    fn from(value: RatingModeDto) -> Self {
        match value {
            RatingModeDto::Original => Self::Original,
            RatingModeDto::Revised => Self::Revised,
        }
    }
}

impl From<RatingMode> for RatingModeDto {
    fn from(value: RatingMode) -> Self {
        match value {
            RatingMode::Original => Self::Original,
            RatingMode::Revised => Self::Revised,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchPolicyDto {
    #[default]
    Compatibility,
    Forge,
}

impl From<SearchPolicyDto> for SearchPolicy {
    fn from(value: SearchPolicyDto) -> Self {
        match value {
            SearchPolicyDto::Compatibility => Self::Compatibility,
            SearchPolicyDto::Forge => Self::Forge,
        }
    }
}

impl From<SearchPolicy> for SearchPolicyDto {
    fn from(value: SearchPolicy) -> Self {
        match value {
            SearchPolicy::Compatibility => Self::Compatibility,
            SearchPolicy::Forge => Self::Forge,
        }
    }
}

/// Correlated application response. The `response` discriminant is flattened
/// so every transport sees the same shallow envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResponseDto {
    pub protocol_version: u16,
    pub request_id: u32,
    #[serde(flatten)]
    pub response: ResponseKindDto,
}

impl ResponseDto {
    fn new(request_id: u32, response: ResponseKindDto) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            response,
        }
    }

    fn error(request_id: u32, error: ErrorDto) -> Self {
        Self::new(request_id, ResponseKindDto::Error { error })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "response", rename_all = "snake_case")]
pub enum ResponseKindDto {
    SessionCreated {
        snapshot: SessionSnapshotDto,
        topology: TopologyDto,
    },
    Snapshot {
        snapshot: SessionSnapshotDto,
    },
    NextHint {
        revision: String,
        #[serde(flatten)]
        outcome: NextHintOutcomeDto,
    },
    AllHints {
        revision: String,
        #[serde(flatten)]
        outcome: AllHintsOutcomeDto,
    },
    Hint {
        revision: String,
        hint_id: String,
        #[serde(flatten)]
        outcome: MaterializedHintOutcomeDto,
    },
    Error {
        error: ErrorDto,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum NextHintOutcomeDto {
    Presented {
        hint_id: String,
        presentation: HintPresentationDto,
        effects: HintEffectsDto,
    },
    Unsupported {
        hint_id: String,
        unsupported: UnsupportedDto,
        effects: HintEffectsDto,
    },
    None,
    Incomplete {
        gap: GapDto,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AllHintsOutcomeDto {
    Complete {
        hints: Vec<HintSummaryDto>,
    },
    ConfirmationRequired,
    Incomplete {
        hints: Vec<HintSummaryDto>,
        gap: GapDto,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HintSummaryDto {
    pub hint_id: String,
    pub category: HintCategoryDto,
    pub group_key: String,
    pub group_name: String,
    pub identity: HintIdentityDto,
    pub effects: HintEffectsDto,
    pub filter_effects: HintEffectsDto,
}

impl From<HintSummary> for HintSummaryDto {
    fn from(value: HintSummary) -> Self {
        Self {
            hint_id: value.hint_id.0.to_string(),
            category: value.category.into(),
            group_key: value.group_key,
            group_name: value.group_name,
            identity: HintIdentityDto {
                technique_key: technique_key(value.technique).to_owned(),
                name: value.name,
                short_name: value.short_name,
                rating_tenths: value.rating.tenths(),
            },
            effects: HintEffectsDto::from_effects(value.effects),
            filter_effects: HintEffectsDto::from_effects(value.filter_effects),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HintCategoryDto {
    Direct,
    Indirect,
}

impl From<sukaku_forge_engine::HintCategory> for HintCategoryDto {
    fn from(value: sukaku_forge_engine::HintCategory) -> Self {
        match value {
            sukaku_forge_engine::HintCategory::Direct => Self::Direct,
            sukaku_forge_engine::HintCategory::Indirect => Self::Indirect,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum MaterializedHintOutcomeDto {
    Presented {
        presentation: HintPresentationDto,
        effects: HintEffectsDto,
    },
    Unsupported {
        unsupported: UnsupportedDto,
        effects: HintEffectsDto,
    },
    Incomplete {
        gap: GapDto,
        effects: HintEffectsDto,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UnsupportedDto {
    pub technique_key: String,
    pub kind: UnsupportedKindDto,
}

impl From<UnsupportedPresentation> for UnsupportedDto {
    fn from(value: UnsupportedPresentation) -> Self {
        Self {
            technique_key: technique_key(value.technique).to_owned(),
            kind: value.kind.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedKindDto {
    MissingChainProof,
    EvidenceNotImplemented,
}

impl From<UnsupportedPresentationKind> for UnsupportedKindDto {
    fn from(value: UnsupportedPresentationKind) -> Self {
        match value {
            UnsupportedPresentationKind::MissingChainProof => Self::MissingChainProof,
            UnsupportedPresentationKind::EvidenceNotImplemented => Self::EvidenceNotImplemented,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GapDto {
    pub code: GapCodeDto,
    pub message: String,
}

impl From<PortGap> for GapDto {
    fn from(value: PortGap) -> Self {
        match value {
            PortGap::Producer(_) => Self {
                code: GapCodeDto::ProducerNotPorted,
                message: "the selected solver producer is not yet ported".to_owned(),
            },
            PortGap::IndirectTechniques => Self {
                code: GapCodeDto::IndirectTechniques,
                message: "the indirect-technique group is not yet ported".to_owned(),
            },
            PortGap::LegacyFcPlus2(_) => Self {
                code: GapCodeDto::LegacyFcPlus2,
                message: "legacy FCPlus=2 reached an unsupported advanced producer".to_owned(),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GapCodeDto {
    ProducerNotPorted,
    IndirectTechniques,
    LegacyFcPlus2,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HintEffectsDto {
    pub placement: Option<CandidateRefDto>,
    pub removals: Vec<CandidateRemovalDto>,
    pub elimination_count: u16,
}

impl HintEffectsDto {
    fn from_effects(effects: HintEffects) -> Self {
        let placement = effects.placement.map(|(cell, digit)| CandidateRefDto {
            cell: cell.raw(),
            digit: digit.get(),
        });
        Self {
            placement,
            elimination_count: effects.removals.elimination_count(),
            removals: effects
                .removals
                .iter()
                .map(|removal| CandidateRemovalDto {
                    cell: removal.cell().raw(),
                    digits: removal.digits().bits(),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CandidateRemovalDto {
    pub cell: u8,
    pub digits: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionSnapshotDto {
    pub revision: String,
    pub values: Vec<u8>,
    pub candidate_masks: Vec<u16>,
    pub givens: Vec<bool>,
    pub can_undo: bool,
    pub can_redo: bool,
}

impl From<SessionSnapshot> for SessionSnapshotDto {
    fn from(value: SessionSnapshot) -> Self {
        Self {
            revision: value.revision.to_string(),
            values: value.values.into_iter().collect(),
            candidate_masks: value.candidate_masks.into_iter().collect(),
            givens: value.givens.into_iter().collect(),
            can_undo: value.can_undo,
            can_redo: value.can_redo,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TopologyDto {
    pub variant: VariantDto,
    pub regions: Vec<TopologyRegionDto>,
}

impl From<&ConstraintTopology> for TopologyDto {
    fn from(topology: &ConstraintTopology) -> Self {
        let regions = topology
            .active_region_types()
            .flat_map(|region_type| {
                (0..topology.region_count(region_type)).map(move |region_index| {
                    let region = RegionId::new(region_type as u8, region_index as u8)
                        .expect("active topology region identity");
                    TopologyRegionDto {
                        region_type: region_type as u8,
                        region_index: region_index as u8,
                        family_key: region_family_key(region_type).to_owned(),
                        label: region_label(region),
                        cells: *topology.region_cells(region),
                    }
                })
            })
            .collect();
        Self {
            variant: topology.config().into(),
            regions,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TopologyRegionDto {
    pub region_type: u8,
    pub region_index: u8,
    pub family_key: String,
    pub label: String,
    pub cells: [u8; 9],
}

const fn region_family_key(region_type: usize) -> &'static str {
    match region_type {
        0 => "block",
        1 => "row",
        2 => "column",
        3 => "disjoint_group",
        4 => "window",
        5 => "main_diagonal",
        6 => "anti_diagonal",
        7 => "girandola",
        8 => "asterisk",
        9 => "center_dot",
        _ => unreachable!(),
    }
}

fn region_label(region: RegionId) -> String {
    let label = match region.type_index() {
        0 => "Block",
        1 => "Row",
        2 => "Column",
        3 => "Disjoint group",
        4 => "Window",
        5 => return "Main diagonal".to_owned(),
        6 => return "Anti-diagonal".to_owned(),
        7 => return "Girandola".to_owned(),
        8 => return "Asterisk".to_owned(),
        9 => return "Center dot".to_owned(),
        _ => unreachable!(),
    };
    format!("{label} {}", region.region_index() + 1)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ErrorDto {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_revision: Option<String>,
}

impl ErrorDto {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            expected_revision: None,
            actual_revision: None,
        }
    }

    fn stale(expected: u64, actual: u64) -> Self {
        Self {
            code: "stale_revision".to_owned(),
            message: format!("stale session revision {expected}; current revision is {actual}"),
            expected_revision: Some(expected.to_string()),
            actual_revision: Some(actual.to_string()),
        }
    }
}

impl From<SessionError> for ErrorDto {
    fn from(value: SessionError) -> Self {
        let code = match value {
            SessionError::StaleRevision { expected, actual } => {
                return Self::stale(expected, actual);
            }
            SessionError::UnknownHint { .. } => "unknown_hint",
            SessionError::NothingToUndo => "nothing_to_undo",
            SessionError::NothingToRedo => "nothing_to_redo",
            SessionError::GivenCell { .. } => "given_cell",
            SessionError::SolvedCell { .. } => "solved_cell",
            SessionError::CandidateUnavailable { .. } => "candidate_unavailable",
            SessionError::CandidateConflicts { .. } => "candidate_conflicts",
        };
        Self::new(code, value.to_string())
    }
}

/// One authoritative session owner used by all eventual transport adapters.
#[derive(Clone, Debug, Default)]
pub struct ApplicationPort {
    session: Option<Session>,
}

impl ApplicationPort {
    #[must_use]
    pub const fn new() -> Self {
        Self { session: None }
    }

    /// Dispatch a typed request without imposing a transport.
    pub fn dispatch(&mut self, request: RequestDto) -> ResponseDto {
        let request_id = request.request_id;
        if request.protocol_version != PROTOCOL_VERSION {
            return ResponseDto::error(
                request_id,
                ErrorDto::new(
                    "unsupported_protocol_version",
                    format!(
                        "protocol version {} is unsupported; expected {PROTOCOL_VERSION}",
                        request.protocol_version
                    ),
                ),
            );
        }
        match self.handle(request.command) {
            Ok(response) => ResponseDto::new(request_id, response),
            Err(error) => ResponseDto::error(request_id, error),
        }
    }

    /// JSON convenience for adapters that exchange UTF-8 text.
    #[must_use]
    pub fn dispatch_json(&mut self, request: &str) -> String {
        let response = match serde_json::from_str::<RequestDto>(request) {
            Ok(request) => self.dispatch(request),
            Err(error) => ResponseDto::error(
                0,
                ErrorDto::new("invalid_request", format!("invalid request: {error}")),
            ),
        };
        serde_json::to_string(&response).expect("application response is serializable")
    }

    fn handle(&mut self, command: CommandDto) -> Result<ResponseKindDto, ErrorDto> {
        match command {
            CommandDto::CreateSession {
                puzzle,
                variant,
                engine,
            } => self.create_session(&puzzle, variant, engine),
            CommandDto::NextHint { expected_revision } => self.next_hint(&expected_revision),
            CommandDto::GetAllHints {
                expected_revision,
                include_expensive,
            } => self.get_all_hints(&expected_revision, include_expensive),
            CommandDto::GetHint {
                expected_revision,
                hint_id,
            } => self.get_hint(&expected_revision, &hint_id),
            CommandDto::ApplyHint {
                expected_revision,
                hint_id,
            } => self.apply_hint(&expected_revision, &hint_id),
            CommandDto::PlaceValue {
                expected_revision,
                cell,
                digit,
            } => self.place_value(&expected_revision, cell, digit),
            CommandDto::ToggleCandidate {
                expected_revision,
                cell,
                digit,
            } => self.toggle_candidate(&expected_revision, cell, digit),
            CommandDto::Undo { expected_revision } => self.undo(&expected_revision),
            CommandDto::Redo { expected_revision } => self.redo(&expected_revision),
        }
    }

    fn create_session(
        &mut self,
        puzzle: &str,
        variant: VariantDto,
        engine: EngineDto,
    ) -> Result<ResponseKindDto, ErrorDto> {
        let puzzle = Puzzle::parse(puzzle)
            .map_err(|error| ErrorDto::new("invalid_puzzle", error.to_string()))?;
        let engine = EngineConfig::try_from(engine)?;
        let topology = Arc::new(ConstraintTopology::new(variant.into()));
        let topology_dto = TopologyDto::from(topology.as_ref());
        let grid = Grid::from_puzzle(topology, &puzzle);
        let session = Session::new(grid, Solver::new(engine));
        let snapshot = session.snapshot().into();
        self.session = Some(session);
        Ok(ResponseKindDto::SessionCreated {
            snapshot,
            topology: topology_dto,
        })
    }

    fn next_hint(&mut self, revision: &str) -> Result<ResponseKindDto, ErrorDto> {
        let session = self.session_mut()?;
        let expected_revision = parse_decimal_u64(revision, "expected_revision", false)?;
        require_revision(session, expected_revision)?;
        let response = session.next_hint();
        let outcome = match response.outcome {
            NextHintOutcome::Presented {
                hint_id,
                presentation,
                effects,
            } => NextHintOutcomeDto::Presented {
                hint_id: hint_id.0.to_string(),
                presentation: (&presentation).into(),
                effects: HintEffectsDto::from_effects(effects),
            },
            NextHintOutcome::Unsupported {
                hint_id,
                unsupported,
                effects,
            } => NextHintOutcomeDto::Unsupported {
                hint_id: hint_id.0.to_string(),
                unsupported: unsupported.into(),
                effects: HintEffectsDto::from_effects(effects),
            },
            NextHintOutcome::None => NextHintOutcomeDto::None,
            NextHintOutcome::Incomplete { gap } => {
                NextHintOutcomeDto::Incomplete { gap: gap.into() }
            }
        };
        Ok(ResponseKindDto::NextHint {
            revision: response.revision.to_string(),
            outcome,
        })
    }

    fn get_all_hints(
        &mut self,
        revision: &str,
        include_expensive: bool,
    ) -> Result<ResponseKindDto, ErrorDto> {
        let session = self.session_mut()?;
        let expected_revision = parse_decimal_u64(revision, "expected_revision", false)?;
        require_revision(session, expected_revision)?;
        let response = session.all_hints(include_expensive);
        let outcome = match response.outcome {
            AllHintsOutcome::Complete { hints } => AllHintsOutcomeDto::Complete {
                hints: hints.into_iter().map(Into::into).collect(),
            },
            AllHintsOutcome::ConfirmationRequired => AllHintsOutcomeDto::ConfirmationRequired,
            AllHintsOutcome::Incomplete { hints, gap } => AllHintsOutcomeDto::Incomplete {
                hints: hints.into_iter().map(Into::into).collect(),
                gap: gap.into(),
            },
        };
        Ok(ResponseKindDto::AllHints {
            revision: response.revision.to_string(),
            outcome,
        })
    }

    fn get_hint(&mut self, revision: &str, hint_id: &str) -> Result<ResponseKindDto, ErrorDto> {
        let session = self.session_mut()?;
        let expected_revision = parse_decimal_u64(revision, "expected_revision", false)?;
        let hint_id = parse_decimal_u64(hint_id, "hint_id", true)?;
        let response = session
            .hint(expected_revision, HintId(hint_id))
            .map_err(ErrorDto::from)?;
        let outcome = match response.outcome {
            MaterializedHintOutcome::Presented {
                presentation,
                effects,
            } => MaterializedHintOutcomeDto::Presented {
                presentation: (&presentation).into(),
                effects: HintEffectsDto::from_effects(effects),
            },
            MaterializedHintOutcome::Unsupported {
                unsupported,
                effects,
            } => MaterializedHintOutcomeDto::Unsupported {
                unsupported: unsupported.into(),
                effects: HintEffectsDto::from_effects(effects),
            },
            MaterializedHintOutcome::Incomplete { gap, effects } => {
                MaterializedHintOutcomeDto::Incomplete {
                    gap: gap.into(),
                    effects: HintEffectsDto::from_effects(effects),
                }
            }
        };
        Ok(ResponseKindDto::Hint {
            revision: response.revision.to_string(),
            hint_id: response.hint_id.0.to_string(),
            outcome,
        })
    }

    fn apply_hint(&mut self, revision: &str, hint_id: &str) -> Result<ResponseKindDto, ErrorDto> {
        let session = self.session_mut()?;
        let expected_revision = parse_decimal_u64(revision, "expected_revision", false)?;
        let hint_id = parse_decimal_u64(hint_id, "hint_id", true)?;
        let snapshot = session
            .apply_hint(expected_revision, HintId(hint_id))
            .map_err(ErrorDto::from)?;
        Ok(ResponseKindDto::Snapshot {
            snapshot: snapshot.into(),
        })
    }

    fn place_value(
        &mut self,
        revision: &str,
        raw_cell: u8,
        raw_digit: u8,
    ) -> Result<ResponseKindDto, ErrorDto> {
        let session = self.session_mut()?;
        let expected_revision = parse_decimal_u64(revision, "expected_revision", false)?;
        let cell = parse_cell(raw_cell)?;
        let digit = parse_digit(raw_digit)?;
        let snapshot = session
            .place_value(expected_revision, cell, digit)
            .map_err(ErrorDto::from)?;
        Ok(ResponseKindDto::Snapshot {
            snapshot: snapshot.into(),
        })
    }

    fn toggle_candidate(
        &mut self,
        revision: &str,
        raw_cell: u8,
        raw_digit: u8,
    ) -> Result<ResponseKindDto, ErrorDto> {
        let session = self.session_mut()?;
        let expected_revision = parse_decimal_u64(revision, "expected_revision", false)?;
        let cell = parse_cell(raw_cell)?;
        let digit = parse_digit(raw_digit)?;
        let snapshot = session
            .toggle_candidate(expected_revision, cell, digit)
            .map_err(ErrorDto::from)?;
        Ok(ResponseKindDto::Snapshot {
            snapshot: snapshot.into(),
        })
    }

    fn undo(&mut self, revision: &str) -> Result<ResponseKindDto, ErrorDto> {
        let session = self.session_mut()?;
        let expected_revision = parse_decimal_u64(revision, "expected_revision", false)?;
        let snapshot = session.undo(expected_revision).map_err(ErrorDto::from)?;
        Ok(ResponseKindDto::Snapshot {
            snapshot: snapshot.into(),
        })
    }

    fn redo(&mut self, revision: &str) -> Result<ResponseKindDto, ErrorDto> {
        let session = self.session_mut()?;
        let expected_revision = parse_decimal_u64(revision, "expected_revision", false)?;
        let snapshot = session.redo(expected_revision).map_err(ErrorDto::from)?;
        Ok(ResponseKindDto::Snapshot {
            snapshot: snapshot.into(),
        })
    }

    fn session_mut(&mut self) -> Result<&mut Session, ErrorDto> {
        self.session.as_mut().ok_or_else(|| {
            ErrorDto::new(
                "session_not_initialized",
                "create_session must succeed before this command",
            )
        })
    }
}

fn parse_decimal_u64(value: &str, field: &str, nonzero: bool) -> Result<u64, ErrorDto> {
    let canonical = value == "0"
        || value
            .strip_prefix(|character: char| matches!(character, '1'..='9'))
            .is_some_and(|rest| rest.bytes().all(|byte| byte.is_ascii_digit()));
    let parsed = canonical.then(|| value.parse::<u64>().ok()).flatten();
    if let Some(parsed) = parsed.filter(|parsed| !nonzero || *parsed != 0) {
        return Ok(parsed);
    }
    let code = if field == "hint_id" {
        "invalid_hint_id"
    } else {
        "invalid_revision"
    };
    Err(ErrorDto::new(
        code,
        format!(
            "{field} must be a canonical {}unsigned 64-bit decimal string",
            if nonzero { "non-zero " } else { "" }
        ),
    ))
}

fn parse_cell(value: u8) -> Result<CellId, ErrorDto> {
    CellId::new(value).ok_or_else(|| ErrorDto::new("invalid_cell", "cell must be between 0 and 80"))
}

fn parse_digit(value: u8) -> Result<Digit, ErrorDto> {
    Digit::new(value).ok_or_else(|| ErrorDto::new("invalid_digit", "digit must be between 1 and 9"))
}

fn require_revision(session: &Session, expected: u64) -> Result<(), ErrorDto> {
    let actual = session.revision();
    if expected == actual {
        Ok(())
    } else {
        Err(ErrorDto::stale(expected, actual))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{ApplicationPort, PROTOCOL_VERSION, RequestDto, parse_decimal_u64};

    fn dispatch(port: &mut ApplicationPort, request: Value) -> Value {
        serde_json::from_str(&port.dispatch_json(&request.to_string())).unwrap()
    }

    fn create_request(request_id: u32, puzzle: &str) -> Value {
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "request_id": request_id,
            "command": "create_session",
            "puzzle": puzzle
        })
    }

    #[test]
    fn request_and_error_json_shapes_are_frozen() {
        let request = json!({
            "protocol_version": PROTOCOL_VERSION,
            "request_id": 12,
            "command": "apply_hint",
            "expected_revision": "9007199254740993",
            "hint_id": "18446744073709551615"
        });
        let parsed: RequestDto = serde_json::from_value(request.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), request);
        assert!(
            serde_json::from_value::<RequestDto>(json!({
                "protocol_version": PROTOCOL_VERSION,
                "request_id": 12,
                "command": "undo",
                "expected_revision": "0",
                "unexpected": true
            }))
            .is_err()
        );

        let mut port = ApplicationPort::new();
        let response = port.dispatch_json(
            &json!({
                "protocol_version": PROTOCOL_VERSION,
                "request_id": 12,
                "command": "undo",
                "expected_revision": "0"
            })
            .to_string(),
        );
        assert_eq!(
            response,
            format!(
                "{{\"protocol_version\":{PROTOCOL_VERSION},\"request_id\":12,\"response\":\"error\",\"error\":{{\"code\":\"session_not_initialized\",\"message\":\"create_session must succeed before this command\"}}}}"
            )
        );
    }

    #[test]
    fn create_session_returns_exact_classic_snapshot_and_ordered_topology() {
        let mut port = ApplicationPort::new();
        let response = dispatch(&mut port, create_request(1, &".".repeat(81)));

        assert_eq!(response["protocol_version"], PROTOCOL_VERSION);
        assert_eq!(response["request_id"], 1);
        assert_eq!(response["response"], "session_created");
        assert_eq!(response["snapshot"]["revision"], "0");
        assert_eq!(response["snapshot"]["values"].as_array().unwrap().len(), 81);
        assert_eq!(
            response["snapshot"]["candidate_masks"]
                .as_array()
                .unwrap()
                .len(),
            81
        );
        assert!(
            response["snapshot"]["candidate_masks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|mask| mask == 0x03fe)
        );
        assert_eq!(response["snapshot"]["givens"].as_array().unwrap().len(), 81);
        assert_eq!(response["snapshot"]["can_undo"], false);
        assert_eq!(response["snapshot"]["can_redo"], false);

        let regions = response["topology"]["regions"].as_array().unwrap();
        assert_eq!(regions.len(), 27);
        assert_eq!(
            regions[0],
            json!({
                "region_type": 0,
                "region_index": 0,
                "family_key": "block",
                "label": "Block 1",
                "cells": [0, 1, 2, 9, 10, 11, 18, 19, 20]
            })
        );
        assert_eq!(regions[9]["family_key"], "row");
        assert_eq!(regions[18]["family_key"], "column");
        assert_eq!(response["topology"]["variant"]["blocks"], true);
        assert_eq!(response["topology"]["variant"]["non_consecutive"], "off");
    }

    #[test]
    fn configured_regions_keep_fixed_type_family_mapping_and_labels() {
        let mut port = ApplicationPort::new();
        let response = dispatch(
            &mut port,
            json!({
                "protocol_version": PROTOCOL_VERSION,
                "request_id": 2,
                "command": "create_session",
                "puzzle": ".".repeat(81),
                "variant": {
                    "blocks": true,
                    "disjoint_groups": true,
                    "windows": true,
                    "sudoku_x": true,
                    "girandola": true,
                    "asterisk": true,
                    "center_dot": true,
                    "non_consecutive": "orthogonal_cyclic",
                    "forbidden_pairs": true
                }
            }),
        );
        let regions = response["topology"]["regions"].as_array().unwrap();
        for (region_type, family_key) in [
            (0, "block"),
            (1, "row"),
            (2, "column"),
            (3, "disjoint_group"),
            (4, "window"),
            (5, "main_diagonal"),
            (6, "anti_diagonal"),
            (7, "girandola"),
            (8, "asterisk"),
            (9, "center_dot"),
        ] {
            assert!(regions.iter().any(|region| {
                region["region_type"] == region_type && region["family_key"] == family_key
            }));
        }
        assert!(
            regions
                .iter()
                .any(|region| { region["region_type"] == 6 && region["label"] == "Anti-diagonal" })
        );
        assert_eq!(
            response["topology"]["variant"]["non_consecutive"],
            "orthogonal_cyclic"
        );
        assert_eq!(response["topology"]["variant"]["forbidden_pairs"], true);
    }

    #[test]
    fn presented_hint_reuses_opaque_id_and_server_apply_returns_snapshot() {
        let mut port = ApplicationPort::new();
        dispatch(
            &mut port,
            create_request(
                1,
                "12345678.........................................................................",
            ),
        );
        let next = json!({
            "protocol_version": PROTOCOL_VERSION,
            "request_id": 2,
            "command": "next_hint",
            "expected_revision": "0"
        });
        let first = dispatch(&mut port, next.clone());
        let second = dispatch(&mut port, next);

        assert_eq!(first["response"], "next_hint");
        assert_eq!(first["revision"], "0");
        assert_eq!(first["outcome"], "presented");
        assert_eq!(first["hint_id"], "1");
        assert_eq!(second["hint_id"], "1");
        assert_eq!(
            first["presentation"]["identity"]["technique_key"],
            "hidden_single"
        );
        assert_eq!(
            first["effects"],
            json!({
                "placement": {"cell": 8, "digit": 9},
                "removals": [],
                "elimination_count": 0
            })
        );

        let applied = dispatch(
            &mut port,
            json!({
                "protocol_version": PROTOCOL_VERSION,
                "request_id": 3,
                "command": "apply_hint",
                "expected_revision": "0",
                "hint_id": "1"
            }),
        );
        assert_eq!(applied["response"], "snapshot");
        assert_eq!(applied["snapshot"]["revision"], "1");
        assert_eq!(applied["snapshot"]["values"][8], 9);
        assert_eq!(applied["snapshot"]["can_undo"], true);
    }

    #[test]
    fn all_hints_catalog_materializes_and_applies_a_selected_opaque_id() {
        let mut port = ApplicationPort::new();
        let puzzle = format!("12345678.45678912.{}", ".".repeat(63));
        dispatch(&mut port, create_request(1, &puzzle));

        let catalog = dispatch(
            &mut port,
            json!({
                "protocol_version": PROTOCOL_VERSION,
                "request_id": 2,
                "command": "get_all_hints",
                "expected_revision": "0"
            }),
        );
        assert_eq!(catalog["response"], "all_hints");
        assert_eq!(catalog["revision"], "0");
        assert_eq!(catalog["outcome"], "complete");
        let hints = catalog["hints"].as_array().unwrap();
        assert!(hints.len() >= 2);
        assert_eq!(hints[0]["category"], "direct");
        assert!(hints[0]["identity"]["technique_key"].is_string());
        assert!(hints[0]["effects"]["placement"].is_object());
        assert!(hints[0]["filter_effects"]["placement"].is_object());
        let selected_id = hints[1]["hint_id"].as_str().unwrap().to_owned();

        let detail = dispatch(
            &mut port,
            json!({
                "protocol_version": PROTOCOL_VERSION,
                "request_id": 3,
                "command": "get_hint",
                "expected_revision": "0",
                "hint_id": selected_id
            }),
        );
        assert_eq!(detail["response"], "hint");
        assert_eq!(detail["outcome"], "presented");
        assert_eq!(detail["hint_id"], selected_id);

        let applied = dispatch(
            &mut port,
            json!({
                "protocol_version": PROTOCOL_VERSION,
                "request_id": 4,
                "command": "apply_hint",
                "expected_revision": "0",
                "hint_id": selected_id
            }),
        );
        assert_eq!(applied["response"], "snapshot");
        assert_eq!(applied["snapshot"]["revision"], "1");

        let stale_detail = dispatch(
            &mut port,
            json!({
                "protocol_version": PROTOCOL_VERSION,
                "request_id": 5,
                "command": "get_hint",
                "expected_revision": "1",
                "hint_id": selected_id
            }),
        );
        assert_eq!(stale_detail["response"], "error");
        assert_eq!(stale_detail["error"]["code"], "unknown_hint");
    }

    #[test]
    fn invalid_values_and_stale_revisions_are_typed_errors_without_mutation() {
        let mut port = ApplicationPort::new();
        let invalid = dispatch(&mut port, create_request(8, "..."));
        assert_eq!(invalid["error"]["code"], "invalid_puzzle");

        dispatch(&mut port, create_request(9, &".".repeat(81)));
        let invalid_cell = dispatch(
            &mut port,
            json!({
                "protocol_version": PROTOCOL_VERSION,
                "request_id": 10,
                "command": "place_value",
                "expected_revision": "0",
                "cell": 81,
                "digit": 5
            }),
        );
        assert_eq!(invalid_cell["error"]["code"], "invalid_cell");

        let invalid_revision = dispatch(
            &mut port,
            json!({
                "protocol_version": PROTOCOL_VERSION,
                "request_id": 11,
                "command": "undo",
                "expected_revision": "01"
            }),
        );
        assert_eq!(invalid_revision["error"]["code"], "invalid_revision");

        let stale = dispatch(
            &mut port,
            json!({
                "protocol_version": PROTOCOL_VERSION,
                "request_id": 12,
                "command": "undo",
                "expected_revision": "7"
            }),
        );
        assert_eq!(stale["error"]["code"], "stale_revision");
        assert_eq!(stale["error"]["expected_revision"], "7");
        assert_eq!(stale["error"]["actual_revision"], "0");
    }

    #[test]
    fn decimal_identifiers_are_canonical_bounded_and_hint_ids_are_nonzero() {
        assert_eq!(parse_decimal_u64("0", "expected_revision", false), Ok(0));
        assert_eq!(
            parse_decimal_u64("18446744073709551615", "expected_revision", false),
            Ok(u64::MAX)
        );
        for invalid in ["", "00", "01", "+1", "-1", "18446744073709551616"] {
            assert_eq!(
                parse_decimal_u64(invalid, "expected_revision", false)
                    .unwrap_err()
                    .code,
                "invalid_revision"
            );
        }
        assert_eq!(
            parse_decimal_u64("0", "hint_id", true).unwrap_err().code,
            "invalid_hint_id"
        );
        assert_eq!(parse_decimal_u64("1", "hint_id", true), Ok(1));
    }
}
