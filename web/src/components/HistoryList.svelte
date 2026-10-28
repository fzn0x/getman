<script lang="ts">
  import { bytes, methodClass, millis, statusClass } from "../lib/format";
  import { groupByDay, matchesHistory } from "../lib/history";
  import { openHistoryEntry, ws } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";

  let query = $state("");
  let list: HTMLDivElement;

  const days = $derived(groupByDay(ws.history.filter((entry) => matchesHistory(entry, query))));

  const clock = (at: number) =>
    new Date(at).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

  /** Arrow keys move between entries; from the search box, down enters the list. */
  function onKeydown(event: KeyboardEvent): void {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const rows = [...list.querySelectorAll<HTMLButtonElement>("button.entry")];
    const at = rows.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === "ArrowDown" ? at + 1 : at - 1;
    if (next < 0 && at >= 0) {
      event.preventDefault();
      list.querySelector<HTMLInputElement>("input")?.focus();
    } else if (rows[next]) {
      event.preventDefault();
      rows[next].focus();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="history" bind:this={list} onkeydown={onKeydown}>
  <div class="search">
    <Icon name="search" />
    <input
      class="input"
      type="search"
      bind:value={query}
      placeholder="Search history"
      aria-label="Search history by name, URL, method or status"
      onkeydown={(event) => {
        if (event.key === "Escape") query = "";
      }}
    />
  </div>
  {#each days as day (day.key)}
    <section aria-label={day.label}>
      <h3 class="bitmap day">{day.label}</h3>
      <ul>
        {#each day.entries as entry (entry.id)}
          <li>
            <button
              class="entry"
              title="{entry.method} {entry.url}"
              onclick={() => void openHistoryEntry(entry)}
            >
              <span class="method {methodClass(entry.method)}">{entry.method}</span>
              <span class="text">
                <span class="title">{entry.name || entry.url}</span>
                <span class="meta">
                  {#if entry.status !== null}
                    <span class="num status-{statusClass(entry.status)}">{entry.status}</span>
                  {:else if entry.error}
                    <span class="failed">failed</span>
                  {/if}
                  {#if entry.time_ms !== null}<span class="num">{millis(entry.time_ms)}</span>{/if}
                  {#if entry.size !== undefined && entry.size !== null}<span class="num"
                      >{bytes(entry.size)}</span
                    >{/if}
                  <span class="num at">{clock(entry.at)}</span>
                </span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {:else}
    <p class="none">
      {ws.history.length === 0
        ? "Requests you send appear here, with the response they got."
        : `Nothing in history matches “${query}”.`}
    </p>
  {/each}
</div>

<style>
  .search {
    position: relative;
    display: flex;
    align-items: center;
    margin: var(--s1) var(--s3) var(--s2);
    color: var(--ink-3);
  }

  .search :global(.icon) {
    position: absolute;
    left: 8px;
  }

  .search .input {
    width: 100%;
    padding-left: 28px;
  }

  .day {
    position: sticky;
    top: 0;
    z-index: 1;
    margin: 0;
    padding: var(--s2) var(--s3) 6px;
    background: var(--paper);
    color: var(--ink-3);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .entry {
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

  .entry:hover,
  .entry:focus-visible {
    background: var(--sunken);
    outline: none;
  }

  .entry .method {
    padding-top: 3px;
  }

  .text {
    display: grid;
    min-width: 0;
  }

  .title {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .meta {
    display: flex;
    gap: var(--s2);
    color: var(--ink-3);
    font-size: var(--fs-meta);
    white-space: nowrap;
  }

  .meta .at {
    margin-left: auto;
  }

  .failed {
    color: var(--error);
  }

  .none {
    margin: 0;
    padding: var(--s3);
    color: var(--ink-3);
  }
</style>
