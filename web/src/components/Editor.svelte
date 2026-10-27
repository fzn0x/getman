<script lang="ts">
  import { openSearchPanel } from "@codemirror/search";
  import { Compartment, EditorState } from "@codemirror/state";
  import { EditorView } from "@codemirror/view";
  import { onMount } from "svelte";
  import {
    baseExtensions,
    languageExtension,
    linkMarks,
    variableMarks,
    type EditorLanguage,
  } from "../lib/editor";
  import { completionNames } from "../lib/store.svelte";

  interface Props {
    value?: string;
    language?: EditorLanguage;
    readonly?: boolean;
    singleLine?: boolean;
    lineNumbers?: boolean;
    wrap?: boolean;
    lint?: boolean;
    placeholder?: string;
    /** Names that are defined; others are flagged. `null` flags nothing. */
    known?: Set<string> | null;
    label?: string;
    onSubmit?: () => void;
    onPaste?: (text: string) => boolean;
    /** Underline URLs and open them when clicked. */
    links?: boolean;
    /** Told the selected text, and the line it starts on, as it changes. */
    onSelection?: (text: string, line: string) => void;
    class?: string;
  }

  let {
    value = $bindable(""),
    language = "text",
    readonly = false,
    singleLine = false,
    lineNumbers = true,
    wrap = false,
    lint = false,
    placeholder = "",
    known = null,
    label = "",
    onSubmit,
    onPaste,
    links = false,
    onSelection,
    class: className = "",
  }: Props = $props();

  let host: HTMLDivElement;
  let view: EditorView | undefined;
  const languageSlot = new Compartment();
  const readonlySlot = new Compartment();
  const variablesSlot = new Compartment();
  const wrapSlot = new Compartment();

  onMount(() => {
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: value,
        extensions: [
          baseExtensions({
            singleLine,
            lineNumbers: lineNumbers && !singleLine,
            placeholder,
            completion: { names: completionNames },
            onSubmit: () => onSubmit?.(),
            onPaste: (text) => onPaste?.(text) ?? false,
          }),
          languageSlot.of(languageExtension(language, lint)),
          readonlySlot.of([EditorState.readOnly.of(readonly), EditorView.editable.of(!readonly)]),
          variablesSlot.of(variableMarks(known)),
          wrapSlot.of(wrap ? EditorView.lineWrapping : []),
          EditorView.contentAttributes.of(label ? { "aria-label": label } : {}),
          links ? linkMarks() : [],
          EditorView.updateListener.of((update) => {
            if (update.docChanged) value = update.state.doc.toString();
            if (onSelection && (update.selectionSet || update.docChanged)) {
              const range = update.state.selection.main;
              onSelection(
                update.state.sliceDoc(range.from, range.to),
                update.state.doc.lineAt(range.from).text,
              );
            }
          }),
        ],
      }),
    });
    return () => view?.destroy();
  });

  $effect(() => {
    const next = value;
    if (view && next !== view.state.doc.toString()) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next } });
    }
  });

  $effect(() => {
    view?.dispatch({ effects: languageSlot.reconfigure(languageExtension(language, lint)) });
  });

  $effect(() => {
    view?.dispatch({
      effects: readonlySlot.reconfigure([
        EditorState.readOnly.of(readonly),
        EditorView.editable.of(!readonly),
      ]),
    });
  });

  $effect(() => {
    view?.dispatch({ effects: variablesSlot.reconfigure(variableMarks(known)) });
  });

  $effect(() => {
    view?.dispatch({ effects: wrapSlot.reconfigure(wrap ? EditorView.lineWrapping : []) });
  });

  /** Opens the find bar: next and previous, match case, whole word, regex. */
  export function openSearch(): void {
    if (view) openSearchPanel(view);
  }

  export function focus(): void {
    view?.focus();
    if (view && singleLine) {
      view.dispatch({ selection: { anchor: view.state.doc.length } });
    }
  }
</script>

<div class="editor {className}" class:single={singleLine} bind:this={host}></div>

<style>
  .editor {
    min-height: 0;
    height: 100%;
    overflow: hidden;
  }

  .editor :global(.cm-editor) {
    height: 100%;
  }

  .editor.single {
    height: auto;
  }

  .editor.single :global(.cm-editor) {
    background: transparent;
  }

  .editor.single :global(.cm-content) {
    padding: 0;
  }

  .editor.single :global(.cm-line) {
    padding: 0;
  }

  .editor.single :global(.cm-scroller) {
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
    line-height: 26px;
  }

  .editor.single :global(.cm-scroller::-webkit-scrollbar) {
    display: none;
  }
</style>
