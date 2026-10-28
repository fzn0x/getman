<script lang="ts">
  import { onMount } from "svelte";
  import { bytes, millis, statusClass } from "../../lib/format";
  import { keyLabel } from "../../lib/keys";
  import type { Execution } from "../../lib/types";
  import TimingPanel from "./TimingPanel.svelte";

  interface Props {
    execution: Execution | null;
    sending: boolean;
    /** The tone, when the status code is not HTTP's. */
    tone?: ReturnType<typeof statusClass>;
    /** What to say while the request is out. */
    busy?: string;
    /** The button that sends, for the hint. */
    action?: string;
  }

  let { execution, sending, tone, busy = "Sending", action = "Send" }: Props = $props();
  let panel = $state<{ x: number; y: number } | null>(null);
  let timeButton = $state<HTMLButtonElement>();

  function togglePanel(event: MouseEvent): void {
    if (panel) {
      panel = null;
      return;
    }
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    panel = { x: rect.left, y: rect.bottom + 6 };
  }

  function closePanel(): void {
    panel = null;
    timeButton?.focus();
  }

  let field: HTMLDivElement;
  let width = $state(480);
  let height = $state(132);

  onMount(() => {
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      width = Math.floor(entry.contentRect.width);
      height = Math.floor(entry.contentRect.height);
    });
    observer.observe(field);
    return () => observer.disconnect();
  });

  const response = $derived(execution?.response ?? null);
  const kind = $derived(
    response
      ? (tone ?? statusClass(response.status))
      : execution?.error && !execution.cancelled
        ? "server"
        : "info",
  );
  // A new response closes the panel of the last one.
  $effect(() => {
    void response;
    panel = null;
  });
  const results = $derived(execution ? [...execution.tests, ...execution.checks] : []);
  const passed = $derived(results.filter((r) => r.passed && !r.skipped).length);
  const failed = $derived(results.filter((r) => !r.passed).length);

  // The field behind the numeral is a halftone whose dots grow with the
  // response time, on a log scale from 1 ms (a pale screen) to 10 s (near
  // solid). Dots are densest under the numeral and thin out to the right,
  // like a printed halftone tile.
  const STEP = 6;
  const darkness = $derived(
    response ? Math.min(1, Math.max(0, Math.log10(Math.max(response.time_ms, 1)) / 4)) : 0.12,
  );

  const dots = $derived.by(() => {
    const columns = Math.ceil(width / STEP);
    const rows = Math.ceil(height / STEP);
    const list: { x: number; y: number; r: number }[] = [];
    for (let c = 0; c < columns; c++) {
      const across = c / Math.max(columns - 1, 1);
      for (let row = 0; row < rows; row++) {
        const down = row / Math.max(rows - 1, 1);
        const falloff = 1 - 0.75 * across + 0.15 * (down - 0.5);
        const r = Math.max(0.35, (0.4 + 2.4 * darkness) * Math.max(falloff, 0.12));
        list.push({ x: c * STEP + STEP / 2, y: row * STEP + STEP / 2, r: Math.min(r, STEP / 2) });
      }
    }
    return list;
  });
</script>

<div
  class="specimen {kind}"
  class:sending
  class:idle={!execution && !sending}
  title={response
    ? `The dots darken with the response time: ${millis(response.time_ms)}`
    : undefined}
>
  <div class="field" bind:this={field} aria-hidden="true">
    <svg {width} {height} viewBox="0 0 {width} {height}">
      {#each dots as dot, i (i)}
        <circle cx={dot.x} cy={dot.y} r={dot.r} />
      {/each}
    </svg>
  </div>

  <div class="setting">
    <div class="line">
      <span class="numeral bitmap" aria-live="polite">
        {#if sending}
          <span class="dots" aria-label={busy}></span>
        {:else if response}
          {response.status}
        {:else if execution?.skipped}
          SKIP
        {:else if execution?.cancelled}
          STOP
        {:else if execution}
          ERR
        {:else}
          ---
        {/if}
      </span>
      <span class="reason bitmap">
        {#if sending}
          {busy}
        {:else if response}
          {response.status_text || "Status"}
        {:else if execution?.skipped}
          Skipped by a script
        {:else if execution?.cancelled}
          Cancelled
        {:else if execution}
          No response
        {:else}
          Not sent yet
        {/if}
      </span>
    </div>

    {#if response}
      <dl class="sizes">
        <div>
          <dt>Time</dt>
          <dd class="num">
            {#if response.timings}
              <button
                class="time"
                bind:this={timeButton}
                aria-haspopup="dialog"
                aria-expanded={panel !== null}
                title="Where the time went, and the connection"
                onclick={togglePanel}>{millis(response.time_ms)}</button
              >
            {:else}
              {millis(response.time_ms)}
            {/if}
          </dd>
        </div>
        <div>
          <dt>First byte</dt>
          <dd class="num">{millis(response.headers_ms)}</dd>
        </div>
        <div>
          <dt>Size</dt>
          <dd class="num">{bytes(response.size.body)}</dd>
        </div>
        {#if response.hops?.length}
          <div>
            <dt>Redirects</dt>
            <dd class="num" title="Select the time to see the redirects Getman followed">
              {response.hops.length}
            </dd>
          </div>
        {/if}
        <div>
          <dt>Protocol</dt>
          <dd>
            {response.http_version.replace("HTTP/", "")}
            {#if response.http3_available}
              <span
                class="offer bitmap"
                title="The server offers HTTP/3 in its Alt-Svc header. Set the HTTP version to HTTP/3 in Settings to use it."
                >H3 offered</span
              >
            {/if}
          </dd>
        </div>
        {#if results.length}
          <div>
            <dt>Tests</dt>
            <dd class="num" class:bad={failed > 0} class:good={failed === 0}>
              {passed}/{results.length}
            </dd>
          </div>
        {/if}
      </dl>
    {:else if !execution && !sending}
      <p class="hint">
        Press {#each keyLabel("mod+enter") as key (key)}<kbd>{key}</kbd>{" "}{/each}or {action}.
      </p>
    {/if}
  </div>
</div>

{#if panel && response}
  <TimingPanel {response} x={panel.x} y={panel.y} onClose={closePanel} />
{/if}

<style>
  .specimen {
    --tone: var(--ink);
    --dot: color-mix(in srgb, var(--tone) var(--dot-strength), transparent);
    position: relative;
    min-height: 132px;
    border-bottom: 1px solid var(--rule);
    overflow: hidden;
  }

  .specimen.ok {
    --tone: var(--ok);
  }

  .specimen.redirect {
    --tone: var(--redirect);
  }

  .specimen.client,
  .specimen.server {
    --tone: var(--error);
  }

  .specimen.idle,
  .specimen.sending {
    --tone: var(--ink-3);
  }

  .field {
    position: absolute;
    inset: 0;
  }

  .field svg {
    display: block;
  }

  circle {
    fill: var(--dot);
  }

  .sending circle {
    animation: scan 1.2s steps(4) infinite;
  }

  @keyframes scan {
    50% {
      fill: color-mix(in srgb, var(--ink) 30%, transparent);
    }
  }

  .setting {
    position: relative;
    display: grid;
    gap: 10px;
    padding: var(--s3) var(--s4);
  }

  .line {
    display: flex;
    align-items: flex-end;
    gap: var(--s3);
    min-width: 0;
  }

  .numeral {
    font-size: var(--b8);
    line-height: 0.82;
    color: var(--tone);
    padding: 2px 6px 0 0;
  }

  .server .numeral {
    padding: 4px 6px 0;
    background: var(--error);
    color: var(--paper);
  }

  .idle .numeral {
    color: var(--t4);
  }

  .reason {
    padding: 2px 6px;
    background: var(--paper);
    font-size: var(--b2);
    line-height: 1;
    color: var(--ink);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .idle .reason {
    color: var(--ink-3);
  }

  .dots {
    display: inline-block;
    width: 3ch;
    height: 0.7em;
    background-image: radial-gradient(circle, var(--ink-2) 3px, transparent 4px);
    background-size: 22px 22px;
    animation: type-on 1.1s steps(4) infinite;
  }

  @keyframes type-on {
    from {
      clip-path: inset(0 100% 0 0);
    }
    to {
      clip-path: inset(0 0 0 0);
    }
  }

  .sizes {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 0;
    margin: 0;
    width: max-content;
    max-width: 100%;
  }

  .sizes div {
    display: grid;
    gap: 4px;
    padding: 6px 10px 6px 8px;
    background: var(--paper);
    border-left: 1px solid var(--rule-2);
  }

  .sizes div:first-child {
    padding-left: 0;
    border-left: 0;
  }

  .sizes dt {
    font-family: var(--font-bitmap);
    font-size: var(--b1);
    line-height: 1;
    text-transform: uppercase;
    color: var(--ink-3);
    -webkit-font-smoothing: none;
  }

  .sizes dd {
    margin: 0;
    font-weight: 650;
    line-height: 1.1;
  }

  .sizes dd.good {
    color: var(--ok);
  }

  .time {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    color: inherit;
    text-decoration: underline dotted;
    text-underline-offset: 3px;
    cursor: pointer;
  }

  .time:hover,
  .time[aria-expanded="true"] {
    background: var(--ink);
    color: var(--paper);
    text-decoration: none;
  }

  .offer {
    margin-left: 4px;
    padding: 1px 3px;
    border: 1px solid var(--rule-2);
    font-size: var(--b1);
    font-weight: 400;
    color: var(--ink-2);
    vertical-align: 1px;
  }

  .sizes dd.bad {
    color: var(--error);
  }

  .hint {
    margin: 0;
    padding: 4px 6px 4px 0;
    width: max-content;
    background: var(--paper);
    color: var(--ink-3);
  }

  @media (max-width: 520px) {
    .numeral {
      font-size: var(--b4);
    }
  }
</style>
