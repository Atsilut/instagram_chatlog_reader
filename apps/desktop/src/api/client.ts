// SPDX-License-Identifier: AGPL-3.0-or-later

import type {
  ImportFinished,
  ImportProgress,
  LibraryInfo,
  MessageView,
  MessageWindow,
  ParticipantInfo,
  ThreadSummary,
} from "./types";

export type Unlisten = () => void;

/**
 * Everything the UI needs from the backend. The desktop app implements it over
 * Tauri IPC; the planned Docker/browser mode will implement it over HTTP.
 */
export interface ApiClient {
  libraryInfo(): Promise<LibraryInfo>;
  listThreads(): Promise<ThreadSummary[]>;
  loadMessages(threadId: number, window: MessageWindow, limit: number): Promise<MessageView[]>;
  /** Returns the thread's participants after the change. */
  setAlias(participantId: number, alias: string | null): Promise<ParticipantInfo[]>;

  /** Lets the user choose ZIP files or one folder; resolves to [] when cancelled. */
  pickImportSources(kind: "zip" | "folder"): Promise<string[]>;
  startImport(paths: string[]): Promise<void>;
  cancelImport(): Promise<void>;
  onImportProgress(cb: (p: ImportProgress) => void): Promise<Unlisten>;
  onImportFinished(cb: (f: ImportFinished) => void): Promise<Unlisten>;
  /** Files or folders dropped onto the window. */
  onDropPaths(cb: (paths: string[]) => void): Promise<Unlisten>;

  /** URL for a stored media file (`storePath` is relative to the library root). */
  mediaUrl(storePath: string): string;
  openMedia(storePath: string): Promise<void>;
  openUrl(url: string): Promise<void>;
}
