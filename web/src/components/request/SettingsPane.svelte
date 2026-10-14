<script lang="ts">
  import type { Settings } from "../../lib/types";

  let { settings = $bindable(), scope = "request" }: { settings: Settings; scope?: string } =
    $props();

  function tri(value: boolean | null | undefined): string {
    return value === true ? "on" : value === false ? "off" : "";
  }

  function fromTri(value: string): boolean | null {
    return value === "on" ? true : value === "off" ? false : null;
  }

  function number(value: string): number | null {
    const n = Number(value);
    return value.trim() === "" || !Number.isFinite(n) || n < 0 ? null : Math.round(n);
  }

  const inherit = $derived(scope === "workspace" ? "Default" : "Inherit");
</script>

<div class="settings">
  <label class="field">
    <span class="bitmap">Timeout</span>
    <span class="with-unit">
      <input
        class="input mono"
        inputmode="numeric"
        placeholder="{inherit} (30000)"
        value={settings.timeout_ms ?? ""}
        oninput={(event) => (settings.timeout_ms = number(event.currentTarget.value))}
      />
      <span class="muted">ms · 0 waits forever</span>
    </span>
  </label>
  <label class="field">
    <span class="bitmap">Follow redirects</span>
    <select
      class="select"
      value={tri(settings.follow_redirects)}
      onchange={(event) => (settings.follow_redirects = fromTri(event.currentTarget.value))}
    >
      <option value="">{inherit} (yes)</option>
      <option value="on">Yes</option>
      <option value="off">No</option>
    </select>
  </label>
  <label class="field">
    <span class="bitmap">Most redirects</span>
    <input
      class="input mono"
      inputmode="numeric"
      placeholder="{inherit} (10)"
      value={settings.max_redirects ?? ""}
      oninput={(event) => (settings.max_redirects = number(event.currentTarget.value))}
    />
  </label>
  <label class="field">
    <span class="bitmap">Check TLS certificates</span>
    <select
      class="select"
      value={tri(settings.verify_tls)}
      onchange={(event) => (settings.verify_tls = fromTri(event.currentTarget.value))}
    >
      <option value="">{inherit} (yes)</option>
      <option value="on">Yes</option>
      <option value="off">No — accept self-signed and expired certificates</option>
    </select>
  </label>
  <p class="muted">
    {scope === "request"
      ? "Unset values come from the folder, then the workspace."
      : "Requests inside use these unless they set their own."}
  </p>
</div>

<style>
  .settings {
    display: grid;
    gap: var(--s3);
    align-content: start;
    max-width: 460px;
  }

  .field {
    display: grid;
    gap: 6px;
  }

  .field > .bitmap {
    color: var(--ink-3);
  }

  .with-unit {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }

  .with-unit .input {
    width: 160px;
  }

  p {
    margin: 0;
  }
</style>
