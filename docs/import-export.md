# Import and export

Getman reads collections, environments, API specs and recordings from
other tools, and writes folders and requests back out. Import in the UI
with <kbd>Alt</kbd> <kbd>I</kbd>, by dropping files or a folder onto the
window, or from the terminal:

```sh
getman import orders.postman_collection.json
getman import openapi.yaml
getman import ./my-bruno-collection      # a folder
```

The format is told from the file's content, so there is nothing to
choose. Each import reports what it could not carry over exactly,
rather than dropping it quietly.

A folder that holds a `bruno.json` or any `.bru` file is one Bruno
collection. Several files, or any other folder, import file by file. A
file that cannot be imported does not stop the others: the report lists
it with the reason, and `getman import` then exits with status 1. Files
in no format Getman reads, such as a README, are passed over and listed
too. When nothing at all could be imported, the import fails. A file's
extension does not matter, except that `.bru` marks a Bruno file.

| Format | What is read |
| --- | --- |
| Postman | Collections (v2.0, v2.1), environments and globals |
| OpenAPI | 3.0 and 3.1 documents, and Swagger 2.0, in JSON or YAML |
| HAR | 1.2 files saved from a browser's network panel |
| Insomnia | v4 JSON exports and v5 YAML collections |
| Hoppscotch | Collections and environments (JSON exports) |
| Bruno | A collection folder of `.bru` files, or one `.bru` file |
| cURL | A command, as copied from documentation or developer tools |

A collection becomes a folder of the workspace, named after it. Its
folders become directories and its requests YAML files, in the same
order, and its variables, authentication and scripts move to that
folder's `_folder.yaml`, so they apply to the same requests as before.

```sh
getman import orders.json --folder legacy   # inside an existing folder
getman import orders.json --root            # as the workspace itself
```

With `--root`, the collection's settings go into `getman.yaml` and its
requests to the top of the workspace, which suits a new repository.
Environments that come with a collection become files in
`environments/`, and values marked secret go to their git-ignored
`*.local.yaml`, never to the file you commit.

## From Postman

Export a collection from Postman as **Collection v2.1** (v2.0 works
too), or export an environment or the globals, and import the file.

| | |
| --- | --- |
| Requests | Method, URL, query and path parameters (disabled ones kept), headers, description |
| Bodies | Raw (JSON, XML, HTML, JavaScript, text), form, multipart, binary file, GraphQL |
| Authentication | Basic, Bearer, API key, Digest, OAuth 1.0, OAuth 2.0 (every grant), AWS Signature, JWT, Hawk, Akamai EdgeGrid, ASAP |
| Scripts | Pre-request and test scripts, unchanged: `pm` is the same object as `gm`. Disabled scripts are left out |
| Variables | Collection, folder, environment and global variables. Disabled ones are left out |
| Settings | Following redirects, the most redirects, keeping the method and the `Authorization` header on a redirect, TLS verification |
| Examples | Saved responses, with their status, headers, body and the request that got them. See [Examples and mocks](examples-and-mocks.md) |

A collection fetched from the Postman API, which wraps it in
`collection`, imports the same way. Other collection formats, such as
v1, are refused; export the collection again as v2.1. An environment's
values of the `secret` type become secrets, kept in its local file. The
globals go to `getman.local.yaml`, the workspace's git-ignored file.

Reported instead: files chosen for form fields or bodies (exports hold
a path on the exporting machine; copy the file into the workspace),
saved OAuth 2.0 tokens (they would end up in a shared file; get a new
one under **Auth**), a callback URL Getman cannot listen on, such as
Postman's own (Getman receives the sign-in on this machine; see
[Authentication](auth.md#signing-in-with-a-browser)), and other
authentication types such as NTLM. A body mode Getman does not know is
dropped and reported. An OAuth 2.0 grant it does not know sends
`{{accessToken}}` as a Bearer token, OAuth 1.0 with a signature method
it does not know signs with HMAC-SHA1, and a JWT with an algorithm it
does not know signs with HS256; each is reported.

## From OpenAPI and Swagger

Each operation becomes a request:

- **Folders** per operation's first tag, in the order of the document's
  `tags`, with the tag's description. Operations without a tag go in a
  folder named after the first path segment that is not a parameter,
  and those with neither stay at the top.
- **URLs** start with `{{baseUrl}}`, which is set in the folder's
  variables from the first server (with its variables at their
  defaults) or from `host`, `basePath` and `schemes`, preferring
  `https`. Path parameters become `:name` segments; one that shares a
  segment with other text, as in `/files/{name}.{ext}`, becomes a
  `{{name}}` variable of the request.
- **Parameters**: query and header parameters are listed with their
  example, default or first enum value, switched on when required. An
  operation's parameters replace the path's with the same name and
  place. Cookie parameters become a `Cookie` header. `Accept`,
  `Content-Type` and `Authorization` header parameters are left out.
- **Bodies** come from the media type's example or examples, or else a
  sample built from the schema: `$ref`s are followed (within the
  document), cycles are cut, and the sample stops eight levels deep.
  JSON is preferred, then forms, multipart, XML and text. In Swagger
  2.0, `body` parameters give the body, and `formData` parameters a
  form, or a multipart body when one is a file or the operation
  consumes `multipart/*`.
- **Samples** use the schema's `example`, `default`, `const`, first
  `examples` or first `enum` value when it has one. Otherwise strings
  follow their `format`, such as `user@example.com` for `email` or
  `2024-01-01` for `date`, numbers start at their `minimum` or `0`,
  booleans are `true`, `allOf` parts are merged, and `oneOf` and `anyOf`
  take the first choice. Objects leave out `readOnly` properties and
  keep at most 64.
- **Authentication** comes from the security schemes: HTTP Bearer,
  Basic and Digest, API keys in a header or the query, and OAuth 2.0
  client credentials and password flows. Secrets are read from
  variables such as `{{token}}`, `{{apiKey}}` or `{{clientSecret}}`;
  set them in an environment, as secrets. The document's `security`
  goes on the collection's folder, and an operation's own on its
  request; `security: []` means no authentication. OpenID Connect and
  OAuth 2.0 flows that need a browser send `{{accessToken}}` as a
  Bearer token.
- **Docs**: the summary names the request when it is 80 characters or
  fewer, else the `operationId`, else the method and path. The summary
  and description become its description, with a note when the
  operation is deprecated. The document's title names the collection,
  and its description becomes the collection's.

The document's responses are not read. Reported instead: references to
other files (bundle the document first), examples kept in other files,
API keys sent as cookies, OpenID Connect and OAuth 2.0 flows that need a
browser, schemes required together (only the first is set), multipart
files and binary bodies to choose, and servers other than the first.

## From a browser (HAR)

In your browser's developer tools, save the network log as a HAR file
and import it. Each entry becomes a request, named after its method and
path. Headers the browser adds on its own are left out: cookies, HTTP/2
pseudo-headers, `Host`, `Content-Length`, `Connection`,
`Accept-Encoding`, `User-Agent`, `Referer`, `Sec-*` and the like.
Bodies are read by their MIME type: forms from their fields, and
multipart bodies with the names of the files they uploaded, which you
copy into the workspace. The responses are not imported. The import says
how many requests carried cookies, and how many headers hold captured
credentials (`Authorization`, and names with `api-key` or `token`), so
you can move them into secret variables before you commit. The
collection is named after the first page's title, or else the first
request's host.

## From Insomnia

Export a collection as JSON (v4) or YAML (v5). Folders, requests, their
order, headers, query parameters, bodies, authentication and scripts
carry over. The base environment's values become the collection
folder's variables, and each sub-environment a Getman environment;
private environments are kept as secrets. `{{ _.name }}` becomes
`{{name}}`, and `{% uuid %}` and `{% now %}` become `{{$uuid}}` and
`{{$isoTimestamp}}`. Other template tags, gRPC and WebSocket requests,
and authentication types Getman does not send are reported. Scripts'
`insomnia.*` calls are rewritten as `gm.*`; check that they still do
what they did.

| Insomnia | Getman |
| --- | --- |
| `{% now 'unix' %}`, `{% now 'millis' %}` | `{{$timestamp}}`, `{{$timestampMs}}` |
| `{% faker 'randomEmail' %}`, `'randomFirstName'`, `'randomLastName'`, `'randomInt'` | `{{$randomEmail}}`, `{{$randomFirstName}}`, `{{$randomLastName}}`, `{{$randomInt}}` |
| Nested environment data, such as `{"db": {"host": "x"}}` | The variable `db.host`, as `{{ _.db.host }}` reads it |
| Basic, Digest, Bearer (with its prefix), API key in a header or the query | The same |
| OAuth 2.0 client credentials and password grants | The same |
| OAuth 2.0 grants that need a browser | `{{accessToken}}` sent as a Bearer token, reported |
| Bodies: JSON and other text, form, multipart, GraphQL, a file | The same; a file to upload is reported, to copy into the workspace |
| Following redirects, when set on the request | The request's setting |

## From Hoppscotch

Export a collection, all collections, or environments, as JSON.
Nested folders, requests, request variables, form bodies and the
authentication types Getman supports carry over, and `<<name>>`
becomes `{{name}}`. Exports do not hold uploaded files; choose them
again. Scripts are kept as they are, and their `pw.*` calls need
rewriting with `gm.*`.

- A collection's headers, authentication, variables and scripts go to
  its folder's `_folder.yaml`. A file with several collections becomes a
  folder named **Hoppscotch collections**, with one folder for each.
- GraphQL collections import too, as `POST` requests with a GraphQL
  body.
- Authentication: Basic, Digest, Bearer, API key in a header or the
  query, and OAuth 2.0 client credentials and password grants. An
  OAuth 2.0 grant that needs a browser sends `{{accessToken}}` as a
  Bearer token and is reported. The token saved in the export is not
  imported, so that it does not end up in a file you commit. Set
  `accessToken`, or sign in again. Switched-off authentication becomes
  none.
- Environments take each variable's current value, or else its initial
  value. Secret variables stay secrets, and an environment without a
  name is called **Globals**.

## From Bruno

Import the collection's folder: `getman import path/to/collection`, or
drop the folder onto the window, or use **Choose a folder** in the
Import dialog.

- `bruno.json` names the collection; `collection.bru` and `folder.bru`
  give headers, authentication, variables, scripts and docs.
- Each request keeps its query and path parameters, headers, body
  (JSON, XML, text, SPARQL, form, multipart, GraphQL, file), variables
  and docs, in the order of the files' `seq`.
- Authentication: Basic, Digest, Bearer, API key in a header or the
  query, and OAuth 2.0 client credentials and password grants. Other
  OAuth 2.0 grants send `{{accessToken}}` as a Bearer token, and other
  types are reported.
- `environments/*.bru` become environments. Secret variables are listed
  without values, which Bruno keeps outside the collection; set them
  in each environment's local file.
- **Assertions** become checks where Getman has the same test:
  `res.status: eq 200` becomes `status == 200`, `res.body.name:
  isString` becomes `json.name is string`, and `eq`, `neq`, `gt`,
  `gte`, `lt`, `lte`, `contains`, `notContains`, `startsWith`,
  `endsWith`, `matches`, `notMatches`, `isDefined`, `isUndefined`,
  `isNull`, `isEmpty`, `isNotEmpty`, `length` and the type tests
  (`isString`, `isNumber`, `isBoolean`, `isArray`, and `isJson` as
  `is object`) all translate. Others, such as `isTruthy`, are
  reported.
- **Post-response variables** that read the response, such as
  `token: res.body.token`, become captures. Assertions and captures
  read `res.status`, `res.responseTime` (as `time`), `res.body` and
  paths in it, `res('path')` and `res.headers.name`.
- **Scripts and tests** are carried over and run as they are: Getman
  provides `bru`, `req`, `res`, `test` and `expect` (see
  [Scripts](scripts.md#scripts-written-for-bruno)). A request's `tests`
  block goes at the end of its post-response script. A call Getman does
  not provide is reported, and needs rewriting with `gm.*`.
- `{{process.env.NAME}}` becomes `{{$env.GETMAN_NAME}}`, since Getman
  reads only environment variables that start with `GETMAN_`.

## From cURL

Paste a cURL command into the URL bar and Getman turns it into a
request. Or use the Import dialog, or the terminal:

```sh
getman import --curl 'curl -X POST https://api.example.com/orders -H "Content-Type: application/json" -d "{\"sku\":\"BOOK-42\"}"'
pbpaste | getman import -        # macOS clipboard
```

Commands copied from browser developer tools work, in both their bash and
Windows `cmd` forms. Getman understands the method, headers, data (as
JSON, form or raw text), `--data-urlencode`, `-F` form fields and files,
`-u` with `--digest` or `--basic`, `-G`, `-I`, `-A`, `-e`, `-b`, `-L`,
`--max-redirs`, `-k`, `-m` and `--oauth2-bearer`. Options that make no
sense for a saved request, such as `-o` or `--proxy`, are skipped.

| Option | Becomes |
| --- | --- |
| `-X`, `--request` | The method. Without it, `HEAD` with `-I`, `POST` with data or a form, else `GET` |
| `-H`, `--header` | A header. `Name;` sends it empty, and `Name:` with no value is left out. `Authorization: Bearer ...` becomes Bearer authentication |
| `-d`, `--data`, `--data-ascii`, `--data-binary`, `--data-raw` | The body, joined with `&`. JSON when the `Content-Type` says so or, without one, when the data is JSON. Else a form, when the `Content-Type` is `application/x-www-form-urlencoded` or missing and the data is one line; XML when the `Content-Type` says XML; text otherwise |
| `-d @file` | A body sent from that file (not with `--data-raw`) |
| `--json` | A JSON body, with `Accept: application/json` unless the command sets one |
| `--data-urlencode` | Form data, encoded |
| `-F`, `--form`, `--form-string` | A multipart field, `name=@file` a file and `;type=` its content type. `--form-string` never reads a file |
| `-u`, `--user` | Basic authentication, or Digest with `--digest` |
| `-G`, `--get` | The data goes to the query string instead |
| `-A`, `-e`, `-b` | `User-Agent`, `Referer` and `Cookie` headers; `-b` only when it holds `name=value`, not a cookie file |
| `--url` | The URL |
| `-L`, `--max-redirs`, `-k`, `-m` | Follow redirects, the most redirects, no TLS verification, a timeout in seconds |
| `--oauth2-bearer` | Bearer authentication |

Short flags written together, such as `-sSL` or `-XPOST`, are read too.
The request is named after its method and the URL's path.

## Export as code

<kbd>Alt</kbd> <kbd>C</kbd> in the UI, or `getman export`, writes the
current request as code, with variables replaced and authentication
applied. Signatures, such as AWS Signature or OAuth 1.0, are made as of
now and soon expire; OAuth 2.0 tokens and Digest answers need a round
trip, so they are left out:

```sh
getman export orders/create.yaml --to python -e staging
```

| `--to` | Writes |
| --- | --- |
| `curl` | A cURL command (the default) |
| `httpie` | An HTTPie command |
| `wget` | A Wget command (no multipart forms) |
| `powershell` | `Invoke-RestMethod` |
| `http` | The raw HTTP/1.1 request |
| `fetch` | JavaScript `fetch` |
| `axios` | JavaScript with axios |
| `node` | Node.js with `node:http` or `node:https` |
| `python` | Python with `requests` |
| `go` | Go with `net/http` |
| `rust` | Rust with `reqwest` |
| `java` | Java with `java.net.http` |
| `kotlin` | Kotlin with OkHttp |
| `csharp` | C# with `HttpClient` |
| `swift` | Swift with `URLSession` |
| `php` | PHP with its cURL extension |
| `ruby` | Ruby with `Net::HTTP` |
| `dart` | Dart with the `http` package |

Every value is quoted for the language it is written in, so quotes,
backslashes, `$`, line breaks and non-ASCII text arrive as they were.

Headers the client sets itself are left out: Getman's own `User-Agent`,
`Accept: */*`, `Content-Length` and `Host`. A multipart body is built by
the code, with its own boundary, so its `Content-Type` is left out too.
Files a request uploads are named relative to the workspace, so the code
works from the workspace folder on any machine.

A [gRPC request](grpc.md) is written only as a `grpcurl` command, with
`--to curl`, the default, or `--to grpcurl`. WebSocket, event stream and
Socket.IO requests are not written as code.

Exported code contains the real values of your variables, tokens
included. To share a snippet, keep the variables instead: switch on
**Keep `{{variables}}`** in the code dialog, or pass `--keep-vars`.
The code then holds `{{token}}` rather than the token, and Basic
credentials that use variables appear as `base64(user:password)`,
since they cannot be encoded.

### Indent and boilerplate

Each language is written the way it is usually written: tabs in Go, two
spaces in cURL, Wget, JavaScript, Ruby and Dart, four elsewhere. Two
options change that, in the code dialog and on the command line:

| Option | Does |
| --- | --- |
| **Indent**, `--indent 2\|4\|tab` | Indents with two spaces, four spaces or tabs, in every language that indents. Text inside quotes, such as a multi-line body, is never touched |
| **Imports and `main`**, `--no-boilerplate` | Leaves out what wraps the request, where a language has it: the imports, the `main` function or class and the `package` line. What is left is the statements that make the request, to paste into your own code |

```sh
getman export orders/create.yaml --to go --indent 2 --no-boilerplate
```

cURL, HTTPie, Wget, PowerShell, the raw request and PHP have no
boilerplate to leave out. HTTPie and the raw request have no indent, and
neither do C# and Swift, whose snippets are top-level statements. The
dialog shows only the options a language has. gRPC requests print as
`grpcurl`, which has neither.

## Export a folder or an environment

Folders, the workspace and environments export as files other tools
read. In the UI, use **Export…** on a folder in the sidebar, **Export
the workspace** in the palette (<kbd>Ctrl</kbd> <kbd>K</kbd>), or
**Export** in the environments view.

```sh
getman export orders --to postman -o orders.postman_collection.json
getman export --to postman -o workspace.json        # the whole workspace
getman export --env staging --to postman            # an environment
getman export --globals --to postman --with-secrets # the globals, values included
getman export orders --to openapi -o orders.yaml    # OpenAPI, as YAML
```

**Postman** collections (v2.1) carry folders, requests, bodies,
authentication, scripts, variables and redirect and TLS settings. A
folder exported on its own takes along what it inherits: the variables,
headers, authentication and scripts of the folders around it and of the
workspace become the collection's. Postman has no place for some
things, so they are written as script lines: folder headers as
`pm.request.headers.upsert(...)`, request variables as
`pm.variables.set(...)`, checks as `pm.test(...)` and captures as
`set(...)` calls in the test script. Checks without a translation stay
as comments and are reported. Every authentication type maps to
Postman's own, with three exceptions that are reported: private keys
read from a file are left out (paste the key into Postman), Postman adds
no `iat` or `exp` to a JWT, and it always signs the payload for AWS.

Also reported: OAuth 1.0 parameters sent in the body, which now go in
the URL; an OAuth 2.0 audience, which only some Postman versions send;
form files, which Postman reads from its working directory and which
are named relative to the workspace; a request's timeout, and settings
on a folder or the workspace, which Postman has no place for; and
request files that cannot be read, which are left out. An exported
environment marks its secret variables with Postman's `secret` type.

Environments and globals export as Postman environment files. Values
kept in git-ignored local files are left out unless you ask for them
(`--with-secrets`, or **Include local values** in the UI): secret values
and other local values of an environment, and all values of the
globals, which live in `getman.local.yaml`.

**OpenAPI 3.1** documents are inferred from the requests, as a starting
point:

- `servers` are the value of `baseUrl` in the workspace or folder
  variables, and in each environment that sets it.
- `paths` come from URLs relative to `{{baseUrl}}`; for other URLs the
  scheme and host are dropped. `:name` and `{{name}}` segments become
  path parameters.
- Query parameters (switched off ones too) and headers other than
  `Content-Type`, `Accept` and `Authorization` become parameters, with
  their values as examples.
- Bodies become a request body of their media type with the body as
  the example; forms and multipart bodies also get a simple schema.
- Top-level folders become tags; the request's name is the summary,
  its description the description, and a unique camelCase form of the
  name the `operationId`. Headers include those the request inherits
  from its folders.
- The title is the folder's or workspace's name, with its description,
  and `info.version` is `1.0.0`.
- Authentication becomes security schemes, for the whole document or
  per operation where it differs. JWT and ASAP become bearer JWTs, and
  OAuth 2.0 grants their flows. Request signing (AWS Signature,
  OAuth 1.0, Hawk, EdgeGrid) has no scheme in OpenAPI and is left out.
- A `status == N` check names the expected response. Operations without
  one have no `responses`, which OpenAPI 3.1 allows.

Methods OpenAPI does not know, and a second request for the same method
and path, are left out and reported. Also reported: a URL that starts
with another variable, such as `{{host}}`, whose path is then taken as
relative to the servers, and a workspace with no `baseUrl` at all,
whose server is then `/`.

Both formats describe HTTP requests. gRPC, WebSocket, event stream and
Socket.IO requests are left out of both, with a warning that names the
request, such as `Chat: Postman collections cannot hold a WebSocket
request; it was left out`.
