# WebSocket, SSE and Socket.IO

Some requests do not end with one response: they open a connection and
carry messages both ways, or a stream of events. Getman connects to three
kinds of them:

| `kind` | Connects to | Sends | Receives |
| --- | --- | --- | --- |
| `websocket` | A WebSocket, `ws://` or `wss://` | Text, JSON and binary messages | Messages |
| `sse` | An HTTP response in the `text/event-stream` format | The request, once | Events |
| `socketio` | A Socket.IO v4 server, over long polling and WebSocket | Events, with acknowledgements | Events |

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

Getman answers the server's pings by itself.

### Proxies and cookies

WebSocket and Socket.IO connections follow the same
[proxy settings](http.md#proxies) as HTTP requests: a proxy URL (an HTTP
proxy gets a `CONNECT` request with its credentials, a SOCKS5 one a login;
`socks5h://` leaves the host name to the proxy), `off`, the bypass list,
and otherwise the system's proxy and the `HTTPS_PROXY`, `HTTP_PROXY`,
`ALL_PROXY` and `NO_PROXY` variables. A proxy that refuses the tunnel is
reported with its answer.

The handshake sends the cookie jar's cookies for the URL, unless the
request sets a `Cookie` header itself, and stores the `Set-Cookie` the
server answers with, so a session cookie from signing in over HTTP works
for the socket and the other way round. Set `cookies: false` in the
request's `settings` to leave the jar alone. See
[Cookies](http.md#cookies).

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

Getman speaks Socket.IO v4 (Engine.IO 4). It opens Engine.IO, answers its
heartbeat, joins the namespace with the auth payload, and reports a
refused join with the server's message.

### Long polling and WebSocket

A Socket.IO connection travels over one of two transports: HTTP long
polling, where the client keeps a `GET` open and sends with `POST`, and
WebSocket. The official client starts with long polling and, once
connected, upgrades to WebSocket if the server offers it. Getman does the
same by default, because that is what your app's clients do, so Getman
sees what they see: the cookies a server sets on the first request, the
sticky sessions that route polling requests, and servers behind a proxy
that lets only polling through. It also makes Getman work with a server
whatever it allows. Long polling goes through the HTTP client, with its
proxy, cookie and TLS settings.

```yaml
socketio:
  transport: websocket          # auto (the default), websocket or polling
```

| `transport` | Does |
| --- | --- |
| `auto` | Starts with long polling and upgrades to WebSocket if the server offers it. A server that allows WebSocket only answers the first polling request with `400`, and Getman then connects over WebSocket and says so. If the upgrade fails, the connection stays on long polling, with a warning |
| `websocket` | WebSocket only, with no polling request first. Use it for a server set to `transports: ["websocket"]`, which saves a round trip |
| `polling` | Long polling only, never upgrading. Use it to test a server's polling transport |

The connection's first entry shows the answer to the first request: `200`
for long polling, `101` for WebSocket. A note says when a connection
upgraded.

An event's `args` is a JSON array of its arguments, or one JSON value for
a single argument. With `ack: true`, the server's acknowledgement shows as
a message with the same `ack` number. Binary attachments in received
events show as `{"base64": "..."}`. The events Socket.IO keeps for itself,
such as `connect` and `disconnect`, cannot be emitted.

## Scripts and checks

Scripts, checks and captures work on these requests as they do on HTTP
ones, around a connection instead of around one response:

- **Pre-request scripts** run before connecting. They change the URL, the
  headers and the variables, with the same `gm.request` and `gm.variables`
  as for [HTTP](scripts.md#the-request). A connection has no body, and the
  method is fixed. A script that throws stops the connection; so does
  `gm.execution.skipRequest()`.
- **Post-response scripts, checks and captures** run when the connection
  ends: in `getman run` and `getman send` when it reaches its limit or the
  server closes it, and in the browser when it disconnects. A connection
  that never opened has no response, so they do not run.
- **The handshake is the response.** `gm.response.code` and the `status`
  check are the handshake's status (`101` for a WebSocket, usually `200`
  otherwise), the headers are its response headers (`header.set-cookie`,
  `gm.response.headers.get(...)`), `time` is how long connecting took in
  milliseconds, and `size` the bytes received. The body is empty.
- **The messages received** are `gm.messages`, an array of objects with a
  `direction` (always `received`), `data`, `time` (milliseconds after the
  connection opened), `size`, and the `event` of an event stream or
  Socket.IO event, the `id` of a stream event and the `ack` of a Socket.IO
  acknowledgement. `gm.messages.json(i)` parses message `i`. For Socket.IO,
  `data` is the event's arguments as a JSON array. Messages Getman sent
  are not in the list, but a run's report shows them.

```yaml
kind: socketio
url: "{{baseUrl}}"
socketio:
  auth: '{"token": "{{token}}"}'
  messages:
    - event: subscribe
      args: '["eu-west"]'
until:
  messages: 2
scripts:
  post_response: |
    gm.test("the feed starts", () => {
      const [welcome, update] = [gm.messages.json(0), gm.messages.json(1)];
      gm.expect(welcome[0]).to.have.property("user");
      gm.expect(update[0].region).to.equal("eu-west");
    });
checks:
  - status == 200
  - messages.length == 2
  - messages[0].event == welcome
  - messages[1].json[0].region == eu-west
captures:
  userId: messages[0].json[0].user.id
```

[Checks](checks.md#messages-of-a-connection) read the messages with the
`messages` target: `messages.length`, `messages[0].data`,
`messages[0].json.x`, `messages[-1].event`. A message whose data is not
JSON has no `json` part, so a check on it fails with a clear message.

### Stopping a run

A person ends a connection in the browser; a run has to end it itself. The
request's `until` says when, and the first limit reached wins:

```yaml
until:
  messages: 10      # after ten received messages
  time_ms: 5000     # or five seconds after the connection opened
```

Without `until`, a run ends a connection after 10 messages or 5 seconds,
whichever comes first, which suits a request that answers at once. If
`until` gives a count and no time, the connection still ends after 60
seconds, so a run never waits for ever. A server that closes the connection
first ends it too. Sent messages do not count, and messages that arrive
while the connection is closing are left out, so `messages.length` is the
count you asked for.

Set `until` to what the checks need: enough messages to see the answer you
check, or enough time for a feed to deliver. `getman send` applies it only
when `--until` is not given, and the browser ignores it.

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
- **Tests** appears once a connection ended, with the results of the
  request's checks and scripts, what they logged, and warnings. The
  **Scripts** and **Checks** tabs work as for HTTP requests; the checks
  run, and the post-response script, when the connection disconnects or
  the server closes it.

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

`getman send` runs the pre-request script, connects, sends the saved
messages you name, and prints each received message on its own line until a
limit or <kbd>Ctrl</kbd> <kbd>C</kbd>. When the connection ends it runs the
post-response script and the checks, and prints their results:

```sh
getman send chat/room.yaml --message Join --until 10     # ten messages
getman send prices/feed.yaml --until 30s                 # thirty seconds
getman send orders/updates.yaml -m Subscribe --until 5 --until 1m
getman send chat/room.yaml --json | jq .                 # every event, as JSON lines
```

Received messages go to standard output, so they can be piped; Socket.IO
events print as `event [arguments]`, acknowledgements as `ack N [...]`.
Everything else (connecting, sent messages, the close, script logs and
results) goes to standard error. `--json` prints every event as one line of
JSON instead, and a last line of type `result` with the tests, checks and
logs when there are any. `--until` takes a number of received messages, or
a time with `ms`, `s` or `m`, counted from when the connection opens; give
both and the first one reached stops. Without `--until`, the request's
`until` applies, and without that <kbd>Ctrl</kbd> <kbd>C</kbd> ends the
connection. The exit status is 1 when the connection could not open or
failed, or a script, test or check failed.

`getman run` connects to these requests too, instead of skipping them. For
each one it runs the pre-request script, connects, sends the saved
messages, collects what arrives until the request's `until` (see
[Stopping a run](#stopping-a-run)), and runs the post-response script,
checks and captures. A failed check or test fails the request and the run,
and the results are in every report: the line in the terminal says how many
messages came, the HTML report lists them, and the JSON report holds the
whole conversation under `realtime`. See [Runner](runner.md).

The [terminal UI](terminal.md) connects to them too, shows their messages
as they arrive and sends saved or typed ones, and its runs work as
`getman run` does. A connection opened by hand there does not run scripts or
checks.
