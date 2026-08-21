#[cfg(test)]
mod tests {
    use crate::*;
    // Aliased: a plain `use actix_web::test` would shadow the `#[test]`
    // attribute with actix's async one
    use actix_web::http::StatusCode;
    use actix_web::test as actix_test;

    // Reads the real data directory; `cargo test` runs with the package root
    // as the working directory, the same place the binary expects it. Asserted
    // by property rather than by content, so editing the word files cannot
    // break it
    #[actix_web::test]
    async fn default_trie_is_seeded_from_the_word_file() {
        let (tries, words, _) = build_tries().await.unwrap();

        assert!(tries.names().contains(&DEFAULT_SCOPE.to_string()));

        let default = tries.get(DEFAULT_SCOPE).unwrap();
        let trie = default.read().unwrap();
        let keys = trie.get_all_keys();

        assert!(!keys.is_empty());
        // Padding in the file must not survive into the keys, or no prefix
        // query would ever match
        assert!(keys.iter().all(|key| key.trim() == key));
        // Every key is searchable in lower case and answerable in the casing
        // the file used
        assert!(keys.iter().all(|key| key.to_lowercase() == *key));

        let spellings = words.spell(DEFAULT_SCOPE, keys.clone());
        assert!(
            spellings
                .iter()
                .zip(&keys)
                .all(|(spelling, key)| spelling.to_lowercase() == *key)
        );
    }

    // The file has mixed-case words, so this is the case the map exists for
    #[actix_web::test]
    async fn original_words_keep_the_casing_of_the_word_file() {
        let (_, words, _) = build_tries().await.unwrap();

        assert_eq!(
            words.spell(DEFAULT_SCOPE, vec!["tradedate".to_string()]),
            ["TradeDate"]
        );
    }

    #[test]
    fn original_words_answer_within_their_scope() {
        let words = OriginalWords::new();

        words
            .get_or_create("columns")
            .write()
            .unwrap()
            .insert("tradedate".to_string(), "TradeDate".to_string());
        words
            .get_or_create("shouting")
            .write()
            .unwrap()
            .insert("tradedate".to_string(), "TRADEDATE".to_string());

        let key = || vec!["tradedate".to_string()];

        // The same key, spelled the way each scope recorded it
        assert_eq!(words.spell("columns", key()), ["TradeDate"]);
        assert_eq!(words.spell("shouting", key()), ["TRADEDATE"]);

        // A scope that recorded nothing, and a key it never saw, answer with
        // what they were given rather than dropping it
        assert_eq!(words.spell("unknown", key()), ["tradedate"]);
        assert_eq!(
            words.spell("columns", vec!["missing".to_string()]),
            ["missing"]
        );
    }

    // Pins the two files the crate ships with rather than the whole list, so
    // adding a word file does not break it
    #[test]
    fn file_trie_loader_names_a_scope_per_word_file() {
        let scopes = FileTrieLoader::get_scopes();

        for expected in [DEFAULT_SCOPE, "sea"] {
            assert!(
                scopes.iter().any(|scope| scope.to_lowercase() == expected),
                "no {expected} scope in {scopes:?}"
            );
        }

        // The extension is not part of the name, or no request could name the
        // scope without knowing how the words are stored
        assert!(scopes.iter().all(|scope| !scope.ends_with(".txt")));
    }

    // Every word file, not only the default one, ends up queryable
    #[actix_web::test]
    async fn each_word_file_becomes_a_scope() {
        let (tries, _, _) = build_tries().await.unwrap();
        let names = tries.names();

        for scope in FileTrieLoader::get_scopes() {
            let scope = scope.to_lowercase();
            assert!(names.contains(&scope), "no {scope} scope in {names:?}");
        }

        // Words from the file, not just the scope itself
        let sea = tries.get("sea").unwrap();
        assert!(!sea.read().unwrap().get_keys_with_prefix("s").is_empty());
    }

    // A word is filed under itself and under every string it becomes when
    // characters are dropped, which is what lets a misspelling meet it
    #[actix_web::test]
    async fn sym_spell_lookup_files_a_word_under_its_deletes() {
        let (tries, _, variants) = build_tries().await.unwrap();

        // The first word of the default file, so its rank is pinned whatever
        // else the file holds
        let id = ("Id".to_string(), 0);

        // Keyed by the lower-cased spelling, the form a query is dropped from
        for variant in ["id", "i", "d", ""] {
            assert!(
                variants.candidates(DEFAULT_SCOPE, variant).contains(&id),
                "Id not filed under {variant:?}"
            );
        }

        // The rank stored is the one the trie was seeded with, so a candidate
        // can be ordered without a second lookup
        let default = tries.get(DEFAULT_SCOPE).unwrap();
        assert_eq!(default.read().unwrap().get("id"), Some(id.1));
    }

    // Two scopes may hold different words, and a variant of one is no answer
    // for the other
    #[actix_web::test]
    async fn sym_spell_lookup_answers_within_its_scope() {
        let (_, _, variants) = build_tries().await.unwrap();

        let seagull = ("seagull".to_string(), 1);

        assert!(variants.candidates("sea", "segull").contains(&seagull));
        assert!(
            !variants
                .candidates(DEFAULT_SCOPE, "segull")
                .contains(&seagull)
        );

        // A scope that was never built stands in for an empty one rather than
        // panicking, the way the word map does
        assert!(variants.candidates("nope", "sea").is_empty());
    }

    // The default file lists `InvRating` twice, so the index has to agree with
    // the trie about which of the two ranks the word has
    #[actix_web::test]
    async fn sym_spell_lookup_files_a_repeated_word_once() {
        let (tries, _, variants) = build_tries().await.unwrap();

        let key = "invrating";
        let candidates = variants.candidates(DEFAULT_SCOPE, key);
        let filed: Vec<_> = candidates
            .iter()
            .filter(|(word, _)| word.to_lowercase() == key)
            .collect();

        assert_eq!(filed.len(), 1, "{key} filed more than once: {candidates:?}");

        let default = tries.get(DEFAULT_SCOPE).unwrap();
        assert_eq!(default.read().unwrap().get(key), Some(filed[0].1));
    }

    // Nothing is filed further out than the index reaches: that bound is what
    // keeps the map to a size the process can hold
    #[actix_web::test]
    async fn sym_spell_lookup_reaches_exactly_the_max_distance() {
        let (_, _, variants) = build_tries().await.unwrap();

        let deletions = variants.get(DEFAULT_SCOPE).unwrap();
        let deletions = deletions.read().unwrap();
        assert!(!deletions.is_empty());

        for (variant, candidates) in deletions.iter() {
            for (word, _) in candidates {
                let key = word.to_lowercase();
                assert!(
                    deletes(&key, MAX_EDIT_DISTANCE).contains(variant),
                    "{word} filed under {variant:?}, which is more than \
                     {MAX_EDIT_DISTANCE} deletes away"
                );
            }
        }
    }

    #[test]
    fn default_ranker_keeps_the_order() {
        let candidates = vec!["shell".to_string(), "sea".to_string()];

        // Also pins that the trait stays dyn-compatible
        let ranker: &dyn Ranker = &DefaultRanker;

        assert_eq!(ranker.rank(candidates.clone()), candidates);
    }

    // Proves the signature can actually be implemented: the returned iterator
    // borrows from the loader, which is the part a `&str` return has to get right
    #[test]
    fn trie_loader_yields_borrowed_words() {
        struct Words(Vec<String>);

        impl TrieLoader for Words {
            fn load(&self) -> impl Iterator<Item = &str> {
                self.0.iter().map(String::as_str)
            }

            fn get_scopes() -> Vec<String> {
                vec!["words".to_string()]
            }
        }

        let words = Words(vec!["sea".to_string(), "shore".to_string()]);

        assert_eq!(words.load().collect::<Vec<_>>(), ["sea", "shore"]);
    }

    // Takes the loader through the trait rather than the concrete type
    fn words_of(loader: &impl TrieLoader) -> Vec<&str> {
        loader.load().collect()
    }

    async fn write_temp_file(name: &str, contents: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        tokio::fs::write(&path, contents).await.unwrap();
        path
    }

    #[actix_web::test]
    async fn file_trie_loader_reads_a_word_per_line() {
        let path = write_temp_file("suggest-loader-words.txt", "sea\nshell\nshore\n").await;

        let loader = FileTrieLoader::open(&path).await.unwrap();

        assert_eq!(words_of(&loader), ["sea", "shell", "shore"]);

        tokio::fs::remove_file(&path).await.unwrap();
    }

    #[actix_web::test]
    async fn file_trie_loader_trims_padding_and_skips_blank_lines() {
        let path = write_temp_file("suggest-loader-padded.txt", "\tId  \n\n  TradeDate\n").await;

        let loader = FileTrieLoader::open(&path).await.unwrap();

        assert_eq!(words_of(&loader), ["Id", "TradeDate"]);

        tokio::fs::remove_file(&path).await.unwrap();
    }

    #[actix_web::test]
    async fn file_trie_loader_reads_an_empty_file_as_no_words() {
        let path = write_temp_file("suggest-loader-empty.txt", "").await;

        let loader = FileTrieLoader::open(&path).await.unwrap();

        assert_eq!(words_of(&loader), Vec::<&str>::new());

        tokio::fs::remove_file(&path).await.unwrap();
    }

    #[actix_web::test]
    async fn file_trie_loader_reports_a_missing_file() {
        let path = std::env::temp_dir().join("suggest-loader-does-not-exist.txt");

        // `unwrap_err` would need `FileTrieLoader: Debug`, which it has no
        // other reason to be
        let Err(error) = FileTrieLoader::open(&path).await else {
            panic!("reading a missing file should fail");
        };

        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }

    #[actix_web::test]
    async fn file_trie_loader_feeds_a_trie() {
        let path = write_temp_file("suggest-loader-trie.txt", "shore\nsea\nshell").await;

        let loader = FileTrieLoader::open(&path).await.unwrap();
        let tries = Tries::new();
        let scope = tries.get_or_create("file");
        {
            let mut trie = scope.write().unwrap();
            for (rank, word) in loader.load().enumerate() {
                trie.put(word.to_string(), rank as u32);
            }
        }

        assert_eq!(
            scope.read().unwrap().get_all_keys(),
            ["sea", "shell", "shore"]
        );

        tokio::fs::remove_file(&path).await.unwrap();
    }

    // The running service seeds the scopes from the data directory, so the
    // ones a request can select are registered here instead.
    fn test_tries() -> (Tries, OriginalWords, SymSpellLookup) {
        let tries = Tries::new();
        let words = OriginalWords::new();
        let variants = SymSpellLookup::new();

        // Seeded in memory rather than from `DATA_DIR`, so the endpoint tests
        // assert on words they control - but registered in all three maps, the
        // way `build_tries` does it
        let seed = |scope: &str, seeds: &[&str]| {
            let handle = tries.get_or_create(scope);
            let spellings = words.get_or_create(scope);
            let deletions = variants.get_or_create(scope);
            let mut trie = handle.write().unwrap();
            let mut spellings = spellings.write().unwrap();
            let mut deletions = deletions.write().unwrap();
            for (rank, word) in seeds.iter().enumerate() {
                let key = word.to_lowercase();
                let rank = rank as u32;
                for variant in deletes(&key, MAX_EDIT_DISTANCE) {
                    deletions
                        .entry(variant)
                        .or_default()
                        .push((word.to_string(), rank));
                }
                spellings.insert(key.clone(), word.to_string());
                trie.put(key, rank);
            }
        };

        seed(DEFAULT_SCOPE, &["sea", "shell", "shore"]);
        seed("fruit", &["apple", "apricot", "shallot"]);
        // Mixed case, so a response has a spelling to restore, and the same
        // word spelled differently in each so one scope cannot answer for the
        // other
        seed("columns", &["TradeDate", "TradePrice"]);
        seed("shouting", &["TRADEDATE", "TRADEPRICE"]);

        // More words than `MAX_LIMIT`, so a request cannot ask for them all.
        // Put straight into the trie, leaving the word map without an entry
        // for any of them: the response falls back to the key itself
        let bulk = tries.get_or_create("bulk");
        {
            let mut trie = bulk.write().unwrap();
            for i in 0..(MAX_LIMIT * 2) {
                trie.put(format!("word-{i:04}"), i as u32);
            }
        }

        (tries, words, variants)
    }

    fn test_app_data() -> (
        web::Data<Tries>,
        web::Data<OriginalWords>,
        web::Data<SymSpellLookup>,
    ) {
        let (tries, words, variants) = test_tries();

        (
            web::Data::new(tries),
            web::Data::new(words),
            web::Data::new(variants),
        )
    }

    async fn get(uri: &str) -> (StatusCode, String) {
        let (tries, words, variants) = test_app_data();
        let app = actix_test::init_service(
            App::new()
                .app_data(tries)
                .app_data(words)
                .app_data(variants)
                .configure(configure),
        )
        .await;

        let request = actix_test::TestRequest::get().uri(uri).to_request();
        let response = actix_test::call_service(&app, request).await;
        let status = response.status();
        let body = String::from_utf8(actix_test::read_body(response).await.to_vec()).unwrap();

        (status, body)
    }

    #[actix_web::test]
    async fn suggest_returns_matches_for_a_prefix() {
        let (status, body) = get("/api/v1/suggest?q=sh").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            r#"{"scope":"default","query":"sh","suggestions":["shell","shore"]}"#
        );
    }

    #[actix_web::test]
    async fn suggest_searches_the_requested_scope() {
        // The same prefix, answered by a different trie
        let (status, body) = get("/api/v1/suggest?q=sh&scope=fruit").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            r#"{"scope":"fruit","query":"sh","suggestions":["shallot"]}"#
        );
    }

    // The prefix is matched in lower case, but the answer carries the spelling
    // the words were seeded with
    #[actix_web::test]
    async fn suggest_answers_with_the_original_casing() {
        let (status, body) = get("/api/v1/suggest?q=trade&scope=columns").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            r#"{"scope":"columns","query":"trade","suggestions":["TradeDate","TradePrice"]}"#
        );
    }

    // Both scopes hold the same keys, so only per-scope spellings can tell the
    // two answers apart
    #[actix_web::test]
    async fn suggest_answers_with_the_casing_of_the_scope_it_searched() {
        let (_, columns) = get("/api/v1/suggest?q=trade&scope=columns").await;
        let (_, shouting) = get("/api/v1/suggest?q=trade&scope=shouting").await;

        assert_eq!(
            columns,
            r#"{"scope":"columns","query":"trade","suggestions":["TradeDate","TradePrice"]}"#
        );
        assert_eq!(
            shouting,
            r#"{"scope":"shouting","query":"trade","suggestions":["TRADEDATE","TRADEPRICE"]}"#
        );
    }

    #[actix_web::test]
    async fn suggest_ignores_the_case_of_the_query() {
        let (status, body) = get("/api/v1/suggest?q=TrAdE&scope=columns").await;

        assert_eq!(status, StatusCode::OK);
        // The query comes back as it was typed, the suggestions as they were
        // seeded
        assert_eq!(
            body,
            r#"{"scope":"columns","query":"TrAdE","suggestions":["TradeDate","TradePrice"]}"#
        );
    }

    // Keys the word map never saw stand in for themselves rather than dropping
    // out of the answer
    #[actix_web::test]
    async fn suggest_answers_with_the_key_when_no_spelling_was_recorded() {
        let (status, body) = get("/api/v1/suggest?q=word-000&scope=bulk&limit=2").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            r#"{"scope":"bulk","query":"word-000","suggestions":["word-0000","word-0001"]}"#
        );
    }

    #[actix_web::test]
    async fn suggest_falls_back_to_the_default_scope() {
        let (_, without_scope) = get("/api/v1/suggest?q=s").await;
        let (_, with_scope) = get("/api/v1/suggest?q=s&scope=default").await;

        assert_eq!(without_scope, with_scope);
    }

    #[actix_web::test]
    async fn suggest_rejects_an_unknown_scope() {
        let (status, body) = get("/api/v1/suggest?q=s&scope=nope").await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body, "Unknown scope");
    }

    #[actix_web::test]
    async fn suggest_returns_every_word_for_an_empty_prefix() {
        let (status, body) = get("/api/v1/suggest?q=").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            r#"{"scope":"default","query":"","suggestions":["sea","shell","shore"]}"#
        );
    }

    // Nothing starts with `seat`, so the query is read as a misspelling and
    // answered with what it could have been rather than with nothing
    #[actix_web::test]
    async fn suggest_corrects_a_query_no_word_starts_with() {
        let (status, body) = get("/api/v1/suggest?q=seat").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            r#"{"scope":"default","query":"seat","suggestions":["sea"]}"#
        );
    }

    // `shorl` is one edit from `shore` and two from `shell`, so the nearer
    // word comes first even though the other was seeded at a better rank
    #[actix_web::test]
    async fn suggest_orders_corrections_by_distance() {
        let (status, body) = get("/api/v1/suggest?q=shorl").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            r#"{"scope":"default","query":"shorl","suggestions":["shore","shell"]}"#
        );
    }

    // Both are two edits from `shale`, so the rank the words were loaded at is
    // what separates them
    #[actix_web::test]
    async fn suggest_orders_equally_near_corrections_by_rank() {
        let (status, body) = get("/api/v1/suggest?q=shale").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            r#"{"scope":"default","query":"shale","suggestions":["shell","shore"]}"#
        );
    }

    // Cut after the ordering, so the limit takes the nearest rather than
    // whichever correction was found first
    #[actix_web::test]
    async fn suggest_honours_the_limit_when_correcting() {
        let (status, body) = get("/api/v1/suggest?q=shale&limit=1").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            r#"{"scope":"default","query":"shale","suggestions":["shell"]}"#
        );
    }

    // A transposition, which is the edit a plain Levenshtein distance would
    // charge twice for and rank behind a word that is genuinely further away
    #[actix_web::test]
    async fn suggest_corrects_a_transposition_in_the_original_casing() {
        let (status, body) = get("/api/v1/suggest?q=tradedaet&scope=columns").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            r#"{"scope":"columns","query":"tradedaet","suggestions":["TradeDate"]}"#
        );
    }

    // A word that starts with the query is a completion, not a correction, so
    // the prefix path answers and the nearer misspelling is not offered
    #[actix_web::test]
    async fn suggest_prefers_a_prefix_match_to_a_correction() {
        let (status, body) = get("/api/v1/suggest?q=she").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            r#"{"scope":"default","query":"she","suggestions":["shell"]}"#
        );
    }

    // Sharing a variant only nominates a word - `abxy` and `pqab` both leave
    // `ab` behind, and they are four edits apart - so each is measured
    #[test]
    fn sym_spell_lookup_measures_a_candidate_before_offering_it() {
        let variants = SymSpellLookup::new();
        let deletions = variants.get_or_create("words");
        {
            let mut deletions = deletions.write().unwrap();
            for (rank, word) in ["abxy", "pqab"].iter().enumerate() {
                for variant in deletes(word, MAX_EDIT_DISTANCE) {
                    deletions
                        .entry(variant)
                        .or_default()
                        .push((word.to_string(), rank as u32));
                }
            }
        }

        // Both words are filed under `ab`, and only one survives measuring
        assert_eq!(variants.candidates("words", "ab").len(), 2);
        assert_eq!(
            variants.correct("words", "abxy", MAX_EDIT_DISTANCE),
            [("abxy".to_string(), 0)]
        );

        // A scope that was never built has nothing to correct with, the way it
        // has nothing to spell with
        assert!(
            variants
                .correct("nope", "abxy", MAX_EDIT_DISTANCE)
                .is_empty()
        );
    }

    // Correcting a query costs a variant per pair of positions in it, so a
    // caller cannot make the service do that work without bound
    #[actix_web::test]
    async fn suggest_does_not_correct_a_query_longer_than_any_word() {
        let query = "s".repeat(MAX_CORRECTED_QUERY + 1);
        let (status, body) = get(&format!("/api/v1/suggest?q={query}")).await;

        assert_eq!(status, StatusCode::OK);

        let response: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(response["suggestions"].as_array().unwrap().is_empty());
    }

    #[actix_web::test]
    async fn suggest_matches_nothing_outside_the_trie() {
        let (status, body) = get("/api/v1/suggest?q=zebra").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            r#"{"scope":"default","query":"zebra","suggestions":[]}"#
        );
    }

    #[actix_web::test]
    async fn suggest_honours_the_limit() {
        let (status, body) = get("/api/v1/suggest?q=s&limit=1").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            r#"{"scope":"default","query":"s","suggestions":["sea"]}"#
        );
    }

    #[actix_web::test]
    async fn suggest_caps_the_limit() {
        let (status, body) = get("/api/v1/suggest?q=word&scope=bulk&limit=100000").await;

        assert_eq!(status, StatusCode::OK);

        let response: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(response["suggestions"].as_array().unwrap().len(), MAX_LIMIT);
    }

    #[actix_web::test]
    async fn suggest_allows_any_origin() {
        let (tries, words, variants) = test_app_data();
        let app = actix_test::init_service(
            App::new()
                .app_data(tries)
                .app_data(words)
                .app_data(variants)
                .configure(configure),
        )
        .await;

        // The prefix hit, the correction, and the unknown scope, since a
        // browser needs the header to read any of them
        for uri in [
            "/api/v1/suggest?q=s",
            "/api/v1/suggest?q=seat",
            "/api/v1/suggest?q=s&scope=nope",
        ] {
            let request = actix_test::TestRequest::get().uri(uri).to_request();
            let response = actix_test::call_service(&app, request).await;
            let origin = response
                .headers()
                .get(ACCESS_CONTROL_ALLOW_ORIGIN)
                .map(|value| value.to_str().unwrap().to_string());

            assert_eq!(origin.as_deref(), Some("*"), "no CORS header on {uri}");
        }
    }

    #[actix_web::test]
    async fn suggest_rejects_a_missing_query() {
        let (status, _) = get("/api/v1/suggest").await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
