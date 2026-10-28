<script lang="ts">
  import type { GraphQLSchema } from "graphql";
  import { api } from "../../lib/api";
  import { logSchema } from "../../lib/console.svelte";
  import { toRequest } from "../../lib/draft";
  import { graphqlExtensions, operationNames, schemaFrom, typeCount } from "../../lib/graphql";
  import { contextPath, message, ws, type RequestTab } from "../../lib/store.svelte";
  import type { Body, GraphqlSchemaAnswer } from "../../lib/types";
  import Editor from "../Editor.svelte";
  import GraphqlDocs from "./GraphqlDocs.svelte";

  type GraphqlBody = Extract<Body, { type: "graphql" }>;

  interface Props {
    body: GraphqlBody;
    tab?: RequestTab;
    known: Set<string>;
  }

  let { body = $bindable(), tab, known }: Props = $props();

  let schema = $state.raw<GraphQLSchema | null>(null);
  let info = $state<{ types: number; at: number; fromMemory: boolean } | null>(null);
  let loading = $state(false);
  let error = $state("");
  let showDocs = $state(false);
  let focus = $state<{ type: string; field?: string; at: number } | null>(null);

  const operations = $derived(operationNames(body.query));
  const extensions = $derived(
    graphqlExtensions(schema, {
      onShowInDocs: (field, type, parentType) => {
        const target = parentType ?? type;
        if (!target) return;
        focus = { type: target, field: parentType ? field : undefined, at: Date.now() };
        showDocs = true;
      },
    }),
  );

  function args(mode: "cached" | "fetch" | "refresh") {
    return {
      path: tab ? contextPath(tab) : null,
      request: toRequest(tab!.draft),
      environment: ws.environment || null,
      mode,
    };
  }

  function show(answer: GraphqlSchemaAnswer): void {
    try {
      const built = schemaFrom(answer.schema);
      schema = built;
      info = {
        types: typeCount(built),
        at: (answer.fetched_at ?? 0) * 1000,
        fromMemory: answer.cached === true,
      };
      error = "";
    } catch (e) {
      schema = null;
      info = null;
      error = `The schema could not be read: ${message(e)}`;
    }
  }

  // Look for a schema kept in memory for this URL when the pane opens and
  // when the URL changes. That sends nothing.
  let looked = 0;
  $effect(() => {
    if (!tab) return;
    void tab.draft.url;
    void ws.environment;
    const turn = ++looked;
    const wait = setTimeout(() => {
      api
        .graphqlSchema(args("cached"))
        .then((answer) => {
          if (turn !== looked || loading) return;
          if (answer.schema) show(answer);
          else {
            schema = null;
            info = null;
          }
        })
        .catch(() => {});
    }, 400);
    return () => clearTimeout(wait);
  });

  async function load(refresh: boolean): Promise<void> {
    if (!tab || loading) return;
    loading = true;
    error = "";
    try {
      const answer = await api.graphqlSchema(args(refresh ? "refresh" : "fetch"));
      show(answer);
      logSchema({
        name: tab.draft.name || "GraphQL schema",
        url: answer.url,
        status: answer.status ?? null,
        timeMs: answer.cached ? null : (answer.time_ms ?? null),
        cached: answer.cached,
      });
    } catch (e) {
      error = message(e);
      logSchema({
        name: tab.draft.name || "GraphQL schema",
        url: tab.draft.url,
        status: null,
        timeMs: null,
        error: error,
      });
    } finally {
      loading = false;
    }
  }

  function clock(at: number): string {
    return at ? new Date(at).toLocaleTimeString([], { hour12: false }) : "";
  }
</script>

<div class="graphql" class:docked={showDocs && schema}>
  <div class="bar">
    <label class="inline">
      <span class="muted">Operation</span>
      <input
        class="input mono"
        list="graphql-operations"
        placeholder={operations.length > 1 ? "Name the one to run" : "Optional"}
        aria-label="Operation name"
        value={body.operation_name ?? ""}
        oninput={(event) => (body.operation_name = event.currentTarget.value)}
      />
      <datalist id="graphql-operations">
        {#each operations as name (name)}<option value={name}></option>{/each}
      </datalist>
    </label>
    {#if operations.length > 1 && !(body.operation_name ?? "").trim()}
      <span class="note">The query has {operations.length} operations; name the one to run.</span>
    {/if}
    <span class="status muted" aria-live="polite">
      {#if loading}
        Reading the schema…
      {:else if info}
        Schema: {info.types} types{info.at ? `, read at ${clock(info.at)}` : ""}
      {:else}
        No schema loaded
      {/if}
    </span>
    {#if tab}
      <button
        class="btn quiet"
        disabled={loading}
        title={schema
          ? "Ask the server for its schema again"
          : "Ask the server for its schema, for completion, checks and docs"}
        onclick={() => void load(!!schema)}
      >
        {schema ? "Refresh" : "Load schema"}
      </button>
    {/if}
    {#if schema}
      <button class="btn quiet" aria-pressed={showDocs} onclick={() => (showDocs = !showDocs)}>
        Docs
      </button>
    {/if}
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}

  <div class="editors">
    <div class="part">
      <span class="bitmap label">Query</span>
      <div class="editor-box">
        <Editor
          bind:value={body.query}
          {known}
          {extensions}
          languageCompletion
          label="GraphQL query"
          placeholder={"query {\n  me { id }\n}"}
        />
      </div>
    </div>
    <div class="part">
      <span class="bitmap label">Variables</span>
      <div class="editor-box">
        <Editor
          bind:value={() => body.variables ?? "", (value) => (body.variables = value)}
          language="json"
          lint
          {known}
          label="GraphQL variables"
          placeholder={'{ "id": 1 }'}
        />
      </div>
    </div>
  </div>

  {#if showDocs && schema}
    <div class="docs">
      <GraphqlDocs {schema} {focus} />
    </div>
  {/if}
</div>

<style>
  .graphql {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: auto auto minmax(0, 1fr);
    gap: var(--s2) var(--s3);
    grid-row: 1 / -1;
    height: 100%;
    min-height: 0;
  }

  .graphql.docked {
    grid-template-columns: minmax(0, 1fr) minmax(240px, 38%);
  }

  .bar,
  .error {
    grid-column: 1 / -1;
  }

  .bar {
    grid-row: 1;
  }

  .error {
    grid-row: 2;
  }

  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--s2) var(--s3);
  }

  .inline {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }

  .inline .input {
    width: 180px;
  }

  .note {
    color: var(--warn);
    font-size: var(--fs-meta);
  }

  .status {
    margin-left: auto;
    font-size: var(--fs-meta);
  }

  .error {
    margin: 0;
    color: var(--error);
    font-size: var(--fs-meta);
  }

  .editors {
    grid-row: 3;
    grid-column: 1;
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

  .label {
    color: var(--ink-3);
  }

  .editor-box {
    min-height: 120px;
    height: 100%;
    border: 1px solid var(--rule-2);
    overflow: hidden;
  }

  .docs {
    grid-row: 3;
    grid-column: 2;
    display: grid;
    min-height: 0;
  }

  @media (max-width: 760px) {
    .graphql.docked {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto auto minmax(0, 1fr) minmax(0, 1fr);
    }

    .docs {
      grid-row: 4;
      grid-column: 1;
    }
  }
</style>
