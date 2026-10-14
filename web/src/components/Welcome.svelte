<script lang="ts">
  import { ago, methodClass } from "../lib/format";
  import { keyLabel } from "../lib/keys";
  import { newRequest, openDialog, openRequest, ws } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";

  const recent = $derived.by(() => {
    const seen = new Set<string>();
    return ws.history
      .filter((entry) => entry.path && !seen.has(entry.path) && seen.add(entry.path))
      .slice(0, 6);
  });
</script>

<section class="welcome">
  <header class="specimen">
    <h1 class="bitmap g44">{ws.name}</h1>
    <p class="lead">Open a request from the sidebar, or start a new one.</p>
  </header>

  <div class="actions">
    <button class="btn primary" onclick={() => newRequest()}>
      <Icon name="plus" /> New request
      <span class="hint">{keyLabel("alt+n").join(" ")}</span>
    </button>
    <button class="btn" onclick={() => openDialog({ kind: "palette" })}>
      <Icon name="search" /> Find
      <span class="hint">{keyLabel("mod+k").join(" ")}</span>
    </button>
    <button class="btn" onclick={() => openDialog({ kind: "import", folder: "" })}>
      <Icon name="import" /> Import
      <span class="hint">{keyLabel("alt+i").join(" ")}</span>
    </button>
  </div>

  {#if recent.length > 0}
    <div class="recent">
      <h2 class="bitmap">Recently sent</h2>
      <ul>
        {#each recent as entry (entry.id)}
          <li>
            <button onclick={() => entry.path && void openRequest(entry.path)}>
              <span class="method {methodClass(entry.method)}">{entry.method}</span>
              <span class="name">{entry.name || entry.url}</span>
              <span class="when muted">{ago(entry.at)}</span>
            </button>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  <p class="note">
    Requests are YAML files in <code>{ws.root}</code>. Commit them with your code; values that are
    yours alone go in git-ignored <code>*.local.yaml</code> files.
  </p>
</section>

<style>
  .welcome {
    display: grid;
    align-content: center;
    justify-items: start;
    gap: var(--s5);
    height: 100%;
    padding: var(--s6) clamp(var(--s5), 6vw, 96px);
    overflow: auto;
  }

  .specimen {
    display: grid;
    gap: var(--s2);
    color: var(--ink);
  }

  .g44 {
    margin: 0;
    font-size: var(--b4);
    font-weight: 400;
    line-height: 1;
    overflow-wrap: anywhere;
  }

  .lead {
    margin: 0;
    color: var(--ink-2);
    font-size: var(--fs-lead);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s2);
  }

  .hint {
    opacity: 0.55;
  }

  .recent {
    width: min(520px, 100%);
  }

  h2 {
    margin: 0 0 var(--s2);
    color: var(--ink-3);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    border-top: 1px solid var(--rule);
  }

  li button {
    display: grid;
    grid-template-columns: 60px minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--s2);
    width: 100%;
    padding: 7px 0;
    border: 0;
    border-bottom: 1px solid var(--rule-3);
    background: none;
    text-align: left;
    color: var(--ink);
  }

  li button:hover {
    background: var(--sunken);
  }

  .name {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .note {
    margin: 0;
    max-width: 62ch;
    color: var(--ink-2);
  }

  code {
    font-family: var(--font-code);
    font-size: var(--fs-meta);
    word-break: break-all;
  }

  @media (max-width: 600px) {
    .g44 {
      font-size: var(--b3);
    }
  }
</style>
