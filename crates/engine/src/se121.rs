//! Corrected, rating-only Sudoku Explainer 1.2.1-derived producer schedule.
//!
//! This is intentionally independent from [`crate::Solver`], whose
//! compatibility contract targets the later Sukaku Explainer registry. The
//! dedicated headless rater may optimize this path aggressively. Confirmed
//! correctness fixes may deliberately diverge from the pristine oracle, while
//! the producer order and numeric ER/EP/ED state machine remain stable.

use std::fmt;

use sukaku_forge_core::{Grid, VariantConfig};

use crate::aligned_exclusion::{
    find_aligned_pair_exclusion_se121, find_aligned_triplet_exclusion_se121,
};
use crate::bug::find_bivalue_universal_grave_se121;
use crate::multiple_chains::find_se121_chain_tail;
use crate::producers::{find_direct_locking_se121, find_locking_se121};
use crate::wings::find_wing_se121;

use crate::{
    EngineConfig, Inference, Rating, RatingMode, SearchPolicy, TechniqueSet,
    find_direct_hidden_set, find_fish, find_forcing_chain_cycle, find_hidden_set,
    find_hidden_single, find_naked_set, find_naked_single, find_nishio_forcing_chain,
    find_unique_loop,
};

/// Classic-only settings for the corrected SE 1.2.1-derived rater.
///
/// The producer schedule and numeric rating table remain rooted in 1.2.1,
/// while confirmed later correctness fixes take precedence over reproducing
/// known defects in the old oracle.
pub const SE121_ENGINE_CONFIG: EngineConfig = EngineConfig {
    variant_latin: false,
    rating_mode: RatingMode::Original,
    search_policy: SearchPolicy::Compatibility,
    forcing_chain_plus: 0,
    unique_loop_fix: true,
    bug_fix: true,
    enabled_techniques: TechniqueSet::ALL,
    java_default_technique_profile: false,
};

/// Product-level switches for the focused SE 1.2.1 rater.
///
/// Uniqueness-dependent deductions are deliberately disabled by default:
/// a puzzle may have zero, one, or several solutions unless the caller opts
/// into the unique-solution assumption explicitly.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Se121Options {
    pub allow_uniqueness: bool,
}

impl Se121Options {
    /// Corrected-profile options with uniqueness assumptions enabled.
    #[must_use]
    pub const fn allowing_uniqueness() -> Self {
        Self {
            allow_uniqueness: true,
        }
    }

    #[must_use]
    const fn enables(self, producer: Se121Producer) -> bool {
        self.allow_uniqueness
            || !matches!(
                producer,
                Se121Producer::UniqueLoops | Se121Producer::BivalueUniversalGrave
            )
    }
}

/// One producer in the immutable SE 1.2.1 classic registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Se121Producer {
    HiddenSingle,
    DirectLocking,
    DirectHiddenPair,
    NakedSingle,
    DirectHiddenTriplet,
    Locking,
    NakedPair,
    XWing,
    HiddenPair,
    NakedTriplet,
    Swordfish,
    HiddenTriplet,
    XYWing,
    XYZWing,
    UniqueLoops,
    NakedQuad,
    Jellyfish,
    HiddenQuad,
    BivalueUniversalGrave,
    AlignedPairExclusion,
    ForcingChainCycle,
    AlignedTripletExclusion,
    NishioForcingChain,
    MultipleForcingChain,
    DynamicForcingChain,
    DynamicForcingChainPlus,
    NestedForcingChain { level: u8 },
}

/// Canonical `Solver.getDifficulty()` order from upstream tag `v1.2.1.2`.
pub const SE121_PRODUCERS: [Se121Producer; 30] = [
    Se121Producer::HiddenSingle,
    Se121Producer::DirectLocking,
    Se121Producer::DirectHiddenPair,
    Se121Producer::NakedSingle,
    Se121Producer::DirectHiddenTriplet,
    Se121Producer::Locking,
    Se121Producer::NakedPair,
    Se121Producer::XWing,
    Se121Producer::HiddenPair,
    Se121Producer::NakedTriplet,
    Se121Producer::Swordfish,
    Se121Producer::HiddenTriplet,
    Se121Producer::XYWing,
    Se121Producer::XYZWing,
    Se121Producer::UniqueLoops,
    Se121Producer::NakedQuad,
    Se121Producer::Jellyfish,
    Se121Producer::HiddenQuad,
    Se121Producer::BivalueUniversalGrave,
    Se121Producer::AlignedPairExclusion,
    Se121Producer::ForcingChainCycle,
    Se121Producer::AlignedTripletExclusion,
    Se121Producer::NishioForcingChain,
    Se121Producer::MultipleForcingChain,
    Se121Producer::DynamicForcingChain,
    Se121Producer::DynamicForcingChainPlus,
    Se121Producer::NestedForcingChain { level: 2 },
    Se121Producer::NestedForcingChain { level: 3 },
    Se121Producer::NestedForcingChain { level: 4 },
    Se121Producer::NestedForcingChain { level: 5 },
];

/// Numeric result emitted by the dedicated headless rater.
///
/// Keeping only the three exact-tenths values avoids the general engine's
/// technique-name allocations whenever a new maximum is observed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Se121Rating {
    er: Rating,
    ep: Rating,
    ed: Rating,
}

impl Se121Rating {
    #[must_use]
    pub const fn new(er: Rating, ep: Rating, ed: Rating) -> Self {
        Self { er, ep, ed }
    }

    #[must_use]
    pub const fn er(self) -> Rating {
        self.er
    }

    #[must_use]
    pub const fn ep(self) -> Rating {
        self.ep
    }

    #[must_use]
    pub const fn ed(self) -> Rating {
        self.ed
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Se121RatingTracker {
    result: Se121Rating,
}

impl Se121RatingTracker {
    fn observe(&mut self, inference: &Inference) {
        if inference.rating() > self.result.er {
            self.result.er = inference.rating();
        }
        if self.result.ep == Rating::default() {
            if self.result.ed == Rating::default() {
                self.result.ed = self.result.er;
            }
            if inference.is_placement() {
                self.result.ep = self.result.er;
            }
        }
    }

    fn beyond_solver(mut self) -> Se121Rating {
        // Canonical Solver.getDifficulty() sets only ER to 20.0 when no hint
        // exists. EP and ED retain the values established by earlier steps.
        self.result.er = Rating::from_tenths(200);
        self.result
    }
}

/// A non-Classic topology supplied to the frozen SE 1.2.1 rater.
///
/// Several old-order producers intentionally use a hard-coded Classic peer
/// catalog. Rejecting variants at the public solver boundary keeps those
/// internal paths from silently producing invalid variant deductions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Se121VariantError {
    actual: VariantConfig,
}

impl Se121VariantError {
    #[must_use]
    pub const fn actual(self) -> VariantConfig {
        self.actual
    }
}

impl fmt::Display for Se121VariantError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SE 1.2.1 rating supports only Classic 9x9 Sudoku")
    }
}

impl std::error::Error for Se121VariantError {}

/// Zero-sized executor for the frozen SE 1.2.1 registry.
#[derive(Clone, Copy, Debug, Default)]
pub struct Se121Solver;

impl Se121Solver {
    /// Return the first accepted inference with uniqueness deductions disabled.
    pub fn next_inference(self, grid: &Grid) -> Result<Option<Inference>, Se121VariantError> {
        self.next_inference_with_options(grid, Se121Options::default())
    }

    /// Return the first accepted inference under explicit product options.
    pub fn next_inference_with_options(
        self,
        grid: &Grid,
        options: Se121Options,
    ) -> Result<Option<Inference>, Se121VariantError> {
        Self::ensure_classic(grid)?;
        Ok(self.next_classic_inference(grid, options))
    }

    fn next_classic_inference(self, grid: &Grid, options: Se121Options) -> Option<Inference> {
        for producer in SE121_PRODUCERS {
            if !options.enables(producer) {
                continue;
            }
            let inference = match producer {
                Se121Producer::HiddenSingle => find_hidden_single(grid, SE121_ENGINE_CONFIG),
                Se121Producer::DirectLocking => find_direct_locking_se121(grid),
                Se121Producer::DirectHiddenPair => {
                    find_direct_hidden_set(grid, SE121_ENGINE_CONFIG, 2)
                }
                Se121Producer::NakedSingle => find_naked_single(grid, SE121_ENGINE_CONFIG),
                Se121Producer::DirectHiddenTriplet => {
                    find_direct_hidden_set(grid, SE121_ENGINE_CONFIG, 3)
                }
                Se121Producer::Locking => find_locking_se121(grid),
                Se121Producer::NakedPair => find_naked_set(grid, SE121_ENGINE_CONFIG, 2, false),
                Se121Producer::XWing => find_fish(grid, SE121_ENGINE_CONFIG, 2),
                Se121Producer::HiddenPair => find_hidden_set(grid, SE121_ENGINE_CONFIG, 2),
                Se121Producer::NakedTriplet => find_naked_set(grid, SE121_ENGINE_CONFIG, 3, false),
                Se121Producer::Swordfish => find_fish(grid, SE121_ENGINE_CONFIG, 3),
                Se121Producer::HiddenTriplet => find_hidden_set(grid, SE121_ENGINE_CONFIG, 3),
                Se121Producer::XYWing => find_wing_se121(grid, false),
                Se121Producer::XYZWing => find_wing_se121(grid, true),
                Se121Producer::UniqueLoops => find_unique_loop(grid, SE121_ENGINE_CONFIG),
                Se121Producer::NakedQuad => find_naked_set(grid, SE121_ENGINE_CONFIG, 4, false),
                Se121Producer::Jellyfish => find_fish(grid, SE121_ENGINE_CONFIG, 4),
                Se121Producer::HiddenQuad => find_hidden_set(grid, SE121_ENGINE_CONFIG, 4),
                Se121Producer::BivalueUniversalGrave => {
                    find_bivalue_universal_grave_se121(grid, SE121_ENGINE_CONFIG)
                }
                Se121Producer::AlignedPairExclusion => find_aligned_pair_exclusion_se121(grid),
                Se121Producer::ForcingChainCycle => {
                    find_forcing_chain_cycle(grid, SE121_ENGINE_CONFIG)
                }
                Se121Producer::AlignedTripletExclusion => {
                    find_aligned_triplet_exclusion_se121(grid)
                }
                Se121Producer::NishioForcingChain => {
                    find_nishio_forcing_chain(grid, SE121_ENGINE_CONFIG)
                }
                // These seven producers are consecutive. The rating-only
                // path searches them together so their large branch arenas,
                // and weak implications can be reused without changing
                // producer or discovery order.
                Se121Producer::MultipleForcingChain => {
                    return find_se121_chain_tail(grid, SE121_ENGINE_CONFIG);
                }
                Se121Producer::DynamicForcingChain
                | Se121Producer::DynamicForcingChainPlus
                | Se121Producer::NestedForcingChain { .. } => {
                    unreachable!("SE121 chain tail is searched from its first producer")
                }
            };
            if inference.is_some() {
                return inference;
            }
        }
        None
    }

    /// Consume and rate one classic grid without cloning it or assuming a
    /// unique solution.
    pub fn rate(self, grid: Grid) -> Result<Se121Rating, Se121VariantError> {
        self.rate_with_options(grid, Se121Options::default())
    }

    /// Consume and rate one classic grid under explicit product options.
    pub fn rate_with_options(
        self,
        mut grid: Grid,
        options: Se121Options,
    ) -> Result<Se121Rating, Se121VariantError> {
        Self::ensure_classic(&grid)?;
        let mut tracker = Se121RatingTracker::default();
        loop {
            if grid.is_solved() {
                return Ok(tracker.result);
            }
            let Some(inference) = self.next_classic_inference(&grid, options) else {
                return Ok(tracker.beyond_solver());
            };
            tracker.observe(&inference);
            inference.apply(&mut grid);
        }
    }

    fn ensure_classic(grid: &Grid) -> Result<(), Se121VariantError> {
        let actual = grid.topology().config();
        if actual == VariantConfig::default() {
            Ok(())
        } else {
            Err(Se121VariantError { actual })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{ConstraintTopology, Grid, Puzzle, VariantConfig};

    use super::{
        SE121_ENGINE_CONFIG, SE121_PRODUCERS, Se121Options, Se121Producer, Se121RatingTracker,
        Se121Solver,
    };
    use crate::{Rating, RatingMode, SearchPolicy, Technique};

    const BUG_DEPENDENT_CLASSIC: &str =
        "1.3.5..8...67.9.2.............3....7.6.......8...14..55316...7......8....7....6..";

    fn classic_grid(text: &str) -> Grid {
        let puzzle = Puzzle::parse(text).unwrap();
        Grid::from_puzzle(
            Arc::new(ConstraintTopology::new(VariantConfig::default())),
            &puzzle,
        )
    }

    #[test]
    fn corrected_profile_keeps_original_ratings_and_applies_uniqueness_fixes() {
        assert_eq!(SE121_ENGINE_CONFIG.rating_mode, RatingMode::Original);
        assert_eq!(
            SE121_ENGINE_CONFIG.search_policy,
            SearchPolicy::Compatibility
        );
        assert_eq!(SE121_ENGINE_CONFIG.forcing_chain_plus, 0);
        const {
            assert!(SE121_ENGINE_CONFIG.unique_loop_fix);
            assert!(SE121_ENGINE_CONFIG.bug_fix);
            assert!(!SE121_ENGINE_CONFIG.java_default_technique_profile);
        }
    }

    #[test]
    fn registry_ends_with_one_old_nested_producer_per_level() {
        assert_eq!(SE121_PRODUCERS.len(), 30);
        assert_eq!(
            &SE121_PRODUCERS[26..],
            &[
                Se121Producer::NestedForcingChain { level: 2 },
                Se121Producer::NestedForcingChain { level: 3 },
                Se121Producer::NestedForcingChain { level: 4 },
                Se121Producer::NestedForcingChain { level: 5 },
            ]
        );
        assert!(!SE121_PRODUCERS.iter().any(|producer| matches!(
            producer,
            Se121Producer::NestedForcingChain { level: 0 | 1 | 6.. }
        )));
    }

    #[test]
    fn uniqueness_dependent_producers_are_opt_in() {
        let defaults = Se121Options::default();
        assert!(!defaults.allow_uniqueness);
        assert!(!defaults.enables(Se121Producer::UniqueLoops));
        assert!(!defaults.enables(Se121Producer::BivalueUniversalGrave));
        assert!(defaults.enables(Se121Producer::XYWing));

        let opted_in = Se121Options::allowing_uniqueness();
        assert!(opted_in.enables(Se121Producer::UniqueLoops));
        assert!(opted_in.enables(Se121Producer::BivalueUniversalGrave));
    }

    #[test]
    fn public_options_select_bug_only_after_uniqueness_opt_in() {
        // This 23-clue value puzzle has exactly one Classic solution. Its
        // corrected SE121 path reaches a real BUG deduction when uniqueness
        // is enabled; the default public path must choose a later producer at
        // that same full-grid state.
        let mut grid = classic_grid(BUG_DEPENDENT_CLASSIC);
        let mut saw_bug = false;
        for _ in 0..512 {
            if grid.is_solved() {
                break;
            }
            let inference = Se121Solver
                .next_inference_with_options(&grid, Se121Options::allowing_uniqueness())
                .unwrap()
                .expect("uniqueness-enabled fixture must remain solvable");
            if inference.technique() == Technique::BivalueUniversalGrave {
                saw_bug = true;
                let default_inference = Se121Solver
                    .next_inference(&grid)
                    .unwrap()
                    .expect("default path must continue with a non-unique technique");
                assert!(!matches!(
                    default_inference.technique(),
                    Technique::UniqueLoop | Technique::BivalueUniversalGrave
                ));
            }
            inference.apply(&mut grid);
        }
        assert!(
            grid.is_solved(),
            "uniqueness-enabled fixture did not finish"
        );
        assert!(saw_bug, "fixture no longer selects BUG");
    }

    #[test]
    fn solved_grid_rates_zero_without_searching() {
        let grid = classic_grid(
            "123456789456789123789123456214365897365897214897214365531642978642978531978531642",
        );
        let rating = Se121Solver.rate(grid).unwrap();
        assert_eq!(rating.er(), Rating::default());
        assert_eq!(rating.ep(), Rating::default());
        assert_eq!(rating.ed(), Rating::default());
    }

    #[test]
    fn public_solver_boundary_rejects_variant_grids() {
        let variant = VariantConfig {
            sudoku_x: true,
            ..VariantConfig::default()
        };
        let puzzle = Puzzle::parse(&".".repeat(81)).unwrap();
        let grid = Grid::from_puzzle(Arc::new(ConstraintTopology::new(variant)), &puzzle);

        let next_error = Se121Solver.next_inference(&grid).unwrap_err();
        assert_eq!(next_error.actual(), variant);
        assert_eq!(Se121Solver.rate(grid), Err(next_error));
    }

    #[test]
    fn beyond_solver_changes_only_er() {
        let mut tracker = Se121RatingTracker::default();
        tracker.result.er = Rating::from_tenths(85);
        tracker.result.ep = Rating::from_tenths(23);
        tracker.result.ed = Rating::from_tenths(15);
        let result = tracker.beyond_solver();
        assert_eq!(result.er(), Rating::from_tenths(200));
        assert_eq!(result.ep(), Rating::from_tenths(23));
        assert_eq!(result.ed(), Rating::from_tenths(15));
    }
}
