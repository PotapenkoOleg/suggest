#[cfg(test)]
mod tests {
    use crate::{BOUNDARY, Match, NGramIndex, grams};
    use std::collections::BTreeSet;

    // "she sells sea shells by the sea shore", the corpus the other crates in
    // this repository are tested with
    const WORDS: [&str; 7] = ["she", "sells", "sea", "shells", "by", "the", "shore"];

    fn index(size: usize) -> NGramIndex {
        let mut index = NGramIndex::new(size);
        for word in WORDS {
            index.insert(word);
        }

        index
    }

    fn words_of(matches: &[Match]) -> Vec<&str> {
        matches.iter().map(|found| found.word.as_str()).collect()
    }

    /// What the index is supposed to answer: the words sharing grams with the
    /// query, counted by intersecting the sets outright.
    fn scan(words: &[&str], query: &str, size: usize, minimum: usize) -> Vec<Match> {
        let asked: BTreeSet<String> = grams(query, size).into_iter().collect();

        let mut matches: Vec<Match> = words
            .iter()
            .map(|word| {
                let held: BTreeSet<String> = grams(word, size).into_iter().collect();

                Match {
                    shared: asked.intersection(&held).count(),
                    word: word.to_string(),
                }
            })
            .filter(|found| found.shared >= minimum.max(1))
            .collect();

        matches.sort_by(|left, right| {
            right
                .shared
                .cmp(&left.shared)
                .then_with(|| left.word.cmp(&right.word))
        });

        matches
    }

    #[test]
    fn a_word_is_cut_into_runs_padded_at_both_ends() {
        assert_eq!(grams("sea", 1), ["s", "e", "a"]);
        assert_eq!(grams("sea", 2), ["#s", "se", "ea", "a#"]);
        assert_eq!(grams("sea", 3), ["##s", "#se", "sea", "ea#", "a##"]);

        // Each character of the word carries `size` grams, and the padding is
        // what buys the first and last theirs
        for size in 1..=4 {
            assert_eq!(grams("sea", size).len(), 3 + size - 1, "size {size}");
        }
    }

    #[test]
    fn a_word_shorter_than_a_gram_still_has_grams() {
        assert_eq!(grams("a", 3), ["##a", "#a#", "a##"]);
        assert_eq!(grams("ab", 4), ["###a", "##ab", "#ab#", "ab##", "b###"]);
    }

    // The empty word is padding and nothing else, so at a size of one there is
    // nothing left of it
    #[test]
    fn the_empty_word_is_all_boundary() {
        assert_eq!(grams("", 3), ["###", "###"]);
        assert_eq!(grams("", 2), ["##"]);
        assert_eq!(grams("", 1), Vec::<String>::new());
    }

    #[test]
    fn runs_are_taken_over_characters_and_not_bytes() {
        // `ï` is two bytes, and cutting between them would spell neither word
        assert_eq!(grams("naïve", 2), ["#n", "na", "aï", "ïv", "ve", "e#"]);
    }

    #[test]
    fn a_run_the_word_repeats_is_listed_each_time() {
        assert_eq!(
            grams("banana", 2),
            ["#b", "ba", "an", "na", "an", "na", "a#"]
        );
    }

    // The marker is an ordinary character: a word spelling it out shares grams
    // with the boundaries of others
    #[test]
    fn the_boundary_marker_is_a_character_like_any_other() {
        assert_eq!(BOUNDARY, '#');
        assert_eq!(grams("c#", 2), ["#c", "c#", "##"]);

        let mut index = NGramIndex::new(2);
        index.insert("c#");
        index.insert("c");

        // `#c` opens both, `c#` closes both
        assert_eq!(
            index.find("c", 1),
            [
                Match {
                    shared: 2,
                    word: "c".to_string()
                },
                Match {
                    shared: 2,
                    word: "c#".to_string()
                }
            ]
        );
    }

    #[test]
    #[should_panic(expected = "at least one character")]
    fn a_gram_of_no_characters_is_refused() {
        grams("sea", 0);
    }

    #[test]
    #[should_panic(expected = "at least one character")]
    fn an_index_of_no_characters_is_refused() {
        NGramIndex::new(0);
    }

    #[test]
    fn an_empty_index_holds_and_finds_nothing() {
        let index = NGramIndex::new(2);

        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
        assert_eq!(index.size(), 2);
        assert!(!index.contains("sea"));
        assert_eq!(index.find("sea", 1), []);
    }

    #[test]
    fn the_same_word_is_stored_once() {
        let mut index = NGramIndex::new(2);

        assert!(index.insert("shore"));
        assert!(!index.insert("shore"));
        assert!(index.insert("sea"));

        assert_eq!(index.len(), 2);
        assert!(index.contains("shore"));
        assert!(!index.contains("shored"));
        // The repeat did not double the count either
        assert_eq!(index.find("shore", 1)[0].shared, 6);
    }

    #[test]
    fn words_come_back_by_how_much_they_share() {
        let index = index(2);

        assert_eq!(
            index.find("shell", 1),
            [
                Match {
                    shared: 5,
                    word: "shells".to_string()
                },
                Match {
                    shared: 3,
                    word: "sells".to_string()
                },
                Match {
                    shared: 3,
                    word: "she".to_string()
                },
                Match {
                    shared: 2,
                    word: "shore".to_string()
                },
                Match {
                    shared: 1,
                    word: "sea".to_string()
                },
                Match {
                    shared: 1,
                    word: "the".to_string()
                },
            ]
        );
    }

    #[test]
    fn a_minimum_keeps_the_answer_to_the_words_worth_it() {
        let index = index(2);

        assert_eq!(
            words_of(&index.find("shell", 3)),
            ["shells", "sells", "she"]
        );
        assert_eq!(words_of(&index.find("shell", 6)), Vec::<&str>::new());

        // Nothing shares nothing, so no minimum can be lower than one
        assert_eq!(index.find("shell", 0), index.find("shell", 1));
    }

    // A query repeating a gram asks about it once, or a word would score for
    // how often the query said the same thing
    #[test]
    fn a_gram_the_query_repeats_counts_once() {
        let mut index = NGramIndex::new(2);
        index.insert("banana");
        index.insert("ana");

        // `#b ba an na a#` - five distinct grams, not the seven `banana` cuts
        // into
        assert_eq!(
            index.find("banana", 1),
            [
                Match {
                    shared: 5,
                    word: "banana".to_string()
                },
                Match {
                    shared: 3,
                    word: "ana".to_string()
                },
            ]
        );
    }

    #[test]
    fn words_are_matched_by_character_and_not_by_byte() {
        let mut index = NGramIndex::new(2);
        for word in ["naïve", "native", "naive"] {
            index.insert(word);
        }

        assert_eq!(
            words_of(&index.find("naive", 1)),
            ["naive", "native", "naïve"]
        );
        assert!(index.contains("naïve"));
    }

    // Findable by name, but sharing nothing with anything: at a size of one it
    // has no grams to share
    #[test]
    fn an_empty_word_is_stored_and_never_shared() {
        let mut index = NGramIndex::new(1);

        assert!(index.insert(""));
        assert!(index.contains(""));
        assert_eq!(index.len(), 1);
        assert_eq!(index.find("", 1), []);
        assert_eq!(index.find("sea", 1), []);
    }

    // Unlike a search that prunes, an index answers exactly: it is the same
    // question asked of a posting list rather than of every word, so any
    // disagreement here is a fault and not a property of the data
    #[test]
    fn the_index_agrees_with_intersecting_every_word() {
        let queries = ["sea", "shell", "hse", "by", "sure", "xyz", "", "banana"];

        for size in 1..=4 {
            let index = index(size);

            for query in queries {
                for minimum in 0..=3 {
                    assert_eq!(
                        index.find(query, minimum),
                        scan(&WORDS, query, size, minimum),
                        "{query} at size {size}, minimum {minimum}"
                    );
                }
            }
        }
    }
}
