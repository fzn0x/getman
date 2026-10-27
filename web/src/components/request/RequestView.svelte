<script lang="ts">
  import { overflowFade } from "../../lib/actions";
  import { untrack } from "svelte";
  import { api } from "../../lib/api";
  import { AUTH_TYPES, BODY_TYPES, METHODS, toDraft, toRequest } from "../../lib/draft";
  import { methodClass } from "../../lib/format";
  import { keyLabel } from "../../lib/keys";
  import { copyText } from "../../lib/clipboard";
  import {
    deleteItem,
    duplicateItem,
    findNode,
    isDirty,
    message,
    notify,
    openDialog,
    openRunner,
    parentOf,
    prefs,
    promptDialog,
    refreshVariables,
    save,
    send,
    signals,
    ws,
    contextPath,
    type RequestTab,
  } from "../../lib/store.svelte";
  import type { Header } from "../../lib/types";
  import { splitUrl, syncPathParams } from "../../lib/url";
  import {
    fromPicker,
    isLive,
    livePanes,
    pickerLabel,
    pickerValue,
    STREAM_CHOICES,
  } from "../../lib/realtime";
  import { connected, live, toggle } from "../../lib/realtime.svelte";
  import { stopSending } from "../../lib/store.svelte";
  import Editor from "../Editor.svelte";
  import Icon from "../Icon.svelte";
  import Menu, { type MenuItem } from "../Menu.svelte";
  import LivePane from "../realtime/LivePane.svelte";
  import MessagePane from "../realtime/MessagePane.svelte";
  import SocketIoPane from "../realtime/SocketIoPane.svelte";
  import StreamSettings from "../realtime/StreamSettings.svelte";
  import ResponseView from "../response/ResponseView.svelte";
  import AuthPane from "./AuthPane.svelte";
  import BodyPane from "./BodyPane.svelte";
  import ChecksPane from "./ChecksPane.svelte";
  import KeyValueTable from "./KeyValueTable.svelte";
  import ParamsPane from "./ParamsPane.svelte";
  import ScriptsPane from "./ScriptsPane.svelte";
  import SettingsPane from "./SettingsPane.svelte";
  import VarsTable from "./VarsTable.svelte";

  let { tab }: { tab: RequestTab } = $props();

  let urlEditor: ReturnType<typeof Editor> | undefined = $state();
  let menu = $state<{ x: number; y: number } | null>(null);
  let sentHeaders = $state<Header[] | null>(null);
  let dragging = $state(false);
  let panes: HTMLDivElement;

  const draft = $derived(tab.draft);
  const dirty = $derived(isDirty(tab));
  const known = $derived(ws.variablesReady ? new Set(ws.variables.map((v) => v.name)) : null);
  const crumbs = $derived.by(() => {
    const path = contextPath(tab);
    if (!path) return [];
    const folder = parentOf(path);
    if (!folder) return [];
    return folder.split("/").map((_, i, parts) => {
      const at = parts.slice(0, i + 1).join("/");
      return findNode(at)?.name ?? parts[i]!;
    });
  });

  // Path parameters follow the :names in the URL.
  $effect(() => {
    const url = draft.url;
    untrack(() => {
      const next = syncPathParams(url, draft.path_params);
      if (JSON.stringify(next) !== JSON.stringify(draft.path_params)) draft.path_params = next;
    });
  });

  // What variables the request sees changes with its own variables.
  let refreshTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    JSON.stringify(draft.variables);
    clearTimeout(refreshTimer);
    refreshTimer = setTimeout(() => void refreshVariables(), 400);
    return () => clearTimeout(refreshTimer);
  });

  let seenFocus = signals.focusUrl;
  $effect(() => {
    if (signals.focusUrl !== seenFocus) {
      seenFocus = signals.focusUrl;
      urlEditor?.focus();
    }
  });

  const counts = $derived({
    params: splitUrl(draft.url).params.length + Object.keys(draft.path_params).length,
    headers: draft.headers.filter((h) => h.enabled !== false && h.name.trim()).length,
    checks: draft.checks.filter((c) => c.trim()).length + Object.keys(draft.captures).length,
    variables: Object.keys(draft.variables).length,
  });

  const panesList = $derived([
    { id: "params", label: "Params", note: counts.params ? String(counts.params) : "" },
    { id: "headers", label: "Headers", note: counts.headers ? String(counts.headers) : "" },
    {
      id: "body",
      label: "Body",
      note:
        draft.body.type === "none"
          ? ""
          : (BODY_TYPES.find((b) => b.type === draft.body.type)?.label ?? ""),
    },
    {
      id: "auth",
      label: "Auth",
      note:
        draft.auth.type === "inherit"
          ? ""
          : (AUTH_TYPES.find((a) => a.type === draft.auth.type)?.label ?? ""),
    },
    {
      id: "scripts",
      label: "Scripts",
      note: draft.scripts.pre_request.trim() || draft.scripts.post_response.trim() ? "JS" : "",
    },
    { id: "checks", label: "Checks", note: counts.checks ? String(counts.checks) : "" },
    { id: "variables", label: "Vars", note: counts.variables ? String(counts.variables) : "" },
    {
      id: "settings",
      label: "Settings",
      note: "",
      mark: Object.values(draft.settings).some((v) => v !== null && v !== undefined),
    },
    { id: "docs", label: "Docs", note: "", mark: draft.description.trim() !== "" },
  ]);

  // WebSocket, event stream and Socket.IO requests: their own panes, a
  // connection instead of a response, and a connect button.
  const liveKind = $derived(isLive(draft.kind));
  $effect.pre(() => {
    if (liveKind && !tab.live) live(tab);
  });
  const shownPanes = $derived.by(() => {
    if (!liveKind) return panesList;
    const extra = [
      {
        id: "message",
        label: "Message",
        note: String(
          (draft.kind === "socketio" ? draft.socketio.messages : draft.websocket.messages).length ||
            "",
        ),
      },
      { id: "socketio", label: "Socket.IO", note: draft.socketio.namespace.trim() },
    ];
    return livePanes(draft.kind).flatMap((id) => {
      const pane = panesList.find((p) => p.id === id) ?? extra.find((p) => p.id === id);
      return pane ? [pane] : [];
    });
  });
  $effect(() => {
    const ids = shownPanes.map((p) => p.id);
    if (!ids.includes(tab.requestPane)) tab.requestPane = ids[0] ?? "params";
  });
  const linked = $derived(liveKind && connected(tab));
  const picker = $derived(pickerValue(draft.kind, draft.method));
  const known_picker = $derived(
    METHODS.includes(picker as (typeof METHODS)[number]) ||
      STREAM_CHOICES.some((c) => c.value === picker),
  );

  function pick(value: string): void {
    if (value === "__custom") {
      if (draft.kind !== "sse") draft.kind = "http";
      customMethod();
      return;
    }
    const chosen = fromPicker(value);
    draft.kind = chosen.kind;
    if (chosen.method) draft.method = chosen.method;
    else draft.method = "GET";
  }

  function primaryClick(): void {
    if (liveKind) void toggle(tab);
    else if (tab.sending) stopSending(tab);
    else void send(tab);
  }

  const HEADER_NAMES = [
    "Accept",
    "Accept-Encoding",
    "Accept-Language",
    "Authorization",
    "Cache-Control",
    "Content-Type",
    "Cookie",
    "If-Match",
    "If-None-Match",
    "Origin",
    "Prefer",
    "Referer",
    "User-Agent",
    "X-Api-Key",
    "X-Correlation-Id",
    "X-Request-Id",
  ];

  function onPaste(text: string): boolean {
    if (!/^\s*curl\s/.test(text)) return false;
    void (async () => {
      try {
        const parsed = await api.parseCurl(text);
        const name = tab.path ? draft.name : parsed.name;
        tab.draft = toDraft({ ...parsed, name });
        notify("Filled in from the cURL command", "ok");
      } catch (error) {
        notify(message(error), "error");
      }
    })();
    return true;
  }

  function customMethod(): void {
    promptDialog({
      title: "Custom method",
      label: "Method",
      value: METHODS.includes(draft.method as (typeof METHODS)[number]) ? "" : draft.method,
      confirm: "Use method",
      onSubmit: (value) => {
        draft.method = value.trim().toUpperCase();
      },
    });
  }

  async function previewHeaders(): Promise<void> {
    try {
      const prepared = await api.prepare({
        path: contextPath(tab),
        request: toRequest(draft),
        environment: ws.environment || null,
      });
      sentHeaders = prepared.request.headers;
    } catch (error) {
      notify(message(error), "error");
    }
  }

  const menuItems = $derived<(MenuItem | "separator")[]>([
    {
      label: "Copy as code…",
      icon: "code",
      hint: keyLabel("alt+c").join(" "),
      action: () => openDialog({ kind: "code", tab }),
    },
    {
      label: "Copy as cURL",
      icon: "copy",
      action: async () => {
        try {
          const { code } = await api.codegen({
            path: contextPath(tab),
            request: toRequest(draft),
            environment: ws.environment || null,
            language: "curl",
          });
          await copyText(code, "cURL command copied");
        } catch (error) {
          notify(message(error), "error");
        }
      },
    },
    "separator",
    { label: "Save as…", icon: "save", action: () => openDialog({ kind: "saveAs", tab }) },
    ...(tab.path
      ? ([
          {
            label: "Duplicate",
            icon: "copy",
            action: () => void duplicateItem(tab.path!, "request"),
          },
          { label: "Run", icon: "play", action: () => openRunner(tab.path!) },
          "separator",
          {
            label: "Delete",
            icon: "trash",
            danger: true,
            action: () => deleteItem(tab.path!, "request", draft.name),
          },
        ] as (MenuItem | "separator")[])
      : []),
  ]);

  function startSplit(event: PointerEvent): void {
    const handle = event.currentTarget as HTMLElement;
    handle.setPointerCapture(event.pointerId);
    dragging = true;
    const rect = panes.getBoundingClientRect();
    const move = (e: PointerEvent) => {
      const fraction =
        prefs.layout === "columns"
          ? (e.clientX - rect.left) / rect.width
          : (e.clientY - rect.top) / rect.height;
      prefs.split = Math.min(0.8, Math.max(0.2, fraction));
    };
    const up = () => {
      dragging = false;
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
  }

  function splitKeys(event: KeyboardEvent): void {
    const back = event.key === "ArrowLeft" || event.key === "ArrowUp";
    const forward = event.key === "ArrowRight" || event.key === "ArrowDown";
    if (back || forward) {
      event.preventDefault();
      prefs.split = Math.min(0.8, Math.max(0.2, prefs.split + (forward ? 0.05 : -0.05)));
    }
  }
</script>

<div class="request" class:rows={prefs.layout === "rows"}>
  <div class="title">
    {#if crumbs.length}
      <span class="crumbs bitmap">
        {#each crumbs as crumb, i (i)}<span>{crumb}</span><span class="slash">/</span>{/each}
      </span>
    {/if}
    <input class="name" bind:value={draft.name} aria-label="Request name" spellcheck="false" />
    {#if !tab.path}
      <span class="state bitmap">Not saved</span>
    {:else if dirty}
      <span class="state bitmap dirty">Changed</span>
    {/if}
    <div class="actions">
      <button
        class="btn quiet"
        onclick={() => void save(tab)}
        disabled={!dirty && !!tab.path}
        title="Save ({keyLabel('mod+s').join(' ')})"
      >
        <Icon name="save" /> Save
      </button>
      <button
        class="btn quiet"
        onclick={() => openDialog({ kind: "code", tab })}
        title="Copy as code ({keyLabel('alt+c').join(' ')})"
      >
        <Icon name="code" /> Code
      </button>
      <button
        class="icon-btn"
        aria-label="More actions"
        onclick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          menu = { x: rect.right - 200, y: rect.bottom + 4 };
        }}
      >
        <Icon name="more" />
      </button>
    </div>
  </div>

  <div class="line">
    <label class="method-pick method {liveKind ? 'other' : methodClass(draft.method)}">
      <span class="sr-only">Method or kind of request</span>
      <select
        value={known_picker ? picker : "__custom"}
        disabled={linked}
        title={linked ? "Disconnect to change the kind of request" : undefined}
        onchange={(event) => pick(event.currentTarget.value)}
      >
        <optgroup label="HTTP">
          {#each METHODS as method (method)}
            <option value={method}>{method}</option>
          {/each}
          {#if !known_picker}
            <option value="__custom">{pickerLabel(picker)}</option>
          {:else}
            <option value="__custom">Custom…</option>
          {/if}
        </optgroup>
        <optgroup label="Streams">
          {#each STREAM_CHOICES as choice (choice.value)}
            <option value={choice.value}>{choice.label}</option>
          {/each}
        </optgroup>
      </select>
    </label>
    <div class="url">
      <Editor
        bind:this={urlEditor}
        bind:value={draft.url}
        singleLine
        {known}
        label="URL"
        placeholder={draft.kind === "websocket"
          ? "wss://echo.example.com/socket"
          : draft.kind === "socketio"
            ? "https://chat.example.com — the path is the namespace"
            : draft.kind === "sse"
              ? "https://api.example.com/events"
              : "https://api.example.com/users — or paste a cURL command"}
        onSubmit={() => void send(tab)}
        {onPaste}
      />
    </div>
    <button
      class="btn primary send"
      class:sending={tab.sending || tab.live?.status === "connecting"}
      onclick={primaryClick}
      title={!liveKind && tab.sending ? "Stop waiting for the response" : undefined}
    >
      {#if liveKind}
        {#if tab.live?.status === "connecting"}
          <span class="scan" aria-hidden="true"></span> Cancel
        {:else if linked}
          <Icon name="stop" /> {draft.kind === "sse" ? "Stop" : "Disconnect"}
        {:else}
          <Icon name="play" /> Connect
        {/if}
      {:else if tab.sending}
        <span class="scan" aria-hidden="true"></span> Stop
      {:else}
        <Icon name="play" /> Send
      {/if}
    </button>
  </div>

  <div class="panes" bind:this={panes} class:dragging style:--split={prefs.split}>
    <section class="editor-side" aria-label="Request">
      <div class="tablist" use:overflowFade role="tablist" aria-label="Request parts">
        {#each shownPanes as pane (pane.id)}
          <button
            class="tab"
            role="tab"
            aria-selected={tab.requestPane === pane.id}
            onclick={() => (tab.requestPane = pane.id)}
          >
            {pane.label}
            {#if pane.note}<span class="count">{pane.note}</span>{/if}
            {#if "mark" in pane && pane.mark}<span class="has" aria-label="set"></span>{/if}
          </button>
        {/each}
      </div>
      <div
        class="pane"
        class:fill={tab.requestPane === "body" || tab.requestPane === "scripts"}
        class:flush={tab.requestPane === "message"}
      >
        {#if tab.requestPane === "message"}
          {#if tab.live}<MessagePane {tab} conn={tab.live} />{/if}
        {:else if tab.requestPane === "socketio"}
          <SocketIoPane bind:settings={draft.socketio} />
        {:else if tab.requestPane === "params"}
          <ParamsPane {draft} />
        {:else if tab.requestPane === "headers"}
          <div class="stack">
            <KeyValueTable
              rows={draft.headers}
              onChange={(rows) => (draft.headers = rows)}
              names={HEADER_NAMES}
              label="Headers"
              nameLabel="Header"
            />
            <div class="sent">
              <button class="btn quiet" onclick={previewHeaders}
                >Show every header that will be sent</button
              >
              {#if sentHeaders}
                <p class="muted">
                  Including those from folders, authentication, the body and Getman itself:
                </p>
                <dl>
                  {#each sentHeaders as header, i (i)}
                    <dt class="mono">{header.name}</dt>
                    <dd class="mono">{header.value}</dd>
                  {/each}
                </dl>
              {/if}
            </div>
          </div>
        {:else if tab.requestPane === "body"}
          <BodyPane bind:body={draft.body} />
        {:else if tab.requestPane === "auth"}
          <AuthPane bind:auth={draft.auth} path={contextPath(tab)} />
        {:else if tab.requestPane === "scripts"}
          <ScriptsPane bind:scripts={draft.scripts} />
        {:else if tab.requestPane === "checks"}
          <ChecksPane
            bind:checks={draft.checks}
            captures={draft.captures}
            onCaptures={(captures) => (draft.captures = captures)}
            results={tab.execution?.checks ?? []}
            response={tab.execution?.response ?? null}
          />
        {:else if tab.requestPane === "variables"}
          <div class="stack">
            <p class="muted">
              Variables for this request only. They beat the environment and folders; script locals
              beat them.
            </p>
            <VarsTable
              vars={draft.variables}
              onChange={(vars) => (draft.variables = vars)}
              label="Request variables"
            />
          </div>
        {:else if tab.requestPane === "settings"}
          {#if liveKind}<StreamSettings {draft} />{/if}
          <SettingsPane bind:settings={draft.settings} />
        {:else if tab.requestPane === "docs"}
          <textarea
            class="input docs"
            bind:value={draft.description}
            placeholder="What this request is for, in Markdown. Saved in the request file."
            aria-label="Description"></textarea>
        {/if}
      </div>
    </section>

    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      class="divider"
      role="separator"
      aria-orientation={prefs.layout === "columns" ? "vertical" : "horizontal"}
      aria-label="Resize the request and response"
      aria-valuenow={Math.round(prefs.split * 100)}
      aria-valuemin={20}
      aria-valuemax={80}
      tabindex="0"
      onpointerdown={startSplit}
      onkeydown={splitKeys}
    ></div>

    <section class="response-side" aria-label={liveKind ? "Connection" : "Response"}>
      {#if liveKind}
        {#if tab.live}<LivePane {tab} conn={tab.live} />{/if}
      {:else}
        <ResponseView {tab} />
      {/if}
    </section>
  </div>
</div>

{#if menu}
  <Menu
    items={menuItems}
    x={menu.x}
    y={menu.y}
    label="Request actions"
    onClose={() => (menu = null)}
  />
{/if}

<style>
  .request {
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    height: 100%;
    min-width: 0;
  }

  .title {
    display: flex;
    align-items: center;
    gap: var(--s2);
    min-width: 0;
    padding: var(--s2) var(--s3) 0 var(--s4);
  }

  .crumbs {
    display: flex;
    gap: 4px;
    color: var(--ink-3);
    white-space: nowrap;
    overflow: hidden;
    flex: 0 1 auto;
    min-width: 0;
  }

  .slash {
    color: var(--t4);
  }

  .name {
    flex: 1 1 200px;
    min-width: 80px;
    height: 30px;
    padding: 0 4px;
    margin-left: -4px;
    border: 1px solid transparent;
    background: transparent;
    font-size: var(--fs-title);
    font-weight: 650;
    color: var(--ink);
  }

  .name:hover {
    border-color: var(--rule-2);
  }

  .name:focus {
    border-color: var(--ink);
    background: var(--field);
  }

  .state {
    color: var(--ink-3);
    white-space: nowrap;
  }

  .state.dirty {
    color: var(--accent-text);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-left: auto;
  }

  .line {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: stretch;
    gap: 0;
    margin: var(--s2) var(--s3) var(--s3) var(--s4);
    border: 2px solid var(--ink);
    background: var(--field);
    height: 40px;
  }

  .method-pick {
    position: relative;
    display: flex;
    border-right: 1px solid var(--rule-2);
  }

  .method-pick select {
    appearance: none;
    height: 100%;
    padding: 0 26px 0 var(--s3);
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: var(--b1);
    cursor: pointer;
  }

  .method-pick select option {
    color: var(--ink);
    font-family: var(--font-ui);
  }

  .method-pick::after {
    content: "";
    position: absolute;
    right: 10px;
    top: 50%;
    width: 7px;
    height: 4px;
    margin-top: -2px;
    background: currentColor;
    clip-path: polygon(0 0, 100% 0, 50% 100%);
    pointer-events: none;
  }

  .url {
    display: flex;
    align-items: center;
    min-width: 0;
    padding: 0 var(--s3);
    font-size: 14px;
  }

  .url :global(.editor) {
    width: 100%;
  }

  .url :global(.cm-scroller) {
    font-size: 14px;
  }

  .send {
    --corner: var(--ink);
    height: auto;
    min-width: 104px;
    border: 0;
    border-left: 2px solid var(--ink);
    font-size: var(--b1);
  }

  .send.sending {
    opacity: 1;
  }

  .scan {
    width: 22px;
    height: 7px;
    background-image: radial-gradient(circle, currentColor 1px, transparent 1.5px);
    background-size: 4px 4px;
    animation: scan 0.9s steps(4) infinite;
  }

  @keyframes scan {
    from {
      clip-path: inset(0 100% 0 0);
    }
    to {
      clip-path: inset(0 0 0 0);
    }
  }

  .panes {
    display: grid;
    grid-template-columns: minmax(0, calc(var(--split) * 100%)) 1px minmax(0, 1fr);
    min-height: 0;
    border-top: 1px solid var(--rule);
  }

  .rows .panes {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, calc(var(--split) * 100%)) 1px minmax(0, 1fr);
  }

  .panes.dragging {
    user-select: none;
  }

  .editor-side {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    min-height: 0;
    min-width: 0;
  }

  .editor-side .tablist {
    padding: 0 var(--s2);
  }

  .pane {
    min-height: 0;
    overflow: auto;
    padding: var(--s3) var(--s4) var(--s4);
  }

  .pane.fill {
    overflow: hidden;
  }

  .pane.flush {
    overflow: hidden;
    padding: 0;
  }

  .divider {
    position: relative;
    background: var(--rule);
    cursor: col-resize;
  }

  .rows .divider {
    cursor: row-resize;
  }

  .divider::before {
    content: "";
    position: absolute;
    inset: 0 -4px;
  }

  .rows .divider::before {
    inset: -4px 0;
  }

  .divider:hover,
  .divider:focus-visible {
    background: var(--accent);
    outline: none;
  }

  .response-side {
    min-width: 0;
    min-height: 0;
  }

  .has {
    width: 5px;
    height: 5px;
    background: var(--ink-3);
  }

  .stack {
    display: grid;
    gap: var(--s4);
    align-content: start;
  }

  .stack > p {
    margin: 0;
  }

  .sent dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px var(--s3);
    margin: var(--s2) 0 0;
  }

  .sent dt {
    color: var(--ink-2);
  }

  .sent dd {
    margin: 0;
    word-break: break-all;
  }

  .sent p {
    margin: var(--s2) 0 0;
  }

  .docs {
    width: 100%;
    height: 100%;
    min-height: 240px;
    resize: none;
    line-height: 1.6;
  }

  @media (max-width: 900px) {
    .panes {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: minmax(0, 1fr) 1px minmax(0, 1fr);
    }

    .divider {
      cursor: row-resize;
    }

    .title {
      padding-left: var(--s3);
    }

    .line {
      margin-left: var(--s3);
    }

    .crumbs {
      display: none;
    }
  }
</style>
