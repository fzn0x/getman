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
claude mcp add --transport stdio getman -- getman mcp
```

Everything after the `--` is the command that starts the server, so
Claude Code's own options, such as `--transport` and `--scope`, go before
it. If the workspace is not at the top of the project, name it, as in
`getman mcp api`.

By default the server is yours alone, in this project: Claude Code keeps
it in `~/.claude.json` (the `local` scope). With `--scope user` it is
available in all your projects, and with `--scope project` it is written
to `.mcp.json` at the top of the project, to commit and share with
everyone working on it:

```sh
claude mcp add --transport stdio --scope project getman -- getman mcp api
```

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

Claude Code asks for your approval before it uses a server from a
`.mcp.json` it did not write. `claude mcp list` shows the servers and
whether they connect, and `/mcp` does the same inside a session.
`getman` must be on the `PATH` that Claude Code starts with; if it is
not, use its full path as the command.

Then ask, for example, "send users/me.yaml with the staging environment
and tell me why the check fails".

These steps follow Claude Code's documentation,
[Connect Claude Code to tools via MCP](https://code.claude.com/docs/en/mcp).

## Claude Desktop

Open the Claude menu, then **Settings**, then **Developer**, then
**Edit Config**. That opens `claude_desktop_config.json`, creating it if
it is not there yet:

| System | File |
| --- | --- |
| macOS | `~/Library/Application Support/Claude/claude_desktop_config.json` |
| Windows | `%APPDATA%\Claude\claude_desktop_config.json` |

Add Getman to it. Give full paths, because the app does not start in your
project:

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
backslashes doubled in JSON. Quit the app completely and start it again
afterwards. If Getman does not appear, the app's logs say why:
`mcp.log` has the connection attempts, and `mcp-server-getman.log` has
what `getman mcp` wrote to standard error, in `~/Library/Logs/Claude` on
macOS and `%APPDATA%\Claude\logs` on Windows.

These steps follow the Model Context Protocol's guide,
[Connect to local MCP servers](https://modelcontextprotocol.io/docs/develop/connect-local-servers).

## Other MCP clients

Any client that starts servers over standard input and output takes the
same two things: the command `getman`, or its full path, and the
arguments `mcp` and the workspace directory. Getman writes only protocol
messages to standard output; notes go to standard error.

To try the server without an agent, use the
[MCP Inspector](https://github.com/modelcontextprotocol/inspector)'s
command line from a workspace:

```sh
npx @modelcontextprotocol/inspector --cli getman mcp . --method tools/list
npx @modelcontextprotocol/inspector --cli getman mcp . \
  --method tools/call --tool-name send_request --tool-arg path=users/me.yaml
```

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
