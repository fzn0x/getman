<script lang="ts">
  import { pickedValue, suggestName } from "../../lib/bytes";
  import { copyText } from "../../lib/clipboard";
  import { bodyKind, bytes, payloadBytes, payloadText, prettyJson } from "../../lib/format";
  import { parsePath, walk } from "../../lib/jsonpath";
  import { keyLabel } from "../../lib/keys";
  import { openDialog, type RequestTab } from "../../lib/store.svelte";
  import type { Response } from "../../lib/types";
  import Editor from "../Editor.svelte";
  import Icon from "../Icon.svelte";
  import HexView from "./HexView.svelte";

  let {
    response,
    tab = null,
    noun = "response",
  }: {
    response: Pick<Response, "headers" | "body" | "truncated" | "size">;
    tab?: RequestTab | null;
    /** What the body belongs to, for labels and the name of a saved file. */
    noun?: "response" | "request";
  } = $props();
  const label = $derived(noun === "request" ? "Request body" : "Response body");

  const contentType = $derived(
    response.headers.find((h) => h.name.toLowerCase() === "content-type")?.value,
  );
  const kind = $derived(bodyKind(contentType, response.body));
  const text = $derived(payloadText(response.body));
  const big = $derived(response.size.body > 4_000_000);

  let mode = $state<"pretty" | "raw" | "hex" | "preview">("pretty");
  let wrap = $state(false);
  let filter = $state("");
  let editor = $state<ReturnType<typeof Editor>>();
  let selected = $state({ text: "", line: "" });

  const previewable = $derived(kind === "html" || kind === "image" || kind === "pdf");
  $effect(() => {
    if ((kind === "image" || kind === "pdf") && mode !== "preview" && mode !== "hex") {
      mode = "preview";
    }
    if (kind === "binary" && mode !== "hex" && mode !== "raw") mode = "hex";
  });
  const hexBytes = $derived(mode === "hex" ? payloadBytes(response.body) : new Uint8Array());
  const showsText = $derived(
    mode !== "hex" && mode !== "preview" && kind !== "binary" && text !== "",
  );

  function setAsVariable(): void {
    openDialog({
      kind: "setVariable",
      value: pickedValue(selected.text),
      name: suggestName(selected.line),
      tab,
    });
  }

  const language = $derived(
    kind === "json"
      ? "json"
      : kind === "xml"
        ? "xml"
        : kind === "html"
          ? "html"
          : kind === "javascript"
            ? "javascript"
            : "text",
  );

  const filtered = $derived.by((): { text: string; error: string } => {
    if (kind !== "json" || filter.trim() === "") return { text: "", error: "" };
    const steps = parsePath(filter);
    if (typeof steps === "string") return { text: "", error: steps };
    try {
      const result = walk(JSON.parse(text), steps);
      return result.found
        ? { text: JSON.stringify(result.value, null, 2) ?? "undefined", error: "" }
        : { text: "", error: "Nothing at that path" };
    } catch {
      return { text: "", error: "The body is not valid JSON" };
    }
  });

  const shown = $derived(
    filtered.text !== ""
      ? filtered.text
      : mode === "pretty" && kind === "json" && !big
        ? prettyJson(text)
        : text,
  );

  let blobUrl = $state("");
  $effect(() => {
    if (mode !== "preview" || (kind !== "image" && kind !== "pdf")) return;
    const bytesView = payloadBytes(response.body);
    const blob = new Blob([bytesView.slice().buffer], {
      type: contentType ?? "application/octet-stream",
    });
    const url = URL.createObjectURL(blob);
    blobUrl = url;
    return () => URL.revokeObjectURL(url);
  });

  function download(): void {
    const data = payloadBytes(response.body);
    const blob = new Blob([data.slice().buffer], {
      type: contentType ?? "application/octet-stream",
    });
    const link = document.createElement("a");
    link.href = URL.createObjectURL(blob);
    const extension =
      kind === "json"
        ? "json"
        : kind === "html"
          ? "html"
          : kind === "xml"
            ? "xml"
            : kind === "image"
              ? (contentType?.split("/")[1] ?? "img")
              : kind === "pdf"
                ? "pdf"
                : "txt";
    link.download = `${noun}.${extension}`;
    link.click();
    setTimeout(() => URL.revokeObjectURL(link.href), 1000);
  }
</script>

<div class="viewer">
  <div class="bar">
    <div class="modes" role="radiogroup" aria-label="View">
      <button
        role="radio"
        aria-checked={mode === "pretty"}
        class:on={mode === "pretty"}
        onclick={() => (mode = "pretty")}
        disabled={kind === "binary" || kind === "image" || kind === "pdf"}>Pretty</button
      >
      <button
        role="radio"
        aria-checked={mode === "raw"}
        class:on={mode === "raw"}
        onclick={() => (mode = "raw")}
        disabled={kind === "image" || kind === "pdf"}>Raw</button
      >
      <button
        role="radio"
        aria-checked={mode === "hex"}
        class:on={mode === "hex"}
        onclick={() => (mode = "hex")}
        disabled={response.size.body === 0}>Hex</button
      >
      <button
        role="radio"
        aria-checked={mode === "preview"}
        class:on={mode === "preview"}
        onclick={() => (mode = "preview")}
        disabled={!previewable}>Preview</button
      >
    </div>
    {#if kind === "json" && mode !== "preview" && mode !== "hex"}
      <label class="filter">
        <span class="sr-only">Filter the JSON by path</span>
        <input
          class="input mono"
          bind:value={filter}
          placeholder="data[0].name"
          spellcheck="false"
        />
      </label>
    {/if}
    <span class="kind bitmap">{kind} · {bytes(response.size.body)}</span>
    {#if showsText}
      <button
        class="icon-btn"
        aria-label="Set the selected text as a variable"
        title={selected.text
          ? "Set the selected text as a variable"
          : "Select a value in the body to set it as a variable"}
        disabled={!selected.text.trim()}
        onclick={setAsVariable}
      >
        <Icon name="pencil" />
      </button>
      <button
        class="icon-btn"
        aria-label="Find in the body"
        title="Find: match case, whole word, regex ({keyLabel('mod+f').join(' ')})"
        onclick={() => editor?.openSearch()}
      >
        <Icon name="search" />
      </button>
    {/if}
    <button
      class="icon-btn"
      aria-label="Wrap lines"
      aria-pressed={wrap}
      title="Wrap lines"
      onclick={() => (wrap = !wrap)}
    >
      <Icon name="stack" />
    </button>
    <button
      class="icon-btn"
      aria-label="Copy the body"
      title="Copy the body"
      onclick={() => void copyText(text, "Body copied")}
    >
      <Icon name="copy" />
    </button>
    <button
      class="icon-btn"
      aria-label="Save the body to a file"
      title="Save to a file"
      onclick={download}
    >
      <Icon name="export" />
    </button>
  </div>

  {#if response.truncated}
    <p class="notice">
      The body is cut short at the size limit, 100 MiB unless Largest response in Settings says
      otherwise.
    </p>
  {/if}
  {#if filtered.error}
    <p class="notice">{filtered.error}</p>
  {/if}

  <div class="content">
    {#if mode === "preview" && kind === "html"}
      <iframe title="HTML preview" sandbox="" srcdoc={text}></iframe>
    {:else if mode === "preview" && kind === "image" && blobUrl}
      <div class="image"><img src={blobUrl} alt={label} /></div>
    {:else if mode === "preview" && kind === "pdf" && blobUrl}
      <iframe title="PDF preview" src={blobUrl}></iframe>
    {:else if mode === "hex"}
      <HexView data={hexBytes} />
    {:else if kind === "binary"}
      <div class="binary">
        <p class="bitmap big">Binary</p>
        <p>{bytes(response.size.body)} of {contentType ?? "data"} that is not text.</p>
        <button class="btn" onclick={download}><Icon name="export" /> Save to a file</button>
      </div>
    {:else if text === ""}
      <div class="binary">
        <p class="bitmap big">Empty</p>
        <p>The {noun} has no body.</p>
      </div>
    {:else}
      <Editor
        bind:this={editor}
        value={shown}
        readonly
        {language}
        {wrap}
        links
        {label}
        onSelection={(text, line) => (selected = { text, line })}
      />
    {/if}
  </div>
</div>

<style>
  .viewer {
    display: grid;
    grid-template-rows: auto auto auto minmax(0, 1fr);
    /* A long line, as in minified code, scrolls instead of widening the bar. */
    grid-template-columns: minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: var(--s2) var(--s3);
    min-width: 0;
  }

  .modes {
    display: flex;
    border: 1px solid var(--rule);
  }

  .modes button {
    height: 24px;
    padding: 0 8px;
    border: 0;
    border-right: 1px solid var(--rule-2);
    background: none;
    color: var(--ink-2);
    font-family: var(--font-bitmap);
    font-size: var(--b1);
    text-transform: uppercase;
    -webkit-font-smoothing: none;
  }

  .modes button:last-child {
    border-right: 0;
  }

  .modes button.on {
    background: var(--ink);
    color: var(--paper);
  }

  .modes button:disabled {
    color: var(--t4);
    cursor: not-allowed;
  }

  .filter {
    flex: 1;
    min-width: 80px;
    max-width: 260px;
  }

  .filter .input {
    width: 100%;
    height: 26px;
  }

  .kind {
    margin-left: auto;
    color: var(--ink-3);
    white-space: nowrap;
  }

  .notice {
    margin: 0 var(--s3) var(--s2);
    padding: 6px var(--s2);
    background: var(--warn-wash);
    color: var(--ink);
  }

  .content {
    min-height: 0;
    border-top: 1px solid var(--rule-3);
  }

  iframe {
    width: 100%;
    height: 100%;
    border: 0;
    background: #fff;
  }

  .image {
    display: grid;
    place-items: center;
    height: 100%;
    overflow: auto;
    padding: var(--s4);
    background-image: radial-gradient(circle, var(--t3) 0.8px, transparent 1.2px);
    background-size: 8px 8px;
  }

  .image img {
    max-width: 100%;
    image-rendering: auto;
  }

  .binary {
    display: grid;
    gap: var(--s2);
    justify-items: start;
    padding: var(--s4);
    color: var(--ink-2);
  }

  .binary p {
    margin: 0;
  }

  .big {
    font-size: var(--b3);
    color: var(--ink);
  }
</style>
