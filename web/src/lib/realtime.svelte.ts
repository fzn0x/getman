// Live connections: WebSocket, event stream and Socket.IO requests. The
// getman server holds the connections; this page opens one stream that
// carries the events of all of them, and keeps a capped log per request
// tab. Closing a tab disconnects it; closing the page ends the stream, and
// the server then closes everything the page opened.

import { api } from "./api";
import { toRequest } from "./draft";
import {
  appendCapped,
  MAX_LOG_ENTRIES,
  newComposer,
  outboundFrom,
  type Composer,
  type DirectionFilter,
  type LogEntry,
  type Outbound,
  type StreamEvent,
} from "./realtime";
import { contextPath, message, notify, tabs, ws, type RequestTab } from "./store.svelte";
import type { Header } from "./types";

/** What a request tab knows about its connection. Kept on the tab. */
export interface LiveState {
  /** Names this state in the logs, which are kept outside the tab. */
  key: string;
  /** The server's id for the connection; empty when there is none. */
  session: string;
  status: "idle" | "connecting" | "open" | "closing" | "closed";
  /** Grows when the log changes, for views to follow it. */
  version: number;
  /** Entries dropped from the start of the log to keep it small. */
  dropped: number;
  chars: number;
  request: { method: string; url: string; headers: Header[]; missing: string[] } | null;
  handshake: { status: number; headers: Header[]; protocol?: string; time_ms: number } | null;
  /** The last error, until the next connection. */
  error: string;
  closed: { code: number | null; reason: string; by_client: boolean } | null;
  sent: number;
  received: number;
  search: string;
  direction: DirectionFilter;
  /** The selected log entry's number, or -1. */
  selected: number;
  composer: Composer;
}

/** The logs, outside the reactive tabs so thousands of entries stay cheap. */
const logs = new Map<string, LogEntry[]>();
/** Open connections by the server's id. */
const sessions = new Map<string, LiveState>();
/** Events for connections whose id this page does not know yet. */
const early = new Map<string, StreamEvent[]>();
/** Events waiting to be shown, by connection. */
const waiting = new Map<string, StreamEvent[]>();

let nextKey = 0;
let nextSeq = 0;
let flushing = false;
let page: Promise<string> | null = null;
let watching = false;

/** The tab's live state, made on first use. */
export function live(tab: RequestTab): LiveState {
  if (!tab.live) {
    tab.live = {
      key: `live-${++nextKey}`,
      session: "",
      status: "idle",
      version: 0,
      dropped: 0,
      chars: 0,
      request: null,
      handshake: null,
      error: "",
      closed: null,
      sent: 0,
      received: 0,
      search: "",
      direction: "all",
      selected: -1,
      composer: newComposer(),
    };
  }
  return tab.live;
}

/** The log of a live state. Reading it in a derived value follows it. */
export function entries(state: LiveState): LogEntry[] {
  void state.version;
  return logs.get(state.key) ?? [];
}

export function clearLog(state: LiveState): void {
  logs.set(state.key, []);
  state.chars = 0;
  state.dropped = 0;
  state.selected = -1;
  state.version++;
}

/** Whether the tab's connection is open or on its way. */
export function connected(tab: RequestTab): boolean {
  const status = tab.live?.status;
  return status === "connecting" || status === "open" || status === "closing";
}

// ---- The page's stream -----------------------------------------------------------

function openPage(): Promise<string> {
  if (page) return page;
  page = new Promise<string>((resolve, reject) => {
    let ready = false;
    api
      .realtimeEvents((line) => {
        if (line.type === "ready") {
          ready = true;
          resolve(String(line.page));
        } else if (line.type !== "ping") {
          receive(line as StreamEvent);
        }
      })
      .then(
        () => lost("Getman ended the stream of events"),
        (error: unknown) => {
          if (!ready) reject(error);
          lost(message(error));
        },
      )
      .finally(() => {
        page = null;
      });
  });
  return page;
}

/** Marks every connection closed when the page's stream ends. */
function lost(reason: string): void {
  flush();
  for (const state of sessions.values()) {
    state.status = "closed";
    state.session = "";
    state.error = `Lost the connection to Getman: ${reason}. Is getman still running?`;
  }
  sessions.clear();
  early.clear();
}

function receive(event: StreamEvent): void {
  const id = event.session ?? "";
  if (!sessions.has(id)) {
    if (early.size > 64) early.clear();
    const list = early.get(id) ?? [];
    list.push(event);
    early.set(id, list);
    return;
  }
  const list = waiting.get(id) ?? [];
  list.push(event);
  // A page in the background may flush rarely; keep the queue bounded too.
  if (list.length > MAX_LOG_ENTRIES) list.splice(0, list.length - MAX_LOG_ENTRIES);
  waiting.set(id, list);
  if (!flushing) {
    flushing = true;
    setTimeout(flush, 50);
  }
}

/** Shows waiting events, many at a time, so a fast stream stays smooth. */
function flush(): void {
  flushing = false;
  for (const [id, events] of waiting) {
    const state = sessions.get(id);
    if (state) apply(id, state, events);
  }
  waiting.clear();
}

function apply(id: string, state: LiveState, events: StreamEvent[]): void {
  const added: LogEntry[] = [];
  for (const event of events) {
    switch (event.type) {
      case "connecting":
        state.request = {
          method: event.method,
          url: event.url,
          headers: event.headers,
          missing: event.missing,
        };
        break;
      case "open":
        state.status = "open";
        state.handshake = {
          status: event.status,
          headers: event.headers,
          protocol: event.protocol,
          time_ms: event.time_ms,
        };
        break;
      case "message":
        if (event.direction === "sent") state.sent++;
        else state.received++;
        break;
      case "error":
        state.error = event.message;
        break;
      case "closed":
        state.status = "closed";
        state.session = "";
        state.closed = { code: event.code, reason: event.reason, by_client: event.by_client };
        sessions.delete(id);
        break;
    }
    added.push({ seq: ++nextSeq, event });
  }
  const log = logs.get(state.key) ?? [];
  const result = appendCapped(log, added, state.chars);
  logs.set(state.key, log);
  state.chars = result.chars;
  state.dropped += result.dropped;
  state.version++;
}

/** Closes the connections of tabs that were closed. */
function watchTabs(): void {
  if (watching) return;
  watching = true;
  $effect.root(() => {
    $effect(() => {
      const open = new Set<string>();
      for (const tab of tabs.list) {
        if (tab.kind === "request" && tab.live) open.add(tab.live.key);
      }
      for (const [id, state] of sessions) {
        if (!open.has(state.key)) {
          sessions.delete(id);
          void api.realtimeClose(id).catch(() => {});
        }
      }
      for (const key of logs.keys()) if (!open.has(key)) logs.delete(key);
    });
  });
}

// ---- Actions -----------------------------------------------------------------------------

export async function connect(tab: RequestTab): Promise<void> {
  const state = live(tab);
  if (connected(tab)) return;
  watchTabs();
  state.status = "connecting";
  state.error = "";
  state.closed = null;
  state.handshake = null;
  state.request = null;
  state.sent = 0;
  state.received = 0;
  try {
    const pageId = await openPage();
    const { id } = await api.realtimeConnect({
      page: pageId,
      path: contextPath(tab),
      request: toRequest(tab.draft),
      environment: ws.environment || null,
    });
    if (state.status !== "connecting") {
      // Cancelled while the server was starting it.
      void api.realtimeClose(id).catch(() => {});
      early.delete(id);
      return;
    }
    state.session = id;
    sessions.set(id, state);
    const first = early.get(id);
    if (first) {
      early.delete(id);
      apply(id, state, first);
    }
  } catch (error) {
    state.status = "closed";
    state.session = "";
    state.error = message(error);
  }
}

export async function disconnect(tab: RequestTab): Promise<void> {
  const state = tab.live;
  if (!state || !connected(tab)) return;
  if (!state.session) {
    state.status = "closed";
    return;
  }
  state.status = "closing";
  try {
    await api.realtimeClose(state.session);
  } catch (error) {
    notify(message(error), "error");
  }
}

/** The connect button: connects, or disconnects an open connection. */
export function toggle(tab: RequestTab): Promise<void> {
  return connected(tab) ? disconnect(tab) : connect(tab);
}

/** Sends a message on the tab's connection. Returns whether it went. */
export async function sendLive(tab: RequestTab, outbound: Outbound): Promise<boolean> {
  const state = live(tab);
  if (state.status !== "open") {
    notify("Connect first, then send.", "error");
    return false;
  }
  try {
    await api.realtimeSend(state.session, outbound);
    return true;
  } catch (error) {
    notify(message(error), "error");
    return false;
  }
}

/** Sends what the message pane holds. */
export function sendComposer(tab: RequestTab): Promise<boolean> {
  const outbound = outboundFrom(tab.draft.kind, live(tab).composer);
  if (typeof outbound === "string") {
    notify(outbound, "error");
    return Promise.resolve(false);
  }
  return sendLive(tab, outbound);
}

/**
 * Turns an HTTP request whose response was an event stream into an event
 * stream request, and connects, so its events show as they arrive.
 */
export function watchAsStream(tab: RequestTab): Promise<void> {
  tab.draft.kind = "sse";
  return connect(tab);
}

/**
 * The send shortcut on a live request: connects, or sends the message
 * being written once connected.
 */
export async function primary(tab: RequestTab): Promise<void> {
  const state = live(tab);
  if (state.status === "open") {
    if (tab.draft.kind !== "sse") await sendComposer(tab);
  } else if (!connected(tab)) {
    await connect(tab);
  }
}
