<script lang="ts">
  import type { GraphQLSchema } from "graphql";
  import {
    describeType,
    namedTypeOf,
    rootTypes,
    searchDocs,
    type DocsEntry,
  } from "../../lib/graphql";

  interface Props {
    schema: GraphQLSchema;
    /** A type to open, and a field to point at. Changes when the editor asks. */
    focus?: { type: string; field?: string; at: number } | null;
  }

  let { schema, focus = null }: Props = $props();

  /** The types opened so far, the last one shown. */
  let trail = $state<string[]>([]);
  let search = $state("");
  let field = $state("");

  $effect(() => {
    if (focus) {
      trail = [...trail.slice(-20), focus.type];
      field = focus.field ?? "";
      search = "";
    }
  });

  // A new schema may not have the types that were open.
  $effect(() => {
    trail = trail.filter((name) => schema.getType(name));
  });

  const current = $derived(trail.length ? describeType(schema, trail[trail.length - 1]!) : null);
  const matches = $derived(search.trim() ? searchDocs(schema, search) : []);
  const roots = $derived(rootTypes(schema));

  function open(name: string, to = ""): void {
    const target = namedTypeOf(name);
    if (!schema.getType(target)) return;
    trail = [...trail, target];
    field = to;
    search = "";
  }

  function back(): void {
    trail = trail.slice(0, -1);
    field = "";
  }

  const KIND: Record<string, string> = {
    object: "type",
    interface: "interface",
    union: "union",
    enum: "enum",
    input: "input",
    scalar: "scalar",
    field: "field",
  };
</script>

{#snippet typeLink(type: string)}
  {#if schema.getType(namedTypeOf(type))}
    <button class="link mono" onclick={() => open(type)}>{type}</button>
  {:else}
    <span class="mono">{type}</span>
  {/if}
{/snippet}

{#snippet entry(item: DocsEntry)}
  <li class:pointed={item.name === field} id="doc-{item.name}">
    <span class="mono name">{item.name}</span>
    {#if item.args.length}
      <span class="mono args">
        ({#each item.args as arg, i (arg.name)}{i ? ", " : ""}{arg.name}: {@render typeLink(
            arg.type,
          )}{arg.defaultValue ? ` = ${arg.defaultValue}` : ""}{/each})
      </span>
    {/if}
    <span class="mono">: </span>{@render typeLink(item.type)}
    {#if item.deprecated !== undefined}<span class="tag bitmap">Deprecated</span>{/if}
    {#if item.description}<p class="description">{item.description}</p>{/if}
    {#if item.deprecated}<p class="description">{item.deprecated}</p>{/if}
  </li>
{/snippet}

<div class="docs">
  <div class="top">
    {#if trail.length}
      <button class="btn quiet" onclick={back}>Back</button>
    {/if}
    <input
      class="input"
      type="search"
      placeholder="Search types and fields"
      aria-label="Search the schema"
      bind:value={search}
    />
  </div>

  <div class="scroll">
    {#if search.trim()}
      {#if matches.length === 0}
        <p class="muted">Nothing in the schema matches.</p>
      {:else}
        <ul class="list">
          {#each matches as match (match.label)}
            <li>
              <button
                class="link mono"
                onclick={() => open(match.type, match.label.split(".")[1] ?? "")}
              >
                {match.label}
              </button>
              <span class="tag bitmap">{KIND[match.kind]}</span>
            </li>
          {/each}
        </ul>
      {/if}
    {:else if current}
      <h3 class="title">
        <span class="mono">{current.name}</span>
        <span class="tag bitmap">{KIND[current.kind]}</span>
      </h3>
      {#if current.description}<p class="description">{current.description}</p>{/if}
      {#if current.implements.length}
        <h4 class="bitmap">Implements</h4>
        <p>
          {#each current.implements as name, i (name)}{i ? ", " : ""}{@render typeLink(name)}{/each}
        </p>
      {/if}
      {#if current.possible.length}
        <h4 class="bitmap">{current.kind === "union" ? "Members" : "Implemented by"}</h4>
        <p>
          {#each current.possible as name, i (name)}{i ? ", " : ""}{@render typeLink(name)}{/each}
        </p>
      {/if}
      {#if current.fields.length}
        <h4 class="bitmap">Fields</h4>
        <ul class="list">
          {#each current.fields as item (item.name)}{@render entry(item)}{/each}
        </ul>
      {/if}
      {#if current.inputFields.length}
        <h4 class="bitmap">Fields</h4>
        <ul class="list">
          {#each current.inputFields as item (item.name)}{@render entry(item)}{/each}
        </ul>
      {/if}
      {#if current.values.length}
        <h4 class="bitmap">Values</h4>
        <ul class="list">
          {#each current.values as value (value.name)}
            <li>
              <span class="mono name">{value.name}</span>
              {#if value.deprecated !== undefined}<span class="tag bitmap">Deprecated</span>{/if}
              {#if value.description}<p class="description">{value.description}</p>{/if}
            </li>
          {/each}
        </ul>
      {/if}
    {:else}
      <h4 class="bitmap">Roots</h4>
      <ul class="list">
        {#each roots as root (root.name)}
          <li>
            <button class="link mono" onclick={() => open(root.name)}>{root.name}</button>
            <span class="tag bitmap">{root.label}</span>
          </li>
        {/each}
      </ul>
      <h4 class="bitmap">All types</h4>
      <ul class="list">
        {#each searchDocs(schema, "", 60) as match (match.label)}
          <li>
            <button class="link mono" onclick={() => open(match.type)}>{match.label}</button>
            <span class="tag bitmap">{KIND[match.kind]}</span>
          </li>
        {/each}
      </ul>
      <p class="muted">Search to find the rest.</p>
    {/if}
  </div>
</div>

<style>
  .docs {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    min-height: 0;
    border: 1px solid var(--rule-2);
    font-size: var(--fs-meta);
  }

  .top {
    display: flex;
    gap: var(--s2);
    padding: var(--s2);
    border-bottom: 1px solid var(--rule-3);
  }

  .top .input {
    flex: 1;
    min-width: 0;
  }

  .scroll {
    padding: var(--s2) var(--s3) var(--s3);
    overflow: auto;
  }

  .list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .list li {
    padding: 3px 0;
    border-bottom: 1px solid var(--rule-3);
  }

  .list li.pointed {
    background: var(--warn-wash);
  }

  .title {
    display: flex;
    align-items: center;
    gap: var(--s2);
    margin: 0 0 var(--s1);
    font-size: var(--fs-title);
    font-weight: 600;
  }

  h4 {
    margin: var(--s3) 0 var(--s1);
    color: var(--ink-3);
  }

  p {
    margin: var(--s1) 0;
  }

  .description {
    color: var(--ink-2);
    overflow-wrap: anywhere;
  }

  .name {
    font-weight: 600;
  }

  .args {
    color: var(--ink-2);
  }

  .tag {
    padding: 1px 4px;
    border: 1px solid var(--rule-2);
    color: var(--ink-3);
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-text);
    text-align: left;
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  .link:hover {
    color: var(--ink);
  }
</style>
