# HTTP and connections

How Getman puts requests on the wire, and the settings that change it:
the HTTP version, proxies, TLS certificates, cookies and the size of
responses.

Every setting here goes under `settings:` in a request, a folder's
`_folder.yaml` or the workspace's `getman.yaml`, and appears in the
Settings tab of each. A request uses its own value, else its folder's,
else the workspace's, else the default. Unset settings are not written.

```yaml
settings:
  timeout_ms: 10000
  http_version: http2
  proxy: http://proxy.internal:3128
  ca_certs: [certs/company-ca.pem]
  cookies: false
  max_response_mb: 20
```

| Field | Default | |
| --- | --- | --- |
| `timeout_ms` | 30000 | Give up after this long; 0 waits forever |
| `follow_redirects` | true | |
| `max_redirects` | 10 | More redirects than this stop the request with an error |
| `follow_original_method` | false | true keeps a POST and its body across a 301 or 302 |
| `follow_authorization` | false | true keeps `Authorization` across a redirect to another host |
| `verify_tls` | true | false accepts self-signed and expired certificates |
| `http_version` | `auto` | `auto`, `http1`, `http2` or `http3` |
| `title_case_headers` | true | Send HTTP/1.1 header names as `Content-Type` rather than `content-type` |
| `proxy` | system | `system`, `off`, a proxy URL, or a map with credentials |
| `client_cert` | none | A certificate to present: PEM files or a PKCS#12 file |
| `ca_certs` | none | More certificate authorities to trust: PEM files |
| `cookies` | true | false leaves the cookie jar alone |
| `max_response_mb` | 100 | Read at most this many MiB of a body; 0 reads everything |

## HTTP versions

| `http_version` | |
| --- | --- |
| `auto` | HTTP/2 when the server offers it over TLS, HTTP/1.1 otherwise. Plain `http://` uses HTTP/1.1 |
| `http1` | HTTP/1.1 only |
| `http2` | HTTP/2 only. Over `http://` Getman speaks HTTP/2 straight away, without asking first (prior knowledge), so the server must expect it |
| `http3` | HTTP/3 over QUIC, on UDP. Needs an `https://` URL and no proxy |

With `auto`, Getman never switches to HTTP/3 on its own. When a server
says in an `Alt-Svc` header that it speaks HTTP/3, the response shows
**H3 offered** beside the protocol, and `getman send -i` prints a hint;
set `http_version: http3` to use it. A server without HTTP/3 does not
answer QUIC at all, so such a request fails once its timeout runs out,
with an error that says HTTP/3 was the problem.

The protocol a response came over shows as `HTTP/1.1`, `HTTP/2` or
`HTTP/3`.

HTTP/3 support needs reqwest's unstable `--cfg reqwest_unstable` at build
time. Release binaries and builds from the repository have it, through
`.cargo/config.toml`. If you set `RUSTFLAGS` yourself when building, add
`--cfg reqwest_unstable` to it, or the build leaves HTTP/3 out and says so
when a request asks for it.

### Header names

HTTP/1.1 header names are not case sensitive, but some old servers care.
Getman sends them in Title-Case: each word capitalized, the rest
lowercase, so `content-type` goes out as `Content-Type` and `X-API-Key` as
`X-Api-Key`. The exact case you typed is not kept beyond that. With
`title_case_headers: false` they go out in lowercase. HTTP/2 and HTTP/3
always send lowercase names; the protocols require it.

## Proxies

By default requests go through the proxy your system is set to use, and
through the `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` and `NO_PROXY`
environment variables. To choose for yourself:

```yaml
settings:
  proxy: off                            # no proxy, whatever the system says
```

```yaml
settings:
  proxy: socks5h://127.0.0.1:1080       # just a URL
```

```yaml
settings:
  proxy:
    url: http://proxy.internal:3128
    username: "{{proxyUser}}"
    password: "{{proxyPassword}}"
    bypass: localhost, .internal.example.com, 10.0.0.0/8
```

- `url` takes `http://`, `https://`, `socks5://`, or `socks5h://` to let the
  SOCKS proxy look host names up. `system` means the default, which turns
  a folder's proxy back off for one request.
- `username` and `password` are sent as Basic credentials to an HTTP
  proxy, or as SOCKS5 credentials. Write them as variables and keep the
  values in a git-ignored `*.local.yaml` file, so they are never
  committed. A variable that is not defined stops the request rather than
  sending `{{proxyPassword}}` as written.
- `bypass` lists hosts reached directly, as in `NO_PROXY`: names,
  `.domain` suffixes, IP addresses and ranges.

`getman send` and `getman run` take `--proxy URL` (or `--proxy off`) to
override every file.

## TLS

Getman checks server certificates against your operating system's trust
store. `verify_tls: false` turns the check off for self-signed and
expired certificates.

### Your own certificate authority

`ca_certs` lists PEM files of certificate authorities to trust as well as
the system's, such as a company CA or the one a local development server
uses. A file may hold several certificates.

```yaml
settings:
  ca_certs: [certs/company-root.pem, certs/dev-ca.pem]
```

### Client certificates

Servers that ask for a client certificate (mutual TLS) get the one in
`client_cert`, as PEM files:

```yaml
settings:
  client_cert:
    cert: certs/client.pem       # the certificate, then any intermediates
    key: certs/client-key.pem    # an unencrypted private key; may be the same file
```

or as a PKCS#12 file with a passphrase:

```yaml
settings:
  client_cert:
    pkcs12: certs/client.p12
    passphrase: "{{certPassphrase}}"
```

PEM keys must be unencrypted; for a key with a passphrase, use PKCS#12
and keep the passphrase in a variable in a `*.local.yaml` file. An empty
`client_cert: {}` stops a folder's certificate from being sent.

Certificate paths are relative to the workspace root and, like files in
request bodies, must be inside it: a collection from someone else cannot
read keys from elsewhere on your disk. Paths may contain variables, so
each person can keep their own certificate in a git-ignored folder.

## Cookies

Getman keeps cookies like a browser: a response's `Set-Cookie` is stored
and sent back to the same site. In the browser UI the jar is kept in the
workspace's git-ignored `.getman/cookies.json`, so cookies survive a
restart until they expire; session cookies are kept too. The Cookies
dialog (in the response's Cookies tab) lists them and lets you add, edit
and remove them.

`cookies: false` leaves the jar alone for a request: nothing is sent from
it, and nothing the response sets is stored. Set your own `Cookie` header
for full control.

`getman send` and `getman run` start each time with an empty jar, so runs
in CI do not depend on what happened before.

## Responses

Bodies compressed with gzip, Brotli, deflate or zstd are decoded, and
Getman asks for all four.

Getman reads at most `max_response_mb` MiB of a body, 100 by default, and
marks the response as cut short when there was more. `0` reads
everything, which can use a lot of memory for a large download.

### Timing

Every response says where its time went. In the browser UI, select the
Time under the status to see it, along with the TLS version and the
server's certificate (who it was issued to and by, its names and when it
expires). `getman send -i` prints the same, and the JSON of `getman send
--json` and of `getman run -r json` includes it.

| | |
| --- | --- |
| DNS | Looking up the host's address. 0 for an IP address, or when a SOCKS5h proxy looks names up |
| Connect | TCP, any proxy tunnel, and the TLS handshake |
| First byte | From sending the request until the response headers arrived |
| Download | Reading the body |

DNS and connect are measured only when a new connection opens. A request
that reuses an open connection shows as reused, with both at 0. Over
HTTP/3 the QUIC handshake cannot be told apart, so it counts toward the
first byte.

The cipher suite is not shown: the HTTP library Getman uses does not
report it.

## Cancelling

While a request is in flight, the Send button in the browser UI turns
into Cancel, and <kbd>Esc</kbd> does the same. The request is dropped and
the response pane says it was cancelled. Scripts that already ran keep
their effects.

## Redirects

Getman follows up to `max_redirects` redirects (301, 302, 303, 307 and
308) itself, one request after the other, and the response lists them:
the browser UI shows the count beside the protocol and the steps in the
panel that opens from the Time, and `getman send -i` prints them with how
long each took. The time of the response covers the whole chain, and the
timing parts still add up to it.

What the next request looks like follows the browsers:

| Status | The next request |
| --- | --- |
| 301, 302 | A POST becomes a GET without the body. Other methods stay as they are. With `follow_original_method: true` a POST stays a POST with its body |
| 303 | Always a GET without the body (a HEAD stays a HEAD) |
| 307, 308 | The same method and body |

When a redirect leads to another host, port or scheme, the `Cookie` and
`Authorization` headers you set, and `Proxy-Authorization`, stay behind, so
a token is not sent to a site you did not choose. With
`follow_authorization: true` `Authorization` and `Proxy-Authorization`
go along. The cookie jar is unaffected by either: every response's
`Set-Cookie` is stored, including those of the redirects, and each request
sends the cookies that belong to its own host.

A redirect that cannot be followed stops the request with an error that
says why: more redirects than `max_redirects`, a `Location` that is not an
http or https URL, or a request repeated (the same method and URL a second
time), which means the redirects go in a circle. `follow_redirects: false`
returns the redirect itself, so you can read its `Location`.
