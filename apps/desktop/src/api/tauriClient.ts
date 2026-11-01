// SPDX-License-Identifier: AGPL-3.0-or-later

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import type { ApiClient, Unlisten } from "./client";
import type {
  DeepDone,
  DeepProgress,
  HitView,
  ImportFinished,
  ImportProgress,
  IndexProgress,
  IndexStatus,
  LibraryInfo,
  MessageView,
  MessageWindow,
  ParticipantInfo,
  SearchFilters,
  SearchQuery,
  SearchResponse,
  ThreadSummary,
} from "./types";

export class TauriClient implements ApiClient {
  private root = "";

  async libraryInfo(): Promise<LibraryInfo> {
    const info = await invoke<LibraryInfo>("library_info");
    this.root = info.root;
    return info;
  }

  listThreads(): Promise<ThreadSummary[]> {
    return invoke("list_threads");
  }

  loadMessages(threadId: number, window: MessageWindow, limit: number): Promise<MessageView[]> {
    return invoke("load_messages", { threadId, window, limit });
  }

  setAlias(participantId: number, alias: string | null): Promise<ParticipantInfo[]> {
    return invoke("set_alias", { participantId, alias });
  }

  async pickImportSources(kind: "zip" | "folder"): Promise<string[]> {
    const picked =
      kind === "zip"
        ? await open({ multiple: true, filters: [{ name: "Instagram export", extensions: ["zip"] }] })
        : await open({ directory: true, multiple: true });
    if (!picked) return [];
    return Array.isArray(picked) ? picked : [picked];
  }

  startImport(paths: string[]): Promise<void> {
    return invoke("start_import", { paths });
  }

  cancelImport(): Promise<void> {
    return invoke("cancel_import");
  }

  onImportProgress(cb: (p: ImportProgress) => void): Promise<Unlisten> {
    return listen<ImportProgress>("import://progress", (e) => cb(e.payload));
  }

  onImportFinished(cb: (f: ImportFinished) => void): Promise<Unlisten> {
    return listen<ImportFinished>("import://finished", (e) => cb(e.payload));
  }

  onDropPaths(cb: (paths: string[]) => void): Promise<Unlisten> {
    return getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "drop" && e.payload.paths.length > 0) cb(e.payload.paths);
    });
  }

  search(query: SearchQuery): Promise<SearchResponse> {
    return invoke("search", { query });
  }

  indexStatus(): Promise<IndexStatus> {
    return invoke("index_status");
  }

  onIndexProgress(cb: (p: IndexProgress) => void): Promise<Unlisten> {
    return listen<IndexProgress>("index://progress", (e) => cb(e.payload));
  }

  onIndexDone(cb: () => void): Promise<Unlisten> {
    return listen("index://done", () => cb());
  }

  startDeepSearch(text: string, filters: SearchFilters): Promise<void> {
    return invoke("start_deep_search", { text, filters });
  }

  cancelDeepSearch(): Promise<void> {
    return invoke("cancel_deep_search");
  }

  onDeepHits(cb: (ids: number[]) => void): Promise<Unlisten> {
    return listen<number[]>("deep://hits", (e) => cb(e.payload));
  }

  onDeepProgress(cb: (p: DeepProgress) => void): Promise<Unlisten> {
    return listen<DeepProgress>("deep://progress", (e) => cb(e.payload));
  }

  onDeepDone(cb: (d: DeepDone) => void): Promise<Unlisten> {
    return listen<DeepDone>("deep://done", (e) => cb(e.payload));
  }

  loadHits(ids: number[]): Promise<HitView[]> {
    return invoke("load_hits", { ids });
  }

  mediaUrl(storePath: string): string {
    return convertFileSrc(this.absolute(storePath));
  }

  openMedia(storePath: string): Promise<void> {
    return openPath(this.absolute(storePath));
  }

  openUrl(url: string): Promise<void> {
    return openUrl(url);
  }

  private absolute(storePath: string): string {
    const sep = this.root.includes("\\") ? "\\" : "/";
    return `${this.root}${sep}${storePath.split("/").join(sep)}`;
  }
}
