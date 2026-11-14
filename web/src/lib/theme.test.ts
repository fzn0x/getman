import { describe, expect, it } from "vitest";
import { nextTheme, parseTheme, themeLabel } from "./theme";

describe("theme", () => {
  it("cycles system, light, dark and back", () => {
    expect(nextTheme("system")).toBe("light");
    expect(nextTheme("light")).toBe("dark");
    expect(nextTheme("dark")).toBe("system");
  });

  it("reads stored light and dark, and falls back to system", () => {
    expect(parseTheme("light")).toBe("light");
    expect(parseTheme("dark")).toBe("dark");
    expect(parseTheme("system")).toBe("system");
    expect(parseTheme("blue")).toBe("system");
    expect(parseTheme(null)).toBe("system");
  });

  it("names the active theme and the next one", () => {
    expect(themeLabel("system")).toBe("Theme: system. Click for light");
    expect(themeLabel("dark")).toBe("Theme: dark. Click for system");
  });
});
