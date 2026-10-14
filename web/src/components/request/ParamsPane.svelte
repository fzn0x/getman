<script lang="ts">
  import { joinUrl, splitUrl } from "../../lib/url";
  import type { Draft, KeyValue } from "../../lib/types";
  import KeyValueTable from "./KeyValueTable.svelte";

  let { draft }: { draft: Draft } = $props();

  const parts = $derived(splitUrl(draft.url));
  const rows = $derived<KeyValue[]>([...parts.params, ...draft.disabled_params]);
  const pathRows = $derived(
    Object.entries(draft.path_params).map(([name, value]) => ({ name, value, enabled: true })),
  );

  function onParams(next: KeyValue[]): void {
    draft.url = joinUrl(
      parts.base,
      next.filter((row) => row.enabled !== false),
      parts.hash,
    );
    draft.disabled_params = next
      .filter((row) => row.enabled === false)
      .map((row) => ({ ...row, enabled: false }));
  }

  function onPath(next: KeyValue[]): void {
    draft.path_params = Object.fromEntries(next.map((row) => [row.name, row.value]));
  }
</script>

<div class="pane">
  <section>
    <h3 class="bitmap">Query parameters</h3>
    <KeyValueTable {rows} onChange={onParams} label="Query parameters" />
  </section>

  {#if pathRows.length > 0}
    <section>
      <h3 class="bitmap">Path parameters</h3>
      <KeyValueTable
        rows={pathRows}
        onChange={onPath}
        fixedNames
        toggles={false}
        bulk={false}
        label="Path parameters"
      />
    </section>
  {:else}
    <p class="hint">
      Write <code>:name</code> in the path, as in <code>/users/:id</code>, to fill it in here.
    </p>
  {/if}
</div>

<style>
  .pane {
    display: grid;
    gap: var(--s5);
    align-content: start;
  }

  section {
    display: grid;
    gap: var(--s2);
  }

  h3 {
    margin: 0;
    color: var(--ink);
  }

  .hint {
    margin: 0;
    color: var(--ink-3);
  }

  code {
    font-family: var(--font-code);
    color: var(--ink-2);
  }
</style>
