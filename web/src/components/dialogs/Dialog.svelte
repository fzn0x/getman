<script lang="ts">
  import { onMount, type Snippet } from "svelte";
  import { closeDialog } from "../../lib/store.svelte";

  interface Props {
    title: string;
    width?: string;
    /** Puts the dialog near the top, as a palette. */
    top?: boolean;
    children: Snippet;
    footer?: Snippet;
  }

  let { title, width = "520px", top = false, children, footer }: Props = $props();
  let element: HTMLDialogElement;

  onMount(() => {
    const opener = document.activeElement as HTMLElement | null;
    element.showModal();
    return () => opener?.focus?.();
  });
</script>

<dialog
  bind:this={element}
  class:top
  style:--width={width}
  aria-label={title}
  onclose={() => closeDialog()}
  onclick={(event) => {
    if (event.target === element) element.close();
  }}
>
  <div class="frame">
    {@render children()}
    {#if footer}
      <footer>{@render footer()}</footer>
    {/if}
  </div>
</dialog>

<style>
  dialog {
    width: min(var(--width), calc(100vw - 32px));
    max-height: min(80vh, 760px);
    padding: 0;
    border: 2px solid var(--ink);
    background: var(--paper);
    color: var(--ink);
    box-shadow: 0 24px 60px -24px color-mix(in srgb, var(--t10) 55%, transparent);
  }

  dialog.top {
    margin-top: 12vh;
  }

  dialog::backdrop {
    background: color-mix(in srgb, var(--t0) 40%, transparent);
    background-image: radial-gradient(
      circle,
      color-mix(in srgb, var(--t10) 22%, transparent) 0.8px,
      transparent 1.2px
    );
    background-size: 4px 4px;
  }

  dialog[open] {
    animation: open 160ms steps(3, end);
  }

  @keyframes open {
    from {
      transform: translateY(8px);
      opacity: 0;
    }
  }

  .frame {
    display: grid;
    max-height: inherit;
    grid-template-rows: minmax(0, 1fr) auto;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--s2);
    padding: var(--s3) var(--s4);
    border-top: 1px solid var(--rule-2);
  }
</style>
