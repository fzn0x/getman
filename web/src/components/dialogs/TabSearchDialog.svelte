<script lang="ts">
  import { tick } from "svelte";
  import { methodClass, methodShort } from "../../lib/format";
  import { requestMark } from "../../lib/grpc";
  import { kindMark } from "../../lib/realtime";
  import {
    closeDialog,
    closedTabs,
    findNode,
    isDirty,
    reopenClosedTab,
    selectTab,
    tabs,
    tabTitle,
    ws,
  } from "../../lib/store.svelte";
  import { describeSaved, searchTabs, type TabHit } from "../../lib/tabsearch";
  import Icon from "../Icon.svelte";
  import Dialog from "./Dialog.svelte";

  let query = $state("");
  let index = $state(0);
  let list: HTMLUListElement;

  const open = $derived(
    tabs.list.map((tab): TabHit => {
      const request = tab.kind === "request";
      return {
        state: "open",
        id: tab.id,
        title: tabTitle(tab),
        detail:
          tab.kind === "request" || tab.kind === "folder"
            ? (tab.path ?? "Not saved yet")
            : tab.kind === "runner"
              ? tab.target
              : "",
        method: request ? kindMark(tab.draft.kind) || methodShort(requestMark(tab.draft)) : "",
        dirty: isDirty(tab),
      };
    }),
  );

  const closed = $derived(
    closedTabs.list.map((saved, at): TabHit => ({
      state: "closed",
      id: String(at),
      ...describeSaved(saved, ws.name, (path) => findNode(path)?.name ?? path),
      method: (() => {
        if (saved.kind !== "request") return "";
        const method = saved.draft?.method ?? saved.method;
        const kind = saved.draft?.kind;
        return kind && kind !== "http" ? kindMark(kind) : methodShort(method);
      })(),
    })),
  );

  const results = $derived(searchTabs(open, closed, query));

  $effect(() => {
    void results.length;
    index = 0;
  });

  async function move(step: number): Promise<void> {
    if (results.length === 0) return;
    index = (index + step + results.length) % results.length;
    await tick();
    list?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function choose(hit: TabHit | undefined): void {
    if (!hit) return;
    closeDialog();
    if (hit.state === "open") selectTab(hit.id);
    else {
      const saved = closedTabs.list[Number(hit.id)];
      if (saved) void reopenClosedTab(saved);
    }
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      void move(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      void move(-1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      choose(results[index]);
    }
  }
</script>

<Dialog title="Search tabs" width="640px" top>
  <div class="finder">
    <label class="search">
      <Icon name="search" />
      <span class="sr-only">Search tabs</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        autofocus
        bind:value={query}
        onkeydown={onKeydown}
        placeholder="Search open and recently closed tabs"
        role="combobox"
        aria-expanded="true"
        aria-controls="tab-results"
        aria-activedescendant={results[index] ? `tab-hit-${index}` : undefined}
        spellcheck="false"
        autocomplete="off"
      />
    </label>
    <ul id="tab-results" role="listbox" aria-label="Tabs" bind:this={list}>
      {#each results as hit, i (`${hit.state}:${hit.id}`)}
        <li
          id="tab-hit-{i}"
          role="option"
          aria-selected={i === index}
          data-index={i}
          class:on={i === index}
          class:closed={hit.state === "closed"}
          onmousemove={() => (index = i)}
          onclick={() => choose(hit)}
          onkeydown={() => {}}
        >
          <span class="lead">
            {#if hit.method}
              <span class="method {methodClass(hit.method)}">{hit.method}</span>
            {:else}
              <Icon name="file" />
            {/if}
          </span>
          <span class="text">
            <span class="title">{hit.title}</span>
            {#if hit.detail}<span class="detail">{hit.detail}</span>{/if}
          </span>
          <span class="state bitmap">
            {#if hit.dirty}Unsaved{/if}
            {#if hit.state === "closed"}Closed{/if}
          </span>
        </li>
      {:else}
        <li class="none" role="option" aria-selected="false">
          {query.trim() ? "No tab matches." : "No tabs are open."}
        </li>
      {/each}
    </ul>
  </div>
</Dialog>

<style>
  .finder {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    min-height: 0;
    max-height: inherit;
  }

  .search {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: 0 var(--s4);
    border-bottom: 1px solid var(--rule);
  }

  .search:focus-within {
    border-bottom-color: var(--accent);
  }

  .search input {
    outline: none;
    flex: 1;
    height: 48px;
    border: 0;
    background: none;
    font-size: var(--fs-lead);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: var(--s1) 0;
    overflow: auto;
  }

  li {
    display: grid;
    grid-template-columns: 56px minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--s2);
    padding: 7px var(--s4);
    cursor: pointer;
  }

  li.on {
    background: var(--ink);
    color: var(--paper);
  }

  li.on .method,
  li.on .detail,
  li.on .state {
    color: inherit;
  }

  li.closed:not(.on) .title {
    color: var(--ink-2);
  }

  .lead {
    display: flex;
    justify-content: flex-start;
  }

  .text {
    display: grid;
    min-width: 0;
  }

  .title,
  .detail {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .detail {
    font-size: var(--fs-meta);
    color: var(--ink-3);
  }

  .state {
    display: flex;
    gap: var(--s2);
    color: var(--ink-3);
  }

  .none {
    display: block;
    color: var(--ink-3);
    cursor: default;
  }
</style>
