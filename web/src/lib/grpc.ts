// gRPC helpers that need no server: names for call types, the schema
// sources a workspace offers, and messages written one after another.

import type { CallType, GrpcMethod, GrpcService } from "./types";

export const CALL_TYPES: Record<CallType, string> = {
  unary: "Unary",
  server_streaming: "Server stream",
  client_streaming: "Client stream",
  bidi_streaming: "Bidirectional stream",
};

/** Whether the client sends a stream of messages. */
export function clientStreams(type: CallType | null | undefined): boolean {
  return type === "client_streaming" || type === "bidi_streaming";
}

/** Whether the server answers with a stream of messages. */
export function serverStreams(type: CallType | null | undefined): boolean {
  return type === "server_streaming" || type === "bidi_streaming";
}

/** The mark a request shows in lists: its HTTP method, or its kind. */
export function requestMark(request: { kind?: string; method: string }): string {
  return request.kind && request.kind !== "http" ? request.kind.toUpperCase() : request.method;
}

export interface SchemaSource {
  value: string;
  label: string;
}

/** Every .proto file, and every folder that holds some, to choose from. */
export function schemaSources(files: string[]): SchemaSource[] {
  const folders = new Set<string>();
  for (const file of files) {
    const parts = file.split("/");
    for (let i = 1; i < parts.length; i++) folders.add(parts.slice(0, i).join("/"));
  }
  return [
    ...[...folders].sort().map((folder) => ({ value: folder, label: `${folder}/ (every .proto)` })),
    ...[...files].sort().map((file) => ({ value: file, label: file })),
  ];
}

/** The method a request names, as `package.Service/Method`. */
export function findMethod(services: GrpcService[], path: string): GrpcMethod | undefined {
  const wanted = path.trim().replace(/^\//, "");
  for (const service of services) {
    const found = service.methods.find((m) => m.path === wanted);
    if (found) return found;
  }
  return undefined;
}

/** The JSON values in a text, one after another, as source slices. */
function splitValues(text: string): string[] {
  const values: string[] = [];
  let depth = 0;
  let start = -1;
  let inString = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i]!;
    if (inString) {
      if (c === "\\") i++;
      else if (c === '"') {
        inString = false;
        if (depth === 0) {
          values.push(text.slice(start, i + 1));
          start = -1;
        }
      }
      continue;
    }
    if (/\s/.test(c)) {
      if (depth === 0 && start >= 0) {
        values.push(text.slice(start, i));
        start = -1;
      }
      continue;
    }
    if (start < 0) start = i;
    if (c === '"') inString = true;
    else if (c === "{" || c === "[") depth++;
    else if (c === "}" || c === "]") {
      depth--;
      if (depth < 0) throw new Error("unbalanced brackets");
      if (depth === 0) {
        values.push(text.slice(start, i + 1));
        start = -1;
      }
    }
  }
  if (inString || depth !== 0) throw new Error("the last message is not finished");
  if (start >= 0) values.push(text.slice(start));
  return values;
}

/**
 * Stands in for `{{variables}}` so the text parses as JSON: inside
 * strings with a plain word, elsewhere with a string.
 */
function mask(text: string): { masked: string; restore: (json: string) => string } {
  const names: string[] = [];
  let masked = "";
  let inString = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i]!;
    if (c === "{" && text[i + 1] === "{") {
      const end = text.indexOf("}}", i + 2);
      if (end > 0 && !text.slice(i + 2, end).includes("{")) {
        names.push(text.slice(i, end + 2));
        const n = names.length - 1;
        masked += inString ? `__gm_str_${n}__` : `"__gm_bare_${n}__"`;
        i = end + 1;
        continue;
      }
    }
    if (inString && c === "\\") {
      masked += c + (text[i + 1] ?? "");
      i++;
      continue;
    }
    if (c === '"') inString = !inString;
    masked += c;
  }
  const restore = (json: string) =>
    json
      .replace(/"__gm_bare_(\d+)__"/g, (_, n: string) => names[Number(n)] ?? "")
      .replace(/__gm_str_(\d+)__/g, (_, n: string) => names[Number(n)] ?? "");
  return { masked, restore };
}

/** How many messages a text holds, or `null` when it is not JSON. */
export function countMessages(text: string): number | null {
  try {
    const values = splitValues(mask(text).masked);
    for (const value of values) JSON.parse(value);
    return values.length;
  } catch {
    return null;
  }
}

/** Each message indented on its own, variables kept; `null` when not JSON. */
export function formatMessages(text: string): string | null {
  try {
    const { masked, restore } = mask(text);
    const values = splitValues(masked).map((value) =>
      restore(JSON.stringify(JSON.parse(value), null, 2)),
    );
    return values.join("\n") + (values.length ? "\n" : "");
  } catch {
    return null;
  }
}

/** A method's example message, ready for the editor. */
export function exampleText(method: GrpcMethod): string {
  return `${JSON.stringify(method.example, null, 2)}\n`;
}
