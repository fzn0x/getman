# Contributing

Thanks for helping! Bug reports, fixes, documentation and new features are
all welcome. For a large change, please open an issue first so we can agree
on the approach before you spend time on it.

## Principles

- **Files are the product.** The YAML format is what people commit, so
  changes to it must stay readable, write nothing at its default, and read
  files written by older versions.
- **Local and private.** Getman never needs an account and never sends
  anything the user did not ask it to. The one request it makes on its
  own is the desktop app's daily update check, which the user can turn
  off. Keep it that way.
- **The server is an attack surface.** Anything that lets a web page,
  a shared collection or a script reach outside its sandbox is a security
  bug. Read [Security and privacy](docs/security.md) before touching
  `crates/getman/src/server`, file paths or the script engine.
- **Keyboard first.** Every action in the UI should be reachable without a
  mouse, and the UI follows [DESIGN.md](DESIGN.md).

## Development

You need Rust 1.88 or newer and Node.js 22.12 or newer.

```sh
cd web && npm ci && npm run build && cd ..   # the UI is embedded in the binary
cargo run -- init --example /tmp/play
cargo run -- /tmp/play                       # open it
```

To work on the browser UI with hot reload, run Getman on its default port
and Vite beside it:

```sh
cargo run -- /tmp/play --no-open             # prints a link with #token=...
cd web && npm run dev                        # then open that link on port 5173
```

Set `GETMAN_URL` if Getman runs on a port other than 7878.

Before you send a pull request:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- --deny warnings
cargo test --workspace
cd web
npm run format
npm run check   # svelte-check and Prettier
npm test
```

Clippy runs with its pedantic lints, and the crates deny unsafe code.
`--workspace` includes the desktop app, which on Linux needs the
libraries in the next section.

CI also runs checks that need other tools. You can run them too:

- **Exported code**: `node .github/scripts/verify-snippets.mjs --getman
  target/debug/getman` exports tricky requests in every language and runs
  each snippet against a local server. It needs the runtimes of the
  languages you test (`--lang python,go` limits it; `--skip-missing`
  skips the ones you lack). Run the shell snippets on Linux or macOS.
  Change the code in `crates/getman-core/src/export/codegen.rs` with this
  in mind: a snippet that compiles can still send the wrong bytes.
- **AI agents**: `node .github/scripts/verify-mcp.mjs --getman
  target/debug/getman` drives `getman mcp` with the MCP Inspector.
- **Dependencies**: `cargo deny check` (install it with `cargo install
  cargo-deny --locked`) reads `deny.toml`, and `node
  .github/scripts/check-npm-licenses.mjs` checks the npm packages. A new
  dependency must have a licence compatible with MIT OR Apache-2.0. If
  one has an advisory you decide is harmless, allow it in `deny.toml` with
  the reason.
- **Workflows**: [actionlint](https://github.com/rhysd/actionlint) checks
  `.github/workflows`.

If you bundle anything of someone else's, such as a font, an icon or a
protocol definition, add its notice to
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and keep its licence file
beside it.

Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).

## The desktop app

`crates/getman-desktop` is a [Tauri](https://tauri.app) 2 app that starts
Getman's server in its own process and shows the UI in a native window.
It needs Rust 1.90 or newer, and on Linux the WebKitGTK development
libraries:

```sh
sudo apt install libwebkit2gtk-4.1-dev librsvg2-dev   # Debian, Ubuntu
sudo dnf install webkit2gtk4.1-devel librsvg2-devel   # Fedora
```

Plain `cargo build`, `cargo run` and `cargo test` leave it out, so the
rest builds without them; `--workspace` or `-p getman-desktop` includes
it. Build the UI first, as for `getman`; a debug build reads `web/dist`
from disk, so `npm run build` and a reload in the window show changes.

```sh
cargo run -p getman-desktop -- desktop /tmp/play   # Windows: without "desktop"
```

On macOS and Linux its program is the `getman` command too, so without
`desktop` it runs the command line. Development builds never look for
updates.

To make the installers for your system, use the Tauri command line
pinned in `crates/getman-desktop/package.json`:

```sh
cd crates/getman-desktop
npm ci
npx tauri build          # installers in target/release/bundle
```

On macOS and Linux this renames the app's program to `getman` in
`target/release`, over the command-line build if there is one. On
Windows the installers bring `getman.exe` beside the app: build it and
put it in `crates/getman-desktop/windows/bin` first, then add
`--config windows/command-line.json`, as the release workflow does.

## Releases

Pushing a `v*` tag runs `.github/workflows/release.yml`, which publishes
the command-line archives, the desktop installers and `SHA256SUMS`. It
reads two repository secrets:

| Secret | |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | The private key that signs the desktop app's update files: the contents of the key file `npx tauri signer generate` writes |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Its password; leave it empty for a key without one |

Without `TAURI_SIGNING_PRIVATE_KEY` a release still has every installer,
but no update files and no `latest.json`, so installed apps are not
offered it. The matching public key is `plugins.updater.pubkey` in
`crates/getman-desktop/tauri.conf.json`. Installed apps accept only
updates signed with that key, so keep the private key safe and out of
the repository: a lost key means users must reinstall to get updates.

### Packages

Each release also carries manifests for package managers, written from
the release's own files by `packaging/render.mjs`: `getman.rb` (Homebrew),
`getman.json` (Scoop), `fzn0x.Getman*.yaml` (winget) and
`io.github.fzn0x.getman.yml` (Flatpak, for the desktop app). They hold
the version, the download URLs and the SHA-256 sums, and they are
only release files: nothing publishes them yet, so Getman is not
available from any of these package managers.

To publish them, the maintainer first has to create:

| What | For |
| --- | --- |
| A repository `fzn0x/homebrew-tap` with a `Formula` folder | Homebrew: `brew install fzn0x/tap/getman` |
| A repository `fzn0x/scoop-bucket` with a `bucket` folder | Scoop: `scoop bucket add fzn0x https://github.com/fzn0x/scoop-bucket` |
| A fork of `microsoft/winget-pkgs` | winget: a pull request adding the three files under `manifests/f/fzn0x/Getman/<version>/`; the first one is reviewed by hand |
| A Flathub submission, or a Flatpak repository | Flatpak: the manifest repackages the `.deb`; try it first with `flatpak-builder` |
| A repository secret `PACKAGES_TOKEN`: a fine-grained personal access token with read and write access to the contents of the tap and the bucket | The publishing job in `release.yml`, which is commented out |

Then uncomment the `publish-packages` job at the end of
`.github/workflows/release.yml`, set its `if` to `true`, and check it on
a release candidate tag first. Try the manifests by hand before that:
`brew install --formula ./getman.rb`, `scoop install ./getman.json`,
`winget validate` on the folder of winget files, and `flatpak-builder`
on the Flatpak one. To render them from a release you have downloaded:

```sh
node packaging/render.mjs --tag v0.2.0 --dist downloads --out manifests
```

## Where things are

| | |
| --- | --- |
| `crates/getman-core/src/workspace.rs` | Reading and writing workspace files |
| `crates/getman-core/src/vars.rs`, `dynamic.rs` | Variables and `{{$dynamic}}` values |
| `crates/getman-core/src/prepare.rs`, `http.rs`, `auth.rs` | Building and sending requests |
| `crates/getman-core/src/checks.rs` | Checks and captures |
| `crates/getman-core/src/script/` | The JavaScript engine and the `gm` API (`prelude.js`) |
| `crates/getman-core/src/runner.rs` | Collection runs |
| `crates/getman-core/src/import/`, `export/` | Postman and cURL import, code export |
| `crates/getman/src/commands/` | The command line |
| `crates/getman/src/server/` | The local server behind the browser UI |
| `crates/getman/src/tui/` | The terminal UI |
| `crates/getman-desktop/` | The desktop app: window, updates, installers |
| `web/src/` | The browser UI |
| `docs/`, `site/` | Documentation and the website built from it |

## Commits

Commits follow [Conventional Commits](https://www.conventionalcommits.org):
`type(scope): summary`, such as `fix(core): keep disabled query parameters
on import`. Common types are `feat`, `fix`, `perf`, `refactor`, `test`,
`docs`, `build` and `ci`; scopes are `core`, `cli`, `server`, `tui`, `web`,
`desktop` and `site`. Keep each commit to one logical change that builds and passes
its tests, and use the body to say why.

## Reporting bugs

Include the Getman version (`getman --version`), your operating system, and
the smallest request file that shows the problem, with secrets removed.
Report security problems privately, as [SECURITY.md](SECURITY.md) describes.
