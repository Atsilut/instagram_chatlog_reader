// SPDX-License-Identifier: AGPL-3.0-or-later

import { useCallback, useEffect, useRef, useState } from "react";
import { useApi, type ThreadSummary } from "./api";
import { ChatView } from "./components/ChatView";
import { ImportDialog, type ImportState } from "./components/ImportDialog";
import { Button } from "./components/Modal";
import { PeopleDialog } from "./components/PeopleDialog";
import { SearchPanel } from "./components/SearchPanel";
import { SettingsDialog } from "./components/SettingsDialog";
import { Sidebar } from "./components/Sidebar";

/** How often the chat list refreshes while an import is running. */
const LIVE_REFRESH_MS = 3000;

export default function App() {
  const api = useApi();
  const [threads, setThreads] = useState<ThreadSummary[] | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [importState, setImportState] = useState<ImportState | null>(null);
  const [importVisible, setImportVisible] = useState(false);
  const [peopleOpen, setPeopleOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchAll, setSearchAll] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [jump, setJump] = useState<{ threadId: number; sortKey: number; nonce: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const lastRefresh = useRef(0);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        setSearchAll(e.shiftKey);
        setSearchOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const selectThread = (id: number) => {
    setSelected(id);
    setJump(null);
  };
  const openHit = (threadId: number, sortKey: number) => {
    setSelected(threadId);
    setJump((j) => ({ threadId, sortKey, nonce: (j?.nonce ?? 0) + 1 }));
  };

  const refresh = useCallback(async () => {
    lastRefresh.current = Date.now();
    try {
      await api.libraryInfo();
      const list = await api.listThreads();
      setThreads(list);
      setSelected((s) => (s !== null && list.some((t) => t.id === s) ? s : (list[0]?.id ?? null)));
    } catch (e) {
      setError(String(e));
    }
  }, [api]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    const subs = [
      api.onImportProgress((progress) => {
        setImportState({ phase: "running", progress });
        if (Date.now() - lastRefresh.current > LIVE_REFRESH_MS) refresh();
      }),
      api.onImportFinished((result) => {
        setImportState({ phase: "done", result });
        setImportVisible(true);
        refresh();
      }),
      api.onDropPaths((paths) => {
        setImportState((s) => (s?.phase === "running" ? s : { phase: "choose", paths: [...new Set([...(s?.phase === "choose" ? s.paths : []), ...paths])] }));
        setImportVisible(true);
      }),
    ];
    return () => subs.forEach((p) => p.then((unlisten) => unlisten()));
  }, [api, refresh]);

  const openImport = () => {
    setImportState((s) => (s?.phase === "running" ? s : { phase: "choose", paths: [] }));
    setImportVisible(true);
  };
  const closeImport = () => {
    setImportVisible(false);
    setImportState((s) => (s?.phase === "running" ? s : null));
  };
  const thread = threads?.find((t) => t.id === selected) ?? null;

  if (error) return <p className="p-8 text-red-500">{error}</p>;
  if (!threads) return <p className="p-8 text-zinc-500">Opening library...</p>;

  return (
    <div className="flex h-full">
      {threads.length === 0 && importState?.phase !== "running" ? (
        <Welcome onImport={openImport} />
      ) : (
        <>
          <Sidebar
            threads={threads}
            selected={selected}
            onSelect={selectThread}
            onImport={openImport}
            onSettings={() => setSettingsOpen(true)}
            importing={importState?.phase === "running" ? importState : null}
          />
          {thread ? (
            <ChatView
              key={thread.id}
              thread={thread}
              target={jump?.threadId === thread.id ? jump.sortKey : undefined}
              jumpNonce={jump?.threadId === thread.id ? jump.nonce : undefined}
              onPeople={() => setPeopleOpen(true)}
              onSearch={() => {
                setSearchAll(false);
                setSearchOpen(true);
              }}
              onLatest={() => setJump(null)}
            />
          ) : (
            <div className="flex flex-1 items-center justify-center text-sm text-zinc-500">Chats appear here as they are imported.</div>
          )}
          {searchOpen && (
            <SearchPanel
              threads={threads}
              current={thread}
              allChats={searchAll || !thread}
              onAllChats={setSearchAll}
              onOpen={openHit}
              onClose={() => setSearchOpen(false)}
            />
          )}
          {settingsOpen && <SettingsDialog onClose={() => setSettingsOpen(false)} />}
        </>
      )}
      {importState && importVisible && <ImportDialog state={importState} onChange={setImportState} onClose={closeImport} />}
      {peopleOpen && thread && (
        <PeopleDialog
          participants={thread.participants}
          onSaved={(participants) => setThreads((list) => list?.map((t) => (t.id === thread.id ? { ...t, participants } : t)) ?? null)}
          onClose={() => setPeopleOpen(false)}
        />
      )}
    </div>
  );
}

function Welcome({ onImport }: { onImport: () => void }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-4 p-8 text-center">
      <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-gradient-to-br from-amber-400 via-pink-500 to-indigo-600 text-white">
        <svg width="30" height="30" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
          <path d="M21 12a8 8 0 0 1-11.6 7.1L4 20l1-4.6A8 8 0 1 1 21 12z" />
        </svg>
      </div>
      <h1 className="text-2xl font-bold">Instagram Chat Viewer</h1>
      <p className="max-w-md text-sm text-zinc-500">
        Import the HTML export from Instagram's "Download your information". Add as many backups as you have, in any order. Messages are merged without duplicates and everything stays on this computer.
      </p>
      <Button primary onClick={onImport}>
        Import backups
      </Button>
      <p className="text-xs text-zinc-400">or drop ZIP files and folders anywhere on this window</p>
    </div>
  );
}
