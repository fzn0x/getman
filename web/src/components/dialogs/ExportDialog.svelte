<script lang="ts">
  import { api } from "../../lib/api";
  import { download } from "../../lib/importing";
  import { closeDialog, message, notify } from "../../lib/store.svelte";
  import Icon from "../Icon.svelte";
  import Dialog from "./Dialog.svelte";

  let {
    target,
    path,
    name,
  }: { target: "folder" | "environment" | "globals"; path: string; name: string } = $props();

  type Format = "postman" | "openapi" | "openapi-yaml";
  let format = $state<Format>("postman");
  let secrets = $state(false);
  let busy = $state(false);
  let error = $state("");
  let warnings = $state<string[]>([]);

  const formats: { id: Format; label: string; note: string }[] = [
    {
      id: "postman",
      label: "Postman collection v2.1",
      note: "Folders, requests, auth and scripts. Checks become pm.test lines.",
    },
    {
      id: "openapi",
      label: "OpenAPI 3.1 (JSON)",
      note: "Paths, parameters and example bodies, inferred from the requests.",
    },
    { id: "openapi-yaml", label: "OpenAPI 3.1 (YAML)", note: "The same document as YAML." },
  ];

  async function run(): Promise<void> {
    busy = true;
    error = "";
    try {
      const exported = await api.exportItem({
        kind: target,
        path,
        format: target === "folder" ? format : "postman",
        secrets,
      });
      download(
        exported.file_name,
        exported.content,
        exported.file_name.endsWith(".yaml") ? "application/yaml" : "application/json",
      );
      warnings = exported.warnings;
      if (!warnings.length) {
        notify(`Saved ${exported.file_name}`, "ok");
        closeDialog();
      }
    } catch (e) {
      error = message(e);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog title="Export" width="560px">
  <div class="body">
    <h2 class="bitmap">Export</h2>
    {#if warnings.length}
      <p>The file was saved. Some things could not be carried over:</p>
      <div class="warnings">
        <ul>
          {#each warnings as warning, i (i)}<li>{warning}</li>{/each}
        </ul>
      </div>
    {:else}
      <p>
        {#if target === "folder"}
          <strong>{name}</strong>{path ? ` (${path}/)` : ", the whole workspace"}, as a file other
          tools can read.
        {:else if target === "environment"}
          The environment <strong>{name}</strong> as a Postman environment file.
        {:else}
          The globals as a Postman globals file.
        {/if}
      </p>
      {#if target === "folder"}
        <fieldset>
          <legend class="bitmap">Format</legend>
          {#each formats as option (option.id)}
            <label class="choice">
              <input type="radio" name="format" value={option.id} bind:group={format} />
              <span>
                <strong>{option.label}</strong>
                <span class="muted">{option.note}</span>
              </span>
            </label>
          {/each}
        </fieldset>
      {:else}
        <label class="choice">
          <input type="checkbox" class="check" bind:checked={secrets} />
          <span>
            <strong>Include local values</strong>
            <span class="muted"
              >{target === "globals"
                ? "Globals live in getman.local.yaml and may hold tokens."
                : "Secrets and other values from the git-ignored local file."} Off by default.</span
            >
          </span>
        </label>
      {/if}
      {#if error}<p class="error" role="alert">{error}</p>{/if}
    {/if}
  </div>
  {#snippet footer()}
    {#if warnings.length}
      <button class="btn primary" onclick={() => closeDialog()}>Done</button>
    {:else}
      <button class="btn" onclick={() => closeDialog()}>Cancel</button>
      <button class="btn primary" onclick={run} disabled={busy}>
        <Icon name="export" />
        {busy ? "Exporting…" : "Download"}
      </button>
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

  fieldset {
    display: grid;
    gap: var(--s2);
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
    margin-bottom: var(--s2);
    color: var(--ink-3);
  }

  .choice {
    display: flex;
    align-items: flex-start;
    gap: var(--s2);
  }

  .choice > span {
    display: grid;
    gap: 2px;
  }

  .choice .muted {
    font-size: var(--fs-meta);
  }

  .error {
    color: var(--error);
  }

  .warnings {
    padding: var(--s3);
    background: var(--warn-wash);
  }

  .warnings ul {
    margin: 0;
    padding-left: 1.2em;
  }
</style>
