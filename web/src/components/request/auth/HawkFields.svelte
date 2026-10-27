<script lang="ts">
  import type { Hawk } from "../../../lib/types";
  import SecretInput from "./SecretInput.svelte";

  let { auth = $bindable() }: { auth: Hawk } = $props();
</script>

<label class="field">
  <span class="bitmap">Hawk ID</span>
  <input class="input mono" bind:value={auth.id} spellcheck="false" />
</label>
<div class="field">
  <span class="bitmap">Key</span>
  <SecretInput bind:value={auth.key} placeholder={"{{hawkKey}}"} label="Hawk key" />
</div>
<label class="field narrow">
  <span class="bitmap">Algorithm</span>
  <select
    class="select"
    value={auth.algorithm ?? "sha256"}
    onchange={(event) =>
      (auth.algorithm = event.currentTarget.value === "sha1" ? "sha1" : undefined)}
  >
    <option value="sha256">SHA-256</option>
    <option value="sha1">SHA-1</option>
  </select>
</label>
<label class="field">
  <span class="bitmap">Ext</span>
  <input
    class="input mono"
    bind:value={auth.ext}
    placeholder="Application data, optional"
    spellcheck="false"
  />
</label>
<div class="row">
  <label class="field">
    <span class="bitmap">App</span>
    <input class="input mono" bind:value={auth.app} spellcheck="false" />
  </label>
  <label class="field">
    <span class="bitmap">Dlg</span>
    <input class="input mono" bind:value={auth.dlg} spellcheck="false" />
  </label>
</div>
<label class="toggle">
  <input
    type="checkbox"
    class="check"
    checked={auth.payload_hash ?? false}
    onchange={(event) => (auth.payload_hash = event.currentTarget.checked || undefined)}
  />
  Sign the body too (payload hash)
</label>
<p class="note">
  Sends <code>Authorization: Hawk …</code> with a fresh timestamp and nonce for every request.
</p>
