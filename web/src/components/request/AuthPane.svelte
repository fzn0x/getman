<script lang="ts">
  import { AUTH_TYPES, emptyAuth } from "../../lib/draft";
  import type { Auth } from "../../lib/types";
  import AsapFields from "./auth/AsapFields.svelte";
  import AwsFields from "./auth/AwsFields.svelte";
  import EdgeGridFields from "./auth/EdgeGridFields.svelte";
  import HawkFields from "./auth/HawkFields.svelte";
  import JwtFields from "./auth/JwtFields.svelte";
  import OAuth1Fields from "./auth/OAuth1Fields.svelte";
  import OAuth2Fields from "./auth/OAuth2Fields.svelte";
  import SecretInput from "./auth/SecretInput.svelte";

  interface Props {
    auth: Auth;
    /** Path used to resolve variables and inheritance. */
    path?: string | null;
    /** The workspace itself: inheriting means no authentication. */
    root?: boolean;
  }

  let { auth = $bindable(), path = null, root = false }: Props = $props();

  const types = $derived(root ? AUTH_TYPES.filter((t) => t.type !== "inherit") : AUTH_TYPES);

  function setType(type: Auth["type"]): void {
    if (type !== auth.type) auth = emptyAuth(type);
  }
</script>

<div class="auth">
  <label class="field type">
    <span class="bitmap">Type</span>
    <select
      class="select"
      value={root && auth.type === "inherit" ? "none" : auth.type}
      onchange={(event) => setType(event.currentTarget.value as Auth["type"])}
    >
      {#each types as option (option.type)}
        <option value={option.type}>{option.label}</option>
      {/each}
    </select>
  </label>

  {#if auth.type === "inherit"}
    <p class="note">
      Uses the authentication of the enclosing folder, or the workspace's. Set it once there for
      every request inside.
    </p>
  {:else if auth.type === "none"}
    <p class="note">Sends no credentials{root ? "" : ", even if a folder above sets some"}.</p>
  {:else if auth.type === "bearer"}
    <label class="field">
      <span class="bitmap">Token</span>
      <input
        class="input mono"
        bind:value={auth.token}
        placeholder={"{{token}}"}
        spellcheck="false"
      />
    </label>
    <label class="field narrow">
      <span class="bitmap">Prefix</span>
      <input class="input mono" bind:value={auth.prefix} placeholder="Bearer" spellcheck="false" />
    </label>
    <p class="note">Sent as <code>Authorization: {auth.prefix || "Bearer"} &lt;token&gt;</code>.</p>
  {:else if auth.type === "basic" || auth.type === "digest"}
    <label class="field">
      <span class="bitmap">Username</span>
      <input class="input mono" bind:value={auth.username} spellcheck="false" autocomplete="off" />
    </label>
    <div class="field">
      <span class="bitmap">Password</span>
      <SecretInput bind:value={auth.password} label="Password" />
    </div>
    <p class="note">
      {auth.type === "digest"
        ? "Answers the server's Digest challenge (MD5, SHA-256, SHA-512-256 and their -sess forms)."
        : "Sent as an Authorization: Basic header."}
      Keep real passwords in a variable from a <code>.local.yaml</code> file, such as
      <code>{"{{password}}"}</code>.
    </p>
  {:else if auth.type === "api_key"}
    <label class="field">
      <span class="bitmap">Name</span>
      <input class="input mono" bind:value={auth.key} placeholder="X-API-Key" spellcheck="false" />
    </label>
    <label class="field">
      <span class="bitmap">Value</span>
      <input
        class="input mono"
        bind:value={auth.value}
        placeholder={"{{apiKey}}"}
        spellcheck="false"
      />
    </label>
    <label class="field narrow">
      <span class="bitmap">Send in</span>
      <select class="select" bind:value={auth.in}>
        <option value="header">Header</option>
        <option value="query">Query string</option>
      </select>
    </label>
  {:else if auth.type === "oauth2"}
    <OAuth2Fields bind:auth {path} />
  {:else if auth.type === "oauth1"}
    <OAuth1Fields bind:auth />
  {:else if auth.type === "jwt"}
    <JwtFields bind:auth />
  {:else if auth.type === "aws_sigv4"}
    <AwsFields bind:auth />
  {:else if auth.type === "hawk"}
    <HawkFields bind:auth />
  {:else if auth.type === "edgegrid"}
    <EdgeGridFields bind:auth />
  {:else if auth.type === "asap"}
    <AsapFields bind:auth />
  {/if}
</div>

<style>
  .auth {
    display: grid;
    gap: var(--s3);
    align-content: start;
    max-width: 640px;
  }

  /* Shared by the fields of every type, which live in their own files. */
  .auth :global(.field) {
    display: grid;
    gap: 6px;
    min-width: 0;
  }

  .auth :global(.field > .bitmap) {
    color: var(--ink-3);
  }

  .auth :global(.field.narrow) {
    max-width: 280px;
  }

  .auth :global(.row) {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: var(--s3);
    align-items: end;
  }

  .auth :global(.toggle) {
    display: flex;
    align-items: center;
    gap: var(--s2);
    min-height: var(--control);
  }

  .auth :global(.editor-box) {
    height: 140px;
    border: 1px solid var(--rule-2);
    overflow: hidden;
  }

  .auth :global(.editor-box.small) {
    height: 88px;
  }

  .auth :global(.more > summary) {
    cursor: pointer;
    color: var(--ink-3);
    padding: var(--s1) 0;
  }

  .auth :global(.more-fields) {
    display: grid;
    gap: var(--s3);
    padding-top: var(--s2);
  }

  .type {
    max-width: 280px;
    padding-bottom: var(--s2);
    border-bottom: 1px solid var(--rule-3);
  }

  .auth :global(.note) {
    margin: 0;
    color: var(--ink-3);
  }

  .auth :global(code) {
    font-family: var(--font-code);
    color: var(--ink-2);
  }

  .auth :global(.token) {
    display: flex;
    align-items: center;
    gap: var(--s3);
    flex-wrap: wrap;
    padding-top: var(--s2);
  }

  .auth :global(.error) {
    margin: 0;
    color: var(--error);
  }

  .auth :global(.token-result) {
    display: grid;
    gap: 6px;
    padding: var(--s3);
    border: 1px solid var(--rule);
    background: var(--field);
  }

  .auth :global(.token-result .bitmap) {
    color: var(--ok);
  }

  .auth :global(.value) {
    padding: 0;
    border: 0;
    background: none;
    text-align: left;
    word-break: break-all;
    color: var(--ink);
  }

  .auth :global(.value:hover) {
    text-decoration: underline;
  }
</style>
