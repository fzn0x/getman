<script lang="ts">
  import { tick } from "svelte";
  import type { KeyValue } from "../../lib/types";
  import Icon from "../Icon.svelte";

  interface Props {
    rows: KeyValue[];
    onChange: (rows: KeyValue[]) => void;
    nameLabel?: string;
    valueLabel?: string;
    /** Suggestions for names, such as common header names. */
    names?: string[];
    /** Names cannot be changed or added (path parameters). */
    fixedNames?: boolean;
    toggles?: boolean;
    bulk?: boolean;
    label: string;
  }

  let {
    rows,
    onChange,
    nameLabel = "Name",
    valueLabel = "Value",
    names = [],
    fixedNames = false,
    toggles = true,
    bulk = true,
    label,
  }: Props = $props();

  let bulkMode = $state(false);
  let bulkText = $state("");
  let table: HTMLDivElement;
  const listId = `names-${Math.random().toString(36).slice(2)}`;

  function edit(index: number, field: "name" | "value", value: string): void {
    const next = rows.map((row, i) => (i === index ? { ...row, [field]: value } : row));
    onChange(next);
  }

  function toggle(index: number): void {
    onChange(
      rows.map((row, i) => (i === index ? { ...row, enabled: row.enabled === false } : row)),
    );
  }

  function remove(index: number): void {
    onChange(rows.filter((_, i) => i !== index));
  }

  async function add(field: "name" | "value", value: string): Promise<void> {
    onChange([
      ...rows,
      { name: field === "name" ? value : "", value: field === "value" ? value : "", enabled: true },
    ]);
    await tick();
    const inputs = table.querySelectorAll<HTMLInputElement>(`[data-field="${field}"]`);
    const input = inputs[inputs.length - 2] ?? inputs[inputs.length - 1];
    input?.focus();
    input?.setSelectionRange(value.length, value.length);
  }

  function openBulk(): void {
    bulkText = rows
      .map((row) => `${row.enabled === false ? "# " : ""}${row.name}: ${row.value}`)
      .join("\n");
    bulkMode = true;
  }

  function closeBulk(): void {
    const next: KeyValue[] = [];
    for (const raw of bulkText.split("\n")) {
      if (raw.trim() === "") continue;
      const disabled = raw.trimStart().startsWith("#");
      const line = disabled ? raw.trimStart().slice(1).trimStart() : raw;
      const colon = line.indexOf(":");
      const name = (colon < 0 ? line : line.slice(0, colon)).trim();
      const value = colon < 0 ? "" : line.slice(colon + 1).trim();
      next.push({ name, value, enabled: !disabled });
    }
    onChange(next);
    bulkMode = false;
  }
</script>

<div class="kv" bind:this={table}>
  {#if bulkMode}
    <textarea
      class="input mono bulk"
      bind:value={bulkText}
      rows={Math.max(6, bulkText.split("\n").length + 1)}
      aria-label="{label}, one per line as name: value"
      spellcheck="false"></textarea>
    <p class="help">
      One per line as <code>name: value</code>. Start a line with <code>#</code> to keep it but switch
      it off.
    </p>
  {:else}
    <div class="grid" class:toggles role="table" aria-label={label}>
      <div class="head" role="row">
        {#if toggles}<span role="columnheader"><span class="sr-only">On</span></span>{/if}
        <span role="columnheader" class="bitmap">{nameLabel}</span>
        <span role="columnheader" class="bitmap">{valueLabel}</span>
        <span role="columnheader"><span class="sr-only">Remove</span></span>
      </div>
      {#each rows as row, i (i)}
        <div class="row" class:off={row.enabled === false} role="row">
          {#if toggles}
            <span role="cell" class="cell-check">
              <input
                type="checkbox"
                class="check"
                checked={row.enabled !== false}
                aria-label="Send {row.name || 'this row'}"
                onchange={() => toggle(i)}
              />
            </span>
          {/if}
          <span role="cell">
            <input
              class="input mono"
              value={row.name}
              placeholder={nameLabel}
              aria-label={nameLabel}
              data-field="name"
              list={names.length ? listId : undefined}
              readonly={fixedNames}
              spellcheck="false"
              oninput={(event) => edit(i, "name", event.currentTarget.value)}
            />
          </span>
          <span role="cell">
            <input
              class="input mono"
              value={row.value}
              placeholder={valueLabel}
              aria-label="{valueLabel} of {row.name || 'this row'}"
              data-field="value"
              spellcheck="false"
              oninput={(event) => edit(i, "value", event.currentTarget.value)}
            />
          </span>
          <span role="cell">
            {#if !fixedNames}
              <button
                class="icon-btn"
                aria-label="Remove {row.name || 'this row'}"
                onclick={() => remove(i)}
              >
                <Icon name="close" size={7} />
              </button>
            {/if}
          </span>
        </div>
      {/each}
      {#if !fixedNames}
        <div class="row ghost" role="row">
          {#if toggles}<span role="cell"></span>{/if}
          <span role="cell">
            <input
              class="input mono"
              value=""
              placeholder="Add {nameLabel.toLowerCase()}"
              aria-label="Add {nameLabel.toLowerCase()}"
              data-field="name"
              list={names.length ? listId : undefined}
              spellcheck="false"
              oninput={(event) => {
                const value = event.currentTarget.value;
                event.currentTarget.value = "";
                void add("name", value);
              }}
            />
          </span>
          <span role="cell">
            <input
              class="input mono"
              value=""
              placeholder={valueLabel}
              aria-label="Add {valueLabel.toLowerCase()}"
              data-field="value"
              spellcheck="false"
              oninput={(event) => {
                const value = event.currentTarget.value;
                event.currentTarget.value = "";
                void add("value", value);
              }}
            />
          </span>
          <span role="cell"></span>
        </div>
      {/if}
    </div>
  {/if}
  {#if bulk && !fixedNames}
    <div class="tools">
      <button class="btn quiet" onclick={() => (bulkMode ? closeBulk() : openBulk())}>
        {bulkMode ? "Back to table" : "Bulk edit"}
      </button>
    </div>
  {/if}
  {#if names.length}
    <datalist id={listId}>
      {#each names as name (name)}<option value={name}></option>{/each}
    </datalist>
  {/if}
</div>

<style>
  .kv {
    display: grid;
    gap: var(--s2);
  }

  .grid {
    display: grid;
    grid-template-columns: minmax(120px, 2fr) minmax(140px, 3fr) 28px;
  }

  .grid.toggles {
    grid-template-columns: auto minmax(120px, 2fr) minmax(140px, 3fr) 28px;
  }

  .head,
  .row {
    display: contents;
  }

  .head span {
    padding: 0 0 6px 9px;
    color: var(--ink-3);
    border-bottom: 1px solid var(--rule);
  }

  .head span:first-child {
    padding-left: 0;
  }

  .row > span {
    display: flex;
    align-items: center;
    min-width: 0;
    border-bottom: 1px solid var(--rule-3);
  }

  .cell-check {
    padding: 0 var(--s2) 0 2px;
  }

  .row .input {
    width: 100%;
    height: 30px;
    border-color: transparent;
    background: transparent;
  }

  .row .input:hover {
    border-color: var(--rule-2);
  }

  .row .input:focus {
    border-color: var(--ink);
    background: var(--field);
  }

  .row.off .input {
    color: var(--ink-3);
    text-decoration: line-through;
    text-decoration-color: var(--t4);
  }

  .ghost .input::placeholder {
    color: var(--t5);
  }

  .bulk {
    width: 100%;
    min-height: 140px;
    font-size: var(--fs-code);
    line-height: 1.55;
  }

  .help {
    margin: 0;
    color: var(--ink-3);
    font-size: var(--fs-meta);
  }

  .help code {
    font-family: var(--font-code);
  }

  .tools {
    display: flex;
    justify-content: flex-end;
  }
</style>
