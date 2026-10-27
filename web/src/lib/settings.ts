// The transport settings as the Settings pane shows them: a proxy may be
// written as just its URL, and a client certificate is PEM files, a
// PKCS#12 file, or none.

import type { ClientCert, ProxySetting, Settings } from "./types";

/** `""` means unset: inherited from the folder, or the default. */
export type ProxyMode = "" | "system" | "off" | "url";
export type CertMode = "" | "none" | "pem" | "pkcs12";

/** The proxy as a map, whichever way it was written. */
export function proxyOf(setting: Settings["proxy"]): ProxySetting | null {
  if (setting === null || setting === undefined) return null;
  if (typeof setting === "string") return { url: setting };
  return setting;
}

export function proxyMode(setting: Settings["proxy"]): ProxyMode {
  const proxy = proxyOf(setting);
  if (!proxy) return "";
  const url = proxy.url.trim().toLowerCase();
  if (url === "" || url === "system") return "system";
  if (url === "off") return "off";
  return "url";
}

/** The proxy setting after choosing a mode, keeping what was typed for a URL. */
export function withProxyMode(mode: ProxyMode, setting: Settings["proxy"]): Settings["proxy"] {
  switch (mode) {
    case "":
      return null;
    case "system":
    case "off":
      return mode;
    case "url": {
      const proxy = proxyOf(setting);
      return proxyMode(setting) === "url" && proxy ? proxy : { url: "" };
    }
  }
}

/** The proxy with one field changed, written as just its URL when that is all it has. */
export function withProxyField(
  setting: Settings["proxy"],
  field: keyof ProxySetting,
  value: string,
): ProxySetting | string {
  const proxy: ProxySetting = { url: "", ...proxyOf(setting), [field]: value };
  const extras = (["username", "password", "bypass"] as const).filter((key) => proxy[key]);
  if (extras.length === 0) return proxy.url;
  const compact: ProxySetting = { url: proxy.url };
  for (const key of extras) compact[key] = proxy[key];
  return compact;
}

export function certMode(cert: ClientCert | null | undefined): CertMode {
  if (cert === null || cert === undefined) return "";
  if (cert.pkcs12 !== undefined && cert.pkcs12 !== null) return "pkcs12";
  if (cert.cert !== undefined || cert.key !== undefined) return "pem";
  return "none";
}

/** The client certificate after choosing a mode. */
export function withCertMode(mode: CertMode): ClientCert | null {
  switch (mode) {
    case "":
      return null;
    case "none":
      return {};
    case "pem":
      return { cert: "", key: "" };
    case "pkcs12":
      return { pkcs12: "", passphrase: "" };
  }
}

/** Certificate paths typed one per line, or separated by commas. */
export function pathList(text: string): string[] | null {
  const paths = text
    .split(/[\n,]/)
    .map((path) => path.trim())
    .filter(Boolean);
  return paths.length ? paths : null;
}
