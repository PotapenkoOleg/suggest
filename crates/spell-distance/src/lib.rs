//! How far apart two words are, counted in single-element edits.
//!
//! ```
//! use spell_distance::{distance, distance_of};
//!
//! // A transposition costs the same as any other edit
//! assert_eq!(distance("teh", "the"), 1);
//! assert_eq!(distance("sea", "shore"), 4);
//!
//! // Any sequence of comparable elements, not only text
//! assert_eq!(distance_of(&[1, 2, 3], &[2, 1, 3]), 1);
//! ```

pub mod damerau_levenshtein;

pub use damerau_levenshtein::{distance, distance_of};
