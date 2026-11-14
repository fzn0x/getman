<script lang="ts">
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
</script>

<div class="specimen {kind}" class:sending class:idle={!execution && !sending}>
  {#if sending}
    <div class="progress" role="progressbar" aria-label={busy}></div>
  {/if}

  <div class="setting">
    <div class="line">
      <span class="numeral bitmap" aria-live="polite">
        {#if sending}
          <span class="dots" aria-hidden="true">···</span>
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
  /* The head of every response: the status in its tone on a faint tint of
     it, then the measurements as label and value pairs. */
  .specimen {
    --tone: var(--on-surface-variant);
    position: relative;
    border-bottom: 1px solid var(--outline-variant);
    background: color-mix(in srgb, var(--tone) 6%, var(--surface));
    overflow: hidden;
    transition: background-color var(--dur-medium) var(--ease);
  }

  .specimen.ok {
    --tone: var(--ok);
  }

  .specimen.redirect {
    --tone: var(--info);
  }

  .specimen.client,
  .specimen.server {
    --tone: var(--error);
  }

  .specimen.idle,
  .specimen.sending {
    --tone: var(--on-surface-variant);
    background: var(--surface-container-low);
  }

  /* Material's indeterminate linear progress indicator. */
  .progress {
    position: absolute;
    inset: 0 0 auto;
    height: 3px;
    background: var(--primary-container);
    overflow: hidden;
  }

  .progress::after {
    content: "";
    position: absolute;
    inset: 0 auto 0 0;
    width: 40%;
    border-radius: 3px;
    background: var(--primary);
    animation: travel 1.4s var(--ease) infinite;
  }

  @keyframes travel {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(260%);
    }
  }

  .setting {
    position: relative;
    display: grid;
    gap: var(--s3);
    padding: var(--s4) var(--s4) var(--s3);
  }

  .line {
    display: flex;
    align-items: baseline;
    gap: var(--s3);
    min-width: 0;
  }

  .numeral {
    font-size: var(--fs-headline-large);
    font-weight: 500;
    line-height: 1;
    letter-spacing: 0;
    font-variant-numeric: tabular-nums;
    color: var(--tone);
  }

  .idle .numeral,
  .sending .numeral {
    color: var(--outline);
  }

  .reason {
    font-size: var(--fs-title);
    font-weight: 500;
    line-height: 1.25;
    color: var(--on-surface);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .idle .reason {
    font-weight: 400;
    color: var(--on-surface-variant);
  }

  .dots {
    animation: pulse 1.2s var(--ease) infinite alternate;
  }

  @keyframes pulse {
    from {
      opacity: 0.35;
    }
  }

  .sizes {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s2) var(--s5);
    margin: 0;
  }

  .sizes div {
    display: grid;
    gap: 2px;
  }

  .sizes dt {
    font-size: var(--fs-meta);
    font-weight: 500;
    line-height: 1.3;
    color: var(--on-surface-variant);
  }

  .sizes dd {
    margin: 0;
    font-weight: 600;
    line-height: 1.3;
  }

  .sizes dd.good {
    color: var(--ok);
  }

  .sizes dd.bad {
    color: var(--error);
  }

  .time {
    margin: -2px -4px;
    padding: 2px 4px;
    border: 0;
    border-radius: var(--r-xs);
    background: none;
    font: inherit;
    color: inherit;
    text-decoration: underline dotted;
    text-underline-offset: 3px;
    cursor: pointer;
  }

  .time:hover,
  .time[aria-expanded="true"] {
    background: color-mix(in srgb, var(--on-surface) var(--hover), transparent);
    text-decoration: none;
  }

  .offer {
    margin-left: 6px;
    padding: 2px 8px;
    border-radius: var(--r-sm);
    background: var(--secondary-container);
    color: var(--on-secondary-container);
    font-size: 11px;
    vertical-align: 1px;
  }

  .hint {
    margin: 0;
    color: var(--on-surface-variant);
  }

  @media (max-width: 520px) {
    .numeral {
      font-size: var(--fs-headline);
    }
  }
</style>
