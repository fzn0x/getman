// The same small path language as checks and captures: `.name`, `[0]`,
// `[-1]`, `["quoted key"]` and `.length`, with or without a leading
// `json`.

type Step = { key: string } | { index: number };

export function parsePath(path: string): Step[] | string {
  let text = path.trim();
  if (text.startsWith("json")) text = text.slice(4);
  if (text !== "" && !text.startsWith(".") && !text.startsWith("[")) text = `.${text}`;
  const steps: Step[] = [];
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (c === ".") {
      let j = i + 1;
      while (j < text.length && text[j] !== "." && text[j] !== "[") j++;
      const key = text.slice(i + 1, j);
      if (!key) return "a name is missing after a dot";
      steps.push({ key });
      i = j;
    } else if (c === "[") {
      const close = text.indexOf("]", i);
      if (close < 0) return "a ] is missing";
      const inner = text.slice(i + 1, close).trim();
      const quoted = /^(["'])(.*)\1$/.exec(inner);
      if (quoted) steps.push({ key: quoted[2] ?? "" });
      else if (/^-?\d+$/.test(inner)) steps.push({ index: Number(inner) });
      else return `${inner || "nothing"} is not an index`;
      i = close + 1;
    } else {
      return `unexpected ${c}`;
    }
  }
  return steps;
}

export function walk(value: unknown, steps: Step[]): { found: boolean; value: unknown } {
  let current = value;
  for (const step of steps) {
    if ("index" in step) {
      if (!Array.isArray(current)) return { found: false, value: undefined };
      const at = step.index < 0 ? current.length + step.index : step.index;
      if (at < 0 || at >= current.length) return { found: false, value: undefined };
      current = current[at];
    } else if (step.key === "length" && (Array.isArray(current) || typeof current === "string")) {
      current = current.length;
    } else if (current !== null && typeof current === "object" && step.key in current) {
      current = (current as Record<string, unknown>)[step.key];
    } else {
      return { found: false, value: undefined };
    }
  }
  return { found: true, value: current };
}
