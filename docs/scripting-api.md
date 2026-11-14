# Scripting API

Every object, function and global a Getman script can use, with its
signature, what it returns, and whether it works before the request,
after the response, or both.

This page is the reference. [Scripts](scripts.md) is the guide: it
explains where scripts live, the order they run in, and shows them at
work. In the signatures below, an argument followed by `?` is optional.

## Globals

| Global | In | Is |
| --- | --- | --- |
| `gm` | Both | The API on this page |
| `pm` | Both | The same object as `gm`, for scripts written for Postman |
| `test(name, fn)` | Both | `gm.test` |
| `expect(value, message?)` | Both | `gm.expect` |
| `console` | Both | See [Console](#console) |
| `require(name)` | Both | See [Modules](#modules) |
| `setTimeout`, `setInterval`, `setImmediate`, `clearTimeout`, `clearInterval` | Both | See [Timers](#timers) |
| `btoa(text)`, `atob(base64)` | Both | Base64; see [Crypto and encoding](#crypto-and-encoding) |
| `crypto.randomUUID()` | Both | A random UUID v4. `crypto` has no other member |
| `tests`, `postman` | Both | See [Postman's older globals](#postmans-older-globals) |
| `responseBody`, `responseCode`, `responseTime`, `responseHeaders` | Post-response | The same |
| `bru`, `req` | Both | See [Bruno globals](#bruno-globals) |
| `res` | Post-response | The same |

The standard JavaScript built-ins are there too; see
[The JavaScript engine](#the-javascript-engine) for what is missing.

## Before and after the request

Most of the API works the same in both kinds of script. These members
differ:

| Member | In pre-request scripts | In post-response scripts |
| --- | --- | --- |
| `gm.request` | The request about to be sent; changes are sent | The request, before variables were replaced; changes do nothing |
| `gm.response` | `undefined` | The response |
| `gm.messages` | `undefined` | The messages of a WebSocket, event stream or Socket.IO request; `undefined` for others |
| `gm.execution.skipRequest()` | The request is not sent | The request was sent already, but its result is still marked skipped |
| `gm.info.eventName` | `"prerequest"` | `"test"` |
| `req.set...` calls | Changes are sent | Changes do nothing |
| `res`, `responseBody`, `responseCode`, `responseTime`, `responseHeaders` | Not defined | Defined |

The pre-request scripts of a request (the workspace's, each folder's,
then the request's) run in one engine, one after another. Its
post-response scripts run in another. So a helper a workspace script
puts on `globalThis` reaches the other scripts of the same kind, but
not the scripts of the other kind. Pass values between them with
[variables](#variables).

## The gm object

| Member | Is |
| --- | --- |
| `gm.environment`, `gm.collectionVariables`, `gm.globals`, `gm.session`, `gm.iterationData`, `gm.variables` | [Variables](#variables) |
| `gm.request` | [The request](#the-request) |
| `gm.response` | [The response](#the-response) |
| `gm.messages` | [Messages of a connection](#messages-of-a-connection) |
| `gm.cookies` | [The cookie jar](#the-cookie-jar) |
| `gm.test(name, fn)`, `gm.test.skip(name, fn?)` | [Tests](#tests) |
| `gm.expect(value, message?)` | [Expectations](#expectations) |
| `gm.sendRequest(request, callback?)` | [Sending requests](#sending-requests) |
| `gm.execution`, `gm.info` | [Run control](#run-control) |
| `gm.crypto` | [Crypto and encoding](#crypto-and-encoding) |

`pm` is the same object, so `pm.environment.get("token")` and
`gm.environment.get("token")` are the same call.

## Variables

| Object | Reads | Writes |
| --- | --- | --- |
| `gm.environment` | The selected environment | The selected environment. Not saved when none is selected |
| `gm.collectionVariables` | The workspace's variables | The workspace's variables |
| `gm.globals` | Globals | Globals |
| `gm.session` | Session variables | Session variables, kept while the UI server runs or for a whole `getman run`, never saved |
| `gm.iterationData` | The current data row of a run | Nothing that lasts: a change is gone when the scripts end |
| `gm.variables` | Every scope, strongest first | Local variables, for the rest of a run; when you send one request, until it ends |

See [Variables](variables.md#where-values-come-from) for the order of
the scopes, and for where the UI and `getman run --persist` save what
scripts set.

### Scope methods

`gm.environment`, `gm.collectionVariables`, `gm.globals`, `gm.session`
and `gm.iterationData` have the same methods:

| Method | Returns | Does |
| --- | --- | --- |
| `get(name)` | The value, or `undefined` | Reads a variable of this scope |
| `has(name)` | Boolean | Whether this scope has the variable |
| `set(name, value)` | The scope | Sets a variable. The value keeps its type until the scripts end, and is stored as text: objects and arrays as JSON |
| `unset(name)` | The scope | Removes a variable |
| `clear()` | The scope | Removes every variable of this scope |
| `toObject()` | Object | A copy of the scope's variables |
| `replaceIn(text)` | String | Replaces `{{name}}`, dynamic values such as `{{$uuid}}`, and `{{$env.GETMAN_NAME}}` in `text`, with values from every scope, not only this one |

`gm.environment.name` is the selected environment's name, or
`undefined` when none is selected.

Values read from files are strings. In `gm.iterationData`, the values
of a CSV row are strings and those of a JSON row keep their JSON type.

### gm.variables

| Method | Returns | Does |
| --- | --- | --- |
| `get(name)` | The value, or `undefined` | The value `{{name}}` would give: local, session, data row, request, folders, environment, workspace, globals |
| `has(name)` | Boolean | Whether any scope has the variable |
| `set(name, value)` | `undefined` | Sets a local variable |
| `unset(name)` | `undefined` | Removes a local variable |
| `clear()` | `undefined` | Removes every local variable |
| `toObject()` | Object | Every scope merged, with the strongest value of each name |
| `replaceIn(text)` | String | As for the scopes above |

`--var name=value` values are local variables too, so `gm.variables`
can remove them for the rest of the run.

The folders' and the request's own variables have no object of their
own. Read them with `gm.variables.get`, or with `bru.getFolderVar` and
`bru.getRequestVar`.

`replaceIn` replaces variables inside values too, ten levels deep. A
name that is not defined, or that refers to itself, is left as written.
The dynamic values are listed in
[Variables](variables.md#dynamic-values).

## The request

`gm.request` holds the request with its `{{variables}}` still in place.
In a pre-request script, what you change is sent.

| Member | Type | Is |
| --- | --- | --- |
| `id` | String | The request's path in the workspace, such as `orders/create.yaml`; empty for an unsaved request |
| `name` | String | The request's name |
| `method` | String | The method. A new value is upper-cased |
| `url` | [URL](#the-url) | The URL. Assign a string to replace it |
| `headers` | [List](#lists) | The request's headers, with those of its folders and the workspace, disabled ones included. Headers Getman adds when sending, such as for authentication, are not in it |
| `body` | Object | See [The body](#the-body) |
| `addHeader(item, value?)` | | `headers.add` |
| `upsertHeader(item, value?)` | | `headers.upsert` |
| `removeHeader(keyOrFn)` | | `headers.remove` |
| `getHeaders()` | Object | The enabled headers as `{ name: value }` |
| `toJSON()` | Object | `{ method, url, headers, body }`, which `JSON.stringify(gm.request)` gives |

Authentication is not part of `gm.request`: a script cannot read or
change the request's [auth](auth.md) settings. To send a credential of
your own, set an `Authorization` header.

### The URL

| Member | Returns | Is |
| --- | --- | --- |
| `toString()` | String | The whole URL, as `String(gm.request.url)` gives it |
| `update(text)` | | Replaces the URL, as `gm.request.url = text` does |
| `getHost()` | String | The host, such as `api.example.com`; `{{baseUrl}}` when a variable holds it |
| `getPath()` | String | The path, starting with `/` |
| `getQueryString()` | String | The enabled query parameters, without the `?` |
| `query` | [List](#lists) | The query parameters. A parameter without `=` has the value `null` |
| `toJSON()` | String | The whole URL |

### Lists

Headers, query parameters, form fields and cookies are lists of items
such as `{ key, value, disabled }`. Lists have these methods:

| Method | Returns | Does |
| --- | --- | --- |
| `get(key)` | String, or `undefined` | The value of the first enabled item with this key |
| `one(key)` | Item, or `undefined` | The first enabled item with this key |
| `has(key, value?)` | Boolean | Whether an enabled item has this key, and this value if one is given |
| `all()` | Array | Copies of every item, disabled ones included |
| `count()` | Number | How many items there are, disabled ones included |
| `each(fn)` | | Calls `fn(item)` for every item |
| `map(fn)`, `filter(fn)`, `find(fn)` | Array, or item | As the array methods, over every item |
| `toObject()` | Object | The enabled items as `{ key: value }` |
| `toString()` | String | The enabled items as `key: value` lines |
| `add(item, value?)` | | Adds an item at the end |
| `upsert(item, value?)` | | Replaces the first item with the same key, or adds one |
| `remove(keyOrFn)` | | Removes every item with this key, or every item for which `fn(item)` is true |
| `clear()` | | Removes every item |

An item to add is an object with `key` (or `name`), `value` and
`disabled`, a `"Key: value"` string, or a key with the value as the
second argument. Values are stored as text. Header names match in any
case; the keys of the other lists match exactly.

```js
gm.request.headers.add({ key: "X-Trace", value: "1" });
gm.request.headers.add("X-Span: 2");    // a "Key: value" string
gm.request.headers.upsert("X-Trace", 3); // a key and a value, stored as "3"
gm.request.url.query.remove((item) => item.key.startsWith("debug"));
```

### The body

| Member | Is |
| --- | --- |
| `mode` | `raw` for a JSON, XML or text body, `graphql`, `urlencoded`, `formdata`, `file` for a binary file, or `null` when there is no body |
| `raw` | The text of a `raw` body, or the query of a `graphql` body; `undefined` or `null` for the others. You can assign it |
| `update(value)` | Sets `raw`: a string as it is, any other value as JSON |
| `toString()` | `raw`, or `""` |
| `urlencoded` | The fields of a URL-encoded form, as a [list](#lists); `undefined` for other bodies |
| `formdata` | The fields of a multipart form, as a [list](#lists); `undefined` for other bodies |

Text set on a request without a body gives it a JSON body when the text
is JSON, and a text body otherwise. `file` bodies and forms ignore
`raw`. The variables of a GraphQL body are not exposed.

A multipart field has a `type`, `text` or `file`. A file field has `src`,
a path inside the workspace, and may have a `contentType`. Add one with
`{ key, type: "file", src, contentType }`; an item with `src` is a file
field even without `type`. See [Form bodies](scripts.md#form-bodies).

### Other kinds of request

| Request | What `gm.request` is |
| --- | --- |
| gRPC | `method` is the method's full name and `body.raw` the message, in JSON. Changes to the URL, headers and message are sent. Do not set `method`: the new value is upper-cased, and method names are case-sensitive |
| WebSocket, event stream and Socket.IO | Changes to the URL and headers are used when connecting. Other changes are ignored |

## The response

`gm.response` exists in post-response scripts.

| Member | Type | Is |
| --- | --- | --- |
| `code` | Number | The status code |
| `status` | String | The reason phrase, such as `OK` |
| `reason()` | String | The reason phrase |
| `headers` | [List](#lists) | The response headers |
| `cookies` | [List](#lists) | The cookies the response set |
| `text()` | String | The body as text |
| `json()` | Any | The body parsed as JSON. It throws an error that quotes the start of the body when it is not JSON |
| `responseTime` | Number | Milliseconds |
| `responseSize` | Number | The body's size in bytes |
| `to` | | [Response assertions](#response-assertions) |
| `toJSON()` | Object | `{ code, status, headers, body }` |

Each item of `gm.response.cookies` has `key` and `name`, both the
cookie's name, its `value`, and one more entry per attribute, named in
lowercase. Attributes with a value, such as `path`, `domain`,
`expires`, `max-age` and `samesite`, hold it as text; flags such as
`httponly` and `secure` are `true`.

For a [gRPC request](grpc.md#checks-captures-and-scripts), `code` is the
gRPC status code, `status` its name, the headers include the trailers,
and `json()` is the response message, or the list of messages of a
stream from the server.

For a WebSocket, event stream or Socket.IO request, the response is the
handshake: `code` is its status, `headers` its headers, `responseTime`
how long connecting took, `responseSize` the bytes received, and
`text()` is empty.

## Messages of a connection

For a [WebSocket, event stream or Socket.IO request](realtime.md#scripts-and-checks),
post-response scripts run when the connection ends, and `gm.messages`
is an array of the messages received, in order. The first 10,000 are
kept.

| Member | Is |
| --- | --- |
| `gm.messages.length` | How many messages arrived |
| `gm.messages[i].direction` | `received` |
| `gm.messages[i].data` | The text. Base64 for a binary message; a JSON array of the arguments for a Socket.IO event |
| `gm.messages[i].binary` | `true` when `data` is Base64; absent otherwise |
| `gm.messages[i].time` | Milliseconds after the connection opened |
| `gm.messages[i].size` | Size in bytes |
| `gm.messages[i].event` | The type of an event stream event, or the name of a Socket.IO event |
| `gm.messages[i].id` | The id of an event stream event |
| `gm.messages[i].ack` | The acknowledgement id of a Socket.IO event or acknowledgement |
| `gm.messages.json(i)` | Message `i`'s `data` parsed as JSON. It throws when there is no message `i`, or its data is not JSON |

`event`, `id` and `ack` are absent from a message that has none.

## The cookie jar

`gm.cookies` is a [list](#lists) of the cookies Getman's cookie jar
holds for the request's host, as `{ key, value, domain, path }`. It is
read when the scripts start, so in a post-response script it includes
the cookies the response stored. It is empty for gRPC requests.

## Tests

| Call | Returns | Does |
| --- | --- | --- |
| `gm.test(name, fn)` | `gm` | Runs `fn` and records a test that passes unless `fn` throws. When `fn` returns a promise, the test fails if the promise rejects |
| `gm.test.skip(name, fn?)` | `gm` | Records a skipped test. `fn` does not run |

A failed test's message is the error's message, or the thrown value
when it is not an `Error`. See [Tests](scripts.md#tests).

## Response assertions

`gm.response.to` asserts on the response. A failed assertion throws an
`AssertionError`, so put assertions inside `gm.test`.

```js
gm.test("created", () => {
  gm.response.to.have.status(201);
  gm.response.to.not.be.serverError;
  gm.response.to.have.jsonBody("data.items.0.id");
});
```

The words `to`, `be`, `been`, `is`, `that`, `which`, `and`, `has`,
`have`, `with`, `at`, `of`, `same`, `but`, `does`, `still` and `also`
only make the line read better. `not` turns the assertions after it
around.

| Method | Passes when |
| --- | --- |
| `status(code)` | A number: the status code is that number. A string: the reason phrase is that string, in any case |
| `header(name, value?)` | The response has the header; with `value`, its first value is exactly `value` |
| `body(expected?)` | Without `expected`: the body is not empty. A string: the body is exactly it. A regular expression: the body matches it. An object: the JSON body deeply equals it |
| `jsonBody(pathOrObject?, value?)` | Without arguments: the body is JSON. An object: the JSON body deeply equals it. A path: the JSON body has it; with `value`, the value there deeply equals `value` |
| `jsonSchema(schema)` | The JSON body matches the [schema](#json-schema) |

A `jsonBody` path is names separated by dots. An array item is its
number, as in `items.0.id`; `items[0]` does not work.

| Property | Passes when |
| --- | --- |
| `ok`, `success` | The status is 200 to 299 |
| `info` | The status is 100 to 199 |
| `redirection` | The status is 300 to 399 |
| `error` | The status is 400 to 599 |
| `clientError` | The status is 400 to 499 |
| `serverError` | The status is 500 to 599 |
| `accepted` | The status is 202 |
| `badRequest` | The status is 400 |
| `unauthorized` | The status is 401 |
| `forbidden` | The status is 403 |
| `notFound` | The status is 404 |
| `rateLimited` | The status is 429 |
| `withBody` | The body is not empty |
| `json` | The body is JSON and the `Content-Type` contains `json` |
| `html` | The `Content-Type` contains `html` |
| `xml` | The `Content-Type` contains `xml` |

`gm.expect(response).to.have.status(...)`, `.header(...)` and
`.jsonBody(...)` work too, on `gm.response` and on a response from
`gm.sendRequest`.

## Expectations

`gm.expect(value, message?)` starts an assertion on `value`. A failed
assertion throws an `AssertionError`. A `message` goes before its text,
as in `the count: expected 1 to equal 2`. Most methods also take a
`message` as their last argument. See
[Expectations](scripts.md#expectations) for examples.

### Words and flags

The words `to`, `be`, `been`, `is`, `that`, `which`, `and`, `has`,
`have`, `with`, `at`, `of`, `same`, `but`, `does`, `still` and `also`
do nothing. These words change the assertions after them:

| Flag | Effect |
| --- | --- |
| `not` | Turns the assertions after it around. A second `not` undoes it |
| `deep` | `equal`, `include`, `members`, `oneOf` and `property` with a value compare with deep equality |
| `own` | `property` looks at own properties only |
| `nested` | `property` takes a path, such as `customer.address.city` or `items[0].id` |
| `any` | `keys` passes when any of the keys is there |
| `all` | Undoes `any`. `keys` then needs exactly the keys given |
| `include`, `includes`, `contain`, `contains` | `keys` and `members` accept a value that has more. These words are also a method; see below |

### Properties that assert

| Property | Passes when the value |
| --- | --- |
| `ok` | Is truthy |
| `true`, `false` | Is exactly `true`, or `false` |
| `null`, `undefined` | Is exactly `null`, or `undefined` |
| `NaN` | Is `NaN` |
| `exist`, `exists` | Is neither `null` nor `undefined` |
| `empty` | Has nothing in it: a string, array, `Map` or `Set` of size 0, or an object without keys. Other values fail |
| `finite` | Is a finite number |

### Equality and comparison

| Method | Passes when the value |
| --- | --- |
| `equal(expected)`, `equals`, `eq` | Is `===` to `expected`, or deeply equal after `deep` |
| `eql(expected)`, `eqls` | Is deeply equal to `expected` |
| `oneOf(list)` | Is one of the items of `list` |
| `satisfy(fn)`, `satisfies` | Makes `fn(value)` truthy |
| `above(n)`, `gt`, `greaterThan` | Is a number above `n` |
| `least(n)`, `gte`, `greaterThanOrEqual` | Is a number of at least `n` |
| `below(n)`, `lt`, `lessThan` | Is a number below `n` |
| `most(n)`, `lte`, `lessThanOrEqual` | Is a number of at most `n` |
| `within(low, high)` | Is from `low` to `high`, both included |
| `closeTo(expected, delta)`, `approximately` | Is no further than `delta` from `expected` |

`above`, `least`, `below` and `most` fail on a value that is not a
number, even with `not`.

### Types

| Method | Passes when the value |
| --- | --- |
| `a(type)`, `an` | Is of `type`, in any case: `string`, `number`, `boolean`, `bigint`, `symbol`, `function`, `undefined`, `object`, `null`, `array`, `date`, `regexp`, `error`, `map`, `set` or `promise` |
| `instanceOf(constructor)`, `instanceof` | Is an instance of `constructor` |

### Properties and keys

| Method | Passes when the value |
| --- | --- |
| `property(name, value?)` | Has the property, its own or inherited, and with `value`, the property is `===` to it. The assertion then goes on with the property's value, except after `not` |
| `ownProperty(name, value?)`, `haveOwnProperty` | Has the property as its own |
| `keys(...names)`, `key` | Has exactly these keys. With `any`, at least one of them; with `include` or `contain`, at least all of them. The names can also be one array, or an object whose keys are used |

```js
gm.expect(order).to.have.property("customer").that.has.property("id", 7);
gm.expect(order).to.have.any.keys("id", "uuid");
gm.expect(order).to.include.keys("id");
```

### Contents and length

| Method | Passes when the value |
| --- | --- |
| `include(x)`, `includes`, `contain`, `contains` | A string: has `x` in it. An array: has an item `===` to `x`, or deeply equal when `x` is an object or after `deep`. A `Map` or `Set`: has `x`. An object: has every key and value of the object `x`, or has the key `x` |
| `members(list)` | Is an array with the same items as `list`, in any order. After `include` or `contain`: has every item of `list` |
| `lengthOf(n)`, `length` | Has a length or size of `n`: a string, array, `Map`, `Set`, or the number of keys of an object |
| `match(regexp)`, `matches` | As text, matches `regexp` |
| `string(text)` | As text, has `text` in it |

### Functions and schemas

| Method | Passes when the value |
| --- | --- |
| `throw(expected?)`, `throws`, `Throw` | Is a function that throws when called. A string: the error's message has it in it. A regular expression: the message matches it. A constructor: the error is an instance of it |
| `jsonSchema(schema)` | Matches the [JSON schema](#json-schema) |
| `status(code)`, `header(name, value?)`, `jsonBody(...)` | Is a response that passes the [response assertion](#response-assertions) of that name |

### JSON Schema

`jsonSchema` knows `type` (one type or a list, with `integer`), `enum`,
`const`, `required`, `properties`, `additionalProperties`, `items` (one
schema for every item), `minItems`, `maxItems`, `uniqueItems`,
`minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum` (as
numbers), `multipleOf`, `minLength`, `maxLength`, `pattern`, `allOf`,
`anyOf`, `oneOf` and `not`, and the schemas `true` and `false`. Other
keywords, such as `$ref` and `format`, are ignored. A failure lists up
to five problems, such as `$.email is required`.

### assert

`require("chai")` gives `{ expect, assert, AssertionError }`. `expect`
is `gm.expect`.

| Call | Passes when |
| --- | --- |
| `assert(value, message?)`, `assert.ok(value, message?)` | `value` is truthy |
| `assert.equal(actual, expected, message?)` | `actual === expected` |
| `assert.notEqual(actual, expected, message?)` | `actual !== expected` |
| `assert.deepEqual(actual, expected, message?)` | They are deeply equal |

## Sending requests

`gm.sendRequest(request, callback?)` sends another request and waits
for its answer.

| Form | Returns |
| --- | --- |
| `gm.sendRequest(request)` | A promise of the response. It rejects with an `Error` when the request cannot be sent |
| `gm.sendRequest(request, callback)` | `undefined`. It calls `callback(error, response)` before it returns |

Any status is a response: a `404` resolves the promise. The response has
the members of [`gm.response`](#the-response), `to` included.

`request` is a URL, or an object:

| Field | Is |
| --- | --- |
| `url` | The URL: a string, or an object such as `gm.request.url` |
| `method` | The method; `GET` when left out |
| `header`, or `headers` | An array of `{ key, value, disabled }` items (`name` works for `key`) or `"Name: value"` strings, or an object of names and values |
| `body` | A string, sent as it is, or one of the forms below |

| Body | Sends |
| --- | --- |
| `{ mode: "raw", raw }` | `raw` as text; a value that is not a string as JSON |
| `{ mode: "urlencoded", urlencoded }` | A URL-encoded form. `urlencoded` is an array of `{ key, value, disabled }` items or an object. Adds `Content-Type: application/x-www-form-urlencoded` unless a header sets it |
| An object without `mode` | The object as JSON. Adds `Content-Type: application/json` unless a header sets it |

Other modes, such as `formdata`, send no body.

The request is sent as given. `{{variables}}` are not replaced, so use
`gm.variables.replaceIn` on the URL and anything else that has some.
The authentication and inherited headers of the request whose script
sends it are not added. It does use that request's connection
settings, such as the proxy, certificates and timeout, and the same
cookie jar. See [Sending requests](scripts.md#sending-requests).

## Run control

### gm.execution

| Member | Returns | Does |
| --- | --- | --- |
| `setNextRequest(name)` | | In a run: after this request, go on with the request of the run that has this name or path. `null` ends the iteration. A name that is not in the run ends the iteration with a warning |
| `skipRequest()` | | In a pre-request script: the request is not sent, and its post-response scripts, checks and captures do not run. It counts as skipped |
| `stopExecution()` | | This request finishes, with its post-response scripts and checks, and then the run stops |
| `location` | Array | `[path]`: the request's path in the workspace |

`setNextRequest` only matters in a run, such as `getman run` or the
Runner in the UI; a request sent on its own ignores it. An iteration
that runs more than 1,000 requests plus 100 for each request of the run
is stopped, with a warning, since `setNextRequest` probably loops.

### gm.info

| Member | Type | Is |
| --- | --- | --- |
| `requestName` | String | The request's name |
| `requestId` | String | The request's path in the workspace |
| `iteration` | Number | The iteration of the run, from 0 |
| `iterationCount` | Number | How many iterations the run has; 1 outside a run |
| `eventName` | String | `prerequest` in pre-request scripts, `test` in post-response scripts |

## Crypto and encoding

| Call | Returns | Does |
| --- | --- | --- |
| `gm.crypto.hash(algorithm, data, encoding?)` | String | The digest of `data` |
| `gm.crypto.hmac(algorithm, key, data, encoding?)` | String | The HMAC of `data` with `key` |
| `gm.crypto.randomUUID()` | String | A random UUID v4 from the system's secure random generator |
| `gm.crypto.base64Encode(text)` | String | `text` as UTF-8, in Base64 |
| `gm.crypto.base64Decode(base64)` | String | Base64 or Base64url decoded as UTF-8 text |
| `crypto.randomUUID()` | String | As `gm.crypto.randomUUID()` |
| `btoa(text)` | String | Base64 of a Latin-1 string. Throws on characters above U+00FF |
| `atob(base64)` | String | Base64 decoded as a Latin-1 string |

`algorithm` is `md5`, `sha1`, `sha256` or `sha512`, in any case and
with or without a dash, so `SHA-256` works. `encoding` is `hex`, the
default, `base64` or `base64url`, which has no padding. `data` and `key`
are read as UTF-8 text; other values are turned into text first, objects
as JSON.

```js
gm.crypto.hmac("sha256", gm.environment.get("secret"), gm.request.body.raw ?? "", "base64");
```

## Modules

`require(name)` loads a built-in module or a `.js` file of the workspace.
Any other name throws an error that lists the modules there are.

| Name | Gives |
| --- | --- |
| `crypto-js` | A part of CryptoJS; see below |
| `uuid` | `{ v4 }`, where `v4()` returns a random UUID v4 |
| `chai` | `{ expect, assert, AssertionError }`; see [assert](#assert) |
| `btoa`, `atob` | The global functions |
| `./path.js`, `../path.js` | A [module of the workspace](scripts.md#your-own-modules) |

### crypto-js

| Member | Returns | Is |
| --- | --- | --- |
| `MD5(data)`, `SHA1(data)`, `SHA256(data)`, `SHA512(data)` | WordArray | The digest of `data`, a string read as UTF-8 or a WordArray |
| `HmacMD5(data, key)`, `HmacSHA1`, `HmacSHA256`, `HmacSHA512` | WordArray | The HMAC of `data` with `key`. The data comes first |
| `wordArray.toString(encoder?)` | String | Hex, or the text `encoder` makes, as in `toString(CryptoJS.enc.Base64)` |
| `wordArray.concat(other)` | WordArray | Appends `other`, in place |
| `wordArray.sigBytes` | Number | The size in bytes |
| `enc.Hex`, `enc.Base64`, `enc.Utf8`, `enc.Latin1` | | Encoders with `stringify(wordArray)` and `parse(text)` |
| `lib.WordArray` | | The WordArray class |

There is nothing else: no ciphers such as AES, and no other hashes.

### Modules of the workspace

| | |
| --- | --- |
| Paths | Start with `./` or `../`, and use `/`. From a script they start at the workspace root; from a module, at its folder. `.js` may be left out |
| Files | Only `.js` files inside the workspace, of 256 KiB at most. A path that leaves the workspace, or a link that points outside it, is refused |
| Names in a module | `module`, `exports`, `require`, `__filename` and `__dirname`, with paths relative to the workspace root. `this` is `module.exports` |
| Loading | Each file runs once for the scripts of one kind, and its exports are kept. Scripts of one kind can load 64 modules at most |
| Errors | A module that throws, or does not parse, fails the script that required it, with the module's path in the message |

## Console

| Call | Writes |
| --- | --- |
| `console.log(...values)` | A line at the `info` level |
| `console.info(...values)` | A line at the `info` level |
| `console.warn(...values)` | A line at the `warn` level |
| `console.error(...values)` | A line at the `error` level |
| `console.debug(...values)` | A line at the `debug` level |

Values are joined with spaces. Strings are written as they are. Other
values are formatted: objects and arrays three levels deep and 20
entries long, errors as `Name: message`, dates in ISO 8601.

Lines appear in the Console tab of the response, on standard error
with `getman send`, and with `getman run --verbose`. `console` has no
other methods: `console.table` and the like are not functions.

## Timers

| Call | Returns | Does |
| --- | --- | --- |
| `setTimeout(fn, ms?, ...args)` | Number | Calls `fn(...args)` after `ms` |
| `setInterval(fn, ms?, ...args)` | Number | Calls `fn(...args)` every `ms`, at least 1 |
| `setImmediate(fn, ...args)` | Number | As `setTimeout` with 0 |
| `clearTimeout(id)`, `clearInterval(id)` | | Cancels a timer of any kind |

Timers run on a virtual clock: callbacks run in the order of their times,
without waiting, and a script ends once its code and its timers are
done. An interval that is never cleared stops after 100,000 callbacks.
An error thrown in a callback is logged as `Uncaught in timer: ...` and
does not fail the script. There is no `clearImmediate`. To wait in real
time, use `bru.sleep(ms)`.

## Postman's older globals

| Global | In | Is |
| --- | --- | --- |
| `tests[name] = value` | Both | Records a test that passes when `value` is truthy. These are recorded after the scripts end |
| `responseBody` | Post-response | `gm.response.text()` |
| `responseCode` | Post-response | `{ code, name, detail }`: the status code, and the reason phrase twice |
| `responseTime` | Post-response | `gm.response.responseTime` |
| `responseHeaders` | Post-response | The response headers as `{ name: value }` |
| `postman.setNextRequest(name)` | Both | `gm.execution.setNextRequest` |
| `postman.setEnvironmentVariable(name, value)`, `getEnvironmentVariable(name)`, `clearEnvironmentVariable(name)` | Both | `gm.environment.set`, `get` and `unset` |
| `postman.setGlobalVariable(name, value)`, `getGlobalVariable(name)`, `clearGlobalVariable(name)` | Both | `gm.globals.set`, `get` and `unset` |

## Bruno globals

Scripts from a [Bruno import](import-export.md#from-bruno) use `bru`,
`req` and `res`. They work on the same request, response and variables
as `gm`. See [Scripts written for Bruno](scripts.md#scripts-written-for-bruno).

### bru

| Call | Returns | Does |
| --- | --- | --- |
| `bru.getEnvName()` | String, or `null` | The selected environment's name |
| `bru.getEnvVar(name)`, `bru.setEnvVar(name, value)` | Value; `undefined` | Reads or sets an environment variable |
| `bru.getGlobalEnvVar(name)`, `bru.setGlobalEnvVar(name, value)` | Value; `undefined` | Reads or sets a global |
| `bru.getVar(name)` | Value | A local variable, or else a session variable |
| `bru.setVar(name, value)` | `undefined` | Sets a session variable |
| `bru.hasVar(name)` | Boolean | Whether a local or session variable has the name |
| `bru.deleteVar(name)` | `undefined` | Removes the name from the local and session variables |
| `bru.deleteAllVars()` | `undefined` | Removes every session and local variable |
| `bru.getCollectionVar(name)`, `bru.getFolderVar(name)`, `bru.getRequestVar(name)` | Value | Reads a variable of the workspace, of the enclosing folders, or of the request |
| `bru.getProcessEnv(name)` | String, or `undefined` | A process environment variable; only `GETMAN_*` names can be read |
| `bru.interpolate(value)` | The same kind of value | Replaces `{{variables}}` in a string, or in the strings inside an object or array |
| `bru.setNextRequest(name)`, `bru.runner.setNextRequest(name)` | `undefined` | `gm.execution.setNextRequest` |
| `bru.runner.skipRequest()` | `undefined` | `gm.execution.skipRequest` |
| `bru.runner.stopExecution()` | `undefined` | `gm.execution.stopExecution` |
| `bru.sendRequest(config, callback?)` | Promise, or `undefined` | Sends a request; see below |
| `bru.sleep(ms)` | Promise | Waits `ms` in real time, then resolves. A wait that would pass the script's time limit stops the script there |

`bru.sendRequest` takes a URL or `{ url, method, headers, data, params }`.
`headers` takes the same forms as for `gm.sendRequest`. `data`, or
`body`, is sent the way `gm.sendRequest` sends a body, so an object goes
as JSON. The entries of the `params` object are added to the query.

The response is `{ status, statusText, headers, data, responseTime }`.
Header names are in lowercase, with repeated headers joined by `, `, and
`data` is the parsed JSON, or the text. Like `gm.sendRequest`, it
resolves with any status, takes an optional `callback(error, response)`,
and does not replace `{{variables}}`.

### req

| Call | Returns | Does |
| --- | --- | --- |
| `req.getUrl()`, `req.setUrl(url)` | String; `undefined` | Reads or replaces the URL, with its `{{variables}}` in place |
| `req.getMethod()`, `req.setMethod(method)` | String; `undefined` | Reads or sets the method |
| `req.getName()` | String | The request's name |
| `req.getHeader(name)` | String, or `undefined` | A header, any case |
| `req.getHeaders()` | Object | The enabled headers as `{ name: value }` |
| `req.setHeader(name, value)` | `undefined` | Replaces the header, or adds it |
| `req.setHeaders(headers)` | `undefined` | Replaces every header with the object's names and values |
| `req.deleteHeader(name)` | `undefined` | Removes the header |
| `req.getBody()` | Any | The body: parsed when it is a JSON object or array, else its text, or `undefined` |
| `req.setBody(value)` | `undefined` | Sets the body: a string as it is, any other value as JSON |

### res

`res` exists in post-response scripts.

| Member | Type | Is |
| --- | --- | --- |
| `res(path)` | Any | The value at `path` in the JSON body, or `undefined`. A path is made of `.name`, `[0]`, `[-1]` from the end and `["a key"]`, and may start with `$.` |
| `res.status`, `res.getStatus()` | Number | The status code |
| `res.statusText` | String | The reason phrase |
| `res.headers`, `res.getHeaders()` | Object | The headers, with lowercase names |
| `res.getHeader(name)` | String, or `undefined` | A header, any case |
| `res.body`, `res.getBody()` | Any | The parsed JSON body, or the text |
| `res.responseTime`, `res.getResponseTime()` | Number | Milliseconds |
| `res.getSize()` | Object | `{ body, headers, total }`, in bytes |

```js
test("the first item has an id", () => {
  expect(res.status).to.equal(200);
  expect(res("data.items[0].id")).to.be.a("number");
});
```

## The JavaScript engine

Scripts run in QuickJS, a JavaScript engine built into Getman. It runs
modern JavaScript: classes, `async` and `await`, `Promise`, `Map`, `Set`,
`Proxy`, `BigInt`, typed arrays, `JSON`, `Date` and `RegExp`, with
`queueMicrotask` and `performance.now()`.

Each script is the body of an `async` function, run in sloppy mode. So
`await` works at the top level, `return` ends the script early, and
the variables one script declares are not seen by the next. Share
helpers through `globalThis`.

These are not available: `fetch`, `XMLHttpRequest`, `URL`,
`URLSearchParams`, `TextEncoder`, `TextDecoder`, `structuredClone`,
`Intl`, `WebAssembly`, `clearImmediate`, the `window` and `global`
objects, and the `process`, `Buffer` and `fs` of Node.js. Nothing is
downloaded and there is no `npm`. Use `gm.sendRequest` to make a
request and `require` for the [modules](#modules) above.

## Limits

| Limit | Value |
| --- | --- |
| Time | 10 seconds per script. `--script-timeout SECONDS` changes it for `getman run` and `getman send` |
| Memory | 256 MiB for the scripts of one kind |
| Stack | 1 MiB |
| Timer callbacks | 100,000 per script |
| Modules | 64 for the scripts of one kind, 256 KiB each |
| Messages of a connection | The first 10,000 |

A script that runs too long fails with
`the script ran longer than 10 s and was stopped`, with its own limit
in place of 10. One that awaits a promise that never settles fails with
`the script never finished`.
See [Limits](scripts.md#limits) for how failures are reported.
