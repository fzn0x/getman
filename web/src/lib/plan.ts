// The list of requests a run will send: which are ticked, and in what
// order. Kept apart from the store so it can be tested on its own.

import type { Node } from "./types";

export interface PlanItem {
  path: string;
  name: string;
  method: string;
  include: boolean;
}

/** Every request a folder or request covers, in run order, all ticked.
 * Requests whose files cannot be read are left out, as the runner does. */
export function planFor(tree: Node[], target: string): PlanItem[] {
  const out: PlanItem[] = [];
  const collect = (nodes: Node[]): void => {
    for (const node of nodes) {
      if (node.kind === "folder") collect(node.children);
      else if (!node.error) {
        out.push({ path: node.path, name: node.name, method: node.method, include: true });
      }
    }
  };
  const find = (nodes: Node[]): Node | undefined => {
    for (const node of nodes) {
      if (node.path === target) return node;
      if (node.kind === "folder") {
        const found = find(node.children);
        if (found) return found;
      }
    }
    return undefined;
  };
  if (target === "") collect(tree);
  else {
    const node = find(tree);
    if (node) collect([node]);
  }
  return out;
}

/** Keeps the ticks and order of an earlier plan for the requests still
 * there; new requests join in their place, ticked. */
export function refreshPlan(previous: PlanItem[], fresh: PlanItem[]): PlanItem[] {
  if (previous.length === 0) return fresh;
  const known = new Map(previous.map((item) => [item.path, item]));
  const kept = previous
    .filter((item) => fresh.some((f) => f.path === item.path))
    .map((item) => {
      const now = fresh.find((f) => f.path === item.path)!;
      return { ...now, include: item.include };
    });
  for (const [at, item] of fresh.entries()) {
    if (!known.has(item.path)) kept.splice(Math.min(at, kept.length), 0, item);
  }
  return kept;
}

/** The list with one item moved from one place to another. */
export function moveTo<T>(list: T[], from: number, to: number): T[] {
  if (from === to || from < 0 || from >= list.length) return list;
  const next = [...list];
  const [item] = next.splice(from, 1);
  next.splice(Math.max(0, Math.min(to, next.length)), 0, item!);
  return next;
}

/** What to ask the server to run: the target itself while the plan is
 * untouched, else the ticked requests in their order. */
export function runTargets(
  target: string,
  plan: PlanItem[],
  natural: PlanItem[],
): { target: string; targets?: string[] } {
  const untouched =
    plan.length === natural.length &&
    plan.every((item, i) => item.include && item.path === natural[i]?.path);
  if (untouched) return { target };
  return { target, targets: plan.filter((item) => item.include).map((item) => item.path) };
}
