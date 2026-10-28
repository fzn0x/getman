// Open tabs as they are remembered between visits, and the small list
// operations on them that do not need the store.

import type { Draft, Folder } from "./types";

/** A tab as remembered in the browser, with its unsaved draft if any. */
export type SavedTab =
  | {
      kind: "request";
      path: string | null;
      inheritPath: string | null;
      /** Name and method, for finding the file if it moved. */
      name: string;
      method: string;
      /** The draft, when it differs from the file or was never saved. */
      draft?: Draft;
      /** The saved state the draft was based on. */
      base?: string;
      requestPane: string;
      responsePane: string;
    }
  | {
      kind: "folder";
      path: string;
      name: string;
      draft?: Folder;
      base?: string;
      pane: string;
    }
  | { kind: "environments"; selected: string }
  | { kind: "runner"; target: string };

export interface SavedSession {
  version: 2;
  tabs: SavedTab[];
  /** Index of the active tab in `tabs`. */
  active: number;
}

/** Reads what was stored, including the list of paths older versions kept. */
export function readSession(raw: unknown): SavedSession {
  const empty: SavedSession = { version: 2, tabs: [], active: -1 };
  if (!raw || typeof raw !== "object") return empty;
  const value = raw as Partial<SavedSession> & { open?: unknown; active?: unknown };
  if (value.version === 2 && Array.isArray(value.tabs)) {
    return {
      version: 2,
      tabs: value.tabs.filter((tab) => tab && typeof tab === "object" && "kind" in tab),
      active: typeof value.active === "number" ? value.active : -1,
    };
  }
  if (Array.isArray(value.open)) {
    const tabs: SavedTab[] = [];
    for (const entry of value.open) {
      if (typeof entry !== "string") continue;
      const path = entry.slice(2);
      if (entry.startsWith("r:")) {
        tabs.push({
          kind: "request",
          path,
          inheritPath: null,
          name: "",
          method: "",
          requestPane: "params",
          responsePane: "body",
        });
      } else if (entry.startsWith("f:")) {
        tabs.push({ kind: "folder", path, name: "", pane: "variables" });
      } else if (entry === "e:") {
        tabs.push({ kind: "environments", selected: "" });
      }
    }
    const active = typeof value.active === "string" ? value.active : "";
    const at = value.open.indexOf(active);
    return { version: 2, tabs, active: at };
  }
  return empty;
}

interface RequestNode {
  path: string;
  name: string;
  method: string;
}

const baseName = (path: string) => path.slice(path.lastIndexOf("/") + 1);
const folderOf = (path: string) => {
  const at = path.lastIndexOf("/");
  return at < 0 ? "" : path.slice(0, at);
};

/**
 * Where a request whose file is gone most likely went: the one request in
 * the same folder with the same name and method (the file was renamed),
 * or the one request elsewhere with the same file name (it was moved).
 */
export function findMoved(
  saved: { path: string; name: string; method: string },
  requests: RequestNode[],
  taken: Set<string> = new Set(),
): string | null {
  const free = requests.filter((r) => !taken.has(r.path) && r.path !== saved.path);
  const unique = (list: RequestNode[]) => (list.length === 1 ? list[0]!.path : null);
  if (saved.name) {
    const renamed = unique(
      free.filter(
        (r) =>
          folderOf(r.path) === folderOf(saved.path) &&
          r.name === saved.name &&
          (!saved.method || r.method.toUpperCase() === saved.method.toUpperCase()),
      ),
    );
    if (renamed) return renamed;
  }
  return unique(free.filter((r) => baseName(r.path) === baseName(saved.path)));
}

/** Ids of the tabs a bulk close removes. */
export function closeTargets(
  ids: string[],
  id: string,
  mode: "others" | "right" | "all",
): string[] {
  const at = ids.indexOf(id);
  switch (mode) {
    case "all":
      return [...ids];
    case "others":
      return ids.filter((other) => other !== id);
    case "right":
      return at < 0 ? [] : ids.slice(at + 1);
  }
}

/** Moves the item at `from` so it ends up at index `to`. */
export function moveTo<T>(list: T[], from: number, to: number): T[] {
  if (from < 0 || from >= list.length) return list;
  const next = [...list];
  const [item] = next.splice(from, 1);
  next.splice(Math.max(0, Math.min(to, next.length)), 0, item!);
  return next;
}

/** Adds a closed tab to the stack that reopens them, keeping the last few. */
export function pushClosed(stack: SavedTab[], tab: SavedTab, limit = 20): SavedTab[] {
  return [...stack, tab].slice(-limit);
}
