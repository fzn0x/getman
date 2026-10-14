# Contributing

Thanks for helping! Bug reports, fixes, documentation and new features are
all welcome. For a large change, please open an issue first so we can agree
on the approach before you spend time on it.

## Principles

- **Files are the product.** The YAML format is what people commit, so
  changes to it must stay readable, write nothing at its default, and read
  files written by older versions.
- **Local and private.** Getman never needs an account, never calls home
  and never sends anything the user did not ask it to. Keep it that way.
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
| `web/src/` | The browser UI |
| `docs/`, `site/` | Documentation and the website built from it |

## Commits

Commits follow [Conventional Commits](https://www.conventionalcommits.org):
`type(scope): summary`, such as `fix(core): keep disabled query parameters
on import`. Common types are `feat`, `fix`, `perf`, `refactor`, `test`,
`docs`, `build` and `ci`; scopes are `core`, `cli`, `server`, `tui`, `web`
and `site`. Keep each commit to one logical change that builds and passes
its tests, and use the body to say why.

## Reporting bugs

Include the Getman version (`getman --version`), your operating system, and
the smallest request file that shows the problem, with secrets removed.
Report security problems privately, as [SECURITY.md](SECURITY.md) describes.
