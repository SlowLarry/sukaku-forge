/// Non-consecutive neighbor and digit-adjacency mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum NonConsecutiveMode {
    #[default]
    Off = 0,
    Orthogonal = 1,
    OrthogonalCyclic = 2,
    Diagonal = 3,
    DiagonalCyclic = 4,
}

impl NonConsecutiveMode {
    #[must_use]
    pub const fn is_cyclic(self) -> bool {
        matches!(self, Self::OrthogonalCyclic | Self::DiagonalCyclic)
    }

    #[must_use]
    pub const fn is_orthogonal(self) -> bool {
        matches!(self, Self::Orthogonal | Self::OrthogonalCyclic)
    }
}

/// Immutable flags used to construct one constraint topology.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VariantConfig {
    pub blocks: bool,
    pub disjoint_groups: bool,
    pub windows: bool,
    pub sudoku_x: bool,
    pub girandola: bool,
    pub asterisk: bool,
    pub center_dot: bool,
    pub anti_ferz: bool,
    pub anti_knight: bool,
    pub toroidal: bool,
    pub non_consecutive: NonConsecutiveMode,
    pub forbidden_pairs: bool,
}

impl Default for VariantConfig {
    fn default() -> Self {
        Self {
            blocks: true,
            disjoint_groups: false,
            windows: false,
            sudoku_x: false,
            girandola: false,
            asterisk: false,
            center_dot: false,
            anti_ferz: false,
            anti_knight: false,
            toroidal: false,
            non_consecutive: NonConsecutiveMode::Off,
            forbidden_pairs: false,
        }
    }
}
