use actix_web::http::header::ACCESS_CONTROL_ALLOW_ORIGIN;
use actix_web::{App, HttpResponse, HttpServer, web};
use serde::Serialize;
use spell_distance::distance;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use std::string::ToString;
use std::sync::{Arc, RwLock};
use sym_spell::deletes;
use tries::{PrefixSearch, SharedTrieMap, SymbolTable};
use utoipa::{IntoParams, OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

mod tests;

type Tries = SharedTrieMap<u32>;

const DEFAULT_SCOPE: &str = "default";

macro_rules! root_path {
    () => {
        "/api/v1/"
    };
}

const SUGGEST_PATH: &str = concat!(root_path!(), "suggest");

const DEFAULT_LIMIT: usize = 10;

const MAX_LIMIT: usize = 100;

/// The longest query worth correcting, in characters.
///
/// Dropping characters costs a variant per pair of positions, so the work a
/// correction does grows with the square of what was typed - and a caller can
/// type as much as it likes. The cut is generous rather than tight: the longest
/// word in the data is 35 characters, and nothing within two edits of a query
/// this long could be a word at all, so no correction is lost that would have
/// been found.
const MAX_CORRECTED_QUERY: usize = 128;

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

/// How many characters a query and a word may each drop before they have to
/// meet on the same string, which is the number of edits the index reaches.
const MAX_EDIT_DISTANCE: usize = 2;

/// One scope's delete index: every string a word becomes when up to
/// [`MAX_EDIT_DISTANCE`] characters are dropped from it, mapped to the words
/// that produced it with the rank each was loaded at.
///
/// A `Vec` rather than one pair, because sharing a variant is the whole point:
/// `sea` and `seat` both leave `sea` behind, and a query dropping down to it
/// has to reach both. One pair per key would keep whichever word was inserted
/// last and silently lose the rest.
type ScopeVariants = Arc<RwLock<HashMap<String, Vec<(String, u32)>>>>;

/// The delete variants of every scope, keyed the way [`Tries`] keys its tries.
///
/// Scoped for the same reason [`OriginalWords`] is: a variant only says which
/// words a misspelling could have been, and the words of one scope are not
/// answers for another. Locking follows the same scheme - the registry lock is
/// held only long enough to clone a handle out of it.
#[derive(Default)]
struct SymSpellLookup {
    scopes: RwLock<HashMap<String, ScopeVariants>>,
}

impl SymSpellLookup {
    fn new() -> Self {
        Self::default()
    }

    /// Returns the variants of `scope`, registering an empty index first if
    /// the scope has none yet.
    fn get_or_create(&self, scope: &str) -> ScopeVariants {
        Arc::clone(
            self.scopes
                .write()
                .expect("variant map lock poisoned")
                .entry(scope.to_string())
                .or_default(),
        )
    }

    /// Returns the variants of `scope`, or `None` if the scope has none.
    fn get(&self, scope: &str) -> Option<ScopeVariants> {
        self.scopes
            .read()
            .expect("variant map lock poisoned")
            .get(scope)
            .map(Arc::clone)
    }

    /// Returns the words of `scope` filed under `variant`, with the rank each
    /// was loaded at, or nothing if no word of that scope leaves it behind.
    ///
    /// Candidates rather than answers: two words meeting on a string only says
    /// they are worth measuring against each other.
    // One bucket at a time, which is what a test asserts on; the search takes
    // the whole probe through `correct` instead, under a single lock
    #[cfg_attr(not(test), allow(dead_code))]
    fn candidates(&self, scope: &str, variant: &str) -> Vec<(String, u32)> {
        self.get(scope)
            .and_then(|variants| {
                variants
                    .read()
                    .expect("variant map lock poisoned")
                    .get(variant)
                    .cloned()
            })
            .unwrap_or_default()
    }

    /// Returns the words of `scope` that `key` could have been a misspelling
    /// of, nearest first and the earliest-ranked among words equally near.
    ///
    /// `key` is dropped down to the same variants its words were filed under,
    /// so the two sides meet: a substitution by dropping the character that
    /// differs on each side, an insertion by dropping the one side has and the
    /// other has not, a transposition by dropping either of the two swapped.
    /// The variants of `key` include `key` itself, so a word spelled correctly
    /// meets its own entry.
    ///
    /// Sharing a variant only nominates a word - `sea` and `sets` both leave
    /// `se` behind - so every candidate is measured before it is offered, and
    /// what comes back is what comparing `key` to every word would have found.
    fn correct(&self, scope: &str, key: &str, max_distance: usize) -> Vec<(String, u32)> {
        let Some(variants) = self.get(scope) else {
            return Vec::new();
        };

        // Deduplicated before anything is measured: one word is filed under as
        // many variants as it has ways of losing characters, and the distance
        // is the expensive part. The rank comes along because the answer is
        // ordered by it, and is the same wherever the word was found
        let candidates: HashMap<String, u32> = {
            // Locked once for the whole probe rather than once per variant
            let variants = variants.read().expect("variant map lock poisoned");

            deletes(key, max_distance)
                .iter()
                .filter_map(|variant| variants.get(variant))
                .flatten()
                .cloned()
                .collect()
        };

        let mut corrections: Vec<(usize, u32, String)> = candidates
            .into_iter()
            .filter_map(|(word, rank)| {
                // Against the lower-cased spelling, since `key` arrives that
                // way: measuring against the file's casing would count every
                // capital as a substitution and throw the word out
                let measured = distance(key, &word.to_lowercase());

                (measured <= max_distance).then_some((measured, rank, word))
            })
            .collect();

        // Nearest first, then the earliest of the words equally near - the
        // files are written commonest first - then alphabetically, so the
        // answer does not depend on the order the map happened to yield
        corrections.sort();

        corrections
            .into_iter()
            .map(|(_, rank, word)| (word, rank))
            .collect()
    }
}

/// Builds the tries alongside the spellings their keys were lower-cased from
/// and the delete variants a misspelling is looked up by.
///
/// Fails rather than starting without the default scope, since a service that
/// answers every query with nothing looks healthy while being useless.
async fn build_tries() -> std::io::Result<(Tries, OriginalWords, SymSpellLookup)> {
    let tries = Tries::new();
    let words = OriginalWords::new();
    let variants = SymSpellLookup::new();

    for scope in FileTrieLoader::get_scopes() {
        let loader = FileTrieLoader::open(Path::new(DATA_DIR).join(format!("{scope}.txt"))).await?;

        // Lower-cased so a request spells the scope the same way whatever the
        // casing of the file happens to be. One key, registered in both: the
        // words of a scope are looked up under the name its trie has
        let scope = scope.to_lowercase();
        let handle = tries.get_or_create(&scope);
        let spellings = words.get_or_create(&scope);
        let deletions = variants.get_or_create(&scope);
        {
            // No `.await` inside: the write guards are released before the
            // caller can suspend
            let mut trie = handle.write().unwrap();
            let mut spellings = spellings.write().unwrap();
            let mut deletions = deletions.write().unwrap();
            for (rank, word) in loader.load().enumerate() {
                // Keyed lower-cased so a prefix matches whatever the caller
                // typed; the spelling from the file is what the answer carries
                let key = word.to_lowercase();
                let rank = rank as u32;

                // A file may list the same word twice - `InvRating` is in the
                // default one at two lines - and the trie and the spellings
                // both keep the later of the two. The variants have to agree:
                // appending would offer the word once per line it was listed
                // on, one of them ranked where nothing else ranks it
                let repeated = spellings.insert(key.clone(), word.to_string()).is_some();

                // Filed under the key rather than the spelling, since a query
                // is lower-cased before it is dropped down to its own variants
                // and the two sides have to meet on the same string. The word
                // is stored as the file spells it, so a candidate needs no
                // second lookup to be displayed
                for variant in deletes(&key, MAX_EDIT_DISTANCE) {
                    let candidates = deletions.entry(variant).or_default();

                    // Only a repeat pays for the search, and only against the
                    // candidates of one variant
                    let filed = repeated
                        .then(|| {
                            candidates
                                .iter_mut()
                                .find(|(candidate, _)| candidate.to_lowercase() == key)
                        })
                        .flatten();

                    match filed {
                        Some(candidate) => *candidate = (word.to_string(), rank),
                        None => candidates.push((word.to_string(), rank)),
                    }
                }

                trie.put(key, rank);
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

    Ok((tries, words, variants))
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
        (status = 200, description = "Suggestions starting with the prefix, or corrections of it if nothing does", body = SuggestResponse),
        (status = 404, description = "Invalid scope"),
        (status = 500, description = "Server error"),
    )
)]
async fn suggest(
    tries: web::Data<Tries>,
    words: web::Data<OriginalWords>,
    variants: web::Data<SymSpellLookup>,
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

    let trie = tries.get(scope).unwrap();

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

    if !suggestions.is_empty() {
        let ranker: &dyn Ranker = &DefaultRanker;
        let suggestions = ranker.rank(suggestions);

        let suggestions = words.spell(scope, suggestions);

        return HttpResponse::Ok()
            .insert_header((ACCESS_CONTROL_ALLOW_ORIGIN, "*"))
            .json(SuggestResponse {
                scope: scope.to_string(),
                query: query.q.clone(),
                suggestions,
            });
    }

    // Unhappy path: nothing in the scope starts with what was typed, so it is
    // read as a misspelling rather than as a prefix. Answered rather than
    // refused - a caller mid-word has a well-formed query that simply has no
    // completion, and 404 is what an unknown scope means here
    let suggestions = if prefix.chars().count() > MAX_CORRECTED_QUERY {
        // Bounded before the variants are built rather than after, since
        // building them is the cost
        Vec::new()
    } else {
        variants
            .correct(scope, &prefix, MAX_EDIT_DISTANCE)
            .into_iter()
            // Ordered before it is cut, or a nearer correction further down
            // the map would be the one dropped
            .take(limit)
            // Filed under the key but stored as the file spells it, so the
            // answer needs no second lookup the way the prefix path does
            .map(|(word, _)| word)
            .collect()
    };

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
    let (tries, words, variants) = build_tries().await?;
    let tries = web::Data::new(tries);
    let words = web::Data::new(words);
    let variants = web::Data::new(variants);

    HttpServer::new(move || {
        App::new()
            .app_data(tries.clone())
            .app_data(words.clone())
            .app_data(variants.clone())
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
