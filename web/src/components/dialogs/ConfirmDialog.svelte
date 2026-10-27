<script lang="ts">
  import { closeDialog, type Dialog as DialogState } from "../../lib/store.svelte";
  import Dialog from "./Dialog.svelte";

  let { dialog }: { dialog: Extract<DialogState, { kind: "confirm" }> } = $props();

  function confirm(): void {
    // Props read through the open dialog, so take them before closing it.
    const { onConfirm } = dialog;
    closeDialog();
    onConfirm();
  }
</script>

<Dialog title={dialog.title} width="440px">
  <div class="body">
    <h2>{dialog.title}</h2>
    <p>{dialog.body}</p>
  </div>
  {#snippet footer()}
    <!-- svelte-ignore a11y_autofocus -->
    <button class="btn" autofocus onclick={() => closeDialog()}>Cancel</button>
    <button
      class="btn"
      class:primary={!dialog.danger}
      class:destroy={dialog.danger}
      onclick={confirm}>{dialog.confirm}</button
    >
  {/snippet}
</Dialog>

<style>
  .body {
    display: grid;
    gap: var(--s2);
    padding: var(--s4);
  }

  h2 {
    margin: 0;
    font-size: var(--fs-title);
  }

  p {
    margin: 0;
    color: var(--ink-2);
    overflow-wrap: anywhere;
  }

  .destroy {
    background: var(--error);
    border-color: var(--error);
    color: var(--paper);
  }

  .destroy:hover:not(:disabled) {
    background: var(--ink);
    border-color: var(--ink);
  }
</style>
