// SPDX-License-Identifier: AGPL-3.0-or-later

import type { ParticipantInfo, ThreadSummary } from "../api";

export const SEQ_PER_MINUTE = 10_000;

// Export times are wall-clock times without a timezone, stored as if UTC. Formatting
// in UTC shows them exactly as Instagram printed them.
const dayFmt = new Intl.DateTimeFormat(undefined, {
  weekday: "long",
  year: "numeric",
  month: "long",
  day: "numeric",
  timeZone: "UTC",
});
const timeFmt = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit", timeZone: "UTC" });
const shortDateFmt = new Intl.DateTimeFormat(undefined, { year: "2-digit", month: "numeric", day: "numeric", timeZone: "UTC" });

export const minuteDate = (tsMinute: number) => new Date(tsMinute * 60_000);
export const dayNumber = (tsMinute: number) => Math.floor(tsMinute / 1440);
export const formatDay = (tsMinute: number) => dayFmt.format(minuteDate(tsMinute));
export const formatTime = (tsMinute: number) => timeFmt.format(minuteDate(tsMinute));
export const formatShortDate = (tsMinute: number) => shortDateFmt.format(minuteDate(tsMinute));
export const sortKeyMinute = (sortKey: number) => Math.floor(sortKey / SEQ_PER_MINUTE);

const monthDayTimeFmt = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit", timeZone: "UTC" });
const fullStampFmt = new Intl.DateTimeFormat(undefined, {
  year: "numeric",
  month: "short",
  day: "numeric",
  hour: "numeric",
  minute: "2-digit",
  timeZone: "UTC",
});
const monthDayFmt = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", timeZone: "UTC" });

/** The local wall clock now, in the same "minutes as if UTC" form as message times. */
function nowMinute(): number {
  const now = new Date();
  return Math.floor((now.getTime() - now.getTimezoneOffset() * 60_000) / 60_000);
}

function sameYear(a: Date, b: Date) {
  return a.getUTCFullYear() === b.getUTCFullYear();
}

/** Message time: only the time for today, month and day for this year, the full date otherwise. */
export function formatStamp(tsMinute: number): string {
  const now = nowMinute();
  if (dayNumber(tsMinute) === dayNumber(now)) return formatTime(tsMinute);
  if (sameYear(minuteDate(tsMinute), minuteDate(now))) return monthDayTimeFmt.format(minuteDate(tsMinute));
  return fullStampFmt.format(minuteDate(tsMinute));
}

export const formatFullStamp = (tsMinute: number) => fullStampFmt.format(minuteDate(tsMinute));

/** Chat list date: the time for today, month and day for this year, a short full date otherwise. */
export function formatListDate(tsMinute: number): string {
  const now = nowMinute();
  if (dayNumber(tsMinute) === dayNumber(now)) return formatTime(tsMinute);
  if (sameYear(minuteDate(tsMinute), minuteDate(now))) return monthDayFmt.format(minuteDate(tsMinute));
  return formatShortDate(tsMinute);
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
