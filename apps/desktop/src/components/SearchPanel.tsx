// SPDX-License-Identifier: AGPL-3.0-or-later

import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useApi, type DeepProgress, type HitView, type IndexProgress, type SearchFilters, type SearchOrder, type ThreadSummary } from "../api";
import { formatStamp, personName, threadTitle } from "../lib/format";

const PAGE = 50;
const DEBOUNCE_MS = 250;
/** Below this many results the deep search button is highlighted. */
const FEW_RESULTS = 5;

interface Props {
  threads: ThreadSummary[];
  current: ThreadSummary | null;
  initialScope: "chat" | "all";
  /** Opens a chat centred on a message. */
  onOpen: (threadId: number, sortKey: number) => void;
  onClose: () => void;
}

interface DeepState {
  running: boolean;
  progress: DeepProgress | null;
  ids: number[];
  hits: HitView[];
  error: string | null;
}

const NO_DEEP: DeepState = { running: false, progress: null, ids: [], hits: [], error: null };

function dateToMinute(date: string, endOfDay: boolean): number | null {
  if (!date) return null;
  const [y, m, d] = date.split("-").map(Number);
  return Date.UTC(y, m - 1, d) / 60_000 + (endOfDay ? 1439 : 0);
}

export function SearchPanel({ threads, current, initialScope, onOpen, onClose }: Props) {
  const api = useApi();
  const [text, setText] = useState("");
  const [scope, setScope] = useState<"chat" | "all">(current ? initialScope : "all");
  const [participant, setParticipant] = useState<number | null>(null);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [order, setOrder] = useState<SearchOrder>("relevance");
  const [hits, setHits] = useState<HitView[]>([]);
  const [total, setTotal] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [indexing, setIndexing] = useState<IndexProgress | null>(null);
  const [pendingIndex, setPendingIndex] = useState(0);
  const [deep, setDeep] = useState<DeepState>(NO_DEEP);
  const [selected, setSelected] = useState<number | null>(null);
  const requestId = useRef(0);
  const input = useRef<HTMLInputElement>(null);

  const threadsById = useMemo(() => new Map(threads.map((t) => [t.id, t])), [threads]);
  const chat = scope === "chat" ? current : null;

  const filters: SearchFilters = useMemo(
    () => ({
      threadId: chat?.id ?? null,
      participantId: chat ? participant : null,
      fromMinute: dateToMinute(from, false),
      toMinute: dateToMinute(to, true),
    }),
    [chat, participant, from, to],
  );

  useEffect(() => input.current?.focus(), []);
  useEffect(() => setScope(current ? initialScope : "all"), [initialScope, current]);
  useEffect(() => setParticipant(null), [chat?.id]);

  const run = useCallback(
    async (offset: number) => {
      const id = ++requestId.current;
      if (!text.trim()) {
        setHits([]);
        setTotal(null);
        return;
      }
      setLoading(true);
      setError(null);
      try {
        const r = await api.search({ text, filters, order, offset, limit: PAGE });
        if (id !== requestId.current) return;
        setTotal(r.total);
        setHits((h) => (offset === 0 ? r.hits : [...h, ...r.hits]));
      } catch (e) {
        if (id === requestId.current) setError(String(e));
      } finally {
        if (id === requestId.current) setLoading(false);
      }
    },
    [api, text, filters, order],
  );

  useEffect(() => {
    api.cancelDeepSearch();
    setDeep(NO_DEEP);
    const t = setTimeout(() => run(0), DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [api, run]);

  // Index status and deep search events.
  useEffect(() => {
    api.indexStatus().then((s) => setPendingIndex(s.indexing ? s.pending : 0));
    const subs = [
      api.onIndexProgress((p) => setIndexing(p)),
      api.onIndexDone(() => {
        setIndexing(null);
        setPendingIndex(0);
        run(0);
      }),
      api.onDeepHits((ids) => setDeep((d) => ({ ...d, ids: [...d.ids, ...ids] }))),
      api.onDeepProgress((progress) => setDeep((d) => ({ ...d, progress }))),
      api.onDeepDone((done) => setDeep((d) => ({ ...d, running: false, error: done.error }))),
    ];
    return () => {
      api.cancelDeepSearch();
      subs.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, [api, run]);

  // Load details for the first page of deep results as ids stream in.
  useEffect(() => {
    if (deep.hits.length >= PAGE || deep.hits.length >= deep.ids.length) return;
    const ids = deep.ids.slice(deep.hits.length, PAGE);
    api.loadHits(ids).then((more) => setDeep((d) => ({ ...d, hits: [...d.hits, ...more] })));
  }, [api, deep.ids, deep.hits.length]);

  const startDeep = () => {
    setDeep({ ...NO_DEEP, running: true });
    api.startDeepSearch(text, filters).catch((e) => setDeep((d) => ({ ...d, running: false, error: String(e) })));
  };
  const moreDeep = async () => {
    const ids = deep.ids.slice(deep.hits.length, deep.hits.length + PAGE);
    const more = await api.loadHits(ids);
    setDeep((d) => ({ ...d, hits: [...d.hits, ...more] }));
  };

  const open = (h: HitView) => {
    setSelected(h.message.id);
    onOpen(h.threadId, h.message.sortKey);
  };
  const terms = useMemo(() => text.toLowerCase().split(/\s+/).filter(Boolean), [text]);
  const few = total !== null && total < FEW_RESULTS;
  const pct = (a: number, b: number) => (b > 0 ? Math.round((a / b) * 100) : 0);

  return (
    <aside className="flex w-[380px] shrink-0 flex-col border-l border-zinc-200 dark:border-zinc-800">
      <div className="flex items-center gap-2 px-4 pt-4 pb-2">
        <h2 className="flex-1 text-base font-semibold">Search messages</h2>
        <button className="rounded-full p-1 text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800" onClick={onClose} aria-label="Close search">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M6 6l12 12M18 6L6 18" />
          </svg>
        </button>
      </div>

      <div className="space-y-2 px-4 pb-3">
        <input
          ref={input}
          className="w-full rounded-lg bg-zinc-100 px-3 py-2 text-sm outline-none placeholder:text-zinc-500 focus:ring-2 focus:ring-indigo-500 dark:bg-zinc-900"
          placeholder="Words, phrases, typos are fine"
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Escape" && onClose()}
        />
        <div className="flex gap-2 text-xs">
          <Select value={scope} onChange={(v) => setScope(v as "chat" | "all")}>
            {current && <option value="chat">This chat</option>}
            <option value="all">All chats</option>
          </Select>
          {chat && (
            <Select value={participant === null ? "" : String(participant)} onChange={(v) => setParticipant(v ? Number(v) : null)}>
              <option value="">Anyone</option>
              {chat.participants.map((p) => (
                <option key={p.id} value={p.id}>
                  {personName(p)}
                  {p.isOwner ? " (you)" : ""}
                </option>
              ))}
            </Select>
          )}
          <Select value={order} onChange={(v) => setOrder(v as SearchOrder)}>
            <option value="relevance">Best match</option>
            <option value="newest">Newest</option>
            <option value="oldest">Oldest</option>
          </Select>
        </div>
        <div className="flex items-center gap-2 text-xs text-zinc-500">
          <input type="date" className="rounded-md bg-zinc-100 px-2 py-1 dark:bg-zinc-900" value={from} onChange={(e) => setFrom(e.target.value)} />
          <span>to</span>
          <input type="date" className="rounded-md bg-zinc-100 px-2 py-1 dark:bg-zinc-900" value={to} onChange={(e) => setTo(e.target.value)} />
          {(from || to) && (
            <button className="text-indigo-600 hover:underline dark:text-indigo-400" onClick={() => (setFrom(""), setTo(""))}>
              Clear
            </button>
          )}
        </div>
      </div>

      {(indexing || pendingIndex > 0) && (
        <p className="mx-4 mb-2 rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-900 dark:bg-amber-900/30 dark:text-amber-200">
          Indexing messages{indexing ? ` (${pct(indexing.done, indexing.total)}%)` : ""}. Results may be incomplete until it finishes.
        </p>
      )}

      <div className="flex-1 overflow-y-auto px-2 pb-4">
        {error && <p className="px-2 text-sm text-red-500">{error}</p>}
        {total !== null && (
          <p className="px-2 pb-2 text-xs text-zinc-500">
            {loading && hits.length === 0 ? "Searching..." : `${total.toLocaleString()} result${total === 1 ? "" : "s"}`}
          </p>
        )}
        {hits.map((h) => (
          <Result key={h.message.id} hit={h} thread={threadsById.get(h.threadId)} showChat={!chat} terms={terms} active={selected === h.message.id} onClick={() => open(h)} />
        ))}
        {total !== null && hits.length < total && (
          <button className="mx-2 my-2 w-[calc(100%-1rem)] rounded-lg bg-zinc-100 py-2 text-sm hover:bg-zinc-200 dark:bg-zinc-900 dark:hover:bg-zinc-800" onClick={() => run(hits.length)} disabled={loading}>
            Load more
          </button>
        )}

        {total !== null && text.trim() && (
          <div className="mx-2 mt-3 rounded-xl border border-zinc-200 p-3 dark:border-zinc-800">
            <p className="mb-2 text-xs text-zinc-500">
              {few ? "Few results. " : ""}Deep search checks every message letter by letter, so it finds heavily misspelled or split words the fast search misses. It can take a while on large libraries.
            </p>
            {deep.running ? (
              <div className="space-y-2">
                <div className="h-1.5 overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-800">
                  <div className="h-full rounded-full bg-indigo-600 transition-all" style={{ width: `${deep.progress ? pct(deep.progress.scanned, deep.progress.total) : 0}%` }} />
                </div>
                <div className="flex justify-between text-xs text-zinc-500">
                  <span>
                    {deep.progress ? `${deep.progress.scanned.toLocaleString()} of ${deep.progress.total.toLocaleString()} checked` : "Starting..."} · {deep.ids.length.toLocaleString()} found
                  </span>
                  <button className="text-red-500 hover:underline" onClick={() => api.cancelDeepSearch()}>
                    Stop
                  </button>
                </div>
              </div>
            ) : (
              <button
                className={`w-full rounded-lg py-2 text-sm font-medium ${few ? "bg-indigo-600 text-white hover:bg-indigo-500" : "bg-zinc-100 hover:bg-zinc-200 dark:bg-zinc-900 dark:hover:bg-zinc-800"}`}
                onClick={startDeep}
              >
                {deep.progress ? "Run deep search again" : "Run deep search"}
              </button>
            )}
            {deep.error && <p className="mt-2 text-xs text-red-500">{deep.error}</p>}
            {deep.progress && !deep.running && <p className="mt-2 text-xs text-zinc-500">Deep search found {deep.ids.length.toLocaleString()} messages.</p>}
          </div>
        )}
        {deep.hits.length > 0 && (
          <div className="mt-2">
            {deep.hits.map((h) => (
              <Result key={`d${h.message.id}`} hit={h} thread={threadsById.get(h.threadId)} showChat={!chat} terms={terms} active={selected === h.message.id} onClick={() => open(h)} />
            ))}
            {deep.hits.length < deep.ids.length && (
              <button className="mx-2 my-2 w-[calc(100%-1rem)] rounded-lg bg-zinc-100 py-2 text-sm hover:bg-zinc-200 dark:bg-zinc-900 dark:hover:bg-zinc-800" onClick={moreDeep}>
                Load more deep results
              </button>
            )}
          </div>
        )}
      </div>
    </aside>
  );
}

function Select({ value, onChange, children }: { value: string; onChange: (v: string) => void; children: ReactNode }) {
  return (
    <select className="min-w-0 flex-1 rounded-md bg-zinc-100 px-2 py-1 outline-none dark:bg-zinc-900" value={value} onChange={(e) => onChange(e.target.value)}>
      {children}
    </select>
  );
}

function Result({
  hit,
  thread,
  showChat,
  terms,
  active,
  onClick,
}: {
  hit: HitView;
  thread: ThreadSummary | undefined;
  showChat: boolean;
  terms: string[];
  active: boolean;
  onClick: () => void;
}) {
  const m = hit.message;
  const sender = thread?.participants.find((p) => p.id === m.participantId);
  const body = m.text || m.parts.map((p) => [p.author, p.caption].filter(Boolean).join(": ")).filter(Boolean).join(" ") || "(attachment)";
  return (
    <button className={`block w-full rounded-xl px-3 py-2.5 text-left ${active ? "bg-indigo-50 dark:bg-indigo-500/15" : "hover:bg-zinc-100 dark:hover:bg-zinc-900"}`} onClick={onClick}>
      <div className="mb-0.5 flex items-baseline justify-between gap-2 text-xs text-zinc-500">
        <span className="truncate">
          {showChat && thread && <span className="font-semibold text-zinc-700 dark:text-zinc-300">{threadTitle(thread)} · </span>}
          {sender?.isOwner ? "You" : personName(sender)}
        </span>
        <span className="shrink-0">{formatStamp(m.tsMinute)}</span>
      </div>
      <p className="line-clamp-3 text-sm break-words">
        <Highlighted text={snippet(body, terms)} terms={terms} />
      </p>
    </button>
  );
}

/** Text around the first matching term, so long messages show the relevant part. */
function snippet(text: string, terms: string[]): string {
  const flat = text.replace(/\s+/g, " ");
  if (flat.length <= 160) return flat;
  const lower = flat.toLowerCase();
  const at = terms.map((t) => lower.indexOf(t)).filter((i) => i >= 0).sort((a, b) => a - b)[0] ?? 0;
  const start = Math.max(0, at - 50);
  return `${start > 0 ? "..." : ""}${flat.slice(start, start + 160)}${start + 160 < flat.length ? "..." : ""}`;
}

function Highlighted({ text, terms }: { text: string; terms: string[] }) {
  const usable = terms.filter((t) => t.length >= 2).map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  if (usable.length === 0) return <>{text}</>;
  const parts = text.split(new RegExp(`(${usable.join("|")})`, "gi"));
  return (
    <>
      {parts.map((p, i) =>
        i % 2 === 1 ? (
          <mark key={i} className="rounded bg-yellow-200 px-0.5 text-inherit dark:bg-yellow-500/40">
            {p}
          </mark>
        ) : (
          p
        ),
      )}
    </>
  );
}
