<script lang="ts">
  import { onMount } from "svelte";
  import { copyText } from "../lib/clipboard";
  import { statusClass } from "../lib/format";
  import { mockFor, refreshMocks, startMock, stopMock } from "../lib/mocks.svelte";
  import Icon from "./Icon.svelte";

  let { folder }: { folder: string } = $props();

  const mock = $derived(mockFor(folder));
  const examples = $derived(
    mock ? mock.mocked.reduce((sum, route) => sum + route.examples.length, 0) : 0,
  );
  let busy = $state(false);

  onMount(() => {
    void refreshMocks();
    // Recent requests to a running mock show up as they come in.
    const timer = setInterval(() => {
      if (mockFor(folder)) void refreshMocks();
    }, 2000);
    return () => clearInterval(timer);
  });

  async function toggle(): Promise<void> {
    busy = true;
    if (mock) await stopMock(folder);
    else await startMock(folder);
    busy = false;
  }

  function time(at: number): string {
    return new Date(at).toLocaleTimeString();
  }
</script>

<section class="mock" class:on={mock} aria-label="Mock server">
  <div class="line">
    <span class="bitmap label">Mock</span>
    {#if mock}
      <button
        class="url mono"
        title="Copy the address"
        onclick={() => void copyText(mock.url, "Mock server address copied")}
      >
        {mock.url}
        <Icon name="copy" />
      </button>
      <span class="muted"
        >{examples}
        {examples === 1 ? "example" : "examples"} on {mock.mocked.length}
        {mock.mocked.length === 1 ? "route" : "routes"}</span
      >
    {:else}
      <span class="muted"
        >Serve the saved examples {folder === "" ? "of the workspace" : "in this folder"} on 127.0.0.1,
        for clients to try against.</span
      >
    {/if}
    <button class="btn" onclick={() => void toggle()} disabled={busy}>
      <Icon name={mock ? "stop" : "globe"} />
      {mock ? "Stop mock" : "Start mock"}
    </button>
  </div>
  {#if mock}
    <p class="hint muted">
      Point <code>{"{{baseUrl}}"}</code> at this address in an environment. Pick an example with the
      <code>x-getman-example</code> header; otherwise the first 2xx answers.
    </p>
    {#if mock.hits.length}
      <ol class="hits" aria-label="Recent requests">
        {#each mock.hits.slice(0, 5) as hit, i (i)}
          <li>
            <span class="muted num">{time(hit.at)}</span>
            <span class="mono">{hit.method} {hit.path}</span>
            <span class="bitmap status-{statusClass(hit.status)}">{hit.status}</span>
            <span class="muted">{hit.example ?? "no example"}</span>
          </li>
        {/each}
      </ol>
    {/if}
  {/if}
</section>

<style>
  .mock {
    margin: 0 var(--s4) var(--s3);
    padding: var(--s2) var(--s3);
    border: 1px solid var(--rule-2);
    background: var(--field);
  }

  .mock.on {
    border-color: var(--ink);
  }

  .line {
    display: flex;
    align-items: center;
    gap: var(--s3);
    flex-wrap: wrap;
  }

  .label {
    color: var(--ink-3);
  }

  .line .btn {
    margin-left: auto;
  }

  .url {
    display: inline-flex;
    align-items: center;
    gap: var(--s2);
    padding: 2px 4px;
    border: 0;
    background: none;
    color: var(--ink);
    font-size: var(--fs-code);
  }

  .url:hover {
    background: var(--sunken);
  }

  .hint {
    margin: var(--s2) 0 0;
    font-size: var(--fs-meta);
  }

  code {
    font-family: var(--font-code);
  }

  .hits {
    list-style: none;
    margin: var(--s2) 0 0;
    padding: 0;
  }

  .hits li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) 4ch auto;
    gap: var(--s3);
    align-items: baseline;
    padding: 2px 0;
    border-top: 1px solid var(--rule-3);
  }

  .hits .mono {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
</style>
