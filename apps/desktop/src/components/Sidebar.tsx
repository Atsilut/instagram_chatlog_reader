// SPDX-License-Identifier: AGPL-3.0-or-later

import { useMemo, useState } from "react";
import type { ThreadSummary } from "../api";
import { formatShortDate, previewText, sortKeyMinute, threadTitle } from "../lib/format";
import { Avatar } from "./ChatView";

interface Props {
  threads: ThreadSummary[];
  selected: number | null;
  onSelect: (id: number) => void;
  onImport: () => void;
}

export function Sidebar({ threads, selected, onSelect, onImport }: Props) {
  const [filter, setFilter] = useState("");
  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return threads;
    return threads.filter((t) => [t.title, ...t.participants.flatMap((p) => [p.displayName, p.alias ?? ""])].some((n) => n.toLowerCase().includes(q)));
  }, [threads, filter]);

  return (
    <aside className="flex w-80 shrink-0 flex-col border-r border-zinc-200 dark:border-zinc-800">
      <div className="flex items-center justify-between px-4 pt-4 pb-3">
        <h1 className="text-lg font-bold tracking-tight">Chats</h1>
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
      <div className="px-4 pb-3">
        <input
          className="w-full rounded-lg bg-zinc-100 px-3 py-2 text-sm outline-none placeholder:text-zinc-500 focus:ring-2 focus:ring-indigo-500 dark:bg-zinc-900"
          placeholder="Filter chats"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      </div>
      <ul className="flex-1 overflow-y-auto px-2 pb-2">
        {shown.map((t) => {
          const title = threadTitle(t);
          const other = t.participants.find((p) => !p.isOwner);
          const mine = t.participants.find((p) => p.id === t.lastParticipantId)?.isOwner;
          return (
            <li key={t.id}>
              <button
                className={`flex w-full items-center gap-3 rounded-xl px-2 py-2.5 text-left ${
                  t.id === selected ? "bg-indigo-50 dark:bg-indigo-500/15" : "hover:bg-zinc-100 dark:hover:bg-zinc-900"
                }`}
                onClick={() => onSelect(t.id)}
              >
                <Avatar name={title} seed={other?.id ?? t.id} size="lg" />
                <div className="min-w-0 flex-1">
                  <div className="flex items-baseline justify-between gap-2">
                    <span className="truncate font-semibold">{title}</span>
                    {t.lastSortKey !== null && <span className="shrink-0 text-[11px] text-zinc-500">{formatShortDate(sortKeyMinute(t.lastSortKey))}</span>}
                  </div>
                  <p className="truncate text-[13px] text-zinc-500">
                    {mine ? "You: " : ""}
                    {previewText(t)}
                  </p>
                </div>
              </button>
            </li>
          );
        })}
        {shown.length === 0 && threads.length > 0 && <li className="px-3 py-6 text-center text-sm text-zinc-500">No chats match.</li>}
      </ul>
    </aside>
  );
}
