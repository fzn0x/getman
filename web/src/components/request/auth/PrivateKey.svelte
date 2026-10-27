<script lang="ts">
  // A PEM private key: text, usually a variable, or a file in the workspace.

  interface Props {
    key: string | undefined;
    file: string | undefined;
  }

  let { key = $bindable(), file = $bindable() }: Props = $props();
  let fromFile = $state(Boolean(file));
</script>

<div class="field">
  <span class="bitmap">Private key</span>
  <span class="choice">
    <select
      class="select"
      aria-label="Where the private key comes from"
      value={fromFile ? "file" : "text"}
      onchange={(event) => {
        fromFile = event.currentTarget.value === "file";
        if (fromFile) key = "";
        else file = "";
      }}
    >
      <option value="text">PEM text or variable</option>
      <option value="file">File in the workspace</option>
    </select>
  </span>
  {#if fromFile}
    <input
      class="input mono"
      aria-label="Key file"
      value={file ?? ""}
      oninput={(event) => (file = event.currentTarget.value)}
      placeholder="keys/signing.pem"
      spellcheck="false"
    />
  {:else}
    <textarea
      class="input mono"
      aria-label="Private key"
      rows="3"
      value={key ?? ""}
      oninput={(event) => (key = event.currentTarget.value)}
      placeholder={"{{privateKey}}"}
      spellcheck="false"
      autocomplete="off"></textarea>
  {/if}
  <span class="note">
    PKCS#8, PKCS#1 or SEC1 PEM. Keep the key out of shared files: put it in a variable from a
    <code>.local.yaml</code> file, or name a git-ignored file.
  </span>
</div>

<style>
  .choice {
    max-width: 280px;
    display: grid;
  }
</style>
