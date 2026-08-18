#[cfg(test)]
mod tests {
    use crate::{BkTree, Match};
    use spell_distance::distance;

    // "she sells sea shells by the sea shore", the corpus the tries in this
    // repository are tested with
    const WORDS: [&str; 7] = ["she", "sells", "sea", "shells", "by", "the", "shore"];

    fn tree() -> BkTree {
        WORDS.into_iter().collect()
    }

    fn words_of(matches: &[Match]) -> Vec<&str> {
        matches.iter().map(|found| found.word.as_str()).collect()
    }

    /// What the tree is supposed to answer: every word within the radius,
    /// found by measuring all of them.
    fn scan(words: &[&str], query: &str, radius: usize) -> Vec<Match> {
        let mut matches: Vec<Match> = words
            .iter()
            .map(|word| Match {
                distance: distance(word, query),
                word: word.to_string(),
            })
            .filter(|found| found.distance <= radius)
            .collect();

        matches.sort();
        matches
    }

    #[test]
    fn an_empty_tree_holds_and_finds_nothing() {
        let tree = BkTree::new();

        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
        assert!(!tree.contains("sea"));
        assert_eq!(tree.find("sea", 3), []);
    }

    #[test]
    fn the_same_word_is_stored_once() {
        let mut tree = BkTree::new();

        assert!(tree.insert("shore"));
        assert!(!tree.insert("shore"));
        // Also when it is not the root the duplicate meets first
        assert!(tree.insert("sea"));
        assert!(!tree.insert("sea"));

        assert_eq!(tree.len(), 2);
        assert!(!tree.is_empty());
    }

    #[test]
    fn every_word_inserted_is_found_at_no_distance() {
        let tree = tree();

        assert_eq!(tree.len(), WORDS.len());
        for word in WORDS {
            assert!(tree.contains(word), "{word} was inserted but not found");
            assert_eq!(words_of(&tree.find(word, 0)), [word]);
        }

        assert!(!tree.contains("shored"));
    }

    #[test]
    fn a_radius_widens_the_answer_to_the_neighbours() {
        let tree = tree();

        assert_eq!(words_of(&tree.find("shell", 1)), ["shells"]);
        assert_eq!(words_of(&tree.find("shell", 2)), ["shells", "sells", "she"]);
    }

    // Nearest first, and alphabetical between words the same distance away
    #[test]
    fn matches_come_back_in_order() {
        let tree: BkTree = ["ape", "apple", "maple", "ample"].into_iter().collect();

        let matches = tree.find("ample", 2);

        assert_eq!(words_of(&matches), ["ample", "apple", "maple", "ape"]);
        assert_eq!(
            matches.iter().map(|found| found.distance).collect::<Vec<_>>(),
            [0, 1, 1, 2]
        );
    }

    // The distance is Damerau-Levenshtein, so a transposition is one edit and
    // not the two plain Levenshtein charges for it
    #[test]
    fn a_transposition_is_one_edit_away() {
        let tree = tree();

        assert_eq!(words_of(&tree.find("hse", 1)), ["she"]);
        assert_eq!(words_of(&tree.find("selsl", 1)), ["sells"]);
    }

    #[test]
    fn words_are_matched_by_character_and_not_by_byte() {
        let tree: BkTree = ["naïve", "native", "naive"].into_iter().collect();

        // The two bytes `ï` takes cost the one edit that replaces it, the same
        // as the one byte `t` takes
        assert_eq!(
            tree.find("naive", 1),
            [
                Match { distance: 0, word: "naive".to_string() },
                Match { distance: 1, word: "native".to_string() },
                Match { distance: 1, word: "naïve".to_string() },
            ]
        );
        assert!(tree.contains("naïve"));
    }

    #[test]
    fn an_empty_word_is_a_word() {
        let mut tree = tree();

        assert!(tree.insert(""));
        assert!(tree.contains(""));
        // Every word is its own length away from it, so a wide enough radius
        // reaches the shortest ones
        assert_eq!(words_of(&tree.find("", 2)), ["", "by"]);
    }

    // The claim the pruning makes: descending only the children within reach
    // finds what measuring every word would have found
    #[test]
    fn the_search_agrees_with_measuring_every_word() {
        let tree = tree();
        let queries = ["sea", "shell", "hse", "by", "sure", "xyz", ""];

        for query in queries {
            for radius in 0..=4 {
                assert_eq!(
                    tree.find(query, radius),
                    scan(&WORDS, query, radius),
                    "{query} within {radius}"
                );
            }
        }
    }

    // Where the claim above stops holding, pinned rather than papered over.
    // The pruning needs the triangle inequality, and the restricted distance
    // does not offer it: `abc` is three edits from `ca` but one from `ac`,
    // which is itself one edit from `ca`. Filed three deep under the root, it
    // is out of reach of a search that only descends one
    #[test]
    fn a_search_can_miss_what_a_scan_would_find() {
        let words = ["ca", "ac", "abc"];

        assert_eq!(distance("ca", "abc"), 3);
        assert_eq!(distance("ca", "ac") + distance("ac", "abc"), 2);

        // `ca` is inserted first, so it is the root every search starts from
        let tree: BkTree = words.into_iter().collect();

        assert_eq!(words_of(&scan(&words, "ac", 1)), ["ac", "abc", "ca"]);
        assert_eq!(words_of(&tree.find("ac", 1)), ["ac", "ca"]);

        // No arrival order rescues it: the set and the distance decide, not
        // the shape the insertions happen to build
        let orders = [
            ["ca", "ac", "abc"],
            ["ca", "abc", "ac"],
            ["ac", "ca", "abc"],
            ["ac", "abc", "ca"],
            ["abc", "ca", "ac"],
            ["abc", "ac", "ca"],
        ];

        for order in orders {
            let tree: BkTree = order.into_iter().collect();

            assert_ne!(tree.find("ac", 1), scan(&words, "ac", 1), "{order:?}");
        }
    }

    #[test]
    fn a_tree_can_be_collected_from_words() {
        let tree: BkTree = ["sea", "sea", "shore"].into_iter().collect();

        // The repeat is one word
        assert_eq!(tree.len(), 2);

        let owned: BkTree = vec!["sea".to_string(), "shore".to_string()]
            .into_iter()
            .collect();

        assert_eq!(owned.len(), 2);
        assert!(owned.contains("shore"));
    }
}
