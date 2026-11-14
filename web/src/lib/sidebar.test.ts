import { beforeEach, describe, expect, it } from "vitest";
import { newRequest, prefs, tabs, viewport } from "./store.svelte";

describe("the sidebar overlay", () => {
  beforeEach(() => {
    tabs.list = [];
    tabs.active = "";
    prefs.sidebarOpen = true;
    viewport.overlay = false;
  });

  it("closes when a request takes focus", () => {
    viewport.overlay = true;
    newRequest();
    expect(tabs.list).toHaveLength(1);
    expect(prefs.sidebarOpen).toBe(false);
  });

  it("stays open beside the page", () => {
    newRequest();
    expect(prefs.sidebarOpen).toBe(true);
  });
});
