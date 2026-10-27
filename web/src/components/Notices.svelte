<script lang="ts">
  import { dismiss, notices } from "../lib/store.svelte";
  import { keyLabel } from "../lib/keys";
</script>

<div class="notices" role="status" aria-live="polite">
  {#each notices.list as notice (notice.id)}
    <p class="notice {notice.kind}" class:acts={notice.action}>
      <span class="dot" aria-hidden="true"></span>
      <span class="text">{notice.text}</span>
      {#if notice.action}
        {@const action = notice.action}
        <button
          class="act bitmap"
          title="{action.label} ({keyLabel('mod+z').join(' ')})"
          onclick={() => {
            dismiss(notice.id);
            action.run();
          }}>{action.label}</button
        >
      {/if}
    </p>
  {/each}
</div>

<style>
  .notices {
    position: fixed;
    z-index: 80;
    right: var(--s4);
    bottom: calc(24px + var(--s3));
    display: grid;
    justify-items: end;
    gap: var(--s2);
    pointer-events: none;
  }

  .notice {
    display: flex;
    align-items: center;
    gap: var(--s2);
    max-width: min(440px, calc(100vw - 32px));
    margin: 0;
    padding: var(--s2) var(--s3);
    background: var(--ink);
    color: var(--paper);
    animation: arrive 180ms steps(3, end);
  }

  .notice.acts {
    padding-right: var(--s2);
    pointer-events: auto;
  }

  .text {
    overflow-wrap: anywhere;
  }

  .dot {
    flex: none;
    width: 6px;
    height: 6px;
    background: var(--paper);
  }

  .notice.ok .dot {
    background: var(--ok);
  }

  .notice.error {
    background: var(--error);
    color: var(--paper);
  }

  .act {
    flex: none;
    height: 22px;
    margin-left: var(--s2);
    padding: 0 var(--s2);
    border: 1px solid var(--paper);
    background: transparent;
    color: var(--paper);
  }

  .act:hover,
  .act:focus-visible {
    background: var(--paper);
    color: var(--ink);
    outline: none;
  }

  @keyframes arrive {
    from {
      transform: translateY(6px);
      opacity: 0;
    }
  }
</style>
