# Development

How to build Getman from source, run it while you change it, and run
the tests and checks that CI runs before you open a pull request.

Getman is a Rust workspace of three crates with a Svelte web UI in
`web/`. [Architecture](architecture.md) explains what lives where.
[CONTRIBUTING.md](https://github.com/fzn0x/getman/blob/main/CONTRIBUTING.md)
has the principles every change follows; read it before a large change.

## Prerequisites

| Tool | Version | Set in |
| --- | --- | --- |
| Rust | 1.88 or newer for `getman` and `getman-core` | `rust-version` in the root `Cargo.toml` |
| Rust | 1.90 or newer for the desktop app, which Tauri 2.12 needs | `rust-version` in `crates/getman-desktop/Cargo.toml` |
| Rust components | `clippy` and `rustfmt` | `rust-toolchain.toml` |
| Node.js | 22.12 or newer | `engines` in `web/package.json`, `site/package.json` and `crates/getman-desktop/package.json` |

`rust-toolchain.toml` selects the `stable` channel with the `minimal`
profile, so rustup builds the repository with the current stable
release. CI uses stable too, plus a separate check on 1.88 (see
[What CI runs](#what-ci-runs)). CI installs Node.js 22.

### The desktop app's system libraries

`crates/getman-desktop` is a [Tauri](https://tauri.app) 2 app. On Linux
it compiles against WebKitGTK 4.1, GTK 3 and librsvg, so install their
development packages:

```sh
sudo apt install libwebkit2gtk-4.1-dev librsvg2-dev   # Debian, Ubuntu
sudo dnf install webkit2gtk4.1-devel librsvg2-devel   # Fedora
```

CI installs the same packages with the composite action in
`.github/actions/setup-desktop-linux`. The release workflow also
installs `file` and `xdg-utils` before it bundles the AppImage. On
Windows and macOS the workflows install nothing beyond Rust and
Node.js.

You only need these libraries for the desktop app. The workspace's
`default-members` are `crates/getman` and `crates/getman-core`, so plain
`cargo build`, `cargo run` and `cargo test` leave the desktop app out.
`--workspace` or `-p getman-desktop` includes it.

## Building

### The web UI

The browser UI is built separately and then built into the binary.
Build it first:

```sh
cd web
npm ci
npm run build
```

`npm run build` runs two steps from `web/package.json`:

1. `vite build` writes the UI to `web/build/`.
2. `node compress.mjs` writes every file of `web/build/` to `web/dist/`,
   Brotli-compressed at the highest quality, as `<file>.br`. It prints
   the size before and after.

Both folders are git-ignored. Storing the files compressed keeps about
a megabyte out of the binary.

### How the binary gets the UI

`crates/getman/src/server/assets.rs` embeds `web/dist` with
[rust-embed](https://crates.io/crates/rust-embed):

- A release build stores the files inside the binary.
- A debug build reads `web/dist` from disk while it runs. After
  `npm run build`, reload the page to see the change; no Rust rebuild
  is needed.
- `crates/getman/build.rs` tells Cargo to rebuild when `web/dist`
  changes, so a release binary never carries a stale UI.
- The folder may be missing. The binary still builds, and its page says
  the build has no UI. The command line works either way.

The server sends each file as stored to a browser that accepts Brotli,
and decompresses it for any other.

### The command line

```sh
cargo build                 # target/debug/getman
cargo build --release       # target/release/getman
```

The release profile in the root `Cargo.toml` turns on LTO, uses one
codegen unit and strips the binary.

### The desktop app

Build the UI first, as for `getman`. Then run the app on a workspace:

```sh
cargo run -p getman-desktop -- desktop /tmp/play   # Windows: without "desktop"
```

On macOS and Linux the app's program is also the `getman` command, so
without `desktop` it runs the command line. A debug build reads
`web/dist` from disk, so `npm run build` and a reload in the window
show UI changes. Development builds never look for updates.

To make the installers for your system, use the Tauri command line
pinned in `crates/getman-desktop/package.json` (`@tauri-apps/cli` 2):

```sh
cd crates/getman-desktop
npm ci
npx tauri build          # installers in target/release/bundle
```

On macOS and Linux this renames the app's program to `getman` in
`target/release`, over the command-line build if there is one.

On Windows the installers bring `getman.exe` beside the app. Build it,
put it in `crates/getman-desktop/windows/bin`, and add the extra
configuration, as the release workflow does:

```powershell
cargo build --release --package getman
New-Item -ItemType Directory -Force crates/getman-desktop/windows/bin
Copy-Item target/release/getman.exe crates/getman-desktop/windows/bin/
cd crates/getman-desktop
npm ci
npx tauri build --config windows/command-line.json
```

## Running locally

### The browser UI

```sh
cargo run -- init --example /tmp/play   # a workspace with example requests
cargo run -- /tmp/play                  # serve the UI and open it
```

Getman listens on 127.0.0.1, port 7878 by default. When that port is
taken it tries the next 19, then any free port. `--port` picks one.
It prints a link that ends in `#token=...`; the UI needs that token.

### With hot reload

Run Getman without opening a browser, and Vite beside it:

```sh
cargo run -- /tmp/play --no-open   # prints a link with #token=...
cd web && npm run dev              # Vite on port 5173
```

Open the printed link with the port changed to 5173. Vite serves the
UI with hot reload and proxies `/api` to Getman. The proxy drops the
`Origin` header, because Getman refuses requests from other origins.

The proxy target is `GETMAN_URL`, which defaults to
`http://127.0.0.1:7878`. Set it when Getman runs on another port:

```sh
GETMAN_URL=http://127.0.0.1:7879 npm run dev
```

```powershell
$env:GETMAN_URL = "http://127.0.0.1:7879"; npm run dev
```

### The other front ends

```sh
cargo run -- tui /tmp/play         # the terminal UI
cargo run -- mock -w /tmp/play     # the mock server, port 7979 by default
cargo run -- mcp /tmp/play         # the MCP server on standard input and output
cargo run -- run -w /tmp/play      # run the workspace, as CI would
```

[Command line](cli.md) lists every command and flag.

### The documentation site

```sh
cd site
npm ci
npm run build     # writes site/dist and checks every link
npm run preview   # builds, then serves http://127.0.0.1:4000/getman/
```

`npm run preview` serves `site/dist` under `/getman/`, as GitHub Pages
does. Set `PORT` to use another port. The build reads the fonts and
`favicon.svg` from `web/public` and the color tokens from
`web/src/styles/tokens.css`, so the site and the app share one scheme.

## Tests and checks

Run these before you send a pull request:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- --deny warnings
cargo test --workspace
cd web
npm run format
npm run check
npm test
```

| Command | Checks |
| --- | --- |
| `cargo fmt --all` | Formats the Rust code. CI runs it with `--check` |
| `cargo clippy --workspace --all-targets -- --deny warnings` | Clippy with its pedantic lints, set in the root `Cargo.toml`; a warning fails |
| `cargo test --workspace` | Every crate's tests, the desktop app's included |
| `npm run format` in `web/` | Formats the UI's files with Prettier |
| `npm run check` in `web/` | `svelte-check --fail-on-warnings`, then `prettier --check .` |
| `npm test` in `web/` | The UI's unit tests with Vitest: `src/**/*.test.ts` |
| `npm run check` in `site/` | `prettier --check .` on the site's code |
| `npm run build` in `site/` | Builds the site and fails on a broken link |

The workspace lints in the root `Cargo.toml` also deny `unsafe` code
and warn on `unreachable_pub` and the Rust 2018 idioms. `--workspace`
includes the desktop app, which on Linux needs the libraries in
[The desktop app's system libraries](#the-desktop-apps-system-libraries).

### Where the tests are

The Rust tests are `#[cfg(test)]` modules beside the code they test.
Tests that send requests use local servers that they start themselves:

- `crates/getman-core/src/test_server.rs` answers over plain HTTP, TLS
  (optionally asking for a client certificate) and HTTP/3.
- `crates/getman-core/src/grpc/test_server.rs` is a gRPC echo service
  on tonic, with optional reflection.
- `crates/getman-core/src/realtime/test_servers.rs`,
  `test_socketio.rs` and `test_proxies.rs` are a WebSocket server, a
  Socket.IO v4 server, and HTTP and SOCKS5 proxies.

Sample files for the importers and the private-key code are in
`crates/getman-core/tests/fixtures`.

### Dependencies and licences

```sh
cargo install cargo-deny --locked
cargo deny check
node .github/scripts/check-npm-licenses.mjs
```

`cargo deny check` reads `deny.toml`. It checks the Rust dependencies,
with all features on, for known advisories and their licences, and
allows crates from crates.io only. A new dependency
must have a permissive licence, one of those `deny.toml` allows. If an advisory
does not apply to Getman, allow it in `deny.toml` and say why, as the
existing entries do.

If you bundle anything of someone else's, such as a font, an icon or a
protocol definition, add its notice to
[THIRD_PARTY_NOTICES.md](https://github.com/fzn0x/getman/blob/main/THIRD_PARTY_NOTICES.md)
and keep its licence file beside it.

### Workflows

[actionlint](https://github.com/rhysd/actionlint) checks the files in
`.github/workflows`. No workflow runs it, so run it yourself after you
change one.

## What CI runs

`.github/workflows/ci.yml` runs on every pull request and on every push
to `main`. A newer push to the same branch cancels the run before it.

| Job | Runs |
| --- | --- |
| Web UI | `npm ci`, `npm run check`, `npm test` and `npm run build` in `web/`, then keeps `web/dist` for the jobs below |
| Format and lint | `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --locked -- --deny warnings`, with WebKitGTK installed |
| Test on ubuntu-latest, windows-latest, macos-latest | `cargo test --workspace --locked`, then a smoke run of `getman init`, `getman import --curl` and `getman export --to python` in a new workspace. On Linux and macOS it also runs `--version` through the desktop app's program |
| Minimum supported Rust version | `cargo +1.88 check --workspace --exclude getman-desktop --all-targets --locked` |
| Dependency advisories and licences | `cargo deny check --all-features`, with `EmbarkStudios/cargo-deny-action` |
| npm licences | `node .github/scripts/check-npm-licenses.mjs` |
| Build getman for the snippet checks | `cargo build --locked --package getman`, kept for the next two jobs |
| MCP server with the Inspector | `node .github/scripts/verify-mcp.mjs` on that build |
| Snippets in ... | `node .github/scripts/verify-snippets.mjs` once per language group: curl, Wget, HTTPie, PowerShell, JavaScript, Python, Go, Rust, Java, Kotlin, C#, Swift, PHP, Ruby and Dart |

`.github/workflows/pages.yml` builds the documentation site; see
[How it is deployed](#how-it-is-deployed).

### verify-snippets.mjs

Checks that exported code sends what the request says. It writes a set
of tricky requests into a throwaway workspace, with quotes, `$`,
backslashes, line breaks, non-ASCII text, binary bodies and multipart
forms. It exports each one with `getman export` in every language and
runs each snippet against a local echo server. The method, path, query,
the headers that matter and the body must arrive as the request file
says. The language `getman` runs `getman send` on each request, as the
reference.

```sh
node .github/scripts/verify-snippets.mjs --getman target/debug/getman
node .github/scripts/verify-snippets.mjs --getman target/debug/getman --lang python,go
node .github/scripts/verify-snippets.mjs --list
```

| Option | Does |
| --- | --- |
| `--getman PATH` | The binary to test. Defaults to `getman` on the `PATH` |
| `--lang a,b` | Only these languages |
| `--case a,b` | Only these cases |
| `--skip-missing` | Reports a language whose runtime is missing as skipped instead of failed. CI never uses it |
| `--work DIR` | Keeps the workspace and the snippets in `DIR` |
| `--list` | Prints the languages and cases, then stops |

Runtimes are found on the `PATH`. `PYTHON` names the interpreter that
has `requests` (default `python3`), `HTTP_BIN` the HTTPie command
(default `http`), and `KOTLINC` and `KOTLIN_CLASSPATH` the Kotlin
compiler and its jars. Run the shell snippets (curl, Wget, HTTPie) on
Linux or macOS. Git Bash on Windows drops carriage returns from
arguments and turns non-ASCII text into question marks, so some cases
fail there for reasons unrelated to the snippet. The script reaches no
network beyond 127.0.0.1, except to install the libraries the snippets
import, once, into its work directory.

Change `crates/getman-core/src/export/codegen.rs` with this check in
mind: a snippet that compiles can still send the wrong bytes.

### verify-mcp.mjs

Drives `getman mcp` with the official MCP Inspector command line,
pinned to `@modelcontextprotocol/inspector@2.12.0`, the way a client
would. It checks the list of tools, calls `list_requests` and
`send_request` against a local server, and checks that a `GETMAN_*`
value is masked in what the agent gets back.

```sh
node .github/scripts/verify-mcp.mjs --getman target/debug/getman
```

It needs Node.js and npm, and downloads the Inspector with `npx` the
first time.

### check-npm-licenses.mjs

Reads the lockfiles of `web/`, `site/` and `crates/getman-desktop/` and
checks every package's licence. A package that ships in a build must
have a permissive licence, such as MIT, ISC, Apache-2.0 or a BSD
licence. A package used only to build (`dev` in the lockfile) may also
be MPL-2.0. A package with no licence fails, so a person looks at each
new dependency.

```sh
node .github/scripts/check-npm-licenses.mjs
```

It reads only the lockfiles, so it needs no `npm ci` first.

### setup-kotlin.sh

Installs what the Kotlin snippets need, since JetBrains publishes no
setup action: the Kotlin compiler 2.4.21, OkHttp 4.12.0 and Okio 3.6.0.
Each download is checked against a pinned SHA-256. Java must already be
installed. The files go to `$RUNNER_TEMP/kotlin`, or `/tmp/kotlin`
outside CI.

```sh
bash .github/scripts/setup-kotlin.sh
```

In CI it writes `KOTLINC` and `KOTLIN_CLASSPATH` to `$GITHUB_ENV`.
Elsewhere it prints them; export both before you run
`verify-snippets.mjs --lang kotlin`.

## The documentation

Each file in `docs/` is one page of the site, and the site's code is in
`site/`:

| Path | Holds |
| --- | --- |
| `docs/*.md` | The pages. `docs/README.md` is the documentation's index |
| `docs/images/` | Images the pages show |
| `site/build.mjs` | The build: the sidebar order, Markdown to HTML, search, sitemap and the link check |
| `site/src/` | The page layout, the landing page, the site's CSS and script |
| `site/static/` | Files copied as they are, such as `install.sh` and `install.ps1` |
| `site/serve.mjs` | The local preview server |

The build renders Markdown with marked and highlights code with Shiki.
A page's `# Title` is its title, and its first paragraph of plain text
becomes the page description, cut to 200 characters. Its `##` headings
form the "On this page" list. Links between pages are written to `.md`
files and served as `.html`.

### How it is deployed

`.github/workflows/pages.yml` has two jobs:

| Job | Runs |
| --- | --- |
| Build the site | `npm ci`, `npm run check`, `sh -n site/static/install.sh` and `npm run build` in `site/`, then uploads `site/dist` |
| Deploy | Publishes the upload to GitHub Pages at https://fzn0x.github.io/getman/, on `main` only |

It runs on a push to `main` that changes `docs/`, `site/`, the fonts,
`favicon.svg`, `web/src/styles/tokens.css` or the workflow itself. A
pull request that changes the docs runs the build without deploying,
so a broken link fails before merge. It can also be started by hand.

### Adding a page

1. Write `docs/<name>.md`. Start with `# Title` and a paragraph of
   plain text: no list, table or code before it.
2. Add `<name>` to the `sections` list in `site/build.mjs`, under the
   section it belongs to. The list sets the sidebar and the previous
   and next links.
3. Run `npm run build` in `site/`.

The build fails when a page in `sections` has no file, or a file in
`docs/` is missing from `sections`. `docs/README.md` and translations
are the exceptions. A translation is named after its guide with a
two-letter language code in capitals, such as
`docs/getting-started-ID.md`. It is built as a page of its own and stays
out of the sidebar and search.

The build also fails when a link between pages points at a page that
does not exist, or at an `#anchor` that no heading has. An anchor is
the heading's text in lower case, with punctuation removed and spaces
turned into hyphens: `## Exit status` is `runner.md#exit-status`. Link
to files outside `docs/` with full GitHub URLs, since only `docs/` is
on the site.

## Commits and pull requests

For a large change, open an issue first, so the approach is agreed
before you spend time on it.

Commits follow [Conventional Commits](https://www.conventionalcommits.org):
`type(scope): summary`, such as
`fix(core): keep disabled query parameters on import`.

| | |
| --- | --- |
| Types | `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `build`, `ci` |
| Scopes | `core`, `cli`, `server`, `tui`, `web`, `desktop`, `site` |

Keep each commit to one logical change that builds and passes its
tests. Use the body to say why.

The pull request template asks you to confirm that:

- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- --deny warnings`
  and `cargo test --workspace` pass.
- For the browser UI, `npm run check` and `npm test` pass in `web/`.
- A change to the YAML format writes nothing at the default and still
  reads older files.
- Tests cover what changed.
- The page in `docs/` is updated, `docs/cli.md` too for commands and
  flags, and `CHANGELOG.md` under Unreleased.
- Commits follow Conventional Commits, one logical change each.

Before you touch `crates/getman/src/server`, file paths or the script
engine, read [Security and privacy](security.md). Anything that lets a
web page, a shared collection or a script reach outside its sandbox is
a security bug. Report security problems privately, as
[SECURITY.md](https://github.com/fzn0x/getman/blob/main/SECURITY.md)
describes. Everyone taking part follows the
[Code of Conduct](https://github.com/fzn0x/getman/blob/main/CODE_OF_CONDUCT.md).
