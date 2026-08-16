use std::path::Path;
use std::string::ToString;
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
}

/// Words for the default scope, resolved against the working directory.
const DEFAULT_SCOPE_FILE: &str = "src/default.txt";

/// Fails rather than starting with an empty default scope, since a service that
/// answers every query with nothing looks healthy while being useless.
async fn build_tries() -> std::io::Result<Tries> {
    let tries = Tries::new();
    let loader = FileTrieLoader::open(DEFAULT_SCOPE_FILE).await?;

    let default = tries.get_or_create(DEFAULT_SCOPE);
    {
        // No `.await` inside: the write guard is released before the caller
        // can suspend
        let mut trie = default.write().unwrap();
        for (rank, word) in loader.load().enumerate() {
            trie.put(word.to_string(), rank as u32);
        }
    }

    Ok(tries)
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
async fn suggest(tries: web::Data<Tries>, query: web::Query<SuggestQuery>) -> HttpResponse {

    let scope = query.scope.as_deref().unwrap_or(DEFAULT_SCOPE);

    if !tries.names().contains(&scope.to_string()) {
        return HttpResponse::NotFound().body("Unknown scope");
    }

    let trie= tries.get(scope).unwrap();

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);

    let suggestions: Vec<String> = trie
        .read()
        .unwrap()
        .get_keys_with_prefix(&query.q) // We're using direct search for small tries. We need a cache in tries for large searches
        .into_iter()
        .take(limit)
        .collect();

    let ranker: &dyn Ranker = &DefaultRanker;
    let suggestions = ranker.rank(suggestions);

    HttpResponse::Ok().json(SuggestResponse {
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
    let tries = web::Data::new(build_tries().await?);

    HttpServer::new(move || {
        App::new()
            .app_data(tries.clone())
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

    // Reads the real word file; `cargo test` runs with the package root as the
    // working directory, the same place the binary expects it. Asserted by
    // property rather than by content, so editing the word list cannot break it
    #[actix_web::test]
    async fn default_trie_is_seeded_from_the_word_file() {
        let tries = build_tries().await.unwrap();

        assert_eq!(tries.names(), [DEFAULT_SCOPE]);

        let default = tries.get(DEFAULT_SCOPE).unwrap();
        let trie = default.read().unwrap();
        let keys = trie.get_all_keys();

        assert!(!keys.is_empty());
        // Padding in the file must not survive into the keys, or no prefix
        // query would ever match
        assert!(keys.iter().all(|key| key.trim() == key));
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

    // The running service seeds only the default trie, so the scopes a request
    // can select are registered here instead.
    fn test_tries() -> Tries {
        let tries = Tries::new();

        // Seeded in memory rather than from `DEFAULT_SCOPE_FILE`, so the
        // endpoint tests assert on words they control
        let default = tries.get_or_create(DEFAULT_SCOPE);
        {
            let mut trie = default.write().unwrap();
            for (rank, word) in ["sea", "shell", "shore"].iter().enumerate() {
                trie.put(word.to_string(), rank as u32);
            }
        }

        let fruit = tries.get_or_create("fruit");
        {
            let mut trie = fruit.write().unwrap();
            for (rank, word) in ["apple", "apricot", "shallot"].iter().enumerate() {
                trie.put(word.to_string(), rank as u32);
            }
        }

        // More words than `MAX_LIMIT`, so a request cannot ask for them all
        let bulk = tries.get_or_create("bulk");
        {
            let mut trie = bulk.write().unwrap();
            for i in 0..(MAX_LIMIT * 2) {
                trie.put(format!("word-{i:04}"), i as u32);
            }
        }

        tries
    }

    async fn get(uri: &str) -> (StatusCode, String) {
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(test_tries()))
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
    async fn suggest_rejects_a_missing_query() {
        let (status, _) = get("/api/v1/suggest").await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
