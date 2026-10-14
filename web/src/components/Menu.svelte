<script lang="ts" module>
  import type { IconName } from "../lib/icons";

  export interface MenuItem {
    label: string;
    icon?: IconName;
    hint?: string;
    danger?: boolean;
    disabled?: boolean;
    action: () => void;
  }
</script>

<script lang="ts">
  import { onMount, tick } from "svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    items: (MenuItem | "separator")[];
    x: number;
    y: number;
    label: string;
    onClose: () => void;
  }

  let { items, x, y, label, onClose }: Props = $props();
  let element: HTMLDivElement;
  let left = $state(0);
  let top = $state(0);

  onMount(() => {
    left = x;
    top = y;
    void tick().then(() => {
      const rect = element.getBoundingClientRect();
      left = Math.max(8, Math.min(x, window.innerWidth - rect.width - 8));
      top = Math.max(8, Math.min(y, window.innerHeight - rect.height - 8));
      element.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
    });
    const outside = (event: PointerEvent) => {
      if (!element.contains(event.target as Node)) onClose();
    };
    const away = () => onClose();
    setTimeout(() => window.addEventListener("pointerdown", outside), 0);
    window.addEventListener("blur", away);
    window.addEventListener("resize", away);
    return () => {
      window.removeEventListener("pointerdown", outside);
      window.removeEventListener("blur", away);
      window.removeEventListener("resize", away);
    };
  });

  function onKeydown(event: KeyboardEvent): void {
    const buttons = [...element.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "ArrowDown") {
      event.preventDefault();
      buttons[(at + 1) % buttons.length]?.focus();
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      buttons[(at - 1 + buttons.length) % buttons.length]?.focus();
    } else if (event.key === "Escape" || event.key === "Tab") {
      event.preventDefault();
      onClose();
    }
  }

  function choose(item: MenuItem): void {
    onClose();
    item.action();
  }
</script>

<div
  class="menu"
  role="menu"
  aria-label={label}
  tabindex="-1"
  bind:this={element}
  style:left="{left}px"
  style:top="{top}px"
  onkeydown={onKeydown}
>
  {#each items as item, i (i)}
    {#if item === "separator"}
      <div class="separator" role="separator"></div>
    {:else}
      <button
        role="menuitem"
        class:danger={item.danger}
        disabled={item.disabled}
        onclick={() => choose(item)}
      >
        <span class="icon">
          {#if item.icon}<Icon name={item.icon} />{/if}
        </span>
        <span class="label">{item.label}</span>
        {#if item.hint}<span class="hint bitmap">{item.hint}</span>{/if}
      </button>
    {/if}
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 60;
    min-width: 200px;
    padding: 4px 0;
    border: 1px solid var(--rule);
    background: var(--field);
    box-shadow: 0 6px 18px -6px color-mix(in srgb, var(--t10) 35%, transparent);
  }

  button {
    display: grid;
    grid-template-columns: 14px 1fr auto;
    align-items: center;
    gap: var(--s2);
    width: 100%;
    height: 28px;
    padding: 0 var(--s3);
    border: 0;
    background: none;
    text-align: left;
    color: var(--ink);
  }

  button:hover:not(:disabled),
  button:focus-visible {
    background: var(--ink);
    color: var(--paper);
    outline: none;
  }

  button.danger {
    color: var(--error);
  }

  button.danger:hover:not(:disabled),
  button.danger:focus-visible {
    background: var(--error);
    color: var(--paper);
  }

  button:disabled {
    color: var(--ink-3);
    cursor: default;
  }

  .hint {
    color: inherit;
    opacity: 0.6;
  }

  .separator {
    height: 1px;
    margin: 4px 0;
    background: var(--rule-3);
  }
</style>
