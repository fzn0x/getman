# Command line

```text
getman [DIR]                   open the browser UI (the default)
getman tui [DIR]               open the terminal UI
getman run [TARGET]            run a folder or the workspace, for CI
getman send REQUEST            send one request, print the body
getman init [DIR]              create a workspace
getman import FILE | --curl    import from Postman or cURL
getman export REQUEST --to X   print a request as code
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
getman run [TARGET] [-e ENV] [--var NAME=VALUE]... [-d DATA] [-n N]
           [--delay MS] [--bail] [-r REPORTER[=PATH]]... [-v] [--persist]
```

## getman send

Sends one request and prints the response body.

| Option | |
| --- | --- |
| `REQUEST` | A path in the workspace, such as `orders/create.yaml`, or a path on disk |
| `-e`, `--env NAME` | Environment to use. `GETMAN_ENVIRONMENT` sets it too |
| `--var NAME=VALUE` | Set a variable. Repeat for more |
| `-i`, `--include` | Print the status line and headers first |
| `--json` | Print everything as JSON: request, response, tests, checks, logs |
| `--script-timeout SECONDS` | Default 10 |

## getman init

Creates `getman.yaml` and a `.gitignore` for local files.

| Option | |
| --- | --- |
| `DIR` | Where. Default: the current directory |
| `--name NAME` | Workspace name. Default: the directory's name |
| `--example` | Add example requests against a public test API, and two environments |

## getman import

Imports a Postman collection, environment or globals file, or a cURL
command. See [Import and export](import-export.md).

| Option | |
| --- | --- |
| `FILE` | The file, or `-` for standard input |
| `--curl COMMAND` | A cURL command, instead of a file |
| `--folder FOLDER` | Folder of the workspace to import into |
| `--root` | Make a collection the workspace itself |

## getman export

Prints a request as code, with variables replaced.

| Option | |
| --- | --- |
| `REQUEST` | The request |
| `-t`, `--to LANGUAGE` | `curl` (default), `httpie`, `powershell`, `fetch`, `python`, `go`, `rust` or `java` |
| `-e`, `--env NAME`, `--var NAME=VALUE` | As for `send` |

## Exit status

| | |
| --- | --- |
| 0 | Success |
| 1 | `run` or `send`: a request failed |
| 2 | The command could not do its work: bad arguments, no workspace, a file that cannot be read |

## Colour

Output is coloured on a terminal and plain when piped. `NO_COLOR=1`
turns colour off, and `CLICOLOR_FORCE=1` keeps it on in CI logs.
