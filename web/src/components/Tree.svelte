<script lang="ts">
  import { copyText } from "../lib/clipboard";
  import { newGrpcRequestIn } from "../lib/grpc.svelte";
  import type { Node } from "../lib/types";
  import {
    deleteItem,
    duplicateItem,
    newRequestIn,
    openDialog,
    openFolder,
    openRequest,
    openRunner,
    renameItem,
  } from "../lib/store.svelte";
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

  function itemsFor(node: Node): (MenuItem | "separator")[] {
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
        { label: "Copy path", action: () => void copyText(node.path) },
        "separator",
        {
          label: "Delete",
          icon: "trash",
          hint: "Del",
          danger: true,
          action: () => deleteItem(node.path, "request", node.name),
        },
      ];
    }
    return [
      { label: "New request here", icon: "plus", action: () => newRequestIn(node.path) },
      {
        label: "New gRPC request here",
        icon: "plus",
        action: () => newGrpcRequestIn(node.path),
      },
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
      { label: "Copy path", action: () => void copyText(node.path) },
      "separator",
      {
        label: "Delete folder",
        icon: "trash",
        hint: "Del",
        danger: true,
        action: () => deleteItem(node.path, "folder", node.name),
      },
    ];
  }

  function onMenu(x: number, y: number, node: Node): void {
    menu = { x, y, items: itemsFor(node), label: `${node.name} actions` };
  }

  /** Moves focus between visible rows with the arrow keys. */
  function onKeydown(event: KeyboardEvent): void {
    const rows = [...tree.querySelectorAll<HTMLElement>("[data-row]")];
    const at = rows.indexOf(document.activeElement as HTMLElement);
    if (at < 0) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      rows[Math.min(at + 1, rows.length - 1)]?.focus();
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      rows[Math.max(at - 1, 0)]?.focus();
    } else if (event.key === "Home") {
      event.preventDefault();
      rows[0]?.focus();
    } else if (event.key === "End") {
      event.preventDefault();
      rows[rows.length - 1]?.focus();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<ul class="tree" role="tree" aria-label="Requests" bind:this={tree} onkeydown={onKeydown}>
  {#each nodes as node (node.path)}
    <TreeItem {node} depth={0} {current} {onMenu} {itemsFor} />
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
