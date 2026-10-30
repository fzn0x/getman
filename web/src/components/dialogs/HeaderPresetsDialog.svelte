<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../../lib/api";
  import { closeDialog, message, notify } from "../../lib/store.svelte";
  import type { HeaderPreset } from "../../lib/types";
  import Icon from "../Icon.svelte";
  import Dialog from "./Dialog.svelte";

  let presets = $state<HeaderPreset[]>([]);
  let loading = $state(true);

  onMount(async () => {
    try {
      presets = await api.headerPresets();
    } catch (error) {
      notify(message(error), "error");
    } finally {
      loading = false;
    }
  });

  async function remove(index: number): Promise<void> {
    const next = presets.filter((_, i) => i !== index);
    try {
      await api.writeHeaderPresets(next);
      presets = next;
    } catch (error) {
      notify(message(error), "error");
    }
  }
</script>

<Dialog title="Header presets" width="560px">
  <div class="body">
    <h2 class="bitmap">Header presets</h2>
    <p class="muted">
      Your presets for this workspace, kept on this machine in
      <code>.getman/header-presets.yaml</code>. Save one from the Presets menu of a Headers tab.
    </p>
    {#if !loading}
      <ul>
        {#each presets as preset, i (preset.name)}
          <li>
            <div class="text">
              <span class="name">{preset.name}</span>
              {#each preset.headers as header, j (j)}
                <span class="header mono">{header.name}: {header.value}</span>
              {/each}
            </div>
            <button
              class="icon-btn"
              aria-label="Delete the {preset.name} preset"
              title="Delete"
              onclick={() => void remove(i)}
            >
              <Icon name="trash" />
            </button>
          </li>
        {:else}
          <li class="none">No presets of your own yet.</li>
        {/each}
      </ul>
    {/if}
  </div>
  {#snippet footer()}
    <button class="btn" onclick={() => closeDialog()}>Done</button>
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

  code {
    font-family: var(--font-code);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    border-top: 1px solid var(--rule-2);
  }

  li {
    display: flex;
    align-items: flex-start;
    gap: var(--s2);
    padding: var(--s2) 0;
    border-bottom: 1px solid var(--rule-3);
  }

  .text {
    display: grid;
    flex: 1;
    gap: 2px;
    min-width: 0;
  }

  .name {
    font-weight: 600;
  }

  .header {
    color: var(--ink-2);
    word-break: break-all;
  }

  .none {
    color: var(--ink-3);
  }
</style>
