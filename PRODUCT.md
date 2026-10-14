# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Rust workspace: `crates/getman-core` (engine) and `crates/getman` (one binary: local server, terminal UI, command line). The browser UI in `web/` is Svelte 5 + Vite + TypeScript with CodeMirror 6 editors, built to `web/dist` and embedded in the binary, served on 127.0.0.1 behind a per-user token and a strict Content-Security-Policy (no third-party scripts, styles, fonts or network calls).

## Users

Keyboard-first developers (confirmed as the primary audience): backend and API developers who keep their API requests in the same repository as their code, work in a terminal and an editor all day, and want to send a request, read the answer and get back to work without a mouse, an account or a sign-in screen. QA engineers and CI pipelines are secondary users of the same files through `getman run`.

## Product Purpose

Getman is a local-first, open-source API client. Send HTTP requests, inspect responses, test APIs with checks and scripts, and run whole collections — from plain YAML files in your repository. Success is reaching a correct response faster than with any other tool, and trusting that nothing leaves the machine.

## Positioning

- No account, ever: collections, environments, variables, scripts, the runner and history all work offline with no sign-in.
- The files in your repository are the only copy: one YAML file per request, reviewable in pull requests, with secrets in git-ignored `*.local.yaml` files.
- One small binary with three front ends over the same files: browser UI, terminal UI and a CI runner with JUnit output.
- Brings Postman work along: imports Postman collections and environments, and runs `pm.*` scripts unchanged (`pm` is an alias of `gm`).
- Open source under MIT or Apache-2.0.

## Operating Context

- Launched from a terminal inside a repository (`getman`), which opens the browser UI on 127.0.0.1; often side by side with an editor and a terminal on one screen, or on a laptop screen alone.
- Workflows: compose and send a request; edit params, headers, body, auth, scripts and checks; read the response (pretty JSON, raw, headers, cookies, tests, console); switch environments; organise requests into folders; run a folder or the whole workspace; import from Postman or a pasted cURL command; copy a request as code; revisit history.
- The same files are edited by hand in an editor and changed by `git checkout`; the UI must tolerate files changing underneath it.

## Capabilities and Constraints

- Requests: any method; URL with `{{variables}}`, query and `:path` parameters; headers; bodies: JSON, XML, text, URL-encoded form, multipart with files, binary file, GraphQL.
- Auth: inherit, none, Basic, Bearer, API key (header or query), Digest, OAuth 2.0 (client credentials, password).
- Variables: globals < workspace < environment < folders < request < data row < script locals; dynamic values like `{{$uuid}}`.
- Scripts: JavaScript pre-request and post-response scripts with the `gm` API; declarative checks (`status == 200`) and captures (`token: json.access_token`).
- Runner: iterations, CSV/JSON data, delay, bail, cancel; streamed results.
- Code export: cURL, HTTPie, PowerShell, fetch, Python, Go, Rust, Java.
- The UI talks only to its own local server (`/api/*`); it may not load anything from the internet.
- Not built: WebSocket, gRPC, mock servers, cloud sync, team accounts. Do not imply them.

## Brand Commitments

- Name: Getman (lowercase `getman` for the command). No logo exists yet.
- Voice: plain, direct, specific; errors say what happened and what to do next.
- Postman is named only in comparison and import/compatibility contexts.

## Evidence on Hand

No users, testimonials, benchmarks or adoption figures exist; do not fabricate any. Real examples: `getman init --example` creates requests against JSONPlaceholder.

## Product Principles

1. Local and owned: the user's files are the truth; never hide state in a database or a cloud.
2. Keyboard first, mouse welcome: every frequent action has a shortcut and a visible focus path.
3. Show the real wire: what was sent, what came back, how long it took, nothing prettified away.
4. Dense but calm: many controls on screen without visual noise; color carries meaning (methods, status, failures), not decoration.
5. Honest errors: every failure explains itself in one line and suggests the fix.

## Accessibility & Inclusion

Full keyboard operation with visible focus; WCAG AA contrast in light and dark themes; respects `prefers-reduced-motion` and the OS color scheme.
