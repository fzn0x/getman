<script lang="ts">
  import { api } from "../../lib/api";
  import {
    allFolders,
    closeDialog,
    message,
    openEnvironments,
    openFolder,
    openRequest,
    refreshTree,
    setExpanded,
    ws,
  } from "../../lib/store.svelte";
  import type { ImportReport } from "../../lib/types";
  import { untrack } from "svelte";
  import Icon from "../Icon.svelte";
  import Dialog from "./Dialog.svelte";

  let { folder: initialFolder }: { folder: string } = $props();

  let text = $state("");
  let fileName = $state("");
  let folder = $state(untrack(() => initialFolder));
  let root = $state(false);
  let busy = $state(false);
  let error = $state("");
  let report = $state<ImportReport | null>(null);
  let dragging = $state(false);

  const kind = $derived.by(() => {
    const trimmed = text.trimStart();
    if (trimmed.startsWith("curl")) return "A cURL command, imported as one request";
    if (!trimmed.startsWith("{")) return trimmed ? "Paste a cURL command or a Postman export" : "";
    if (/"_postman_variable_scope"\s*:\s*"globals"/.test(trimmed)) return "Postman globals";
    if (/"values"\s*:/.test(trimmed) && !/"item"\s*:/.test(trimmed)) return "A Postman environment";
    if (/"item"\s*:/.test(trimmed)) return "A Postman collection";
    return "JSON that does not look like a Postman export";
  });
  const isCollection = $derived(kind === "A Postman collection");

  async function readFile(file: File | undefined): Promise<void> {
    if (!file) return;
    fileName = file.name;
    text = await file.text();
  }

  async function run(): Promise<void> {
    busy = true;
    error = "";
    try {
      report = await api.importText(text, folder, root && isCollection);
      if (report.kind === "collection" && report.path) setExpanded(report.path, true);
      if (folder) setExpanded(folder, true);
      await refreshTree();
    } catch (e) {
      error = message(e);
    } finally {
      busy = false;
    }
  }

  function open(): void {
    if (!report) return;
    closeDialog();
    if (report.kind === "request") void openRequest(report.path);
    else if (report.kind === "environment") openEnvironments(report.path);
    else if (report.kind === "collection") void openFolder(report.path);
    else openEnvironments("__globals");
  }
</script>

<Dialog title="Import" width="640px">
  <div class="body">
    <h2 class="bitmap">Import</h2>
    {#if report}
      <div class="done">
        <p class="bitmap ok">Imported</p>
        <p>
          {#if report.kind === "request"}
            The request <strong>{report.name}</strong> as <code>{report.path}</code>.
          {:else if report.kind === "collection"}
            <strong>{report.name}</strong>: {report.requests} requests in {report.folders} folders{report.path
              ? `, in ${report.path}/`
              : ", as the workspace"}. Its scripts keep working: <code>pm</code> is the same as
            <code>gm</code>.
          {:else if report.kind === "environment"}
            The environment <strong>{report.name}</strong> as
            <code>environments/{report.path}.yaml</code>. Secret values went to its local file.
          {:else}
            The globals, into <code>getman.local.yaml</code>.
          {/if}
        </p>
        {#if report.warnings.length}
          <div class="warnings">
            <p class="bitmap">To check</p>
            <ul>
              {#each report.warnings as warning, i (i)}<li>{warning}</li>{/each}
            </ul>
          </div>
        {/if}
      </div>
    {:else}
      <p class="muted">
        Paste a cURL command, as copied from documentation or a browser's developer tools, or a
        Postman collection (v2.0 or v2.1), environment or globals export. Or drop a file here.
      </p>
      <textarea
        class="input mono"
        class:dragging
        bind:value={text}
        rows="10"
        placeholder={"curl https://api.example.com/users -H 'Accept: application/json'"}
        aria-label="Text to import"
        spellcheck="false"
        ondragover={(event) => {
          event.preventDefault();
          dragging = true;
        }}
        ondragleave={() => (dragging = false)}
        ondrop={(event) => {
          event.preventDefault();
          dragging = false;
          void readFile(event.dataTransfer?.files[0]);
        }}></textarea>
      <div class="row">
        <label class="btn">
          <Icon name="file" /> Choose a file
          <input
            type="file"
            class="sr-only"
            accept=".json,.txt,.sh,application/json,text/plain"
            onchange={(event) => void readFile(event.currentTarget.files?.[0])}
          />
        </label>
        <span class="muted">{fileName || kind}</span>
      </div>
      {#if fileName && kind}<p class="muted kind">{kind}</p>{/if}
      <div class="options">
        <label>
          <span class="bitmap">Into</span>
          <select class="select" bind:value={folder} disabled={root && isCollection}>
            <option value="">{ws.name} (top level)</option>
            {#each allFolders() as option (option.path)}
              <option value={option.path}>{option.path}</option>
            {/each}
          </select>
        </label>
        {#if isCollection}
          <label class="toggle">
            <input type="checkbox" class="check" bind:checked={root} />
            Make the collection the workspace itself
          </label>
        {/if}
      </div>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
    {/if}
  </div>
  {#snippet footer()}
    {#if report}
      <button class="btn" onclick={() => ((report = null), (text = ""), (fileName = ""))}
        >Import another</button
      >
      <button class="btn primary" onclick={open}>Open</button>
    {:else}
      <button class="btn" onclick={() => closeDialog()}>Cancel</button>
      <button class="btn primary" onclick={run} disabled={busy || text.trim() === ""}
        >{busy ? "Importing…" : "Import"}</button
      >
    {/if}
  {/snippet}
</Dialog>

<style>
  .body {
    display: grid;
    gap: var(--s3);
    padding: var(--s4);
    overflow: auto;
  }

  h2 {
    margin: 0;
    font-size: var(--b2);
    font-weight: 400;
  }

  p {
    margin: 0;
  }

  textarea {
    width: 100%;
    resize: vertical;
    min-height: 160px;
  }

  textarea.dragging {
    border: 2px dashed var(--accent);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--s3);
    min-width: 0;
  }

  .row .muted {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .options {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: var(--s4);
  }

  .options label:not(.toggle) {
    display: grid;
    gap: 6px;
  }

  .options .bitmap {
    color: var(--ink-3);
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: var(--s2);
    height: var(--control);
  }

  .error {
    color: var(--error);
  }

  code {
    font-family: var(--font-code);
  }

  .ok {
    color: var(--ok);
    font-size: var(--b2);
  }

  .done {
    display: grid;
    gap: var(--s3);
  }

  .warnings {
    padding: var(--s3);
    background: var(--warn-wash);
  }

  .warnings ul {
    margin: var(--s2) 0 0;
    padding-left: 1.2em;
  }
</style>
