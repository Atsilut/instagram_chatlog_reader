// SPDX-License-Identifier: AGPL-3.0-or-later

import { useEffect, useRef, useState } from "react";
import { calendarMonthTitle, calendarPlaceholder, formatCalendarDate, weekdayLabels, weekStartsOn } from "../lib/format";
import { useDisplaySettings } from "../lib/settings";

interface Props {
  value: string;
  onChange: (iso: string) => void;
  align?: "left" | "right";
}

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

function cellsFor(year: number, monthIndex: number, weekStart: number): (number | null)[] {
  const first = new Date(Date.UTC(year, monthIndex, 1)).getUTCDay();
  const count = new Date(Date.UTC(year, monthIndex + 1, 0)).getUTCDate();
  const lead = (first - weekStart + 7) % 7;
  const cells: (number | null)[] = Array(lead).fill(null);
  for (let day = 1; day <= count; day++) cells.push(day);
  while (cells.length % 7 !== 0) cells.push(null);
  return cells;
}

/** A date field whose text and calendar follow the language and time style. */
export function DateField({ value, onChange, align = "left" }: Props) {
  const { language, timeStyle } = useDisplaySettings();
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const selected = value.split("-").map(Number);
  const initial = selected.length === 3 && selected.every(Boolean) ? { year: selected[0], month: selected[1] - 1 } : null;
  const now = new Date();
  const [view, setView] = useState(initial ?? { year: now.getFullYear(), month: now.getMonth() });

  const toggle = () => {
    if (!open && initial) setView(initial);
    setOpen((current) => !current);
  };

  useEffect(() => {
    if (!open) return;
    const onPointer = (event: MouseEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const shift = (delta: number) => {
    setView((current) => {
      const month = current.month + delta;
      return { year: current.year + Math.floor(month / 12), month: ((month % 12) + 12) % 12 };
    });
  };

  const pick = (day: number) => {
    onChange(`${view.year}-${pad(view.month + 1)}-${pad(day)}`);
    setOpen(false);
  };

  const today = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
  const labels = weekdayLabels(language, timeStyle);

  return (
    <div className="relative min-w-0 flex-1" ref={root}>
      <button
        type="button"
        className="w-full truncate rounded-md bg-zinc-100 px-2 py-1 text-left dark:bg-zinc-900"
        onClick={toggle}
      >
        {value ? <span className="text-zinc-800 dark:text-zinc-100">{formatCalendarDate(value)}</span> : <span className="text-zinc-400">{calendarPlaceholder(language, timeStyle)}</span>}
      </button>
      {open && (
        <div className={`absolute top-full z-30 mt-1 w-56 rounded-xl border border-zinc-200 bg-white p-2 shadow-lg dark:border-zinc-700 dark:bg-zinc-900 ${align === "right" ? "right-0" : "left-0"}`}>
          <div className="mb-1 flex items-center justify-between gap-1">
            <Step label="Previous month" onClick={() => shift(-1)}>
              ‹
            </Step>
            <span className="truncate text-xs font-semibold text-zinc-800 dark:text-zinc-100">{calendarMonthTitle(view.year, view.month, language, timeStyle)}</span>
            <Step label="Next month" onClick={() => shift(1)}>
              ›
            </Step>
          </div>
          <div className="grid grid-cols-7 text-center text-[10px] text-zinc-400">
            {labels.map((name, index) => (
              <span key={`${name}-${index}`} className="py-1">
                {name}
              </span>
            ))}
          </div>
          <div className="grid grid-cols-7">
            {cellsFor(view.year, view.month, weekStartsOn(timeStyle)).map((day, index) => {
              if (day == null) return <span key={index} />;
              const iso = `${view.year}-${pad(view.month + 1)}-${pad(day)}`;
              const chosen = iso === value;
              return (
                <button
                  key={iso}
                  type="button"
                  className={`mx-auto flex h-7 w-7 items-center justify-center rounded-full text-xs ${
                    chosen ? "bg-indigo-600 text-white" : iso === today ? "text-indigo-600 ring-1 ring-indigo-400 dark:text-indigo-300" : "hover:bg-zinc-100 dark:hover:bg-zinc-800"
                  }`}
                  onClick={() => pick(day)}
                >
                  {day}
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

function Step({ label, onClick, children }: { label: string; onClick: () => void; children: string }) {
  return (
    <button type="button" aria-label={label} className="rounded-md px-2 py-1 text-sm text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800" onClick={onClick}>
      {children}
    </button>
  );
}
