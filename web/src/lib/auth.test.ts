import { describe, expect, it } from "vitest";
import { duration, expiryText, isLoopbackUrl, wholeNumber, splitList, usesBrowser } from "./auth";
import type { Token } from "./types";

const token = (fields: Partial<Token>): Token => ({
  access_token: "t",
  token_type: "Bearer",
  expires_in: null,
  scope: null,
  ...fields,
});

describe("expiry", () => {
  it("counts down and says when a token can be refreshed", () => {
    const now = 1_000_000;
    expect(expiryText(token({}), now)).toBe("No expiry given");
    expect(expiryText(token({ expires_at: now + 90_000 }), now)).toBe("Expires in 2 min");
    expect(expiryText(token({ expires_at: now - 30_000 }), now)).toBe("Expired 30 s ago");
    expect(expiryText(token({ expires_at: now - 7_200_000, refreshable: true }), now)).toBe(
      "Expired 2 h ago; refreshed when next used",
    );
    expect(duration(3 * 86_400_000)).toBe("3 days");
  });
});

describe("callback URLs", () => {
  it("accepts only plain http on this machine", () => {
    expect(isLoopbackUrl("http://127.0.0.1:8765/callback")).toBe(true);
    expect(isLoopbackUrl("http://localhost:3000/cb")).toBe(true);
    expect(isLoopbackUrl("http://[::1]:3000/cb")).toBe(true);
    expect(isLoopbackUrl("https://127.0.0.1/cb")).toBe(false);
    expect(isLoopbackUrl("https://oauth.pstmn.io/v1/callback")).toBe(false);
    expect(isLoopbackUrl("not a url")).toBe(false);
  });
});

describe("fields", () => {
  it("splits lists and reads whole numbers", () => {
    expect(splitList("X-A, X-B,, ")).toEqual(["X-A", "X-B"]);
    expect(wholeNumber("300")).toBe(300);
    expect(wholeNumber("")).toBeNull();
    expect(wholeNumber("-1")).toBeNull();
    expect(wholeNumber("soon")).toBeNull();
  });

  it("knows which grants sign in with a browser", () => {
    const oauth = (grant: "implicit" | "password") =>
      usesBrowser({ type: "oauth2", grant, token_url: "", client_id: "", client_auth: "basic" });
    expect(oauth("implicit")).toBe(true);
    expect(oauth("password")).toBe(false);
    expect(usesBrowser({ type: "none" })).toBe(false);
  });
});
