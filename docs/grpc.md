# gRPC

Getman calls gRPC methods as well as HTTP endpoints: unary calls and all
three kinds of stream, over HTTP/2 with or without TLS. A gRPC request is
a YAML file like any other, so it gets the same variables, folders,
authentication, scripts, checks, runner and history.

```yaml
# orders/get-order.yaml
name: Get order
kind: grpc
method: shop.Orders/GetOrder
url: "{{ordersHost}}"
grpc:
  proto: protos/shop/orders.proto
  message: |
    { "id": "{{orderId}}" }
headers:
  - name: x-tenant
    value: acme
checks:
  - status == 0
  - json.state == SHIPPED
captures:
  trackingCode: json.tracking.code
```

| Field | |
| --- | --- |
| `kind: grpc` | Makes the request a gRPC call. HTTP requests leave `kind` out |
| `method` | The method's full name: `package.Service/Method` |
| `url` | The server's address: `host:port`, or `https://host:port` for TLS |
| `grpc.proto` | A `.proto` file or a folder of them, relative to the workspace root. Leave it out to ask the server through reflection |
| `grpc.tls` | `true` to connect with TLS. `https://` addresses always do |
| `grpc.message` | The message as JSON, with `{{variables}}` |
| `headers` | Sent as metadata |

A file with a `kind` this version of Getman does not know fails to load
with an error that says so, rather than being sent as HTTP.

## Starting a request

In the browser UI, choose **New gRPC request** in the command palette
(<kbd>Ctrl</kbd> <kbd>K</kbd>), or **New gRPC request here** in a
folder's menu. Then:

1. Write the server's address, and switch on **TLS** if it needs it.
2. Choose the **Schema**: **Server reflection**, a `.proto` file, or a
   folder of them. **Refresh** reads the files or asks the server again.
3. Choose the **Method**. Methods are grouped by service, and streaming
   ones say which side streams.
4. **Use example** fills the message with every field of its type.
5. Press **Invoke**, or <kbd>Ctrl</kbd> <kbd>Enter</kbd>.

**Generate requests** writes a folder with one request per method of a
service, each with an example message. The new folder takes the request's
metadata and authentication, so the requests inherit them.

## Schemas

**From `.proto` files.** Getman compiles them itself; you do not need
`protoc`. A request names one file, or a folder whose `.proto` files are
all read. Imports are looked up next to the file (or in the folder),
then from the workspace root. They never reach outside the workspace,
and the standard `google/protobuf/*.proto` types are built in. A schema
may use up to 2000 files, 4 MB per file and 32 MB in all.

**From server reflection.** With no `proto`, Getman asks the server for
its services through gRPC server reflection, `grpc.reflection.v1` or the
older `v1alpha`. Reflection runs with the request's metadata and
authentication. The answer is kept until you press **Refresh** or
restart Getman.

## Messages

Messages are protobuf's JSON form: field names in `lowerCamelCase` (the
`snake_case` names work too), enums by name, 64-bit integers as strings,
`bytes` as Base64, and `Timestamp` and `Duration` as text such as
`"2026-10-28T09:00:00Z"` and `"1.5s"`. A field the message type does not
have is an error, which catches typos.

Responses show every field, including those at their default value, so a
check such as `json.count == 0` finds them.

## Streams

| Call | The request sends | The response |
| --- | --- | --- |
| Unary | One message | One message |
| Server stream | One message | Messages as they arrive |
| Client stream | Several messages | One message |
| Bidirectional | Several messages | Messages as they arrive |

For a stream from the client, write the messages one after another:

```yaml
grpc:
  message: |
    { "sku": "BOOK-42", "quantity": 1 }
    { "sku": "PEN-7", "quantity": 3 }
```

`getman run` and `getman send` send every message, then end the stream.
In the browser UI, **Invoke** opens the stream and sends nothing yet.
**Send** sends what the editor holds, **End stream** tells the server
you are done, and **Cancel** stops the call at any time. Replies appear
in the response as they arrive, each with its time and size.

## Metadata and authentication

Headers of the request and its folders are sent as metadata. Names are
lowercased. A name ending in `-bin` carries binary data: write its value
as Base64. Other values must be printable ASCII. Names gRPC sets itself,
such as `content-type` and `te`, are not sent, so a folder can share
headers between HTTP and gRPC requests.

Authentication works as for HTTP and arrives as metadata: Basic, Bearer
and OAuth 2.0 tokens in `authorization`, an API key under its own name
(also when it is set to go in the query string). Digest does not work
with gRPC, and the types that sign each request (AWS Signature, JWT,
ASAP, OAuth 1.0, Hawk, EdgeGrid) are not sent with gRPC calls yet; use a
Bearer token in a variable instead.

## Deadlines and TLS

The request's **timeout** is the call's deadline, sent to the server as
`grpc-timeout` and enforced by Getman. Unary calls without a timeout get
30 seconds, like HTTP requests. Streams get none unless you set one, as
they may run for a long time. A timeout of `0` means no deadline.

TLS checks certificates against your system's trusted roots. For a
self-signed certificate, set **Check TLS certificates** to No in the
request's settings, or `verify_tls: false` in a folder's `settings`.

## Results

The response shows the status code and name, such as `0 OK` or
`5 NOT_FOUND`, the server's message, the response headers and trailers,
and every message sent and received with its time. When a server attaches
`grpc-status-details-bin`, it is decoded as a `google.rpc.Status`: the
standard details (`ErrorInfo`, `BadRequest`, `RetryInfo` and the others)
and types from the request's own schema show as JSON, others as Base64.

## Checks, captures and scripts

[Checks and captures](checks.md) see a gRPC call as a response:

| Target | |
| --- | --- |
| `status` | The gRPC status code; `0` is OK |
| `json`, `json.<path>` | The response message. For a server or bidirectional stream, the list of messages: `json.length`, `json[0].id` |
| `header.<name>` | Response headers, then trailers, such as `header.grpc-status` |
| `time` | Milliseconds until the call ended |
| `size` | Bytes of the messages received |

```yaml
checks:
  - status == 0
  - json.items.length > 0
  - header.x-request-id exists
```

[Scripts](scripts.md) see the same: `gm.response.code` is the status
code, `gm.response.status` its name, and `gm.response.json()` the
message, or the list of messages of a stream. In a pre-request script,
`gm.request.body.raw` is the message and can be changed.

## Command line and CI

`getman run` runs gRPC requests with their checks, beside HTTP ones; a
stream from the client sends its saved messages. `getman send` prints the
response messages as they arrive, and `--json` prints one line per
message:

```sh
getman send orders/watch-order.yaml --json | jq .state
```

`getman export` prints a gRPC request as a `grpcurl` command, and the
**Code** button in the UI does the same. The [terminal UI](terminal.md)
also calls gRPC methods: <kbd>s</kbd> invokes the method and shows each
reply as it arrives, <kbd>m</kbd> and <kbd>t</kbd> send the saved or a
typed message on a stream from the client, <kbd>f</kbd> ends it and
<kbd>x</kbd> cancels the call.
