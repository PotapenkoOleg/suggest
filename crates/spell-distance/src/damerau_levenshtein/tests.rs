#[cfg(test)]
mod tests {
    use crate::{distance, distance_of};

    // The case the crate is named for: unrestricted Damerau-Levenshtein
    // answers 2 by transposing `CA` into `AC` and inserting `B` between the
    // two elements it just swapped. Restricted, that pair is spent - the
    // cheapest alignment left is three edits
    #[test]
    fn a_transposed_pair_cannot_be_edited_again() {
        assert_eq!(distance("CA", "ABC"), 3);
    }

    #[test]
    fn equal_sequences_are_no_edits_apart() {
        for word in ["", "a", "sea", "shore", "naïve"] {
            assert_eq!(distance(word, word), 0, "{word} differs from itself");
        }
    }

    #[test]
    fn an_empty_side_costs_one_edit_per_element_of_the_other() {
        assert_eq!(distance("", ""), 0);
        assert_eq!(distance("", "sea"), 3);
        assert_eq!(distance("shore", ""), 5);
    }

    #[test]
    fn a_single_edit_of_each_kind_costs_one() {
        // Insertion, deletion, substitution, transposition
        assert_eq!(distance("sea", "seas"), 1);
        assert_eq!(distance("shore", "hore"), 1);
        assert_eq!(distance("sea", "tea"), 1);
        assert_eq!(distance("teh", "the"), 1);
    }

    // A transposition is one edit here where plain Levenshtein charges two
    #[test]
    fn transpositions_are_charged_once() {
        assert_eq!(distance("ca", "ac"), 1);
        assert_eq!(distance("shells", "shesll"), 2);
        assert_eq!(distance("distance", "distacne"), 1);
    }

    // The textbook pairs, which any implementation is expected to agree on
    #[test]
    fn known_pairs_keep_their_published_distances() {
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("saturday", "sunday"), 3);
        assert_eq!(distance("sea", "shore"), 4);
    }

    #[test]
    fn distance_does_not_depend_on_the_order_of_the_arguments() {
        let pairs = [
            ("CA", "ABC"),
            ("", "sea"),
            ("kitten", "sitting"),
            ("saturday", "sunday"),
            ("teh", "the"),
            ("naïve", "naive"),
            ("a", "shore"),
        ];

        for (left, right) in pairs {
            assert_eq!(
                distance(left, right),
                distance(right, left),
                "{left} to {right} is not {right} to {left}"
            );
        }
    }

    // No alignment can be cheaper than matching the lengths, nor dearer than
    // rewriting the longer sequence outright
    #[test]
    fn distance_stays_between_the_length_difference_and_the_longer_length() {
        let words = ["", "a", "sea", "shore", "seashore", "shoreline"];

        for left in words {
            for right in words {
                let distance = distance(left, right);
                let (shorter, longer) = if left.len() <= right.len() {
                    (left.len(), right.len())
                } else {
                    (right.len(), left.len())
                };

                assert!(distance >= longer - shorter, "{left} to {right}");
                assert!(distance <= longer, "{left} to {right}");
            }
        }
    }

    // Characters, not bytes: `ï` takes two bytes and `☃` three, and neither
    // may cost more than the one edit that replaces it
    #[test]
    fn multi_byte_characters_count_as_one_element() {
        assert_eq!(distance("naïve", "naive"), 1);
        assert_eq!(distance("naïve", "naïve"), 0);
        assert_eq!(distance("☃", "x"), 1);
        assert_eq!(distance("sea☃", "sea"), 1);
        // Transposed either side of the boundary a byte-wise pass would split
        assert_eq!(distance("ïa", "aï"), 1);
    }

    // The `&str` entry point is the char-sequence one with the conversion done
    #[test]
    fn distance_of_takes_any_comparable_elements() {
        assert_eq!(distance_of(&[1, 2, 3], &[2, 1, 3]), 1);
        assert_eq!(distance_of(b"teh", b"the"), 1);
        assert_eq!(distance_of(&["by", "the", "sea"], &["by", "sea", "the"]), 1);
        assert_eq!(
            distance_of(&['s', 'e', 'a'], &['s', 'h', 'o', 'r', 'e']),
            distance("sea", "shore")
        );
    }
}
