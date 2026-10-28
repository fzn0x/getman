// CodeMirror set up for Getman: themed from the page's tokens, with
// `{{variables}}` marked (and unknown ones flagged) and completed.

import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
  type CompletionContext,
  type CompletionResult,
  type CompletionSource,
} from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { html } from "@codemirror/lang-html";
import { javascript } from "@codemirror/lang-javascript";
import { json, jsonParseLinter } from "@codemirror/lang-json";
import { xml } from "@codemirror/lang-xml";
import {
  bracketMatching,
  foldGutter,
  foldKeymap,
  HighlightStyle,
  indentOnInput,
  syntaxHighlighting,
} from "@codemirror/language";
import { linter } from "@codemirror/lint";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { EditorState, type Extension } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  hoverTooltip,
  keymap,
  lineNumbers,
  MatchDecorator,
  placeholder as placeholderExt,
  ViewPlugin,
  type ViewUpdate,
} from "@codemirror/view";
import { tags as t } from "@lezer/highlight";
import { trimUrl, URL_PATTERN } from "./bytes";
import { iconRects, type IconName } from "./icons";
import { shownValue } from "./variables";

/** A pixel icon as a DOM node, for places outside Svelte. */
function pixelIcon(name: IconName): HTMLElement {
  const wrapper = document.createElement("span");
  wrapper.className = "cm-gm-fold";
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 7 7");
  svg.setAttribute("width", "7");
  svg.setAttribute("height", "7");
  svg.setAttribute("fill", "currentColor");
  svg.setAttribute("shape-rendering", "crispEdges");
  for (const rect of iconRects(name)) {
    const node = document.createElementNS(ns, "rect");
    node.setAttribute("x", String(rect.x));
    node.setAttribute("y", String(rect.y));
    node.setAttribute("width", String(rect.w));
    node.setAttribute("height", "1");
    svg.append(node);
  }
  wrapper.append(svg);
  return wrapper;
}

export type EditorLanguage = "json" | "javascript" | "xml" | "html" | "css" | "text";

export function languageExtension(language: EditorLanguage, lint: boolean): Extension {
  switch (language) {
    case "json":
      return lint ? [json(), linter(jsonParseLinter(), { delay: 400 })] : json();
    case "javascript":
      return javascript();
    case "xml":
      return xml();
    case "html":
      return html();
    default:
      return [];
  }
}

const highlight = HighlightStyle.define([
  { tag: [t.propertyName], color: "var(--ink)" },
  { tag: [t.string, t.special(t.string)], color: "var(--ok)" },
  { tag: [t.number, t.integer, t.float], color: "var(--redirect)" },
  {
    tag: [t.bool, t.null, t.atom, t.keyword, t.controlKeyword, t.moduleKeyword],
    color: "var(--var-ink)",
  },
  { tag: [t.comment, t.lineComment, t.blockComment], color: "var(--ink-3)", fontStyle: "italic" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "var(--m-post)" },
  { tag: [t.tagName], color: "var(--m-put)" },
  { tag: [t.attributeName], color: "var(--m-post)" },
  { tag: [t.punctuation, t.separator, t.bracket], color: "var(--ink-3)" },
  { tag: [t.invalid], color: "var(--error)" },
]);

export const theme = EditorView.theme({
  "&": {
    color: "var(--ink)",
    backgroundColor: "var(--field)",
    fontSize: "var(--fs-code)",
    height: "100%",
  },
  ".cm-scroller": {
    fontFamily: "var(--font-code)",
    lineHeight: "1.55",
  },
  ".cm-content": { caretColor: "var(--accent)", padding: "6px 0" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--accent)", borderLeftWidth: "2px" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
    backgroundColor: "var(--t3) !important",
  },
  ".cm-gutters": {
    backgroundColor: "var(--field)",
    color: "var(--ink-3)",
    border: "none",
    borderRight: "1px solid var(--rule-3)",
  },
  ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--t1) 55%, transparent)" },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--ink)" },
  ".cm-lineNumbers .cm-gutterElement": {
    padding: "0 8px 0 10px",
    fontVariantNumeric: "tabular-nums",
  },
  ".cm-foldGutter .cm-gutterElement": {
    padding: "0 4px",
    color: "var(--ink-3)",
    cursor: "pointer",
  },
  ".cm-gm-fold": {
    display: "inline-grid",
    placeItems: "center",
    height: "100%",
    minHeight: "20px",
  },
  ".cm-matchingBracket": { backgroundColor: "var(--t2)", outline: "none" },
  ".cm-searchMatch": { backgroundColor: "var(--warn-wash)", outline: "1px solid var(--warn)" },
  ".cm-searchMatch-selected": { backgroundColor: "var(--accent-wash)" },
  ".cm-selectionMatch": { backgroundColor: "var(--t2)" },
  ".cm-placeholder": { color: "var(--ink-3)" },
  ".cm-gm-var": {
    color: "var(--var-ink)",
    backgroundColor: "var(--var-wash)",
    borderRadius: "0",
  },
  ".cm-gm-link": {
    textDecoration: "underline",
    textDecorationColor: "color-mix(in srgb, currentColor 50%, transparent)",
    textUnderlineOffset: "3px",
    cursor: "pointer",
  },
  ".cm-gm-link:hover": { textDecorationColor: "currentColor" },
  ".cm-gm-var-missing": {
    color: "var(--var-missing)",
    textDecoration: "underline dotted",
    textUnderlineOffset: "3px",
  },
  ".cm-tooltip": {
    border: "1px solid var(--rule)",
    backgroundColor: "var(--field)",
    color: "var(--ink)",
  },
  ".cm-gm-var-tip": {
    display: "grid",
    gap: "4px",
    justifyItems: "start",
    maxWidth: "320px",
    padding: "8px",
    fontFamily: "var(--font-ui)",
    fontSize: "var(--fs-ui)",
  },
  ".cm-gm-var-tip-head": {
    display: "flex",
    gap: "12px",
    justifyContent: "space-between",
    width: "100%",
    color: "var(--var-ink)",
    fontFamily: "var(--font-code)",
  },
  ".cm-gm-var-tip-scope": {
    color: "var(--ink-3)",
    fontFamily: "var(--font-bitmap)",
    fontSize: "var(--b1)",
    textTransform: "uppercase",
    WebkitFontSmoothing: "none",
  },
  ".cm-gm-var-tip-value": {
    maxHeight: "120px",
    overflow: "auto",
    fontFamily: "var(--font-code)",
    whiteSpace: "pre-wrap",
    wordBreak: "break-all",
  },
  ".cm-gm-var-tip button": {
    height: "22px",
    padding: "0 8px",
    border: "1px solid var(--rule)",
    background: "var(--field)",
    color: "var(--ink)",
    fontFamily: "var(--font-bitmap)",
    fontSize: "var(--b1)",
    textTransform: "uppercase",
  },
  ".cm-tooltip-autocomplete > ul": { fontFamily: "var(--font-code)", fontSize: "var(--fs-code)" },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": {
    backgroundColor: "var(--ink)",
    color: "var(--paper)",
  },
  ".cm-completionDetail": { color: "var(--ink-3)", fontStyle: "normal", marginLeft: "1em" },
  ".cm-panels": { backgroundColor: "var(--paper)", color: "var(--ink)" },
  ".cm-panels.cm-panels-bottom": { borderTop: "1px solid var(--rule-2)" },
  ".cm-panel.cm-search": { padding: "6px 8px", fontFamily: "var(--font-ui)" },
  ".cm-panel.cm-search input, .cm-panel.cm-search button": {
    font: "inherit",
    border: "1px solid var(--rule-2)",
    background: "var(--field)",
    color: "var(--ink)",
    borderRadius: "0",
    backgroundImage: "none",
  },
  ".cm-lintRange-error": { backgroundImage: "none", textDecoration: "underline wavy var(--error)" },
  ".cm-diagnostic-error": { borderLeft: "2px solid var(--error)" },
  ".cm-foldPlaceholder": {
    backgroundColor: "var(--t2)",
    border: "none",
    color: "var(--ink-2)",
  },
});

/** Marks `{{variables}}`; names not in `known` are flagged when `known` is set. */
export function variableMarks(known: Set<string> | null): Extension {
  const decorator = new MatchDecorator({
    regexp: /\{\{\s*([^{}\n"]+?)\s*\}\}/g,
    decoration: (match) => {
      const name = match[1] ?? "";
      const missing = known !== null && !name.startsWith("$") && !known.has(name);
      return Decoration.mark({
        class: missing ? "cm-gm-var cm-gm-var-missing" : "cm-gm-var",
      });
    },
  });
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = decorator.createDeco(view);
      }
      update(update: ViewUpdate) {
        this.decorations = decorator.updateDeco(update, this.decorations);
      }
    },
    { decorations: (plugin) => plugin.decorations },
  );
}

/** Underlines web addresses and opens one in a new tab when clicked. */
export function linkMarks(): Extension {
  const decorator = new MatchDecorator({
    regexp: new RegExp(URL_PATTERN.source, "g"),
    decorate: (add, from, _to, match) => {
      const url = trimUrl(match[0]);
      if (url.length <= "https://".length) return;
      add(
        from,
        from + url.length,
        Decoration.mark({
          class: "cm-gm-link",
          attributes: { "data-url": url, title: `${url}\nClick to open it in a new tab` },
        }),
      );
    },
  });
  return [
    ViewPlugin.fromClass(
      class {
        decorations: DecorationSet;
        constructor(view: EditorView) {
          this.decorations = decorator.createDeco(view);
        }
        update(update: ViewUpdate) {
          this.decorations = decorator.updateDeco(update, this.decorations);
        }
      },
      { decorations: (plugin) => plugin.decorations },
    ),
    EditorView.domEventHandlers({
      click: (event, view) => {
        const link = (event.target as HTMLElement | null)?.closest<HTMLElement>(".cm-gm-link");
        const url = link?.dataset.url;
        // A drag that selects text is not a click on the link.
        if (!url || event.button !== 0 || !view.state.selection.main.empty) return false;
        window.open(url, "_blank", "noopener,noreferrer");
        return true;
      },
    }),
  ];
}

export interface VariableSource {
  names: () => { label: string; detail?: string }[];
  /** What a name holds, for the card shown on hover. */
  describe?: (name: string) => VariableCard;
}

/** A variable's scope and value, or what a dynamic value makes. */
export type VariableCard =
  | { scope: string; value: string; secret: boolean }
  | { scope: "dynamic"; description: string }
  | null;

/** Shows a variable's value and scope when the pointer rests on it. */
export function variableHover(source: VariableSource): Extension {
  return hoverTooltip((view, pos) => {
    const line = view.state.doc.lineAt(pos);
    for (const match of line.text.matchAll(/\{\{\s*([^{}\n"]+?)\s*\}\}/g)) {
      const from = line.from + match.index;
      const to = from + match[0].length;
      if (pos < from || pos > to) continue;
      const name = match[1] ?? "";
      const card = source.describe?.(name) ?? null;
      return {
        pos: from,
        end: to,
        create: () => ({ dom: variableCard(name, card) }),
      };
    }
    return null;
  });
}

function variableCard(name: string, card: VariableCard): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "cm-gm-var-tip";
  const head = document.createElement("div");
  head.className = "cm-gm-var-tip-head";
  const title = document.createElement("span");
  title.textContent = name;
  const scope = document.createElement("span");
  scope.className = "cm-gm-var-tip-scope";
  scope.textContent = card ? card.scope : "not defined";
  head.append(title, scope);
  const body = document.createElement("div");
  body.className = "cm-gm-var-tip-value";
  dom.append(head, body);
  if (!card) {
    body.textContent = "No scope defines it, so it is sent as written.";
  } else if ("description" in card) {
    body.textContent = card.description;
  } else {
    let revealed = false;
    const show = () =>
      (body.textContent = shownValue(card.value, card.secret, revealed) || "(empty)");
    show();
    if (card.secret) {
      const reveal = document.createElement("button");
      reveal.type = "button";
      reveal.textContent = "Reveal";
      reveal.addEventListener("mousedown", (event) => event.preventDefault());
      reveal.addEventListener("click", () => {
        revealed = !revealed;
        reveal.textContent = revealed ? "Hide" : "Reveal";
        show();
      });
      dom.append(reveal);
    }
  }
  return dom;
}

/** Completes variable names after `{{`. */
export function variableCompletion(source: VariableSource) {
  return (context: CompletionContext): CompletionResult | null => {
    const before = context.matchBefore(/\{\{\s*[\w$.-]*$/);
    if (!before) return null;
    const start = before.text.indexOf("{{") + 2;
    const leading = before.text.slice(start).length - before.text.slice(start).trimStart().length;
    return {
      from: before.from + start + leading,
      options: source.names().map((item) => ({
        label: item.label,
        detail: item.detail,
        type: item.label.startsWith("$") ? "function" : "variable",
        apply: (view, _completion, from, to) => {
          const after = view.state.sliceDoc(to, to + 2);
          const insert = after === "}}" ? item.label : `${item.label}}}`;
          view.dispatch({
            changes: { from, to, insert },
            selection: { anchor: from + insert.length + (after === "}}" ? 2 : 0) },
          });
        },
      })),
      validFor: /^[\w$.-]*$/,
    };
  };
}

/** Completes with what the editor's language offers, such as a GraphQL schema's names. */
const languageCompletion: CompletionSource = (context) => {
  for (const source of context.state.languageDataAt<CompletionSource>(
    "autocomplete",
    context.pos,
  )) {
    const result = source(context);
    if (result) return result;
  }
  return null;
};

export interface SetupOptions {
  /** Also offer the completions of the editor's language. */
  languageCompletion?: boolean;
  singleLine: boolean;
  lineNumbers: boolean;
  placeholder: string;
  completion: VariableSource;
  onSubmit: () => void;
  onPaste: (text: string) => boolean;
}

export function baseExtensions(options: SetupOptions): Extension[] {
  const extensions: Extension[] = [
    theme,
    syntaxHighlighting(highlight),
    history(),
    drawSelection(),
    highlightSelectionMatches(),
    closeBrackets(),
    autocompletion({
      override: options.languageCompletion
        ? [variableCompletion(options.completion), languageCompletion]
        : [variableCompletion(options.completion)],
      icons: false,
      activateOnTyping: true,
    }),
    variableHover(options.completion),
    EditorView.domEventHandlers({
      paste: (event) => {
        const text = event.clipboardData?.getData("text/plain") ?? "";
        if (text && options.onPaste(text)) {
          event.preventDefault();
          return true;
        }
        return false;
      },
    }),
    keymap.of([
      { key: "Mod-Enter", run: () => (options.onSubmit(), true) },
      ...closeBracketsKeymap,
      ...completionKeymap,
      ...historyKeymap,
    ]),
  ];
  if (options.placeholder) extensions.push(placeholderExt(options.placeholder));
  if (options.singleLine) {
    extensions.push(
      EditorState.transactionFilter.of((tr) => {
        if (!tr.docChanged || tr.newDoc.lines <= 1) return tr;
        const text = tr.newDoc.toString().replace(/\s*[\r\n]+\s*/g, "");
        return [tr, { changes: { from: 0, to: tr.newDoc.length, insert: text }, sequential: true }];
      }),
      keymap.of([
        { key: "Enter", run: () => (options.onSubmit(), true) },
        ...defaultKeymap.filter((binding) => binding.key !== "Enter"),
      ]),
      EditorView.contentAttributes.of({ "aria-multiline": "false" }),
    );
  } else {
    extensions.push(
      search({ top: false }),
      indentOnInput(),
      bracketMatching(),
      highlightActiveLine(),
      keymap.of([...defaultKeymap, ...searchKeymap, ...foldKeymap, indentWithTab]),
    );
    if (options.lineNumbers) {
      extensions.push(
        lineNumbers(),
        highlightActiveLineGutter(),
        foldGutter({ markerDOM: (open) => pixelIcon(open ? "chevronDown" : "chevronRight") }),
      );
    }
  }
  return extensions;
}
