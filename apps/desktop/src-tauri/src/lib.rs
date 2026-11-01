// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chat_core::import::{import, ImportOptions, ImportReport, Progress};
use chat_core::library::{LibraryStats, SEARCH_INDEX_DIR};
use chat_core::query::{self, HitView, MessageView, ParticipantInfo, ThreadSummary, Window};
use chat_core::search::deep::{deep_search, sort_hits, DeepHit, DeepProgress};
use chat_core::search::{Filters, IndexProgress, SearchIndex, SearchQuery, SortOrder};
use chat_core::Library;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

const IMPORT_PROGRESS: &str = "import://progress";
const IMPORT_FINISHED: &str = "import://finished";
const INDEX_PROGRESS: &str = "index://progress";
const INDEX_DONE: &str = "index://done";
const DEEP_PROGRESS: &str = "deep://progress";
const DEEP_DONE: &str = "deep://done";
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const MAX_PAGE: usize = 200;

struct AppState {
    root: PathBuf,
    library: Mutex<Library>,
    search: Arc<SearchIndex>,
    /// Set while the background indexer runs.
    indexing: Arc<AtomicBool>,
    /// Cancel flag of the running import, if any.
    import: Mutex<Option<Arc<AtomicBool>>>,
    /// Cancel flag of the running deep search, if any.
    deep: Mutex<Option<Arc<AtomicBool>>>,
    /// Bumped under `deep_hits`'s lock so a cancelled scan cannot append after the next one clears it.
    deep_gen: Arc<AtomicU64>,
    /// Matches of the latest deep search, kept so pages can be read in any order.
    deep_hits: Arc<Mutex<Vec<DeepHit>>>,
}

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Throttles progress events to one per [`PROGRESS_INTERVAL`].
struct Throttle(Mutex<Instant>);

impl Throttle {
    fn new() -> Self {
        Self(Mutex::new(Instant::now() - PROGRESS_INTERVAL))
    }
    fn ready(&self, last_one: bool) -> bool {
        let mut t = self.0.lock().expect("throttle lock");
        if last_one || t.elapsed() >= PROGRESS_INTERVAL {
            *t = Instant::now();
            true
        } else {
            false
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryInfo {
    root: String,
    stats: LibraryStats,
}

#[derive(Deserialize)]
#[serde(tag = "kind", content = "sortKey", rename_all = "camelCase")]
enum WindowArg {
    Latest,
    Before(i64),
    After(i64),
    Around(i64),
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportFinished {
    report: Option<ImportReport>,
    error: Option<String>,
    cancelled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchResponse {
    total: usize,
    hits: Vec<HitView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IndexStatus {
    pending: usize,
    indexing: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeepProgressEvent {
    scanned: usize,
    total: usize,
    matches: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeepDone {
    matches: usize,
    cancelled: bool,
    error: Option<String>,
}

#[tauri::command]
async fn library_info(state: State<'_, AppState>) -> CmdResult<LibraryInfo> {
    let lib = state.library.lock().map_err(err)?;
    Ok(LibraryInfo {
        root: lib.root().display().to_string(),
        stats: lib.stats().map_err(err)?,
    })
}

#[tauri::command]
async fn list_threads(state: State<'_, AppState>) -> CmdResult<Vec<ThreadSummary>> {
    let lib = state.library.lock().map_err(err)?;
    query::threads(lib.conn()).map_err(err)
}

#[tauri::command]
async fn load_messages(
    state: State<'_, AppState>,
    thread_id: i64,
    window: WindowArg,
    limit: usize,
) -> CmdResult<Vec<MessageView>> {
    let window = match window {
        WindowArg::Latest => Window::Latest,
        WindowArg::Before(k) => Window::Before(k),
        WindowArg::After(k) => Window::After(k),
        WindowArg::Around(k) => Window::Around(k),
    };
    let lib = state.library.lock().map_err(err)?;
    query::messages(lib.conn(), thread_id, window, limit.min(2000)).map_err(err)
}

#[tauri::command]
async fn set_alias(
    state: State<'_, AppState>,
    participant_id: i64,
    alias: Option<String>,
) -> CmdResult<Vec<ParticipantInfo>> {
    let lib = state.library.lock().map_err(err)?;
    query::set_alias(lib.conn(), participant_id, alias.as_deref()).map_err(err)?;
    let thread: i64 = lib
        .conn()
        .query_row(
            "SELECT thread_id FROM participants WHERE id = ?1",
            [participant_id],
            |r| r.get(0),
        )
        .map_err(err)?;
    query::participants(lib.conn(), thread).map_err(err)
}

#[tauri::command]
async fn search(state: State<'_, AppState>, query: SearchQuery) -> CmdResult<SearchResponse> {
    let query = SearchQuery {
        limit: query.limit.clamp(1, MAX_PAGE),
        ..query
    };
    let results = state.search.search(&query).map_err(err)?;
    let lib = state.library.lock().map_err(err)?;
    Ok(SearchResponse {
        total: results.total,
        hits: query::messages_by_ids(lib.conn(), &results.message_ids).map_err(err)?,
    })
}

/// A page of the latest deep search's matches (found so far, if still running).
#[tauri::command]
async fn deep_page(
    state: State<'_, AppState>,
    order: SortOrder,
    offset: usize,
    limit: usize,
) -> CmdResult<SearchResponse> {
    let (total, ids) = {
        let mut hits = state.deep_hits.lock().map_err(err)?.clone();
        sort_hits(&mut hits, order);
        let ids: Vec<i64> = hits
            .iter()
            .skip(offset)
            .take(limit.clamp(1, MAX_PAGE))
            .map(|h| h.message_id)
            .collect();
        (hits.len(), ids)
    };
    let lib = state.library.lock().map_err(err)?;
    Ok(SearchResponse {
        total,
        hits: query::messages_by_ids(lib.conn(), &ids).map_err(err)?,
    })
}

#[tauri::command]
async fn index_status(state: State<'_, AppState>) -> CmdResult<IndexStatus> {
    let lib = state.library.lock().map_err(err)?;
    Ok(IndexStatus {
        pending: SearchIndex::pending(lib.conn()).map_err(err)?,
        indexing: state.indexing.load(Ordering::Relaxed),
    })
}

/// Scans every message in the background, collecting matches for [`deep_page`].
/// Progress arrives as events; a new deep search cancels the previous one.
#[tauri::command]
fn start_deep_search(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    filters: Filters,
) -> CmdResult<()> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut running = state.deep.lock().map_err(err)?;
        if let Some(previous) = running.replace(cancel.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
    }
    let root = state.root.clone();
    let store = state.deep_hits.clone();
    let gens = state.deep_gen.clone();
    let my_gen = {
        let mut hits = store.lock().map_err(err)?;
        let gen = gens.fetch_add(1, Ordering::AcqRel) + 1;
        hits.clear();
        gen
    };
    std::thread::spawn(move || {
        let throttle = Throttle::new();
        let result = Library::open(&root).and_then(|lib| {
            deep_search(
                lib.conn(),
                &text,
                &filters,
                None,
                Some(&cancel),
                &mut |hits| {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(mut all) = store.lock() {
                        if gens.load(Ordering::Acquire) == my_gen {
                            all.extend(hits);
                        }
                    }
                },
                &mut |p: &DeepProgress| {
                    if throttle.ready(p.scanned == p.total) {
                        let _ = app.emit(
                            DEEP_PROGRESS,
                            DeepProgressEvent {
                                scanned: p.scanned,
                                total: p.total,
                                matches: p.matches,
                            },
                        );
                    }
                },
            )
        });
        let done = match result {
            Ok(matches) => DeepDone {
                matches,
                cancelled: false,
                error: None,
            },
            Err(chat_core::Error::Cancelled) => DeepDone {
                matches: 0,
                cancelled: true,
                error: None,
            },
            Err(e) => DeepDone {
                matches: 0,
                cancelled: false,
                error: Some(e.to_string()),
            },
        };
        if let Ok(mut running) = app.state::<AppState>().deep.lock() {
            if running.as_ref().is_some_and(|c| Arc::ptr_eq(c, &cancel)) {
                *running = None;
            }
        }
        let _ = app.emit(DEEP_DONE, done);
    });
    Ok(())
}

#[tauri::command]
fn cancel_deep_search(state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(flag) = state.deep.lock().map_err(err)?.as_ref() {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// Indexes queued messages on a background thread unless the indexer already runs.
fn spawn_indexing(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.indexing.swap(true, Ordering::AcqRel) {
        return;
    }
    let (root, search, flag, app) = (
        state.root.clone(),
        state.search.clone(),
        state.indexing.clone(),
        app.clone(),
    );
    std::thread::spawn(move || {
        let throttle = Throttle::new();
        let result = Library::open(&root).and_then(|lib| {
            search.sync(lib.conn(), None, &mut |p: IndexProgress| {
                if throttle.ready(p.done == p.total) {
                    let _ = app.emit(INDEX_PROGRESS, p);
                }
            })
        });
        flag.store(false, Ordering::Release);
        let _ = app.emit(INDEX_DONE, result.err().map(|e| e.to_string()));
    });
}

/// Starts importing `paths` (ZIP files and folders) on a background thread. Progress
/// and the final report arrive as events; the UI keeps reading the library meanwhile
/// through its own connection. New messages are indexed for search afterwards.
#[tauri::command]
fn start_import(app: AppHandle, state: State<'_, AppState>, paths: Vec<String>) -> CmdResult<()> {
    if paths.is_empty() {
        return Err("Nothing to import.".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut running = state.import.lock().map_err(err)?;
        if running.is_some() {
            return Err("An import is already running.".into());
        }
        *running = Some(cancel.clone());
    }
    let root = state.root.clone();
    let inputs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();

    std::thread::spawn(move || {
        let throttle = Throttle::new();
        let progress = |p: &Progress| {
            if throttle.ready(p.pages_done == p.pages_total) {
                let _ = app.emit(IMPORT_PROGRESS, p);
            }
        };
        let opts = ImportOptions {
            cancel: Some(&cancel),
            progress: Some(&progress),
        };
        let result = Library::open(&root).and_then(|mut lib| import(&mut lib, &inputs, &opts));
        let finished = match result {
            Ok(report) => ImportFinished {
                report: Some(report),
                error: None,
                cancelled: false,
            },
            Err(chat_core::Error::Cancelled) => ImportFinished {
                report: None,
                error: None,
                cancelled: true,
            },
            Err(e) => ImportFinished {
                report: None,
                error: Some(e.to_string()),
                cancelled: false,
            },
        };
        if let Ok(mut running) = app.state::<AppState>().import.lock() {
            *running = None;
        }
        let _ = app.emit(IMPORT_FINISHED, finished);
        spawn_indexing(&app);
    });
    Ok(())
}

#[tauri::command]
fn cancel_import(state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(flag) = state.import.lock().map_err(err)?.as_ref() {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // ICV_LIBRARY_DIR points development runs at a scratch library.
            let root = match std::env::var_os("ICV_LIBRARY_DIR") {
                Some(dir) => PathBuf::from(dir),
                None => app.path().app_data_dir()?.join("library"),
            };
            let library = Library::open(&root)?;
            let search = SearchIndex::open(&root.join(SEARCH_INDEX_DIR), library.conn())?;
            app.asset_protocol_scope()
                .allow_directory(root.join(chat_core::library::MEDIA_DIR), true)?;
            app.manage(AppState {
                root,
                library: Mutex::new(library),
                search: Arc::new(search),
                indexing: Arc::new(AtomicBool::new(false)),
                import: Mutex::new(None),
                deep: Mutex::new(None),
                deep_gen: Arc::new(AtomicU64::new(0)),
                deep_hits: Arc::new(Mutex::new(Vec::new())),
            });
            spawn_indexing(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            library_info,
            list_threads,
            load_messages,
            set_alias,
            search,
            deep_page,
            index_status,
            start_deep_search,
            cancel_deep_search,
            start_import,
            cancel_import
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
