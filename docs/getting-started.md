# Getting started

## Install

Getman is one program with no runtime to install. Pick one:

**With the install script** (Linux and macOS):

```sh
curl -fsSL https://fzn0x.github.io/getman/install.sh | sh
```

**With the install script** (Windows, PowerShell):

```powershell
irm https://fzn0x.github.io/getman/install.ps1 | iex
```

Both scripts download the latest release for your system from GitHub,
check its SHA-256 checksum against the release's `SHA256SUMS`, and install
`getman` for your user only. On Linux and macOS it goes in `~/.local/bin`,
and the script tells you what to add to your `PATH` if that directory is
not on it yet. On Windows it goes in `%LOCALAPPDATA%\Programs\getman`,
which the script adds to your user `PATH`.

Set `GETMAN_VERSION` (such as `v0.1.0`) to install a given release, or
`GETMAN_INSTALL_DIR` to choose the directory.

**By hand:** download the archive for your system from the
[releases page](https://github.com/fzn0x/getman/releases), unpack it and
put `getman` (or `getman.exe`) somewhere on your `PATH`.

**From source:** you need [Rust](https://rustup.rs) 1.88 or newer and
[Node.js](https://nodejs.org) 22.12 or newer.

```sh
git clone https://github.com/fzn0x/getman.git
cd getman/web
npm ci
npm run build
cd ..
cargo install --path crates/getman
```

The browser UI is built first because it is embedded in the binary.

Check that it works:

```sh
getman --version
```

## Your first requests

Make a workspace with example requests, in a new folder:

```sh
mkdir my-api && cd my-api
getman init --example
```

`getman init` writes `getman.yaml`, which marks the folder as a workspace,
and a `.gitignore` that keeps machine-only files out of git. `--example`
adds a few requests against [JSONPlaceholder](https://jsonplaceholder.typicode.com),
a free test API, and two environments.

Now open it:

```sh
getman
```

Getman starts a server on `http://127.0.0.1:7878` (or the next free port)
and opens your browser on it. Press <kbd>Ctrl</kbd> <kbd>K</kbd> (or
<kbd>Cmd</kbd> <kbd>K</kbd> on a Mac), type `list`, press Enter to open
**List posts**, then <kbd>Ctrl</kbd> <kbd>Enter</kbd> to send it.

Run in a directory that is not a workspace, `getman` opens your personal
workspace instead, kept in your user data folder, so you always have
somewhere to try a request.

## A workspace in your repository

Getman works best with requests next to the code they call. In your
repository:

```sh
mkdir api && cd api
getman init --name "Orders API"
```

Then add requests in the UI, or by hand: each one is a YAML file.

```yaml
# api/orders/create-order.yaml
name: Create order
method: POST
url: "{{baseUrl}}/orders"
headers:
  - name: Accept
    value: application/json
body:
  type: json
  content: |
    { "sku": "BOOK-42", "quantity": 1 }
checks:
  - status == 201
  - json.id exists
captures:
  orderId: json.id
```

Commit the YAML files. Everything that is yours alone, such as tokens
and personal overrides, goes in `*.local.yaml` files, which the generated
`.gitignore` leaves out. See [Workspaces](workspaces.md) for the full
format.

## Three ways in

| Command | What it opens |
| --- | --- |
| `getman` | The browser UI, on 127.0.0.1 |
| `getman tui` | The terminal UI, for SSH sessions and keyboard purists |
| `getman run` | Every request, with results for people or for CI |

They all read and write the same files, so you can edit a request in the
browser, send it from the terminal and run it in CI without exporting
anything.

## Keyboard

| Keys | Does |
| --- | --- |
| <kbd>Ctrl</kbd> <kbd>Enter</kbd> | Send the request |
| <kbd>Ctrl</kbd> <kbd>S</kbd> | Save |
| <kbd>Ctrl</kbd> <kbd>K</kbd> | Find a request or command |
| <kbd>Alt</kbd> <kbd>N</kbd> | New request |
| <kbd>Alt</kbd> <kbd>L</kbd> | Edit the URL |
| <kbd>Alt</kbd> <kbd>E</kbd> | Switch environment |
| <kbd>Alt</kbd> <kbd>C</kbd> | Copy the request as code |
| <kbd>Alt</kbd> <kbd>W</kbd> | Close the tab |
| <kbd>Alt</kbd> <kbd>[</kbd> <kbd>Alt</kbd> <kbd>]</kbd> | Previous and next tab |
| <kbd>/</kbd> | Filter the sidebar |
| <kbd>?</kbd> | Show every shortcut |

On a Mac, <kbd>Cmd</kbd> takes the place of <kbd>Ctrl</kbd>. Tab and window
keys use <kbd>Alt</kbd> because browsers keep <kbd>Ctrl</kbd> <kbd>N</kbd>,
<kbd>Ctrl</kbd> <kbd>W</kbd> and <kbd>Ctrl</kbd> <kbd>Tab</kbd> for
themselves.

## Next

- [Workspaces](workspaces.md): the files and how they fit together
- [Variables and environments](variables.md)
- [Checks and scripts](checks.md)
- [Running in CI](runner.md)
- [Coming from Postman](why-getman.md#moving-over)
