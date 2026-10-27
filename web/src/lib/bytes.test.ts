import { describe, expect, it } from "vitest";
import { findLinks, hexDump, pickedValue, suggestName, trimUrl } from "./bytes";

describe("hexDump", () => {
  it("prints offsets, hex in two groups of eight and printable ASCII", () => {
    const bytes = new TextEncoder().encode("Hello, world!\n\u0000ÿABC");
    const dump = hexDump(bytes);
    expect(dump.offsets).toBe("00000000\n00000010");
    expect(dump.hex.split("\n")[0]).toBe("48 65 6c 6c 6f 2c 20 77  6f 72 6c 64 21 0a 00 c3");
    expect(dump.hex.split("\n")[1]).toBe("bf 41 42 43");
    expect(dump.ascii).toBe("Hello, world!...\n.ABC");
  });

  it("dumps a slice, keeping offsets from the start of the body", () => {
    const bytes = new Uint8Array(40).map((_, i) => i);
    const dump = hexDump(bytes, 16, 32);
    expect(dump.offsets).toBe("00000010");
    expect(dump.hex.startsWith("10 11")).toBe(true);
  });
});

describe("links", () => {
  it("finds URLs and leaves out trailing punctuation", () => {
    const text = 'See https://example.com/a?b=1. And (https://x.dev/path) or "http://y.io/q"';
    expect(findLinks(text).map((l) => l.url)).toEqual([
      "https://example.com/a?b=1",
      "https://x.dev/path",
      "http://y.io/q",
    ]);
    const first = findLinks(text)[0]!;
    expect(text.slice(first.from, first.to)).toBe("https://example.com/a?b=1");
  });

  it("keeps brackets a URL opened", () => {
    expect(trimUrl("https://en.wikipedia.org/wiki/Foo_(bar))")).toBe(
      "https://en.wikipedia.org/wiki/Foo_(bar)",
    );
    expect(findLinks("https:// nothing")).toEqual([]);
  });
});

describe("picking values", () => {
  it("unquotes a JSON string and trims", () => {
    expect(pickedValue(' "abc\\n" ')).toBe("abc\n");
    expect(pickedValue("42")).toBe("42");
    expect(pickedValue('"half')).toBe('"half');
  });

  it("suggests the JSON key on the line as the name", () => {
    expect(suggestName('    "access_token": "eyJ...",')).toBe("access_token");
    expect(suggestName("plain text")).toBe("");
    expect(suggestName('"has space": 1')).toBe("");
  });
});
