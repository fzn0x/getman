// WebSocket, Server-Sent Events and Socket.IO requests: their parts of a
// draft, the events their connections report, and the log the UI keeps of
// them. Everything here is plain logic; the connections themselves live in
// realtime.svelte.ts.

import type {
  Draft,
  Header,
  LogEntry as ScriptLog,
  MessageFormat,
  Request,
  RequestKind,
  SavedEmit,
  SavedMessage,
  TestResult,
  Until,
  VarChange,
} from "./types";

/** Whether requests of this kind stay open, rather than being sent once. */
export function isLive(kind: RequestKind | undefined): boolean {
  return kind === "websocket" || kind === "sse" || kind === "socketio";
}

export const KIND_LABELS: Record<RequestKind, string> = {
  http: "HTTP",
  websocket: "WebSocket",
  sse: "Event stream",
  socketio: "Socket.IO",
  grpc: "gRPC",
};

/** The short mark shown where an HTTP request shows its method. */
export function kindMark(kind: RequestKind | undefined): string {
  switch (kind) {
    case "websocket":
      return "WS";
    case "sse":
      return "SSE";
    case "socketio":
      return "SIO";
    default:
      return "";
  }
}

// ---- The request line's method and kind picker ---------------------------------

/** The kinds offered beside the HTTP methods, as picker values. */
export const STREAM_CHOICES: { value: string; label: string }[] = [
  { value: "websocket", label: "WebSocket" },
  { value: "socketio", label: "Socket.IO" },
  { value: "sse:GET", label: "SSE" },
  { value: "sse:POST", label: "SSE POST" },
];

/** The picker value for a draft: its method, or its kind. */
export function pickerValue(kind: RequestKind, method: string): string {
  if (kind === "sse") return `sse:${method}`;
  if (kind === "websocket" || kind === "socketio") return kind;
  return method;
}

/** The label of a picker value that is not among the choices. */
export function pickerLabel(value: string): string {
  return value.startsWith("sse:") ? `SSE ${value.slice(4)}` : value;
}

/** What a picker value sets: the kind, and the method if it names one. */
export function fromPicker(value: string): { kind: RequestKind; method: string | null } {
  if (value === "websocket" || value === "socketio") return { kind: value, method: null };
  if (value.startsWith("sse:")) return { kind: "sse", method: value.slice(4) };
  return { kind: "http", method: value };
}

// ---- Drafts ------------------------------------------------------------------------

/** The realtime parts of a draft, complete for the editors. */
export function liveDraft(
  request: Request,
): Pick<Draft, "kind" | "websocket" | "sse" | "socketio" | "until"> {
  return {
    kind: request.kind ?? "http",
    websocket: {
      subprotocols: [...(request.websocket?.subprotocols ?? [])],
      messages: (request.websocket?.messages ?? []).map((m) => ({
        name: m.name ?? "",
        type: m.type ?? "text",
        content: m.content ?? "",
      })),
    },
    sse: { reconnect: request.sse?.reconnect ?? false },
    socketio: {
      namespace: request.socketio?.namespace ?? "",
      path: request.socketio?.path ?? "",
      transport: request.socketio?.transport ?? "auto",
      auth: request.socketio?.auth ?? "",
      events: [...(request.socketio?.events ?? [])],
      messages: (request.socketio?.messages ?? []).map((m) => ({
        name: m.name ?? "",
        event: m.event ?? "",
        args: m.args ?? "",
        ack: m.ack ?? false,
      })),
    },
    until: { messages: request.until?.messages ?? null, time_ms: request.until?.time_ms ?? null },
  };
}

/**
 * The realtime parts of a request to save: the kind unless it is HTTP, and
 * only the section of that kind, without empty values.
 */
export function liveRequest(draft: Draft): Partial<Request> {
  const out: Partial<Request> = {};
  if (draft.kind === "http") return out;
  out.kind = draft.kind;
  if (draft.kind === "websocket") {
    const subprotocols = draft.websocket.subprotocols.map((p) => p.trim()).filter(Boolean);
    const messages: SavedMessage[] = draft.websocket.messages.map((m) => ({
      ...(m.name.trim() ? { name: m.name.trim() } : {}),
      ...(m.type !== "text" ? { type: m.type } : {}),
      content: m.content,
    }));
    if (subprotocols.length || messages.length) {
      out.websocket = {
        ...(subprotocols.length ? { subprotocols } : {}),
        ...(messages.length ? { messages } : {}),
      };
    }
  } else if (draft.kind === "sse") {
    if (draft.sse.reconnect) out.sse = { reconnect: true };
  } else if (draft.kind === "socketio") {
    const s = draft.socketio;
    const events = s.events.map((e) => e.trim()).filter(Boolean);
    const messages: SavedEmit[] = s.messages.map((m) => ({
      ...(m.name.trim() ? { name: m.name.trim() } : {}),
      event: m.event.trim(),
      ...(m.args.trim() ? { args: m.args } : {}),
      ...(m.ack ? { ack: true } : {}),
    }));
    const socketio = {
      ...(s.namespace.trim() ? { namespace: s.namespace.trim() } : {}),
      ...(s.path.trim() ? { path: s.path.trim() } : {}),
      ...(s.transport !== "auto" ? { transport: s.transport } : {}),
      ...(s.auth.trim() ? { auth: s.auth } : {}),
      ...(events.length ? { events } : {}),
      ...(messages.length ? { messages } : {}),
    };
    if (Object.keys(socketio).length) out.socketio = socketio;
  }
  const messages = draft.until.messages;
  const time = draft.until.time_ms;
  const until: Until = {
    ...(typeof messages === "number" && Number.isInteger(messages) && messages >= 0
      ? { messages }
      : {}),
    ...(typeof time === "number" && time > 0 ? { time_ms: Math.round(time) } : {}),
  };
  if (Object.keys(until).length) out.until = until;
  return out;
}

/** The request editor's panes for a realtime kind. */
export function livePanes(kind: RequestKind): string[] {
  switch (kind) {
    case "websocket":
      return [
        "message",
        "params",
        "headers",
        "auth",
        "scripts",
        "checks",
        "variables",
        "settings",
        "docs",
      ];
    case "socketio":
      return [
        "message",
        "socketio",
        "params",
        "headers",
        "auth",
        "scripts",
        "checks",
        "variables",
        "settings",
        "docs",
      ];
    default:
      return [
        "params",
        "headers",
        "body",
        "auth",
        "scripts",
        "checks",
        "variables",
        "settings",
        "docs",
      ];
  }
}

/** Splits a list typed as text, on commas or lines. */
export function splitList(text: string): string[] {
  return text
    .split(/[\n,]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

// ---- What connections report -------------------------------------------------------

export interface StreamMessage {
  direction: "sent" | "received";
  /** Text as it is; Base64 when `binary`; Socket.IO arguments as a JSON array. */
  data: string;
  binary?: boolean;
  size: number;
  truncated?: boolean;
  name?: string;
  event?: string;
  id?: string;
  retry?: number;
  ack?: number;
}

export type StreamEvent = { at: number; session?: string } & (
  | { type: "connecting"; method: string; url: string; headers: Header[]; missing: string[] }
  | { type: "open"; status: number; headers: Header[]; protocol?: string; time_ms: number }
  | ({ type: "message" } & StreamMessage)
  | { type: "note" | "warning" | "error"; message: string }
  | { type: "closed"; code: number | null; reason: string; by_client: boolean }
  | ({ type: "result" } & ScriptResults)
);

/** What scripts, checks and captures made of a connection that ended. */
export interface ScriptResults {
  tests: TestResult[];
  checks: TestResult[];
  logs: ScriptLog[];
  script_errors: { origin: string; message: string }[];
  warnings: string[];
  changes: VarChange[];
}

/** A message to send on a connection. */
export type Outbound = { message: SavedMessage } | { emit: SavedEmit };

// ---- The log -------------------------------------------------------------------------

/** An event in a connection's log, numbered so the view can track it. */
export interface LogEntry {
  seq: number;
  event: StreamEvent;
}

/** The most entries a log keeps; older ones are dropped. */
export const MAX_LOG_ENTRIES = 2000;
/** The most message data a log keeps, in characters. */
export const MAX_LOG_CHARS = 8_000_000;

function weight(entry: LogEntry): number {
  return entry.event.type === "message" ? entry.event.data.length : 64;
}

/**
 * Adds entries to a log, dropping the oldest past the limits. Returns how
 * many were dropped and the new weight of the log.
 */
export function appendCapped(
  log: LogEntry[],
  added: LogEntry[],
  chars: number,
  limits = { entries: MAX_LOG_ENTRIES, chars: MAX_LOG_CHARS },
): { dropped: number; chars: number } {
  log.push(...added);
  let total = chars + added.reduce((sum, entry) => sum + weight(entry), 0);
  let drop = 0;
  while (drop < log.length - 1 && (log.length - drop > limits.entries || total > limits.chars)) {
    total -= weight(log[drop]!);
    drop++;
  }
  if (drop > 0) log.splice(0, drop);
  return { dropped: drop, chars: total };
}

export type DirectionFilter = "all" | "sent" | "received";

/** Whether a log entry matches the search text and direction filter. */
export function matches(entry: LogEntry, search: string, direction: DirectionFilter): boolean {
  const event = entry.event;
  if (direction !== "all" && (event.type !== "message" || event.direction !== direction)) {
    return false;
  }
  const needle = search.trim().toLowerCase();
  if (!needle) return true;
  return searchable(event).toLowerCase().includes(needle);
}

function searchable(event: StreamEvent): string {
  switch (event.type) {
    case "message":
      return `${event.event ?? ""} ${event.name ?? ""} ${event.id ?? ""} ${event.data}`;
    case "note":
    case "warning":
    case "error":
      return event.message;
    case "closed":
      return `closed ${event.code ?? ""} ${event.reason}`;
    case "connecting":
      return event.url;
    case "open":
      return `open ${event.status}`;
    case "result":
      return "";
  }
}

/** A message's data for reading: indented JSON when it is JSON. */
export function readable(message: StreamMessage): { text: string; json: boolean } {
  if (message.binary) return { text: message.data, json: false };
  const trimmed = message.data.trim();
  if (trimmed.startsWith("{") || trimmed.startsWith("[")) {
    try {
      return { text: JSON.stringify(JSON.parse(trimmed), null, 2), json: true };
    } catch {
      // Not JSON after all.
    }
  }
  return { text: message.data, json: false };
}

/** A message's data on one line, for the log. */
export function preview(message: StreamMessage, max = 240): string {
  const flat = message.data.replace(/\s+/g, " ");
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
}

/** The time of day of an event, to the millisecond. */
export function clock(at: number): string {
  const time = new Date(at);
  const pad = (n: number, width = 2) => String(n).padStart(width, "0");
  return `${pad(time.getHours())}:${pad(time.getMinutes())}:${pad(time.getSeconds())}.${pad(time.getMilliseconds(), 3)}`;
}

/** Why a connection closed, in words. */
export function closeText(code: number | null, reason: string, byClient: boolean): string {
  const who = byClient ? "Disconnected" : "The server closed the connection";
  const parts = [who];
  if (code !== null) parts.push(`(${code}${CLOSE_CODES[code] ? ` ${CLOSE_CODES[code]}` : ""})`);
  const text = parts.join(" ");
  return reason ? `${text}: ${reason}` : text;
}

const CLOSE_CODES: Record<number, string> = {
  1000: "normal",
  1001: "going away",
  1002: "protocol error",
  1003: "unsupported data",
  1007: "invalid data",
  1008: "policy violation",
  1009: "message too big",
  1011: "server error",
  1012: "restarting",
  1013: "try again later",
};

// ---- The composer ---------------------------------------------------------------------

/** What is being written in the message pane. */
export interface Composer {
  type: MessageFormat;
  content: string;
  event: string;
  args: string;
  ack: boolean;
}

export function newComposer(): Composer {
  return { type: "text", content: "", event: "", args: "", ack: false };
}

/** The composer as a message to send, or why it cannot be sent. */
export function outboundFrom(kind: RequestKind, composer: Composer): Outbound | string {
  if (kind === "socketio") {
    if (!composer.event.trim()) return "Name the event to emit.";
    return {
      emit: { event: composer.event.trim(), args: composer.args, ack: composer.ack },
    };
  }
  if (kind === "websocket") {
    if (composer.content === "" && composer.type !== "text") return "Write the message first.";
    return { message: { type: composer.type, content: composer.content } };
  }
  return "An event stream only receives; nothing can be sent on it.";
}
