# Command line

```text
getman [DIR]                   open the browser UI (the default)
getman open [DIR]              the same
getman desktop [DIR]           open the desktop app's window (macOS, Linux)
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
current directory and its parents. `run`, `send`, `import`, `export` and
`mock` take `-w DIR` (`--workspace DIR`) to point elsewhere; `open`, `tui`
and `mcp` take the directory as `DIR`. `getman help COMMAND` or `getman
COMMAND --help` shows every option, and `-V` (`--version`) prints the
version.

A request or folder on the command line, such as a `TARGET` or a
`REQUEST`, is a path in the workspace, such as `orders/create.yaml`, or a
path on disk inside the workspace.

## getman / getman open

Starts the local server and opens the UI in your browser.

| Option | |
| --- | --- |
| `DIR` | The workspace that holds this directory, or a new one created in it when there is none. Without `DIR`, the workspace around the current directory, or your personal workspace |
| `-p`, `--port PORT` | Port to listen on. Default 7878, or the next free one |
| `--no-open` | Print the address instead of opening a browser |

`getman` without a command takes the same options, so `getman --port
9000` and `getman open --port 9000` do the same.

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

Opens the [terminal UI](terminal.md). Takes the same `DIR`. Text you
edit in an external editor opens in `$VISUAL`, then `$EDITOR`, then `vi`
(Notepad on Windows).

## getman run

Runs requests and reports the results. See [Running in CI](runner.md).

```sh
getman run [TARGET]... [-w DIR] [-e ENV] [--var NAME=VALUE]... [-d DATA]
           [-n N] [--delay MS] [--bail] [-r REPORTER[=PATH]]... [-v]
           [--persist] [--save] [--http-version VERSION] [--proxy URL]
           [--script-timeout SECONDS] [--no-scripts]
```

| Option | |
| --- | --- |
| `TARGET` | A folder or a request. Several run in the order given; none runs the whole workspace |
| `-w`, `--workspace DIR` | The workspace |
| `-e`, `--env NAME` | Environment to use, by file name or display name. `GETMAN_ENVIRONMENT` sets it too |
| `--var NAME=VALUE` | Set a variable above every other scope. Repeat for more |
| `-d`, `--data FILE` | A CSV or JSON file with one row of variables per iteration |
| `-n`, `--iterations N` | How many times to run. Default: the number of data rows, or 1 |
| `--delay MS` | Milliseconds to wait between requests. Default 0 |
| `--bail` | Stop at the first failure |
| `-r`, `--reporter NAME[=PATH]` | `cli` (the default), `json`, `junit`, `html`, `csv` or `xlsx`. `=PATH` writes a file instead of standard output, and `xlsx` needs one. Repeat for several |
| `-v`, `--verbose` | Print every response's status line, headers and body, and script console output |
| `--persist` | Save the variables the run set to the git-ignored local files, as the UI does |
| `--save` | Keep the run in `.getman/runs` for the UI's Runner. The newest 20 are kept |
| `--http-version VERSION` | As for `send` |
| `--proxy URL` | As for `send` |
| `--script-timeout SECONDS` | Seconds a script may run before it is stopped. Default 10 |
| `--no-scripts` | Run no [scripts](scripts.md#turning-scripts-off); checks and captures still run, and each request notes how many scripts it skipped |

WebSocket, event stream and Socket.IO requests are connected to, sent
their saved messages and left at their `until` (10 messages or 5 seconds
when they have none), and their scripts and checks count like any
other's; see [WebSocket, SSE and Socket.IO](realtime.md#stopping-a-run).
<kbd>Ctrl</kbd> <kbd>C</kbd> stops the run after the current request,
still writes the reports, and exits with 1.

## getman send

Sends one request and prints the response body. On a WebSocket, event
stream or Socket.IO request, it connects instead and prints each message
it receives on a line, running the request's scripts and checks around the
connection; see [WebSocket, SSE and Socket.IO](realtime.md).

| Option | |
| --- | --- |
| `REQUEST` | A path in the workspace, such as `orders/create.yaml`, or a path on disk |
| `-w`, `--workspace DIR` | The workspace |
| `-e`, `--env NAME` | Environment to use, by file name or display name. `GETMAN_ENVIRONMENT` sets it too |
| `--var NAME=VALUE` | Set a variable above every other scope. Repeat for more |
| `-i`, `--include` | Print the status line and headers first, and the redirects followed, the timing and TLS details after; for a connection, the handshake's answer |
| `--http-version VERSION` | `auto`, `http1`, `http2` or `http3` (or `1`, `1.1`, `2`, `3`), over the files' settings |
| `--proxy URL` | A proxy URL such as `http://proxy:3128` or `socks5h://127.0.0.1:1080`, `system` or `off`, over the files' settings. See [Proxies](http.md#proxies) |
| `--json` | Print everything as JSON: request, response, tests, checks, logs, variable changes and warnings. For a connection, every event as one line of JSON |
| `-m`, `--message NAME` | WebSocket and Socket.IO: a saved message to send once connected, by name in any case. Repeat to send several, in order |
| `--until N\|TIME` | Connections: stop after `N` received messages, or after a time such as `30s`, `500ms` or `2m`, counted from when the connection opens. Repeat for both; the first reached stops. Without it, the request's `until` applies, and without that <kbd>Ctrl</kbd> <kbd>C</kbd> stops |
| `--script-timeout SECONDS` | Seconds a script may run before it is stopped. Default 10 |
| `--no-scripts` | Run no scripts; checks and captures still run, and the result notes how many scripts were skipped |

The body goes to standard output. Everything else, such as the status
summary, script logs, warnings and the results of tests and checks, goes
to standard error, so `getman send` fits in pipelines. On an HTTP
request, `--message` and `--until` are ignored with a warning. On a
connection, the first <kbd>Ctrl</kbd> <kbd>C</kbd> disconnects and then
runs the post-response script and checks; a second one exits at once
with status 130.

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

The `.gitignore` lists `*.local.yaml` and `.getman/`, the files that stay
on your machine. See [Workspaces](workspaces.md#what-to-commit). `DIR`
is created when it does not exist. A `getman.yaml` already there stops
the command, and a `.gitignore` already there is left as it is, so add
those two lines to it yourself.

## getman import

Imports Postman, Insomnia, Hoppscotch and Bruno collections and
environments, OpenAPI and Swagger documents, HAR files and cURL
commands. The format is told from the content. See
[Import and export](import-export.md).

| Option | |
| --- | --- |
| `FILE` | The file, a folder (such as a Bruno collection), or `-` for standard input |
| `--curl COMMAND` | A cURL command, instead of a file |
| `-w`, `--workspace DIR` | The workspace |
| `--folder FOLDER` | Folder of the workspace to import into. Default: the top of the workspace |
| `--root` | Make a collection the workspace itself instead of a folder in it |

A folder is read with every file in it and its subfolders, except hidden
ones, `node_modules`, binary files and files over 64 MB, and holds at
most 5,000 files. The command prints what each import created and what
it could not carry over.

## getman export

Prints a request as code, with variables replaced. With `--to postman`
or `--to openapi`, exports a folder, the workspace, an environment or
the globals instead.

| Option | |
| --- | --- |
| `ITEM` | The request, or for `postman` and `openapi` the folder; the whole workspace when left out |
| `-t`, `--to FORMAT` | A language (see [Export as code](import-export.md#export-as-code)), `postman` or `openapi`. Default `curl`. gRPC requests print as a `grpcurl` command, with `curl` or `grpcurl` |
| `-o`, `--output FILE` | Write to a file instead of standard output; `.yaml` or `.yml` gives YAML for `openapi` |
| `--keep-vars` | Leave `{{variables}}` as written in code, so tokens stay out of it |
| `--indent 2\|4\|tab` | Indent code with two spaces, four spaces or tabs, instead of the language's own habit |
| `--no-boilerplate` | Leave out the imports and the `main` function, where the language has them (see [Indent and boilerplate](import-export.md#indent-and-boilerplate)) |
| `-w`, `--workspace DIR` | The workspace |
| `-e`, `--env NAME` | Environment to fill in variables from; with `--to postman` and no `ITEM`, the environment to export. `GETMAN_ENVIRONMENT` sets it too |
| `--globals` | With `--to postman`, export the globals. Takes no `ITEM` |
| `--with-secrets` | With `--to postman`, include values from git-ignored local files |
| `--var NAME=VALUE` | As for `send` |
| `--script-timeout SECONDS` | Accepted as for `send`; export runs no scripts |

WebSocket, event stream and Socket.IO requests are not written as code;
connect to them with `getman send`. Because `GETMAN_ENVIRONMENT` counts as
`-e`, `getman export --to postman` without an `ITEM` exports that
environment, not the workspace, while the variable is set.

## getman mock

Serves the saved examples of the workspace, or of a folder, as a mock API
on 127.0.0.1, and prints each request it answers. See
[Examples and mocks](examples-and-mocks.md).

| Option | |
| --- | --- |
| `FOLDER` | A folder of the workspace, or a directory on disk, which then names its own workspace. Without one, every example in the workspace |
| `-p`, `--port PORT` | Port to listen on. Default 7979, or the next free one |
| `--delay MS` | Milliseconds to wait before each answer, to try slow responses. Default 0 |
| `-w`, `--workspace DIR` | The workspace |

It lists the examples it serves, picks up examples saved while it runs,
and runs until <kbd>Ctrl</kbd> <kbd>C</kbd>.

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
| 1 | `run`: a request failed, or the run stopped early. `send`: the request could not be sent, a script failed, a test or check failed, or a gRPC call ended with a status other than `0 OK`. `import`: some files of a folder could not be imported |
| 2 | The command could not do its work: bad arguments, no workspace, an unknown environment or target, a file that cannot be read or written |
| 130 | `send` on a connection: a second <kbd>Ctrl</kbd> <kbd>C</kbd> |

## Environment variables

| Variable | |
| --- | --- |
| `GETMAN_ENVIRONMENT` | The environment for `run`, `send` and `export`, as `-e` gives it. `-e` wins |
| `GETMAN_<NAME>` | Read in requests as `{{$env.GETMAN_NAME}}`; no other environment variable can be read. See [Variables](variables.md#secrets-from-the-environment) |
| `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, `NO_PROXY` | The proxy, while a request's proxy setting is the default, `system`. See [Proxies](http.md#proxies) |
| `NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE` | Colour; see below |
| `VISUAL`, `EDITOR` | The editor `getman tui` opens text in |

## Colour

Output is coloured on a terminal and plain when piped. `NO_COLOR=1` or
`CLICOLOR=0` turns colour off, and `CLICOLOR_FORCE=1` keeps it on in CI
logs. `NO_COLOR` wins over `CLICOLOR_FORCE`.
