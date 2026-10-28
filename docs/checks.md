# Checks and captures

Most API tests are one line: the status is 200, the body has an id, it
answered quickly. Getman lets you write those as **checks**, without a
script, and copy values out of the response as **captures**.

```yaml
checks:
  - status == 200
  - time < 500
  - header.content-type contains json
  - json.data.length > 0
  - json.data[0].email matches ^[^@]+@example\.com$
  - json.data[0].role is string
captures:
  firstUserId: json.data[0].id
  etag: header.etag
```

Checks run after the response arrives, after the post-response scripts.
In the UI they appear in the Checks tab of the request, with their last
result next to each one, and under Tests in the response. In a run, a
failed check fails the request, and `getman run` exits with status 1.

## Writing a check

A check is a **target**, an **operator** and usually a **value**,
separated by spaces.

### Targets

| Target | Is |
| --- | --- |
| `status` | The status code |
| `time` | Milliseconds from sending to the end of the body |
| `size` | The body's size in bytes |
| `url` | The final URL, after redirects |
| `body` | The body as text |
| `header.<name>` | A response header, any case: `header.content-type` |
| `cookie.<name>` | A cookie the response set |
| `json` | The whole body, parsed as JSON |
| `json.<path>` | Part of it |

| `messages` | The messages a connection received; see [below](#messages-of-a-connection) |
| `messages.<path>` | Part of them, like `messages[0].data` |

For a [gRPC request](grpc.md#checks-captures-and-scripts), `status` is
the gRPC status code (`0` is OK), `json` is the response message (the list
of messages for a stream from the server), and `header.<name>` finds the
trailers too.

A JSON path is made of `.name`, `[index]` (negative counts from the end),
`["a key with spaces"]` and `.length` for arrays and strings:
`json.data[0].id`, `json.items[-1].price`, `json["full name"]`,
`json.tags.length`.

### Messages of a connection

On a [WebSocket, event stream or Socket.IO request](realtime.md#scripts-and-checks)
the handshake is the response: `status` is its status (`101` for a
WebSocket, usually `200` otherwise), `header.<name>` and `cookie.<name>`
read its headers, `time` is how long connecting took, and `size` the bytes
received. The messages the connection received, in order, are `messages`:

| Target | Is |
| --- | --- |
| `messages.length` | How many arrived |
| `messages[0].data` | The first one's text; `[-1]` is the last |
| `messages[0].json.x` | Part of it, when its data is JSON |
| `messages[0].event` | The type of a stream event, or the name of a Socket.IO event |
| `messages[0].time` | Milliseconds after the connection opened |
| `messages[0].id`, `.ack`, `.size`, `.direction` | A stream event's id, a Socket.IO acknowledgement's id, its size in bytes, and `received` |

```yaml
checks:
  - status == 101
  - messages.length >= 2
  - messages[0].json.type == welcome
  - messages[-1].time < 3000
```

They run when the connection ends, after the post-response script.
`getman run` ends it at the request's `until`, so a check such as
`messages.length == 10` needs `until: { messages: 10 }`. A target on a
message that did not arrive does not exist, so `messages[5] !exists` passes
and the others fail; on an HTTP request, `messages` is an error.

### Operators

| Operator | Passes when the target |
| --- | --- |
| `==` `!=` | equals, or does not equal, the value |
| `<` `<=` `>` `>=` | compares that way, as numbers |
| `contains` `!contains` | has the value as part of its text, as an element of an array, or as a key of an object |
| `startsWith` `endsWith` | begins or ends with the value |
| `matches` `!matches` | matches a regular expression |
| `exists` `!exists` | is there at all (`null` exists); takes no value |
| `is` | has a type: `string`, `number`, `integer`, `boolean`, `array`, `object` or `null` |
| `in` `not in` | is, or is not, one of a list: a JSON array (`in [200, 201]`) or values separated by commas (`in 200, 201, 204`) |
| `between A and B` | is a number from A to B, both included |
| `is empty` `is not empty` | has nothing in it (`""`, `[]`, `{}` or `null`), or has something; take no value |
| `is truthy` `is falsy` | is true or false as in JavaScript: `false`, `0`, `""` and `null` are falsy, everything else is truthy, even an empty array; take no value |

### Values

A value that is valid JSON is compared as JSON: `== 42` is a number,
`== "42"` a string, `== true` a boolean, `== null` null, and
`contains {"id": 1}` an object. Anything else is text, so
`== Ada Lovelace` needs no quotes. Numbers and numeric strings compare
equal (`"3" == 3`), which suits headers and query values.

```yaml
checks:
  - status in 200, 201, 204
  - json.role not in [guest, banned]
  - time between 1 and 500
  - json.data is not empty
  - json.user.verified is truthy
```

Items of a list are read like any other value, so `in 200, 201` compares
numbers and `in admin, editor` text. Numbers and numeric strings compare
equal here too.

Lines starting with `#` are ignored, so you can keep a check without
running it.

### When a check fails

The message says what was found and what was expected:

```text
✗ status == 200
  status is 404; expected == 200
✗ json.data[0].id exists
  json.data[0].id does not exist
```

## Captures

A capture copies part of the response into a variable, using the same
targets as checks:

```yaml
captures:
  token: json.access_token
  sessionId: cookie.session
  location: header.location
```

Captured values go into the selected environment, or into the workspace's
variables when no environment is selected. In the UI they are saved to
the git-ignored local file; in `getman run` they last for the run (or are
saved with `--persist`). Strings are copied as they are; numbers,
booleans, arrays and objects as JSON text.

A typical login flow needs no script at all:

```yaml
# auth/login.yaml
name: Log in
method: POST
url: "{{baseUrl}}/login"
body:
  type: json
  content: '{ "user": "{{user}}", "password": "{{password}}" }'
checks:
  - status == 200
captures:
  token: json.token
```

```yaml
# getman.yaml
auth:
  type: bearer
  token: "{{token}}"
```

For anything checks cannot express, use [scripts](scripts.md).
