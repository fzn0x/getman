<script lang="ts">
  import { notices } from "../lib/store.svelte";
</script>

<div class="notices" role="status" aria-live="polite">
  {#each notices.list as notice (notice.id)}
    <p class="notice {notice.kind}">
      <span class="dot" aria-hidden="true"></span>
      {notice.text}
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

  @keyframes arrive {
    from {
      transform: translateY(6px);
      opacity: 0;
    }
  }
</style>
