<script lang="ts">
  import { iconRects, type IconName } from "../lib/icons";

  interface Props {
    name: IconName;
    /** Rendered size in px; keep to multiples of 7 so pixels stay crisp. */
    size?: 7 | 14 | 21;
  }

  let { name, size = 14 }: Props = $props();
  const rects = $derived(iconRects(name));
</script>

<svg
  class="icon"
  viewBox="0 0 7 7"
  width={size}
  height={size}
  shape-rendering="crispEdges"
  fill="currentColor"
  aria-hidden="true"
>
  {#each rects as rect, i (i)}
    <rect x={rect.x} y={rect.y} width={rect.w} height="1" />
  {/each}
</svg>

<style>
  .icon {
    flex: none;
    display: block;
  }
</style>
