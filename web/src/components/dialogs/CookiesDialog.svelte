<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../../lib/api";
  import { message, notify } from "../../lib/store.svelte";
  import type { Cookie } from "../../lib/types";
  import Icon from "../Icon.svelte";
  import Dialog from "./Dialog.svelte";

  let cookies = $state<Cookie[]>([]);
  let loaded = $state(false);

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
    await load();
  }

  async function clear(): Promise<void> {
    await api.clearCookies().catch(() => {});
    await load();
  }
</script>

<Dialog title="Cookies" width="760px">
  <div class="body">
    <h2 class="bitmap">Cookies</h2>
    <p class="muted">
      Cookies servers set are kept for this session and sent back like a browser would. They are not
      saved to disk.
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
            <th><span class="sr-only">Remove</span></th>
          </tr>
        </thead>
        <tbody>
          {#each cookies as cookie (`${cookie.domain}${cookie.path}${cookie.name}`)}
            <tr>
              <td class="mono">{cookie.domain}{cookie.path === "/" ? "" : cookie.path}</td>
              <td class="mono">{cookie.name}</td>
              <td class="mono value">{cookie.value}</td>
              <td class="muted"
                >{cookie.expires ? new Date(cookie.expires).toLocaleString() : "session"}</td
              >
              <td>
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
  </div>
  {#snippet footer()}
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

  p {
    margin: 0;
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

  .value {
    max-width: 240px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
