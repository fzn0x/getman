<script lang="ts">
  import {
    certMode,
    pathList,
    proxyMode,
    proxyOf,
    withCertMode,
    withProxyField,
    withProxyMode,
    type CertMode,
    type ProxyMode,
  } from "../../lib/settings";
  import type { ClientCert, HttpVersion, ProxySetting, Settings } from "../../lib/types";

  let { settings = $bindable(), scope = "request" }: { settings: Settings; scope?: string } =
    $props();

  function tri(value: boolean | null | undefined): string {
    return value === true ? "on" : value === false ? "off" : "";
  }

  function fromTri(value: string): boolean | null {
    return value === "on" ? true : value === "off" ? false : null;
  }

  function number(value: string): number | null {
    const n = Number(value);
    return value.trim() === "" || !Number.isFinite(n) || n < 0 ? null : Math.round(n);
  }

  const inherit = $derived(scope === "workspace" ? "Default" : "Inherit");

  const proxy = $derived(proxyOf(settings.proxy));
  const mode = $derived(proxyMode(settings.proxy));
  const cert = $derived(settings.client_cert ?? null);
  const certKind = $derived(certMode(settings.client_cert));
  const caText = $derived((settings.ca_certs ?? []).join("\n"));

  function setProxyField(field: keyof ProxySetting, value: string): void {
    settings.proxy = withProxyField(settings.proxy, field, value);
  }

  function setCertField(field: keyof ClientCert, value: string): void {
    settings.client_cert = { ...(settings.client_cert ?? {}), [field]: value };
  }
</script>

<div class="settings">
  <h3 class="bitmap">Sending</h3>
  <label class="field">
    <span class="bitmap">Timeout</span>
    <span class="with-unit">
      <input
        class="input mono"
        inputmode="numeric"
        placeholder="{inherit} (30000)"
        value={settings.timeout_ms ?? ""}
        oninput={(event) => (settings.timeout_ms = number(event.currentTarget.value))}
      />
      <span class="muted">ms · 0 waits forever</span>
    </span>
  </label>
  <label class="field">
    <span class="bitmap">Follow redirects</span>
    <select
      class="select"
      value={tri(settings.follow_redirects)}
      onchange={(event) => (settings.follow_redirects = fromTri(event.currentTarget.value))}
    >
      <option value="">{inherit} (yes)</option>
      <option value="on">Yes</option>
      <option value="off">No</option>
    </select>
  </label>
  <label class="field">
    <span class="bitmap">Most redirects</span>
    <input
      class="input mono"
      inputmode="numeric"
      placeholder="{inherit} (10)"
      value={settings.max_redirects ?? ""}
      oninput={(event) => (settings.max_redirects = number(event.currentTarget.value))}
    />
  </label>
  <label class="field">
    <span class="bitmap">Cookie jar</span>
    <select
      class="select"
      value={tri(settings.cookies)}
      onchange={(event) => (settings.cookies = fromTri(event.currentTarget.value))}
    >
      <option value="">{inherit} (send and store cookies)</option>
      <option value="on">Send and store cookies</option>
      <option value="off">Leave the jar alone</option>
    </select>
  </label>
  <label class="field">
    <span class="bitmap">Largest response</span>
    <span class="with-unit">
      <input
        class="input mono"
        inputmode="numeric"
        placeholder="{inherit} (100)"
        value={settings.max_response_mb ?? ""}
        oninput={(event) => (settings.max_response_mb = number(event.currentTarget.value))}
      />
      <span class="muted">MiB · the rest is cut off · 0 reads everything</span>
    </span>
  </label>

  <h3 class="bitmap">Protocol</h3>
  <label class="field">
    <span class="bitmap">HTTP version</span>
    <select
      class="select"
      value={settings.http_version ?? ""}
      onchange={(event) =>
        (settings.http_version = (event.currentTarget.value || null) as HttpVersion | null)}
    >
      <option value="">{inherit} (auto)</option>
      <option value="auto">Auto — HTTP/2 when the server offers it, else HTTP/1.1</option>
      <option value="http1">HTTP/1.1 only</option>
      <option value="http2">HTTP/2 only — without asking first on http://</option>
      <option value="http3">HTTP/3 over QUIC — https:// only, no proxy</option>
    </select>
  </label>
  <label class="field">
    <span class="bitmap">Header names on HTTP/1.1</span>
    <select
      class="select"
      value={tri(settings.title_case_headers)}
      onchange={(event) => (settings.title_case_headers = fromTri(event.currentTarget.value))}
    >
      <option value="">{inherit} (Title-Case)</option>
      <option value="on">Title-Case — Content-Type</option>
      <option value="off">Lowercase — content-type</option>
    </select>
  </label>
  <p class="muted">
    HTTP/2 and HTTP/3 always send header names in lowercase. Title-Case capitalizes each word, so
    X-API-Key goes out as X-Api-Key.
  </p>

  <h3 class="bitmap">Proxy</h3>
  <label class="field">
    <span class="bitmap">Proxy</span>
    <select
      class="select"
      value={mode}
      onchange={(event) =>
        (settings.proxy = withProxyMode(event.currentTarget.value as ProxyMode, settings.proxy))}
    >
      <option value="">{inherit} (system)</option>
      <option value="system">System settings and HTTPS_PROXY, HTTP_PROXY, NO_PROXY</option>
      <option value="off">None — ignore every proxy setting</option>
      <option value="url">This proxy</option>
    </select>
  </label>
  {#if mode === "url" && proxy}
    <label class="field">
      <span class="bitmap">Proxy URL</span>
      <input
        class="input mono"
        placeholder="http://proxy.example.com:3128"
        spellcheck="false"
        value={proxy.url}
        oninput={(event) => setProxyField("url", event.currentTarget.value)}
      />
    </label>
    <div class="pair">
      <label class="field">
        <span class="bitmap">User name</span>
        <input
          class="input mono"
          placeholder={"{{proxyUser}}"}
          spellcheck="false"
          value={proxy.username ?? ""}
          oninput={(event) => setProxyField("username", event.currentTarget.value)}
        />
      </label>
      <label class="field">
        <span class="bitmap">Password</span>
        <input
          class="input mono"
          placeholder={"{{proxyPassword}}"}
          spellcheck="false"
          value={proxy.password ?? ""}
          oninput={(event) => setProxyField("password", event.currentTarget.value)}
        />
      </label>
    </div>
    <label class="field">
      <span class="bitmap">Reach directly</span>
      <input
        class="input mono"
        placeholder="localhost, .internal.example.com, 10.0.0.0/8"
        spellcheck="false"
        value={proxy.bypass ?? ""}
        oninput={(event) => setProxyField("bypass", event.currentTarget.value)}
      />
    </label>
    <p class="muted">
      http://, https://, socks5:// or socks5h:// (the proxy looks names up). Keep credentials in
      variables from a git-ignored <code>.local.yaml</code> file rather than typing them here.
    </p>
  {/if}

  <h3 class="bitmap">TLS</h3>
  <label class="field">
    <span class="bitmap">Check TLS certificates</span>
    <select
      class="select"
      value={tri(settings.verify_tls)}
      onchange={(event) => (settings.verify_tls = fromTri(event.currentTarget.value))}
    >
      <option value="">{inherit} (yes)</option>
      <option value="on">Yes</option>
      <option value="off">No — accept self-signed and expired certificates</option>
    </select>
  </label>
  <label class="field">
    <span class="bitmap">Client certificate</span>
    <select
      class="select"
      value={certKind}
      onchange={(event) =>
        (settings.client_cert = withCertMode(event.currentTarget.value as CertMode))}
    >
      <option value="">{inherit} (none)</option>
      <option value="none">None</option>
      <option value="pem">PEM certificate and key files</option>
      <option value="pkcs12">PKCS#12 file (.p12, .pfx)</option>
    </select>
  </label>
  {#if certKind === "pem" && cert}
    <div class="pair">
      <label class="field">
        <span class="bitmap">Certificate file</span>
        <input
          class="input mono"
          placeholder="certs/client.pem"
          spellcheck="false"
          value={cert.cert ?? ""}
          oninput={(event) => setCertField("cert", event.currentTarget.value)}
        />
      </label>
      <label class="field">
        <span class="bitmap">Key file</span>
        <input
          class="input mono"
          placeholder="certs/client-key.pem"
          spellcheck="false"
          value={cert.key ?? ""}
          oninput={(event) => setCertField("key", event.currentTarget.value)}
        />
      </label>
    </div>
  {:else if certKind === "pkcs12" && cert}
    <div class="pair">
      <label class="field">
        <span class="bitmap">PKCS#12 file</span>
        <input
          class="input mono"
          placeholder="certs/client.p12"
          spellcheck="false"
          value={cert.pkcs12 ?? ""}
          oninput={(event) => setCertField("pkcs12", event.currentTarget.value)}
        />
      </label>
      <label class="field">
        <span class="bitmap">Passphrase</span>
        <input
          class="input mono"
          placeholder={"{{certPassphrase}}"}
          spellcheck="false"
          value={cert.passphrase ?? ""}
          oninput={(event) => setCertField("passphrase", event.currentTarget.value)}
        />
      </label>
    </div>
  {/if}
  <label class="field">
    <span class="bitmap">Trusted CA certificates</span>
    <textarea
      class="input mono paths"
      rows="2"
      placeholder={settings.ca_certs === null || settings.ca_certs === undefined
        ? `${inherit} (the system's only) · one PEM file per line, such as certs/ca.pem`
        : ""}
      spellcheck="false"
      value={caText}
      onchange={(event) => (settings.ca_certs = pathList(event.currentTarget.value))}></textarea>
  </label>
  <p class="muted">
    Files are relative to the workspace and must be inside it. PEM keys must be unencrypted; use
    PKCS#12 for a key with a passphrase, and keep the passphrase in a variable.
  </p>

  <p class="muted scope">
    {scope === "request"
      ? "Unset values come from the folder, then the workspace."
      : "Requests inside use these unless they set their own."}
  </p>
</div>

<style>
  .settings {
    display: grid;
    gap: var(--s3);
    align-content: start;
    max-width: 520px;
  }

  h3 {
    margin: var(--s3) 0 0;
    padding-top: var(--s3);
    border-top: 1px solid var(--rule-3);
    font-size: var(--b1);
    font-weight: 400;
    color: var(--ink);
  }

  h3:first-child {
    margin-top: 0;
    padding-top: 0;
    border-top: 0;
  }

  .field {
    display: grid;
    gap: 6px;
    min-width: 0;
  }

  .field > .bitmap {
    color: var(--ink-3);
  }

  .with-unit {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }

  .with-unit .input {
    width: 160px;
    flex: none;
  }

  .pair {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: var(--s3);
  }

  .paths {
    height: auto;
    min-height: 52px;
    padding-top: 6px;
    resize: vertical;
  }

  code {
    font-family: var(--font-code);
  }

  p {
    margin: 0;
  }

  .scope {
    margin-top: var(--s2);
  }
</style>
