//! Opt-in, ordered proof data for presenting one selected chaining inference.
//!
//! The normal solver and rating path deliberately keep only compact
//! [`Inference`](crate::Inference) evidence. These types are populated only by
//! the presentation-oriented search entry points after the winning static
//! forcing-chain candidate has been ranked.

use sukaku_forge_core::{CellId, Digit, RegionId};

use crate::Inference;

/// Whether one candidate is asserted to be present or absent at a proof node.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ChainState {
    Off,
    On,
}

impl ChainState {
    #[must_use]
    pub const fn is_on(self) -> bool {
        matches!(self, Self::On)
    }
}

/// Semantic reason for one implication edge in a selected proof.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ChainCause {
    #[default]
    None,
    /// The implication follows from the candidates in one cell.
    Cell,
    /// The implication follows from one concrete topology region.
    Region(RegionId),
    /// The implication follows from a non-region visibility constraint.
    Visibility,
}

/// A node identity scoped to one [`ChainProofView`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChainNodeId(u32);

impl ChainNodeId {
    pub(crate) fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect("selected chain proof node index"))
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// One ordered incoming edge from a parent potential.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChainProofParent {
    node: ChainNodeId,
    cause: ChainCause,
}

impl ChainProofParent {
    pub(crate) const fn new(node: ChainNodeId, cause: ChainCause) -> Self {
        Self { node, cause }
    }

    #[must_use]
    pub const fn node(self) -> ChainNodeId {
        self.node
    }

    #[must_use]
    pub const fn cause(self) -> ChainCause {
        self.cause
    }
}

/// One potential in a selected, ordered implication graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChainProofNode {
    cell: CellId,
    digit: Digit,
    state: ChainState,
    /// Parent order is compatibility data. Static FCC nodes have at most one.
    parents: Box<[ChainProofParent]>,
}

impl ChainProofNode {
    pub(crate) fn new(
        cell: CellId,
        digit: Digit,
        state: ChainState,
        parents: Box<[ChainProofParent]>,
    ) -> Self {
        Self {
            cell,
            digit,
            state,
            parents,
        }
    }

    #[must_use]
    pub const fn cell(&self) -> CellId {
        self.cell
    }

    #[must_use]
    pub const fn digit(&self) -> Digit {
        self.digit
    }

    #[must_use]
    pub const fn state(&self) -> ChainState {
        self.state
    }

    #[must_use]
    pub fn parents(&self) -> &[ChainProofParent] {
        &self.parents
    }
}

/// The legacy flat view represented by one selected proof graph.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ChainProofViewKind {
    Forcing,
    CycleForward,
    CycleReverse,
}

/// One target-first, breadth-first legacy chain view.
///
/// Parents always refer to nodes later in `nodes` for the current static,
/// single-parent chains. Keeping explicit IDs makes the contract usable by
/// future multi-parent proof materializers without changing traversal order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChainProofView {
    kind: ChainProofViewKind,
    target: ChainNodeId,
    nodes: Box<[ChainProofNode]>,
}

impl ChainProofView {
    pub(crate) fn new(kind: ChainProofViewKind, nodes: Vec<ChainProofNode>) -> Self {
        debug_assert!(!nodes.is_empty());
        Self {
            kind,
            target: ChainNodeId::from_index(0),
            nodes: nodes.into_boxed_slice(),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> ChainProofViewKind {
        self.kind
    }

    #[must_use]
    pub const fn target(&self) -> ChainNodeId {
        self.target
    }

    #[must_use]
    pub fn nodes(&self) -> &[ChainProofNode] {
        &self.nodes
    }

    #[must_use]
    pub fn node(&self, id: ChainNodeId) -> Option<&ChainProofNode> {
        self.nodes.get(id.index())
    }
}

/// Full flat presentation proof for one selected static FCC winner.
///
/// Forcing chains contain one `Forcing` view. Bidirectional cycles contain a
/// `CycleForward` view followed by Java's complemented `CycleReverse` view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedChainProof {
    views: Box<[ChainProofView]>,
}

impl SelectedChainProof {
    pub(crate) fn new(views: Vec<ChainProofView>) -> Self {
        debug_assert!(matches!(views.len(), 1 | 2));
        Self {
            views: views.into_boxed_slice(),
        }
    }

    #[must_use]
    pub fn views(&self) -> &[ChainProofView] {
        &self.views
    }
}

/// One static FCC inference paired with its opt-in selected proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForcingChainWithProof {
    inference: Inference,
    proof: SelectedChainProof,
}

impl ForcingChainWithProof {
    pub(crate) const fn new(inference: Inference, proof: SelectedChainProof) -> Self {
        Self { inference, proof }
    }

    #[must_use]
    pub const fn inference(&self) -> &Inference {
        &self.inference
    }

    #[must_use]
    pub const fn proof(&self) -> &SelectedChainProof {
        &self.proof
    }

    #[must_use]
    pub fn into_parts(self) -> (Inference, SelectedChainProof) {
        (self.inference, self.proof)
    }
}
