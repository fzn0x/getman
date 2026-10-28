// Where a response's time went, and what its TLS certificate says, for
// the timing panel under the response's Time.

import type { Timings } from "./types";

export interface Phase {
  key: "dns" | "connect" | "wait" | "download";
  label: string;
  ms: number;
  /** Where the phase starts, as a fraction of the whole. */
  start: number;
  /** How much of the whole it takes, as a fraction. */
  share: number;
}

/** The phases of a response in order, laid end to end. */
export function phases(timings: Timings): Phase[] {
  const parts: Omit<Phase, "start" | "share">[] = [
    { key: "dns", label: "DNS lookup", ms: timings.dns_ms },
    { key: "connect", label: "Connect and TLS", ms: timings.connect_ms ?? 0 },
    { key: "wait", label: "Waiting for the first byte", ms: timings.first_byte_ms },
    { key: "download", label: "Download", ms: timings.download_ms },
  ];
  const total = parts.reduce((sum, part) => sum + Math.max(0, part.ms), 0);
  let at = 0;
  return parts.map((part) => {
    const share = total > 0 ? Math.max(0, part.ms) / total : 0;
    const phase = { ...part, start: at, share };
    at += share;
    return phase;
  });
}

/** Why some phases are empty, if they are. */
export function timingNote(timings: Timings): string {
  if (timings.reused)
    return "An open connection was reused, so nothing was looked up or connected. DNS and connect times show only for new connections.";
  if (timings.connect_ms === null)
    return "Over HTTP/3 the QUIC handshake is part of waiting for the first byte.";
  if (timings.dns_ms === 0)
    return "No DNS lookup: the host is an IP address, or the proxy looks names up.";
  return "";
}

/** When a certificate expires, relative to now: `in 42 days`, `expired 3 days ago`. */
export function expiry(notAfter: number, now = Date.now()): string {
  const days = Math.round((notAfter - now) / 86_400_000);
  if (days > 1) return `in ${days} days`;
  if (days === 1) return "tomorrow";
  if (days === 0) return notAfter >= now ? "today" : "earlier today";
  if (days === -1) return "expired yesterday";
  return `expired ${-days} days ago`;
}

/** The common name of an X.509 name such as `C=US, O=Example, CN=api.example.com`. */
export function commonName(name: string): string {
  const match = /(?:^|,\s*)CN=([^,]+)/.exec(name);
  return match?.[1]?.trim() ?? name;
}
