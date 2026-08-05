//! Compact, UI-independent state and topology for Sukaku Forge.

mod candidate_mask;
mod candidate_removals;
mod cell_mask;
mod config;
mod grid;
mod ids;
mod puzzle;
mod topology;

pub use candidate_mask::{CandidateMask, DigitIter, PositionIter, PositionMask};
pub use candidate_removals::{CandidateRemoval, CandidateRemovals, CandidateRemovalsBuilder};
pub use cell_mask::{CellIter, CellMask};
pub use config::{NonConsecutiveMode, VariantConfig};
pub use grid::{Grid, GridStateError};
pub use ids::{CellId, Digit, RegionId};
pub use puzzle::{ParsePuzzleError, Puzzle};
pub use topology::{ConstraintTopology, REGION_TYPE_COUNT, write_all_java_topologies};
