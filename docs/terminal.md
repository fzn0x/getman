# Terminal UI

`getman tui` opens the workspace in the terminal: a tree of requests, the
open request and its response, side by side. It needs no browser and no
open port, so it works over SSH, in a container and in a tmux pane.

```sh
getman tui            # the workspace around the current directory
getman tui ./api      # another one
```

It reads and writes the same files as the browser UI. Simple changes,
such as the URL and method, are made in place. Everything else (headers,
the body, authentication, checks) opens in your own editor:
`$VISUAL`, then `$EDITOR`, then `vi` (Notepad on Windows). Save and close
the editor, and the change appears, ready to send and save.

The terminal UI works with HTTP requests. WebSocket, event stream and
Socket.IO requests show in the tree as `WS`, `SSE` and `SIO`, and gRPC
requests as `GRPC`, but it does not connect to them or open them yet:
use `getman send` or the browser UI. See
[WebSocket, SSE and Socket.IO](realtime.md) and [gRPC](grpc.md). Runs
started from it include gRPC requests.

## Keys

Press <kbd>?</kbd> in the terminal UI to see these.

### Anywhere

| Keys | Does |
| --- | --- |
| <kbd>Tab</kbd> / <kbd>Shift</kbd> <kbd>Tab</kbd> | Move between the tree, the request and the response |
| <kbd>s</kbd> | Send the open request |
| <kbd>w</kbd> | Save it |
| <kbd>e</kbd> / <kbd>E</kbd> | Next or previous environment |
| <kbd>/</kbd> | Filter requests by name or path |
| <kbd>r</kbd> | Run the selected folder, or everything |
| <kbd>R</kbd> | Reload the tree from disk |
| <kbd>q</kbd> | Quit (twice, if the request has unsaved changes) |

### Tree

| Keys | Does |
| --- | --- |
| <kbd>j</kbd> <kbd>k</kbd> or arrows | Move |
| <kbd>g</kbd> <kbd>G</kbd> | First, last |
| <kbd>Enter</kbd> or <kbd>l</kbd> | Open a request, expand a folder |
| <kbd>h</kbd> | Collapse |
| <kbd>a</kbd> | Add a request in the selected folder |
| <kbd>d</kbd> or <kbd>Del</kbd> | Move the selected request or folder to the trash, after <kbd>y</kbd> confirms |
| <kbd>u</kbd> | Put back what you moved to the trash last |

Deleted items wait in `.getman/trash/`, which git ignores; the browser
UI's Trash lists them, puts them back and empties it.

### Request

| Keys | Does |
| --- | --- |
| <kbd>i</kbd> or <kbd>Enter</kbd> | Edit the URL; <kbd>Enter</kbd> sends, <kbd>Esc</kbd> stops editing |
| <kbd>m</kbd> / <kbd>M</kbd> | Next or previous method |
| <kbd>1</kbd>–<kbd>5</kbd>, <kbd>[</kbd> <kbd>]</kbd> | Params, headers, body, auth, checks |
| <kbd>E</kbd> or <kbd>o</kbd> | Edit that part in your editor |

### Response

| Keys | Does |
| --- | --- |
| <kbd>j</kbd> <kbd>k</kbd>, <kbd>PgUp</kbd> <kbd>PgDn</kbd>, <kbd>g</kbd> <kbd>G</kbd> | Scroll |
| <kbd>1</kbd>–<kbd>4</kbd>, <kbd>[</kbd> <kbd>]</kbd> | Body, headers, tests, console |
