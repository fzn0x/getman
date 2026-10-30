<script lang="ts">
  import { api } from "../../lib/api";
  import {
    describeFiles,
    describeText,
    isCollection as canBeWorkspace,
    readDropped,
    readFileList,
    type ImportFile,
  } from "../../lib/importing";
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
  import type { FilesImport, ImportReport } from "../../lib/types";
  import { untrack } from "svelte";
  import Icon from "../Icon.svelte";
  import Dialog from "./Dialog.svelte";

  let { folder: initialFolder, files: initialFiles }: { folder: string; files?: ImportFile[] } =
    $props();

  let text = $state("");
  let files = $state<ImportFile[]>(untrack(() => initialFiles ?? []));
  let folder = $state(untrack(() => initialFolder));
  let root = $state(false);
  let busy = $state(false);
  let error = $state("");
  let reports = $state<ImportReport[] | null>(null);
  /** Files of several that were not imported. */
  let passedOver = $state<string[]>([]);
  let failed = $state<FilesImport["failed"]>([]);
  let dragging = $state(false);

  const kind = $derived(files.length ? describeFiles(files) : describeText(text));
  const fileLabel = $derived(
    files.length === 1 ? (files[0]?.path ?? "") : files.length > 1 ? `${files.length} files` : "",
  );
  const isCollection = $derived(canBeWorkspace(kind));
  const ready = $derived(files.length > 0 || text.trim() !== "");

  async function choose(list: FileList | null | undefined): Promise<void> {
    if (!list?.length) return;
    files = await readFileList(list);
    error = "";
  }

  async function drop(data: DataTransfer | null): Promise<void> {
    if (!data) return;
    const dropped = await readDropped(data);
    if (dropped.length) {
      files = dropped;
      error = "";
    }
  }

  async function run(): Promise<void> {
    busy = true;
    error = "";
    const intoRoot = root && isCollection;
    try {
      if (files.length > 1) {
        const done = await api.importFiles(files, folder, intoRoot);
        reports = done.reports;
        passedOver = done.passed_over;
        failed = done.failed;
      } else if (files[0])
        reports = [await api.importText(files[0].text, folder, intoRoot, files[0].path)];
      else reports = [await api.importText(text, folder, intoRoot)];
      for (const report of reports)
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
    const report = reports?.[0];
    if (!report) return;
    closeDialog();
    if (report.kind === "request") void openRequest(report.path);
    else if (report.kind === "environment") openEnvironments(report.path);
    else if (report.kind === "collection") void openFolder(report.path);
    else openEnvironments("__globals");
  }

  function reset(): void {
    reports = null;
    passedOver = [];
    failed = [];
    text = "";
    files = [];
  }

  const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
</script>

<Dialog title="Import" width="640px">
  <div class="body">
    <h2 class="bitmap">Import</h2>
    {#if reports}
      <div class="done">
        <p class="bitmap ok">Imported</p>
        {#each reports as report, r (r)}
          <p>
            {#if report.kind === "request"}
              The request <strong>{report.name}</strong> as <code>{report.path}</code>.
            {:else if report.kind === "collection"}
              <strong>{report.name}</strong>{report.format ? ` (${report.format})` : ""}: {plural(
                report.requests,
                "request",
                "requests",
              )} in {plural(report.folders, "folder", "folders")}{report.path
                ? `, in ${report.path}/`
                : ", as the workspace"}{report.environments?.length
                ? `, and ${plural(report.environments.length, "environment", "environments")}`
                : ""}.
            {:else if report.kind === "environment"}
              {#if (report.environments?.length ?? 0) > 1}
                The environments <strong>{report.name}</strong>.
              {:else}
                The environment <strong>{report.name}</strong> as
                <code>environments/{report.path}.yaml</code>.
              {/if}
              Secret values went to the local files.
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
        {/each}
        {#if failed.length}
          <div class="failed" role="alert">
            <p class="bitmap">Not imported</p>
            <ul>
              {#each failed as file (file.path)}
                <li><code>{file.path}</code>: {file.error}</li>
              {/each}
            </ul>
          </div>
        {/if}
        {#if passedOver.length}
          <p class="muted">
            Passed over {plural(passedOver.length, "file", "files")} in no format Getman imports:
            {passedOver.slice(0, 8).join(", ")}{passedOver.length > 8
              ? ` and ${passedOver.length - 8} more`
              : ""}.
          </p>
        {/if}
      </div>
    {:else}
      <p class="muted">
        Paste a cURL command or an export, or drop files or a folder here: Postman, Insomnia,
        Hoppscotch and Bruno collections and environments, OpenAPI and Swagger documents, and HAR
        files from a browser.
      </p>
      {#if files.length}
        <div class="files" class:dragging>
          <p><Icon name="file" /> <strong>{fileLabel}</strong></p>
          {#if files.length > 1}
            <ul>
              {#each files.slice(0, 8) as file (file.path)}<li><code>{file.path}</code></li>{/each}
              {#if files.length > 8}<li class="muted">and {files.length - 8} more</li>{/if}
            </ul>
          {/if}
          <button class="btn quiet" onclick={() => (files = [])}>Clear</button>
        </div>
      {:else}
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
            event.stopPropagation();
            dragging = false;
            void drop(event.dataTransfer);
          }}></textarea>
      {/if}
      <div class="row">
        <label class="btn">
          <Icon name="file" /> Choose files
          <input
            type="file"
            class="sr-only"
            multiple
            accept=".json,.yaml,.yml,.har,.bru,.txt,.sh,application/json,text/plain"
            onchange={(event) => void choose(event.currentTarget.files)}
          />
        </label>
        <label class="btn">
          <Icon name="folder" /> Choose a folder
          <input
            type="file"
            class="sr-only"
            webkitdirectory
            onchange={(event) => void choose(event.currentTarget.files)}
          />
        </label>
        <span class="muted">{kind}</span>
      </div>
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
    {#if reports}
      <button class="btn" onclick={reset}>Import another</button>
      <button class="btn primary" onclick={open}>Open</button>
    {:else}
      <button class="btn" onclick={() => closeDialog()}>Cancel</button>
      <button class="btn primary" onclick={run} disabled={busy || !ready}
        >{busy ? "Importing…" : "Import"}</button
      >
    {/if}
  {/snippet}
</Dialog>

<style>
  .body {
    display: grid;
    /* A long file name or path wraps instead of widening every row. */
    grid-template-columns: minmax(0, 1fr);
    gap: var(--s3);
    padding: var(--s4);
    overflow: auto;
    overflow-wrap: anywhere;
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

  .files {
    display: grid;
    gap: var(--s2);
    justify-items: start;
    padding: var(--s3);
    border: 1px solid var(--rule-2);
    background: var(--field);
  }

  .files ul {
    margin: 0;
    padding-left: 1.2em;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
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

  .warnings ul,
  .failed ul {
    margin: var(--s2) 0 0;
    padding-left: 1.2em;
  }

  .failed {
    padding: var(--s3);
    border-left: 2px solid var(--error);
    background: var(--field);
  }

  .failed .bitmap {
    color: var(--error);
  }
</style>
