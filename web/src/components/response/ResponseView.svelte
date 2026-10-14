<script lang="ts">
  import { overflowFade } from "../../lib/actions";
  import { copyText } from "../../lib/clipboard";
  import { payloadText } from "../../lib/format";
  import { openDialog, openEnvironments, type RequestTab } from "../../lib/store.svelte";
  import Icon from "../Icon.svelte";
  import BodyViewer from "./BodyViewer.svelte";
  import Specimen from "./Specimen.svelte";

  let { tab }: { tab: RequestTab } = $props();

  const execution = $derived(tab.execution);
  const response = $derived(execution?.response ?? null);
  const results = $derived(execution ? [...execution.checks, ...execution.tests] : []);
  const failing = $derived(
    results.filter((r) => !r.passed).length + (execution?.script_errors.length ?? 0),
  );

  const cookies = $derived.by(() => {
    if (!response) return [];
    return response.headers
      .filter((h) => h.name.toLowerCase() === "set-cookie")
      .map((h) => {
        const [pair = "", ...attributes] = h.value.split(";");
        const eq = pair.indexOf("=");
        const attrs: Record<string, string> = {};
        for (const attribute of attributes) {
          const [name = "", ...value] = attribute.split("=");
          attrs[name.trim().toLowerCase()] = value.join("=").trim() || "yes";
        }
        return { name: pair.slice(0, eq).trim(), value: pair.slice(eq + 1).trim(), attrs };
      });
  });

  const panes = $derived([
    { id: "body", label: "Body", note: "" },
    { id: "headers", label: "Headers", note: response ? String(response.headers.length) : "" },
    { id: "cookies", label: "Cookies", note: cookies.length ? String(cookies.length) : "" },
    {
      id: "tests",
      label: "Tests",
      note:
        results.length || execution?.script_errors.length
          ? `${results.length - results.filter((r) => !r.passed).length}/${results.length}`
          : "",
      bad: failing > 0,
    },
    {
      id: "console",
      label: "Console",
      note: execution?.logs.length ? String(execution.logs.length) : "",
    },
    { id: "sent", label: "Sent", note: "" },
  ]);

  function errorHint(error: string): string {
    if (error.startsWith("Could not connect"))
      return "Is the server running, and is the host and port right? Check the URL and the environment.";
    if (error.startsWith("The request timed out"))
      return "Raise the timeout in the request's Settings, or check that the server answers.";
    if (error.includes("certificate"))
      return "For a self-signed certificate, turn off Check TLS certificates in Settings.";
    if (error.includes("not a valid URL"))
      return "Fill in the variables it uses, or check the URL for typos.";
    return "";
  }

  const sentBody = $derived.by(() => {
    const body = execution?.request?.body;
    if (!body || body.type === "none") return "";
    if (body.type === "bytes") return payloadText(body.data);
    if (body.type === "file") return `The contents of ${body.path}`;
    return body.parts
      .map((part) => `${part.name}: ${part.file ? `file ${part.file}` : (part.value ?? "")}`)
      .join("\n");
  });
</script>

<div class="response">
  <Specimen {execution} sending={tab.sending} />

  {#if execution && !tab.sending}
    {#if execution.error}
      <div class="error" role="alert">
        <p class="message">{execution.error}</p>
        {#if errorHint(execution.error)}<p class="hint">{errorHint(execution.error)}</p>{/if}
      </div>
    {/if}
    {#if execution.missing_variables.length}
      <div class="warning">
        <Icon name="warning" />
        <p>
          Not defined:
          {#each execution.missing_variables as name, i (name)}<code>{`{{${name}}}`}</code>{i <
            execution.missing_variables.length - 1
              ? ", "
              : ""}{/each}. They were sent as written.
        </p>
        <button class="btn quiet" onclick={() => openEnvironments()}>Environments</button>
      </div>
    {/if}
    {#if execution.warnings.length}
      <div class="warning">
        <Icon name="warning" />
        <p>{execution.warnings.join(" ")}</p>
      </div>
    {/if}
  {/if}

  {#if response || (execution && !tab.sending && (results.length || execution.logs.length || execution.script_errors.length || execution.request))}
    <div class="tablist" use:overflowFade role="tablist" aria-label="Response parts">
      {#each panes as pane (pane.id)}
        <button
          class="tab"
          role="tab"
          aria-selected={tab.responsePane === pane.id}
          onclick={() => (tab.responsePane = pane.id)}
        >
          {pane.label}
          {#if pane.note}<span class="count" class:bad={"bad" in pane && pane.bad}>{pane.note}</span
            >{/if}
        </button>
      {/each}
    </div>
    <div class="pane" class:fill={tab.responsePane === "body"}>
      {#if tab.responsePane === "body"}
        {#if response}
          <BodyViewer {response} />
        {:else}
          <p class="none">No response arrived.</p>
        {/if}
      {:else if tab.responsePane === "headers"}
        {#if response}
          <dl class="pairs">
            {#each response.headers as header, i (i)}
              <dt class="mono">{header.name}</dt>
              <dd class="mono">
                <button
                  class="value"
                  title="Copy"
                  onclick={() => void copyText(header.value, `${header.name} copied`)}
                  >{header.value}</button
                >
              </dd>
            {/each}
          </dl>
          <p class="meta muted">
            {response.http_version} · from {response.url}{response.remote_addr
              ? ` (${response.remote_addr})`
              : ""}
          </p>
        {/if}
      {:else if tab.responsePane === "cookies"}
        {#if cookies.length}
          <table class="cookies">
            <thead>
              <tr
                ><th class="bitmap">Name</th><th class="bitmap">Value</th><th class="bitmap"
                  >Attributes</th
                ></tr
              >
            </thead>
            <tbody>
              {#each cookies as cookie, i (i)}
                <tr>
                  <td class="mono">{cookie.name}</td>
                  <td class="mono value-cell">{cookie.value}</td>
                  <td class="muted"
                    >{Object.entries(cookie.attrs)
                      .map(([k, v]) => (v === "yes" ? k : `${k}=${v}`))
                      .join(" · ")}</td
                  >
                </tr>
              {/each}
            </tbody>
          </table>
        {:else}
          <p class="none">The response set no cookies.</p>
        {/if}
        <button class="btn quiet" onclick={() => openDialog({ kind: "cookies" })}>
          <Icon name="cookie" /> Every cookie Getman holds
        </button>
      {:else if tab.responsePane === "tests"}
        {#if execution}
          {#each execution.script_errors as scriptError, i (i)}
            <div class="result fail">
              <span class="mark"><Icon name="warning" size={7} /></span>
              <div>
                <p class="name">Script in {scriptError.origin} failed</p>
                <p class="why">{scriptError.message}</p>
              </div>
            </div>
          {/each}
          {#if execution.checks.length}
            <h3 class="bitmap">Checks</h3>
            {#each execution.checks as result, i (i)}
              <div class="result" class:fail={!result.passed}>
                <span class="mark"><Icon name={result.passed ? "check" : "close"} size={7} /></span>
                <div>
                  <p class="name mono">{result.name}</p>
                  {#if result.message}<p class="why">{result.message}</p>{/if}
                </div>
              </div>
            {/each}
          {/if}
          {#if execution.tests.length}
            <h3 class="bitmap">Script tests</h3>
            {#each execution.tests as result, i (i)}
              <div class="result" class:fail={!result.passed} class:skip={result.skipped}>
                <span class="mark"
                  ><Icon
                    name={result.skipped ? "more" : result.passed ? "check" : "close"}
                    size={7}
                  /></span
                >
                <div>
                  <p class="name">{result.name}</p>
                  {#if result.message}<p class="why">{result.message}</p>{/if}
                </div>
              </div>
            {/each}
          {/if}
          {#if !results.length && !execution.script_errors.length}
            <p class="none">
              No tests yet. Add checks such as <code>status == 200</code> in the Checks tab, or
              <code>gm.test(...)</code> in a post-response script.
            </p>
          {/if}
        {/if}
      {:else if tab.responsePane === "console"}
        {#if execution?.logs.length}
          <ol class="logs">
            {#each execution.logs as log, i (i)}
              <li class={log.level}>
                <span class="origin bitmap">{log.origin}</span>
                <pre>{log.message}</pre>
              </li>
            {/each}
          </ol>
        {:else}
          <p class="none">Nothing logged. <code>console.log</code> in a script writes here.</p>
        {/if}
      {:else if tab.responsePane === "sent"}
        {#if execution?.request}
          <p class="line mono">
            <strong>{execution.request.method}</strong>
            {execution.request.url}
          </p>
          <dl class="pairs">
            {#each execution.request.headers as header, i (i)}
              <dt class="mono">{header.name}</dt>
              <dd class="mono">{header.value}</dd>
            {/each}
          </dl>
          {#if sentBody}
            <pre class="sent-body">{sentBody}</pre>
          {/if}
          <p class="meta muted">
            Exactly what went out, after variables, folders and authentication. Cookies and
            Content-Length are added on the wire.
          </p>
        {:else}
          <p class="none">The request was not sent.</p>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .response {
    display: grid;
    grid-template-rows: auto auto auto auto auto minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .error {
    padding: var(--s3) var(--s4);
    background: var(--accent-wash);
    border-bottom: 1px solid var(--error);
  }

  .error p {
    margin: 0;
  }

  .message {
    color: var(--error);
    font-weight: 600;
    word-break: break-word;
  }

  .hint {
    margin-top: 4px !important;
    color: var(--ink-2);
  }

  .warning {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: 6px var(--s4);
    background: var(--warn-wash);
    color: var(--ink);
    border-bottom: 1px solid var(--rule-3);
  }

  .warning p {
    flex: 1;
    margin: 0;
  }

  .warning :global(.icon) {
    color: var(--warn);
  }

  code {
    font-family: var(--font-code);
  }

  .tablist {
    padding: 0 var(--s2);
  }

  .pane {
    min-height: 0;
    overflow: auto;
    padding: var(--s3) var(--s4);
  }

  .pane.fill {
    padding: 0;
    overflow: hidden;
  }

  .none {
    margin: 0 0 var(--s3);
    color: var(--ink-3);
  }

  .pairs {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    margin: 0;
  }

  .pairs dt,
  .pairs dd {
    padding: 5px var(--s3) 5px 0;
    border-bottom: 1px solid var(--rule-3);
  }

  .pairs dt {
    color: var(--ink-2);
  }

  .pairs dd {
    margin: 0;
    word-break: break-all;
  }

  .value {
    padding: 0;
    border: 0;
    background: none;
    text-align: left;
    color: inherit;
    font: inherit;
  }

  .value:hover {
    text-decoration: underline;
  }

  .meta {
    margin: var(--s3) 0 0;
    font-size: var(--fs-meta);
    word-break: break-all;
  }

  .cookies {
    width: 100%;
    border-collapse: collapse;
    margin-bottom: var(--s3);
  }

  .cookies th {
    padding: 0 var(--s3) 6px 0;
    border-bottom: 1px solid var(--rule);
    text-align: left;
    color: var(--ink-3);
    font-weight: 400;
  }

  .cookies td {
    padding: 6px var(--s3) 6px 0;
    border-bottom: 1px solid var(--rule-3);
    vertical-align: top;
  }

  .value-cell {
    word-break: break-all;
  }

  h3 {
    margin: var(--s3) 0 var(--s2);
    color: var(--ink-3);
  }

  h3:first-child {
    margin-top: 0;
  }

  .result {
    display: grid;
    grid-template-columns: 14px minmax(0, 1fr);
    gap: var(--s2);
    padding: 6px 0;
    border-bottom: 1px solid var(--rule-3);
  }

  .result p {
    margin: 0;
  }

  .mark {
    display: grid;
    place-items: center;
    width: 14px;
    height: 14px;
    margin-top: 2px;
    background: var(--ok);
    color: var(--paper);
  }

  .fail .mark {
    background: var(--error);
  }

  .skip .mark {
    background: var(--t4);
  }

  .why {
    color: var(--error);
    font-size: var(--fs-meta);
  }

  .logs {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .logs li {
    display: grid;
    grid-template-columns: 110px minmax(0, 1fr);
    gap: var(--s2);
    padding: 4px 0;
    border-bottom: 1px solid var(--rule-3);
  }

  .logs .origin {
    color: var(--ink-3);
    padding-top: 3px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .logs pre {
    margin: 0;
    font: var(--fs-code) / 1.5 var(--font-code);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .logs .warn pre {
    color: var(--warn);
  }

  .logs .error pre {
    color: var(--error);
  }

  .logs .debug pre {
    color: var(--ink-3);
  }

  .line {
    margin: 0 0 var(--s2);
    word-break: break-all;
  }

  .sent-body {
    margin: var(--s3) 0 0;
    padding: var(--s3);
    border: 1px solid var(--rule-3);
    background: var(--field);
    font: var(--fs-code) / 1.5 var(--font-code);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 320px;
    overflow: auto;
  }
</style>
