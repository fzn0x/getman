import { describe, expect, it } from "vitest";
import { moveTo, planFor, refreshPlan, runTargets } from "./plan";
import type { Node } from "./types";

const tree: Node[] = [
  {
    kind: "folder",
    path: "users",
    name: "Users",
    children: [
      { kind: "request", path: "users/login.yaml", name: "Login", method: "POST" },
      { kind: "request", path: "users/me.yaml", name: "Me", method: "GET" },
      { kind: "request", path: "users/broken.yaml", name: "broken", method: "", error: "bad" },
    ],
  },
  { kind: "request", path: "health.yaml", name: "Health", method: "GET" },
];

describe("plan", () => {
  it("lists the requests a target covers, in order", () => {
    expect(planFor(tree, "").map((i) => i.path)).toEqual([
      "users/login.yaml",
      "users/me.yaml",
      "health.yaml",
    ]);
    expect(planFor(tree, "users").map((i) => i.name)).toEqual(["Login", "Me"]);
    expect(planFor(tree, "health.yaml")).toHaveLength(1);
    expect(planFor(tree, "nope")).toEqual([]);
  });

  it("moves items", () => {
    expect(moveTo(["a", "b", "c"], 0, 2)).toEqual(["b", "c", "a"]);
    expect(moveTo(["a", "b", "c"], 2, 0)).toEqual(["c", "a", "b"]);
    expect(moveTo(["a", "b"], 5, 0)).toEqual(["a", "b"]);
  });

  it("runs the folder until the plan changes", () => {
    const natural = planFor(tree, "users");
    expect(runTargets("users", natural, natural)).toEqual({ target: "users" });
    const reordered = moveTo(natural, 1, 0);
    expect(runTargets("users", reordered, natural).targets).toEqual([
      "users/me.yaml",
      "users/login.yaml",
    ]);
    const unticked = natural.map((item, i) => ({ ...item, include: i === 0 }));
    expect(runTargets("users", unticked, natural).targets).toEqual(["users/login.yaml"]);
  });

  it("keeps choices when the tree changes", () => {
    const natural = planFor(tree, "");
    const chosen = moveTo(natural, 2, 0).map((item) => ({
      ...item,
      include: item.path !== "users/me.yaml",
    }));
    const grown = planFor(
      [...tree, { kind: "request", path: "zz.yaml", name: "New", method: "GET" }],
      "",
    ).filter((item) => item.path !== "users/login.yaml");
    const refreshed = refreshPlan(chosen, grown);
    expect(refreshed.map((i) => [i.path, i.include])).toEqual([
      ["health.yaml", true],
      ["users/me.yaml", false],
      ["zz.yaml", true],
    ]);
  });
});
