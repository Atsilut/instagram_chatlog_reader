// SPDX-License-Identifier: AGPL-3.0-or-later

import { invoke } from "@tauri-apps/api/core";
import type { ApiClient } from "./client";
import type { LibraryInfo } from "./types";

export class TauriClient implements ApiClient {
  libraryInfo(): Promise<LibraryInfo> {
    return invoke<LibraryInfo>("library_info");
  }
}
