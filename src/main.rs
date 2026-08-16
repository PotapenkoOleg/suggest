use actix_web::{App, HttpResponse, HttpServer, web};
use serde::Serialize;
use tries::{PrefixSearch, SharedTrieMap, SymbolTable};
use utoipa::{IntoParams, OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

/// Every trie the service can serve suggestions from, keyed by name. The value
/// stored against each word is its rank, lowest first.
type Tries = SharedTrieMap<u32>;

/// The trie used by requests that name no other one.
const DEFAULT_TRIE: &str = "default";

/// How many suggestions a request gets when it asks for no particular number.
const DEFAULT_LIMIT: usize = 10;

const MAX_LIMIT: usize = 100;

/// Builds the shared map and seeds the default trie.
fn build_tries() -> Tries {
    let tries = Tries::new();

    let default = tries.get_or_create(DEFAULT_TRIE);
    {
        let mut trie = default.write().unwrap();
        for (rank, word) in ["sea", "shell", "shore"].iter().enumerate() {
            trie.put(word.to_string(), rank as u32);
        }
    }

    tries
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
    /// Prefix to complete. An empty prefix matches every word.
    q: String,
    /// Most suggestions to return. Defaults to [`DEFAULT_LIMIT`].
    limit: Option<usize>,
}

#[derive(Serialize, ToSchema)]
struct SuggestResponse {
    /// The prefix that was searched for.
    query: String,
    /// Matching words, in lexicographic order.
    suggestions: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/suggest",
    params(SuggestQuery),
    responses(
        (status = 200, description = "Words in the default trie starting with the prefix", body = SuggestResponse),
        (status = 500, description = "The default trie is not registered"),
    )
)]
async fn suggest(tries: web::Data<Tries>, query: web::Query<SuggestQuery>) -> HttpResponse {
    let Some(trie) = tries.get(DEFAULT_TRIE) else {
        // Seeded at startup, so its absence is a server-side fault rather than
        // a query that simply matched nothing
        return HttpResponse::InternalServerError().body("no default trie");
    };

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    // The read guard is taken and dropped inside this statement: never held
    // across an `.await`, and never blocking a writer for longer than the scan
    let suggestions: Vec<String> = trie
        .read()
        .unwrap()
        .get_keys_with_prefix(&query.q)
        .into_iter()
        .take(limit)
        .collect();

    HttpResponse::Ok().json(SuggestResponse {
        query: query.q.clone(),
        suggestions,
    })
}

/// Registers the routes. Shared with the tests so they exercise the real ones.
fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/", web::get().to(index))
        .route("/suggest", web::get().to(suggest));
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Built before the workers start, not inside the closure below:
    // `HttpServer::new` runs that closure once per worker thread, so building
    // the map there would give every worker its own. `web::Data` puts it behind
    // an `Arc` that the workers share.
    let tries = web::Data::new(build_tries());

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

    #[test]
    fn default_trie_is_seeded() {
        let tries = build_tries();

        assert_eq!(tries.names(), [DEFAULT_TRIE]);

        let default = tries.get(DEFAULT_TRIE).unwrap();
        let trie = default.read().unwrap();

        assert_eq!(trie.get_all_keys(), ["sea", "shell", "shore"]);
        assert_eq!(trie.get_keys_with_prefix("sh"), ["shell", "shore"]);
        assert_eq!(trie.get("sea"), Some(0));
    }

    async fn get(uri: &str) -> (StatusCode, String) {
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(build_tries()))
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
        let (status, body) = get("/suggest?q=sh").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, r#"{"query":"sh","suggestions":["shell","shore"]}"#);
    }

    #[actix_web::test]
    async fn suggest_returns_every_word_for_an_empty_prefix() {
        let (status, body) = get("/suggest?q=").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            r#"{"query":"","suggestions":["sea","shell","shore"]}"#
        );
    }

    #[actix_web::test]
    async fn suggest_matches_nothing_outside_the_trie() {
        let (status, body) = get("/suggest?q=zebra").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, r#"{"query":"zebra","suggestions":[]}"#);
    }

    #[actix_web::test]
    async fn suggest_honours_the_limit() {
        let (status, body) = get("/suggest?q=s&limit=1").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, r#"{"query":"s","suggestions":["sea"]}"#);
    }

    #[actix_web::test]
    async fn suggest_rejects_a_missing_query() {
        let (status, _) = get("/suggest").await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
