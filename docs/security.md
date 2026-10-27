# Security and privacy

Getman has no account, no cloud and no telemetry. It sends nothing
anywhere except the requests you send, with one exception you can turn
off: the [desktop app](desktop.md) looks for a new version once a day.
Your requests, responses, tokens and history stay in files on your
machine.

## The local server

The browser UI talks to a server that `getman` starts on your machine.
A server on localhost is reachable by every web page you visit, so
Getman's server defends itself against them:

- **It listens on 127.0.0.1 only**, never on your network.
- **It answers only requests addressed to `127.0.0.1` or `localhost`** on
  its port. A malicious site cannot reach it by pointing its own domain
  name at 127.0.0.1 (DNS rebinding).
- **It refuses requests from other origins.** A page on another site
  cannot call it, with or without CORS.
- **Every API call needs a token.** The token is a random 256-bit value
  that the terminal hands to the browser in the address it opens, in the
  part after `#`, which browsers never send over the network. It is kept in
  your configuration folder, readable only by you, so a bookmarked UI
  keeps working after a restart. Tokens are compared in constant time.
- **The page is locked down** with a Content Security Policy that allows
  only Getman's own scripts, and it cannot be framed by another site.

If you open the address without its token, the UI asks you to use the link
the terminal printed.

## The desktop app

The [desktop app](desktop.md) runs the same server, with the same checks,
inside its own process, and opens its window on the address with the
token. Only that page may call the app's own functions (the folder
picker, updates, restart); links that lead elsewhere open in your browser,
not in the window.

**The update check is the only request the app makes on its own.** Once a
day it downloads `latest.json` from the newest release on GitHub, which
names the newest version and its files; GitHub sees your IP address and
an app user agent, as for any download, and nothing else is sent. To
stop it, untick **Check for updates automatically** in
**Settings › About**. Copies that cannot update themselves (development
builds, the .deb and .rpm packages, the portable zip) never check.

Updates are signed when they are released, with a key kept out of the
repository, and the app installs a download only if its signature
matches the public key built into it.

## Files

A workspace is a folder you chose. Getman reads and writes only inside it:

- Request, folder and environment paths are checked so that `..` or an
  absolute path cannot reach outside the workspace.
- Files a request uploads, in form fields or binary bodies, must be inside
  the workspace too. A collection someone sends you cannot upload your SSH
  key by naming it.
- Files are written atomically, so a crash never leaves a request half
  written.

## Secrets

- Values only you should have, such as tokens and passwords, go in
  `*.local.yaml` files, which `getman init` adds to `.gitignore`.
- Variables marked `secret` in an environment are never written to the
  shared file you commit.
- `{{$env.NAME}}` reads only process environment variables that start with
  `GETMAN_`, so a workspace cannot read other credentials on your machine
  or in CI.
- History, in `.getman/history.json`, is git-ignored and keeps the last 200
  requests.

## Scripts

Scripts run in an embedded JavaScript engine with no access to files, the
network stack or your environment variables. They can do only what the
`gm` object offers: read and change the request, read the response, set
variables and send requests. Each script has a time limit (10 seconds by
default) and a memory limit (256 MiB).

Scripts can send requests to any address, as they can in other API
clients. Before you send requests from a collection you did not write,
read its scripts: they run when you send.

## TLS

Getman verifies certificates with your operating system's trust store,
so company certificate authorities work without extra setup. A request
can turn verification off with `verify_tls: false`; Getman never turns it
off for you.

## Reporting a vulnerability

Please report security problems privately, as described in
[SECURITY.md](https://github.com/fzn0x/getman/blob/main/SECURITY.md),
rather than in a public issue.
