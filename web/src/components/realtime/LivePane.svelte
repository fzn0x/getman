<script lang="ts">
  import { overflowFade } from "../../lib/actions";
  import { copyText } from "../../lib/clipboard";
  import { bytes, millis } from "../../lib/format";
  import { keyLabel } from "../../lib/keys";
  import {
    clock,
    closeText,
    matches,
    preview,
    readable,
    type LogEntry,
    type StreamMessage,
  } from "../../lib/realtime";
  import { clearLog, entries, type LiveState } from "../../lib/realtime.svelte";
  import { openEnvironments, type RequestTab } from "../../lib/store.svelte";
  import Editor from "../Editor.svelte";
  import Icon from "../Icon.svelte";

  let { tab, conn }: { tab: RequestTab; conn: LiveState } = $props();

  let pane = $state<"messages" | "handshake" | "tests">("messages");
  let list: HTMLOListElement | undefined = $state();
  let pinned = true;

  // The log array itself stays the same as it grows, so derive what
  // changes from it: its length, and a filtered copy.
  const total = $derived(entries(conn).length);
  const visible = $derived(
    entries(conn).filter((entry) => matches(entry, conn.search, conn.direction)),
  );
  const selected = $derived(
    visible.find((entry) => entry.seq === conn.selected && entry.event.type === "message"),
  );
  const message = $derived(
    selected && selected.event.type === "message" ? (selected.event as StreamMessage) : null,
  );
  const shown = $derived(message ? readable(message) : null);
  const missing = $derived(conn.request?.missing ?? []);
  const results = $derived(conn.results);
  const failures = $derived(
    results
      ? [...results.checks, ...results.tests].filter((r) => !r.passed).length +
          results.script_errors.length
      : 0,
  );

  const label = $derived(
    {
      idle: "Idle",
      connecting: "Connecting",
      open: "Open",
      closing: "Closing",
      closed: "Closed",
    }[conn.status],
  );

  // Keep the newest entry in view, unless the reader scrolled up.
  $effect(() => {
    void visible.length;
    if (list && pinned) list.scrollTop = list.scrollHeight;
  });

  function onScroll(): void {
    if (!list) return;
    pinned = list.scrollHeight - list.scrollTop - list.clientHeight < 24;
  }

  function hint(error: string): string {
    if (error.startsWith("Could not connect"))
      return "Is the server running, and is the host and port right? Check the URL and the environment.";
    if (error.includes("timed out"))
      return "Raise the timeout in the request's Settings, or check that the server answers.";
    if (error.includes("certificate"))
      return "For a self-signed certificate, turn off Check TLS certificates in Settings.";
    if (error.includes("instead of opening a WebSocket"))
      return "Check the path, and that the server expects WebSocket there; the Handshake tab shows what was sent.";
    return "";
  }

  function systemText(entry: LogEntry): string {
    const event = entry.event;
    switch (event.type) {
      case "connecting":
        return `Connecting to ${event.url}`;
      case "open":
        return `Connected · ${event.status} · ${millis(event.time_ms)}${event.protocol ? ` · subprotocol ${event.protocol}` : ""}`;
      case "closed":
        return closeText(event.code, event.reason, event.by_client);
      case "note":
      case "warning":
      case "error":
        return event.message;
      default:
        return "";
    }
  }

  function tag(message: StreamMessage): string {
    if (message.event) return message.event;
    if (message.ack !== undefined) return `ack ${message.ack}`;
    return message.name ?? "";
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const messages = visible.filter((entry) => entry.event.type === "message");
    if (messages.length === 0) return;
    event.preventDefault();
    const at = messages.findIndex((entry) => entry.seq === conn.selected);
    const next =
      event.key === "ArrowDown"
        ? Math.min(messages.length - 1, at + 1)
        : Math.max(0, at < 0 ? messages.length - 1 : at - 1);
    conn.selected = messages[next]!.seq;
    list
      ?.querySelector<HTMLElement>(`[data-seq="${conn.selected}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }
</script>

<div class="live">
  <header class="head halftone-soft" class:open={conn.status === "open"}>
    <span
      class="state bitmap"
      class:ok={conn.status === "open"}
      class:bad={conn.status === "closed" && conn.error !== ""}
      class:busy={conn.status === "connecting" || conn.status === "closing"}
    >
      {#if conn.status === "connecting" || conn.status === "closing"}<span
          class="scan"
          aria-hidden="true"
        ></span>{/if}
      {label}
    </span>
    <dl class="facts">
      {#if conn.handshake}
        <div>
          <dt class="bitmap">Status</dt>
          <dd class="num">{conn.handshake.status}</dd>
        </div>
        <div>
          <dt class="bitmap">Opened in</dt>
          <dd class="num">{millis(conn.handshake.time_ms)}</dd>
        </div>
        {#if conn.handshake.protocol}
          <div>
            <dt class="bitmap">Protocol</dt>
            <dd class="mono">{conn.handshake.protocol}</dd>
          </div>
        {/if}
      {/if}
      <div>
        <dt class="bitmap">Sent</dt>
        <dd class="num">{conn.sent}</dd>
      </div>
      <div>
        <dt class="bitmap">Received</dt>
        <dd class="num">{conn.received}</dd>
      </div>
      {#if conn.closed?.code}
        <div>
          <dt class="bitmap">Close code</dt>
          <dd class="num">{conn.closed.code}</dd>
        </div>
      {/if}
    </dl>
  </header>

  {#if conn.error && conn.status === "closed"}
    <div class="error" role="alert">
      <p class="message">{conn.error}</p>
      {#if hint(conn.error)}<p class="hint">{hint(conn.error)}</p>{/if}
    </div>
  {/if}
  {#if missing.length}
    <div class="warning">
      <Icon name="warning" />
      <p>
        Not defined:
        {#each missing as name, i (name)}<code>{`{{${name}}}`}</code>{i < missing.length - 1
            ? ", "
            : ""}{/each}. They were sent as written.
      </p>
      <button class="btn quiet" onclick={() => openEnvironments()}>Environments</button>
    </div>
  {/if}

  <div class="tablist" use:overflowFade role="tablist" aria-label="Connection parts">
    <button
      class="tab"
      role="tab"
      aria-selected={pane === "messages"}
      onclick={() => (pane = "messages")}
    >
      Messages
      {#if total}<span class="count">{conn.sent + conn.received}</span>{/if}
    </button>
    <button
      class="tab"
      role="tab"
      aria-selected={pane === "handshake"}
      onclick={() => (pane = "handshake")}
    >
      Handshake
      {#if conn.handshake}<span class="count">{conn.handshake.headers.length}</span>{/if}
    </button>
    {#if results}
      <button
        class="tab"
        role="tab"
        aria-selected={pane === "tests"}
        onclick={() => (pane = "tests")}
      >
        Tests
        <span class="count" class:bad={failures > 0}
          >{failures > 0 ? failures : results.checks.length + results.tests.length}</span
        >
      </button>
    {/if}
  </div>

  {#if pane === "messages"}
    <div class="messages" class:with-detail={message !== null}>
      <div class="toolbar">
        <label class="search">
          <Icon name="search" />
          <input
            class="input"
            type="search"
            placeholder="Filter messages"
            aria-label="Filter messages"
            bind:value={conn.search}
          />
        </label>
        <select class="select" aria-label="Direction" bind:value={conn.direction}>
          <option value="all">Both ways</option>
          <option value="received">Received</option>
          <option value="sent">Sent</option>
        </select>
        <span class="muted shown">{visible.length} of {total}</span>
        <button
          class="btn quiet"
          onclick={() => clearLog(conn)}
          disabled={total === 0}
          title="Clear the log"
        >
          <Icon name="trash" /> Clear
        </button>
      </div>

      {#if total === 0}
        <div class="empty-log">
          {#if conn.status === "idle" || conn.status === "closed"}
            <p>
              {tab.draft.kind === "sse" ? "Events" : "Messages"} show here as they arrive. Press Connect,
              or <kbd>{keyLabel("mod+enter").join(" ")}</kbd>.
            </p>
          {:else}
            <p>Waiting for the first message…</p>
          {/if}
        </div>
      {:else}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <ol
          class="log"
          bind:this={list}
          onscroll={onScroll}
          onkeydown={onKeydown}
          aria-label="Messages"
        >
          {#if conn.dropped}
            <li class="sys note">
              {conn.dropped} older {conn.dropped === 1 ? "entry was" : "entries were"} dropped to keep
              the log small.
            </li>
          {/if}
          {#each visible as entry (entry.seq)}
            {#if entry.event.type === "message"}
              {@const m = entry.event as StreamMessage}
              <li>
                <button
                  class="row"
                  class:sent={m.direction === "sent"}
                  class:current={entry.seq === conn.selected}
                  data-seq={entry.seq}
                  onclick={() => (conn.selected = entry.seq === conn.selected ? -1 : entry.seq)}
                >
                  <span class="dir bitmap" title={m.direction === "sent" ? "Sent" : "Received"}
                    >{m.direction === "sent" ? "Out" : "In"}</span
                  >
                  <span class="time mono">{clock(entry.event.at)}</span>
                  {#if tag(m)}<span class="tag mono">{tag(m)}</span>{/if}
                  <span class="text mono"
                    >{m.binary ? `Binary · ${m.data.slice(0, 80)}` : preview(m)}</span
                  >
                  <span class="size num">{bytes(m.size)}</span>
                </button>
              </li>
            {:else}
              <li class="sys {entry.event.type}">
                <span class="time mono">{clock(entry.event.at)}</span>
                <span>{systemText(entry)}</span>
              </li>
            {/if}
          {/each}
        </ol>
      {/if}

      {#if message && shown}
        <section class="detail" aria-label="Selected message">
          <div class="detail-head">
            <span class="bitmap">{message.direction === "sent" ? "Sent" : "Received"}</span>
            {#if message.event}<span class="mono">{message.event}</span>{/if}
            {#if message.id}<span class="muted">id {message.id}</span>{/if}
            {#if message.ack !== undefined}<span class="muted">ack {message.ack}</span>{/if}
            {#if message.retry !== undefined}<span class="muted">retry {message.retry} ms</span
              >{/if}
            <span class="muted"
              >{bytes(message.size)}{message.truncated ? " · cut short" : ""}{message.binary
                ? " · binary, in Base64"
                : ""}</span
            >
            <span class="spacer"></span>
            <button class="btn quiet" onclick={() => void copyText(message.data, "Message copied")}>
              <Icon name="copy" /> Copy
            </button>
            <button
              class="icon-btn"
              aria-label="Close the message"
              onclick={() => (conn.selected = -1)}><Icon name="close" /></button
            >
          </div>
          {#key selected?.seq}
            <Editor
              value={shown.text}
              readonly
              wrap
              language={shown.json ? "json" : "text"}
              label="Message content"
            />
          {/key}
        </section>
      {/if}
    </div>
  {:else if pane === "tests" && results}
    <div class="handshake">
      {#each results.script_errors as scriptError, i (i)}
        <div class="result fail">
          <span class="mark"><Icon name="warning" size={7} /></span>
          <div>
            <p class="name">Script in {scriptError.origin} failed</p>
            <p class="why">{scriptError.message}</p>
          </div>
        </div>
      {/each}
      {#if results.checks.length}
        <h3 class="bitmap">Checks</h3>
        {#each results.checks as result, i (i)}
          <div class="result" class:fail={!result.passed}>
            <span class="mark"><Icon name={result.passed ? "check" : "close"} size={7} /></span>
            <div>
              <p class="name mono">{result.name}</p>
              {#if result.message}<p class="why">{result.message}</p>{/if}
            </div>
          </div>
        {/each}
      {/if}
      {#if results.tests.length}
        <h3 class="bitmap">Script tests</h3>
        {#each results.tests as result, i (i)}
          <div class="result" class:fail={!result.passed} class:skip={result.skipped}>
            <span class="mark"
              ><Icon
                name={result.skipped ? "more" : result.passed ? "check" : "close"}
                size={7}
              /></span
            >
            <div>
              <p class="name">{result.name}</p>
              {#if result.message}<p class="why">{result.message}</p>{/if}
            </div>
          </div>
        {/each}
      {/if}
      {#if results.logs.length}
        <h3 class="bitmap">Console</h3>
        <ol class="logs">
          {#each results.logs as log, i (i)}
            <li class={log.level}>
              <span class="origin bitmap">{log.origin}</span>
              <pre>{log.message}</pre>
            </li>
          {/each}
        </ol>
      {/if}
      {#each results.warnings as warning, i (i)}
        <p class="why">{warning}</p>
      {/each}
    </div>
  {:else}
    <div class="handshake">
      {#if conn.request}
        <h3 class="bitmap">Request</h3>
        <p class="line mono"><strong>{conn.request.method}</strong> {conn.request.url}</p>
        <dl class="pairs">
          {#each conn.request.headers as header, i (i)}
            <dt class="mono">{header.name}</dt>
            <dd class="mono">{header.value}</dd>
          {/each}
        </dl>
      {/if}
      {#if conn.handshake}
        <h3 class="bitmap">Response</h3>
        <p class="line mono"><strong>{conn.handshake.status}</strong></p>
        <dl class="pairs">
          {#each conn.handshake.headers as header, i (i)}
            <dt class="mono">{header.name}</dt>
            <dd class="mono">
              <button
                class="value"
                title="Copy"
                onclick={() => void copyText(header.value, `${header.name} copied`)}
                >{header.value}</button
              >
            </dd>
          {/each}
        </dl>
      {/if}
      {#if !conn.request}
        <p class="none">Connect to see the request that opens the connection and the answer.</p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .live {
    display: grid;
    grid-template-rows: auto auto auto auto minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .head {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--s3) var(--s5);
    padding: var(--s3) var(--s4);
    border-bottom: 1px solid var(--rule);
  }

  .state {
    display: inline-flex;
    align-items: center;
    gap: var(--s2);
    padding: 4px var(--s2);
    background: var(--paper);
    font-size: var(--b2);
    line-height: 1;
    color: var(--ink-3);
  }

  .conn.ok {
    color: var(--ok);
  }

  .conn.bad {
    color: var(--error);
  }

  .conn.busy {
    color: var(--ink);
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

  .facts {
    display: flex;
    flex-wrap: wrap;
    margin: 0;
    background: var(--paper);
  }

  .facts div {
    display: grid;
    gap: 3px;
    padding: 4px var(--s3);
    border-right: 1px solid var(--rule-3);
  }

  .facts div:last-child {
    border-right: 0;
  }

  .facts dt {
    color: var(--ink-3);
  }

  .facts dd {
    margin: 0;
    font-weight: 650;
  }

  .error {
    padding: var(--s3) var(--s4);
    background: var(--accent-wash);
    border-bottom: 1px solid var(--error);
  }

  .error p {
    margin: 0;
  }

  .message {
    color: var(--error);
    font-weight: 600;
    word-break: break-word;
  }

  .hint {
    margin-top: 4px !important;
    color: var(--ink-2);
  }

  .warning {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: 6px var(--s4);
    background: var(--warn-wash);
    border-bottom: 1px solid var(--rule-3);
  }

  .warning p {
    flex: 1;
    margin: 0;
  }

  .warning :global(.icon) {
    color: var(--warn);
  }

  code {
    font-family: var(--font-code);
  }

  .tablist {
    padding: 0 var(--s2);
  }

  .messages {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    min-height: 0;
  }

  .messages.with-detail {
    grid-template-rows: auto minmax(0, 1fr) minmax(120px, 40%);
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: var(--s2) var(--s4);
    border-bottom: 1px solid var(--rule-3);
  }

  .search {
    position: relative;
    flex: 1 1 160px;
    min-width: 0;
  }

  .search :global(.icon) {
    position: absolute;
    left: 8px;
    top: 50%;
    margin-top: -7px;
    color: var(--ink-3);
    pointer-events: none;
  }

  .search .input {
    width: 100%;
    padding-left: 28px;
  }

  .shown {
    white-space: nowrap;
    font-size: var(--fs-meta);
  }

  .empty-log {
    padding: var(--s4);
    color: var(--ink-3);
  }

  .empty-log p {
    margin: 0;
    max-width: 52ch;
  }

  .log {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow: auto;
    min-height: 0;
  }

  .row {
    display: grid;
    grid-template-columns: 28px 96px auto minmax(0, 1fr) auto;
    align-items: baseline;
    gap: var(--s2);
    width: 100%;
    min-height: 28px;
    padding: 4px var(--s4);
    border: 0;
    border-bottom: 1px solid var(--rule-3);
    background: transparent;
    color: var(--ink);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .row:hover {
    background: var(--sunken);
  }

  .row.current {
    background: var(--ink);
    color: var(--paper);
  }

  .row.current .time,
  .row.current .size,
  .row.current .dir {
    color: var(--paper);
  }

  .dir {
    color: var(--ok);
  }

  .row.sent .dir {
    color: var(--redirect);
  }

  .time,
  .size {
    color: var(--ink-3);
    font-size: var(--fs-meta);
    white-space: nowrap;
  }

  .tag {
    max-width: 18ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--var-ink);
  }

  .row.current .tag {
    color: var(--paper);
  }

  .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sys {
    display: grid;
    grid-template-columns: 96px minmax(0, 1fr);
    gap: var(--s2);
    padding: 5px var(--s4) 5px calc(var(--s4) + 36px);
    border-bottom: 1px solid var(--rule-3);
    background: var(--paper);
    color: var(--ink-2);
    word-break: break-word;
  }

  .sys.open {
    color: var(--ok);
  }

  .sys.warning {
    background: var(--warn-wash);
    color: var(--ink);
  }

  .sys.error {
    background: var(--accent-wash);
    color: var(--error);
  }

  .sys.note {
    color: var(--ink-3);
  }

  .detail {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    min-height: 0;
    border-top: 1px solid var(--rule);
  }

  .detail-head {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--s2) var(--s3);
    padding: 4px var(--s2) 4px var(--s4);
    border-bottom: 1px solid var(--rule-3);
    font-size: var(--fs-meta);
  }

  .spacer {
    flex: 1;
  }

  .handshake {
    overflow: auto;
    padding: var(--s3) var(--s4);
  }

  h3 {
    margin: var(--s3) 0 var(--s2);
    color: var(--ink-3);
  }

  h3:first-child {
    margin-top: 0;
  }

  .line {
    margin: 0 0 var(--s2);
    word-break: break-all;
  }

  .result {
    display: grid;
    grid-template-columns: 14px minmax(0, 1fr);
    gap: var(--s2);
    padding: 6px 0;
    border-bottom: 1px solid var(--rule-3);
  }

  .result p {
    margin: 0;
  }

  .result .mark {
    display: grid;
    place-items: center;
    width: 14px;
    height: 14px;
    margin-top: 2px;
    background: var(--ok);
    color: var(--paper);
  }

  .result.fail .mark {
    background: var(--error);
  }

  .result.skip .mark {
    background: var(--t4);
  }

  .why {
    color: var(--error);
    font-size: var(--fs-meta);
  }

  .logs {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .logs li {
    display: grid;
    grid-template-columns: 110px minmax(0, 1fr);
    gap: var(--s2);
    padding: 4px 0;
    border-bottom: 1px solid var(--rule-3);
  }

  .logs .origin {
    color: var(--ink-3);
    padding-top: 3px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .logs pre {
    margin: 0;
    font: var(--fs-code) / 1.5 var(--font-code);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .count.bad {
    color: var(--error);
  }

  .pairs {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    margin: 0;
  }

  .pairs dt,
  .pairs dd {
    padding: 5px var(--s3) 5px 0;
    border-bottom: 1px solid var(--rule-3);
  }

  .pairs dt {
    color: var(--ink-2);
  }

  .pairs dd {
    margin: 0;
    word-break: break-all;
  }

  .value {
    padding: 0;
    border: 0;
    background: none;
    text-align: left;
    color: inherit;
    font: inherit;
  }

  .value:hover {
    text-decoration: underline;
  }

  .none {
    margin: 0;
    color: var(--ink-3);
  }

  @media (max-width: 520px) {
    .row {
      grid-template-columns: 24px minmax(0, 1fr) auto;
    }

    .row .time,
    .row .tag {
      display: none;
    }

    .sys {
      grid-template-columns: minmax(0, 1fr);
      padding-left: var(--s4);
    }

    .sys .time {
      display: none;
    }
  }
</style>
