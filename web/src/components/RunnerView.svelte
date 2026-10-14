<script lang="ts">
  import { millis, methodClass, statusClass } from "../lib/format";
  import {
    findNode,
    openRequest,
    startRun,
    stopRun,
    ws,
    type RunnerTab,
  } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";

  let { tab }: { tab: RunnerTab } = $props();

  const title = $derived(tab.target === "" ? ws.name : (findNode(tab.target)?.name ?? tab.target));
  const total = $derived(
    Math.max(tab.items.length * Math.max(tab.iterationCount, 1), tab.results.length),
  );
  const failed = $derived(tab.results.filter((r) => failedResult(r.execution)).length);
  const passed = $derived(tab.results.length - failed);
  const selectedResult = $derived(tab.selected >= 0 ? tab.results[tab.selected] : undefined);

  function failedResult(execution: (typeof tab.results)[number]["execution"]): boolean {
    return (
      execution.error !== null ||
      execution.script_errors.length > 0 ||
      [...execution.tests, ...execution.checks].some((t) => !t.passed)
    );
  }

  async function loadData(event: Event): Promise<void> {
    const file = (event.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    tab.data = await file.text();
    tab.dataName = file.name;
  }

  const rowsCount = $derived(
    tab.data.trim() === ""
      ? 0
      : tab.data.trim().startsWith("[")
        ? (() => {
            try {
              return (JSON.parse(tab.data) as unknown[]).length;
            } catch {
              return 0;
            }
          })()
        : Math.max(0, tab.data.trim().split(/\r?\n/).length - 1),
  );
</script>

<div class="runner">
  <header>
    <div>
      <p class="bitmap kind">Run · {tab.target === "" ? "whole workspace" : tab.target}</p>
      <h2 class="bitmap">{title}</h2>
    </div>
    <div class="go">
      {#if tab.running}
        <button class="btn" onclick={() => void stopRun(tab)}><Icon name="stop" /> Stop</button>
      {:else}
        <button class="btn primary" onclick={() => void startRun(tab)}
          ><Icon name="play" /> Run</button
        >
      {/if}
    </div>
  </header>

  <form class="config" onsubmit={(event) => event.preventDefault()}>
    <label>
      <span class="bitmap">Environment</span>
      <select class="select" bind:value={tab.environment} disabled={tab.running}>
        <option value="">No environment</option>
        {#each ws.environments as environment (environment.id)}
          <option value={environment.id}>{environment.name}</option>
        {/each}
      </select>
    </label>
    <label>
      <span class="bitmap">Iterations</span>
      <input
        class="input"
        type="number"
        min="1"
        placeholder={rowsCount ? String(rowsCount) : "1"}
        value={tab.iterations ?? ""}
        disabled={tab.running}
        oninput={(event) => {
          const n = Number(event.currentTarget.value);
          tab.iterations =
            event.currentTarget.value === "" || !Number.isFinite(n)
              ? null
              : Math.max(1, Math.round(n));
        }}
      />
    </label>
    <label>
      <span class="bitmap">Delay</span>
      <span class="unit">
        <input class="input" type="number" min="0" bind:value={tab.delay} disabled={tab.running} />
        <span class="muted">ms</span>
      </span>
    </label>
    <label class="data">
      <span class="bitmap">Data file</span>
      <span class="unit">
        <input
          type="file"
          accept=".csv,.json,text/csv,application/json"
          class="file"
          onchange={loadData}
          disabled={tab.running}
        />
        <span class="muted"
          >{tab.dataName
            ? `${tab.dataName}: ${rowsCount} rows`
            : "CSV or JSON, one iteration per row"}</span
        >
        {#if tab.dataName}
          <button class="btn quiet" onclick={() => ((tab.data = ""), (tab.dataName = ""))}
            >Clear</button
          >
        {/if}
      </span>
    </label>
    <label class="toggle">
      <input type="checkbox" class="check" bind:checked={tab.bail} disabled={tab.running} />
      Stop at the first failure
    </label>
    <label class="toggle">
      <input type="checkbox" class="check" bind:checked={tab.persist} disabled={tab.running} />
      Keep variables the run sets
    </label>
  </form>

  {#if tab.error}
    <p class="error" role="alert">{tab.error}</p>
  {/if}

  {#if tab.items.length || tab.results.length}
    <section class="matrix-wrap" aria-label="Results">
      <div class="tally">
        <span
          class="bitmap big"
          class:bad={failed > 0}
          class:good={tab.summary !== null && failed === 0}
        >
          {passed}<span class="slash">/</span>{total}
        </span>
        <span class="words">
          {#if tab.running}
            running…
          {:else if tab.summary}
            {tab.summary.stopped
              ? `stopped: ${tab.summary.stopped}`
              : failed
                ? `${failed} failed`
                : "all passed"}
            · {millis(tab.summary.duration_ms)} total · {millis(tab.summary.average_response_ms)} average
            · {tab.summary.tests_passed + tab.summary.checks_passed} of
            {tab.summary.tests_passed +
              tab.summary.tests_failed +
              tab.summary.checks_passed +
              tab.summary.checks_failed} assertions passed
          {/if}
        </span>
      </div>
      <div class="matrix" role="grid" aria-label="One cell per request and iteration">
        {#each Array.from({ length: Math.max(tab.iterationCount, 1) }, (_, i) => i) as iteration (iteration)}
          <div class="iteration" role="row">
            {#if tab.iterationCount > 1}<span class="bitmap label" role="rowheader"
                >{iteration + 1}</span
              >{/if}
            {#each tab.items as item, index (index)}
              {@const at = tab.results.findIndex(
                (r) => r.iteration === iteration && r.index === index,
              )}
              {@const result = at >= 0 ? tab.results[at] : undefined}
              <button
                role="gridcell"
                class="cell"
                class:pass={result && !failedResult(result.execution)}
                class:fail={result && failedResult(result.execution)}
                class:skip={result?.execution.skipped}
                class:chosen={at >= 0 && at === tab.selected}
                disabled={!result}
                title="{item.name}{result
                  ? ` · ${result.execution.response?.status ?? 'no response'}`
                  : ''}"
                aria-label="{item.name}, iteration {iteration + 1}{result
                  ? failedResult(result.execution)
                    ? ', failed'
                    : ', passed'
                  : ', not run yet'}"
                onclick={() => (tab.selected = at)}
              ></button>
            {/each}
          </div>
        {/each}
      </div>
    </section>

    <section class="results" aria-label="Requests in the run">
      <ol>
        {#each tab.results as result, i (i)}
          {@const execution = result.execution}
          {@const bad = failedResult(execution)}
          <li>
            <button
              class="line"
              class:selected={tab.selected === i}
              onclick={() => (tab.selected = tab.selected === i ? -1 : i)}
            >
              <span class="mark" class:bad aria-hidden="true"
                ><Icon name={bad ? "close" : "check"} size={7} /></span
              >
              <span
                class="method {methodClass(
                  execution.request?.method ?? tab.items[result.index]?.method ?? '',
                )}"
              >
                {execution.request?.method ?? tab.items[result.index]?.method}
              </span>
              <span class="name">{execution.name || tab.items[result.index]?.name}</span>
              {#if tab.iterationCount > 1}<span class="bitmap muted">#{result.iteration + 1}</span
                >{/if}
              <span
                class="status bitmap status-{execution.response
                  ? statusClass(execution.response.status)
                  : 'server'}"
              >
                {execution.response
                  ? execution.response.status
                  : execution.skipped
                    ? "skip"
                    : "err"}
              </span>
              <span class="time num muted"
                >{execution.response ? millis(execution.response.time_ms) : ""}</span
              >
            </button>
            {#if tab.selected === i && selectedResult}
              <div class="detail">
                {#if execution.error}<p class="why">{execution.error}</p>{/if}
                {#each execution.script_errors as error, j (j)}<p class="why">
                    Script in {error.origin}: {error.message}
                  </p>{/each}
                {#each [...execution.checks, ...execution.tests] as test, j (j)}
                  <p class="test" class:fail={!test.passed}>
                    <span class="tick" aria-hidden="true"
                      ><Icon name={test.passed ? "check" : "close"} size={7} /></span
                    >
                    {test.name}{#if test.message}<span class="msg"> — {test.message}</span>{/if}
                  </p>
                {/each}
                {#if execution.path}
                  <button
                    class="btn quiet"
                    onclick={() => execution.path && void openRequest(execution.path)}
                    >Open request</button
                  >
                {/if}
              </div>
            {/if}
          </li>
        {/each}
      </ol>
    </section>
  {:else if !tab.running}
    <div class="empty">
      <h2>Ready to run</h2>
      <p>
        Every request {tab.target === "" ? "in the workspace" : "in this folder"} runs in order, with
        its scripts and checks. The same run works in CI:
      </p>
      <pre class="mono">getman run {tab.target || ""}{tab.environment
          ? ` -e ${tab.environment}`
          : ""} --reporter junit=report.xml</pre>
    </div>
  {/if}
</div>

<style>
  .runner {
    display: grid;
    grid-template-rows: auto auto auto auto minmax(0, 1fr);
    height: 100%;
    min-height: 0;
    overflow: auto;
  }

  header {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--s3);
    padding: var(--s4) var(--s4) var(--s3);
  }

  .kind {
    margin: 0 0 6px;
    color: var(--ink-3);
    text-transform: none;
  }

  h2 {
    margin: 0;
    font-size: var(--b3);
    font-weight: 400;
    line-height: 1;
  }

  .config {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: var(--s3) var(--s4);
    padding: 0 var(--s4) var(--s3);
    border-bottom: 1px solid var(--rule);
  }

  .config label {
    display: grid;
    gap: 6px;
  }

  .config label > .bitmap {
    color: var(--ink-3);
  }

  .config .input[type="number"] {
    width: 90px;
  }

  .unit {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }

  .file {
    max-width: 210px;
    font-size: var(--fs-meta);
  }

  .toggle {
    display: flex !important;
    align-items: center;
    gap: var(--s2);
    height: var(--control);
  }

  .error {
    margin: var(--s3) var(--s4) 0;
    padding: var(--s2) var(--s3);
    background: var(--accent-wash);
    color: var(--error);
  }

  .matrix-wrap {
    padding: var(--s4) var(--s4) var(--s3);
    border-bottom: 1px solid var(--rule-2);
  }

  .tally {
    display: flex;
    align-items: baseline;
    gap: var(--s3);
    flex-wrap: wrap;
    margin-bottom: var(--s3);
  }

  .big {
    font-size: var(--b4);
    line-height: 1;
  }

  .big.good {
    color: var(--ok);
  }

  .big.bad {
    color: var(--error);
  }

  .slash {
    color: var(--t4);
  }

  .words {
    color: var(--ink-2);
  }

  .matrix {
    display: grid;
    gap: 3px;
  }

  .iteration {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 3px;
  }

  .label {
    width: 3ch;
    color: var(--ink-3);
  }

  .cell {
    width: 14px;
    height: 14px;
    padding: 0;
    border: 1px solid var(--rule-2);
    background: transparent;
  }

  .cell:disabled {
    cursor: default;
    background-image: radial-gradient(circle, var(--t3) 0.8px, transparent 1.2px);
    background-size: 4px 4px;
  }

  .cell.pass {
    background: var(--ok);
    border-color: var(--ok);
  }

  .cell.fail {
    background: var(--error);
    border-color: var(--error);
  }

  .cell.skip {
    background: var(--t4);
    border-color: var(--t4);
  }

  .cell.chosen {
    outline: 2px solid var(--ink);
    outline-offset: 1px;
  }

  .results ol {
    list-style: none;
    margin: 0;
    padding: 0 0 var(--s5);
  }

  .line {
    display: grid;
    grid-template-columns: 14px 56px minmax(0, 1fr) auto auto 70px;
    align-items: center;
    gap: var(--s3);
    width: 100%;
    padding: 7px var(--s4);
    border: 0;
    border-bottom: 1px solid var(--rule-3);
    background: none;
    text-align: left;
    color: var(--ink);
  }

  .line:hover {
    background: var(--sunken);
  }

  .line.selected {
    background: var(--t1);
  }

  .mark {
    display: grid;
    place-items: center;
    width: 14px;
    height: 14px;
    background: var(--ok);
    color: var(--paper);
  }

  .mark.bad {
    background: var(--error);
  }

  .name {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .time {
    text-align: right;
  }

  .detail {
    display: grid;
    gap: 4px;
    justify-items: start;
    padding: var(--s2) var(--s4) var(--s3) calc(var(--s4) + 14px + var(--s3));
    border-bottom: 1px solid var(--rule-3);
    background: var(--field);
  }

  .detail p {
    margin: 0;
  }

  .why {
    color: var(--error);
  }

  .test {
    display: flex;
    gap: var(--s2);
    align-items: baseline;
  }

  .tick {
    color: var(--ok);
  }

  .test.fail .tick,
  .test.fail {
    color: var(--error);
  }

  .msg {
    color: var(--ink-2);
  }

  .empty pre {
    margin: 0;
    padding: var(--s2) var(--s3);
    background: var(--field);
    border: 1px solid var(--rule-3);
    max-width: 100%;
    overflow-x: auto;
  }
</style>
