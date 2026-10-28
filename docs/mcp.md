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

Every result is scrubbed before it reaches the agent. A value is
**secret** if it is:

- any value in `getman.local.yaml` (variables and globals) or in an
  environment's `.local.yaml` file;
- the value of a variable an environment lists under `secrets`;
- the value of an environment variable of the process named `GETMAN_*`.

Nothing else is treated as secret. In particular, a value you typed
straight into a request file, or a token a script keeps in memory with
`gm.session`, is not known to be secret: keep such values in a
`.local.yaml` file, or list the variable under `secrets`, if they should
stay hidden.

A secret is replaced by `[secret]` wherever it appears in the text of a
result: URLs, headers, bodies, messages, logs and field names. It does
not matter how long the secret is or what surrounds it: `Bearer abc123`,
`key=abc123&x=1` and `xabc123y` are all masked. The same goes for its
other forms:

- JSON-escaped, as it looks inside a body that is itself a JSON string;
- percent-encoded, in either case, with `%20` or `+` for a space;
- hexadecimal, in either case;
- Base64 and Base64URL, with or without padding, also when the secret
  sits inside a longer text that was encoded as a whole, such as the
  `user:secret` of a Basic `Authorization` header. The part of the
  encoded text that belongs to the secret is masked and the rest stays.

Because every length counts, a very short secret hides every occurrence
of itself, and the results around it can look garbled. A one-character
value in a local file is masked like any other. Give secrets a real
length. Remember that every `GETMAN_*` variable of the process counts,
also one that only sets an option, such as `GETMAN_ENVIRONMENT=dev`:
the word `dev` is masked in results too.

Hashes and encryption are not undone: a response that echoes a hash or a
signature of a secret, or the secret in a form not listed here, is not
caught, so treat responses from APIs that do that with care.

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
