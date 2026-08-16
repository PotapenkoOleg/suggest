#[cfg(test)]
mod tests {
    use crate::{PatriciaTrie, PrefixSearch, SharedTrieMap, SymbolTable};
    use std::sync::{Arc, Barrier};
    use std::thread;

    // The whole point of the type: it has to cross thread boundaries and be
    // usable from several at once. A compile failure here is the real assertion.
    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_is_send_and_sync() {
        assert_send_sync::<SharedTrieMap<u32>>();
        assert_send_sync::<SharedTrieMap<String>>();
        assert_send_sync::<Arc<SharedTrieMap<u32>>>();
    }

    #[test]
    fn test_basic_operations() {
        let map = SharedTrieMap::<i32>::new();

        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
        assert!(map.get("en").is_none());
        assert!(!map.contains("en"));

        map.get_or_create("en")
            .write()
            .unwrap()
            .put("sea".to_string(), 2);
        map.get_or_create("fr")
            .write()
            .unwrap()
            .put("mer".to_string(), 3);

        assert!(!map.is_empty());
        assert_eq!(map.len(), 2);
        assert!(map.contains("en"));
        assert_eq!(map.names(), ["en", "fr"]);
        assert_eq!(map.get("en").unwrap().read().unwrap().get("sea"), Some(2));
        assert_eq!(map.get("fr").unwrap().read().unwrap().get("sea"), None);

        map.clear();
        assert!(map.is_empty());
        assert_eq!(map.names(), Vec::<String>::new());
    }

    #[test]
    fn test_get_or_create_returns_the_same_trie() {
        let map = SharedTrieMap::<i32>::new();

        let first = map.get_or_create("en");
        let second = map.get_or_create("en");

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(map.len(), 1);

        // A write through one handle is visible through the other
        first.write().unwrap().put("sea".to_string(), 2);
        assert_eq!(second.read().unwrap().get("sea"), Some(2));
    }

    #[test]
    fn test_insert_and_remove() {
        let map = SharedTrieMap::<i32>::new();

        let mut trie = PatriciaTrie::new();
        trie.put("sea".to_string(), 2);
        assert!(map.insert("en".to_string(), trie).is_none());
        assert_eq!(map.get("en").unwrap().read().unwrap().get("sea"), Some(2));

        // Replacing hands back the old trie, which its holders keep using
        let mut replacement = PatriciaTrie::new();
        replacement.put("shore".to_string(), 7);
        let displaced = map.insert("en".to_string(), replacement).unwrap();
        assert_eq!(displaced.read().unwrap().get("sea"), Some(2));
        assert_eq!(map.get("en").unwrap().read().unwrap().get("sea"), None);
        assert_eq!(map.len(), 1);

        let removed = map.remove("en").unwrap();
        assert_eq!(removed.read().unwrap().get("shore"), Some(7));
        assert!(map.is_empty());
        assert!(map.remove("en").is_none());
    }

    #[test]
    fn test_concurrent_writes_to_one_trie() {
        const THREADS: usize = 8;
        const PER_THREAD: usize = 100;

        let map = SharedTrieMap::<usize>::new();

        thread::scope(|scope| {
            for t in 0..THREADS {
                let map = &map;
                scope.spawn(move || {
                    for i in 0..PER_THREAD {
                        let trie = map.get_or_create("words");
                        trie.write()
                            .unwrap()
                            .put(format!("key-{t}-{i}"), t * PER_THREAD + i);
                    }
                });
            }
        });

        let trie = map.get("words").unwrap();
        let trie = trie.read().unwrap();

        assert_eq!(map.len(), 1);
        assert_eq!(trie.get_size(), THREADS * PER_THREAD);
        for t in 0..THREADS {
            for i in 0..PER_THREAD {
                assert_eq!(trie.get(&format!("key-{t}-{i}")), Some(t * PER_THREAD + i));
            }
        }
    }

    #[test]
    fn test_concurrent_get_or_create_of_one_name() {
        const THREADS: usize = 16;

        let map = SharedTrieMap::<i32>::new();
        // Release every thread at once so they genuinely race on the entry
        let barrier = Barrier::new(THREADS);

        let handles: Vec<_> = thread::scope(|scope| {
            let spawned: Vec<_> = (0..THREADS)
                .map(|_| {
                    let map = &map;
                    let barrier = &barrier;
                    scope.spawn(move || {
                        barrier.wait();
                        map.get_or_create("contended")
                    })
                })
                .collect();

            spawned.into_iter().map(|h| h.join().unwrap()).collect()
        });

        // Exactly one trie was created, and every racer got that one
        assert_eq!(map.len(), 1);
        for handle in &handles {
            assert!(Arc::ptr_eq(handle, &handles[0]));
        }
    }

    #[test]
    fn test_concurrent_readers_and_writers_across_tries() {
        const NAMES: [&str; 4] = ["en", "fr", "de", "es"];
        const WORDS: [&str; 4] = ["sea", "sells", "shells", "shore"];

        let map = SharedTrieMap::<usize>::new();

        thread::scope(|scope| {
            // Writers: one per trie, each filling its own
            for name in NAMES {
                let map = &map;
                scope.spawn(move || {
                    let trie = map.get_or_create(name);
                    for (i, word) in WORDS.iter().enumerate() {
                        trie.write().unwrap().put(word.to_string(), i);
                    }
                });
            }

            // Readers: querying names that may not exist yet, which must be a
            // miss rather than a hang or a panic
            for _ in 0..NAMES.len() {
                let map = &map;
                scope.spawn(move || {
                    for _ in 0..50 {
                        for name in NAMES {
                            if let Some(trie) = map.get(name) {
                                let keys = trie.read().unwrap().get_keys_with_prefix("s");
                                assert!(keys.len() <= WORDS.len());
                            }
                        }
                        map.names();
                    }
                });
            }
        });

        assert_eq!(map.names(), ["de", "en", "es", "fr"]);
        for name in NAMES {
            let trie = map.get(name).unwrap();
            let trie = trie.read().unwrap();
            assert_eq!(trie.get_size(), WORDS.len());
            assert_eq!(
                trie.get_keys_with_prefix("s"),
                ["sea", "sells", "shells", "shore"]
            );
        }
    }

    #[test]
    fn test_handles_outlive_removal() {
        let map = SharedTrieMap::<i32>::new();
        let trie = map.get_or_create("en");
        trie.write().unwrap().put("sea".to_string(), 2);

        map.remove("en");
        assert!(map.get("en").is_none());

        // The removed trie is still usable through the handle held here
        trie.write().unwrap().put("shore".to_string(), 7);
        assert_eq!(trie.read().unwrap().get_all_keys(), ["sea", "shore"]);

        // ...and a new `get_or_create` starts from an empty one
        assert!(map.get_or_create("en").read().unwrap().is_empty());
    }

    #[test]
    fn test_shared_across_threads_behind_arc() {
        let map = Arc::new(SharedTrieMap::<i32>::new());

        let writer = {
            let map = Arc::clone(&map);
            thread::spawn(move || {
                map.get_or_create("en")
                    .write()
                    .unwrap()
                    .put("sea".to_string(), 2);
            })
        };
        writer.join().unwrap();

        assert_eq!(map.get("en").unwrap().read().unwrap().get("sea"), Some(2));
    }
}
