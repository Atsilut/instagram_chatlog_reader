// SPDX-License-Identifier: AGPL-3.0-or-later

import { useState } from "react";
import { useApi, type ParticipantInfo } from "../api";
import { Avatar } from "./ChatView";
import { Button, Modal } from "./Modal";

interface Props {
  participants: ParticipantInfo[];
  onSaved: (participants: ParticipantInfo[]) => void;
  onClose: () => void;
}

/** Rename people in a chat. Names come from the export; a custom name only changes what this app shows. */
export function PeopleDialog({ participants, onSaved, onClose }: Props) {
  const api = useApi();
  const [drafts, setDrafts] = useState(() => new Map(participants.map((p) => [p.id, p.alias ?? ""])));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      let latest = participants;
      for (const p of participants) {
        const draft = drafts.get(p.id)?.trim() ?? "";
        if (draft !== (p.alias ?? "")) latest = await api.setAlias(p.id, draft || null);
      }
      onSaved(latest);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title="People"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button primary disabled={saving} onClick={save}>
            Save
          </Button>
        </>
      }
    >
      <p className="mb-4 text-sm text-zinc-500">Give anyone a custom name. Leave it empty to use the name from Instagram.</p>
      <ul className="space-y-3">
        {participants.map((p) => (
          <li key={p.id} className="flex items-center gap-3">
            <Avatar name={drafts.get(p.id) || p.displayName} seed={p.id} size="md" />
            <div className="min-w-0 flex-1">
              <div className="mb-1 text-xs text-zinc-500">
                {p.displayName}
                {p.isOwner && <span className="ml-1 font-semibold text-indigo-600 dark:text-indigo-400">(you)</span>}
              </div>
              <input
                className="w-full rounded-lg bg-zinc-100 px-3 py-1.5 text-sm outline-none focus:ring-2 focus:ring-indigo-500 dark:bg-zinc-800"
                placeholder={p.displayName}
                value={drafts.get(p.id) ?? ""}
                onChange={(e) => setDrafts(new Map(drafts).set(p.id, e.target.value))}
              />
            </div>
          </li>
        ))}
      </ul>
      {error && <p className="mt-3 text-sm text-red-500">{error}</p>}
    </Modal>
  );
}
