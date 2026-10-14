<script lang="ts">
  import { keyLabel } from "../lib/keys";
  import {
    openDialog,
    openEnvironments,
    prefs,
    selectEnvironment,
    setTheme,
    signals,
    ws,
  } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";

  let environmentSelect: HTMLSelectElement;
  let seen = signals.focusEnvironment;

  $effect(() => {
    if (signals.focusEnvironment !== seen) {
      seen = signals.focusEnvironment;
      environmentSelect?.focus();
      environmentSelect?.showPicker?.();
    }
  });

  function onEnvironment(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value;
    if (value === "__manage") {
      (event.currentTarget as HTMLSelectElement).value = ws.environment;
      openEnvironments(ws.environment);
    } else {
      selectEnvironment(value);
    }
  }

  const dark = $derived(
    prefs.theme === "dark" ||
      (prefs.theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches),
  );
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
    <span class="block" aria-hidden="true"></span>
    <span class="bitmap word">Getman</span>
  </div>

  <button class="find" onclick={() => openDialog({ kind: "palette" })}>
    <Icon name="search" />
    <span>Find requests and commands</span>
    <span class="keys">
      {#each keyLabel("mod+k") as key (key)}<kbd>{key}</kbd>{/each}
    </span>
  </button>

  <div class="right">
    <label class="environment" title="Environment ({keyLabel('alt+e').join(' ')})">
      <Icon name="globe" />
      <span class="sr-only">Environment</span>
      <select
        class="select"
        bind:this={environmentSelect}
        value={ws.environment}
        onchange={onEnvironment}
        class:none={ws.environment === ""}
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
      class="icon-btn"
      aria-label="Import"
      title="Import a cURL command or Postman export ({keyLabel('alt+i').join(' ')})"
      onclick={() => openDialog({ kind: "import", folder: "" })}
    >
      <Icon name="import" />
    </button>
    <button
      class="icon-btn"
      aria-label="Cookies"
      title="Cookies"
      onclick={() => openDialog({ kind: "cookies" })}
    >
      <Icon name="cookie" />
    </button>
    <button
      class="icon-btn"
      aria-label={prefs.layout === "columns"
        ? "Stack the response below"
        : "Put the response beside"}
      title={prefs.layout === "columns" ? "Response below" : "Response beside"}
      onclick={() => (prefs.layout = prefs.layout === "columns" ? "rows" : "columns")}
    >
      <Icon name={prefs.layout === "columns" ? "stack" : "split"} />
    </button>
    <button
      class="icon-btn"
      aria-label={dark ? "Use the light theme" : "Use the dark theme"}
      title={dark ? "Light theme" : "Dark theme"}
      onclick={() => setTheme(dark ? "light" : "dark")}
    >
      <Icon name={dark ? "sun" : "moon"} />
    </button>
  </div>
</header>

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

  .block {
    width: 11px;
    height: 11px;
    background: var(--accent);
  }

  .word {
    font-size: var(--b2);
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
    border: 1px solid var(--rule-2);
    background: var(--field);
    color: var(--ink-3);
    text-align: left;
  }

  .find:hover {
    border-color: var(--ink);
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
    display: flex;
    align-items: center;
    gap: 6px;
    padding-right: var(--s2);
    margin-right: var(--s1);
    border-right: 1px solid var(--rule-3);
    color: var(--ink-2);
  }

  .environment .select {
    max-width: 200px;
    border-color: var(--ink);
    font-weight: 600;
  }

  .environment .select.none {
    border-color: var(--rule-2);
    font-weight: 400;
    color: var(--ink-2);
  }

  @media (max-width: 720px) {
    .find span:not(.keys),
    .keys,
    .word {
      display: none;
    }

    .find {
      flex: 0 0 auto;
      margin-left: 0;
    }

    .environment .select {
      max-width: 120px;
    }
  }
</style>
