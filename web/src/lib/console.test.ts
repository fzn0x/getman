import { describe, expect, it } from "vitest";
import {
  BODY_CAP,
  MAX_ROWS,
  NO_FILTER,
  capBody,
  filterRows,
  pushRow,
  rowFromExecution,
  rowFromGrpc,
  rowFromRealtime,
  rowText,
  windowOf,
  type ConsoleRow,
} from "./console";
import type { Execution } from "./types";

function execution(patch: Partial<Execution> = {}): Execution {
  return {
    path: null,
    name: "Create user",
    request: {
      method: "POST",
      url: "https://api.test/users",
      headers: [{ name: "Authorization", value: "Bearer abc" }],
      body: { type: "bytes", data: { text: '{"a":1}' } },
    },
    response: {
      status: 201,
      status_text: "Created",
      http_version: "HTTP/1.1",
      url: "https://api.test/users",
      headers: [{ name: "Content-Type", value: "application/json" }],
      body: { text: '{"id":7}' },
      truncated: false,
      time_ms: 42,
      headers_ms: 30,
      size: { headers: 100, body: 8 },
    },
    error: null,
    tests: [],
    checks: [],
    logs: [{ level: "info", message: "hello", origin: "post-response" }],
    script_errors: [],
    changes: [],
    missing_variables: [],
    warnings: [],
    skipped: false,
    next_request: null,
    ...patch,
  };
}

function row(patch: Partial<ConsoleRow> = {}): ConsoleRow {
  return { id: 1, at: 0, ...rowFromExecution(execution(), false), ...patch };
}

describe("rows", () => {
  it("summarise a send", () => {
    const fresh = rowFromExecution(execution(), false);
    expect(fresh).toMatchObject({
      kind: "http",
      method: "POST",
      url: "https://api.test/users",
      status: 201,
      timeMs: 42,
      size: 108,
      requestBody: '{"a":1}',
      responseBody: '{"id":7}',
      failed: false,
    });
    expect(fresh.logs).toHaveLength(1);
    expect(rowFromExecution(execution(), true).kind).toBe("graphql");
  });

  it("fail on errors, bad statuses, scripts and checks", () => {
    const base = execution();
    const response = base.response!;
    expect(
      rowFromExecution({ ...base, response: { ...response, status: 404 } }, false).failed,
    ).toBe(true);
    expect(rowFromExecution({ ...base, error: "refused", response: null }, false)).toMatchObject({
      failed: true,
      problems: ["refused"],
    });
    expect(
      rowFromExecution({ ...base, checks: [{ name: "status == 200", passed: false }] }, false)
        .problems,
    ).toEqual(["Failed: status == 200"]);
    expect(
      rowFromExecution({ ...base, script_errors: [{ origin: "pre", message: "boom" }] }, false)
        .failed,
    ).toBe(true);
    expect(rowFromExecution({ ...base, missing_variables: ["id"] }, false).problems).toEqual([
      "Not defined: {{id}}",
    ]);
  });

  it("cap bodies", () => {
    expect(capBody("short")).toEqual({ text: "short", cut: false });
    const long = "x".repeat(BODY_CAP + 10);
    const capped = capBody(long);
    expect(capped.text).toHaveLength(BODY_CAP);
    expect(capped.cut).toBe(true);
    const big = execution();
    big.response!.body = { text: long };
    const made = rowFromExecution(big, false);
    expect(made.responseBody).toHaveLength(BODY_CAP);
    expect(made.responseBodyCut).toBe(true);
  });

  it("summarise gRPC calls and realtime connections", () => {
    const grpc = rowFromGrpc(execution({ name: "Get" }), {
      method: "shop.Orders/Get",
      call_type: "unary",
      address: "localhost:50051",
      tls: false,
      status: { code: 5, name: "NOT_FOUND", message: "no order" },
      headers: [],
      trailers: [],
      messages: [
        { direction: "sent", message: { id: 1 }, time_ms: 1, size: 8 },
        { direction: "received", message: { ok: false }, time_ms: 2, size: 12 },
      ],
      time_ms: 9,
      headers_ms: 3,
    });
    expect(grpc).toMatchObject({ kind: "grpc", failed: true, status: 5, size: 20 });
    expect(grpc.problems[0]).toBe("NOT_FOUND: no order");
    expect(grpc.requestBody).toContain('"id": 1');

    const live = rowFromRealtime({
      name: "Feed",
      method: "GET",
      url: "wss://x.test/feed",
      headers: [],
      status: 101,
      responseHeaders: [],
      timeMs: 5000,
      sent: 2,
      received: 9,
      closed: { code: 1000, reason: "bye", by_client: true },
      error: "",
    });
    expect(live.kind).toBe("realtime");
    expect(live.summary).toBe("Closed by you with code 1000 (bye). 2 sent, 9 received.");
    expect(live.failed).toBe(false);
  });

  it("copy as text", () => {
    const text = rowText(row());
    expect(text).toContain("POST https://api.test/users");
    expect(text).toContain("Status: 201 Created");
    expect(text).toContain("Authorization: Bearer abc");
    expect(text).toContain('Response body:\n{"id":7}');
  });
});

describe("filtering", () => {
  const rows = [
    row({ id: 1, url: "https://api.test/users", method: "GET", status: 200 }),
    row({ id: 2, url: "https://api.test/orders", method: "POST", status: 500, failed: true }),
    row({ id: 3, kind: "grpc", url: "localhost:50051", method: "shop.Orders/Get", failed: false }),
  ];

  it("shows everything without a filter", () => {
    expect(filterRows(rows, NO_FILTER)).toBe(rows);
  });

  it("needs every word to match", () => {
    expect(filterRows(rows, { ...NO_FILTER, text: "orders" }).map((r) => r.id)).toEqual([2, 3]);
    expect(filterRows(rows, { ...NO_FILTER, text: "post 500" }).map((r) => r.id)).toEqual([2]);
    expect(filterRows(rows, { ...NO_FILTER, text: "USERS" }).map((r) => r.id)).toEqual([1]);
  });

  it("can show errors only, and hide kinds", () => {
    expect(filterRows(rows, { ...NO_FILTER, errorsOnly: true }).map((r) => r.id)).toEqual([2]);
    expect(filterRows(rows, { ...NO_FILTER, hidden: ["grpc"] }).map((r) => r.id)).toEqual([1, 2]);
  });
});

describe("keeping rows", () => {
  it("drops the oldest beyond the limit", () => {
    const rows: ConsoleRow[] = [];
    let dropped = 0;
    for (let id = 1; id <= MAX_ROWS + 5; id++) dropped += pushRow(rows, row({ id }));
    expect(rows).toHaveLength(MAX_ROWS);
    expect(rows[0]?.id).toBe(6);
    expect(dropped).toBe(5);
  });
});

describe("windowOf", () => {
  it("draws a screenful of thousands of rows", () => {
    const view = windowOf(5000, 24, 24 * 1000, 240, 5);
    expect(view.from).toBe(995);
    expect(view.to).toBe(1015);
    expect(view.before).toBe(995 * 24);
    expect(view.after).toBe((5000 - 1015) * 24);
  });

  it("stays inside the list", () => {
    expect(windowOf(3, 24, 0, 240)).toEqual({ from: 0, to: 3, before: 0, after: 0 });
    expect(windowOf(0, 24, 500, 240)).toEqual({ from: 0, to: 0, before: 0, after: 0 });
    const end = windowOf(100, 24, 24 * 95, 240, 2);
    expect(end.to).toBe(100);
    expect(end.after).toBe(0);
  });
});
