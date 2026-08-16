#[cfg(test)]
mod integration_tests {
    use crate::{PrefixSearch, SymbolTable, TernarySearchTrie};

    // Define a struct for testing with a custom type
    #[derive(Clone, Debug, PartialEq)]
    struct User {
        id: u32,
        name: String,
    }

    #[test]
    fn test_dictionary_operations() {
        let mut dictionary = TernarySearchTrie::<String>::new();

        let words = vec![
            ("apple", "A fruit that grows on trees"),
            ("application", "A software program"),
            ("apply", "To put something into practice"),
            ("banana", "A yellow curved fruit"),
            ("band", "A group of musicians"),
            ("cat", "A small domesticated carnivorous mammal"),
            ("category", "A class or division of people or things"),
        ];

        for (word, definition) in words {
            dictionary.put(word.to_string(), definition.to_string());
        }

        assert_eq!(dictionary.get_size(), 7);

        assert_eq!(
            dictionary.get("apple"),
            Some("A fruit that grows on trees".to_string())
        );
        assert_eq!(
            dictionary.get("band"),
            Some("A group of musicians".to_string())
        );

        let suggestions = dictionary.get_keys_with_prefix("app");
        assert_eq!(suggestions.len(), 3);
        assert!(suggestions.contains(&"apple".to_string()));
        assert!(suggestions.contains(&"application".to_string()));
        assert!(suggestions.contains(&"apply".to_string()));

        dictionary.delete("apple");
        assert_eq!(dictionary.get("apple"), None);
        let suggestions_after = dictionary.get_keys_with_prefix("app");
        assert_eq!(suggestions_after.len(), 2);
    }

    #[test]
    fn test_custom_type_storage() {
        let mut user_db = TernarySearchTrie::<User>::new();

        user_db.put(
            "john.doe".to_string(),
            User {
                id: 1,
                name: "John Doe".to_string(),
            },
        );
        user_db.put(
            "jane.smith".to_string(),
            User {
                id: 2,
                name: "Jane Smith".to_string(),
            },
        );
        user_db.put(
            "bob.johnson".to_string(),
            User {
                id: 3,
                name: "Bob Johnson".to_string(),
            },
        );

        let john = user_db.get("john.doe").unwrap();
        assert_eq!(john.id, 1);
        assert_eq!(john.name, "John Doe");

        let j_users = user_db.get_keys_with_prefix("j");
        assert_eq!(j_users.len(), 2);

        user_db.put(
            "john.doe".to_string(),
            User {
                id: 1,
                name: "John Updated".to_string(),
            },
        );
        let john_updated = user_db.get("john.doe").unwrap();
        assert_eq!(john_updated.name, "John Updated");
    }

    #[test]
    fn test_prefix_matching_usecase() {
        let mut router = TernarySearchTrie::<String>::new();

        router.put("/api/users".to_string(), "GET_USERS".to_string());
        router.put("/api/users/:id".to_string(), "GET_USER_BY_ID".to_string());
        router.put("/api/posts".to_string(), "GET_POSTS".to_string());
        router.put("/api/posts/:id".to_string(), "GET_POST_BY_ID".to_string());
        router.put(
            "/api/posts/:id/comments".to_string(),
            "GET_POST_COMMENTS".to_string(),
        );

        assert_eq!(router.get("/api/users"), Some("GET_USERS".to_string()));

        let api_routes = router.get_keys_with_prefix("/api");
        assert_eq!(api_routes.len(), 5);

        let post_routes = router.get_keys_with_prefix("/api/posts");
        assert_eq!(post_routes.len(), 3);

        let incoming_route = "/api/posts/123/comments/456";
        let matched_route = router.longest_prefix_of(incoming_route);
        assert_eq!(matched_route, Some("/api/posts".to_string()));
    }

    #[test]
    fn test_performance_with_large_dataset() {
        let mut tst = TernarySearchTrie::<usize>::new();

        const NUM_ITEMS: usize = 1000;

        for i in 0..NUM_ITEMS {
            let key = format!("key_{:05}", i);
            tst.put(key, i);
        }

        assert_eq!(tst.get_size(), NUM_ITEMS);

        for i in 0..NUM_ITEMS {
            let key = format!("key_{:05}", i);
            assert_eq!(tst.get(&key), Some(i));
        }

        let prefix_matches = tst.get_keys_with_prefix("key_000");
        assert_eq!(prefix_matches.len(), 100); // key_00000 to key_00099

        for i in (0..NUM_ITEMS).step_by(2) {
            let key = format!("key_{:05}", i);
            tst.delete(&key);
        }

        assert_eq!(tst.get_size(), NUM_ITEMS / 2);

        for i in (1..NUM_ITEMS).step_by(2) {
            let key = format!("key_{:05}", i);
            assert_eq!(tst.get(&key), Some(i));
        }
    }
}
