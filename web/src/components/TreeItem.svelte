<script lang="ts">
  import { methodClass, methodShort } from "../lib/format";
  import type { Node } from "../lib/types";
  import {
    clearSelection,
    deleteItem,
    deleteItems,
    moveItem,
    moveItems,
    openRequest,
    parentOf,
    renameItem,
    reorderBefore,
    selection,
    setExpanded,
    ws,
  } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";
  import Self from "./TreeItem.svelte";

  interface Props {
    node: Node;
    depth: number;
    current: string | null;
    onMenu: (x: number, y: number, node: Node) => void;
    pick: (path: string, mode: "toggle" | "range") => void;
  }

  let { node, depth, current, onMenu, pick }: Props = $props();
  let drop = $state<"" | "into" | "before">("");

  const open = $derived(node.kind === "folder" && ws.expanded[node.path] === true);
  const picked = $derived(selection.paths.includes(node.path));
  const DRAG = "application/x-getman-path";
  const DRAG_MANY = "application/x-getman-paths";

  function activate(): void {
    if (node.kind === "folder") setExpanded(node.path, !open);
    else void openRequest(node.path);
  }

  function onClick(event: MouseEvent): void {
    if (event.ctrlKey || event.metaKey) {
      pick(node.path, "toggle");
    } else if (event.shiftKey) {
      pick(node.path, "range");
    } else {
      clearSelection();
      selection.anchor = node.path;
      activate();
    }
  }

  /** What the row's actions apply to: the picked items if it is one of them. */
  function targets(): string[] {
    return picked && selection.paths.length > 1 ? [...selection.paths] : [node.path];
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === " " && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      pick(node.path, "toggle");
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      activate();
    } else if (event.key === "ArrowRight" && node.kind === "folder" && !open) {
      event.preventDefault();
      setExpanded(node.path, true);
    } else if (event.key === "ArrowLeft") {
      if (node.kind === "folder" && open) {
        event.preventDefault();
        setExpanded(node.path, false);
      } else if (depth > 0) {
        event.preventDefault();
        const row = (event.currentTarget as HTMLElement)
          .closest('[role="group"]')
          ?.closest("li")
          ?.querySelector<HTMLElement>("[data-row]");
        row?.focus();
      }
    } else if (event.key === "F2") {
      event.preventDefault();
      renameItem(node.path, node.kind, node.name);
    } else if (event.key === "Delete") {
      event.preventDefault();
      const paths = targets();
      if (paths.length > 1) {
        clearSelection();
        deleteItems(paths);
      } else {
        deleteItem(node.path);
      }
    } else if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
      event.preventDefault();
      const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
      onMenu(rect.left + 24, rect.bottom, node);
    }
  }

  function onDragStart(event: DragEvent): void {
    if (!event.dataTransfer) return;
    event.dataTransfer.setData(DRAG, node.path);
    const paths = targets();
    if (paths.length > 1) event.dataTransfer.setData(DRAG_MANY, JSON.stringify(paths));
    event.dataTransfer.effectAllowed = "move";
  }

  function onDragOver(event: DragEvent): void {
    if (!event.dataTransfer?.types.includes(DRAG)) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
    // A sorted view has no order of its own to drop into.
    drop = node.kind === "folder" ? "into" : ws.sortTree ? "" : "before";
  }

  async function onDrop(event: DragEvent): Promise<void> {
    event.preventDefault();
    const where = drop;
    drop = "";
    const many = event.dataTransfer?.getData(DRAG_MANY);
    if (many) {
      const paths = (JSON.parse(many) as string[]).filter((p) => p !== node.path);
      clearSelection();
      await moveItems(paths, where === "into" ? node.path : parentOf(node.path));
      return;
    }
    const path = event.dataTransfer?.getData(DRAG);
    if (!path || path === node.path || node.path.startsWith(`${path}/`)) return;
    if (where === "into" && node.kind === "folder") {
      await moveItem(path, node.path);
    } else if (parentOf(path) === parentOf(node.path)) {
      if (!ws.sortTree) await reorderBefore(path, node.path);
    } else {
      await moveItem(path, parentOf(node.path));
    }
  }
</script>

<li role="none">
  <div
    class="row"
    class:current={current === node.path}
    class:picked
    class:into={drop === "into"}
    class:before={drop === "before"}
    class:broken={node.kind === "request" && node.error}
    style:--depth={depth}
    data-row
    data-path={node.path}
    tabindex="0"
    role="treeitem"
    aria-level={depth + 1}
    aria-expanded={node.kind === "folder" ? open : undefined}
    aria-selected={picked || current === node.path}
    draggable="true"
    title={node.kind === "request" ? (node.error ?? node.path) : node.path}
    onclick={onClick}
    onkeydown={onKeydown}
    oncontextmenu={(event) => {
      event.preventDefault();
      onMenu(event.clientX, event.clientY, node);
    }}
    ondragstart={onDragStart}
    ondragover={onDragOver}
    ondragleave={() => (drop = "")}
    ondrop={onDrop}
  >
    {#if node.kind === "folder"}
      <span class="twisty"><Icon name={open ? "chevronDown" : "chevronRight"} size={7} /></span>
      <span class="glyph"><Icon name="folder" /></span>
      <span class="label folder">{node.name}</span>
    {:else}
      <span class="twisty"></span>
      <span class="method {methodClass(node.method)}" title={node.method}>
        {node.error ? "ERR" : methodShort(node.method)}
      </span>
      <span class="label">{node.name}</span>
    {/if}
    <button
      class="more icon-btn"
      tabindex="-1"
      aria-label="Actions for {node.name}"
      onclick={(event) => {
        event.stopPropagation();
        const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
        onMenu(rect.left, rect.bottom, node);
      }}
    >
      <Icon name="more" />
    </button>
  </div>
  {#if node.kind === "folder" && open}
    <ul role="group">
      {#each node.children as child (child.path)}
        <Self node={child} depth={depth + 1} {current} {onMenu} {pick} />
      {:else}
        <li class="vacant" style:--depth={depth + 1} role="none">Empty folder</li>
      {/each}
    </ul>
  {/if}
</li>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .row {
    position: relative;
    display: grid;
    grid-template-columns: 10px 30px minmax(0, 1fr) auto;
    align-items: center;
    gap: 4px;
    height: 28px;
    padding-left: calc(var(--s2) + var(--depth) * 14px);
    padding-right: var(--s1);
    color: var(--ink);
    cursor: pointer;
    user-select: none;
  }

  .row:hover {
    background: var(--sunken);
  }

  .row:focus-visible {
    outline: 2px dotted var(--ink);
    outline-offset: -2px;
  }

  /* Picked for a group action: a faint fill with an ink bar. */
  .row.picked {
    background: var(--t2);
    box-shadow: inset 3px 0 0 var(--ink);
  }

  .row.current {
    background: var(--ink);
    color: var(--paper);
  }

  .row.current.picked {
    box-shadow: inset 3px 0 0 var(--accent);
  }

  .row.current .method {
    color: var(--paper);
  }

  .row.into {
    background: var(--accent-wash);
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .row.before::before {
    content: "";
    position: absolute;
    top: -1px;
    left: calc(var(--s2) + var(--depth) * 14px);
    right: 0;
    height: 2px;
    background: var(--accent);
  }

  .row.broken .label {
    color: var(--error);
  }

  .twisty {
    display: grid;
    place-items: center;
    color: var(--ink-3);
  }

  .current .twisty {
    color: inherit;
  }

  .glyph {
    display: grid;
    justify-items: end;
    padding-right: 4px;
    color: var(--ink-2);
  }

  .current .glyph {
    color: inherit;
  }

  .method {
    text-align: right;
    padding-right: 4px;
  }

  .label {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .label.folder {
    font-weight: 600;
  }

  .more {
    width: 22px;
    height: 22px;
    opacity: 0;
    color: inherit;
  }

  .row:hover .more,
  .row:focus-within .more {
    opacity: 1;
  }

  .row.current .more:hover {
    background: var(--paper);
    color: var(--ink);
  }

  .vacant {
    padding: 4px 0 4px calc(var(--s2) + var(--depth) * 14px + 44px);
    color: var(--ink-3);
    font-size: var(--fs-meta);
  }
</style>
