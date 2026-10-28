# Authentication

A request's `auth` says how it proves who it is. Set it on a folder or in
`getman.yaml` and every request inside inherits it; a request can set its
own, or `none` to send no credentials. Every field accepts
`{{variables}}`.

Keep secrets out of the files you commit. Put them in a `.local.yaml`
file, which Git ignores, and refer to them by name:

```yaml
# environments/staging.local.yaml (not committed)
variables:
  clientSecret: s3cr3t
  signingKey: |
    -----BEGIN PRIVATE KEY-----
    MIIEvQIBADANBgkqhkiG9w0BAQEFAASC...
    -----END PRIVATE KEY-----
```

```yaml
# api/_folder.yaml (committed)
auth:
  type: oauth2
  token_url: https://id.example.com/oauth/token
  client_id: reporting
  client_secret: "{{clientSecret}}"
```

Files only list fields that differ from their defaults, so the examples
below are complete.

## The types

| `type` | Sends |
| --- | --- |
| `inherit` | The default: the folder's or the workspace's authentication |
| `none` | No credentials, even if a folder sets some |
| `basic` | `Authorization: Basic`, from `username` and `password` |
| `bearer` | `Authorization: Bearer <token>`; `prefix` changes `Bearer` |
| `api_key` | `key` and `value` in a header, or the query with `in: query` |
| `digest` | An answer to the server's Digest challenge |
| `oauth2` | An OAuth 2.0 access token |
| `oauth1` | An OAuth 1.0a signature |
| `jwt` | A JSON Web Token signed for each request |
| `aws_sigv4` | An AWS Signature Version 4 |
| `hawk` | A Hawk signature |
| `edgegrid` | An Akamai EdgeGrid signature |
| `asap` | An Atlassian ASAP token |

Signatures and signed tokens are made for each request after its
pre-request scripts run, so they cover exactly what is sent. The
response's **Sent** pane shows the headers and query parameters they
added, as it does for Digest and OAuth 2.0. Code exported with
**Copy as code** includes signatures as they would be made now, so it
works only for a short while; it leaves out OAuth 2.0 tokens and Digest
answers, which need a round trip.

## Digest

```yaml
auth:
  type: digest
  username: ada
  password: "{{password}}"
```

Getman sends the request, reads the server's challenge and sends it again
with the answer. It supports MD5, SHA-256 and SHA-512-256, and their
`-sess` forms, with `qop=auth` and `auth-int`. When the server offers
several, Getman answers the first one it supports.

## OAuth 2.0

```yaml
auth:
  type: oauth2
  grant: client_credentials        # or password, authorization_code, implicit
  token_url: https://id.example.com/oauth/token
  client_id: reporting
  client_secret: "{{clientSecret}}"
  scope: reports:read
```

| Field | |
| --- | --- |
| `grant` | `client_credentials` (default), `password`, `authorization_code` or `implicit` |
| `auth_url` | Authorization endpoint, for `authorization_code` and `implicit` |
| `token_url` | Token endpoint |
| `client_id`, `client_secret` | The client's credentials. Without a secret, the client is public and sends only its ID |
| `client_auth` | How the client sends them: `basic` (default, an `Authorization` header) or `body` |
| `username`, `password` | For the `password` grant |
| `scope`, `audience`, `resource` | Sent when set |
| `callback_url` | Where the provider sends the browser back; see below |
| `pkce` | `s256` or `plain`, for `authorization_code`; off when unset |
| `auth_params`, `token_params` | More parameters for the authorization and token requests, like headers: `name`, `value`, `enabled` |
| `prefix` | Written before the token. Default `Bearer`, or the token type the server names |
| `token_in` | `query` sends the token as `access_token` in the query instead of a header |

Client credentials and password tokens are fetched when a request needs
one and reused until 30 seconds before they expire. A token that came
with a refresh token is refreshed then; otherwise Getman asks for a new
one. These tokens live in memory only.

### Signing in with a browser

The `authorization_code` and `implicit` grants need you to sign in:

```yaml
auth:
  type: oauth2
  grant: authorization_code
  auth_url: https://id.example.com/authorize
  token_url: https://id.example.com/oauth/token
  client_id: getman-local
  pkce: s256
  scope: openid profile orders:write
```

In the browser UI, open the request's **Auth** tab and choose
**Get new token**. Getman opens your browser at the authorization URL and
listens for the provider to send it back. It checks the `state` it sent,
exchanges the code for a token (or, for `implicit`, takes the token the
browser was given) and keeps it. The exchange goes out the way the
request's other requests do: through the proxy, with the client
certificate and extra certificate authorities, and with the TLS and HTTP
version settings of its folder. The tab shows when the token expires,
and **Clear token** forgets it. In the [terminal UI](terminal.md#signing-in-with-a-browser),
press <kbd>b</kbd> on the auth pane; it shows the expiry too.

Getman listens on `http://127.0.0.1:<free port>/callback` unless
`callback_url` names a plain `http` address on this machine, such as
`http://127.0.0.1:8765/callback` or `http://localhost:8765/callback`; then
it listens there. Register that redirect URI with your provider.
RFC 8252 asks providers to accept any port on a loopback redirect URI;
for one that needs an exact address, set `callback_url`. A callback on
another host cannot reach Getman, so it is not used.

Tokens from a browser sign-in are saved in the workspace's
`.getman/oauth2-tokens.json`, which Git ignores, so they survive a
restart and `getman run` and `getman send` use them too. When one
expires, Getman refreshes it if the provider sent a refresh token. If
there is no token, or it cannot be refreshed, those commands fail with a
message telling you to get one in the UI first.

The listener accepts only `GET` on the callback path, reads at most
16 KiB from each connection, gives each connection 10 seconds, and stops
after five minutes or when you cancel. Answers without the right `state`
get an error page and are otherwise ignored. The `state` and the PKCE
verifier come from the operating system's random number generator.

## OAuth 1.0a

```yaml
auth:
  type: oauth1
  consumer_key: dpf43f3p2l4k3l03
  consumer_secret: "{{consumerSecret}}"
  token: "{{accessToken}}"
  token_secret: "{{tokenSecret}}"
```

| Field | |
| --- | --- |
| `signature_method` | `HMAC-SHA1` (default), `HMAC-SHA256`, `HMAC-SHA512`, `PLAINTEXT`, `RSA-SHA1`, `RSA-SHA256` or `RSA-SHA512` |
| `consumer_key`, `consumer_secret` | The client's credentials |
| `token`, `token_secret` | The access token; leave them empty to ask for a request token |
| `private_key`, `private_key_file` | For the RSA methods; see [Private keys](#private-keys) |
| `realm`, `callback`, `verifier` | Sent when set |
| `version` | `oauth_version`; `1.0` when unset |
| `timestamp`, `nonce` | Fixed values, for testing; otherwise the current time and a random nonce |
| `body_hash` | `true` adds `oauth_body_hash`, for bodies that are not forms |
| `in` | `header` (default), `query`, or `body` for a URL-encoded form |

The signature covers the method, the URL, the query and, for a
URL-encoded form, its fields.

## JWT bearer

```yaml
auth:
  type: jwt
  algorithm: RS256
  private_key: "{{signingKey}}"
  payload: |
    { "sub": "{{userId}}", "aud": "orders" }
  issued_at: true
  expires_in: 300
```

| Field | |
| --- | --- |
| `algorithm` | `HS256` (default), `HS384`, `HS512`, `RS256`, `RS384`, `RS512`, `PS256`, `PS384`, `PS512`, `ES256` or `ES384` |
| `secret` | For the HS algorithms; `secret_base64: true` if it is base64 |
| `private_key`, `private_key_file` | For the others; see [Private keys](#private-keys) |
| `payload` | The claims, as JSON |
| `header` | More header fields, as JSON, such as `{"kid": "key-1"}`. `alg` and `typ` are set for you |
| `issued_at` | `true` adds `iat`, the time of signing |
| `expires_in` | Adds `exp`, this many seconds from now |
| `in` | `header` (default) or `query` |
| `prefix` | Written before the token in the header. Default `Bearer` |
| `param` | The query parameter's name. Default `token` |

## AWS Signature Version 4

```yaml
auth:
  type: aws_sigv4
  access_key: "{{awsAccessKeyId}}"
  secret_key: "{{awsSecretAccessKey}}"
  region: eu-west-1
  service: execute-api
```

| Field | |
| --- | --- |
| `access_key`, `secret_key` | The credentials |
| `session_token` | For temporary credentials |
| `region`, `service` | Such as `us-east-1` and `s3` |
| `in` | `header` (default), or `query` for a presigned URL |
| `expires_in` | Seconds a presigned URL is valid. Default 3600 |
| `unsigned_payload` | `true` signs `UNSIGNED-PAYLOAD` instead of the body's hash, as S3 allows |

Getman signs the host, `X-Amz-Date`, `X-Amz-Security-Token` and the
request's headers, except those proxies may change, such as
`User-Agent`. For S3 it also sends `X-Amz-Content-Sha256`. A presigned URL
signs only the host.

Signatures that hash the body (AWS Signature, the OAuth 1.0a
`body_hash`, Hawk's `payload_hash` and EdgeGrid) cover multipart bodies
too: Getman builds the whole body, boundary included, before it signs, and
sends those same bytes.

## Hawk

```yaml
auth:
  type: hawk
  id: dh37fgj492je
  key: "{{hawkKey}}"
```

| Field | |
| --- | --- |
| `id`, `key` | The credentials |
| `algorithm` | `sha256` (default) or `sha1` |
| `ext` | Application data |
| `app`, `dlg` | For delegated credentials |
| `payload_hash` | `true` also signs the body and its content type |

## Akamai EdgeGrid

```yaml
auth:
  type: edgegrid
  client_token: akab-client-token-xxx
  client_secret: "{{edgegridSecret}}"
  access_token: akab-access-token-xxx
  base_url: https://akab-host.luna.akamaiapis.net
```

| Field | |
| --- | --- |
| `client_token`, `client_secret`, `access_token` | From your `.edgerc` |
| `base_url` | The `host` from your `.edgerc`. When set, requests go to it, keeping their path and query |
| `headers_to_sign` | Headers included in the signature, in order |
| `max_body` | Most body bytes a POST signature covers. Default 131072 |

## Atlassian ASAP

```yaml
auth:
  type: asap
  issuer: billing-service
  audience: ledger-service
  key_id: billing-service/key1
  private_key: "{{asapKey}}"
```

| Field | |
| --- | --- |
| `issuer`, `subject`, `audience` | The `iss`, `sub` and `aud` claims; separate several audiences with commas |
| `key_id` | The `kid` header |
| `private_key`, `private_key_file` | See [Private keys](#private-keys) |
| `algorithm` | `RS256` (default), or another RS, PS or ES algorithm |
| `claims` | More claims, as JSON |
| `expires_in` | Seconds the token is valid. Default 3600 |

Each request gets a new token, with a random `jti`, sent as
`Authorization: Bearer`.

## Private keys

JWT, ASAP and the RSA methods of OAuth 1.0a sign with a private key in
PEM form: PKCS#8 (`BEGIN PRIVATE KEY`), PKCS#1 (`BEGIN RSA PRIVATE KEY`)
or SEC1 (`BEGIN EC PRIVATE KEY`). RSA keys work with the RS and PS
algorithms, P-256 keys with ES256 and P-384 keys with ES384. Encrypted keys
are not read; decrypt them first.

- `private_key` holds the PEM text. Make it a variable from a
  `.local.yaml` file. A key written on one line with `\n` for line breaks,
  as environment variables often hold it, works too.
- `private_key_file` names a file inside the workspace instead, such as
  `keys/signing.pem`. Add it to `.gitignore`. Files outside the workspace
  are refused.

## With gRPC

[gRPC calls](grpc.md#metadata-and-authentication) send authentication as
metadata. Basic, Bearer, API key, OAuth 2.0, JWT and ASAP work as for HTTP:
a JWT or ASAP token goes in `authorization`, and a JWT set to go in the
query string goes under the name of its `param` instead. Digest does not
work with gRPC.

AWS Signature, OAuth 1.0a, Hawk and EdgeGrid sign the call as the
HTTP/2 request it is: a `POST` to `https://host/package.Service/Method`
(`http` for plaintext), your metadata as the headers, and the
length-prefixed protobuf message as the body. That covers unary and
server-streaming calls, which have one message. A stream from the client
(client and bidirectional streams) has no body known in advance, so
these sign without one:

| `type` | Streams from the client |
| --- | --- |
| `aws_sigv4` | Signs `UNSIGNED-PAYLOAD` and sends `X-Amz-Content-Sha256` to say so |
| `oauth1` | Sends no `oauth_body_hash`, whatever `body_hash` says |
| `hawk` | Sends no payload `hash`, whatever `payload_hash` says |
| `edgegrid` | Refused: it signs the body of every `POST` |

Server reflection is a stream too, so a request that reads its schema by
reflection signs that part the same way. Use a `.proto` file with
EdgeGrid. `in: query` for AWS Signature and `in: query` or `in: body` for
OAuth 1.0a are refused, since a gRPC call has no query string or form.
EdgeGrid's `base_url` is ignored: the call goes to the request's address,
and that is what the signature names.


`getman import` maps Postman's Basic, Bearer, API key, Digest, OAuth 1.0,
OAuth 2.0 (every grant), AWS Signature, JWT, Hawk, Akamai EdgeGrid and
ASAP settings. Saved OAuth 2.0 tokens are not imported, since they would
land in a shared file: get a new one under **Auth**. Postman's callback
URL cannot reach Getman, so it is dropped with a warning. See
[Import and export](import-export.md).
