# Changelog

All notable changes to Getman are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and Getman uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The response's Sent tab shows the request body in the viewer response
  bodies use, with Pretty, Raw, Hex, Find, Copy and Save, and lets you
  copy a header by clicking it.
- The Sent tab and the console hide the values of the environment's
  secret variables, also Base64 and percent-encoded, until you choose
  **Show secrets**.
- `send_request` in `getman mcp` returns the headers and body of the
  request as it went out, with secrets masked.
- `getman run`, its reports, kept runs and the browser's Runner hide the
  values of the environment's secret variables, also in URLs, test names
  and console output.

### Fixed

- The browser UI could not send or save a request whose body was over
  2 MB, and said Getman was not answering.
- `getman mcp` left a secret unmasked in its result when a script
  replaced it during the call, such as an old token that was sent.
- A response with a very long line, such as minified code, pushed the
  body's buttons out of view.
- A long file name or item name in the import, export and other dialogs
  widened their contents past the edge instead of wrapping.
- Opening a request twice before it had loaded, as a double-click on a
  large one can, stopped the tabs from updating until a reload.
- Shortcut hints on the new request and close tab buttons, and the note
  in the shortcuts list, named Alt and Ctrl on a Mac.

## [0.3.0] - 2026-10-28

### Added

- GraphQL: requests send an operation name, the editor completes and
  checks a query against the schema, and Getman reads the schema from the
  server by introspection.
- The browser UI has a console that logs what the UI sends, a search over
  open and recently closed tabs, and undo and redo for renames and moves
  in the sidebar.
- `getman export` takes `--indent` and `--no-boilerplate`, and the code
  dialog has the same two choices.
- gRPC requests connect through proxies and with the request's TLS
  settings, and can send signed authentication (AWS Signature Version 4,
  JWT and the others) as metadata.
- The terminal UI connects to WebSocket, SSE and Socket.IO requests,
  calls gRPC methods and signs in with OAuth 2.0.
- WebSocket, SSE and Socket.IO requests run pre-request and
  post-response scripts and checks, which read the messages received
  (`gm.messages`, the `messages` check target), and `getman run` and
  `getman send` run them instead of skipping them. A request's `until`
  says when to stop listening.
- Socket.IO speaks HTTP long polling and upgrades to WebSocket as the
  official client does (`socketio.transport`), and WebSocket and
  Socket.IO handshakes go through proxies and use the cookie jar.
- Redirects keep the method and the `Authorization` header when asked
  (`follow_original_method`, `follow_authorization`), and every hop is
  shown with its status and time.
- Multipart bodies can be signed with AWS Signature Version 4, OAuth 1.0,
  Hawk and Akamai EdgeGrid.
- Scripts: the rest of Postman's dynamic variables, editable form bodies in
  `pm.request.body`, `pm.request.id`, `pm.execution.stopExecution()`,
  Bruno's `bru`, `req` and `res`, and `require` of `.js` files in the
  workspace.
- Session variables (`gm.session`), kept while the UI server or a run
  lasts and never written to a file.
- Checks: `in`, `not in`, `between`, `is empty`, `is not empty`,
  `is truthy` and `is falsy`.
- Scripts can be turned off for a workspace on your machine, or for one
  command with `--no-scripts`.
- Each release carries manifests for package managers, written from the
  release's own files: a Homebrew formula, a Scoop manifest, winget
  manifests and a Flatpak manifest for the desktop app. They are release
  files only; Getman is not yet published to any of these.
- Every exported code snippet is now run for real in CI, in all 18
  languages, against a local server, and `getman mcp` is checked with the
  official MCP Inspector command line.
- A code of conduct, this changelog, a dependency policy checked by
  `cargo deny` and a licence check for the npm packages.

### Changed

- HTTP/3 runs on quinn and h3 with the same crypto library as the rest of
  Getman, so the binary is smaller and builds need no extra flag.
- `getman mcp` masks secrets of any length, also percent-encoded, in hex
  and in Base64, including inside a longer Base64 text such as a Basic
  `Authorization` header.

- The setup steps in [AI agents (MCP)](docs/mcp.md) follow the current
  Claude Code and Claude Desktop documentation, with the location of
  Claude Desktop's configuration file.

### Fixed

- A browser sign-in exchanged its code without the folder's proxy and
  certificate settings.
- Switching workspace in the desktop app left live connections and gRPC
  calls of the old workspace running.

- Python snippets for a multipart form with no file sent it URL-encoded;
  they now send every part as multipart, in order, with each part's type.
- Go snippets for a form with no file did not compile, and sent every file
  as `application/octet-stream`.
- Rust, HTTPie and PowerShell snippets dropped a file part's content type
  or reordered the parts of a form.
- The raw HTTP snippet cut the last line breaks off a body, so its
  `Content-Length` was too long.
- Ruby snippets lost the carriage return of a CRLF line break in a body.

## [0.2.0] - 2026-10-28

### Added

- A desktop app for Windows, macOS and Linux that runs the same server in
  a native window, updates itself, and installs the `getman` command.
- gRPC requests over HTTP/2: unary and streaming calls, schemas from
  `.proto` files or server reflection, a generated request for each
  method, checks on statuses and messages, and `grpcurl` export.
- WebSocket, SSE and Socket.IO requests, with live message logs in the
  browser UI and in `getman send`.
- OAuth 2.0 authorization code and implicit grants with a browser sign-in,
  and signing with AWS Signature Version 4, JWT bearer and Atlassian ASAP
  tokens, OAuth 1.0a, Hawk and Akamai EdgeGrid. Digest answers SHA-512-256
  challenges. A new [Authentication](docs/auth.md) page describes them all.
- HTTP version (including HTTP/3), proxy, TLS, client certificate, cookie
  and size settings per request, folder or workspace; timings and TLS
  details in every response; and an editable, persistent cookie jar.
- Imports from OpenAPI 3 and Swagger 2.0, Insomnia v4 and v5, Hoppscotch,
  Bruno, HAR files and cURL, detected from the file, from whole folders and
  from dropped files. Exports of folders as Postman collections, of
  environments and globals, and of folders as OpenAPI 3.1.
- Code snippets in ten more languages, each quoting every value for the
  language it lands in, and an option to keep `{{variables}}` out of them.
- `getman mock` serves saved responses ("examples") as a mock server, and
  `getman mcp` lets an AI agent list, read, send and write requests over
  the Model Context Protocol, with secrets scrubbed from every result.
- `getman run` takes several folders and requests in order, and writes
  HTML, CSV and XLSX reports. The browser UI picks and orders what runs,
  keeps the newest 20 runs and saves responses as examples.
- In the browser UI: tabs that survive reloads and can be dragged,
  history by day with each response, a trash with undo, header presets,
  duplicate environments, a hex view, links and find in response bodies,
  variable hover and completion in tables, and cancelling a send with Esc.
- A website and documentation for search engines and link previews.

### Changed

- `getman` is now a library and a thin binary, and the desktop app embeds
  the same server.
- The landing page and the README describe Getman as it is.

### Fixed

- A deleted request, folder or environment goes to a trash that can restore
  it, instead of being removed for good.
- `gm.variables.clear()` exists, and a variable saved from the UI goes to
  the scope chosen.
- Files Getman writes under `.getman` stay out of git, and OAuth 2.0 tokens
  are kept by the executor that saves cookies.
- Sidebar, tab and table layout bugs in the browser UI.

### Security

- `crypto.randomUUID` in scripts is unpredictable, and the Digest client
  nonce comes from the operating system's random generator.
- Only `http` and `https` authorization URLs are opened for an OAuth 2.0
  sign-in.
- Proxy credentials, client keys and the cookie jar are kept out of files
  meant to be committed, as [Security and privacy](docs/security.md)
  describes.

## [0.1.0] - 2026-10-14

The first release: requests as YAML files in your repository, opened in a
browser UI on `127.0.0.1`, in a terminal UI or run in CI with
`getman run`; variables, environments and secrets kept in git-ignored
files; checks and captures; JavaScript scripts with the `gm` API (and a
`pm` alias); import from Postman and cURL, and export as code.

[Unreleased]: https://github.com/fzn0x/getman/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/fzn0x/getman/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/fzn0x/getman/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/fzn0x/getman/releases/tag/v0.1.0
