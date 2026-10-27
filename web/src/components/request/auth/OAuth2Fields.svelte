<script lang="ts">
  import { api } from "../../../lib/api";
  import { expiryText, isLoopbackUrl, usesBrowser } from "../../../lib/auth";
  import { copyText } from "../../../lib/clipboard";
  import { message, ws } from "../../../lib/store.svelte";
  import type { Authorization, OAuth2, OAuth2Grant, Token } from "../../../lib/types";
  import KeyValueTable from "../KeyValueTable.svelte";
  import SecretInput from "./SecretInput.svelte";

  interface Props {
    auth: OAuth2;
    /** Path used to resolve variables and inheritance. */
    path?: string | null;
  }

  let { auth = $bindable(), path = null }: Props = $props();
  let token = $state<Token | null>(null);
  let tokenError = $state("");
  let fetching = $state(false);
  let signIn = $state<Authorization | null>(null);
  let now = $state(Date.now());

  const browser = $derived(usesBrowser(auth));
  const implicit = $derived(auth.grant === "implicit");
  const callbackOk = $derived(!auth.callback_url || isLoopbackUrl(auth.callback_url));
  const canGet = $derived(
    browser ? Boolean(auth.auth_url && (implicit || auth.token_url)) : Boolean(auth.token_url),
  );

  function args() {
    return { path, auth: $state.snapshot(auth), environment: ws.environment || null };
  }

  // Show the token Getman holds for these settings, once they settle.
  $effect(() => {
    const request = JSON.stringify(args());
    const timer = setTimeout(async () => {
      try {
        const status = await api.oauthStatus(JSON.parse(request));
        if (!fetching) token = status.token;
      } catch {
        // The settings are incomplete; there is nothing to show.
      }
    }, 300);
    return () => clearTimeout(timer);
  });

  // Keep the expiry current.
  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 15_000);
    return () => clearInterval(timer);
  });

  function setGrant(grant: OAuth2Grant): void {
    auth.grant = grant;
    if (grant === "authorization_code" && !auth.pkce) auth.pkce = "s256";
    token = null;
    tokenError = "";
  }

  async function getToken(): Promise<void> {
    fetching = true;
    tokenError = "";
    try {
      if (browser) {
        signIn = await api.oauthAuthorize(args());
        token = await api.oauthWait(signIn.id);
      } else {
        token = await api.oauthToken(args());
      }
    } catch (error) {
      tokenError = message(error);
    } finally {
      fetching = false;
      signIn = null;
      now = Date.now();
    }
  }

  async function cancel(): Promise<void> {
    if (signIn) await api.oauthCancel(signIn.id).catch(() => undefined);
  }

  async function clearToken(): Promise<void> {
    tokenError = "";
    try {
      await api.oauthClear(args());
      token = null;
    } catch (error) {
      tokenError = message(error);
    }
  }
</script>

<label class="field narrow">
  <span class="bitmap">Grant</span>
  <select
    class="select"
    value={auth.grant}
    onchange={(event) => setGrant(event.currentTarget.value as OAuth2Grant)}
  >
    <option value="authorization_code">Authorization code</option>
    <option value="implicit">Implicit</option>
    <option value="client_credentials">Client credentials</option>
    <option value="password">Password</option>
  </select>
</label>
{#if browser}
  <label class="field">
    <span class="bitmap">Authorization URL</span>
    <input
      class="input mono"
      bind:value={auth.auth_url}
      placeholder="https://auth.example.com/authorize"
      spellcheck="false"
    />
  </label>
{/if}
{#if !implicit}
  <label class="field">
    <span class="bitmap">Token URL</span>
    <input
      class="input mono"
      bind:value={auth.token_url}
      placeholder="https://auth.example.com/oauth/token"
      spellcheck="false"
    />
  </label>
{/if}
<label class="field">
  <span class="bitmap">Client ID</span>
  <input class="input mono" bind:value={auth.client_id} spellcheck="false" />
</label>
{#if !implicit}
  <div class="field">
    <span class="bitmap">Client secret</span>
    <SecretInput
      bind:value={auth.client_secret}
      placeholder={browser ? "Empty for a public client" : "{{clientSecret}}"}
      label="Client secret"
    />
  </div>
{/if}
{#if browser}
  <label class="field">
    <span class="bitmap">Callback URL</span>
    <input
      class="input mono"
      bind:value={auth.callback_url}
      placeholder="http://127.0.0.1:<free port>/callback"
      spellcheck="false"
    />
    {#if !callbackOk}
      <span class="error"
        >Getman can only receive the answer on this machine, at a plain http URL such as
        http://127.0.0.1:8765/callback. It uses a free port instead.</span
      >
    {/if}
  </label>
  {#if !implicit}
    <label class="field narrow">
      <span class="bitmap">PKCE</span>
      <select
        class="select"
        value={auth.pkce ?? "off"}
        onchange={(event) => {
          const value = event.currentTarget.value;
          auth.pkce = value === "s256" || value === "plain" ? value : undefined;
        }}
      >
        <option value="s256">SHA-256 (S256)</option>
        <option value="plain">Plain</option>
        <option value="off">Off</option>
      </select>
    </label>
  {/if}
{/if}
{#if auth.grant === "password"}
  <label class="field">
    <span class="bitmap">Username</span>
    <input class="input mono" bind:value={auth.username} spellcheck="false" />
  </label>
  <div class="field">
    <span class="bitmap">Password</span>
    <SecretInput bind:value={auth.password} placeholder={"{{password}}"} label="Password" />
  </div>
{/if}
<label class="field">
  <span class="bitmap">Scope</span>
  <input class="input mono" bind:value={auth.scope} placeholder="read write" spellcheck="false" />
</label>
<div class="row">
  <label class="field">
    <span class="bitmap">Audience</span>
    <input
      class="input mono"
      bind:value={auth.audience}
      placeholder="Only if asked for"
      spellcheck="false"
    />
  </label>
  <label class="field">
    <span class="bitmap">Resource</span>
    <input
      class="input mono"
      bind:value={auth.resource}
      placeholder="Only if asked for"
      spellcheck="false"
    />
  </label>
</div>
{#if !implicit}
  <label class="field narrow">
    <span class="bitmap">Client sends</span>
    <select class="select" bind:value={auth.client_auth}>
      <option value="basic">Basic header</option>
      <option value="body">In the form body</option>
    </select>
  </label>
{/if}
<div class="row">
  <label class="field">
    <span class="bitmap">Send token in</span>
    <select
      class="select"
      value={auth.token_in ?? "header"}
      onchange={(event) =>
        (auth.token_in = event.currentTarget.value === "query" ? "query" : undefined)}
    >
      <option value="header">Authorization header</option>
      <option value="query">Query (access_token)</option>
    </select>
  </label>
  {#if auth.token_in !== "query"}
    <label class="field">
      <span class="bitmap">Prefix</span>
      <input class="input mono" bind:value={auth.prefix} placeholder="Bearer" spellcheck="false" />
    </label>
  {/if}
</div>
<details class="more">
  <summary class="bitmap">Extra parameters</summary>
  <div class="more-fields">
    {#if browser}
      <KeyValueTable
        rows={auth.auth_params ?? []}
        onChange={(rows) => (auth.auth_params = rows.length ? rows : undefined)}
        label="Authorization request parameters"
        nameLabel="Authorization request"
        bulk={false}
      />
    {/if}
    {#if !implicit}
      <KeyValueTable
        rows={auth.token_params ?? []}
        onChange={(rows) => (auth.token_params = rows.length ? rows : undefined)}
        label="Token request parameters"
        nameLabel="Token request"
        bulk={false}
      />
    {/if}
  </div>
</details>

<div class="token">
  <button class="btn" type="button" onclick={getToken} disabled={fetching || !canGet}>
    {fetching ? (browser ? "Waiting for sign-in…" : "Getting token…") : "Get new token"}
  </button>
  {#if signIn}
    <button class="btn quiet" type="button" onclick={cancel}>Cancel</button>
  {/if}
  {#if token && !fetching}
    <button class="btn quiet" type="button" onclick={clearToken}>Clear token</button>
  {/if}
</div>
{#if signIn}
  <p class="note" role="status">
    {signIn.opened ? "Sign in in the browser window Getman opened." : "Open the sign-in page:"}
    <a href={signIn.url} target="_blank" rel="noreferrer noopener"
      >{signIn.opened ? "Open it again" : signIn.url}</a
    >. Getman listens on <code>{signIn.redirect_uri}</code> for five minutes.
  </p>
  {#each signIn.notes as note (note)}
    <p class="error">{note}</p>
  {/each}
{/if}
{#if tokenError}
  <p class="error" role="alert">{tokenError}</p>
{/if}
{#if token}
  <div class="token-result">
    <span class="bitmap"
      >{token.token_type || "Bearer"} · {expiryText(token, now)}{token.refreshable
        ? " · Refreshable"
        : ""}</span
    >
    <button
      class="value mono"
      title="Copy the token"
      onclick={() => void copyText(token?.access_token ?? "", "Token copied")}
    >
      {token.access_token}
    </button>
    {#if token.scope}<span class="note">Scope: {token.scope}</span>{/if}
  </div>
{/if}
<p class="note">
  {#if browser}
    Get new token opens your browser to sign in; Getman checks the answer and keeps the token in the
    git-ignored <code>.getman/</code> folder, where <code>getman run</code> and
    <code>getman send</code> find it. It is refreshed when it expires, if the provider sent a refresh
    token.
  {:else}
    Tokens are fetched when needed, reused until 30 s before they expire, and refreshed with a
    refresh token when there is one.
  {/if}
</p>
