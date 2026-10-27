<!--
  A plain text input that shows {{variables}} the way the URL bar does,
  completes them after {{, and tells what one holds on hover or when the
  caret is in it.

  The text is highlighted by a copy laid behind the input, whose own text
  turns transparent while it holds a variable. A native input per cell
  stays fast with hundreds of rows, where a code editor per cell takes
  about 0.7 ms each to build (275 ms for 200 rows, 1.5 s for 500, against
  9 ms and 22 ms this way, measured in Edge).
-->
<script lang="ts">
  import { tick } from "svelte";
  import { completionNames, describeVariable, knownNames } from "../../lib/store.svelte";
  import {
    applyCompletion,
    completionAt,
    rankNames,
    referenceAt,
    segments,
    shownValue,
  } from "../../lib/variables";

  interface Props {
    value: string;
    onValue: (value: string) => void;
    label: string;
    placeholder?: string;
    /** Marks the input for the table's own lookups, as `data-field`. */
    field?: string;
    /** Id of a datalist of suggestions. */
    list?: string;
    readonly?: boolean;
    /** The row is switched off: strike the text through. */
    off?: boolean;
  }

  let {
    value,
    onValue,
    label,
    placeholder = "",
    field,
    list,
    readonly = false,
    off = false,
  }: Props = $props();

  const uid = Math.random().toString(36).slice(2);
  let input: HTMLInputElement;
  let mirror = $state<HTMLSpanElement>();

  const parts = $derived(value.includes("{{") ? segments(value) : []);
  const marked = $derived(parts.some((part) => part.name !== undefined));
  const known = $derived(knownNames());
  const missing = (name: string) => known !== null && !name.startsWith("$") && !known.has(name);

  let complete = $state<{ from: number; index: number; query: string } | null>(null);
  let at = $state({ left: 0, top: 0, width: 0 });
  const options = $derived(complete ? rankNames(completionNames(), complete.query) : []);

  let tip = $state<{ name: string; left: number; top: number; keyboard: boolean } | null>(null);
  let revealed = $state(false);
  let hide: ReturnType<typeof setTimeout> | undefined;

  // The copy behind scrolls with the input when the text runs past its edge.
  $effect(() => {
    void parts;
    if (mirror && input) mirror.scrollLeft = input.scrollLeft;
  });

  function sync(): void {
    if (mirror) mirror.scrollLeft = input.scrollLeft;
  }

  function updateCompletion(): void {
    const found = completionAt(input.value, input.selectionStart ?? input.value.length);
    if (!found) {
      complete = null;
      return;
    }
    const rect = input.getBoundingClientRect();
    at = { left: rect.left, top: rect.bottom + 2, width: rect.width };
    complete = {
      from: found.from,
      query: found.query,
      index: complete?.query === found.query ? complete.index : 0,
    };
  }

  async function accept(label: string): Promise<void> {
    if (!complete) return;
    const caret = input.selectionStart ?? input.value.length;
    const result = applyCompletion(input.value, complete.from, caret, label);
    complete = null;
    onValue(result.text);
    await tick();
    if (input.value !== result.text) input.value = result.text;
    input.setSelectionRange(result.caret, result.caret);
    sync();
    showCaretTip();
  }

  /** The span behind the input that shows the reference starting at `from`. */
  function spanAt(from: number): HTMLElement | undefined {
    return mirror?.querySelector<HTMLElement>(`[data-from="${from}"]`) ?? undefined;
  }

  function showTip(name: string, rect: DOMRect, keyboard: boolean): void {
    clearTimeout(hide);
    if (tip?.name !== name) revealed = false;
    tip = {
      name,
      left: Math.max(8, Math.min(rect.left, window.innerWidth - 328)),
      top: rect.bottom + 4,
      keyboard,
    };
  }

  /** Describes the reference the caret is in, so the keyboard sees it too. */
  function showCaretTip(): void {
    if (complete || document.activeElement !== input) return;
    const caret = input.selectionStart ?? 0;
    const reference =
      input.selectionStart === input.selectionEnd ? referenceAt(value, caret) : null;
    const span = reference ? spanAt(reference.from) : undefined;
    if (reference?.name && span) showTip(reference.name, span.getBoundingClientRect(), true);
    else if (tip?.keyboard) tip = null;
  }

  function onPointerMove(event: PointerEvent): void {
    if (!marked || !mirror) return;
    for (const span of mirror.querySelectorAll<HTMLElement>("[data-name]")) {
      const rect = span.getBoundingClientRect();
      if (
        event.clientX >= rect.left &&
        event.clientX <= rect.right &&
        event.clientY >= rect.top &&
        event.clientY <= rect.bottom
      ) {
        showTip(span.dataset.name!, rect, false);
        return;
      }
    }
    if (tip && !tip.keyboard) later();
  }

  function later(): void {
    clearTimeout(hide);
    hide = setTimeout(() => {
      if (!tip?.keyboard) tip = null;
    }, 200);
  }

  function onKeydown(event: KeyboardEvent): void {
    if (complete && options.length > 0) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const step = event.key === "ArrowDown" ? 1 : -1;
        complete.index = (complete.index + step + options.length) % options.length;
        return;
      }
      if (event.key === "Enter" || event.key === "Tab") {
        event.preventDefault();
        void accept(options[complete.index]!.label);
        return;
      }
    }
    if (event.key === "Escape" && (complete || tip)) {
      event.preventDefault();
      event.stopPropagation();
      if (complete) complete = null;
      else tip = null;
    }
  }

  const info = $derived(tip ? describeVariable(tip.name) : null);
</script>

<span class="var-input" class:marked>
  {#if marked}
    <span class="mirror mono" class:off aria-hidden="true" bind:this={mirror}
      >{#each parts as part (part.from)}{#if part.name !== undefined}<span
            class="ref"
            class:missing={missing(part.name)}
            data-name={part.name}
            data-from={part.from}>{part.text}</span
          >{:else}{part.text}{/if}{/each}</span
    >
  {/if}
  <input
    bind:this={input}
    class="input mono"
    {value}
    {placeholder}
    aria-label={label}
    data-field={field}
    {list}
    {readonly}
    spellcheck="false"
    autocomplete="off"
    role="combobox"
    aria-autocomplete="list"
    aria-expanded={complete !== null && options.length > 0}
    aria-controls="vc-{uid}"
    aria-activedescendant={complete && options.length ? `vc-${uid}-${complete.index}` : undefined}
    aria-describedby={tip ? `vt-${uid}` : undefined}
    oninput={() => {
      onValue(input.value);
      updateCompletion();
      sync();
    }}
    onkeydown={onKeydown}
    onkeyup={(event) => {
      if (event.key.startsWith("Arrow") || event.key === "Home" || event.key === "End") {
        if (complete) updateCompletion();
        showCaretTip();
      }
    }}
    onclick={showCaretTip}
    onscroll={sync}
    onblur={() => {
      complete = null;
      if (tip?.keyboard) tip = null;
    }}
    onpointermove={onPointerMove}
    onpointerleave={() => {
      if (tip && !tip.keyboard) later();
    }}
  />
</span>

{#if complete && options.length > 0}
  <ul
    class="complete"
    id="vc-{uid}"
    role="listbox"
    aria-label="Variables"
    style:left="{at.left}px"
    style:top="{at.top}px"
    style:min-width="{Math.min(at.width, 420)}px"
  >
    {#each options as option, i (option.label)}
      <li
        id="vc-{uid}-{i}"
        role="option"
        aria-selected={i === complete.index}
        class:on={i === complete.index}
        onpointerdown={(event) => {
          event.preventDefault();
          void accept(option.label);
        }}
      >
        <span class="name">{option.label}</span>
        {#if option.detail}<span class="detail">{option.detail}</span>{/if}
      </li>
    {/each}
  </ul>
{/if}

{#if tip}
  <div
    class="tip"
    id="vt-{uid}"
    role="tooltip"
    style:left="{tip.left}px"
    style:top="{tip.top}px"
    onpointerenter={() => clearTimeout(hide)}
    onpointerleave={() => {
      if (tip && !tip.keyboard) later();
    }}
  >
    <div class="head">
      <span class="mono">{tip.name}</span>
      <span class="bitmap scope"
        >{info?.kind === "variable" ? info.variable.scope : info ? "dynamic" : "not defined"}</span
      >
    </div>
    {#if info?.kind === "variable"}
      <div class="value mono" class:masked={info.variable.secret && !revealed}>
        {shownValue(info.variable.value, info.variable.secret === true, revealed) || "(empty)"}
      </div>
      {#if info.variable.secret}
        <button
          class="btn quiet"
          tabindex="-1"
          onpointerdown={(event) => event.preventDefault()}
          onclick={() => (revealed = !revealed)}>{revealed ? "Hide" : "Reveal"}</button
        >
      {/if}
    {:else if info}
      <div class="value">{info.description}</div>
    {:else}
      <div class="value missing">
        No scope defines it. Add it to an environment, a folder or the request's Vars, or it is sent
        as written.
      </div>
    {/if}
  </div>
{/if}

<style>
  .var-input {
    position: relative;
    display: block;
    width: 100%;
    min-width: 0;
  }

  .var-input.marked {
    background: var(--field);
  }

  .var-input input {
    position: relative;
    width: 100%;
  }

  /* The input keeps the caret and the selection; the copy behind shows the
     text with its variables marked. */
  .var-input.marked input {
    color: transparent;
    background: transparent;
  }

  .mirror {
    position: absolute;
    inset: 0;
    display: block;
    overflow: hidden;
    padding: 0 var(--s2);
    border: 1px solid transparent;
    color: var(--ink);
    font-size: var(--fs-ui);
    line-height: 28px;
    white-space: pre;
    pointer-events: none;
  }

  .mirror.off {
    color: var(--ink-3);
    text-decoration: line-through;
    text-decoration-color: var(--t4);
  }

  .ref {
    color: var(--var-ink);
    background: var(--var-wash);
  }

  .ref.missing {
    color: var(--var-missing);
    text-decoration: underline dotted;
    text-underline-offset: 3px;
  }

  .complete {
    position: fixed;
    z-index: 70;
    max-width: 420px;
    max-height: 240px;
    overflow: auto;
    margin: 0;
    padding: 2px 0;
    list-style: none;
    border: 1px solid var(--rule);
    background: var(--field);
    box-shadow: 0 6px 18px -6px color-mix(in srgb, var(--t10) 35%, transparent);
    font-family: var(--font-code);
    font-size: var(--fs-code);
  }

  .complete li {
    display: flex;
    gap: 1em;
    padding: 2px var(--s2);
    white-space: nowrap;
    cursor: pointer;
  }

  .complete li.on,
  .complete li:hover {
    background: var(--ink);
    color: var(--paper);
  }

  .detail {
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--ink-3);
  }

  .complete li.on .detail,
  .complete li:hover .detail {
    color: inherit;
    opacity: 0.7;
  }

  .tip {
    position: fixed;
    z-index: 70;
    display: grid;
    gap: 4px;
    justify-items: start;
    width: max-content;
    max-width: 320px;
    padding: var(--s2);
    border: 1px solid var(--rule);
    background: var(--field);
    box-shadow: 0 6px 18px -6px color-mix(in srgb, var(--t10) 35%, transparent);
  }

  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--s3);
    width: 100%;
    color: var(--var-ink);
  }

  .scope {
    color: var(--ink-3);
  }

  .value {
    max-height: 120px;
    overflow: auto;
    color: var(--ink);
    white-space: pre-wrap;
    word-break: break-all;
  }

  .value.masked {
    letter-spacing: 1px;
  }

  .value.missing {
    color: var(--ink-2);
    font-size: var(--fs-meta);
    word-break: normal;
  }

  .tip .btn {
    height: 22px;
    padding: 0 var(--s2);
  }
</style>
