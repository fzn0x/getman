# Why Getman

Getman is for people who want their API requests to live where their code
lives, work without an account, and run the same way on a laptop, over SSH
and in CI. This page compares it with Postman, the tool most people come
from, and says plainly where Postman is still the better choice.

Facts about Postman below were checked against Postman's own pages on
14 October 2026; plans and features change, so follow the links before
deciding.

## The short version

| | Getman | Postman |
| --- | --- | --- |
| Account needed to save requests and use environments | No | Yes. Signed out, the [lightweight API Client][postman-client] can send requests and keep history, but saving to collections and using environments and variables need a workspace. |
| People on the free plan | No limit; there are no accounts | 1 user on Free; teams start at $19 per user per month, billed annually ([pricing][postman-pricing]) |
| Where your requests are stored | Only in your repository, as one YAML file per request | In Postman's cloud, or, since Postman v12, as YAML files in a Git repository connected to a Postman workspace ([Native Git][postman-git]) |
| Source code | Open, under MIT or Apache-2.0 | Proprietary, licensed under Postman's [terms of service][postman-terms]; Newman and some libraries are [open source][postmanlabs] |
| Interfaces | Desktop app, browser UI, terminal UI and a command line, from one code base | Desktop and web apps, and the [Postman CLI][postman-cli] |
| Scripts | JavaScript with the `gm` API; `pm` is an alias, so Postman scripts keep working | JavaScript with the `pm` API ([examples][postman-tests]) |
| Assertions without code | Checks such as `status == 200` and `json.data[0].id exists` | Written as JavaScript tests, which Postman's AI [Agent Mode][postman-agent] can generate |

## What you get with Getman

**No account, ever.** Collections, environments, variables, scripts, the
runner, history and cookies all work offline. Nothing is uploaded, and
there is no telemetry.

**Requests are files you own.** Each request is a short YAML file next to
your code. Review them in pull requests, grep them, generate them, move
them with `git mv`. Values that must not be shared, such as tokens, live in
git-ignored `*.local.yaml` files beside the shared ones, so the shared files
stay safe to commit.

**Four ways in.** The [desktop app](desktop.md) opens a workspace in its
own window and updates itself from signed releases. `getman` opens the
same workspace in your browser, `getman tui` in the terminal, and
`getman run` runs the same requests in CI with JUnit, HTML, JSON, CSV or
Excel reports. The command line, browser UI included, is a single file
with no runtime to install.

**More than HTTP.** HTTP/1.1, HTTP/2 and HTTP/3, [gRPC](grpc.md) from
`.proto` files or server reflection, and [WebSocket, Server-Sent Events
and Socket.IO](realtime.md), in the same workspace and the same files.

**Bring your Postman work along.** `getman import collection.json` turns a
Postman collection (format v2.0 or v2.1) into folders and request files, in
the same order, with their auth, scripts and variables. Environments keep
their secret values out of the shared file. Because `pm` is the same object
as `gm`, imported scripts run unchanged in most cases; the import lists
anything it could not carry over.

**Checks without code.** `status == 200`, `time < 500` and
`header.content-type contains json` are lines in the request file, not
scripts. Captures such as `token: json.access_token` copy values into
variables the same way.

**Safe by default.** The local server answers only on 127.0.0.1, refuses
requests from other web pages and requires a token only your terminal sees.
Requests can upload files only from inside the workspace, and
`{{$env.NAME}}` reads only environment variables that start with `GETMAN_`,
so a collection from someone else cannot quietly send your private files or
secrets anywhere. See [Security](security.md).

**Free for any team.** Share a workspace the way you share code: commit it.
There are no seats to buy.

**Mocks and agents, locally.** Save responses as examples and serve them
with `getman mock`, on your machine or in CI. `getman mcp` lets AI agents
read and send the workspace's requests, with secrets masked.

## When to stay with Postman

Getman is deliberately small. Choose Postman, or use both, if you need:

- **MQTT.** Getman speaks HTTP, gRPC, WebSocket, Server-Sent Events and
  Socket.IO, but not MQTT, which Postman also handles.
- **Hosted features.** Mock servers that others reach over the internet,
  monitors that run on a schedule from the cloud, and API publishing need
  a server that someone runs for you. Getman's mocks run on your machine.
- **Real-time collaboration.** Getman shares work through Git, not through
  live shared workspaces and comments.
- **A long track record.** Postman has years of integrations, a public
  API network and a large community. Getman is new.

## Moving over

1. [Install Getman](getting-started.md#install).
2. Export your collection from Postman as **Collection v2.1**, and your
   environments as JSON. Collections from OpenAPI, HAR files, Insomnia,
   Hoppscotch and Bruno import the same way; see
   [Import and export](import-export.md).
3. In the folder that should hold the requests:

   ```sh
   getman init
   getman import "My API.postman_collection.json" --root
   getman import "Staging.postman_environment.json"
   ```

4. Run `getman` to open it, or `getman run` to run it all.

[postman-client]: https://learning.postman.com/docs/getting-started/using-api-client
[postman-pricing]: https://www.postman.com/pricing/
[postman-git]: https://www.postman.com/product/native-git/
[postman-terms]: https://www.postman.com/legal/terms/
[postmanlabs]: https://github.com/postmanlabs
[postman-cli]: https://learning.postman.com/docs/postman-cli/postman-cli-overview/
[postman-tests]: https://learning.postman.com/docs/tests-and-scripts/write-scripts/test-examples
[postman-agent]: https://blog.postman.com/testing-apis-with-postman-agent-mode-a-practical-guide/
