// Everything the UI holds: the workspace, open tabs, preferences and the
// dialogs. Components read and change it directly; the functions here do
// what involves the server.

import { api, ApiError } from "./api";
import { clone, fingerprint, newDraft, toDraft, toRequest } from "./draft";
import {
  closeTargets,
  findMoved,
  moveTo,
  pushClosed,
  readSession,
  type SavedSession,
  type SavedTab,
} from "./session";
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
  TrashItem,
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
  /** Show the tree sorted A to Z instead of in each folder's order. */
  sortTree: false,
  history: [] as HistoryEntry[],
  /** Deleted requests and folders, newest first. */
  trash: [] as TrashItem[],
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
    ws.sortTree = load(scoped("sortTree"), false);
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
    void refreshTrash();
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

export async function refreshTrash(): Promise<void> {
  try {
    ws.trash = await api.trash();
  } catch {
    // The trash list is a convenience; the items stay on disk.
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

export function collapseAll(): void {
  ws.expanded = {};
  store(scoped("expanded"), ws.expanded);
}

/** Sorts the sidebar A to Z, for this workspace in this browser only. */
export function setSortTree(on: boolean): void {
  ws.sortTree = on;
  store(scoped("sortTree"), on);
}

/** Items picked in the sidebar with Ctrl or Shift, to act on together. */
export const selection = $state({ paths: [] as string[], anchor: "" });

export function clearSelection(): void {
  selection.paths = [];
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
  /** Opened from history: when it was sent, and whether the body was cut. */
  fromHistory?: { at: number; bodyCut: boolean };
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
  rememberClosed([tab]);
  tabs.list.splice(at, 1);
  if (tabs.active === id) {
    const next = tabs.list[Math.min(at, tabs.list.length - 1)];
    tabs.active = next?.id ?? "";
  }
  persistTabs();
}

/** Closes several tabs, asking once if any has changes that are not saved. */
export function closeTabs(ids: string[], force = false): void {
  const closing = tabs.list.filter((tab) => ids.includes(tab.id));
  if (closing.length === 0) return;
  const dirty = closing.filter(isDirty);
  if (!force && dirty.length > 0) {
    confirmDialog({
      title:
        closing.length === 1 ? `Close ${tabTitle(closing[0]!)}?` : `Close ${closing.length} tabs?`,
      body:
        dirty.length === 1
          ? `${tabTitle(dirty[0]!)} has changes that are not saved.`
          : `${dirty.length} of them have changes that are not saved.`,
      confirm: "Close without saving",
      danger: true,
      onConfirm: () => closeTabs(ids, true),
    });
    return;
  }
  rememberClosed(closing);
  const before = tabs.list;
  const at = before.findIndex((tab) => tab.id === tabs.active);
  const stays = (tab: Tab) => !ids.includes(tab.id);
  tabs.list = before.filter(stays);
  if (!tabs.list.some((tab) => tab.id === tabs.active)) {
    const next =
      before.slice(at + 1).find(stays) ?? before.slice(0, Math.max(0, at)).reverse().find(stays);
    tabs.active = next?.id ?? "";
  }
  persistTabs();
}

/** Closes the other tabs, those to the right of one, or all of them. */
export function closeTabsFrom(id: string, mode: "others" | "right" | "all"): void {
  closeTabs(
    closeTargets(
      tabs.list.map((tab) => tab.id),
      id,
      mode,
    ),
  );
}

/** Puts a tab at another place in the strip. */
export function moveTab(id: string, to: number): void {
  const from = tabs.list.findIndex((tab) => tab.id === id);
  if (from < 0 || from === to) return;
  tabs.list = moveTo(tabs.list, from, to);
  persistTabs();
}

/** Opens a copy of a request tab as a new, unsaved request. */
export function duplicateTab(id: string): void {
  const tab = tabs.list.find((t) => t.id === id);
  if (tab?.kind !== "request") return;
  const copy = toRequest(clone(tab.draft));
  copy.name = `${tab.draft.name || "Untitled request"} copy`;
  newRequest(copy, contextPath(tab));
}

// ---- Remembering tabs ----------------------------------------------------------
//
// Open tabs and their unsaved drafts are kept in this browser, per workspace,
// so they survive a reload or a restart. Closed tabs go on a short stack that
// Alt+Shift+T (or Ctrl+Shift+T where the browser lets it through) reopens.

export const closedTabs = $state({ list: [] as SavedTab[] });

function saveTab(tab: Tab): SavedTab {
  switch (tab.kind) {
    case "request": {
      const keep = !tab.path || isDirty(tab);
      return {
        kind: "request",
        path: tab.path,
        inheritPath: tab.inheritPath,
        name: tab.draft.name,
        method: tab.draft.method,
        ...(keep ? { draft: clone(tab.draft), base: tab.saved } : {}),
        requestPane: tab.requestPane,
        responsePane: tab.responsePane,
      };
    }
    case "folder":
      return {
        kind: "folder",
        path: tab.path,
        name: tab.draft.name,
        ...(isDirty(tab) ? { draft: clone(tab.draft), base: tab.saved } : {}),
        pane: tab.pane,
      };
    case "environments":
      return { kind: "environments", selected: tab.selected };
    case "runner":
      return { kind: "runner", target: tab.target };
  }
}

function rememberClosed(list: Tab[]): void {
  for (const tab of list) closedTabs.list = pushClosed(closedTabs.list, saveTab(tab));
}

/** Writes the open tabs down; false if the drafts did not fit. */
function persistTabs(): boolean {
  if (!ws.root || !sessionRestored) return true;
  const session: SavedSession = {
    version: 2,
    tabs: tabs.list.map(saveTab),
    active: tabs.list.findIndex((tab) => tab.id === tabs.active),
  };
  try {
    localStorage.setItem(`getman.${scoped("tabs")}`, JSON.stringify(session));
    return true;
  } catch {
    // Too big for the browser's storage: keep the tabs without drafts.
    const light = session.tabs.map((tab) =>
      tab.kind === "request" || tab.kind === "folder" ? { ...tab, draft: undefined } : tab,
    );
    store(scoped("tabs"), { ...session, tabs: light });
    return false;
  }
}

let sessionRestored = false;
let persistTimer: ReturnType<typeof setTimeout> | undefined;

/** Saves the tabs now, as the page goes away; false if drafts were lost. */
export function flushSession(): boolean {
  clearTimeout(persistTimer);
  return persistTabs();
}

/** Saves the tabs a moment after any draft changes. */
function watchSession(): void {
  $effect.root(() => {
    $effect(() => {
      JSON.stringify(
        tabs.list.map((tab) =>
          tab.kind === "request"
            ? [tab.draft, tab.requestPane, tab.responsePane]
            : tab.kind === "folder"
              ? [tab.draft, tab.pane]
              : tab.kind === "environments"
                ? tab.selected
                : tab.target,
        ),
      );
      void tabs.active;
      clearTimeout(persistTimer);
      persistTimer = setTimeout(persistTabs, 400);
    });
  });
  window.addEventListener("pagehide", flushSession);
}

function savedId(saved: SavedTab): string {
  switch (saved.kind) {
    case "request":
      return saved.path ? `r:${saved.path}` : "";
    case "folder":
      return `f:${saved.path}`;
    case "environments":
      return "environments";
    case "runner":
      return `run:${saved.target}`;
  }
}

const restoreNotes = { lost: 0, changed: 0 };

/** Opens a remembered tab, coping with files moved or deleted meanwhile. */
async function openSaved(saved: SavedTab, taken: Set<string>): Promise<Tab | null> {
  switch (saved.kind) {
    case "request": {
      const asNew = (inheritPath: string | null) => {
        const tab = requestTab(null, toRequest(newDraft()), inheritPath);
        tab.draft = toDraft(saved.draft as unknown as Request);
        tab.requestPane = saved.requestPane;
        tab.responsePane = saved.responsePane;
        return tab;
      };
      const folderExists = (path: string | null) =>
        path !== null && (path === "" || findNode(path)?.kind === "folder");
      if (saved.path === null) {
        if (!saved.draft) return null;
        return asNew(folderExists(saved.inheritPath) ? saved.inheritPath : null);
      }
      let path = saved.path;
      let request = await api.readRequest(path).catch(() => null);
      if (!request) {
        const moved = findMoved(
          { path, name: saved.draft?.name ?? saved.name, method: saved.method },
          allRequests(),
          taken,
        );
        if (moved) {
          path = moved;
          request = await api.readRequest(moved).catch(() => null);
        }
      }
      if (!request) {
        if (!saved.draft) return null;
        // The file is gone; keep the unsaved work as a new request.
        restoreNotes.lost++;
        const folder = parentOf(saved.path);
        return asNew(folderExists(folder) ? folder : null);
      }
      const tab = requestTab(path, request);
      if (saved.draft) {
        if (saved.base !== undefined && saved.base !== tab.saved) restoreNotes.changed++;
        tab.draft = toDraft(saved.draft as unknown as Request);
      }
      tab.requestPane = saved.requestPane;
      tab.responsePane = saved.responsePane;
      return tab;
    }
    case "folder": {
      const folder = await api.readFolder(saved.path).catch(() => null);
      if (!folder) {
        if (saved.draft) restoreNotes.lost++;
        return null;
      }
      const tab = folderTab(saved.path, folder);
      if (saved.draft) {
        if (saved.base !== undefined && saved.base !== tab.saved) restoreNotes.changed++;
        tab.draft = normalizeFolder(saved.draft);
      }
      tab.pane = saved.pane;
      return tab;
    }
    case "environments":
      return { kind: "environments", id: "environments", selected: saved.selected };
    case "runner":
      return saved.target === "" || findNode(saved.target) ? runnerTab(saved.target) : null;
  }
}

async function restoreTabs(): Promise<void> {
  const saved = readSession(load<unknown>(scoped("tabs"), null));
  const taken = new Set<string>();
  let active = "";
  restoreNotes.lost = 0;
  restoreNotes.changed = 0;
  for (const [i, entry] of saved.tabs.entries()) {
    try {
      const tab = await openSaved(entry, taken);
      if (!tab || tabs.list.some((t) => t.id === tab.id)) continue;
      tabs.list.push(tab);
      if (tab.kind === "request" && tab.path) taken.add(tab.path);
      if (i === saved.active) active = tab.id;
    } catch {
      // A tab that cannot be read back is left closed.
    }
  }
  tabs.active = active || (tabs.list[0]?.id ?? "");
  sessionRestored = true;
  watchSession();
  if (restoreNotes.lost > 0) {
    notify(
      restoreNotes.lost === 1
        ? "A file with unsaved changes is gone; its changes are open as a new request."
        : `${restoreNotes.lost} files with unsaved changes are gone; their changes are open as new requests.`,
      "info",
    );
  } else if (restoreNotes.changed > 0) {
    notify(
      "Some files changed on disk since your unsaved edits; saving keeps your version.",
      "info",
    );
  }
  void refreshVariables();
}

/** Reopens the tab closed last. */
export async function reopenClosedTab(): Promise<void> {
  const saved = closedTabs.list.at(-1);
  if (!saved) {
    notify("No closed tabs to reopen.");
    return;
  }
  closedTabs.list = closedTabs.list.slice(0, -1);
  const id = savedId(saved);
  if (id && tabs.list.some((tab) => tab.id === id)) {
    focusTab(id);
    return;
  }
  const open = new Set(
    tabs.list.flatMap((tab) => (tab.kind === "request" && tab.path ? [tab.path] : [])),
  );
  try {
    const tab = await openSaved(saved, open);
    if (tab) addTab(tab);
    else notify("That tab's file is gone.", "error");
  } catch (error) {
    notify(message(error), "error");
  }
}

/** Shows an item in the sidebar: opens it and its folders, and focuses it. */
export function revealInTree(path: string): void {
  prefs.sidebarOpen = true;
  let folder = parentOf(path);
  while (folder) {
    setExpanded(folder, true);
    folder = parentOf(folder);
  }
  signals.revealPath = path;
  signals.reveal++;
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

/** Opens a request from history as a new tab, with the response it got. */
export async function openHistoryEntry(entry: HistoryEntry): Promise<void> {
  const detail = await api.historyEntry(entry.id).catch(() => null);
  newRequest({ ...entry.request, name: entry.request.name || entry.url }, entry.path);
  const tab = activeTab();
  if (tab?.kind !== "request") return;
  tab.fromHistory = { at: entry.at, bodyCut: detail?.body_cut ?? false };
  if (!detail?.response && !entry.error) return;
  tab.execution = {
    path: entry.path,
    name: entry.name,
    request: null,
    response: detail?.response ?? null,
    error: entry.error,
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
  addTab(runnerTab(target));
}

function runnerTab(target: string): RunnerTab {
  return {
    kind: "runner",
    id: `run:${target}`,
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
  };
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
  tab.fromHistory = undefined;
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
  const inside = (tab: Tab) => {
    const own = tab.kind === "request" ? tab.path : tab.kind === "folder" ? tab.path : null;
    return !!own && (own === path || own.startsWith(`${path}/`));
  };
  rememberClosed(tabs.list.filter(inside));
  tabs.list = tabs.list.filter((tab) => !inside(tab));
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
  | { kind: "move"; paths: string[] }
  | { kind: "headerPresets" };

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
  /** A button on the notice, such as Undo. */
  action?: { label: string; run: () => void };
}

export const notices = $state({ list: [] as Notice[] });
let noticeId = 0;

export function notify(
  text: string,
  kind: Notice["kind"] = "info",
  action?: Notice["action"],
): void {
  const id = ++noticeId;
  notices.list.push({ id, text, kind, action });
  setTimeout(() => dismiss(id), action ? 10000 : kind === "error" ? 6000 : 2200);
}

export function dismiss(id: number): void {
  notices.list = notices.list.filter((n) => n.id !== id);
}

/** Runs the action of the newest notice that offers one, such as Undo. */
export function runNoticeAction(): boolean {
  const notice = [...notices.list].reverse().find((n) => n.action);
  if (!notice?.action) return false;
  dismiss(notice.id);
  notice.action.run();
  return true;
}

export function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Requests to move focus, counted so the same request can repeat. */
export const signals = $state({
  focusUrl: 0,
  focusEnvironment: 0,
  focusFilter: 0,
  /** Asks the sidebar to show `revealPath`. */
  reveal: 0,
  revealPath: "",
});

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

/** Moves an item to the trash; the kind and name are not needed any more. */
export function deleteItem(path: string, _kind?: "request" | "folder", _name?: string): void {
  deleteItems([path]);
}

/** Paths without those inside another of them. */
export function outermost(paths: string[]): string[] {
  const unique = [...new Set(paths)];
  return unique.filter((path) => !unique.some((other) => path.startsWith(`${other}/`)));
}

/**
 * Moves items to the trash, offering to undo it. Asks first only when an
 * open tab inside them has changes that are not saved.
 */
export function deleteItems(paths: string[]): void {
  const targets = outermost(paths).filter(Boolean);
  if (targets.length === 0) return;
  const inside = (own: string) => targets.some((p) => own === p || own.startsWith(`${p}/`));
  const unsaved = tabs.list.filter(
    (tab) =>
      (tab.kind === "request" || tab.kind === "folder") &&
      tab.path &&
      inside(tab.path) &&
      isDirty(tab),
  );
  const go = () => void trashItems(targets);
  if (unsaved.length === 0) {
    go();
    return;
  }
  confirmDialog({
    title: unsaved.length === 1 ? `Delete ${tabTitle(unsaved[0]!)}?` : "Delete these items?",
    body:
      unsaved.length === 1
        ? "It has changes that are not saved. They go; the file can be put back from the trash."
        : `${unsaved.length} open tabs have changes that are not saved. They go; the files can be put back from the trash.`,
    confirm: "Delete",
    danger: true,
    onConfirm: go,
  });
}

async function trashItems(paths: string[]): Promise<void> {
  clearSelection();
  const trashed: TrashItem[] = [];
  for (const path of paths) {
    try {
      trashed.push(await api.remove(path));
      dropTabs(path);
    } catch (error) {
      notify(message(error), "error");
    }
  }
  await refreshTree();
  await refreshTrash();
  if (trashed.length === 0) return;
  const what = trashed.length === 1 ? trashed[0]!.name : `${trashed.length} items`;
  notify(`Moved ${what} to the trash`, "info", {
    label: "Undo",
    run: () => void restoreItems(trashed),
  });
}

/** Puts items back from the trash, the last deleted first. */
export async function restoreItems(items: TrashItem[]): Promise<void> {
  const restored: string[] = [];
  for (const item of [...items].reverse()) {
    try {
      const { path } = await api.restore(item.id);
      restored.push(path);
      if (parentOf(path)) setExpanded(parentOf(path), true);
    } catch (error) {
      notify(message(error), "error");
    }
  }
  await refreshTree();
  await refreshTrash();
  if (restored.length > 0) {
    notify(
      restored.length === 1 ? `Put back ${restored[0]}` : `Put back ${restored.length} items`,
      "ok",
    );
  }
}

export function emptyTrash(): void {
  const count = ws.trash.length;
  confirmDialog({
    title: "Empty the trash?",
    body: `${count === 1 ? "The item" : `All ${count} items`} in the trash will be deleted from disk for good. Git can bring back only what was committed.`,
    confirm: "Empty trash",
    danger: true,
    onConfirm: async () => {
      try {
        await api.emptyTrash();
      } catch (error) {
        notify(message(error), "error");
      }
      await refreshTrash();
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

/** Copies several items next to their originals. */
export async function duplicateItems(paths: string[]): Promise<void> {
  let copies = 0;
  for (const path of outermost(paths)) {
    try {
      await api.duplicate(path);
      copies++;
    } catch (error) {
      notify(message(error), "error");
    }
  }
  await refreshTree();
  if (copies > 0) notify(copies === 1 ? "Duplicated" : `Duplicated ${copies} items`, "ok");
}

/** Moves several items into one folder. */
export async function moveItems(paths: string[], to: string): Promise<void> {
  clearSelection();
  let moved = 0;
  for (const path of outermost(paths)) {
    if (parentOf(path) === to || to === path || to.startsWith(`${path}/`)) continue;
    try {
      const result = await api.move(path, to);
      retargetTabs(path, result.path);
      moved++;
    } catch (error) {
      notify(message(error), "error");
    }
  }
  if (to) setExpanded(to, true);
  await refreshTree();
  if (moved > 0) {
    const where = to === "" ? ws.name : (findNode(to)?.name ?? to);
    notify(moved === 1 ? `Moved to ${where}` : `Moved ${moved} items to ${where}`, "ok");
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
