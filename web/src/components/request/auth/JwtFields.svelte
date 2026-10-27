<script lang="ts">
  import { wholeNumber } from "../../../lib/auth";
  import { ws } from "../../../lib/store.svelte";
  import type { Jwt, JwtAlgorithm } from "../../../lib/types";
  import Editor from "../../Editor.svelte";
  import PrivateKey from "./PrivateKey.svelte";
  import SecretInput from "./SecretInput.svelte";

  let { auth = $bindable() }: { auth: Jwt } = $props();

  const ALGORITHMS: JwtAlgorithm[] = [
    "HS256",
    "HS384",
    "HS512",
    "RS256",
    "RS384",
    "RS512",
    "PS256",
    "PS384",
    "PS512",
    "ES256",
    "ES384",
  ];
  const known = $derived(new Set(ws.variables.map((v) => v.name)));
  const hmac = $derived((auth.algorithm ?? "HS256").startsWith("HS"));
</script>

<label class="field narrow">
  <span class="bitmap">Algorithm</span>
  <select
    class="select"
    value={auth.algorithm ?? "HS256"}
    onchange={(event) => (auth.algorithm = event.currentTarget.value as JwtAlgorithm)}
  >
    {#each ALGORITHMS as algorithm (algorithm)}
      <option value={algorithm}>{algorithm}</option>
    {/each}
  </select>
</label>
{#if hmac}
  <div class="field">
    <span class="bitmap">Secret</span>
    <SecretInput bind:value={auth.secret} placeholder={"{{jwtSecret}}"} label="Secret" />
  </div>
  <label class="toggle">
    <input
      type="checkbox"
      class="check"
      checked={auth.secret_base64 ?? false}
      onchange={(event) => (auth.secret_base64 = event.currentTarget.checked || undefined)}
    />
    The secret is base64-encoded
  </label>
{:else}
  <PrivateKey bind:key={auth.private_key} bind:file={auth.private_key_file} />
{/if}
<div class="field">
  <span class="bitmap">Payload</span>
  <div class="editor-box">
    <Editor
      bind:value={() => auth.payload ?? "", (v) => (auth.payload = v)}
      language="json"
      {known}
      label="JWT payload"
      placeholder={'{\n  "sub": "{{userId}}"\n}'}
    />
  </div>
</div>
<div class="row">
  <label class="toggle">
    <input
      type="checkbox"
      class="check"
      checked={auth.issued_at ?? false}
      onchange={(event) => (auth.issued_at = event.currentTarget.checked || undefined)}
    />
    Add <code>iat</code>, the time it is signed
  </label>
  <label class="field">
    <span class="bitmap">Expires in (seconds)</span>
    <input
      class="input mono"
      inputmode="numeric"
      placeholder="No exp claim"
      value={auth.expires_in ?? ""}
      oninput={(event) => (auth.expires_in = wholeNumber(event.currentTarget.value))}
    />
  </label>
</div>
<div class="field">
  <span class="bitmap">Extra header fields</span>
  <div class="editor-box small">
    <Editor
      bind:value={() => auth.header ?? "", (v) => (auth.header = v)}
      language="json"
      {known}
      label="JWT header"
      placeholder={'{ "kid": "key-1" }'}
    />
  </div>
</div>
<div class="row">
  <label class="field">
    <span class="bitmap">Send in</span>
    <select
      class="select"
      value={auth.in ?? "header"}
      onchange={(event) => (auth.in = event.currentTarget.value === "query" ? "query" : undefined)}
    >
      <option value="header">Authorization header</option>
      <option value="query">Query parameter</option>
    </select>
  </label>
  {#if auth.in === "query"}
    <label class="field">
      <span class="bitmap">Parameter</span>
      <input class="input mono" bind:value={auth.param} placeholder="token" spellcheck="false" />
    </label>
  {:else}
    <label class="field">
      <span class="bitmap">Prefix</span>
      <input class="input mono" bind:value={auth.prefix} placeholder="Bearer" spellcheck="false" />
    </label>
  {/if}
</div>
<p class="note">
  Signed for every request, after pre-request scripts. <code>alg</code> and <code>typ</code> are set
  for you; <code>{"{{variables}}"}</code> work in the payload and header.
</p>
