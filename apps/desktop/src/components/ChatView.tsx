// SPDX-License-Identifier: AGPL-3.0-or-later

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Virtuoso } from "react-virtuoso";
import { useApi, type MessageView, type ParticipantInfo, type ThreadSummary } from "../api";
import { avatarColor, dayNumber, formatDay, formatTime, initials, personName, threadTitle } from "../lib/format";
import { Lightbox, Part } from "./Parts";

const PAGE = 150;
/** Virtuoso needs a stable, large index base to prepend older messages without jumping. */
const START_INDEX = 1_000_000_000;

type Row =
  | { kind: "day"; key: string; label: string }
  | { kind: "msg"; key: string; msg: MessageView; start: boolean; end: boolean };

const isNotice = (m: MessageView) => m.kind === "like" || m.kind === "reaction_notice";

function sameRun(a: MessageView, b: MessageView) {
  return (
    a.participantId === b.participantId &&
    dayNumber(a.tsMinute) === dayNumber(b.tsMinute) &&
    Math.abs(b.tsMinute - a.tsMinute) <= 10 &&
    !isNotice(a) &&
    !isNotice(b)
  );
}

function buildRows(msgs: MessageView[]): Row[] {
  const rows: Row[] = [];
  msgs.forEach((m, i) => {
    const prev = msgs[i - 1];
    const next = msgs[i + 1];
    if (!prev || dayNumber(prev.tsMinute) !== dayNumber(m.tsMinute)) {
      rows.push({ kind: "day", key: `d${dayNumber(m.tsMinute)}`, label: formatDay(m.tsMinute) });
    }
    rows.push({ kind: "msg", key: `m${m.id}`, msg: m, start: !prev || !sameRun(prev, m), end: !next || !sameRun(m, next) });
  });
  return rows;
}

interface Props {
  thread: ThreadSummary;
  /** Open centred on this message instead of at the newest one. */
  target?: number;
  onPeople: () => void;
}

export function ChatView({ thread, target, onPeople }: Props) {
  const api = useApi();
  const [msgs, setMsgs] = useState<MessageView[]>([]);
  const [firstIndex, setFirstIndex] = useState(START_INDEX);
  const [initialIndex, setInitialIndex] = useState<number | null>(null);
  const [hasOlder, setHasOlder] = useState(true);
  const [hasNewer, setHasNewer] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [image, setImage] = useState<string | null>(null);
  const loading = useRef(false);
  const msgsRef = useRef(msgs);
  msgsRef.current = msgs;

  const people = useMemo(() => new Map(thread.participants.map((p) => [p.id, p])), [thread.participants]);
  const isGroup = thread.kind === "group";

  useEffect(() => {
    let alive = true;
    setMsgs([]);
    setInitialIndex(null);
    setError(null);
    const window = target !== undefined ? ({ kind: "around", sortKey: target } as const) : ({ kind: "latest" } as const);
    api.loadMessages(thread.id, window, PAGE * 2).then(
      (loaded) => {
        if (!alive) return;
        const rows = buildRows(loaded);
        const targetRow = target === undefined ? rows.length - 1 : Math.max(0, rows.findIndex((r) => r.kind === "msg" && r.msg.sortKey === target));
        setFirstIndex(START_INDEX - rows.length);
        setInitialIndex(targetRow);
        setHasOlder(loaded.length > 0);
        setHasNewer(target !== undefined);
        setMsgs(loaded);
      },
      (e) => alive && setError(String(e)),
    );
    return () => {
      alive = false;
    };
  }, [api, thread.id, target]);

  const rows = useMemo(() => buildRows(msgs), [msgs]);

  const loadOlder = useCallback(async () => {
    const current = msgsRef.current;
    if (loading.current || !hasOlder || current.length === 0) return;
    loading.current = true;
    try {
      const older = await api.loadMessages(thread.id, { kind: "before", sortKey: current[0].sortKey }, PAGE);
      if (older.length < PAGE) setHasOlder(false);
      if (older.length > 0) {
        const next = [...older, ...current];
        setFirstIndex((i) => i - (buildRows(next).length - buildRows(current).length));
        setMsgs(next);
      }
    } finally {
      loading.current = false;
    }
  }, [api, thread.id, hasOlder]);

  const loadNewer = useCallback(async () => {
    const current = msgsRef.current;
    if (loading.current || !hasNewer || current.length === 0) return;
    loading.current = true;
    try {
      const newer = await api.loadMessages(thread.id, { kind: "after", sortKey: current[current.length - 1].sortKey }, PAGE);
      if (newer.length < PAGE) setHasNewer(false);
      if (newer.length > 0) setMsgs([...current, ...newer]);
    } finally {
      loading.current = false;
    }
  }, [api, thread.id, hasNewer]);

  const title = threadTitle(thread);

  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <header className="flex items-center gap-3 border-b border-zinc-200 px-5 py-3 dark:border-zinc-800">
        <Avatar name={title} seed={thread.participants.find((p) => !p.isOwner)?.id ?? thread.id} size="md" />
        <div className="min-w-0 flex-1">
          <h1 className="truncate text-[15px] font-semibold">{title}</h1>
          <p className="text-xs text-zinc-500">{thread.messageCount.toLocaleString()} messages</p>
        </div>
        <button className="rounded-lg px-3 py-1.5 text-sm text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800" onClick={onPeople}>
          People
        </button>
      </header>

      {error && <p className="p-6 text-sm text-red-500">{error}</p>}
      {initialIndex !== null && (
        <Virtuoso
          key={`${thread.id}:${target ?? "latest"}`}
          className="flex-1"
          data={rows}
          firstItemIndex={firstIndex}
          initialTopMostItemIndex={{ index: initialIndex, align: target === undefined ? "end" : "center" }}
          computeItemKey={(_, row) => row.key}
          startReached={loadOlder}
          endReached={loadNewer}
          increaseViewportBy={{ top: 800, bottom: 800 }}
          itemContent={(_, row) =>
            row.kind === "day" ? (
              <div className="flex justify-center py-3">
                <span className="rounded-full bg-zinc-100 px-3 py-1 text-xs font-medium text-zinc-500 dark:bg-zinc-900 dark:text-zinc-400">{row.label}</span>
              </div>
            ) : (
              <MessageRow
                row={row}
                people={people}
                showNames={isGroup}
                highlight={row.msg.sortKey === target}
                onOpenImage={setImage}
              />
            )
          }
          components={{ Footer: () => <div className="h-4" /> }}
        />
      )}
      {image && <Lightbox src={image} onClose={() => setImage(null)} />}
    </section>
  );
}

function MessageRow({
  row,
  people,
  showNames,
  highlight,
  onOpenImage,
}: {
  row: Extract<Row, { kind: "msg" }>;
  people: Map<number, ParticipantInfo>;
  showNames: boolean;
  highlight: boolean;
  onOpenImage: (src: string) => void;
}) {
  const { msg, start, end } = row;
  const sender = people.get(msg.participantId);
  const mine = sender?.isOwner ?? false;
  const name = personName(sender);

  if (isNotice(msg)) {
    const what = msg.kind === "like" ? "liked a message" : (msg.text ?? "reacted to a message").replace(/^Reacted/, "reacted").replace(/ to your message$/, " to a message");
    return (
      <p className="px-6 py-1 text-center text-xs text-zinc-500">
        {mine ? "You" : name} {what} · {formatTime(msg.tsMinute)}
      </p>
    );
  }

  const textIsUrl = msg.kind === "link" && msg.parts.some((p) => p.url === msg.text);
  const showText = msg.text && !textIsUrl;
  const reactions = groupReactions(msg, people);

  return (
    <div className={`flex gap-2 px-5 ${start ? "pt-2" : "pt-0.5"} ${mine ? "flex-row-reverse" : ""}`}>
      {!mine && <div className="w-7 shrink-0">{end && <Avatar name={name} seed={msg.participantId} size="sm" />}</div>}
      <div className={`flex max-w-[70%] min-w-0 flex-col ${mine ? "items-end" : "items-start"}`}>
        {start && showNames && !mine && <span className="mb-0.5 ml-3 text-xs text-zinc-500">{name}</span>}
        <div className={`flex flex-col gap-1 ${mine ? "items-end" : "items-start"} ${highlight ? "flash rounded-2xl" : ""}`}>
          {showText && (
            <div
              className={`rounded-2xl px-3.5 py-2 text-[14px] leading-snug break-words whitespace-pre-wrap ${
                mine ? "bg-indigo-600 text-white" : "bg-zinc-100 text-zinc-900 dark:bg-zinc-800 dark:text-zinc-100"
              }`}
              title={formatTime(msg.tsMinute)}
            >
              {msg.text}
            </div>
          )}
          {msg.parts.map((p, i) => (
            <Part key={i} part={p} onOpenImage={onOpenImage} />
          ))}
          {msg.kind === "empty" && (
            <div className="rounded-2xl border border-dashed border-zinc-300 px-3.5 py-2 text-xs text-zinc-500 italic dark:border-zinc-700">
              Unsupported or unsent message
            </div>
          )}
        </div>
        {reactions.length > 0 && (
          <div className="-mt-1 flex gap-1 px-2">
            {reactions.map((r) => (
              <span key={r.emoji} title={r.names.join(", ")} className="rounded-full border border-white bg-zinc-100 px-1.5 text-xs dark:border-zinc-950 dark:bg-zinc-800">
                {r.emoji}
                {r.names.length > 1 && <span className="ml-0.5 text-zinc-500">{r.names.length}</span>}
              </span>
            ))}
          </div>
        )}
        {(end || msg.missingFromLatest) && (
          <span className="mt-0.5 px-2 text-[11px] text-zinc-400">
            {msg.missingFromLatest && (
              <span className="mr-1.5 rounded bg-amber-100 px-1 text-amber-800 dark:bg-amber-900/40 dark:text-amber-300" title="This message is in an older backup but not in the newest one (it may have been unsent or deleted).">
                Not in latest backup
              </span>
            )}
            {end && formatTime(msg.tsMinute)}
          </span>
        )}
      </div>
    </div>
  );
}

function groupReactions(msg: MessageView, people: Map<number, ParticipantInfo>) {
  const groups = new Map<string, string[]>();
  for (const r of msg.reactions) {
    const list = groups.get(r.emoji) ?? [];
    list.push(personName(people.get(r.participantId)));
    groups.set(r.emoji, list);
  }
  return [...groups].map(([emoji, names]) => ({ emoji, names }));
}

export function Avatar({ name, seed, size }: { name: string; seed: number; size: "sm" | "md" | "lg" }) {
  const dims = size === "sm" ? "h-7 w-7 text-xs" : size === "md" ? "h-10 w-10 text-sm" : "h-12 w-12 text-base";
  return <div className={`flex shrink-0 items-center justify-center rounded-full font-semibold text-white ${dims} ${avatarColor(seed)}`}>{initials(name)}</div>;
}
