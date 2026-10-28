<script lang="ts">
  import { untrack } from "svelte";
  import {
    allFolders,
    closeDialog,
    findNode,
    moveItems,
    parentOf,
    ws,
  } from "../../lib/store.svelte";
  import { moveTargets } from "../../lib/tree";
  import Dialog from "./Dialog.svelte";

  let { paths }: { paths: string[] } = $props();

  const moving = untrack(() => [...paths]);
  const folders = moveTargets(["", ...allFolders().map((folder) => folder.path)], moving);
  // Start on the folder the items are in, when they share one.
  const shared = moving.every((path) => parentOf(path) === parentOf(moving[0] ?? ""))
    ? parentOf(moving[0] ?? "")
    : "";
  let folder = $state(folders.includes(shared) ? shared : "");

  const title =
    moving.length === 1
      ? `Move ${findNode(moving[0]!)?.name ?? moving[0]}`
      : `Move ${moving.length} items`;

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const to = folder;
    closeDialog();
    await moveItems(moving, to);
  }
</script>

<Dialog {title}>
  <form class="form" id="move-form" onsubmit={submit}>
    <h2 class="bitmap">{title}</h2>
    <label>
      <span class="bitmap">To folder</span>
      <!-- svelte-ignore a11y_autofocus -->
      <select class="select" bind:value={folder} autofocus>
        {#each folders as option (option)}
          <option value={option}>{option === "" ? `${ws.name} (top level)` : option}</option>
        {/each}
      </select>
    </label>
    <p class="muted">The files move on disk; open tabs follow them.</p>
  </form>
  {#snippet footer()}
    <button class="btn" type="button" onclick={() => closeDialog()}>Cancel</button>
    <button class="btn primary" type="submit" form="move-form">Move</button>
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
