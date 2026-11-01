// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Mutex;

use chat_core::library::LibraryStats;
use chat_core::Library;
use tauri::{Manager, State};

struct AppState {
    library: Mutex<Library>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryInfo {
    root: String,
    stats: LibraryStats,
}

#[tauri::command]
fn library_info(state: State<'_, AppState>) -> Result<LibraryInfo, String> {
    let lib = state.library.lock().map_err(|e| e.to_string())?;
    Ok(LibraryInfo {
        root: lib.root().display().to_string(),
        stats: lib.stats().map_err(|e| e.to_string())?,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let root = app.path().app_data_dir()?.join("library");
            let library = Library::open(root)?;
            app.manage(AppState {
                library: Mutex::new(library),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![library_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
