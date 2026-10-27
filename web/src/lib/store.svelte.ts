// Everything the UI holds: the workspace, open tabs, preferences and the
// dialogs. Components read and change it directly; the functions here do
// what involves the server.

import { api, ApiError } from "./api";
import { clone, fingerprint, newDraft, toDraft, toRequest } from "./draft";
import type {
  Draft,
  DynamicVar,
  EnvironmentSummary,
  Execution,
  Folder,
  HistoryEntry,
  Language,
  Node,
  Request,
  ResolvedVar,
  RunEvent,
  RunItem,
  RunSummary,
} from "./types";

// ---- Preferences, kept in this browser only --------------------------------

function load<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(`getman.${key}`);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

export function store(key: string, value: unknown): void {
  try {
    localStorage.setItem(`getman.${key}`, JSON.stringify(value));
  } catch {
    // Private windows may refuse storage; preferences then last a session.
  }
}

export type Theme = "system" | "light" | "dark";
export type Layout = "columns" | "rows";

export const prefs = $state({
  theme: (() => {
    try {
      return (localStorage.getItem("getman.theme") as Theme | null) ?? "system";
    } catch {
      return "system" as Theme;
    }
  })(),
  layout: load<Layout>("layout", "columns"),
  sidebarWidth: load<number>("sidebarWidth", 280),
  sidebarOpen: load<boolean>("sidebarOpen", true),
  split: load<number>("split", 0.5),
});

export function setTheme(theme: Theme): void {
  prefs.theme = theme;
  try {
    localStorage.setItem("getman.theme", theme);
  } catch {
    // ignore
  }
  if (theme === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
}

// ---- Workspace ------------------------------------------------------------------

export const ws = $state({
  ready: false,
  signedOut: false,
  failure: "",
  name: "",
  root: "",
  version: "",
  tree: [] as Node[],
  environments: [] as EnvironmentSummary[],
  environment: "",
  expanded: {} as Record<string, boolean>,
  history: [] as HistoryEntry[],
  dynamic: [] as DynamicVar[],
  languages: [] as Language[],
  /** Variables the active request sees, for highlighting and completion. */
  variables: [] as ResolvedVar[],
  variablesReady: false,
});

const scoped = (key: string) => `${key}:${ws.root}`;

export function workspaceKey(key: string): string {
  return scoped(key);
}

export async function loadWorkspace(): Promise<void> {
  try {
    const info = await api.workspace();
    ws.name = info.name;
    ws.root = info.root;
    ws.version = info.version;
    ws.tree = info.tree;
    ws.environments = info.environments;
    ws.expanded = load(scoped("expanded"), {});
    const saved = load<string>(scoped("environment"), "");
    ws.environment = info.environments.some((e) => e.id === saved) ? saved : "";
    ws.ready = true;
    ws.failure = "";
    void Promise.all([api.dynamic(), api.languages(), api.history()]).then(
      ([dynamic, languages, history]) => {
        ws.dynamic = dynamic;
        ws.languages = languages;
        ws.history = history;
      },
    );
    await restoreTabs();
  } catch (error) {
    if (!(error instanceof ApiError && error.code === "token")) ws.failure = message(error);
  }
}

export async function refreshTree(): Promise<void> {
  try {
    const [tree, environments] = await Promise.all([api.tree(), api.environments()]);
    ws.tree = tree;
    ws.environments = environments;
    if (ws.environment && !environments.some((e) => e.id === ws.environment)) {
      selectEnvironment("");
    }
  } catch (error) {
    notify(message(error), "error");
  }
}

export async function refreshHistory(): Promise<void> {
  try {
    ws.history = await api.history();
  } catch {
    // History is a convenience; ignore failures.
  }
}

export function selectEnvironment(id: string): void {
  ws.environment = id;
  store(scoped("environment"), id);
  void refreshVariables();
}

export function setExpanded(path: string, open: boolean): void {
  ws.expanded[path] = open;
  store(scoped("expanded"), ws.expanded);
}

/** Every request in the tree, in order. */
export function allRequests(nodes: Node[] = ws.tree): Extract<Node, { kind: "request" }>[] {
  return nodes.flatMap((node) => (node.kind === "folder" ? allRequests(node.children) : [node]));
}

export function allFolders(nodes: Node[] = ws.tree): Extract<Node, { kind: "folder" }>[] {
  return nodes.flatMap((node) =>
    node.kind === "folder" ? [node, ...allFolders(node.children)] : [],
  );
}

export function findNode(path: string, nodes: Node[] = ws.tree): Node | undefined {
  for (const node of nodes) {
    if (node.path === path) return node;
    if (node.kind === "folder") {
      const found = findNode(path, node.children);
      if (found) return found;
    }
  }
  return undefined;
}

export function parentOf(path: string): string {
  const at = path.lastIndexOf("/");
  return at < 0 ? "" : path.slice(0, at);
}

/** Names offered when completing `{{`. */
export function completionNames(): { label: string; detail?: string }[] {
  return [
    ...ws.variables.map((v) => ({ label: v.name, detail: `${v.scope} · ${preview(v.value)}` })),
    ...ws.dynamic.map((d) => ({ label: d.name, detail: d.description })),
  ];
}

function preview(value: string): string {
  return value.length > 32 ? `${value.slice(0, 31)}…` : value;
}

// ---- Tabs --------------------------------------------------------------------------

export interface RequestTab {
  kind: "request";
  id: string;
  /** Where the request is saved; `null` until it is. */
  path: string | null;
  /** Folder whose settings an unsaved request inherits. */
  inheritPath: string | null;
  draft: Draft;
  saved: string;
  execution: Execution | null;
  sending: boolean;
  requestPane: string;
  responsePane: string;
}

export interface FolderTab {
  kind: "folder";
  id: string;
  path: string;
  draft: Folder;
  saved: string;
  pane: string;
}

export interface EnvironmentsTab {
  kind: "environments";
  id: "environments";
  selected: string;
}

export interface RunnerTab {
  kind: "runner";
  id: string;
  target: string;
  environment: string;
  iterations: number | null;
  delay: number;
  bail: boolean;
  persist: boolean;
  data: string;
  dataName: string;
  running: boolean;
  runId: string;
  items: RunItem[];
  iterationCount: number;
  results: { iteration: number; index: number; execution: Execution }[];
  summary: RunSummary | null;
  error: string;
  selected: number;
}

export type Tab = RequestTab | FolderTab | EnvironmentsTab | RunnerTab;

export const tabs = $state({ list: [] as Tab[], active: "" });

let untitled = 0;

export function activeTab(): Tab | undefined {
  return tabs.list.find((tab) => tab.id === tabs.active);
}

export function isDirty(tab: Tab): boolean {
  if (tab.kind === "request") return fingerprint(tab.draft) !== tab.saved;
  if (tab.kind === "folder") return JSON.stringify(tab.draft) !== tab.saved;
  return false;
}

export function tabTitle(tab: Tab): string {
  switch (tab.kind) {
    case "request":
      return tab.draft.name || "Untitled request";
    case "folder":
      return tab.draft.name || (tab.path === "" ? ws.name : tab.path);
    case "environments":
      return "Environments";
    case "runner":
      return `Run ${tab.target === "" ? ws.name : (findNode(tab.target)?.name ?? tab.target)}`;
  }
}

function focusTab(id: string): void {
  tabs.active = id;
  persistTabs();
  void refreshVariables();
}

function addTab(tab: Tab): void {
  const at = tabs.list.findIndex((t) => t.id === tabs.active);
  tabs.list.splice(at < 0 ? tabs.list.length : at + 1, 0, tab);
  focusTab(tab.id);
}

export function selectTab(id: string): void {
  if (tabs.list.some((tab) => tab.id === id)) focusTab(id);
}

export function cycleTab(step: number): void {
  if (tabs.list.length === 0) return;
  const at = tabs.list.findIndex((tab) => tab.id === tabs.active);
  const next = (at + step + tabs.list.length) % tabs.list.length;
  focusTab(tabs.list[next]!.id);
}

export function closeTab(id: string, force = false): void {
  const at = tabs.list.findIndex((tab) => tab.id === id);
  if (at < 0) return;
  const tab = tabs.list[at]!;
  if (!force && isDirty(tab)) {
    confirmDialog({
      title: `Close ${tabTitle(tab)}?`,
      body: "It has changes that are not saved.",
      confirm: "Close without saving",
      danger: true,
      onConfirm: () => closeTab(id, true),
    });
    return;
  }
  tabs.list.splice(at, 1);
  if (tabs.active === id) {
    const next = tabs.list[Math.min(at, tabs.list.length - 1)];
    tabs.active = next?.id ?? "";
  }
  persistTabs();
}

function persistTabs(): void {
  if (!ws.root) return;
  const open = tabs.list
    .map((tab) =>
      tab.kind === "request" && tab.path
        ? `r:${tab.path}`
        : tab.kind === "folder"
          ? `f:${tab.path}`
          : tab.kind === "environments"
            ? "e:"
            : "",
    )
    .filter(Boolean);
  store(scoped("tabs"), { open, active: tabs.active });
}

async function restoreTabs(): Promise<void> {
  const saved = load<{ open: string[]; active: string }>(scoped("tabs"), { open: [], active: "" });
  for (const entry of saved.open) {
    const kind = entry.slice(0, 2);
    const path = entry.slice(2);
    try {
      if (kind === "r:") {
        const request = await api.readRequest(path);
        tabs.list.push(requestTab(path, request));
      } else if (kind === "f:") {
        const folder = await api.readFolder(path);
        tabs.list.push(folderTab(path, folder));
      } else if (kind === "e:") {
        tabs.list.push({ kind: "environments", id: "environments", selected: "" });
      }
    } catch {
      // The file is gone; skip it.
    }
  }
  tabs.active = tabs.list.some((t) => t.id === saved.active)
    ? saved.active
    : (tabs.list[0]?.id ?? "");
  void refreshVariables();
}

function requestTab(
  path: string | null,
  request: Request,
  inheritPath: string | null = null,
): RequestTab {
  const draft = toDraft(request);
  return {
    kind: "request",
    id: path ? `r:${path}` : `new:${++untitled}`,
    path,
    inheritPath,
    draft,
    saved: path ? fingerprint(draft) : "",
    execution: null,
    sending: false,
    requestPane: "params",
    responsePane: "body",
  };
}

function folderTab(path: string, folder: Folder): FolderTab {
  const draft = normalizeFolder(folder);
  return {
    kind: "folder",
    id: `f:${path}`,
    path,
    draft,
    saved: JSON.stringify(draft),
    pane: "variables",
  };
}

function normalizeFolder(folder: Folder): Folder {
  return {
    name: folder.name ?? "",
    description: folder.description ?? "",
    variables: { ...(folder.variables ?? {}) },
    headers: (folder.headers ?? []).map((h) => ({ ...h, enabled: h.enabled !== false })),
    auth: clone(folder.auth ?? { type: "inherit" }),
    scripts: {
      pre_request: folder.scripts?.pre_request ?? "",
      post_response: folder.scripts?.post_response ?? "",
    },
    settings: { ...(folder.settings ?? {}) },
    order: [...(folder.order ?? [])],
  };
}

export async function openRequest(path: string): Promise<void> {
  const id = `r:${path}`;
  if (tabs.list.some((tab) => tab.id === id)) {
    focusTab(id);
    return;
  }
  try {
    const request = await api.readRequest(path);
    addTab(requestTab(path, request));
  } catch (error) {
    notify(message(error), "error");
  }
}

export function newRequest(seed?: Request, inheritPath: string | null = null): void {
  const tab = requestTab(null, seed ?? toRequest(newDraft()), inheritPath);
  addTab(tab);
}

export async function openFolder(path: string): Promise<void> {
  const id = `f:${path}`;
  if (tabs.list.some((tab) => tab.id === id)) {
    focusTab(id);
    return;
  }
  try {
    addTab(folderTab(path, await api.readFolder(path)));
  } catch (error) {
    notify(message(error), "error");
  }
}

export function openEnvironments(selected = ""): void {
  const existing = tabs.list.find((tab) => tab.kind === "environments") as
    EnvironmentsTab | undefined;
  if (existing) {
    if (selected) existing.selected = selected;
    focusTab(existing.id);
    return;
  }
  addTab({ kind: "environments", id: "environments", selected });
}

export function openRunner(target: string): void {
  const id = `run:${target}`;
  if (tabs.list.some((tab) => tab.id === id)) {
    focusTab(id);
    return;
  }
  addTab({
    kind: "runner",
    id,
    target,
    environment: ws.environment,
    iterations: null,
    delay: 0,
    bail: false,
    persist: false,
    data: "",
    dataName: "",
    running: false,
    runId: "",
    items: [],
    iterationCount: 0,
    results: [],
    summary: null,
    error: "",
    selected: -1,
  });
}

/** Path whose folder settings a request tab inherits. */
export function contextPath(tab: RequestTab): string | null {
  return tab.path ?? tab.inheritPath;
}

export async function refreshVariables(): Promise<void> {
  const tab = activeTab();
  if (!tab || tab.kind !== "request") return;
  if (!ws.ready) return;
  try {
    ws.variables = await api.variables({
      path: contextPath(tab),
      request: toRequest(tab.draft),
      environment: ws.environment || null,
    });
    ws.variablesReady = true;
  } catch {
    // Highlighting falls back to marking nothing as missing.
  }
}

// ---- Saving and sending ---------------------------------------------------------------

export async function save(tab: Tab | undefined = activeTab()): Promise<void> {
  if (!tab) return;
  if (tab.kind === "request") {
    if (!tab.path) {
      saveAsDialog(tab);
      return;
    }
    try {
      const request = toRequest(tab.draft);
      await api.writeRequest(tab.path, request);
      tab.saved = fingerprint(tab.draft);
      const node = findNode(tab.path);
      if (
        node?.kind === "request" &&
        (node.name !== tab.draft.name || node.method !== tab.draft.method)
      ) {
        if (node.name !== tab.draft.name) {
          const renamed = await api.rename(tab.path, tab.draft.name || "Untitled request");
          retargetTabs(tab.path, renamed.path);
        }
        await refreshTree();
      }
      notify("Saved", "ok");
    } catch (error) {
      notify(message(error), "error");
    }
  } else if (tab.kind === "folder") {
    try {
      await api.writeFolder(tab.path, clone(tab.draft));
      tab.saved = JSON.stringify(tab.draft);
      await refreshTree();
      if (tab.path === "") ws.name = tab.draft.name || ws.name;
      notify("Saved", "ok");
    } catch (error) {
      notify(message(error), "error");
    }
  }
}

export async function saveAs(tab: RequestTab, folder: string, name: string): Promise<void> {
  try {
    tab.draft.name = name;
    const { path } = await api.createRequest(folder, name, toRequest(tab.draft));
    tab.path = path;
    tab.id = `r:${path}`;
    tabs.active = tab.id;
    tab.saved = fingerprint(tab.draft);
    if (folder) setExpanded(folder, true);
    await refreshTree();
    persistTabs();
    notify(`Saved as ${path}`, "ok");
  } catch (error) {
    notify(message(error), "error");
  }
}

export async function send(tab: RequestTab): Promise<void> {
  if (tab.sending) return;
  tab.sending = true;
  try {
    const execution = await api.send({
      path: contextPath(tab),
      request: toRequest(tab.draft),
      environment: ws.environment || null,
    });
    tab.execution = execution;
    const failing =
      execution.script_errors.length > 0 ||
      [...execution.tests, ...execution.checks].some((t) => !t.passed);
    if (failing && execution.response) tab.responsePane = "tests";
    else if (
      tab.responsePane === "tests" &&
      !failing &&
      execution.tests.length + execution.checks.length === 0
    ) {
      tab.responsePane = "body";
    }
    if (execution.changes.length > 0) void refreshVariables();
    void refreshHistory();
  } catch (error) {
    tab.execution = {
      path: tab.path,
      name: tab.draft.name,
      request: null,
      response: null,
      error: message(error),
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
  } finally {
    tab.sending = false;
  }
}

/** Points tabs at an item's new path after a rename or move. */
export function retargetTabs(from: string, to: string): void {
  for (const tab of tabs.list) {
    if (tab.kind === "request" && tab.path) {
      if (tab.path === from || tab.path.startsWith(`${from}/`)) {
        const was = tab.id;
        tab.path = to + tab.path.slice(from.length);
        tab.id = `r:${tab.path}`;
        if (tabs.active === was) tabs.active = tab.id;
      }
    } else if (tab.kind === "folder" && tab.path !== "") {
      if (tab.path === from || tab.path.startsWith(`${from}/`)) {
        const was = tab.id;
        tab.path = to + tab.path.slice(from.length);
        tab.id = `f:${tab.path}`;
        if (tabs.active === was) tabs.active = tab.id;
      }
    }
  }
  persistTabs();
}

/** Closes tabs of an item that no longer exists. */
export function dropTabs(path: string): void {
  tabs.list = tabs.list.filter((tab) => {
    const own = tab.kind === "request" ? tab.path : tab.kind === "folder" ? tab.path : null;
    return !(own && (own === path || own.startsWith(`${path}/`)));
  });
  if (!tabs.list.some((t) => t.id === tabs.active)) tabs.active = tabs.list[0]?.id ?? "";
  persistTabs();
}

// ---- Runs ------------------------------------------------------------------------------

export async function startRun(tab: RunnerTab): Promise<void> {
  if (tab.running) return;
  tab.running = true;
  tab.results = [];
  tab.summary = null;
  tab.error = "";
  tab.items = [];
  tab.selected = -1;
  try {
    await api.run(
      {
        target: tab.target,
        environment: tab.environment || null,
        iterations: tab.iterations,
        data: tab.data || null,
        delay_ms: tab.delay,
        bail: tab.bail,
        persist: tab.persist,
      },
      (event: RunEvent) => {
        switch (event.event) {
          case "start":
            tab.items = event.requests;
            tab.iterationCount = event.iterations;
            break;
          case "request":
            tab.results.push({
              iteration: event.iteration,
              index: event.index,
              execution: event.execution,
            });
            break;
          case "finish":
            tab.summary = event.summary;
            break;
          case "error":
            tab.error = event.error;
            break;
        }
      },
      (id) => (tab.runId = id),
    );
    void refreshHistory();
  } catch (error) {
    tab.error = message(error);
  } finally {
    tab.running = false;
    tab.runId = "";
  }
}

export async function stopRun(tab: RunnerTab): Promise<void> {
  if (tab.runId) await api.cancelRun(tab.runId).catch(() => {});
}

// ---- Dialogs and notices -----------------------------------------------------------

export type Dialog =
  | { kind: "palette" }
  | { kind: "import"; folder: string }
  | { kind: "code"; tab: RequestTab }
  | { kind: "shortcuts" }
  | { kind: "cookies" }
  | {
      kind: "confirm";
      title: string;
      body: string;
      confirm: string;
      danger: boolean;
      onConfirm: () => void;
    }
  | {
      kind: "prompt";
      title: string;
      label: string;
      value: string;
      confirm: string;
      onSubmit: (value: string) => void | Promise<void>;
    }
  | { kind: "saveAs"; tab: RequestTab }
  | { kind: "settings" }
  | { kind: "update" };

export const dialogs = $state({ current: null as Dialog | null });

export function openDialog(dialog: Dialog): void {
  dialogs.current = dialog;
}

export function closeDialog(): void {
  dialogs.current = null;
}

export function confirmDialog(options: {
  title: string;
  body: string;
  confirm: string;
  danger?: boolean;
  onConfirm: () => void;
}): void {
  openDialog({ kind: "confirm", danger: false, ...options });
}

export function promptDialog(options: {
  title: string;
  label: string;
  value?: string;
  confirm: string;
  onSubmit: (value: string) => void | Promise<void>;
}): void {
  openDialog({ kind: "prompt", value: "", ...options });
}

export function saveAsDialog(tab: RequestTab): void {
  openDialog({ kind: "saveAs", tab });
}

export interface Notice {
  id: number;
  text: string;
  kind: "ok" | "error" | "info";
}

export const notices = $state({ list: [] as Notice[] });
let noticeId = 0;

export function notify(text: string, kind: Notice["kind"] = "info"): void {
  const id = ++noticeId;
  notices.list.push({ id, text, kind });
  setTimeout(
    () => {
      notices.list = notices.list.filter((n) => n.id !== id);
    },
    kind === "error" ? 6000 : 2200,
  );
}

export function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Requests to move focus, counted so the same request can repeat. */
export const signals = $state({ focusUrl: 0, focusEnvironment: 0, focusFilter: 0 });

// ---- Item actions -------------------------------------------------------------------

/** File stem or directory name: how a folder's `order` names an item. */
export function itemKey(path: string): string {
  const last = path.slice(path.lastIndexOf("/") + 1);
  return last.replace(/\.ya?ml$/, "");
}

export function renameItem(path: string, kind: "request" | "folder", current: string): void {
  promptDialog({
    title: kind === "folder" ? "Rename folder" : "Rename request",
    label: "Name",
    value: current,
    confirm: "Rename",
    onSubmit: async (name) => {
      try {
        const renamed = await api.rename(path, name);
        retargetTabs(path, renamed.path);
        for (const tab of tabs.list) {
          if (tab.kind === "request" && tab.path === renamed.path) {
            const clean = !isDirty(tab);
            tab.draft.name = name;
            if (clean) tab.saved = fingerprint(tab.draft);
          } else if (tab.kind === "folder" && tab.path === renamed.path) {
            const clean = !isDirty(tab);
            tab.draft.name = name;
            if (clean) tab.saved = JSON.stringify(tab.draft);
          }
        }
        await refreshTree();
      } catch (error) {
        notify(message(error), "error");
      }
    },
  });
}

export function deleteItem(path: string, kind: "request" | "folder", name: string): void {
  confirmDialog({
    title: `Delete ${name}?`,
    body:
      kind === "folder"
        ? `The folder ${path}/ and everything in it will be deleted from disk. Git can bring it back if it was committed.`
        : `${path} will be deleted from disk. Git can bring it back if it was committed.`,
    confirm: "Delete",
    danger: true,
    onConfirm: async () => {
      try {
        await api.remove(path);
        dropTabs(path);
        await refreshTree();
      } catch (error) {
        notify(message(error), "error");
      }
    },
  });
}

export async function duplicateItem(path: string, kind: "request" | "folder"): Promise<void> {
  try {
    const copy = await api.duplicate(path);
    await refreshTree();
    if (kind === "request") await openRequest(copy.path);
  } catch (error) {
    notify(message(error), "error");
  }
}

export async function moveItem(path: string, to: string): Promise<void> {
  if (parentOf(path) === to) return;
  try {
    const moved = await api.move(path, to);
    retargetTabs(path, moved.path);
    if (to) setExpanded(to, true);
    await refreshTree();
  } catch (error) {
    notify(message(error), "error");
  }
}

/** Puts `path` right before `before` in their shared folder. */
export async function reorderBefore(path: string, before: string | null): Promise<void> {
  const folder = parentOf(path);
  const siblings =
    folder === ""
      ? ws.tree
      : ((findNode(folder) as Extract<Node, { kind: "folder" }> | undefined)?.children ?? []);
  const keys = siblings.map((node) => itemKey(node.path)).filter((key) => key !== itemKey(path));
  const at = before === null ? keys.length : keys.indexOf(itemKey(before));
  keys.splice(at < 0 ? keys.length : at, 0, itemKey(path));
  try {
    await api.order(folder, keys);
    await refreshTree();
  } catch (error) {
    notify(message(error), "error");
  }
}

export function newRequestIn(folder: string): void {
  promptDialog({
    title: "New request",
    label: "Request name",
    confirm: "Create request",
    onSubmit: async (name) => {
      try {
        const { path } = await api.createRequest(folder, name, toRequest(newDraft(name)));
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
