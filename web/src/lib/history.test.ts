import { describe, expect, it } from "vitest";
import { dayLabel, groupByDay, matchesHistory } from "./history";
import type { HistoryEntry } from "./types";

const at = (y: number, m: number, d: number, h = 12) => new Date(y, m - 1, d, h).getTime();

function entry(id: string, when: number, extra: Partial<HistoryEntry> = {}): HistoryEntry {
  return {
    id,
    at: when,
    path: null,
    environment: null,
    name: `Request ${id}`,
    method: "GET",
    url: `https://api.example.com/${id}`,
    status: 200,
    time_ms: 12,
    error: null,
    request: { name: "", method: "GET", url: "" },
    ...extra,
  };
}

describe("groupByDay", () => {
  const now = at(2026, 10, 28, 15);

  it("puts entries under their day, newest first", () => {
    const days = groupByDay(
      [
        entry("a", at(2026, 10, 26, 9)),
        entry("b", at(2026, 10, 28, 1)),
        entry("c", at(2026, 10, 27, 23)),
        entry("d", at(2026, 10, 28, 14)),
      ],
      now,
    );
    expect(days.map((d) => d.entries.map((e) => e.id))).toEqual([["d", "b"], ["c"], ["a"]]);
    expect(days[0]!.label).toBe("Today");
    expect(days[1]!.label).toBe("Yesterday");
    expect(days[2]!.label).not.toMatch(/Today|Yesterday/);
  });

  it("names days from another year with the year", () => {
    expect(dayLabel(at(2025, 3, 2, 0), now)).toMatch(/2025/);
    expect(dayLabel(at(2026, 3, 2, 0), now)).not.toMatch(/2026/);
  });
});

describe("matchesHistory", () => {
  const sent = entry("users", 0, {
    name: "List users",
    method: "POST",
    status: 404,
    path: "users/list.yaml",
  });

  it("matches every word against the parts of an entry", () => {
    expect(matchesHistory(sent, "")).toBe(true);
    expect(matchesHistory(sent, "users 404")).toBe(true);
    expect(matchesHistory(sent, "post LIST")).toBe(true);
    expect(matchesHistory(sent, "users 500")).toBe(false);
    expect(
      matchesHistory(entry("x", 0, { status: null, error: "Could not connect" }), "failed"),
    ).toBe(true);
  });
});
