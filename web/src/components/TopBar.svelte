<script lang="ts">
  import { keyLabel } from "../lib/keys";
  import { nextTheme, themeLabel } from "../lib/theme";
  import {
    openDialog,
    openEnvironments,
    prefs,
    selectEnvironment,
    setTheme,
    signals,
    ws,
  } from "../lib/store.svelte";
  import DesktopActions from "./DesktopActions.svelte";
  import Icon from "./Icon.svelte";
  import Menu, { type MenuItem } from "./Menu.svelte";

  let environmentSelect: HTMLSelectElement;
  let seen = signals.focusEnvironment;

  $effect(() => {
    if (signals.focusEnvironment !== seen) {
      seen = signals.focusEnvironment;
      environmentSelect?.focus();
      environmentSelect?.showPicker?.();
    }
  });

  // On a phone the less-used buttons fold into this menu.
  let more = $state<{ x: number; y: number } | null>(null);
  const moreItems = $derived<MenuItem[]>([
    {
      label: "Import",
      icon: "import",
      action: () => openDialog({ kind: "import", folder: "" }),
    },
    { label: "Cookies", icon: "cookie", action: () => openDialog({ kind: "cookies" }) },
    {
      label: themeLabel(prefs.theme),
      icon: "contrast",
      action: () => setTheme(nextTheme(prefs.theme)),
    },
  ]);

  function openMore(event: MouseEvent): void {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    more = { x: rect.right, y: rect.bottom + 4 };
  }

  function onEnvironment(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value;
    if (value === "__manage") {
      (event.currentTarget as HTMLSelectElement).value = ws.environment;
      openEnvironments(ws.environment);
    } else {
      selectEnvironment(value);
    }
  }
</script>

<header class="top">
  <button
    class="icon-btn"
    aria-label={prefs.sidebarOpen ? "Hide the sidebar" : "Show the sidebar"}
    aria-pressed={prefs.sidebarOpen}
    title="Sidebar ({keyLabel('mod+b').join(' ')})"
    onclick={() => (prefs.sidebarOpen = !prefs.sidebarOpen)}
  >
    <Icon name="sidebar" />
  </button>

  <div class="mark" aria-label="Getman">
    <svg class="logo" viewBox="4 6 40 36" width="22" height="20" aria-hidden="true">
      <path
        d="M32.42 12.33L27.82 16.93A10 10 0 1 0 27.82 31.07L31.64 27.25L19.5 27.25L26 20.75L40.83 20.75L42.95 22.87A1.6 1.6 0 0 1 42.95 25.13L32.42 35.67A16.5 16.5 0 1 1 32.42 12.33Z"
      />
    </svg>
    <span class="word">Getman</span>
  </div>

  <button
    class="find"
    aria-label="Find requests and commands"
    onclick={() => openDialog({ kind: "palette" })}
  >
    <Icon name="search" />
    <span>Find requests and commands</span>
    <span class="keys keyhint">
      {#each keyLabel("mod+k") as key (key)}<kbd>{key}</kbd>{/each}
    </span>
  </button>

  <div class="right">
    <label class="environment" title="Environment ({keyLabel('alt+e').join(' ')})">
      <Icon name="globe" />
      <span class="sr-only">Environment</span>
      <!-- The native select sits invisibly over a label that truncates
           cleanly at any width. -->
      <span class="current select" class:none={ws.environment === ""} aria-hidden="true">
        {ws.environments.find((e) => e.id === ws.environment)?.name ?? "No environment"}
      </span>
      <select
        class="overlay"
        bind:this={environmentSelect}
        value={ws.environment}
        onchange={onEnvironment}
      >
        <option value="">No environment</option>
        {#each ws.environments as environment (environment.id)}
          <option value={environment.id}>{environment.name}</option>
        {/each}
        <option disabled>──────────</option>
        <option value="__manage">Manage environments…</option>
      </select>
    </label>

    <button
      class="icon-btn wide"
      aria-label="Import"
      title="Import a cURL command or Postman export ({keyLabel('alt+i').join(' ')})"
      onclick={() => openDialog({ kind: "import", folder: "" })}
    >
      <Icon name="import" />
    </button>
    <button
      class="icon-btn wide"
      aria-label="Cookies"
      title="Cookies"
      onclick={() => openDialog({ kind: "cookies" })}
    >
      <Icon name="cookie" />
    </button>
    <button
      class="icon-btn wide stacked-hides"
      aria-label={prefs.layout === "columns"
        ? "Stack the response below"
        : "Put the response beside"}
      title={prefs.layout === "columns" ? "Response below" : "Response beside"}
      onclick={() => (prefs.layout = prefs.layout === "columns" ? "rows" : "columns")}
    >
      <Icon name={prefs.layout === "columns" ? "stack" : "split"} />
    </button>
    <button
      class="icon-btn wide"
      aria-label={themeLabel(prefs.theme)}
      title={themeLabel(prefs.theme)}
      onclick={() => setTheme(nextTheme(prefs.theme))}
    >
      <Icon name="contrast" />
    </button>
    <button
      class="icon-btn phone"
      aria-label="More"
      aria-haspopup="menu"
      title="Import, cookies, theme"
      onclick={openMore}
    >
      <Icon name="more" />
    </button>
    <DesktopActions />
  </div>
</header>

{#if more}
  <Menu items={moreItems} x={more.x} y={more.y} label="More" onClose={() => (more = null)} />
{/if}

<style>
  .top {
    grid-area: top;
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: 0 var(--s2);
    border-bottom: 1px solid var(--rule);
    background: var(--paper);
    min-width: 0;
  }

  .mark {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-right: var(--s2);
  }

  .logo {
    flex: none;
    fill: var(--primary);
  }

  .word {
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--on-surface);
  }

  .find {
    display: flex;
    align-items: center;
    gap: var(--s2);
    flex: 0 1 420px;
    min-width: 0;
    height: var(--control);
    margin-left: var(--s4);
    padding: 0 var(--s2);
    border: 1px solid transparent;
    background: var(--fill);
    color: var(--ink-3);
    text-align: left;
    transition: background-color var(--dur-short) var(--ease);
  }

  .find:hover {
    background: var(--fill-hover);
    color: var(--ink-2);
  }

  .find span:not(.keys) {
    flex: 1;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .keys {
    display: flex;
    gap: 2px;
  }

  .right {
    display: flex;
    align-items: center;
    gap: var(--s1);
    margin-left: auto;
  }

  .environment {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    padding-right: var(--s2);
    margin-right: var(--s1);
    border-right: 1px solid var(--rule-3);
    color: var(--ink-2);
  }

  .current {
    display: block;
    line-height: 26px;
    max-width: 200px;
    border-color: transparent;
    border-radius: var(--r-sm);
    background-color: var(--secondary-container);
    font-weight: 500;
    color: var(--on-secondary-container);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* The label is as tall as the touch-sized field, so its text centres in it. */
  @media (pointer: coarse) {
    .current {
      line-height: calc(var(--control) - 2px);
    }
  }

  .current.none {
    border-color: var(--divider);
    background-color: transparent;
    font-weight: 400;
    color: var(--on-surface-variant);
  }

  .overlay {
    position: absolute;
    inset: 0;
    width: 100%;
    opacity: 0;
    cursor: pointer;
  }

  .environment:has(.overlay:focus-visible) .current {
    outline: 2px solid var(--primary);
    outline-offset: 1px;
  }

  .environment:hover .current {
    background-color: color-mix(
      in srgb,
      var(--on-secondary-container) var(--hover),
      var(--secondary-container)
    );
  }

  .phone {
    display: none;
  }

  /* The response always sits below at this width, so there is nothing to switch. */
  @media (max-width: 900px) and (min-height: 501px) {
    .stacked-hides {
      display: none;
    }
  }

  @media (max-width: 720px) {
    .find span:not(.keys),
    .keys,
    .word {
      display: none;
    }

    .find {
      flex: 0 0 auto;
      justify-content: center;
      width: var(--control);
      margin-left: 0;
      padding: 0;
    }

    .wide {
      display: none;
    }

    .phone {
      display: inline-grid;
    }

    /* Let the environment name give way, so the bar never forces the page wider. */
    .right {
      min-width: 0;
      flex: 0 1 auto;
    }

    .environment {
      min-width: 0;
      margin-right: 0;
      padding-right: var(--s1);
      border-right: 0;
    }

    .current {
      max-width: 96px;
    }

    .mark {
      padding-right: 0;
    }
  }
</style>
