// The console's state: the rows are kept in this page's memory and nowhere
// else, and are gone when the page is closed.

import {
  NO_FILTER,
  pushRow,
  rowFromExecution,
  rowFromGrpc,
  rowFromRealtime,
  type ConsoleFilter,
  type ConsoleKind,
  type ConsoleRow,
  type RealtimeSummary,
} from "./console";
import type { Execution } from "./types";

const HEIGHT_KEY = "getman.consoleHeight";

function savedHeight(): number {
  try {
    const height = Number(JSON.parse(localStorage.getItem(HEIGHT_KEY) ?? "260"));
    return Number.isFinite(height) ? Math.min(800, Math.max(120, height)) : 260;
  } catch {
    return 260;
  }
}

export const consoleView = $state({
  open: false,
  height: savedHeight(),
  /** Grows when a row is added or removed, for views to follow. */
  version: 0,
  filter: { ...NO_FILTER } as ConsoleFilter,
  /** The open row's id, or -1. */
  selected: -1,
});

/** The rows, outside the reactive state so thousands of them stay cheap. */
const rows: ConsoleRow[] = [];
let nextId = 0;

/** Every row, oldest first. Reading it in a derived value follows it. */
export function consoleRows(): ConsoleRow[] {
  void consoleView.version;
  return rows;
}

function add(fresh: Omit<ConsoleRow, "id" | "at">): void {
  pushRow(rows, { ...fresh, id: ++nextId, at: Date.now() });
  consoleView.version++;
}

/** Logs a request sent from the UI, with its response and script output. */
export function logExecution(
  execution: Execution,
  graphql = false,
  /** What to show when the request never got as far as being built. */
  draft?: { method: string; url: string },
): void {
  if (execution.grpc) {
    add(rowFromGrpc(execution, execution.grpc));
    return;
  }
  const row = rowFromExecution(execution, graphql);
  if (!row.url && draft) {
    row.method = draft.method;
    row.url = draft.url;
  }
  add(row);
}

/** Logs a WebSocket, event stream or Socket.IO connection that closed. */
export function logRealtime(summary: RealtimeSummary): void {
  add(rowFromRealtime(summary));
}

/** Logs a schema read for the GraphQL editor. */
export function logSchema(row: {
  name: string;
  url: string;
  status: number | null;
  timeMs: number | null;
  error?: string;
  cached?: boolean;
}): void {
  add({
    kind: "graphql",
    name: row.name,
    method: "POST",
    url: row.url,
    status: row.status,
    statusText: "",
    failed: !!row.error,
    timeMs: row.timeMs,
    size: null,
    requestHeaders: [],
    requestBody: "",
    requestBodyCut: false,
    responseHeaders: [],
    responseBody: "",
    responseBodyCut: false,
    logs: [],
    problems: row.error ? [row.error] : [],
    summary: row.error
      ? "Schema introspection failed."
      : row.cached
        ? "Schema read from memory."
        : "Schema introspection.",
  });
}

export function clearConsole(): void {
  rows.length = 0;
  consoleView.selected = -1;
  consoleView.version++;
}

export function toggleConsole(): void {
  consoleView.open = !consoleView.open;
}

export function setConsoleHeight(height: number): void {
  consoleView.height = Math.round(Math.min(800, Math.max(120, height)));
  try {
    localStorage.setItem(HEIGHT_KEY, JSON.stringify(consoleView.height));
  } catch {
    // The height then lasts until the page closes.
  }
}

export function toggleKind(kind: ConsoleKind): void {
  const hidden = consoleView.filter.hidden;
  consoleView.filter.hidden = hidden.includes(kind)
    ? hidden.filter((k) => k !== kind)
    : [...hidden, kind];
}

/** The number of rows that went wrong, for the status bar. */
export function failedCount(): number {
  void consoleView.version;
  return rows.reduce((n, row) => n + (row.failed ? 1 : 0), 0);
}
