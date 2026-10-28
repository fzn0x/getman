import { describe, expect, it } from "vitest";
import { moveTargets, pathRange, sortNodes } from "./tree";
import type { Node } from "./types";

const request = (path: string, name: string): Node => ({
  kind: "request",
  path,
  name,
  method: "GET",
});

describe("sortNodes", () => {
  it("sorts every level by name, folders first, numbers as numbers", () => {
    const tree: Node[] = [
      request("b.yaml", "beta"),
      {
        kind: "folder",
        path: "z",
        name: "Zed",
        children: [request("z/10.yaml", "Item 10"), request("z/9.yaml", "Item 9")],
      },
      request("a.yaml", "Alpha"),
      { kind: "folder", path: "m", name: "middle", children: [] },
    ];
    const sorted = sortNodes(tree);
    expect(sorted.map((n) => n.name)).toEqual(["middle", "Zed", "Alpha", "beta"]);
    const zed = sorted[1] as Extract<Node, { kind: "folder" }>;
    expect(zed.children.map((n) => n.name)).toEqual(["Item 9", "Item 10"]);
    // The tree itself is left as it was.
    expect(tree[0]!.name).toBe("beta");
  });
});

describe("pathRange", () => {
  const visible = ["a", "b", "c", "d"];

  it("covers the rows between two, either way round", () => {
    expect(pathRange(visible, "b", "d")).toEqual(["b", "c", "d"]);
    expect(pathRange(visible, "c", "a")).toEqual(["a", "b", "c"]);
    expect(pathRange(visible, "gone", "b")).toEqual(["b"]);
    expect(pathRange(visible, "a", "gone")).toEqual([]);
  });
});

describe("moveTargets", () => {
  it("leaves out the folders being moved and what is inside them", () => {
    expect(moveTargets(["", "a", "a/b", "ab", "c"], ["a", "c/x.yaml"])).toEqual(["", "ab", "c"]);
  });
});
