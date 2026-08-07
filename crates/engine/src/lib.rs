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

pub use aligned_exclusion::{
    collect_aligned_pair_exclusions, collect_aligned_triplet_exclusions,
    find_aligned_pair_exclusion, find_aligned_triplet_exclusion,
};
pub use alphabet_wings::{collect_alphabet_wing, find_alphabet_wing};
pub use bug::{collect_bivalue_universal_grave, find_bivalue_universal_grave};
pub use config::{EngineConfig, RatingMode, SearchPolicy, TechniqueGate, TechniqueSet};
pub use forcing_chains::{
    collect_forcing_chain_cycles, find_forcing_chain_cycle, find_forcing_chain_cycle_with_proof,
    replay_forcing_chain_cycle_proof,
};
pub use inference::{
    AlignedPairCombinationSequence, AlignedTripletCombinationIter,
    AlignedTripletCombinationSequence, BugCellSequence, BugKind, CellSequence, ChainCellSequence,
    ChainKind, Evidence, Inference, MultipleChainKind, Technique, UniqueLoopKind,
    region_family_name, region_full_name,
};
pub use multiple_chains::{
    LegacyFcPlusBoundary, collect_dynamic_forcing_chain_plus_checked,
    collect_dynamic_forcing_chains, collect_multiple_forcing_chains,
    collect_nested_forcing_chains_checked, find_dynamic_forcing_chain,
    find_dynamic_forcing_chain_plus, find_dynamic_forcing_chain_plus_checked,
    find_dynamic_forcing_chain_plus_with_proof, find_dynamic_forcing_chain_plus_with_proof_checked,
    find_dynamic_forcing_chain_with_proof, find_multiple_forcing_chain,
    find_multiple_forcing_chain_with_proof, find_nested_forcing_chain,
    find_nested_forcing_chain_checked, find_nested_forcing_chain_with_proof,
    find_nested_forcing_chain_with_proof_checked, replay_dynamic_forcing_chain_plus_proof,
    replay_dynamic_forcing_chain_proof, replay_multiple_forcing_chain_proof,
    replay_nested_forcing_chain_proof_checked,
};
pub use nishio::{
    collect_nishio_forcing_chains, find_nishio_forcing_chain, find_nishio_forcing_chain_with_proof,
    replay_nishio_forcing_chain_proof,
};
pub use non_consecutive::{
    NonConsecutiveCellSequence, NonConsecutiveDigitSequence, NonConsecutiveGeometry,
    NonConsecutiveHint, NonConsecutiveHintKind, collect_forcing_cell_ferz_non_consecutive,
    collect_forcing_cell_non_consecutive, collect_locked_ferz_non_consecutive,
    collect_locked_non_consecutive, find_forcing_cell_ferz_non_consecutive,
    find_forcing_cell_non_consecutive, find_locked_ferz_non_consecutive,
    find_locked_non_consecutive,
};
pub use presentation_proof::{
    ChainCause, ChainNodeId, ChainProofNode, ChainProofParent, ChainProofView, ChainProofViewKind,
    ChainState, ForcingChainWithProof, MultipleForcingChainWithProof, NishioForcingChainWithProof,
    SelectedChainProof,
};
pub use producers::{
    collect_direct_hidden_sets, collect_direct_locking, collect_generalized_intersections,
    collect_hidden_singles, collect_locking, collect_naked_singles, find_direct_hidden_set,
    find_direct_locking, find_generalized_intersections, find_hidden_single, find_locking,
    find_naked_single,
};
pub use rating::{RatedTechnique, Rating, RatingResult, RatingTracker};
pub use sets::{
    collect_fish, collect_hidden_sets, collect_naked_sets, find_fish, find_hidden_set,
    find_naked_set,
};
pub use solver::{
    AllHintsSearchOutcome, CollectedInference, HintCategory, PortGap, PresentationInference,
    PresentationSearchOutcome, ProducerKind, ProducerSpec, ProducerState, RateOutcome,
    SearchOutcome, Solver,
};
pub use strong_links::{
    collect_four_strong_links, collect_three_strong_links, collect_two_strong_links,
    find_four_strong_links, find_three_strong_links, find_two_strong_links,
};
pub use unique_loops::{collect_unique_loop, find_unique_loop};
pub use wings::{collect_wings, find_wing};
