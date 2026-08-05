use core::fmt;

use crate::{Inference, Technique};

/// Difficulty stored in exact tenths, avoiding floating-point ordering.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Rating(u16);

impl Rating {
    #[must_use]
    pub const fn from_tenths(tenths: u16) -> Self {
        Self(tenths)
    }

    #[must_use]
    pub const fn tenths(self) -> u16 {
        self.0
    }
}

impl fmt::Display for Rating {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{:01}", self.0 / 10, self.0 % 10)
    }
}

/// One rating field and the first technique that established it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RatedTechnique {
    rating: Rating,
    technique: Option<Technique>,
    name: Option<Box<str>>,
    short_name: Option<Box<str>>,
}

impl RatedTechnique {
    #[must_use]
    pub const fn rating(&self) -> Rating {
        self.rating
    }

    #[must_use]
    pub const fn technique(&self) -> Option<Technique> {
        self.technique
    }

    #[must_use]
    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("No solution")
    }

    #[must_use]
    pub fn short_name(&self) -> &str {
        self.short_name.as_deref().unwrap_or("O")
    }
}

/// Exact standard-mode ER/EP/ED snapshot.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RatingResult {
    er: RatedTechnique,
    ep: RatedTechnique,
    ed: RatedTechnique,
}

impl RatingResult {
    #[must_use]
    pub const fn er(&self) -> &RatedTechnique {
        &self.er
    }

    #[must_use]
    pub const fn ep(&self) -> &RatedTechnique {
        &self.ep
    }

    #[must_use]
    pub const fn ed(&self) -> &RatedTechnique {
        &self.ed
    }
}

/// Incremental Java-compatible rating bookkeeping used by solvers and observers.
#[derive(Clone, Debug, Default)]
pub struct RatingTracker {
    result: RatingResult,
}

impl RatingTracker {
    pub fn observe(&mut self, inference: &Inference) {
        if inference.rating() > self.result.er.rating {
            self.result.er = RatedTechnique {
                rating: inference.rating(),
                technique: Some(inference.technique()),
                name: Some(inference.name().into_boxed_str()),
                short_name: Some(inference.short_name().into_boxed_str()),
            };
        }
        if self.result.ep.rating == Rating::default() {
            if self.result.ed.rating == Rating::default() {
                self.result.ed = self.result.er.clone();
            }
            if inference.is_placement() {
                self.result.ep = self.result.er.clone();
            }
        }
    }

    #[must_use]
    pub fn result(self) -> RatingResult {
        self.result
    }
}

#[cfg(test)]
mod tests {
    use sukaku_forge_core::{CellId, Digit, RegionId};

    use super::{Rating, RatingTracker};
    use crate::{Evidence, Inference, Technique};

    #[test]
    fn equal_maximum_retains_the_first_technique() {
        let first = Inference::placement(
            Technique::HiddenSingle,
            Rating::from_tenths(15),
            CellId::new(0).unwrap(),
            Digit::new(1).unwrap(),
            Evidence::HiddenSingle {
                region: RegionId::new(1, 0).unwrap(),
                alone: false,
            },
        );
        let second = Inference::placement(
            Technique::NakedSingle,
            Rating::from_tenths(15),
            CellId::new(1).unwrap(),
            Digit::new(2).unwrap(),
            Evidence::NakedSingle,
        );
        let mut tracker = RatingTracker::default();
        tracker.observe(&first);
        tracker.observe(&second);
        let result = tracker.result();
        assert_eq!(result.er().technique(), Some(Technique::HiddenSingle));
        assert_eq!(result.ep(), result.ed());
    }
}
