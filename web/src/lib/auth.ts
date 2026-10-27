// Small helpers for the authentication editor.

import type { Auth, Token } from "./types";

/** Whether the settings get their token by signing in with a browser. */
export function usesBrowser(auth: Auth): boolean {
  return (
    auth.type === "oauth2" && (auth.grant === "authorization_code" || auth.grant === "implicit")
  );
}

/** A rough length of time: "45 s", "12 min", "3 h", "2 days". */
export function duration(ms: number): string {
  const seconds = Math.round(Math.abs(ms) / 1000);
  if (seconds < 60) return `${seconds} s`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 48) return `${hours} h`;
  return `${Math.round(hours / 24)} days`;
}

/** When a token expires, in words. */
export function expiryText(token: Token, now = Date.now()): string {
  if (token.expires_at == null) return "No expiry given";
  const left = token.expires_at - now;
  if (left > 0) return `Expires in ${duration(left)}`;
  return token.refreshable
    ? `Expired ${duration(left)} ago; refreshed when next used`
    : `Expired ${duration(left)} ago`;
}

/** Whether a URL is plain http on this machine, where Getman can listen. */
export function isLoopbackUrl(text: string): boolean {
  let url: URL;
  try {
    url = new URL(text.trim());
  } catch {
    return false;
  }
  if (url.protocol !== "http:") return false;
  const host = url.hostname.toLowerCase();
  return host === "localhost" || host === "[::1]" || /^127(\.\d{1,3}){3}$/.test(host);
}

/** "X-A, X-B" as a list of names. */
export function splitList(text: string): string[] {
  return text
    .split(",")
    .map((item) => item.trim())
    .filter((item) => item !== "");
}

/** A whole number from a text field; `null` when empty or not one. */
export function wholeNumber(text: string): number | null {
  const value = Number(text.trim());
  return text.trim() === "" || !Number.isFinite(value) || value < 0 ? null : Math.round(value);
}
