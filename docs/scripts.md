# Scripts

Scripts are JavaScript that runs before a request is sent and after its
response arrives. They can change the request, set variables, test the
response and send other requests. They run in an embedded QuickJS engine
inside Getman: no Node.js, no browser, no access to your files.

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
are.

## Variables

| API | Scope |
| --- | --- |
| `gm.environment` | The selected environment; `gm.environment.name` is its name |
| `gm.collectionVariables` | The workspace's variables |
| `gm.globals` | Globals |
| `gm.variables` | Reads any scope, strongest first; writes local variables that last for the rest of a run |
| `gm.iterationData` | The current data row in a run (read only) |

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
```

Header and query lists have `get`, `has`, `add`, `upsert`, `remove`,
`all`, `each`, `toObject` and `count`.

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
gm.expect(order).to.be.jsonSchema; // see jsonSchema() below
gm.expect(order).to.satisfy((o) => o.items.length > 0);
```

Also: `ok`, `true`, `false`, `null`, `undefined`, `NaN`, `empty`,
`finite`, `eql`, `least`, `most`, `below`, `above` (and `gt`, `gte`, `lt`,
`lte`), `closeTo`, `instanceOf`, `string`, `keys` with `any` or `all`,
and `jsonSchema(schema)`.

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
not available.

## Console and timers

`console.log`, `info`, `warn`, `error` and `debug` write to the Console
tab of the response and to `getman run --verbose`.

`setTimeout`, `setInterval` and `setImmediate` work, on a virtual clock:
callbacks run in order without waiting, so a script never slows a run
down.

## Limits

A script may run for 10 seconds (`--script-timeout` changes it for runs)
and use 256 MiB of memory. A script that fails is reported with its line
number:

```text
script in folder "Orders" failed: ReferenceError: tokn is not defined (line 3)
```

A failing pre-request script stops the request from being sent, and the
scripts after it do not run.
