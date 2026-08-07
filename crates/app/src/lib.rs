//! Stateful application boundary shared by graphical Sukaku Forge clients.
//!
//! The session owns the mutable grid and solver. Clients receive immutable
//! primitive snapshots and opaque hint handles; they never apply an inference
//! payload supplied by the client.

pub mod port;

use core::array;
use core::fmt;

use sukaku_forge_core::{
    CandidateMask, CandidateRemovals, CandidateRemovalsBuilder, CellId, Digit, Grid,
    NonConsecutiveMode,
};
use sukaku_forge_engine::{
    AllHintsSearchOutcome, CollectedInference, HintCategory, Inference, PortGap,
    PresentationSearchOutcome, ProducerKind, Rating, SelectedChainProof, Solver, Technique,
};
use sukaku_forge_presentation::{
    HintPresentation, UnsupportedPresentation, present, present_with_selected_chain_proof,
};

/// Opaque identity of an inference retained by one [`Session`].
///
/// Clients may compare and return this value, but cannot construct one.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HintId(u64);

/// Exact frontend-facing state at one session revision.
///
/// Values are zero for unresolved cells. Candidate masks use Java-compatible
/// bits 1 through 9 (`0x03fe` is the full mask).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSnapshot {
    pub revision: u64,
    pub values: [u8; CellId::COUNT],
    pub candidate_masks: [u16; CellId::COUNT],
    pub givens: [bool; CellId::COUNT],
    pub can_undo: bool,
    pub can_redo: bool,
}

/// Result of asking the selected compatibility solver for its next inference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NextHintResponse {
    pub revision: u64,
    pub outcome: NextHintOutcome,
}

/// Lightweight identity and exact effects for one retained all-hints entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HintSummary {
    pub hint_id: HintId,
    pub category: HintCategory,
    pub group_key: String,
    pub group_name: String,
    pub technique: Technique,
    pub name: String,
    pub short_name: String,
    pub rating: Rating,
    pub effects: HintEffects,
    /// Exact outcome projection used by Java's greedy "filter similar" UI.
    /// Chain placements include the target cell's other candidates here even
    /// though applying the inference is still a value placement.
    pub filter_effects: HintEffects,
}

/// Revision-bound result of the legacy tiered `Get all hints` search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllHintsResponse {
    pub revision: u64,
    pub outcome: AllHintsOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AllHintsOutcome {
    Complete {
        hints: Vec<HintSummary>,
    },
    ConfirmationRequired,
    Incomplete {
        hints: Vec<HintSummary>,
        gap: PortGap,
    },
}

/// Full presentation result for one opaque all-hints entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializedHintResponse {
    pub revision: u64,
    pub hint_id: HintId,
    pub outcome: MaterializedHintOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum MaterializedHintOutcome {
    Presented {
        presentation: HintPresentation,
        effects: HintEffects,
    },
    Unsupported {
        unsupported: UnsupportedPresentation,
        effects: HintEffects,
    },
    Incomplete {
        gap: PortGap,
        effects: HintEffects,
    },
}

/// Exact application effects retained alongside a hint presentation.
///
/// The sparse removals preserve producer order. Transport adapters may derive
/// an elimination count with [`CandidateRemovals::elimination_count`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HintEffects {
    pub placement: Option<(CellId, Digit)>,
    pub removals: CandidateRemovals,
}

impl HintEffects {
    fn from_inference(inference: &Inference) -> Self {
        Self {
            placement: inference.placement_cell().zip(inference.placement_digit()),
            removals: inference.removals().clone(),
        }
    }

    fn for_legacy_filter(grid: &Grid, producer: ProducerKind, inference: &Inference) -> Self {
        let mut effects = Self::from_inference(inference);
        if producer.is_chaining_hint()
            && let Some((cell, digit)) = effects.placement
        {
            let other_candidates = grid.candidates(cell).without(CandidateMask::of(digit));
            let mut builder = CandidateRemovalsBuilder::with_capacity(1);
            builder.add(cell, other_candidates);
            effects.removals = builder.build();
        }
        effects
    }
}

/// Complete next-hint result without exposing the retained [`Inference`].
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum NextHintOutcome {
    Presented {
        hint_id: HintId,
        presentation: HintPresentation,
        effects: HintEffects,
    },
    Unsupported {
        hint_id: HintId,
        unsupported: UnsupportedPresentation,
        effects: HintEffects,
    },
    None,
    Incomplete {
        gap: PortGap,
    },
}

/// Rejected application command. Rejections never mutate the session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionError {
    StaleRevision { expected: u64, actual: u64 },
    UnknownHint { hint_id: HintId },
    NothingToUndo,
    NothingToRedo,
    GivenCell { cell: CellId },
    SolvedCell { cell: CellId },
    CandidateUnavailable { cell: CellId, digit: Digit },
    CandidateConflicts { cell: CellId, digit: Digit },
}

impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::StaleRevision { expected, actual } => {
                write!(
                    formatter,
                    "stale session revision {expected}; current revision is {actual}"
                )
            }
            Self::UnknownHint { hint_id } => {
                write!(
                    formatter,
                    "hint {hint_id:?} is not pending for this revision"
                )
            }
            Self::NothingToUndo => formatter.write_str("there is no session change to undo"),
            Self::NothingToRedo => formatter.write_str("there is no session change to redo"),
            Self::GivenCell { cell } => write!(formatter, "{cell} is a given cell"),
            Self::SolvedCell { cell } => write!(formatter, "{cell} already has a value"),
            Self::CandidateUnavailable { cell, digit } => {
                write!(formatter, "candidate {digit} is not available in {cell}")
            }
            Self::CandidateConflicts { cell, digit } => {
                write!(
                    formatter,
                    "candidate {digit} conflicts with a placed value at {cell}"
                )
            }
        }
    }
}

impl std::error::Error for SessionError {}

#[derive(Clone, Debug)]
struct PendingHint {
    id: HintId,
    inference: Inference,
    producer: Option<ProducerKind>,
    category: HintCategory,
    selected_chain_proof: Option<SelectedChainProof>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingCatalog {
    Complete,
    ConfirmationRequired,
    Incomplete(PortGap),
}

/// One authoritative puzzle editing and solving session.
#[derive(Clone, Debug)]
pub struct Session {
    grid: Grid,
    solver: Solver,
    revision: u64,
    history: Vec<Grid>,
    future: Vec<Grid>,
    pending_hints: Vec<PendingHint>,
    catalog_hint_ids: Vec<HintId>,
    pending_catalog: Option<PendingCatalog>,
    next_hint_id: u64,
}

impl Session {
    #[must_use]
    pub const fn new(grid: Grid, solver: Solver) -> Self {
        Self {
            grid,
            solver,
            revision: 0,
            history: Vec::new(),
            future: Vec::new(),
            pending_hints: Vec::new(),
            catalog_hint_ids: Vec::new(),
            pending_catalog: None,
            next_hint_id: 1,
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            revision: self.revision,
            values: array::from_fn(|index| self.grid.value(cell(index))),
            candidate_masks: array::from_fn(|index| self.grid.candidates(cell(index)).bits()),
            givens: array::from_fn(|index| self.grid.givens().contains(cell(index))),
            can_undo: !self.history.is_empty(),
            can_redo: !self.future.is_empty(),
        }
    }

    /// Find and retain the next inference for the current revision.
    ///
    /// Repeated calls without a mutation reuse the retained inference and its
    /// opaque ID, avoiding both a repeated search and observable ID churn.
    #[must_use]
    pub fn next_hint(&mut self) -> NextHintResponse {
        if self.pending_hints.is_empty() {
            self.pending_catalog = None;
            match self.solver.next_inference_with_selected_proof(&self.grid) {
                PresentationSearchOutcome::Found(selected) => {
                    let (inference, selected_chain_proof) = selected.into_parts();
                    let id = self.allocate_hint_id();
                    self.pending_hints.push(PendingHint {
                        id,
                        inference,
                        producer: None,
                        category: HintCategory::Indirect,
                        selected_chain_proof,
                    });
                }
                PresentationSearchOutcome::None => {
                    return NextHintResponse {
                        revision: self.revision,
                        outcome: NextHintOutcome::None,
                    };
                }
                PresentationSearchOutcome::Incomplete(gap) => {
                    return NextHintResponse {
                        revision: self.revision,
                        outcome: NextHintOutcome::Incomplete { gap },
                    };
                }
            }
        }

        let materialized = self.materialize_hint_at(0);
        let outcome = match materialized.outcome {
            MaterializedHintOutcome::Presented {
                presentation,
                effects,
            } => NextHintOutcome::Presented {
                hint_id: materialized.hint_id,
                presentation,
                effects,
            },
            MaterializedHintOutcome::Unsupported {
                unsupported,
                effects,
            } => NextHintOutcome::Unsupported {
                hint_id: materialized.hint_id,
                unsupported,
                effects,
            },
            MaterializedHintOutcome::Incomplete { gap, .. } => NextHintOutcome::Incomplete { gap },
        };
        NextHintResponse {
            revision: self.revision,
            outcome,
        }
    }

    /// Collect and retain the current legacy all-hints tier.
    ///
    /// Repeated calls reuse the same ordered opaque IDs. Passing
    /// `include_expensive` after a confirmation response continues into the
    /// first productive nested level, matching the desktop Java workflow.
    #[must_use]
    pub fn all_hints(&mut self, include_expensive: bool) -> AllHintsResponse {
        let should_recompute = self.pending_catalog.is_none()
            || matches!(
                self.pending_catalog,
                Some(PendingCatalog::ConfirmationRequired)
            ) && include_expensive;
        if should_recompute {
            self.catalog_hint_ids.clear();
            let outcome = self.solver.all_inferences(&self.grid, include_expensive);
            self.pending_catalog = Some(match outcome {
                AllHintsSearchOutcome::Complete(collected) => {
                    self.retain_collected(collected);
                    PendingCatalog::Complete
                }
                AllHintsSearchOutcome::ConfirmationRequired => PendingCatalog::ConfirmationRequired,
                AllHintsSearchOutcome::Incomplete { hints, gap } => {
                    self.retain_collected(hints);
                    PendingCatalog::Incomplete(gap)
                }
            });
        }
        self.all_hints_response()
    }

    /// Materialize one retained all-hints entry on demand.
    pub fn hint(
        &mut self,
        expected_revision: u64,
        hint_id: HintId,
    ) -> Result<MaterializedHintResponse, SessionError> {
        self.require_revision(expected_revision)?;
        let Some(index) = self
            .pending_hints
            .iter()
            .position(|pending| pending.id == hint_id)
        else {
            return Err(SessionError::UnknownHint { hint_id });
        };
        Ok(self.materialize_hint_at(index))
    }

    /// Apply the exact retained inference, never a client-supplied effect.
    pub fn apply_hint(
        &mut self,
        expected_revision: u64,
        hint_id: HintId,
    ) -> Result<SessionSnapshot, SessionError> {
        self.require_revision(expected_revision)?;
        let Some(index) = self
            .pending_hints
            .iter()
            .position(|pending| pending.id == hint_id)
        else {
            return Err(SessionError::UnknownHint { hint_id });
        };

        let inference = self.pending_hints[index].inference.clone();
        self.push_history();
        inference.apply(&mut self.grid);
        self.advance_revision();
        Ok(self.snapshot())
    }

    /// Place an available candidate as a value using normal grid propagation.
    pub fn place_value(
        &mut self,
        expected_revision: u64,
        cell: CellId,
        digit: Digit,
    ) -> Result<SessionSnapshot, SessionError> {
        self.require_revision(expected_revision)?;
        self.require_editable(cell)?;
        if !self.grid.candidates(cell).contains(digit) {
            return Err(SessionError::CandidateUnavailable { cell, digit });
        }
        self.push_history();
        self.grid.place(cell, digit);
        self.advance_revision();
        Ok(self.snapshot())
    }

    /// Toggle a pencilmark while preserving all currently placed constraints.
    pub fn toggle_candidate(
        &mut self,
        expected_revision: u64,
        cell: CellId,
        digit: Digit,
    ) -> Result<SessionSnapshot, SessionError> {
        self.require_revision(expected_revision)?;
        self.require_editable(cell)?;
        let current = self.grid.candidates(cell);
        let next = if current.contains(digit) {
            current.without(CandidateMask::of(digit))
        } else {
            if !self.candidate_allowed_by_values(cell, digit) {
                return Err(SessionError::CandidateConflicts { cell, digit });
            }
            current.union(CandidateMask::of(digit))
        };
        self.push_history();
        self.grid.set_candidates(cell, next);
        self.advance_revision();
        Ok(self.snapshot())
    }

    /// Restore the previous exact grid, including all candidate masks.
    pub fn undo(&mut self, expected_revision: u64) -> Result<SessionSnapshot, SessionError> {
        self.require_revision(expected_revision)?;
        let Some(previous) = self.history.pop() else {
            return Err(SessionError::NothingToUndo);
        };
        self.future.push(self.grid.clone());
        self.grid = previous;
        self.clear_pending_hints();
        self.advance_revision();
        Ok(self.snapshot())
    }

    /// Restore the next exact grid that was displaced by [`Session::undo`].
    pub fn redo(&mut self, expected_revision: u64) -> Result<SessionSnapshot, SessionError> {
        self.require_revision(expected_revision)?;
        let Some(next) = self.future.pop() else {
            return Err(SessionError::NothingToRedo);
        };
        self.history.push(self.grid.clone());
        self.grid = next;
        self.clear_pending_hints();
        self.advance_revision();
        Ok(self.snapshot())
    }

    fn require_revision(&self, expected: u64) -> Result<(), SessionError> {
        if expected == self.revision {
            Ok(())
        } else {
            Err(SessionError::StaleRevision {
                expected,
                actual: self.revision,
            })
        }
    }

    fn require_editable(&self, cell: CellId) -> Result<(), SessionError> {
        if self.grid.givens().contains(cell) {
            Err(SessionError::GivenCell { cell })
        } else if self.grid.value(cell) != 0 {
            Err(SessionError::SolvedCell { cell })
        } else {
            Ok(())
        }
    }

    fn candidate_allowed_by_values(&self, source: CellId, digit: Digit) -> bool {
        if self
            .grid
            .topology()
            .visible_peers(source)
            .iter()
            .any(|&raw| self.grid.value(CellId::new(raw).expect("topology peer")) == digit.get())
        {
            return false;
        }

        let Some(neighbors) = self.grid.topology().forbidden_pair_neighbors(source) else {
            return true;
        };
        let mode = self.grid.topology().config().non_consecutive;
        neighbors.iter().all(|&raw| {
            let other = self
                .grid
                .value(CellId::new(raw).expect("topology neighbor"));
            other == 0 || !digits_are_forbidden_neighbors(digit.get(), other, mode)
        })
    }

    fn push_history(&mut self) {
        self.history.push(self.grid.clone());
        self.future.clear();
        self.clear_pending_hints();
    }

    fn retain_collected(&mut self, collected: Vec<CollectedInference>) {
        self.pending_hints.reserve(collected.len());
        for collected in collected {
            let producer = collected.producer();
            let category = collected.category();
            let inference = collected.into_inference();
            if let Some(existing) = self.pending_hints.iter_mut().find(|pending| {
                pending.inference == inference && !self.catalog_hint_ids.contains(&pending.id)
            }) {
                existing.producer = Some(producer);
                existing.category = category;
                self.catalog_hint_ids.push(existing.id);
                continue;
            }
            let id = self.allocate_hint_id();
            self.pending_hints.push(PendingHint {
                id,
                inference,
                producer: Some(producer),
                category,
                selected_chain_proof: None,
            });
            self.catalog_hint_ids.push(id);
        }
    }

    fn all_hints_response(&self) -> AllHintsResponse {
        let hints = self
            .catalog_hint_ids
            .iter()
            .map(|id| {
                self.pending_hints
                    .iter()
                    .find(|pending| pending.id == *id)
                    .expect("catalog ID refers to a retained hint")
            })
            .map(|pending| HintSummary {
                filter_effects: HintEffects::for_legacy_filter(
                    &self.grid,
                    pending.producer.expect("catalog hint retains its producer"),
                    &pending.inference,
                ),
                hint_id: pending.id,
                category: pending.category,
                group_key: pending
                    .producer
                    .expect("catalog hint retains its producer")
                    .hint_group_key(&pending.inference)
                    .to_owned(),
                group_name: pending
                    .producer
                    .expect("catalog hint retains its producer")
                    .hint_group_name(&pending.inference)
                    .to_owned(),
                technique: pending.inference.technique(),
                name: pending.inference.name(),
                short_name: pending.inference.short_name(),
                rating: pending.inference.rating(),
                effects: HintEffects::from_inference(&pending.inference),
            })
            .collect();
        let outcome = match self
            .pending_catalog
            .expect("all-hints response requires a cached catalog outcome")
        {
            PendingCatalog::Complete => AllHintsOutcome::Complete { hints },
            PendingCatalog::ConfirmationRequired => AllHintsOutcome::ConfirmationRequired,
            PendingCatalog::Incomplete(gap) => AllHintsOutcome::Incomplete { hints, gap },
        };
        AllHintsResponse {
            revision: self.revision,
            outcome,
        }
    }

    fn materialize_hint_at(&mut self, index: usize) -> MaterializedHintResponse {
        let id = self.pending_hints[index].id;
        let effects = HintEffects::from_inference(&self.pending_hints[index].inference);

        if self.pending_hints[index].selected_chain_proof.is_none()
            && let Some(producer) = self.pending_hints[index].producer
        {
            match self.solver.replay_selected_proof(
                &self.grid,
                producer,
                &self.pending_hints[index].inference,
            ) {
                Ok(Some(proof)) => {
                    self.pending_hints[index].selected_chain_proof = Some(proof);
                }
                Ok(None) => {}
                Err(gap) => {
                    return MaterializedHintResponse {
                        revision: self.revision,
                        hint_id: id,
                        outcome: MaterializedHintOutcome::Incomplete { gap, effects },
                    };
                }
            }
        }

        let pending = &self.pending_hints[index];
        let presentation = if let Some(proof) = pending.selected_chain_proof.as_ref() {
            present_with_selected_chain_proof(&self.grid, &pending.inference, proof)
        } else {
            present(&self.grid, &pending.inference)
        };
        let outcome = match presentation {
            Ok(presentation) => MaterializedHintOutcome::Presented {
                presentation,
                effects,
            },
            Err(unsupported) => MaterializedHintOutcome::Unsupported {
                unsupported,
                effects,
            },
        };
        MaterializedHintResponse {
            revision: self.revision,
            hint_id: id,
            outcome,
        }
    }

    fn clear_pending_hints(&mut self) {
        self.pending_hints.clear();
        self.catalog_hint_ids.clear();
        self.pending_catalog = None;
    }

    fn advance_revision(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("session revision space exhausted");
    }

    fn allocate_hint_id(&mut self) -> HintId {
        let id = HintId(self.next_hint_id);
        self.next_hint_id = self
            .next_hint_id
            .checked_add(1)
            .expect("session hint ID space exhausted");
        id
    }
}

fn cell(index: usize) -> CellId {
    CellId::new(index as u8).expect("81-cell array index")
}

fn digits_are_forbidden_neighbors(first: u8, second: u8, mode: NonConsecutiveMode) -> bool {
    let distance = first.abs_diff(second);
    distance == 1 || mode.is_cyclic() && distance == 8
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sukaku_forge_core::{ConstraintTopology, Puzzle, VariantConfig};
    use sukaku_forge_engine::Technique;

    use super::{
        AllHintsOutcome, MaterializedHintOutcome, NextHintOutcome, Session, SessionError, Solver,
    };

    fn classic_session(puzzle: &str) -> Session {
        let puzzle = Puzzle::parse(puzzle).unwrap();
        let topology = Arc::new(ConstraintTopology::new(VariantConfig::default()));
        Session::new(
            sukaku_forge_core::Grid::from_puzzle(topology, &puzzle),
            Solver::default(),
        )
    }

    fn selected_fcc_session() -> Session {
        let values = Puzzle::parse(
            "....4.8.....5.8.14..4.......5....4..4.285.....3.49......5.63.4..4.7.5.6.....84...",
        )
        .unwrap();
        let candidates = Puzzle::parse(
            "1.3.567.912...67.91.3...7.9123..6......4......2...67.9.......8..23.5.7.9.23.567.9.23..67.9.2...67....3..67.9....5.....2....7.........8..23..67.91...........4.....123.5..8.1....6789...4.....1.3..6...123...7........7.9.2..56....2..5.7.9.2..567.91....67.9....5....1....6789.23..6.....3...7..12...67.....4......2....789.2...67.9...4.....1....67.9.2..............8.....5....1.....7..1....67....3...7.91.3..67.91....67....3......1......8....4.............912...67..12....7...2..5.78.12..5.7..12....7891......89....5....1.......9.....6.....3......12....7.....4.....12....78912.....89...4.....1.......9......7..12...........5....123.....9.....6...123....89123..67.912...6..9..3..67..12......9.......8....4.....1...5.7.9.2....7.912..5.7.9",
        )
        .unwrap();
        let grid = sukaku_forge_core::Grid::from_snapshot(
            Arc::new(ConstraintTopology::new(VariantConfig {
                anti_knight: true,
                ..VariantConfig::default()
            })),
            &values,
            &candidates,
        )
        .unwrap();
        Session::new(grid, Solver::default())
    }

    #[test]
    fn supported_hidden_single_round_trips_through_presentation_and_server_apply() {
        let mut session = classic_session(
            "12345678.........................................................................",
        );
        let response = session.next_hint();
        assert_eq!(response.revision, 0);
        let NextHintOutcome::Presented {
            hint_id,
            presentation,
            effects,
        } = response.outcome
        else {
            panic!("hidden single must have a supported presentation");
        };
        assert_eq!(presentation.identity.technique, Technique::HiddenSingle);
        assert_eq!(presentation.views.len(), 1);
        assert_eq!(presentation.views[0].candidate_marks.len(), 1);
        assert_eq!(effects.placement.unwrap().0.raw(), 8);
        assert_eq!(effects.placement.unwrap().1.get(), 9);
        assert!(effects.removals.is_empty());

        let applied = session.apply_hint(0, hint_id).unwrap();
        assert_eq!(applied.revision, 1);
        assert_eq!(applied.values[8], 9);
        assert!(applied.can_undo);
        assert!(!applied.can_redo);
    }

    #[test]
    fn stale_revision_and_wrong_hint_are_rejected_without_losing_pending_hint() {
        let mut session = classic_session(
            "12345678.........................................................................",
        );
        let first = session.next_hint();
        let NextHintOutcome::Presented { hint_id, .. } = first.outcome else {
            panic!("expected a presented hint");
        };

        assert_eq!(
            session.apply_hint(7, hint_id),
            Err(SessionError::StaleRevision {
                expected: 7,
                actual: 0,
            })
        );

        let mut other = classic_session(
            "12345678.........................................................................",
        );
        let NextHintOutcome::Presented {
            hint_id: wrong_id, ..
        } = other.next_hint().outcome
        else {
            panic!("expected another presented hint");
        };
        // IDs are session-local, so consume one ID before obtaining a distinct
        // handle in the other session.
        other.apply_hint(0, wrong_id).unwrap();
        other.undo(1).unwrap();
        let NextHintOutcome::Presented {
            hint_id: wrong_id, ..
        } = other.next_hint().outcome
        else {
            panic!("expected another presented hint");
        };
        assert_ne!(wrong_id, hint_id);
        assert_eq!(
            session.apply_hint(0, wrong_id),
            Err(SessionError::UnknownHint { hint_id: wrong_id })
        );

        let applied = session.apply_hint(0, hint_id).unwrap();
        assert_eq!(applied.values[8], 9);
    }

    #[test]
    fn undo_and_redo_restore_exact_values_candidates_and_givens() {
        let mut session = classic_session(
            "12345678.........................................................................",
        );
        let before = session.snapshot();
        let NextHintOutcome::Presented { hint_id, .. } = session.next_hint().outcome else {
            panic!("expected a presented hint");
        };
        let applied = session.apply_hint(0, hint_id).unwrap();

        let undone = session.undo(1).unwrap();
        assert_eq!(undone.revision, 2);
        assert_eq!(undone.values, before.values);
        assert_eq!(undone.candidate_masks, before.candidate_masks);
        assert_eq!(undone.givens, before.givens);
        assert!(undone.can_redo);

        let redone = session.redo(2).unwrap();
        assert_eq!(redone.revision, 3);
        assert_eq!(redone.values, applied.values);
        assert_eq!(redone.candidate_masks, applied.candidate_masks);
        assert_eq!(redone.givens, applied.givens);
        assert!(!redone.can_redo);
    }

    #[test]
    fn checked_edits_clear_pending_hints_and_participate_in_exact_history() {
        let mut session = classic_session(&".".repeat(81));
        let before = session.snapshot();
        let edited = session
            .toggle_candidate(
                0,
                sukaku_forge_core::CellId::new(0).unwrap(),
                sukaku_forge_core::Digit::new(9).unwrap(),
            )
            .unwrap();
        assert_eq!(edited.revision, 1);
        assert_eq!(edited.candidate_masks[0] & (1 << 9), 0);
        let undone = session.undo(1).unwrap();
        assert_eq!(undone.candidate_masks, before.candidate_masks);
        let redone = session.redo(2).unwrap();
        assert_eq!(redone.candidate_masks, edited.candidate_masks);
    }

    #[test]
    fn checked_value_placement_propagates_and_conflicting_candidate_addition_is_rejected() {
        let mut session = classic_session(&".".repeat(81));
        let source = sukaku_forge_core::CellId::new(0).unwrap();
        let peer = sukaku_forge_core::CellId::new(1).unwrap();
        let five = sukaku_forge_core::Digit::new(5).unwrap();

        let placed = session.place_value(0, source, five).unwrap();
        assert_eq!(placed.revision, 1);
        assert_eq!(placed.values[0], 5);
        assert_eq!(placed.candidate_masks[1] & (1 << 5), 0);
        assert_eq!(
            session.toggle_candidate(1, peer, five),
            Err(SessionError::CandidateConflicts {
                cell: peer,
                digit: five,
            })
        );
        assert_eq!(session.revision(), 1, "a rejected edit is not a mutation");
    }

    #[test]
    fn selected_fcc_proof_is_presented_once_and_retained_for_the_revision() {
        let mut session = selected_fcc_session();
        let first = session.next_hint();
        let repeated = session.next_hint();
        assert_eq!(repeated, first, "pending proof and hint ID are reused");

        let NextHintOutcome::Presented {
            presentation,
            effects,
            ..
        } = first.outcome
        else {
            panic!("selected FCC must be presentation-complete");
        };
        assert_eq!(
            presentation.identity.technique,
            Technique::ForcingChainCycle
        );
        assert_eq!(presentation.views.len(), 1);
        assert_eq!(presentation.views[0].key, "forcing");
        assert_eq!(presentation.views[0].label, "Forcing chain");
        assert!(!presentation.views[0].links.is_empty());
        assert!(effects.placement.is_none());
        assert_eq!(effects.removals.elimination_count(), 1);
    }

    #[test]
    fn all_hints_reuses_ordered_ids_and_applies_any_selected_entry() {
        let puzzle = format!("12345678.45678912.{}", ".".repeat(63));
        let mut session = classic_session(&puzzle);
        let NextHintOutcome::Presented {
            hint_id: next_hint_id,
            ..
        } = session.next_hint().outcome
        else {
            panic!("fixture must expose a presented next hint");
        };
        let first = session.all_hints(false);
        let repeated = session.all_hints(false);
        assert_eq!(
            repeated, first,
            "catalog identities are stable per revision"
        );

        let AllHintsOutcome::Complete { hints } = first.outcome else {
            panic!("ordinary logical hints must form a complete catalog");
        };
        assert!(hints.len() >= 2, "fixture exposes multiple direct hints");
        assert_eq!(
            hints[0].hint_id, next_hint_id,
            "a read-only catalog expansion preserves the advertised next-hint handle"
        );
        let selected = hints[1].clone();
        assert!(selected.effects.placement.is_some());

        let materialized = session.hint(0, selected.hint_id).unwrap();
        assert_eq!(materialized.hint_id, selected.hint_id);
        assert!(matches!(
            materialized.outcome,
            MaterializedHintOutcome::Presented { .. }
        ));

        let (cell, digit) = selected.effects.placement.unwrap();
        let applied = session.apply_hint(0, selected.hint_id).unwrap();
        assert_eq!(applied.values[usize::from(cell.raw())], digit.get());
        assert_eq!(applied.revision, 1);
        assert_eq!(
            session.hint(1, selected.hint_id),
            Err(SessionError::UnknownHint {
                hint_id: selected.hint_id,
            }),
            "every catalog handle is invalidated by mutation"
        );
    }
}
