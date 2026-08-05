/// Java-compatible rating table selection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RatingMode {
    #[default]
    Original,
    Revised,
}

/// Selects the solver's search policy independently of its rating table.
///
/// `Compatibility` freezes Sukaku Explainer's producer order and tie-breaking
/// for both [`RatingMode::Original`] and [`RatingMode::Revised`]. `Forge` is a
/// deliberately separate boundary for future search-policy experiments; it is
/// behaviorally identical until such experiments are introduced explicitly.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SearchPolicy {
    #[default]
    Compatibility,
    Forge,
}

/// Enable gates used while constructing the ordered producer registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum TechniqueGate {
    HiddenSingle,
    NakedSingle,
    DirectPointing,
    DirectHiddenPair,
    DirectHiddenTriplet,
    ForcingCellNonConsecutive,
    LockedNonConsecutive,
    ForcingCellFerzNonConsecutive,
    LockedFerzNonConsecutive,
    PointingClaiming,
    GeneralizedIntersections,
    NakedPair,
    GeneralizedNakedPair,
    XWing,
    HiddenPair,
    NakedTriplet,
    GeneralizedNakedTriplet,
    Swordfish,
    HiddenTriplet,
    TurbotFish,
    XYWing,
    XYZWing,
    UniqueLoop,
    NakedQuad,
    GeneralizedNakedQuad,
    Jellyfish,
    HiddenQuad,
    ThreeStrongLinks,
    GeneralizedNakedQuint,
    WXYZWing,
    BivalueUniversalGrave,
    FourStrongLinks,
    VWXYZWing,
    AlignedPairExclusion,
    FiveStrongLinks,
    GeneralizedNakedSext,
    UVWXYZWing,
    SixStrongLinks,
    ForcingChainCycle,
    TUVWXYZWing,
    AlignedTripletExclusion,
    NishioForcingChain,
    MultipleForcingChain,
    DynamicForcingChain,
    DynamicForcingChainPlus,
    NestedForcingChain,
}

/// Compact set of enabled technique gates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TechniqueSet(u64);

impl TechniqueSet {
    pub const ALL: Self = Self((1_u64 << 46) - 1);

    #[must_use]
    pub const fn contains(self, technique: TechniqueGate) -> bool {
        self.0 & (1_u64 << technique as u8) != 0
    }

    #[must_use]
    pub const fn without(self, technique: TechniqueGate) -> Self {
        Self(self.0 & !(1_u64 << technique as u8))
    }
}

impl Default for TechniqueSet {
    fn default() -> Self {
        Self::ALL
    }
}

/// Settings that alter inference order or ratings rather than topology.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineConfig {
    pub variant_latin: bool,
    pub rating_mode: RatingMode,
    pub search_policy: SearchPolicy,
    /// Java's FCPlus setting: 0 is the default schedule, 1 and 2 append
    /// progressively broader advanced deductions to nested forcing chains.
    pub forcing_chain_plus: u8,
    /// Branch-local Unique Rectangle/Loop extra-value tracking (Java default).
    pub unique_loop_fix: bool,
    /// lkSudoku's corrected multi-cell BUG discovery and Type 3 order.
    pub bug_fix: bool,
    pub enabled_techniques: TechniqueSet,
    /// Apply Sukaku Explainer's classic-versus-added-variant defaults.
    pub java_default_technique_profile: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            variant_latin: false,
            rating_mode: RatingMode::Original,
            search_policy: SearchPolicy::Compatibility,
            forcing_chain_plus: 0,
            unique_loop_fix: true,
            bug_fix: true,
            enabled_techniques: TechniqueSet::ALL,
            java_default_technique_profile: true,
        }
    }
}
