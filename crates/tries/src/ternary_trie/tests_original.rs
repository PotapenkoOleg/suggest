#[cfg(test)]
mod tests_original {
    use crate::{PrefixSearch, SymbolTable, TernarySearchTrie};
    use std::collections::HashMap;

    fn build_trie() -> TernarySearchTrie<i32> {
        let mut symbol_table = TernarySearchTrie::new();
        symbol_table.put("she".to_string(), 0);
        symbol_table.put("sells".to_string(), 1);
        symbol_table.put("sea".to_string(), 2);
        symbol_table.put("shells".to_string(), 3);
        symbol_table.put("by".to_string(), 4);
        symbol_table.put("the".to_string(), 5);
        symbol_table.put("sea".to_string(), 6);
        symbol_table.put("shore".to_string(), 7);
        symbol_table.put("a".to_string(), 8);
        symbol_table
    }

    #[test]
    fn put() {
        let symbol_table = build_trie();
        assert_eq!(symbol_table.get("a"), Some(8));
        assert_eq!(symbol_table.get("by"), Some(4));
        assert_eq!(symbol_table.get("sea"), Some(6));
        assert_eq!(symbol_table.get("sells"), Some(1));
        assert_eq!(symbol_table.get("she"), Some(0));
        assert_eq!(symbol_table.get("shells"), Some(3));
        assert_eq!(symbol_table.get("shore"), Some(7));
        assert_eq!(symbol_table.get("the"), Some(5));
    }

    #[test]
    fn get() {
        let symbol_table = build_trie();
        assert_eq!(symbol_table.get("a"), Some(8));
        assert_eq!(symbol_table.get("by"), Some(4));
        assert_eq!(symbol_table.get("sea"), Some(6));
        assert_eq!(symbol_table.get("sells"), Some(1));
        assert_eq!(symbol_table.get("she"), Some(0));
        assert_eq!(symbol_table.get("shells"), Some(3));
        assert_eq!(symbol_table.get("shore"), Some(7));
        assert_eq!(symbol_table.get("the"), Some(5));
        assert_eq!(symbol_table.get("invalid"), None);
    }

    #[test]
    fn delete() {
        let mut symbol_table = build_trie();
        symbol_table.delete("a");
        assert_eq!(symbol_table.get("a"), None);

        symbol_table.delete("by");
        assert_eq!(symbol_table.get("by"), None);

        symbol_table.delete("shore");
        assert_eq!(symbol_table.get("shore"), None);

        assert_eq!(symbol_table.get("shells"), Some(3));
        assert_eq!(symbol_table.get("sea"), Some(6));

        symbol_table.delete("sea");
        assert_eq!(symbol_table.get("sells"), Some(1));

        symbol_table.delete("by");
        assert_eq!(symbol_table.get("by"), None);
    }

    #[test]
    fn clear() {
        let mut symbol_table = build_trie();
        assert!(!symbol_table.is_empty());
        symbol_table.clear();
        assert!(symbol_table.is_empty());
    }

    #[test]
    fn is_empty() {
        let mut symbol_table = build_trie();
        assert!(!symbol_table.is_empty());
        symbol_table.clear();
        assert!(symbol_table.is_empty());
        symbol_table.put("placeholder".to_string(), 42);
        assert!(!symbol_table.is_empty());
    }

    #[test]
    fn get_size() {
        let mut symbol_table = build_trie();
        assert_eq!(symbol_table.get_size(), 8);
        symbol_table.delete("by");
        assert_eq!(symbol_table.get_size(), 7);
        symbol_table.clear();
        assert_eq!(symbol_table.get_size(), 0);
    }

    #[test]
    fn get_all_keys() {
        let symbol_table = build_trie();
        let mut map = HashMap::new();
        map.insert("she".to_string(), 0);
        map.insert("sells".to_string(), 0);
        map.insert("shells".to_string(), 0);
        map.insert("by".to_string(), 0);
        map.insert("the".to_string(), 0);
        map.insert("sea".to_string(), 0);
        map.insert("shore".to_string(), 0);
        map.insert("a".to_string(), 0);

        let all_keys = symbol_table.get_all_keys();
        assert_eq!(check_keys(all_keys, &map), 8);
    }

    #[test]
    fn get_keys_with_prefix() {
        let symbol_table = build_trie();
        let mut map = HashMap::new();
        map.insert("she".to_string(), 0);
        map.insert("sells".to_string(), 0);
        map.insert("shells".to_string(), 0);
        map.insert("sea".to_string(), 0);
        map.insert("shore".to_string(), 0);

        let prefix = "s";
        let all_keys = symbol_table.get_keys_with_prefix(prefix);
        assert_eq!(check_keys(all_keys, &map), 5);

        map.clear();
        map.insert("she".to_string(), 0);
        map.insert("shells".to_string(), 0);
        map.insert("shore".to_string(), 0);

        let prefix = "sh";
        let all_keys = symbol_table.get_keys_with_prefix(prefix);
        assert_eq!(check_keys(all_keys, &map), 3);

        let prefix = "Invalid";
        let all_keys = symbol_table.get_keys_with_prefix(prefix);
        assert!(all_keys.is_empty());
    }

    #[test]
    fn longest_prefix_of() {
        let mut symbol_table = build_trie();
        assert_eq!(
            symbol_table.longest_prefix_of("shellsort"),
            Some("shells".to_string())
        );
        assert_eq!(symbol_table.longest_prefix_of("a"), Some("a".to_string()));
        assert_eq!(symbol_table.longest_prefix_of("Invalid"), None);

        symbol_table.clear();
        symbol_table.put("128".to_string(), 0);
        symbol_table.put("128.112.055".to_string(), 0);
        symbol_table.put("128.112.055.015".to_string(), 0);
        symbol_table.put("128.112.136".to_string(), 0);
        symbol_table.put("128.112.155.011".to_string(), 0);
        symbol_table.put("128.112.155.013".to_string(), 0);
        symbol_table.put("128.112".to_string(), 0);
        symbol_table.put("128.222".to_string(), 0);
        symbol_table.put("128.222.136".to_string(), 0);

        assert_eq!(
            symbol_table.longest_prefix_of("128.112.136.011"),
            Some("128.112.136".to_string())
        );
        assert_eq!(
            symbol_table.longest_prefix_of("128.112.100.016"),
            Some("128.112".to_string())
        );
        assert_eq!(
            symbol_table.longest_prefix_of("128.166.123.045"),
            Some("128".to_string())
        );

        symbol_table.clear();
        symbol_table.put("a".to_string(), 0);
        assert_eq!(symbol_table.longest_prefix_of("a"), Some("a".to_string()));
    }

    fn check_keys(all_keys: Vec<String>, map: &HashMap<String, i32>) -> usize {
        let mut number_of_items = 0;
        for key in all_keys {
            number_of_items += 1;
            if map.get(key.as_str()).is_none() {
                panic!("Invalid key");
            }
        }
        number_of_items
    }
}
