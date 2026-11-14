# Changelog

All notable changes to Getman are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and Getman uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Getman for Android and iOS. Each app runs Getman's engine and shows the
  same UI as the desktop app, with one workspace, made with the example
  requests, in its own storage. See [Mobile apps](docs/installation.md#mobile-apps).
  - Android: `Getman-android-universal.apk`, one signed APK for ARM64,
    ARMv7 and x86_64, for Android 8.0 or newer. HTTPS is checked with the
    phone's own certificate store.
  - iOS: `Getman-ios.ipa`, unsigned, for iOS 15 or newer. AltStore,
    SideStore or Sideloadly sign it with your Apple ID when they install it.
    The Files app shows the workspace under On My iPhone › Getman.
- Flatpak bundles for Linux x86_64 and aarch64, on the GNOME runtime:
  `flatpak install --user Getman-linux-x86_64.flatpak`.
- A Nix flake that builds the command line, and on Linux the desktop app,
  from source: `nix run github:fzn0x/getman`.
- A Homebrew cask for the desktop app, beside the formula for the command
  line: `brew install --cask fzn0x/tap/getman-desktop`.
- A native `getman.exe` for ARM64 Windows. The install script and Scoop
  take it on ARM64, and fall back to the x64 build for releases without
  it.
- A downloads section for Windows, macOS, Linux, Android and iOS on the
  [website](https://fzn0x.github.io/getman/) and in the README. A phone
  sees its own app first.
- Documentation of every feature, with new pages for
  [installation](docs/installation.md),
  [troubleshooting](docs/troubleshooting.md),
  [keyboard shortcuts](docs/keyboard-shortcuts.md),
  [settings](docs/settings.md), the [scripting API](docs/scripting-api.md),
  the [workspace format](docs/workspace-format.md),
  [development](docs/development.md) and the
  [architecture](docs/architecture.md). Each page of the documentation
  site sits on a card, with a Copy page button for its Markdown.
- The release workflow can push the AUR package `getman-bin`. It does so
  once the repository has an AUR key, which 0.4.0 does not yet have.

### Changed

- Getman is now licensed under MIT OR GPL-3.0-or-later, in place of MIT OR
  Apache-2.0. `LICENSE-GPL` holds the GPL text; the Apache-2.0 text moves to
  `LICENSES/Apache-2.0.txt`, for the bundled files under it. Releases up to
  0.3.4 keep MIT OR Apache-2.0.
- The Linux command line links against glibc 2.28, so it runs on RHEL 8
  and 9, Debian 11 and Ubuntu 20.04 and newer. 0.3.4 needed glibc 2.39.
- The browser UI fits phones and tablets. On a narrow screen the top bar
  folds Import, Cookies and the theme into a menu, the sidebar closes when
  a request opens, and keyboard hints hide on touch screens, whose controls
  grow to finger size. Desktop layouts are unchanged.
- Quieter edges: fields and controls are filled, dividers are softer, and
  the Runner, folder settings, live connections and the HTML run report
  follow Material Design 3.
- The theme button and command cycle system, light and dark, and say which
  comes next.
- A run that a script ends with `gm.execution.stopExecution()` exits 0 when
  nothing failed, and the Runner marks it passed. The JSON report says so
  in `summary.stopped_by_script`.
- `getman export` has only its own options. `--script-timeout` is gone,
  since export runs no scripts, and `GETMAN_ENVIRONMENT` applies to code
  exports only, so `getman export --to postman` without `-e` exports the
  workspace.
- Settings › About names Flatpak, AUR and Nix installs, and how each of
  them updates, in place of telling them to install a new .deb.
- The package descriptions for Homebrew, Scoop and winget, and the desktop
  app's own, describe Getman as an open-source Postman alternative in
  place of slogans.
- The desktop updater moves to tauri-plugin-updater 2.14.1 and zip 9,
  which clears four advisories against the older zip.

### Fixed

- `getman init` adds its entries to an existing `.gitignore` instead of
  leaving it as it is.
- Environment files ending in `.yml`, and their `.local.yml` files, are
  read and saved in place. Before, only `.yaml` worked.
- YAML numbers and booleans stay as written where Getman expects text:
  `version: 1.0` stays `1.0`, and `0x10` and `1e3` stay as they are.
- Deleting a request moves its `.examples` folder to the trash with it, and
  restoring it brings the folder back.
- Postman and OpenAPI exports leave out gRPC, WebSocket, event stream and
  Socket.IO requests, with a warning, instead of writing them as broken
  HTTP requests.
- A Hoppscotch import no longer copies saved OAuth 2.0 tokens into
  request files.
- WebSocket and Socket.IO connections use the request's `ca_certs` and
  `client_cert`, as HTTP requests do.
- Scripts: a gRPC method name set in a script keeps its case,
  `skipRequest()` in a post-response script does nothing, waits for
  `gm.sendRequest` and `bru.sleep` count toward the script's time limit,
  and an interval that is never cleared fails the script after 100,000
  callbacks.
- A gRPC call whose stream closes before its first message goes out says
  why, such as a TLS server refusing to go on without a client
  certificate, instead of only that the server closed the stream.
- On macOS, Install the getman command no longer replaces a `getman` link
  from another install, such as Homebrew's.
- On a Mac, Option 1 to 9 and Option [ and ] switch tabs again. Option
  with a digit or a bracket types a symbol there, so they now match the
  physical key too.
- The response's JSON filter keeps room for its placeholder when the
  toolbar wraps.

## [0.3.4] - 2026-11-14

### Added

- A logo: a G whose round bowl runs into a forward tip, like a request on
  its way out, in indigo. The
  [README](https://github.com/fzn0x/getman/blob/main/README.md) opens with
  it, the browser tab shows it, and the desktop app uses it for its icons.
- The empty workspace and the page a browser shows without the token
  picture the logo with what Getman speaks circling it. It is a Svelte port
  of Magic UI's Orbiting Circles (MIT), and it holds still when the system
  asks for reduced motion.

### Changed

- The browser UI and the desktop app are redesigned in Material Design 3.
  One color scheme comes from the logo's indigo, in light and dark; Google
  Sans Flex and Google Sans Code replace the bitmap face; Material Symbols
  replace the pixel icons; and selection and hover show as tonal fills
  instead of inverting to black. The request line, the response header,
  the open-request tabs, the status bar, menus, dialogs and notices follow
  the new design.
- The [documentation site](https://fzn0x.github.io/getman/) has the same
  design, with a landing page whose feature cards replay what Getman really
  shows, and it links translated guides to each other for search engines.
  The README and the site show screenshots of the new design.
- The browser UI built into `getman` and the desktop app is 49 KiB larger
  compressed, 407 KiB instead of 358 KiB: the two new fonts add 64 KiB and
  the icons and styles 7 KiB, and the bitmap face it no longer needs takes
  22 KiB off.

### Fixed

- Below about 1280 px wide, the response toolbar and, in the Body tab, the
  body types and the Beautify and Minify row could not shrink. They pushed
  their pane past its edge, so the last buttons and the right side of both
  editors were cut off. They now wrap.

## [0.3.3] - 2026-11-14

### Added

- Homebrew and Scoop packages, updated with each release:
  `brew install fzn0x/tap/getman` on macOS and Linux, or
  `scoop bucket add fzn0x https://github.com/fzn0x/scoop-bucket` and then
  `scoop install fzn0x/getman` on Windows.
- An Indonesian translation of the [README](README-ID.md) and of
  [Getting started](docs/getting-started-ID.md), by @Urdemonlord.
- [Other API clients](docs/compare.md) compares Getman with Yaak,
  Hoppscotch, Bruno and Beam, with a source for every claim.

### Changed

- `getman` and the desktop app are about 1 MB smaller: the browser UI is
  built in Brotli-compressed, and browsers that take Brotli get it that
  way. From source, `npm run build` in `web/` now writes Vite's output to
  `web/build` and the compressed files to `web/dist`.
- The winget manifests attached to each release use schema 1.28.0.

### Fixed

- A gRPC call to a server that requires a client certificate could still
  fail with only "channel closed", when the server closed the connection
  before the call started, without the hint to set a certificate.
- In the browser UI, a WebSocket, Server-Sent Events or Socket.IO
  connection that had just closed could still be found for a moment, so
  sending on it then answered "closed" instead of saying it was gone.

## [0.3.2] - 2026-11-14

### Fixed

- A script or capture that set a variable a stronger scope also defines
  changed nothing, with no sign of it. For example,
  `gm.collectionVariables.set("csrfToken", ...)` while the environment
  has `csrfToken` left later requests sending the environment's value,
  even an empty one. The request now gets a warning that names the scope
  that wins and how to set the value there.
- `getman run` and the browser's Runner left out request files they
  could not read, such as one with headers written as a map, so a run of
  only such files said "0 requests" and passed. Each one now fails with
  the reason it cannot be read.
- Reading a gRPC server's schema through reflection could fail with
  "the server closed the stream" when the server offers only the older
  v1alpha reflection, instead of falling back to it.

## [0.3.1] - 2026-10-30

### Added

- The response's Sent tab shows the request body in the viewer response
  bodies use, with Pretty, Raw, Hex, Find, Copy and Save, and lets you
  copy a header by clicking it.
- The Sent tab and the console hide the values of the environment's
  secret variables, also Base64 and percent-encoded, until you choose
  **Show secrets**.
- `send_request` in `getman mcp` returns the headers and body of the
  request as it went out, with secrets masked.
- The Sent tab and the console list the headers Getman added when
  sending: `Host`, `Accept`, `Accept-Encoding`, the cookie jar's cookies
  and `Content-Length`. `send_request` in `getman mcp` returns them as
  `added_headers`.
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
- Importing several files stopped at the first one that failed, after
  the ones before it were already imported, and reported only the
  error. Now the other files are imported, and the report lists the
  failed files with their reasons and the files passed over for being
  in no format Getman reads.
- A gRPC call to a server that requires a client certificate could fail
  with only "broken pipe" or "connection reset", as it can on macOS,
  without the hint to set a certificate.

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

[Unreleased]: https://github.com/fzn0x/getman/compare/v0.3.4...HEAD
[0.3.4]: https://github.com/fzn0x/getman/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/fzn0x/getman/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/fzn0x/getman/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/fzn0x/getman/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/fzn0x/getman/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/fzn0x/getman/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/fzn0x/getman/releases/tag/v0.1.0
