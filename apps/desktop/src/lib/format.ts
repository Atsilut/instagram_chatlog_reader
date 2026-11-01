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
    month: "numeric",
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
  const month = parts.month.padStart(2, "0");
  const day = parts.day.padStart(2, "0");
  const { year } = parts;
  if (style === "kr" && language === "ko") {
    return withYear ? `${year}년 ${Number(month)}월 ${Number(day)}일` : `${Number(month)}월 ${Number(day)}일`;
  }
  if (style === "kr" && language === "ja") {
    return withYear ? `${year}年${Number(month)}月${Number(day)}日` : `${Number(month)}月${Number(day)}日`;
  }
  if (style === "kr") return withYear ? `${year}. ${month}. ${day}.` : `${month}. ${day}.`;
  if (style === "us") return withYear ? `${month}/${day}/${year}` : `${month}/${day}`;
  return withYear ? `${day}/${month}/${year}` : `${day}/${month}`;
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

/** A chosen calendar day, written with the current language and time style. `iso` is `YYYY-MM-DD`. */
export function formatCalendarDate(iso: string): string {
  const [y, m, d] = iso.split("-").map(Number);
  if (!y || !m || !d) return iso;
  const { language, timeStyle } = getDisplaySettings();
  const parts = dateParts(new Date(Date.UTC(y, m - 1, d, 12)), language, timeStyle);
  return dateText(parts, timeStyle, language, true);
}

/** Empty date-field hint in the same order the time style uses. */
export function calendarPlaceholder(language: AppLanguage, timeStyle: TimeStyle): string {
  if (timeStyle === "kr") {
    if (language === "ko") return "YYYY년 M월 D일";
    if (language === "ja") return "YYYY年M月D日";
    if (language === "es") return "AAAA. mes D";
    return "YYYY. Mon D";
  }
  if (timeStyle === "us") {
    if (language === "ko") return "M월 D, YYYY";
    if (language === "ja") return "M月 D, YYYY";
    if (language === "es") return "mes d, aaaa";
    return "Mon D, YYYY";
  }
  if (language === "ko") return "D M월 YYYY";
  if (language === "ja") return "D M月 YYYY";
  if (language === "es") return "d mes aaaa";
  return "D Mon YYYY";
}

/** Sunday is 0. The US week starts on Sunday; the others start on Monday. */
export const weekStartsOn = (timeStyle: TimeStyle) => (timeStyle === "us" ? 0 : 1);

export function weekdayLabels(language: AppLanguage, timeStyle: TimeStyle): string[] {
  const fmt = new Intl.DateTimeFormat(LOCALE[language], { weekday: "short", timeZone: "UTC" });
  const sundayFirst = Array.from({ length: 7 }, (_, i) => fmt.format(new Date(Date.UTC(2023, 0, 1 + i))));
  const start = weekStartsOn(timeStyle);
  return [...sundayFirst.slice(start), ...sundayFirst.slice(0, start)];
}

/** Month heading for the date field. Korea puts the year first. */
export function calendarMonthTitle(year: number, monthIndex: number, language: AppLanguage, timeStyle: TimeStyle): string {
  const date = new Date(Date.UTC(year, monthIndex, 1));
  const month = new Intl.DateTimeFormat(LOCALE[language], { month: "long", timeZone: "UTC" }).format(date);
  const yearText = new Intl.DateTimeFormat(LOCALE[language], { year: "numeric", timeZone: "UTC" }).format(date);
  if (timeStyle === "kr") {
    if (language === "ja") return `${yearText}${month}`;
    if (language === "ko") return `${yearText} ${month}`;
    return `${yearText}. ${month}`;
  }
  return `${month} ${yearText}`;
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
