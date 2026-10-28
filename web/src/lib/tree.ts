// Views of the workspace tree that change nothing on disk.

import type { Node } from "./types";

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** The tree sorted A to Z at every level, folders first. */
export function sortNodes(nodes: Node[]): Node[] {
  return [...nodes]
    .sort((a, b) =>
      a.kind !== b.kind ? (a.kind === "folder" ? -1 : 1) : collator.compare(a.name, b.name),
    )
    .map((node) =>
      node.kind === "folder" ? { ...node, children: sortNodes(node.children) } : node,
    );
}

/** The paths from `from` to `to` in a list of visible rows, both included. */
export function pathRange(visible: string[], from: string, to: string): string[] {
  const a = visible.indexOf(from);
  const b = visible.indexOf(to);
  if (b < 0) return [];
  if (a < 0) return [to];
  return visible.slice(Math.min(a, b), Math.max(a, b) + 1);
}

/** Folders an item can move into: not itself, nor anything inside it. */
export function moveTargets(folders: string[], moving: string[]): string[] {
  return folders.filter(
    (folder) => !moving.some((path) => folder === path || folder.startsWith(`${path}/`)),
  );
}
