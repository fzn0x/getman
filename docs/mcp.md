# AI agents (MCP)

`getman mcp` lets an AI agent work with a workspace: list and read its
requests, send them, run folders, and write new requests. It is a
[Model Context Protocol](https://modelcontextprotocol.io) server that the
agent's app starts and talks to over standard input and output. Nothing
listens on the network, and nothing leaves your machine except the
requests the agent sends.

```sh
getman mcp            # the workspace around the current directory
getman mcp ./api      # a workspace elsewhere
```

You do not run it yourself: you tell your agent's app how to start it.

## Claude Code

From your project:

```sh
claude mcp add getman -- getman mcp
```

If the workspace is not at the top of the project, name it, as in
`getman mcp api`. To share the setup with everyone working on the
project, add `--scope project`, which writes it to `.mcp.json`:

```json
{
  "mcpServers": {
    "getman": {
      "command": "getman",
      "args": ["mcp", "api"]
    }
  }
}
```

Then ask, for example, "send users/me.yaml with the staging environment
and tell me why the check fails".

## Claude Desktop

Open **Settings**, then **Developer**, then **Edit Config**, and add
Getman to `claude_desktop_config.json`. Give full paths, because the app
does not start in your project:

```json
{
  "mcpServers": {
    "getman": {
      "command": "/home/ada/.local/bin/getman",
      "args": ["mcp", "/home/ada/code/shop/api"]
    }
  }
}
```

The install script puts Getman in `~/.local/bin` on Linux and macOS, and
in `%LOCALAPPDATA%\Programs\getman` on Windows, where the command reads
`C:\\Users\\Ada\\AppData\\Local\\Programs\\getman\\getman.exe`, with the
backslashes doubled in JSON. Restart the app afterwards.

## Other MCP clients

Any client that starts servers over standard input and output takes the
same two things: the command `getman`, or its full path, and the
arguments `mcp` and the workspace directory. Getman writes only protocol
messages to standard output; notes go to standard error.

## Tools

| Tool | Does |
| --- | --- |
| `list_requests` | The folders and requests as a tree, in run order, with paths and methods. Takes an optional `folder` |
| `read_request` | One request as stored, variables unresolved, plus the names of its [examples](examples-and-mocks.md) |
| `send_request` | Sends a request with its scripts, checks and captures. Takes `path`, an optional `environment` and `variables` that override every other value. Returns the status, headers, the body cut to `max_body_chars` (20 000 unless set), the time, and check and test results |
| `run_folder` | Runs a folder, a request or the whole workspace, with `environment`, `variables`, `iterations` and `bail`. Returns the totals and each request's status, time and failures |
| `list_environments` | The environments and their variables |
| `save_request` | Writes a request from JSON in the shape `read_request` returns: `path` to replace one, or `folder` and `name` to create one. Examples are kept unless the JSON has `examples` |
| `import_curl` | Turns a cURL command into a new request |

`send_request` saves nothing unless asked: with `keep_variables: true`,
variables its scripts and captures set, such as a token from a login,
go to the git-ignored local files, as the UI does, so later requests can
use them.

## Secrets

Every result is scrubbed before it reaches the agent. These values are
replaced by `[secret]` wherever they appear, in URLs, headers, bodies,
messages and logs:

- everything in `getman.local.yaml` and `environments/*.local.yaml`;
- variables an environment lists under `secrets`;
- the values of `GETMAN_*` environment variables of the process.

Values shorter than 4 characters are left alone, since hiding every `1`
or `true` would hide nothing. Only exact values are replaced, so
`send_request` does not return the headers it sent: a Basic
`Authorization` header holds the password in Base64. A response that
echoes a secret in another form, encoded or hashed, is not caught, so
treat responses from APIs that do that with care.

An agent can still send requests with your credentials: that is what it
is for. Keep your app's approval prompts on for `send_request`,
`run_folder` and `save_request`, at least against production
environments. The tools tell clients which of them only read
(`list_requests`, `read_request`, `list_environments`) and which reach
the network.

## Protocol

`getman mcp` speaks JSON-RPC 2.0, one message per line. It serves the
current revision of the protocol, 2026-07-28, where every request carries
its version in `_meta` and `server/discover` describes the server, and
the earlier revisions with an `initialize` handshake: 2025-11-25,
2025-06-18, 2025-03-26 and 2024-11-05. A client that asks `initialize`
for a version Getman does not know gets 2025-11-25.

It offers tools only: no resources, prompts or notifications.
