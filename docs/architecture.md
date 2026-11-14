# Architecture

An overview of Getman's source: the three Rust crates, the Svelte web
UI, how one request flows through the engine, and what a release
builds.

Everything sits in one repository. The engine is a library, and every
front end is a thin layer over it:

```text
 web/  Svelte UI in a browser tab ───┐
                                     │  JSON over HTTP on 127.0.0.1, /api
 crates/getman-desktop  Tauri window ┤  (the desktop app runs the same server
                                     │   in its own process)
                                     ▼
 crates/getman  CLI, local server, terminal UI, MCP server, mock server, reports
                                     │
                                     ▼
 crates/getman-core  workspace files, variables, auth, scripts, HTTP, gRPC,
                     WebSocket, SSE, Socket.IO, checks, runner, import, export
```

| Path | Holds |
| --- | --- |
| `crates/getman-core` | The engine. No UI and no command line |
| `crates/getman` | The `getman` binary, and a library the desktop app links |
| `crates/getman-desktop` | The desktop app: a Tauri 2 window with in-app updates |
| `web/` | The browser UI, built into the binary |
| `docs/`, `site/` | These pages, and the site built from them |
| `packaging/` | Templates for the Homebrew, Scoop, winget and Flatpak manifests |

The root `Cargo.toml` makes the crates one workspace. Its
`default-members` leave the desktop app out, so the rest builds without
a webview's system libraries. Its lints deny `unsafe` code in every
crate and turn on Clippy's pedantic lints. See
[Development](development.md) for building and testing.

## getman-core: the engine

`crates/getman-core/src/lib.rs` describes the crate as the engine
behind Getman: the workspace format, variables, sending requests,
scripts, checks and the collection runner.

| Module | Does |
| --- | --- |
| `workspace.rs`, `workspace/trash.rs` | A workspace on disk: `getman.yaml` at the root, folders with `_folder.yaml`, request files, `environments/`, the git-ignored `*.local.yaml` files and the `.getman/` state directory. Deleted items go to `.getman/trash/` |
| `model.rs` | The documents: requests, folders, environments and their settings. Each one round-trips through YAML on disk and JSON over the local API, and fields at their default are not written |
| `lenient.rs` | Reads any YAML scalar as text where text is expected, so `port: 3000` works |
| `vars.rs` | Variable scopes and `{{name}}` replacement |
| `dynamic.rs`, `dynamic/data.rs` | `{{$uuid}}`, `{{$timestamp}}`, `{{$randomEmail}}` and the other dynamic values |
| `prepare.rs` | Turns a stored request into one ready to send: inherited settings applied, variables replaced, body encoded, simple auth and signatures added |
| `multipart.rs` | Writes `multipart/form-data` bodies as bytes, so they can be signed |
| `auth.rs`, `auth/` | Digest and OAuth 2.0, which need a round trip, the browser sign-in listener, and the signatures: AWS Signature Version 4, OAuth 1.0a, Hawk, Akamai EdgeGrid, JWT and Atlassian ASAP |
| `http.rs`, `http3.rs` | Putting requests on the wire: HTTP/1.1 and HTTP/2 through reqwest, HTTP/3 over QUIC with quinn and h3, cookies and redirects |
| `tls.rs`, `tunnel.rs`, `timing.rs` | Client certificates and extra certificate authorities, proxies for the connections Getman opens itself, and where a request's time goes |
| `execute.rs` | One request from start to finish. See [How one request flows](#how-one-request-flows) |
| `script/` | The JavaScript engine: `mod.rs` runs it, `prelude.js` defines the `gm` API |
| `checks.rs` | Checks such as `status == 200` and captures that set variables from a response |
| `runner.rs`, `data.rs` | Running a folder or the workspace in order, for one or more iterations, with rows from a CSV or JSON data file |
| `grpc/` | gRPC calls, schemas and messages |
| `realtime/` | WebSocket, Server-Sent Events and Socket.IO |
| `import/`, `export/` | Bringing requests in from other formats, and writing them out as code, Postman collections or OpenAPI |
| `examples.rs`, `mock.rs` | Saved responses, and matching incoming requests to them for the mock server |
| `error.rs`, `time.rs` | Errors, and formatting times without a date library |

The file formats themselves are described in
[Workspace file format](workspace-format.md).

## getman: the program

`crates/getman` is one program with three front ends over a workspace:
the browser UI, the terminal UI and the command line. `main.rs` only
calls the library, which the desktop app links too.

| Path | Does |
| --- | --- |
| `cli.rs` | The command-line arguments, with clap. Without a command, Getman opens the browser UI |
| `commands/` | One file per command: `run`, `send` (with `send_grpc.rs` and `stream.rs` for gRPC and live connections), `init`, `import`, `export` and `mock` |
| `workspaces.rs` | Finding the workspace a command works on, or the personal workspace |
| `server/` | The local server behind the browser UI. See [The web UI](#the-web-ui) |
| `tui/` | The terminal UI, on ratatui and crossterm |
| `mcp/` | `getman mcp`: a Model Context Protocol server over standard input and output, with JSON-RPC 2.0 messages one per line. `tools.rs` holds the tools |
| `mock.rs` | The mock server, on 127.0.0.1 and port 7979 by default. It answers with saved examples and reads them again at most once a second |
| `report/` | Run reports: in the terminal, and as JSON, JUnit XML, HTML, CSV or Excel files |
| `runs.rs` | Runs kept in `.getman/runs` for the UI's Runner, without bodies, headers or variable values |
| `mask.rs` | Hides secret values in what the MCP tools return, in the request as sent that the UI shows, and in run reports |
| `style.rs` | Terminal colors, dropped when output is not a terminal or `NO_COLOR` is set |
| `examples.rs` | The example requests `getman init --example` writes |

The commands are documented in [Command line](cli.md), the terminal UI
in [Terminal UI](terminal.md), the MCP server in
[AI agents (MCP)](mcp.md) and the mock server in
[Examples and mocks](examples-and-mocks.md#mock-servers).

## getman-desktop: the app

`crates/getman-desktop` shows the browser UI in a native window, with
Getman's server running in the same process. It is built on Tauri 2
and needs Rust 1.90.

| Path | Does |
| --- | --- |
| `src/main.rs`, `src/launch.rs` | Decides whether a start opens the window or runs the command line. On macOS and Linux the app's executable is also the `getman` command. On Windows the app is only a window, and `getman.exe` is installed beside it |
| `src/app.rs` | Starts the server with `getman::server::Server` on 127.0.0.1 and opens the window on it. Links that leave the app open in the system's browser |
| `src/commands.rs` | What the UI may ask of the app, such as checking for updates or opening another workspace |
| `src/updates.rs`, `src/install.rs` | In-app updates, and whether this copy can replace itself |
| `src/settings.rs` | The app's `settings.json`: the last workspace, the update choices and the port |
| `src/command_line.rs` | "Install the getman command" on macOS |
| `tauri.conf.json` | The bundle settings and the updater's endpoint and public key. `tauri.linux.conf.json` and `tauri.macos.conf.json` name the program `getman` on those systems |
| `windows/` | The Windows installers' extras: `command-line.json`, the NSIS hooks and the WiX fragment that put `getman.exe` on the `PATH` |
| `build.rs` | Lists the app's commands, for Tauri's permissions |

The app asks for its last port, so the page keeps its storage between
launches. Only the page served by its own server may call the app's
commands: `app.rs` adds a capability for `http://127.0.0.1:<port>` and
the main window only. The app also serves its update status under
`/api/app/`, behind the same guard and token as the rest of the API.

### The updater

Updates use `tauri-plugin-updater`. While automatic checks are on, the
app reads `latest.json` from the newest GitHub release once a day: the
only request it makes on its own. Every download is checked against the
minisign public key in `tauri.conf.json` before it is installed.
Development builds, the `.deb` and `.rpm` packages and the portable zip
cannot replace themselves, so they never look. See
[Desktop app](desktop.md#updates).

## The web UI

`web/` is a Svelte 5 app in TypeScript, built with Vite. The code
editor is CodeMirror 6, with the GraphQL language service for GraphQL
bodies. It follows
[DESIGN.md](https://github.com/fzn0x/getman/blob/main/DESIGN.md), a
Material Design 3 system.

| Path | Holds |
| --- | --- |
| `src/main.ts`, `src/App.svelte` | Startup: takes the token from the address, then mounts the app shell |
| `src/lib/api.ts` | The client for the local API. Every call goes through it |
| `src/lib/store.svelte.ts` | Everything the UI holds: the workspace, open tabs, preferences and dialogs |
| `src/lib/realtime.svelte.ts`, `src/lib/grpc.svelte.ts` | Live connections and gRPC calls |
| `src/lib/desktop.svelte.ts` | The bridge to the desktop app. In a browser tab it does nothing |
| `src/lib/editor.ts` | CodeMirror set up for Getman, with `{{variables}}` marked and completed |
| `src/lib/*.ts` | The rest of the logic, with Vitest tests beside it in `*.test.ts` |
| `src/components/` | The app shell: `Sidebar`, `Tree`, `TabStrip`, `TopBar`, `RunnerView`, `FolderView`, `EnvironmentsView`, `ConsolePanel`, `HistoryList`, `MockPanel`, `TrashList`, `Welcome` and `SignedOut` |
| `src/components/request/`, `response/` | The request editor's panes and the response viewer |
| `src/components/grpc/`, `realtime/` | The views for gRPC requests and live connections |
| `src/components/dialogs/` | Dialogs such as import, export, code, settings, cookies, shortcuts and the palette |
| `src/styles/tokens.css` | The Material Design 3 color roles, generated from the seed color. Do not edit by hand |
| `src/styles/app.css` | The app's own styles |

How the UI is used is described in [Browser UI](browser-ui.md).

### Talking to the server

The UI talks to Getman only through its JSON API under `/api`.

- `web/src/lib/api.ts` sends each call with `fetch` to `/api<path>`,
  with the token in the `x-getman-token` header and a JSON body. Most
  calls are `POST`. An error comes back as `{"error": "..."}`, and a
  `401` with the code `token` shows the signed-out page.
- `crates/getman/src/server/api.rs` holds the routes for the
  workspace, requests, folders, environments, sending, runs, import
  and export, cookies, history, OAuth 2.0 sign-in, examples and mocks.
- `server/realtime.rs`, `server/grpc.rs` and `server/graphql.rs` add
  the routes for live connections, gRPC calls and GraphQL schemas.
  Live connections share one stream of newline-delimited JSON per page.
  A gRPC call streams its events as JSON lines.
- `server/mod.rs` nests these routes under `/api` and serves every
  other path from `server/assets.rs`, the embedded UI.
- `server/history.rs` keeps the requests sent from the UI in
  `.getman/history.json`, with each response in
  `.getman/history/<id>.json`. `server/presets.rs` keeps header presets
  in `.getman/header-presets.yaml`.
- `server/background.rs` runs the same server inside another program,
  as the desktop app does. It can switch workspaces without changing
  its address.

### How the server protects itself

Any web page in the same browser could try to reach a server on
127.0.0.1, so the server checks every request. The checks are in the
`guard` middleware in `crates/getman/src/server/mod.rs`:

- It listens on 127.0.0.1 only. The port is 7878, or one of the next
  19 if that is taken, or any free port.
- The `Host` header must be `127.0.0.1`, `localhost` or `[::1]` with
  the server's port. This stops DNS rebinding.
- An `Origin` header, when present, must be the server's own origin.
  Requests from other web pages are refused, even with the token.
- Every path under `/api/` needs the token in `x-getman-token`.
- Every response carries `X-Content-Type-Options: nosniff`,
  `Referrer-Policy: no-referrer`, `X-Frame-Options: DENY` and
  `Cross-Origin-Resource-Policy: same-origin`. The UI's page also gets
  a Content Security Policy from `server/assets.rs`.

`crates/getman/src/server/token.rs` makes the token: 32 random bytes as
64 hex characters. It is saved as `getman/ui-token` in your
configuration directory, readable by you only on Unix, so a bookmarked
UI keeps working after a restart. Tokens are compared in constant
time. The UI reads the token from `#token=...` in the address once,
keeps it in `localStorage` and removes it from the address.

The engine adds its own limits. `Workspace::resolve` in
`crates/getman-core/src/workspace.rs` refuses any path that could leave
the workspace or name a hidden file. `{{$env.NAME}}` reads only
environment variables whose names start with `GETMAN_`. Scripts run in
a sandbox with no file access beyond the workspace's `.js` modules.
[Security and privacy](security.md) covers all of it from the user's
side.

## How one request flows

`Executor::execute` in `crates/getman-core/src/execute.rs` runs one
request for every front end: the UI's Send, `getman send`, `getman run`,
the terminal UI and the MCP tools.

1. **Load.** `Workspace::read_request` reads the request file. The
   executor reads its folder chain, from the workspace's `getman.yaml`
   through each `_folder.yaml` down to the request, the git-ignored
   local values and the selected environment.
2. **Inherit.** `prepare.rs` merges the chain's headers, auth, scripts
   and settings into the request's effective values.
3. **Choose the engine.** A gRPC request goes to `grpc::execute`. A
   WebSocket, SSE or Socket.IO request goes to `realtime::execute`.
   An HTTP request continues here.
4. **Pre-request scripts.** The workspace's, each folder's and then
   the request's scripts run in one fresh QuickJS engine. They can edit
   the method, URL, headers and body, set variables, or skip the
   request. A script that throws stops the request.
5. **Resolve variables.** `vars.rs` stacks the scopes, weakest first:
   globals, the workspace, the environment, folders, the request, the
   data row, session variables and local variables. `{{name}}` takes
   the strongest, `{{$name}}` makes a dynamic value, and names that
   stay undefined are listed in the result.
6. **Prepare.** `prepare.rs` replaces the variables, encodes the body
   and adds Basic, Bearer or API key auth. Signatures such as AWS
   Signature Version 4, OAuth 1.0a and Hawk are added last, over the
   final bytes.
7. **Send.** `auth::Authenticator::send` completes Digest and OAuth 2.0,
   fetching, caching and refreshing tokens. `http.rs` sends with
   reqwest. With `auto`, the default, that is HTTP/2 when the server
   offers it over TLS and HTTP/1.1 otherwise. `http1` and `http2` force
   one version, and `http2` over plain `http://` does not ask first.
   `http3` goes through `http3.rs` over QUIC instead. The client keeps
   cookies, follows redirects and records timings, and a `Cancel`
   switch can stop the send.
8. **Post-response scripts, checks and captures.** The scripts run
   with the response. Then `checks.rs` evaluates the request's checks
   and captures. A capture sets an environment variable when an
   environment is selected, and a workspace variable otherwise.
9. **Result.** Everything comes back as one `Execution`: the request as
   sent, the response, test and check results, console output, variable
   changes and warnings.

What happens next depends on the caller. The UI server saves variable
changes to the git-ignored local files with `persist_changes`, adds
the request to the history and masks secrets before it answers.
`getman run` collects each `Execution` through `runner.rs` and writes
the reports in `crates/getman/src/report/`.

The HTTP details are in [HTTP and connections](http.md#http-versions),
the scopes in [Variables and environments](variables.md), auth in
[Authentication](auth.md) and checks in [Checks and captures](checks.md).

## The other engines

### gRPC

`crates/getman-core/src/grpc/` runs unary and streaming calls over
HTTP/2. Schemas come from `.proto` files in the workspace, compiled
with protox, or from server reflection, `v1` or `v1alpha`
(`reflection.rs`). Messages are read and shown as JSON with
prost-reflect (`json.rs`). `transport.rs` holds its own HTTP/2
connection on hyper, plaintext or TLS. Headers and auth become
metadata (`metadata.rs`), and `status.rs` reads status codes and the
rich error details servers attach. `execute.rs` wraps a call in the
same steps as an HTTP request, so scripts, checks and captures work.
In the UI, `server/grpc.rs` streams a live call's events. See
[gRPC](grpc.md).

### WebSocket, SSE and Socket.IO

`crates/getman-core/src/realtime/` handles requests that stay open.
`connect` prepares the request like an HTTP one, then a task holds the
connection and reports events on a session.

| File | Does |
| --- | --- |
| `websocket.rs` | WebSocket, with tokio-tungstenite |
| `sse.rs` | Server-Sent Events, read event by event through the HTTP client |
| `socketio.rs`, `engine.rs` | Socket.IO v4, over Engine.IO v4 on WebSocket or HTTP long polling |
| `dial.rs`, `tls.rs` | Reaching the server directly or through a proxy, with TLS checked the way the HTTP client checks it |
| `run.rs` | A run: pre-request scripts, connect, send the saved messages, collect until a limit, then post-response scripts and checks on the handshake and the messages |

The UI keeps connections open as long as the user likes, so
`server/realtime.rs` uses the pieces of `run.rs` around its own
session. See [WebSocket, SSE and Socket.IO](realtime.md).

### Scripts

`crates/getman-core/src/script/` runs JavaScript in QuickJS, through
rquickjs. Each request gets a fresh engine. A JSON snapshot of the
request, the response and the variables goes in, and what the scripts
did comes back the same way: test results, logs, variable changes and
edits to the request. `prelude.js` defines `gm`, its alias `pm`, the
`bru`, `req` and `res` globals of Bruno scripts, `console`, `require`
for the workspace's `.js` files, and timers.

A script may use 256 MiB of memory and a 1 MiB stack. An interrupt
handler stops it after its timeout, 10 seconds unless
`--script-timeout` says otherwise. `gm.sendRequest` goes through the
same HTTP client as the request. See [Scripts](scripts.md) and the
[Scripting API](scripting-api.md).

### Import and export

`crates/getman-core/src/import/` reads Postman collections and
environments, Insomnia v4 and v5 exports, Hoppscotch collections,
Bruno folders, OpenAPI 3.0 and 3.1 and Swagger 2.0 documents, HAR
files and cURL commands. `mod.rs` tells the formats apart by their
content. Every format but Postman's is read into one in-memory import
first, with a warning for anything that could not be carried over,
then saved into the workspace.

`crates/getman-core/src/export/` writes the other way. `codegen.rs`
writes a request as code in 18 languages and tools, quoting each value
by the rules of the language it lands in. `postman.rs` writes Postman
collections in format v2.1 and environment files. `openapi.rs` infers
an OpenAPI 3.1 document from the requests. See
[Import and export](import-export.md).

## The release pipeline

Pushing a `v*` tag runs `.github/workflows/release.yml`:

| Job | Builds or publishes |
| --- | --- |
| `web` | The web UI, once, for every other job |
| `build` | `getman` for Linux x86_64 and ARM64, macOS on Apple silicon and Intel, and Windows x64, each packed with the README, licences and notices as a `.tar.gz` or `.zip` |
| `desktop` | The desktop app: NSIS and MSI installers and a portable zip for Windows x64 and ARM64, one universal `.dmg` for macOS, and an AppImage, `.deb` and `.rpm` for Linux x86_64 and ARM64 |
| `release` | The release notes from the version's section of `CHANGELOG.md`, `latest.json` for the updater, the package manifests from `packaging/render.mjs` and `SHA256SUMS`, then the GitHub release itself |
| `publish-packages` | Commits the Homebrew formula to `fzn0x/homebrew-tap` and the Scoop manifest to `fzn0x/scoop-bucket` |

The desktop update files are signed, and `latest.json` written, only
when the `TAURI_SIGNING_PRIVATE_KEY` secret is set. The Linux desktop
bundles are built on Ubuntu 22.04, so they run on any distribution
with that glibc or newer. The secrets and the package managers are
described under Releases in
[CONTRIBUTING.md](https://github.com/fzn0x/getman/blob/main/CONTRIBUTING.md#releases).
