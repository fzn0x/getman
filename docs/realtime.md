# WebSocket, SSE and Socket.IO

Some requests do not end with one response: they open a connection and
carry messages both ways, or a stream of events. Getman connects to three
kinds of them:

| `kind` | Connects to | Sends | Receives |
| --- | --- | --- | --- |
| `websocket` | A WebSocket, `ws://` or `wss://` | Text, JSON and binary messages | Messages |
| `sse` | An HTTP response in the `text/event-stream` format | The request, once | Events |
| `socketio` | A Socket.IO v4 server, over WebSocket | Events, with acknowledgements | Events |

They are request files like any other, with a `kind` line. Files without
one are HTTP requests, as before. URL, query parameters, headers,
authentication, variables and settings work as they do for HTTP requests,
and folders pass theirs down the same way.

## WebSocket

```yaml
# chat/room.yaml
name: Chat room
kind: websocket
url: "wss://{{host}}/chat?room={{room}}"
headers:
  - name: X-Client
    value: getman
auth:
  type: bearer
  token: "{{token}}"
websocket:
  subprotocols: [chat.v2]       # offered in Sec-WebSocket-Protocol
  messages:                     # saved messages, sent with one click
    - name: Join
      type: json
      content: '{"type": "join", "room": "{{room}}"}'
    - name: Ping bytes
      type: binary              # Base64
      content: AAEC
settings:
  timeout_ms: 5000              # how long connecting may take
  verify_tls: false             # for a self-signed certificate
```

`http://` and `https://` URLs are taken as `ws://` and `wss://`. The
headers and authentication go with the handshake: Basic, Bearer, API keys
(in a header or the query) and OAuth 2.0, whose token is fetched first.
Digest needs a challenge and answer that a handshake does not allow, so
it is refused.

A message's `type` is `text` (the default), `json`, which must be valid
JSON when it is sent, or `binary`, written in Base64. Messages may use
`{{variables}}`; they are filled in when the message is sent, and any
that are not defined are reported.

Getman answers the server's pings by itself. Proxies are not used for
WebSocket and Socket.IO connections.

## Server-Sent Events

```yaml
name: Price feed
kind: sse
url: "{{baseUrl}}/prices/stream"
auth:
  type: bearer
  token: "{{token}}"
sse:
  reconnect: true               # connect again when the stream ends
```

An event stream is an HTTP request whose response keeps coming, so it
takes a method, a body and any authentication, Digest included: a `POST`
with a JSON body that streams its answer back is an event stream too.
Getman
sends `Accept: text/event-stream` and `Cache-Control: no-cache` unless the
request sets them. Each event shows as it arrives, with its type (`event`),
`id`, `retry` and data; data that is JSON is shown indented.

With `reconnect: true`, Getman connects again when the stream ends or
breaks, after the delay the server asked for with `retry` (3 seconds
otherwise), and sends the last event's id in `Last-Event-ID`, as browsers
do. An error answer, such as `404`, ends it for good.

A plain HTTP request whose response turns out to be an event stream reads
until the stream ends or the request times out. When it times out, the
response keeps the events that arrived, says it is a stream, and offers to
watch it live. While any request is on its way, its Send button stops it.

## Socket.IO

```yaml
name: Order updates
kind: socketio
url: "https://{{host}}"         # http(s) or ws(s)
socketio:
  namespace: /orders            # defaults to the URL's path, or /
  auth: '{"token": "{{token}}"}'  # read by the server as socket.handshake.auth
  events: [order:created, order:shipped]  # show only these; all when empty
  path: /socket.io/             # the default
  messages:
    - name: Subscribe
      event: subscribe
      args: '["eu-west", {"limit": 10}]'
      ack: true                 # ask the server to acknowledge
```

Getman speaks Socket.IO v4 (Engine.IO 4) over WebSocket only, without
long polling. It opens Engine.IO, answers its heartbeat, joins the
namespace with the auth payload, and reports a refused join with the
server's message.

An event's `args` is a JSON array of its arguments, or one JSON value for
a single argument. With `ack: true`, the server's acknowledgement shows as
a message with the same `ack` number. Binary attachments in received
events show as `{"base64": "..."}`. The events Socket.IO keeps for itself,
such as `connect` and `disconnect`, cannot be emitted.

## In the browser

Pick **WebSocket**, **Socket.IO** or **SSE** in the method list of the
request line, or run **New WebSocket request** from the command palette
(<kbd>Ctrl</kbd> <kbd>K</kbd>). The request line's button becomes
**Connect** and **Disconnect** (**Stop** for an event stream).

- **Message** writes a message, as text, JSON or Base64, or a Socket.IO
  event with its arguments. **Send** sends it; <kbd>Ctrl</kbd>
  <kbd>Enter</kbd> connects, and once connected sends. **Save** keeps it
  in the request's saved messages, which send with one click.
- **Messages**, beside the request, logs everything: what was sent and
  received, with its time, size and content, and the connection opening,
  closing with its code and reason, and failing. Filter it by text or
  direction, clear it, and select a message to read it in full, JSON
  indented.
- **Handshake** shows the request that opened the connection and the
  server's answer, with its headers.

The getman server holds the connections and streams their events to the
page, behind the same token and checks as the rest of its API. Closing
the tab disconnects the request, and closing the page disconnects
everything it opened.

To keep memory in check, the log holds the last 2000 entries, a message
longer than 256 kB is shown cut short (its full size is still given), and
a message larger than 16 MB ends the connection. When messages arrive
faster than the page can show them, the ones it cannot keep up with are
skipped and counted.

## From the command line

`getman send` connects, sends the saved messages you name, and prints each
received message on its own line until a limit or <kbd>Ctrl</kbd>
<kbd>C</kbd>:

```sh
getman send chat/room.yaml --message Join --until 10     # ten messages
getman send prices/feed.yaml --until 30s                 # thirty seconds
getman send orders/updates.yaml -m Subscribe --until 5 --until 1m
getman send chat/room.yaml --json | jq .                 # every event, as JSON lines
```

Received messages go to standard output, so they can be piped; Socket.IO
events print as `event [arguments]`, acknowledgements as `ack N [...]`.
Everything else (connecting, sent messages, the close) goes to standard
error. `--json` prints every event as one line of JSON instead.
`--until` takes a number of received messages, or a time with `ms`, `s`
or `m`; give both and the first one reached stops. The exit status is 1
when the connection could not open or failed.

`getman run` skips these requests and says why: a connection that stays
open has no single response to check. The [terminal UI](terminal.md)
connects to them, shows their messages as they arrive and sends saved or
typed ones.

## What does not run

Scripts, checks and captures belong to HTTP requests, and do not run for
these kinds. Cookies from the cookie jar go with event streams, which are
sent by the HTTP client, but not with WebSocket and Socket.IO handshakes.
