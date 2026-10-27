import { describe, expect, it } from "vitest";
import { cookieForm, emptyCookieForm, formCookie, localDateTime } from "./cookies";

const now = new Date(2026, 9, 28, 12, 0).getTime();

describe("formCookie", () => {
  it("turns a form into a cookie", () => {
    const cookie = formCookie(
      {
        ...emptyCookieForm(".API.example.com"),
        name: "session",
        value: "abc",
        path: "v1",
        subdomains: true,
        expires: "2026-10-29T12:00",
      },
      now,
    );
    expect(cookie).toEqual({
      domain: "api.example.com",
      path: "/v1",
      name: "session",
      value: "abc",
      expires: new Date(2026, 9, 29, 12, 0).getTime(),
      secure: false,
      http_only: false,
      host_only: false,
    });
  });

  it("explains what is wrong", () => {
    const form = { ...emptyCookieForm("example.com"), name: "a" };
    expect(formCookie({ ...form, domain: "" }, now)).toMatch(/domain/);
    expect(formCookie({ ...form, domain: "https://example.com" }, now)).toMatch(/alone/);
    expect(formCookie({ ...form, name: "" }, now)).toMatch(/name/);
    expect(formCookie({ ...form, value: "a;b" }, now)).toMatch(/;/);
    expect(formCookie({ ...form, expires: "2020-01-01T00:00" }, now)).toMatch(/past/);
  });

  it("round-trips through the form", () => {
    const cookie = {
      domain: "example.com",
      path: "/",
      name: "id",
      value: "1",
      expires: new Date(2027, 0, 1, 8, 30).getTime(),
      secure: true,
      http_only: true,
      host_only: true,
    };
    expect(formCookie(cookieForm(cookie), now)).toEqual(cookie);
    expect(cookieForm({ ...cookie, expires: null }).expires).toBe("");
    expect(localDateTime(cookie.expires)).toBe("2027-01-01T08:30");
  });
});
