<script lang="ts">
  import {
    formatCheck,
    OPERATORS,
    parseCheck,
    TARGETS,
    UNARY,
    type CheckRow,
  } from "../../lib/checks";
  import type { Response, TestResult, Vars } from "../../lib/types";
  import Icon from "../Icon.svelte";
  import VarsTable from "./VarsTable.svelte";

  interface Props {
    checks: string[];
    captures: Vars;
    onCaptures: (captures: Vars) => void;
    results: TestResult[];
    response: Response | null;
  }

  let { checks = $bindable(), captures, onCaptures, results, response }: Props = $props();

  const rows = $derived(checks.map(parseCheck));
  const resultFor = (line: string) => results.find((r) => r.name === line.trim());

  function update(index: number, patch: Partial<CheckRow>): void {
    const row = { ...rows[index]!, ...patch };
    checks[index] = formatCheck(row);
  }

  function add(line = "status == 200"): void {
    checks = [...checks, line];
  }

  function remove(index: number): void {
    checks = checks.filter((_, i) => i !== index);
  }

  const suggestions = $derived.by(() => {
    if (!response) return [];
    const list = [`status == ${response.status}`];
    list.push(`time < ${Math.max(100, Math.ceil((response.time_ms * 2) / 100) * 100)}`);
    const type = response.headers.find((h) => h.name.toLowerCase() === "content-type")?.value;
    if (type?.includes("json")) list.push("header.content-type contains json");
    if ("text" in response.body) {
      try {
        const json = JSON.parse(response.body.text) as unknown;
        if (Array.isArray(json)) list.push(`json.length == ${json.length}`);
        else if (json && typeof json === "object") {
          for (const key of Object.keys(json).slice(0, 2)) list.push(`json.${key} exists`);
        }
      } catch {
        // Not JSON; no JSON suggestions.
      }
    }
    return list.filter((line) => !checks.includes(line));
  });
</script>

<div class="checks">
  <section>
    <header>
      <h3 class="bitmap">Checks</h3>
      <p class="muted">
        Assertions without code, run after every response. Failures fail the request in runs and CI.
      </p>
    </header>
    {#if rows.length > 0}
      <div class="grid" role="table" aria-label="Checks">
        {#each rows as row, i (i)}
          {@const result = resultFor(checks[i] ?? "")}
          <div class="row" role="row">
            <span role="cell" class="mark">
              {#if result}
                <span
                  class="result"
                  class:pass={result.passed}
                  class:fail={!result.passed}
                  title={result.message ?? "Passed"}
                >
                  <Icon name={result.passed ? "check" : "close"} size={7} />
                </span>
              {/if}
            </span>
            <span role="cell">
              <input
                class="input mono"
                value={row.target}
                list="check-targets"
                aria-label="What to check"
                spellcheck="false"
                oninput={(event) => update(i, { target: event.currentTarget.value })}
              />
            </span>
            <span role="cell">
              <select
                class="select mono"
                value={row.operator}
                aria-label="Comparison"
                onchange={(event) => update(i, { operator: event.currentTarget.value })}
              >
                {#each OPERATORS as operator (operator)}
                  <option value={operator}>{operator}</option>
                {/each}
              </select>
            </span>
            <span role="cell">
              <input
                class="input mono"
                value={row.value}
                disabled={UNARY.has(row.operator)}
                placeholder={UNARY.has(row.operator) ? "" : "value"}
                aria-label="Expected value"
                spellcheck="false"
                oninput={(event) => update(i, { value: event.currentTarget.value })}
              />
            </span>
            <span role="cell">
              <button class="icon-btn" aria-label="Remove this check" onclick={() => remove(i)}>
                <Icon name="close" size={7} />
              </button>
            </span>
          </div>
          {#if result && !result.passed && result.message}
            <div class="why" role="row"><span role="cell">{result.message}</span></div>
          {/if}
        {/each}
      </div>
    {/if}
    <div class="add">
      <button class="btn" onclick={() => add()}><Icon name="plus" /> Add check</button>
      {#each suggestions as line (line)}
        <button class="suggest mono" title="Add this check" onclick={() => add(line)}
          >+ {line}</button
        >
      {/each}
    </div>
    <datalist id="check-targets">
      {#each TARGETS as target (target.target)}
        <option value={target.target}>{target.hint}</option>
      {/each}
    </datalist>
  </section>

  <section>
    <header>
      <h3 class="bitmap">Captures</h3>
      <p class="muted">
        Copy values out of the response into variables, such as <code>token</code> from
        <code>json.access_token</code>. They land in the selected environment, or the workspace.
      </p>
    </header>
    <VarsTable
      vars={captures}
      onChange={onCaptures}
      label="Captures"
      nameLabel="Variable"
      valueLabel="From"
    />
  </section>
</div>

<style>
  .checks {
    display: grid;
    gap: var(--s5);
    align-content: start;
  }

  section {
    display: grid;
    gap: var(--s3);
  }

  header {
    display: grid;
    gap: 4px;
  }

  h3 {
    margin: 0;
  }

  header p {
    margin: 0;
    max-width: 70ch;
  }

  code {
    font-family: var(--font-code);
    color: var(--ink-2);
  }

  .grid {
    display: grid;
    grid-template-columns: 18px minmax(120px, 2fr) 120px minmax(120px, 2fr) 28px;
    border-top: 1px solid var(--rule);
  }

  .row {
    display: contents;
  }

  .row > span {
    display: flex;
    align-items: center;
    min-width: 0;
    padding: 2px 4px 2px 0;
    border-bottom: 1px solid var(--rule-3);
  }

  .row .input,
  .row .select {
    width: 100%;
  }

  .result {
    display: grid;
    place-items: center;
    width: 14px;
    height: 14px;
    color: var(--paper);
  }

  .result.pass {
    background: var(--ok);
  }

  .result.fail {
    background: var(--error);
  }

  .why {
    grid-column: 2 / -1;
    margin: 0;
    padding: 4px 0 8px;
    color: var(--error);
    border-bottom: 1px solid var(--rule-3);
    font-size: var(--fs-meta);
  }

  .add {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--s2);
  }

  .suggest {
    height: 26px;
    padding: 0 var(--s2);
    border: 1px dashed var(--rule-2);
    background: none;
    color: var(--ink-2);
    font-size: var(--fs-meta);
  }

  .suggest:hover {
    border-style: solid;
    border-color: var(--ink);
    color: var(--ink);
  }
</style>
