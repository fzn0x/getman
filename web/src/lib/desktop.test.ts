import { describe, expect, it } from "vitest";
import {
  inDesktopApp,
  installLabel,
  lastChecked,
  percent,
  plainNotes,
  releaseDate,
} from "./desktop";

describe("desktop", () => {
  it("is detected only inside the app", () => {
    expect(inDesktopApp({})).toBe(false);
    expect(inDesktopApp({ __TAURI_INTERNALS__: {} })).toBe(true);
  });

  it("says when it last checked", () => {
    const now = new Date(1_800_000_000_000);
    const seconds = now.getTime() / 1000;
    expect(lastChecked(null, now)).toBe("Not checked yet");
    expect(lastChecked(seconds - 20, now)).toBe("Checked just now");
    expect(lastChecked(seconds - 60, now)).toBe("Checked 1 minute ago");
    expect(lastChecked(seconds - 5 * 60, now)).toBe("Checked 5 minutes ago");
    expect(lastChecked(seconds - 3 * 3600, now)).toBe("Checked 3 hours ago");
    expect(lastChecked(seconds - 3 * 86400, now)).toMatch(/^Last checked /);
    // A clock that went back reads as just now, not as the future.
    expect(lastChecked(seconds + 100, now)).toBe("Checked just now");
  });

  it("counts a download in whole percent", () => {
    expect(percent(0, null)).toBeNull();
    expect(percent(10, 0)).toBeNull();
    expect(percent(512, 1024)).toBe(50);
    expect(percent(1023, 1024)).toBe(99);
    expect(percent(2048, 1024)).toBe(100);
  });

  it("words how a copy was installed", () => {
    expect(installLabel("deb")).toBe("Installed from the .deb package");
    expect(installLabel("development")).toBe("Development build");
  });

  it("reads release dates and leaves out bad ones", () => {
    expect(releaseDate(null)).toBe("");
    expect(releaseDate("not a date")).toBe("");
    expect(releaseDate("2026-10-28T12:00:00Z")).toMatch(/2026/);
  });

  it("turns GitHub's release notes into plain text", () => {
    const notes = [
      "## What's Changed",
      "* feat(web): **faster** tabs by @fzn0x in [#12](https://github.com/fzn0x/getman/pull/12)",
      "",
      "",
      "",
      "**Full Changelog**: https://github.com/fzn0x/getman/compare/v0.1.0...v0.2.0",
    ].join("\r\n");
    expect(plainNotes(notes)).toBe("What's Changed\n• feat(web): faster tabs by @fzn0x in #12");
  });
});
