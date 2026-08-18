//! Which stored words are spelled like this one, found by the runs of
//! characters they share.
//!
//! ```
//! use n_gram_index::{NGramIndex, grams};
//!
//! let mut index = NGramIndex::new(2);
//! for word in ["sea", "shells", "shore", "she"] {
//!     index.insert(word);
//! }
//!
//! // `#` marks where a word begins and ends, so `sh` starting a word and `sh`
//! // inside one are told apart
//! assert_eq!(grams("she", 2), ["#s", "sh", "he", "e#"]);
//!
//! let matches = index.find("shell", 1);
//! assert_eq!(matches[0].word, "shells");
//! assert_eq!(matches[0].shared, 5);
//! ```

pub mod index;

pub use index::{BOUNDARY, Match, NGramIndex, grams};
