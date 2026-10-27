<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../../lib/api";
  import { toRequest } from "../../lib/draft";
  import { asResponse, payloadSize, requestFrom } from "../../lib/example";
  import { examples, loadExamples } from "../../lib/examples.svelte";
  import { statusClass } from "../../lib/format";
  import {
    confirmDialog,
    message,
    newRequest,
    notify,
    promptDialog,
    type RequestTab,
  } from "../../lib/store.svelte";
  import type { Example, Payload } from "../../lib/types";
  import Icon from "../Icon.svelte";
  import BodyViewer from "../response/BodyViewer.svelte";

  let { tab }: { tab: RequestTab } = $props();

  const path = $derived(tab.path);
  const list = $derived(path ? (examples.byPath[path] ?? []) : []);
  let chosen = $state("");
  const current = $derived(list.find((e) => e.name === chosen) ?? list[0]);
  let body = $state<Payload | null>(null);
  let bodyError = $state("");

  onMount(() => {
    if (tab.path) void loadExamples(tab.path).catch(() => {});
  });

  // An inline body is at hand; a body file is fetched.
  $effect(() => {
    const example = current;
    const at = path;
    bodyError = "";
    if (!example || !at) {
      body = null;
      return;
    }
    if (!example.body_file) {
      body = { text: example.body ?? "" };
      return;
    }
    let live = true;
    body = null;
    api
      .exampleBody(at, example.name)
      .then((answer) => {
        if (live) body = answer.body;
      })
      .catch((error: unknown) => {
        if (live) bodyError = message(error);
      });
    return () => {
      live = false;
    };
  });

  function rename(example: Example): void {
    const at = path;
    if (!at) return;
    promptDialog({
      title: "Rename example",
      label: "Name",
      value: example.name,
      confirm: "Rename",
      onSubmit: async (name) => {
        try {
          examples.byPath[at] = await api.renameExample(at, example.name, name);
          chosen = name.trim();
        } catch (error) {
          notify(message(error), "error");
        }
      },
    });
  }

  function remove(example: Example): void {
    const at = path;
    if (!at) return;
    confirmDialog({
      title: `Delete the example ${example.name}?`,
      body: "It is removed from the request file, with its body file if it has one. Git can bring it back if it was committed.",
      confirm: "Delete",
      danger: true,
      onConfirm: async () => {
        try {
          examples.byPath[at] = await api.deleteExample(at, example.name);
        } catch (error) {
          notify(message(error), "error");
        }
      },
    });
  }

  function open(example: Example): void {
    newRequest(requestFrom(example, toRequest(tab.draft)), tab.path);
  }
</script>

<div class="examples">
  {#if !tab.path}
    <p class="muted">Save the request first. Its examples are kept in its file.</p>
  {:else if list.length === 0}
    <div class="none">
      <p>No examples yet.</p>
      <p class="muted">
        Send the request, then choose <strong>Save as example</strong> beside the response. Examples
        are kept in the request file, so they are reviewed and shared with it, and
        <code>getman mock</code> serves them.
      </p>
    </div>
  {:else}
    <ul class="list" aria-label="Examples">
      {#each list as example (example.name)}
        <li class:on={current?.name === example.name}>
          <button
            class="pick"
            aria-pressed={current?.name === example.name}
            onclick={() => (chosen = example.name)}
          >
            <span class="status bitmap status-{statusClass(example.status)}">{example.status}</span>
            <span class="name">{example.name}</span>
            {#if example.body_file}
              <span class="file mono muted" title="The body is kept in this file">
                {example.body_file}
              </span>
            {/if}
          </button>
          <button
            class="icon-btn"
            aria-label="Rename {example.name}"
            title="Rename"
            onclick={() => rename(example)}><Icon name="pencil" /></button
          >
          <button
            class="icon-btn"
            aria-label="Open the request of {example.name} in a new tab"
            title="Open as a new request"
            onclick={() => open(example)}><Icon name="copy" /></button
          >
          <button
            class="icon-btn"
            aria-label="Delete {example.name}"
            title="Delete"
            onclick={() => remove(example)}><Icon name="trash" /></button
          >
        </li>
      {/each}
    </ul>

    {#if current}
      <section class="detail" aria-label="Example {current.name}">
        {#if current.request?.url}
          <p class="sent mono">
            <strong>{current.request.method}</strong>
            {current.request.url}
          </p>
        {/if}
        {#if current.headers?.length}
          <dl class="pairs">
            {#each current.headers as header, i (i)}
              <dt class="mono">{header.name}</dt>
              <dd class="mono">{header.value}</dd>
            {/each}
          </dl>
        {/if}
        <div class="body">
          {#if body}
            <BodyViewer response={asResponse(current, body, payloadSize(body))} />
          {:else if bodyError}
            <p class="why">{bodyError}</p>
          {:else}
            <p class="muted">Loading the body…</p>
          {/if}
        </div>
      </section>
    {/if}
  {/if}
</div>

<style>
  .examples {
    display: grid;
    gap: var(--s3);
    align-content: start;
  }

  .examples p {
    margin: 0;
  }

  .none {
    display: grid;
    gap: var(--s2);
    max-width: 60ch;
  }

  code {
    font-family: var(--font-code);
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    border-top: 1px solid var(--rule-3);
  }

  .list li {
    display: flex;
    align-items: center;
    gap: 2px;
    border-bottom: 1px solid var(--rule-3);
  }

  .list li.on {
    background: var(--t1);
  }

  .pick {
    display: flex;
    align-items: center;
    gap: var(--s3);
    flex: 1;
    min-width: 0;
    height: var(--control);
    padding: 0 var(--s2);
    border: 0;
    background: none;
    color: var(--ink);
    text-align: left;
  }

  .pick:hover {
    background: var(--sunken);
  }

  .status {
    width: 3ch;
  }

  .name {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .file {
    margin-left: auto;
    font-size: var(--fs-meta);
    white-space: nowrap;
  }

  .detail {
    display: grid;
    gap: var(--s2);
  }

  .sent {
    color: var(--ink-2);
    word-break: break-all;
  }

  .pairs {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    margin: 0;
  }

  .pairs dt,
  .pairs dd {
    padding: 4px var(--s3) 4px 0;
    border-bottom: 1px solid var(--rule-3);
  }

  .pairs dt {
    color: var(--ink-2);
  }

  .pairs dd {
    margin: 0;
    word-break: break-all;
  }

  .body {
    height: 360px;
    border: 1px solid var(--rule-2);
    background: var(--field);
  }

  .body > p {
    padding: var(--s3);
  }

  .why {
    color: var(--error);
  }
</style>
