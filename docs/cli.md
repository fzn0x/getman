# Command line

```text
getman [DIR]                   open the browser UI (the default)
getman tui [DIR]               open the terminal UI
getman run [TARGET]...         run folders, requests or the workspace, for CI
getman send REQUEST            send one request, print the body, or
                               connect to a WebSocket, SSE or Socket.IO one
getman init [DIR]              create a workspace
getman import FILE|DIR         import collections, specs, HAR files or cURL
getman export [ITEM] --to X    a request as code, a folder as Postman or OpenAPI
getman mock [FOLDER]           serve saved examples as a mock API
getman mcp [DIR]               let AI agents use the workspace
```

Every command finds the workspace by looking for `getman.yaml` in the
current directory and its parents. `-w DIR` points elsewhere. `getman
help COMMAND` shows every option.

## getman / getman open

Starts the local server and opens the UI in your browser.

| Option | |
| --- | --- |
| `DIR` | The workspace. Without one, the workspace around the current directory, or your personal workspace |
| `-p`, `--port PORT` | Port to listen on. Default 7878, or the next free one |
| `--no-open` | Print the address instead of opening a browser |

The server listens on 127.0.0.1 only and stops with <kbd>Ctrl</kbd>
<kbd>C</kbd>. See [Security](security.md) for how it guards against other
web pages.

Your **personal workspace** is where Getman opens outside of any
workspace, so there is always somewhere to try a request. It starts with
the example requests, and lives in your user data folder:

| | |
| --- | --- |
| Linux | `~/.local/share/getman/workspace` |
| macOS | `~/Library/Application Support/getman/workspace` |
| Windows | `%APPDATA%\getman\workspace` |

## getman desktop

Opens the [desktop app](desktop.md)'s window, on the workspace in `DIR` or
the one used last. It exists in the `getman` that comes with the desktop
app on macOS and Linux, where the app's program is the `getman` command;
the command-line download has no window to open.

```sh
getman desktop            # the workspace used last
getman desktop ./api      # this one
```

## getman tui

Opens the [terminal UI](terminal.md). Takes the same `DIR`.

## getman run

Runs requests and reports the results. See [Running in CI](runner.md).

```sh
getman run [TARGET]... [-e ENV] [--var NAME=VALUE]... [-d DATA] [-n N]
           [--delay MS] [--bail] [-r REPORTER[=PATH]]... [-v] [--persist]
           [--http-version VERSION] [--proxy URL]
           [--save]
```

Each `TARGET` is a folder or a request; several run in the order given,
and none runs the whole workspace. WebSocket, event stream and Socket.IO
requests are connected to, sent their saved messages and left at their
`until` (10 messages or 5 seconds when they have none), and their scripts
and checks count like any other's; see
[WebSocket, SSE and Socket.IO](realtime.md#stopping-a-run). Reporters are `cli`, `json`, `junit`,
`html`, `csv` and `xlsx`. `--save` keeps the run in `.getman/runs` for
the UI's Runner.

## getman send

Sends one request and prints the response body. On a WebSocket, event
stream or Socket.IO request, it connects instead and prints each message
it receives on a line, running the request's scripts and checks around the
connection; see [WebSocket, SSE and Socket.IO](realtime.md).

| Option | |
| --- | --- |
| `REQUEST` | A path in the workspace, such as `orders/create.yaml`, or a path on disk |
| `-e`, `--env NAME` | Environment to use. `GETMAN_ENVIRONMENT` sets it too |
| `--var NAME=VALUE` | Set a variable. Repeat for more |
| `-i`, `--include` | Print the status line and headers first, and the timing and TLS details after; for a connection, the handshake's answer |
| `--http-version VERSION` | `auto`, `http1`, `http2` or `http3`, over the files' settings |
| `--proxy URL` | A proxy URL, `system` or `off`, over the files' settings |
| `--json` | Print everything as JSON: request, response, tests, checks, logs. For a connection, every event as one line of JSON |
| `-m`, `--message NAME` | WebSocket and Socket.IO: a saved message to send once connected. Repeat to send several, in order |
| `--until N\|TIME` | Connections: stop after `N` received messages, or after a time such as `30s`, `500ms` or `2m`, counted from when the connection opens. Repeat for both; the first reached stops. Without it, the request's `until` applies, and without that <kbd>Ctrl</kbd> <kbd>C</kbd> stops |
| `--script-timeout SECONDS` | Default 10 |

For a [gRPC request](grpc.md), `send` prints each response message as
pretty JSON as it arrives, and the status, its message and any error
details on stderr. `--json` prints one line of JSON per message instead,
for piping into tools such as `jq`; `-i` adds the response headers before
the messages and the trailers after them. A status other than `0 OK`
exits with 1.

## getman init

Creates `getman.yaml` and a `.gitignore` for local files.

| Option | |
| --- | --- |
| `DIR` | Where. Default: the current directory |
| `--name NAME` | Workspace name. Default: the directory's name |
| `--example` | Add example requests against a public test API, and two environments |

## getman import

Imports Postman, Insomnia, Hoppscotch and Bruno collections and
environments, OpenAPI and Swagger documents, HAR files and cURL
commands. The format is told from the content. See
[Import and export](import-export.md).

| Option | |
| --- | --- |
| `FILE` | The file, a folder (such as a Bruno collection), or `-` for standard input |
| `--curl COMMAND` | A cURL command, instead of a file |
| `--folder FOLDER` | Folder of the workspace to import into |
| `--root` | Make a collection the workspace itself |

## getman export

Prints a request as code, with variables replaced. With `--to postman`
or `--to openapi`, exports a folder, the workspace, an environment or
the globals instead.

| Option | |
| --- | --- |
| `ITEM` | The request, or for `postman` and `openapi` the folder; the whole workspace when left out |
| `-t`, `--to FORMAT` | A language (see [Export as code](import-export.md#export-as-code)), `postman` or `openapi`. gRPC requests print as a `grpcurl` command, also with `--to grpcurl` |
| `-o`, `--output FILE` | Write to a file instead of standard output; `.yaml` gives YAML for `openapi` |
| `--keep-vars` | Leave `{{variables}}` as written in code, so tokens stay out of it |
| `--indent 2\|4\|tab` | Indent code with two spaces, four spaces or tabs, instead of the language's own habit |
| `--no-boilerplate` | Leave out the imports and the `main` function, where the language has them (see [Indent and boilerplate](import-export.md#indent-and-boilerplate)) |
| `-e`, `--env NAME` | Environment to fill in variables from; with `--to postman` and no `ITEM`, the environment to export |
| `--globals` | With `--to postman`, export the globals |
| `--with-secrets` | With `--to postman`, include values from git-ignored local files |
| `--var NAME=VALUE` | As for `send` |

## getman mock

Serves the saved examples of the workspace, or of a folder, as a mock API
on 127.0.0.1, and prints each request it answers. See
[Examples and mocks](examples-and-mocks.md).

| Option | |
| --- | --- |
| `FOLDER` | A folder of the workspace, or a directory on disk. Without one, every example in the workspace |
| `-p`, `--port PORT` | Port to listen on. Default 7979, or the next free one |
| `--delay MS` | Wait before each answer, to try slow responses |
| `-w`, `--workspace DIR` | The workspace |

It runs until <kbd>Ctrl</kbd> <kbd>C</kbd>.

## getman mcp

Serves the workspace to AI agents over the Model Context Protocol, on
standard input and output. Your agent's app starts it; see
[AI agents](mcp.md) for the setup.

| Option | |
| --- | --- |
| `DIR` | The workspace. Default: the one around the current directory |

## Exit status

| | |
| --- | --- |
| 0 | Success |
| 1 | `run` or `send`: a request failed |
| 2 | The command could not do its work: bad arguments, no workspace, a file that cannot be read |

## Colour

Output is coloured on a terminal and plain when piped. `NO_COLOR=1`
turns colour off, and `CLICOLOR_FORCE=1` keeps it on in CI logs.
