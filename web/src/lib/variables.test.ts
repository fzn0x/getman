import { describe, expect, it } from "vitest";
import {
  applyCompletion,
  completionAt,
  rankNames,
  referenceAt,
  segments,
  shownValue,
} from "./variables";

describe("segments", () => {
  it("cuts text into plain runs and references", () => {
    expect(segments("Bearer {{ token }}!")).toEqual([
      { text: "Bearer ", from: 0 },
      { text: "{{ token }}", name: "token", from: 7 },
      { text: "!", from: 18 },
    ]);
    expect(segments("{{a}}{{$uuid}}").map((s) => s.name)).toEqual(["a", "$uuid"]);
    expect(segments("no {{ braces")).toEqual([{ text: "no {{ braces", from: 0 }]);
  });

  it("finds the reference at a position, inside the braces only", () => {
    const text = "x {{base}}/y";
    expect(referenceAt(text, 4)?.name).toBe("base");
    expect(referenceAt(text, 2)).toBeNull();
    expect(referenceAt(text, 10)).toBeNull();
  });
});

describe("completion", () => {
  it("starts after {{ and follows what is typed", () => {
    expect(completionAt("Bearer {{", 9)).toEqual({ from: 9, query: "" });
    expect(completionAt("{{ to", 5)).toEqual({ from: 3, query: "to" });
    expect(completionAt("{{token}} x", 11)).toBeNull();
    expect(completionAt("plain", 5)).toBeNull();
  });

  it("closes the braces, or reuses the ones there", () => {
    expect(applyCompletion("Bearer {{to", 9, 11, "token")).toEqual({
      text: "Bearer {{token}}",
      caret: 16,
    });
    expect(applyCompletion("{{to}}/x", 2, 4, "token")).toEqual({ text: "{{token}}/x", caret: 9 });
    expect(applyCompletion("{{tok}}", 2, 3, "token")).toEqual({ text: "{{token}}", caret: 9 });
  });

  it("ranks names that start with the query first", () => {
    const items = ["baseUrl", "token", "authToken", "$timestamp"].map((label) => ({ label }));
    expect(rankNames(items, "to").map((i) => i.label)).toEqual(["token", "authToken"]);
    expect(rankNames(items, "")).toHaveLength(4);
  });
});

describe("shownValue", () => {
  it("masks secrets until revealed", () => {
    expect(shownValue("hunter2", true, false)).toBe("•••••••");
    expect(shownValue("hunter2", true, true)).toBe("hunter2");
    expect(shownValue("x".repeat(400), false, false)).toHaveLength(300);
  });
});
