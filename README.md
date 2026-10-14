<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/getman-dark.png">
    <img src="docs/images/getman-light.png" width="900" alt="Getman showing a request's checks beside its response: a large 200 on a halftone field, with time, size and 4 of 4 tests passed">
  </picture>
</p>

# Getman

[![CI](https://github.com/fzn0x/getman/actions/workflows/ci.yml/badge.svg)](https://github.com/fzn0x/getman/actions/workflows/ci.yml)
[![Docs](https://github.com/fzn0x/getman/actions/workflows/pages.yml/badge.svg)](https://fzn0x.github.io/getman/)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

A local-first API client. Your requests are plain YAML files in your
repository, and one small program opens them in your browser, in your
terminal or in CI. No account, no cloud, no telemetry.

**Documentation: <https://fzn0x.github.io/getman/>**

```sh
getman init --example   # a workspace with example requests
getman                  # open it in your browser
getman tui              # or in the terminal
getman run              # or run every request, for CI
```

## Why Getman over Postman

- **No account, ever.** Saving requests, environments, variables, scripts,
  the runner and history all work offline. Postman needs you signed in to
  a workspace to save to collections or use environments.
- **Your requests live with your code.** One short YAML file per request,
  reviewed in pull requests and versioned with the API it calls. Tokens
  and personal values go in git-ignored `*.local.yaml` files, so the rest
  is always safe to commit.
- **Free for any team.** You share a workspace by committing it. There are
  no seats, and no paid plan for teams to collaborate.
- **One binary, three ways in.** A browser UI, a terminal UI for SSH
  sessions, and a CI runner with JUnit and JSON reports, in one file of
  about 13 MB with no runtime to install.
- **Tests without code.** Checks such as `status == 200` and
  `json.data[0].id exists` are lines in the request file. Scripts are
  there when you need them.
- **Bring your collections.** `getman import` reads Postman collections,
  environments and globals. `pm` is an alias of Getman's `gm` script API,
  so most scripts run unchanged.
- **Open source**, under MIT or Apache-2.0.

Postman still does more: other protocols such as WebSocket and gRPC,
hosted mocks and monitors, and real-time collaboration. The
[full comparison](docs/why-getman.md) links Postman's own pages for every
claim and says when to stay.

## Install

Linux and macOS:

```sh
curl -fsSL https://fzn0x.github.io/getman/install.sh | sh
```

Windows (PowerShell):

```powershell
irm https://fzn0x.github.io/getman/install.ps1 | iex
```

The scripts download the latest release for your system, verify its
SHA-256 checksum and install `getman` for your user only. You can also
download an archive from the [releases page](https://github.com/fzn0x/getman/releases),
or [build from source](#building-from-source).

## Requests are files

```text
api/
  getman.yaml               settings for every request
  environments/
    staging.yaml            shared values
    staging.local.yaml      your secrets (git-ignored)
  orders/
    _folder.yaml            auth, headers and scripts for the folder
    create-order.yaml
```

```yaml
# api/orders/create-order.yaml
name: Create order
method: POST
url: "{{baseUrl}}/orders"
body:
  type: json
  content: |
    { "sku": "BOOK-42", "quantity": 1 }
checks:
  - status == 201
  - json.id exists
  - time < 500
captures:
  orderId: json.id
```

Fields at their defaults are not written, so diffs stay small. Folders
and the workspace pass headers, authentication, variables, scripts and
settings down to the requests inside them. See
[Workspaces](docs/workspaces.md).

## Features

- **A keyboard-first browser UI.** Tabs, a command palette
  (<kbd>Ctrl</kbd> <kbd>K</kbd>), send with <kbd>Ctrl</kbd>
  <kbd>Enter</kbd>, variable highlighting and completion, a JSON path
  filter on responses, and light and dark themes.
- **Every common body and auth type.** JSON, XML, text, forms, multipart
  with files, binary, GraphQL; Basic, Bearer, API key, Digest and OAuth 2.0
  client credentials and password grants, with tokens cached.
- **Variables with clear rules.** Globals, workspace, environment,
  folder, request, data and local scopes, more than 30 dynamic values
  such as `{{$uuid}}`, and secrets from `GETMAN_*` environment variables.
- **Scripts** in an embedded JavaScript engine: an expressive assertion
  library, JSON Schema, `sendRequest`, crypto helpers and run control.
- **A runner** with iterations, CSV and JSON data files, delays, bail
  and reports, in the UI and as `getman run`.
- **History** of sent requests, kept per workspace, and a cookie jar you
  can inspect and clear.
- **cURL in and out.** Paste a cURL command into the URL bar; export any
  request as cURL, HTTPie, PowerShell, fetch, Python, Go, Rust or Java.
- **A terminal UI** for when there is no browser.

## Run it in CI

```yaml
# .github/workflows/api.yml
- name: Install Getman
  run: |
    curl -fsSL https://fzn0x.github.io/getman/install.sh | sh
    echo "$HOME/.local/bin" >> "$GITHUB_PATH"
- name: Run the API checks
  run: getman run -w api -e staging -r cli -r junit=getman.xml
  env:
    GETMAN_API_TOKEN: ${{ secrets.API_TOKEN }}
```

```text
Orders API with the staging environment: 3 requests

  ✓ POST    Create order   201 Created 182 ms · 96 B
  ✗ GET     Get order      404 Not Found 41 ms · 23 B
      ✗ status == 200
        status is 404; expected == 200
  ✓ DELETE  Archive order  204 No Content 37 ms · 0 B
```

`getman run` exits with 1 when a request fails, and 2 when it cannot
start. See [Running in CI](docs/runner.md).

## Security

The browser UI is served by Getman on `127.0.0.1` only. The server
refuses requests addressed to other host names (DNS rebinding) or sent
from other web pages, and every API call needs a random token that only
the link from your terminal carries. Requests can upload files only from
inside the workspace, and `{{$env.…}}` reads only variables that start
with `GETMAN_`. See [Security and privacy](docs/security.md), and
[SECURITY.md](SECURITY.md) to report a vulnerability.

## Documentation

- [Getting started](docs/getting-started.md)
- [Why Getman](docs/why-getman.md), and moving over from Postman
- [Workspaces](docs/workspaces.md) ·
  [Variables and environments](docs/variables.md) ·
  [Checks and captures](docs/checks.md) · [Scripts](docs/scripts.md)
- [Running in CI](docs/runner.md) ·
  [Import and export](docs/import-export.md) ·
  [Terminal UI](docs/terminal.md) · [Command line](docs/cli.md)
- [Security and privacy](docs/security.md)

## Building from source

You need [Rust](https://rustup.rs) 1.88 or newer and
[Node.js](https://nodejs.org) 22.12 or newer. The browser UI is built
first, because it is embedded in the binary.

```sh
git clone https://github.com/fzn0x/getman.git
cd getman/web
npm ci
npm run build
cd ..
cargo install --path crates/getman
```

### Project layout

| Path | |
| --- | --- |
| `crates/getman-core` | The engine: workspace files, variables, HTTP, auth, checks, scripts, the runner, import and code export |
| `crates/getman` | The `getman` binary: command line, local server, terminal UI and reporters |
| `web` | The browser UI: Svelte 5, TypeScript and CodeMirror 6 |
| `site` | This documentation as a website, published to GitHub Pages |
| `docs` | The documentation pages |

### Testing

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- --deny warnings
cd web && npm run check && npm test
```

CI runs these on Linux, Windows and macOS, smoke-tests the command line,
and checks the minimum supported Rust version. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE)
or the [MIT license](LICENSE-MIT), at your option. Contributions are
accepted under the same terms. The Departure Mono font in `web/public/fonts`
is under the SIL Open Font License; see its licence file there.

Postman is a trademark of Postman, Inc. Getman is not affiliated with or
endorsed by Postman.
