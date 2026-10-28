<script lang="ts">
  import { wholeNumber, splitList } from "../../../lib/auth";
  import type { EdgeGrid } from "../../../lib/types";
  import SecretInput from "./SecretInput.svelte";

  let { auth = $bindable() }: { auth: EdgeGrid } = $props();
</script>

<label class="field">
  <span class="bitmap">Client token</span>
  <input class="input mono" bind:value={auth.client_token} spellcheck="false" />
</label>
<div class="field">
  <span class="bitmap">Client secret</span>
  <SecretInput
    bind:value={auth.client_secret}
    placeholder={"{{edgegridSecret}}"}
    label="Client secret"
  />
</div>
<label class="field">
  <span class="bitmap">Access token</span>
  <input class="input mono" bind:value={auth.access_token} spellcheck="false" />
</label>
<label class="field">
  <span class="bitmap">Base URL</span>
  <input
    class="input mono"
    bind:value={auth.base_url}
    placeholder="https://akab-….luna.akamaiapis.net"
    spellcheck="false"
  />
</label>
<div class="row">
  <label class="field">
    <span class="bitmap">Headers to sign</span>
    <input
      class="input mono"
      value={(auth.headers_to_sign ?? []).join(", ")}
      onchange={(event) => {
        const list = splitList(event.currentTarget.value);
        auth.headers_to_sign = list.length ? list : undefined;
      }}
      placeholder="X-Header, X-Other"
      spellcheck="false"
    />
  </label>
  <label class="field">
    <span class="bitmap">Max body (bytes)</span>
    <input
      class="input mono"
      inputmode="numeric"
      placeholder="131072"
      value={auth.max_body ?? ""}
      oninput={(event) => (auth.max_body = wholeNumber(event.currentTarget.value))}
    />
  </label>
</div>
<p class="note">
  The values of an <code>.edgerc</code> section. With a base URL, requests go to that host and keep
  their path and query. Keep the secret in a variable from a <code>.local.yaml</code> file.
</p>
