use crate::CellId;

/// An 81-cell set represented by the same two-word split as the Java engine.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct CellMask {
    low: u64,
    high: u64,
}

impl CellMask {
    pub const EMPTY: Self = Self { low: 0, high: 0 };

    #[must_use]
    pub const fn from_words(low: u64, high: u64) -> Self {
        Self {
            low,
            high: high & 0x1ffff,
        }
    }

    #[must_use]
    pub const fn low(self) -> u64 {
        self.low
    }

    #[must_use]
    pub const fn high(self) -> u64 {
        self.high
    }

    #[must_use]
    pub const fn contains(self, cell: CellId) -> bool {
        if cell.raw() < 64 {
            self.low & (1_u64 << cell.raw()) != 0
        } else {
            self.high & (1_u64 << (cell.raw() - 64)) != 0
        }
    }

    pub const fn insert(&mut self, cell: CellId) {
        if cell.raw() < 64 {
            self.low |= 1_u64 << cell.raw();
        } else {
            self.high |= 1_u64 << (cell.raw() - 64);
        }
    }

    pub const fn remove(&mut self, cell: CellId) {
        if cell.raw() < 64 {
            self.low &= !(1_u64 << cell.raw());
        } else {
            self.high &= !(1_u64 << (cell.raw() - 64));
        }
    }

    #[must_use]
    pub const fn intersect(self, other: Self) -> Self {
        Self {
            low: self.low & other.low,
            high: self.high & other.high,
        }
    }

    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self {
            low: self.low & !other.low,
            high: self.high & !other.high,
        }
    }

    #[must_use]
    pub const fn count(self) -> u32 {
        self.low.count_ones() + self.high.count_ones()
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.low == 0 && self.high == 0
    }

    #[must_use]
    pub const fn iter(self) -> CellIter {
        CellIter {
            low: self.low,
            high: self.high,
        }
    }
}

/// Ascending iterator over cell indexes.
#[derive(Clone, Debug)]
pub struct CellIter {
    low: u64,
    high: u64,
}

impl Iterator for CellIter {
    type Item = CellId;

    fn next(&mut self) -> Option<Self::Item> {
        if self.low != 0 {
            let index = self.low.trailing_zeros() as u8;
            self.low &= self.low - 1;
            return CellId::new(index);
        }
        if self.high != 0 {
            let index = self.high.trailing_zeros() as u8;
            self.high &= self.high - 1;
            return CellId::new(index + 64);
        }
        None
    }
}
