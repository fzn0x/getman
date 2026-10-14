// Numbers, sizes, times and bodies, for people.

import type { Payload } from "./types";

export function bytes(n: number): string {
  if (n < 1000) return `${n} B`;
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)} kB`;
  return `${(n / 1_000_000).toFixed(1)} MB`;
}

export function millis(ms: number): string {
  if (ms < 1) return "<1 ms";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

export function ago(at: number, now = Date.now()): string {
  const seconds = Math.max(0, Math.round((now - at) / 1000));
  if (seconds < 45) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  if (days < 7) return `${days} d ago`;
  return new Date(at).toLocaleDateString();
}

/** The body as text, decoding Base64 as UTF-8 where possible. */
export function payloadText(payload: Payload): string {
  if ("text" in payload) return payload.text;
  try {
    const binary = atob(payload.base64);
    const array = Uint8Array.from(binary, (c) => c.charCodeAt(0));
    return new TextDecoder("utf-8", { fatal: false }).decode(array);
  } catch {
    return "";
  }
}

export function payloadBytes(payload: Payload): Uint8Array {
  if ("text" in payload) return new TextEncoder().encode(payload.text);
  const binary = atob(payload.base64);
  return Uint8Array.from(binary, (c) => c.charCodeAt(0));
}

export type BodyKind =
  "json" | "xml" | "html" | "javascript" | "css" | "image" | "pdf" | "text" | "binary";

/** What a response body is, from its content type and contents. */
export function bodyKind(contentType: string | undefined, payload: Payload): BodyKind {
  const type = (contentType ?? "").toLowerCase();
  if (type.startsWith("image/")) return "image";
  if (type.includes("pdf")) return "pdf";
  if (type.includes("json")) return "json";
  if (type.includes("html")) return "html";
  if (type.includes("xml")) return "xml";
  if (type.includes("javascript") || type.includes("ecmascript")) return "javascript";
  if (type.includes("css")) return "css";
  if ("base64" in payload) return "binary";
  const text = payload.text.trimStart();
  if (text.startsWith("{") || text.startsWith("[")) {
    try {
      JSON.parse(payload.text);
      return "json";
    } catch {
      return "text";
    }
  }
  if (text.startsWith("<!") || /^<html/i.test(text)) return "html";
  if (text.startsWith("<")) return "xml";
  return "text";
}

/** Indents JSON for reading; leaves anything else as it is. */
export function prettyJson(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

export function statusClass(status: number): "ok" | "redirect" | "client" | "server" | "info" {
  if (status >= 500) return "server";
  if (status >= 400) return "client";
  if (status >= 300) return "redirect";
  if (status >= 200) return "ok";
  return "info";
}

export function methodClass(method: string): string {
  const m = method.toUpperCase();
  return ["GET", "POST", "PUT", "PATCH", "DELETE"].includes(m) ? m.toLowerCase() : "other";
}

/** A short form of a method for narrow places. */
export function methodShort(method: string): string {
  const m = method.toUpperCase();
  if (m === "DELETE") return "DEL";
  if (m === "OPTIONS") return "OPT";
  if (m === "PATCH") return "PAT";
  return m.slice(0, 4);
}
