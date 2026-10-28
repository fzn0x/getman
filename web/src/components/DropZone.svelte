<script lang="ts">
  // Files or a folder dropped anywhere on the window open the Import
  // dialog with them. Drops that something else handled, such as an
  // editor, are left alone.
  import { carriesFiles, readDropped } from "../lib/importing";
  import { dialogs, message, notify, openDialog } from "../lib/store.svelte";

  let depth = $state(0);
  const active = $derived(depth > 0);

  function enter(event: DragEvent): void {
    if (!carriesFiles(event.dataTransfer)) return;
    depth++;
  }

  function over(event: DragEvent): void {
    if (!carriesFiles(event.dataTransfer)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
  }

  function leave(event: DragEvent): void {
    if (!carriesFiles(event.dataTransfer)) return;
    depth = Math.max(0, depth - 1);
  }

  async function drop(event: DragEvent): Promise<void> {
    depth = 0;
    if (event.defaultPrevented || !carriesFiles(event.dataTransfer) || !event.dataTransfer) return;
    event.preventDefault();
    try {
      const files = await readDropped(event.dataTransfer);
      if (!files.length) return;
      const folder = dialogs.current?.kind === "import" ? dialogs.current.folder : "";
      openDialog({ kind: "import", folder, files });
    } catch (error) {
      notify(`The dropped files could not be read: ${message(error)}`, "error");
    }
  }
</script>

<svelte:window
  ondragenter={enter}
  ondragover={over}
  ondragleave={leave}
  ondrop={(event) => void drop(event)}
/>

{#if active}
  <div class="drop" aria-hidden="true">
    <p class="bitmap">Drop to import</p>
    <p class="muted">Collections, environments, API specs, HAR files or a Bruno folder</p>
  </div>
{/if}

<style>
  .drop {
    position: fixed;
    inset: var(--s4);
    z-index: 90;
    display: grid;
    place-content: center;
    gap: var(--s2);
    text-align: center;
    border: 2px dashed var(--accent);
    background: color-mix(in srgb, var(--paper) 88%, transparent);
    pointer-events: none;
  }

  .bitmap {
    margin: 0;
    font-size: var(--b3);
  }

  .muted {
    margin: 0;
  }
</style>
