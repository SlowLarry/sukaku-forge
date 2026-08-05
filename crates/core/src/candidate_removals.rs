use crate::{CandidateMask, CellId, Grid};

/// One cell and the candidate digits an inference removes from it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandidateRemoval {
    cell: CellId,
    digits: CandidateMask,
}

impl CandidateRemoval {
    #[must_use]
    pub const fn cell(self) -> CellId {
        self.cell
    }

    #[must_use]
    pub const fn digits(self) -> CandidateMask {
        self.digits
    }
}

/// Immutable, sparse application payload for candidate-elimination hints.
///
/// Entries retain the first-touched cell order used by the producer. Repeated
/// additions for one cell are merged without moving that entry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CandidateRemovals {
    entries: Box<[CandidateRemoval]>,
    elimination_count: u16,
}

impl CandidateRemovals {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub const fn elimination_count(&self) -> u16 {
        self.elimination_count
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = CandidateRemoval> + '_ {
        self.entries.iter().copied()
    }

    pub fn apply(&self, grid: &mut Grid) {
        for entry in &self.entries {
            grid.remove_candidates(entry.cell, entry.digits);
        }
    }
}

/// Mutable inference-time builder for [`CandidateRemovals`].
#[derive(Clone, Debug, Default)]
pub struct CandidateRemovalsBuilder {
    entries: Vec<CandidateRemoval>,
    elimination_count: u16,
}

impl CandidateRemovalsBuilder {
    #[must_use]
    pub fn with_capacity(expected_cells: usize) -> Self {
        Self {
            entries: Vec::with_capacity(expected_cells),
            elimination_count: 0,
        }
    }

    pub fn add(&mut self, cell: CellId, digits: CandidateMask) {
        if digits.is_empty() {
            return;
        }
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.cell == cell) {
            let added = digits.without(entry.digits);
            entry.digits = entry.digits.union(digits);
            self.elimination_count += added.count() as u16;
            return;
        }
        self.entries.push(CandidateRemoval { cell, digits });
        self.elimination_count += digits.count() as u16;
    }

    #[must_use]
    pub fn build(self) -> CandidateRemovals {
        CandidateRemovals {
            entries: self.entries.into_boxed_slice(),
            elimination_count: self.elimination_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CandidateRemovalsBuilder;
    use crate::{CandidateMask, CellId, Digit};

    #[test]
    fn builder_merges_without_changing_first_touch_order() {
        let first = CellId::new(12).unwrap();
        let second = CellId::new(3).unwrap();
        let one = CandidateMask::of(Digit::new(1).unwrap());
        let two = CandidateMask::of(Digit::new(2).unwrap());
        let mut builder = CandidateRemovalsBuilder::with_capacity(2);
        builder.add(first, one);
        builder.add(second, two);
        builder.add(first, one.union(two));

        let removals = builder.build();
        let entries = removals.iter().collect::<Vec<_>>();
        assert_eq!(entries[0].cell(), first);
        assert_eq!(entries[0].digits(), one.union(two));
        assert_eq!(entries[1].cell(), second);
        assert_eq!(removals.elimination_count(), 3);
    }
}
