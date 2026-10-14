<script lang="ts">
  import { keyLabel } from "../lib/keys";
  import { isDirty, newRequestIn, openRunner, save, ws, type FolderTab } from "../lib/store.svelte";
  import type { Auth, Settings } from "../lib/types";
  import Icon from "./Icon.svelte";
  import AuthPane from "./request/AuthPane.svelte";
  import KeyValueTable from "./request/KeyValueTable.svelte";
  import ScriptsPane from "./request/ScriptsPane.svelte";
  import SettingsPane from "./request/SettingsPane.svelte";
  import VarsTable from "./request/VarsTable.svelte";

  let { tab }: { tab: FolderTab } = $props();

  const root = $derived(tab.path === "");
  const dirty = $derived(isDirty(tab));
  const draft = $derived(tab.draft);

  const panes = [
    { id: "variables", label: "Variables" },
    { id: "headers", label: "Headers" },
    { id: "auth", label: "Auth" },
    { id: "scripts", label: "Scripts" },
    { id: "settings", label: "Settings" },
    { id: "docs", label: "Docs" },
  ];
</script>

<div class="folder">
  <header class="head">
    <div class="titles">
      <p class="bitmap kind">
        {root ? "Workspace" : "Folder"} · {root ? "getman.yaml" : `${tab.path}/_folder.yaml`}
      </p>
      <input
        class="name"
        bind:value={draft.name}
        aria-label="{root ? 'Workspace' : 'Folder'} name"
        spellcheck="false"
      />
    </div>
    <div class="actions">
      {#if dirty}<span class="bitmap changed">Changed</span>{/if}
      <button class="btn" onclick={() => newRequestIn(tab.path)}
        ><Icon name="plus" /> Request here</button
      >
      <button class="btn" onclick={() => openRunner(tab.path)}
        ><Icon name="play" /> Run {root ? "all" : "folder"}</button
      >
      <button
        class="btn primary"
        onclick={() => void save(tab)}
        disabled={!dirty}
        title="Save ({keyLabel('mod+s').join(' ')})"
      >
        <Icon name="save" /> Save
      </button>
    </div>
  </header>

  <p class="lead">
    {root
      ? `Everything here applies to every request in ${ws.name}. Folders and requests can override it.`
      : "Everything here applies to every request in this folder, after the workspace's. Requests can override it."}
  </p>

  <div class="tablist" role="tablist" aria-label="{root ? 'Workspace' : 'Folder'} settings">
    {#each panes as pane (pane.id)}
      <button
        class="tab"
        role="tab"
        aria-selected={tab.pane === pane.id}
        onclick={() => (tab.pane = pane.id)}
      >
        {pane.label}
      </button>
    {/each}
  </div>

  <div class="pane" class:fill={tab.pane === "scripts"}>
    {#if tab.pane === "variables"}
      <div class="stack">
        <p class="muted">
          {root
            ? "Workspace variables, committed with the workspace. Environments override them; put values only you should see in getman.local.yaml."
            : "Folder variables, for requests in this folder. They override the environment."}
        </p>
        <VarsTable
          vars={draft.variables ?? {}}
          onChange={(vars) => (draft.variables = vars)}
          label="Variables"
        />
      </div>
    {:else if tab.pane === "headers"}
      <div class="stack">
        <p class="muted">Added to every request inside, unless the request sets the same header.</p>
        <KeyValueTable
          rows={draft.headers ?? []}
          onChange={(rows) => (draft.headers = rows)}
          label="Headers"
          nameLabel="Header"
        />
      </div>
    {:else if tab.pane === "auth"}
      <AuthPane
        bind:auth={() => draft.auth ?? { type: "inherit" }, (auth: Auth) => (draft.auth = auth)}
        path={root ? null : `${tab.path}/_folder.yaml`}
        {root}
      />
    {:else if tab.pane === "scripts"}
      <ScriptsPane
        bind:scripts={() => draft.scripts ?? {}, (scripts) => (draft.scripts = scripts)}
        scope={root ? "workspace" : "folder"}
      />
    {:else if tab.pane === "settings"}
      <SettingsPane
        bind:settings={
          () => draft.settings ?? {}, (settings: Settings) => (draft.settings = settings)
        }
        scope={root ? "workspace" : "folder"}
      />
    {:else if tab.pane === "docs"}
      <textarea
        class="input docs"
        value={draft.description ?? ""}
        oninput={(event) => (draft.description = event.currentTarget.value)}
        placeholder="What these requests are for, in Markdown."
        aria-label="Description"></textarea>
    {/if}
  </div>
</div>

<style>
  .folder {
    display: grid;
    grid-template-rows: auto auto auto minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .head {
    display: flex;
    align-items: flex-end;
    gap: var(--s3);
    flex-wrap: wrap;
    padding: var(--s4) var(--s4) var(--s2);
  }

  .titles {
    display: grid;
    gap: 6px;
    flex: 1 1 260px;
    min-width: 0;
  }

  .kind {
    margin: 0;
    color: var(--ink-3);
    text-transform: none;
  }

  .name {
    height: 38px;
    padding: 0 4px;
    margin-left: -4px;
    border: 1px solid transparent;
    background: transparent;
    font-family: var(--font-bitmap);
    font-size: var(--b3);
    text-transform: uppercase;
    -webkit-font-smoothing: none;
    color: var(--ink);
    min-width: 0;
  }

  .name:hover {
    border-color: var(--rule-2);
  }

  .name:focus {
    border-color: var(--ink);
    background: var(--field);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--s2);
    flex-wrap: wrap;
  }

  .changed {
    color: var(--accent-text);
  }

  .lead {
    margin: 0;
    padding: 0 var(--s4) var(--s3);
    color: var(--ink-2);
    max-width: 80ch;
  }

  .tablist {
    padding: 0 var(--s3);
  }

  .pane {
    min-height: 0;
    overflow: auto;
    padding: var(--s3) var(--s4) var(--s5);
  }

  .pane.fill {
    overflow: hidden;
  }

  .stack {
    display: grid;
    gap: var(--s3);
    align-content: start;
    max-width: 960px;
  }

  .stack p {
    margin: 0;
  }

  .docs {
    width: 100%;
    height: 100%;
    min-height: 240px;
    resize: none;
  }
</style>
