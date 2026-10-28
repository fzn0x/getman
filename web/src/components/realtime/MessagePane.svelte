<script lang="ts">
  import { keyLabel } from "../../lib/keys";
  import type { LiveState } from "../../lib/realtime.svelte";
  import { sendComposer, sendLive } from "../../lib/realtime.svelte";
  import { notify, promptDialog, ws, type RequestTab } from "../../lib/store.svelte";
  import type { MessageFormat } from "../../lib/types";
  import Editor from "../Editor.svelte";
  import Icon from "../Icon.svelte";

  let { tab, conn }: { tab: RequestTab; conn: LiveState } = $props();

  const draft = $derived(tab.draft);
  const composer = $derived(conn.composer);
  const io = $derived(draft.kind === "socketio");
  const open = $derived(conn.status === "open");
  const known = $derived(ws.variablesReady ? new Set(ws.variables.map((v) => v.name)) : null);

  const FORMATS: { value: MessageFormat; label: string }[] = [
    { value: "text", label: "Text" },
    { value: "json", label: "JSON" },
    { value: "binary", label: "Binary (Base64)" },
  ];

  function save(): void {
    promptDialog({
      title: io ? "Save event" : "Save message",
      label: "Name",
      value: io ? composer.event : "",
      confirm: "Save",
      onSubmit: (name) => {
        const trimmed = name.trim();
        if (io) {
          if (!composer.event.trim()) {
            notify("Name the event to emit first.", "error");
            return;
          }
          draft.socketio.messages.push({
            name: trimmed,
            event: composer.event.trim(),
            args: composer.args,
            ack: composer.ack,
          });
        } else {
          draft.websocket.messages.push({
            name: trimmed,
            type: composer.type,
            content: composer.content,
          });
        }
        notify("Saved with the request; save the request to keep it", "ok");
      },
    });
  }

  function load(index: number): void {
    if (io) {
      const saved = draft.socketio.messages[index]!;
      composer.event = saved.event;
      composer.args = saved.args;
      composer.ack = saved.ack;
    } else {
      const saved = draft.websocket.messages[index]!;
      composer.type = saved.type;
      composer.content = saved.content;
    }
  }

  function remove(index: number): void {
    if (io) draft.socketio.messages.splice(index, 1);
    else draft.websocket.messages.splice(index, 1);
  }

  function sendSaved(index: number): void {
    if (io) {
      const saved = draft.socketio.messages[index]!;
      void sendLive(tab, { emit: { ...saved } });
    } else {
      const saved = draft.websocket.messages[index]!;
      void sendLive(tab, { message: { ...saved } });
    }
  }

  const saved = $derived(
    io
      ? draft.socketio.messages.map((m) => ({
          name: m.name || m.event,
          tag: m.ack ? `${m.event} · ack` : m.event,
          text: m.args,
        }))
      : draft.websocket.messages.map((m) => ({
          name: m.name || "Untitled",
          tag: FORMATS.find((f) => f.value === m.type)?.label ?? m.type,
          text: m.content,
        })),
  );
</script>

<div class="composer">
  <div class="bar">
    {#if io}
      <input
        class="input mono event"
        placeholder="Event name"
        aria-label="Event name"
        spellcheck="false"
        bind:value={composer.event}
      />
      <label class="ack">
        <input type="checkbox" class="check" bind:checked={composer.ack} />
        Ask for an acknowledgement
      </label>
    {:else}
      <select class="select" aria-label="Message type" bind:value={composer.type}>
        {#each FORMATS as format (format.value)}
          <option value={format.value}>{format.label}</option>
        {/each}
      </select>
    {/if}
    <span class="spacer"></span>
    <button class="btn quiet" onclick={save} title="Keep this message in the request file">
      <Icon name="save" /> Save
    </button>
    <button
      class="btn"
      onclick={() => void sendComposer(tab)}
      disabled={!open}
      title={open ? `Send (${keyLabel("mod+enter").join(" ")})` : "Connect first, then send"}
    >
      <Icon name="play" />
      {io ? "Emit" : "Send"}
    </button>
  </div>

  <div class="write">
    {#if io}
      <Editor
        bind:value={composer.args}
        language="json"
        {known}
        label="Event arguments"
        placeholder={'Arguments: a JSON array, such as ["room-1", {"limit": 10}], or one value. May use {{variables}}.'}
      />
    {:else}
      <Editor
        bind:value={composer.content}
        language={composer.type === "json" ? "json" : "text"}
        lint={composer.type === "json"}
        {known}
        label="Message"
        placeholder={composer.type === "binary"
          ? "The bytes to send, in Base64"
          : "The message to send. May use {{variables}}."}
      />
    {/if}
  </div>

  <section class="saved" aria-label="Saved messages">
    <h3 class="bitmap">Saved {io ? "events" : "messages"}</h3>
    {#if saved.length === 0}
      <p class="muted">
        Saved {io ? "events" : "messages"} are kept in the request file, ready to send with one click
        here, or with <code>getman send --message NAME</code>.
      </p>
    {:else}
      <ul>
        {#each saved as item, i (i)}
          <li>
            <button class="pick" onclick={() => load(i)} title="Put it in the editor">
              <span class="name">{item.name}</span>
              <span class="tag bitmap">{item.tag}</span>
              <span class="text mono">{item.text.replace(/\s+/g, " ")}</span>
            </button>
            <button
              class="icon-btn"
              aria-label="Send {item.name}"
              title={open ? "Send" : "Connect first, then send"}
              disabled={!open}
              onclick={() => sendSaved(i)}><Icon name="play" /></button
            >
            <button class="icon-btn" aria-label="Delete {item.name}" onclick={() => remove(i)}
              ><Icon name="trash" /></button
            >
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</div>

<style>
  .composer {
    display: grid;
    /* The saved messages take what they need, up to 40% of the pane. */
    grid-template-rows: auto minmax(120px, 1fr) fit-content(40%);
    height: 100%;
    min-height: 0;
  }

  .bar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--s2);
    padding: var(--s2) var(--s4);
    border-bottom: 1px solid var(--rule-3);
  }

  .event {
    flex: 0 1 220px;
    min-width: 120px;
  }

  .ack {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }

  .spacer {
    flex: 1;
  }

  .write {
    min-height: 0;
    border-bottom: 1px solid var(--rule-3);
  }

  .saved {
    overflow: auto;
    padding: var(--s3) var(--s4);
  }

  h3 {
    margin: 0 0 var(--s2);
    color: var(--ink-3);
  }

  .saved p {
    margin: 0;
    max-width: 52ch;
  }

  code {
    font-family: var(--font-code);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  li {
    display: flex;
    align-items: center;
    gap: 2px;
    border-bottom: 1px solid var(--rule-3);
  }

  .pick {
    display: grid;
    grid-template-columns: auto auto minmax(0, 1fr);
    align-items: baseline;
    gap: var(--s2);
    flex: 1;
    min-width: 0;
    min-height: 28px;
    padding: 4px var(--s2) 4px 0;
    border: 0;
    background: transparent;
    color: var(--ink);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .pick:hover .name {
    text-decoration: underline;
  }

  .name {
    font-weight: 600;
    white-space: nowrap;
  }

  .tag {
    color: var(--ink-3);
    white-space: nowrap;
  }

  .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ink-2);
  }
</style>
