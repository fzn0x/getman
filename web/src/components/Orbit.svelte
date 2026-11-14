<script lang="ts" generics="T">
  // Items that circle the middle of their container, spread evenly round
  // one ring. A Svelte port of Magic UI's Orbiting Circles (MIT, see
  // THIRD_PARTY_NOTICES.md). The container must be position: relative.
  import type { Snippet } from "svelte";

  interface Props {
    items: T[];
    /** Draws one item. */
    item: Snippet<[T]>;
    /** Radius of the ring in px. */
    radius?: number;
    /** Seconds for one turn. */
    duration?: number;
    /** Turn anticlockwise. */
    reverse?: boolean;
    /** Draw the ring itself. */
    path?: boolean;
  }

  let { items, item, radius = 160, duration = 20, reverse = false, path = true }: Props = $props();
</script>

{#if path}
  <svg class="ring" aria-hidden="true">
    <circle cx="50%" cy="50%" r={radius} />
  </svg>
{/if}
{#each items as entry, i (i)}
  <div
    class="orbiter"
    class:reverse
    style:--radius={radius}
    style:--angle={(360 / items.length) * i}
    style:--duration={duration}
  >
    <div class="slot">{@render item(entry)}</div>
  </div>
{/each}

<style>
  .ring {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
    pointer-events: none;
  }

  circle {
    fill: none;
    stroke: var(--outline-variant);
    stroke-width: 1;
  }

  /* A point at the middle of the container; the keyframes move it round
     the ring, and the slot centres the item on it. */
  .orbiter {
    position: absolute;
    top: 50%;
    left: 50%;
    width: 0;
    height: 0;
    animation: orbit calc(var(--duration) * 1s) linear infinite;
  }

  .slot {
    position: absolute;
    left: 0;
    top: 0;
    display: grid;
    place-items: center;
    transform: translate(-50%, -50%);
    white-space: nowrap;
  }

  .orbiter.reverse {
    animation-direction: reverse;
  }

  @keyframes orbit {
    from {
      transform: rotate(calc(var(--angle) * 1deg)) translateY(calc(var(--radius) * 1px))
        rotate(calc(var(--angle) * -1deg));
    }
    to {
      transform: rotate(calc(var(--angle) * 1deg + 360deg)) translateY(calc(var(--radius) * 1px))
        rotate(calc(var(--angle) * -1deg - 360deg));
    }
  }
</style>
