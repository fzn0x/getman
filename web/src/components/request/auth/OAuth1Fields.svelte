<script lang="ts">
  import type { OAuth1, OAuth1Method } from "../../../lib/types";
  import PrivateKey from "./PrivateKey.svelte";
  import SecretInput from "./SecretInput.svelte";

  let { auth = $bindable() }: { auth: OAuth1 } = $props();

  const METHODS: OAuth1Method[] = [
    "HMAC-SHA1",
    "HMAC-SHA256",
    "HMAC-SHA512",
    "PLAINTEXT",
    "RSA-SHA1",
    "RSA-SHA256",
    "RSA-SHA512",
  ];
  const rsa = $derived((auth.signature_method ?? "HMAC-SHA1").startsWith("RSA"));
</script>

<label class="field narrow">
  <span class="bitmap">Signature method</span>
  <select
    class="select"
    value={auth.signature_method ?? "HMAC-SHA1"}
    onchange={(event) => (auth.signature_method = event.currentTarget.value as OAuth1Method)}
  >
    {#each METHODS as method (method)}
      <option value={method}>{method}</option>
    {/each}
  </select>
</label>
<label class="field">
  <span class="bitmap">Consumer key</span>
  <input class="input mono" bind:value={auth.consumer_key} spellcheck="false" />
</label>
{#if rsa}
  <PrivateKey bind:key={auth.private_key} bind:file={auth.private_key_file} />
{:else}
  <div class="field">
    <span class="bitmap">Consumer secret</span>
    <SecretInput
      bind:value={auth.consumer_secret}
      placeholder={"{{consumerSecret}}"}
      label="Consumer secret"
    />
  </div>
{/if}
<label class="field">
  <span class="bitmap">Access token</span>
  <input
    class="input mono"
    bind:value={auth.token}
    placeholder="Leave empty to ask for a request token"
    spellcheck="false"
  />
</label>
{#if !rsa}
  <div class="field">
    <span class="bitmap">Token secret</span>
    <SecretInput
      bind:value={auth.token_secret}
      placeholder={"{{tokenSecret}}"}
      label="Token secret"
    />
  </div>
{/if}
<details class="more">
  <summary class="bitmap">More parameters</summary>
  <div class="more-fields">
    <div class="row">
      <label class="field">
        <span class="bitmap">Callback</span>
        <input class="input mono" bind:value={auth.callback} spellcheck="false" />
      </label>
      <label class="field">
        <span class="bitmap">Verifier</span>
        <input class="input mono" bind:value={auth.verifier} spellcheck="false" />
      </label>
    </div>
    <div class="row">
      <label class="field">
        <span class="bitmap">Realm</span>
        <input class="input mono" bind:value={auth.realm} spellcheck="false" />
      </label>
      <label class="field">
        <span class="bitmap">Version</span>
        <input class="input mono" bind:value={auth.version} placeholder="1.0" spellcheck="false" />
      </label>
    </div>
    <div class="row">
      <label class="field">
        <span class="bitmap">Timestamp</span>
        <input
          class="input mono"
          bind:value={auth.timestamp}
          placeholder="Now"
          spellcheck="false"
        />
      </label>
      <label class="field">
        <span class="bitmap">Nonce</span>
        <input class="input mono" bind:value={auth.nonce} placeholder="Random" spellcheck="false" />
      </label>
    </div>
  </div>
</details>
<div class="row">
  <label class="field">
    <span class="bitmap">Send in</span>
    <select
      class="select"
      value={auth.in ?? "header"}
      onchange={(event) => {
        const value = event.currentTarget.value;
        auth.in = value === "query" || value === "body" ? value : undefined;
      }}
    >
      <option value="header">Authorization header</option>
      <option value="query">Query string</option>
      <option value="body">Form body</option>
    </select>
  </label>
  <label class="toggle">
    <input
      type="checkbox"
      class="check"
      checked={auth.body_hash ?? false}
      onchange={(event) => (auth.body_hash = event.currentTarget.checked || undefined)}
    />
    Sign the body (<code>oauth_body_hash</code>)
  </label>
</div>
<p class="note">
  Signs the method, URL, query and form fields of the finished request. Keep secrets in variables
  from a <code>.local.yaml</code> file.
</p>
