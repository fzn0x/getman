// Turning stored requests into complete drafts the editors can bind to,
// and back into the compact form saved to disk.

import type { Auth, Body, Draft, Request, Settings } from "./types";

/** A deep copy that also works on Svelte's reactive proxies. */
export function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

export const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] as const;

export function newDraft(name = "Untitled request"): Draft {
  return toDraft({ name, method: "GET", url: "" });
}

export function toDraft(request: Request): Draft {
  return {
    name: request.name ?? "",
    method: (request.method || "GET").toUpperCase(),
    url: request.url ?? "",
    description: request.description ?? "",
    path_params: { ...(request.path_params ?? {}) },
    disabled_params: (request.disabled_params ?? []).map((p) => ({ ...p, enabled: false })),
    headers: (request.headers ?? []).map((h) => ({ ...h, enabled: h.enabled !== false })),
    body: clone(request.body ?? { type: "none" }) as Body,
    auth: clone(request.auth ?? { type: "inherit" }) as Auth,
    variables: { ...(request.variables ?? {}) },
    scripts: {
      pre_request: request.scripts?.pre_request ?? "",
      post_response: request.scripts?.post_response ?? "",
    },
    checks: [...(request.checks ?? [])],
    captures: { ...(request.captures ?? {}) },
    settings: { ...(request.settings ?? {}) },
  };
}

/** The draft as a request to save or send, without empty rows. */
export function toRequest(draft: Draft): Request {
  const filled = <T extends { name: string; value?: string }>(rows: T[]) =>
    rows.filter((r) => r.name.trim() !== "" || (r.value ?? "").trim() !== "");
  const body = clone(draft.body);
  if (body.type === "form") body.fields = filled(body.fields);
  if (body.type === "multipart") body.fields = filled(body.fields);
  const settings: Settings = {};
  for (const [key, value] of Object.entries(draft.settings)) {
    if (value !== null && value !== undefined && value !== "") {
      (settings as Record<string, unknown>)[key] = value;
    }
  }
  return {
    name: draft.name,
    method: draft.method.trim().toUpperCase() || "GET",
    url: draft.url,
    description: draft.description,
    path_params: { ...draft.path_params },
    disabled_params: filled(draft.disabled_params).map((p) => ({ ...p, enabled: false })),
    headers: filled(draft.headers),
    body,
    auth: clone(draft.auth),
    variables: Object.fromEntries(
      Object.entries(draft.variables).filter(([name]) => name.trim() !== ""),
    ),
    scripts: { ...draft.scripts },
    checks: draft.checks.filter((c) => c.trim() !== ""),
    captures: Object.fromEntries(
      Object.entries(draft.captures).filter(([name, target]) => name.trim() && target.trim()),
    ),
    settings,
  };
}

/** A stable fingerprint of what would be saved, for telling a draft is dirty. */
export function fingerprint(draft: Draft): string {
  return JSON.stringify(toRequest(draft));
}

export function emptyAuth(type: Auth["type"]): Auth {
  switch (type) {
    case "basic":
    case "digest":
      return { type, username: "", password: "" };
    case "bearer":
      return { type, token: "" };
    case "api_key":
      return { type, key: "X-API-Key", value: "", in: "header" };
    case "oauth2":
      return {
        type,
        grant: "client_credentials",
        token_url: "",
        client_id: "",
        client_auth: "basic",
      };
    default:
      return { type };
  }
}

/** Changes a body's type, carrying text across text-like types. */
export function convertBody(body: Body, type: Body["type"]): Body {
  const text =
    body.type === "json" || body.type === "xml" || body.type === "text"
      ? body.content
      : body.type === "graphql"
        ? body.query
        : "";
  switch (type) {
    case "json":
      return { type, content: text || "{\n  \n}" };
    case "xml":
    case "text":
      return { type, content: text };
    case "form":
      return { type, fields: body.type === "form" ? body.fields : [] };
    case "multipart":
      return { type, fields: body.type === "multipart" ? body.fields : [] };
    case "binary":
      return { type, file: body.type === "binary" ? body.file : "" };
    case "graphql":
      return { type, query: text, variables: "" };
    default:
      return { type: "none" };
  }
}

export const BODY_TYPES: { type: Body["type"]; label: string }[] = [
  { type: "none", label: "None" },
  { type: "json", label: "JSON" },
  { type: "xml", label: "XML" },
  { type: "text", label: "Text" },
  { type: "form", label: "Form" },
  { type: "multipart", label: "Multipart" },
  { type: "binary", label: "File" },
  { type: "graphql", label: "GraphQL" },
];

export const AUTH_TYPES: { type: Auth["type"]; label: string }[] = [
  { type: "inherit", label: "Inherit from folder" },
  { type: "none", label: "No auth" },
  { type: "bearer", label: "Bearer token" },
  { type: "basic", label: "Basic" },
  { type: "api_key", label: "API key" },
  { type: "digest", label: "Digest" },
  { type: "oauth2", label: "OAuth 2.0" },
];
