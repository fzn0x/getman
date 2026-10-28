<script lang="ts">
  import { api } from "../../lib/api";
  import { copyText } from "../../lib/clipboard";
  import { toRequest } from "../../lib/draft";
  import type { EditorLanguage } from "../../lib/editor";
  import { contextPath, message, store, ws, type RequestTab } from "../../lib/store.svelte";
  import Editor from "../Editor.svelte";
  import Icon from "../Icon.svelte";
  import Dialog from "./Dialog.svelte";

  let { tab }: { tab: RequestTab } = $props();

  let language = $state(
    (() => {
      try {
        return JSON.parse(localStorage.getItem("getman.codeLanguage") ?? '"curl"') as string;
      } catch {
        return "curl";
      }
    })(),
  );
  /** `""` keeps each language's own indent. */
  type Indent = "" | "2" | "4" | "tab";
  const indents: [Indent, string][] = [
    ["", "Default"],
    ["2", "2 spaces"],
    ["4", "4 spaces"],
    ["tab", "Tab"],
  ];
  let indent = $state<Indent>(
    (() => {
      try {
        const saved = JSON.parse(localStorage.getItem("getman.codeIndent") ?? '""');
        return saved === "2" || saved === "4" || saved === "tab" ? (saved as Indent) : "";
      } catch {
        return "";
      }
    })(),
  );
  /** Whether to write the imports and `main` function, where there are some. */
  let boilerplate = $state(
    (() => {
      try {
        return JSON.parse(localStorage.getItem("getman.codeBoilerplate") ?? "true") !== false;
      } catch {
        return true;
      }
    })(),
  );
  let code = $state("");
  let error = $state("");
  let missing = $state<string[]>([]);
  /** Leaves `{{variables}}` as written, so tokens stay out of the code. */
  let keepVars = $state(false);

  // gRPC requests have one form outside Getman: a grpcurl command.
  const grpc = $derived(tab.draft.kind === "grpc");
  const languages = $derived(
    grpc ? [{ id: "grpcurl", label: "grpcurl", syntax: "shell" }] : ws.languages,
  );

  const current = $derived(ws.languages.find((l) => l.id === language));

  const syntax = $derived<EditorLanguage>(
    ws.languages.find((l) => l.id === language)?.syntax === "javascript" ? "javascript" : "text",
  );

  $effect(() => {
    const chosen = grpc ? "grpcurl" : language;
    const keep = keepVars;
    const spaces = indent;
    const bare = !boilerplate;
    if (!grpc) store("codeLanguage", chosen);
    store("codeIndent", spaces);
    store("codeBoilerplate", !bare);
    void (async () => {
      try {
        const args = {
          path: contextPath(tab),
          request: toRequest(tab.draft),
          environment: ws.environment || null,
        };
        const result = grpc
          ? await api.grpcCode(args)
          : await api.codegen({
              ...args,
              language: chosen,
              keep_vars: keep,
              indent: spaces || null,
              no_boilerplate: bare,
            });
        code = result.code;
        missing = result.missing;
        error = "";
      } catch (e) {
        error = message(e);
        code = "";
      }
    })();
  });
</script>

<Dialog title="Copy as code" width="860px">
  <div class="code-dialog">
    <nav class="languages" aria-label="Language">
      <h2 class="bitmap">Code</h2>
      {#each languages as option (option.id)}
        <button
          class:on={grpc || option.id === language}
          aria-pressed={grpc || option.id === language}
          onclick={() => (language = option.id)}
        >
          {option.label}
        </button>
      {/each}
    </nav>
    <div class="output">
      {#if error}
        <p class="error">{error}</p>
      {:else}
        <div class="editor">
          <Editor
            value={code}
            readonly
            language={syntax}
            lineNumbers={false}
            label="Generated code"
          />
        </div>
      {/if}
      {#if !grpc && (current?.indent || current?.boilerplate)}
        <div class="options">
          {#if current?.indent}
            <div class="group" role="group" aria-label="Indent">
              <span class="muted">Indent</span>
              {#each indents as [value, label] (value)}
                <button
                  class="chip"
                  class:on={indent === value}
                  aria-pressed={indent === value}
                  onclick={() => (indent = value)}
                >
                  {label}
                </button>
              {/each}
            </div>
          {/if}
          {#if current?.boilerplate}
            <label class="toggle">
              <input type="checkbox" class="check" bind:checked={boilerplate} />
              Imports and <code>main</code>
            </label>
          {/if}
        </div>
      {/if}
      <div class="note">
        <label class="toggle">
          <input type="checkbox" class="check" bind:checked={keepVars} />
          Keep <code>{"{{variables}}"}</code>
        </label>
        <p class="muted">
          {#if keepVars}
            Variables stay as written, so tokens and other values are not in the code.
          {:else}
            Variables are filled in{ws.environment ? " from the selected environment" : ""}.
            {#if missing.length}Not defined, left as written: {missing
                .map((m) => `{{${m}}}`)
                .join(", ")}.{/if}
            Credentials appear as they will be sent.
          {/if}
        </p>
      </div>
    </div>
  </div>
  {#snippet footer()}
    <button class="btn primary" onclick={() => void copyText(code, "Code copied")} disabled={!code}>
      <Icon name="copy" /> Copy
    </button>
  {/snippet}
</Dialog>

<style>
  .code-dialog {
    display: grid;
    grid-template-columns: 200px minmax(0, 1fr);
    min-height: 420px;
    max-height: 70vh;
  }

  .languages {
    display: grid;
    align-content: start;
    gap: 1px;
    padding: var(--s4) var(--s2);
    border-right: 1px solid var(--rule-2);
    overflow: auto;
  }

  h2 {
    margin: 0 var(--s2) var(--s3);
    font-size: var(--b2);
    font-weight: 400;
  }

  .languages button {
    padding: 6px var(--s2);
    border: 0;
    background: none;
    text-align: left;
    color: var(--ink-2);
  }

  .languages button:hover {
    background: var(--sunken);
    color: var(--ink);
  }

  .languages button.on {
    background: var(--ink);
    color: var(--paper);
  }

  .output {
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto auto;
    min-height: 0;
  }

  .options {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--s2) var(--s4);
    padding: var(--s2) var(--s4);
    border-top: 1px solid var(--rule-3);
    font-size: var(--fs-meta);
  }

  .group {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--s1);
  }

  .group .muted {
    margin-right: var(--s1);
  }

  .chip {
    padding: 2px var(--s2);
    border: 1px solid var(--rule-2);
    background: none;
    color: var(--ink-2);
  }

  .chip:hover {
    background: var(--sunken);
    color: var(--ink);
  }

  .chip.on {
    border-color: var(--ink);
    background: var(--ink);
    color: var(--paper);
  }

  .editor {
    min-height: 0;
    overflow: hidden;
  }

  .note {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--s2) var(--s4);
    margin: 0;
    padding: var(--s2) var(--s4);
    border-top: 1px solid var(--rule-3);
    font-size: var(--fs-meta);
  }

  .note p {
    flex: 1 1 240px;
    margin: 0;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: var(--s2);
    white-space: nowrap;
  }

  .toggle code {
    font-family: var(--font-code);
  }

  .error {
    margin: var(--s4);
    color: var(--error);
  }

  @media (max-width: 640px) {
    .code-dialog {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto minmax(0, 1fr);
    }

    .languages {
      grid-auto-flow: column;
      grid-auto-columns: max-content;
      border-right: 0;
      border-bottom: 1px solid var(--rule-2);
    }

    h2 {
      display: none;
    }
  }
</style>
