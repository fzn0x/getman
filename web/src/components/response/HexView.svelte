<script lang="ts">
  import { hexDump } from "../../lib/bytes";
  import { bytes as size } from "../../lib/format";

  let { data }: { data: Uint8Array } = $props();

  /** How much is shown at first, and added by each Show more. */
  const FIRST = 16 * 1024;
  const MORE = 64 * 1024;

  let shown = $state(FIRST);
  const dump = $derived(hexDump(data, 0, shown));
  const rest = $derived(Math.max(0, data.length - shown));
</script>

<div class="hex" role="region" aria-label="Body in hex">
  <div class="dump">
    <pre class="offsets" aria-label="Offsets">{dump.offsets}</pre>
    <pre class="bytes" aria-label="Bytes in hex">{dump.hex}</pre>
    <pre class="ascii" aria-label="Bytes as ASCII">{dump.ascii}</pre>
  </div>
  {#if rest > 0}
    <div class="more">
      <span class="muted">Showing the first {size(shown)} of {size(data.length)}.</span>
      <button class="btn" onclick={() => (shown += MORE)}>
        Show {size(Math.min(MORE, rest))} more
      </button>
      {#if rest > MORE}
        <button class="btn quiet" onclick={() => (shown = data.length)}>Show all</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .hex {
    height: 100%;
    overflow: auto;
    background: var(--field);
  }

  .dump {
    display: grid;
    grid-template-columns: auto auto auto;
    justify-content: start;
    column-gap: var(--s4);
    padding: 6px var(--s3);
  }

  pre {
    margin: 0;
    font: var(--fs-code) / 1.55 var(--font-code);
    font-variant-numeric: tabular-nums;
  }

  .offsets {
    padding-right: var(--s3);
    border-right: 1px solid var(--rule-3);
    color: var(--ink-3);
    user-select: none;
  }

  .ascii {
    padding-left: var(--s3);
    border-left: 1px solid var(--rule-3);
    color: var(--ink-2);
  }

  .more {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--s2);
    padding: var(--s2) var(--s3) var(--s4);
  }
</style>
