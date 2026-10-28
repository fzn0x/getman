<script lang="ts">
  // The top bar's buttons that only the desktop app has, to open another
  // workspace folder, and Settings, which every window has.
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
{/if}
<button
  class="icon-btn settings"
  aria-label={update ? "Settings: a new version is ready" : "Settings"}
  title={update ? "Settings · a new version is ready" : "Settings"}
  onclick={() => openDialog({ kind: "settings" })}
>
  <Icon name="gear" />
  {#if update}<span class="badge" aria-hidden="true"></span>{/if}
</button>

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
