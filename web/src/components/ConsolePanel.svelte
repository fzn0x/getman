<script lang="ts">
  import { tick } from "svelte";
  import {
    filterRows,
    filtering,
    rowText,
    windowOf,
    CONSOLE_KINDS,
    type ConsoleRow,
  } from "../lib/console";
  import {
    clearConsole,
    consoleRows,
    consoleView,
    setConsoleHeight,
    toggleConsole,
    toggleKind,
  } from "../lib/console.svelte";
  import { copyText } from "../lib/clipboard";
  import { bytes, millis, statusClass } from "../lib/format";
  import Icon from "./Icon.svelte";

  const ROW = 24;

  const all = $derived(consoleRows());
  const rows = $derived(filterRows(all, consoleView.filter));
  const selected = $derived(rows.find((row) => row.id === consoleView.selected) ?? null);

  let scroller = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let viewport = $state(200);
  /** Follow new rows while the list is scrolled to the end. */
  let stuck = true;

  const view = $derived(windowOf(rows.length, ROW, scrollTop, viewport));
  const shown = $derived(rows.slice(view.from, view.to));

  function onScroll(): void {
    if (!scroller) return;
    scrollTop = scroller.scrollTop;
    stuck = scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - ROW;
  }

  $effect(() => {
    // Follow the end of the list as rows arrive.
    void rows.length;
    if (!stuck || !scroller) return;
    const list = scroller;
    void tick().then(() => {
      list.scrollTop = list.scrollHeight;
      scrollTop = list.scrollTop;
    });
  });

  function time(at: number): string {
    return new Date(at).toLocaleTimeString([], { hour12: false });
  }

  function tone(row: ConsoleRow): string {
    if (row.status === null) return row.failed ? "client" : "info";
    if (row.kind === "grpc") return row.status === 0 ? "ok" : "client";
    if (row.kind === "realtime") return row.failed ? "client" : "ok";
    return statusClass(row.status);
  }

  function statusLabel(row: ConsoleRow): string {
    if (row.status === null) return row.failed ? "Failed" : "";
    return row.kind === "grpc" ? row.statusText || String(row.status) : String(row.status);
  }

  function move(event: KeyboardEvent): void {
    if (
      event.key !== "ArrowDown" &&
      event.key !== "ArrowUp" &&
      event.key !== "Home" &&
      event.key !== "End"
    )
      return;
    if (rows.length === 0) return;
    event.preventDefault();
    const at = rows.findIndex((row) => row.id === consoleView.selected);
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? rows.length - 1
          : Math.max(
              0,
              Math.min(
                rows.length - 1,
                (at < 0 ? rows.length : at) + (event.key === "ArrowDown" ? 1 : -1),
              ),
            );
    const row = rows[next];
    if (!row || !scroller) return;
    consoleView.selected = row.id;
    const top = next * ROW;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + ROW > scroller.scrollTop + scroller.clientHeight) {
      scroller.scrollTop = top + ROW - scroller.clientHeight;
    }
  }

  function startResize(event: PointerEvent): void {
    const handle = event.currentTarget as HTMLElement;
    handle.setPointerCapture(event.pointerId);
    const startY = event.clientY;
    const startHeight = consoleView.height;
    const drag = (e: PointerEvent) => setConsoleHeight(startHeight + (startY - e.clientY));
    const up = () => {
      handle.removeEventListener("pointermove", drag);
      handle.removeEventListener("pointerup", up);
    };
    handle.addEventListener("pointermove", drag);
    handle.addEventListener("pointerup", up);
  }

  function resizeWithKeys(event: KeyboardEvent): void {
    if (event.key === "ArrowUp") setConsoleHeight(consoleView.height + 24);
    else if (event.key === "ArrowDown") setConsoleHeight(consoleView.height - 24);
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && !(event.target instanceof HTMLInputElement)) {
      event.preventDefault();
      toggleConsole();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section
  class="console"
  aria-label="Console"
  style:height="{consoleView.height}px"
  onkeydown={onKeydown}
>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="grip"
    role="separator"
    aria-orientation="horizontal"
    aria-label="Resize the console"
    aria-valuenow={consoleView.height}
    tabindex="0"
    onpointerdown={startResize}
    onkeydown={resizeWithKeys}
  ></div>
  <header class="head">
    <h2 class="bitmap">Console</h2>
    <input
      class="input find"
      type="search"
      placeholder="Filter by URL, method, status or text"
      aria-label="Filter the console"
      bind:value={consoleView.filter.text}
    />
    <label class="errors">
      <input type="checkbox" class="check" bind:checked={consoleView.filter.errorsOnly} />
      Errors only
    </label>
    <div class="kinds" role="group" aria-label="Show">
      {#each CONSOLE_KINDS as option (option.kind)}
        {@const on = !consoleView.filter.hidden.includes(option.kind)}
        <button
          class="chip bitmap"
          class:on
          aria-pressed={on}
          onclick={() => toggleKind(option.kind)}
        >
          {option.label}
        </button>
      {/each}
    </div>
    <span class="count muted num" aria-live="polite">
      {filtering(consoleView.filter) ? `${rows.length} of ${all.length}` : all.length} sent
    </span>
    <button class="btn quiet" onclick={clearConsole} disabled={all.length === 0}>Clear</button>
    <button
      class="icon-btn"
      aria-label="Close the console"
      title="Close (Esc)"
      onclick={toggleConsole}
    >
      <Icon name="close" />
    </button>
  </header>

  <div class="body">
    <div class="list">
      {#if all.length === 0}
        <p class="empty-note muted">
          Requests you send from this window show here: the method, URL, status, time and size, with
          headers, bodies and script logs. Nothing is kept once you close the page.
        </p>
      {:else if rows.length === 0}
        <p class="empty-note muted">No request matches the filter.</p>
      {:else}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <div
          class="scroller"
          role="listbox"
          aria-label="Requests sent"
          tabindex="0"
          bind:this={scroller}
          bind:clientHeight={viewport}
          onscroll={onScroll}
          onkeydown={move}
        >
          <div style:height="{view.before}px" aria-hidden="true"></div>
          {#each shown as row (row.id)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <div
              class="row"
              class:on={row.id === consoleView.selected}
              class:failed={row.failed}
              role="option"
              tabindex="-1"
              aria-selected={row.id === consoleView.selected}
              style:height="{ROW}px"
              onclick={() => (consoleView.selected = row.id)}
            >
              <span class="at muted num">{time(row.at)}</span>
              <span class="kind bitmap muted"
                >{row.kind === "realtime" ? "RT" : row.kind === "graphql" ? "GQL" : row.kind}</span
              >
              <span class="method bitmap">{row.method}</span>
              <span class="status status-{tone(row)} num">{statusLabel(row)}</span>
              <span class="url mono" title={row.url}>{row.url || row.name}</span>
              <span class="took muted num">{row.timeMs === null ? "" : millis(row.timeMs)}</span>
              <span class="size muted num">{row.size === null ? "" : bytes(row.size)}</span>
            </div>
          {/each}
          <div style:height="{view.after}px" aria-hidden="true"></div>
        </div>
      {/if}
    </div>

    <div class="detail">
      {#if selected}
        {@const row = selected}
        <div class="actions">
          <strong class="line mono">{row.method} {row.url}</strong>
          <button class="btn quiet" onclick={() => void copyText(rowText(row), "Copied")}>
            <Icon name="copy" /> Copy
          </button>
        </div>
        {#if row.name}<p class="muted name">{row.name}</p>{/if}
        {#if row.summary}<p class="summary">{row.summary}</p>{/if}
        {#if row.problems.length}
          <h3 class="bitmap bad">Problems</h3>
          <ul class="problems">
            {#each row.problems as problem, i (i)}<li>{problem}</li>{/each}
          </ul>
        {/if}
        {#if row.logs.length}
          <h3 class="bitmap">Script log</h3>
          <ul class="logs mono">
            {#each row.logs as entry, i (i)}
              <li class="level-{entry.level}">
                <span class="muted">{entry.origin}</span>
                {entry.message}
              </li>
            {/each}
          </ul>
        {/if}
        {#each [{ title: "Request headers", headers: row.requestHeaders }, { title: "Response headers", headers: row.responseHeaders }] as group (group.title)}
          {#if group.headers.length}
            <h3 class="bitmap">{group.title}</h3>
            <dl class="headers mono">
              {#each group.headers as header, i (i)}
                <dt>{header.name}</dt>
                <dd>{header.value}</dd>
              {/each}
            </dl>
          {/if}
        {/each}
        {#if row.requestBody}
          <h3 class="bitmap">Request body{row.requestBodyCut ? " (cut)" : ""}</h3>
          <pre class="mono body-text">{row.requestBody}</pre>
        {/if}
        {#if row.responseBody}
          <h3 class="bitmap">Response body{row.responseBodyCut ? " (cut)" : ""}</h3>
          <pre class="mono body-text">{row.responseBody}</pre>
        {/if}
        {#if row.requestBodyCut || row.responseBodyCut}
          <p class="muted note">The console keeps the first 16 KB of a body.</p>
        {/if}
      {:else}
        <p class="empty-note muted">Choose a request to see its headers, bodies and logs.</p>
      {/if}
    </div>
  </div>
</section>

<style>
  .console {
    position: relative;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    min-height: 0;
    border-top: 1px solid var(--rule);
    background: var(--paper);
  }

  .grip {
    position: absolute;
    z-index: 2;
    inset: -3px 0 auto;
    height: 6px;
    cursor: row-resize;
  }

  .grip:hover,
  .grip:focus-visible {
    background: var(--accent);
    height: 2px;
    inset-block-start: -1px;
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--s2) var(--s3);
    padding: var(--s1) var(--s3);
    border-bottom: 1px solid var(--rule-2);
  }

  h2 {
    margin: 0;
  }

  .find {
    flex: 1 1 180px;
    min-width: 120px;
    max-width: 360px;
  }

  .errors {
    display: flex;
    align-items: center;
    gap: var(--s2);
    white-space: nowrap;
  }

  .kinds {
    display: flex;
    gap: 2px;
  }

  .chip {
    height: 22px;
    padding: 0 var(--s2);
    border: 1px solid var(--rule-2);
    background: none;
    color: var(--ink-3);
  }

  .chip.on {
    border-color: var(--ink);
    background: var(--ink);
    color: var(--paper);
  }

  .count {
    margin-left: auto;
    font-size: var(--fs-meta);
    white-space: nowrap;
  }

  .body {
    display: grid;
    grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
    min-height: 0;
  }

  .list,
  .detail {
    min-height: 0;
    min-width: 0;
  }

  .list {
    border-right: 1px solid var(--rule-2);
  }

  .scroller {
    height: 100%;
    overflow: auto;
  }

  .row {
    display: grid;
    grid-template-columns: 62px 38px 52px 62px minmax(0, 1fr) 64px 64px;
    align-items: center;
    gap: var(--s2);
    padding: 0 var(--s3);
    border-bottom: 1px solid var(--rule-3);
    font-size: var(--fs-meta);
    cursor: default;
  }

  .row:hover {
    background: var(--sunken);
  }

  .row.on {
    background: var(--ink);
    color: var(--paper);
  }

  .row.on .muted,
  .row.on .status {
    color: inherit;
  }

  .row.failed:not(.on) .url {
    color: var(--error);
  }

  .url,
  .method,
  .kind {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .took,
  .size {
    text-align: right;
  }

  .detail {
    overflow: auto;
    padding: var(--s2) var(--s3) var(--s3);
    font-size: var(--fs-meta);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }

  .line {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name,
  .summary,
  .note {
    margin: var(--s1) 0;
  }

  h3 {
    margin: var(--s3) 0 var(--s1);
    color: var(--ink-3);
  }

  h3.bad {
    color: var(--error);
  }

  .problems,
  .logs {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .problems li {
    color: var(--error);
  }

  .logs li {
    overflow-wrap: anywhere;
  }

  .level-warn {
    color: var(--warn);
  }

  .level-error {
    color: var(--error);
  }

  .headers {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 1px var(--s3);
    margin: 0;
  }

  .headers dt {
    color: var(--ink-3);
  }

  .headers dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .body-text {
    max-height: 240px;
    margin: 0;
    padding: var(--s2);
    overflow: auto;
    border: 1px solid var(--rule-3);
    background: var(--field);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .empty-note {
    margin: 0;
    padding: var(--s3);
  }

  @media (max-width: 760px) {
    .body {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: minmax(0, 1fr) minmax(0, 1fr);
    }

    .list {
      border-right: 0;
      border-bottom: 1px solid var(--rule-2);
    }

    .row {
      grid-template-columns: 62px 52px 62px minmax(0, 1fr);
    }

    .kind,
    .took,
    .size {
      display: none;
    }
  }
</style>
