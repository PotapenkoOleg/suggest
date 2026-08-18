//! Which stored words are close to this one, without asking each of them.
//!
//! ```
//! use bk_tree::BkTree;
//!
//! let tree: BkTree = ["sea", "shells", "shore", "she", "sells"]
//!     .into_iter()
//!     .collect();
//!
//! let matches = tree.find("shell", 1);
//! let words: Vec<&str> = matches.iter().map(|found| found.word.as_str()).collect();
//!
//! assert_eq!(words, ["shells"]);
//! assert_eq!(matches[0].distance, 1);
//! ```

pub mod burkhard_keller;

pub use burkhard_keller::{BkTree, Match};
