<script lang="ts">
  import { overflowFade } from "../../lib/actions";
  import { copyText } from "../../lib/clipboard";
  import { bytes, millis } from "../../lib/format";
  import { grpcState } from "../../lib/grpc.svelte";
  import { openEnvironments, type RequestTab } from "../../lib/store.svelte";
  import type { Header, StreamMessage } from "../../lib/types";
  import Icon from "../Icon.svelte";
  import Specimen from "../response/Specimen.svelte";

  let { tab }: { tab: RequestTab } = $props();

  const rpc = $derived(grpcState(tab));
  const live = $derived(rpc.call);
  const execution = $derived(tab.execution);
  const call = $derived(execution?.grpc ?? null);
  const status = $derived(call?.status ?? null);
  const messages = $derived<StreamMessage[]>(live ? live.messages : (call?.messages ?? []));
  const headers = $derived<Header[]>(live ? live.headers : (call?.headers ?? []));
  const trailers = $derived<Header[]>(live ? [] : (call?.trailers ?? []));
  const results = $derived(execution ? [...execution.checks, ...execution.tests] : []);
  const failing = $derived(
    results.filter((r) => !r.passed).length + (execution?.script_errors.length ?? 0),
  );
  const received = $derived(messages.filter((m) => m.direction === "received").length);

  const panes = $derived([
    { id: "messages", label: "Messages", note: messages.length ? String(messages.length) : "" },
    {
      id: "headers",
      label: "Metadata",
      note: headers.length + trailers.length ? String(headers.length + trailers.length) : "",
    },
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

  const pane = $derived(
    panes.some((p) => p.id === tab.responsePane) ? tab.responsePane : "messages",
  );

  function errorHint(error: string): string {
    if (error.startsWith("Could not connect")) {
      if (error.includes("TLS") || error.includes("certificate"))
        return "Check that the server speaks TLS. For a self-signed certificate, turn off Check TLS certificates in Settings.";
      if (error.includes("HTTP/2"))
        return "The server may expect TLS: turn on TLS, or check the port.";
      return "Is the server running, and is the address right? Check it and the environment.";
    }
    if (error.includes("reflection"))
      return "Choose the .proto files for this request in Schema instead.";
    return "";
  }

  function pretty(value: unknown): string {
    return JSON.stringify(value, null, 2);
  }
</script>

<div class="response">
  <Specimen
    {execution}
    sending={tab.sending}
    tone={status ? (status.code === 0 ? "ok" : "client") : undefined}
    busy={live?.callType === "client_streaming" || live?.callType === "bidi_streaming"
      ? "Streaming"
      : "Calling"}
    action="Invoke"
  />

  {#if live}
    <div class="live" aria-live="polite">
      <span class="dot" aria-hidden="true"></span>
      {#if live.ended}
        Stream ended; waiting for the server.
      {:else if live.callType === "client_streaming" || live.callType === "bidi_streaming"}
        Stream open. Send messages from the editor, then end the stream.
      {:else}
        Waiting for the server.
      {/if}
      <span class="muted">{received} received</span>
    </div>
  {:else if execution}
    {#if execution.error}
      <div class="error" role="alert">
        <p class="message">{execution.error}</p>
        {#if errorHint(execution.error)}<p class="hint">{errorHint(execution.error)}</p>{/if}
      </div>
    {:else if status && status.code !== 0}
      <div class="error" role="alert">
        <p class="message">{status.name}{status.message ? `: ${status.message}` : ""}</p>
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

  {#if live || execution}
    <div class="tablist" use:overflowFade role="tablist" aria-label="Response parts">
      {#each panes as item (item.id)}
        <button
          class="tab"
          role="tab"
          aria-selected={pane === item.id}
          onclick={() => (tab.responsePane = item.id)}
        >
          {item.label}
          {#if item.note}<span class="count" class:bad={"bad" in item && item.bad}>{item.note}</span
            >{/if}
        </button>
      {/each}
    </div>
    <div class="pane">
      {#if pane === "messages"}
        {#if status?.details}
          <section class="details">
            <h3 class="bitmap">Error details</h3>
            <pre class="json">{pretty(status.details)}</pre>
          </section>
        {/if}
        {#if messages.length}
          <ol class="messages">
            {#each messages as item, i (i)}
              <li class={item.direction}>
                <div class="meta">
                  <span class="bitmap direction"
                    >{item.direction === "sent" ? "Sent" : "Received"}</span
                  >
                  <span class="num muted">{millis(item.time_ms)} · {bytes(item.size)}</span>
                  <button
                    class="icon-btn"
                    aria-label="Copy message"
                    title="Copy"
                    onclick={() => void copyText(pretty(item.message), "Message copied")}
                    ><Icon name="copy" /></button
                  >
                </div>
                {#if item.error}<p class="why">Could not read it: {item.error}</p>{/if}
                <pre class="json">{pretty(item.message)}</pre>
              </li>
            {/each}
          </ol>
          {#if call?.dropped_messages}
            <p class="none">{call.dropped_messages} later messages were not kept.</p>
          {/if}
        {:else}
          <p class="none">{live ? "No messages yet." : "No messages were sent or received."}</p>
        {/if}
      {:else if pane === "headers"}
        <h3 class="bitmap">Headers</h3>
        {#if headers.length}
          <dl class="pairs">
            {#each headers as header, i (i)}
              <dt class="mono">{header.name}</dt>
              <dd class="mono">{header.value}</dd>
            {/each}
          </dl>
        {:else}
          <p class="none">None yet.</p>
        {/if}
        <h3 class="bitmap">Trailers</h3>
        {#if trailers.length}
          <dl class="pairs">
            {#each trailers as header, i (i)}
              <dt class="mono">{header.name}</dt>
              <dd class="mono">{header.value}</dd>
            {/each}
          </dl>
        {:else}
          <p class="none">
            {live ? "They arrive when the call ends." : "None: the status came with the headers."}
          </p>
        {/if}
        {#if call}
          <p class="note muted">
            {call.tls ? "TLS" : "Plaintext"} HTTP/2 to {call.address}. Checks read both lists as
            <code>header.name</code>, headers first.
          </p>
        {/if}
      {:else if pane === "tests"}
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
          {#each results as result, i (i)}
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
          {#if !results.length && !execution.script_errors.length}
            <p class="none">
              No tests yet. Add checks such as <code>status == 0</code> or
              <code>json.id exists</code> in the Checks tab.
            </p>
          {/if}
        {:else}
          <p class="none">Results appear when the call ends.</p>
        {/if}
      {:else if pane === "console"}
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
      {:else if pane === "sent"}
        {#if execution?.request}
          <p class="line mono">{execution.request.url}</p>
          {#if execution.request.headers.length}
            <dl class="pairs">
              {#each execution.request.headers as header, i (i)}
                <dt class="mono">{header.name}</dt>
                <dd class="mono">{header.value}</dd>
              {/each}
            </dl>
          {:else}
            <p class="none">No metadata of your own.</p>
          {/if}
          <p class="note muted">
            The metadata that went out, after variables, folders and authentication. gRPC adds
            content-type, te, user-agent and the deadline itself.
          </p>
        {:else}
          <p class="none">The call was not made.</p>
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

  .live {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: 6px var(--s4);
    border-bottom: 1px solid var(--rule-3);
  }

  .live .muted {
    margin-left: auto;
  }

  .dot {
    width: 6px;
    height: 6px;
    background: var(--accent);
    animation: blink 1s steps(2) infinite;
  }

  @keyframes blink {
    50% {
      opacity: 0;
    }
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

  .none {
    margin: 0 0 var(--s3);
    color: var(--ink-3);
  }

  h3 {
    margin: var(--s3) 0 var(--s2);
    color: var(--ink-3);
  }

  h3:first-child {
    margin-top: 0;
  }

  .messages {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .messages li {
    padding: var(--s2) 0 var(--s2) var(--s3);
    border-bottom: 1px solid var(--rule-3);
    border-left: 3px solid var(--ok);
  }

  .messages li.sent {
    border-left-color: var(--t4);
  }

  .messages .meta {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }

  .messages .meta .icon-btn {
    margin-left: auto;
  }

  .direction {
    color: var(--ink-3);
  }

  .json {
    margin: 4px 0 0;
    font: var(--fs-code) / 1.5 var(--font-code);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .details {
    margin-bottom: var(--s3);
    padding-bottom: var(--s3);
    border-bottom: 1px solid var(--rule);
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

  .note {
    margin: var(--s3) 0 0;
    font-size: var(--fs-meta);
  }

  .line {
    margin: 0 0 var(--s2);
    word-break: break-all;
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
    margin: 0;
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

  @media (prefers-reduced-motion: reduce) {
    .dot {
      animation: none;
    }
  }
</style>
