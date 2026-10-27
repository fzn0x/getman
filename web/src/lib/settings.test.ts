import { describe, expect, it } from "vitest";
import {
  certMode,
  pathList,
  proxyMode,
  proxyOf,
  withCertMode,
  withProxyField,
  withProxyMode,
} from "./settings";

describe("proxies", () => {
  it("reads either way a proxy is written", () => {
    expect(proxyOf("off")).toEqual({ url: "off" });
    expect(proxyOf({ url: "http://p:3128", username: "{{u}}" })?.username).toBe("{{u}}");
    expect(proxyOf(null)).toBeNull();
    expect(proxyMode(undefined)).toBe("");
    expect(proxyMode("system")).toBe("system");
    expect(proxyMode("OFF")).toBe("off");
    expect(proxyMode({ url: "socks5h://127.0.0.1:1080" })).toBe("url");
  });

  it("keeps a typed URL when switching back to it", () => {
    expect(withProxyMode("off", "http://p:1")).toBe("off");
    expect(withProxyMode("", "http://p:1")).toBeNull();
    expect(withProxyMode("url", "off")).toEqual({ url: "" });
    expect(withProxyMode("url", { url: "http://p:1" })).toEqual({ url: "http://p:1" });
  });

  it("writes just the URL when nothing else is set", () => {
    expect(withProxyField({ url: "" }, "url", "http://p:1")).toBe("http://p:1");
    expect(withProxyField("http://p:1", "password", "{{pw}}")).toEqual({
      url: "http://p:1",
      password: "{{pw}}",
    });
    expect(withProxyField({ url: "http://p:1", password: "x" }, "password", "")).toBe("http://p:1");
  });
});

describe("client certificates", () => {
  it("tells PEM, PKCS#12, none and unset apart", () => {
    expect(certMode(undefined)).toBe("");
    expect(certMode({})).toBe("none");
    expect(certMode({ cert: "a.pem" })).toBe("pem");
    expect(certMode({ pkcs12: "" })).toBe("pkcs12");
    expect(certMode(withCertMode("pkcs12"))).toBe("pkcs12");
    expect(certMode(withCertMode("pem"))).toBe("pem");
    expect(withCertMode("")).toBeNull();
  });
});

describe("pathList", () => {
  it("splits lines and commas and drops blanks", () => {
    expect(pathList("certs/a.pem\n\n certs/b.pem, certs/c.pem ")).toEqual([
      "certs/a.pem",
      "certs/b.pem",
      "certs/c.pem",
    ]);
    expect(pathList("  ")).toBeNull();
  });
});
