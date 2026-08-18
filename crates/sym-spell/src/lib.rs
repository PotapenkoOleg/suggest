//! Which stored word was this misspelling meant to be.
//!
//! ```
//! use sym_spell::SymSpell;
//!
//! let mut speller = SymSpell::new(2);
//! for (word, count) in [("sea", 40), ("she", 90), ("shore", 12), ("sells", 7)] {
//!     speller.insert(word, count);
//! }
//!
//! let suggestions = speller.lookup("seh", 1);
//!
//! // `she` is a transposition away and `sea` a substitution, so both are one
//! // edit out; the commoner word is offered first
//! assert_eq!(suggestions[0].word, "she");
//! assert_eq!(suggestions[1].word, "sea");
//! assert!(suggestions.iter().all(|found| found.distance == 1));
//! ```

pub mod symmetric_delete;

pub use symmetric_delete::{Suggestion, SymSpell, deletes};
