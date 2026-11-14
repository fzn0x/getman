<script lang="ts">
  import { iconPath, type IconName } from "../lib/icons";

  interface Props {
    name: IconName;
    /** Rendered size in px. Material Symbols are drawn for 20 and 24; 16
        suits the small marks inside rows and tabs. */
    size?: number;
  }

  let { name, size = 18 }: Props = $props();
</script>

<svg
  class="icon"
  viewBox="0 -960 960 960"
  width={size}
  height={size}
  fill="currentColor"
  aria-hidden="true"
>
  <path d={iconPath(name)} />
</svg>

<style>
  .icon {
    flex: none;
    display: block;
  }
</style>
