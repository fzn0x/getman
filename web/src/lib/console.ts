// The console: a log of what the UI sent, kept in memory only. These are
// the pure parts: turning a send into a row, filtering, and which rows a
// scrolled list has to draw.

import { payloadText } from "./format";
import type { Execution, GrpcCall, Header, LogEntry, Outgoing, Payload } from "./types";

export type ConsoleKind = "http" | "grpc" | "realtime" | "graphql";

export const CONSOLE_KINDS: { kind: ConsoleKind; label: string }[] = [
  { kind: "http", label: "HTTP" },
  { kind: "graphql", label: "GraphQL" },
  { kind: "grpc", label: "gRPC" },
  { kind: "realtime", label: "Realtime" },
];

/** Rows kept; the oldest go first. */
export const MAX_ROWS = 2000;

/** Characters of a body kept in a row. */
export const BODY_CAP = 16_384;

export interface ConsoleRow {
  id: number;
  /** When it was sent, in milliseconds since 1970. */
  at: number;
  kind: ConsoleKind;
  name: string;
  method: string;
  url: string;
  /** The status code, or `null` when there was none. */
  status: number | null;
  statusText: string;
  /** Whether it went wrong: an error, a status of 400 or more, a failed script. */
  failed: boolean;
  timeMs: number | null;
  /** Bytes of headers and body received. */
  size: number | null;
  requestHeaders: Header[];
  requestBody: string;
  requestBodyCut: boolean;
  responseHeaders: Header[];
  responseBody: string;
  responseBodyCut: boolean;
  /** The script console's lines. */
  logs: LogEntry[];
  /** Warnings, script errors and the error that stopped the send. */
  problems: string[];
  /** For a realtime connection, how it went. */
  summary: string;
}

/** Cuts a body at [`BODY_CAP`] characters. */
export function capBody(text: string): { text: string; cut: boolean } {
  if (text.length <= BODY_CAP) return { text, cut: false };
  return { text: text.slice(0, BODY_CAP), cut: true };
}

function payload(data: Payload | undefined): string {
  return data ? payloadText(data) : "";
}

function outgoingBody(request: Outgoing | null): string {
  const body = request?.body;
  if (!body) return "";
  switch (body.type) {
    case "bytes":
      return payload(body.data);
    case "file":
      return `[the contents of ${body.path}]`;
    case "multipart":
      return body.parts
        .map((p) => `${p.name}: ${p.file ? `[file ${p.file}]` : (p.value ?? "")}`)
        .join("\n");
    default:
      return "";
  }
}

type Fresh = Omit<ConsoleRow, "id" | "at">;

function empty(kind: ConsoleKind, name: string): Fresh {
  return {
    kind,
    name,
    method: "",
    url: "",
    status: null,
    statusText: "",
    failed: false,
    timeMs: null,
    size: null,
    requestHeaders: [],
    requestBody: "",
    requestBodyCut: false,
    responseHeaders: [],
    responseBody: "",
    responseBodyCut: false,
    logs: [],
    problems: [],
    summary: "",
  };
}

function problemsOf(execution: Execution): string[] {
  const list: string[] = [];
  if (execution.error) list.push(execution.error);
  for (const error of execution.script_errors) list.push(`${error.origin}: ${error.message}`);
  for (const test of [...execution.tests, ...execution.checks]) {
    if (!test.passed) list.push(`Failed: ${test.name}${test.message ? ` (${test.message})` : ""}`);
  }
  list.push(...execution.warnings);
  if (execution.missing_variables.length > 0) {
    list.push(`Not defined: ${execution.missing_variables.map((v) => `{{${v}}}`).join(", ")}`);
  }
  return list;
}

/** The row for an HTTP or GraphQL request that was sent. */
export function rowFromExecution(execution: Execution, graphql: boolean): Fresh {
  const row = empty(graphql ? "graphql" : "http", execution.name);
  // Secret values stay hidden here; the response's Sent tab shows them on request.
  const request = execution.request_masked ?? execution.request;
  const { response } = execution;
  row.method = request?.method ?? "";
  row.url = request?.url ?? "";
  row.requestHeaders = request?.headers ?? [];
  const sent = capBody(outgoingBody(request));
  row.requestBody = sent.text;
  row.requestBodyCut = sent.cut;
  if (response) {
    row.status = response.status;
    row.statusText = response.status_text;
    row.timeMs = response.time_ms;
    row.size = response.size.headers + response.size.body;
    row.responseHeaders = response.headers;
    const received = capBody(payload(response.body));
    row.responseBody = received.text;
    row.responseBodyCut = received.cut || response.truncated;
  }
  row.logs = execution.logs;
  row.problems = problemsOf(execution);
  if (execution.skipped) row.problems.unshift("A pre-request script skipped this request.");
  if (execution.cancelled) row.problems.unshift("Cancelled.");
  row.failed =
    !!execution.error ||
    !!execution.cancelled ||
    execution.script_errors.length > 0 ||
    [...execution.tests, ...execution.checks].some((t) => !t.passed) ||
    (response?.status ?? 0) >= 400;
  return row;
}

function messageText(message: unknown): string {
  return typeof message === "string" ? message : JSON.stringify(message, null, 2);
}

/** The row for a gRPC call that ended. */
export function rowFromGrpc(execution: Execution, call: GrpcCall): Fresh {
  const row = empty("grpc", execution.name);
  row.method = call.method;
  row.url = call.address;
  row.statusText = call.status?.name ?? "";
  // gRPC's own status: 0 is OK.
  row.status = call.status?.code ?? null;
  row.timeMs = call.time_ms;
  row.requestHeaders = execution.request?.headers ?? [];
  row.responseHeaders = [...call.headers, ...call.trailers];
  const side = (direction: "sent" | "received") =>
    capBody(
      call.messages
        .filter((m) => m.direction === direction)
        .map((m) => messageText(m.message))
        .join("\n"),
    );
  const sent = side("sent");
  const received = side("received");
  row.requestBody = sent.text;
  row.requestBodyCut = sent.cut;
  row.responseBody = received.text;
  row.responseBodyCut = received.cut || (call.dropped_messages ?? 0) > 0;
  row.size = call.messages.reduce((sum, m) => sum + m.size, 0);
  row.logs = execution.logs;
  row.problems = problemsOf(execution);
  row.failed = !!execution.error || (call.status !== null && call.status.code !== 0);
  if (call.status && call.status.code !== 0) {
    row.problems.unshift(`${call.status.name}: ${call.status.message}`);
  }
  return row;
}

export interface RealtimeSummary {
  name: string;
  method: string;
  url: string;
  headers: Header[];
  status: number | null;
  responseHeaders: Header[];
  /** Milliseconds from connecting until it closed. */
  timeMs: number;
  sent: number;
  received: number;
  closed: { code: number | null; reason: string; by_client: boolean } | null;
  error: string;
}

/** The row for a WebSocket, event stream or Socket.IO connection that closed. */
export function rowFromRealtime(summary: RealtimeSummary): Fresh {
  const row = empty("realtime", summary.name);
  row.method = summary.method;
  row.url = summary.url;
  row.status = summary.status;
  row.timeMs = summary.timeMs;
  row.requestHeaders = summary.headers;
  row.responseHeaders = summary.responseHeaders;
  const { closed } = summary;
  const how = closed
    ? `${closed.by_client ? "Closed by you" : "Closed by the server"}${
        closed.code === null ? "" : ` with code ${closed.code}`
      }${closed.reason ? ` (${closed.reason})` : ""}`
    : "Closed";
  row.summary = `${how}. ${summary.sent} sent, ${summary.received} received.`;
  if (summary.error) row.problems.push(summary.error);
  row.failed = !!summary.error;
  return row;
}

export interface ConsoleFilter {
  text: string;
  errorsOnly: boolean;
  /** Kinds to hide. */
  hidden: ConsoleKind[];
}

export const NO_FILTER: ConsoleFilter = { text: "", errorsOnly: false, hidden: [] };

/** Whether any filter is set. */
export function filtering(filter: ConsoleFilter): boolean {
  return filter.text.trim() !== "" || filter.errorsOnly || filter.hidden.length > 0;
}

/** What a row is searched by: its line, its headers and its logs. */
function haystack(row: ConsoleRow): string {
  return [
    row.method,
    row.url,
    row.name,
    row.status ?? "",
    row.statusText,
    row.summary,
    ...row.problems,
    ...row.logs.map((l) => l.message),
  ]
    .join(" ")
    .toLowerCase();
}

/** The rows that pass the filter, newest last. Every word must match. */
export function filterRows(rows: ConsoleRow[], filter: ConsoleFilter): ConsoleRow[] {
  if (!filtering(filter)) return rows;
  const words = filter.text.toLowerCase().split(/\s+/).filter(Boolean);
  return rows.filter((row) => {
    if (filter.hidden.includes(row.kind)) return false;
    if (filter.errorsOnly && !row.failed) return false;
    if (words.length === 0) return true;
    const text = haystack(row);
    return words.every((word) => text.includes(word));
  });
}

/** Adds a row, dropping the oldest beyond [`MAX_ROWS`]. Returns how many went. */
export function pushRow(rows: ConsoleRow[], row: ConsoleRow): number {
  rows.push(row);
  const extra = rows.length - MAX_ROWS;
  if (extra > 0) rows.splice(0, extra);
  return Math.max(0, extra);
}

/**
 * The rows a scrolled list of equal-height rows must draw: from, to and
 * the space above and below, so thousands of rows cost a screenful.
 */
export function windowOf(
  count: number,
  rowHeight: number,
  scrollTop: number,
  viewport: number,
  overscan = 6,
): { from: number; to: number; before: number; after: number } {
  const first = Math.floor(scrollTop / rowHeight);
  const from = Math.max(0, Math.min(count, first - overscan));
  const to = Math.max(
    from,
    Math.min(count, Math.ceil((scrollTop + viewport) / rowHeight) + overscan),
  );
  return { from, to, before: from * rowHeight, after: (count - to) * rowHeight };
}

/** A row as plain text, for copying. */
export function rowText(row: ConsoleRow): string {
  const lines = [
    `${row.method} ${row.url}`.trim(),
    row.status === null ? "" : `Status: ${row.status} ${row.statusText}`.trim(),
  ];
  if (row.requestHeaders.length) {
    lines.push("", "Request headers:", ...row.requestHeaders.map((h) => `${h.name}: ${h.value}`));
  }
  if (row.requestBody) lines.push("", "Request body:", row.requestBody);
  if (row.responseHeaders.length) {
    lines.push("", "Response headers:", ...row.responseHeaders.map((h) => `${h.name}: ${h.value}`));
  }
  if (row.responseBody) lines.push("", "Response body:", row.responseBody);
  if (row.summary) lines.push("", row.summary);
  if (row.logs.length)
    lines.push("", "Script log:", ...row.logs.map((l) => `[${l.level}] ${l.message}`));
  if (row.problems.length) lines.push("", "Problems:", ...row.problems);
  return lines.filter((l, i) => l !== "" || i > 0).join("\n");
}
