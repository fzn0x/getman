import { describe, expect, it } from "vitest";
import { parsePath, walk } from "./jsonpath";

const data = { data: [{ id: 1, "full name": "Ada" }, { id: 2 }], meta: { total: 2 } };

function get(path: string): unknown {
  const steps = parsePath(path);
  if (typeof steps === "string") throw new Error(steps);
  return walk(data, steps).value;
}

describe("json paths", () => {
  it("walks keys, indexes and lengths", () => {
    expect(get("data[0].id")).toBe(1);
    expect(get("json.data[-1].id")).toBe(2);
    expect(get('data[0]["full name"]')).toBe("Ada");
    expect(get("data.length")).toBe(2);
    expect(get("")).toBe(data);
  });

  it("reports what is wrong", () => {
    expect(parsePath("data[x]")).toBe("x is not an index");
    expect(walk(data, [{ key: "nope" }]).found).toBe(false);
  });
});
