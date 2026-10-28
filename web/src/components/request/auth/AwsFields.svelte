<script lang="ts">
  import { wholeNumber } from "../../../lib/auth";
  import type { AwsSigV4 } from "../../../lib/types";
  import SecretInput from "./SecretInput.svelte";

  let { auth = $bindable() }: { auth: AwsSigV4 } = $props();
</script>

<label class="field">
  <span class="bitmap">Access key ID</span>
  <input
    class="input mono"
    bind:value={auth.access_key}
    placeholder={"{{awsAccessKeyId}}"}
    spellcheck="false"
  />
</label>
<div class="field">
  <span class="bitmap">Secret access key</span>
  <SecretInput
    bind:value={auth.secret_key}
    placeholder={"{{awsSecretAccessKey}}"}
    label="Secret access key"
  />
</div>
<div class="field">
  <span class="bitmap">Session token</span>
  <SecretInput
    bind:value={auth.session_token}
    placeholder="Only for temporary credentials"
    label="Session token"
  />
</div>
<div class="row">
  <label class="field">
    <span class="bitmap">Region</span>
    <input class="input mono" bind:value={auth.region} placeholder="us-east-1" spellcheck="false" />
  </label>
  <label class="field">
    <span class="bitmap">Service</span>
    <input
      class="input mono"
      bind:value={auth.service}
      placeholder="execute-api, s3, …"
      spellcheck="false"
    />
  </label>
</div>
<div class="row">
  <label class="field">
    <span class="bitmap">Sign in</span>
    <select
      class="select"
      value={auth.in ?? "header"}
      onchange={(event) => (auth.in = event.currentTarget.value === "query" ? "query" : undefined)}
    >
      <option value="header">Headers</option>
      <option value="query">Presigned URL (query)</option>
    </select>
  </label>
  {#if auth.in === "query"}
    <label class="field">
      <span class="bitmap">Valid for</span>
      <input
        class="input mono"
        inputmode="numeric"
        placeholder="3600"
        value={auth.expires_in ?? ""}
        oninput={(event) => (auth.expires_in = wholeNumber(event.currentTarget.value))}
      />
    </label>
  {/if}
</div>
<label class="toggle">
  <input
    type="checkbox"
    class="check"
    checked={auth.unsigned_payload ?? false}
    onchange={(event) => (auth.unsigned_payload = event.currentTarget.checked || undefined)}
  />
  Unsigned payload: sign <code>UNSIGNED-PAYLOAD</code> instead of the body's hash, as S3 allows
</label>
<p class="note">
  Signs the finished request, after pre-request scripts, with
  <code>AWS4-HMAC-SHA256</code>{auth.in === "query"
    ? " in the query string; the URL works until it expires"
    : " in the Authorization header"}. Keep the keys in variables from a
  <code>.local.yaml</code> file.
</p>
