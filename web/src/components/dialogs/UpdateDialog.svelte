<script lang="ts">
  import { percent, plainNotes, releaseDate } from "../../lib/desktop";
  import { desktop, installUpdate, later, restartApp } from "../../lib/desktop.svelte";
  import { closeDialog } from "../../lib/store.svelte";
  import Dialog from "./Dialog.svelte";

  const status = $derived(desktop.status);
  const phase = $derived(status?.phase ?? { state: "idle" as const });
  const done = $derived(phase.state === "downloading" ? percent(phase.done, phase.total) : null);
</script>

<Dialog title="Update Getman" width="560px">
  <div class="body">
    {#if phase.state === "available"}
      <h2 class="bitmap">Getman {phase.version} is out</h2>
      <p class="muted">
        {#if releaseDate(phase.date)}Released {releaseDate(phase.date)}.{/if}
        You have {status?.version}.
      </p>
      {#if plainNotes(phase.notes)}
        <div class="notes" tabindex="-1">{plainNotes(phase.notes)}</div>
      {/if}
    {:else if phase.state === "downloading"}
      <h2 class="bitmap">Getman {phase.version}</h2>
      <div
        class="bar"
        class:unknown={done === null}
        role="progressbar"
        aria-label="Download"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={done ?? undefined}
      >
        <span style:width="{done ?? 0}%"></span>
      </div>
      <p class="muted">
        Downloading{done === null ? "…" : ` ${done}%`}. Getman checks its signature before it
        installs it.
      </p>
    {:else if phase.state === "installed"}
      <h2 class="bitmap">Getman {phase.version} is ready</h2>
      <p>
        {status?.installsOnRestart
          ? "It installs when Getman restarts, or when you next quit it."
          : "It is installed, and runs once Getman restarts."}
      </p>
    {:else if phase.state === "failed"}
      <h2 class="bitmap">The update failed</h2>
      <p class="error" role="alert">{phase.message}</p>
    {:else}
      <h2 class="bitmap">Getman is up to date</h2>
      <p class="muted">You have {status?.version}.</p>
    {/if}
  </div>
  {#snippet footer()}
    {#if phase.state === "available"}
      <!-- svelte-ignore a11y_autofocus -->
      <button class="btn" autofocus onclick={later}>Later</button>
      <button class="btn primary" onclick={() => void installUpdate()}>Install</button>
    {:else if phase.state === "downloading"}
      <button class="btn" onclick={() => closeDialog()}>Hide</button>
    {:else if phase.state === "installed"}
      <button class="btn" onclick={later}>Later</button>
      <button class="btn primary" onclick={restartApp}>Restart now</button>
    {:else if phase.state === "failed"}
      <button class="btn" onclick={() => closeDialog()}>Close</button>
      <button class="btn primary" onclick={() => void installUpdate()}>Try again</button>
    {:else}
      <button class="btn" onclick={() => closeDialog()}>Close</button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--s3);
    padding: var(--s4);
    overflow: auto;
    overflow-wrap: anywhere;
  }

  h2 {
    margin: 0;
    font-size: var(--b2);
    font-weight: 400;
  }

  p {
    margin: 0;
  }

  .notes {
    max-height: 260px;
    overflow: auto;
    padding: var(--s3);
    border: 1px solid var(--rule-2);
    background: var(--field);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .bar {
    height: 12px;
    border: 1px solid var(--ink);
    background: var(--field);
  }

  .bar span {
    display: block;
    height: 100%;
    background: var(--ink);
  }

  .bar.unknown {
    background-image: radial-gradient(circle, var(--ink-3) 0.8px, transparent 1.2px);
    background-size: 4px 4px;
  }

  .error {
    color: var(--error);
    overflow-wrap: anywhere;
  }
</style>
