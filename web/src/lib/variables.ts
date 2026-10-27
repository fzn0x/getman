// `{{variables}}` in plain text fields: finding them, completing them,
// and describing them, for inputs that are not CodeMirror editors.

/** The same pattern the editors mark. */
const REFERENCE = /\{\{\s*([^{}\n"]+?)\s*\}\}/g;

export interface Segment {
  text: string;
  /** Set when the segment is a `{{reference}}`. */
  name?: string;
  /** Where the segment starts in the text. */
  from: number;
}

/** Text cut into plain runs and `{{references}}`. */
export function segments(text: string): Segment[] {
  const out: Segment[] = [];
  let at = 0;
  for (const match of text.matchAll(REFERENCE)) {
    if (match.index > at) out.push({ text: text.slice(at, match.index), from: at });
    out.push({ text: match[0], name: match[1], from: match.index });
    at = match.index + match[0].length;
  }
  if (at < text.length) out.push({ text: text.slice(at), from: at });
  return out;
}

/** The reference the caret or a pointer is in, if any. */
export function referenceAt(text: string, index: number): Segment | null {
  return (
    segments(text).find(
      (s) => s.name !== undefined && index > s.from && index < s.from + s.text.length,
    ) ?? null
  );
}

/** What is being completed after `{{` before the caret: where and what so far. */
export function completionAt(text: string, caret: number): { from: number; query: string } | null {
  const match = /\{\{\s*([\w$.-]*)$/.exec(text.slice(0, caret));
  if (!match) return null;
  const query = match[1] ?? "";
  return { from: caret - query.length, query };
}

/** Puts a completed name in place, closing the braces if they are not there. */
export function applyCompletion(
  text: string,
  from: number,
  caret: number,
  name: string,
): { text: string; caret: number } {
  const after = text.slice(caret);
  const closed = /^[\w$.-]*\s*\}\}/.exec(after);
  if (closed) {
    const rest = after.slice(closed[0].length);
    const next = `${text.slice(0, from)}${name}}}${rest}`;
    return { text: next, caret: from + name.length + 2 };
  }
  const next = `${text.slice(0, from)}${name}}}${after}`;
  return { text: next, caret: from + name.length + 2 };
}

/** Names that start with the query first, then names that contain it. */
export function rankNames<T extends { label: string }>(items: T[], query: string, limit = 50): T[] {
  const q = query.toLowerCase();
  if (!q) return items.slice(0, limit);
  const starts = items.filter((item) => item.label.toLowerCase().startsWith(q));
  const contains = items.filter(
    (item) => !item.label.toLowerCase().startsWith(q) && item.label.toLowerCase().includes(q),
  );
  return [...starts, ...contains].slice(0, limit);
}

/** A value for showing: secrets as dots until revealed, long ones cut. */
export function shownValue(value: string, secret: boolean, revealed: boolean): string {
  if (secret && !revealed) return "•".repeat(Math.min(12, Math.max(6, value.length)));
  return value.length > 300 ? `${value.slice(0, 299)}…` : value;
}
