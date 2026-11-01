// SPDX-License-Identifier: AGPL-3.0-or-later

import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useApi, type DeepProgress, type HitView, type IndexProgress, type SearchFilters, type SearchOrder, type ThreadSummary } from "../api";
import { formatStamp, personName, threadTitle } from "../lib/format";

const PAGE = 50;
const DEBOUNCE_MS = 300;
/** How often deep search results refresh while it runs. */
const DEEP_REFRESH_MS = 800;
/** Below this many normal results, deep search is suggested. */
const FEW_RESULTS = 5;

interface Props {
  threads: ThreadSummary[];
  current: ThreadSummary | null;
  initialScope: "chat" | "all";
  /** Opens a chat centred on a message. */
  onOpen: (threadId: number, sortKey: number) => void;
  onClose: () => void;
}

interface DeepRun {
  running: boolean;
  progress: DeepProgress | null;
  error: string | null;
}

const IDLE: DeepRun = { running: false, progress: null, error: null };

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
  const [deepMode, setDeepMode] = useState(false);
  const [similarWords, setSimilarWords] = useState(() => localStorage.getItem("icv-similar-words") !== "0");
  const [order, setOrder] = useState<SearchOrder>("oldest");
  const [hits, setHits] = useState<HitView[]>([]);
  const [total, setTotal] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [indexing, setIndexing] = useState<IndexProgress | null>(null);
  const [pendingIndex, setPendingIndex] = useState(0);
  const [deep, setDeep] = useState<DeepRun>(IDLE);
  const [selected, setSelected] = useState<number | null>(null);
  const requestId = useRef(0);
  const input = useRef<HTMLInputElement>(null);
  const loaded = useRef(0);
  loaded.current = hits.length;

  const threadsById = useMemo(() => new Map(threads.map((t) => [t.id, t])), [threads]);
  const chat = scope === "chat" ? current : null;
  const query = text.trim();

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

  const toggleDeep = (on: boolean) => {
    setDeepMode(on);
    setOrder((o) => (on ? "bestMatch" : o === "bestMatch" ? "oldest" : o));
  };

  // Normal search.
  const runNormal = useCallback(
    async (offset: number) => {
      const id = ++requestId.current;
      if (!query) {
        setHits([]);
        setTotal(null);
        return;
      }
      setLoading(true);
      setError(null);
      try {
        const r = await api.search({
          text: query,
          filters,
          order: order === "bestMatch" ? "oldest" : order,
          offset,
          limit: PAGE,
          similarWords,
        });
        if (id !== requestId.current) return;
        setTotal(r.total);
        setHits((h) => (offset === 0 ? r.hits : [...h, ...r.hits]));
      } catch (e) {
        if (id === requestId.current) setError(String(e));
      } finally {
        if (id === requestId.current) setLoading(false);
      }
    },
    [api, query, filters, order, similarWords],
  );

  useEffect(() => {
    if (deepMode) return;
    const t = setTimeout(() => runNormal(0), DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [deepMode, runNormal]);

  // Deep search: (re)start when the query or filters change, re-page when the order changes.
  const fetchDeep = useCallback(
    async (count: number) => {
      const id = ++requestId.current;
      const r = await api.deepPage(order, 0, Math.max(PAGE, count));
      if (id !== requestId.current) return;
      setTotal(r.total);
      setHits(r.hits);
    },
    [api, order],
  );
  const fetchDeepRef = useRef(fetchDeep);
  fetchDeepRef.current = fetchDeep;

  useEffect(() => {
    if (!deepMode) {
      api.cancelDeepSearch();
      setDeep(IDLE);
      return;
    }
    requestId.current++;
    setHits([]);
    setTotal(null);
    if (!query) {
      api.cancelDeepSearch();
      setDeep(IDLE);
      return;
    }
    const t = setTimeout(() => {
      setDeep({ running: true, progress: null, error: null });
      setTotal(0);
      api.startDeepSearch(query, filters).catch((e) => setDeep({ running: false, progress: null, error: String(e) }));
    }, DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [api, deepMode, query, filters]);

  useEffect(() => {
    if (deepMode && query) fetchDeep(loaded.current);
  }, [deepMode, order, fetchDeep, query]);

  // Index status and deep search events.
  useEffect(() => {
    let lastRefresh = 0;
    api.indexStatus().then((s) => setPendingIndex(s.indexing ? s.pending : 0));
    const subs = [
      api.onIndexProgress((p) => setIndexing(p)),
      api.onIndexDone(() => {
        setIndexing(null);
        setPendingIndex(0);
      }),
      api.onDeepProgress((progress) => {
        setDeep((d) => (d.running ? { ...d, progress } : d));
        if (Date.now() - lastRefresh > DEEP_REFRESH_MS) {
          lastRefresh = Date.now();
          fetchDeepRef.current(loaded.current);
        }
      }),
      api.onDeepDone((done) => {
        if (done.cancelled) return;
        setDeep((d) => ({ ...d, running: false, error: done.error }));
        fetchDeepRef.current(loaded.current);
      }),
    ];
    return () => {
      api.cancelDeepSearch();
      subs.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, [api]);

  const loadMore = async () => {
    if (!deepMode) return runNormal(hits.length);
    const r = await api.deepPage(order, hits.length, PAGE);
    setTotal(r.total);
    setHits((h) => [...h, ...r.hits]);
  };

  const open = (h: HitView) => {
    setSelected(h.message.id);
    onOpen(h.threadId, h.message.sortKey);
  };
  const terms = useMemo(() => query.toLowerCase().split(/\s+/).filter(Boolean), [query]);
  const pct = (a: number, b: number) => (b > 0 ? Math.round((a / b) * 100) : 0);
  const fewNormal = !deepMode && total !== null && total < FEW_RESULTS && !loading;

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
          placeholder="Search words, including word forms"
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Escape" && onClose()}
        />
        <label className="group relative flex cursor-pointer items-start gap-2 rounded-lg px-1 py-1 text-sm">
          <input
            type="checkbox"
            className="mt-0.5 accent-indigo-600"
            checked={similarWords}
            onChange={(e) => {
              setSimilarWords(e.target.checked);
              localStorage.setItem("icv-similar-words", e.target.checked ? "1" : "0");
            }}
          />
          <span>Include similar words</span>
          <span className="pointer-events-none absolute top-full left-0 z-20 mt-1 hidden w-80 rounded-lg bg-zinc-900 p-2.5 text-xs leading-relaxed text-white shadow-lg group-hover:block dark:bg-zinc-100 dark:text-zinc-900">
            On: the normal search also finds similar verb variations and simple typos. Searching "love" finds "loved", "have loved" and "kove". Off: the normal search only finds the exact word. Searching "love" finds "love", not "loved" or "kove". Capitals and accents still match, so "Café" finds "cafe". "lovely" always needs deep search.
          </span>
        </label>
        <label className="flex cursor-pointer items-start gap-2 rounded-lg px-1 py-1 text-sm">
          <input type="checkbox" className="mt-0.5 accent-indigo-600" checked={deepMode} onChange={(e) => toggleDeep(e.target.checked)} />
          <span>
            <span className="font-medium">Deep search</span>
            <span className="block text-xs text-zinc-500">Also finds related words such as lovely, heavier typos and words written without spaces. Checks every message, so it's slower.</span>
          </span>
        </label>
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
            {deepMode && <option value="bestMatch">Best match</option>}
            <option value="oldest">Oldest first</option>
            <option value="newest">Newest first</option>
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

      {!deepMode && (indexing || pendingIndex > 0) && (
        <p className="mx-4 mb-2 rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-900 dark:bg-amber-900/30 dark:text-amber-200">
          Indexing messages{indexing ? ` (${pct(indexing.done, indexing.total)}%)` : ""}. Results may be incomplete until it finishes.
        </p>
      )}

      {deepMode && deep.running && (
        <div className="mx-4 mb-2 space-y-1.5">
          <div className="h-1.5 overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-800">
            <div className="h-full rounded-full bg-indigo-600 transition-all" style={{ width: `${deep.progress ? pct(deep.progress.scanned, deep.progress.total) : 0}%` }} />
          </div>
          <div className="flex justify-between text-xs text-zinc-500">
            <span>{deep.progress ? `${deep.progress.scanned.toLocaleString()} of ${deep.progress.total.toLocaleString()} messages checked` : "Starting..."}</span>
            <button className="text-red-500 hover:underline" onClick={() => api.cancelDeepSearch().then(() => setDeep((d) => ({ ...d, running: false })))}>
              Stop
            </button>
          </div>
        </div>
      )}

      <div className="flex-1 overflow-y-auto px-2 pb-4">
        {(error || deep.error) && <p className="px-2 text-sm text-red-500">{error ?? deep.error}</p>}
        {total !== null && query && (
          <p className="px-2 pb-2 text-xs text-zinc-500">
            {loading && hits.length === 0 ? "Searching..." : `${total.toLocaleString()} result${total === 1 ? "" : "s"}${deep.running ? " so far" : ""}`}
          </p>
        )}
        {fewNormal && (
          <div className="mx-2 mb-2 rounded-lg bg-indigo-50 px-3 py-2 text-xs text-indigo-900 dark:bg-indigo-500/15 dark:text-indigo-200">
            Not finding it? Deep search also looks for related words and heavier typos.
            <button className="ml-1 font-semibold underline" onClick={() => toggleDeep(true)}>
              Turn on deep search
            </button>
          </div>
        )}
        {hits.map((h) => (
          <Result key={h.message.id} hit={h} thread={threadsById.get(h.threadId)} showChat={!chat} terms={terms} active={selected === h.message.id} onClick={() => open(h)} />
        ))}
        {total !== null && hits.length < total && (
          <button
            className="mx-2 my-2 w-[calc(100%-1rem)] rounded-lg bg-zinc-100 py-2 text-sm hover:bg-zinc-200 dark:bg-zinc-900 dark:hover:bg-zinc-800"
            onClick={loadMore}
            disabled={loading}
          >
            Load more
          </button>
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
