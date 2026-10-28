// The client for getman's local API. Every call carries the token the
// terminal printed; the server refuses calls without it.

import type {
  Auth,
  Cookie,
  DynamicVar,
  EnvironmentFiles,
  EnvironmentSummary,
  Example,
  Execution,
  Folder,
  HistoryEntry,
  ImportReport,
  Language,
  LocalState,
  MockServer,
  Node,
  Outgoing,
  Payload,
  ReportFormat,
  Request,
  ResolvedVar,
  RunEvent,
  RunListing,
  StoredRun,
  Token,
  WorkspaceInfo,
} from "./types";

const TOKEN_KEY = "getman.token";

/** Takes the token from `#token=...` in the address, once, and keeps it. */
export function adoptToken(): void {
  const hash = new URLSearchParams(location.hash.slice(1));
  const token = hash.get("token");
  if (token) {
    try {
      localStorage.setItem(TOKEN_KEY, token);
    } catch {
      memoryToken = token;
    }
    history.replaceState(null, "", location.pathname + location.search);
  }
}

let memoryToken = "";

function token(): string {
  try {
    return localStorage.getItem(TOKEN_KEY) ?? memoryToken;
  } catch {
    return memoryToken;
  }
}

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly code?: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

/** Called when the server says this page is not signed in. */
let onSignedOut: () => void = () => {};
export function whenSignedOut(handler: () => void): void {
  onSignedOut = handler;
}

async function call<T>(method: "GET" | "POST" | "PUT", path: string, body?: unknown): Promise<T> {
  let response: globalThis.Response;
  try {
    response = await fetch(`/api${path}`, {
      method,
      headers: {
        "x-getman-token": token(),
        ...(body === undefined ? {} : { "content-type": "application/json" }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new ApiError(
      "Getman is not answering. Is `getman` still running in your terminal?",
      0,
      "offline",
    );
  }
  const text = await response.text();
  let data: unknown = null;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    data = { error: text };
  }
  if (!response.ok) {
    const error = data as { error?: string; code?: string } | null;
    if (response.status === 401 && error?.code === "token") onSignedOut();
    throw new ApiError(error?.error ?? response.statusText, response.status, error?.code);
  }
  return data as T;
}

const post = <T>(path: string, body: unknown = {}) => call<T>("POST", path, body);

export interface SendArgs {
  path?: string | null;
  request: Request;
  environment?: string | null;
  persist?: boolean;
  /** Names the send, so `cancelSend` can stop it. */
  id?: string;
}

export interface RunArgs {
  target: string;
  /** Folders and requests to run in this order, instead of `target`. */
  targets?: string[];
  environment?: string | null;
  iterations?: number | null;
  data?: string | null;
  delay_ms?: number;
  bail?: boolean;
  persist?: boolean;
}

export const api = {
  workspace: () => call<WorkspaceInfo>("GET", "/workspace"),
  tree: () => call<Node[]>("GET", "/tree"),

  readRequest: (path: string) => post<Request>("/request/read", { path }),
  writeRequest: (path: string, request: Request) => post("/request/write", { path, request }),
  createRequest: (folder: string, name: string, request?: Request) =>
    post<{ path: string }>("/request/create", { folder, name, request }),

  readFolder: (path: string) => post<Folder>("/folder/read", { path }),
  writeFolder: (path: string, folder: Folder) => post("/folder/write", { path, folder }),
  createFolder: (parent: string, name: string) =>
    post<{ path: string }>("/folder/create", { parent, name }),

  rename: (path: string, name: string) => post<{ path: string }>("/item/rename", { path, name }),
  move: (path: string, to: string) => post<{ path: string }>("/item/move", { path, to }),
  duplicate: (path: string) => post<{ path: string }>("/item/duplicate", { path }),
  remove: (path: string) => post("/item/delete", { path }),
  order: (folder: string, order: string[]) => post("/item/order", { folder, order }),

  environments: () => call<EnvironmentSummary[]>("GET", "/environments"),
  createEnvironment: (name: string) => post<{ id: string }>("/environment/create", { name }),
  readEnvironment: (id: string) => post<EnvironmentFiles>("/environment/read", { id }),
  writeEnvironment: (id: string, environment: EnvironmentFiles) =>
    post("/environment/write", { id, environment }),
  renameEnvironment: (id: string, name: string) =>
    post<{ id: string }>("/environment/rename", { id, name }),
  deleteEnvironment: (id: string) => post("/environment/delete", { id }),
  globals: () => call<LocalState>("GET", "/globals"),
  writeGlobals: (state: LocalState) => call("PUT", "/globals", state),

  send: (args: SendArgs) => post<Execution>("/send", args),
  cancelSend: (id: string) => post("/send/cancel", { id }),
  prepare: (args: SendArgs) => post<{ request: Outgoing; missing: string[] }>("/prepare", args),
  codegen: (args: SendArgs & { language: string }) =>
    post<{ code: string; missing: string[] }>("/codegen", args),
  languages: () => call<Language[]>("GET", "/languages"),
  variables: (args: SendArgs) => post<ResolvedVar[]>("/variables", args),
  dynamic: () => call<DynamicVar[]>("GET", "/dynamic"),
  oauthToken: (args: { path?: string | null; auth: Auth; environment?: string | null }) =>
    post<Token>("/oauth2/token", args),

  importText: (text: string, folder = "", root = false) =>
    post<ImportReport>("/import", { text, folder, root }),
  parseCurl: (text: string) => post<Request>("/parse-curl", { text }),

  cookies: () => call<Cookie[]>("GET", "/cookies"),
  /** Adds a cookie, or edits `previous` into it. */
  setCookie: (cookie: Cookie, previous?: Pick<Cookie, "domain" | "path" | "name">) =>
    post("/cookies/set", { cookie, previous }),
  deleteCookie: (cookie: Pick<Cookie, "domain" | "path" | "name">) =>
    post("/cookies/delete", cookie),
  clearCookies: () => post("/cookies/clear"),

  history: () => call<HistoryEntry[]>("GET", "/history"),
  clearHistory: () => post("/history/clear"),

  cancelRun: (id: string) => post("/run/cancel", { id }),

  runs: () => call<RunListing[]>("GET", "/runs"),
  readRun: (id: string) => post<StoredRun>("/runs/read", { id }),
  deleteRun: (id: string) => post("/runs/delete", { id }),

  /** A past run's report as a file, with the name the server suggests. */
  async runReport(id: string, format: ReportFormat): Promise<{ blob: Blob; name: string }> {
    let response: globalThis.Response;
    try {
      response = await fetch("/api/runs/report", {
        method: "POST",
        headers: { "x-getman-token": token(), "content-type": "application/json" },
        body: JSON.stringify({ id, format }),
      });
    } catch {
      throw new ApiError("Getman is not answering.", 0, "offline");
    }
    if (!response.ok) {
      const data = (await response.json().catch(() => ({}))) as { error?: string; code?: string };
      if (response.status === 401 && data.code === "token") onSignedOut();
      throw new ApiError(data.error ?? response.statusText, response.status);
    }
    const disposition = response.headers.get("content-disposition") ?? "";
    const name = /filename="([^"]+)"/.exec(disposition)?.[1] ?? `getman-run.${format}`;
    return { blob: await response.blob(), name };
  },

  examples: (path: string) => post<Example[]>("/examples", { path }),
  saveExample: (path: string, example: Example, body: Payload) =>
    post<Example[]>("/example/save", { path, example, body }),
  exampleBody: (path: string, name: string) =>
    post<{ body: Payload }>("/example/body", { path, name }),
  renameExample: (path: string, from: string, to: string) =>
    post<Example[]>("/example/rename", { path, from, to }),
  deleteExample: (path: string, name: string) => post<Example[]>("/example/delete", { path, name }),

  mocks: () => call<MockServer[]>("GET", "/mocks"),
  startMock: (folder: string, port: number | null = null, delayMs = 0) =>
    post<MockServer>("/mock/start", { folder, port, delay_ms: delayMs }),
  stopMock: (folder: string) => post("/mock/stop", { folder }),

  /** Starts a run; events arrive as they happen. Resolves when it ends. */
  async run(
    args: RunArgs,
    onEvent: (event: RunEvent) => void,
    onStart: (id: string) => void,
  ): Promise<void> {
    let response: globalThis.Response;
    try {
      response = await fetch("/api/run", {
        method: "POST",
        headers: { "x-getman-token": token(), "content-type": "application/json" },
        body: JSON.stringify(args),
      });
    } catch {
      throw new ApiError("Getman is not answering.", 0, "offline");
    }
    if (!response.ok || !response.body) {
      const data = (await response.json().catch(() => ({}))) as { error?: string; code?: string };
      if (response.status === 401 && data.code === "token") onSignedOut();
      throw new ApiError(data.error ?? response.statusText, response.status);
    }
    onStart(response.headers.get("x-run-id") ?? "");
    const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
    let buffer = "";
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      buffer += value;
      let newline: number;
      while ((newline = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, newline).trim();
        buffer = buffer.slice(newline + 1);
        if (line) onEvent(JSON.parse(line) as RunEvent);
      }
    }
  },
};
