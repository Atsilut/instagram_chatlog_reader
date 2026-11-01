// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chat_core::import::{import, ImportOptions, ImportReport, Progress};
use chat_core::library::LibraryStats;
use chat_core::query::{self, MessageView, ParticipantInfo, ThreadSummary, Window};
use chat_core::Library;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

const PROGRESS_EVENT: &str = "import://progress";
const FINISHED_EVENT: &str = "import://finished";
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

struct AppState {
    root: PathBuf,
    library: Mutex<Library>,
    /// Cancel flag of the running import, if any.
    import: Mutex<Option<Arc<AtomicBool>>>,
}

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
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

/// Starts importing `paths` (ZIP files and folders) on a background thread. Progress
/// and the final report arrive as events; the UI keeps reading the library meanwhile
/// through its own connection.
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
        let last = Mutex::new(Instant::now() - PROGRESS_INTERVAL);
        let progress = |p: &Progress| {
            let mut last = last.lock().expect("progress lock");
            if last.elapsed() >= PROGRESS_INTERVAL || p.pages_done == p.pages_total {
                *last = Instant::now();
                let _ = app.emit(PROGRESS_EVENT, p);
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
        let _ = app.emit(FINISHED_EVENT, finished);
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
            let root = app.path().app_data_dir()?.join("library");
            let library = Library::open(&root)?;
            app.manage(AppState {
                root,
                library: Mutex::new(library),
                import: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            library_info,
            list_threads,
            load_messages,
            set_alias,
            start_import,
            cancel_import
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
