//! A Burkhard-Keller tree: a set of words that answers "which of you are
//! within `radius` edits of this query" without measuring every word.
//!
//! Each node keeps its children in a map keyed by their distance from it. A
//! search measures the query against a node once and then descends only into
//! the children whose key is within `radius` of that measurement, because the
//! triangle inequality puts everything else too far away to match. Most of the
//! set is never touched:
//!
//! ```
//! use bk_tree::BkTree;
//!
//! let mut tree = BkTree::new();
//! for word in ["sea", "shore", "shells", "she", "sells", "by", "the"] {
//!     tree.insert(word);
//! }
//!
//! // A transposition is one edit, so the misspelling is found at radius 1
//! let matches = tree.find("hse", 1);
//! assert_eq!(matches.len(), 1);
//! assert_eq!(matches[0].word, "she");
//!
//! assert!(tree.contains("shore"));
//! assert!(!tree.contains("shored"));
//! ```
//!
//! Distances come from [`spell_distance`], the restricted
//! Damerau-Levenshtein, which is not a metric: the triangle inequality the
//! pruning rests on can fail, and where it does a search leaves out a word a
//! scan of the whole set would have returned. `abc` is three edits from `ca`
//! but one from `ac`, which is itself one from `ca`, so filed three deep under
//! `ca` it sits out of reach of a search that descends one.
//!
//! Nothing here compensates for that, so recall is what the data makes it
//! rather than something the structure guarantees. Measured over the word
//! files this repository ships - 471 words, searched by every word, by
//! misspellings of each, and by random strings, 7,652 searches in all - one
//! came back short: `PayingSwapQnyt` at radius 1 missed `PayingSwapQnty`,
//! the only one of 10,529 matches lost. Where a miss cannot be tolerated,
//! scan.

use spell_distance::distance_of;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

mod tests;

/// A word [`BkTree::find`] turned up, and how far it is from the query.
///
/// Ordered nearest first and alphabetically within a distance, which is the
/// order a caller wants to offer them in.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Match {
    pub distance: usize,
    pub word: String,
}

struct Node {
    // Held as characters, the form the distance is measured in, so a word is
    // converted once when it is inserted rather than once per comparison
    chars: Vec<char>,
    // Keyed by the distance from this node to the child. Ordered rather than
    // hashed, so a search takes the span it needs instead of filtering every
    // child; two words the same distance away share a subtree
    children: BTreeMap<usize, Node>,
}

impl Node {
    fn new(chars: Vec<char>) -> Self {
        Node {
            chars,
            children: BTreeMap::new(),
        }
    }

    fn word(&self) -> String {
        self.chars.iter().collect()
    }

    /// Files `chars` under the child whose distance it shares, recursing until
    /// it reaches a distance no child holds yet. Returns `false` if the word is
    /// already in this subtree.
    fn insert(&mut self, chars: Vec<char>) -> bool {
        let distance = distance_of(&self.chars, &chars);

        // Nothing to file: a word is no edits from itself, and the tree is a
        // set
        if distance == 0 {
            return false;
        }

        match self.children.entry(distance) {
            Entry::Occupied(mut child) => child.get_mut().insert(chars),
            Entry::Vacant(slot) => {
                slot.insert(Node::new(chars));
                true
            }
        }
    }
}

/// A set of words searchable by edit distance.
///
/// # Examples
///
/// ```
/// use bk_tree::BkTree;
///
/// let mut tree = BkTree::new();
///
/// assert!(tree.insert("shore"));
/// // The same word twice is one word
/// assert!(!tree.insert("shore"));
/// assert_eq!(tree.len(), 1);
/// ```
#[derive(Default)]
pub struct BkTree {
    root: Option<Node>,
    size: usize,
}

impl BkTree {
    /// Creates an empty tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `word`, returning `false` if the tree already held it.
    ///
    /// The first word inserted becomes the root every later search starts
    /// from, so a word from the middle of the set shapes a shallower tree than
    /// an outlier does.
    pub fn insert(&mut self, word: &str) -> bool {
        let chars: Vec<char> = word.chars().collect();

        let inserted = match self.root.as_mut() {
            Some(root) => root.insert(chars),
            None => {
                self.root = Some(Node::new(chars));
                true
            }
        };

        if inserted {
            self.size += 1;
        }

        inserted
    }

    /// Returns the words within `radius` edits of `query`, nearest first.
    ///
    /// A radius of zero asks whether the word itself is stored, and costs one
    /// comparison per level rather than a walk of the set.
    ///
    /// # Examples
    ///
    /// ```
    /// use bk_tree::BkTree;
    ///
    /// let tree: BkTree = ["sea", "tea", "sell", "shore"].into_iter().collect();
    /// let words: Vec<String> = tree
    ///     .find("sea", 2)
    ///     .into_iter()
    ///     .map(|found| found.word)
    ///     .collect();
    ///
    /// // `sell` is two edits away: `a` becomes `l`, and a second `l` follows
    /// assert_eq!(words, ["sea", "tea", "sell"]);
    /// ```
    pub fn find(&self, query: &str, radius: usize) -> Vec<Match> {
        let query: Vec<char> = query.chars().collect();

        let mut matches = Vec::new();
        // An explicit stack rather than recursion: the depth of a tree is the
        // data's business, not something a search should stake the call stack
        // on
        let mut pending: Vec<&Node> = self.root.iter().collect();

        while let Some(node) = pending.pop() {
            let distance = distance_of(&node.chars, &query);

            if distance <= radius {
                matches.push(Match {
                    distance,
                    word: node.word(),
                });
            }

            // A child sits at a known distance from this node, so nothing
            // nearer to the query than `distance - radius` or further than
            // `distance + radius` can be within reach of it
            let nearest = distance.saturating_sub(radius);
            let furthest = distance.saturating_add(radius);

            pending.extend(node.children.range(nearest..=furthest).map(|(_, child)| child));
        }

        // The stack hands nodes back in whatever order the descent stacked
        // them; `Match` orders itself by distance and then alphabetically
        matches.sort();
        matches
    }

    /// Returns `true` if `word` is stored, exactly as spelled.
    pub fn contains(&self, word: &str) -> bool {
        !self.find(word, 0).is_empty()
    }

    /// Returns how many words are stored.
    pub fn len(&self) -> usize {
        self.size
    }

    /// Returns `true` if no word is stored.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
}

impl<S: AsRef<str>> FromIterator<S> for BkTree {
    fn from_iter<I: IntoIterator<Item = S>>(words: I) -> Self {
        let mut tree = BkTree::new();

        for word in words {
            tree.insert(word.as_ref());
        }

        tree
    }
}
