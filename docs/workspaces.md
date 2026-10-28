# Workspaces

A workspace is a directory with a `getman.yaml` file at its root. Folders
inside it are folders of requests, and requests are YAML files. There is
no database and no hidden state worth keeping: the files are the
workspace.

```text
api/
  getman.yaml               settings for every request
  getman.local.yaml         yours only: overrides and globals (git-ignored)
  .gitignore                written by getman init
  environments/
    staging.yaml            shared values (committed)
    staging.local.yaml      yours only: secrets, tokens (git-ignored)
  orders/
    _folder.yaml            settings for the folder (optional)
    create-order.yaml       a request
    get-order.yaml
  health.yaml
```

Getman finds the workspace by looking for `getman.yaml` in the current
directory and its parents, like git looks for `.git`. Commands take
`-w DIR` to point elsewhere.

## Requests

Every field except `url` is optional, and fields left at their defaults
are not written, so files stay short.

```yaml
name: Create order            # shown in the UI; defaults to the file name
method: POST                  # any method; GET when missing
url: "{{baseUrl}}/orders/:id?notify=true"
description: |                # Markdown, shown in the Docs tab
  Places an order for the signed-in customer.
path_params:                  # values for :name segments of the path
  id: "{{orderId}}"
disabled_params:              # query parameters kept but switched off
  - name: debug
    value: "1"
headers:
  - name: Accept
    value: application/json
  - name: X-Trace
    value: "1"
    enabled: false            # kept in the file, not sent
body:
  type: json
  content: |
    { "sku": "BOOK-42" }
auth:
  type: bearer
  token: "{{token}}"
variables:                    # variables for this request only
  sku: BOOK-42
scripts:
  pre_request: |
    gm.variables.set("requestId", crypto.randomUUID());
  post_response: |
    gm.test("created", () => gm.response.to.have.status(201));
checks:
  - status == 201
  - json.id exists
captures:
  orderId: json.id
settings:
  timeout_ms: 5000
  follow_redirects: false
  verify_tls: true
examples:                     # saved responses; see Examples and mocks
  - name: Created
    status: 201
    body: '{ "id": 42 }'
```

Examples are described in [Examples and mocks](examples-and-mocks.md).

### Bodies

| `type` | Fields | Sent as |
| --- | --- | --- |
| `json` | `content` | `application/json` |
| `xml` | `content` | `application/xml` |
| `text` | `content`, `content_type` | `text/plain` unless `content_type` says otherwise |
| `form` | `fields` (like headers) | `application/x-www-form-urlencoded` |
| `multipart` | `fields`, each with `value` or `file`, and an optional `content_type` | `multipart/form-data` |
| `binary` | `file`, `content_type` | the file's bytes, typed from its name unless set |
| `graphql` | `query`, `variables` (JSON text) | `application/json` |

Paths in `file` are relative to the workspace root, and must stay inside
it.

A request's own `Content-Type` header always wins over the one its body
type implies.

### Authentication

| `type` | Fields |
| --- | --- |
| `inherit` | The default: use the folder's, or the workspace's |
| `none` | Send no credentials, even if a folder sets some |
| `basic` | `username`, `password` |
| `bearer` | `token`, and `prefix` (default `Bearer`) |
| `api_key` | `key`, `value`, and `in`: `header` (default) or `query` |
| `digest` | `username`, `password`; MD5, SHA-256 and their `-sess` forms |
| `oauth2` | `grant` (`client_credentials` or `password`), `token_url`, `client_id`, `client_secret`, `scope`, `audience`, `username`, `password`, `client_auth` (`basic` or `body`) |

OAuth 2.0 tokens are fetched when a request needs one and reused until 30
seconds before they expire.

### Settings

| Field | Default | |
| --- | --- | --- |
| `timeout_ms` | 30000 | 0 waits forever |
| `follow_redirects` | true | |
| `max_redirects` | 10 | |
| `verify_tls` | true | false accepts self-signed and expired certificates |
| `http_version` | `auto` | `http1`, `http2` or `http3` |
| `title_case_headers` | true | HTTP/1.1 header names as `Content-Type` |
| `proxy` | system | `off`, a proxy URL, or a map with credentials |
| `client_cert` | none | PEM files or a PKCS#12 file |
| `ca_certs` | none | Extra certificate authorities, as PEM files |
| `cookies` | true | false leaves the cookie jar alone |
| `max_response_mb` | 100 | 0 reads the whole body |

Unset settings come from the folder, then the workspace. [HTTP and
connections](http.md) explains each one.

## Folders and the workspace

`_folder.yaml` and `getman.yaml` share one shape. Everything in them
applies to every request inside:

```yaml
name: Orders
variables:
  pageSize: "20"
headers:
  - name: Accept
    value: application/json
auth:
  type: bearer
  token: "{{token}}"
scripts:
  pre_request: |
    gm.request.headers.upsert({ key: "X-Request-Id", value: crypto.randomUUID() });
settings:
  timeout_ms: 10000
order: [create-order, get-order, archive]
```

- **Headers** are added unless the request sets a header of the same
  name.
- **Authentication** is inherited by every request whose auth is
  `inherit`; the nearest folder that sets one wins.
- **Scripts** run around every request inside, outermost first: the
  workspace's, then each folder's, then the request's own.
- **Order** lists the folder's items by file or directory name. Items not
  listed follow, folders first, then alphabetically. Dragging items in the
  UI writes this list.

A folder needs no `_folder.yaml`; a directory with request files in it is a
folder. YAML files that are not requests, such as a `docker-compose.yaml`,
are ignored, and a request file that fails to parse is shown with its
error so a bad merge is easy to spot.

## Environments

An environment is a set of variables to switch between, such as `local`,
`staging` and `production`. Each is a file in `environments/`:

```yaml
# environments/staging.yaml
name: Staging
variables:
  baseUrl: https://staging.example.com
  token: ""
secrets: [token]
```

Beside it, a git-ignored `staging.local.yaml` holds values that are yours
alone:

```yaml
# environments/staging.local.yaml
variables:
  token: eyJhbGciOi…
```

Local values win over shared ones. Variables listed under `secrets` are
never written to the shared file: the UI saves their values to the local
file instead. Scripts that call `gm.environment.set` save there too, so a
login request's token survives a restart without ever being committed.

## What to commit

| Commit | Keep local |
| --- | --- |
| `getman.yaml`, `_folder.yaml`, request files, `*.examples/` (example bodies), `environments/*.yaml` | `*.local.yaml`, `.getman/` (history, past runs, cookies, trash, header presets) |

`getman init` writes a `.gitignore` with exactly those two lines.
