<script lang="ts">
  // The top bar's buttons that only the desktop app has: open another
  // workspace folder, and Settings. In a browser tab there are none.
  import { desktop, openWorkspaceFolder } from "../lib/desktop.svelte";
  import { openDialog } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";

  const update = $derived(
    desktop.status?.phase.state === "available" || desktop.status?.phase.state === "installed",
  );
</script>

{#if desktop.available}
  <button
    class="icon-btn"
    aria-label="Open a workspace folder"
    title="Open a workspace folder"
    onclick={openWorkspaceFolder}
  >
    <Icon name="folder" />
  </button>
  <button
    class="icon-btn settings"
    aria-label={update ? "Settings: a new version is ready" : "Settings"}
    title={update ? "Settings · a new version is ready" : "Settings"}
    onclick={() => openDialog({ kind: "settings" })}
  >
    <Icon name="gear" />
    {#if update}<span class="badge" aria-hidden="true"></span>{/if}
  </button>
{/if}

<style>
  .settings {
    position: relative;
  }

  .badge {
    position: absolute;
    top: 5px;
    right: 5px;
    width: 4px;
    height: 4px;
    background: var(--accent);
  }
</style>
