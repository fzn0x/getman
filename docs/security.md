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

WebSocket, event stream and Socket.IO connections opened from the UI are
held by this server and stream their events to the page through the same
checked API. When the page goes away, its connections are closed.

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
  requests, each with its response (bodies cut to their first MiB) in
  `.getman/history/`. Clear history in the UI to delete them.
- Deleted requests and folders wait in `.getman/trash/` until you empty the
  trash. `.getman/` holds its own `.gitignore`, so none of it is committed
  even where the repository's ignore rules do not mention it.
- Proxy user names and passwords, and the passphrase of a PKCS#12 client
  certificate, are read from variables, so their values can live in
  `*.local.yaml` files. A variable that is not defined stops the request
  instead of sending the placeholder.
- OAuth 2.0 tokens from a browser sign-in are kept in
  `.getman/oauth2-tokens.json`, readable only by you on macOS and Linux,
  so `getman run` can use them. Client credentials and password tokens
  stay in memory.
- Private keys for JWT, ASAP and OAuth 1.0 come from a variable or a file
  inside the workspace, like uploads.

## Cookies

The browser UI keeps its cookie jar in the workspace's `.getman/cookies.json`
so logins survive a restart. Like the rest of `.getman/`, it is
git-ignored. It holds session cookies, which work as passwords for as long
as they are valid, so treat it like a password file: it is written readable
only by you on macOS and Linux, and cookies that expire are dropped when it
is read. Remove all in the Cookies dialog empties it. `getman send` and
`getman run` never read or write it.

## OAuth 2.0 sign-in

Signing in with a browser makes Getman listen on a port of 127.0.0.1 for
a few minutes, where anything on your machine can connect. The listener
reads at most 16 KiB from each connection and gives each 10 seconds,
answers only `GET` on the callback path, and finishes only for an answer
that carries the random `state` it sent; others get an error page. The
`state` and the PKCE verifier come from the operating system's random
number generator. Getman opens only `http` and `https` authorization URLs,
so a shared collection cannot make it launch another program.

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
off for you. `ca_certs` adds certificate authorities to trust for the
requests that set it; it never removes the system's.

Client certificates and CA files are read from inside the workspace only,
like files in request bodies, so a collection someone sends you cannot
present a key from elsewhere on your disk. A client certificate's private
key is a secret: keep it, or the folder it is in, out of git (add it to
`.gitignore`), and prefer a PKCS#12 file whose passphrase is in a
`*.local.yaml` file over an unencrypted PEM key. Getman holds keys in
memory only while it runs and never writes them anywhere.

## Proxies

A proxy sees every request sent through it, and, for `http://` URLs, their
contents. Getman uses your system's proxy settings unless a request says
otherwise, and `proxy: off` sends a request directly whatever the system
says. Proxy credentials go to the proxy in a `Proxy-Authorization` header
(Basic) or in the SOCKS5 handshake, which is plain text unless the proxy
URL is `https://`.

## Reporting a vulnerability

Please report security problems privately, as described in
[SECURITY.md](https://github.com/fzn0x/getman/blob/main/SECURITY.md),
rather than in a public issue.
