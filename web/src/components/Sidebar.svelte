<script lang="ts">
  import { tick } from "svelte";
  import { api } from "../lib/api";
  import { methodClass } from "../lib/format";
  import { rank } from "../lib/fuzzy";
  import { sortNodes } from "../lib/tree";
  import {
    allRequests,
    collapseAll,
    emptyTrash,
    message,
    newRequest,
    notify,
    openDialog,
    openFolder,
    openRequest,
    openRunner,
    promptDialog,
    refreshTrash,
    refreshTree,
    setExpanded,
    setSortTree,
    signals,
    activeTab,
    ws,
  } from "../lib/store.svelte";
  import HistoryList from "./HistoryList.svelte";
  import Icon from "./Icon.svelte";
  import TrashList from "./TrashList.svelte";
  import Tree from "./Tree.svelte";

  let view = $state<"requests" | "history" | "trash">("requests");
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

  let revealed = signals.reveal;
  $effect(() => {
    if (signals.reveal === revealed) return;
    revealed = signals.reveal;
    view = "requests";
    filter = "";
    const path = signals.revealPath;
    void tick().then(() => {
      const row = [...document.querySelectorAll<HTMLElement>(".sidebar [data-row]")].find(
        (element) => element.dataset.path === path,
      );
      row?.scrollIntoView({ block: "nearest" });
      row?.focus();
    });
  });

  const matches = $derived(
    filter.trim() === ""
      ? []
      : rank(filter, allRequests(), (r) => `${r.name} ${r.path}`).slice(0, 80),
  );

  const nodes = $derived(ws.sortTree ? sortNodes(ws.tree) : ws.tree);
  const anyOpen = $derived(Object.values(ws.expanded).some(Boolean));

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

  <div class="switch-row">
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
      <button
        class="tab"
        role="tab"
        aria-selected={view === "trash"}
        onclick={() => {
          view = "trash";
          void refreshTrash();
        }}
        >Trash {#if ws.trash.length}<span class="count">{ws.trash.length}</span>{/if}</button
      >
    </div>
    {#if view === "requests"}
      <div class="tools">
        <button
          class="icon-btn"
          aria-label="Collapse all folders"
          title="Collapse all folders"
          disabled={!anyOpen}
          onclick={collapseAll}
        >
          <Icon name="collapse" />
        </button>
        <button
          class="icon-btn"
          class:on={ws.sortTree}
          aria-label="Sort A to Z"
          aria-pressed={ws.sortTree}
          title={ws.sortTree
            ? "Sorted A to Z; click for each folder's own order"
            : "Sort A to Z (only here, the files keep their order)"}
          onclick={() => setSortTree(!ws.sortTree)}
        >
          <Icon name="sort" />
        </button>
      </div>
    {/if}
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
                <span class="method {methodClass(request.method)}">{request.method}</span>
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
        <Tree {nodes} {current} onNewFolder={newFolder} />
      {/if}
    {:else if view === "trash"}
      <TrashList />
    {:else}
      <HistoryList />
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
    {:else if view === "trash"}
      <button class="btn quiet danger" onclick={emptyTrash} disabled={ws.trash.length === 0}>
        <Icon name="trash" /> Empty trash
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

  .switch-row {
    display: flex;
    align-items: center;
    border-bottom: 1px solid var(--rule-2);
  }

  .switch {
    flex: 1;
    min-width: 0;
    padding: 0 var(--s1);
    border-bottom: 0;
  }

  .tools {
    display: flex;
    gap: 2px;
    padding-right: var(--s1);
  }

  .tools .on {
    background: var(--ink);
    color: var(--paper);
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
