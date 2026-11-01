// SPDX-License-Identifier: AGPL-3.0-or-later

// Prevents an additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    instagram_chat_viewer_lib::run()
}
