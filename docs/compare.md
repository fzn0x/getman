# Other API clients

Getman is one of several open-source API clients. This page sets it beside
four others, [Yaak][yaak], [Hoppscotch][hoppscotch], [Bruno][bruno] and
[Beam][beam], and says plainly where each of them is the better choice.
[Why Getman](why-getman.md) does the same for Postman.

Facts about the other clients were checked against their own repositories,
documentation, pricing pages and release files on 14 November 2026. Sizes
are from each one's latest stable release: Getman 0.3.2, Yaak 2026.9.1,
Hoppscotch 2026.10.1, Bruno 4.3.0 and Beam 0.11.0. All of this changes
often, so follow the links before deciding.

## The short version

| | Getman | Yaak | Hoppscotch | Bruno | Beam |
| --- | --- | --- | --- | --- | --- |
| Licence | MIT or Apache-2.0 | [MIT][yaak-readme]; work use of the official builds needs a [paid licence][yaak-pricing] | [MIT][hopp-license], with a paid [Enterprise Edition][hopp-ee] | [MIT][bruno-license], with [paid editions][bruno-editions] | [GPL-3.0][beam-license] |
| Price | Free for any use | Free for personal use; $79 a year for one person at work, $149 per user a year for a business ([pricing][yaak-pricing]) | Free; paid plans from $6 per user a month, billed annually ([pricing][hopp-pricing]) | Free; $6, $11 or $18 per user a month, billed annually ([pricing][bruno-pricing]) | Free |
| Account | None | [None][yaak-privacy] | Optional; needed to sync, work in teams and use [MCP][hopp-mcp] ([README][hopp-readme]) | [None][bruno-intro] | [None][beam-readme] |
| Where requests live | A YAML file per request, in your repository ([workspaces](workspaces.md)) | A local database ([privacy][yaak-privacy]), with optional [sync to YAML files][yaak-sync] | Browser storage or the desktop app's store, and a server when signed in ([security notes][hopp-security]) | [`.bru`][bruno-bru] or [YAML][bruno-yaml] files | A [TOML file][beam-storage] per request |
| Ways in | Desktop app, browser UI, terminal UI, command line | Desktop app, [command line][yaak-cli] | [Web app][hopp-web], [desktop app][hopp-desktop], [command line][hopp-cli] | Desktop app, [command line][bruno-cli], [VS Code extension][bruno-vscode] | Desktop app ([README][beam-readme]) |
| Other request types | [gRPC](grpc.md), [WebSocket, Server-Sent Events, Socket.IO](realtime.md), [GraphQL](http.md#graphql) | [gRPC][yaak-grpc], [WebSocket][yaak-ws], [Server-Sent Events][yaak-sse], [GraphQL][yaak-graphql] | [WebSocket, Server-Sent Events, Socket.IO, MQTT][hopp-realtime], [GraphQL][hopp-readme] | [gRPC, WebSocket, Server-Sent Events, GraphQL, SOAP][bruno-protocols] | GraphQL as a body type ([README][beam-readme]) |
| HTTP/3 | [Yes](http.md) | [Not enabled][yaak-http] in its HTTP engine | Listed by the desktop app's [request relay][hopp-relay], not in the user docs | Not documented ([request types][bruno-protocols]) | Not documented ([README][beam-readme]) |
| Scripts | JavaScript; `pm` is an alias of `gm`, so most Postman scripts run unchanged ([scripts](scripts.md)) | [None][yaak-scripts]; template functions and plugins instead | JavaScript, with a [`pm` compatibility layer][hopp-scripts] | JavaScript with its own `bru` API; Postman scripts are [converted on import][bruno-postman] | JavaScript [after the response only][beam-scripts], with a few `pm` calls |
| Command-line runs | `getman run`, with JUnit, HTML, JSON, CSV and Excel reports ([runner](runner.md)) | [`yaak send`][yaak-runs], with no report files documented | [`hopp test`][hopp-cli], with JUnit reports | [`bru run`][bruno-reports], with JSON, JUnit and HTML reports | None documented ([README][beam-readme]) |
| Mock servers | Local, from saved examples ([mocks](examples-and-mocks.md)) | None documented ([docs][yaak-docs]) | [Hosted][hopp-mock]; listed under the Business plan ([pricing][hopp-pricing]) | [Local, in beta][bruno-mock] | None documented ([README][beam-readme]) |
| MCP server for AI agents | Built in ([AI agents](mcp.md)) | A [plugin][yaak-mcp] | [Needs an account][hopp-mcp]; listed under the Business plan ([pricing][hopp-pricing]) | A [separate project][bruno-mcp] at version 0.1.0 | None documented ([README][beam-readme]) |
| Telemetry | [None](security.md) | [None][yaak-privacy] | [On by default][hopp-telemetry], with a [switch in Settings][hopp-settings] | An anonymous [daily start event][bruno-telemetry] | [None][beam-readme] |
| Windows x64 download | 15.3 MiB installer ([release][getman-release]) | 44.6 MiB installer ([release][yaak-release]) | 45.3 MiB installer ([release][hopp-release]) | 117.4 MiB installer ([release][bruno-release]) | 14.1 MiB zip, no installer ([release][beam-release]) |
| Built with | Rust, Svelte, Tauri | [Rust, React, Tauri][yaak-readme] | [Vue][hopp-vue], [Tauri][hopp-desktop] | [Electron, React][bruno-electron] | [Rust, GPUI][beam-cargo] |

## Yaak

[Yaak][yaak] is a desktop client built with Tauri, Rust and React. Like
Getman, it needs no account, has no cloud and sends no telemetry
([security and privacy][yaak-privacy]).

**Choose Yaak if you want:**

- **Git inside the app.** With [directory sync][yaak-sync], Yaak writes
  its requests to YAML files and can branch, commit, push and pull for
  you. Getman leaves Git to the tools you already use.
- **Encrypted secrets.** Yaak encrypts `secure(...)` values with a key
  kept in the system keychain ([security and privacy][yaak-privacy]), and
  reads secrets from [1Password][yaak-1password]. Getman keeps secrets as
  plain text in git-ignored `*.local.yaml` files ([variables](variables.md))
  and masks them on screen ([browser UI](browser-ui.md)).
- **Requests without code.** Yaak builds dynamic values with template
  functions and TypeScript plugins instead of scripts
  ([templating][yaak-scripts]).

**Where Getman differs:**

- **Scripts and tests.** "Yaak does not currently support request
  scripting" ([templating][yaak-scripts]). Getman runs JavaScript before
  each request and after its response, Postman's `pm` calls included, and
  checks such as `status == 200` without any code ([checks](checks.md)).
- **Work use.** Yaak's source is MIT and free to build yourself for any
  use ([README][yaak-readme]), but work use of its official builds needs a
  paid licence ([pricing][yaak-pricing]). Getman's releases are free for
  any use.
- **Reports for CI.** `yaak send` runs a folder or a workspace and prints
  a summary ([running requests][yaak-runs]). `getman run` can also run once
  per row of a CSV or JSON file, and writes JUnit, HTML, JSON, CSV and
  Excel reports ([Running in CI](runner.md)).
- **More ways in.** Getman also has a browser UI and a terminal UI.

## Hoppscotch

[Hoppscotch][hoppscotch] is an open-source API platform: a
[web app][hopp-web], a [desktop app][hopp-desktop] built on Tauri, a
[command line][hopp-cli], and a server you can [host yourself][hopp-selfhost].

**Choose Hoppscotch if you want:**

- **Nothing to install.** The [web app][hopp-web] runs in a browser.
  Getman's browser UI needs the `getman` program running on your machine.
- **MQTT**, which Getman does not speak
  ([real-time protocols][hopp-realtime]).
- **Live teamwork.** Hoppscotch's paid plans add team workspaces with
  real-time collaboration ([pricing][hopp-pricing]). Getman shares work
  only through Git.
- **AI in the app.** Experimental features name requests and write
  bodies, pre-request scripts and tests ([AI features][hopp-ai]).

**Where Getman differs:**

- **Where requests live.** Hoppscotch keeps them in the browser's storage
  or the desktop app's store, and on its server when you sign in
  ([security notes][hopp-security]). Getman's requests are files in your
  repository from the start, reviewed like code.
- **gRPC**, which is not among the protocols Hoppscotch documents
  ([real-time protocols][hopp-realtime]). Getman calls gRPC services from
  `.proto` files or server reflection ([gRPC](grpc.md)).
- **No telemetry.** Hoppscotch's telemetry setting is
  [on by default][hopp-telemetry]. Getman has none
  ([security and privacy](security.md)).
- **Mocks and agents without a plan.** Hoppscotch's pricing puts API
  mocking and MCP server support on the Business plan
  ([pricing][hopp-pricing]), and its MCP server needs an account
  ([MCP server][hopp-mcp]). `getman mock` and `getman mcp` run on your
  machine, free.
- **One program for CI.** The `hopp` command needs Node.js 22 or newer and
  writes JUnit reports ([CLI][hopp-cli]). `getman` is a single program
  with no runtime to install, and writes five report formats.

## Bruno

[Bruno][bruno] is the closest to Getman in spirit: an offline-only client
whose collections are plain files meant for Git, with no account and "no
plans to add cloud-sync" ([README][bruno-readme]). It is built on Electron
and React ([package][bruno-electron]).

**Choose Bruno if you want:**

- **SOAP**, with WSDL import ([request types][bruno-protocols]).
- **A VS Code extension** ([extension][bruno-vscode]).
- **Secret managers.** Bruno encrypts the secrets it keeps, and its
  Ultimate edition reads HashiCorp Vault, AWS Secrets Manager, Azure Key
  Vault and Google Secret Manager ([secrets][bruno-secrets]).
- **Git commands inside the app** ([Git integration][bruno-git]), several
  of them on the paid editions ([pricing][bruno-pricing]).
- **A larger community.** Bruno's repository has over 48,000 stars on
  GitHub.

**Where Getman differs:**

- **Size.** Bruno's Windows installer is 117.4 MiB
  ([release][bruno-release]). Getman's is 15.3 MiB
  ([release][getman-release]): its desktop app draws with the web view
  built into the system, WebView2 on Windows ([desktop app](desktop.md)).
- **Postman scripts as they are.** Bruno translates `pm` calls into its
  own API when importing ([Postman migration][bruno-postman]). In Getman
  `pm` is an alias of `gm`, so the scripts keep their Postman form.
- **Nothing behind a plan.** Bruno's pricing lists data-driven runs from
  Pro and report generation from Ultimate ([pricing][bruno-pricing]),
  though its command line documents JSON, JUnit and HTML reports without
  a paid badge ([reports][bruno-reports]). In Getman, runs over CSV or
  JSON data and every report format are free.
- **HTTP/3 and Socket.IO**, which Bruno does not document
  ([request types][bruno-protocols]).
- **No telemetry.** Bruno sends an anonymous start event once a day, with
  the operating system and the app's version ([source][bruno-telemetry]);
  its docs describe a setting to turn it off ([settings][bruno-settings]).
  Getman has none.
- **A browser UI and a terminal UI**, besides the desktop app and the
  command line.

## Beam

[Beam][beam] is a young native HTTP client written in Rust with GPUI, by a
single developer, under GPL-3.0. Like Getman, it is local-first, with no
account and no telemetry ([README][beam-readme]).

**Choose Beam if you want:**

- **The smallest download here:** a 14.1 MiB zip for Windows and an
  11.4 MiB archive for Apple silicon Macs ([release][beam-release]).
  Getman's are a 15.3 MiB installer and a 22.2 MiB disk image for both
  kinds of Mac ([release][getman-release]).
- **An interface drawn natively** with GPUI ([README][beam-readme]).
- **Copyleft.** GPL-3.0 keeps derived work open
  ([licence][beam-license]).

**Where Getman differs:**

- **More than HTTP.** Beam sends HTTP requests, with GraphQL as a body
  type ([README][beam-readme]). Getman adds gRPC, WebSocket, Server-Sent
  Events and Socket.IO.
- **Scripts on both sides.** Beam runs JavaScript after the response,
  with a few `pm` calls ([script API][beam-scripts]). Getman also runs
  scripts before the request, and its `pm` is the whole of Getman's
  script API ([scripts](scripts.md)).
- **Collection runs and CI.** Beam documents no runner or command line
  ([README][beam-readme]). Getman runs folders in the app, in the terminal
  and in CI.
- **Installers and updates.** Beam ships archives
  ([release][beam-release]) and is updated by hand ([README][beam-readme]).
  Getman ships installers and packages for Windows, macOS and Linux, and
  its desktop app updates itself ([desktop app](desktop.md)).

## Where Getman falls short

Getman is young and deliberately small. Next to these four:

- **A short record.** Getman's first release came out in October 2026
  ([releases][getman-releases]). Hoppscotch, Bruno and Yaak have years of
  releases and large communities.
- **No MQTT or SOAP.** Hoppscotch speaks MQTT, and Bruno speaks SOAP.
- **No browser-only use.** Hoppscotch's web app needs no install.
  Getman's browser UI needs `getman` running on your machine.
- **No live collaboration or hosted mocks.** Getman shares work through
  Git, and its mocks run on your machine or in CI.
- **No Git or AI assistant inside the app.** Yaak and Bruno run Git for
  you, and Hoppscotch and [Bruno][bruno-ai] write scripts and tests with
  AI. Getman leaves Git to your own tools, and lets AI agents work with
  your requests through [MCP](mcp.md).
- **Secrets are not encrypted at rest.** Yaak and Bruno encrypt the
  secrets they keep. Getman keeps them as plain text in git-ignored files.
- **Unsigned installers.** Windows and macOS warn the first time
  Getman's desktop app opens ([desktop app](desktop.md)).

## Moving over

`getman import` reads Hoppscotch collections and environments, and Bruno
collections of `.bru` files, besides Postman, Insomnia, OpenAPI, HAR and
cURL ([Import and export](import-export.md)). It does not read Bruno's YAML
collections, Yaak's or Beam's files yet.

```sh
getman init
getman import ./my-bruno-collection
getman import hoppscotch-collection.json
```

[yaak]: https://github.com/mountain-loop/yaak
[yaak-readme]: https://github.com/mountain-loop/yaak/blob/main/README.md
[yaak-pricing]: https://yaak.app/pricing
[yaak-privacy]: https://yaak.app/docs/getting-started/security-and-privacy
[yaak-sync]: https://yaak.app/docs/collaboration/local-directory-git-sync
[yaak-cli]: https://yaak.app/docs/getting-started/cli-usage
[yaak-grpc]: https://yaak.app/docs/request-types/grpc-requests
[yaak-ws]: https://yaak.app/docs/request-types/websockets
[yaak-sse]: https://yaak.app/docs/request-types/server-sent-events
[yaak-graphql]: https://yaak.app/docs/request-types/graphql-requests
[yaak-http]: https://github.com/mountain-loop/yaak/blob/main/crates/yaak-http/Cargo.toml
[yaak-scripts]: https://yaak.app/docs/templating/pre-post-request-scripting
[yaak-runs]: https://yaak.app/docs/advanced/running-multiple-requests
[yaak-docs]: https://yaak.app/docs
[yaak-mcp]: https://yaak.app/docs/getting-started/mcp-server
[yaak-1password]: https://yaak.app/docs/templating/1password-vault-integration
[yaak-release]: https://github.com/mountain-loop/yaak/releases/tag/v2026.9.1
[hoppscotch]: https://github.com/hoppscotch/hoppscotch
[hopp-readme]: https://github.com/hoppscotch/hoppscotch/blob/main/README.md
[hopp-license]: https://github.com/hoppscotch/hoppscotch/blob/main/LICENSE
[hopp-ee]: https://docs.hoppscotch.io/documentation/self-host/enterprise-edition/getting-started
[hopp-selfhost]: https://docs.hoppscotch.io/documentation/self-host/getting-started
[hopp-pricing]: https://hoppscotch.com/pricing
[hopp-security]: https://github.com/hoppscotch/hoppscotch/blob/main/SECURITY.md
[hopp-web]: https://docs.hoppscotch.io/documentation/clients/web/overview
[hopp-desktop]: https://docs.hoppscotch.io/documentation/clients/desktop/overview
[hopp-cli]: https://docs.hoppscotch.io/documentation/clients/cli/overview
[hopp-realtime]: https://docs.hoppscotch.io/documentation/protocols/realtime
[hopp-relay]: https://github.com/hoppscotch/hoppscotch/blob/main/packages/hoppscotch-desktop/plugin-workspace/relay/README.md
[hopp-scripts]: https://docs.hoppscotch.io/documentation/features/scripts
[hopp-mock]: https://docs.hoppscotch.io/documentation/features/mock
[hopp-mcp]: https://docs.hoppscotch.io/documentation/clients/mcp-server/overview
[hopp-telemetry]: https://github.com/hoppscotch/hoppscotch/blob/main/packages/hoppscotch-common/src/newstore/settings.ts
[hopp-settings]: https://github.com/hoppscotch/hoppscotch/blob/main/packages/hoppscotch-common/src/pages/settings.vue
[hopp-ai]: https://docs.hoppscotch.io/documentation/features/ai-features
[hopp-vue]: https://github.com/hoppscotch/hoppscotch/blob/main/packages/hoppscotch-common/package.json
[hopp-release]: https://github.com/hoppscotch/releases/releases/tag/v26.10.1-0
[bruno]: https://github.com/usebruno/bruno
[bruno-readme]: https://github.com/usebruno/bruno/blob/main/readme.md
[bruno-license]: https://github.com/usebruno/bruno/blob/main/license.md
[bruno-editions]: https://docs.usebruno.com/license-overview
[bruno-pricing]: https://www.usebruno.com/pricing
[bruno-intro]: https://docs.usebruno.com/introduction
[bruno-bru]: https://docs.usebruno.com/bru-lang/overview
[bruno-yaml]: https://docs.usebruno.com/opencollection-yaml/overview
[bruno-cli]: https://docs.usebruno.com/bru-cli/overview
[bruno-vscode]: https://docs.usebruno.com/vs-code-extension/overview
[bruno-protocols]: https://docs.usebruno.com/send-requests/overview
[bruno-postman]: https://docs.usebruno.com/get-started/import-export-data/postman-migration
[bruno-reports]: https://docs.usebruno.com/bru-cli/run/reports
[bruno-mock]: https://docs.usebruno.com/mock-servers/overview
[bruno-mcp]: https://github.com/usebruno/bruno-mcp
[bruno-telemetry]: https://github.com/usebruno/bruno/blob/main/packages/bruno-app/src/providers/App/useTelemetry.js
[bruno-settings]: https://docs.usebruno.com/get-started/configure/settings
[bruno-secrets]: https://docs.usebruno.com/secrets-management/overview
[bruno-git]: https://docs.usebruno.com/git-integration/overview
[bruno-ai]: https://docs.usebruno.com/ai/bruno-ai/introduction
[bruno-electron]: https://github.com/usebruno/bruno/blob/main/packages/bruno-electron/package.json
[bruno-release]: https://github.com/usebruno/bruno/releases/tag/v4.3.0
[beam]: https://github.com/hlcfan/beam
[beam-readme]: https://github.com/hlcfan/beam/blob/main/README.md
[beam-license]: https://github.com/hlcfan/beam/blob/main/LICENSE
[beam-storage]: https://github.com/hlcfan/beam/blob/main/src/storage/workspace_repo.rs
[beam-scripts]: https://github.com/hlcfan/beam/blob/main/docs/POST_REQUEST_SCRIPT_API_REFERENCE.md
[beam-cargo]: https://github.com/hlcfan/beam/blob/main/Cargo.toml
[beam-release]: https://github.com/hlcfan/beam/releases/tag/v0.11.0
[getman-release]: https://github.com/fzn0x/getman/releases/tag/v0.3.2
[getman-releases]: https://github.com/fzn0x/getman/releases
