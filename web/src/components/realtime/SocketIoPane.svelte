<script lang="ts">
  import { splitList } from "../../lib/realtime";
  import { ws } from "../../lib/store.svelte";
  import type { Draft } from "../../lib/types";
  import Editor from "../Editor.svelte";

  let { settings = $bindable() }: { settings: Draft["socketio"] } = $props();

  const known = $derived(ws.variablesReady ? new Set(ws.variables.map((v) => v.name)) : null);
</script>

<div class="socketio">
  <label class="field">
    <span class="bitmap">Namespace</span>
    <input
      class="input mono"
      placeholder="The URL's path, or /"
      spellcheck="false"
      bind:value={settings.namespace}
    />
  </label>
  <div class="field">
    <span class="bitmap" id="sio-auth">Auth payload</span>
    <div class="auth">
      <Editor
        bind:value={settings.auth}
        language="json"
        lint
        lineNumbers={false}
        {known}
        label="Auth payload"
        placeholder={'A JSON object the server reads as socket.handshake.auth, such as {"token": "{{token}}"}'}
      />
    </div>
  </div>
  <label class="field">
    <span class="bitmap">Listen to</span>
    <input
      class="input mono"
      placeholder="Every event"
      spellcheck="false"
      value={settings.events.join(", ")}
      onchange={(event) => (settings.events = splitList(event.currentTarget.value))}
    />
    <span class="muted note">Event names, separated by commas. Others are not shown.</span>
  </label>
  <label class="field">
    <span class="bitmap">Path</span>
    <input
      class="input mono"
      placeholder="/socket.io/"
      spellcheck="false"
      bind:value={settings.path}
    />
  </label>
  <p class="muted">
    Getman speaks Socket.IO v4 over WebSocket. Headers and authentication from the other tabs go
    with the connection.
  </p>
</div>

<style>
  .socketio {
    display: grid;
    gap: var(--s3);
    align-content: start;
    max-width: 560px;
  }

  .field {
    display: grid;
    gap: 6px;
  }

  .field > .bitmap {
    color: var(--ink-3);
  }

  .auth {
    height: 96px;
    border: 1px solid var(--rule-2);
    background: var(--field);
  }

  .note {
    font-size: var(--fs-meta);
  }

  p {
    margin: 0;
  }
</style>
