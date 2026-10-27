# Examples and mocks

An **example** is a response saved with its request: a status, headers, a
body, and the request that got it. Examples show what an API answers
without calling it, they travel with the request in its file, and
`getman mock` serves them as a stand-in for the real API.

## Saving an example

Send a saved request, then choose **Save as example** beside the
response. The example is named after the status line, such as `200 OK`;
type another name, or the name of an existing example to replace it.

The request's **Examples** tab lists its examples. Pick one to see its
request line, headers and body in the same viewer as responses. From
there you can rename or delete it, or open the request that got it as a
new, unsaved request.

Examples record the request as you wrote it, with `{{variables}}` left
in, never the values they had when it was sent, so a token from an
environment cannot end up in the file. `Set-Cookie` headers are left out
for the same reason. The body is kept as it came back: do not save a
response that holds a secret, because request files are committed.

## In the request file

Examples sit under `examples:` at the end of the request file:

```yaml
name: Get user
url: "{{baseUrl}}/users/:id"
checks:
  - status == 200
examples:
  - name: Found
    status: 200
    headers:
      - name: Content-Type
        value: application/json
    body: |
      { "id": 7, "name": "Ada" }
    request:
      method: GET
      url: "{{baseUrl}}/users/:id"
  - name: Not found
    status: 404
    body: '{ "error": "no such user" }'
```

You can write and edit them by hand. Only `name` is needed; `status`
defaults to 200, and `headers`, `body` and `request` can be left out.

### Bodies in files

A body is written inline when it is text of at most 32 KiB. A longer
body, or one that is not text (an image, a PDF, a zip), goes to a file
of its own in a directory named after the request, beside it, and the
example names the file in `body_file`:

```text
users/
  get-user.yaml               examples: [{ name: Avatar, body_file: avatar.png }]
  get-user.examples/
    avatar.png
```

The file is named after the example, with an extension from its
`Content-Type`. The `.examples` directory is renamed, moved, copied and
deleted together with its request, and it never shows up as a folder.
Commit it with the request.

## From Postman

`getman import` keeps a Postman collection's saved responses: each
request's examples get their name, status code, headers, body and
original request. See [Import and export](import-export.md).

## Mock servers

`getman mock` serves the examples of the workspace, or of one folder,
on 127.0.0.1:

```sh
getman mock                      # every example in the workspace
getman mock users                # the examples in users/
getman mock users --port 9000    # on a given port
getman mock --delay 300          # wait 300 ms before each answer
```

```text
getman mock Shop at http://127.0.0.1:7979/
  GET     /users/:id    Found 200, Not found 404   users/get-user.yaml
  POST    /orders       Created 201                orders/create-order.yaml

  GET     /users/7 → 200 Found from users/get-user.yaml
  GET     /nothing → 404 no example
```

It listens on port 7979, or the next free one, and prints each request it
answers. Point an environment's `baseUrl` at it to try a client, or a
collection, against the examples:

```yaml
# environments/mock.yaml
variables:
  baseUrl: http://127.0.0.1:7979
```

Examples are read again at most once a second, so an example you save
while the mock runs is served right away.

### How requests are matched

Each example answers the method and URL of its own `request`, or of the
request it is saved on when it has none.

- The URL is reduced to its path: the scheme and host are dropped, and so
  is a leading `{{variable}}`, which stands for the base URL.
- A path segment written as `:name`, or holding a `{{variable}}`, matches
  any one segment. `/users/:id` answers `/users/7`.
- The query string is ignored, unless the example's own request has one.
  Then each of its parameters must be there, with the same value unless
  the value is a `{{variable}}`.
- When several examples match, the most specific wins: the most literal
  path segments, then the most query parameters. `/users/me` beats
  `/users/:id`.
- Among the examples that are left, the one named in the
  `x-getman-example` request header answers. Without the header, the
  first by status code answers, with 2xx first.
- `HEAD` is answered by the `GET` example, without a body.

```sh
curl -H "x-getman-example: Not found" http://127.0.0.1:7979/users/7
```

The answer has the example's status, headers and body, and an
`x-getman-example` header naming the example. Headers that described how
the original response travelled, such as `Content-Encoding`,
`Content-Length` and `Transfer-Encoding`, are dropped, since the body is
stored decoded. When an example has no `Content-Type`, one is guessed
from the body.

A request that nothing matches gets a `404` with a JSON list of what is
mocked, and a header naming an example that does not match gets a `404`
listing the names that do.

### From the UI

Every folder's page, and the workspace's, has a **Mock** line. Start a
mock server there, copy its address, and see the latest requests it
answered; stop it the same way. One mock runs per folder at most, and
they all stop when Getman stops.

### Safety

The mock server listens on 127.0.0.1 only and answers only requests
addressed to `127.0.0.1` or `localhost`, so a web page cannot read the
examples through DNS rebinding. It adds no CORS headers of its own: if a
web page on another port calls it, add `Access-Control-Allow-Origin` to
the examples' headers.
