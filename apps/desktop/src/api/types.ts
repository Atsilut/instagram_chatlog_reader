// SPDX-License-Identifier: AGPL-3.0-or-later

export interface LibraryStats {
  threads: number;
  messages: number;
  media: number;
}

export interface LibraryInfo {
  root: string;
  stats: LibraryStats;
}

export interface ParticipantInfo {
  id: number;
  threadId: number;
  displayName: string;
  alias: string | null;
  isOwner: boolean;
}

export interface ThreadSummary {
  id: number;
  title: string;
  kind: "direct" | "group" | null;
  messageCount: number;
  lastSortKey: number | null;
  lastText: string | null;
  lastKind: string | null;
  lastParticipantId: number | null;
  participants: ParticipantInfo[];
  /** Left out of the main list. Still in the library, and still searchable while open. */
  hidden: boolean;
}

export interface MediaView {
  id: number;
  mime: string | null;
  storePath: string | null;
  width: number | null;
  height: number | null;
  present: boolean;
}

export type PartType = "photo" | "video" | "audio" | "gif" | "file" | "share" | "link" | "remote_gif";

export interface PartView {
  type: PartType;
  url: string | null;
  caption: string | null;
  author: string | null;
  media: MediaView | null;
}

export interface ReactionView {
  emoji: string;
  participantId: number;
}

export type MessageKind = "text" | "media" | "share" | "link" | "reaction_notice" | "like" | "empty";

export interface MessageView {
  id: number;
  sortKey: number;
  /** Wall-clock minutes since 1970 as printed in the export (no timezone). */
  tsMinute: number;
  participantId: number;
  kind: MessageKind;
  text: string | null;
  parts: PartView[];
  reactions: ReactionView[];
  missingFromLatest: boolean;
}

export type MessageWindow =
  | { kind: "latest" }
  | { kind: "before"; sortKey: number }
  | { kind: "after"; sortKey: number }
  | { kind: "around"; sortKey: number };

export interface SearchFilters {
  threadId?: number | null;
  participantId?: number | null;
  fromMinute?: number | null;
  toMinute?: number | null;
}

/** `bestMatch` is only meaningful for deep search. */
export type SearchOrder = "bestMatch" | "oldest" | "newest";

export interface SearchQuery {
  text: string;
  filters: SearchFilters;
  order: SearchOrder;
  offset: number;
  limit: number;
  /** Also match verb forms (loved, have loved) and typos of them. Plurals still match when this is false. */
  similarWords: boolean;
}

export interface HitView {
  threadId: number;
  message: MessageView;
}

export interface SearchResponse {
  total: number;
  hits: HitView[];
}

export interface IndexStatus {
  pending: number;
  indexing: boolean;
}

export interface IndexProgress {
  done: number;
  total: number;
}

export interface DeepProgress {
  scanned: number;
  total: number;
  matches: number;
}

export interface DeepDone {
  matches: number;
  cancelled: boolean;
  error: string | null;
}

export interface ImportProgress {
  pages_done: number;
  pages_total: number;
  chat: string;
}

export interface TzShift {
  chat: string;
  export: string;
  minutes: number;
  evidence: number;
}

export interface ImportReport {
  exports: number;
  threads: number;
  pages_parsed: number;
  parse_issues: number;
  days_skipped: number;
  days_merged: number;
  messages_new: number;
  messages_existing: number;
  media_files_stored: number;
  media_missing: number;
  tz_shifts: TzShift[];
  warnings: string[];
  scan: { files: number; exports: number; threads: number; pages: number; skipped_request_pages: number };
}

export interface ImportFinished {
  report: ImportReport | null;
  error: string | null;
  cancelled: boolean;
}
