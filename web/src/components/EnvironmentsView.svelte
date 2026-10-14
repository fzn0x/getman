<script lang="ts">
  import { api } from "../lib/api";
  import {
    confirmDialog,
    message,
    notify,
    promptDialog,
    refreshTree,
    refreshVariables,
    selectEnvironment,
    ws,
    type EnvironmentsTab,
  } from "../lib/store.svelte";
  import type { EnvironmentFiles, Vars } from "../lib/types";
  import Icon from "./Icon.svelte";

  let { tab }: { tab: EnvironmentsTab } = $props();

  interface Row {
    name: string;
    shared: string;
    local: string;
    hasLocal: boolean;
    secret: boolean;
  }

  const GLOBALS = "__globals";
  let rows = $state<Row[]>([]);
  let displayName = $state("");
  let saved = $state("");
  let loading = $state(false);
  let reveal = $state(false);

  const selected = $derived(tab.selected || ws.environments[0]?.id || GLOBALS);
  const isGlobals = $derived(selected === GLOBALS);
  const dirty = $derived(JSON.stringify(rows) !== saved);

  async function load(id: string): Promise<void> {
    loading = true;
    try {
      if (id === GLOBALS) {
        const state = await api.globals();
        rows = Object.entries(state.globals ?? {}).map(([name, value]) => ({
          name,
          shared: "",
          local: value,
          hasLocal: true,
          secret: false,
        }));
        displayName = "Globals";
      } else {
        const files = await api.readEnvironment(id);
        rows = toRows(files);
        displayName = files.shared.name || id;
      }
      saved = JSON.stringify(rows);
    } catch (error) {
      notify(message(error), "error");
    } finally {
      loading = false;
    }
  }

  function toRows(files: EnvironmentFiles): Row[] {
    const shared = files.shared.variables ?? {};
    const secrets = new Set(files.shared.secrets ?? []);
    const names = [...new Set([...Object.keys(shared), ...Object.keys(files.local)])];
    return names.map((name) => ({
      name,
      shared: shared[name] ?? "",
      local: files.local[name] ?? "",
      hasLocal: name in files.local,
      secret: secrets.has(name),
    }));
  }

  $effect(() => {
    void load(selected);
  });

  async function saveRows(): Promise<void> {
    const clean = rows.filter((row) => row.name.trim() !== "");
    try {
      if (isGlobals) {
        const state = await api.globals();
        state.globals = Object.fromEntries(clean.map((row) => [row.name, row.local]));
        await api.writeGlobals(state);
      } else {
        const shared: Vars = {};
        const local: Vars = {};
        for (const row of clean) {
          shared[row.name] = row.secret ? "" : row.shared;
          if (row.secret || row.hasLocal || row.local !== "") {
            if (row.local !== "" || row.secret) local[row.name] = row.local;
          }
        }
        await api.writeEnvironment(selected, {
          shared: {
            name: displayName,
            variables: shared,
            secrets: clean.filter((row) => row.secret).map((row) => row.name),
          },
          local,
        });
      }
      await load(selected);
      void refreshVariables();
      notify("Saved", "ok");
    } catch (error) {
      notify(message(error), "error");
    }
  }

  function create(): void {
    promptDialog({
      title: "New environment",
      label: "Name, such as Staging or Local",
      confirm: "Create",
      onSubmit: async (name) => {
        try {
          const { id } = await api.createEnvironment(name);
          await refreshTree();
          tab.selected = id;
        } catch (error) {
          notify(message(error), "error");
        }
      },
    });
  }

  function rename(): void {
    promptDialog({
      title: "Rename environment",
      label: "Name",
      value: displayName,
      confirm: "Rename",
      onSubmit: async (name) => {
        try {
          const { id } = await api.renameEnvironment(selected, name);
          const wasActive = ws.environment === selected;
          await refreshTree();
          tab.selected = id;
          if (wasActive) selectEnvironment(id);
        } catch (error) {
          notify(message(error), "error");
        }
      },
    });
  }

  function remove(): void {
    confirmDialog({
      title: `Delete ${displayName}?`,
      body: `environments/${selected}.yaml and its local values will be deleted from disk.`,
      confirm: "Delete",
      danger: true,
      onConfirm: async () => {
        try {
          await api.deleteEnvironment(selected);
          if (ws.environment === selected) selectEnvironment("");
          tab.selected = "";
          await refreshTree();
        } catch (error) {
          notify(message(error), "error");
        }
      },
    });
  }

  function addRow(): void {
    rows.push({ name: "", shared: "", local: "", hasLocal: false, secret: false });
  }
</script>

<div class="environments">
  <aside class="list" aria-label="Environments">
    <p class="bitmap heading">Environments</p>
    <ul>
      {#each ws.environments as environment (environment.id)}
        <li>
          <button
            class:selected={selected === environment.id}
            onclick={() => (tab.selected = environment.id)}
          >
            <span class="name">{environment.name}</span>
            {#if ws.environment === environment.id}<span class="bitmap active">In use</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
    <button class="btn quiet" onclick={create}><Icon name="plus" /> New environment</button>
    <p class="bitmap heading">This machine</p>
    <ul>
      <li>
        <button class:selected={isGlobals} onclick={() => (tab.selected = GLOBALS)}>
          <span class="name">Globals</span>
        </button>
      </li>
    </ul>
  </aside>

  <section class="editor" aria-busy={loading}>
    <header>
      <div>
        <h2 class="bitmap">{displayName}</h2>
        <p class="file mono">
          {isGlobals ? "getman.local.yaml (git-ignored)" : `environments/${selected}.yaml`}
        </p>
      </div>
      <div class="actions">
        {#if !isGlobals}
          {#if ws.environment === selected}
            <button class="btn" onclick={() => selectEnvironment("")}>Stop using</button>
          {:else}
            <button class="btn" onclick={() => selectEnvironment(selected)}>Use</button>
          {/if}
          <button class="btn quiet" onclick={rename}><Icon name="pencil" /> Rename</button>
          <button class="btn quiet danger" onclick={remove}><Icon name="trash" /> Delete</button>
        {/if}
        {#if isGlobals || ws.environments.length > 0}
          <button class="btn primary" onclick={saveRows} disabled={!dirty}
            ><Icon name="save" /> Save</button
          >
        {/if}
      </div>
    </header>

    <p class="explain">
      {#if isGlobals}
        Globals are the weakest variables: anything else with the same name wins. They stay on this
        machine.
      {:else}
        <strong>Shared</strong> values are committed with the workspace. <strong>Local</strong>
        values live in the git-ignored <code>{selected}.local.yaml</code>, win over shared ones, and
        are where scripts save what they set. Mark a variable <strong>secret</strong> to keep its value
        local only.
      {/if}
    </p>

    {#if ws.environments.length === 0 && !isGlobals}
      <div class="empty">
        <h2>No environments</h2>
        <p>
          Environments switch a set of variables at once, such as <code>baseUrl</code> for local, staging
          and production.
        </p>
        <button class="btn primary" onclick={create}><Icon name="plus" /> New environment</button>
      </div>
    {:else}
      <div class="table" role="table" aria-label="Variables of {displayName}">
        <div class="row head" role="row">
          <span role="columnheader" class="bitmap">Variable</span>
          {#if !isGlobals}<span role="columnheader" class="bitmap">Shared value</span>{/if}
          <span role="columnheader" class="bitmap">{isGlobals ? "Value" : "Local value"}</span>
          {#if !isGlobals}<span role="columnheader" class="bitmap">Secret</span>{/if}
          <span role="columnheader"><span class="sr-only">Remove</span></span>
        </div>
        {#each rows as row, i (i)}
          <div class="row" role="row">
            <span role="cell">
              <input
                class="input mono"
                bind:value={row.name}
                placeholder="name"
                aria-label="Variable name"
                spellcheck="false"
              />
            </span>
            {#if !isGlobals}
              <span role="cell">
                {#if row.secret}
                  <span class="secret-note muted">kept local</span>
                {:else}
                  <input
                    class="input mono"
                    bind:value={row.shared}
                    placeholder="committed value"
                    aria-label="Shared value of {row.name}"
                    spellcheck="false"
                  />
                {/if}
              </span>
            {/if}
            <span role="cell">
              <input
                class="input mono"
                type={row.secret && !reveal ? "password" : "text"}
                bind:value={row.local}
                oninput={() => (row.hasLocal = true)}
                placeholder={isGlobals
                  ? "value"
                  : row.secret
                    ? "secret value"
                    : "overrides the shared value"}
                aria-label="{isGlobals ? 'Value' : 'Local value'} of {row.name}"
                spellcheck="false"
                autocomplete="off"
              />
            </span>
            {#if !isGlobals}
              <span role="cell" class="center">
                <input
                  type="checkbox"
                  class="check"
                  bind:checked={row.secret}
                  aria-label="{row.name} is secret"
                />
              </span>
            {/if}
            <span role="cell">
              <button
                class="icon-btn"
                aria-label="Remove {row.name}"
                onclick={() => rows.splice(i, 1)}
              >
                <Icon name="close" size={7} />
              </button>
            </span>
          </div>
        {/each}
      </div>
      <div class="foot">
        <button class="btn" onclick={addRow}><Icon name="plus" /> Add variable</button>
        {#if rows.some((row) => row.secret)}
          <button class="btn quiet" onclick={() => (reveal = !reveal)}
            >{reveal ? "Hide secrets" : "Show secrets"}</button
          >
        {/if}
      </div>
    {/if}
  </section>
</div>

<style>
  .environments {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .list {
    display: grid;
    align-content: start;
    gap: var(--s2);
    padding: var(--s4) var(--s2);
    border-right: 1px solid var(--rule-2);
    overflow: auto;
  }

  .heading {
    margin: var(--s2) var(--s2) 0;
    color: var(--ink-3);
  }

  .list ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .list li button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s2);
    width: 100%;
    padding: 6px var(--s2);
    border: 0;
    background: none;
    text-align: left;
    color: var(--ink);
  }

  .list li button:hover {
    background: var(--sunken);
  }

  .list li button.selected {
    background: var(--ink);
    color: var(--paper);
  }

  .name {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .active {
    color: var(--ok);
  }

  .selected .active {
    color: var(--paper);
  }

  .editor {
    display: grid;
    align-content: start;
    gap: var(--s3);
    padding: var(--s4);
    overflow: auto;
  }

  header {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--s3);
    flex-wrap: wrap;
  }

  h2 {
    margin: 0;
    font-size: var(--b3);
    font-weight: 400;
    line-height: 1;
  }

  .file {
    margin: 6px 0 0;
    color: var(--ink-3);
    font-size: var(--fs-meta);
  }

  .actions {
    display: flex;
    gap: var(--s2);
    flex-wrap: wrap;
  }

  .explain {
    margin: 0;
    max-width: 82ch;
    color: var(--ink-2);
  }

  code {
    font-family: var(--font-code);
  }

  .table {
    display: grid;
    grid-template-columns: minmax(140px, 1.2fr) minmax(160px, 2fr) minmax(160px, 2fr) 56px 28px;
    max-width: 1100px;
  }

  .table:not(:has(.center)) {
    grid-template-columns: minmax(140px, 1fr) minmax(200px, 2fr) 28px;
  }

  .row {
    display: contents;
  }

  .row > span {
    display: flex;
    align-items: center;
    min-width: 0;
    padding: 2px var(--s2) 2px 0;
    border-bottom: 1px solid var(--rule-3);
  }

  .row.head > span {
    padding-bottom: 6px;
    border-bottom: 1px solid var(--rule);
    color: var(--ink-3);
  }

  .row .input {
    width: 100%;
  }

  .center {
    justify-content: center;
  }

  .secret-note {
    padding-left: var(--s2);
    font-style: italic;
  }

  .foot {
    display: flex;
    gap: var(--s2);
  }

  .empty {
    height: auto;
    place-content: start;
    padding: var(--s4) 0;
  }

  @media (max-width: 760px) {
    .environments {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto minmax(0, 1fr);
    }

    .list {
      border-right: 0;
      border-bottom: 1px solid var(--rule-2);
    }
  }
</style>
