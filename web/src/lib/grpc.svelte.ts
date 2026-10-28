// What gRPC tabs hold beside their draft: the schema they call into and
// the live call, if one runs. The functions here talk to the server.

import { api } from "./api";
import { logExecution } from "./console.svelte";
import { newGrpcDraft, toRequest } from "./draft";
import { clientStreams } from "./grpc";
import {
  contextPath,
  message,
  notify,
  openRequest,
  parentOf,
  promptDialog,
  refreshHistory,
  refreshTree,
  refreshVariables,
  setExpanded,
  signals,
  ws,
  type RequestTab,
} from "./store.svelte";
import type { CallType, Execution, GrpcService, Header, StreamMessage } from "./types";

export interface LiveCall {
  id: string;
  callType: CallType | null;
  headers: Header[];
  messages: StreamMessage[];
  /** The client's side of the stream was ended. */
  ended: boolean;
}

export interface GrpcTabState {
  services: GrpcService[];
  /** What the schema was loaded for, to tell when it is stale. */
  loadedFor: string;
  loading: boolean;
  error: string;
  call: LiveCall | null;
}

/** `.proto` files in the workspace, for choosing a schema. */
export const protos = $state({ files: [] as string[], loaded: false });

const states = new WeakMap<RequestTab, GrpcTabState>();

function fresh(): GrpcTabState {
  const state = $state<GrpcTabState>({
    services: [],
    loadedFor: "",
    loading: false,
    error: "",
    call: null,
  });
  return state;
}

/** The gRPC state of a tab, made on first use. */
export function grpcState(tab: RequestTab): GrpcTabState {
  let state = states.get(tab);
  if (!state) {
    state = fresh();
    states.set(tab, state);
  }
  return state;
}

export async function loadProtos(): Promise<void> {
  try {
    protos.files = (await api.grpcProtos()).files;
    protos.loaded = true;
  } catch (error) {
    notify(message(error), "error");
  }
}

/** What a schema depends on: its files, or the server it reflects. */
export function schemaKey(tab: RequestTab): string {
  const { proto, tls } = tab.draft.grpc;
  return proto.trim()
    ? `proto:${proto.trim()}`
    : `reflect:${tab.draft.url.trim()}|${tls}|${ws.environment}`;
}

/** Loads the services the request can call. `refresh` asks the server again. */
export async function loadSchema(tab: RequestTab, refresh = false): Promise<void> {
  const state = grpcState(tab);
  const key = schemaKey(tab);
  if (!tab.draft.grpc.proto.trim() && !tab.draft.url.trim()) {
    state.services = [];
    state.error = "";
    state.loadedFor = key;
    return;
  }
  state.loading = true;
  try {
    const { services } = await api.grpcSchema({
      path: contextPath(tab),
      request: toRequest(tab.draft),
      environment: ws.environment || null,
      refresh,
    });
    if (schemaKey(tab) !== key) return;
    state.services = services;
    state.error = "";
  } catch (error) {
    if (schemaKey(tab) !== key) return;
    state.services = [];
    state.error = message(error);
  } finally {
    state.loadedFor = key;
    state.loading = false;
  }
}

function failed(tab: RequestTab, error: string): Execution {
  return {
    path: tab.path,
    name: tab.draft.name,
    request: null,
    response: null,
    error,
    tests: [],
    checks: [],
    logs: [],
    script_errors: [],
    changes: [],
    missing_variables: [],
    warnings: [],
    skipped: false,
    next_request: null,
  };
}

/** Starts the call; it runs until the server ends it or it is cancelled. */
export async function invoke(tab: RequestTab): Promise<void> {
  const state = grpcState(tab);
  if (state.call || tab.sending) return;
  tab.sending = true;
  tab.execution = null;
  tab.responsePane = "messages";
  state.call = { id: "", callType: null, headers: [], messages: [], ended: false };
  try {
    await api.grpcInvoke(
      {
        path: contextPath(tab),
        request: toRequest(tab.draft),
        environment: ws.environment || null,
      },
      (event) => {
        const call = state.call;
        switch (event.event) {
          case "started":
            if (call) call.callType = event.call_type;
            break;
          case "headers":
            if (call) call.headers = event.headers;
            break;
          case "message":
            call?.messages.push(event.message);
            break;
          case "done": {
            const execution = event.execution;
            tab.execution = execution;
            const failing =
              execution.script_errors.length > 0 ||
              [...execution.tests, ...execution.checks].some((t) => !t.passed);
            if (failing && execution.response) tab.responsePane = "tests";
            if (execution.changes.length > 0) void refreshVariables();
            break;
          }
        }
      },
      (id) => {
        if (state.call) state.call.id = id;
      },
    );
    if (!tab.execution) tab.execution = failed(tab, "The call ended without an answer.");
    logExecution(tab.execution, false, { method: tab.draft.method, url: tab.draft.url });
    void refreshHistory();
  } catch (error) {
    tab.execution = failed(tab, message(error));
    logExecution(tab.execution, false, { method: tab.draft.method, url: tab.draft.url });
  } finally {
    tab.sending = false;
    state.call = null;
  }
}

/** Sends the messages in the editor on the open stream. */
export async function sendMessages(tab: RequestTab): Promise<void> {
  const call = grpcState(tab).call;
  if (!call?.id || call.ended) return;
  try {
    await api.grpcSend(call.id, tab.draft.grpc.message);
  } catch (error) {
    notify(message(error), "error");
  }
}

/** Ends the client's side of the stream; the server may still answer. */
export async function endStream(tab: RequestTab): Promise<void> {
  const call = grpcState(tab).call;
  if (!call?.id || call.ended) return;
  call.ended = true;
  try {
    await api.grpcEnd(call.id);
  } catch (error) {
    notify(message(error), "error");
  }
}

export async function cancelCall(tab: RequestTab): Promise<void> {
  const call = grpcState(tab).call;
  if (call?.id) await api.grpcCancel(call.id).catch(() => {});
}

/** Whether the tab has a stream open to send messages on. */
export function streamOpen(tab: RequestTab): boolean {
  const call = grpcState(tab).call;
  return !!call && clientStreams(call.callType) && !call.ended;
}

/** The main action: send on an open stream, or start the call. */
export async function grpcPrimary(tab: RequestTab): Promise<void> {
  if (streamOpen(tab)) await sendMessages(tab);
  else if (!grpcState(tab).call) await invoke(tab);
}

/** Writes a request for each method of a service, next to this request. */
export async function generateRequests(tab: RequestTab, service: string): Promise<void> {
  const path = contextPath(tab);
  const folder = path ? parentOf(path) : "";
  try {
    const created = await api.grpcGenerate({
      path,
      request: toRequest(tab.draft),
      environment: ws.environment || null,
      service,
      folder,
    });
    setExpanded(created.folder, true);
    await refreshTree();
    notify(
      `Created ${created.requests} ${created.requests === 1 ? "request" : "requests"} in ${created.folder}/`,
      "ok",
    );
  } catch (error) {
    notify(message(error), "error");
  }
}

/** Asks for a name, then creates a gRPC request in a folder and opens it. */
export function newGrpcRequestIn(folder: string): void {
  promptDialog({
    title: "New gRPC request",
    label: "Request name",
    confirm: "Create request",
    onSubmit: async (name) => {
      try {
        const { path } = await api.createRequest(folder, name, toRequest(newGrpcDraft(name)));
        if (folder) setExpanded(folder, true);
        await refreshTree();
        await openRequest(path);
        signals.focusUrl++;
      } catch (error) {
        notify(message(error), "error");
      }
    },
  });
}
