<script lang="ts">
  import {
    allFolders,
    closeDialog,
    parentOf,
    saveAs,
    ws,
    type RequestTab,
  } from "../../lib/store.svelte";
  import { untrack } from "svelte";
  import Dialog from "./Dialog.svelte";

  let { tab }: { tab: RequestTab } = $props();

  let name = $state(untrack(() => (tab.draft.name === "Untitled request" ? "" : tab.draft.name)));
  let folder = $state(
    untrack(() =>
      tab.path ? parentOf(tab.path) : tab.inheritPath ? parentOf(tab.inheritPath) : "",
    ),
  );
  const valid = $derived(name.trim() !== "");

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (!valid) return;
    closeDialog();
    await saveAs(tab, folder, name.trim());
  }
</script>

<Dialog title="Save request">
  <form class="form" id="save-as" onsubmit={submit}>
    <h2 class="bitmap">Save request</h2>
    <label>
      <span class="bitmap">Name</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="input" bind:value={name} autofocus placeholder="List users" />
    </label>
    <label>
      <span class="bitmap">Folder</span>
      <select class="select" bind:value={folder}>
        <option value="">{ws.name} (top level)</option>
        {#each allFolders() as option (option.path)}
          <option value={option.path}>{option.path}</option>
        {/each}
      </select>
    </label>
    <p class="muted">Saved as a YAML file you can commit; the file is named after the request.</p>
  </form>
  {#snippet footer()}
    <button class="btn" type="button" onclick={() => closeDialog()}>Cancel</button>
    <button class="btn primary" type="submit" form="save-as" disabled={!valid}>Save</button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: grid;
    gap: var(--s3);
    padding: var(--s4);
  }

  h2 {
    margin: 0;
    font-size: var(--b2);
    font-weight: 400;
  }

  label {
    display: grid;
    gap: 6px;
  }

  label .bitmap {
    color: var(--ink-3);
  }

  p {
    margin: 0;
  }
</style>
