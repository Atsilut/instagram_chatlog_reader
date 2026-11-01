// SPDX-License-Identifier: AGPL-3.0-or-later

import type { ReactNode } from "react";
import { formatSample } from "../lib/format";
import { LANGUAGES, TIME_STYLES, setDisplaySettings, useDisplaySettings } from "../lib/settings";
import { Modal } from "./Modal";

interface Props {
  onClose: () => void;
}

/** Language chooses month and weekday names. Time style chooses the date order and clock. */
export function SettingsDialog({ onClose }: Props) {
  const { language, timeStyle } = useDisplaySettings();

  return (
    <Modal title="Settings" onClose={onClose} wide>
      <section>
        <h3 className="mb-2 text-sm font-semibold">Language</h3>
        <p className="mb-3 text-xs text-zinc-500">Month and weekday names. The rest of the app stays in English.</p>
        <div className="grid grid-cols-2 gap-2">
          {LANGUAGES.map((item) => (
            <Choice key={item.id} selected={language === item.id} onClick={() => setDisplaySettings({ language: item.id })}>
              {item.label}
            </Choice>
          ))}
        </div>
      </section>
      <section className="mt-5">
        <h3 className="mb-2 text-sm font-semibold">Time</h3>
        <p className="mb-3 text-xs text-zinc-500">How dates and times are written. Today's messages still show only the time.</p>
        <div className="grid grid-cols-2 gap-2">
          {TIME_STYLES.map((item) => (
            <Choice key={item.id} selected={timeStyle === item.id} onClick={() => setDisplaySettings({ timeStyle: item.id })}>
              <span className="block">{item.label}</span>
              <span className="mt-0.5 block text-xs font-normal text-zinc-500">{formatSample(language, item.id)}</span>
            </Choice>
          ))}
        </div>
      </section>
    </Modal>
  );
}

function Choice({ selected, onClick, children }: { selected: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      className={`rounded-xl border px-3 py-2 text-left text-sm font-medium ${
        selected ? "border-indigo-500 bg-indigo-50 text-indigo-700 dark:bg-indigo-500/15 dark:text-indigo-300" : "border-zinc-200 hover:bg-zinc-50 dark:border-zinc-700 dark:hover:bg-zinc-800"
      }`}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
