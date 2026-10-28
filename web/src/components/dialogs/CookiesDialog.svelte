<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../../lib/api";
  import { cookieForm, emptyCookieForm, formCookie, type CookieForm } from "../../lib/cookies";
  import { message, notify } from "../../lib/store.svelte";
  import type { Cookie } from "../../lib/types";
  import Icon from "../Icon.svelte";
  import Dialog from "./Dialog.svelte";

  let cookies = $state<Cookie[]>([]);
  let loaded = $state(false);
  /** The cookie being added or edited, and which one it was. */
  let editing = $state<{ form: CookieForm; previous: Cookie | null } | null>(null);
  let problem = $state("");
  let nameInput = $state<HTMLInputElement>();

  async function load(): Promise<void> {
    try {
      cookies = await api.cookies();
    } catch (error) {
      notify(message(error), "error");
    } finally {
      loaded = true;
    }
  }

  onMount(() => void load());

  async function remove(cookie: Cookie): Promise<void> {
    await api.deleteCookie(cookie).catch(() => {});
    if (editing?.previous === cookie) editing = null;
    await load();
  }

  async function clear(): Promise<void> {
    await api.clearCookies().catch(() => {});
    editing = null;
    await load();
  }

  function edit(cookie: Cookie | null): void {
    problem = "";
    editing = {
      form: cookie ? cookieForm(cookie) : emptyCookieForm(cookies.at(-1)?.domain ?? ""),
      previous: cookie,
    };
    queueMicrotask(() => nameInput?.focus());
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (!editing) return;
    const cookie = formCookie(editing.form);
    if (typeof cookie === "string") {
      problem = cookie;
      return;
    }
    try {
      const previous = editing.previous;
      await api.setCookie(
        cookie,
        previous
          ? { domain: previous.domain, path: previous.path, name: previous.name }
          : undefined,
      );
      editing = null;
      await load();
    } catch (error) {
      problem = message(error);
    }
  }
</script>

<Dialog title="Cookies" width="760px">
  <div class="body">
    <h2 class="bitmap">Cookies</h2>
    <p class="muted">
      Cookies servers set are sent back like a browser would. Getman keeps them in the workspace's
      git-ignored <code>.getman/cookies.json</code>, so they last until they expire.
    </p>
    {#if loaded && cookies.length === 0}
      <p class="none">No cookies yet.</p>
    {:else}
      <table>
        <thead>
          <tr>
            <th class="bitmap">Domain</th>
            <th class="bitmap">Name</th>
            <th class="bitmap">Value</th>
            <th class="bitmap">Expires</th>
            <th><span class="sr-only">Edit or remove</span></th>
          </tr>
        </thead>
        <tbody>
          {#each cookies as cookie (`${cookie.domain}${cookie.path}${cookie.name}`)}
            <tr class:current={editing?.previous === cookie}>
              <td class="mono"
                >{cookie.host_only ? "" : "."}{cookie.domain}{cookie.path === "/"
                  ? ""
                  : cookie.path}</td
              >
              <td class="mono">{cookie.name}</td>
              <td class="mono value">{cookie.value}</td>
              <td class="muted"
                >{cookie.expires ? new Date(cookie.expires).toLocaleString() : "session"}</td
              >
              <td class="actions">
                <button
                  class="icon-btn"
                  aria-label="Edit {cookie.name}"
                  onclick={() => edit(cookie)}
                >
                  <Icon name="pencil" size={7} />
                </button>
                <button
                  class="icon-btn"
                  aria-label="Remove {cookie.name}"
                  onclick={() => void remove(cookie)}
                >
                  <Icon name="close" size={7} />
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}

    {#if editing}
      <form class="editor" onsubmit={submit}>
        <h3 class="bitmap">{editing.previous ? "Edit cookie" : "Add a cookie"}</h3>
        <div class="grid">
          <label class="field">
            <span class="bitmap">Name</span>
            <input
              class="input mono"
              bind:this={nameInput}
              bind:value={editing.form.name}
              spellcheck="false"
            />
          </label>
          <label class="field">
            <span class="bitmap">Value</span>
            <input class="input mono" bind:value={editing.form.value} spellcheck="false" />
          </label>
          <label class="field">
            <span class="bitmap">Domain</span>
            <input
              class="input mono"
              bind:value={editing.form.domain}
              placeholder="api.example.com"
              spellcheck="false"
            />
          </label>
          <label class="field">
            <span class="bitmap">Path</span>
            <input class="input mono" bind:value={editing.form.path} spellcheck="false" />
          </label>
          <label class="field">
            <span class="bitmap">Expires</span>
            <input class="input" type="datetime-local" bind:value={editing.form.expires} />
          </label>
        </div>
        <div class="flags">
          <label
            ><input type="checkbox" class="check" bind:checked={editing.form.subdomains} /> Subdomains
            too</label
          >
          <label
            ><input type="checkbox" class="check" bind:checked={editing.form.secure} /> HTTPS only</label
          >
          <label
            ><input type="checkbox" class="check" bind:checked={editing.form.httpOnly} /> HttpOnly</label
          >
        </div>
        <p class="muted">Leave Expires empty for a session cookie.</p>
        {#if problem}<p class="problem" role="alert">{problem}</p>{/if}
        <div class="buttons">
          <button type="button" class="btn quiet" onclick={() => (editing = null)}>Cancel</button>
          <button type="submit" class="btn primary">
            <Icon name="save" />
            {editing.previous ? "Save cookie" : "Add cookie"}
          </button>
        </div>
      </form>
    {/if}
  </div>
  {#snippet footer()}
    <button class="btn" onclick={() => edit(null)} disabled={editing !== null && !editing.previous}
      ><Icon name="plus" /> Add cookie</button
    >
    <button class="btn danger" onclick={clear} disabled={cookies.length === 0}
      ><Icon name="trash" /> Remove all</button
    >
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

  h3 {
    margin: 0;
    font-size: var(--b1);
    font-weight: 400;
  }

  p {
    margin: 0;
  }

  code {
    font-family: var(--font-code);
  }

  .none {
    color: var(--ink-3);
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th {
    padding: 0 var(--s2) 6px 0;
    border-bottom: 1px solid var(--rule);
    text-align: left;
    font-weight: 400;
    color: var(--ink-3);
  }

  td {
    padding: 5px var(--s2) 5px 0;
    border-bottom: 1px solid var(--rule-3);
    vertical-align: middle;
  }

  tr.current td {
    background: var(--sunken);
  }

  .value {
    max-width: 240px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    white-space: nowrap;
    text-align: right;
  }

  .editor {
    display: grid;
    gap: var(--s3);
    padding-top: var(--s3);
    border-top: 1px solid var(--rule);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: var(--s3);
  }

  .field {
    display: grid;
    gap: 6px;
    min-width: 0;
  }

  .field > .bitmap {
    color: var(--ink-3);
  }

  .flags {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s2) var(--s4);
  }

  .flags label {
    display: inline-flex;
    align-items: center;
    gap: var(--s2);
  }

  .problem {
    color: var(--error);
    font-weight: 600;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: var(--s2);
  }
</style>
