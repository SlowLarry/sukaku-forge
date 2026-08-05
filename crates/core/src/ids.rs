use core::fmt;

/// A cell index in row-major order, from 0 through 80.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CellId(u8);

impl CellId {
    pub const COUNT: usize = 81;

    #[must_use]
    pub const fn new(index: u8) -> Option<Self> {
        if index < 81 { Some(Self(index)) } else { None }
    }

    #[must_use]
    pub const fn from_row_column(row: u8, column: u8) -> Option<Self> {
        if row < 9 && column < 9 {
            Some(Self(row * 9 + column))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn row(self) -> u8 {
        self.0 / 9
    }

    #[must_use]
    pub const fn column(self) -> u8 {
        self.0 % 9
    }
}

impl fmt::Display for CellId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "r{}c{}", self.row() + 1, self.column() + 1)
    }
}

/// A Sudoku digit from 1 through 9.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Digit(u8);

impl Digit {
    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if value >= 1 && value <= 9 {
            Some(Self(value))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl fmt::Display for Digit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Stable region identity: family index followed by the region index.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RegionId {
    type_index: u8,
    region_index: u8,
}

impl RegionId {
    #[must_use]
    pub const fn new(type_index: u8, region_index: u8) -> Option<Self> {
        if type_index < 10 && region_index < 9 {
            Some(Self {
                type_index,
                region_index,
            })
        } else {
            None
        }
    }

    #[must_use]
    pub const fn type_index(self) -> usize {
        self.type_index as usize
    }

    #[must_use]
    pub const fn region_index(self) -> usize {
        self.region_index as usize
    }
}
