// Turning responses into saved examples and examples back into things
// the editors and viewers take.

import type { Draft, Example, Execution, Payload, Request, Response } from "./types";

/** A name for a new example that no other example of the request has:
 * the status line, numbered when it is taken. */
export function exampleName(response: Response, taken: string[]): string {
  const base = `${response.status} ${response.status_text}`.trim();
  if (!taken.includes(base)) return base;
  let n = 2;
  while (taken.includes(`${base} ${n}`)) n++;
  return `${base} ${n}`;
}

/** The example a response makes, with the request as written in the
 * editor (variables unresolved, so no secret values end up in the file),
 * and the body to store. */
export function exampleFrom(
  draft: Draft,
  execution: Execution,
  name: string,
): { example: Example; body: Payload } | null {
  const response = execution.response;
  if (!response) return null;
  const body =
    draft.body.type === "json" || draft.body.type === "xml" || draft.body.type === "text"
      ? draft.body.content
      : draft.body.type === "graphql"
        ? draft.body.query
        : "";
  return {
    example: {
      name,
      status: response.status,
      headers: response.headers.map((h) => ({ name: h.name, value: h.value })),
      request: {
        method: draft.method,
        url: draft.url,
        headers: draft.headers
          .filter((h) => h.enabled !== false && h.name.trim() !== "")
          .map((h) => ({ name: h.name, value: h.value })),
        body,
      },
    },
    body: response.body,
  };
}

/** An example as a response, for the body viewer. */
export function asResponse(example: Example, body: Payload, size: number): Response {
  return {
    status: example.status,
    status_text: "",
    http_version: "",
    url: example.request?.url ?? "",
    headers: (example.headers ?? []).map((h) => ({ name: h.name, value: h.value })),
    body,
    truncated: false,
    time_ms: 0,
    headers_ms: 0,
    size: { headers: 0, body: size },
  };
}

/** A new, unsaved request that sends what got an example. */
export function requestFrom(example: Example, saved: Request): Request {
  const sent = example.request ?? {};
  const text = sent.body ?? "";
  let body: Request["body"] = { type: "none" };
  if (text.trim() !== "") {
    try {
      JSON.parse(text);
      body = { type: "json", content: text };
    } catch {
      body = { type: "text", content: text };
    }
  }
  return {
    name: `${saved.name} (${example.name})`,
    method: sent.method || saved.method,
    url: sent.url || saved.url,
    headers: sent.headers ?? saved.headers ?? [],
    body: sent.url ? body : (saved.body ?? body),
  };
}

/** The size of a body in bytes. */
export function payloadSize(payload: Payload): number {
  if ("text" in payload) return new TextEncoder().encode(payload.text).length;
  return (
    Math.floor((payload.base64.length * 3) / 4) - (payload.base64.match(/=+$/)?.[0].length ?? 0)
  );
}
