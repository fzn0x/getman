<script lang="ts">
  import { installLabel, lastChecked, percent } from "../../lib/desktop";
  import {
    checkNow,
    desktop,
    installCommand,
    restartApp,
    setUpdatePreferences,
  } from "../../lib/desktop.svelte";
  import { openDialog } from "../../lib/store.svelte";
  import Dialog from "./Dialog.svelte";

  const status = $derived(desktop.status);
  const phase = $derived(status?.phase ?? { state: "idle" as const });
  // Ticks so "Checked 5 minutes ago" stays true while the dialog is open.
  let now = $state(new Date());
  $effect(() => {
    const timer = setInterval(() => (now = new Date()), 30_000);
    return () => clearInterval(timer);
  });
</script>

<Dialog title="Settings" width="560px">
  <div class="body">
    <h2 class="bitmap">Settings</h2>
    <section aria-labelledby="about">
      <h3 class="bitmap" id="about">About</h3>
      {#if status}
        <p class="version">
          <span class="bitmap">Getman {status.version}</span>
          <span class="muted">{installLabel(status.install)}</span>
        </p>

        {#if status.notUpdating}
          <p class="muted">{status.notUpdating}</p>
        {:else}
          <label class="toggle">
            <input
              type="checkbox"
              class="check"
              checked={status.checkForUpdates}
              onchange={(event) =>
                void setUpdatePreferences(event.currentTarget.checked, status.installWithoutAsking)}
            />
            <span>
              Check for updates automatically
              <span class="muted hint"
                >Once a day, Getman reads the newest release on GitHub. It is the only request
                Getman makes on its own.</span
              >
            </span>
          </label>
          <label class="toggle" class:off={!status.checkForUpdates}>
            <input
              type="checkbox"
              class="check"
              checked={status.installWithoutAsking}
              disabled={!status.checkForUpdates}
              onchange={(event) => void setUpdatePreferences(true, event.currentTarget.checked)}
            />
            <span>
              Install updates without asking
              <span class="muted hint"
                >New versions download, are verified and install in the background, and run at the
                next launch.</span
              >
            </span>
          </label>

          <div class="row">
            <button
              class="btn"
              disabled={desktop.checking || phase.state === "downloading"}
              onclick={() => void checkNow()}
            >
              {desktop.checking ? "Checking…" : "Check now"}
            </button>
            <p class="state" role="status">
              {#if phase.state === "upToDate"}
                Getman is up to date. {lastChecked(status.lastCheck, now)}.
              {:else if phase.state === "available"}
                Getman {phase.version} is out.
                <button class="link" onclick={() => openDialog({ kind: "update" })}
                  >See what's new</button
                >
              {:else if phase.state === "downloading"}
                Downloading Getman {phase.version}{percent(phase.done, phase.total) === null
                  ? "…"
                  : ` ${percent(phase.done, phase.total)}%`}
              {:else if phase.state === "installed"}
                Getman {phase.version}
                {status.installsOnRestart ? "installs" : "runs"} when Getman restarts.
                <button class="link" onclick={restartApp}>Restart now</button>
              {:else if phase.state === "failed"}
                <span class="error">{phase.message}</span>
              {:else}
                <span class="muted">{lastChecked(status.lastCheck, now)}.</span>
              {/if}
            </p>
          </div>
        {/if}

        {#if desktop.commandLine.offered}
          <div class="row">
            {#if desktop.commandLine.installed}
              <p class="muted">
                The <code>getman</code> command is installed in
                <code>{desktop.commandLine.installed}</code>.
              </p>
            {:else}
              <button class="btn" onclick={() => void installCommand()}
                >Install the getman command</button
              >
              <p class="muted">Use Getman from a terminal and in scripts.</p>
            {/if}
          </div>
        {/if}
      {/if}
    </section>
  </div>
</Dialog>

<style>
  .body {
    display: grid;
    gap: var(--s4);
    padding: var(--s4);
    overflow: auto;
  }

  h2 {
    margin: 0;
    font-size: var(--b2);
    font-weight: 400;
  }

  section {
    display: grid;
    gap: var(--s3);
  }

  h3 {
    margin: 0;
    padding-bottom: var(--s2);
    border-bottom: 1px solid var(--rule-2);
    color: var(--ink-3);
    font-weight: 400;
  }

  p {
    margin: 0;
  }

  .version {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--s3);
  }

  .version .bitmap {
    font-size: var(--b2);
  }

  .toggle {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--s2);
    align-items: start;
  }

  .toggle .check {
    margin-top: 2px;
  }

  .toggle.off {
    color: var(--ink-3);
  }

  .hint {
    display: block;
    font-size: var(--fs-meta);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--s3);
  }

  .state {
    flex: 1;
    min-width: 0;
  }

  .link {
    border: 0;
    padding: 0;
    background: none;
    color: var(--ink);
    text-decoration: underline;
    cursor: pointer;
  }

  .error {
    color: var(--error);
    overflow-wrap: anywhere;
  }
</style>
