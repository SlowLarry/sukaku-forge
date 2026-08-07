//! Minimal corrected SE 1.2.1-derived classic 9×9 rating product.
//!
//! The original producer schedule remains recognizable, confirmed later fixes
//! take precedence over known defects, and uniqueness assumptions are opt-in.

use std::fmt;
use std::sync::Arc;

use sukaku_forge_core::{ConstraintTopology, Grid, ParsePuzzleError, Puzzle, VariantConfig};
use sukaku_forge_engine::{Se121Options, Se121Rating, Se121Solver, Se121VariantError};

/// Reuses the immutable classic topology across an entire rating batch.
#[derive(Clone, Debug)]
pub struct ClassicRater {
    topology: Arc<ConstraintTopology>,
    options: Se121Options,
}

impl ClassicRater {
    #[must_use]
    pub fn new() -> Self {
        Self {
            topology: Arc::new(ConstraintTopology::new(VariantConfig::default())),
            options: Se121Options::default(),
        }
    }

    /// Enable or disable techniques which assume that the puzzle has exactly
    /// one solution.
    ///
    /// Unique Loops and BUG are disabled by default. Enabling them restores
    /// their positions in the original SE 1.2.1 producer schedule.
    #[must_use]
    pub fn with_uniqueness(mut self, allow_uniqueness: bool) -> Self {
        self.options.allow_uniqueness = allow_uniqueness;
        self
    }

    /// Whether Unique Loops and BUG are enabled for this rater.
    #[must_use]
    pub const fn allows_uniqueness(&self) -> bool {
        self.options.allow_uniqueness
    }

    /// Parse and rate one 81-character classic Sudoku value grid.
    pub fn rate_text(&self, text: &str) -> Result<Se121Rating, RateError> {
        let text = text.trim();
        if text.len() != 81 {
            return Err(RateError::InvalidLength { actual: text.len() });
        }
        let puzzle = Puzzle::parse(text).map_err(RateError::Parse)?;
        let grid = Grid::from_classic_puzzle(Arc::clone(&self.topology), &puzzle);
        Se121Solver
            .rate_with_options(grid, self.options)
            .map_err(RateError::Variant)
    }
}

impl Default for ClassicRater {
    fn default() -> Self {
        Self::new()
    }
}

/// Invalid input at the intentionally narrow classic-rater boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RateError {
    InvalidLength { actual: usize },
    Parse(ParsePuzzleError),
    Variant(Se121VariantError),
}

impl fmt::Display for RateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { actual } => {
                write!(
                    formatter,
                    "puzzle must contain exactly 81 values, got {actual}"
                )
            }
            Self::Parse(error) => error.fmt(formatter),
            Self::Variant(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidLength { .. } => None,
            Self::Parse(error) => Some(error),
            Self::Variant(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ClassicRater, RateError};
    use sukaku_forge_engine::{Rating, Se121Rating};

    const SOLVED: &str =
        "123456789456789123789123456214365897365897214897214365531642978642978531978531642";
    const BUG_DEPENDENT_CLASSIC: &str =
        "1.3.5..8...67.9.2.............3....7.6.......8...14..55316...7......8....7....6..";

    #[test]
    fn accepts_dot_and_zero_value_grids() {
        let rater = ClassicRater::new();
        let dots = SOLVED.replacen('1', ".", 1);
        let zeros = SOLVED.replacen('1', "0", 1);
        assert_eq!(
            rater.rate_text(&dots).unwrap(),
            rater.rate_text(&zeros).unwrap()
        );
    }

    #[test]
    fn uniqueness_techniques_require_an_explicit_opt_in() {
        assert!(!ClassicRater::new().allows_uniqueness());
        assert!(
            ClassicRater::new()
                .with_uniqueness(true)
                .allows_uniqueness()
        );

        assert_eq!(
            ClassicRater::new()
                .rate_text(BUG_DEPENDENT_CLASSIC)
                .unwrap(),
            Se121Rating::new(
                Rating::from_tenths(71),
                Rating::from_tenths(12),
                Rating::from_tenths(12),
            ),
        );
        assert_eq!(
            ClassicRater::new()
                .with_uniqueness(true)
                .rate_text(BUG_DEPENDENT_CLASSIC)
                .unwrap(),
            Se121Rating::new(
                Rating::from_tenths(57),
                Rating::from_tenths(12),
                Rating::from_tenths(12),
            ),
        );
    }

    #[test]
    fn rejects_non_value_and_non_ascii_inputs() {
        let rater = ClassicRater::new();
        assert_eq!(
            rater.rate_text(&".".repeat(80)),
            Err(RateError::InvalidLength { actual: 80 })
        );
        assert!(matches!(
            rater.rate_text(&format!("{}x", &SOLVED[..80])),
            Err(RateError::Parse(_))
        ));
        assert_eq!(
            rater.rate_text(&"123456789".repeat(81)),
            Err(RateError::InvalidLength { actual: 729 })
        );
    }

    #[test]
    fn corrected_nonprotected_corpus_matches_frozen_expectations() {
        let cases = [
            (
                "53..7....6..195....98....6.8...6...34..8.3..17...2...6.6....28....419..5....8..79",
                (12, 12, 12),
            ),
            (
                "200080300060070084030500209000105408000000000402706000301007040720040060004010003",
                (12, 12, 12),
            ),
            (
                "000000907000420180000705026100904000050000040000507009920108000034059000507000000",
                (20, 12, 12),
            ),
            (
                "4.....8.5.3..........7......2.....6.....8.4......1.......6.3.7.5..2.....1.4......",
                (26, 12, 12),
            ),
            (
                "100000002520070049009000500000689000000703000090105030640010025010000070900000008",
                (89, 15, 15),
            ),
            (
                "300205000000000010008060200000007604009300000600080000000000920500104006070000005",
                (93, 12, 12),
            ),
            (
                ".3...89.2..2....4.......567...76...34...53........485.96..3....28.41.6.......617.",
                (72, 12, 12),
            ),
            (
                "........1.....2....34..........5..6...17..3..8....9..4...6...7...8..4..9.2..3.5..",
                (98, 98, 95),
            ),
        ];
        let rater = ClassicRater::new().with_uniqueness(true);
        for (puzzle, (er, ep, ed)) in cases {
            assert_eq!(
                rater.rate_text(puzzle).unwrap(),
                Se121Rating::new(
                    Rating::from_tenths(er),
                    Rating::from_tenths(ep),
                    Rating::from_tenths(ed),
                ),
                "corrected Classic rating mismatch for {puzzle}"
            );
        }
    }
}
