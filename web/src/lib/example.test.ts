import { describe, expect, it } from "vitest";
import { newDraft } from "./draft";
import { asResponse, exampleFrom, exampleName, payloadSize, requestFrom } from "./example";
import type { Execution, Response } from "./types";

const response: Response = {
  status: 201,
  status_text: "Created",
  http_version: "HTTP/1.1",
  url: "https://api.example.com/orders",
  headers: [{ name: "Content-Type", value: "application/json" }],
  body: { text: '{"id": 1}' },
  truncated: false,
  time_ms: 12,
  headers_ms: 10,
  size: { headers: 40, body: 9 },
};

const execution: Execution = {
  path: "orders/create.yaml",
  name: "Create",
  request: {
    method: "POST",
    url: "https://api.example.com/orders",
    headers: [{ name: "Authorization", value: "Bearer s3cret" }],
    body: { type: "none" },
  },
  response,
  error: null,
  tests: [],
  checks: [],
  logs: [],
  script_errors: [],
  changes: [],
  missing_variables: [],
  warnings: [],
  skipped: false,
  next_request: null,
};

describe("examples", () => {
  it("are named after the status line, numbered when taken", () => {
    expect(exampleName(response, [])).toBe("201 Created");
    expect(exampleName(response, ["201 Created", "201 Created 2"])).toBe("201 Created 3");
  });

  it("keep the request as written, not as sent", () => {
    const draft = newDraft("Create");
    draft.method = "POST";
    draft.url = "{{baseUrl}}/orders";
    draft.headers = [
      { name: "Authorization", value: "Bearer {{token}}", enabled: true },
      { name: "X-Off", value: "1", enabled: false },
    ];
    draft.body = { type: "json", content: '{"sku": "{{sku}}"}' };
    const made = exampleFrom(draft, execution, "Created")!;
    expect(made.example.status).toBe(201);
    expect(made.example.request?.url).toBe("{{baseUrl}}/orders");
    expect(made.example.request?.headers).toEqual([
      { name: "Authorization", value: "Bearer {{token}}" },
    ]);
    expect(JSON.stringify(made)).not.toContain("s3cret");
    expect(made.body).toEqual({ text: '{"id": 1}' });
    expect(exampleFrom(draft, { ...execution, response: null }, "x")).toBeNull();
  });

  it("open as requests and show as responses", () => {
    const seeded = requestFrom(
      { name: "Bad", status: 400, request: { method: "POST", url: "/o", body: "nope" } },
      { name: "Create", method: "PUT", url: "/x" },
    );
    expect(seeded).toMatchObject({
      name: "Create (Bad)",
      method: "POST",
      url: "/o",
      body: { type: "text", content: "nope" },
    });
    const plain = requestFrom(
      { name: "Ok", status: 200 },
      { name: "Get", method: "GET", url: "/y" },
    );
    expect(plain.url).toBe("/y");
    expect(asResponse({ name: "Ok", status: 204 }, { text: "" }, 0).status).toBe(204);
    expect(payloadSize({ text: "é" })).toBe(2);
    expect(payloadSize({ base64: "iVBORw==" })).toBe(4);
  });
});
