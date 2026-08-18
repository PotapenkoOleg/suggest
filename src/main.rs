use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use std::string::ToString;
use std::sync::{Arc, RwLock};
use actix_web::http::header::ACCESS_CONTROL_ALLOW_ORIGIN;
use actix_web::{App, HttpResponse, HttpServer, web};
use serde::Serialize;
use tries::{PrefixSearch, SharedTrieMap, SymbolTable};
use utoipa::{IntoParams, OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

type Tries = SharedTrieMap<u32>;

const DEFAULT_SCOPE: &str = "default";

macro_rules! root_path { () => { "/api/v1/" } }

const SUGGEST_PATH: &str = concat!(root_path!(), "suggest");

const DEFAULT_LIMIT: usize = 10;

const MAX_LIMIT: usize = 100;

/// Supplies the words a trie is seeded with, borrowed from wherever the
/// implementation holds them.
trait TrieLoader {
    fn load(&self) -> impl Iterator<Item = &str>;

    /// Names every scope the implementation can supply words for. Takes no
    /// `self` because the caller needs the list before it has anything to load
    /// from: the scopes decide which loaders to open, not the other way round.
    fn get_scopes() -> Vec<String>;
}

/// Loads a word per line from a file.
struct FileTrieLoader {
    contents: String,
}

impl FileTrieLoader {
    /// Reads the file. The I/O happens here rather than in `load` because
    /// `load` is synchronous and hands out borrows of what was read.
    async fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let contents = tokio::fs::read_to_string(path).await?;
        Ok(FileTrieLoader { contents })
    }
}

impl TrieLoader for FileTrieLoader {
    fn load(&self) -> impl Iterator<Item = &str> {
        // Borrowed from the one buffer read above, so no per-word allocation:
        // `trim` reslices rather than copying. Indentation and trailing spaces
        // would otherwise become part of the key and break prefix search
        self.contents
            .lines()
            .map(str::trim)
            .filter(|word| !word.is_empty())
    }

    /// One scope per word file in the data directory, named after the file.
    ///
    /// Only `.txt` files count, and the extension is matched exactly, since
    /// the caller reads the file back as `{scope}.txt`: a directory, a stray
    /// `.DS_Store`, or a `.TXT` would otherwise become a scope whose file
    /// cannot be opened on a case-sensitive filesystem, and the service would
    /// refuse to start over it. Blocking I/O is fine here - this runs once,
    /// before the server binds.
    fn get_scopes() -> Vec<String> {
        // An unreadable directory is no scopes, which `build_tries` turns into
        // the same startup failure as a directory without the default scope
        let Ok(entries) = std::fs::read_dir(DATA_DIR) else {
            return Vec::new();
        };

        entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file() && path.extension().and_then(OsStr::to_str) == Some("txt")
            })
            // Stems as they are on disk, so the file can be found again; the
            // casing is dropped where the scope is registered
            .filter_map(|path| Some(path.file_stem()?.to_str()?.to_string()))
            .collect()
    }
}

/// One word file per scope, resolved against the working directory.
const DATA_DIR: &str = "src/data";

/// One scope's spellings: each lower-cased key its trie stores, mapped back to
/// the word as the file spells it, so a case-insensitive search can still
/// answer with `TradeDate` rather than `tradedate`.
type ScopeWords = Arc<RwLock<HashMap<String, String>>>;

/// The spellings of every scope, keyed the way [`Tries`] keys its tries.
///
/// Scoped rather than shared: two scopes may spell the same word differently,
/// and each has to answer with its own. Locking follows [`SharedTrieMap`] -
/// the registry lock is held only long enough to clone a handle out of it, so
/// one scope's lookups do not block another's.
///
/// Every method takes `&self`, so one instance serves every worker thread
/// behind a single [`web::Data`] rather than being cloned per request; each
/// one handles a poisoned lock by panicking, the way the tries do.
#[derive(Default)]
struct OriginalWords {
    scopes: RwLock<HashMap<String, ScopeWords>>,
}

impl OriginalWords {
    fn new() -> Self {
        Self::default()
    }

    /// Returns the spellings of `scope`, registering an empty set first if the
    /// scope has none yet.
    fn get_or_create(&self, scope: &str) -> ScopeWords {
        Arc::clone(
            self.scopes
                .write()
                .expect("word map lock poisoned")
                .entry(scope.to_string())
                .or_default(),
        )
    }

    /// Returns the spellings of `scope`, or `None` if the scope has none.
    fn get(&self, scope: &str) -> Option<ScopeWords> {
        self.scopes
            .read()
            .expect("word map lock poisoned")
            .get(scope)
            .map(Arc::clone)
    }

    /// Replaces every key with the word `scope` recorded for it. A key with no
    /// spelling recorded - or a scope with none at all - stands in for itself,
    /// so a suggestion is never dropped for want of an entry.
    fn spell(&self, scope: &str, keys: Vec<String>) -> Vec<String> {
        let Some(spellings) = self.get(scope) else {
            return keys;
        };

        // Locked once for the whole answer rather than once per key
        let spellings = spellings.read().expect("word map lock poisoned");

        keys.into_iter()
            .map(|key| spellings.get(&key).cloned().unwrap_or(key))
            .collect()
    }
}

/// Builds the tries alongside the spellings their keys were lower-cased from.
///
/// Fails rather than starting without the default scope, since a service that
/// answers every query with nothing looks healthy while being useless.
async fn build_tries() -> std::io::Result<(Tries, OriginalWords)> {
    let tries = Tries::new();
    let words = OriginalWords::new();

    for scope in FileTrieLoader::get_scopes() {
        let loader = FileTrieLoader::open(Path::new(DATA_DIR).join(format!("{scope}.txt"))).await?;

        // Lower-cased so a request spells the scope the same way whatever the
        // casing of the file happens to be. One key, registered in both: the
        // words of a scope are looked up under the name its trie has
        let scope = scope.to_lowercase();
        let handle = tries.get_or_create(&scope);
        let spellings = words.get_or_create(&scope);
        {
            // No `.await` inside: the write guards are released before the
            // caller can suspend
            let mut trie = handle.write().unwrap();
            let mut spellings = spellings.write().unwrap();
            for (rank, word) in loader.load().enumerate() {
                // Keyed lower-cased so a prefix matches whatever the caller
                // typed; the spelling from the file is what the answer carries
                let key = word.to_lowercase();
                spellings.insert(key.clone(), word.to_string());
                trie.put(key, rank as u32);
            }
        }
    }

    // Covers a missing directory, an empty one, and a data directory that has
    // word files but not the one every scope-less request falls back to
    if tries.get(DEFAULT_SCOPE).is_none() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no {DEFAULT_SCOPE} scope in {DATA_DIR}"),
        ));
    }

    Ok((tries, words))
}

#[derive(OpenApi)]
#[openapi(paths(index, suggest), components(schemas(SuggestResponse)))]
struct ApiDoc;

#[utoipa::path(
    get,
    path = "/",
    responses((status = 200, description = "OK"))
)]
async fn index() -> HttpResponse {
    HttpResponse::Ok().finish()
}

#[derive(serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct SuggestQuery {
    q: String,
    scope: Option<String>,
    limit: Option<usize>,
}

#[derive(Serialize, ToSchema)]
struct SuggestResponse {
    scope: String,
    query: String,
    suggestions: Vec<String>,
}

/// Reorders the candidates a prefix search turned up, since the trie yields
/// them lexicographically rather than best-first.
trait Ranker {
    fn rank(&self, candidates: Vec<String>) -> Vec<String>;
}

struct DefaultRanker;

impl Ranker for DefaultRanker {
    fn rank(&self, candidates: Vec<String>) -> Vec<String> {
        candidates
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/suggest",
    params(SuggestQuery),
    responses(
        (status = 200, description = "Suggestions starting with the prefix", body = SuggestResponse),
        (status = 404, description = "Invalid scope"),
        (status = 500, description = "Server error"),
    )
)]
async fn suggest(
    tries: web::Data<Tries>,
    words: web::Data<OriginalWords>,
    query: web::Query<SuggestQuery>,
) -> HttpResponse {

    let scope = query.scope.as_deref().unwrap_or(DEFAULT_SCOPE);

    if !tries.names().contains(&scope.to_string()) {
        // Also on the error path: without the header a browser cannot read the
        // response at all, so the caller sees a CORS failure instead of a 404
        return HttpResponse::NotFound()
            .insert_header((ACCESS_CONTROL_ALLOW_ORIGIN, "*"))
            .body("Unknown scope");
    }

    let trie= tries.get(scope).unwrap();

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);

    // Lower-cased to match the keys, which the tries are seeded with in that
    // form, so the answer does not depend on how the caller capitalised
    let prefix = query.q.to_lowercase();

    let suggestions: Vec<String> = trie
        .read()
        .unwrap()
        .get_keys_with_prefix(&prefix) // We're using direct search for small tries. We need a cache in tries for large searches
        .into_iter()
        .take(limit)
        .collect();

    let ranker: &dyn Ranker = &DefaultRanker;
    let suggestions = ranker.rank(suggestions);

    // Ranked as keys, answered as words: the lower-cased key is what the trie
    // holds, the file's spelling is what the caller wants to display. Taken
    // from the scope that was searched, since another may spell it differently
    let suggestions = words.spell(scope, suggestions);

    // Open to any origin: the endpoint is read-only, unauthenticated, and sends
    // no cookies, so there is no per-caller state for another site to reach
    HttpResponse::Ok()
        .insert_header((ACCESS_CONTROL_ALLOW_ORIGIN, "*"))
        .json(SuggestResponse {
            scope: scope.to_string(),
            query: query.q.clone(),
            suggestions,
        })
}

fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/", web::get().to(index))
        .route(SUGGEST_PATH, web::get().to(suggest));
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let (tries, words) = build_tries().await?;
    let tries = web::Data::new(tries);
    let words = web::Data::new(words);

    HttpServer::new(move || {
        App::new()
            .app_data(tries.clone())
            .app_data(words.clone())
            .configure(configure)
            .service(
                SwaggerUi::new("/swagger-ui/{_:.*}")
                    .url("/api-docs/openapi.json", ApiDoc::openapi()),
            )
    })
    .bind(("0.0.0.0", 8030))?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let (tries, words) = build_tries().await.unwrap();

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
        let (_, words) = build_tries().await.unwrap();

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
        let (tries, _) = build_tries().await.unwrap();
        let names = tries.names();

        for scope in FileTrieLoader::get_scopes() {
            let scope = scope.to_lowercase();
            assert!(names.contains(&scope), "no {scope} scope in {names:?}");
        }

        // Words from the file, not just the scope itself
        let sea = tries.get("sea").unwrap();
        assert!(!sea.read().unwrap().get_keys_with_prefix("s").is_empty());
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
    fn test_tries() -> (Tries, OriginalWords) {
        let tries = Tries::new();
        let words = OriginalWords::new();

        // Seeded in memory rather than from `DATA_DIR`, so the endpoint tests
        // assert on words they control - but registered in both maps, the way
        // `build_tries` does it
        let seed = |scope: &str, seeds: &[&str]| {
            let handle = tries.get_or_create(scope);
            let spellings = words.get_or_create(scope);
            let mut trie = handle.write().unwrap();
            let mut spellings = spellings.write().unwrap();
            for (rank, word) in seeds.iter().enumerate() {
                let key = word.to_lowercase();
                spellings.insert(key.clone(), word.to_string());
                trie.put(key, rank as u32);
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

        (tries, words)
    }

    fn test_app_data() -> (web::Data<Tries>, web::Data<OriginalWords>) {
        let (tries, words) = test_tries();

        (web::Data::new(tries), web::Data::new(words))
    }

    async fn get(uri: &str) -> (StatusCode, String) {
        let (tries, words) = test_app_data();
        let app = actix_test::init_service(
            App::new()
                .app_data(tries)
                .app_data(words)
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
        let (tries, words) = test_app_data();
        let app = actix_test::init_service(
            App::new()
                .app_data(tries)
                .app_data(words)
                .configure(configure),
        )
        .await;

        // Both the hit and the miss, since a browser needs the header to read
        // either one
        for uri in ["/api/v1/suggest?q=s", "/api/v1/suggest?q=s&scope=nope"] {
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
