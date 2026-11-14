# Settings

Every setting Getman has and where it is kept: the UI's preferences,
request settings in the workspace files, the desktop app's choices,
environment variables and command-line options.

| What | Where it is kept | Shared through git |
| --- | --- | --- |
| [UI preferences](#ui-preferences), such as the theme | This browser's storage, for the UI's address | No |
| [Scripts on or off](#the-settings-dialog), local values, header presets | Git-ignored files in the workspace | No |
| [Request settings](#request-settings), such as timeouts and proxies | `settings:` in request, folder and workspace files | Yes |
| [Desktop app settings](#desktop-app-settings), such as updates | The app's configuration folder | No |
| [Environment variables](#environment-variables) | Your shell or CI | No |
| [Command-line options](#command-line-options) | The command you type | No |

## UI preferences

The browser UI keeps its preferences in the browser's local storage, which
belongs to the address the UI was opened on: `127.0.0.1` and its port.
Another browser, or the UI on another port, starts with the defaults. To
keep them, open the UI on the same port each time, such as with
`getman --port 7878`. The desktop app asks for the port it used last for
the same reason; see [Desktop app settings](#desktop-app-settings).

| Preference | Default | Change it with | Storage key |
| --- | --- | --- | --- |
| Theme | Follows the system's light or dark mode | The contrast button in the top bar, or **Switch light and dark theme** in the command palette | `getman.theme` |
| Response position | Beside the request | The layout button in the top bar, or **Put the response below the request** in the palette | `getman.layout` |
| Sidebar shown | Shown. On a window 900 pixels wide or less, it starts hidden | <kbd>Ctrl</kbd> <kbd>B</kbd>, or the sidebar button | `getman.sidebarOpen` |
| Sidebar width | 280 pixels, from 200 to 520 | Drag its edge, or focus the edge and press <kbd>←</kbd> <kbd>→</kbd> | `getman.sidebarWidth` |
| Request and response split | Half each, from 20% to 80% | Drag the divider, or focus it and press the arrow keys | `getman.split` |
| Console height | 260 pixels, from 120 to 800 | Drag its top edge, or focus it and press <kbd>↑</kbd> <kbd>↓</kbd> | `getman.consoleHeight` |
| Code language | cURL | **Copy as code** (<kbd>Alt</kbd> <kbd>C</kbd>) | `getman.codeLanguage` |
| Code indent | Each language's own | **Indent** in **Copy as code** | `getman.codeIndent` |
| Imports and `main` in code | On | **Imports and main** in **Copy as code** | `getman.codeBoilerplate` |

Once you switch the theme, the UI keeps light or dark and stops following
the system. To follow the system again, remove `getman.theme` from the
page's local storage, such as in the browser's developer tools.

**Keep `{{variables}}`** in **Copy as code** is not kept: it is off each
time the dialog opens. See [Export as code](import-export.md#export-as-code)
for what the code options do.

### Per workspace

These are kept for each workspace apart, in the same storage, under keys
that end with `:` and the workspace's folder.

| Preference | Default | Storage key |
| --- | --- | --- |
| Open tabs, with changes not yet saved | None | `getman.tabs:<folder>` |
| Folders opened in the sidebar | All closed | `getman.expanded:<folder>` |
| **Sort A to Z** in the sidebar | Off: each folder's own order | `getman.sortTree:<folder>` |
| The environment selected | No environment | `getman.environment:<folder>` |

See [Tabs](browser-ui.md#tabs) and [Sidebar](browser-ui.md#sidebar). The
storage also holds `getman.token`, the key the page uses to reach its
server; see [The local server](security.md#the-local-server).

### The Settings dialog

The gear at the top right opens **Settings** in every window. In the
desktop app, **Settings** in the command palette opens it too.

| Setting | Default | What it does | Kept in |
| --- | --- | --- | --- |
| **Run scripts** | On | Off sends requests without their pre-request and post-response scripts, for a workspace you have not read yet. Checks and captures still run | `scripts_off: true` in the workspace's `getman.local.yaml`, on this machine only |
| **Clear session variables** | | Clears the values scripts keep with `gm.session` until Getman stops | Nothing; it is a button |
| **About** | | The version and the update choices. Only in the desktop app | See [Desktop app settings](#desktop-app-settings) |

See [Turning scripts off](scripts.md#turning-scripts-off).

### Kept with the workspace on this machine

Some choices belong to one workspace on one machine, so Getman keeps them
in git-ignored files inside the workspace rather than in the browser.

| File | Holds |
| --- | --- |
| `getman.local.yaml` | Your overrides of workspace variables, the globals, and `scripts_off` |
| `environments/<name>.local.yaml` | Your values of an environment: secrets, tokens, your own base URL |
| `.getman/header-presets.yaml` | The header presets you saved; see [Header presets](browser-ui.md#header-presets) |

See [Values that stay on your machine](variables.md#values-that-stay-on-your-machine).
The [workspace file format](workspace-format.md) lists every key of these
files.

## Request settings

How a request goes on the wire is set under `settings:` in a request
file, a folder's `_folder.yaml` or the workspace's `getman.yaml`. In the
UI, each has a **Settings** tab: a request's, a folder's, and the
workspace's under **Workspace settings**.

```yaml
settings:
  timeout_ms: 10000
  verify_tls: false
  proxy: http://proxy.internal:3128
```

A request uses its own value of each setting. Where it sets none, it uses
the nearest folder's, then the folders above it in turn, then the
workspace's, then the default. Each setting is taken whole: a request's
`ca_certs` list replaces its folder's rather than adding to it. Settings
left unset are not written to the file.

| Setting | Default | Effect |
| --- | --- | --- |
| `timeout_ms` | 30000 | Give up after this many milliseconds. 0 waits forever |
| `follow_redirects` | true | Follow redirects. false returns the redirect itself |
| `max_redirects` | 10 | Stop with an error after more redirects than this |
| `follow_original_method` | false | true keeps a POST and its body across a 301 or 302, instead of turning it into a GET |
| `follow_authorization` | false | true keeps `Authorization` when a redirect leads to another host, port or scheme |
| `verify_tls` | true | false accepts self-signed and expired certificates |
| `http_version` | `auto` | `auto`, `http1`, `http2` or `http3` |
| `title_case_headers` | true | Send HTTP/1.1 header names as `Content-Type`; false sends `content-type` |
| `proxy` | `system` | `system`, `off`, a proxy URL, or a map with `url`, `username`, `password` and `bypass` |
| `client_cert` | None | A certificate to present: PEM files, or a PKCS#12 file and its passphrase |
| `ca_certs` | None | More certificate authorities to trust, as PEM files |
| `cookies` | true | false leaves the cookie jar alone: nothing is sent from it or stored in it |
| `max_response_mb` | 100 | Read at most this many MiB of a body. 0 reads everything |

[HTTP and connections](http.md) explains each one. The same settings
apply to gRPC calls, where the timeout is the call's deadline; see
[Deadlines and TLS](grpc.md#deadlines-and-tls). WebSocket and Socket.IO
connections follow `proxy` and `cookies`; see
[Proxies and cookies](realtime.md#proxies-and-cookies).

`getman run` and `getman send` take `--http-version` and `--proxy`, which
win over every file for that command.

### Connection settings

WebSocket, event stream and Socket.IO requests have settings of their
own, next to `settings:` in the request file.

| Setting | Default | Effect |
| --- | --- | --- |
| `websocket.subprotocols` | None | Subprotocols to offer, preferred first |
| `sse.reconnect` | false | Connect again when the stream ends, sending the last event's id |
| `socketio.namespace` | The URL's path, or `/` | The namespace to join |
| `socketio.path` | `/socket.io/` | The path the server answers on |
| `socketio.auth` | None | A JSON object the server reads as `socket.handshake.auth` |
| `socketio.transport` | `auto` | `auto`, `websocket` or `polling` |
| `socketio.events` | Every event | The events to show |
| `until.messages`, `until.time_ms` | 10 messages or 5 seconds | When `getman run` ends the connection. With only a count, it still ends after 60 seconds |

See [WebSocket, SSE and Socket.IO](realtime.md).

## Desktop app settings

The [desktop app](desktop.md) keeps its own settings in `settings.json`,
in its configuration folder. Only the values that differ from the
defaults are written, and a missing or broken file reads as the defaults.

| Key | Default | What it holds |
| --- | --- | --- |
| `workspace` | None: the personal workspace opens | The folder of the workspace opened last, which the app opens next time |
| `checkForUpdates` | true | **Check for updates automatically**: look for a new version once a day |
| `installWithoutAsking` | false | **Install updates without asking**. Only possible while checks are on |
| `lastCheck` | None | When the app last looked for an update, in seconds since 1970 |
| `port` | Any free port | The port of the app's server, asked for again so the UI keeps its storage. If it is taken, the app takes another, and the UI may start with its defaults |

Change the update choices in **Settings › About**. A folder given to
`getman desktop DIR` opens instead of the one used last. Copies that
never update themselves, such as the portable zip and the .deb and .rpm
packages, show why in **Settings › About** instead of the choices; see
[Updates](desktop.md#updates).

The window's size and place, and whether it is maximized or full screen,
are kept in `.window-state.json` in the same folder. The window first
opens at 1280 by 820 pixels, and cannot be made smaller than 640 by 480.

| System | Folder |
| --- | --- |
| Windows | `%APPDATA%\io.github.fzn0x.getman` |
| macOS | `~/Library/Application Support/io.github.fzn0x.getman` |
| Linux | `~/.config/io.github.fzn0x.getman` |

The [UI preferences](#ui-preferences) live in the window's web storage,
in another folder. See
[Where the app keeps its settings](desktop.md#where-the-app-keeps-its-settings).

## Environment variables

| Variable | Read by | Effect |
| --- | --- | --- |
| `GETMAN_ENVIRONMENT` | `getman run`, `send` and `export` | The environment to use, as `-e` names it. `-e` wins |
| `GETMAN_<NAME>` | Request, folder and environment files | Read as `{{$env.GETMAN_NAME}}`. No other environment variable can be read this way. See [Secrets from the environment](variables.md#secrets-from-the-environment) |
| `GETMAN_VERSION`, `GETMAN_INSTALL_DIR` | The install scripts | The release to install, such as `v0.1.0`, and the folder to install it in. See [Install](getting-started.md#install) |
| `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, `NO_PROXY` | Requests, connections and gRPC calls | The proxy, while a request's `proxy` is `system`, the default. The lowercase names work too. See [Proxies](http.md#proxies) |
| `SSL_CERT_FILE`, `SSL_CERT_DIR` | TLS connections, on Linux | A PEM file, or folders of them separated by `:`, to trust instead of the system's certificate store. `ca_certs` still adds to them. Windows and macOS ask the system and ignore both |
| `NO_COLOR` | The command line | Set to anything, it turns colour off |
| `CLICOLOR` | The command line | `0` turns colour off |
| `CLICOLOR_FORCE` | The command line | Set to anything, it keeps colour on when the output is not a terminal, as in CI logs. `NO_COLOR` wins over it |
| `VISUAL`, then `EDITOR` | `getman tui` | The editor to open text in. It may carry arguments, such as `code --wait`. Without either, `vi`, or Notepad on Windows |
| `XDG_CONFIG_HOME`, `XDG_DATA_HOME` | Getman and the desktop app, on Linux | Move the configuration folder (the UI's token, the desktop app's settings) and the data folder (the personal workspace) from `~/.config` and `~/.local/share` |
| `WEBKIT_DISABLE_DMABUF_RENDERER` | The desktop app's window, on Linux | `1` works around a blank window with some graphics drivers. See [Troubleshooting](desktop.md#troubleshooting) |

`{{$env.GETMAN_...}}` is the only way requests read the environment, so a
workspace from someone else cannot read your other variables, such as
cloud credentials.

## Command-line options

These options act like settings for one command. [Command line](cli.md)
lists every option of every command.

| Option | Commands | Effect |
| --- | --- | --- |
| `DIR` | `getman`, `open`, `tui`, `desktop` | The workspace that holds this folder, or a new one created in it. Without it, the workspace around the current directory, or your personal workspace |
| `DIR` | `mcp` | The workspace that holds this folder. Without it, the workspace around the current directory |
| `-w`, `--workspace DIR` | `run`, `send`, `import`, `export`, `mock` | The workspace. Without it, the workspace around the current directory |
| `-p`, `--port PORT` | `getman`, `open` | The port of the UI. Without it, 7878, or the next free port up to 7897, or else any free port. A port you give must be free, or Getman stops with an error |
| `--no-open` | `getman`, `open` | Print the address instead of opening a browser |
| `-p`, `--port PORT` | `mock` | The port of the mock server. Default 7979, or the next free port |
| `-e`, `--env NAME` | `run`, `send`, `export` | The environment, by file name or display name. `GETMAN_ENVIRONMENT` sets it too |
| `--var NAME=VALUE` | `run`, `send`, `export` | Set a variable for this command, over every other value |
| `--http-version VERSION` | `run`, `send` | `auto`, `http1`, `http2` or `http3`, over every file's setting |
| `--proxy URL` | `run`, `send` | A proxy URL, `system` or `off`, over every file's setting |
| `--no-scripts` | `run`, `send` | Run no scripts, as **Run scripts** off does in the UI |
| `--script-timeout SECONDS` | `run`, `send` | How long a script may run. Default 10 |

There is no colour option; the [environment variables](#environment-variables)
above set it. See [Colour](cli.md#colour).
