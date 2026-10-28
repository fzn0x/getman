<script lang="ts">
  import { onMount, tick } from "svelte";
  import { millis } from "../../lib/format";
  import { commonName, expiry, phases, steps, timingNote } from "../../lib/timing";
  import type { Response } from "../../lib/types";

  interface Props {
    response: Response;
    x: number;
    y: number;
    onClose: () => void;
  }

  let { response, x, y, onClose }: Props = $props();
  let element: HTMLDivElement;
  let left = $state(0);
  let top = $state(0);

  const timings = $derived(response.timings);
  const list = $derived(timings ? phases(timings) : []);
  const note = $derived(timings ? timingNote(timings) : "");
  const route = $derived(steps(response));
  const tls = $derived(response.tls);
  const certificate = $derived(tls?.certificate);

  onMount(() => {
    left = x;
    top = y;
    void tick().then(() => {
      const rect = element.getBoundingClientRect();
      left = Math.max(8, Math.min(x, window.innerWidth - rect.width - 8));
      top = Math.max(8, Math.min(y, window.innerHeight - rect.height - 8));
      element.focus();
    });
    const outside = (event: PointerEvent) => {
      if (!element.contains(event.target as Node)) onClose();
    };
    const away = () => onClose();
    setTimeout(() => window.addEventListener("pointerdown", outside), 0);
    window.addEventListener("blur", away);
    window.addEventListener("resize", away);
    return () => {
      window.removeEventListener("pointerdown", outside);
      window.removeEventListener("blur", away);
      window.removeEventListener("resize", away);
    };
  });

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    }
  }

  function day(ms: number): string {
    return new Date(ms).toISOString().slice(0, 10);
  }

  function fine(ms: number): string {
    return ms > 0 && ms < 10 ? `${ms.toFixed(1)} ms` : millis(ms);
  }
</script>

<div
  class="panel"
  role="dialog"
  aria-label="Timing and connection"
  tabindex="-1"
  bind:this={element}
  style:left="{left}px"
  style:top="{top}px"
  onkeydown={onKeydown}
>
  {#if timings}
    <div class="head">
      <h3 class="bitmap">Timing</h3>
      <span class="total num">{millis(response.time_ms)}</span>
    </div>
    <div class="bar" aria-hidden="true">
      {#each list as phase (phase.key)}
        {#if phase.share > 0}
          <span
            class="seg {phase.key}"
            style:left="{phase.start * 100}%"
            style:width="{phase.share * 100}%"
          ></span>
        {/if}
      {/each}
    </div>
    <dl class="phases">
      {#each list as phase (phase.key)}
        <div>
          <dt><span class="swatch {phase.key}" aria-hidden="true"></span>{phase.label}</dt>
          <dd class="num">
            {phase.key === "connect" && timings.connect_ms === null ? "—" : fine(phase.ms)}
          </dd>
        </div>
      {/each}
    </dl>
    {#if note}<p class="note">{note}</p>{/if}
  {/if}

  {#if route.length}
    <div class="head">
      <h3 class="bitmap">Redirects</h3>
      <span class="num">{route.length - 1}</span>
    </div>
    <ol class="route">
      {#each route as step, i (i)}
        <li>
          <span class="status num" class:final={step.ms === null}>{step.status}</span>
          <span class="where mono">{step.url}</span>
          {#if step.ms !== null}<span class="num ms">{fine(step.ms)}</span>{/if}
        </li>
      {/each}
    </ol>
  {/if}

  <div class="head">
    <h3 class="bitmap">Connection</h3>
  </div>
  <dl class="facts">
    <dt>Protocol</dt>
    <dd>{response.http_version}{tls?.version ? ` over ${tls.version}` : ""}</dd>
    {#if response.remote_addr}
      <dt>Server</dt>
      <dd class="mono">{response.remote_addr}</dd>
    {/if}
    {#if certificate}
      <dt>Issued to</dt>
      <dd title={certificate.subject}>{commonName(certificate.subject)}</dd>
      <dt>Issued by</dt>
      <dd title={certificate.issuer}>{commonName(certificate.issuer)}</dd>
      <dt>Valid until</dt>
      <dd class:bad={certificate.not_after < Date.now()}>
        {day(certificate.not_after)} · {expiry(certificate.not_after)}
      </dd>
      {#if certificate.names?.length}
        <dt>Names</dt>
        <dd class="mono names">{certificate.names.join(", ")}</dd>
      {/if}
    {:else if response.url.startsWith("https:") && !tls}
      <dt>TLS</dt>
      <dd class="muted">Not reported for this protocol.</dd>
    {/if}
  </dl>
</div>

<style>
  .panel {
    position: fixed;
    z-index: 60;
    width: min(360px, calc(100vw - 16px));
    display: grid;
    gap: var(--s2);
    padding: var(--s3);
    border: 1px solid var(--rule);
    background: var(--field);
    box-shadow: 0 6px 18px -6px color-mix(in srgb, var(--t10) 35%, transparent);
  }

  .panel:focus-visible {
    outline: 2px dotted var(--ink);
    outline-offset: 2px;
  }

  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--s2);
  }

  .head:not(:first-child) {
    margin-top: var(--s2);
    padding-top: var(--s3);
    border-top: 1px solid var(--rule-3);
  }

  h3 {
    margin: 0;
    font-size: var(--b1);
    font-weight: 400;
    color: var(--ink-3);
  }

  .num {
    font-variant-numeric: tabular-nums;
  }

  .total {
    font-weight: 650;
  }

  .bar {
    position: relative;
    height: 12px;
    background: var(--sunken);
  }

  .seg {
    position: absolute;
    top: 0;
    bottom: 0;
    min-width: 1px;
  }

  .dns {
    background: var(--t4);
  }

  .connect {
    background: var(--t6);
  }

  .wait {
    background: var(--ink);
  }

  .download {
    background: var(--t8);
  }

  dl {
    margin: 0;
  }

  .phases div {
    display: flex;
    justify-content: space-between;
    gap: var(--s3);
    padding: 3px 0;
  }

  .phases dt {
    display: flex;
    align-items: center;
    gap: var(--s2);
    color: var(--ink-2);
  }

  .phases dd {
    margin: 0;
    font-weight: 650;
  }

  .swatch {
    width: 8px;
    height: 8px;
    flex: none;
  }

  .note {
    margin: 0;
    color: var(--ink-3);
    font-size: var(--fs-meta);
  }

  .route {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 4px;
  }

  .route li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: var(--s2);
    align-items: baseline;
  }

  .route .status {
    font-weight: 650;
    color: var(--ink-2);
  }

  .route .status.final {
    color: var(--ink);
  }

  .route .where {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .route .ms {
    color: var(--ink-3);
  }

  .facts {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 4px var(--s3);
  }

  .facts dt {
    color: var(--ink-3);
  }

  .facts dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .facts dd.bad {
    color: var(--error);
  }

  .names {
    font-size: var(--fs-meta);
  }
</style>
