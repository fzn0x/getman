// Keeping the URL, its query parameters table and its :path parameters in
// step. The URL text is the source of truth; values are kept exactly as
// typed, `{{variables}}` included, and encoded only when sent.

import type { KeyValue, Vars } from "./types";

export interface SplitUrl {
  base: string;
  params: KeyValue[];
  hash: string;
}

/** Splits a URL into the part before `?`, its parameters and its `#hash`. */
export function splitUrl(url: string): SplitUrl {
  let rest = url;
  let hash = "";
  const hashAt = indexOutsideVars(rest, "#");
  if (hashAt >= 0) {
    hash = rest.slice(hashAt);
    rest = rest.slice(0, hashAt);
  }
  const queryAt = indexOutsideVars(rest, "?");
  if (queryAt < 0) return { base: rest, params: [], hash };
  const base = rest.slice(0, queryAt);
  const query = rest.slice(queryAt + 1);
  const params = query === "" ? [] : query.split("&").map(parsePair);
  return { base, params, hash };
}

function parsePair(pair: string): KeyValue {
  const eq = pair.indexOf("=");
  return eq < 0
    ? { name: pair, value: "", enabled: true }
    : { name: pair.slice(0, eq), value: pair.slice(eq + 1), enabled: true };
}

/** Finds a character that is not inside `{{...}}`. */
function indexOutsideVars(text: string, char: string): number {
  let depth = 0;
  for (let i = 0; i < text.length; i++) {
    if (text.startsWith("{{", i)) {
      depth++;
      i++;
    } else if (text.startsWith("}}", i) && depth > 0) {
      depth--;
      i++;
    } else if (depth === 0 && text[i] === char) {
      return i;
    }
  }
  return -1;
}

/** Joins a base and enabled parameters back into a URL. */
export function joinUrl(base: string, params: KeyValue[], hash = ""): string {
  const pairs = params
    .filter((p) => p.enabled !== false && (p.name !== "" || p.value !== ""))
    .map((p) => (p.value === "" ? p.name : `${p.name}=${p.value}`));
  return pairs.length ? `${base}?${pairs.join("&")}${hash}` : `${base}${hash}`;
}

/** The `:name` segments in the URL's path, in order. */
export function pathParamNames(url: string): string[] {
  const { base } = splitUrl(url);
  // Skip the scheme and authority, so `http://host:8080` is not a parameter.
  const afterScheme = base.replace(/^[a-z][\w+.-]*:\/\/[^/]*/i, "").replace(/^\{\{[^}]*\}\}/, "");
  const names: string[] = [];
  for (const segment of afterScheme.split("/")) {
    const match = /^:([A-Za-z_][\w-]*)$/.exec(segment);
    if (match?.[1] && !names.includes(match[1])) names.push(match[1]);
  }
  return names;
}

/** Path parameter values for the URL's current `:names`, keeping known values. */
export function syncPathParams(url: string, current: Vars): Vars {
  const next: Vars = {};
  for (const name of pathParamNames(url)) next[name] = current[name] ?? "";
  return next;
}

/** Host of a URL for display, ignoring variables it cannot resolve. */
export function displayHost(url: string): string {
  const match = /^[a-z][\w+.-]*:\/\/([^/?#]+)/i.exec(url);
  return match?.[1] ?? "";
}
