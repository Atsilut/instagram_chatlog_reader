// SPDX-License-Identifier: AGPL-3.0-or-later

import { useState } from "react";
import { useApi, type ImportFinished, type ImportProgress } from "../api";
import { Button, Modal } from "./Modal";

export type ImportState =
  | { phase: "choose"; paths: string[] }
  | { phase: "running"; progress: ImportProgress | null }
  | { phase: "done"; result: ImportFinished };

interface Props {
  state: ImportState;
  onChange: (s: ImportState) => void;
  /** Closes the dialog; a running import keeps going in the background. */
  onClose: () => void;
}

export function progressPercent(state: ImportState): number | null {
  if (state.phase !== "running" || !state.progress || state.progress.pages_total === 0) return null;
  return Math.round((state.progress.pages_done / state.progress.pages_total) * 100);
}

export function ImportDialog({ state, onChange, onClose }: Props) {
  const api = useApi();
  const [confirmStop, setConfirmStop] = useState(false);

  if (state.phase === "running") {
    const p = state.progress;
    const pct = progressPercent(state);
    return (
      <Modal
        title="Importing"
        onClose={onClose}
        footer={
          confirmStop ? (
            <>
              <span className="mr-auto self-center text-sm text-zinc-500">Stop now? Everything imported so far is kept.</span>
              <Button onClick={() => setConfirmStop(false)}>Keep importing</Button>
              <Button onClick={() => api.cancelImport()}>Stop import</Button>
            </>
          ) : (
            <>
              <Button onClick={() => setConfirmStop(true)}>Stop import...</Button>
              <Button primary onClick={onClose}>
                Hide
              </Button>
            </>
          )
        }
      >
        <p className="mb-3 text-sm text-zinc-500">
          {p ? `${p.pages_done.toLocaleString()} of ${p.pages_total.toLocaleString()} pages` : "Scanning your backups..."}
          {p?.chat && <span className="ml-1">· {p.chat}</span>}
        </p>
        <div className="h-2 overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-800">
          <div className={`h-full rounded-full bg-indigo-600 transition-all ${pct === null ? "w-1/3 animate-pulse" : ""}`} style={pct === null ? undefined : { width: `${pct}%` }} />
        </div>
        <p className="mt-3 text-xs text-zinc-500">Hide this window to keep reading chats while the import continues. Progress stays visible in the chat list.</p>
      </Modal>
    );
  }

  if (state.phase === "done") {
    const { report, error, cancelled } = state.result;
    return (
      <Modal title={error ? "Import failed" : cancelled ? "Import cancelled" : "Import complete"} onClose={onClose} footer={<Button primary onClick={onClose}>Done</Button>}>
        {error && <p className="text-sm text-red-500">{error}</p>}
        {cancelled && <p className="text-sm text-zinc-500">Everything imported before stopping has been kept. Import the same backups again to finish; nothing will be duplicated.</p>}
        {report && (
          <div className="space-y-3 text-sm">
            <dl className="grid grid-cols-2 gap-2">
              <Stat label="New messages" value={report.messages_new} />
              <Stat label="Already in library" value={report.messages_existing} />
              <Stat label="Chats" value={report.threads} />
              <Stat label="Backups" value={report.exports} />
              <Stat label="New media files" value={report.media_files_stored} />
              <Stat label="Media not in backups" value={report.media_missing} />
            </dl>
            {report.scan.threads === 0 && (
              <p className="rounded-lg bg-amber-50 p-3 text-amber-900 dark:bg-amber-900/30 dark:text-amber-200">
                No chats were found. Choose an Instagram export (HTML format) ZIP or folder, a chat folder, or message_N.html files.
              </p>
            )}
            {report.tz_shifts.map((s, i) => (
              <p key={i} className="rounded-lg bg-zinc-100 p-3 dark:bg-zinc-800">
                Times in "{s.chat}" from {s.export} were shifted by {s.minutes / 60} h to match your other backups (timezone difference).
              </p>
            ))}
            {report.warnings.map((w, i) => (
              <p key={i} className="rounded-lg bg-amber-50 p-3 text-amber-900 dark:bg-amber-900/30 dark:text-amber-200">
                {w}
              </p>
            ))}
            {report.parse_issues > 0 && <p className="text-zinc-500">{report.parse_issues} messages had unreadable timestamps and were skipped.</p>}
          </div>
        )}
      </Modal>
    );
  }

  const add = async (kind: "zip" | "folder") => {
    const picked = await api.pickImportSources(kind);
    onChange({ phase: "choose", paths: [...new Set([...state.paths, ...picked])] });
  };
  const start = async () => {
    onChange({ phase: "running", progress: null });
    try {
      await api.startImport(state.paths);
    } catch (e) {
      onChange({ phase: "done", result: { report: null, error: String(e), cancelled: false } });
    }
  };

  return (
    <Modal
      title="Import Instagram backups"
      onClose={onClose}
      wide
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button primary disabled={state.paths.length === 0} onClick={start}>
            Import {state.paths.length > 0 ? `(${state.paths.length})` : ""}
          </Button>
        </>
      }
    >
      <p className="mb-4 text-sm text-zinc-500">
        Add any mix of export ZIPs and folders: whole backups, single chat folders, or message_N.html files. Overlapping backups are merged without duplicates. You can also drop files onto the window.
      </p>
      <div className="mb-4 flex gap-2">
        <Button onClick={() => add("zip")}>Add ZIP files...</Button>
        <Button onClick={() => add("folder")}>Add folder...</Button>
      </div>
      {state.paths.length === 0 ? (
        <div className="rounded-xl border border-dashed border-zinc-300 p-6 text-center text-sm text-zinc-500 dark:border-zinc-700">Nothing selected yet</div>
      ) : (
        <ul className="space-y-1">
          {state.paths.map((p) => (
            <li key={p} className="flex items-center gap-2 rounded-lg bg-zinc-100 px-3 py-2 text-sm dark:bg-zinc-800">
              <span className="min-w-0 flex-1 truncate" title={p}>
                {p}
              </span>
              <button className="text-zinc-500 hover:text-red-500" onClick={() => onChange({ phase: "choose", paths: state.paths.filter((x) => x !== p) })}>
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
    </Modal>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-lg bg-zinc-100 px-3 py-2 dark:bg-zinc-800">
      <dt className="text-xs text-zinc-500">{label}</dt>
      <dd className="text-lg font-semibold">{value.toLocaleString()}</dd>
    </div>
  );
}
