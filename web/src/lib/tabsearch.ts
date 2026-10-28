// Searching the open and recently closed tabs.

import { rank } from "./fuzzy";
import type { SavedTab } from "./session";

export interface TabHit {
  /** Which list the hit came from. */
  state: "open" | "closed";
  /** The open tab's id, or the closed tab's place in the closed list. */
  id: string;
  title: string;
  /** The file's path, or what else says where the tab leads. */
  detail: string;
  /** The method of a request, for the tab's mark. */
  method: string;
  /** Whether the tab has unsaved changes. */
  dirty: boolean;
}

/** What a closed tab shows in the list. */
export function describeSaved(
  saved: SavedTab,
  workspaceName: string,
  nameOf: (path: string) => string,
): Pick<TabHit, "title" | "detail" | "method" | "dirty"> {
  switch (saved.kind) {
    case "request":
      return {
        title:
          saved.draft?.name || saved.name || saved.path?.split("/").pop() || "Untitled request",
        detail: saved.path ?? "Not saved yet",
        method: saved.draft?.method ?? saved.method,
        dirty: saved.draft !== undefined,
      };
    case "folder":
      return {
        title: saved.draft?.name || saved.name || (saved.path === "" ? workspaceName : saved.path),
        detail: saved.path === "" ? "Workspace settings" : saved.path,
        method: "",
        dirty: saved.draft !== undefined,
      };
    case "environments":
      return { title: "Environments", detail: "", method: "", dirty: false };
    case "runner":
      return {
        title: `Run ${saved.target === "" ? workspaceName : nameOf(saved.target)}`,
        detail: saved.target,
        method: "",
        dirty: false,
      };
  }
}

/**
 * The tabs that match the text, open ones first and each group best
 * match first. Without text, every open tab in tab order, then the
 * closed ones with the latest first.
 */
export function searchTabs(open: TabHit[], closed: TabHit[], query: string): TabHit[] {
  const newestFirst = [...closed].reverse();
  if (query.trim() === "") return [...open, ...newestFirst];
  const text = (hit: TabHit) => `${hit.title} ${hit.detail} ${hit.method}`;
  return [...rank(query, open, text), ...rank(query, newestFirst, text)];
}
