<script lang="ts">
  import { ago, methodClass, methodShort } from "../lib/format";
  import { restoreItems, ws } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";
</script>

<ul class="trash" aria-label="Trash">
  {#each ws.trash as item (item.id)}
    <li>
      <span class="mark">
        {#if item.kind === "folder"}
          <Icon name="folder" />
        {:else}
          <span class="method {methodClass(item.method ?? '')}"
            >{methodShort(item.method ?? "")}</span
          >
        {/if}
      </span>
      <span class="text">
        <span class="title">{item.name}</span>
        <span class="path" title={item.path}>{item.path} · {ago(item.deleted_at)}</span>
      </span>
      <button
        class="btn quiet"
        aria-label="Restore {item.name}"
        title="Put it back at {item.path}"
        onclick={() => void restoreItems([item])}>Restore</button
      >
    </li>
  {:else}
    <li class="none">
      Deleted requests and folders wait here, in <code>.getman/trash/</code>, until you empty the
      trash.
    </li>
  {/each}
</ul>

<style>
  .trash {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  li {
    display: grid;
    grid-template-columns: 44px minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--s2);
    padding: 4px var(--s1) 4px var(--s3);
  }

  li:hover {
    background: var(--sunken);
  }

  .mark {
    display: grid;
    justify-items: end;
    color: var(--ink-2);
  }

  .text {
    display: grid;
    min-width: 0;
  }

  .title,
  .path {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .path {
    color: var(--ink-3);
    font-size: var(--fs-meta);
  }

  .btn {
    padding: 0 var(--s2);
  }

  li.none {
    display: block;
    padding: var(--s3);
    color: var(--ink-3);
  }

  li.none:hover {
    background: none;
  }

  code {
    font-family: var(--font-code);
    font-size: var(--fs-meta);
  }
</style>
