#[cfg(test)]
mod tests {
    use crate::{PrefixSearch, SymbolTable, TernarySearchTrie};

    #[test]
    fn test_basic_operations() {
        let mut tst = TernarySearchTrie::<String>::new();

        // Test is_empty and get_size
        assert!(tst.is_empty());
        assert_eq!(tst.get_size(), 0);

        // Test put and get
        tst.put("hello".to_string(), "world".to_string());
        tst.put("help".to_string(), "me".to_string());
        tst.put("hell".to_string(), "yeah".to_string());

        assert_eq!(tst.get("hello"), Some("world".to_string()));
        assert_eq!(tst.get("help"), Some("me".to_string()));
        assert_eq!(tst.get("hell"), Some("yeah".to_string()));
        assert_eq!(tst.get("he"), None);
        assert_eq!(tst.get("helps"), None);

        // Test contains
        assert!(tst.contains("hello"));
        assert!(!tst.contains("helloworld"));

        // Test size
        assert_eq!(tst.get_size(), 3);
        assert!(!tst.is_empty());

        // Test overwriting existing keys
        tst.put("hello".to_string(), "universe".to_string());
        assert_eq!(tst.get("hello"), Some("universe".to_string()));
        assert_eq!(tst.get_size(), 3); // Size should not change

        // Test delete
        tst.delete("hello");
        assert_eq!(tst.get("hello"), None);
        assert_eq!(tst.get_size(), 2);
        assert!(tst.contains("help"));
        assert!(tst.contains("hell"));

        // Test clear
        tst.clear();
        assert!(tst.is_empty());
        assert_eq!(tst.get_size(), 0);
        assert_eq!(tst.get("help"), None);
    }

    #[test]
    fn test_prefix_operations() {
        let mut tst = TernarySearchTrie::<i32>::new();

        tst.put("shell".to_string(), 1);
        tst.put("shore".to_string(), 2);
        tst.put("shellfish".to_string(), 3);
        tst.put("shelve".to_string(), 4);
        tst.put("shelter".to_string(), 5);
        tst.put("hello".to_string(), 6);

        // Test get_all_keys
        let all_keys = tst.get_all_keys();
        assert_eq!(all_keys.len(), 6);
        assert!(all_keys.contains(&"shell".to_string()));
        assert!(all_keys.contains(&"shore".to_string()));
        assert!(all_keys.contains(&"shellfish".to_string()));
        assert!(all_keys.contains(&"shelve".to_string()));
        assert!(all_keys.contains(&"shelter".to_string()));
        assert!(all_keys.contains(&"hello".to_string()));

        // Test get_keys_with_prefix
        let shell_prefixed = tst.get_keys_with_prefix("shell");
        assert_eq!(shell_prefixed.len(), 2);
        assert!(shell_prefixed.contains(&"shell".to_string()));
        assert!(shell_prefixed.contains(&"shellfish".to_string()));

        let sh_prefixed = tst.get_keys_with_prefix("sh");
        assert_eq!(sh_prefixed.len(), 5);
        assert!(!sh_prefixed.contains(&"hello".to_string()));

        let empty_prefix = tst.get_keys_with_prefix("");
        assert_eq!(empty_prefix.len(), 6);

        let nonexistent_prefix = tst.get_keys_with_prefix("abc");
        assert_eq!(nonexistent_prefix.len(), 0);

        // Test longest_prefix_of
        assert_eq!(
            tst.longest_prefix_of("shellfish soup"),
            Some("shellfish".to_string())
        );
        assert_eq!(
            tst.longest_prefix_of("shell shocked"),
            Some("shell".to_string())
        );
        assert_eq!(tst.longest_prefix_of("she sells"), None); // No key exactly "she"
        assert_eq!(tst.longest_prefix_of("abc"), None);
        assert_eq!(tst.longest_prefix_of(""), None);
    }

    #[test]
    fn test_edge_cases() {
        let mut tst = TernarySearchTrie::<String>::new();

        // Test empty key (should be ignored)
        tst.put("".to_string(), "empty".to_string());
        assert_eq!(tst.get(""), None);
        assert_eq!(tst.get_size(), 0);

        // Test very long key
        let long_key = "a".repeat(1000);
        tst.put(long_key.clone(), "long".to_string());
        assert_eq!(tst.get(&long_key), Some("long".to_string()));

        // Test keys with special characters
        tst.put("!@#$%^&*()".to_string(), "special".to_string());
        tst.put("русский текст".to_string(), "russian".to_string());
        tst.put("中文文本".to_string(), "chinese".to_string());

        assert_eq!(tst.get("!@#$%^&*()"), Some("special".to_string()));
        assert_eq!(tst.get("русский текст"), Some("russian".to_string()));
        assert_eq!(tst.get("中文文本"), Some("chinese".to_string()));

        assert_eq!(tst.get_size(), 4);

        // Test deleting the root
        tst = TernarySearchTrie::<String>::new();
        tst.put("a".to_string(), "value".to_string());
        assert_eq!(tst.get_size(), 1);
        tst.delete("a");
        assert_eq!(tst.get_size(), 0);
        assert!(tst.is_empty());
    }

    #[test]
    fn test_structure_correctness() {
        let mut tst = TernarySearchTrie::<i32>::new();

        // Insert keys that test the structure of the trie
        tst.put("cat".to_string(), 1);
        tst.put("dog".to_string(), 2);
        tst.put("bat".to_string(), 3);
        tst.put("bar".to_string(), 4);
        tst.put("cab".to_string(), 5);

        // Verify all keys exist
        assert_eq!(tst.get("cat"), Some(1));
        assert_eq!(tst.get("dog"), Some(2));
        assert_eq!(tst.get("bat"), Some(3));
        assert_eq!(tst.get("bar"), Some(4));
        assert_eq!(tst.get("cab"), Some(5));

        // "b" comes before "c" and "d" > "c"
        // Testing left/right links are correct
        assert_eq!(tst.get_keys_with_prefix("b").len(), 2);
        assert_eq!(tst.get_keys_with_prefix("c").len(), 2);
        assert_eq!(tst.get_keys_with_prefix("d").len(), 1);

        // Delete a key with a common prefix
        tst.delete("cat");
        assert_eq!(tst.get("cat"), None);
        assert_eq!(tst.get("cab"), Some(5));

        // Delete a key with a unique first character
        tst.delete("dog");
        assert_eq!(tst.get("dog"), None);

        assert_eq!(tst.get_size(), 3);
    }
}
