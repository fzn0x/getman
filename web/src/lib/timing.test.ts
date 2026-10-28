import { describe, expect, it } from "vitest";
import { commonName, expiry, phases, timingNote } from "./timing";

const fresh = {
  dns_ms: 10,
  connect_ms: 30,
  first_byte_ms: 50,
  download_ms: 10,
  reused: false,
};

describe("phases", () => {
  it("lays the phases end to end", () => {
    const list = phases(fresh);
    expect(list.map((p) => p.key)).toEqual(["dns", "connect", "wait", "download"]);
    expect(list.map((p) => p.share)).toEqual([0.1, 0.3, 0.5, 0.1]);
    expect(list[2]?.start).toBeCloseTo(0.4);
  });

  it("copes with nothing to show", () => {
    const list = phases({
      ...fresh,
      dns_ms: 0,
      connect_ms: null,
      first_byte_ms: 0,
      download_ms: 0,
    });
    expect(list.every((p) => p.share === 0)).toBe(true);
  });
});

describe("timingNote", () => {
  it("explains empty phases", () => {
    expect(timingNote({ ...fresh, reused: true, dns_ms: 0, connect_ms: 0 })).toMatch(/reused/);
    expect(timingNote({ ...fresh, connect_ms: null })).toMatch(/HTTP\/3/);
    expect(timingNote({ ...fresh, dns_ms: 0 })).toMatch(/IP address/);
    expect(timingNote(fresh)).toBe("");
  });
});

describe("expiry", () => {
  const now = Date.UTC(2026, 0, 1);
  it("says how long a certificate has left", () => {
    expect(expiry(now + 42 * 86_400_000, now)).toBe("in 42 days");
    expect(expiry(now + 86_400_000, now)).toBe("tomorrow");
    expect(expiry(now - 3 * 86_400_000, now)).toBe("expired 3 days ago");
  });
});

describe("commonName", () => {
  it("picks the CN out of a name", () => {
    expect(commonName("C=US, O=Google Trust Services, CN=WR2")).toBe("WR2");
    expect(commonName("CN=api.example.com")).toBe("api.example.com");
    expect(commonName("O=No common name")).toBe("O=No common name");
  });
});
