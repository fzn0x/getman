<script lang="ts">
  import { untrack } from "svelte";
  import { overflowFade } from "../../lib/actions";
  import { api } from "../../lib/api";
  import { copyText } from "../../lib/clipboard";
  import { AUTH_TYPES, toRequest } from "../../lib/draft";
  import {
    CALL_TYPES,
    clientStreams,
    countMessages,
    exampleText,
    findMethod,
    formatMessages,
    schemaSources,
  } from "../../lib/grpc";
  import {
    cancelCall,
    endStream,
    generateRequests,
    grpcPrimary,
    grpcState,
    loadProtos,
    loadSchema,
    protos,
    schemaKey,
    sendMessages,
  } from "../../lib/grpc.svelte";
  import { keyLabel } from "../../lib/keys";
  import {
    contextPath,
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
    refreshVariables,
    save,
    signals,
    ws,
    type RequestTab,
  } from "../../lib/store.svelte";
  import Editor from "../Editor.svelte";
  import Icon from "../Icon.svelte";
  import Menu, { type MenuItem } from "../Menu.svelte";
  import AuthPane from "../request/AuthPane.svelte";
  import ChecksPane from "../request/ChecksPane.svelte";
  import KeyValueTable from "../request/KeyValueTable.svelte";
  import ScriptsPane from "../request/ScriptsPane.svelte";
  import VarsTable from "../request/VarsTable.svelte";
  import GrpcResponseView from "./GrpcResponseView.svelte";

  let { tab }: { tab: RequestTab } = $props();

  let addressEditor: ReturnType<typeof Editor> | undefined = $state();
  let menu = $state<{
    x: number;
    y: number;
    items: (MenuItem | "separator")[];
    label: string;
  } | null>(null);
  let dragging = $state(false);
  let panes: HTMLDivElement;

  const draft = $derived(tab.draft);
  const rpc = $derived(grpcState(tab));
  const dirty = $derived(isDirty(tab));
  const known = $derived(ws.variablesReady ? new Set(ws.variables.map((v) => v.name)) : null);
  const method = $derived(findMethod(rpc.services, draft.method));
  const live = $derived(rpc.call);
  const callType = $derived(live?.callType ?? method?.call_type ?? null);
  const streamOpen = $derived(!!live && clientStreams(live.callType) && !live.ended);
  const count = $derived(countMessages(draft.grpc.message));
  const sources = $derived(schemaSources(protos.files));
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

  if (!protos.loaded) void loadProtos();

  // The schema follows its source: the .proto files, or the server and
  // environment it is reflected from.
  let schemaTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const key = schemaKey(tab);
    untrack(() => {
      if (key === rpc.loadedFor) return;
      clearTimeout(schemaTimer);
      schemaTimer = setTimeout(() => void loadSchema(tab), 500);
    });
    return () => clearTimeout(schemaTimer);
  });

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
      addressEditor?.focus();
    }
  });

  const counts = $derived({
    metadata: draft.headers.filter((h) => h.enabled !== false && h.name.trim()).length,
    checks: draft.checks.filter((c) => c.trim()).length + Object.keys(draft.captures).length,
    variables: Object.keys(draft.variables).length,
  });

  const panesList = $derived([
    {
      id: "message",
      label: clientStreams(callType) ? "Messages" : "Message",
      note: count && count > 1 ? String(count) : "",
    },
    { id: "metadata", label: "Metadata", note: counts.metadata ? String(counts.metadata) : "" },
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
      mark: draft.settings.timeout_ms != null || draft.settings.verify_tls != null,
    },
    { id: "docs", label: "Docs", note: "", mark: draft.description.trim() !== "" },
  ]);

  const pane = $derived(
    panesList.some((p) => p.id === tab.requestPane) ? tab.requestPane : "message",
  );

  function useExample(): void {
    if (!method) return;
    draft.grpc.message = exampleText(method);
  }

  function format(): void {
    const formatted = formatMessages(draft.grpc.message);
    if (formatted === null)
      notify("The message is not valid JSON, so it cannot be formatted.", "error");
    else draft.grpc.message = formatted;
  }

  function chooseMethod(path: string): void {
    const chosen = findMethod(rpc.services, path);
    const before = findMethod(rpc.services, draft.method);
    draft.method = path;
    // A new method starts from its example, unless a message was written.
    const untouched =
      !draft.grpc.message.trim() || (before && draft.grpc.message === exampleText(before));
    if (chosen && untouched) draft.grpc.message = exampleText(chosen);
  }

  function generateMenu(event: MouseEvent): void {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    menu = {
      x: rect.left,
      y: rect.bottom + 4,
      label: "Generate requests",
      items: rpc.services.map((service) => ({
        label: `A request for each method of ${service.name}`,
        icon: "plus" as const,
        action: () => void generateRequests(tab, service.name),
      })),
    };
  }

  async function copyGrpcurl(): Promise<void> {
    try {
      const { code } = await api.grpcCode({
        path: contextPath(tab),
        request: toRequest(draft),
        environment: ws.environment || null,
      });
      await copyText(code, "grpcurl command copied");
    } catch (error) {
      notify(message(error), "error");
    }
  }

  function moreMenu(event: MouseEvent): void {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const items: (MenuItem | "separator")[] = [
      {
        label: "Copy as grpcurl",
        icon: "copy",
        hint: keyLabel("alt+c").join(" "),
        action: () => void copyGrpcurl(),
      },
      "separator",
      { label: "Save as…", icon: "save", action: () => openDialog({ kind: "saveAs", tab }) },
    ];
    if (tab.path) {
      const path = tab.path;
      items.push(
        { label: "Duplicate", icon: "copy", action: () => void duplicateItem(path, "request") },
        { label: "Run", icon: "play", action: () => openRunner(path) },
        "separator",
        {
          label: "Delete",
          icon: "trash",
          danger: true,
          action: () => deleteItem(path, "request", draft.name),
        },
      );
    }
    menu = { x: rect.right - 200, y: rect.bottom + 4, items, label: "Request actions" };
  }

  function number(value: string): number | null {
    const n = Number(value);
    return value.trim() === "" || !Number.isFinite(n) || n < 0 ? null : Math.round(n);
  }

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
        title="Copy as grpcurl ({keyLabel('alt+c').join(' ')})"
      >
        <Icon name="code" /> Code
      </button>
      <button class="icon-btn" aria-label="More actions" onclick={moreMenu}>
        <Icon name="more" />
      </button>
    </div>
  </div>

  <div class="line">
    <span class="kind bitmap" title="A gRPC call over HTTP/2">gRPC</span>
    <div class="address">
      <Editor
        bind:this={addressEditor}
        bind:value={draft.url}
        singleLine
        {known}
        label="Server address"
        placeholder="localhost:50051, or https://api.example.com for TLS"
        onSubmit={() => void grpcPrimary(tab)}
      />
    </div>
    <label class="tls" title="Connect with TLS. Addresses starting with https:// always do.">
      <input type="checkbox" class="check" bind:checked={draft.grpc.tls} />
      <span class="bitmap">TLS</span>
    </label>
    {#if live}
      <button class="btn stop" onclick={() => void cancelCall(tab)} disabled={!live.id}>
        <Icon name="stop" /> Cancel
      </button>
    {:else}
      <button
        class="btn primary invoke"
        onclick={() => void grpcPrimary(tab)}
        disabled={!draft.method.trim()}
        title="Invoke ({keyLabel('mod+enter').join(' ')})"
      >
        <Icon name="play" /> Invoke
      </button>
    {/if}
  </div>

  <div class="schema">
    <label class="pick">
      <span class="bitmap">Schema</span>
      <select
        class="select"
        value={draft.grpc.proto}
        onchange={(event) => (draft.grpc.proto = event.currentTarget.value)}
      >
        <option value="">Server reflection</option>
        {#each sources as source (source.value)}
          <option value={source.value}>{source.label}</option>
        {/each}
        {#if draft.grpc.proto && !sources.some((s) => s.value === draft.grpc.proto)}
          <option value={draft.grpc.proto}>{draft.grpc.proto} (not found)</option>
        {/if}
      </select>
    </label>
    <button
      class="btn quiet"
      title={draft.grpc.proto
        ? "Read the .proto files again"
        : "Ask the server for its schema again"}
      onclick={() => {
        void loadProtos();
        void loadSchema(tab, true);
      }}
      disabled={rpc.loading}
    >
      Refresh
    </button>
    <label class="pick method">
      <span class="bitmap">Method</span>
      <select
        class="select mono"
        value={draft.method}
        onchange={(event) => chooseMethod(event.currentTarget.value)}
        disabled={!rpc.services.length && !draft.method}
      >
        {#if !draft.method}
          <option value="">{rpc.services.length ? "Choose a method" : "No methods yet"}</option>
        {/if}
        {#each rpc.services as service (service.name)}
          <optgroup label={service.name}>
            {#each service.methods as option (option.path)}
              <option value={option.path}
                >{option.name}{option.call_type === "unary"
                  ? ""
                  : ` · ${CALL_TYPES[option.call_type]}`}</option
              >
            {/each}
          </optgroup>
        {/each}
        {#if draft.method && !method}
          <option value={draft.method}>{draft.method}</option>
        {/if}
      </select>
    </label>
    <button
      class="btn quiet"
      onclick={generateMenu}
      disabled={!rpc.services.length}
      title="Write a request for each method of a service"
    >
      <Icon name="plus" /> Generate requests
    </button>
    <p class="schema-note" class:bad={!!rpc.error} aria-live="polite">
      {#if rpc.loading}
        Reading the schema…
      {:else if rpc.error}
        {rpc.error}
      {:else if method}
        {CALL_TYPES[method.call_type]}: <code>{method.input}</code> → <code>{method.output}</code>
      {:else if draft.method && rpc.services.length}
        The schema has no method {draft.method}.
      {:else if !draft.grpc.proto && !draft.url.trim()}
        Fill in the address to ask the server for its methods, or choose .proto files.
      {/if}
    </p>
  </div>

  <div class="panes" bind:this={panes} class:dragging style:--split={prefs.split}>
    <section class="editor-side" aria-label="Request">
      <div class="tablist" use:overflowFade role="tablist" aria-label="Request parts">
        {#each panesList as item (item.id)}
          <button
            class="tab"
            role="tab"
            aria-selected={pane === item.id}
            onclick={() => (tab.requestPane = item.id)}
          >
            {item.label}
            {#if item.note}<span class="count">{item.note}</span>{/if}
            {#if "mark" in item && item.mark}<span class="has" aria-label="set"></span>{/if}
          </button>
        {/each}
      </div>
      <div class="pane" class:fill={pane === "message" || pane === "scripts"}>
        {#if pane === "message"}
          <div class="message-pane">
            <div class="toolbar">
              <span class="muted">
                {#if method}<code>{method.input}</code>{:else}JSON{/if}
                {#if clientStreams(callType)}· write several messages one after another{/if}
              </span>
              <button class="btn quiet" onclick={useExample} disabled={!method}>Use example</button>
              <button class="btn quiet" onclick={format}>Beautify</button>
            </div>
            <div class="editor-box">
              <Editor
                bind:value={draft.grpc.message}
                language="json"
                lint={!clientStreams(callType)}
                {known}
                label="Message"
                placeholder={method ? "Use example to start from the message's fields" : "{}"}
              />
            </div>
            {#if live && clientStreams(live.callType)}
              <div class="stream">
                {#if streamOpen}
                  <button
                    class="btn primary"
                    onclick={() => void sendMessages(tab)}
                    disabled={!live.id || !count}
                    title="Send ({keyLabel('mod+enter').join(' ')})"
                  >
                    <Icon name="play" />
                    {count && count > 1 ? `Send ${count} messages` : "Send"}
                  </button>
                  <button class="btn" onclick={() => void endStream(tab)} disabled={!live.id}>
                    End stream
                  </button>
                  <span class="muted">
                    {count === null
                      ? "The editor does not hold valid JSON."
                      : "The stream stays open until you end it."}
                  </span>
                {:else}
                  <span class="muted">The stream is ended; the server may still answer.</span>
                {/if}
              </div>
            {:else if clientStreams(callType)}
              <p class="stream muted">
                Invoke opens the stream; then send messages one at a time. Runs send every message
                here.
              </p>
            {/if}
          </div>
        {:else if pane === "metadata"}
          <div class="stack">
            <KeyValueTable
              rows={draft.headers}
              onChange={(rows) => (draft.headers = rows)}
              label="Metadata"
              nameLabel="Key"
            />
            <p class="muted">
              Sent as gRPC metadata, with the folder's headers and the authentication. Keys are
              lowercase; a key ending in <code>-bin</code> takes its value as Base64.
            </p>
          </div>
        {:else if pane === "auth"}
          <div class="stack">
            <AuthPane bind:auth={draft.auth} path={contextPath(tab)} />
            <p class="muted">
              Credentials go out as metadata, such as <code>authorization</code>. Digest does not
              work with gRPC.
            </p>
          </div>
        {:else if pane === "scripts"}
          <ScriptsPane bind:scripts={draft.scripts} />
        {:else if pane === "checks"}
          <ChecksPane
            bind:checks={draft.checks}
            captures={draft.captures}
            onCaptures={(captures) => (draft.captures = captures)}
            results={tab.execution?.checks ?? []}
            response={tab.execution?.response ?? null}
          />
        {:else if pane === "variables"}
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
        {:else if pane === "settings"}
          <div class="settings">
            <label class="field">
              <span class="bitmap">Deadline</span>
              <span class="with-unit">
                <input
                  class="input mono"
                  inputmode="numeric"
                  placeholder="Inherit"
                  value={draft.settings.timeout_ms ?? ""}
                  oninput={(event) =>
                    (draft.settings.timeout_ms = number(event.currentTarget.value))}
                />
                <span class="muted">ms · 0 for none</span>
              </span>
            </label>
            <p class="muted">
              The request's timeout, sent as the call's deadline. Unary calls default to 30000;
              streams have none unless you set one.
            </p>
            <label class="field">
              <span class="bitmap">Check TLS certificates</span>
              <select
                class="select"
                value={draft.settings.verify_tls === true
                  ? "on"
                  : draft.settings.verify_tls === false
                    ? "off"
                    : ""}
                onchange={(event) => {
                  const value = event.currentTarget.value;
                  draft.settings.verify_tls =
                    value === "on" ? true : value === "off" ? false : null;
                }}
              >
                <option value="">Inherit (yes)</option>
                <option value="on">Yes</option>
                <option value="off">No — accept self-signed and expired certificates</option>
              </select>
            </label>
          </div>
        {:else if pane === "docs"}
          <textarea
            class="input docs"
            bind:value={draft.description}
            placeholder="What this call is for, in Markdown. Saved in the request file."
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

    <section class="response-side" aria-label="Response">
      <GrpcResponseView {tab} />
    </section>
  </div>
</div>

{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} label={menu.label} onClose={() => (menu = null)} />
{/if}

<style>
  .request {
    display: grid;
    grid-template-rows: auto auto auto minmax(0, 1fr);
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
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    align-items: stretch;
    margin: var(--s2) var(--s3) var(--s2) var(--s4);
    border: 2px solid var(--ink);
    background: var(--field);
    height: 40px;
  }

  .kind {
    display: flex;
    align-items: center;
    padding: 0 var(--s3);
    border-right: 1px solid var(--rule-2);
    color: var(--ink-2);
  }

  .address {
    display: flex;
    align-items: center;
    min-width: 0;
    padding: 0 var(--s3);
    font-size: 14px;
  }

  .address :global(.editor) {
    width: 100%;
  }

  .address :global(.cm-scroller) {
    font-size: 14px;
  }

  .tls {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 var(--s3);
    border-left: 1px solid var(--rule-2);
    cursor: pointer;
  }

  .invoke,
  .stop {
    --corner: var(--ink);
    height: auto;
    min-width: 104px;
    border: 0;
    border-left: 2px solid var(--ink);
    font-size: var(--b1);
  }

  .schema {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--s2) var(--s3);
    margin: 0 var(--s3) var(--s3) var(--s4);
  }

  .pick {
    display: flex;
    align-items: center;
    gap: var(--s2);
    min-width: 0;
  }

  .pick > .bitmap {
    color: var(--ink-3);
  }

  .pick .select {
    max-width: 280px;
  }

  .pick.method .select {
    max-width: 340px;
  }

  .schema-note {
    flex-basis: 100%;
    margin: 0;
    min-height: 1.45em;
    color: var(--ink-3);
    font-size: var(--fs-meta);
    word-break: break-word;
  }

  .schema-note.bad {
    color: var(--error);
  }

  code {
    font-family: var(--font-code);
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

  .message-pane {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    gap: var(--s2);
    height: 100%;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }

  .toolbar .muted {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .editor-box {
    min-height: 0;
    border: 1px solid var(--rule-2);
    background: var(--field);
    overflow: auto;
  }

  .stream {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--s2);
    margin: 0;
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

  .settings {
    display: grid;
    gap: var(--s3);
    align-content: start;
    max-width: 460px;
  }

  .settings p {
    margin: 0;
  }

  .field {
    display: grid;
    gap: 6px;
  }

  .field > .bitmap {
    color: var(--ink-3);
  }

  .with-unit {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }

  .with-unit .input {
    width: 160px;
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

    .line,
    .schema {
      margin-left: var(--s3);
    }

    .crumbs {
      display: none;
    }
  }

  @media (max-width: 520px) {
    .line {
      grid-template-columns: minmax(0, 1fr) auto auto;
    }

    .kind {
      display: none;
    }
  }
</style>
