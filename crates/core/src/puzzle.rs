use core::fmt;

use crate::{CandidateMask, CellId, CellMask, Digit};

/// Either an 81-character value grid or a 729-character pencilmark grid.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Puzzle {
    Values {
        values: [u8; CellId::COUNT],
        givens: CellMask,
    },
    Pencilmarks([CandidateMask; CellId::COUNT]),
}

impl Puzzle {
    /// Parse the two input formats accepted at the headless compatibility seam.
    pub fn parse(input: &str) -> Result<Self, ParsePuzzleError> {
        let input = input.trim();
        match input.len() {
            81 => parse_values(input),
            729 => parse_pencilmarks(input),
            actual => Err(ParsePuzzleError::InvalidLength { actual }),
        }
    }

    #[must_use]
    pub const fn values(&self) -> Option<&[u8; CellId::COUNT]> {
        match self {
            Self::Values { values, .. } => Some(values),
            Self::Pencilmarks(_) => None,
        }
    }

    #[must_use]
    pub const fn givens(&self) -> CellMask {
        match self {
            Self::Values { givens, .. } => *givens,
            Self::Pencilmarks(_) => CellMask::EMPTY,
        }
    }

    #[must_use]
    pub const fn pencilmarks(&self) -> Option<&[CandidateMask; CellId::COUNT]> {
        match self {
            Self::Values { .. } => None,
            Self::Pencilmarks(candidates) => Some(candidates),
        }
    }

    #[must_use]
    pub fn to_input_string(&self) -> String {
        match self {
            Self::Values { values, .. } => values
                .iter()
                .map(|value| {
                    if *value == 0 {
                        '.'
                    } else {
                        char::from(b'0' + value)
                    }
                })
                .collect(),
            Self::Pencilmarks(candidates) => candidate_string(candidates),
        }
    }
}

fn parse_values(input: &str) -> Result<Puzzle, ParsePuzzleError> {
    let mut values = [0_u8; CellId::COUNT];
    let mut givens = CellMask::EMPTY;
    for (index, byte) in input.bytes().enumerate() {
        match byte {
            b'1'..=b'9' => {
                values[index] = byte - b'0';
                givens.insert(CellId::new(index as u8).expect("81-character input"));
            }
            b'0' | b'.' => {}
            _ => {
                return Err(ParsePuzzleError::InvalidCharacter {
                    index,
                    character: char::from(byte),
                });
            }
        }
    }
    Ok(Puzzle::Values { values, givens })
}

fn parse_pencilmarks(input: &str) -> Result<Puzzle, ParsePuzzleError> {
    let mut candidates = [CandidateMask::EMPTY; CellId::COUNT];
    for (index, byte) in input.bytes().enumerate() {
        if !byte.is_ascii_digit() || byte == b'0' {
            continue;
        }
        let actual = byte - b'0';
        let expected = (index % 9 + 1) as u8;
        if actual != expected {
            return Err(ParsePuzzleError::MisplacedPencilmark {
                index,
                expected,
                actual,
            });
        }
        candidates[index / 9].insert(Digit::new(actual).expect("ASCII digit 1 through 9"));
    }
    Ok(Puzzle::Pencilmarks(candidates))
}

pub(crate) fn candidate_string(candidates: &[CandidateMask; CellId::COUNT]) -> String {
    let mut result = String::with_capacity(729);
    for &mask in candidates {
        for value in 1_u8..=9 {
            let digit = Digit::new(value).expect("digit loop");
            result.push(if mask.contains(digit) {
                char::from(b'0' + value)
            } else {
                '.'
            });
        }
    }
    result
}

/// Invalid value-grid or pencilmark input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParsePuzzleError {
    InvalidLength {
        actual: usize,
    },
    InvalidCharacter {
        index: usize,
        character: char,
    },
    MisplacedPencilmark {
        index: usize,
        expected: u8,
        actual: u8,
    },
}

impl fmt::Display for ParsePuzzleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { actual } => {
                write!(
                    formatter,
                    "puzzle must contain 81 values or 729 pencilmarks, got {actual}"
                )
            }
            Self::InvalidCharacter { index, character } => {
                write!(
                    formatter,
                    "invalid value-grid character {character:?} at offset {index}"
                )
            }
            Self::MisplacedPencilmark {
                index,
                expected,
                actual,
            } => write!(
                formatter,
                "pencilmark {actual} at offset {index} occupies digit slot {expected}"
            ),
        }
    }
}

impl std::error::Error for ParsePuzzleError {}

#[cfg(test)]
mod tests {
    use super::Puzzle;

    #[test]
    fn value_grid_round_trips() {
        let text =
            "98.7..6....5.4...........9.8..9...6..4..5...9..9..32..1.........7.1...8...8..2..3";
        let puzzle = Puzzle::parse(text).unwrap();
        assert_eq!(puzzle.to_input_string(), text);
        assert_eq!(puzzle.givens().count(), 23);
    }

    #[test]
    fn pencilmarks_round_trip() {
        let text = "123456789".repeat(81);
        let puzzle = Puzzle::parse(&text).unwrap();
        assert_eq!(puzzle.to_input_string(), text);
    }
}
