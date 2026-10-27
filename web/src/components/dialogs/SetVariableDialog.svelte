<script lang="ts">
  import { untrack } from "svelte";
  import { api } from "../../lib/api";
  import {
    closeDialog,
    message,
    notify,
    refreshVariables,
    ws,
    type RequestTab,
  } from "../../lib/store.svelte";
  import Dialog from "./Dialog.svelte";

  interface Props {
    value: string;
    name: string;
    tab: RequestTab | null;
  }

  let props: Props = $props();

  type Scope = "request" | "environment" | "workspace" | "global";

  const tab = untrack(() => props.tab);
  const environment = ws.environments.find((e) => e.id === ws.environment) ?? null;
  let name = $state(untrack(() => props.name));
  let value = $state(untrack(() => props.value));
  let scope = $state<Scope>(environment ? "environment" : tab ? "request" : "workspace");

  const valid = $derived(name.trim() !== "" && !/[{}"\n]/.test(name));

  const scopes = $derived(
    [
      {
        id: "request" as Scope,
        label: "This request",
        note: "An unsaved change to the request's variables; save the request to keep it.",
        off: !tab,
      },
      {
        id: "environment" as Scope,
        label: environment ? `Environment: ${environment.name}` : "Environment",
        note: environment
          ? `In environments/${environment.id}.local.yaml, on this machine only.`
          : "Choose an environment first.",
        off: !environment,
      },
      {
        id: "workspace" as Scope,
        label: "Workspace",
        note: "In getman.local.yaml, on this machine only.",
        off: false,
      },
      {
        id: "global" as Scope,
        label: "Globals",
        note: "In getman.local.yaml; the weakest scope, for every request.",
        off: false,
      },
    ].filter((option) => !option.off || option.id === "environment"),
  );

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (!valid) return;
    const chosen = { name: name.trim(), value, scope };
    closeDialog();
    try {
      if (chosen.scope === "request") {
        if (!tab) return;
        tab.draft.variables = { ...tab.draft.variables, [chosen.name]: chosen.value };
        notify(`Set {{${chosen.name}}} in this request`, "ok");
      } else {
        await api.setVariable({
          scope: chosen.scope,
          name: chosen.name,
          value: chosen.value,
          environment: environment?.id ?? null,
        });
        notify(`Saved {{${chosen.name}}}`, "ok");
      }
      void refreshVariables();
    } catch (error) {
      notify(message(error), "error");
    }
  }
</script>

<Dialog title="Set as variable" width="520px">
  <form class="form" id="set-variable" onsubmit={submit}>
    <h2 class="bitmap">Set as variable</h2>
    <label>
      <span class="bitmap">Name</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="input mono"
        bind:value={name}
        autofocus
        spellcheck="false"
        placeholder="token"
        onfocus={(event) => event.currentTarget.select()}
      />
    </label>
    <label>
      <span class="bitmap">Value</span>
      <textarea class="input mono" bind:value rows="3" spellcheck="false"></textarea>
    </label>
    <fieldset>
      <legend class="bitmap">Save in</legend>
      {#each scopes as option (option.id)}
        <label class="scope" class:off={option.off}>
          <input
            type="radio"
            class="check"
            name="scope"
            value={option.id}
            bind:group={scope}
            disabled={option.off}
          />
          <span>
            <span class="label">{option.label}</span>
            <span class="note">{option.note}</span>
          </span>
        </label>
      {/each}
    </fieldset>
  </form>
  {#snippet footer()}
    <button class="btn" type="button" onclick={() => closeDialog()}>Cancel</button>
    <button class="btn primary" type="submit" form="set-variable" disabled={!valid}>Save</button>
  {/snippet}
</Dialog>

<style>
  .form {
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

  label {
    display: grid;
    gap: 6px;
  }

  label > .bitmap,
  legend {
    color: var(--ink-3);
  }

  textarea {
    resize: vertical;
  }

  fieldset {
    display: grid;
    gap: var(--s2);
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
    padding: 0 0 6px;
  }

  .scope {
    display: grid;
    grid-template-columns: 14px minmax(0, 1fr);
    align-items: start;
    gap: var(--s2);
    cursor: pointer;
  }

  .scope .check {
    margin-top: 2px;
    border-radius: 0;
  }

  .scope.off {
    color: var(--ink-3);
    cursor: not-allowed;
  }

  .scope > span {
    display: grid;
  }

  .note {
    color: var(--ink-3);
    font-size: var(--fs-meta);
  }
</style>
