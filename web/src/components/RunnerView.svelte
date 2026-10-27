<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { ago, millis, methodClass, statusClass } from "../lib/format";
  import { moveTo, planFor, refreshPlan, runTargets } from "../lib/plan";
  import {
    REPORT_FORMATS,
    downloadReport,
    forgetRun,
    pastRuns,
    refreshRuns,
    runPassed,
    runTally,
    runWhat,
  } from "../lib/runs.svelte";
  import {
    findNode,
    openRequest,
    reopenRun,
    startRun,
    stopRun,
    ws,
    type RunnerTab,
  } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";
  import Menu from "./Menu.svelte";

  let { tab }: { tab: RunnerTab } = $props();

  let planOpen = $state(untrack(() => tab.results.length === 0));
  let dragFrom = $state(-1);
  let dropAt = $state(-1);
  let reportMenu = $state<{ x: number; y: number; id: string } | null>(null);

  onMount(() => {
    void refreshRuns();
    tab.plan = refreshPlan(tab.plan, planFor(ws.tree, tab.target));
  });

  const title = $derived(tab.target === "" ? ws.name : (findNode(tab.target)?.name ?? tab.target));
  const ticked = $derived(tab.plan.filter((item) => item.include).length);

  async function run(): Promise<void> {
    await startRun(tab);
    if (tab.summary) planOpen = false;
    await refreshRuns();
  }

  function tickAll(include: boolean): void {
    tab.plan = tab.plan.map((item) => ({ ...item, include }));
  }

  function resetPlan(): void {
    tab.plan = planFor(ws.tree, tab.target);
  }

  async function move(from: number, to: number): Promise<void> {
    if (to < 0 || to >= tab.plan.length) return;
    const path = tab.plan[from]?.path;
    tab.plan = moveTo(tab.plan, from, to);
    await tick();
    document.querySelector<HTMLButtonElement>(`[data-grip="${CSS.escape(path ?? "")}"]`)?.focus();
  }

  function gripKeys(event: KeyboardEvent, index: number): void {
    if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      void move(index, index + (event.key === "ArrowUp" ? -1 : 1));
    }
  }

  function drop(event: DragEvent, index: number): void {
    event.preventDefault();
    if (dragFrom >= 0) tab.plan = moveTo(tab.plan, dragFrom, index);
    dragFrom = -1;
    dropAt = -1;
  }

  function openReports(event: MouseEvent, id: string): void {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    reportMenu = { x: rect.left, y: rect.bottom + 4, id };
  }

  /** The same run from a terminal. */
  const cliCommand = $derived.by(() => {
    const picked = runTargets(tab.target, tab.plan, planFor(ws.tree, tab.target));
    const paths = picked.targets ?? (tab.target ? [tab.target] : []);
    return [
      "getman run",
      ...paths,
      tab.environment ? `-e ${tab.environment}` : "",
      "-r html=report.html",
    ]
      .filter(Boolean)
      .join(" ");
  });

  const reportItems = $derived(
    reportMenu
      ? REPORT_FORMATS.map(({ format, label }) => ({
          label,
          icon: "export" as const,
          action: () => void downloadReport(reportMenu!.id, format),
        }))
      : [],
  );
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
      <h2 class="bitmap">{title}</h2>
      <p class="file mono">
        {ticked === tab.plan.length
          ? tab.target === ""
            ? "Every request in the workspace"
            : `Every request in ${tab.target}`
          : `${ticked} of ${tab.plan.length} requests ${tab.target === "" ? "in the workspace" : `in ${tab.target}`}`}
      </p>
    </div>
    <div class="go">
      {#if tab.running}
        <button class="btn" onclick={() => void stopRun(tab)}><Icon name="stop" /> Stop</button>
      {:else}
        <button class="btn primary" onclick={() => void run()} disabled={ticked === 0}
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
        <label class="btn" class:disabled={tab.running}>
          <Icon name="file" /> Choose a file
          <input
            type="file"
            accept=".csv,.json,text/csv,application/json"
            class="sr-only"
            onchange={loadData}
            disabled={tab.running}
          />
        </label>
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

  <section class="plan" aria-label="Requests to run">
    <div class="plan-head">
      <button
        class="fold"
        aria-expanded={planOpen}
        onclick={() => (planOpen = !planOpen)}
        aria-controls="plan-{tab.id}"
      >
        <Icon name={planOpen ? "chevronDown" : "chevronRight"} size={7} />
        <span class="bitmap">Requests</span>
        <span class="bitmap muted">{ticked}/{tab.plan.length}</span>
      </button>
      {#if planOpen}
        <span class="plan-tools">
          <button class="btn quiet" onclick={() => tickAll(true)} disabled={tab.running}>All</button
          >
          <button class="btn quiet" onclick={() => tickAll(false)} disabled={tab.running}
            >None</button
          >
          <button class="btn quiet" onclick={resetPlan} disabled={tab.running}>Reset order</button>
        </span>
      {/if}
    </div>
    {#if planOpen}
      {#if tab.plan.length === 0}
        <p class="none">There are no requests here yet.</p>
      {:else}
        <p class="hint muted">
          Untick what to leave out. Drag a request, or focus its handle and press <kbd>↑</kbd>
          <kbd>↓</kbd>, to change the order. The files are not changed.
        </p>
        <ol class="plan-list" id="plan-{tab.id}">
          {#each tab.plan as item, index (item.path)}
            <li
              class:off={!item.include}
              class:drop={dropAt === index && dragFrom !== index}
              draggable={!tab.running}
              ondragstart={(event) => {
                dragFrom = index;
                event.dataTransfer?.setData("text/plain", item.path);
                if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
              }}
              ondragover={(event) => {
                event.preventDefault();
                dropAt = index;
              }}
              ondrop={(event) => drop(event, index)}
              ondragend={() => ((dragFrom = -1), (dropAt = -1))}
            >
              <button
                class="grip"
                data-grip={item.path}
                aria-label="Move {item.name}: arrow keys"
                title="Drag, or press the arrow keys, to move"
                disabled={tab.running}
                onkeydown={(event) => gripKeys(event, index)}
              >
                <span aria-hidden="true"></span>
              </button>
              <label class="pick">
                <input
                  type="checkbox"
                  class="check"
                  bind:checked={item.include}
                  disabled={tab.running}
                />
                <span class="method {methodClass(item.method)}">{item.method}</span>
                <span class="name">{item.name}</span>
                <span class="path mono muted">{item.path}</span>
              </label>
            </li>
          {/each}
        </ol>
      {/if}
    {/if}
  </section>

  {#if tab.reopened}
    <p class="past-note">
      A past run, from {new Date(tab.reopened.started).toLocaleString()}, of
      {runWhat(tab.reopened, ws.name)}{tab.reopened.environment
        ? ` with ${tab.reopened.environment}`
        : ""}. Bodies and headers are not kept; Run starts a new one.
    </p>
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
        {#if tab.shownRun && !tab.running}
          <button
            class="btn report"
            aria-haspopup="menu"
            onclick={(event) => openReports(event, tab.shownRun)}
          >
            <Icon name="export" /> Report
          </button>
        {/if}
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
        The ticked requests run in the order above, with their scripts and checks. The same run
        works in CI:
      </p>
      <pre class="mono">{cliCommand}</pre>
    </div>
  {/if}

  <section class="past" aria-label="Past runs">
    <h3 class="bitmap">Past runs</h3>
    {#if pastRuns.list.length === 0}
      <p class="none">
        {pastRuns.loaded
          ? "None yet. Runs from here are kept in .getman/runs, the newest 20, with their reports."
          : "Loading…"}
      </p>
    {:else}
      <ol>
        {#each pastRuns.list as past (past.id)}
          {@const tally = runTally(past.summary)}
          {@const good = runPassed(past.summary)}
          <li class:current={tab.shownRun === past.id}>
            <button
              class="past-line"
              onclick={() => void reopenRun(tab, past.id)}
              disabled={tab.running}
              title="Show this run's results"
            >
              <span class="mark" class:bad={!good} aria-label={good ? "passed" : "failed"}
                ><Icon name={good ? "check" : "close"} size={7} /></span
              >
              <span class="name">{runWhat(past.about, ws.name)}</span>
              <span class="muted env">{past.about.environment ?? ""}</span>
              <span class="num" class:bad-text={!good}>{tally.passed}/{tally.total}</span>
              <span class="num muted">{past.summary ? millis(past.summary.duration_ms) : ""}</span>
              <span class="muted when" title={new Date(past.about.started).toLocaleString()}
                >{ago(past.about.started)}</span
              >
            </button>
            <button
              class="icon-btn"
              aria-label="Download a report of this run"
              title="Report"
              aria-haspopup="menu"
              onclick={(event) => openReports(event, past.id)}
            >
              <Icon name="export" />
            </button>
            <button
              class="icon-btn"
              aria-label="Forget this run"
              title="Forget"
              onclick={() => void forgetRun(past.id)}
            >
              <Icon name="trash" />
            </button>
          </li>
        {/each}
      </ol>
    {/if}
  </section>
</div>

{#if reportMenu}
  <Menu
    items={reportItems}
    x={reportMenu.x}
    y={reportMenu.y}
    label="Download a report"
    onClose={() => (reportMenu = null)}
  />
{/if}

<style>
  .runner {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: auto;
  }

  .runner > * {
    flex: none;
  }

  header {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--s3);
    padding: var(--s4) var(--s4) var(--s3);
  }

  .file {
    margin: 6px 0 0;
    color: var(--ink-3);
    font-size: var(--fs-meta);
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

  .config label.btn {
    display: inline-flex;
  }

  .btn.disabled {
    opacity: 0.45;
    pointer-events: none;
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

  .empty {
    height: auto;
    place-content: start;
    padding: var(--s5) var(--s4);
  }

  .empty pre {
    margin: 0;
    padding: var(--s2) var(--s3);
    background: var(--field);
    border: 1px solid var(--rule-3);
    max-width: 100%;
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-all;
  }

  /* ---- The requests to run ---- */

  .plan {
    border-bottom: 1px solid var(--rule-2);
  }

  .plan-head {
    display: flex;
    align-items: center;
    gap: var(--s2);
    flex-wrap: wrap;
    padding: var(--s2) var(--s3);
  }

  .fold {
    display: flex;
    align-items: center;
    gap: var(--s2);
    height: var(--control);
    padding: 0 var(--s2);
    border: 0;
    background: none;
    color: var(--ink);
  }

  .fold:hover {
    background: var(--sunken);
  }

  .plan-tools {
    display: flex;
    gap: 2px;
    margin-left: auto;
  }

  .hint {
    margin: 0;
    padding: 0 var(--s4) var(--s2);
    font-size: var(--fs-meta);
  }

  .none {
    margin: 0;
    padding: 0 var(--s4) var(--s3);
    color: var(--ink-3);
  }

  .plan-list {
    list-style: none;
    margin: 0;
    padding: 0 0 var(--s3);
    max-height: 320px;
    overflow: auto;
  }

  .plan-list li {
    display: flex;
    align-items: center;
    gap: var(--s1);
    padding: 0 var(--s4) 0 var(--s2);
    border-top: 2px solid transparent;
  }

  .plan-list li:hover {
    background: var(--sunken);
  }

  .plan-list li.drop {
    border-top-color: var(--accent);
  }

  .plan-list li.off .name,
  .plan-list li.off .method {
    color: var(--ink-3);
    text-decoration: line-through;
  }

  .grip {
    display: grid;
    place-items: center;
    width: 20px;
    height: var(--control);
    padding: 0;
    border: 0;
    background: none;
    color: var(--ink-3);
    cursor: grab;
  }

  .grip span {
    width: 6px;
    height: 10px;
    background-image: radial-gradient(circle, currentColor 0.9px, transparent 1.3px);
    background-size: 3px 3px;
  }

  .grip:hover:not(:disabled),
  .grip:focus-visible {
    color: var(--ink);
  }

  .pick {
    display: grid;
    grid-template-columns: 14px 56px minmax(0, max-content) minmax(0, 1fr);
    align-items: center;
    gap: var(--s3);
    flex: 1;
    min-width: 0;
    height: var(--control);
  }

  .pick .path {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: var(--fs-meta);
  }

  .past-note {
    margin: var(--s3) var(--s4) 0;
    padding: var(--s2) var(--s3);
    border-left: 3px solid var(--ink);
    background: var(--field);
    color: var(--ink-2);
  }

  .report {
    margin-left: auto;
  }

  /* ---- Past runs ---- */

  .past {
    padding: var(--s4) 0 var(--s5);
    border-top: 1px solid var(--rule-2);
  }

  .past h3 {
    margin: 0 0 var(--s2);
    padding: 0 var(--s4);
    color: var(--ink-3);
    font-weight: 400;
  }

  .past ol {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .past li {
    display: flex;
    align-items: center;
    gap: 2px;
    padding-right: var(--s3);
    border-bottom: 1px solid var(--rule-3);
  }

  .past li.current {
    background: var(--t1);
  }

  .past-line {
    display: grid;
    grid-template-columns: 14px minmax(0, 1fr) auto auto 70px 90px;
    align-items: center;
    gap: var(--s3);
    flex: 1;
    min-width: 0;
    padding: 6px var(--s4);
    border: 0;
    background: none;
    text-align: left;
    color: var(--ink);
  }

  .past-line:hover:not(:disabled) {
    background: var(--sunken);
  }

  .past-line .num {
    text-align: right;
  }

  .past-line .when {
    text-align: right;
    white-space: nowrap;
  }

  .env {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    max-width: 16ch;
  }

  .bad-text {
    color: var(--error);
  }

  @media (max-width: 640px) {
    .pick {
      grid-template-columns: 14px 48px minmax(0, 1fr);
    }

    .pick .path,
    .past-line .env,
    .past-line .when {
      display: none;
    }

    .past-line {
      grid-template-columns: 14px minmax(0, 1fr) auto 70px;
    }
  }
</style>
