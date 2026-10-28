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
  {/if}
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

  .toggle {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }

  p {
    margin: 0;
  }
</style>
