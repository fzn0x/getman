<script lang="ts">
  import { onMount } from "svelte";
  import { bytes, millis, statusClass } from "../../lib/format";
  import { keyLabel } from "../../lib/keys";
  import type { Execution } from "../../lib/types";

  let { execution, sending }: { execution: Execution | null; sending: boolean } = $props();

  let gauge: HTMLDivElement;
  let width = $state(480);

  onMount(() => {
    const observer = new ResizeObserver(([entry]) => {
      if (entry) width = Math.floor(entry.contentRect.width);
    });
    observer.observe(gauge);
    return () => observer.disconnect();
  });

  const response = $derived(execution?.response ?? null);
  const kind = $derived(
    response ? statusClass(response.status) : execution?.error ? "server" : "info",
  );
  const results = $derived(execution ? [...execution.tests, ...execution.checks] : []);
  const passed = $derived(results.filter((r) => r.passed && !r.skipped).length);
  const failed = $derived(results.filter((r) => !r.passed).length);

  // The gauge spans 1 ms to 10 s on a log scale; dots grow towards the
  // measured time, so a slow answer reads dark from across the room.
  const STEP = 4;
  const ROWS = 5;
  const position = (ms: number) => Math.min(1, Math.max(0, Math.log10(Math.max(ms, 1)) / 4));
  const ticks = [
    { label: "10 ms", at: position(10) },
    { label: "100 ms", at: position(100) },
    { label: "1 s", at: position(1000) },
    { label: "10 s", at: position(10_000) },
  ];

  const dots = $derived.by(() => {
    const columns = Math.max(1, Math.floor(width / STEP));
    const reach = response ? position(response.time_ms) : 0;
    const list: { x: number; y: number; r: number; on: boolean }[] = [];
    for (let c = 0; c < columns; c++) {
      const at = (c + 0.5) / columns;
      const on = at <= reach;
      const weight = on ? 0.45 + 0.55 * (at / Math.max(reach, 0.0001)) : 0;
      for (let row = 0; row < ROWS; row++) {
        list.push({
          x: c * STEP + STEP / 2,
          y: row * STEP + STEP / 2,
          r: on ? 0.55 + weight * 1.2 : 0.45,
          on,
        });
      }
    }
    return list;
  });

  const firstByte = $derived(response ? position(response.headers_ms) : null);
</script>

<div class="specimen {kind}" class:sending class:idle={!execution && !sending}>
  <div class="numeral bitmap" aria-live="polite">
    {#if sending}
      <span class="dots" aria-label="Sending"></span>
    {:else if response}
      {response.status}
    {:else if execution?.skipped}
      SKIP
    {:else if execution}
      ERR
    {:else}
      ---
    {/if}
  </div>

  <div class="facts">
    <p class="reason bitmap">
      {#if sending}
        Sending
      {:else if response}
        {response.status_text || "Status"}
      {:else if execution?.skipped}
        Skipped by a script
      {:else if execution}
        No response
      {:else}
        Not sent yet
      {/if}
    </p>
    {#if response}
      <dl class="stats">
        <div>
          <dt>Time</dt>
          <dd class="num">{millis(response.time_ms)}</dd>
        </div>
        <div>
          <dt>Size</dt>
          <dd class="num">{bytes(response.size.body)}</dd>
        </div>
        <div>
          <dt>Protocol</dt>
          <dd>{response.http_version.replace("HTTP/", "")}</dd>
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
        Press {#each keyLabel("mod+enter") as key (key)}<kbd>{key}</kbd>{" "}{/each}or Send.
      </p>
    {/if}
  </div>

  <div class="gauge" bind:this={gauge} aria-hidden="true">
    <svg {width} height={ROWS * STEP} viewBox="0 0 {width} {ROWS * STEP}">
      {#each dots as dot, i (i)}
        <circle cx={dot.x} cy={dot.y} r={dot.r} class:on={dot.on} />
      {/each}
      {#if firstByte !== null}
        <rect class="ttfb" x={Math.round(firstByte * width)} y="0" width="1" height={ROWS * STEP} />
      {/if}
    </svg>
    <div class="ticks bitmap">
      {#each ticks as tick (tick.label)}
        <span style:left="{tick.at * 100}%">{tick.label}</span>
      {/each}
    </div>
  </div>
  {#if response}
    <p class="sr-only">
      Answered {response.status}
      {response.status_text} in {millis(response.time_ms)}, first byte after {millis(
        response.headers_ms,
      )}.
    </p>
  {/if}
</div>

<style>
  .specimen {
    --tone: var(--ink);
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    grid-template-areas:
      "numeral facts"
      "gauge gauge";
    align-items: end;
    gap: 6px var(--s4);
    padding: var(--s3) var(--s4) var(--s2);
    border-bottom: 1px solid var(--rule);
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

  .numeral {
    grid-area: numeral;
    font-size: var(--b8);
    line-height: 0.82;
    color: var(--tone);
    min-width: 2ch;
  }

  .server .numeral {
    padding: 4px 6px 0;
    background: var(--error);
    color: var(--paper);
  }

  .idle .numeral {
    color: var(--t3);
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

  .facts {
    grid-area: facts;
    display: grid;
    gap: 8px;
    min-width: 0;
    padding-bottom: 2px;
  }

  .reason {
    margin: 0;
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

  .stats {
    display: flex;
    flex-wrap: wrap;
    gap: 2px var(--s4);
    margin: 0;
  }

  .stats div {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }

  .stats dt {
    font-family: var(--font-bitmap);
    font-size: var(--b1);
    text-transform: uppercase;
    color: var(--ink-3);
    -webkit-font-smoothing: none;
  }

  .stats dd {
    margin: 0;
    font-weight: 600;
  }

  .stats dd.good {
    color: var(--ok);
  }

  .stats dd.bad {
    color: var(--error);
  }

  .hint {
    margin: 0;
    color: var(--ink-3);
  }

  .gauge {
    grid-area: gauge;
    position: relative;
    min-width: 0;
    padding-bottom: 14px;
  }

  .gauge svg {
    display: block;
  }

  circle {
    fill: var(--t3);
  }

  circle.on {
    fill: var(--tone);
  }

  .sending circle {
    animation: scan 1.2s steps(6) infinite;
  }

  @keyframes scan {
    50% {
      fill: var(--ink-2);
    }
  }

  .ttfb {
    fill: var(--ink);
  }

  .ticks {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 11px;
    color: var(--ink-3);
  }

  .ticks span {
    position: absolute;
    transform: translateX(-50%);
    white-space: nowrap;
  }

  .ticks span:last-child {
    transform: translateX(-100%);
  }

  @media (max-width: 520px) {
    .numeral {
      font-size: var(--b4);
    }
  }
</style>
