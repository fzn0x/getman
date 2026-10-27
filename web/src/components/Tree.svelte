<script lang="ts">
  import { copyText } from "../lib/clipboard";
  import type { Node } from "../lib/types";
  import {
    clearSelection,
    deleteItem,
    deleteItems,
    duplicateItem,
    duplicateItems,
    newRequestIn,
    openDialog,
    openFolder,
    openRequest,
    openRunner,
    renameItem,
    selection,
  } from "../lib/store.svelte";
  import { pathRange } from "../lib/tree";
  import Menu, { type MenuItem } from "./Menu.svelte";
  import TreeItem from "./TreeItem.svelte";

  interface Props {
    nodes: Node[];
    current: string | null;
    onNewFolder: (parent: string) => void;
  }

  let { nodes, current, onNewFolder }: Props = $props();
  let menu = $state<{
    x: number;
    y: number;
    items: (MenuItem | "separator")[];
    label: string;
  } | null>(null);
  let tree: HTMLUListElement;

  /** Actions on everything picked, when the row is one of several. */
  function selectionItems(): (MenuItem | "separator")[] {
    const paths = [...selection.paths];
    return [
      {
        label: `Duplicate ${paths.length} items`,
        icon: "copy",
        action: () => void duplicateItems(paths),
      },
      {
        label: `Move ${paths.length} items to…`,
        icon: "folder",
        action: () => openDialog({ kind: "move", paths }),
      },
      { label: "Clear selection", hint: "Esc", action: clearSelection },
      "separator",
      {
        label: `Delete ${paths.length} items`,
        icon: "trash",
        hint: "Del",
        danger: true,
        action: () => {
          clearSelection();
          deleteItems(paths);
        },
      },
    ];
  }

  function itemsFor(node: Node): (MenuItem | "separator")[] {
    if (selection.paths.length > 1 && selection.paths.includes(node.path)) {
      return selectionItems();
    }
    if (node.kind === "request") {
      return [
        { label: "Open", icon: "file", action: () => void openRequest(node.path) },
        { label: "Run", icon: "play", action: () => openRunner(node.path) },
        "separator",
        {
          label: "Rename…",
          icon: "pencil",
          hint: "F2",
          action: () => renameItem(node.path, "request", node.name),
        },
        {
          label: "Duplicate",
          icon: "copy",
          action: () => void duplicateItem(node.path, "request"),
        },
        {
          label: "Move to…",
          icon: "folder",
          action: () => openDialog({ kind: "move", paths: [node.path] }),
        },
        { label: "Copy path", action: () => void copyText(node.path) },
        "separator",
        {
          label: "Delete",
          icon: "trash",
          hint: "Del",
          danger: true,
          action: () => deleteItem(node.path),
        },
      ];
    }
    return [
      { label: "New request here", icon: "plus", action: () => newRequestIn(node.path) },
      { label: "New folder here", icon: "folder", action: () => onNewFolder(node.path) },
      {
        label: "Import here…",
        icon: "import",
        action: () => openDialog({ kind: "import", folder: node.path }),
      },
      "separator",
      { label: "Run folder", icon: "play", action: () => openRunner(node.path) },
      { label: "Folder settings", icon: "gear", action: () => void openFolder(node.path) },
      "separator",
      {
        label: "Rename…",
        icon: "pencil",
        hint: "F2",
        action: () => renameItem(node.path, "folder", node.name),
      },
      { label: "Duplicate", icon: "copy", action: () => void duplicateItem(node.path, "folder") },
      {
        label: "Move to…",
        icon: "folder",
        action: () => openDialog({ kind: "move", paths: [node.path] }),
      },
      { label: "Copy path", action: () => void copyText(node.path) },
      "separator",
      {
        label: "Delete folder",
        icon: "trash",
        hint: "Del",
        danger: true,
        action: () => deleteItem(node.path),
      },
    ];
  }

  function onMenu(x: number, y: number, node: Node): void {
    const many = selection.paths.length > 1 && selection.paths.includes(node.path);
    menu = {
      x,
      y,
      items: itemsFor(node),
      label: many ? `Actions for ${selection.paths.length} items` : `${node.name} actions`,
    };
  }

  /** Paths of the rows on screen, top to bottom. */
  function visible(): string[] {
    return [...tree.querySelectorAll<HTMLElement>("[data-row]")].map((row) => row.dataset.path!);
  }

  /** Picks one more row, or the rows from the first picked one to this. */
  function pick(path: string, mode: "toggle" | "range"): void {
    if (mode === "toggle") {
      selection.paths = selection.paths.includes(path)
        ? selection.paths.filter((p) => p !== path)
        : [...selection.paths, path];
      selection.anchor = path;
    } else {
      selection.paths = pathRange(visible(), selection.anchor || current || path, path);
      if (!selection.anchor) selection.anchor = selection.paths[0] ?? path;
    }
  }

  /** Moves focus between visible rows with the arrow keys; Shift picks. */
  function onKeydown(event: KeyboardEvent): void {
    const rows = [...tree.querySelectorAll<HTMLElement>("[data-row]")];
    const at = rows.indexOf(document.activeElement as HTMLElement);
    if (at < 0) return;
    let to = -1;
    if (event.key === "ArrowDown") to = Math.min(at + 1, rows.length - 1);
    else if (event.key === "ArrowUp") to = Math.max(at - 1, 0);
    else if (event.key === "Home") to = 0;
    else if (event.key === "End") to = rows.length - 1;
    else if (event.key === "Escape" && selection.paths.length > 0) {
      event.preventDefault();
      clearSelection();
      return;
    }
    if (to < 0) return;
    event.preventDefault();
    const row = rows[to]!;
    if (event.shiftKey) {
      if (!selection.anchor || selection.paths.length === 0) {
        selection.anchor = rows[at]!.dataset.path!;
      }
      pick(row.dataset.path!, "range");
    }
    row.focus();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<ul
  class="tree"
  role="tree"
  aria-label="Requests"
  aria-multiselectable="true"
  bind:this={tree}
  onkeydown={onKeydown}
>
  {#each nodes as node (node.path)}
    <TreeItem {node} depth={0} {current} {onMenu} {pick} />
  {/each}
</ul>

{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} label={menu.label} onClose={() => (menu = null)} />
{/if}

<style>
  .tree {
    list-style: none;
    margin: 0;
    padding: 0;
  }
</style>
