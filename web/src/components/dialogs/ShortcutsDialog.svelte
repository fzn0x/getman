<script lang="ts">
  import { keyLabel, SHORTCUTS } from "../../lib/keys";

  const kept = ["mod+n", "mod+w"].map((combo) => keyLabel(combo).join("+"));
  import Dialog from "./Dialog.svelte";
</script>

<Dialog title="Keyboard shortcuts" width="520px">
  <div class="body">
    <h2 class="bitmap">Shortcuts</h2>
    <dl>
      {#each SHORTCUTS as shortcut (shortcut.combo)}
        <dt>{shortcut.action}</dt>
        <dd>
          {#each keyLabel(shortcut.combo) as key (key)}<kbd>{key}</kbd>{/each}
        </dd>
      {/each}
      <dt>Filter the sidebar</dt>
      <dd><kbd>/</kbd></dd>
      <dt>Find in the body or a script</dt>
      <dd>
        {#each keyLabel("mod+f") as key (key)}<kbd>{key}</kbd>{/each}
      </dd>
      <dt>Rename or delete in the tree</dt>
      <dd><kbd>F2</kbd> <kbd>Del</kbd></dd>
    </dl>
    <p class="muted">
      Tab and window keys use {keyLabel("alt")[0]} because the browser keeps {kept[0]},
      {kept[1]} and Ctrl+Tab.
    </p>
  </div>
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

  dl {
    display: grid;
    grid-template-columns: 1fr auto;
    margin: 0;
  }

  dt,
  dd {
    padding: 6px 0;
    border-bottom: 1px solid var(--rule-3);
  }

  dd {
    display: flex;
    gap: 3px;
    justify-content: flex-end;
    margin: 0;
  }

  p {
    margin: 0;
    font-size: var(--fs-meta);
  }
</style>
