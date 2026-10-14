# Security policy

## Reporting a vulnerability

Please do not open a public issue for a security problem. Report it
privately through GitHub's
[private vulnerability reporting](https://github.com/fzn0x/getman/security/advisories/new)
instead.

Include what an attacker can do, the steps to reproduce it, and the
Getman version (`getman --version`) and operating system. You should get an
answer within a week. Once a fix is released, the advisory is published
with credit to you, unless you prefer otherwise.

## Supported versions

Fixes go into the latest release. Please check that a problem still
happens there before reporting it.

## What counts

Getman runs a server on `127.0.0.1` and opens files and runs scripts from
workspaces that may come from other people, so these are in scope:

- A web page, other than the UI opened from your terminal, that can call
  the local server, read its responses or learn its token.
- A workspace, collection or script that can read, write or upload files
  outside the workspace directory.
- A script that can escape the JavaScript sandbox, read the process
  environment beyond `GETMAN_*` variables, or run past its time and memory
  limits.
- Secrets written to files meant to be committed, such as a `secret`
  variable's value ending up in a shared environment file.
- An import (Postman or cURL) that writes outside the workspace.

These are not vulnerabilities on their own:

- Scripts sending requests to any host: that is what `gm.sendRequest` is
  for. Read scripts from people you do not trust before you send.
- Someone with access to your user account reading your token or local
  files.
- Turning off TLS verification with `verify_tls: false`.

See [Security and privacy](docs/security.md) for how Getman defends
itself.
