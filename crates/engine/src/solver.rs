use core::fmt;

use sukaku_forge_core::{
    CellId, CellMask, Digit, Grid, NonConsecutiveMode, PositionMask, RegionId,
};

use crate::SelectedChainProof;
use crate::nested_chains::{EffectKey, chaining_effect_key};
use crate::{
    EngineConfig, Evidence, Inference, LegacyFcPlusBoundary, NonConsecutiveGeometry,
    NonConsecutiveHint, NonConsecutiveHintKind, Rating, RatingMode, RatingResult, RatingTracker,
    SearchPolicy, Technique, TechniqueGate, collect_aligned_pair_exclusions,
    collect_aligned_triplet_exclusions, collect_alphabet_wing, collect_bivalue_universal_grave,
    collect_direct_hidden_sets, collect_direct_locking, collect_dynamic_forcing_chain_plus_checked,
    collect_dynamic_forcing_chains, collect_fish, collect_forcing_cell_ferz_non_consecutive,
    collect_forcing_cell_non_consecutive, collect_forcing_chain_cycles, collect_four_strong_links,
    collect_generalized_intersections, collect_hidden_sets, collect_hidden_singles,
    collect_locked_ferz_non_consecutive, collect_locked_non_consecutive, collect_locking,
    collect_multiple_forcing_chains, collect_naked_sets, collect_naked_singles,
    collect_nested_forcing_chains_checked, collect_nishio_forcing_chains,
    collect_three_strong_links, collect_two_strong_links, collect_unique_loop, collect_wings,
    find_aligned_pair_exclusion, find_aligned_triplet_exclusion, find_alphabet_wing,
    find_bivalue_universal_grave, find_direct_hidden_set, find_direct_locking,
    find_dynamic_forcing_chain, find_dynamic_forcing_chain_plus_checked,
    find_dynamic_forcing_chain_plus_with_proof_checked, find_dynamic_forcing_chain_with_proof,
    find_fish, find_forcing_cell_ferz_non_consecutive, find_forcing_cell_non_consecutive,
    find_forcing_chain_cycle, find_forcing_chain_cycle_with_proof, find_four_strong_links,
    find_generalized_intersections, find_hidden_set, find_hidden_single,
    find_locked_ferz_non_consecutive, find_locked_non_consecutive, find_locking,
    find_multiple_forcing_chain, find_multiple_forcing_chain_with_proof, find_naked_set,
    find_naked_single, find_nested_forcing_chain_checked,
    find_nested_forcing_chain_with_proof_checked, find_nishio_forcing_chain,
    find_nishio_forcing_chain_with_proof, find_three_strong_links, find_two_strong_links,
    find_unique_loop, find_wing, replay_dynamic_forcing_chain_plus_proof,
    replay_dynamic_forcing_chain_proof, replay_forcing_chain_cycle_proof,
    replay_multiple_forcing_chain_proof, replay_nested_forcing_chain_proof_checked,
    replay_nishio_forcing_chain_proof,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProducerKind {
    HiddenSingle,
    NakedSingle,
    DirectLocking,
    DirectHiddenSet { degree: u8 },
    Locking,
    GeneralizedIntersections,
    NakedSet { degree: u8, generalized: bool },
    Fish { degree: u8 },
    HiddenSet { degree: u8 },
    TurbotFish,
    XYWing,
    XYZWing,
    UniqueLoops,
    StrongLinks { degree: u8 },
    WXYZWing,
    BivalueUniversalGrave,
    VWXYZWing,
    AlignedPairExclusion,
    UVWXYZWing,
    ForcingChainCycle,
    TUVWXYZWing,
    AlignedTripletExclusion,
    NishioForcingChain,
    MultipleForcingChain,
    DynamicForcingChain,
    DynamicForcingChainPlus,
    NestedForcingChain { level: u8, nesting_limit: u8 },
    ForcingCellNonConsecutive,
    LockedNonConsecutive,
    ForcingCellFerzNonConsecutive,
    LockedFerzNonConsecutive,
}

impl ProducerKind {
    /// Whether this producer publishes a Java `ChainingHint` subtype.
    #[must_use]
    pub const fn is_chaining_hint(self) -> bool {
        matches!(
            self,
            Self::ForcingChainCycle
                | Self::NishioForcingChain
                | Self::MultipleForcingChain
                | Self::DynamicForcingChain
                | Self::DynamicForcingChainPlus
                | Self::NestedForcingChain { .. }
        )
    }

    /// Stable identity of the legacy rule node that produced this hint.
    #[must_use]
    pub fn hint_group_key(self, inference: &Inference) -> &'static str {
        match self {
            Self::HiddenSingle => "hidden_single",
            Self::NakedSingle => "naked_single",
            Self::DirectLocking => "direct_intersections",
            Self::DirectHiddenSet { degree: 2 } => "direct_hidden_pair",
            Self::DirectHiddenSet { degree: 3 } => "direct_hidden_triplet",
            Self::DirectHiddenSet { .. } => "direct_hidden_set",
            Self::Locking => "intersections",
            Self::GeneralizedIntersections => "generalized_intersections",
            Self::NakedSet {
                degree: 2,
                generalized: false,
            } => "naked_pair",
            Self::NakedSet {
                degree: 2,
                generalized: true,
            } => "generalized_naked_pair",
            Self::NakedSet {
                degree: 3,
                generalized: false,
            } => "naked_triplet",
            Self::NakedSet {
                degree: 3,
                generalized: true,
            } => "generalized_naked_triplet",
            Self::NakedSet {
                degree: 4,
                generalized: false,
            } => "naked_quad",
            Self::NakedSet {
                degree: 4,
                generalized: true,
            } => "generalized_naked_quad",
            Self::NakedSet { .. } => "naked_set",
            Self::Fish { degree: 2 } => "x_wing",
            Self::Fish { degree: 3 } => "swordfish",
            Self::Fish { degree: 4 } => "jellyfish",
            Self::Fish { .. } => "fish",
            Self::HiddenSet { degree: 2 } => "hidden_pair",
            Self::HiddenSet { degree: 3 } => "hidden_triplet",
            Self::HiddenSet { degree: 4 } => "hidden_quad",
            Self::HiddenSet { .. } => "hidden_set",
            Self::TurbotFish => match inference.evidence() {
                Evidence::TwoStrongLinks {
                    rating_mode: RatingMode::Original,
                    ..
                } => "two_strong_links",
                Evidence::TwoStrongLinks { .. } => "turbot_fishes",
                _ => "two_strong_links",
            },
            Self::XYWing | Self::XYZWing => "xy_wings",
            Self::UniqueLoops | Self::BivalueUniversalGrave => "unique_patterns",
            Self::StrongLinks { degree: 3 } => "three_strong_links",
            Self::StrongLinks { degree: 4 } => "four_strong_links",
            Self::StrongLinks { .. } => "strong_links",
            Self::WXYZWing => "wxyz_wing",
            Self::VWXYZWing => "vwxyz_wing",
            Self::AlignedPairExclusion => "aligned_pair_exclusion",
            Self::UVWXYZWing => "uvwxyz_wing",
            Self::ForcingChainCycle => "forcing_chains_cycles",
            Self::TUVWXYZWing => "tuvwxyz_wing",
            Self::AlignedTripletExclusion => "aligned_triplet_exclusion",
            Self::NishioForcingChain => "nishio_forcing_chains",
            Self::MultipleForcingChain => "multiple_forcing_chains",
            Self::DynamicForcingChain => "dynamic_forcing_chains",
            Self::DynamicForcingChainPlus => "dynamic_forcing_chains_plus",
            Self::NestedForcingChain { level: 2, .. } => "nested_forcing_chains_l2",
            Self::NestedForcingChain { level: 3, .. } => "nested_forcing_chains_l3",
            Self::NestedForcingChain { level: 4, .. } => "nested_forcing_chains_l4",
            Self::NestedForcingChain { .. } => "nested_forcing_chains",
            Self::ForcingCellNonConsecutive | Self::ForcingCellFerzNonConsecutive => {
                "non_consecutive_forcing_cell"
            }
            Self::LockedNonConsecutive | Self::LockedFerzNonConsecutive => "locked_non_consecutive",
        }
    }

    /// Display label of the legacy rule tree node.
    #[must_use]
    pub fn hint_group_name(self, inference: &Inference) -> &'static str {
        match self {
            Self::DirectLocking => "Direct Intersections",
            Self::Locking => "Intersections",
            Self::TurbotFish => match inference.evidence() {
                Evidence::TwoStrongLinks {
                    rating_mode: RatingMode::Original,
                    ..
                } => "2 Strong links",
                Evidence::TwoStrongLinks { .. } => "Turbot Fishes",
                _ => "2 Strong links",
            },
            Self::XYWing | Self::XYZWing => "XY-Wings",
            Self::UniqueLoops | Self::BivalueUniversalGrave => "Unique patterns",
            Self::ForcingChainCycle => "Forcing Chains & Cycles",
            Self::NishioForcingChain => "Nishio Forcing Chains",
            Self::MultipleForcingChain => "Multiple Forcing Chains",
            Self::DynamicForcingChain => "Dynamic Forcing Chains",
            Self::DynamicForcingChainPlus => "Dynamic Forcing Chains (+)",
            Self::NestedForcingChain { level: 2, .. } => {
                "Dynamic Forcing Chains (+ Forcing Chains)"
            }
            Self::NestedForcingChain { level: 3, .. } => {
                "Dynamic Forcing Chains (+ Multiple Forcing Chains)"
            }
            Self::NestedForcingChain { level: 4, .. } => {
                "Dynamic Forcing Chains (+ Dynamic Forcing Chains)"
            }
            Self::NestedForcingChain { .. } => "Nested Forcing Chains",
            Self::ForcingCellNonConsecutive | Self::ForcingCellFerzNonConsecutive => {
                "NC Forcing Cell"
            }
            Self::LockedNonConsecutive | Self::LockedFerzNonConsecutive => "Locked NC",
            _ => self.default_hint_group_name(),
        }
    }

    const fn default_hint_group_name(self) -> &'static str {
        match self {
            Self::HiddenSingle => "Hidden Single",
            Self::NakedSingle => "Naked Single",
            Self::DirectHiddenSet { degree: 2 } => "Direct Hidden Pair",
            Self::DirectHiddenSet { degree: 3 } => "Direct Hidden Triplet",
            Self::DirectHiddenSet { .. } => "Direct Hidden Set",
            Self::GeneralizedIntersections => "Generalized Intersections",
            Self::NakedSet {
                degree: 2,
                generalized: false,
            } => "Naked Pair",
            Self::NakedSet {
                degree: 2,
                generalized: true,
            } => "Generalized Naked Pair",
            Self::NakedSet {
                degree: 3,
                generalized: false,
            } => "Naked Triplet",
            Self::NakedSet {
                degree: 3,
                generalized: true,
            } => "Generalized Naked Triplet",
            Self::NakedSet {
                degree: 4,
                generalized: false,
            } => "Naked Quad",
            Self::NakedSet {
                degree: 4,
                generalized: true,
            } => "Generalized Naked Quad",
            Self::NakedSet { .. } => "Naked Set",
            Self::Fish { degree: 2 } => "X-Wing",
            Self::Fish { degree: 3 } => "Swordfish",
            Self::Fish { degree: 4 } => "Jellyfish",
            Self::Fish { .. } => "Fish",
            Self::HiddenSet { degree: 2 } => "Hidden Pair",
            Self::HiddenSet { degree: 3 } => "Hidden Triplet",
            Self::HiddenSet { degree: 4 } => "Hidden Quad",
            Self::HiddenSet { .. } => "Hidden Set",
            Self::TurbotFish => "Turbot Fish",
            Self::XYWing | Self::XYZWing => "XY-Wings",
            Self::StrongLinks { degree: 3 } => "3 Strong links",
            Self::StrongLinks { degree: 4 } => "4 Strong links",
            Self::StrongLinks { .. } => "Strong links",
            Self::WXYZWing => "WXYZ-Wing",
            Self::VWXYZWing => "VWXYZ-Wing",
            Self::AlignedPairExclusion => "Aligned Pair Exclusion",
            Self::UVWXYZWing => "UVWXYZ-Wing",
            Self::TUVWXYZWing => "TUVWXYZ-Wing",
            Self::AlignedTripletExclusion => "Aligned Triplet Exclusion",
            Self::ForcingCellNonConsecutive
            | Self::LockedNonConsecutive
            | Self::ForcingCellFerzNonConsecutive
            | Self::LockedFerzNonConsecutive
            | Self::BivalueUniversalGrave => unreachable!(),
            Self::DirectLocking
            | Self::Locking
            | Self::UniqueLoops
            | Self::ForcingChainCycle
            | Self::NishioForcingChain
            | Self::MultipleForcingChain
            | Self::DynamicForcingChain
            | Self::DynamicForcingChainPlus
            | Self::NestedForcingChain { .. } => unreachable!(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProducerBand {
    Direct,
    Indirect,
    Chaining,
    ChainingPlus,
    Advanced,
}

const fn producer_band(kind: ProducerKind) -> ProducerBand {
    match kind {
        ProducerKind::HiddenSingle
        | ProducerKind::NakedSingle
        | ProducerKind::DirectLocking
        | ProducerKind::DirectHiddenSet { .. }
        | ProducerKind::ForcingCellNonConsecutive
        | ProducerKind::LockedNonConsecutive
        | ProducerKind::ForcingCellFerzNonConsecutive
        | ProducerKind::LockedFerzNonConsecutive => ProducerBand::Direct,
        ProducerKind::ForcingChainCycle
        | ProducerKind::TUVWXYZWing
        | ProducerKind::AlignedTripletExclusion
        | ProducerKind::NishioForcingChain
        | ProducerKind::MultipleForcingChain
        | ProducerKind::DynamicForcingChain => ProducerBand::Chaining,
        ProducerKind::DynamicForcingChainPlus => ProducerBand::ChainingPlus,
        ProducerKind::NestedForcingChain { .. } => ProducerBand::Advanced,
        _ => ProducerBand::Indirect,
    }
}

// Swing's HintsTreeBuilder groups by concrete Java hint subtype, not by the
// producer tier used by Solver.getAllHints. Direct hidden sets, direct locking,
// and non-consecutive direct producers all publish IndirectHint instances.
const fn hint_category(kind: ProducerKind) -> HintCategory {
    match kind {
        ProducerKind::HiddenSingle | ProducerKind::NakedSingle => HintCategory::Direct,
        _ => HintCategory::Indirect,
    }
}

const fn is_chaining_hint(kind: ProducerKind) -> bool {
    kind.is_chaining_hint()
}

const fn is_locking_hint(kind: ProducerKind) -> bool {
    matches!(kind, ProducerKind::Locking | ProducerKind::Fish { .. })
}

const fn is_wing_hint(kind: ProducerKind) -> bool {
    matches!(kind, ProducerKind::XYWing | ProducerKind::XYZWing)
}

const fn is_aligned_exclusion_hint(kind: ProducerKind) -> bool {
    matches!(
        kind,
        ProducerKind::AlignedPairExclusion | ProducerKind::AlignedTripletExclusion
    )
}

fn region_position_cells(grid: &Grid, region: RegionId, positions: PositionMask) -> CellMask {
    let mut cells = CellMask::EMPTY;
    for position in positions.iter() {
        let raw = grid.topology().region_cells(region)[usize::from(position)];
        cells.insert(CellId::new(raw).expect("topology stores valid cell indexes"));
    }
    cells
}

fn direct_locking_key(grid: &Grid, inference: &Inference) -> Option<(Digit, CellMask)> {
    let Evidence::DirectLocking {
        secondary,
        pattern_positions,
        ..
    } = inference.evidence()
    else {
        return None;
    };
    Some((
        inference.placement_digit()?,
        region_position_cells(grid, secondary, pattern_positions),
    ))
}

fn locking_key(grid: &Grid, inference: &Inference) -> Option<(Digit, CellMask)> {
    match inference.evidence() {
        Evidence::Locking {
            secondary,
            digit,
            pattern_positions,
            ..
        } => Some((
            digit,
            region_position_cells(grid, secondary, pattern_positions),
        )),
        Evidence::Fish {
            digit,
            selected_cells,
            ..
        } => {
            let mut cells = CellMask::EMPTY;
            for cell in selected_cells.iter() {
                cells.insert(cell);
            }
            Some((digit, cells))
        }
        _ => None,
    }
}

fn wing_key(kind: ProducerKind, inference: &Inference) -> Option<(bool, CellId, Digit, CellMask)> {
    let Evidence::Wing {
        pivot,
        xz,
        yz,
        digit,
    } = inference.evidence()
    else {
        return None;
    };
    let mut pincers = CellMask::EMPTY;
    pincers.insert(xz);
    pincers.insert(yz);
    Some((kind == ProducerKind::XYZWing, pivot, digit, pincers))
}

fn aligned_exclusion_key(inference: &Inference) -> Option<(CellMask, EffectKey)> {
    let mut cells = CellMask::EMPTY;
    match inference.evidence() {
        Evidence::AlignedPairExclusion {
            cells: base_cells, ..
        } => {
            for cell in base_cells {
                cells.insert(cell);
            }
        }
        Evidence::AlignedTripletExclusion {
            cells: base_cells, ..
        } => {
            for cell in base_cells {
                cells.insert(cell);
            }
        }
        _ => return None,
    }
    Some((cells, EffectKey::new(inference.removals())))
}

fn non_consecutive_forcing_key(inference: &Inference) -> Option<(NonConsecutiveGeometry, CellId)> {
    match inference.evidence() {
        Evidence::NonConsecutive {
            geometry,
            kind: NonConsecutiveHintKind::ForcingCell { cell, .. },
        } => Some((geometry, cell)),
        _ => None,
    }
}

/// Released `DefaultHintsAccumulator` calls `candidate.equals(existing)`.
/// This models only equality families that cross producer calls; concrete
/// collector-local equality and its historical quirks remain in each producer.
fn java_hint_equals(
    grid: &Grid,
    candidate_kind: ProducerKind,
    candidate: &Inference,
    existing: &CollectedInference,
) -> bool {
    let existing_kind = existing.producer;
    let existing_inference = &existing.inference;

    match candidate_kind {
        ProducerKind::HiddenSingle | ProducerKind::NakedSingle => {
            candidate_kind == existing_kind
                && candidate.placement_cell() == existing_inference.placement_cell()
                && candidate.placement_digit() == existing_inference.placement_digit()
        }
        ProducerKind::DirectLocking => {
            existing_kind == ProducerKind::DirectLocking
                && direct_locking_key(grid, candidate)
                    == direct_locking_key(grid, existing_inference)
        }
        kind if is_locking_hint(kind) => {
            is_locking_hint(existing_kind)
                && locking_key(grid, candidate) == locking_key(grid, existing_inference)
        }
        kind if is_wing_hint(kind) => {
            is_wing_hint(existing_kind)
                && wing_key(kind, candidate) == wing_key(existing_kind, existing_inference)
        }
        kind if is_aligned_exclusion_hint(kind) => {
            is_aligned_exclusion_hint(existing_kind)
                && aligned_exclusion_key(candidate) == aligned_exclusion_key(existing_inference)
        }
        kind if is_chaining_hint(kind) => {
            is_chaining_hint(existing_kind)
                && chaining_effect_key(grid, candidate)
                    == chaining_effect_key(grid, existing_inference)
        }
        ProducerKind::ForcingCellNonConsecutive | ProducerKind::ForcingCellFerzNonConsecutive => {
            matches!(
                existing_kind,
                ProducerKind::ForcingCellNonConsecutive
                    | ProducerKind::ForcingCellFerzNonConsecutive
            ) && non_consecutive_forcing_key(candidate)
                == non_consecutive_forcing_key(existing_inference)
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProducerState {
    Ported,
    Unported,
    KnownEmpty,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProducerSpec {
    kind: ProducerKind,
    enable_gate: TechniqueGate,
    state: ProducerState,
}

impl ProducerSpec {
    #[must_use]
    pub const fn kind(self) -> ProducerKind {
        self.kind
    }

    #[must_use]
    pub const fn enable_gate(self) -> TechniqueGate {
        self.enable_gate
    }

    #[must_use]
    pub const fn state(self) -> ProducerState {
        self.state
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortGap {
    Producer(ProducerKind),
    IndirectTechniques,
    LegacyFcPlus2(LegacyFcPlusBoundary),
}

impl fmt::Display for PortGap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Producer(kind) => write!(formatter, "unported producer {kind:?}"),
            Self::IndirectTechniques => formatter.write_str("unported indirect-technique group"),
            Self::LegacyFcPlus2(boundary) => write!(
                formatter,
                "legacy Java FCPlus=2 fails at advanced producer {boundary:?}"
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
// Keeping `Found` inline avoids a heap allocation on every successful solver
// step. The larger payload is bounded primitive evidence and returned in place.
#[allow(clippy::large_enum_variant)]
pub enum SearchOutcome {
    Found(Inference),
    None,
    Incomplete(PortGap),
}

/// One inference selected for a GUI client, with optional opt-in proof data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentationInference {
    inference: Inference,
    selected_chain_proof: Option<SelectedChainProof>,
}

impl PresentationInference {
    #[must_use]
    pub const fn inference(&self) -> &Inference {
        &self.inference
    }

    #[must_use]
    pub const fn selected_chain_proof(&self) -> Option<&SelectedChainProof> {
        self.selected_chain_proof.as_ref()
    }

    #[must_use]
    pub fn into_parts(self) -> (Inference, Option<SelectedChainProof>) {
        (self.inference, self.selected_chain_proof)
    }
}

/// Presentation-oriented counterpart of [`SearchOutcome`].
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum PresentationSearchOutcome {
    Found(PresentationInference),
    None,
    Incomplete(PortGap),
}

/// Legacy GUI classification used to build the two solving-hint tree roots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HintCategory {
    Direct,
    Indirect,
}

/// One inference from the opt-in, insertion-ordered all-hints search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectedInference {
    producer: ProducerKind,
    category: HintCategory,
    inference: Inference,
}

impl CollectedInference {
    #[must_use]
    pub const fn producer(&self) -> ProducerKind {
        self.producer
    }

    #[must_use]
    pub const fn category(&self) -> HintCategory {
        self.category
    }

    #[must_use]
    pub const fn inference(&self) -> &Inference {
        &self.inference
    }

    #[must_use]
    pub fn into_inference(self) -> Inference {
        self.inference
    }
}

/// Result of the opt-in legacy `Get all hints` search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AllHintsSearchOutcome {
    Complete(Vec<CollectedInference>),
    ConfirmationRequired,
    Incomplete {
        hints: Vec<CollectedInference>,
        gap: PortGap,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RateOutcome {
    Rated(RatingResult),
    Incomplete { gap: PortGap, partial: RatingResult },
}

/// Exact-order standard solver registry for the currently ported layers.
#[derive(Clone, Debug)]
pub struct Solver {
    config: EngineConfig,
}

impl Solver {
    #[must_use]
    pub const fn new(config: EngineConfig) -> Self {
        Self { config }
    }

    #[must_use]
    pub const fn config(&self) -> EngineConfig {
        self.config
    }

    #[must_use]
    pub fn producer_specs(&self, grid: &Grid) -> Vec<ProducerSpec> {
        match self.config.search_policy {
            SearchPolicy::Compatibility | SearchPolicy::Forge => {
                self.compatibility_producer_specs(grid)
            }
        }
    }

    fn compatibility_producer_specs(&self, grid: &Grid) -> Vec<ProducerSpec> {
        let mut result = Vec::with_capacity(46);
        let add = |result: &mut Vec<ProducerSpec>,
                   kind: ProducerKind,
                   gate: TechniqueGate,
                   state: ProducerState| {
            if self.technique_enabled(grid, gate) {
                result.push(ProducerSpec {
                    kind,
                    enable_gate: gate,
                    state,
                });
            }
        };
        match self.config.rating_mode {
            crate::RatingMode::Revised => {
                add(
                    &mut result,
                    ProducerKind::HiddenSingle,
                    TechniqueGate::HiddenSingle,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::NakedSingle,
                    TechniqueGate::NakedSingle,
                    ProducerState::Ported,
                );
                self.add_non_consecutive_specs(&mut result, grid);
                add(
                    &mut result,
                    ProducerKind::DirectLocking,
                    TechniqueGate::DirectPointing,
                    if grid.topology().config().blocks {
                        ProducerState::Ported
                    } else {
                        ProducerState::KnownEmpty
                    },
                );
                add(
                    &mut result,
                    ProducerKind::DirectHiddenSet { degree: 2 },
                    TechniqueGate::DirectHiddenPair,
                    ProducerState::Ported,
                );
                // Compatibility: Java gates the revised triplet with the pair bit.
                add(
                    &mut result,
                    ProducerKind::DirectHiddenSet { degree: 3 },
                    TechniqueGate::DirectHiddenPair,
                    ProducerState::Ported,
                );
            }
            crate::RatingMode::Original => {
                add(
                    &mut result,
                    ProducerKind::HiddenSingle,
                    TechniqueGate::HiddenSingle,
                    ProducerState::Ported,
                );
                if grid.topology().config().blocks {
                    add(
                        &mut result,
                        ProducerKind::DirectLocking,
                        TechniqueGate::DirectPointing,
                        ProducerState::Ported,
                    );
                }
                add(
                    &mut result,
                    ProducerKind::DirectHiddenSet { degree: 2 },
                    TechniqueGate::DirectHiddenPair,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::NakedSingle,
                    TechniqueGate::NakedSingle,
                    ProducerState::Ported,
                );
                self.add_non_consecutive_specs(&mut result, grid);
                add(
                    &mut result,
                    ProducerKind::DirectHiddenSet { degree: 3 },
                    TechniqueGate::DirectHiddenTriplet,
                    ProducerState::Ported,
                );
            }
        }
        if grid.topology().config().blocks {
            add(
                &mut result,
                ProducerKind::Locking,
                TechniqueGate::PointingClaiming,
                ProducerState::Ported,
            );
        }
        add(
            &mut result,
            ProducerKind::GeneralizedIntersections,
            TechniqueGate::GeneralizedIntersections,
            ProducerState::Ported,
        );
        match self.config.rating_mode {
            crate::RatingMode::Revised => {
                add(
                    &mut result,
                    ProducerKind::HiddenSet { degree: 2 },
                    TechniqueGate::HiddenPair,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::NakedSet {
                        degree: 2,
                        generalized: false,
                    },
                    TechniqueGate::NakedPair,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::NakedSet {
                        degree: 2,
                        generalized: true,
                    },
                    TechniqueGate::GeneralizedNakedPair,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::Fish { degree: 2 },
                    TechniqueGate::XWing,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::NakedSet {
                        degree: 3,
                        generalized: false,
                    },
                    TechniqueGate::NakedTriplet,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::NakedSet {
                        degree: 3,
                        generalized: true,
                    },
                    TechniqueGate::GeneralizedNakedTriplet,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::HiddenSet { degree: 3 },
                    TechniqueGate::HiddenTriplet,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::TurbotFish,
                    TechniqueGate::TurbotFish,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::Fish { degree: 3 },
                    TechniqueGate::Swordfish,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::XYWing,
                    TechniqueGate::XYWing,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::XYZWing,
                    TechniqueGate::XYZWing,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::UniqueLoops,
                    TechniqueGate::UniqueLoop,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::NakedSet {
                        degree: 4,
                        generalized: false,
                    },
                    TechniqueGate::NakedQuad,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::NakedSet {
                        degree: 4,
                        generalized: true,
                    },
                    TechniqueGate::GeneralizedNakedQuad,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::Fish { degree: 4 },
                    TechniqueGate::Jellyfish,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::HiddenSet { degree: 4 },
                    TechniqueGate::HiddenQuad,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::StrongLinks { degree: 3 },
                    TechniqueGate::ThreeStrongLinks,
                    ProducerState::Ported,
                );
            }
            crate::RatingMode::Original => {
                add(
                    &mut result,
                    ProducerKind::NakedSet {
                        degree: 2,
                        generalized: false,
                    },
                    TechniqueGate::NakedPair,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::NakedSet {
                        degree: 2,
                        generalized: true,
                    },
                    TechniqueGate::GeneralizedNakedPair,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::Fish { degree: 2 },
                    TechniqueGate::XWing,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::HiddenSet { degree: 2 },
                    TechniqueGate::HiddenPair,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::NakedSet {
                        degree: 3,
                        generalized: false,
                    },
                    TechniqueGate::NakedTriplet,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::NakedSet {
                        degree: 3,
                        generalized: true,
                    },
                    TechniqueGate::GeneralizedNakedTriplet,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::Fish { degree: 3 },
                    TechniqueGate::Swordfish,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::HiddenSet { degree: 3 },
                    TechniqueGate::HiddenTriplet,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::TurbotFish,
                    TechniqueGate::TurbotFish,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::XYWing,
                    TechniqueGate::XYWing,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::XYZWing,
                    TechniqueGate::XYZWing,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::UniqueLoops,
                    TechniqueGate::UniqueLoop,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::NakedSet {
                        degree: 4,
                        generalized: false,
                    },
                    TechniqueGate::NakedQuad,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::NakedSet {
                        degree: 4,
                        generalized: true,
                    },
                    TechniqueGate::GeneralizedNakedQuad,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::Fish { degree: 4 },
                    TechniqueGate::Jellyfish,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::HiddenSet { degree: 4 },
                    TechniqueGate::HiddenQuad,
                    ProducerState::Ported,
                );
                add(
                    &mut result,
                    ProducerKind::StrongLinks { degree: 3 },
                    TechniqueGate::ThreeStrongLinks,
                    ProducerState::Ported,
                );
            }
        }
        add(
            &mut result,
            ProducerKind::NakedSet {
                degree: 5,
                generalized: true,
            },
            TechniqueGate::GeneralizedNakedQuint,
            ProducerState::Unported,
        );
        add(
            &mut result,
            ProducerKind::WXYZWing,
            TechniqueGate::WXYZWing,
            ProducerState::Ported,
        );
        add(
            &mut result,
            ProducerKind::BivalueUniversalGrave,
            TechniqueGate::BivalueUniversalGrave,
            ProducerState::Ported,
        );
        add(
            &mut result,
            ProducerKind::StrongLinks { degree: 4 },
            TechniqueGate::FourStrongLinks,
            ProducerState::Ported,
        );
        add(
            &mut result,
            ProducerKind::VWXYZWing,
            TechniqueGate::VWXYZWing,
            ProducerState::Ported,
        );
        add(
            &mut result,
            ProducerKind::AlignedPairExclusion,
            TechniqueGate::AlignedPairExclusion,
            ProducerState::Ported,
        );
        add(
            &mut result,
            ProducerKind::StrongLinks { degree: 5 },
            TechniqueGate::FiveStrongLinks,
            ProducerState::Unported,
        );
        add(
            &mut result,
            ProducerKind::NakedSet {
                degree: 6,
                generalized: true,
            },
            TechniqueGate::GeneralizedNakedSext,
            ProducerState::Unported,
        );
        add(
            &mut result,
            ProducerKind::UVWXYZWing,
            TechniqueGate::UVWXYZWing,
            ProducerState::Ported,
        );
        add(
            &mut result,
            ProducerKind::StrongLinks { degree: 6 },
            TechniqueGate::SixStrongLinks,
            ProducerState::Unported,
        );
        add(
            &mut result,
            ProducerKind::ForcingChainCycle,
            TechniqueGate::ForcingChainCycle,
            ProducerState::Ported,
        );
        add(
            &mut result,
            ProducerKind::TUVWXYZWing,
            TechniqueGate::TUVWXYZWing,
            ProducerState::Ported,
        );
        add(
            &mut result,
            ProducerKind::AlignedTripletExclusion,
            TechniqueGate::AlignedTripletExclusion,
            ProducerState::Ported,
        );
        add(
            &mut result,
            ProducerKind::NishioForcingChain,
            TechniqueGate::NishioForcingChain,
            ProducerState::Ported,
        );
        add(
            &mut result,
            ProducerKind::MultipleForcingChain,
            TechniqueGate::MultipleForcingChain,
            ProducerState::Ported,
        );
        add(
            &mut result,
            ProducerKind::DynamicForcingChain,
            TechniqueGate::DynamicForcingChain,
            ProducerState::Ported,
        );
        add(
            &mut result,
            ProducerKind::DynamicForcingChainPlus,
            TechniqueGate::DynamicForcingChainPlus,
            ProducerState::Ported,
        );
        for level in [2_u8, 3] {
            add(
                &mut result,
                ProducerKind::NestedForcingChain {
                    level,
                    nesting_limit: 0,
                },
                TechniqueGate::NestedForcingChain,
                ProducerState::Ported,
            );
        }
        let final_nesting_limit = match self.config.rating_mode {
            crate::RatingMode::Original => 3,
            crate::RatingMode::Revised => 2,
        };
        for nesting_limit in 0..=final_nesting_limit {
            add(
                &mut result,
                ProducerKind::NestedForcingChain {
                    level: 4,
                    nesting_limit,
                },
                TechniqueGate::NestedForcingChain,
                ProducerState::Ported,
            );
        }
        result
    }

    /// Collect the complete legacy GUI hint tier for the current grid.
    ///
    /// This path is deliberately separate from [`Self::next_inference`] and
    /// [`Self::rate`]. It may allocate many inferences, but it never changes
    /// the compact first-hint/rating path. Java gathers every direct and
    /// indirect producer first, enters the ordinary chaining band only when
    /// that result is empty, then DFC+, and finally the first productive
    /// nested producer after an explicit expensive-search confirmation.
    #[must_use]
    pub fn all_inferences(&self, grid: &Grid, include_expensive: bool) -> AllHintsSearchOutcome {
        if grid.is_solved() {
            return AllHintsSearchOutcome::Complete(Vec::new());
        }

        let specs = self.producer_specs(grid);
        let mut hints = Vec::new();

        for band in [ProducerBand::Direct, ProducerBand::Indirect] {
            if let Err(gap) = self.collect_band(grid, &specs, band, &mut hints) {
                return AllHintsSearchOutcome::Incomplete { hints, gap };
            }
        }
        if !hints.is_empty() {
            return AllHintsSearchOutcome::Complete(hints);
        }

        if let Err(gap) = self.collect_band(grid, &specs, ProducerBand::Chaining, &mut hints) {
            return AllHintsSearchOutcome::Incomplete { hints, gap };
        }
        if !hints.is_empty() {
            return AllHintsSearchOutcome::Complete(hints);
        }

        if let Err(gap) = self.collect_band(grid, &specs, ProducerBand::ChainingPlus, &mut hints) {
            return AllHintsSearchOutcome::Incomplete { hints, gap };
        }
        if !hints.is_empty() {
            return AllHintsSearchOutcome::Complete(hints);
        }

        let has_advanced = specs
            .iter()
            .any(|producer| producer_band(producer.kind) == ProducerBand::Advanced);
        if has_advanced && !include_expensive {
            return AllHintsSearchOutcome::ConfirmationRequired;
        }

        for producer in specs
            .iter()
            .copied()
            .filter(|producer| producer_band(producer.kind) == ProducerBand::Advanced)
        {
            if let Err(gap) = self.collect_producer(grid, producer, &mut hints) {
                return AllHintsSearchOutcome::Incomplete { hints, gap };
            }
            if !hints.is_empty() {
                return AllHintsSearchOutcome::Complete(hints);
            }
        }

        AllHintsSearchOutcome::Complete(hints)
    }

    fn collect_band(
        &self,
        grid: &Grid,
        specs: &[ProducerSpec],
        band: ProducerBand,
        hints: &mut Vec<CollectedInference>,
    ) -> Result<(), PortGap> {
        for producer in specs
            .iter()
            .copied()
            .filter(|producer| producer_band(producer.kind) == band)
        {
            self.collect_producer(grid, producer, hints)?;
        }
        Ok(())
    }

    fn collect_producer(
        &self,
        grid: &Grid,
        producer: ProducerSpec,
        hints: &mut Vec<CollectedInference>,
    ) -> Result<(), PortGap> {
        match producer.state {
            ProducerState::KnownEmpty => return Ok(()),
            ProducerState::Unported => return Err(PortGap::Producer(producer.kind)),
            ProducerState::Ported => {}
        }

        let inferences = match producer.kind {
            ProducerKind::HiddenSingle => collect_hidden_singles(grid, self.config),
            ProducerKind::NakedSingle => collect_naked_singles(grid, self.config),
            ProducerKind::ForcingCellNonConsecutive => collect_forcing_cell_non_consecutive(grid)
                .into_iter()
                .map(non_consecutive_inference)
                .collect(),
            ProducerKind::LockedNonConsecutive => collect_locked_non_consecutive(grid)
                .into_iter()
                .map(non_consecutive_inference)
                .collect(),
            ProducerKind::ForcingCellFerzNonConsecutive => {
                collect_forcing_cell_ferz_non_consecutive(grid)
                    .into_iter()
                    .map(non_consecutive_inference)
                    .collect()
            }
            ProducerKind::LockedFerzNonConsecutive => collect_locked_ferz_non_consecutive(grid)
                .into_iter()
                .map(non_consecutive_inference)
                .collect(),
            ProducerKind::DirectLocking => collect_direct_locking(grid),
            ProducerKind::DirectHiddenSet { degree } => {
                collect_direct_hidden_sets(grid, self.config, degree)
            }
            ProducerKind::Locking => collect_locking(grid),
            ProducerKind::GeneralizedIntersections => collect_generalized_intersections(grid),
            ProducerKind::NakedSet {
                degree,
                generalized,
            } => collect_naked_sets(grid, self.config, degree, generalized),
            ProducerKind::Fish { degree } => collect_fish(grid, self.config, degree),
            ProducerKind::HiddenSet { degree } => collect_hidden_sets(grid, self.config, degree),
            ProducerKind::TurbotFish => collect_two_strong_links(grid, self.config),
            ProducerKind::XYWing => collect_wings(grid, false),
            ProducerKind::XYZWing => collect_wings(grid, true),
            ProducerKind::UniqueLoops => collect_unique_loop(grid, self.config),
            ProducerKind::StrongLinks { degree: 3 } => {
                collect_three_strong_links(grid, self.config)
            }
            ProducerKind::StrongLinks { degree: 4 } => collect_four_strong_links(grid, self.config),
            ProducerKind::WXYZWing => collect_alphabet_wing(grid, 4),
            ProducerKind::BivalueUniversalGrave => {
                collect_bivalue_universal_grave(grid, self.config)
            }
            ProducerKind::VWXYZWing => collect_alphabet_wing(grid, 5),
            ProducerKind::AlignedPairExclusion => collect_aligned_pair_exclusions(grid),
            ProducerKind::UVWXYZWing => collect_alphabet_wing(grid, 6),
            ProducerKind::ForcingChainCycle => collect_forcing_chain_cycles(grid, self.config),
            ProducerKind::TUVWXYZWing => collect_alphabet_wing(grid, 7),
            ProducerKind::AlignedTripletExclusion => collect_aligned_triplet_exclusions(grid),
            ProducerKind::NishioForcingChain => collect_nishio_forcing_chains(grid, self.config),
            ProducerKind::MultipleForcingChain => {
                collect_multiple_forcing_chains(grid, self.config)
            }
            ProducerKind::DynamicForcingChain => collect_dynamic_forcing_chains(grid, self.config),
            ProducerKind::DynamicForcingChainPlus => {
                collect_dynamic_forcing_chain_plus_checked(grid, self.config)
                    .map_err(PortGap::LegacyFcPlus2)?
            }
            ProducerKind::NestedForcingChain {
                level,
                nesting_limit,
            } => collect_nested_forcing_chains_checked(grid, self.config, level, nesting_limit)
                .map_err(PortGap::LegacyFcPlus2)?,
            ProducerKind::StrongLinks { .. } => {
                unreachable!("unported producers stop before all-hints dispatch")
            }
        };

        let category = hint_category(producer.kind);
        // DefaultHintsAccumulator calls candidate.equals(existing) in this
        // exact sequence. Most collectors implement their concrete Java hint
        // equality locally; this final pass covers equality families that span
        // producer instances. Never use structural `Inference` equality here:
        // several released hint classes intentionally retain identity-equal
        // duplicates.
        for inference in inferences {
            if hints
                .iter()
                .any(|existing| java_hint_equals(grid, producer.kind, &inference, existing))
            {
                continue;
            }
            hints.push(CollectedInference {
                producer: producer.kind,
                category,
                inference,
            });
        }
        Ok(())
    }

    /// Rebuild presentation proof data for one retained all-hints inference.
    /// Non-chain families require no extra proof and return `Ok(None)`.
    pub fn replay_selected_proof(
        &self,
        grid: &Grid,
        producer: ProducerKind,
        inference: &Inference,
    ) -> Result<Option<SelectedChainProof>, PortGap> {
        let proof = match producer {
            ProducerKind::ForcingChainCycle => {
                replay_forcing_chain_cycle_proof(grid, self.config, inference)
            }
            ProducerKind::NishioForcingChain => {
                replay_nishio_forcing_chain_proof(grid, self.config, inference)
            }
            ProducerKind::MultipleForcingChain => {
                replay_multiple_forcing_chain_proof(grid, self.config, inference)
            }
            ProducerKind::DynamicForcingChain => {
                replay_dynamic_forcing_chain_proof(grid, self.config, inference)
            }
            ProducerKind::DynamicForcingChainPlus => {
                return replay_dynamic_forcing_chain_plus_proof(grid, self.config, inference)
                    .map(Some)
                    .map_err(PortGap::LegacyFcPlus2);
            }
            ProducerKind::NestedForcingChain {
                level,
                nesting_limit,
            } => {
                return replay_nested_forcing_chain_proof_checked(
                    grid,
                    self.config,
                    level,
                    nesting_limit,
                    inference,
                )
                .map(Some)
                .map_err(PortGap::LegacyFcPlus2);
            }
            _ => return Ok(None),
        };
        Ok(Some(proof))
    }

    #[must_use]
    pub fn next_inference(&self, grid: &Grid) -> SearchOutcome {
        if grid.is_solved() {
            return SearchOutcome::None;
        }
        for producer in self.producer_specs(grid) {
            match producer.state {
                ProducerState::KnownEmpty => continue,
                ProducerState::Unported => {
                    return SearchOutcome::Incomplete(PortGap::Producer(producer.kind));
                }
                ProducerState::Ported => {}
            }
            let inference = match producer.kind {
                ProducerKind::HiddenSingle => find_hidden_single(grid, self.config),
                ProducerKind::NakedSingle => find_naked_single(grid, self.config),
                ProducerKind::ForcingCellNonConsecutive => {
                    find_forcing_cell_non_consecutive(grid).map(non_consecutive_inference)
                }
                ProducerKind::LockedNonConsecutive => {
                    find_locked_non_consecutive(grid).map(non_consecutive_inference)
                }
                ProducerKind::ForcingCellFerzNonConsecutive => {
                    find_forcing_cell_ferz_non_consecutive(grid).map(non_consecutive_inference)
                }
                ProducerKind::LockedFerzNonConsecutive => {
                    find_locked_ferz_non_consecutive(grid).map(non_consecutive_inference)
                }
                ProducerKind::DirectLocking => find_direct_locking(grid),
                ProducerKind::DirectHiddenSet { degree } => {
                    find_direct_hidden_set(grid, self.config, degree)
                }
                ProducerKind::Locking => find_locking(grid),
                ProducerKind::GeneralizedIntersections => find_generalized_intersections(grid),
                ProducerKind::NakedSet {
                    degree,
                    generalized,
                } => find_naked_set(grid, self.config, degree, generalized),
                ProducerKind::Fish { degree } => find_fish(grid, self.config, degree),
                ProducerKind::HiddenSet { degree } => find_hidden_set(grid, self.config, degree),
                ProducerKind::TurbotFish => find_two_strong_links(grid, self.config),
                ProducerKind::XYWing => find_wing(grid, false),
                ProducerKind::XYZWing => find_wing(grid, true),
                ProducerKind::UniqueLoops => find_unique_loop(grid, self.config),
                ProducerKind::StrongLinks { degree: 3 } => {
                    find_three_strong_links(grid, self.config)
                }
                ProducerKind::StrongLinks { degree: 4 } => {
                    find_four_strong_links(grid, self.config)
                }
                ProducerKind::WXYZWing => find_alphabet_wing(grid, 4),
                ProducerKind::BivalueUniversalGrave => {
                    find_bivalue_universal_grave(grid, self.config)
                }
                ProducerKind::VWXYZWing => find_alphabet_wing(grid, 5),
                ProducerKind::AlignedPairExclusion => find_aligned_pair_exclusion(grid),
                ProducerKind::UVWXYZWing => find_alphabet_wing(grid, 6),
                ProducerKind::ForcingChainCycle => find_forcing_chain_cycle(grid, self.config),
                ProducerKind::TUVWXYZWing => find_alphabet_wing(grid, 7),
                ProducerKind::AlignedTripletExclusion => find_aligned_triplet_exclusion(grid),
                ProducerKind::NishioForcingChain => find_nishio_forcing_chain(grid, self.config),
                ProducerKind::MultipleForcingChain => {
                    find_multiple_forcing_chain(grid, self.config)
                }
                ProducerKind::DynamicForcingChain => find_dynamic_forcing_chain(grid, self.config),
                ProducerKind::DynamicForcingChainPlus => {
                    match find_dynamic_forcing_chain_plus_checked(grid, self.config) {
                        Ok(inference) => inference,
                        Err(boundary) => {
                            return SearchOutcome::Incomplete(PortGap::LegacyFcPlus2(boundary));
                        }
                    }
                }
                ProducerKind::NestedForcingChain {
                    level,
                    nesting_limit,
                } => {
                    match find_nested_forcing_chain_checked(grid, self.config, level, nesting_limit)
                    {
                        Ok(inference) => inference,
                        Err(boundary) => {
                            return SearchOutcome::Incomplete(PortGap::LegacyFcPlus2(boundary));
                        }
                    }
                }
                ProducerKind::StrongLinks { .. } => {
                    unreachable!("unported producers stop before dispatch")
                }
            };
            if let Some(inference) = inference {
                return SearchOutcome::Found(inference);
            }
        }
        SearchOutcome::Incomplete(PortGap::IndirectTechniques)
    }

    /// Select the next inference and materialize proof views only for the
    /// winning chain-family hint.
    ///
    /// This walks the same producer registry as [`Self::next_inference`] once.
    /// Ordinary producers use their compact finders; supported chain slots
    /// call opt-in detailed finders only when reached. The compact method
    /// remains a separate, unchanged path used by rating.
    #[must_use]
    pub fn next_inference_with_selected_proof(&self, grid: &Grid) -> PresentationSearchOutcome {
        if grid.is_solved() {
            return PresentationSearchOutcome::None;
        }
        for producer in self.producer_specs(grid) {
            match producer.state {
                ProducerState::KnownEmpty => continue,
                ProducerState::Unported => {
                    return PresentationSearchOutcome::Incomplete(PortGap::Producer(producer.kind));
                }
                ProducerState::Ported => {}
            }
            let (inference, selected_chain_proof) = match producer.kind {
                ProducerKind::HiddenSingle => (find_hidden_single(grid, self.config), None),
                ProducerKind::NakedSingle => (find_naked_single(grid, self.config), None),
                ProducerKind::ForcingCellNonConsecutive => (
                    find_forcing_cell_non_consecutive(grid).map(non_consecutive_inference),
                    None,
                ),
                ProducerKind::LockedNonConsecutive => (
                    find_locked_non_consecutive(grid).map(non_consecutive_inference),
                    None,
                ),
                ProducerKind::ForcingCellFerzNonConsecutive => (
                    find_forcing_cell_ferz_non_consecutive(grid).map(non_consecutive_inference),
                    None,
                ),
                ProducerKind::LockedFerzNonConsecutive => (
                    find_locked_ferz_non_consecutive(grid).map(non_consecutive_inference),
                    None,
                ),
                ProducerKind::DirectLocking => (find_direct_locking(grid), None),
                ProducerKind::DirectHiddenSet { degree } => {
                    (find_direct_hidden_set(grid, self.config, degree), None)
                }
                ProducerKind::Locking => (find_locking(grid), None),
                ProducerKind::GeneralizedIntersections => {
                    (find_generalized_intersections(grid), None)
                }
                ProducerKind::NakedSet {
                    degree,
                    generalized,
                } => (find_naked_set(grid, self.config, degree, generalized), None),
                ProducerKind::Fish { degree } => (find_fish(grid, self.config, degree), None),
                ProducerKind::HiddenSet { degree } => {
                    (find_hidden_set(grid, self.config, degree), None)
                }
                ProducerKind::TurbotFish => (find_two_strong_links(grid, self.config), None),
                ProducerKind::XYWing => (find_wing(grid, false), None),
                ProducerKind::XYZWing => (find_wing(grid, true), None),
                ProducerKind::UniqueLoops => (find_unique_loop(grid, self.config), None),
                ProducerKind::StrongLinks { degree: 3 } => {
                    (find_three_strong_links(grid, self.config), None)
                }
                ProducerKind::StrongLinks { degree: 4 } => {
                    (find_four_strong_links(grid, self.config), None)
                }
                ProducerKind::WXYZWing => (find_alphabet_wing(grid, 4), None),
                ProducerKind::BivalueUniversalGrave => {
                    (find_bivalue_universal_grave(grid, self.config), None)
                }
                ProducerKind::VWXYZWing => (find_alphabet_wing(grid, 5), None),
                ProducerKind::AlignedPairExclusion => (find_aligned_pair_exclusion(grid), None),
                ProducerKind::UVWXYZWing => (find_alphabet_wing(grid, 6), None),
                ProducerKind::ForcingChainCycle => {
                    if let Some(detailed) = find_forcing_chain_cycle_with_proof(grid, self.config) {
                        let (inference, proof) = detailed.into_parts();
                        (Some(inference), Some(proof))
                    } else {
                        (None, None)
                    }
                }
                ProducerKind::TUVWXYZWing => (find_alphabet_wing(grid, 7), None),
                ProducerKind::AlignedTripletExclusion => {
                    (find_aligned_triplet_exclusion(grid), None)
                }
                ProducerKind::NishioForcingChain => {
                    if let Some(detailed) = find_nishio_forcing_chain_with_proof(grid, self.config)
                    {
                        let (inference, proof) = detailed.into_parts();
                        (Some(inference), Some(proof))
                    } else {
                        (None, None)
                    }
                }
                ProducerKind::MultipleForcingChain => {
                    if let Some(detailed) =
                        find_multiple_forcing_chain_with_proof(grid, self.config)
                    {
                        let (inference, proof) = detailed.into_parts();
                        (Some(inference), Some(proof))
                    } else {
                        (None, None)
                    }
                }
                ProducerKind::DynamicForcingChain => {
                    if let Some(detailed) = find_dynamic_forcing_chain_with_proof(grid, self.config)
                    {
                        let (inference, proof) = detailed.into_parts();
                        (Some(inference), Some(proof))
                    } else {
                        (None, None)
                    }
                }
                ProducerKind::DynamicForcingChainPlus => {
                    match find_dynamic_forcing_chain_plus_with_proof_checked(grid, self.config) {
                        Ok(Some(detailed)) => {
                            let (inference, proof) = detailed.into_parts();
                            (Some(inference), Some(proof))
                        }
                        Ok(None) => (None, None),
                        Err(boundary) => {
                            return PresentationSearchOutcome::Incomplete(PortGap::LegacyFcPlus2(
                                boundary,
                            ));
                        }
                    }
                }
                ProducerKind::NestedForcingChain {
                    level,
                    nesting_limit,
                } => {
                    match find_nested_forcing_chain_with_proof_checked(
                        grid,
                        self.config,
                        level,
                        nesting_limit,
                    ) {
                        Ok(Some(detailed)) => {
                            let (inference, proof) = detailed.into_parts();
                            (Some(inference), Some(proof))
                        }
                        Ok(None) => (None, None),
                        Err(boundary) => {
                            return PresentationSearchOutcome::Incomplete(PortGap::LegacyFcPlus2(
                                boundary,
                            ));
                        }
                    }
                }
                ProducerKind::StrongLinks { .. } => {
                    unreachable!("unported producers stop before dispatch")
                }
            };
            if let Some(inference) = inference {
                return PresentationSearchOutcome::Found(PresentationInference {
                    inference,
                    selected_chain_proof,
                });
            }
        }
        PresentationSearchOutcome::Incomplete(PortGap::IndirectTechniques)
    }

    /// Rate a cloned working state, leaving the caller's grid untouched.
    #[must_use]
    pub fn rate(&self, grid: &Grid) -> RateOutcome {
        let mut working = grid.clone();
        let mut tracker = RatingTracker::default();
        loop {
            match self.next_inference(&working) {
                SearchOutcome::Found(inference) => {
                    tracker.observe(&inference);
                    inference.apply(&mut working);
                }
                SearchOutcome::None => return RateOutcome::Rated(tracker.result()),
                SearchOutcome::Incomplete(gap) => {
                    return RateOutcome::Incomplete {
                        gap,
                        partial: tracker.result(),
                    };
                }
            }
        }
    }

    fn add_non_consecutive_specs(&self, result: &mut Vec<ProducerSpec>, grid: &Grid) {
        let pair = match grid.topology().config().non_consecutive {
            NonConsecutiveMode::Off => return,
            NonConsecutiveMode::Orthogonal | NonConsecutiveMode::OrthogonalCyclic => [
                (
                    ProducerKind::ForcingCellNonConsecutive,
                    TechniqueGate::ForcingCellNonConsecutive,
                ),
                (
                    ProducerKind::LockedNonConsecutive,
                    TechniqueGate::LockedNonConsecutive,
                ),
            ],
            NonConsecutiveMode::Diagonal | NonConsecutiveMode::DiagonalCyclic => [
                (
                    ProducerKind::ForcingCellFerzNonConsecutive,
                    TechniqueGate::ForcingCellFerzNonConsecutive,
                ),
                (
                    ProducerKind::LockedFerzNonConsecutive,
                    TechniqueGate::LockedFerzNonConsecutive,
                ),
            ],
        };
        for (kind, gate) in pair {
            if self.technique_enabled(grid, gate) {
                result.push(ProducerSpec {
                    kind,
                    enable_gate: gate,
                    state: ProducerState::Ported,
                });
            }
        }
    }

    fn technique_enabled(&self, grid: &Grid, gate: TechniqueGate) -> bool {
        if !self.config.enabled_techniques.contains(gate) {
            return false;
        }
        if !self.config.java_default_technique_profile {
            return true;
        }
        let variant = grid.topology().config();
        let has_added_region_or_chess_variant = variant.disjoint_groups
            || variant.windows
            || variant.sudoku_x
            || variant.girandola
            || variant.asterisk
            || variant.center_dot
            || variant.anti_ferz
            || variant.anti_knight;
        match gate {
            TechniqueGate::PointingClaiming => !has_added_region_or_chess_variant,
            TechniqueGate::GeneralizedIntersections => has_added_region_or_chess_variant,
            TechniqueGate::NakedPair => !has_added_region_or_chess_variant,
            TechniqueGate::GeneralizedNakedPair => has_added_region_or_chess_variant,
            TechniqueGate::NakedTriplet => !has_added_region_or_chess_variant,
            TechniqueGate::GeneralizedNakedTriplet => has_added_region_or_chess_variant,
            TechniqueGate::NakedQuad => !has_added_region_or_chess_variant,
            TechniqueGate::GeneralizedNakedQuad => has_added_region_or_chess_variant,
            TechniqueGate::GeneralizedNakedQuint => false,
            TechniqueGate::FiveStrongLinks
            | TechniqueGate::GeneralizedNakedSext
            | TechniqueGate::SixStrongLinks => false,
            _ => true,
        }
    }
}

fn non_consecutive_inference(hint: NonConsecutiveHint) -> Inference {
    let rating = Rating::from_tenths(u16::from(hint.rating_tenths()));
    let (geometry, kind, removals) = hint.into_parts();
    let technique = match kind {
        NonConsecutiveHintKind::ForcingCell { .. } => Technique::NonConsecutiveForcingCell,
        NonConsecutiveHintKind::Locked { .. } => Technique::LockedNonConsecutive,
    };
    Inference::elimination(
        technique,
        rating,
        removals,
        Evidence::NonConsecutive { geometry, kind },
    )
}

impl Default for Solver {
    fn default() -> Self {
        Self::new(EngineConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{ConstraintTopology, Grid, NonConsecutiveMode, Puzzle, VariantConfig};

    use super::{
        AllHintsSearchOutcome, HintCategory, PortGap, PresentationSearchOutcome, ProducerKind,
        ProducerState, SearchOutcome, Solver,
    };
    use crate::{
        ChainProofViewKind, EngineConfig, LegacyFcPlusBoundary, Rating, RatingMode, SearchPolicy,
        Technique, TechniqueGate, TechniqueSet,
    };

    fn empty_grid(config: VariantConfig) -> Grid {
        Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(config)),
            &Puzzle::parse(&".".repeat(81)).unwrap(),
        )
    }

    fn snapshot_grid(config: VariantConfig, entries: &[(usize, &str)]) -> Grid {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = "123456789".repeat(81).chars().collect::<Vec<_>>();
        for &(cell, digits) in entries {
            display[cell * 9..cell * 9 + 9].fill('.');
            for byte in digits.bytes() {
                display[cell * 9 + usize::from(byte - b'1')] = char::from(byte);
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
    fn presentation_search_matches_compact_fcc_and_adds_only_selected_proof() {
        let values = Puzzle::parse(
            "....4.8.....5.8.14..4.......5....4..4.285.....3.49......5.63.4..4.7.5.6.....84...",
        )
        .unwrap();
        let candidates = Puzzle::parse(
            "1.3.567.912...67.91.3...7.9123..6......4......2...67.9.......8..23.5.7.9.23.567.9.23..67.9.2...67....3..67.9....5.....2....7.........8..23..67.91...........4.....123.5..8.1....6789...4.....1.3..6...123...7........7.9.2..56....2..5.7.9.2..567.91....67.9....5....1....6789.23..6.....3...7..12...67.....4......2....789.2...67.9...4.....1....67.9.2..............8.....5....1.....7..1....67....3...7.91.3..67.91....67....3......1......8....4.............912...67..12....7...2..5.78.12..5.7..12....7891......89....5....1.......9.....6.....3......12....7.....4.....12....78912.....89...4.....1.......9......7..12...........5....123.....9.....6...123....89123..67.912...6..9..3..67..12......9.......8....4.....1...5.7.9.2....7.912..5.7.9",
        )
        .unwrap();
        let grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            })),
            &values,
            &candidates,
        )
        .unwrap();
        let solver = Solver::default();
        let SearchOutcome::Found(compact) = solver.next_inference(&grid) else {
            panic!("compact FCC fixture");
        };
        let PresentationSearchOutcome::Found(detailed) =
            solver.next_inference_with_selected_proof(&grid)
        else {
            panic!("presentation FCC fixture");
        };

        assert_eq!(detailed.inference(), &compact);
        assert_eq!(compact.technique(), Technique::ForcingChainCycle);
        let proof = detailed
            .selected_chain_proof()
            .expect("FCC has selected proof");
        assert_eq!(proof.views().len(), 1);
        assert_eq!(proof.views()[0].kind(), ChainProofViewKind::Forcing);
    }

    #[test]
    fn presentation_search_matches_compact_nishio_and_keeps_both_targets() {
        let values = Puzzle::parse(
            "....4.8.....5.8.14..4.......5....4..4.285.....3.49......5.63.4..4.7.5.6.....84...",
        )
        .unwrap();
        let candidates = Puzzle::parse(
            "1.3.567.912...67.91.3...7.9123..6......4......2...67.9.......8..23.5.7.9.23.567.9.23..67.9.2...67....3..67.9....5.....2....7.........8..23..67.91...........4.....123.5..8.1....6789...4.....1.3..6...123...7........7.9.2..56....2..5.7.9.2..567.91....67.9....5....1....6789.23..6.....3...7..12...67.....4......2....789.2...67.9...4.....1....67.9.2..............8.....5....1.....7..1....67....3...7.91.3..67.91....67....3......1......8....4.............912...67..12....7...2..5.78.12..5.7..12....7.91......89....5....1.......9.....6.....3......12....7.....4.....12....78912.....89...4.....1.......9......7..12...........5....123.....9.....6...123....89123..67.912...6..9..3..67..12......9.......8....4.....1...5.7.9.2....7.912..5.7.9",
        )
        .unwrap();
        let grid = Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            })),
            &values,
            &candidates,
        )
        .unwrap();
        let solver = Solver::default();
        let SearchOutcome::Found(compact) = solver.next_inference(&grid) else {
            panic!("compact Nishio fixture");
        };
        let PresentationSearchOutcome::Found(detailed) =
            solver.next_inference_with_selected_proof(&grid)
        else {
            panic!("presentation Nishio fixture");
        };

        assert_eq!(detailed.inference(), &compact);
        assert_eq!(compact.technique(), Technique::NishioForcingChain);
        let proof = detailed
            .selected_chain_proof()
            .expect("Nishio has selected proof");
        assert_eq!(proof.views().len(), 2);
        assert_eq!(proof.views()[0].kind(), ChainProofViewKind::NishioOn);
        assert_eq!(proof.views()[1].kind(), ChainProofViewKind::NishioOff);
        assert_eq!(
            proof.views()[0].nodes()[0].cell(),
            proof.views()[1].nodes()[0].cell()
        );
        assert_eq!(
            proof.views()[0].nodes()[0].digit(),
            proof.views()[1].nodes()[0].digit()
        );
        assert!(proof.views()[0].nodes()[0].state().is_on());
        assert!(!proof.views()[1].nodes()[0].state().is_on());
    }

    #[test]
    fn presentation_search_matches_compact_multiple_and_level_zero_dynamic() {
        let mut grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &Puzzle::parse(
                "100000002520070049009000500000689000000703000090105030640010025010000070900000008",
            )
            .unwrap(),
        );
        let solver = Solver::default();
        for step in 1..=7 {
            let SearchOutcome::Found(inference) = solver.next_inference(&grid) else {
                panic!("classic trace setup step {step}");
            };
            inference.apply(&mut grid);
        }

        let SearchOutcome::Found(compact_multiple) = solver.next_inference(&grid) else {
            panic!("compact MFC fixture");
        };
        let PresentationSearchOutcome::Found(detailed_multiple) =
            solver.next_inference_with_selected_proof(&grid)
        else {
            panic!("presentation MFC fixture");
        };
        assert_eq!(detailed_multiple.inference(), &compact_multiple);
        assert_eq!(
            compact_multiple.technique(),
            Technique::MultipleForcingChain
        );
        let multiple_proof = detailed_multiple
            .selected_chain_proof()
            .expect("MFC has selected proof");
        assert!(!multiple_proof.views().is_empty());
        for (branch, view) in multiple_proof.views().iter().enumerate() {
            assert_eq!(
                view.kind(),
                ChainProofViewKind::CellBranch {
                    branch: u8::try_from(branch).expect("MFC branch index"),
                }
            );
        }
        compact_multiple.apply(&mut grid);

        let SearchOutcome::Found(second_multiple) = solver.next_inference(&grid) else {
            panic!("second compact MFC fixture");
        };
        assert_eq!(second_multiple.technique(), Technique::MultipleForcingChain);
        second_multiple.apply(&mut grid);

        let SearchOutcome::Found(compact_dynamic) = solver.next_inference(&grid) else {
            panic!("compact level-zero DFC fixture");
        };
        let PresentationSearchOutcome::Found(detailed_dynamic) =
            solver.next_inference_with_selected_proof(&grid)
        else {
            panic!("presentation level-zero DFC fixture");
        };
        assert_eq!(detailed_dynamic.inference(), &compact_dynamic);
        assert_eq!(compact_dynamic.technique(), Technique::DynamicForcingChain);
        let dynamic_proof = detailed_dynamic
            .selected_chain_proof()
            .expect("level-zero DFC has selected proof");
        assert_eq!(dynamic_proof.views().len(), 2);
        assert_eq!(
            dynamic_proof.views()[0].kind(),
            ChainProofViewKind::ContradictionOn
        );
        assert_eq!(
            dynamic_proof.views()[1].kind(),
            ChainProofViewKind::ContradictionOff
        );
    }

    #[test]
    fn original_registry_preserves_java_order() {
        let kinds = Solver::default()
            .producer_specs(&empty_grid(VariantConfig::default()))
            .into_iter()
            .map(|spec| spec.kind())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                ProducerKind::HiddenSingle,
                ProducerKind::DirectLocking,
                ProducerKind::DirectHiddenSet { degree: 2 },
                ProducerKind::NakedSingle,
                ProducerKind::DirectHiddenSet { degree: 3 },
                ProducerKind::Locking,
                ProducerKind::NakedSet {
                    degree: 2,
                    generalized: false,
                },
                ProducerKind::Fish { degree: 2 },
                ProducerKind::HiddenSet { degree: 2 },
                ProducerKind::NakedSet {
                    degree: 3,
                    generalized: false,
                },
                ProducerKind::Fish { degree: 3 },
                ProducerKind::HiddenSet { degree: 3 },
                ProducerKind::TurbotFish,
                ProducerKind::XYWing,
                ProducerKind::XYZWing,
                ProducerKind::UniqueLoops,
                ProducerKind::NakedSet {
                    degree: 4,
                    generalized: false,
                },
                ProducerKind::Fish { degree: 4 },
                ProducerKind::HiddenSet { degree: 4 },
                ProducerKind::StrongLinks { degree: 3 },
                ProducerKind::WXYZWing,
                ProducerKind::BivalueUniversalGrave,
                ProducerKind::StrongLinks { degree: 4 },
                ProducerKind::VWXYZWing,
                ProducerKind::AlignedPairExclusion,
                ProducerKind::UVWXYZWing,
                ProducerKind::ForcingChainCycle,
                ProducerKind::TUVWXYZWing,
                ProducerKind::AlignedTripletExclusion,
                ProducerKind::NishioForcingChain,
                ProducerKind::MultipleForcingChain,
                ProducerKind::DynamicForcingChain,
                ProducerKind::DynamicForcingChainPlus,
                ProducerKind::NestedForcingChain {
                    level: 2,
                    nesting_limit: 0,
                },
                ProducerKind::NestedForcingChain {
                    level: 3,
                    nesting_limit: 0,
                },
                ProducerKind::NestedForcingChain {
                    level: 4,
                    nesting_limit: 0,
                },
                ProducerKind::NestedForcingChain {
                    level: 4,
                    nesting_limit: 1,
                },
                ProducerKind::NestedForcingChain {
                    level: 4,
                    nesting_limit: 2,
                },
                ProducerKind::NestedForcingChain {
                    level: 4,
                    nesting_limit: 3,
                },
            ]
        );
    }

    #[test]
    fn all_hints_collects_the_complete_direct_tier_without_changing_next() {
        let puzzle = Puzzle::parse(&format!("12345678.45678912.{}", ".".repeat(63))).unwrap();
        let grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &puzzle,
        );
        let solver = Solver::default();
        let SearchOutcome::Found(next) = solver.next_inference(&grid) else {
            panic!("fixture must have a next inference");
        };
        let AllHintsSearchOutcome::Complete(hints) = solver.all_inferences(&grid, false) else {
            panic!("ordinary direct tier must be complete");
        };

        assert!(hints.len() >= 2);
        assert_eq!(hints[0].inference(), &next);
        assert_eq!(hints[0].category(), HintCategory::Direct);
        assert_eq!(hints[0].inference().placement_cell().unwrap().raw(), 8);
        assert_eq!(hints[1].inference().placement_cell().unwrap().raw(), 17);
    }

    #[test]
    fn legacy_hint_tree_category_follows_java_hint_subtype_not_search_tier() {
        assert_eq!(
            super::hint_category(ProducerKind::HiddenSingle),
            HintCategory::Direct
        );
        assert_eq!(
            super::hint_category(ProducerKind::NakedSingle),
            HintCategory::Direct
        );
        assert_eq!(
            super::hint_category(ProducerKind::DirectLocking),
            HintCategory::Indirect
        );
        assert_eq!(
            super::hint_category(ProducerKind::DirectHiddenSet { degree: 2 }),
            HintCategory::Indirect
        );
        assert_eq!(
            super::hint_category(ProducerKind::ForcingCellNonConsecutive),
            HintCategory::Indirect
        );
    }

    #[test]
    fn forge_policy_starts_from_the_frozen_compatibility_registry() {
        let grid = empty_grid(VariantConfig::default());
        let compatibility = Solver::default().producer_specs(&grid);
        let forge = Solver::new(EngineConfig {
            search_policy: SearchPolicy::Forge,
            ..EngineConfig::default()
        })
        .producer_specs(&grid);
        assert_eq!(forge, compatibility);

        let revised_compatibility = Solver::new(EngineConfig {
            rating_mode: RatingMode::Revised,
            ..EngineConfig::default()
        })
        .producer_specs(&grid);
        let revised_forge = Solver::new(EngineConfig {
            rating_mode: RatingMode::Revised,
            search_policy: SearchPolicy::Forge,
            ..EngineConfig::default()
        })
        .producer_specs(&grid);
        assert_eq!(revised_forge, revised_compatibility);
    }

    #[test]
    fn all_four_non_consecutive_registry_slots_are_ported() {
        for (mode, expected) in [
            (
                NonConsecutiveMode::Orthogonal,
                [
                    ProducerKind::ForcingCellNonConsecutive,
                    ProducerKind::LockedNonConsecutive,
                ],
            ),
            (
                NonConsecutiveMode::OrthogonalCyclic,
                [
                    ProducerKind::ForcingCellNonConsecutive,
                    ProducerKind::LockedNonConsecutive,
                ],
            ),
            (
                NonConsecutiveMode::Diagonal,
                [
                    ProducerKind::ForcingCellFerzNonConsecutive,
                    ProducerKind::LockedFerzNonConsecutive,
                ],
            ),
            (
                NonConsecutiveMode::DiagonalCyclic,
                [
                    ProducerKind::ForcingCellFerzNonConsecutive,
                    ProducerKind::LockedFerzNonConsecutive,
                ],
            ),
        ] {
            let grid = empty_grid(VariantConfig {
                non_consecutive: mode,
                forbidden_pairs: true,
                ..VariantConfig::default()
            });
            let actual = Solver::default()
                .producer_specs(&grid)
                .into_iter()
                .filter(|spec| expected.contains(&spec.kind()))
                .map(|spec| (spec.kind(), spec.state()))
                .collect::<Vec<_>>();
            assert_eq!(actual, expected.map(|kind| (kind, ProducerState::Ported)));
        }
    }

    #[test]
    fn non_consecutive_result_converts_to_exact_public_inference() {
        let values = Puzzle::parse(&".".repeat(81)).unwrap();
        let mut display = ['.'; 729];
        for (cell, digits) in [(0_usize, "45"), (1, "45"), (9, "45")] {
            for digit in digits.bytes() {
                display[cell * 9 + usize::from(digit - b'1')] = char::from(digit);
            }
        }
        let candidates = Puzzle::parse(&display.iter().collect::<String>()).unwrap();
        let topology = Arc::new(ConstraintTopology::new(VariantConfig {
            non_consecutive: NonConsecutiveMode::Orthogonal,
            forbidden_pairs: true,
            ..VariantConfig::default()
        }));
        let grid = Grid::from_snapshot(topology, &values, &candidates).unwrap();
        let hint = crate::find_forcing_cell_non_consecutive(&grid).expect("forcing-cell NC");
        let inference = super::non_consecutive_inference(hint);

        assert_eq!(inference.technique(), Technique::NonConsecutiveForcingCell);
        assert_eq!(inference.rating(), Rating::from_tenths(24));
        assert_eq!(inference.name(), "Non-Consecutive Forcing Cell");
        assert_eq!(inference.short_name(), "kNC");
        assert_eq!(
            inference.description(grid.topology()),
            "Cell r1c1 on value(s) 4,5"
        );
    }

    #[test]
    fn solver_surfaces_the_legacy_fcplus_two_boundary() {
        let puzzle = Puzzle::parse(
            "........1.....2....34..........5..6...17..3..8....9..4...6...7...8..4..9.2..3.5..",
        )
        .unwrap();
        let grid = Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &puzzle,
        );
        let solver = Solver::new(EngineConfig {
            forcing_chain_plus: 2,
            ..EngineConfig::default()
        });
        let outcome = solver.next_inference(&grid);
        assert_eq!(
            outcome,
            SearchOutcome::Incomplete(PortGap::LegacyFcPlus2(LegacyFcPlusBoundary::UniqueLoops))
        );
        assert_eq!(
            solver.next_inference_with_selected_proof(&grid),
            PresentationSearchOutcome::Incomplete(PortGap::LegacyFcPlus2(
                LegacyFcPlusBoundary::UniqueLoops,
            ))
        );
    }

    #[test]
    fn anti_knight_defaults_replace_locking_with_generalized_intersections() {
        let kinds = Solver::default()
            .producer_specs(&empty_grid(VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            }))
            .into_iter()
            .map(|spec| spec.kind())
            .collect::<Vec<_>>();
        assert!(!kinds.contains(&ProducerKind::Locking));
        assert!(kinds.contains(&ProducerKind::GeneralizedIntersections));
        assert!(!kinds.contains(&ProducerKind::NakedSet {
            degree: 2,
            generalized: false,
        }));
        assert!(kinds.contains(&ProducerKind::NakedSet {
            degree: 2,
            generalized: true,
        }));
        assert!(!kinds.contains(&ProducerKind::NakedSet {
            degree: 3,
            generalized: false,
        }));
        assert!(kinds.contains(&ProducerKind::NakedSet {
            degree: 3,
            generalized: true,
        }));
        assert!(!kinds.contains(&ProducerKind::NakedSet {
            degree: 4,
            generalized: false,
        }));
        assert!(kinds.contains(&ProducerKind::NakedSet {
            degree: 4,
            generalized: true,
        }));
        assert!(kinds.contains(&ProducerKind::Fish { degree: 4 }));
        assert!(kinds.contains(&ProducerKind::HiddenSet { degree: 4 }));
        assert!(kinds.contains(&ProducerKind::StrongLinks { degree: 3 }));
        assert!(kinds.contains(&ProducerKind::WXYZWing));
    }

    #[test]
    fn revised_registry_keeps_turbot_fish_before_swordfish() {
        let kinds = Solver::new(EngineConfig {
            rating_mode: RatingMode::Revised,
            ..EngineConfig::default()
        })
        .producer_specs(&empty_grid(VariantConfig::default()))
        .into_iter()
        .map(|spec| spec.kind())
        .collect::<Vec<_>>();
        let hidden_triplet = kinds
            .iter()
            .position(|kind| *kind == ProducerKind::HiddenSet { degree: 3 })
            .unwrap();
        let turbot = kinds
            .iter()
            .position(|kind| *kind == ProducerKind::TurbotFish)
            .unwrap();
        let swordfish = kinds
            .iter()
            .position(|kind| *kind == ProducerKind::Fish { degree: 3 })
            .unwrap();
        let xy_wing = kinds
            .iter()
            .position(|kind| *kind == ProducerKind::XYWing)
            .unwrap();
        let xyz_wing = kinds
            .iter()
            .position(|kind| *kind == ProducerKind::XYZWing)
            .unwrap();
        let unique_loops = kinds
            .iter()
            .position(|kind| *kind == ProducerKind::UniqueLoops)
            .unwrap();
        let naked_quad = kinds
            .iter()
            .position(|kind| {
                *kind
                    == ProducerKind::NakedSet {
                        degree: 4,
                        generalized: false,
                    }
            })
            .unwrap();
        let jellyfish = kinds
            .iter()
            .position(|kind| *kind == ProducerKind::Fish { degree: 4 })
            .unwrap();
        let hidden_quad = kinds
            .iter()
            .position(|kind| *kind == ProducerKind::HiddenSet { degree: 4 })
            .unwrap();
        let three_strong_links = kinds
            .iter()
            .position(|kind| *kind == ProducerKind::StrongLinks { degree: 3 })
            .unwrap();
        let wxyz_wing = kinds
            .iter()
            .position(|kind| *kind == ProducerKind::WXYZWing)
            .unwrap();
        assert!(
            hidden_triplet < turbot
                && turbot < swordfish
                && swordfish < xy_wing
                && xy_wing < xyz_wing
                && xyz_wing < unique_loops
                && unique_loops < naked_quad
                && naked_quad < jellyfish
                && jellyfish < hidden_quad
                && hidden_quad < three_strong_links
                && three_strong_links < wxyz_wing
        );
        let nested = kinds
            .iter()
            .copied()
            .filter(|kind| matches!(kind, ProducerKind::NestedForcingChain { .. }))
            .collect::<Vec<_>>();
        assert_eq!(
            nested,
            vec![
                ProducerKind::NestedForcingChain {
                    level: 2,
                    nesting_limit: 0,
                },
                ProducerKind::NestedForcingChain {
                    level: 3,
                    nesting_limit: 0,
                },
                ProducerKind::NestedForcingChain {
                    level: 4,
                    nesting_limit: 0,
                },
                ProducerKind::NestedForcingChain {
                    level: 4,
                    nesting_limit: 1,
                },
                ProducerKind::NestedForcingChain {
                    level: 4,
                    nesting_limit: 2,
                },
            ]
        );
    }

    #[test]
    fn generalized_naked_quint_is_only_present_outside_java_default_profiles() {
        let grid = empty_grid(VariantConfig::default());
        let default_kinds = Solver::default()
            .producer_specs(&grid)
            .into_iter()
            .map(|spec| spec.kind())
            .collect::<Vec<_>>();
        let quint = ProducerKind::NakedSet {
            degree: 5,
            generalized: true,
        };
        assert!(!default_kinds.contains(&quint));

        let custom_kinds = Solver::new(EngineConfig {
            java_default_technique_profile: false,
            ..EngineConfig::default()
        })
        .producer_specs(&grid)
        .into_iter()
        .map(|spec| spec.kind())
        .collect::<Vec<_>>();
        let quint_index = custom_kinds.iter().position(|kind| *kind == quint).unwrap();
        let wxyz_index = custom_kinds
            .iter()
            .position(|kind| *kind == ProducerKind::WXYZWing)
            .unwrap();
        assert!(quint_index < wxyz_index);
    }

    #[test]
    fn late_registry_preserves_the_java_order_and_port_frontier() {
        let specs = Solver::default().producer_specs(&empty_grid(VariantConfig::default()));
        let expected = [
            (ProducerKind::WXYZWing, ProducerState::Ported),
            (ProducerKind::BivalueUniversalGrave, ProducerState::Ported),
            (
                ProducerKind::StrongLinks { degree: 4 },
                ProducerState::Ported,
            ),
            (ProducerKind::VWXYZWing, ProducerState::Ported),
            (ProducerKind::AlignedPairExclusion, ProducerState::Ported),
            (ProducerKind::UVWXYZWing, ProducerState::Ported),
            (ProducerKind::ForcingChainCycle, ProducerState::Ported),
            (ProducerKind::TUVWXYZWing, ProducerState::Ported),
            (ProducerKind::AlignedTripletExclusion, ProducerState::Ported),
            (ProducerKind::NishioForcingChain, ProducerState::Ported),
            (ProducerKind::MultipleForcingChain, ProducerState::Ported),
            (ProducerKind::DynamicForcingChain, ProducerState::Ported),
            (ProducerKind::DynamicForcingChainPlus, ProducerState::Ported),
            (
                ProducerKind::NestedForcingChain {
                    level: 2,
                    nesting_limit: 0,
                },
                ProducerState::Ported,
            ),
            (
                ProducerKind::NestedForcingChain {
                    level: 3,
                    nesting_limit: 0,
                },
                ProducerState::Ported,
            ),
            (
                ProducerKind::NestedForcingChain {
                    level: 4,
                    nesting_limit: 0,
                },
                ProducerState::Ported,
            ),
            (
                ProducerKind::NestedForcingChain {
                    level: 4,
                    nesting_limit: 1,
                },
                ProducerState::Ported,
            ),
            (
                ProducerKind::NestedForcingChain {
                    level: 4,
                    nesting_limit: 2,
                },
                ProducerState::Ported,
            ),
            (
                ProducerKind::NestedForcingChain {
                    level: 4,
                    nesting_limit: 3,
                },
                ProducerState::Ported,
            ),
        ];
        let actual = specs
            .iter()
            .filter_map(|spec| {
                expected
                    .iter()
                    .any(|(kind, _)| *kind == spec.kind())
                    .then_some((spec.kind(), spec.state()))
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);

        let kinds = specs.iter().map(|spec| spec.kind()).collect::<Vec<_>>();
        assert!(!kinds.contains(&ProducerKind::StrongLinks { degree: 5 }));
        assert!(!kinds.contains(&ProducerKind::NakedSet {
            degree: 6,
            generalized: true,
        }));
        assert!(!kinds.contains(&ProducerKind::StrongLinks { degree: 6 }));
    }

    #[test]
    fn quad_family_keeps_independent_java_gates() {
        let grid = empty_grid(VariantConfig::default());
        for (disabled_gate, expected_kind) in [
            (
                TechniqueGate::NakedQuad,
                ProducerKind::NakedSet {
                    degree: 4,
                    generalized: false,
                },
            ),
            (TechniqueGate::Jellyfish, ProducerKind::Fish { degree: 4 }),
            (
                TechniqueGate::HiddenQuad,
                ProducerKind::HiddenSet { degree: 4 },
            ),
            (
                TechniqueGate::ThreeStrongLinks,
                ProducerKind::StrongLinks { degree: 3 },
            ),
        ] {
            let kinds = Solver::new(EngineConfig {
                enabled_techniques: TechniqueSet::ALL.without(disabled_gate),
                ..EngineConfig::default()
            })
            .producer_specs(&grid)
            .into_iter()
            .map(|spec| spec.kind())
            .collect::<Vec<_>>();
            assert!(!kinds.contains(&expected_kind));
        }
    }

    #[test]
    fn full_registry_reaches_hidden_quad_after_the_other_quad_slots() {
        let grid = snapshot_grid(
            VariantConfig::default(),
            &[
                (0, "145"),
                (1, "56789"),
                (2, "237"),
                (9, "56789"),
                (10, "126"),
                (11, "56789"),
                (18, "56789"),
                (19, "348"),
                (20, "56789"),
            ],
        );
        let SearchOutcome::Found(inference) = Solver::default().next_inference(&grid) else {
            panic!("full registry did not reach the Java Hidden Quad fixture");
        };
        assert_eq!(inference.technique(), Technique::HiddenQuad);
        assert_eq!(inference.rating(), Rating::from_tenths(54));
        assert_eq!(
            inference.description(grid.topology()),
            "Hidden Quad: Cells r1c1,r1c3,r2c2,r3c2: 1,2,3,4 in block"
        );
        assert_eq!(inference.removals().elimination_count(), 4);
    }

    #[test]
    fn revised_triplet_uses_the_pair_enable_gate() {
        let grid = empty_grid(VariantConfig::default());
        let without_pair = Solver::new(EngineConfig {
            rating_mode: RatingMode::Revised,
            enabled_techniques: TechniqueSet::ALL.without(TechniqueGate::DirectHiddenPair),
            ..EngineConfig::default()
        });
        assert!(
            !without_pair
                .producer_specs(&grid)
                .iter()
                .any(|spec| matches!(spec.kind(), ProducerKind::DirectHiddenSet { .. }))
        );

        let without_triplet = Solver::new(EngineConfig {
            rating_mode: RatingMode::Revised,
            enabled_techniques: TechniqueSet::ALL.without(TechniqueGate::DirectHiddenTriplet),
            ..EngineConfig::default()
        });
        assert!(without_triplet.producer_specs(&grid).iter().any(|spec| {
            spec.kind() == ProducerKind::DirectHiddenSet { degree: 3 }
                && spec.enable_gate() == TechniqueGate::DirectHiddenPair
        }));
    }

    #[test]
    fn revised_no_blocks_keeps_a_known_empty_locking_slot() {
        let specs = Solver::new(EngineConfig {
            rating_mode: RatingMode::Revised,
            ..EngineConfig::default()
        })
        .producer_specs(&empty_grid(VariantConfig {
            blocks: false,
            ..VariantConfig::default()
        }));
        assert!(specs.iter().any(|spec| {
            spec.kind() == ProducerKind::DirectLocking && spec.state() == ProducerState::KnownEmpty
        }));
    }

    #[test]
    fn xy_and_xyz_wing_keep_independent_java_gates() {
        let grid = empty_grid(VariantConfig::default());
        let without_xy = Solver::new(EngineConfig {
            enabled_techniques: TechniqueSet::ALL.without(TechniqueGate::XYWing),
            ..EngineConfig::default()
        });
        let kinds = without_xy
            .producer_specs(&grid)
            .into_iter()
            .map(|spec| spec.kind())
            .collect::<Vec<_>>();
        assert!(!kinds.contains(&ProducerKind::XYWing));
        assert!(kinds.contains(&ProducerKind::XYZWing));

        let without_xyz = Solver::new(EngineConfig {
            enabled_techniques: TechniqueSet::ALL.without(TechniqueGate::XYZWing),
            ..EngineConfig::default()
        });
        let kinds = without_xyz
            .producer_specs(&grid)
            .into_iter()
            .map(|spec| spec.kind())
            .collect::<Vec<_>>();
        assert!(kinds.contains(&ProducerKind::XYWing));
        assert!(!kinds.contains(&ProducerKind::XYZWing));
    }
}
