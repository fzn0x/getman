import { describe, expect, it } from "vitest";
import { altDigit, matches } from "./keys";

type Init = { key?: string; code?: string; altKey?: boolean; ctrlKey?: boolean; metaKey?: boolean };

function press(init: Init): KeyboardEvent {
  return {
    key: "",
    code: "",
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    ...init,
  } as KeyboardEvent;
}

describe("alt shortcuts by physical key", () => {
  it("matches brackets when Option types a symbol", () => {
    const next = press({ altKey: true, key: "\u201c", code: "BracketLeft" });
    expect(matches(next, "alt+[")).toBe(true);
    expect(matches(next, "alt+]")).toBe(false);
    expect(matches(press({ altKey: true, key: "\u2018", code: "BracketRight" }), "alt+]")).toBe(
      true,
    );
  });

  it("still matches the character, and needs Alt", () => {
    expect(matches(press({ altKey: true, key: "[", code: "KeyX" }), "alt+[")).toBe(true);
    expect(matches(press({ key: "\u201c", code: "BracketLeft" }), "alt+[")).toBe(false);
  });

  it("keeps letter shortcuts working", () => {
    expect(matches(press({ altKey: true, key: "n", code: "KeyN" }), "alt+n")).toBe(true);
    expect(matches(press({ altKey: true, key: "\u02dc", code: "KeyN" }), "alt+n")).toBe(true);
  });

  it("reads the digit from the code when Option types a symbol", () => {
    expect(altDigit(press({ altKey: true, key: "\u00a1", code: "Digit1" }))).toBe(1);
    expect(altDigit(press({ altKey: true, key: "9", code: "Digit9" }))).toBe(9);
    expect(altDigit(press({ altKey: true, key: "3" }))).toBe(3);
  });

  it("ignores digits without Alt, with Ctrl or Cmd, and Digit0", () => {
    expect(altDigit(press({ key: "1", code: "Digit1" }))).toBeNull();
    expect(altDigit(press({ altKey: true, ctrlKey: true, code: "Digit1", key: "1" }))).toBeNull();
    expect(altDigit(press({ altKey: true, metaKey: true, code: "Digit1", key: "1" }))).toBeNull();
    expect(altDigit(press({ altKey: true, key: "0", code: "Digit0" }))).toBeNull();
  });
});
