use sukaku_forge_core::{
    CandidateMask, CandidateRemovals, CellId, ConstraintTopology, Digit, Grid, PositionMask,
    RegionId,
};

use crate::non_consecutive::{NonConsecutiveGeometry, NonConsecutiveHintKind};
use crate::{Rating, RatingMode};

/// Stable technique identity, deliberately independent of producer order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Technique {
    HiddenSingle,
    NakedSingle,
    NonConsecutiveForcingCell,
    LockedNonConsecutive,
    DirectPointing,
    DirectClaiming,
    DirectHiddenPair,
    DirectHiddenTriplet,
    Pointing,
    Claiming,
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
    FourStrongLinks,
    WXYZWing,
    VWXYZWing,
    UVWXYZWing,
    TUVWXYZWing,
    BivalueUniversalGrave,
    AlignedPairExclusion,
    ForcingChainCycle,
    AlignedTripletExclusion,
    NishioForcingChain,
    MultipleForcingChain,
    DynamicForcingChain,
    DynamicForcingChainPlus,
    NestedForcingChain,
}

impl Technique {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::HiddenSingle => "Hidden Single",
            Self::NakedSingle => "Naked Single",
            Self::NonConsecutiveForcingCell => "Non-Consecutive Forcing Cell",
            Self::LockedNonConsecutive => "Locked Non Consecutive",
            Self::DirectPointing => "Direct Pointing",
            Self::DirectClaiming => "Direct Claiming",
            Self::DirectHiddenPair => "Direct Hidden Pair",
            Self::DirectHiddenTriplet => "Direct Hidden Triplet",
            Self::Pointing => "Pointing",
            Self::Claiming => "Claiming",
            Self::GeneralizedIntersections => "Generalized Intersections",
            Self::NakedPair => "Naked Pair",
            Self::GeneralizedNakedPair => "Generalized Naked Pair",
            Self::XWing => "X-Wing",
            Self::HiddenPair => "Hidden Pair",
            Self::NakedTriplet => "Naked Triplet",
            Self::GeneralizedNakedTriplet => "Generalized Naked Triplet",
            Self::Swordfish => "Swordfish",
            Self::HiddenTriplet => "Hidden Triplet",
            Self::TurbotFish => "Turbot Fish",
            Self::XYWing => "XY-Wing",
            Self::XYZWing => "XYZ-Wing",
            Self::UniqueLoop => "Unique Loop",
            Self::NakedQuad => "Naked Quad",
            Self::GeneralizedNakedQuad => "Generalized Naked Quad",
            Self::Jellyfish => "Jellyfish",
            Self::HiddenQuad => "Hidden Quad",
            Self::ThreeStrongLinks => "3 Strong links",
            Self::FourStrongLinks => "4 Strong links",
            Self::WXYZWing => "WXYZ-Wing",
            Self::VWXYZWing => "VWXYZ-Wing",
            Self::UVWXYZWing => "UVWXYZ-Wing",
            Self::TUVWXYZWing => "TUVWXYZ-Wing",
            Self::BivalueUniversalGrave => "Bivalue Universal Grave",
            Self::AlignedPairExclusion => "Aligned Pair Exclusion",
            Self::ForcingChainCycle => "Forcing Chains & Cycles",
            Self::AlignedTripletExclusion => "Aligned Triplet Exclusion",
            Self::NishioForcingChain => "Nishio Forcing Chains",
            Self::MultipleForcingChain => "Multiple Forcing Chains",
            Self::DynamicForcingChain => "Dynamic Forcing Chains",
            Self::DynamicForcingChainPlus => "Dynamic Forcing Chains (+)",
            Self::NestedForcingChain => "Nested Forcing Chains",
        }
    }

    #[must_use]
    pub const fn short_name(self) -> &'static str {
        match self {
            Self::HiddenSingle => "HS",
            Self::NakedSingle => "NS",
            Self::NonConsecutiveForcingCell => "kNC",
            Self::LockedNonConsecutive => "lNC",
            Self::DirectPointing => "DP",
            Self::DirectClaiming => "DC",
            Self::DirectHiddenPair => "DP",
            Self::DirectHiddenTriplet => "DT",
            Self::Pointing => "Po",
            Self::Claiming => "Cl",
            Self::GeneralizedIntersections => "gI",
            Self::NakedPair => "NP",
            Self::GeneralizedNakedPair => "gNP",
            Self::XWing => "XW",
            Self::HiddenPair => "HP",
            Self::NakedTriplet => "NT",
            Self::GeneralizedNakedTriplet => "gNT",
            Self::Swordfish => "SF",
            Self::HiddenTriplet => "HT",
            Self::TurbotFish => "TF",
            Self::XYWing => "XYW",
            Self::XYZWing => "XYZW",
            Self::UniqueLoop => "UL",
            Self::NakedQuad => "NQ",
            Self::GeneralizedNakedQuad => "gNQ",
            Self::Jellyfish => "JF",
            Self::HiddenQuad => "HQ",
            Self::ThreeStrongLinks => "3SL",
            Self::FourStrongLinks => "4SL",
            Self::WXYZWing => "WXY",
            Self::VWXYZWing => "VXY",
            Self::UVWXYZWing => "UXY",
            Self::TUVWXYZWing => "TXY",
            Self::BivalueUniversalGrave => "BUG",
            Self::AlignedPairExclusion => "APE",
            Self::ForcingChainCycle => "FCC",
            Self::AlignedTripletExclusion => "ATE",
            Self::NishioForcingChain => "NFC",
            Self::MultipleForcingChain => "MFC",
            Self::DynamicForcingChain => "DFC",
            Self::DynamicForcingChainPlus => "DFC+",
            Self::NestedForcingChain => "NFC",
        }
    }
}

/// Exact public shape of a Multiple or level-0 Dynamic Forcing Chain hint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MultipleChainKind {
    Contradiction {
        source_cell: CellId,
        source_digit: Digit,
        source_on: bool,
        target_cell: CellId,
        target_digit: Digit,
    },
    Double {
        source_cell: CellId,
        source_digit: Digit,
        target_cell: CellId,
        target_digit: Digit,
        target_on: bool,
    },
    Cell {
        source_cell: CellId,
        target_cell: CellId,
        target_digit: Digit,
        target_on: bool,
    },
    Region {
        source_region: RegionId,
        source_digit: Digit,
        target_cell: CellId,
        target_digit: Digit,
        target_on: bool,
    },
}

/// Static unary-chain category used by Java's Forcing Chains & Cycles slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChainKind {
    XCycle,
    YCycle,
    XyCycle,
    XForcing,
    XyForcing,
}

impl ChainKind {
    #[must_use]
    pub const fn is_cycle(self) -> bool {
        matches!(self, Self::XCycle | Self::YCycle | Self::XyCycle)
    }
}

/// Java-ordered distinct cells touched by a static bidirectional cycle.
///
/// A legal chain may touch more cells than the 18-cell all-different bound
/// used by `CellSequence`, so this has its own exact 81-cell capacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChainCellSequence {
    cells: [u8; 81],
    len: u8,
}

impl ChainCellSequence {
    pub(crate) const fn new() -> Self {
        Self {
            cells: [0; 81],
            len: 0,
        }
    }

    pub(crate) fn push(&mut self, cell: CellId) {
        self.cells[usize::from(self.len)] = cell.raw();
        self.len += 1;
    }

    pub fn iter(self) -> impl ExactSizeIterator<Item = CellId> {
        self.cells
            .into_iter()
            .take(usize::from(self.len))
            .map(|raw| CellId::new(raw).expect("stored chain cell index"))
    }

    #[must_use]
    pub const fn len(self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }
}

/// Java-ordered locked value pairs retained for the APE explanation view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AlignedPairCombinationSequence {
    /// Digit one, digit two, then locking-cell index + 1 (zero is duplicate).
    entries: [u16; 81],
    len: u8,
}

impl AlignedPairCombinationSequence {
    pub(crate) const fn new() -> Self {
        Self {
            entries: [0; 81],
            len: 0,
        }
    }

    pub(crate) fn push(&mut self, first: Digit, second: Digit, locking_cell: Option<CellId>) {
        let locking_code = locking_cell.map_or(0_u16, |cell| u16::from(cell.raw()) + 1);
        self.entries[usize::from(self.len)] =
            u16::from(first.get()) | (u16::from(second.get()) << 4) | (locking_code << 8);
        self.len += 1;
    }

    pub fn iter(self) -> impl ExactSizeIterator<Item = (Digit, Digit, Option<CellId>)> {
        self.entries
            .into_iter()
            .take(usize::from(self.len))
            .map(|entry| {
                let first = Digit::new((entry & 0x0f) as u8).expect("stored APE digit");
                let second = Digit::new(((entry >> 4) & 0x0f) as u8).expect("stored APE digit");
                let locking_code = ((entry >> 8) & 0x7f) as u8;
                let locking_cell = (locking_code != 0)
                    .then(|| CellId::new(locking_code - 1).expect("stored APE locking cell"));
                (first, second, locking_cell)
            })
    }

    #[must_use]
    pub const fn len(self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }
}

/// Exact Java-ordered invalid value combinations retained for an ATE proof.
///
/// Materializing all reachable rows would require up to 243 entries: the
/// first two bases are uncapped, while the tail is itself a 2- or 3-candidate
/// excluder. Instead, the three
/// pre-hint candidate masks and the ordered common excluders form a compact,
/// self-contained recipe from which every row and its first locking cell can
/// be regenerated without consulting a possibly mutated grid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AlignedTripletCombinationSequence {
    candidate_masks: [CandidateMask; 3],
    /// bit 0 = positions 0/1, bit 1 = 0/2, bit 2 = 1/2.
    visible_pairs: u8,
    /// Seven cell bits followed by the normalized nine candidate bits.
    excluders: [u16; 81],
    len: u8,
}

impl AlignedTripletCombinationSequence {
    pub(crate) const fn new(candidate_masks: [CandidateMask; 3], visible_pairs: u8) -> Self {
        Self {
            candidate_masks,
            visible_pairs,
            excluders: [0; 81],
            len: 0,
        }
    }

    pub(crate) fn push_excluder(&mut self, cell: CellId, values: CandidateMask) {
        self.excluders[usize::from(self.len)] =
            (u16::from(cell.raw()) << 9) | ((values.bits() >> 1) & 0x01ff);
        self.len += 1;
    }

    /// Iterate only invalid rows in Java's descending mixed-radix order.
    ///
    /// A `None` locking cell denotes a repeated value in two mutually visible
    /// base cells. Otherwise the cell is the first ordered common excluder
    /// whose candidates are a subset of the row's selected values.
    #[must_use]
    pub fn iter(self) -> AlignedTripletCombinationIter {
        AlignedTripletCombinationIter::new(self)
    }

    #[must_use]
    pub const fn common_excluder_count(self) -> usize {
        self.len as usize
    }
}

/// Lazy iterator over the proof rows represented by
/// [`AlignedTripletCombinationSequence`].
#[derive(Clone, Debug)]
pub struct AlignedTripletCombinationIter {
    sequence: AlignedTripletCombinationSequence,
    current: [u8; 3],
    finished: bool,
}

impl AlignedTripletCombinationIter {
    fn new(sequence: AlignedTripletCombinationSequence) -> Self {
        Self {
            current: sequence.candidate_masks.map(highest_candidate),
            sequence,
            finished: false,
        }
    }

    fn advance(&mut self) {
        for position in 0..3 {
            let current = self.current[position];
            let lower = self.sequence.candidate_masks[position].bits() & ((1_u16 << current) - 1);
            if lower != 0 {
                self.current[position] = highest_candidate(CandidateMask::from_bits(lower));
                return;
            }
            self.current[position] = highest_candidate(self.sequence.candidate_masks[position]);
        }
        self.finished = true;
    }
}

impl Iterator for AlignedTripletCombinationIter {
    type Item = ([Digit; 3], Option<CellId>);

    fn next(&mut self) -> Option<Self::Item> {
        while !self.finished {
            let digits = self
                .current
                .map(|raw| Digit::new(raw).expect("stored ATE candidate digit"));
            self.advance();

            let duplicate = (self.sequence.visible_pairs & 0b001 != 0 && digits[0] == digits[1])
                || (self.sequence.visible_pairs & 0b010 != 0 && digits[0] == digits[2])
                || (self.sequence.visible_pairs & 0b100 != 0 && digits[1] == digits[2]);
            if duplicate {
                return Some((digits, None));
            }

            let selected = CandidateMask::of(digits[0])
                .union(CandidateMask::of(digits[1]))
                .union(CandidateMask::of(digits[2]));
            for entry in self
                .sequence
                .excluders
                .iter()
                .copied()
                .take(usize::from(self.sequence.len))
            {
                let values = CandidateMask::from_bits((entry & 0x01ff) << 1);
                if values.without(selected).is_empty() {
                    let cell =
                        CellId::new((entry >> 9) as u8).expect("stored ATE common excluder cell");
                    return Some((digits, Some(cell)));
                }
            }
        }
        None
    }
}

fn highest_candidate(values: CandidateMask) -> u8 {
    debug_assert!(!values.is_empty());
    (u16::BITS - 1 - values.bits().leading_zeros()) as u8
}

/// Ordered BUG cells can exceed the 18-cell bound of valid Unique Loops.
///
/// Java retains discovery order, so this deliberately uses fixed primitive
/// storage instead of sorting or allocating a list for every probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BugCellSequence {
    /// Seven cell-index bits followed by the normalized nine candidate bits.
    entries: [u16; 81],
    len: u8,
}

impl BugCellSequence {
    pub(crate) const fn new() -> Self {
        Self {
            entries: [0; 81],
            len: 0,
        }
    }

    pub(crate) fn push_with_values(&mut self, cell: CellId, values: CandidateMask) {
        self.entries[usize::from(self.len)] = u16::from(cell.raw()) | ((values.bits() >> 1) << 7);
        self.len += 1;
    }

    pub fn iter(self) -> impl ExactSizeIterator<Item = CellId> {
        self.entries
            .into_iter()
            .take(usize::from(self.len))
            .map(|entry| CellId::new((entry & 0x7f) as u8).expect("stored BUG cell index"))
    }

    pub fn iter_with_values(self) -> impl ExactSizeIterator<Item = (CellId, CandidateMask)> {
        self.entries
            .into_iter()
            .take(usize::from(self.len))
            .map(|entry| {
                (
                    CellId::new((entry & 0x7f) as u8).expect("stored BUG cell index"),
                    CandidateMask::from_bits(((entry >> 7) & 0x01ff) << 1),
                )
            })
    }

    #[must_use]
    pub const fn len(self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }
}

/// Type-specific presentation metadata for a Bivalue Universal Grave.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BugKind {
    Type1 {
        cell: CellId,
        extra_values: CandidateMask,
    },
    Type2 {
        bug_cells: BugCellSequence,
        digit: Digit,
    },
    Type3 {
        bug_cells: BugCellSequence,
        set_cells: CellSequence,
        region: RegionId,
        set_values: CandidateMask,
        all_extra_values: CandidateMask,
        generalized: bool,
    },
    Type4 {
        bug_cells: [CellId; 2],
        extra_values: [CandidateMask; 2],
        region: RegionId,
        locked_digit: Digit,
        all_extra_values: CandidateMask,
    },
}

impl BugKind {
    #[must_use]
    pub const fn hint_type(self) -> u8 {
        match self {
            Self::Type1 { .. } => 1,
            Self::Type2 { .. } => 2,
            Self::Type3 { .. } => 3,
            Self::Type4 { .. } => 4,
        }
    }
}

/// Small ordered cell list used when topology alone cannot reconstruct hint order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CellSequence {
    cells: [u8; 18],
    len: u8,
}

impl CellSequence {
    pub(crate) const fn new() -> Self {
        Self {
            cells: [0; 18],
            len: 0,
        }
    }

    pub(crate) fn push(&mut self, cell: CellId) {
        self.cells[usize::from(self.len)] = cell.raw();
        self.len += 1;
    }

    pub fn iter(self) -> impl ExactSizeIterator<Item = CellId> {
        self.cells
            .into_iter()
            .take(usize::from(self.len))
            .map(|raw| CellId::new(raw).expect("stored cell index"))
    }

    #[must_use]
    pub const fn len(self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }
}

/// Type-specific presentation metadata for a Unique Rectangle or Loop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UniqueLoopKind {
    Type1 {
        rescue: CellId,
    },
    Type2 {
        extra_cells: CellSequence,
        digit: Digit,
    },
    Type3Naked {
        rescue_cells: [CellId; 2],
        region: RegionId,
        extra_values: CandidateMask,
        set_cells: CellSequence,
        set_values: CandidateMask,
    },
    Type3Hidden {
        rescue_cells: [CellId; 2],
        region: RegionId,
        extra_values: CandidateMask,
        hidden_positions: PositionMask,
        hidden_values: CandidateMask,
    },
    Type4 {
        rescue_cells: [CellId; 2],
        region: RegionId,
        lock_digit: Digit,
        remove_digit: Digit,
    },
}

impl UniqueLoopKind {
    #[must_use]
    pub const fn hint_type(self) -> u8 {
        match self {
            Self::Type1 { .. } => 1,
            Self::Type2 { .. } => 2,
            Self::Type3Naked { .. } | Self::Type3Hidden { .. } => 3,
            Self::Type4 { .. } => 4,
        }
    }
}

/// Compact explanation metadata retained independently from application effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Evidence {
    HiddenSingle {
        region: RegionId,
        alone: bool,
    },
    NakedSingle,
    NonConsecutive {
        geometry: NonConsecutiveGeometry,
        kind: NonConsecutiveHintKind,
    },
    DirectLocking {
        primary: RegionId,
        secondary: RegionId,
        /// Candidate cells in Java's secondary-region presentation order.
        pattern_positions: PositionMask,
    },
    HiddenSet {
        degree: u8,
        region: RegionId,
        tuple_digits: CandidateMask,
        tuple_positions: PositionMask,
    },
    Locking {
        primary: RegionId,
        secondary: RegionId,
        digit: Digit,
        /// Locked candidates in Java's secondary-region presentation order.
        pattern_positions: PositionMask,
    },
    GeneralizedIntersections {
        region: RegionId,
        digit: Digit,
        locked_positions: PositionMask,
    },
    NakedSet {
        degree: u8,
        region: RegionId,
        tuple_digits: CandidateMask,
        tuple_positions: PositionMask,
        generalized: bool,
    },
    Fish {
        degree: u8,
        digit: Digit,
        base_type: u8,
        cover_type: u8,
        selected_cells: CellSequence,
    },
    TwoStrongLinks {
        digit: Digit,
        pattern_cells: CellSequence,
        link_regions: [RegionId; 2],
        link_positions: [PositionMask; 2],
        /// Endpoint groups in link-0/end-0, link-0/end-1, link-1/end-0,
        /// link-1/end-1 order. The representative is first in each sequence.
        endpoint_groups: [CellSequence; 4],
        bridge_region: RegionId,
        ring_region: Option<RegionId>,
        grouped_links: [bool; 2],
        rating_mode: RatingMode,
    },
    ThreeStrongLinks {
        digit: Digit,
        pattern_cells: CellSequence,
        /// Base-link metadata remains in candidate-tuple order.
        link_regions: [RegionId; 3],
        link_positions: [PositionMask; 3],
        /// Endpoint groups in link-0/end-0, link-0/end-1, ... order.
        endpoint_groups: [CellSequence; 6],
        /// Weak-link regions in displayed chain order.
        bridge_regions: [RegionId; 2],
        ring_region: Option<RegionId>,
        grouped_links: [bool; 3],
        /// Permutation from displayed-chain position to base-link index.
        link_order: [u8; 3],
    },
    FourStrongLinks {
        digit: Digit,
        pattern_cells: CellSequence,
        link_regions: [RegionId; 4],
        link_positions: [PositionMask; 4],
        endpoint_groups: [CellSequence; 8],
        bridge_regions: [RegionId; 3],
        ring_region: Option<RegionId>,
        grouped_links: [bool; 4],
        link_order: [u8; 4],
    },
    Wing {
        pivot: CellId,
        xz: CellId,
        yz: CellId,
        digit: Digit,
    },
    AlphabetWing {
        pattern_cells: CellSequence,
        x_digit: Digit,
        z_digit: Digit,
        double_link: bool,
        biggest_cardinality: u8,
        wing_size: u8,
        wing_set: CandidateMask,
    },
    UniqueLoop {
        loop_cells: CellSequence,
        first_digit: Digit,
        second_digit: Digit,
        kind: UniqueLoopKind,
    },
    Bug {
        kind: BugKind,
    },
    AlignedPairExclusion {
        cells: [CellId; 2],
        locked_combinations: AlignedPairCombinationSequence,
    },
    AlignedTripletExclusion {
        cells: [CellId; 3],
        locked_combinations: AlignedTripletCombinationSequence,
    },
    ForcingChainCycle {
        kind: ChainKind,
        target_cell: CellId,
        target_digit: Digit,
        target_on: bool,
        complexity: u16,
        selected_cells: ChainCellSequence,
    },
    NishioForcingChain {
        source_cell: CellId,
        source_digit: Digit,
        source_on: bool,
        target_cell: CellId,
        target_digit: Digit,
        complexity: u16,
    },
    MultipleForcingChain {
        dynamic: bool,
        level: u8,
        kind: MultipleChainKind,
        complexity: u32,
    },
}

/// A logical inference with compact effects and presentation-independent evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inference {
    technique: Technique,
    rating: Rating,
    removals: CandidateRemovals,
    placement: Option<(CellId, Digit)>,
    evidence: Evidence,
}

impl Inference {
    pub(crate) fn placement(
        technique: Technique,
        rating: Rating,
        cell: CellId,
        digit: Digit,
        evidence: Evidence,
    ) -> Self {
        Self {
            technique,
            rating,
            removals: CandidateRemovals::default(),
            placement: Some((cell, digit)),
            evidence,
        }
    }

    pub(crate) fn elimination(
        technique: Technique,
        rating: Rating,
        removals: CandidateRemovals,
        evidence: Evidence,
    ) -> Self {
        assert!(!removals.is_empty());
        Self {
            technique,
            rating,
            removals,
            placement: None,
            evidence,
        }
    }

    #[must_use]
    pub const fn technique(&self) -> Technique {
        self.technique
    }

    #[must_use]
    pub const fn rating(&self) -> Rating {
        self.rating
    }

    #[must_use]
    pub const fn placement_cell(&self) -> Option<CellId> {
        match self.placement {
            Some((cell, _)) => Some(cell),
            None => None,
        }
    }

    #[must_use]
    pub const fn placement_digit(&self) -> Option<Digit> {
        match self.placement {
            Some((_, digit)) => Some(digit),
            None => None,
        }
    }

    #[must_use]
    pub const fn is_placement(&self) -> bool {
        self.placement.is_some()
    }

    #[must_use]
    pub const fn evidence(&self) -> Evidence {
        self.evidence
    }

    /// Exact displayed hint name, which may carry pattern-specific metadata.
    #[must_use]
    pub fn name(&self) -> String {
        match self.evidence {
            Evidence::TwoStrongLinks {
                link_regions,
                ring_region,
                grouped_links,
                rating_mode,
                ..
            } => two_strong_links_name(
                link_regions,
                ring_region.is_some(),
                grouped_links,
                rating_mode,
            ),
            Evidence::ThreeStrongLinks {
                link_regions,
                ring_region,
                grouped_links,
                link_order,
                ..
            } => three_strong_links_name(
                link_regions,
                link_order,
                ring_region.is_some(),
                grouped_links,
            ),
            Evidence::FourStrongLinks {
                link_regions,
                ring_region,
                grouped_links,
                link_order,
                ..
            } => four_strong_links_name(
                link_regions,
                link_order,
                ring_region.is_some(),
                grouped_links,
            ),
            Evidence::UniqueLoop {
                loop_cells, kind, ..
            } => unique_loop_name(loop_cells.len(), kind.hint_type()),
            Evidence::AlphabetWing {
                double_link,
                biggest_cardinality,
                wing_size,
                ..
            } => format!(
                "{} {}{}{}",
                self.technique.name(),
                if double_link { 2 } else { 1 },
                biggest_cardinality,
                wing_size
            ),
            Evidence::Bug { kind } => format!("BUG type {}", kind.hint_type()),
            Evidence::ForcingChainCycle {
                kind,
                complexity,
                selected_cells,
                ..
            } => chain_name(kind, complexity, selected_cells.len()).to_owned(),
            Evidence::MultipleForcingChain {
                dynamic,
                level,
                kind,
                ..
            } => multiple_chain_name(dynamic, level, kind),
            _ => self.technique.name().to_owned(),
        }
    }

    /// Exact displayed short hint name.
    #[must_use]
    pub fn short_name(&self) -> String {
        match self.evidence {
            Evidence::TwoStrongLinks {
                link_regions,
                ring_region,
                grouped_links,
                rating_mode,
                ..
            } => two_strong_links_short_name(
                link_regions,
                ring_region.is_some(),
                grouped_links,
                rating_mode,
            ),
            Evidence::ThreeStrongLinks {
                link_regions,
                ring_region,
                grouped_links,
                link_order,
                ..
            } => three_strong_links_short_name(
                link_regions,
                link_order,
                ring_region.is_some(),
                grouped_links,
            ),
            Evidence::FourStrongLinks {
                link_regions,
                ring_region,
                grouped_links,
                link_order,
                ..
            } => four_strong_links_short_name(
                link_regions,
                link_order,
                ring_region.is_some(),
                grouped_links,
            ),
            Evidence::UniqueLoop {
                loop_cells, kind, ..
            } => unique_loop_short_name(loop_cells.len(), kind.hint_type()),
            Evidence::AlphabetWing {
                double_link,
                biggest_cardinality,
                wing_size,
                ..
            } => format!(
                "{}{}{}{}",
                self.technique.short_name(),
                if double_link { 2 } else { 1 },
                biggest_cardinality,
                wing_size
            ),
            Evidence::Bug { kind } => format!("BUG{}", kind.hint_type()),
            Evidence::ForcingChainCycle {
                kind,
                complexity,
                selected_cells,
                ..
            } => chain_short_name(kind, complexity, selected_cells.len()).to_owned(),
            Evidence::MultipleForcingChain {
                dynamic,
                level,
                kind,
                ..
            } => multiple_chain_short_name(dynamic, level, kind),
            _ => self.technique.short_name().to_owned(),
        }
    }

    #[must_use]
    pub fn removals(&self) -> &CandidateRemovals {
        &self.removals
    }

    pub fn apply(&self, grid: &mut Grid) {
        self.removals.apply(grid);
        if let Some((cell, digit)) = self.placement {
            grid.place(cell, digit);
        }
    }

    #[must_use]
    pub fn description(&self, topology: &ConstraintTopology) -> String {
        match self.evidence {
            Evidence::HiddenSingle { region, .. } => {
                let (cell, digit) = self.placement.expect("Hidden Single placement");
                format!(
                    "{}: {cell}: {digit} in {}",
                    self.technique.name(),
                    region_family_name(region.type_index())
                )
            }
            Evidence::NakedSingle => {
                let (cell, digit) = self.placement.expect("Naked Single placement");
                format!("{}: {cell}: {digit}", self.technique.name())
            }
            Evidence::NonConsecutive { kind, .. } => non_consecutive_description(kind),
            Evidence::DirectLocking {
                primary,
                secondary,
                pattern_positions,
            } => {
                let (_, digit) = self.placement.expect("direct Locking placement");
                format!(
                    "{}: {}: {digit} of {} in {}",
                    self.technique.name(),
                    full_cell_list(topology, secondary, pattern_positions),
                    region_family_name(primary.type_index()),
                    region_family_name(secondary.type_index())
                )
            }
            Evidence::HiddenSet {
                region,
                tuple_digits,
                tuple_positions,
                ..
            } => {
                let values = tuple_digits
                    .iter()
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "{}: {}: {values} in {}",
                    self.technique.name(),
                    full_cell_list(topology, region, tuple_positions),
                    region_family_name(region.type_index())
                )
            }
            Evidence::Locking {
                primary,
                secondary,
                digit,
                pattern_positions,
            } => format!(
                "{}: {}: {digit} in {} and {}",
                self.technique.name(),
                full_cell_list(topology, secondary, pattern_positions),
                region_family_name(primary.type_index()),
                region_family_name(secondary.type_index())
            ),
            Evidence::GeneralizedIntersections {
                region,
                digit,
                locked_positions,
            } => format!(
                "{} on value {digit} in {}",
                full_cell_list(topology, region, locked_positions),
                region_full_name(region)
            ),
            Evidence::NakedSet {
                region,
                tuple_digits,
                tuple_positions,
                ..
            } => format!(
                "{}: {}: {} in {}",
                self.technique.name(),
                full_cell_list(topology, region, tuple_positions),
                digit_list(tuple_digits),
                region_family_name(region.type_index())
            ),
            Evidence::Fish {
                degree,
                digit,
                base_type,
                cover_type,
                selected_cells,
            } => format!(
                "{}: {}: {digit} in {degree} {}s and {degree} {}s",
                self.technique.name(),
                full_cell_sequence(selected_cells),
                region_family_name(usize::from(base_type)),
                region_family_name(usize::from(cover_type))
            ),
            Evidence::TwoStrongLinks {
                digit,
                pattern_cells,
                rating_mode,
                ..
            } => {
                let cell_label = match rating_mode {
                    RatingMode::Original => full_cell_chain(pattern_cells),
                    RatingMode::Revised => full_cell_sequence(pattern_cells),
                };
                format!("{}: {cell_label} on value {digit}", self.name())
            }
            Evidence::ThreeStrongLinks {
                digit,
                pattern_cells,
                ..
            } => format!(
                "{}: {} on value {digit}",
                self.name(),
                full_cell_chain(pattern_cells)
            ),
            Evidence::FourStrongLinks {
                digit,
                pattern_cells,
                ..
            } => format!(
                "{}: {} on value {digit}",
                self.name(),
                full_cell_chain(pattern_cells)
            ),
            Evidence::Wing {
                pivot,
                xz,
                yz,
                digit,
            } => format!(
                "{}: Cells {pivot},{xz},{yz} on value {digit}",
                self.technique.name()
            ),
            Evidence::AlphabetWing {
                pattern_cells,
                x_digit,
                z_digit,
                double_link,
                ..
            } => {
                let values = if double_link {
                    format!("values {x_digit},{z_digit}")
                } else {
                    format!("value {z_digit}")
                };
                format!(
                    "{}: {} on {values}",
                    self.name(),
                    full_cell_sequence(pattern_cells)
                )
            }
            Evidence::UniqueLoop {
                loop_cells,
                first_digit,
                second_digit,
                ..
            } => format!(
                "{}: {} on {first_digit}, {second_digit}",
                self.name(),
                full_cell_sequence(loop_cells)
            ),
            Evidence::Bug { kind } => match kind {
                BugKind::Type1 { cell, .. } => format!("{}: {cell}", self.name()),
                BugKind::Type2 { bug_cells, digit } => format!(
                    "{}: {} on {digit}",
                    self.name(),
                    bug_cell_sequence(bug_cells)
                ),
                BugKind::Type3 {
                    bug_cells,
                    set_values,
                    ..
                } => format!(
                    "{}: {} on {}",
                    self.name(),
                    bug_cell_sequence(bug_cells),
                    set_values
                        .iter()
                        .map(|digit| digit.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                BugKind::Type4 {
                    bug_cells,
                    locked_digit,
                    ..
                } => format!(
                    "{}: {},{} on {locked_digit}",
                    self.name(),
                    bug_cells[0],
                    bug_cells[1]
                ),
            },
            Evidence::AlignedPairExclusion { cells, .. } => {
                format!("{}: {},{}", self.technique.name(), cells[0], cells[1])
            }
            Evidence::AlignedTripletExclusion { cells, .. } => format!(
                "{}: {},{},{}",
                self.technique.name(),
                cells[0],
                cells[1],
                cells[2]
            ),
            Evidence::ForcingChainCycle {
                kind,
                target_cell,
                target_digit,
                target_on,
                complexity,
                selected_cells,
            } => {
                let name = chain_name(kind, complexity, selected_cells.len());
                if kind.is_cycle() {
                    format!("{name}: {}", chain_cell_sequence(selected_cells))
                } else {
                    format!(
                        "{name}: {target_cell}.{target_digit} {}",
                        if target_on { "on" } else { "off" }
                    )
                }
            }
            Evidence::NishioForcingChain {
                source_cell,
                source_digit,
                source_on,
                target_cell,
                target_digit,
                ..
            } => format!(
                "Nishio Forcing Chain: {source_cell}.{source_digit} {} ==> \
                 {target_cell}.{target_digit} both on & off",
                if source_on { "on" } else { "off" }
            ),
            Evidence::MultipleForcingChain { kind, .. } => match kind {
                MultipleChainKind::Contradiction {
                    source_cell,
                    source_digit,
                    source_on,
                    target_cell,
                    target_digit,
                } => format!(
                    "Contradiction Forcing Chain: {source_cell}.{source_digit} {} ==> \
                     {target_cell}.{target_digit} both on & off",
                    if source_on { "on" } else { "off" }
                ),
                MultipleChainKind::Double {
                    source_cell,
                    source_digit,
                    target_cell,
                    target_digit,
                    target_on,
                } => format!(
                    "Double Forcing Chain: {source_cell}.{source_digit} on & off ==> \
                     {target_cell}.{target_digit} {}",
                    if target_on { "on" } else { "off" }
                ),
                MultipleChainKind::Cell {
                    source_cell,
                    target_cell,
                    target_digit,
                    target_on,
                } => format!(
                    "Cell Forcing Chains: {source_cell} ==> {target_cell}.{target_digit} {}",
                    if target_on { "on" } else { "off" }
                ),
                MultipleChainKind::Region {
                    source_region,
                    source_digit,
                    target_cell,
                    target_digit,
                    target_on,
                } => format!(
                    "Region Forcing Chains: {source_digit} in {} ==> \
                     {target_cell}.{target_digit} {}",
                    region_family_name(source_region.type_index()),
                    if target_on { "on" } else { "off" }
                ),
            },
        }
    }
}

fn non_consecutive_description(kind: NonConsecutiveHintKind) -> String {
    match kind {
        NonConsecutiveHintKind::ForcingCell { cell, values } => format!(
            "Cell {cell} on value(s) {}",
            values
                .iter()
                .map(|digit| digit.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ),
        NonConsecutiveHintKind::Locked {
            cells,
            values,
            digit,
            ..
        } => {
            let label = if cells.len() == 1 { "Cell" } else { "Cells" };
            let cells = cells
                .iter()
                .map(|cell| cell.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let values = values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!("{digit}: {label} {cells} on value(s) {values}")
        }
    }
}

fn multiple_chain_name(dynamic: bool, level: u8, kind: MultipleChainKind) -> String {
    let base = match (dynamic, kind) {
        (true, MultipleChainKind::Contradiction { .. }) => "Dynamic Contradiction Forcing Chains",
        (true, MultipleChainKind::Double { .. }) => "Dynamic Double Forcing Chains",
        (true, MultipleChainKind::Cell { .. }) => "Dynamic Cell Forcing Chains",
        (true, MultipleChainKind::Region { .. }) => "Dynamic Region Forcing Chains",
        (false, MultipleChainKind::Cell { .. }) => "Cell Forcing Chains",
        (false, MultipleChainKind::Region { .. }) => "Region Forcing Chains",
        (false, MultipleChainKind::Contradiction { .. } | MultipleChainKind::Double { .. }) => {
            unreachable!("static multiple chains do not publish binary hints")
        }
    };
    format!("{base}{}", multiple_chain_suffix(level))
}

fn multiple_chain_short_name(dynamic: bool, level: u8, kind: MultipleChainKind) -> String {
    let base = match (dynamic, kind) {
        (true, MultipleChainKind::Contradiction { .. }) => "DCFC",
        (true, MultipleChainKind::Double { .. }) => "DdFC",
        (true, MultipleChainKind::Cell { .. }) => "DLFC",
        (true, MultipleChainKind::Region { .. }) => "DRFC",
        (false, MultipleChainKind::Cell { .. }) => "LFC",
        (false, MultipleChainKind::Region { .. }) => "RFC",
        (false, MultipleChainKind::Contradiction { .. } | MultipleChainKind::Double { .. }) => {
            unreachable!("static multiple chains do not publish binary hints")
        }
    };
    format!("{base}{}", multiple_chain_short_suffix(level))
}

fn multiple_chain_suffix(level: u8) -> &'static str {
    match level {
        0 => "",
        1 => " (+)",
        2 => " (+ Forcing Chains)",
        3 => " (+ Multiple Forcing Chains)",
        4 => " (+ Dynamic Forcing Chains)",
        _ => unreachable!("unsupported nested-chain level"),
    }
}

fn multiple_chain_short_suffix(level: u8) -> &'static str {
    match level {
        0 => "",
        1 => "+",
        2 => "+FC",
        3 => "+MFC",
        4 => "+DFC",
        _ => unreachable!("unsupported nested-chain level"),
    }
}

fn chain_name(kind: ChainKind, complexity: u16, selected_cell_count: usize) -> &'static str {
    match kind {
        ChainKind::XCycle if selected_cell_count == 4 => "Generalized X-Wing",
        ChainKind::XCycle => "Bidirectional X-Cycle",
        ChainKind::YCycle => "Bidirectional Y-Cycle",
        ChainKind::XyCycle => "Bidirectional Cycle",
        ChainKind::XForcing if complexity == 6 => "Turbot Fish",
        ChainKind::XForcing => "Forcing X-Chain",
        ChainKind::XyForcing => "Forcing Chain",
    }
}

fn chain_short_name(kind: ChainKind, complexity: u16, selected_cell_count: usize) -> &'static str {
    match kind {
        ChainKind::XCycle if selected_cell_count == 4 => "GXW",
        ChainKind::XCycle => "BiXCy",
        ChainKind::YCycle => "BiYCy",
        ChainKind::XyCycle => "BiCy",
        ChainKind::XForcing if complexity == 6 => "TF",
        ChainKind::XForcing => "FXC",
        ChainKind::XyForcing => "FC",
    }
}

fn chain_cell_sequence(cells: ChainCellSequence) -> String {
    cells
        .iter()
        .map(|cell| cell.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn bug_cell_sequence(cells: BugCellSequence) -> String {
    cells
        .iter()
        .map(|cell| cell.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn unique_loop_name(loop_size: usize, hint_type: u8) -> String {
    if loop_size == 4 {
        format!("Unique Rectangle type {hint_type}")
    } else {
        format!("Unique Loop {loop_size} type {hint_type}")
    }
}

fn unique_loop_short_name(loop_size: usize, hint_type: u8) -> String {
    if loop_size == 4 {
        format!("UR{hint_type}")
    } else {
        format!("UL{loop_size}{hint_type}")
    }
}

fn two_strong_links_name(
    regions: [RegionId; 2],
    ring: bool,
    grouped: [bool; 2],
    mode: RatingMode,
) -> String {
    match mode {
        RatingMode::Original => original_two_strong_links_name(regions, ring, grouped),
        RatingMode::Revised => revised_turbot_name(regions, ring, grouped),
    }
}

fn two_strong_links_short_name(
    regions: [RegionId; 2],
    ring: bool,
    grouped: [bool; 2],
    mode: RatingMode,
) -> String {
    match mode {
        RatingMode::Original => {
            let suffix = original_two_strong_links_suffix(regions, grouped);
            let types = normalized_link_types(regions);
            let mut name = if ring {
                String::from("2XL")
            } else if !types.contains(&0) && !types.contains(&2) {
                String::from("SS")
            } else if types.contains(&0) {
                String::from("2SL")
            } else {
                String::from("2SK")
            };
            if grouped.into_iter().any(|value| value) {
                name.insert(0, 'g');
            }
            format!("{name}{suffix}")
        }
        RatingMode::Revised => {
            let suffix = revised_turbot_suffix(grouped);
            let base = regions[0].type_index();
            let cover = regions[1].type_index();
            let grouped_count = grouped.into_iter().filter(|value| *value).count();
            let grouped_block = (grouped[0] && base == 0) ^ (grouped[1] && cover == 0);
            let both_grouped_blocks = grouped[0] && base == 0 && grouped[1] && cover == 0;
            let mut name = if grouped_count != 0 {
                if grouped_block {
                    String::from("gTC")
                } else if both_grouped_blocks {
                    String::from("g2SL")
                } else if base == cover {
                    String::from("gSS")
                } else {
                    String::from("g2SK")
                }
            } else if base > 2 || cover > 2 {
                String::from("g2SL")
            } else {
                match (base, cover) {
                    (1, 1) | (2, 2) => String::from("SS"),
                    (1, 2) | (2, 1) => String::from("2SK"),
                    _ => String::from("TC"),
                }
            };
            if ring {
                name.push('r');
            }
            if grouped_count != 0 || base > 2 || cover > 2 {
                name.push_str(&suffix);
            }
            name
        }
    }
}

fn original_two_strong_links_name(
    regions: [RegionId; 2],
    ring: bool,
    grouped: [bool; 2],
) -> String {
    let suffix = original_two_strong_links_suffix(regions, grouped);
    let grouped_count = grouped.into_iter().filter(|value| *value).count();
    if ring {
        return format!(
            "(2 Strong Links) {}X-Loop {suffix}",
            if grouped_count == 0 { "" } else { "Grouped " }
        );
    }
    let types = normalized_link_types(regions);
    let base_name = if !types.contains(&0) && !types.contains(&2) {
        "Skyscraper"
    } else if !types.contains(&0) {
        "2-String Kite"
    } else {
        "2 Strong links"
    };
    format!(
        "{}{base_name} {suffix}",
        if grouped_count == 0 { "" } else { "Grouped " }
    )
}

fn revised_turbot_name(regions: [RegionId; 2], ring: bool, grouped: [bool; 2]) -> String {
    let base = regions[0].type_index();
    let cover = regions[1].type_index();
    let grouped_count = grouped.into_iter().filter(|value| *value).count();
    let ring_name = if ring { " X-Loop" } else { "" };
    let suffix = revised_turbot_suffix(grouped);
    if (grouped[0] && base == 0) ^ (grouped[1] && cover == 0) {
        return format!("Grouped Turbot Crane{ring_name} {suffix}");
    }
    if grouped[0] && base == 0 && grouped[1] && cover == 0 {
        return format!("Grouped 2 strong links{ring_name} {suffix}");
    }
    if grouped_count != 0 && base == cover {
        return format!("Grouped Skyscraper{ring_name} {suffix}");
    }
    if grouped_count != 0 {
        return format!("Grouped 2-String Kite{ring_name} {suffix}");
    }
    if base > 2 || cover > 2 {
        return format!("Grouped 2 strong links{ring_name} {suffix}");
    }
    let base_name = match (base, cover) {
        (1, 1) | (2, 2) => "Skyscraper",
        (1, 2) | (2, 1) => "Two-string Kite",
        _ => "Turbot Crane",
    };
    format!("{base_name}{ring_name}")
}

fn original_two_strong_links_suffix(regions: [RegionId; 2], grouped: [bool; 2]) -> String {
    let types = normalized_link_types(regions);
    format!(
        "{}{}{}",
        grouped.into_iter().filter(|value| *value).count(),
        types[0],
        types[1]
    )
}

fn normalized_link_types(regions: [RegionId; 2]) -> [usize; 2] {
    let mut original = [regions[0].type_index(), regions[1].type_index()];
    if original[0] > original[1] {
        original.reverse();
    }
    if !original.contains(&1) {
        return original.map(|value| if value == 2 { 1 } else { value });
    }
    let swapped = original.map(|value| match value {
        1 => 2,
        2 => 1,
        _ => value,
    });
    let reversed_swapped = [swapped[1], swapped[0]];
    original.min(swapped.min(reversed_swapped))
}

fn revised_turbot_suffix(grouped: [bool; 2]) -> String {
    match grouped.into_iter().filter(|value| *value).count() {
        0 => String::from("00"),
        1 => String::from("01"),
        2 => String::from("11"),
        _ => unreachable!("two links"),
    }
}

pub(crate) fn two_strong_links_suffix(
    regions: [RegionId; 2],
    grouped: [bool; 2],
    mode: RatingMode,
) -> String {
    match mode {
        RatingMode::Original => original_two_strong_links_suffix(regions, grouped),
        RatingMode::Revised => revised_turbot_suffix(grouped),
    }
}

fn is_forward_lex(types: &[usize]) -> bool {
    for index in 0..types.len() / 2 {
        match types[index].cmp(&types[types.len() - 1 - index]) {
            core::cmp::Ordering::Less => return true,
            core::cmp::Ordering::Greater => return false,
            core::cmp::Ordering::Equal => {}
        }
    }
    true
}

fn java_line_name(types: &[usize]) -> String {
    let mut original = String::with_capacity(types.len());
    let mut lines = String::with_capacity(types.len());
    let mut reverse_swap = String::with_capacity(types.len());
    let mut current_swap = String::with_capacity(types.len());
    let mut contains_row = false;
    for &type_index in types {
        let type_char = char::from(b'0' + type_index as u8);
        original.push(type_char);
        if type_index == 1 {
            contains_row = true;
            reverse_swap.insert(0, '2');
            current_swap.push('2');
        }
        if type_index == 2 {
            lines.push('1');
            reverse_swap.insert(0, '1');
            current_swap.push('1');
        } else {
            lines.push(type_char);
            if !(1..=2).contains(&type_index) {
                reverse_swap.insert(0, type_char);
                current_swap.push(type_char);
            }
        }
    }
    if reverse_swap > current_swap {
        reverse_swap = current_swap;
    }
    if contains_row {
        original.min(reverse_swap)
    } else {
        lines
    }
}

pub(crate) fn three_strong_links_suffix(
    regions: [RegionId; 3],
    order: [u8; 3],
    grouped: [bool; 3],
) -> String {
    multi_strong_links_suffix(regions, order, grouped)
}

pub(crate) fn four_strong_links_suffix(
    regions: [RegionId; 4],
    order: [u8; 4],
    grouped: [bool; 4],
) -> String {
    multi_strong_links_suffix(regions, order, grouped)
}

fn multi_strong_links_suffix<const N: usize>(
    regions: [RegionId; N],
    order: [u8; N],
    grouped: [bool; N],
) -> String {
    let mut types = order.map(|index| regions[usize::from(index)].type_index());
    if !is_forward_lex(&types) {
        types.reverse();
    }
    format!(
        "{}{}",
        grouped.into_iter().filter(|value| *value).count(),
        java_line_name(&types)
    )
}

fn three_strong_links_name(
    regions: [RegionId; 3],
    order: [u8; 3],
    ring: bool,
    grouped: [bool; 3],
) -> String {
    let suffix = three_strong_links_suffix(regions, order, grouped);
    multi_strong_links_name(
        3,
        suffix,
        ring,
        grouped.into_iter().filter(|value| *value).count(),
    )
}

fn three_strong_links_short_name(
    regions: [RegionId; 3],
    order: [u8; 3],
    ring: bool,
    grouped: [bool; 3],
) -> String {
    let suffix = three_strong_links_suffix(regions, order, grouped);
    multi_strong_links_short_name(3, suffix, ring, grouped.into_iter().any(|value| value))
}

fn four_strong_links_name(
    regions: [RegionId; 4],
    order: [u8; 4],
    ring: bool,
    grouped: [bool; 4],
) -> String {
    let suffix = four_strong_links_suffix(regions, order, grouped);
    multi_strong_links_name(
        4,
        suffix,
        ring,
        grouped.into_iter().filter(|value| *value).count(),
    )
}

fn four_strong_links_short_name(
    regions: [RegionId; 4],
    order: [u8; 4],
    ring: bool,
    grouped: [bool; 4],
) -> String {
    let suffix = four_strong_links_suffix(regions, order, grouped);
    multi_strong_links_short_name(4, suffix, ring, grouped.into_iter().any(|value| value))
}

fn multi_strong_links_name(
    degree: usize,
    suffix: String,
    ring: bool,
    grouped_count: usize,
) -> String {
    if ring {
        return format!(
            "({degree} Strong Links) {}X-Loop {suffix}",
            if grouped_count == 0 { "" } else { "Grouped " }
        );
    }
    let structure = &suffix[1..];
    let base_name = if !structure.contains('0') && !structure.contains('2') {
        format!("{degree} Skyscrapers")
    } else if !structure.contains('0') && degree < 4 {
        format!("{degree}-String Kite")
    } else {
        format!("{degree} Strong links")
    };
    format!(
        "{}{base_name} {suffix}",
        if grouped_count == 0 { "" } else { "Grouped " }
    )
}

fn multi_strong_links_short_name(
    degree: usize,
    suffix: String,
    ring: bool,
    grouped: bool,
) -> String {
    let structure = &suffix[1..];
    let mut name = if ring {
        format!("{degree}XL")
    } else if !structure.contains('0') && !structure.contains('2') {
        format!("{degree}SS")
    } else if structure.contains('0') || degree > 3 {
        format!("{degree}SL")
    } else {
        format!("{degree}SK")
    };
    if grouped {
        name.insert(0, 'g');
    }
    format!("{name}{suffix}")
}

fn digit_list(digits: CandidateMask) -> String {
    digits
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn full_cell_list(
    topology: &ConstraintTopology,
    region: RegionId,
    positions: PositionMask,
) -> String {
    let mut result = if positions.count() == 1 {
        String::from("Cell ")
    } else {
        String::from("Cells ")
    };
    for (ordinal, position) in positions.iter().enumerate() {
        if ordinal != 0 {
            result.push(',');
        }
        let cell = CellId::new(topology.region_cells(region)[usize::from(position)])
            .expect("region cell index");
        result.push_str(&cell.to_string());
    }
    result
}

fn full_cell_sequence(cells: CellSequence) -> String {
    let mut result = if cells.len() == 1 {
        String::from("Cell ")
    } else {
        String::from("Cells ")
    };
    for (ordinal, cell) in cells.iter().enumerate() {
        if ordinal != 0 {
            result.push(',');
        }
        result.push_str(&cell.to_string());
    }
    result
}

fn full_cell_chain(cells: CellSequence) -> String {
    let mut result = String::from("Cell ");
    for (ordinal, cell) in cells.iter().enumerate() {
        if ordinal != 0 {
            result.push(',');
        }
        result.push_str(&cell.to_string());
    }
    result
}

#[must_use]
pub fn region_family_name(type_index: usize) -> &'static str {
    match type_index {
        0 => "block",
        1 => "row",
        2 => "column",
        3 => "disjoint group",
        4 => "window group",
        5 => "Main Diagonal",
        6 => "Anti Diagonal",
        7 => "Girandola group",
        8 => "Asterisk group",
        9 => "Center Dot group",
        _ => unreachable!("region type is bounded"),
    }
}

#[must_use]
pub fn region_full_name(region: RegionId) -> String {
    match region.type_index() {
        0..=4 => format!(
            "{} {}",
            region_family_name(region.type_index()),
            region.region_index() + 1
        ),
        5..=9 => region_family_name(region.type_index()).to_owned(),
        _ => unreachable!("region type is bounded"),
    }
}

#[cfg(test)]
mod tests {
    use sukaku_forge_core::RegionId;

    use super::{
        four_strong_links_name, four_strong_links_short_name, four_strong_links_suffix,
        three_strong_links_name, three_strong_links_short_name, three_strong_links_suffix,
    };

    #[test]
    fn three_strong_links_names_cover_open_ring_and_grouped_classes() {
        let block = RegionId::new(0, 0).unwrap();
        let rows = [
            RegionId::new(1, 0).unwrap(),
            RegionId::new(1, 1).unwrap(),
            RegionId::new(1, 2).unwrap(),
        ];
        let column = RegionId::new(2, 0).unwrap();
        let order = [0, 1, 2];

        assert_eq!(three_strong_links_suffix(rows, order, [false; 3]), "0111");
        assert_eq!(
            three_strong_links_name(rows, order, false, [false; 3]),
            "3 Skyscrapers 0111"
        );
        assert_eq!(
            three_strong_links_short_name(rows, order, false, [false; 3]),
            "3SS0111"
        );
        assert_eq!(
            three_strong_links_name(rows, order, true, [false; 3]),
            "(3 Strong Links) X-Loop 0111"
        );
        assert_eq!(
            three_strong_links_short_name(rows, order, true, [false; 3]),
            "3XL0111"
        );
        assert_eq!(
            three_strong_links_name(rows, order, true, [true, false, false]),
            "(3 Strong Links) Grouped X-Loop 1111"
        );
        assert_eq!(
            three_strong_links_short_name(rows, order, true, [true, false, false]),
            "g3XL1111"
        );
        assert_eq!(
            three_strong_links_name([rows[0], rows[1], column], order, false, [false; 3]),
            "3-String Kite 0112"
        );
        assert_eq!(
            three_strong_links_short_name([rows[0], rows[1], column], order, false, [false; 3]),
            "3SK0112"
        );
        assert_eq!(
            three_strong_links_name([block, rows[0], column], order, false, [false; 3]),
            "3 Strong links 0012"
        );
        assert_eq!(
            three_strong_links_short_name([block, rows[0], column], order, false, [false; 3]),
            "3SL0012"
        );
    }

    #[test]
    fn four_strong_links_names_preserve_java_degree_four_classes() {
        let block = RegionId::new(0, 0).unwrap();
        let rows = [
            RegionId::new(1, 0).unwrap(),
            RegionId::new(1, 1).unwrap(),
            RegionId::new(1, 2).unwrap(),
            RegionId::new(1, 3).unwrap(),
        ];
        let column = RegionId::new(2, 0).unwrap();
        let order = [0, 1, 2, 3];

        assert_eq!(four_strong_links_suffix(rows, order, [false; 4]), "01111");
        assert_eq!(
            four_strong_links_name(rows, order, false, [false; 4]),
            "4 Skyscrapers 01111"
        );
        assert_eq!(
            four_strong_links_short_name(rows, order, false, [false; 4]),
            "4SS01111"
        );
        assert_eq!(
            four_strong_links_name(rows, order, true, [false; 4]),
            "(4 Strong Links) X-Loop 01111"
        );
        assert_eq!(
            four_strong_links_short_name(rows, order, true, [false; 4]),
            "4XL01111"
        );
        assert_eq!(
            four_strong_links_name(
                [rows[0], rows[1], rows[2], column],
                order,
                false,
                [false; 4],
            ),
            "4 Strong links 01112"
        );
        assert_eq!(
            four_strong_links_short_name(
                [rows[0], rows[1], rows[2], column],
                order,
                false,
                [false; 4],
            ),
            "4SL01112"
        );
        assert_eq!(
            four_strong_links_name(
                [block, rows[0], rows[1], column],
                order,
                true,
                [true, false, false, false],
            ),
            "(4 Strong Links) Grouped X-Loop 10112"
        );
        assert_eq!(
            four_strong_links_short_name(
                [block, rows[0], rows[1], column],
                order,
                true,
                [true, false, false, false],
            ),
            "g4XL10112"
        );
    }
}
