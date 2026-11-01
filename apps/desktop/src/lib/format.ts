// SPDX-License-Identifier: AGPL-3.0-or-later

import type { ParticipantInfo, ThreadSummary } from "../api";
import { getDisplaySettings, type AppLanguage, type TimeStyle } from "./settings";

export const SEQ_PER_MINUTE = 10_000;

// Export times are wall-clock times without a timezone, stored as if UTC. Formatting
// in UTC shows them exactly as Instagram printed them.
const LOCALE: Record<AppLanguage, string> = { en: "en", ko: "ko", ja: "ja", es: "es" };

export const minuteDate = (tsMinute: number) => new Date(tsMinute * 60_000);
export const dayNumber = (tsMinute: number) => Math.floor(tsMinute / 1440);
export const sortKeyMinute = (sortKey: number) => Math.floor(sortKey / SEQ_PER_MINUTE);

type DateParts = Record<string, string>;

function dateParts(date: Date, language: AppLanguage, timeStyle: TimeStyle): DateParts {
  const parts: DateParts = {};
  for (const part of new Intl.DateTimeFormat(LOCALE[language], {
    weekday: "long",
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
    hourCycle: timeStyle === "uk" ? "h23" : "h12",
    timeZone: "UTC",
  }).formatToParts(date)) {
    if (part.type !== "literal") parts[part.type] = part.value;
  }
  return parts;
}

function clock(parts: DateParts): string {
  const time = `${parts.hour}:${parts.minute}`;
  if (!parts.dayPeriod) return time;
  return /^[A-Za-z]/.test(parts.dayPeriod) ? `${time} ${parts.dayPeriod}` : `${parts.dayPeriod} ${time}`;
}

function dateText(parts: DateParts, style: TimeStyle, language: AppLanguage, withYear: boolean): string {
  const { month, day, year } = parts;
  if (style === "kr") {
    if (language === "ko") return withYear ? `${year}년 ${month} ${day}일` : `${month} ${day}일`;
    if (language === "ja") return withYear ? `${year}年${month}${day}日` : `${month}${day}日`;
    return withYear ? `${year}. ${month} ${day}` : `${month} ${day}`;
  }
  if (style === "us") return withYear ? `${month} ${day}, ${year}` : `${month} ${day}`;
  return withYear ? `${day} ${month} ${year}` : `${day} ${month}`;
}

/** The local wall clock now, in the same "minutes as if UTC" form as message times. */
function nowMinute(): number {
  const now = new Date();
  return Math.floor((now.getTime() - now.getTimezoneOffset() * 60_000) / 60_000);
}

function sameYear(a: Date, b: Date) {
  return a.getUTCFullYear() === b.getUTCFullYear();
}

function rendered(tsMinute: number, withDate: boolean, withYear: boolean, withWeekday: boolean): string {
  const { language, timeStyle } = getDisplaySettings();
  const parts = dateParts(minuteDate(tsMinute), language, timeStyle);
  const time = clock(parts);
  if (!withDate) return time;
  const date = dateText(parts, timeStyle, language, withYear);
  const dated = withWeekday && parts.weekday ? `${parts.weekday}, ${date}` : date;
  return `${dated}, ${time}`;
}

/** Message time: only the time for today, month and day for this year, the full date otherwise. */
export function formatStamp(tsMinute: number): string {
  const now = nowMinute();
  if (dayNumber(tsMinute) === dayNumber(now)) return rendered(tsMinute, false, false, false);
  return rendered(tsMinute, true, !sameYear(minuteDate(tsMinute), minuteDate(now)), false);
}

export const formatFullStamp = (tsMinute: number) => rendered(tsMinute, true, true, false);

/** Day separator: weekday and the full date, without the time. */
export function formatDay(tsMinute: number): string {
  const { language, timeStyle } = getDisplaySettings();
  const parts = dateParts(minuteDate(tsMinute), language, timeStyle);
  const date = dateText(parts, timeStyle, language, true);
  return parts.weekday ? `${parts.weekday}, ${date}` : date;
}

/** Chat list date: the time for today, month and day for this year, the full date otherwise. */
export function formatListDate(tsMinute: number): string {
  const now = nowMinute();
  if (dayNumber(tsMinute) === dayNumber(now)) return rendered(tsMinute, false, false, false);
  const { language, timeStyle } = getDisplaySettings();
  return dateText(dateParts(minuteDate(tsMinute), language, timeStyle), timeStyle, language, !sameYear(minuteDate(tsMinute), minuteDate(now)));
}

/** A fixed afternoon, so the settings screen can show what a style looks like. */
export function formatSample(language: AppLanguage, timeStyle: TimeStyle): string {
  const parts = dateParts(new Date(Date.UTC(2025, 6, 18, 14, 15)), language, timeStyle);
  return `${dateText(parts, timeStyle, language, true)}, ${clock(parts)}`;
}

export function personName(p: ParticipantInfo | undefined): string {
  if (!p) return "Unknown";
  return p.alias ?? p.displayName;
}

/** One-to-one chats are titled with the other person (or their alias). */
export function threadTitle(t: ThreadSummary): string {
  if (t.kind === "direct") {
    const other = t.participants.find((p) => !p.isOwner);
    if (other) return personName(other);
  }
  return t.title;
}

export function initials(name: string): string {
  const letters = Array.from(name.trim()).filter((c) => /\p{L}|\p{N}/u.test(c));
  return (letters[0] ?? "?").toUpperCase();
}

const PALETTE = ["bg-rose-500", "bg-amber-500", "bg-emerald-500", "bg-sky-500", "bg-violet-500", "bg-fuchsia-500", "bg-teal-500"];

export function avatarColor(seed: number): string {
  return PALETTE[Math.abs(seed) % PALETTE.length];
}

export function previewText(t: ThreadSummary): string {
  switch (t.lastKind) {
    case "media":
      return "Sent an attachment";
    case "share":
      return "Shared a post";
    case "like":
      return "Liked a message";
    case "reaction_notice":
      return t.lastText ?? "Reacted to a message";
    case "empty":
      return "Unsupported message";
    default:
      return (t.lastText ?? "").replace(/\s+/g, " ");
  }
}

export function hostOf(url: string): string {
  try {
    return new URL(url).host.replace(/^www\./, "");
  } catch {
    return url;
  }
}
