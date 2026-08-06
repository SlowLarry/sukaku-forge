//! Ordered, trace-compatible logical inference over `sukaku-forge-core` state.

mod aligned_exclusion;
mod alphabet_wings;
mod bug;
mod config;
mod forcing_chains;
mod inference;
mod multiple_chains;
mod nested_chains;
mod nishio;
mod non_consecutive;
mod presentation_proof;
mod producers;
mod rating;
mod sets;
mod solver;
mod strong_links;
mod unique_loops;
mod wings;

pub use aligned_exclusion::{find_aligned_pair_exclusion, find_aligned_triplet_exclusion};
pub use alphabet_wings::find_alphabet_wing;
pub use bug::find_bivalue_universal_grave;
pub use config::{EngineConfig, RatingMode, SearchPolicy, TechniqueGate, TechniqueSet};
pub use forcing_chains::{find_forcing_chain_cycle, find_forcing_chain_cycle_with_proof};
pub use inference::{
    AlignedPairCombinationSequence, AlignedTripletCombinationIter,
    AlignedTripletCombinationSequence, BugCellSequence, BugKind, CellSequence, ChainCellSequence,
    ChainKind, Evidence, Inference, MultipleChainKind, Technique, UniqueLoopKind,
    region_family_name, region_full_name,
};
pub use multiple_chains::{
    LegacyFcPlusBoundary, find_dynamic_forcing_chain, find_dynamic_forcing_chain_plus,
    find_dynamic_forcing_chain_plus_checked, find_dynamic_forcing_chain_plus_with_proof,
    find_dynamic_forcing_chain_plus_with_proof_checked, find_dynamic_forcing_chain_with_proof,
    find_multiple_forcing_chain, find_multiple_forcing_chain_with_proof, find_nested_forcing_chain,
    find_nested_forcing_chain_checked, find_nested_forcing_chain_with_proof,
    find_nested_forcing_chain_with_proof_checked,
};
pub use nishio::{find_nishio_forcing_chain, find_nishio_forcing_chain_with_proof};
pub use non_consecutive::{
    NonConsecutiveCellSequence, NonConsecutiveDigitSequence, NonConsecutiveGeometry,
    NonConsecutiveHint, NonConsecutiveHintKind, find_forcing_cell_ferz_non_consecutive,
    find_forcing_cell_non_consecutive, find_locked_ferz_non_consecutive,
    find_locked_non_consecutive,
};
pub use presentation_proof::{
    ChainCause, ChainNodeId, ChainProofNode, ChainProofParent, ChainProofView, ChainProofViewKind,
    ChainState, ForcingChainWithProof, MultipleForcingChainWithProof, NishioForcingChainWithProof,
    SelectedChainProof,
};
pub use producers::{
    find_direct_hidden_set, find_direct_locking, find_generalized_intersections,
    find_hidden_single, find_locking, find_naked_single,
};
pub use rating::{RatedTechnique, Rating, RatingResult, RatingTracker};
pub use sets::{find_fish, find_hidden_set, find_naked_set};
pub use solver::{
    PortGap, PresentationInference, PresentationSearchOutcome, ProducerKind, ProducerSpec,
    ProducerState, RateOutcome, SearchOutcome, Solver,
};
pub use strong_links::{find_four_strong_links, find_three_strong_links, find_two_strong_links};
pub use unique_loops::find_unique_loop;
pub use wings::find_wing;
