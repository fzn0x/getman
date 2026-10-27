<script lang="ts">
  import { overflowFade } from "../lib/actions";
  import { copyText } from "../lib/clipboard";
  import { methodClass, methodShort } from "../lib/format";
  import { keyLabel } from "../lib/keys";
  import {
    closedTabs,
    closeTab,
    closeTabsFrom,
    duplicateTab,
    isDirty,
    moveTab,
    newRequest,
    reopenClosedTab,
    revealInTree,
    selectTab,
    tabs,
    tabTitle,
    type Tab,
  } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";
  import Menu, { type MenuItem } from "./Menu.svelte";

  const DRAG = "application/x-getman-tab";
  let menu = $state<{ x: number; y: number; items: (MenuItem | "separator")[] } | null>(null);
  let dragging = $state("");
  let drop = $state<{ id: string; side: "before" | "after" } | null>(null);

  // A tab opened or chosen from elsewhere may sit past the edge of a
  // strip that overflows, so bring it into view.
  $effect(() => {
    void tabs.list.length;
    document
      .getElementById(`tab-${tabs.active}`)
      ?.parentElement?.scrollIntoView({ block: "nearest", inline: "nearest" });
  });

  function pathOf(tab: Tab): string | null {
    return tab.kind === "request" || tab.kind === "folder" ? tab.path : null;
  }

  function itemsFor(tab: Tab): (MenuItem | "separator")[] {
    const at = tabs.list.indexOf(tab);
    const path = pathOf(tab);
    return [
      {
        label: "Close",
        icon: "close",
        hint: keyLabel("alt+w").join(" "),
        action: () => closeTab(tab.id),
      },
      {
        label: "Close others",
        disabled: tabs.list.length < 2,
        action: () => closeTabsFrom(tab.id, "others"),
      },
      {
        label: "Close to the right",
        disabled: at === tabs.list.length - 1,
        action: () => closeTabsFrom(tab.id, "right"),
      },
      { label: "Close all", action: () => closeTabsFrom(tab.id, "all") },
      {
        label: "Reopen closed tab",
        hint: keyLabel("alt+shift+t").join(" "),
        disabled: closedTabs.list.length === 0,
        action: () => void reopenClosedTab(),
      },
      "separator",
      {
        label: "Duplicate",
        icon: "copy",
        disabled: tab.kind !== "request",
        action: () => duplicateTab(tab.id),
      },
      {
        label: "Reveal in sidebar",
        icon: "sidebar",
        disabled: !path,
        action: () => path && revealInTree(path),
      },
      {
        label: "Copy path",
        disabled: !path,
        action: () => path && void copyText(path, "Path copied"),
      },
    ];
  }

  function openMenu(tab: Tab, x: number, y: number): void {
    menu = { x, y, items: itemsFor(tab) };
  }

  function onKeydown(event: KeyboardEvent, tab: Tab): void {
    const at = tabs.list.indexOf(tab);
    if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
      event.preventDefault();
      const step = event.key === "ArrowRight" ? 1 : -1;
      if (event.shiftKey) {
        // Shift moves the tab itself.
        moveTab(tab.id, Math.max(0, at + step));
        requestAnimationFrame(() => document.getElementById(`tab-${tab.id}`)?.focus());
        return;
      }
      const next = tabs.list[at + step];
      if (next) {
        selectTab(next.id);
        document.getElementById(`tab-${next.id}`)?.focus();
      }
    } else if (event.key === "Delete") {
      event.preventDefault();
      closeTab(tab.id);
    } else if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
      event.preventDefault();
      const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
      openMenu(tab, rect.left + 8, rect.bottom);
    }
  }

  function onDragStart(event: DragEvent, tab: Tab): void {
    if (!event.dataTransfer) return;
    event.dataTransfer.setData(DRAG, tab.id);
    event.dataTransfer.effectAllowed = "move";
    dragging = tab.id;
  }

  function onDragOver(event: DragEvent, tab: Tab): void {
    if (!event.dataTransfer?.types.includes(DRAG)) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const side = event.clientX < rect.left + rect.width / 2 ? "before" : "after";
    drop = { id: tab.id, side };
  }

  function onDrop(event: DragEvent, tab: Tab): void {
    event.preventDefault();
    const id = event.dataTransfer?.getData(DRAG);
    const side = drop?.side ?? "before";
    drop = null;
    dragging = "";
    if (!id || id === tab.id) return;
    const from = tabs.list.findIndex((t) => t.id === id);
    let to = tabs.list.indexOf(tab) + (side === "after" ? 1 : 0);
    if (from < to) to--;
    moveTab(id, to);
  }
</script>

<div class="strip">
  <div class="list" role="tablist" aria-label="Open tabs" use:overflowFade>
    {#each tabs.list as tab (tab.id)}
      {@const dirty = isDirty(tab)}
      <div
        class="item"
        class:active={tab.id === tabs.active}
        class:lifted={dragging === tab.id}
        class:drop-before={drop?.id === tab.id && drop.side === "before"}
        class:drop-after={drop?.id === tab.id && drop.side === "after"}
        role="presentation"
        draggable="true"
        ondragstart={(event) => onDragStart(event, tab)}
        ondragover={(event) => onDragOver(event, tab)}
        ondragleave={() => {
          if (drop?.id === tab.id) drop = null;
        }}
        ondrop={(event) => onDrop(event, tab)}
        ondragend={() => {
          dragging = "";
          drop = null;
        }}
        oncontextmenu={(event) => {
          event.preventDefault();
          openMenu(tab, event.clientX, event.clientY);
        }}
      >
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
          onkeydown={(event) => onKeydown(event, tab)}
        >
          {#if tab.kind === "request"}
            <span class="method {methodClass(tab.draft.method)}"
              >{methodShort(tab.draft.method)}</span
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

{#if menu}
  <Menu
    items={menu.items}
    x={menu.x}
    y={menu.y}
    label="Tab actions"
    onClose={() => (menu = null)}
  />
{/if}

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

  .item.lifted {
    opacity: 0.45;
  }

  /* Where a dragged tab lands: a 2px red line, as between tree rows. */
  .item.drop-before,
  .item.drop-after {
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .item.drop-after {
    box-shadow: inset -2px 0 0 var(--accent);
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
