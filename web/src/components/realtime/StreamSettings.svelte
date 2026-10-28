<script lang="ts">
  import { splitList } from "../../lib/realtime";
  import type { Draft } from "../../lib/types";

  let { draft }: { draft: Draft } = $props();
</script>

<div class="stream">
  {#if draft.kind === "websocket"}
    <label class="field">
      <span class="bitmap">Subprotocols</span>
      <input
        class="input mono"
        placeholder="None, such as chat.v2, graphql-ws"
        spellcheck="false"
        value={draft.websocket.subprotocols.join(", ")}
        onchange={(event) => (draft.websocket.subprotocols = splitList(event.currentTarget.value))}
      />
      <span class="muted note">Offered in Sec-WebSocket-Protocol, preferred first.</span>
    </label>
  {:else if draft.kind === "sse"}
    <label class="toggle">
      <input type="checkbox" class="check" bind:checked={draft.sse.reconnect} />
      Reconnect when the stream ends, sending the last event id in Last-Event-ID
    </label>
  {:else if draft.kind === "socketio"}
    <label class="field">
      <span class="bitmap">Transport</span>
      <select class="select" bind:value={draft.socketio.transport}>
        <option value="auto">Long polling, then WebSocket</option>
        <option value="websocket">WebSocket only</option>
        <option value="polling">Long polling only</option>
      </select>
      <span class="muted note">
        As the Socket.IO client does: start on HTTP long polling and upgrade to WebSocket if the
        server offers it. Choose WebSocket only for a server that allows nothing else.
      </span>
    </label>
  {/if}
  <fieldset class="until">
    <legend class="bitmap">Stop after, in runs</legend>
    <label>
      Messages
      <input
        class="input"
        type="number"
        min="0"
        step="1"
        placeholder="10"
        bind:value={draft.until.messages}
      />
    </label>
    <label>
      Milliseconds
      <input
        class="input"
        type="number"
        min="0"
        step="100"
        placeholder="5000"
        bind:value={draft.until.time_ms}
      />
    </label>
    <span class="muted note">
      <code>getman run</code> and scripted sends end the connection at the first limit reached, then run
      the post-response script and checks. Leave both empty for 10 messages or 5 seconds.
    </span>
  </fieldset>
  <p class="muted">
    The timeout below is how long connecting may take; an open connection stays open until it is
    closed.
  </p>
</div>

<style>
  .stream {
    display: grid;
    gap: var(--s3);
    max-width: 460px;
    margin-bottom: var(--s4);
    padding-bottom: var(--s4);
    border-bottom: 1px solid var(--rule-3);
  }

  .field {
    display: grid;
    gap: 6px;
  }

  .field > .bitmap {
    color: var(--ink-3);
  }

  .note {
    font-size: var(--fs-meta);
  }

  .until {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--s2) var(--s3);
    margin: 0;
    padding: 0;
    border: 0;
  }

  .until legend {
    padding: 0;
    margin-bottom: 6px;
    color: var(--ink-3);
  }

  .until label {
    display: grid;
    gap: 6px;
  }

  .until .note {
    grid-column: 1 / -1;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }

  p {
    margin: 0;
  }
</style>
