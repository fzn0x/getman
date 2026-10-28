# Scripts

Scripts are JavaScript that runs before a request is sent and after its
response arrives. They can change the request, set variables, test the
response and send other requests. They run in an embedded QuickJS engine
inside Getman: no Node.js, no browser, and no access to your files except
the `.js` modules of the workspace that they `require`.

```yaml
scripts:
  pre_request: |
    gm.request.headers.upsert({ key: "X-Request-Id", value: crypto.randomUUID() });
  post_response: |
    gm.test("returns the order", () => {
      gm.response.to.have.status(200);
      const order = gm.response.json();
      gm.expect(order).to.have.property("id");
      gm.expect(order.items).to.be.an("array").that.is.not.empty;
    });
    gm.environment.set("orderId", gm.response.json().id);
```

Folders and the workspace can have scripts too. They run around every
request inside, outermost first: the workspace's, each folder's, then the
request's. All scripts of one request share one engine, so a workspace
script can define helpers on `globalThis` for the others.

**Coming from Postman?** `pm` is the same object as `gm`, and the older
`tests["name"] = ...`, `responseBody`, `responseCode` and
`postman.setNextRequest` globals exist too, so most scripts run as they
are. **Coming from Bruno?** `bru`, `req`, `res`, `test` and `expect` work
too; see [Scripts written for Bruno](#scripts-written-for-bruno).

## Variables

| API | Scope |
| --- | --- |
| `gm.environment` | The selected environment; `gm.environment.name` is its name |
| `gm.collectionVariables` | The workspace's variables |
| `gm.globals` | Globals |
| `gm.variables` | Reads any scope, strongest first; writes local variables that last for the rest of a run |
| `gm.iterationData` | The current data row in a run (read only) |
| `gm.session` | Session variables: they last while the UI server runs, or for a whole `getman run`, and are never saved |

Each has `get(name)`, `set(name, value)`, `has(name)`, `unset(name)`,
`clear()`, `toObject()` and `replaceIn(text)`, which replaces
`{{variables}}` in a string. Values keep their type within a script;
they are saved as text, with objects and arrays as JSON.

Values set in the environment, workspace and globals are saved to the
git-ignored local files when you send from the UI. See
[Variables](variables.md).

## The request

In pre-request scripts, `gm.request` is the request about to be sent,
before variables are replaced. Changes are sent.

```js
gm.request.method = "PUT";
gm.request.url = "{{baseUrl}}/v2/orders";
gm.request.url.query.add({ key: "expand", value: "items" });
gm.request.url.query.remove("debug");
gm.request.headers.upsert({ key: "Authorization", value: `Bearer ${token}` });
gm.request.headers.remove("X-Old");
gm.request.body.update({ sku: "BOOK-42" }); // objects become JSON
gm.request.body.raw; // the body text
gm.request.id; // the request's path in the workspace, such as "orders/create.yaml"
```

Header, query and form lists have `get`, `has`, `one`, `add`, `upsert`,
`remove`, `clear`, `all`, `each`, `map`, `filter`, `find`, `toObject` and
`count`.

### Form bodies

When the body is a form, its fields are lists you can edit before the
request goes out. `gm.request.body.mode` says which: `urlencoded` or
`formdata` (multipart). The other list is `undefined`.

```js
// A URL-encoded form
const form = gm.request.body.urlencoded;
form.upsert({ key: "grant_type", value: "client_credentials" });
form.add({ key: "scope", value: "read write" });
form.remove("debug");

// A multipart form: text fields and files
const parts = gm.request.body.formdata;
parts.upsert({ key: "title", value: "Cat" });
parts.add({ key: "photo", type: "file", src: "img/cat.png", contentType: "image/png" });
parts.one("photo").src; // "img/cat.png"
```

A file field names a file inside the workspace, and is read when the
request is sent, as in the request's Body tab. You cannot turn another
kind of body into a form from a script.

## The response

In post-response scripts:

| | |
| --- | --- |
| `gm.response.code` | Status code |
| `gm.response.status` | Reason phrase, such as `OK` |
| `gm.response.headers.get(name)` | A header, any case |
| `gm.response.cookies.get(name)` | A cookie the response set |
| `gm.response.text()` | The body as text |
| `gm.response.json()` | The body parsed; throws a clear error if it is not JSON |
| `gm.response.responseTime` | Milliseconds |
| `gm.response.responseSize` | Body size in bytes |

For a [gRPC request](grpc.md#checks-captures-and-scripts),
`gm.response.code` is the gRPC status code, `gm.response.status` its
name, the headers include the trailers, and `gm.response.json()` is the
response message, or the list of messages of a stream from the server.

### WebSocket, event stream and Socket.IO requests

For a [WebSocket, event stream or Socket.IO request](realtime.md#scripts-and-checks),
the pre-request script runs before it connects, and the post-response
script when the connection ends. The handshake is the response, and the
messages received are `gm.messages`:

| | |
| --- | --- |
| `gm.response.code` | The handshake's status: `101` for a WebSocket, usually `200` otherwise |
| `gm.response.headers.get(name)` | A header of the handshake's answer |
| `gm.response.responseTime` | Milliseconds connecting took |
| `gm.response.responseSize` | Bytes received |
| `gm.messages` | The received messages, in order: an array of `{ direction, data, time, size, event, id, ack }` |
| `gm.messages.length` | How many arrived |
| `gm.messages.json(i)` | Message `i` parsed as JSON; throws a clear error if it is not JSON |

`time` is milliseconds after the connection opened, `data` the message's
text (Base64 when it is binary, a JSON array of arguments for a Socket.IO
event), and `event` the type of a stream event or the name of a Socket.IO
event. `gm.response.text()` is empty. `gm.messages` is `undefined` in
pre-request scripts and for HTTP requests.

```js
gm.test("the feed starts with a welcome", () => {
  gm.expect(gm.messages.length).to.be.at.least(2);
  gm.expect(gm.messages[0].event).to.equal("welcome");
  gm.expect(gm.messages.json(1).price).to.be.a("number");
});
gm.environment.set("lastId", gm.messages[gm.messages.length - 1].id);
```

## Tests

`gm.test(name, fn)` records a test that passes unless `fn` throws.
`gm.test.skip(name, fn)` records it as skipped. Async functions work.

```js
gm.test("is fast", () => gm.expect(gm.response.responseTime).to.be.below(300));
gm.test.skip("not ready yet", () => {});
```

### Response assertions

```js
gm.response.to.have.status(201);
gm.response.to.have.status("Created");
gm.response.to.have.header("Location");
gm.response.to.have.header("Content-Type", "application/json");
gm.response.to.have.body("pong");
gm.response.to.have.jsonBody("data.items");
gm.response.to.have.jsonBody("data.total", 3);
gm.response.to.have.jsonSchema({ type: "object", required: ["id"] });
gm.response.to.be.ok; // also success, error, clientError, serverError,
//                      notFound, unauthorized, forbidden, json, html, xml
gm.response.to.not.have.status(500);
```

### Expectations

`gm.expect(value)` reads like English. Chain words (`to`, `be`, `been`,
`is`, `that`, `which`, `and`, `has`, `have`, `with`, `at`, `of`, `same`,
`does`) only make it read better.

```js
gm.expect(order.id).to.equal(7);
gm.expect(order).to.deep.equal({ id: 7, items: [] });
gm.expect(order).to.have.property("status", "paid");
gm.expect(order).to.have.nested.property("customer.address.city");
gm.expect(order.items).to.have.lengthOf(2);
gm.expect(order.items).to.include.members(["BOOK-42"]);
gm.expect(order.tags).to.include("gift").and.not.include("void");
gm.expect(order).to.have.all.keys("id", "status", "items");
gm.expect(order.total).to.be.a("number").that.is.above(0);
gm.expect(order.total).to.be.within(1, 100);
gm.expect(order.created).to.match(/^\d{4}-/);
gm.expect(order.status).to.be.oneOf(["paid", "pending"]);
gm.expect(order.note).to.not.exist;
gm.expect(() => JSON.parse("{")).to.throw(SyntaxError);
gm.expect(order).to.have.jsonSchema({ required: ["id", "items"] });
gm.expect(order).to.satisfy((o) => o.items.length > 0);
```

Also: `ok`, `true`, `false`, `null`, `undefined`, `NaN`, `empty`,
`finite`, `eql`, `least`, `most`, `below`, `above` (and `gt`, `gte`, `lt`,
`lte`), `closeTo`, `instanceOf`, `string`, and `keys` with `any` or
`all`.

The JSON Schema support covers `type` (with `integer`), `enum`, `const`,
`required`, `properties`, `additionalProperties`, `items`, `minItems`,
`maxItems`, `uniqueItems`, `minimum`, `maximum`, the exclusive forms,
`multipleOf`, `minLength`, `maxLength`, `pattern`, `allOf`, `anyOf`,
`oneOf` and `not`.

## Sending requests

`gm.sendRequest` sends another request, for example to fetch a token
first. Await it, or pass a callback.

```js
const res = await gm.sendRequest({
  url: gm.variables.replaceIn("{{authUrl}}/token"),
  method: "POST",
  header: [{ key: "Content-Type", value: "application/json" }],
  body: { mode: "raw", raw: JSON.stringify({ client: "cli" }) },
});
gm.environment.set("token", res.json().access_token);

gm.sendRequest("https://example.com/ping", (err, res) => {
  if (err) console.error(err.message);
});
```

Bodies can be `{ mode: "raw", raw }`, `{ mode: "urlencoded", urlencoded }`
or a plain object, sent as JSON.

## Controlling a run

```js
gm.execution.setNextRequest("Get order"); // jump, by name or path
gm.execution.setNextRequest(null);        // end this iteration
gm.execution.skipRequest();               // in a pre-request script: don't send
gm.execution.stopExecution();             // the request finishes; the rest of the run does not start
gm.info.requestName; gm.info.iteration; gm.info.iterationCount; gm.info.eventName;
```

## Crypto, encoding and modules

```js
gm.crypto.hash("sha256", "text");                 // hex; md5, sha1, sha256, sha512
gm.crypto.hmac("sha256", key, message, "base64"); // hex, base64 or base64url
gm.crypto.base64Encode("text"); gm.crypto.base64Decode("dGV4dA==");
crypto.randomUUID(); btoa("user:pass"); atob("dXNlcjpwYXNz");

const CryptoJS = require("crypto-js"); // MD5, SHA1, SHA256, SHA512, their Hmac forms, and enc
CryptoJS.HmacSHA256(message, secret).toString(CryptoJS.enc.Base64);
```

`require` also knows `uuid`, `chai`, `btoa` and `atob`. Other modules are
not available, and nothing is downloaded: there is no `npm`.

### Your own modules

Put helpers in `.js` files of the workspace and `require` them with a
relative path. They are CommonJS modules, so use `module.exports`:

```js
// lib/helpers.js
const CryptoJS = require("crypto-js");
module.exports = {
  sign: (body, secret) => CryptoJS.HmacSHA256(body, secret).toString(),
};
```

```js
// in any script
const { sign } = require("./lib/helpers.js");
gm.request.headers.upsert({ key: "X-Signature", value: sign(gm.request.body.raw, secret) });
```

- A path in a script starts at the workspace root; in a module it starts
  at the module's own folder, so `require("./sign.js")` inside
  `lib/helpers.js` loads `lib/sign.js`. `.js` may be left out.
- Only files inside the workspace load. A path that leaves it with `..`,
  or reaches it through a link, is refused. So are files that are not
  `.js`, and files over 256 KiB.
- A module runs once per script run, however many scripts require it,
  and every script of the request gets the same exports. Modules that
  require each other get what the other has exported so far, as in
  Node.js. The `process`, `fs` and `Buffer` globals of Node.js do not
  exist.
- A module that throws, or has a syntax error, fails the script that
  required it, with the module's path in the message.

## Scripts written for Bruno

Scripts from a [Bruno import](import-export.md#from-bruno) run as they
are. Getman provides the globals Bruno's scripts use:

| | |
| --- | --- |
| `bru.getVar`, `setVar`, `hasVar`, `deleteVar`, `deleteAllVars` | Bruno's runtime variables, which here are [session variables](variables.md#session-variables): they last while Getman runs |
| `bru.getEnvVar`, `setEnvVar`, `getEnvName` | The selected environment |
| `bru.getGlobalEnvVar`, `setGlobalEnvVar` | Globals |
| `bru.getCollectionVar`, `getFolderVar`, `getRequestVar` | The workspace's, the enclosing folders' and the request's variables (read only) |
| `bru.getProcessEnv(name)` | A process environment variable; only `GETMAN_*` names are readable, others give `undefined` |
| `bru.interpolate(text)` | Replaces `{{variables}}`, in a string or in the strings of an object |
| `bru.setNextRequest(name)`, `bru.runner.setNextRequest(name)` | As `gm.execution.setNextRequest` |
| `bru.runner.skipRequest()`, `bru.runner.stopExecution()` | Skip this request; finish this request and end the run |
| `bru.sendRequest(config, callback)` | Sends a request with `{ url, method, headers, data, params }` and gives `{ status, statusText, headers, data }`, with any status; without a callback it returns a promise |
| `bru.sleep(ms)` | Waits, in real time, but never past the script time limit |
| `req.getUrl`, `setUrl`, `getMethod`, `setMethod`, `getName` | The request, with its `{{variables}}` still in place |
| `req.getHeader`, `getHeaders`, `setHeader`, `setHeaders`, `deleteHeader` | Headers |
| `req.getBody`, `req.setBody` | The body, as an object if it is JSON |
| `res(path)`, `res.status`, `statusText`, `headers`, `body`, `responseTime` | The response, after it arrives; `res("data.items[0].id")` reads the JSON body |
| `res.getStatus`, `getHeader`, `getHeaders`, `getBody`, `getResponseTime`, `getSize` | The same, as calls |
| `test(name, fn)`, `expect(value)` | As `gm.test` and `gm.expect` |

`res` exists in post-response scripts only. Bruno's other calls (for
example `bru.cwd`) are not provided; the importer lists the ones a
script uses.

## Console and timers

`console.log`, `info`, `warn`, `error` and `debug` write to the Console
tab of the response and to `getman run --verbose`.

`setTimeout`, `setInterval` and `setImmediate` work, on a virtual clock:
callbacks run in order without waiting, so a script never slows a run
down.

## Turning scripts off

You may not want to run scripts that came with a workspace you have
not read. Settings in the UI has a "Run scripts" switch; it writes
`scripts_off: true` to `getman.local.yaml`, which stays on your machine.
`getman send --no-scripts` and `getman run --no-scripts` skip scripts
for one command. Requests are sent as saved, checks and captures still
run, and the result says how many scripts did not run:

```text
Scripts are turned off for this run, so 2 scripts did not run.
```

## Limits

A script may run for 10 seconds (`--script-timeout` changes it for runs)
and use 256 MiB of memory. A script that fails is reported with where it
came from and its line number:

```text
script in folder "Orders": ReferenceError: tokn is not defined (line 3)
```

A failing pre-request script stops the request from being sent, and the
scripts after it do not run.
