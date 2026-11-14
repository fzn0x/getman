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
    padding: var(--s2) var(--s4);
    border-radius: var(--r-xs);
    background: var(--inverse-surface);
    color: var(--inverse-on-surface);
    box-shadow: var(--elevation-3);
    animation: arrive var(--dur-medium) var(--ease-out);
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
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--inverse-on-surface);
  }

  .notice.ok .dot {
    background: var(--inverse-primary);
  }

  .notice.error {
    background: var(--error-container);
    color: var(--on-error-container);
  }

  .act {
    flex: none;
    height: 28px;
    margin-left: var(--s2);
    padding: 0 var(--s3);
    border: 0;
    border-radius: var(--r-full);
    background: transparent;
    color: var(--inverse-primary);
    font-weight: 500;
  }

  .notice.error .act {
    color: var(--on-error-container);
  }

  .act:hover,
  .act:focus-visible {
    background: color-mix(in srgb, var(--inverse-primary) var(--hover), transparent);
    outline: none;
  }

  @keyframes arrive {
    from {
      transform: translateY(6px);
      opacity: 0;
    }
  }
</style>
