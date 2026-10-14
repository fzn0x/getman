<script lang="ts">
  import { BODY_TYPES, convertBody } from "../../lib/draft";
  import { prettyJson } from "../../lib/format";
  import { notify, ws } from "../../lib/store.svelte";
  import type { Body, FormField } from "../../lib/types";
  import Editor from "../Editor.svelte";
  import Icon from "../Icon.svelte";
  import KeyValueTable from "./KeyValueTable.svelte";

  let { body = $bindable() }: { body: Body } = $props();

  const known = $derived(new Set(ws.variables.map((v) => v.name)));

  function setType(type: Body["type"]): void {
    if (type !== body.type) body = convertBody(body, type);
  }

  function beautify(): void {
    if (body.type !== "json") return;
    try {
      // Variables such as {{id}} are not JSON; stand them in for parsing.
      const holders: string[] = [];
      const masked = body.content.replace(/\{\{[^{}]+\}\}/g, (match) => {
        holders.push(match);
        return `"__gm_var_${holders.length - 1}__"`;
      });
      let pretty = prettyJson(masked);
      if (pretty === masked) JSON.parse(masked);
      pretty = pretty.replace(/"__gm_var_(\d+)__"/g, (_, i: string) => holders[Number(i)] ?? "");
      body.content = pretty;
    } catch {
      notify("The body is not valid JSON, so it cannot be formatted.", "error");
    }
  }

  function minify(): void {
    if (body.type !== "json") return;
    try {
      body.content = JSON.stringify(JSON.parse(body.content));
    } catch {
      notify("The body is not valid JSON, so it cannot be minified.", "error");
    }
  }

  function setField(index: number, patch: Partial<FormField>): void {
    if (body.type !== "multipart") return;
    body.fields = body.fields.map((field, i) => (i === index ? { ...field, ...patch } : field));
  }
</script>

<div class="body-pane">
  <div class="types" role="radiogroup" aria-label="Body type">
    {#each BODY_TYPES as option (option.type)}
      <label class="type" class:on={body.type === option.type}>
        <input
          type="radio"
          name="body-type"
          class="sr-only"
          checked={body.type === option.type}
          onchange={() => setType(option.type)}
        />
        <span class="bitmap">{option.label}</span>
      </label>
    {/each}
  </div>

  <div class="content">
    {#if body.type === "none"}
      <p class="none">This request sends no body.</p>
    {:else if body.type === "json"}
      <div class="toolbar">
        <span class="muted">Sent as <code>application/json</code></span>
        <button class="btn quiet" onclick={beautify}>Beautify</button>
        <button class="btn quiet" onclick={minify}>Minify</button>
      </div>
      <div class="editor-box">
        <Editor bind:value={body.content} language="json" lint {known} label="JSON body" />
      </div>
    {:else if body.type === "xml"}
      <div class="toolbar"><span class="muted">Sent as <code>application/xml</code></span></div>
      <div class="editor-box">
        <Editor bind:value={body.content} language="xml" {known} label="XML body" />
      </div>
    {:else if body.type === "text"}
      <div class="toolbar">
        <label class="inline">
          <span class="muted">Content type</span>
          <input
            class="input mono"
            placeholder="text/plain"
            value={body.content_type ?? ""}
            oninput={(event) => {
              if (body.type === "text") body.content_type = event.currentTarget.value || null;
            }}
          />
        </label>
      </div>
      <div class="editor-box">
        <Editor bind:value={body.content} wrap {known} label="Text body" />
      </div>
    {:else if body.type === "form"}
      <div class="scroll">
        <KeyValueTable
          rows={body.fields}
          onChange={(fields) => {
            if (body.type === "form") body.fields = fields;
          }}
          label="Form fields"
          nameLabel="Field"
        />
      </div>
    {:else if body.type === "multipart"}
      <div class="scroll">
        <div class="multipart" role="table" aria-label="Multipart fields">
          <div class="mrow head" role="row">
            <span role="columnheader"><span class="sr-only">On</span></span>
            <span role="columnheader" class="bitmap">Field</span>
            <span role="columnheader" class="bitmap">Kind</span>
            <span role="columnheader" class="bitmap">Value or file</span>
            <span role="columnheader"><span class="sr-only">Remove</span></span>
          </div>
          {#each body.fields as field, i (i)}
            <div class="mrow" role="row" class:off={field.enabled === false}>
              <span role="cell">
                <input
                  type="checkbox"
                  class="check"
                  checked={field.enabled !== false}
                  aria-label="Send {field.name || 'this field'}"
                  onchange={() => setField(i, { enabled: field.enabled === false })}
                />
              </span>
              <span role="cell">
                <input
                  class="input mono"
                  value={field.name}
                  placeholder="name"
                  aria-label="Field name"
                  oninput={(event) => setField(i, { name: event.currentTarget.value })}
                />
              </span>
              <span role="cell">
                <select
                  class="select"
                  value={field.file !== undefined && field.file !== null ? "file" : "text"}
                  aria-label="Kind of {field.name || 'field'}"
                  onchange={(event) =>
                    setField(
                      i,
                      event.currentTarget.value === "file"
                        ? { file: "", value: "" }
                        : { file: null, value: "" },
                    )}
                >
                  <option value="text">Text</option>
                  <option value="file">File</option>
                </select>
              </span>
              <span role="cell">
                {#if field.file !== undefined && field.file !== null}
                  <input
                    class="input mono"
                    value={field.file}
                    placeholder="fixtures/photo.png (in the workspace)"
                    aria-label="File path of {field.name || 'field'}"
                    oninput={(event) => setField(i, { file: event.currentTarget.value })}
                  />
                {:else}
                  <input
                    class="input mono"
                    value={field.value ?? ""}
                    placeholder="value"
                    aria-label="Value of {field.name || 'field'}"
                    oninput={(event) => setField(i, { value: event.currentTarget.value })}
                  />
                {/if}
              </span>
              <span role="cell">
                <button
                  class="icon-btn"
                  aria-label="Remove {field.name || 'this field'}"
                  onclick={() => {
                    if (body.type === "multipart")
                      body.fields = body.fields.filter((_, j) => j !== i);
                  }}
                >
                  <Icon name="close" size={7} />
                </button>
              </span>
            </div>
          {/each}
        </div>
        <div class="add">
          <button
            class="btn"
            onclick={() => {
              if (body.type === "multipart")
                body.fields = [...body.fields, { name: "", value: "", enabled: true }];
            }}><Icon name="plus" /> Text field</button
          >
          <button
            class="btn"
            onclick={() => {
              if (body.type === "multipart")
                body.fields = [...body.fields, { name: "", file: "", enabled: true }];
            }}><Icon name="file" /> File field</button
          >
        </div>
        <p class="muted note">
          Files are read from the workspace when the request is sent; paths outside it are refused.
        </p>
      </div>
    {:else if body.type === "binary"}
      <div class="scroll binary">
        <label class="field">
          <span class="bitmap">File</span>
          <input
            class="input mono"
            placeholder="fixtures/upload.bin"
            value={body.file}
            oninput={(event) => {
              if (body.type === "binary") body.file = event.currentTarget.value;
            }}
          />
        </label>
        <label class="field">
          <span class="bitmap">Content type</span>
          <input
            class="input mono"
            placeholder="Guessed from the file name"
            value={body.content_type ?? ""}
            oninput={(event) => {
              if (body.type === "binary") body.content_type = event.currentTarget.value || null;
            }}
          />
        </label>
        <p class="muted note">
          The path is relative to the workspace root; files outside it are refused.
        </p>
      </div>
    {:else if body.type === "graphql"}
      <div class="graphql">
        <div class="part">
          <span class="bitmap label">Query</span>
          <div class="editor-box">
            <Editor
              bind:value={body.query}
              {known}
              label="GraphQL query"
              placeholder={"query {\n  me { id }\n}"}
            />
          </div>
        </div>
        <div class="part">
          <span class="bitmap label">Variables</span>
          <div class="editor-box">
            <Editor
              bind:value={
                () => (body.type === "graphql" ? (body.variables ?? "") : ""),
                (value) => {
                  if (body.type === "graphql") body.variables = value;
                }
              }
              language="json"
              lint
              {known}
              label="GraphQL variables"
              placeholder={'{ "id": 1 }'}
            />
          </div>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .body-pane {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: var(--s3);
    height: 100%;
    min-height: 0;
  }

  .types {
    display: flex;
    flex-wrap: wrap;
    gap: 0;
    border: 1px solid var(--rule);
    width: max-content;
    max-width: 100%;
  }

  .type {
    display: grid;
    place-items: center;
    height: 26px;
    padding: 0 10px;
    border-right: 1px solid var(--rule-2);
    color: var(--ink-2);
    cursor: pointer;
  }

  .type:last-child {
    border-right: 0;
  }

  .type:hover {
    background: var(--sunken);
    color: var(--ink);
  }

  .type.on {
    background: var(--ink);
    color: var(--paper);
  }

  .type:has(input:focus-visible) {
    outline: 2px dotted var(--ink);
    outline-offset: 2px;
  }

  .content {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: var(--s2);
    min-height: 0;
  }

  .content > .none,
  .content > .scroll,
  .content > .graphql {
    grid-row: 1 / -1;
  }

  .none {
    margin: 0;
    color: var(--ink-3);
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }

  .toolbar > .muted {
    margin-right: auto;
  }

  code {
    font-family: var(--font-code);
  }

  .editor-box {
    min-height: 120px;
    height: 100%;
    border: 1px solid var(--rule-2);
    overflow: hidden;
  }

  .inline {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }

  .scroll {
    overflow: auto;
    min-height: 0;
  }

  .multipart {
    display: grid;
    grid-template-columns: auto minmax(100px, 2fr) 90px minmax(160px, 4fr) 28px;
  }

  .mrow {
    display: contents;
  }

  .mrow > span {
    display: flex;
    align-items: center;
    min-width: 0;
    padding: 2px 4px 2px 0;
    border-bottom: 1px solid var(--rule-3);
  }

  .mrow.head > span {
    padding-bottom: 6px;
    color: var(--ink-3);
    border-bottom-color: var(--rule);
  }

  .mrow .input,
  .mrow .select {
    width: 100%;
  }

  .mrow.off .input {
    color: var(--ink-3);
    text-decoration: line-through;
  }

  .add {
    display: flex;
    gap: var(--s2);
    margin-top: var(--s3);
  }

  .note {
    margin: var(--s3) 0 0;
    font-size: var(--fs-meta);
  }

  .binary {
    display: grid;
    gap: var(--s3);
    align-content: start;
    max-width: 560px;
  }

  .field {
    display: grid;
    gap: 6px;
  }

  .field .bitmap,
  .label {
    color: var(--ink-3);
  }

  .graphql {
    display: grid;
    grid-template-rows: minmax(0, 2fr) minmax(0, 1fr);
    gap: var(--s3);
    min-height: 0;
  }

  .part {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 6px;
    min-height: 0;
  }
</style>
