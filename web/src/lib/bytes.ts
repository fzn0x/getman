// Looking at bytes: hex dumps, links in text, and picking values out of a
// response body.

const HEX = Array.from({ length: 256 }, (_, i) => i.toString(16).padStart(2, "0"));

/** Bytes per row of a hex dump. */
export const HEX_ROW = 16;

export interface HexDump {
  offsets: string;
  hex: string;
  ascii: string;
}

/**
 * A classic hex dump of `bytes[start, end)`, one row of 16 bytes per line
 * in three columns: the offset, the bytes in hex (a gap after eight), and
 * the bytes as ASCII with `.` for anything not printable.
 */
export function hexDump(bytes: Uint8Array, start = 0, end = bytes.length): HexDump {
  const offsets: string[] = [];
  const hex: string[] = [];
  const ascii: string[] = [];
  const stop = Math.min(end, bytes.length);
  for (let row = start; row < stop; row += HEX_ROW) {
    offsets.push(row.toString(16).padStart(8, "0"));
    let line = "";
    let text = "";
    for (let i = 0; i < HEX_ROW; i++) {
      const at = row + i;
      if (i === 8) line += " ";
      if (at < stop) {
        const byte = bytes[at]!;
        line += `${HEX[byte]} `;
        text += byte >= 0x20 && byte < 0x7f ? String.fromCharCode(byte) : ".";
      } else {
        line += "   ";
      }
    }
    hex.push(line.trimEnd());
    ascii.push(text);
  }
  return { offsets: offsets.join("\n"), hex: hex.join("\n"), ascii: ascii.join("\n") };
}

/** URLs in text, without trailing punctuation that is likely not theirs. */
export const URL_PATTERN = /\bhttps?:\/\/[^\s"'<>`{}|\\^]+/g;

export function trimUrl(url: string): string {
  let end = url.length;
  for (;;) {
    const last = url[end - 1];
    if (last === undefined) break;
    if (".,;:!?".includes(last)) {
      end--;
    } else if (")]".includes(last)) {
      // Keep a closing bracket only if the URL opened one.
      const open = last === ")" ? "(" : "[";
      const slice = url.slice(0, end);
      if (slice.split(open).length > slice.split(last).length - 1) break;
      end--;
    } else {
      break;
    }
  }
  return url.slice(0, end);
}

export function findLinks(text: string): { from: number; to: number; url: string }[] {
  const links: { from: number; to: number; url: string }[] = [];
  for (const match of text.matchAll(URL_PATTERN)) {
    const url = trimUrl(match[0]);
    if (url.length > "https://".length) {
      links.push({ from: match.index, to: match.index + url.length, url });
    }
  }
  return links;
}

/** A value picked out of a body: trimmed, and a JSON string unquoted. */
export function pickedValue(text: string): string {
  const trimmed = text.trim();
  if (trimmed.length >= 2 && trimmed.startsWith('"') && trimmed.endsWith('"')) {
    try {
      const value: unknown = JSON.parse(trimmed);
      if (typeof value === "string") return value;
    } catch {
      // Not a JSON string; keep the quotes.
    }
  }
  return trimmed;
}

/** A variable name for a value, from the JSON key on its line if any. */
export function suggestName(line: string): string {
  const key = /^\s*"([^"\\]{1,64})"\s*:/.exec(line)?.[1] ?? "";
  return /^[\w.$-]+$/.test(key) ? key : "";
}
