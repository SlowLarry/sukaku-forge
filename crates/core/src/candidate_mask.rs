use crate::Digit;

/// Candidate digits stored in Java-compatible bits 1 through 9.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct CandidateMask(u16);

impl CandidateMask {
    pub const EMPTY: Self = Self(0);
    pub const ALL: Self = Self(0x03fe);

    #[must_use]
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits & Self::ALL.0)
    }

    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    #[must_use]
    pub const fn of(digit: Digit) -> Self {
        Self(1_u16 << digit.get())
    }

    #[must_use]
    pub const fn contains(self, digit: Digit) -> bool {
        self.0 & Self::of(digit).0 != 0
    }

    pub const fn insert(&mut self, digit: Digit) {
        self.0 |= Self::of(digit).0;
    }

    pub const fn remove(&mut self, digit: Digit) {
        self.0 &= !Self::of(digit).0;
    }

    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0 & Self::ALL.0)
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self((self.0 | other.0) & Self::ALL.0)
    }

    #[must_use]
    pub const fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    #[must_use]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn single(self) -> Option<Digit> {
        if self.count() == 1 {
            Digit::new(self.0.trailing_zeros() as u8)
        } else {
            None
        }
    }

    #[must_use]
    pub const fn iter(self) -> DigitIter {
        DigitIter(self.0)
    }
}

/// Ascending iterator over candidate digits.
#[derive(Clone, Debug)]
pub struct DigitIter(u16);

impl Iterator for DigitIter {
    type Item = Digit;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0 == 0 {
            return None;
        }
        let value = self.0.trailing_zeros() as u8;
        self.0 &= self.0 - 1;
        Digit::new(value)
    }
}

/// Positions inside a nine-cell region, stored in bits 0 through 8.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PositionMask(u16);

impl PositionMask {
    pub const EMPTY: Self = Self(0);
    pub const ALL: Self = Self(0x01ff);

    #[must_use]
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits & Self::ALL.0)
    }

    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    #[must_use]
    pub const fn contains(self, position: u8) -> bool {
        position < 9 && self.0 & (1_u16 << position) != 0
    }

    pub const fn insert(&mut self, position: u8) {
        if position < 9 {
            self.0 |= 1_u16 << position;
        }
    }

    pub const fn remove(&mut self, position: u8) {
        if position < 9 {
            self.0 &= !(1_u16 << position);
        }
    }

    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0 & Self::ALL.0)
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self((self.0 | other.0) & Self::ALL.0)
    }

    #[must_use]
    pub const fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    #[must_use]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn single(self) -> Option<u8> {
        if self.count() == 1 {
            Some(self.0.trailing_zeros() as u8)
        } else {
            None
        }
    }

    #[must_use]
    pub const fn iter(self) -> PositionIter {
        PositionIter(self.0)
    }
}

/// Ascending iterator over positions in a region.
#[derive(Clone, Debug)]
pub struct PositionIter(u16);

impl Iterator for PositionIter {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0 == 0 {
            return None;
        }
        let position = self.0.trailing_zeros() as u8;
        self.0 &= self.0 - 1;
        Some(position)
    }
}

#[cfg(test)]
mod tests {
    use super::{CandidateMask, PositionMask};
    use crate::Digit;

    #[test]
    fn candidate_subsets_round_trip_exhaustively() {
        for subset in 0_u16..512 {
            let java_bits = subset << 1;
            let mask = CandidateMask::from_bits(java_bits);
            assert_eq!(mask.bits(), java_bits);
            assert_eq!(mask.count(), subset.count_ones());
            let digits: Vec<u8> = mask.iter().map(Digit::get).collect();
            let expected: Vec<u8> = (1_u8..=9)
                .filter(|digit| java_bits & (1_u16 << digit) != 0)
                .collect();
            assert_eq!(digits, expected);
        }
    }

    #[test]
    fn position_subsets_round_trip_exhaustively() {
        for subset in 0_u16..512 {
            let mask = PositionMask::from_bits(subset);
            assert_eq!(mask.bits(), subset);
            assert_eq!(mask.count(), subset.count_ones());
            assert_eq!(
                mask.iter().collect::<Vec<_>>(),
                (0_u8..9)
                    .filter(|position| subset & (1_u16 << position) != 0)
                    .collect::<Vec<_>>()
            );
        }
    }
}
