import { describe, expect, it } from "vitest";
import type { SavedTab } from "./session";
import { describeSaved, searchTabs, type TabHit } from "./tabsearch";

function hit(state: TabHit["state"], id: string, title: string, detail = ""): TabHit {
  return { state, id, title, detail, method: "GET", dirty: false };
}

describe("searchTabs", () => {
  const open = [hit("open", "r:a", "List users", "users/list.yaml"), hit("open", "r:b", "Orders")];
  const closed = [
    hit("closed", "0", "Old login", "auth/login.yaml"),
    hit("closed", "1", "Delete user", "users/delete.yaml"),
  ];

  it("lists open tabs in order, then closed ones latest first", () => {
    expect(searchTabs(open, closed, "").map((h) => h.id)).toEqual(["r:a", "r:b", "1", "0"]);
  });

  it("matches the title and the path", () => {
    expect(searchTabs(open, closed, "users").map((h) => h.id)).toEqual(["r:a", "1"]);
    expect(searchTabs(open, closed, "login").map((h) => h.id)).toEqual(["0"]);
    expect(searchTabs(open, closed, "zzz")).toEqual([]);
  });

  it("keeps open tabs ahead of closed ones", () => {
    const ids = searchTabs(open, closed, "o").map((h) => h.state);
    expect(ids.indexOf("closed")).toBeGreaterThan(ids.lastIndexOf("open"));
  });
});

describe("describeSaved", () => {
  const name = (path: string) => path.toUpperCase();

  it("names requests", () => {
    const saved: SavedTab = {
      kind: "request",
      path: "users/list.yaml",
      inheritPath: null,
      name: "List users",
      method: "GET",
      requestPane: "params",
      responsePane: "body",
    };
    expect(describeSaved(saved, "Shop", name)).toEqual({
      title: "List users",
      detail: "users/list.yaml",
      method: "GET",
      dirty: false,
    });
    expect(describeSaved({ ...saved, path: null, name: "" }, "Shop", name)).toMatchObject({
      title: "Untitled request",
      detail: "Not saved yet",
    });
  });

  it("names folders, environments and runs", () => {
    expect(
      describeSaved({ kind: "folder", path: "", name: "", pane: "variables" }, "Shop", name),
    ).toMatchObject({ title: "Shop", detail: "Workspace settings" });
    expect(describeSaved({ kind: "environments", selected: "" }, "Shop", name).title).toBe(
      "Environments",
    );
    expect(describeSaved({ kind: "runner", target: "orders" }, "Shop", name).title).toBe(
      "Run ORDERS",
    );
  });
});
