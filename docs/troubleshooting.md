# Troubleshooting

Fixes for the problems you may meet installing and running Getman. Each
heading names the symptom, or the message Getman shows, so you can search
the page for the text you see.

The [desktop app](desktop.md#troubleshooting) page has a short list for
the app; this page covers the same ground and more. Error messages from
your operating system differ between systems, so the ones quoted here
may read a little differently on yours.

## Getting more detail

Getman writes no log file and has no log level to raise. These show what
happened instead:

| Where | Shows |
| --- | --- |
| `getman send -i REQUEST` | The status line and headers before the body, and the timing and TLS details after it |
| `getman send --json REQUEST` | Everything that happened, as JSON |
| `getman run -v` | Every response's status line, headers and body |
| `getman run -r json=run.json` | The whole run as a JSON file; see [Reports](runner.md#reports) |
| **Console** in the UI, <kbd>Ctrl</kbd> <kbd>J</kbd> | Each request with its headers, bodies, script output, failed checks and the error that stopped it; see [Console](browser-ui.md#console) |
| The response's **Sent** tab | The request exactly as it went out; see [The request as sent](browser-ui.md#the-request-as-sent) |
| **Settings › About** in the desktop app | The version, how this copy was installed, and why an update failed |

The command line prints errors to standard error after `error:`, and
warnings after `warning:`. Its exit status says what kind of failure it
was; see [Exit status](cli.md#exit-status).

When the desktop app cannot start, it shows the reason in a dialog and
prints it to standard error as `getman: Could not start: ...`. On macOS
and Linux, start it from a terminal with `getman desktop` to see it
there too.

## Installing

### The getman command is not found

A terminal says `getman: command not found`, or PowerShell says the term
`getman` is not recognized.

- **Open a new terminal.** A terminal keeps the `PATH` it started with.
  So do programs that were running already, such as your editor and the
  terminals inside it: restart them.
- **Install script on Linux or macOS:** the script does not change your
  `PATH`. Add the line it printed to your shell's startup file; see
  [Install script for Linux and macOS](installation.md#install-script-for-linux-and-macos).
- **Windows setup:** the box **Add the getman command to PATH** was
  unticked. Run the setup again with it ticked, or add
  `%LOCALAPPDATA%\Getman\bin` to your `PATH`.
- **Windows MSI** installed with `ADDTOPATH=0`: add
  `C:\Program Files\Getman\bin` to your `PATH`.
- **Portable zip:** it never touches `PATH`. Run `bin\getman.exe` from
  its folder, or add that folder to your `PATH`.
- **macOS app:** choose **Install the getman command** in
  **Settings › About**. When it says it installed the command in
  `~/.local/bin`, add that directory to your `PATH`, since macOS does not
  have it there by default.
- **AppImage:** it is not on `PATH`. Run the AppImage itself with a
  command, such as `./Getman-linux-x86_64.AppImage run`.

### An older getman runs

`getman --version` shows another version than the one you installed.
More than one `getman` is on your `PATH`, and the first one wins. List
them:

```sh
type -a getman
```

```powershell
where.exe getman
```

Remove the copies you do not want, or update them too. Each kind of
install has its own place: the install script uses `~/.local/bin` or
`%LOCALAPPDATA%\Programs\getman`, the .deb and .rpm packages
`/usr/bin`, the Windows setup `%LOCALAPPDATA%\Getman\bin` and the MSI
`C:\Program Files\Getman\bin`. Homebrew and Scoop use their own
directories.

### Install script errors

On Linux and macOS the script's messages start with `getman install:`.

| Message | What to do |
| --- | --- |
| `install curl or wget first` | Install one of them with your package manager |
| `there is no release for <system>; on Windows use install.ps1, or build from source` | The script is for Linux and macOS. On Windows, including Git Bash, use the [PowerShell script](installation.md#install-script-for-windows). On other systems, [build from source](installation.md#building-from-source) |
| `there is no release for <machine>; build from source` | There are builds for x86_64 and ARM64 only. [Build from source](installation.md#building-from-source) |
| `could not reach GitHub` | The script could not open `https://github.com/fzn0x/getman/releases/latest`. Check your connection and proxy |
| `could not find the latest release at https://github.com/fzn0x/getman/releases` | GitHub did not lead to a release. Name one with `GETMAN_VERSION=v0.3.4` |
| `could not download <url>` | Check `GETMAN_VERSION`: it is the release tag, with its `v` |
| `<file> is not listed in SHA256SUMS` | That release has no build for your system |
| `sha256sum or shasum is needed to check the download` | Install coreutils, which has `sha256sum` |
| `the checksum does not match, so nothing was installed` | Run it again. If it keeps failing, something between you and GitHub changes the downloads |

The PowerShell script's messages:

| Message | What to do |
| --- | --- |
| `There is no Getman release for <architecture> Windows.` | 32-bit Windows has no build. [Build from source](installation.md#building-from-source) |
| An error from `Invoke-RestMethod` about `api.github.com` | The script asks the GitHub API for the newest release, and GitHub limits how often one address may ask. Set `$env:GETMAN_VERSION = 'v0.3.4'` first, which skips the question |
| `<file> is not listed in SHA256SUMS.` | That release has no build for Windows |
| `The checksum does not match, so nothing was installed.` | Run it again. If it keeps failing, something between you and GitHub changes the downloads |

### GLIBC not found on Linux

`getman` stops at once with an error such as:

```text
getman: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.39' not found (required by getman)
```

The command-line archives, which the install script and Homebrew
install, are built on Ubuntu 24.04. They need a glibc at least as new as
the one they were built against: 2.39 for Getman 0.3.4, as in Ubuntu
24.04 and Debian 13. `ldd --version` shows yours. On an older
distribution:

- install the desktop app's [.deb, .rpm or AppImage](installation.md#linux),
  which are built on Ubuntu 22.04 and bring the `getman` command too; or
- [build from source](installation.md#building-from-source).

There is no build for musl systems such as Alpine. In a container, use
an image with glibc, such as `debian:stable-slim` in the
[GitLab example](runner.md#gitlab-ci).

## Desktop app

### Windows protected your PC

SmartScreen shows this the first time you run the setup, the MSI or the
portable `Getman.exe`, because Getman is not signed with a paid
certificate. Choose **More info**, then **Run anyway**. To make sure the
file is the one GitHub published, check it against the release's
`SHA256SUMS` first; see [Verifying a download](installation.md#verifying-a-download).

### The portable app does not open

The portable copy needs the Microsoft Edge WebView2 Runtime, which
Windows 10 and 11 normally have. Without it, the window cannot open.
Install the runtime from
[Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/), or
use the setup or the MSI, which install it when it is missing.

### macOS cannot check Getman for malicious software

Getman is not notarized by Apple, so the first time you open it macOS
refuses. Click **Done**, open **System Settings › Privacy & Security**,
scroll to the message about Getman, click **Open Anyway** and confirm.
After that it opens like any app.

A command-line archive downloaded with a browser can get the same check.
Clear it for the program you unpacked, or install with the install script
or Homebrew instead, which download with `curl`:

```sh
xattr -d com.apple.quarantine ./getman
```

### Getman is damaged

macOS says "Getman is damaged and can't be opened". Download the disk
image again. If it still says so, clear the quarantine flag of the app:

```sh
xattr -dr com.apple.quarantine /Applications/Getman.app
```

### Install the getman command is missing

**Settings › About** offers the button only on macOS, and only while the
app does not run from the mounted disk image. Drag Getman to
**Applications**, open it from there and look again. Once the command is
installed, the section says where in place of the button.

If it fails:

- `<path> is a getman command installed another way; Getman leaves it as
  it is.` A `getman` that is not a link to the app is already there, such
  as one from the install script. Remove it, or keep using that one.
- `Could not create <path>: ...` Getman could write to neither
  `/usr/local/bin` nor `~/.local/bin`. Check that you own `~/.local`.

### The AppImage does not start

- **Permission denied:** a download is not executable. Run
  `chmod +x Getman-linux-x86_64.AppImage`.
- **It says it needs FUSE:** install FUSE 2, with
  `sudo apt install libfuse2` on Ubuntu 22.04 or
  `sudo apt install libfuse2t64` on 24.04.
- **It runs the command line instead of the window:** you gave it
  arguments. Without arguments it opens the window, and so does
  `./Getman-linux-x86_64.AppImage desktop`.

### The window stays blank on Linux

Some graphics drivers trip WebKitGTK, which draws the window. Start
Getman with `WEBKIT_DISABLE_DMABUF_RENDERER=1` set:

```sh
WEBKIT_DISABLE_DMABUF_RENDERER=1 ./Getman-linux-x86_64.AppImage
WEBKIT_DISABLE_DMABUF_RENDERER=1 getman desktop
```

### A second Getman does not open

The app runs once. Starting it again brings the open window to the front.
To work on a second workspace at the same time, open it in the browser
UI beside the app: `getman ./other-api`.

### The app opens another workspace

The app opens the workspace you used last. When that folder has moved,
been renamed or been deleted, it opens your personal workspace instead.
Choose **Open a workspace folder** and pick the folder again. On macOS
and Linux, `getman desktop ./api` opens the app on a given folder.

### Updates do not arrive

**Settings › About** says when a copy does not update itself, and why:

| It says | What to do |
| --- | --- |
| Development builds don't update themselves. | It was built from source. Build the new version, or install a release |
| Getman was installed from its .deb package, so your package manager updates it | Install the newer .deb from the releases page |
| Getman was installed from its .rpm package, so your package manager updates it | Install the newer .rpm from the releases page |
| This is the portable copy, which doesn't replace itself | Download the new zip from the releases page |
| This copy wasn't installed by a Getman installer, so it doesn't replace itself. | It was copied by hand. Install it with a desktop installer to get updates |

Otherwise:

- **Check for updates automatically** may be off. **Check now** looks
  when you ask, whatever that setting.
- `Could not look for a new version: ...` The app could not read
  `https://github.com/fzn0x/getman/releases/latest/download/latest.json`.
  It goes through your system's proxy settings. The daily check tries
  again an hour later.
- `Could not download Getman <version>: ...` or
  `Could not install Getman <version>: ...` Try **Check now** later, or
  download the installer from the
  [releases page](https://github.com/fzn0x/getman/releases) and run it.
- A pre-release is offered only to a copy that is a pre-release too.

[Updates](desktop.md#updates) explains how the app updates.

## The browser UI

### The port is in use

Without `--port`, Getman tries port 7878 and the next 19, then any free
port, and prints the address it took. So this error comes only when you
ask for a port:

```text
error: cannot listen on port 7878: Only one usage of each socket address (protocol/network address/port) is normally permitted. (os error 10048)
```

On Linux and macOS the reason reads `Address already in use`. Choose
another port, leave out `--port`, or find the program that holds it,
which is often another `getman`:

```sh
lsof -i :7878
```

```powershell
Get-NetTCPConnection -LocalPort 7878 | Select-Object OwningProcess
```

`getman mock` does the same from port 7979. The desktop app asks for the
port it used last, and takes any free one when that is taken, so it never
fails this way.

The UI keeps its theme, layout and open tabs in the browser, for each
address. On another port, or on `localhost` instead of `127.0.0.1`, it
starts without them, so use the same `--port` to keep them.

### The browser does not open

Getman prints the link to the UI in the terminal as it starts, such as
`Open http://127.0.0.1:7878/#token=... in your browser.` Copy the whole
link, with its `#token=` part, into a browser on the same computer.
`--no-open` prints the link and opens nothing.

### Not signed in

The page shows **Error 401** and **Not signed in**: it has no key to
your Getman. Open the link Getman printed in your terminal when it
started; it ends in `#token=`. This happens when you type the address
yourself, or open it on another port or host name than the link's.

The token is saved in your configuration folder, so a bookmarked page
keeps working after a restart. If that file is deleted, Getman makes a
new token, and old pages need the new link. See
[What stays behind](installation.md#what-stays-behind) for where it is,
and [The local server](security.md#the-local-server) for why it exists.

### Not connected

The page says **Not connected** and "Getman is not answering. Is
`getman` still running in your terminal?". The server stopped: you
pressed <kbd>Ctrl</kbd> <kbd>C</kbd>, or closed its terminal. Start
`getman` again and choose **Try again**. If it started on another port,
open the new link it prints.

### The UI on another address or machine

The server listens on `127.0.0.1` only. It answers requests addressed to
`127.0.0.1` or `localhost` on its own port, and refuses others with
"Getman only answers requests addressed to 127.0.0.1 or localhost". This
guards it against other web pages, and it cannot be turned off.

- Open `http://127.0.0.1:<port>/`, not your computer's name or IP
  address.
- **Over SSH**, forward the same port number on both ends, then open the
  link Getman prints on the server in your local browser:

  ```sh
  ssh -L 7878:127.0.0.1:7878 server   # on your computer
  getman --no-open --port 7878        # then on the server
  ```

  Or use the [terminal UI](terminal.md), `getman tui`, in the SSH session.
- **In a container**, the server cannot be reached from outside it. Use
  the terminal UI or the command line inside it.
- A reverse proxy or tunnel that changes the `Host` header is refused.

"Getman does not answer requests from other web pages" means a page from
another site tried to call it. Nothing needs fixing.

### This build has no browser UI

`getman` warns `this build has no browser UI; build it with npm run build
in web/, then rebuild getman`. It was built from source before the UI
was. Run `npm ci` and `npm run build` in `web/`, then build `getman`
again; see [Building from source](installation.md#building-from-source).

## Workspaces

### No workspace here

```text
error: no workspace here: /home/me/src has no getman.yaml, nor does any directory above it. Create one with `getman init`.
```

`run`, `send` and the other commands work on the workspace around the
current directory: the nearest folder above it with a `getman.yaml`.
Change to a folder inside the workspace, point at it with `-w DIR`, or
create one with `getman init`. Plain `getman` opens your personal
workspace instead of failing.

### Getman opened an empty workspace

`getman DIR`, `getman desktop DIR` and **Open a workspace folder** create
a workspace in a folder that has none, and say `Created a workspace in
<folder>`. A typing mistake in the path makes a new, empty one. Delete
that folder and open the right one.

### A file cannot be read

```text
error: api/orders/create.yaml: found unexpected end of stream at line 5 column 1, while scanning a quoted scalar at line 3 column 6
```

The file is not valid YAML, often after a merge left conflict markers in
it. Fix the line the message names. [Workspaces](workspaces.md) describes
each file.

### Files cannot be saved

```text
error: /home/me/api/.getman.yaml.tmp: Permission denied (os error 13)
```

On Windows the reason reads `Access is denied. (os error 5)`. Getman
writes a file by writing a temporary one beside it, then renaming it, so
it needs write access to the folder as well as to the file. Check who
owns the workspace folder, for example one created with `sudo`, and that
it is not on a read-only drive or share.

### There is no environment

```text
error: there is no environment "staging"; choose one of dev, prod
```

`-e` takes an environment's file name or its display name. The message
lists the ones the workspace has. The `GETMAN_ENVIRONMENT` environment
variable chooses one too, so check that it is not left over in your
shell.

## Requests

### Could not connect

- `Could not connect: ... tcp connect error: ...` Nothing answered at
  that address. Check that the server runs, and the host and port in the
  URL and the environment.
- `Could not connect: ... dns error: ...` The host name could not be
  looked up. Check its spelling, and your VPN or DNS.

### Certificate errors

Getman checks server certificates against your operating system's trust
store. A failed check reads like this:

```text
error: Could not connect: client error (Connect): invalid peer certificate: UnknownIssuer
```

| The message ends in | Means | Fix |
| --- | --- | --- |
| `UnknownIssuer` | An authority your system does not trust signed it: a self-signed certificate, a company authority, or a proxy that inspects HTTPS | Add the authority's certificate under **Trusted CA certificates** in **Settings**, which is `ca_certs` in the file, or install it in your system's trust store |
| `Expired` | The certificate is out of date | Renew it, or for a test server turn off **Check TLS certificates** |
| `NotValidForName` | The URL's host is not a name the certificate was made for | Use a host name the certificate lists, not an IP address or another name |

Turning off **Check TLS certificates** is `verify_tls: false` in the
file; there is no command-line flag for it. See [TLS](http.md#tls), and
[Client certificates](http.md#client-certificates) for servers that ask
for one.

### Proxies

Requests go through your system's proxy and the `HTTPS_PROXY`,
`HTTP_PROXY`, `ALL_PROXY` and `NO_PROXY` environment variables, unless a
file says otherwise.

| Message | What to do |
| --- | --- |
| `Could not connect: ... tunnel error: failed to create underlying connection: ...` | The proxy cannot be reached. Check its address, or send directly with `--proxy off` or `proxy: off` |
| `the proxy URL starts with ftp://; use http://, https://, socks5:// or socks5h://` | Write the proxy with one of those schemes |
| `the proxy "..." is not a valid URL (...); write it as http://host:port` | Fix the proxy's address |
| `{{proxyPassword}} is not defined` | Define the variable, in a git-ignored `*.local.yaml` file |
| `HTTP/3 cannot go through a proxy; turn the proxy off or choose another HTTP version` | HTTP/3 needs a direct connection |

`--proxy system` on `send` and `run` goes back to the system's proxy for
one command. See [Proxies](http.md#proxies).

### The request timed out

A request gets 30 seconds by default. Raise the timeout in the request's
**Settings**, or `timeout_ms` in its file or its folder's; `0` waits
forever. Check too that the server answers at all.

### A variable is not defined

```text
warning: {{baseUrl}} is not defined
error: The request is invalid: Parsed Url is not a valid Uri
```

The request uses a variable that no scope sets, so its URL is not a URL.
Choose the environment that defines it, with `-e` or the environment
picker in the UI, or define it. `{{$env.NAME}} is not set in the
environment` means the shell that started Getman lacks that variable.
See [Variables and environments](variables.md).

### A script ran too long

`the script ran longer than 10 s and was stopped`: a script is stopped
after 10 seconds. Look for a loop that never ends. For a script that is
slow on purpose, `send` and `run` take `--script-timeout SECONDS`.

## Reporting a bug

If none of this helps, open a
[bug report](https://github.com/fzn0x/getman/issues/new?template=bug_report.yml)
on GitHub. Include:

- what happened, and what you expected;
- the Getman version, from `getman --version` or **Settings › About** in
  the desktop app;
- your operating system and its version, and how you installed Getman;
- the smallest request file that shows the problem, with secrets
  removed, and whether you sent it from the browser UI, the terminal UI
  or `getman run`;
- the exact error, copied as text.

Report security problems privately instead, as
[SECURITY.md](https://github.com/fzn0x/getman/blob/main/SECURITY.md)
describes.
