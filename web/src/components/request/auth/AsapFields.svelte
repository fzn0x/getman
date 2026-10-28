<script lang="ts">
  import { wholeNumber } from "../../../lib/auth";
  import { ws } from "../../../lib/store.svelte";
  import type { Asap, JwtAlgorithm } from "../../../lib/types";
  import Editor from "../../Editor.svelte";
  import PrivateKey from "./PrivateKey.svelte";

  let { auth = $bindable() }: { auth: Asap } = $props();

  const ALGORITHMS: JwtAlgorithm[] = [
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
</script>

<div class="row">
  <label class="field">
    <span class="bitmap">Issuer</span>
    <input class="input mono" bind:value={auth.issuer} spellcheck="false" />
  </label>
  <label class="field">
    <span class="bitmap">Audience</span>
    <input
      class="input mono"
      bind:value={auth.audience}
      placeholder="Separate several with commas"
      spellcheck="false"
    />
  </label>
</div>
<div class="row">
  <label class="field">
    <span class="bitmap">Key ID</span>
    <input
      class="input mono"
      bind:value={auth.key_id}
      placeholder="issuer/key-name"
      spellcheck="false"
    />
  </label>
  <label class="field">
    <span class="bitmap">Subject</span>
    <input class="input mono" bind:value={auth.subject} placeholder="Optional" spellcheck="false" />
  </label>
</div>
<PrivateKey bind:key={auth.private_key} bind:file={auth.private_key_file} />
<div class="row">
  <label class="field">
    <span class="bitmap">Algorithm</span>
    <select
      class="select"
      value={auth.algorithm ?? "RS256"}
      onchange={(event) => {
        const value = event.currentTarget.value as JwtAlgorithm;
        auth.algorithm = value === "RS256" ? undefined : value;
      }}
    >
      {#each ALGORITHMS as algorithm (algorithm)}
        <option value={algorithm}>{algorithm}</option>
      {/each}
    </select>
  </label>
  <label class="field">
    <span class="bitmap">Expires in (seconds)</span>
    <input
      class="input mono"
      inputmode="numeric"
      placeholder="3600"
      value={auth.expires_in ?? ""}
      oninput={(event) => (auth.expires_in = wholeNumber(event.currentTarget.value))}
    />
  </label>
</div>
<div class="field">
  <span class="bitmap">More claims</span>
  <div class="editor-box small">
    <Editor
      bind:value={() => auth.claims ?? "", (v) => (auth.claims = v)}
      language="json"
      {known}
      label="ASAP claims"
      placeholder={'{ "scope": "read" }'}
    />
  </div>
</div>
<p class="note">
  Sent as a Bearer token signed for every request, with <code>iss</code>, <code>sub</code>,
  <code>aud</code>, a unique <code>jti</code>, <code>iat</code> and <code>exp</code>.
</p>
