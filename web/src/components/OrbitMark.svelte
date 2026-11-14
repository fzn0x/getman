<script lang="ts">
  // The logo mark with what Getman speaks circling it: tools on the inner
  // ring, HTTP methods on the outer one. Decoration only.
  import { methodClass } from "../lib/format";
  import type { IconName } from "../lib/icons";
  import Icon from "./Icon.svelte";
  import Orbit from "./Orbit.svelte";

  interface Props {
    /** Width and height in px. */
    size?: number;
  }

  let { size = 320 }: Props = $props();
  const scale = $derived(size / 320);

  const tools: IconName[] = ["globe", "terminal", "code", "check", "folder", "play"];
  const methods = ["GET", "POST", "PUT", "PATCH", "DELETE"];
</script>

<div class="orbit-mark" aria-hidden="true" style:--size="{size}px">
  <div class="core">
    <svg viewBox="4 6 40 36" width={52 * scale} height={47 * scale}>
      <path
        d="M32.42 12.33L27.82 16.93A10 10 0 1 0 27.82 31.07L31.64 27.25L19.5 27.25L26 20.75L40.83 20.75L42.95 22.87A1.6 1.6 0 0 1 42.95 25.13L32.42 35.67A16.5 16.5 0 1 1 32.42 12.33Z"
      />
    </svg>
  </div>
  <Orbit items={tools} radius={76 * scale} duration={36}>
    {#snippet item(name)}
      <span class="tool"><Icon {name} size={18} /></span>
    {/snippet}
  </Orbit>
  <Orbit items={methods} radius={124 * scale} duration={54} reverse>
    {#snippet item(method)}
      <span class="method {methodClass(method)}">{method}</span>
    {/snippet}
  </Orbit>
</div>

<style>
  .orbit-mark {
    position: relative;
    flex: none;
    width: var(--size);
    height: var(--size);
  }

  .core {
    position: absolute;
    top: 50%;
    left: 50%;
    display: grid;
    place-items: center;
    width: calc(var(--size) * 0.3);
    height: calc(var(--size) * 0.3);
    margin: calc(var(--size) * -0.15) 0 0 calc(var(--size) * -0.15);
    border-radius: 50%;
    background: var(--primary-container);
    fill: var(--primary);
    box-shadow: var(--elevation-1);
  }

  .tool {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background: var(--surface-container-high);
    color: var(--on-surface-variant);
    box-shadow: var(--elevation-1);
  }

  .method {
    padding: 3px 8px;
    border: 1px solid var(--divider);
    border-radius: var(--r-sm);
    background: var(--surface-container-lowest);
    white-space: nowrap;
  }
</style>
