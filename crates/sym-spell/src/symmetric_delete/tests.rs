#[cfg(test)]
mod tests {
    use crate::{Suggestion, SymSpell, deletes};
    use spell_distance::distance;

    // "she sells sea shells by the sea shore", counted as the phrase spells it
    const WORDS: [(&str, u64); 7] = [
        ("she", 1),
        ("sells", 1),
        ("sea", 2),
        ("shells", 1),
        ("by", 1),
        ("the", 1),
        ("shore", 1),
    ];

    fn speller(max_distance: usize) -> SymSpell {
        let mut speller = SymSpell::new(max_distance);
        for (word, count) in WORDS {
            speller.insert(word, count);
        }

        speller
    }

    fn words_of(suggestions: &[Suggestion]) -> Vec<&str> {
        suggestions
            .iter()
            .map(|found| found.word.as_str())
            .collect()
    }

    /// What the dictionary is supposed to answer: every word within the
    /// distance, found by measuring all of them.
    fn scan(words: &[(&str, u64)], query: &str, distance_limit: usize) -> Vec<Suggestion> {
        let mut suggestions: Vec<Suggestion> = words
            .iter()
            .map(|(word, count)| Suggestion {
                distance: distance(query, word),
                count: *count,
                word: word.to_string(),
            })
            .filter(|found| found.distance <= distance_limit)
            .collect();

        suggestions.sort_by(|left, right| {
            left.distance
                .cmp(&right.distance)
                .then_with(|| right.count.cmp(&left.count))
                .then_with(|| left.word.cmp(&right.word))
        });

        suggestions
    }

    #[test]
    fn a_word_stands_for_itself_and_for_what_dropping_characters_leaves() {
        assert_eq!(deletes("sea", 0).into_iter().collect::<Vec<_>>(), ["sea"]);
        assert_eq!(
            deletes("sea", 1).into_iter().collect::<Vec<_>>(),
            ["ea", "sa", "se", "sea"]
        );
        assert_eq!(
            deletes("sea", 2).into_iter().collect::<Vec<_>>(),
            ["a", "e", "ea", "s", "sa", "se", "sea"]
        );
    }

    // Two ways of dropping to the same string are one entry, or a word would
    // be filed twice under it
    #[test]
    fn the_same_string_reached_twice_is_kept_once() {
        assert_eq!(
            deletes("anna", 1).into_iter().collect::<Vec<_>>(),
            ["ana", "ann", "anna", "nna"]
        );
    }

    #[test]
    fn characters_are_dropped_and_not_bytes() {
        // Dropping the `ï` leaves `nave`, not the four bytes of `naïve` cut
        // one short
        assert_eq!(
            deletes("naïve", 1).into_iter().collect::<Vec<_>>(),
            ["aïve", "nave", "naïe", "naïv", "naïve", "nïve"]
        );
    }

    #[test]
    fn dropping_more_characters_than_a_word_has_leaves_the_empty_string() {
        assert_eq!(deletes("a", 2).into_iter().collect::<Vec<_>>(), ["", "a"]);
        assert_eq!(deletes("", 2).into_iter().collect::<Vec<_>>(), [""]);
    }

    #[test]
    fn an_empty_dictionary_holds_and_offers_nothing() {
        let speller = SymSpell::new(2);

        assert!(speller.is_empty());
        assert_eq!(speller.len(), 0);
        assert_eq!(speller.max_distance(), 2);
        assert!(!speller.contains("sea"));
        assert_eq!(speller.count("sea"), None);
        assert_eq!(speller.lookup("sea", 2), []);
    }

    #[test]
    fn a_word_met_again_is_counted_and_not_filed_again() {
        let mut speller = SymSpell::new(1);

        assert!(speller.insert("sea", 2));
        assert!(!speller.insert("sea", 3));

        assert_eq!(speller.len(), 1);
        assert_eq!(speller.count("sea"), Some(5));
        assert!(speller.contains("sea"));

        // Filed twice, the word would be offered twice
        assert_eq!(
            speller.lookup("se", 1),
            [Suggestion {
                distance: 1,
                count: 5,
                word: "sea".to_string()
            }]
        );
    }

    #[test]
    fn a_correctly_spelled_word_finds_itself() {
        let speller = speller(2);

        for (word, _) in WORDS {
            assert_eq!(speller.lookup(word, 0), scan(&WORDS, word, 0), "{word}");
            assert_eq!(speller.lookup(word, 2)[0].word, word, "{word}");
        }
    }

    // Every kind of edit the distance charges one for, met through the same
    // index
    #[test]
    fn a_misspelling_of_each_kind_finds_the_word() {
        let speller = speller(1);

        // Substitution, deletion, insertion, transposition
        assert_eq!(words_of(&speller.lookup("shu", 1)), ["she"]);
        assert_eq!(words_of(&speller.lookup("shor", 1)), ["shore"]);
        assert_eq!(words_of(&speller.lookup("shell", 1)), ["shells"]);
        assert_eq!(words_of(&speller.lookup("hte", 1)), ["the"]);
    }

    #[test]
    fn the_nearest_word_comes_first_and_the_commonest_of_those() {
        let speller = speller(2);

        // `sea` and `she` are both one edit from `seh`, and `sea` was counted
        // twice
        assert_eq!(
            speller.lookup("seh", 2),
            [
                Suggestion {
                    distance: 1,
                    count: 2,
                    word: "sea".to_string()
                },
                Suggestion {
                    distance: 1,
                    count: 1,
                    word: "she".to_string()
                },
                Suggestion {
                    distance: 2,
                    count: 1,
                    word: "the".to_string()
                },
            ]
        );
    }

    #[test]
    fn a_query_beyond_every_word_is_answered_with_nothing() {
        let speller = speller(2);

        assert_eq!(speller.lookup("xyzzy", 2), []);
    }

    // The dictionary knows what it was built for: answering further out would
    // be answering short, since those words were never filed where the query
    // would meet them
    #[test]
    #[should_panic(expected = "cannot answer for 3 edits")]
    fn a_distance_beyond_the_dictionary_is_refused() {
        speller(2).lookup("sea", 3);
    }

    #[test]
    fn a_dictionary_of_no_edits_is_an_exact_search() {
        let speller = speller(0);

        assert_eq!(words_of(&speller.lookup("sea", 0)), ["sea"]);
        assert_eq!(speller.lookup("seh", 0), []);
    }

    #[test]
    fn words_are_measured_by_character_and_not_by_byte() {
        let mut speller = SymSpell::new(1);
        for word in ["naïve", "native", "naive"] {
            speller.insert(word, 1);
        }

        // `native` is an inserted `t` away, `naïve` a substituted character
        assert_eq!(
            words_of(&speller.lookup("naive", 1)),
            ["naive", "native", "naïve"]
        );
        assert!(speller.contains("naïve"));
    }

    // The empty query is every word's length away from it, so a wide enough
    // dictionary still meets the shortest ones
    #[test]
    fn the_empty_query_reaches_the_shortest_words() {
        let speller = speller(2);

        assert_eq!(words_of(&speller.lookup("", 2)), ["by"]);
    }

    // Deleting on both sides is not a heuristic: whatever the edit, one
    // deletion on each side reaches the same string, so the index finds every
    // word a measurement of all of them would have found. A disagreement here
    // is a fault, not a property of the data
    #[test]
    fn the_lookup_agrees_with_measuring_every_word() {
        let queries = [
            "sea",
            "seh",
            "shell",
            "hte",
            "shre",
            "by",
            "xyzzy",
            "",
            "sells",
            "shoreline",
        ];

        for max_distance in 0..=3 {
            let speller = speller(max_distance);

            for query in queries {
                for distance in 0..=max_distance {
                    assert_eq!(
                        speller.lookup(query, distance),
                        scan(&WORDS, query, distance),
                        "{query} within {distance} of a dictionary of {max_distance}"
                    );
                }
            }
        }
    }

    // The same claim over the strings that broke the triangle inequality the
    // BK-tree leans on: nothing here leans on it, so the crowded neighbourhood
    // costs nothing
    #[test]
    fn the_lookup_agrees_where_a_distance_that_is_no_metric_would_cost_a_search() {
        let words: Vec<(&str, u64)> = [
            "ca", "ac", "abc", "cab", "bca", "bac", "abcd", "acbd", "a", "b", "ab", "ba", "",
        ]
        .into_iter()
        .map(|word| (word, 1))
        .collect();

        for max_distance in 0..=2 {
            let mut speller = SymSpell::new(max_distance);
            for (word, count) in &words {
                speller.insert(word, *count);
            }

            for (query, _) in &words {
                for distance in 0..=max_distance {
                    assert_eq!(
                        speller.lookup(query, distance),
                        scan(&words, query, distance),
                        "{query} within {distance} of a dictionary of {max_distance}"
                    );
                }
            }

            // And queries the dictionary never saw
            for query in ["cba", "acb", "dcba", "aabbcc", "cc"] {
                for distance in 0..=max_distance {
                    assert_eq!(
                        speller.lookup(query, distance),
                        scan(&words, query, distance),
                        "{query} within {distance} of a dictionary of {max_distance}"
                    );
                }
            }
        }
    }
}
