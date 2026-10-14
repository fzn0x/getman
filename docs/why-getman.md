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
| Source code | Open, under MIT or Apache-2.0 | Closed |
| Interfaces | Browser UI, terminal UI and a command line, in one binary | Desktop and web apps, and the Postman CLI |
| Scripts | JavaScript with the `gm` API; `pm` is an alias, so Postman scripts keep working | JavaScript with the `pm` API |
| Assertions without code | Checks such as `status == 200` and `json.data[0].id exists` | Written in scripts |

## What you get with Getman

**No account, ever.** Collections, environments, variables, scripts, the
runner, history and cookies all work offline. Nothing is uploaded, and
there is no telemetry.

**Requests are files you own.** Each request is a short YAML file next to
your code. Review them in pull requests, grep them, generate them, move
them with `git mv`. Values that must not be shared, such as tokens, live in
git-ignored `*.local.yaml` files beside the shared ones, so the shared files
stay safe to commit.

**One binary, three ways in.** `getman` opens the browser UI,
`getman tui` the same workspace in the terminal, and `getman run` the
same requests in CI with JUnit or JSON reports. The whole program, browser
UI included, is a single file of about 13 MB with no runtime to install.

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

## When to stay with Postman

Getman is deliberately small. Choose Postman, or use both, if you need:

- **Other protocols.** Getman sends HTTP requests. Postman also handles
  WebSocket, gRPC, MQTT and Socket.IO.
- **Hosted features.** Mock servers, monitors that run on a schedule from
  the cloud, and API publishing need a server that someone runs for you.
- **Real-time collaboration.** Getman shares work through Git, not through
  live shared workspaces and comments.
- **A long track record.** Postman has years of integrations, a public
  API network and a large community. Getman is new.
- **OAuth 2.0 flows that need a browser.** Getman fetches tokens with the
  client credentials and password grants; authorization code and implicit
  flows are not supported yet.

## Moving over

1. [Install Getman](getting-started.md#install).
2. Export your collection from Postman as **Collection v2.1**, and your
   environments as JSON.
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
