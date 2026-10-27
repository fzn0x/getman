<script lang="ts">
  import { overflowFade } from "../lib/actions";
  import { methodClass, methodShort } from "../lib/format";
  import { kindMark } from "../lib/realtime";
  import { closeTab, isDirty, newRequest, selectTab, tabs, tabTitle } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";

  // A tab opened or chosen from elsewhere may sit past the edge of a
  // strip that overflows, so bring it into view.
  $effect(() => {
    void tabs.list.length;
    document
      .getElementById(`tab-${tabs.active}`)
      ?.parentElement?.scrollIntoView({ block: "nearest", inline: "nearest" });
  });

  function onKeydown(event: KeyboardEvent, id: string): void {
    const at = tabs.list.findIndex((tab) => tab.id === id);
    if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
      event.preventDefault();
      const next = tabs.list[at + (event.key === "ArrowRight" ? 1 : -1)];
      if (next) {
        selectTab(next.id);
        document.getElementById(`tab-${next.id}`)?.focus();
      }
    } else if (event.key === "Delete") {
      event.preventDefault();
      closeTab(id);
    }
  }
</script>

<div class="strip">
  <div class="list" role="tablist" aria-label="Open tabs" use:overflowFade>
    {#each tabs.list as tab (tab.id)}
      {@const dirty = isDirty(tab)}
      <div class="item" class:active={tab.id === tabs.active}>
        <button
          id="tab-{tab.id}"
          class="open"
          role="tab"
          aria-selected={tab.id === tabs.active}
          tabindex={tab.id === tabs.active ? 0 : -1}
          title={tab.kind === "request" ? (tab.path ?? "Not saved yet") : tabTitle(tab)}
          onclick={() => selectTab(tab.id)}
          onauxclick={(event) => {
            if (event.button === 1) closeTab(tab.id);
          }}
          onkeydown={(event) => onKeydown(event, tab.id)}
        >
          {#if tab.kind === "request"}
            <span class="method {methodClass(kindMark(tab.draft.kind) || tab.draft.method)}"
              >{kindMark(tab.draft.kind) || methodShort(tab.draft.method)}</span
            >
          {:else if tab.kind === "folder"}
            <Icon name="folder" />
          {:else if tab.kind === "environments"}
            <Icon name="globe" />
          {:else}
            <Icon name="play" />
          {/if}
          <span class="title" class:unsaved={tab.kind === "request" && !tab.path}
            >{tabTitle(tab)}</span
          >
        </button>
        <button
          class="close"
          class:dirty
          aria-label={dirty
            ? `Close ${tabTitle(tab)}, which has unsaved changes`
            : `Close ${tabTitle(tab)}`}
          title={dirty ? "Unsaved changes" : "Close (Alt W)"}
          onclick={() => closeTab(tab.id)}
        >
          <span class="mark"><Icon name={dirty ? "dot" : "close"} size={7} /></span>
        </button>
      </div>
    {/each}
  </div>
  <button
    class="icon-btn add"
    aria-label="New request"
    title="New request (Alt N)"
    onclick={() => newRequest()}
  >
    <Icon name="plus" />
  </button>
</div>

<style>
  .strip {
    display: flex;
    align-items: stretch;
    min-width: 0;
    height: 34px;
    border-bottom: 1px solid var(--rule);
    background: var(--sunken);
  }

  .list {
    display: flex;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .list::-webkit-scrollbar {
    display: none;
  }

  .item {
    position: relative;
    display: flex;
    align-items: center;
    flex: none;
    max-width: 240px;
    border-right: 1px solid var(--rule-2);
    color: var(--ink-2);
  }

  .item.active {
    background: var(--paper);
    color: var(--ink);
  }

  .item.active::before {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 3px;
    background: var(--accent);
  }

  .item.active::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    bottom: -1px;
    height: 1px;
    background: var(--paper);
  }

  .open {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    height: 100%;
    padding: 0 4px 0 var(--s3);
    border: 0;
    background: none;
    color: inherit;
  }

  .open:focus-visible {
    outline-offset: -3px;
  }

  .title {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .title.unsaved {
    font-style: italic;
  }

  .close {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    margin-right: 6px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--ink-3);
  }

  .close .mark {
    visibility: hidden;
  }

  .item:hover .close .mark,
  .item.active .close .mark,
  .close:focus-visible .mark,
  .close.dirty .mark {
    visibility: visible;
  }

  .close.dirty {
    color: var(--accent);
  }

  .close:hover {
    background: var(--ink);
    color: var(--paper);
  }

  .add {
    align-self: center;
    margin: 0 var(--s1);
  }
</style>
