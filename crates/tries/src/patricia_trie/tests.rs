#[cfg(test)]
mod tests {
    use crate::{PatriciaTrie, PrefixSearch, SymbolTable, TernarySearchTrie};

    fn build_trie() -> PatriciaTrie<i32> {
        let mut trie = PatriciaTrie::new();
        trie.put("she".to_string(), 0);
        trie.put("sells".to_string(), 1);
        trie.put("sea".to_string(), 2);
        trie.put("shells".to_string(), 3);
        trie.put("by".to_string(), 4);
        trie.put("the".to_string(), 5);
        trie.put("sea".to_string(), 6);
        trie.put("shore".to_string(), 7);
        trie.put("a".to_string(), 8);
        trie
    }

    #[test]
    fn test_basic_operations() {
        let mut trie = PatriciaTrie::<String>::new();

        assert!(trie.is_empty());
        assert_eq!(trie.get_size(), 0);

        trie.put("hello".to_string(), "world".to_string());
        trie.put("help".to_string(), "me".to_string());
        trie.put("hell".to_string(), "yeah".to_string());

        assert_eq!(trie.get("hello"), Some("world".to_string()));
        assert_eq!(trie.get("help"), Some("me".to_string()));
        assert_eq!(trie.get("hell"), Some("yeah".to_string()));
        assert_eq!(trie.get("he"), None);
        assert_eq!(trie.get("helps"), None);

        assert!(trie.contains("hello"));
        assert!(!trie.contains("helloworld"));

        assert_eq!(trie.get_size(), 3);
        assert!(!trie.is_empty());

        // Replacing a value is not a new key
        trie.put("hello".to_string(), "again".to_string());
        assert_eq!(trie.get("hello"), Some("again".to_string()));
        assert_eq!(trie.get_size(), 3);

        trie.clear();
        assert!(trie.is_empty());
        assert_eq!(trie.get_size(), 0);
        assert_eq!(trie.get_all_keys(), Vec::<String>::new());
    }

    #[test]
    fn test_edge_splitting() {
        let mut trie = PatriciaTrie::<i32>::new();

        // Each insert forces a different split of the "appl" edge
        trie.put("application".to_string(), 1);
        trie.put("apply".to_string(), 2);
        trie.put("app".to_string(), 3);
        trie.put("a".to_string(), 4);
        trie.put("apple".to_string(), 5);

        assert_eq!(trie.get("application"), Some(1));
        assert_eq!(trie.get("apply"), Some(2));
        assert_eq!(trie.get("app"), Some(3));
        assert_eq!(trie.get("a"), Some(4));
        assert_eq!(trie.get("apple"), Some(5));
        assert_eq!(trie.get_size(), 5);

        // Intermediate nodes created by splitting are not keys themselves
        assert_eq!(trie.get("ap"), None);
        assert_eq!(trie.get("appl"), None);

        assert_eq!(
            trie.get_all_keys(),
            ["a", "app", "apple", "application", "apply"]
        );
    }

    #[test]
    fn test_delete_prunes_and_merges() {
        let mut trie = build_trie();
        let before = trie.get_size();

        // Deleting a leaf drops it; deleting an interior key must leave the
        // keys below it reachable
        trie.delete("she");
        assert_eq!(trie.get("she"), None);
        assert_eq!(trie.get("shells"), Some(3));
        assert_eq!(trie.get("shore"), Some(7));
        assert_eq!(trie.get_size(), before - 1);

        // Deleting an absent key changes nothing
        trie.delete("shel");
        trie.delete("nonexistent");
        assert_eq!(trie.get_size(), before - 1);

        trie.delete("shells");
        assert_eq!(trie.get("shore"), Some(7));
        assert_eq!(trie.get_keys_with_prefix("sh"), ["shore"]);

        // Emptying the trie one key at a time leaves it genuinely empty
        for key in trie.get_all_keys() {
            trie.delete(&key);
        }
        assert!(trie.is_empty());
        assert_eq!(trie.get_all_keys(), Vec::<String>::new());
        assert_eq!(trie.get_keys_with_prefix("s"), Vec::<String>::new());
    }

    #[test]
    fn test_prefix_operations() {
        let trie = build_trie();

        assert_eq!(
            trie.get_keys_with_prefix("s"),
            ["sea", "sells", "she", "shells", "shore"]
        );
        assert_eq!(trie.get_keys_with_prefix("sh"), ["she", "shells", "shore"]);
        assert_eq!(trie.get_keys_with_prefix("she"), ["she", "shells"]);
        assert_eq!(trie.get_keys_with_prefix("z"), Vec::<String>::new());

        // An empty prefix matches every key
        assert_eq!(trie.get_keys_with_prefix(""), trie.get_all_keys());

        assert_eq!(
            trie.longest_prefix_of("shellsort"),
            Some("shells".to_string())
        );
        assert_eq!(trie.longest_prefix_of("she"), Some("she".to_string()));
        assert_eq!(trie.longest_prefix_of("shel"), Some("she".to_string()));
        assert_eq!(trie.longest_prefix_of("xyz"), None);
        assert_eq!(trie.longest_prefix_of(""), None);
    }

    #[test]
    fn test_edge_cases() {
        let mut trie = PatriciaTrie::<i32>::new();

        // The empty string is not a storable key
        trie.put(String::new(), 1);
        assert_eq!(trie.get_size(), 0);
        assert_eq!(trie.get(""), None);
        assert!(!trie.contains(""));

        trie.delete("");
        assert_eq!(trie.get_size(), 0);

        // Queries against an empty trie
        assert_eq!(trie.get_all_keys(), Vec::<String>::new());
        assert_eq!(trie.get_keys_with_prefix(""), Vec::<String>::new());
        assert_eq!(trie.longest_prefix_of("anything"), None);

        // A key that is a prefix of another, inserted in both orders
        trie.put("ab".to_string(), 1);
        trie.put("a".to_string(), 2);
        assert_eq!(trie.get("a"), Some(2));
        assert_eq!(trie.get("ab"), Some(1));
        assert_eq!(trie.get_size(), 2);
    }

    #[test]
    fn test_multibyte_keys() {
        let mut trie = PatriciaTrie::<i32>::new();
        trie.put("héllo".to_string(), 1);
        trie.put("héllo wörld".to_string(), 2);
        trie.put("日本語".to_string(), 3);

        assert_eq!(trie.get("héllo"), Some(1));
        assert_eq!(trie.get("日本語"), Some(3));
        assert_eq!(trie.get_keys_with_prefix("hé"), ["héllo", "héllo wörld"]);

        // Positions are counted in characters, so slicing a multi-byte key back
        // out must not split a character
        assert_eq!(
            trie.longest_prefix_of("héllo there"),
            Some("héllo".to_string())
        );
        assert_eq!(
            trie.longest_prefix_of("日本語版"),
            Some("日本語".to_string())
        );
    }

    // Everything observable about a table, so two implementations can be
    // compared directly rather than by re-asserting the same expectations twice
    fn snapshot<T>(words: &[&str], queries: &[&str], to_delete: &[&str]) -> Vec<String>
    where
        T: SymbolTable<i32> + PrefixSearch + Default,
    {
        let mut table = T::default();
        let mut log = Vec::new();

        for (i, word) in words.iter().enumerate() {
            table.put(word.to_string(), i as i32);
        }
        log.push(format!(
            "size={} empty={} keys={:?}",
            table.get_size(),
            table.is_empty(),
            table.get_all_keys()
        ));

        for query in queries {
            log.push(format!(
                "{query}: get={:?} contains={} lpo={:?} prefix={:?}",
                table.get(query),
                table.contains(query),
                table.longest_prefix_of(query),
                table.get_keys_with_prefix(query)
            ));
        }

        for key in to_delete {
            table.delete(key);
        }
        log.push(format!(
            "post-delete size={} keys={:?}",
            table.get_size(),
            table.get_all_keys()
        ));

        for query in queries {
            log.push(format!(
                "post-delete {query}: get={:?} lpo={:?} prefix={:?}",
                table.get(query),
                table.longest_prefix_of(query),
                table.get_keys_with_prefix(query)
            ));
        }

        table.clear();
        log.push(format!(
            "cleared size={} empty={} keys={:?}",
            table.get_size(),
            table.is_empty(),
            table.get_all_keys()
        ));

        log
    }

    // ASCII only, deliberately: TernarySearchTrie::longest_prefix_of counts
    // matched characters but then slices the query by byte offset, so on
    // multi-byte input it either truncates ("héllo" -> "héll") or panics when
    // the offset lands inside a character. PatriciaTrie rebuilds from chars and
    // handles both - see test_multibyte_keys. Widening this test's inputs is
    // the way to catch the regression once that is fixed.
    #[test]
    fn test_matches_ternary_search_trie() {
        let words = [
            "she",
            "sells",
            "sea",
            "shells",
            "by",
            "the",
            "seashore",
            "a",
            "app",
            "apple",
            "application",
            "apply",
            "sea",
            "zebra",
        ];
        let queries = [
            "",
            "a",
            "ap",
            "app",
            "apply",
            "applying",
            "s",
            "se",
            "sea",
            "seashores",
            "sh",
            "shellsort",
            "z",
            "zzz",
            "nomatch",
        ];
        let to_delete = ["app", "sea", "zebra", "absent", "appl"];

        assert_eq!(
            snapshot::<PatriciaTrie<i32>>(&words, &queries, &to_delete),
            snapshot::<TernarySearchTrie<i32>>(&words, &queries, &to_delete),
            "PatriciaTrie and TernarySearchTrie disagree through the traits"
        );
    }
}
