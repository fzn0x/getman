import { describe, expect, it } from "vitest";
import { closeTargets, findMoved, moveTo, pushClosed, readSession, type SavedTab } from "./session";

describe("readSession", () => {
  it("reads the list of paths older versions kept", () => {
    const session = readSession({ open: ["r:a.yaml", "f:users", "e:"], active: "f:users" });
    expect(session.tabs.map((t) => t.kind)).toEqual(["request", "folder", "environments"]);
    expect(session.active).toBe(1);
  });

  it("ignores what it cannot read", () => {
    expect(readSession(null).tabs).toEqual([]);
    expect(
      readSession({ version: 2, tabs: [1, null, { kind: "runner", target: "" }] }).tabs,
    ).toEqual([{ kind: "runner", target: "" }]);
  });
});

describe("findMoved", () => {
  const requests = [
    { path: "users/list.yaml", name: "List users", method: "GET" },
    { path: "users/create-user.yaml", name: "Create", method: "POST" },
    { path: "admin/ping.yaml", name: "Ping", method: "GET" },
    { path: "login.yaml", name: "Login", method: "POST" },
    { path: "old/login.yaml", name: "Login", method: "POST" },
  ];

  it("finds a renamed file by its name in the same folder", () => {
    expect(
      findMoved({ path: "users/all-users.yaml", name: "List users", method: "GET" }, requests),
    ).toBe("users/list.yaml");
  });

  it("finds a moved file by its file name", () => {
    expect(findMoved({ path: "ping.yaml", name: "Pong", method: "GET" }, requests)).toBe(
      "admin/ping.yaml",
    );
  });

  it("gives up when it is not sure", () => {
    expect(findMoved({ path: "x/login.yaml", name: "Other", method: "GET" }, requests)).toBeNull();
    expect(findMoved({ path: "gone.yaml", name: "Gone", method: "GET" }, requests)).toBeNull();
    expect(
      findMoved(
        { path: "users/all-users.yaml", name: "List users", method: "GET" },
        requests,
        new Set(["users/list.yaml"]),
      ),
    ).toBeNull();
  });
});

describe("tab lists", () => {
  const ids = ["a", "b", "c", "d"];

  it("picks the tabs a bulk close removes", () => {
    expect(closeTargets(ids, "b", "others")).toEqual(["a", "c", "d"]);
    expect(closeTargets(ids, "b", "right")).toEqual(["c", "d"]);
    expect(closeTargets(ids, "d", "right")).toEqual([]);
    expect(closeTargets(ids, "b", "all")).toEqual(ids);
  });

  it("moves a tab", () => {
    expect(moveTo(ids, 0, 2)).toEqual(["b", "c", "a", "d"]);
    expect(moveTo(ids, 3, 0)).toEqual(["d", "a", "b", "c"]);
    expect(moveTo(ids, 1, 99)).toEqual(["a", "c", "d", "b"]);
    expect(moveTo(ids, 7, 0)).toEqual(ids);
  });

  it("keeps the last closed tabs", () => {
    let stack: SavedTab[] = [];
    for (let i = 0; i < 25; i++) stack = pushClosed(stack, { kind: "runner", target: String(i) });
    expect(stack).toHaveLength(20);
    expect(stack.at(-1)).toEqual({ kind: "runner", target: "24" });
  });
});
