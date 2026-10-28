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

HTTP, WebSocket, event stream, Socket.IO and gRPC requests all open
here. The tree marks the last four `WS`, `SSE`, `SIO` and `GRPC`. See
[Connections and calls](#connections-and-calls) for them and
[Signing in with a browser](#signing-in-with-a-browser) for OAuth 2.0.
Runs started from it include gRPC requests, and connect to WebSocket, event
stream and Socket.IO requests as `getman run` does.

Press <kbd>Esc</kbd> or <kbd>x</kbd> to stop whatever is running: a
request being sent, a connection, a call, a sign-in. A request stopped
this way shows "The request was cancelled." and can be sent again.

## Keys

Press <kbd>?</kbd> in the terminal UI to see these.

### Anywhere

| Keys | Does |
| --- | --- |
| <kbd>Tab</kbd> / <kbd>Shift</kbd> <kbd>Tab</kbd> | Move between the tree, the request and the response |
| <kbd>s</kbd> | Send the open request, or connect or call |
| <kbd>Esc</kbd> or <kbd>x</kbd> | Stop what is running: a request, a connection, a call or a sign-in |
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
| <kbd>b</kbd> | On the auth pane, sign in to OAuth 2.0 with your browser |

### Response

| Keys | Does |
| --- | --- |
| <kbd>j</kbd> <kbd>k</kbd>, <kbd>PgUp</kbd> <kbd>PgDn</kbd>, <kbd>g</kbd> <kbd>G</kbd> | Scroll |
| <kbd>1</kbd>–<kbd>4</kbd>, <kbd>[</kbd> <kbd>]</kbd> | Body, headers, tests, console |

### WebSocket, Socket.IO and gRPC

These work on any of these requests once it is open, wherever the focus is.

| Keys | Does |
| --- | --- |
| <kbd>m</kbd> | WebSocket and Socket.IO: choose a saved message to send. gRPC: send the request's message |
| <kbd>t</kbd> | Type a message to send |
| <kbd>f</kbd> | End the stream a gRPC call sends |
| <kbd>x</kbd> | Disconnect, or cancel the call |

## Connections and calls

<kbd>s</kbd> connects a WebSocket, event stream or Socket.IO request and
calls a gRPC method. The response becomes a log that scrolls as messages
come in: the time since the connection began, an arrow for the direction
(`→` sent, `←` received), and the message, JSON pretty-printed and
coloured. It follows the end of the log until you scroll up
(<kbd>k</kbd>, <kbd>PgUp</kbd>, <kbd>g</kbd>); <kbd>G</kbd> returns to
the end. The tabs hold the messages, the headers the server answered with,
and, for gRPC, the results of checks and the console.

The title says where the connection is: connecting, connected with the
status the server answered, how many messages went each way, and, once it
is over, how it ended. The log keeps the last 2000 entries.

Opening another request disconnects the one you leave. Opening the same
request again keeps its connection.

**WebSocket and Socket.IO.** <kbd>m</kbd> lists the messages saved with
the request; choose one with <kbd>j</kbd> <kbd>k</kbd> and
<kbd>Enter</kbd>, or press its number. <kbd>t</kbd> opens a line to type
on: a WebSocket message is sent as you write it, a Socket.IO event as its
name and then its arguments as JSON, such as `chat ["hi"]`.
<kbd>Enter</kbd> sends, <kbd>Esc</kbd> closes the line. You can send before
the connection has opened; the message waits for it. <kbd>x</kbd>
disconnects.

**Event streams.** They only receive, so there is nothing to send;
<kbd>x</kbd> disconnects.

**gRPC.** The Body pane shows the service, the method, the schema and the
message; <kbd>E</kbd> edits the message in your editor. Unary and server
streaming calls send the message when they start, and each reply appears
in the log. A call that streams from the client (client and bidirectional
streams) starts without sending anything, as in the browser UI:
<kbd>m</kbd> sends the request's saved message, <kbd>t</kbd> types
another, and <kbd>f</kbd> ends your side of the stream so the server can
answer. <kbd>x</kbd> cancels the call at any time. When the call ends the
log shows its status, such as `0 OK` or `5 NOT_FOUND`, with the server's
message and details; the headers tab lists the response headers and the
trailers.

## Signing in with a browser

For a request whose authentication is OAuth 2.0 with the
`authorization_code` or `implicit` grant, set by the request or inherited
from a folder, the auth pane (<kbd>4</kbd>) says whether you are signed
in and when the token expires. Press <kbd>b</kbd> there to sign in:
Getman starts listening for the provider's callback on this machine and
opens your system's browser at the authorization URL. The pane shows that
it is waiting, with the address in case the browser did not open, and
<kbd>Esc</kbd> cancels. When you have signed in, the pane shows the
token's expiry, for example `Signed in · expires in 59 min`.

The terminal UI keeps the token the way the browser UI does, in
`.getman/oauth2-tokens.json`, so `getman send` and `getman run` use it
too. See [Authentication](auth.md#signing-in-with-a-browser). The
callback reaches Getman on `127.0.0.1`, so the browser must run on the
same machine; over SSH, forward the callback port first or sign in from
the browser UI.
