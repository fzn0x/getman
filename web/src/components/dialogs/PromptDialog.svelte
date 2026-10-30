<script lang="ts">
  import { closeDialog, type Dialog as DialogState } from "../../lib/store.svelte";
  import { untrack } from "svelte";
  import Dialog from "./Dialog.svelte";

  let { dialog }: { dialog: Extract<DialogState, { kind: "prompt" }> } = $props();
  let value = $state(untrack(() => dialog.value));
  const valid = $derived(value.trim() !== "");

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (!valid) return;
    // Props read through the open dialog, so take them before closing it.
    const { onSubmit } = dialog;
    closeDialog();
    await onSubmit(value.trim());
  }
</script>

<Dialog title={dialog.title} width="440px">
  <form class="body" id="prompt-form" onsubmit={submit}>
    <h2>{dialog.title}</h2>
    <label>
      <span class="bitmap">{dialog.label}</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="input" bind:value autofocus onfocus={(event) => event.currentTarget.select()} />
    </label>
  </form>
  {#snippet footer()}
    <button class="btn" type="button" onclick={() => closeDialog()}>Cancel</button>
    <button class="btn primary" type="submit" form="prompt-form" disabled={!valid}
      >{dialog.confirm}</button
    >
  {/snippet}
</Dialog>

<style>
  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--s3);
    padding: var(--s4);
    overflow-wrap: anywhere;
  }

  h2 {
    margin: 0;
    font-size: var(--fs-title);
  }

  label {
    display: grid;
    gap: 6px;
  }

  label .bitmap {
    color: var(--ink-3);
    text-transform: none;
  }
</style>
