<script lang="ts">
  import { api } from "../../lib/api";
  import { copyText } from "../../lib/clipboard";
  import { AUTH_TYPES, emptyAuth } from "../../lib/draft";
  import { message, ws } from "../../lib/store.svelte";
  import type { Auth, Token } from "../../lib/types";

  interface Props {
    auth: Auth;
    /** Path used to resolve variables and inheritance. */
    path?: string | null;
    /** The workspace itself: inheriting means no authentication. */
    root?: boolean;
  }

  let { auth = $bindable(), path = null, root = false }: Props = $props();
  let reveal = $state(false);
  let token = $state<Token | null>(null);
  let tokenError = $state("");
  let fetching = $state(false);

  const types = $derived(root ? AUTH_TYPES.filter((t) => t.type !== "inherit") : AUTH_TYPES);

  function setType(type: Auth["type"]): void {
    if (type !== auth.type) auth = emptyAuth(type);
    token = null;
    tokenError = "";
  }

  async function getToken(): Promise<void> {
    fetching = true;
    tokenError = "";
    token = null;
    try {
      token = await api.oauthToken({ path, auth, environment: ws.environment || null });
    } catch (error) {
      tokenError = message(error);
    } finally {
      fetching = false;
    }
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
    <label class="field">
      <span class="bitmap">Password</span>
      <span class="secret">
        <input
          class="input mono"
          type={reveal ? "text" : "password"}
          bind:value={auth.password}
          spellcheck="false"
          autocomplete="off"
        />
        <button class="btn quiet" onclick={() => (reveal = !reveal)}
          >{reveal ? "Hide" : "Show"}</button
        >
      </span>
    </label>
    <p class="note">
      {auth.type === "digest"
        ? "Answers the server's Digest challenge (MD5, SHA-256 and their -sess forms)."
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
    <label class="field narrow">
      <span class="bitmap">Grant</span>
      <select class="select" bind:value={auth.grant}>
        <option value="client_credentials">Client credentials</option>
        <option value="password">Password</option>
      </select>
    </label>
    <label class="field">
      <span class="bitmap">Token URL</span>
      <input
        class="input mono"
        bind:value={auth.token_url}
        placeholder="https://auth.example.com/oauth/token"
        spellcheck="false"
      />
    </label>
    <label class="field">
      <span class="bitmap">Client ID</span>
      <input class="input mono" bind:value={auth.client_id} spellcheck="false" />
    </label>
    <label class="field">
      <span class="bitmap">Client secret</span>
      <span class="secret">
        <input
          class="input mono"
          type={reveal ? "text" : "password"}
          value={auth.client_secret ?? ""}
          oninput={(event) => {
            if (auth.type === "oauth2") auth.client_secret = event.currentTarget.value;
          }}
          spellcheck="false"
          autocomplete="off"
        />
        <button class="btn quiet" onclick={() => (reveal = !reveal)}
          >{reveal ? "Hide" : "Show"}</button
        >
      </span>
    </label>
    {#if auth.grant === "password"}
      <label class="field">
        <span class="bitmap">Username</span>
        <input
          class="input mono"
          value={auth.username ?? ""}
          oninput={(event) => {
            if (auth.type === "oauth2") auth.username = event.currentTarget.value;
          }}
          spellcheck="false"
        />
      </label>
      <label class="field">
        <span class="bitmap">Password</span>
        <input
          class="input mono"
          type={reveal ? "text" : "password"}
          value={auth.password ?? ""}
          oninput={(event) => {
            if (auth.type === "oauth2") auth.password = event.currentTarget.value;
          }}
          spellcheck="false"
          autocomplete="off"
        />
      </label>
    {/if}
    <label class="field">
      <span class="bitmap">Scope</span>
      <input
        class="input mono"
        value={auth.scope ?? ""}
        oninput={(event) => {
          if (auth.type === "oauth2") auth.scope = event.currentTarget.value;
        }}
        placeholder="read write"
        spellcheck="false"
      />
    </label>
    <label class="field">
      <span class="bitmap">Audience</span>
      <input
        class="input mono"
        value={auth.audience ?? ""}
        oninput={(event) => {
          if (auth.type === "oauth2") auth.audience = event.currentTarget.value;
        }}
        placeholder="Only if your provider asks for one"
        spellcheck="false"
      />
    </label>
    <label class="field narrow">
      <span class="bitmap">Client sends</span>
      <select class="select" bind:value={auth.client_auth}>
        <option value="basic">Basic header</option>
        <option value="body">In the form body</option>
      </select>
    </label>
    <div class="token">
      <button class="btn" onclick={getToken} disabled={fetching || !auth.token_url}>
        {fetching ? "Getting token…" : "Get new token"}
      </button>
      <span class="note"
        >Tokens are fetched when needed and reused until 30 s before they expire.</span
      >
    </div>
    {#if tokenError}
      <p class="error">{tokenError}</p>
    {/if}
    {#if token}
      <div class="token-result">
        <span class="bitmap"
          >{token.token_type || "Bearer"} · {token.expires_in
            ? `expires in ${token.expires_in} s`
            : "no expiry given"}</span
        >
        <button
          class="value mono"
          title="Copy the token"
          onclick={() => void copyText(token?.access_token ?? "", "Token copied")}
        >
          {token.access_token}
        </button>
      </div>
    {/if}
  {/if}
</div>

<style>
  .auth {
    display: grid;
    gap: var(--s3);
    align-content: start;
    max-width: 640px;
  }

  .field {
    display: grid;
    gap: 6px;
  }

  .field > .bitmap {
    color: var(--ink-3);
  }

  .field.narrow {
    max-width: 280px;
  }

  .type {
    max-width: 280px;
    padding-bottom: var(--s2);
    border-bottom: 1px solid var(--rule-3);
  }

  .secret {
    display: flex;
    gap: var(--s1);
  }

  .secret .input {
    flex: 1;
  }

  .note {
    margin: 0;
    color: var(--ink-3);
  }

  code {
    font-family: var(--font-code);
    color: var(--ink-2);
  }

  .token {
    display: flex;
    align-items: center;
    gap: var(--s3);
    flex-wrap: wrap;
    padding-top: var(--s2);
  }

  .error {
    margin: 0;
    color: var(--error);
  }

  .token-result {
    display: grid;
    gap: 6px;
    padding: var(--s3);
    border: 1px solid var(--rule);
    background: var(--field);
  }

  .token-result .bitmap {
    color: var(--ok);
  }

  .value {
    padding: 0;
    border: 0;
    background: none;
    text-align: left;
    word-break: break-all;
    color: var(--ink);
  }

  .value:hover {
    text-decoration: underline;
  }
</style>
