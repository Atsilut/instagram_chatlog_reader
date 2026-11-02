// SPDX-License-Identifier: AGPL-3.0-or-later

import { useMemo, useState } from "react";
import type { ThreadSummary } from "../api";
import { formatListDate, previewText, sortKeyMinute, threadTitle } from "../lib/format";
import { useDisplaySettings } from "../lib/settings";
import { Avatar } from "./ChatView";
import { progressPercent, type ImportState } from "./ImportDialog";

interface Props {
  threads: ThreadSummary[];
  selected: number | null;
  onSelect: (id: number) => void;
  onImport: () => void;
  onSettings: () => void;
  onSetHidden: (id: number, hidden: boolean) => void;
  onDelete: (thread: ThreadSummary) => void;
  importing: ImportState | null;
}

export function Sidebar({ threads, selected, onSelect, onImport, onSettings, onSetHidden, onDelete, importing }: Props) {
  useDisplaySettings();
  const [filter, setFilter] = useState("");
  const [showHidden, setShowHidden] = useState(false);
  const [menuId, setMenuId] = useState<number | null>(null);
  const hiddenCount = threads.filter((t) => t.hidden).length;
  const shown = useMemo(() => {
    const listed = threads.filter((t) => t.hidden === showHidden);
    const q = filter.trim().toLowerCase();
    if (!q) return listed;
    return listed.filter((t) =>
      [t.title, ...t.participants.filter((p) => !p.isOwner).flatMap((p) => [p.displayName, p.alias ?? ""])].some((n) => n.toLowerCase().includes(q)),
    );
  }, [threads, filter, showHidden]);

  return (
    <aside className="flex w-80 shrink-0 flex-col border-r border-zinc-200 dark:border-zinc-800">
      <div className="flex items-center justify-between px-4 pt-4 pb-3">
        <h1 className="text-lg font-bold tracking-tight">Chats</h1>
        <div className="flex items-center gap-1.5">
        <button
          className="rounded-lg p-1.5 text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
          onClick={onSettings}
          aria-label="Settings"
          title="Language and time"
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </svg>
        </button>
        <button
          className="flex items-center gap-1.5 rounded-lg bg-indigo-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-indigo-500"
          onClick={onImport}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M12 3v12m0 0l-4-4m4 4l4-4M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2" />
          </svg>
          Import
        </button>
        </div>
      </div>
      {importing && (
        <button className="mx-4 mb-3 rounded-lg bg-indigo-50 px-3 py-2 text-left dark:bg-indigo-500/15" onClick={onImport}>
          <div className="mb-1.5 flex justify-between text-xs font-medium text-indigo-700 dark:text-indigo-300">
            <span>Importing...</span>
            <span>{progressPercent(importing) ?? 0}%</span>
          </div>
          <div className="h-1.5 overflow-hidden rounded-full bg-indigo-100 dark:bg-indigo-900/50">
            <div className="h-full rounded-full bg-indigo-600 transition-all" style={{ width: `${progressPercent(importing) ?? 0}%` }} />
          </div>
        </button>
      )}
      <div className="px-4 pb-3">
        <input
          className="w-full rounded-lg bg-zinc-100 px-3 py-2 text-sm outline-none placeholder:text-zinc-500 focus:ring-2 focus:ring-indigo-500 dark:bg-zinc-900"
          placeholder="Search users"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      </div>
      <div className="mx-4 mb-3 flex rounded-lg bg-zinc-100 p-0.5 text-sm dark:bg-zinc-900">
        <button
          className={`flex-1 rounded-md px-2 py-1 font-medium ${showHidden ? "text-zinc-500" : "bg-white text-zinc-900 shadow-sm dark:bg-zinc-800 dark:text-zinc-100"}`}
          onClick={() => {
            setShowHidden(false);
            setMenuId(null);
          }}
        >
          Chats
        </button>
        <button
          className={`flex-1 rounded-md px-2 py-1 font-medium ${showHidden ? "bg-white text-zinc-900 shadow-sm dark:bg-zinc-800 dark:text-zinc-100" : "text-zinc-500"}`}
          onClick={() => {
            setShowHidden(true);
            setMenuId(null);
          }}
        >
          Hidden{hiddenCount > 0 ? ` ${hiddenCount}` : ""}
        </button>
      </div>
      <ul className="flex-1 overflow-y-auto px-2 pb-2">
        {shown.map((t) => {
          const title = threadTitle(t);
          const other = t.participants.find((p) => !p.isOwner);
          const mine = t.participants.find((p) => p.id === t.lastParticipantId)?.isOwner;
          const open = menuId === t.id;
          return (
            <li key={t.id}>
              <div
                className={`flex items-center rounded-xl ${
                  t.id === selected ? "bg-indigo-50 dark:bg-indigo-500/15" : "hover:bg-zinc-100 dark:hover:bg-zinc-900"
                }`}
              >
                <button className="flex min-w-0 flex-1 items-center gap-3 px-2 py-2.5 text-left" onClick={() => onSelect(t.id)}>
                  <Avatar name={title} seed={other?.id ?? t.id} size="lg" />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline justify-between gap-2">
                      <span className="truncate font-semibold">{title}</span>
                      {t.lastSortKey !== null && <span className="shrink-0 text-[11px] text-zinc-500">{formatListDate(sortKeyMinute(t.lastSortKey))}</span>}
                    </div>
                    <p className="truncate text-[13px] text-zinc-500">
                      {mine ? "You: " : ""}
                      {previewText(t)}
                    </p>
                  </div>
                </button>
                <button
                  className="mr-1 rounded-lg p-1.5 text-zinc-500 hover:bg-zinc-200/80 dark:hover:bg-zinc-800"
                  aria-label={`Actions for ${title}`}
                  aria-expanded={open}
                  title="Hide or delete"
                  onClick={() => setMenuId(open ? null : t.id)}
                >
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
                    <circle cx="12" cy="5" r="1.7" />
                    <circle cx="12" cy="12" r="1.7" />
                    <circle cx="12" cy="19" r="1.7" />
                  </svg>
                </button>
              </div>
              {open && (
                <div className="mb-1 flex gap-1 px-2">
                  <button
                    className="flex-1 rounded-lg bg-zinc-100 px-2 py-1.5 text-sm font-medium hover:bg-zinc-200 dark:bg-zinc-800 dark:hover:bg-zinc-700"
                    onClick={() => {
                      setMenuId(null);
                      onSetHidden(t.id, !t.hidden);
                    }}
                  >
                    {showHidden ? "Unhide" : "Hide"}
                  </button>
                  <button
                    className="flex-1 rounded-lg bg-red-50 px-2 py-1.5 text-sm font-medium text-red-600 hover:bg-red-100 dark:bg-red-500/10 dark:text-red-400 dark:hover:bg-red-500/20"
                    onClick={() => {
                      setMenuId(null);
                      onDelete(t);
                    }}
                  >
                    Delete
                  </button>
                </div>
              )}
            </li>
          );
        })}
        {shown.length === 0 && (
          <li className="px-3 py-6 text-center text-sm text-zinc-500">
            {filter.trim() ? "No users match." : showHidden ? "No hidden chats." : "No chats."}
          </li>
        )}
      </ul>
    </aside>
  );
}
