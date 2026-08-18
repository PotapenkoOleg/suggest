//! SymSpell: spelling correction by symmetric deletes.
//!
//! Correcting a misspelling by generating its neighbourhood costs more with
//! every edit allowed and every character in the alphabet - insertions and
//! substitutions have to try every letter. Deletions do not: a word of `n`
//! characters has `n` of them, whatever the alphabet.
//!
//! So both sides delete. Every dictionary word is filed under each string it
//! becomes when up to `max_distance` characters are dropped, and a query is
//! looked up by the same strings dropped from it. Whatever an edit is, at most
//! one deletion on each side reaches the same string:
//!
//! - a substitution, by dropping the character that differs on both sides
//! - an insertion, by dropping the character one side has and the other has not
//! - a transposition, by dropping either of the two swapped characters on each
//!   side
//!
//! Whatever the query and a word within `max_distance` of each other are, they
//! meet on some string. That makes the index exact rather than a heuristic:
//! what it finds is what comparing the query to every word would have found,
//! and the check below holds it to that.
//!
//! ```
//! use sym_spell::{SymSpell, deletes};
//!
//! // `sea` and everything left of it when a character is dropped
//! assert_eq!(
//!     deletes("sea", 1).into_iter().collect::<Vec<_>>(),
//!     ["ea", "sa", "se", "sea"]
//! );
//!
//! let mut speller = SymSpell::new(2);
//! speller.insert("shore", 5);
//! speller.insert("she", 90);
//!
//! // `shre` is one edit from either - an `o` put back, or an `r` taken out -
//! // so the count decides which is offered first
//! let suggestions = speller.lookup("shre", 2);
//! assert_eq!(suggestions[0].word, "she");
//! assert_eq!(suggestions[1].word, "shore");
//! assert!(suggestions.iter().all(|found| found.distance == 1));
//! ```
//!
//! The cost is the index: a word of `n` characters is filed under one string
//! per way of dropping up to `max_distance` of them, so a larger distance buys
//! reach with memory. Nothing here shortens that by indexing only a prefix of
//! each word, the way the original does - a prefix bounds the memory but stops
//! the index being exact, which is the property worth keeping.

use spell_distance::distance_of;
use std::collections::{BTreeSet, HashMap};

mod tests;

/// A word the dictionary offers for a query, with how far it is and how often
/// it was counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub distance: usize,
    pub count: u64,
    pub word: String,
}

/// Returns `word` together with every string it becomes when up to `distance`
/// characters are dropped from it.
///
/// Characters, not bytes, so dropping one from `naïve` leaves four characters
/// and not a broken encoding. The word itself is part of the set: a query that
/// is spelled correctly has to meet its own entry.
///
/// # Examples
///
/// ```
/// use sym_spell::deletes;
///
/// assert_eq!(
///     deletes("sea", 1).into_iter().collect::<Vec<_>>(),
///     ["ea", "sa", "se", "sea"]
/// );
///
/// // Dropping either `n` of `anna` leaves the same string, and the set holds
/// // it once
/// assert_eq!(
///     deletes("anna", 1).into_iter().collect::<Vec<_>>(),
///     ["ana", "ann", "anna", "nna"]
/// );
/// ```
pub fn deletes(word: &str, distance: usize) -> BTreeSet<String> {
    let mut variants = BTreeSet::new();
    variants.insert(word.to_string());

    // One round per character dropped, each starting from what the last round
    // produced, so a string is never built twice from the same deletions
    let mut round: BTreeSet<Vec<char>> = BTreeSet::from([word.chars().collect()]);
    for _ in 0..distance {
        let mut shorter = BTreeSet::new();

        for variant in &round {
            for at in 0..variant.len() {
                let mut dropped = variant.clone();
                dropped.remove(at);
                shorter.insert(dropped);
            }
        }

        variants.extend(
            shorter
                .iter()
                .map(|variant| variant.iter().collect::<String>()),
        );
        round = shorter;
    }

    variants
}

// A dictionary word. The characters are what the distance is measured over,
// and the word itself is spelled back out of them when it is offered
struct Entry {
    chars: Vec<char>,
    count: u64,
}

impl Entry {
    fn word(&self) -> String {
        self.chars.iter().collect()
    }
}

/// A dictionary of words that answers which of them a query might have been.
///
/// # Examples
///
/// ```
/// use sym_spell::SymSpell;
///
/// let mut speller = SymSpell::new(1);
///
/// // A word met for the first time, then counted again
/// assert!(speller.insert("shore", 2));
/// assert!(!speller.insert("shore", 3));
///
/// assert_eq!(speller.len(), 1);
/// assert_eq!(speller.count("shore"), Some(5));
/// ```
pub struct SymSpell {
    max_distance: usize,
    // Words by the position they were inserted at, which is the identifier the
    // index carries
    words: Vec<Entry>,
    // The same words the other way round, so a word met twice is counted
    // rather than filed twice
    ids: HashMap<String, usize>,
    // Every string a word becomes when characters are dropped, with the words
    // it came from. Two words leaving the same string behind share an entry -
    // which is the whole point, since that is how a misspelling meets what it
    // was meant to be
    variants: HashMap<String, Vec<usize>>,
}

impl SymSpell {
    /// Creates an empty dictionary reaching `max_distance` edits from a query.
    ///
    /// A distance of zero indexes nothing but the words themselves, which
    /// makes [`lookup`](SymSpell::lookup) an exact search.
    pub fn new(max_distance: usize) -> Self {
        SymSpell {
            max_distance,
            words: Vec::new(),
            ids: HashMap::new(),
            variants: HashMap::new(),
        }
    }

    /// Returns how many edits from a query this dictionary can reach.
    pub fn max_distance(&self) -> usize {
        self.max_distance
    }

    /// Adds `count` occurrences of `word`, returning `false` if the dictionary
    /// already held it and this only raised its count.
    ///
    /// Counts add up because a word met twice is the same word met twice; only
    /// the first meeting files it under its deletions.
    pub fn insert(&mut self, word: &str, count: u64) -> bool {
        if let Some(id) = self.ids.get(word) {
            self.words[*id].count += count;
            return false;
        }

        let id = self.words.len();
        for variant in deletes(word, self.max_distance) {
            self.variants.entry(variant).or_default().push(id);
        }

        self.words.push(Entry {
            chars: word.chars().collect(),
            count,
        });
        self.ids.insert(word.to_string(), id);

        true
    }

    /// Returns the words within `distance` edits of `query`, nearest first,
    /// the commonest first among words equally near.
    ///
    /// # Panics
    ///
    /// If `distance` is beyond the `max_distance` the dictionary was built
    /// for. The words that far out were never filed under the strings a query
    /// would meet them by, so the answer would be missing some of them rather
    /// than merely narrow.
    ///
    /// # Examples
    ///
    /// ```
    /// use sym_spell::SymSpell;
    ///
    /// let mut speller = SymSpell::new(2);
    /// speller.insert("sea", 40);
    /// speller.insert("seat", 3);
    ///
    /// let words: Vec<String> = speller
    ///     .lookup("set", 2)
    ///     .into_iter()
    ///     .map(|found| found.word)
    ///     .collect();
    ///
    /// assert_eq!(words, ["sea", "seat"]);
    /// ```
    pub fn lookup(&self, query: &str, distance: usize) -> Vec<Suggestion> {
        assert!(
            distance <= self.max_distance,
            "a dictionary of {} cannot answer for {distance} edits",
            self.max_distance
        );

        // Where the query could meet a word: both sides having dropped up to
        // `distance` characters, they meet on a string one of them is filed
        // under
        let mut candidates: BTreeSet<usize> = BTreeSet::new();
        for variant in deletes(query, distance) {
            candidates.extend(self.variants.get(&variant).into_iter().flatten());
        }

        let query: Vec<char> = query.chars().collect();

        // Meeting on a string only says a word is worth measuring: `sea` and
        // `set` both leave `se`, but so do `sea` and `sets`
        let mut suggestions: Vec<Suggestion> = candidates
            .into_iter()
            .filter_map(|id| {
                let entry = &self.words[id];
                let measured = distance_of(&query, &entry.chars);

                (measured <= distance).then(|| Suggestion {
                    distance: measured,
                    count: entry.count,
                    word: entry.word(),
                })
            })
            .collect();

        // Nearest first, then the commonest of the words equally near, then
        // alphabetically so the answer does not depend on insertion order
        suggestions.sort_by(|left, right| {
            left.distance
                .cmp(&right.distance)
                .then_with(|| right.count.cmp(&left.count))
                .then_with(|| left.word.cmp(&right.word))
        });

        suggestions
    }

    /// Returns how often `word` was counted, or `None` if it is not stored.
    pub fn count(&self, word: &str) -> Option<u64> {
        self.ids.get(word).map(|id| self.words[*id].count)
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
