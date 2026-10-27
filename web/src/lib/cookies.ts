// The cookie form in the Cookies dialog: what people type, and the cookie
// it becomes.

import type { Cookie } from "./types";

export interface CookieForm {
  domain: string;
  path: string;
  name: string;
  value: string;
  /** A `datetime-local` value in local time; empty for a session cookie. */
  expires: string;
  secure: boolean;
  httpOnly: boolean;
  /** Also send it to subdomains of `domain`. */
  subdomains: boolean;
}

export function emptyCookieForm(domain = ""): CookieForm {
  return {
    domain,
    path: "/",
    name: "",
    value: "",
    expires: "",
    secure: false,
    httpOnly: false,
    subdomains: false,
  };
}

export function cookieForm(cookie: Cookie): CookieForm {
  return {
    domain: cookie.domain,
    path: cookie.path,
    name: cookie.name,
    value: cookie.value,
    expires: cookie.expires === null ? "" : localDateTime(cookie.expires),
    secure: cookie.secure,
    httpOnly: cookie.http_only,
    subdomains: !cookie.host_only,
  };
}

/** The cookie a form describes, or why it cannot be one. */
export function formCookie(form: CookieForm, now = Date.now()): Cookie | string {
  const domain = form.domain.trim().replace(/^\./, "").toLowerCase();
  const name = form.name.trim();
  if (!domain) return "A cookie needs a domain.";
  if (/[\s/:]/.test(domain)) return "Write the domain alone, such as api.example.com.";
  if (!name) return "A cookie needs a name.";
  if (/[\s;,=]/.test(name)) return "A cookie name cannot hold spaces, ;, , or =.";
  if (/[;\r\n]/.test(form.value)) return "A cookie value cannot hold ; or line breaks.";
  let expires: number | null = null;
  if (form.expires) {
    expires = new Date(form.expires).getTime();
    if (Number.isNaN(expires)) return "The expiry is not a date.";
    if (expires <= now) return "The expiry is in the past.";
  }
  const path = form.path.trim() || "/";
  return {
    domain,
    path: path.startsWith("/") ? path : `/${path}`,
    name,
    value: form.value,
    expires,
    secure: form.secure,
    http_only: form.httpOnly,
    host_only: !form.subdomains,
  };
}

/** Milliseconds since the epoch as a `datetime-local` value, in local time. */
export function localDateTime(ms: number): string {
  const date = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}`;
}
