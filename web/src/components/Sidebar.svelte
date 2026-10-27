<script lang="ts">
  import { api } from "../lib/api";
  import { ago, methodClass } from "../lib/format";
  import { kindMark } from "../lib/realtime";
  import { rank } from "../lib/fuzzy";
  import {
    allRequests,
    message,
    newRequest,
    notify,
    openDialog,
    openFolder,
    openRequest,
    openRunner,
    promptDialog,
    refreshTree,
    setExpanded,
    signals,
    activeTab,
    ws,
  } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";
  import Tree from "./Tree.svelte";

  let view = $state<"requests" | "history">("requests");
  let filter = $state("");
  let filterInput: HTMLInputElement;
  let seen = signals.focusFilter;

  $effect(() => {
    if (signals.focusFilter !== seen) {
      seen = signals.focusFilter;
      view = "requests";
      filterInput?.focus();
      filterInput?.select();
    }
  });

  const matches = $derived(
    filter.trim() === ""
      ? []
      : rank(filter, allRequests(), (r) => `${r.name} ${r.path}`).slice(0, 80),
  );

  const current = $derived.by(() => {
    const tab = activeTab();
    return tab?.kind === "request" ? tab.path : tab?.kind === "folder" ? tab.path : null;
  });

  function newFolder(parent = ""): void {
    promptDialog({
      title: "New folder",
      label: "Folder name",
      confirm: "Create folder",
      onSubmit: async (name) => {
        try {
          const { path } = await api.createFolder(parent, name);
          if (parent) setExpanded(parent, true);
          setExpanded(path, true);
          await refreshTree();
        } catch (error) {
          notify(message(error), "error");
        }
      },
    });
  }

  function onFilterKey(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      filter = "";
    } else if (event.key === "Enter" && matches[0]) {
      void openRequest(matches[0].path);
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      (document.querySelector(".results button") as HTMLButtonElement | null)?.focus();
    }
  }

  function openHistory(entry: (typeof ws.history)[number]): void {
    newRequest({ ...entry.request, name: entry.request.name || entry.url }, entry.path);
  }

  async function clearHistory(): Promise<void> {
    await api.clearHistory().catch(() => {});
    ws.history = [];
  }
</script>

<nav class="sidebar" aria-label="Workspace">
  <div class="head">
    <button
      class="workspace"
      title="Workspace settings: variables, headers, auth and scripts for every request"
      onclick={() => void openFolder("")}
    >
      <span class="bitmap name">{ws.name}</span>
      <Icon name="gear" />
    </button>
  </div>

  <div class="filter">
    <Icon name="search" />
    <input
      class="input"
      placeholder="Filter requests"
      aria-label="Filter requests"
      bind:value={filter}
      bind:this={filterInput}
      onkeydown={onFilterKey}
    />
    <kbd aria-hidden="true">/</kbd>
  </div>

  <div class="switch tablist" role="tablist" aria-label="Sidebar">
    <button
      class="tab"
      role="tab"
      aria-selected={view === "requests"}
      onclick={() => (view = "requests")}>Requests</button
    >
    <button
      class="tab"
      role="tab"
      aria-selected={view === "history"}
      onclick={() => (view = "history")}
      >History <span class="count">{ws.history.length}</span></button
    >
  </div>

  <div class="body">
    {#if view === "requests"}
      {#if filter.trim() !== ""}
        <ul class="results">
          {#each matches as request (request.path)}
            <li>
              <button
                class:current={current === request.path}
                onclick={() => void openRequest(request.path)}
              >
                <span class="method {methodClass(kindMark(request.request_kind) || request.method)}"
                  >{kindMark(request.request_kind) || request.method}</span
                >
                <span class="text">
                  <span class="title">{request.name}</span>
                  <span class="path">{request.path}</span>
                </span>
              </button>
            </li>
          {:else}
            <li class="none">Nothing matches “{filter}”.</li>
          {/each}
        </ul>
      {:else if ws.tree.length === 0}
        <div class="starter">
          <p class="bitmap big">No requests yet</p>
          <p>Requests are YAML files in <code>{ws.root}</code>. Start one, or bring yours in.</p>
          <button class="btn" onclick={() => newRequest()}>
            <Icon name="plus" /> New request
          </button>
          <button class="btn" onclick={() => openDialog({ kind: "import", folder: "" })}>
            <Icon name="import" /> Import
          </button>
        </div>
      {:else}
        <Tree nodes={ws.tree} {current} onNewFolder={newFolder} />
      {/if}
    {:else}
      <ul class="results history">
        {#each ws.history as entry (entry.id)}
          <li>
            <button onclick={() => openHistory(entry)} title={entry.url}>
              <span class="method {methodClass(entry.method)}">{entry.method}</span>
              <span class="text">
                <span class="title">{entry.name || entry.url}</span>
                <span class="path">
                  {#if entry.status !== null}
                    <span class="num">{entry.status}</span> ·
                  {:else if entry.error}
                    <span class="failed">failed</span> ·
                  {/if}
                  {ago(entry.at)}
                </span>
              </span>
            </button>
          </li>
        {:else}
          <li class="none">Requests you send appear here.</li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="foot">
    {#if view === "requests"}
      <button class="btn quiet" onclick={() => newRequest()} title="New request (Alt N)">
        <Icon name="plus" /> Request
      </button>
      <button class="btn quiet" onclick={() => newFolder()}>
        <Icon name="folder" /> Folder
      </button>
      <button
        class="btn quiet run"
        onclick={() => openRunner("")}
        disabled={ws.tree.length === 0}
        title="Run every request in the workspace"
      >
        <Icon name="play" /> Run
      </button>
    {:else}
      <button class="btn quiet" onclick={clearHistory} disabled={ws.history.length === 0}>
        <Icon name="trash" /> Clear history
      </button>
    {/if}
  </div>
</nav>

<style>
  .sidebar {
    display: grid;
    grid-template-rows: auto auto auto minmax(0, 1fr) auto;
    height: 100%;
    min-width: 0;
  }

  .head {
    padding: var(--s3) var(--s3) var(--s2);
  }

  .workspace {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s2);
    width: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: var(--ink);
    text-align: left;
  }

  .workspace :global(.icon) {
    color: var(--ink-3);
  }

  .workspace:hover :global(.icon) {
    color: var(--ink);
  }

  .name {
    font-size: var(--b2);
    line-height: 1.1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .filter {
    position: relative;
    display: flex;
    align-items: center;
    margin: 0 var(--s3) var(--s2);
    color: var(--ink-3);
  }

  .filter :global(.icon) {
    position: absolute;
    left: 8px;
  }

  .filter .input {
    width: 100%;
    padding-left: 28px;
    padding-right: 28px;
  }

  .filter kbd {
    position: absolute;
    right: 6px;
  }

  .switch {
    padding: 0 var(--s1);
  }

  .body {
    min-height: 0;
    overflow: auto;
    padding: var(--s1) 0 var(--s3);
  }

  .results {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .results button {
    display: grid;
    grid-template-columns: 44px minmax(0, 1fr);
    align-items: start;
    gap: var(--s2);
    width: 100%;
    padding: 6px var(--s3);
    border: 0;
    background: none;
    text-align: left;
    color: var(--ink);
  }

  .results button:hover,
  .results button:focus-visible {
    background: var(--sunken);
    outline: none;
  }

  .results button.current {
    background: var(--ink);
    color: var(--paper);
  }

  .results .method {
    padding-top: 3px;
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

  .current .path {
    color: inherit;
    opacity: 0.7;
  }

  .failed {
    color: var(--error);
  }

  .none {
    padding: var(--s3);
    color: var(--ink-3);
  }

  .starter {
    display: grid;
    justify-items: start;
    gap: var(--s3);
    padding: var(--s4) var(--s3);
    color: var(--ink-2);
  }

  .starter p {
    margin: 0;
  }

  .starter .big {
    font-size: var(--b2);
    color: var(--ink);
    line-height: 1.1;
  }

  .starter code {
    font-family: var(--font-code);
    font-size: var(--fs-meta);
    word-break: break-all;
  }

  .foot {
    display: flex;
    gap: 2px;
    padding: var(--s1);
    border-top: 1px solid var(--rule-2);
  }

  .foot .btn {
    padding: 0 var(--s2);
  }

  .foot .run {
    margin-left: auto;
  }
</style>
