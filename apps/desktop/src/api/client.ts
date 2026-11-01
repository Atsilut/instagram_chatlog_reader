// SPDX-License-Identifier: AGPL-3.0-or-later

import type { LibraryInfo } from "./types";

/**
 * Everything the UI needs from the backend. The desktop app implements it over
 * Tauri IPC; the planned Docker/browser mode will implement it over HTTP.
 */
export interface ApiClient {
  libraryInfo(): Promise<LibraryInfo>;
}
