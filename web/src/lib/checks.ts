// Checks are stored as lines such as `status == 200`; the editor shows
// them as rows of target, operator and value.

export const OPERATORS = [
  "==",
  "!=",
  "<",
  "<=",
  ">",
  ">=",
  "contains",
  "!contains",
  "startsWith",
  "endsWith",
  "matches",
  "!matches",
  "exists",
  "!exists",
  "is",
  "in",
  "not in",
  "between",
  "is empty",
  "is not empty",
  "is truthy",
  "is falsy",
] as const;

/** Operators that take no value. */
export const UNARY = new Set([
  "exists",
  "!exists",
  "is empty",
  "is not empty",
  "is truthy",
  "is falsy",
]);

/** Operators written with more than one word, longest first. */
const MULTI_WORD = ["is not empty", "is empty", "is truthy", "is falsy", "not in"];

/** What each operator wants as its value, for the placeholder. */
export const VALUE_HINTS: Record<string, string> = {
  in: "200, 201 or [200, 201]",
  "not in": "400, 404 or [400, 404]",
  between: "1 and 10",
};

export interface CheckRow {
  target: string;
  operator: string;
  value: string;
}

/** Splits a check line into its parts. Unparseable lines keep their text as the target. */
export function parseCheck(line: string): CheckRow {
  const text = line.trim();
  let depth = 0;
  let quote = "";
  let split = text.length;
  for (let i = 0; i < text.length; i++) {
    const c = text[i]!;
    if (quote) {
      if (c === quote) quote = "";
    } else if ((c === '"' || c === "'") && depth > 0) quote = c;
    else if (c === "[") depth++;
    else if (c === "]") depth = Math.max(0, depth - 1);
    else if (/\s/.test(c) && depth === 0) {
      split = i;
      break;
    }
  }
  let target = text.slice(0, split);
  let rest = text.slice(split).trim();
  for (const symbol of ["==", "!=", "<=", ">=", "<", ">"]) {
    const at = target.indexOf(symbol);
    if (at > 0) {
      rest = `${symbol} ${target.slice(at + symbol.length)} ${rest}`.trim();
      target = target.slice(0, at);
      break;
    }
  }
  const words = rest.split(/\s+/);
  for (const multi of MULTI_WORD) {
    const count = multi.split(" ").length;
    if (words.slice(0, count).join(" ") === multi) {
      return {
        target,
        operator: multi,
        value: UNARY.has(multi) ? "" : words.slice(count).join(" "),
      };
    }
  }
  const space = rest.search(/\s/);
  const operator = space < 0 ? rest : rest.slice(0, space);
  const value = space < 0 ? "" : rest.slice(space).trim();
  return { target, operator: operator || "==", value };
}

export function formatCheck(row: CheckRow): string {
  const parts = [row.target.trim(), row.operator.trim()];
  if (!UNARY.has(row.operator) && row.value.trim() !== "") parts.push(row.value.trim());
  return parts.join(" ");
}

/** Targets offered in the editor, with what they mean. */
export const TARGETS: { target: string; hint: string }[] = [
  { target: "status", hint: "Status code" },
  { target: "time", hint: "Response time in ms" },
  { target: "size", hint: "Body size in bytes" },
  { target: "body", hint: "Body as text" },
  { target: "header.content-type", hint: "A response header" },
  { target: "cookie.", hint: "A cookie set by the response" },
  { target: "json", hint: "The whole JSON body" },
  { target: "json.", hint: "A JSON path, like json.data[0].id" },
  { target: "url", hint: "Final URL after redirects" },
];
