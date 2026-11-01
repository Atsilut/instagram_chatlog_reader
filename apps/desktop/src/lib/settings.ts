// SPDX-License-Identifier: AGPL-3.0-or-later

import { useEffect, useState } from "react";

/** Month and weekday names. */
export type AppLanguage = "en" | "ko" | "ja" | "es";
/** Date order and clock. US is month/day, Australia and the UK are day/month, Korea is year/month/day. */
export type TimeStyle = "us" | "au" | "uk" | "kr";

export interface DisplaySettings {
  language: AppLanguage;
  timeStyle: TimeStyle;
}

export const LANGUAGES: { id: AppLanguage; label: string }[] = [
  { id: "en", label: "English" },
  { id: "ko", label: "한국어" },
  { id: "ja", label: "日本語" },
  { id: "es", label: "Español" },
];

export const TIME_STYLES: { id: TimeStyle; label: string }[] = [
  { id: "us", label: "United States" },
  { id: "au", label: "Australia" },
  { id: "uk", label: "United Kingdom" },
  { id: "kr", label: "Korea" },
];

const LANGUAGE_KEY = "icv-language";
const TIME_KEY = "icv-time-style";

function detected(): DisplaySettings {
  const locale = navigator.language.toLowerCase();
  const language: AppLanguage = locale.startsWith("ko") ? "ko" : locale.startsWith("ja") ? "ja" : locale.startsWith("es") ? "es" : "en";
  const timeStyle: TimeStyle = locale.startsWith("ko") || locale.startsWith("ja") ? "kr" : locale.includes("-au") ? "au" : locale.includes("-gb") ? "uk" : "us";
  return { language, timeStyle };
}

function load(): DisplaySettings {
  const fallback = detected();
  const language = localStorage.getItem(LANGUAGE_KEY);
  const timeStyle = localStorage.getItem(TIME_KEY);
  return {
    language: LANGUAGES.some((l) => l.id === language) ? (language as AppLanguage) : fallback.language,
    timeStyle: TIME_STYLES.some((s) => s.id === timeStyle) ? (timeStyle as TimeStyle) : fallback.timeStyle,
  };
}

let current = load();
const listeners = new Set<() => void>();

export function getDisplaySettings(): DisplaySettings {
  return current;
}

export function setDisplaySettings(patch: Partial<DisplaySettings>) {
  current = { ...current, ...patch };
  localStorage.setItem(LANGUAGE_KEY, current.language);
  localStorage.setItem(TIME_KEY, current.timeStyle);
  listeners.forEach((listener) => listener());
}

/** Re-renders the component when the language or time style changes. */
export function useDisplaySettings(): DisplaySettings {
  const [, redraw] = useState(0);
  useEffect(() => {
    const listener = () => redraw((n) => n + 1);
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, []);
  return current;
}
