//! An n-gram index: every word filed under each run of `n` characters it
//! contains, so a query can ask which words share its runs instead of being
//! compared to all of them.
//!
//! A word is padded with `n - 1` boundary markers on each side before it is
//! cut up, so its first and last characters carry as many grams as the ones in
//! the middle, and a run that starts a word is a different gram from the same
//! run inside one:
//!
//! ```
//! use n_gram_index::grams;
//!
//! assert_eq!(grams("sea", 2), ["#s", "se", "ea", "a#"]);
//! assert_eq!(grams("sea", 3), ["##s", "#se", "sea", "ea#", "a##"]);
//! ```
//!
//! The marker is an ordinary character, so a word containing `#` shares grams
//! with a boundary. The padding is what makes it worth that: without it,
//! `shore` and `hor` would look alike from the ends inward.
//!
//! What comes back is a count of shared grams, which grows with the length of
//! the words as much as with their likeness. Divide it by what the two could
//! have shared to compare across lengths:
//!
//! ```
//! use n_gram_index::{NGramIndex, grams};
//! use std::collections::BTreeSet;
//!
//! let mut index = NGramIndex::new(2);
//! index.insert("shells");
//!
//! let found = &index.find("shell", 1)[0];
//! let query: BTreeSet<String> = grams("shell", 2).into_iter().collect();
//! let word: BTreeSet<String> = grams(&found.word, 2).into_iter().collect();
//!
//! // Jaccard: shared over the size of both sets together
//! let union = query.len() + word.len() - found.shared;
//! assert_eq!(found.shared, 5);
//! assert_eq!(union, 8);
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};

mod tests;

/// The character standing in for what comes before a word and after it.
pub const BOUNDARY: char = '#';

/// Cuts `word` into every run of `size` characters, padded with [`BOUNDARY`]
/// so the ends of the word are as well covered as its middle.
///
/// Runs are taken over characters rather than bytes, and in the order they
/// appear, repeats included - the index is what makes a set of them.
///
/// A word shorter than the padding still yields grams; only the empty word at
/// a size of one has none, there being nothing to pad it with.
///
/// # Panics
///
/// If `size` is zero, there being no such thing as a run of no characters.
///
/// # Examples
///
/// ```
/// use n_gram_index::grams;
///
/// assert_eq!(grams("she", 2), ["#s", "sh", "he", "e#"]);
/// assert_eq!(grams("banana", 2), ["#b", "ba", "an", "na", "an", "na", "a#"]);
/// assert_eq!(grams("a", 3), ["##a", "#a#", "a##"]);
/// assert_eq!(grams("sea", 1), ["s", "e", "a"]);
/// ```
pub fn grams(word: &str, size: usize) -> Vec<String> {
    assert!(size > 0, "an n-gram is at least one character long");

    let padding = size - 1;
    let padded: Vec<char> = std::iter::repeat_n(BOUNDARY, padding)
        .chain(word.chars())
        .chain(std::iter::repeat_n(BOUNDARY, padding))
        .collect();

    padded
        .windows(size)
        .map(|gram| gram.iter().collect())
        .collect()
}

/// A word [`NGramIndex::find`] turned up, and how many grams it shares with
/// the query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub shared: usize,
    pub word: String,
}

/// A set of words, each filed under the grams it is made of.
///
/// # Examples
///
/// ```
/// use n_gram_index::NGramIndex;
///
/// let mut index = NGramIndex::new(3);
///
/// assert!(index.insert("shore"));
/// // The same word twice is one word
/// assert!(!index.insert("shore"));
/// assert_eq!(index.len(), 1);
/// assert!(index.contains("shore"));
/// ```
pub struct NGramIndex {
    size: usize,
    // Words by the position they were inserted at, which is the identifier the
    // postings carry: a gram lists numbers rather than repeating the words
    words: Vec<String>,
    // The same words the other way round, so inserting a word already stored
    // and asking whether one is stored cost a lookup rather than a scan
    ids: HashMap<String, usize>,
    // Every gram, with the words containing it. Ordered rather than hashed, so
    // the index reads the same way twice
    postings: BTreeMap<String, BTreeSet<usize>>,
}

impl NGramIndex {
    /// Creates an empty index over grams of `size` characters.
    ///
    /// # Panics
    ///
    /// If `size` is zero.
    pub fn new(size: usize) -> Self {
        assert!(size > 0, "an n-gram is at least one character long");

        NGramIndex {
            size,
            words: Vec::new(),
            ids: HashMap::new(),
            postings: BTreeMap::new(),
        }
    }

    /// Returns how many characters a gram of this index is.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Adds `word`, returning `false` if the index already held it.
    pub fn insert(&mut self, word: &str) -> bool {
        if self.ids.contains_key(word) {
            return false;
        }

        let id = self.words.len();
        for gram in grams(word, self.size) {
            // A gram a word repeats files it once: the postings are sets, and
            // a search asks each gram once too
            self.postings.entry(gram).or_default().insert(id);
        }

        self.words.push(word.to_string());
        self.ids.insert(word.to_string(), id);

        true
    }

    /// Returns the words sharing at least `minimum` grams with `query`, the
    /// ones sharing most first and alphabetically within a count.
    ///
    /// A word has to share a gram to be counted at all, so a minimum of zero
    /// asks the same question as a minimum of one. The empty word at a size of
    /// one shares nothing with anything, itself included, and is never found
    /// this way - [`contains`](NGramIndex::contains) still reports it.
    ///
    /// # Examples
    ///
    /// ```
    /// use n_gram_index::NGramIndex;
    ///
    /// let mut index = NGramIndex::new(2);
    /// for word in ["sea", "seat", "shore"] {
    ///     index.insert(word);
    /// }
    ///
    /// let words: Vec<String> = index
    ///     .find("sea", 1)
    ///     .into_iter()
    ///     .map(|found| found.word)
    ///     .collect();
    ///
    /// assert_eq!(words, ["sea", "seat", "shore"]);
    ///
    /// // `shore` shares only `#s`, so a stricter minimum drops it
    /// let words: Vec<String> = index
    ///     .find("sea", 2)
    ///     .into_iter()
    ///     .map(|found| found.word)
    ///     .collect();
    ///
    /// assert_eq!(words, ["sea", "seat"]);
    /// ```
    pub fn find(&self, query: &str, minimum: usize) -> Vec<Match> {
        // Each gram asked once, however often the query repeats it
        let asked: BTreeSet<String> = grams(query, self.size).into_iter().collect();

        let mut shared: HashMap<usize, usize> = HashMap::new();
        for gram in asked {
            for id in self.postings.get(&gram).into_iter().flatten() {
                *shared.entry(*id).or_default() += 1;
            }
        }

        let minimum = minimum.max(1);
        let mut matches: Vec<Match> = shared
            .into_iter()
            .filter(|(_, shared)| *shared >= minimum)
            .map(|(id, shared)| Match {
                shared,
                word: self.words[id].clone(),
            })
            .collect();

        // Most shared first, which is the reverse of the order counts sort in,
        // and alphabetically between words sharing as much
        matches.sort_by(|left, right| {
            right
                .shared
                .cmp(&left.shared)
                .then_with(|| left.word.cmp(&right.word))
        });

        matches
    }

    /// Returns `true` if `word` is stored, exactly as spelled.
    pub fn contains(&self, word: &str) -> bool {
        self.ids.contains_key(word)
    }

    /// Returns how many words are stored.
    pub fn len(&self) -> usize {
        self.words.len()
    }

    /// Returns `true` if no word is stored.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}
