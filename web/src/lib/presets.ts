// Named sets of headers added to a request in one go.

import type { HeaderPreset, KeyValue } from "./types";

/** Presets every workspace has. People's own live in .getman/. */
export const BUILT_IN_PRESETS: HeaderPreset[] = [
  {
    name: "JSON",
    headers: [
      { name: "Content-Type", value: "application/json" },
      { name: "Accept", value: "application/json" },
    ],
  },
  {
    name: "Form",
    headers: [{ name: "Content-Type", value: "application/x-www-form-urlencoded" }],
  },
  {
    name: "XML",
    headers: [
      { name: "Content-Type", value: "application/xml" },
      { name: "Accept", value: "application/xml" },
    ],
  },
  {
    name: "No cache",
    headers: [
      { name: "Cache-Control", value: "no-cache" },
      { name: "Pragma", value: "no-cache" },
    ],
  },
];

/**
 * Adds a preset's headers to rows: a header already there, in any case,
 * takes the preset's value and is switched on; others are added at the end.
 */
export function applyPreset(rows: KeyValue[], headers: KeyValue[]): KeyValue[] {
  const next = rows.map((row) => ({ ...row }));
  for (const header of headers) {
    const name = header.name.trim().toLowerCase();
    const existing = next.find((row) => row.name.trim().toLowerCase() === name);
    if (existing) {
      existing.value = header.value;
      existing.enabled = true;
    } else {
      next.push({ name: header.name, value: header.value, enabled: true });
    }
  }
  return next;
}

/** The headers worth keeping in a preset: named and switched on. */
export function presetHeaders(rows: KeyValue[]): KeyValue[] {
  return rows
    .filter((row) => row.name.trim() !== "" && row.enabled !== false)
    .map((row) => ({ name: row.name.trim(), value: row.value }));
}

/** The list with a preset added, replacing one of the same name. */
export function withPreset(list: HeaderPreset[], preset: HeaderPreset): HeaderPreset[] {
  const name = preset.name.trim().toLowerCase();
  const at = list.findIndex((p) => p.name.trim().toLowerCase() === name);
  return at < 0 ? [...list, preset] : list.map((p, i) => (i === at ? preset : p));
}
