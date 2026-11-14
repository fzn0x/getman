# Workspace file format

Every file in a Getman workspace and every key it can hold, with its type,
default and meaning. [Workspaces](workspaces.md) is the guide; this page is
the reference.

## Layout

```text
shop/
  getman.yaml                 the workspace file: marks the root
  getman.local.yaml           your overrides and globals (git-ignored)
  .gitignore                  written by getman init
  environments/
    staging.yaml              an environment
    staging.local.yaml        your values for it (git-ignored)
  orders/
    _folder.yaml              settings of the folder (optional)
    create-order.yaml         an HTTP request
    create-order.examples/    bodies of its saved responses
      receipt.pdf
    get-order.yaml            a gRPC request
  chat/
    room.yaml                 a WebSocket request
  protos/shop/orders.proto    a schema a gRPC request names
  fixtures/cover.png          a file a request body sends
  lib/helpers.js              a module scripts require
  data/users.csv              a data file for getman run --data
  .getman/                    Getman's own state (git-ignored)
```

| Path | Holds | Commit |
| --- | --- | --- |
| `getman.yaml` | [The workspace file](#the-workspace-file) | Yes |
| `getman.local.yaml` | [Your overrides, globals and the scripts switch](#the-local-workspace-file) | No |
| `environments/<id>.yaml` | [An environment](#environment-files) | Yes |
| `environments/<id>.local.yaml` | Your values for that environment | No |
| `<folder>/_folder.yaml` | [Settings of a folder](#folder-files) | Yes |
| `<name>.yaml` or `<name>.yml` | [A request](#request-files) | Yes |
| `<name>.examples/` | [Example bodies](#example-bodies) too long, or not text, for the request file | Yes |
| `.getman/` | [Getman's state](#getmans-state): history, runs, cookies, tokens, trash, header presets | No |
| Files that requests name | Body files, `.proto` schemas, `.js` modules, data files, certificates and private keys | Your choice. Keep private keys and client certificates out of git |

### The .gitignore

`getman init` writes this `.gitignore`:

```text
# Values for this machine only, and Getman's local state
*.local.yaml
.getman/
```

It writes it only when the directory has no `.gitignore` yet. An existing
one is left as it is, so add those two lines to it yourself.

The first time Getman writes to `.getman/`, it puts a `.gitignore` holding
`*` inside it. That keeps its state out of git whatever the repository's
own rules say. `*.local.yaml` files have no such guard: only the root
`.gitignore` keeps them out.

### How Getman finds files

- The workspace is the nearest directory, from the current one upwards,
  that holds `getman.yaml`. Commands take `-w DIR` to point elsewhere.
- Every directory inside it is a folder, except directories whose names
  start with `.` or `_`, `node_modules` and `target` at any depth,
  `environments` at the root, and names ending in `.examples`. Folders
  more than 32 levels deep are not read.
- A directory with no `_folder.yaml` and nothing to list is left out.
- A file is a request when its name ends in `.yaml` or `.yml`, does not
  start with `.` or `_`, and does not end in `.local.yaml`, and it holds a
  YAML mapping with a `url` or a `method` key. Other YAML files, such as a
  `compose.yaml`, are ignored.
- A request file that cannot be read is still listed, with its error, so
  a bad merge is easy to spot.
- `getman.yaml`, `_folder.yaml` and environment files must end in `.yaml`.
  Only request files may end in `.yml`.

## Names and order

| Item | Its name | When `name` is missing |
| --- | --- | --- |
| The workspace | `name` in `getman.yaml` | The directory's name |
| A folder | `name` in `_folder.yaml` | The directory's name |
| A request | `name` in the request file | The file name without its extension |
| An environment | `name` in its file | The file name without `.yaml`, which is also its id |

A folder's `order` lists its items by key: a request's file name without
its extension, or a folder's directory name. Keys must match exactly,
capitals included, and keys that match nothing are ignored. Listed items
come first, in that order. The rest follow: folders first, then requests,
each sorted by key without regard to case.

```yaml
# getman.yaml
order: [orders, auth, health]
```

Once a folder has an `order`, Getman keeps it up to date. A new item joins
the end, a copy goes right after its original, a renamed item keeps its
place, and a deleted one leaves the list. Dragging items in the UI writes
the list. A folder without `order` gets none until you drag something.

Environments are listed by name, without regard to case.

### File names Getman chooses

When you create, rename or copy an item in the UI, its file name comes from
its name:

- Letters and digits, in lowercase, with one `-` for each run of anything
  else: `Create user (v2)!` becomes `create-user-v2`. Letters of any
  alphabet are kept.
- At most 60 characters. A name without letters or digits becomes
  `untitled`.
- Names Windows keeps for devices, such as `con`, `nul` or `com1`, get
  `-item` added.
- A name already taken by a request or a folder gets `-2`, `-3` and so on,
  so Getman never gives a request and a folder in the same folder one key.
- A copy is named `<name> copy`.

Renaming an item renames its file to match, unless the new name makes the
same file name, as when only capitals change.
Getman writes request files as `.yaml`, so renaming a `.yml` request saves
it as `.yaml`. A request's `.examples` directory is renamed, moved and
copied with it. Environments get their ids the same way.

## Reading and writing

The tables on this page give each key one of these types:

| Type | Means |
| --- | --- |
| text | Any scalar. A number or boolean is read as text, and an empty value or `~` as empty text |
| string | YAML text. See below for when it must be quoted |
| boolean | `true` or `false` |
| integer | A whole number, not quoted |
| pairs | A list of [name and value pairs](#name-and-value-pairs) |
| map | Names and their text values, in the order written |
| one of | One of the words listed, as written |

### What is forgiven

- An empty file reads as one with every key at its default.
- Unknown keys are ignored, at every level, with no warning. A misspelt key
  is ignored too, so check its spelling when a setting seems to do nothing.
- Keys of type text take any scalar. Map keys and values do too, so
  `port: 3000` and `debug: true` are fine under `variables`. A number is
  read as a number first, then turned back into text: `1.0` becomes `1`,
  `1e3` becomes `1000` and `0x10` becomes `16`. Quote values whose exact
  spelling matters, such as `apiVersion: "1.0"`.
- Keys of type string take an unquoted number or boolean exactly as
  written, except inside an `auth` or `body` block. There it is an error,
  so quote it: `scope: "5"`, `version: "1.0"`.
- `kind` ignores capitals and spaces around it, and an empty `kind` means
  `http`.
- `method` is sent in capitals: `post` sends `POST`.
- `proxy: false` means `off`, and `proxy: true` means `system`.
- A list or map left empty, such as `headers:` with nothing after it, is
  empty.

### What is an error

- A boolean that is not `true` or `false`. `yes`, `no`, `on` and `off` are
  text, so `enabled: yes` fails.
- A quoted number where an integer goes: `timeout_ms: "500"` fails.
- `checks` written as one line instead of a list.
- A name and value pair without a `name`.
- `body:` or `auth:` with nothing after it. Each needs a `type`.
- An unknown `type` of `body` or `auth`, or an unknown `kind`. The message
  lists the ones this version knows. A `kind` written by a newer version
  fails to load rather than being sent as HTTP.

The message names the file and the key, and usually the line and column:

```text
error: /home/ada/shop/orders/create-order.yaml: headers[0].enabled: invalid type: string "yes", expected a boolean at line 5 column 14
```

A request file with an error shows in the tree with that message. An
error in a `_folder.yaml` stops every request inside the folder from being
sent, with the same kind of message. An error in an environment file stops
requests sent with that environment.

### When Getman saves

Getman saves a file by writing it whole, from what it read:

- Keys come out in a fixed order, and most keys at their default are left
  out.
- Comments, unknown keys and your own layout are not kept.
- The new text goes to a hidden `.<file>.tmp` beside the file, which is
  then renamed over it, so a crash never leaves half a file.

Getman rewrites a request file when you save it in the UI, rename, move or
copy it, or save, rename or delete one of its examples. It rewrites a
`_folder.yaml` or `getman.yaml` when you edit or rename the folder, or when
its `order` changes. It rewrites an environment file when you edit the
environment in the UI. Files you only edit by hand keep their comments.

## The workspace file

`getman.yaml` marks the root of the workspace and holds what every request
inherits. It has the same keys as a [folder file](#folder-files), and
`getman init` writes only `name`. Its `variables` are the workspace
variables.

```yaml
# getman.yaml
name: Shop
description: |
  Requests for the Shop API.
variables:
  baseUrl: https://api.example.com
  apiVersion: "2"
headers:
  - name: X-Api-Version
    value: "{{apiVersion}}"
auth:
  type: bearer
  token: "{{token}}"
scripts:
  pre_request: |
    gm.request.headers.upsert({ key: "X-Request-Id", value: gm.variables.replaceIn("{{$uuid}}") });
settings:
  timeout_ms: 10000
order: [orders, health]
```

## Folder files

A `_folder.yaml` holds what every request inside the folder, at any depth,
inherits. A folder works without one.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `name` | string | The directory's name | Name shown in the tree |
| `description` | string | Empty | Markdown about the folder |
| `variables` | map | None | Variables for every request inside |
| `headers` | pairs | None | Headers added to every request inside |
| `auth` | [auth block](#authentication) | `inherit` | Authentication for every request inside whose own is `inherit` |
| `scripts` | [scripts](#scripts) | None | Scripts run around every request inside |
| `settings` | [settings](#settings) | None | Transport settings for every request inside |
| `order` | list of strings | None | Order of the folder's items; see [Names and order](#names-and-order) |

```yaml
# orders/_folder.yaml
name: Orders
description: Create, read and cancel orders.
variables:
  customerId: c-1001
headers:
  - name: Accept
    value: application/json
settings:
  verify_tls: true
order: [create-order, get-order]
```

### How a request inherits

A request inherits from `getman.yaml`, then from each folder above it,
outermost first, then adds its own:

- **Variables.** Each level is a scope, and the inner one wins.
  [Variables and environments](variables.md#where-values-come-from) gives
  the full order, environments included.
- **Headers.** The enabled headers of every level are combined. A header
  replaces an outer one of the same name, in any case. A disabled header
  does not hide an outer one.
- **Authentication.** The nearest level whose `auth` is not `inherit`
  wins. When every level inherits, no credentials are sent.
- **Scripts.** Every level's run, outermost first: the workspace's, each
  folder's, then the request's own.
- **Settings.** Each key on its own: the nearest level that sets it wins,
  and a key no level sets takes its default. A list, such as `ca_certs`,
  replaces an outer one whole.

## Request files

A request file holds one request. `kind` says what it is: an HTTP request
unless it says otherwise. Every key but `url` is optional.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `name` | string | The file name | Name shown in the tree |
| `kind` | one of `http`, `grpc`, `websocket`, `sse`, `socketio` | `http` | What the file holds |
| `method` | string | `GET` | The HTTP method; any token works. For gRPC, the method's full name. WebSocket and Socket.IO always connect with `GET` |
| `url` | text | Empty | The address, with the enabled query parameters, `:name` path segments and `{{variables}}`. An HTTP URL without a scheme gets `http://` |
| `grpc` | [gRPC block](#grpc-requests) | None | Schema, TLS and message of a gRPC request |
| `description` | string | Empty | Markdown, shown in the Docs tab |
| `path_params` | map | None | Values for the `:name` segments of the URL's path, percent-encoded when sent |
| `disabled_params` | pairs | None | Query parameters kept but switched off, so not in `url`. The UI writes `enabled: false` on each |
| `headers` | pairs | None | Request headers. For gRPC, the metadata |
| `body` | [body block](#bodies) | None | The body, for HTTP and Server-Sent Events requests |
| `auth` | [auth block](#authentication) | `inherit` | How the request authenticates |
| `variables` | map | None | Variables for this request only |
| `scripts` | [scripts](#scripts) | None | Its pre-request and post-response scripts |
| `checks` | list of strings | None | [Checks](#checks-and-captures) run on the response |
| `captures` | map | None | [Values copied](#checks-and-captures) from the response into variables |
| `settings` | [settings](#settings) | None | Transport settings, over the folders' |
| `examples` | list of [examples](#examples) | None | Saved responses |
| `websocket` | [WebSocket block](#websocket-requests) | None | Subprotocols and saved messages |
| `sse` | [SSE block](#server-sent-events-requests) | None | Settings of an event stream |
| `socketio` | [Socket.IO block](#socketio-requests) | None | Namespace, transport, events and saved events |
| `until` | [limits](#connection-limits) | None | When a run ends a WebSocket, event stream or Socket.IO connection |

Getman always writes `method` and `url`. Most keys work the same for every
kind; the sections on [HTTP](#http-requests), [gRPC](#grpc-requests),
[WebSocket](#websocket-requests),
[Server-Sent Events](#server-sent-events-requests) and
[Socket.IO](#socketio-requests) say what differs.

### Name and value pairs

Headers, disabled query parameters, form fields, the extra OAuth 2.0
parameters and the headers of examples are lists of pairs:

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `name` | string | Required | The name |
| `value` | text | Empty | The value. May hold `{{variables}}` |
| `enabled` | boolean | `true` | `false` keeps the pair in the file without sending it |
| `description` | string | Empty | A note for people reading the file |

```yaml
headers:
  - name: Idempotency-Key
    value: "{{$uuid}}"
    description: A new key for each attempt
  - name: X-Trace
    value: "1"
    enabled: false
```

### Bodies

`body` has a `type` and the keys that type takes. Paths in `file` are
relative to the workspace root and must stay inside it. A request's own
`Content-Type` header wins over the one its body type sends.

| `type` | Key | Type | Default | Description |
| --- | --- | --- | --- | --- |
| `none` | | | | No body. The default |
| `json` | `content` | string | Empty | JSON text, sent as `application/json` |
| `xml` | `content` | string | Empty | XML text, sent as `application/xml` |
| `text` | `content` | string | Empty | Any text |
| | `content_type` | string | `text/plain` | The type to send instead |
| `form` | `fields` | pairs | None | Sent as `application/x-www-form-urlencoded`, without the disabled fields |
| `multipart` | `fields` | list of form fields | None | Sent as `multipart/form-data`; see below |
| `binary` | `file` | string | Empty | A file whose bytes are the body |
| | `content_type` | string | From the file name | The type to send; `application/octet-stream` when the name says nothing |
| `graphql` | `query` | string | Empty | The query or mutation, sent as JSON; see [GraphQL](#graphql) |
| | `variables` | string | Empty | The operation's variables, as JSON text. It must be valid JSON once `{{variables}}` are filled in |
| | `operation_name` | string | Empty | Which operation of `query` to run, sent as `operationName` |

A multipart field is text or a file:

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `name` | string | Required | The field's name |
| `value` | text | Empty | Its text, when there is no `file` |
| `file` | string | None | A file to upload |
| `content_type` | string | From the file name | The part's content type |
| `enabled` | boolean | `true` | `false` keeps the field without sending it |

```yaml
body:
  type: multipart
  fields:
    - name: caption
      value: Front cover
    - name: image
      file: fixtures/cover.png
      content_type: image/png
```

[Workspaces](workspaces.md#bodies) says how multipart bodies are built.

### Authentication

`auth` has a `type` and the keys that type takes. Every text key may hold
`{{variables}}`. Keys that hold secrets should name a variable kept in a
`*.local.yaml` file. [Authentication](auth.md) explains how each type
works.

| `type` | Keys |
| --- | --- |
| `inherit` | None. The default: the nearest folder's, or the workspace's |
| `none` | None. Sends no credentials, even when a folder sets some |
| `basic` | `username`, `password` |
| `bearer` | `token`, `prefix` |
| `api_key` | `key`, `value`, `in` |
| `digest` | `username`, `password` |
| `oauth2` | `grant`, `token_url`, `client_id`, `client_secret` and more, below |
| `oauth1` | `consumer_key`, `consumer_secret`, `token`, `token_secret` and more, below |
| `jwt` | `algorithm`, `secret` or `private_key`, `payload` and more, below |
| `aws_sigv4` | `access_key`, `secret_key`, `region`, `service` and more, below |
| `hawk` | `id`, `key` and more, below |
| `edgegrid` | `client_token`, `client_secret`, `access_token` and more, below |
| `asap` | `issuer`, `audience`, `key_id`, `private_key` and more, below |

| `type` | Key | Type | Default | Description |
| --- | --- | --- | --- | --- |
| `basic` | `username` | text | Empty | User name |
| | `password` | text | Empty | Password |
| `bearer` | `token` | text | Empty | The token |
| | `prefix` | string | `Bearer` | The scheme written before it |
| `api_key` | `key` | string | Empty | Header or query parameter name |
| | `value` | text | Empty | The key |
| | `in` | one of `header`, `query` | `header` | Where it goes |
| `digest` | `username` | text | Empty | User name |
| | `password` | text | Empty | Password; see [Digest](auth.md#digest) |

Getman writes the keys of `basic`, `bearer`, `api_key` and `digest` even
when they are empty, except `prefix`.

**`type: oauth2`**, described in [OAuth 2.0](auth.md#oauth-20):

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `grant` | one of `client_credentials`, `password`, `authorization_code`, `implicit` | `client_credentials` | How the token is got |
| `auth_url` | text | Empty | Authorization endpoint, for `authorization_code` and `implicit` |
| `token_url` | text | Empty | Token endpoint |
| `client_id` | text | Empty | The client's ID |
| `client_secret` | text | Empty | The client's secret. Without one, the client is public |
| `callback_url` | string | `http://127.0.0.1:<free port>/callback` | Where the provider sends the browser back; see [Signing in with a browser](auth.md#signing-in-with-a-browser) |
| `pkce` | one of `off`, `s256`, `plain` | `off` | Proof Key for Code Exchange, for `authorization_code` |
| `scope` | string | Empty | Scopes, separated by spaces |
| `audience` | string | Empty | Sent when set |
| `resource` | string | Empty | A resource indicator, sent when set |
| `username`, `password` | text | Empty | For the `password` grant |
| `client_auth` | one of `basic`, `body` | `basic` | How the client sends its credentials to the token endpoint |
| `auth_params` | pairs | None | More parameters for the authorization request |
| `token_params` | pairs | None | More parameters for token requests |
| `prefix` | string | `Bearer`, or the token type the server names | Written before the token |
| `token_in` | one of `header`, `query` | `header` | `query` sends the token as `access_token` |

Getman always writes `grant`, `token_url`, `client_id` and `client_auth`.

**`type: oauth1`**, described in [OAuth 1.0a](auth.md#oauth-10a):

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `signature_method` | one of `HMAC-SHA1`, `HMAC-SHA256`, `HMAC-SHA512`, `PLAINTEXT`, `RSA-SHA1`, `RSA-SHA256`, `RSA-SHA512` | `HMAC-SHA1` | How the request is signed |
| `consumer_key` | text | Empty | The client's key |
| `consumer_secret` | text | Empty | The client's secret, for the HMAC methods and `PLAINTEXT` |
| `token`, `token_secret` | text | Empty | The access token and its secret |
| `private_key` | text | Empty | A PEM private key, for the RSA methods |
| `private_key_file` | string | Empty | A PEM file in the workspace, instead of `private_key` |
| `realm` | string | Empty | Sent in the header only |
| `callback` | string | Empty | `oauth_callback`, when asking for a request token |
| `verifier` | text | Empty | `oauth_verifier`, when exchanging a request token |
| `version` | string | `1.0` | `oauth_version` |
| `timestamp` | text | The current time | A fixed `oauth_timestamp`, for testing |
| `nonce` | text | Random | A fixed `oauth_nonce`, for testing |
| `body_hash` | boolean | `false` | `true` adds `oauth_body_hash` |
| `in` | one of `header`, `query`, `body` | `header` | Where the parameters go; `body` for a URL-encoded form |

Getman always writes `consumer_key`.

**`type: jwt`**, described in [JWT bearer](auth.md#jwt-bearer):

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `algorithm` | one of `HS256`, `HS384`, `HS512`, `RS256`, `RS384`, `RS512`, `PS256`, `PS384`, `PS512`, `ES256`, `ES384` | `HS256` | The signature algorithm |
| `secret` | text | Empty | The secret, for the HS algorithms |
| `secret_base64` | boolean | `false` | `true` when `secret` is base64 |
| `private_key` | text | Empty | A PEM private key, for the others |
| `private_key_file` | string | Empty | A PEM file in the workspace, instead of `private_key` |
| `header` | string | Empty | More header fields, as JSON. `alg` and `typ` are set for you |
| `payload` | string | Empty | The claims, as JSON |
| `issued_at` | boolean | `false` | `true` adds `iat` |
| `expires_in` | integer | None | Adds `exp`, this many seconds from now |
| `in` | one of `header`, `query` | `header` | Where the token goes |
| `prefix` | string | `Bearer` | Written before the token in the header |
| `param` | string | `token` | The query parameter's name |

**`type: aws_sigv4`**, described in [AWS Signature Version 4](auth.md#aws-signature-version-4):

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `access_key`, `secret_key` | text | Empty | The credentials |
| `session_token` | text | Empty | For temporary credentials |
| `region` | text | Empty | Such as `eu-west-1` |
| `service` | text | Empty | Such as `s3` or `execute-api` |
| `in` | one of `header`, `query` | `header` | `query` makes a presigned URL |
| `expires_in` | integer | 3600 | Seconds a presigned URL is valid |
| `unsigned_payload` | boolean | `false` | `true` signs `UNSIGNED-PAYLOAD` instead of the body's hash |

Getman always writes `access_key`, `secret_key`, `region` and `service`.

**`type: hawk`**, described in [Hawk](auth.md#hawk):

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `id`, `key` | text | Empty | The credentials |
| `algorithm` | one of `sha256`, `sha1` | `sha256` | The MAC algorithm |
| `ext` | text | Empty | Application data |
| `app`, `dlg` | text | Empty | For delegated credentials |
| `payload_hash` | boolean | `false` | `true` also signs the body and its content type |

Getman always writes `id` and `key`.

**`type: edgegrid`**, described in [Akamai EdgeGrid](auth.md#akamai-edgegrid):

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `client_token`, `client_secret`, `access_token` | text | Empty | From your `.edgerc` |
| `base_url` | text | Empty | The `host` from your `.edgerc`. When set, requests go there, keeping their path and query |
| `headers_to_sign` | list of strings | None | Headers the signature covers, in order |
| `max_body` | integer | 131072 | Most body bytes a `POST` signature covers |

Getman always writes `client_token`, `client_secret` and `access_token`.

**`type: asap`**, described in [Atlassian ASAP](auth.md#atlassian-asap):

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `algorithm` | one of the RS, PS and ES algorithms of `jwt` | `RS256` | The signature algorithm |
| `issuer` | text | Empty | The `iss` claim |
| `subject` | text | Empty | The `sub` claim, left out when empty |
| `audience` | text | Empty | The `aud` claim; separate several with commas |
| `key_id` | text | Empty | The `kid` header |
| `private_key` | text | Empty | A PEM private key |
| `private_key_file` | string | Empty | A PEM file in the workspace, instead of `private_key` |
| `claims` | string | Empty | More claims, as JSON |
| `expires_in` | integer | 3600 | Seconds the token is valid |

Getman always writes `issuer`, `audience` and `key_id`.

[Private keys](auth.md#private-keys) lists the key formats `private_key`
and `private_key_file` accept. A `private_key_file` outside the workspace is
refused.

### Settings

Transport settings. Each one left unset comes from the nearest folder that
sets it, then from the workspace, then from the default below. See
[HTTP and connections](http.md).

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `timeout_ms` | integer | 30000 | Milliseconds before giving up; `0` waits forever |
| `follow_redirects` | boolean | `true` | Follow redirects |
| `max_redirects` | integer | 10 | Most redirects to follow |
| `follow_original_method` | boolean | `false` | `true` keeps the method and body across a 301 or 302 instead of turning a `POST` into a `GET` |
| `follow_authorization` | boolean | `false` | `true` keeps `Authorization` when a redirect leads to another host, port or scheme |
| `verify_tls` | boolean | `true` | `false` accepts self-signed and expired certificates |
| `http_version` | one of `auto`, `http1`, `http2`, `http3` | `auto` | See [HTTP versions](http.md#http-versions) |
| `title_case_headers` | boolean | `true` | Sends HTTP/1.1 header names as `Content-Type`. HTTP/2 and HTTP/3 always send lowercase |
| `proxy` | proxy | `system` | The proxy; see below |
| `client_cert` | client certificate | None | The certificate to present when a server asks for one; see below |
| `ca_certs` | list of strings | None | PEM files of certificate authorities to trust besides the system's |
| `cookies` | boolean | `true` | `false` leaves the cookie jar alone |
| `max_response_mb` | integer | 100 | Largest response body to read, in MiB; the rest is cut off. `0` reads it all |

Not every kind uses every setting:

- **gRPC** uses `timeout_ms` as the call's deadline, and `verify_tls`,
  `ca_certs`, `client_cert` and `proxy`. See
  [Deadlines and TLS](grpc.md#deadlines-and-tls).
- **WebSocket**, and Socket.IO over WebSocket, use `timeout_ms` for
  connecting, and `verify_tls`, `proxy` and `cookies`. They neither trust
  `ca_certs` nor present a `client_cert`.
- **Server-Sent Events**, and Socket.IO over long polling, go through the
  HTTP client with its settings.

`proxy` is a word, a URL or a map. See [Proxies](http.md#proxies).

| Form | Means |
| --- | --- |
| `proxy: system` | The system's proxy settings and the proxy environment variables. The default, and a way to undo a folder's proxy |
| `proxy: off` | No proxy |
| `proxy: socks5h://127.0.0.1:1080` | This proxy: `http://`, `https://`, `socks5://` or `socks5h://` |
| A map | A proxy with credentials or a bypass list, with the keys below |

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `url` | text | `system` | `system`, `off` or the proxy's URL |
| `username`, `password` | text | Empty | Credentials for the proxy |
| `bypass` | text | Empty | Hosts reached without the proxy, separated by commas, as in `NO_PROXY` |

`client_cert` names PEM files or a PKCS#12 file. `client_cert: {}` stops a
folder's certificate from being sent. See
[Client certificates](http.md#client-certificates).

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `cert` | string | Empty | PEM file with the certificate, then any intermediates |
| `key` | string | Empty | PEM file with the unencrypted private key; may be the same file as `cert` |
| `pkcs12` | string | Empty | A `.p12` or `.pfx` file, instead of `cert` and `key` |
| `passphrase` | text | Empty | The PKCS#12 file's passphrase |

Paths in `ca_certs` and `client_cert` are relative to the workspace root
and must stay inside it. They, and the proxy's keys, may hold
`{{variables}}`; one that is not defined stops the request instead of
being sent as written.

```yaml
settings:
  timeout_ms: 5000
  http_version: http2
  proxy:
    url: http://proxy.internal:3128
    username: "{{proxyUser}}"
    password: "{{proxyPassword}}"
    bypass: localhost, .internal.example.com
  ca_certs: [certs/company-root.pem]
  client_cert:
    pkcs12: certs/me.p12
    passphrase: "{{p12Passphrase}}"
```

### Scripts

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `pre_request` | string | Empty | JavaScript run before the request is sent; it may change it |
| `post_response` | string | Empty | JavaScript run after the response arrives; usually holds tests |

A script of only spaces and line breaks counts as empty. Scripts may
`require` `.js` files of the workspace; see
[Your own modules](scripts.md#your-own-modules) and [Scripts](scripts.md).

### Checks and captures

`checks` is a list of one-line assertions, such as `status == 200`. Empty
lines and lines starting with `#` are skipped. `captures` maps a variable's
name to a target of the response, such as `json.id`, whose value it
copies into that variable. [Checks and captures](checks.md) has the
targets and operators.

```yaml
checks:
  - status == 201
  - json.id exists
captures:
  orderId: json.id
```

### Examples

`examples` lists saved responses. They show what an API answers, and
`getman mock` serves them; see [Examples and mocks](examples-and-mocks.md).

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `name` | string | Empty | The example's name, unique within the request |
| `status` | integer | 200 | The status code |
| `headers` | pairs | None | Response headers |
| `body` | text | Empty | The body, when it is short text |
| `body_file` | string | None | A file in the request's `.examples` directory that holds the body; see [Example bodies](#example-bodies) |
| `request` | example request | None | The request that got this response |

| `request` key | Type | Default | Description |
| --- | --- | --- | --- |
| `method` | string | Empty | The method |
| `url` | text | Empty | The URL, with `{{variables}}` as written. `getman mock` matches requests against it |
| `headers` | pairs | None | Request headers |
| `body` | text | Empty | The request's body, when it was text |

Getman always writes `name` and `status`.

### Connection limits

`until` says when `getman run` ends a WebSocket, event stream or Socket.IO
connection. The first limit reached ends it.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `messages` | integer | 10 when `until` is missing | End after this many received messages. `0` ends the connection once it opens and the saved messages are sent |
| `time_ms` | integer | 5000 when `until` is missing, 60000 otherwise | End this many milliseconds after the connection opened. `0` counts as unset |

`getman send` applies `until` only without `--until`. The browser ignores
it. See [Stopping a run](realtime.md#stopping-a-run).

## HTTP requests

An HTTP request has no `kind`. It uses `method`, `url`, `body` and every
other key above except the `grpc`, `websocket`, `sse`, `socketio` and
`until` blocks.

```yaml
# orders/create-order.yaml
name: Create order
method: POST
url: "{{baseUrl}}/orders/:customerId?notify=true"
description: |
  Places an order for the customer and returns it.
path_params:
  customerId: "{{customerId}}"
disabled_params:
  - name: debug
    value: "1"
    enabled: false
headers:
  - name: Idempotency-Key
    value: "{{$uuid}}"
    description: A new key for each attempt
  - name: X-Trace
    value: "1"
    enabled: false
body:
  type: json
  content: |
    {
      "sku": "{{sku}}",
      "quantity": 2
    }
variables:
  sku: BOOK-42
scripts:
  post_response: |
    gm.test("returns the order", () => {
      gm.expect(gm.response.json().sku).to.equal(gm.variables.get("sku"));
    });
checks:
  - status == 201
  - header.content-type contains json
  - json.id exists
captures:
  orderId: json.id
settings:
  timeout_ms: 5000
  follow_redirects: false
examples:
  - name: Created
    status: 201
    headers:
      - name: Content-Type
        value: application/json
    body: |
      { "id": 42, "sku": "BOOK-42", "quantity": 2 }
    request:
      method: POST
      url: "{{baseUrl}}/orders/:customerId?notify=true"
```

This request has no `auth`, so it inherits the workspace's.

### GraphQL

A GraphQL request is an HTTP request with a `graphql` body. See
[GraphQL](http.md#graphql).

```yaml
# catalog/search-books.yaml
name: Search books
method: POST
url: "{{baseUrl}}/graphql"
body:
  type: graphql
  query: |
    query Search($term: String!, $first: Int) {
      books(search: $term, first: $first) { id title }
    }
  variables: |
    { "term": "{{term}}", "first": 5 }
  operation_name: Search
variables:
  term: rust
checks:
  - status == 200
  - json.errors !exists
  - json.data.books.length > 0
captures:
  firstBookId: json.data.books[0].id
```

## gRPC requests

`kind: grpc` makes a gRPC call. `method` is the method's full name, `url`
is the server's address, `headers` are sent as metadata, and the `grpc`
block holds the rest. `body` is not used. See [gRPC](grpc.md).

| `grpc` key | Type | Default | Description |
| --- | --- | --- | --- |
| `proto` | string | Empty | A `.proto` file or a folder of them, relative to the workspace root. Empty asks the server through reflection |
| `tls` | boolean | `false` | Connect with TLS. Addresses starting with `https://` always do |
| `message` | text | Empty | The message as JSON, with `{{variables}}`. For a stream from the client, several messages one after another |

```yaml
# orders/get-order.yaml
name: Get order
kind: grpc
method: shop.Orders/GetOrder
url: "{{ordersHost}}"
grpc:
  proto: protos/shop/orders.proto
  message: |
    { "id": "{{orderId}}" }
headers:
  - name: x-tenant
    value: acme
auth:
  type: bearer
  token: "{{token}}"
checks:
  - status == 0
  - json.state == SHIPPED
captures:
  trackingCode: json.tracking.code
settings:
  timeout_ms: 3000
```

`url` is `host:port`, or `https://host:port` for TLS.

## WebSocket requests

`kind: websocket` opens a WebSocket. `url` is `ws://` or `wss://`;
`http://` and `https://` are taken as those. Headers and authentication go
with the handshake, and `body` is not used. Digest authentication is
refused. See [WebSocket](realtime.md#websocket).

| `websocket` key | Type | Default | Description |
| --- | --- | --- | --- |
| `subprotocols` | list of strings | None | Offered in `Sec-WebSocket-Protocol`, preferred first |
| `messages` | list of messages | None | Saved messages. `getman run` sends them all, in order, once connected |

| Message key | Type | Default | Description |
| --- | --- | --- | --- |
| `name` | string | Empty | The name it is picked by |
| `type` | one of `text`, `json`, `binary` | `text` | `json` must be valid JSON when sent. `binary` content is Base64 |
| `content` | text | Empty | The message, with `{{variables}}` |

```yaml
# chat/room.yaml
name: Chat room
kind: websocket
url: "wss://{{host}}/chat?room={{room}}"
headers:
  - name: X-Client
    value: getman
variables:
  room: lobby
websocket:
  subprotocols: [chat.v2]
  messages:
    - name: Join
      type: json
      content: '{"type": "join", "room": "{{room}}"}'
    - name: Ping bytes
      type: binary
      content: AAEC
until:
  messages: 3
  time_ms: 5000
checks:
  - status == 101
  - header.sec-websocket-protocol == chat.v2
  - messages.length == 3
  - messages[0].json.type == welcome
captures:
  lastEcho: messages[1].json.data
settings:
  timeout_ms: 5000
```

## Server-Sent Events requests

`kind: sse` reads a `text/event-stream` response. It is an HTTP request in
every other way: it takes a `method`, a `body` and any authentication,
Digest included. `url` must be `http://` or `https://`. See
[Server-Sent Events](realtime.md#server-sent-events).

| `sse` key | Type | Default | Description |
| --- | --- | --- | --- |
| `reconnect` | boolean | `false` | Connect again when the stream ends, sending the last event's id in `Last-Event-ID` |

```yaml
# prices/feed.yaml
name: Price feed
kind: sse
url: "{{baseUrl}}/prices/stream"
sse:
  reconnect: true
until:
  messages: 3
checks:
  - status == 200
  - messages.length == 3
  - messages[0].event == price
  - messages[0].json.sku == BOOK-42
scripts:
  post_response: |
    gm.test("prices are numbers", () => {
      gm.expect(gm.messages.json(0).price).to.be.a("number");
    });
```

## Socket.IO requests

`kind: socketio` connects to a Socket.IO v4 server. `url` is `http://`,
`https://`, `ws://` or `wss://`. `method` and `body` are not used, and
Digest authentication is refused. See [Socket.IO](realtime.md#socketio).

| `socketio` key | Type | Default | Description |
| --- | --- | --- | --- |
| `namespace` | string | The URL's path, or `/` | The namespace to join, such as `/orders` |
| `path` | string | `/socket.io/` | The path the server answers on |
| `auth` | string | Empty | A JSON object sent when joining, read by the server as `socket.handshake.auth` |
| `transport` | one of `auto`, `websocket`, `polling` | `auto` | See [Long polling and WebSocket](realtime.md#long-polling-and-websocket) |
| `events` | list of strings | None | Events to show; every event when empty |
| `messages` | list of events | None | Saved events. `getman run` emits them all, in order, once connected |

| Event key | Type | Default | Description |
| --- | --- | --- | --- |
| `name` | string | Empty | The name it is picked by |
| `event` | string | Empty | The event to emit |
| `args` | text | Empty | Its arguments: a JSON array, or one JSON value for a single argument |
| `ack` | boolean | `false` | Ask the server to acknowledge it |

```yaml
# live/order-updates.yaml
name: Order updates
kind: socketio
url: "{{baseUrl}}"
socketio:
  namespace: /orders
  auth: '{"token": "{{token}}"}'
  transport: websocket
  events: [welcome, order:updated]
  messages:
    - name: Subscribe
      event: subscribe
      args: '["eu-west", {"limit": 10}]'
      ack: true
until:
  messages: 2
  time_ms: 10000
checks:
  - status == 101
  - messages.length == 2
  - messages[0].event == welcome
  - messages[1].json[0].region == eu-west
captures:
  userId: messages[0].json[0].user.id
```

With `transport: websocket` the handshake answers `101`. With `auto` or
`polling`, the first answer is a long-polling `200`.

## Environment files

An environment is `environments/<id>.yaml`. Commands name it by its id, as
in `getman run -e staging`, or by its `name`.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `name` | string | The id | Name shown in the UI |
| `variables` | map | None | The environment's variables |
| `secrets` | list of strings | None | Names of variables whose values stay out of this file |

Beside it, `environments/<id>.local.yaml` holds your own values. It has one
key:

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `variables` | map | None | Values that win over the shared file's |

```yaml
# environments/staging.yaml
name: Staging
variables:
  baseUrl: https://staging.example.com
  token: ""
secrets: [token]
```

```yaml
# environments/staging.local.yaml
variables:
  token: eyJhbGciOiJIUzI1NiJ9.e30.dev-only
```

When the UI saves an environment, the value of each variable listed in
`secrets` goes to the local file, and the shared file keeps it empty.
`name` is left out when it equals the id, and the local file is deleted
when it would be empty. Scripts that call `gm.environment.set`, and
captures in the UI, write to the local file too. Environment files whose
names start with `.` or `_` are not listed.

## The local workspace file

`getman.local.yaml` holds values for the whole workspace that stay on
your machine. Getman deletes it when nothing is left in it.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `variables` | map | None | Values that win over the variables of `getman.yaml` |
| `globals` | map | None | Global variables, the weakest scope |
| `scripts_off` | boolean | `false` | `true` sends requests without running their scripts; see [Turning scripts off](scripts.md#turning-scripts-off) |

```yaml
# getman.local.yaml
variables:
  baseUrl: http://localhost:3000
globals:
  region: eu-west-1
scripts_off: true
```

## Example bodies

An example's body is written inline, in `body`, when it is UTF-8 text of
at most 32 KiB with no control characters other than tabs and line
breaks. Any other body goes to a file in the `<request>.examples`
directory beside the request file, and the example names it in
`body_file`:

```text
orders/
  create-order.yaml           examples: [{ name: Receipt, body_file: receipt.pdf }]
  create-order.examples/
    receipt.pdf
```

- The file is named after the example, made safe as file names are, with
  `-2` and so on when the name is taken.
- Its extension comes from the example's `Content-Type`, such as `json`,
  `xml`, `html`, `pdf` or `png`. It is `txt` or `bin` when the type says
  nothing, and never `yaml`, so a body is never read as a request.
- `body_file` must be a plain file name in that directory.
- Deleting or replacing an example deletes its file, and the directory once
  it is empty. Renaming an example keeps the file's name.
- The directory never shows as a folder. Commit it with the request.

## Data files

A data file gives a run one iteration per row; see
[Data files](runner.md#data-files). Getman reads data files but never
writes or lists them, so keep them anywhere. In the workspace they can be
committed with the requests that use them.

- `getman run --data FILE` reads `FILE` as given, from the current
  directory. A name ending in `.json` is read as JSON, anything else as
  CSV.
- The Runner in the browser reads the file you choose as JSON when its
  text starts with `[`, and as CSV otherwise.
- CSV: the first row names the columns, with spaces around the names
  trimmed and a leading byte order mark ignored. Every row needs as many
  fields as the first. Values are text.
- JSON: an array of objects. Values that are not strings reach variables
  as JSON text.

## Getman's state

`.getman/` holds what Getman keeps for you on this machine. Nothing in the
workspace depends on it: deleting it loses only what is listed here.

| Path | Written by | Holds |
| --- | --- | --- |
| `.getman/.gitignore` | Getman, the first time it writes here | `*`, so git ignores everything here |
| `.getman/history.json` | The UI | The last 200 requests sent from it: when, the environment, the method, URL, status, time and size, and the request as it was in the editor |
| `.getman/history/<id>.json` | The UI | The response of each, with its body cut to 1 MiB |
| `.getman/runs/<id>.json` | The Runner, and `getman run --save` | The newest 20 runs: results, timings and checks, without bodies, headers or variable values |
| `.getman/cookies.json` | The UI | The cookie jar; see [Cookies](http.md#cookies) |
| `.getman/oauth2-tokens.json` | Signing in with a browser | OAuth 2.0 tokens, which `getman run` and `getman send` use too |
| `.getman/trash/<id>/` | Deleting in the UI or the terminal UI | `entry.json`, which says where the item was, its name and when it went, beside the deleted file or folder |
| `.getman/header-presets.yaml` | The UI | Named sets of headers |

The header presets are plain YAML you can edit or copy to another
workspace:

```yaml
# .getman/header-presets.yaml
- name: Internal
  headers:
    - name: X-Api-Version
      value: "2"
```

Deleting a request moves only its file to the trash. Its `.examples`
directory stays where it is, and is used again if the request is put back
under the same name.
