// Sorting history into days and searching it.

import type { HistoryEntry } from "./types";

export interface Day {
  /** Midnight of the day, local time. */
  key: number;
  label: string;
  entries: HistoryEntry[];
}

function midnight(at: number): number {
  const date = new Date(at);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

/** A day's name: Today, Yesterday, or its date. */
export function dayLabel(key: number, now = Date.now()): string {
  const today = midnight(now);
  const yesterday = midnight(today - 12 * 3600 * 1000);
  if (key === today) return "Today";
  if (key === yesterday) return "Yesterday";
  const date = new Date(key);
  return date.toLocaleDateString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
    year: date.getFullYear() === new Date(now).getFullYear() ? undefined : "numeric",
  });
}

/** Entries, newest first, grouped by the local day they were sent. */
export function groupByDay(entries: HistoryEntry[], now = Date.now()): Day[] {
  const days: Day[] = [];
  for (const entry of [...entries].sort((a, b) => b.at - a.at)) {
    const key = midnight(entry.at);
    let day = days.at(-1);
    if (day?.key !== key) {
      day = { key, label: dayLabel(key, now), entries: [] };
      days.push(day);
    }
    day.entries.push(entry);
  }
  return days;
}

/** Whether an entry matches every word of a search, in any of its parts. */
export function matchesHistory(entry: HistoryEntry, query: string): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return true;
  const haystack = [
    entry.name,
    entry.method,
    entry.url,
    entry.path ?? "",
    entry.status === null ? "" : String(entry.status),
    entry.error ? `failed ${entry.error}` : "",
    entry.environment ?? "",
  ]
    .join(" ")
    .toLowerCase();
  return words.every((word) => haystack.includes(word));
}
