import { describe, expect, it } from "vitest";
import { applyPreset, BUILT_IN_PRESETS, presetHeaders, withPreset } from "./presets";

describe("applyPreset", () => {
  it("replaces headers already there and adds the rest", () => {
    const rows = [
      { name: "content-type", value: "text/plain", enabled: false },
      { name: "X-Trace", value: "1", enabled: true },
    ];
    const json = BUILT_IN_PRESETS.find((p) => p.name === "JSON")!;
    expect(applyPreset(rows, json.headers)).toEqual([
      { name: "content-type", value: "application/json", enabled: true },
      { name: "X-Trace", value: "1", enabled: true },
      { name: "Accept", value: "application/json", enabled: true },
    ]);
    // The rows given are left as they were.
    expect(rows[0]!.value).toBe("text/plain");
  });
});

describe("presetHeaders", () => {
  it("keeps named headers that are switched on", () => {
    expect(
      presetHeaders([
        { name: " X-Api-Version ", value: "2", enabled: true },
        { name: "", value: "orphan" },
        { name: "X-Off", value: "1", enabled: false },
      ]),
    ).toEqual([{ name: "X-Api-Version", value: "2" }]);
  });
});

describe("withPreset", () => {
  it("adds a preset or replaces one with the same name", () => {
    const list = [{ name: "Internal", headers: [] }];
    const next = { name: "internal", headers: [{ name: "X", value: "1" }] };
    expect(withPreset(list, next)).toEqual([next]);
    expect(withPreset(list, { name: "Other", headers: [] })).toHaveLength(2);
  });
});
