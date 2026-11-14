// The landing page: what Getman is, the download and the install command
// for the visitor's system, a screenshot, a bento grid of features whose
// small mockups replay what the app and the command line really show, and
// a few measured facts.

import { bar, footer, head, orbitMark, palette } from "./layout.mjs";
import { icon } from "./icons.mjs";

// The head script names the visitor's system on <html data-system>, and
// site.css hides what is for other systems. "other" is for phones, tablets
// and pages read without script.
const systems = [
  { system: "windows", name: "Windows", file: "Getman-windows-x64-setup.exe" },
  { system: "macos", name: "macOS", file: "Getman-macos-universal.dmg" },
  { system: "linux", name: "Linux", file: "Getman-linux-x86_64.AppImage" },
];

const commands = [
  { systems: "windows", text: "irm https://fzn0x.github.io/getman/install.ps1 | iex" },
  {
    systems: "macos linux other",
    text: "curl -fsSL https://fzn0x.github.io/getman/install.sh | sh",
  },
];

const escape = (text) =>
  String(text).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** A command that may wrap on narrow screens, but only after a slash or a space. */
function breakable(command) {
  return escape(command).replace(/\//g, "/<wbr>");
}

// ---- Feature mockups ---------------------------------------------------------
// Each mirrors the app or the command line with real names and output: the
// example workspace from `getman init --example`, its checks, a real
// `getman run`, and a real `getman import` of a Postman collection.

const yaml = [
  ['<span class="k">name</span>: List posts'],
  ['<span class="k">method</span>: <span class="m-get">GET</span>'],
  [
    '<span class="k">url</span>: <span class="s">\'<span class="v">{{baseUrl}}</span>/posts?_limit=5\'</span>',
  ],
  ['<span class="k">checks</span>:'],
  ['- <span class="s">status == 200</span>'],
  ['- <span class="s">header.content-type contains json</span>'],
  ['- <span class="s">json.length == 5</span>'],
  ['- <span class="s">json[0].id exists</span>'],
];

function filesMock() {
  const tree = [
    ["dir", "posts/"],
    ["file", "_folder.yaml"],
    ["file on", "list-posts.yaml"],
    ["file", "get-a-post.yaml"],
    ["file", "create-a-post.yaml"],
    ["dir", "environments/"],
    ["file", "public.yaml"],
    ["file ignored", "public.local.yaml"],
  ];
  return `<div class="mock files">
    <ul class="tree">${tree
      .map(
        ([kind, name]) =>
          `<li class="${kind}">${icon(kind === "dir" ? "folder" : "file", 16)}<span>${name}</span>${kind.includes("ignored") ? `<span class="lock" title="git-ignored">${icon("lock", 14)}</span>` : ""}</li>`,
      )
      .join("")}</ul>
    <div class="editor">
      <p class="editor-path">posts/list-posts.yaml</p>
      <ol class="lines stagger">${yaml.map(([line], i) => `<li style="--i:${i}"><span>${line}</span></li>`).join("")}</ol>
    </div>
  </div>`;
}

const protocols = [
  {
    tab: "HTTP/3",
    kind: "GET",
    cls: "m-get",
    action: "Send",
    url: "https://api.example.com/posts",
    out: '<span class="ok">200 OK</span> · 41 ms · HTTP/3',
  },
  {
    tab: "gRPC",
    kind: "gRPC",
    cls: "m-other",
    action: "Invoke",
    url: "localhost:50051",
    out: 'blog.v1.Posts/ListPosts · <span class="ok">OK</span> · 3 messages',
  },
  {
    tab: "WebSocket",
    kind: "WebSocket",
    cls: "m-other",
    action: "Connect",
    url: "wss://example.com/socket",
    out: '<span class="up">↑</span> {"type":"ping"} <span class="down">↓</span> {"type":"pong"}',
  },
  {
    tab: "SSE",
    kind: "SSE",
    cls: "m-other",
    action: "Connect",
    url: "https://api.example.com/events",
    out: '<span class="down">↓</span> event: tick · data: 42',
  },
  {
    tab: "Socket.IO",
    kind: "Socket.IO",
    cls: "m-other",
    action: "Connect",
    url: "https://example.com/chat",
    out: '<span class="up">↑</span> message <span class="down">↓</span> ack: ok',
  },
];

function protocolsMock() {
  return `<div class="mock protocols">
    <div class="segmented" role="presentation">${protocols
      .map((p, i) => `<span class="seg${i === 0 ? " on" : ""}">${p.tab}</span>`)
      .join("")}</div>
    <div class="panels">${protocols
      .map(
        (p, i) => `<div class="panel${i === 0 ? " on" : ""}">
        <p class="panel-name">${p.tab}</p>
        <div class="request-line"><span class="kind ${p.cls}">${p.kind}</span><span class="url">${breakable(p.url)}</span><span class="send">${icon("play", 14)} ${p.action}</span></div>
        <p class="answer">${p.out}</p>
      </div>`,
      )
      .join("")}</div>
  </div>`;
}

function checksMock() {
  const checks = [
    "status == 200",
    "header.content-type contains json",
    "json.length == 5",
    "json[0].id exists",
  ];
  return `<div class="mock checks">
    <ul class="stagger">${checks
      .map(
        (c, i) =>
          `<li style="--i:${i}"><span class="tick">${icon("check", 14)}</span><code>${c}</code></li>`,
      )
      .join("")}</ul>
    <p class="total"><span class="count" data-to="4">4</span>/4 passed</p>
  </div>`;
}

function runMock() {
  const out = [
    '<span class="ok">✓</span> GET  List posts     <span class="ok">200 OK</span> 98 ms',
    '<span class="ok">✓</span> GET  Get a post     <span class="ok">200 OK</span> 26 ms',
    '<span class="ok">✓</span> POST Create a post  <span class="ok">201 Created</span>',
    '<span class="dim">Checks    6 passed · 0 failed</span>',
    '<span class="ok">All passed.</span>',
  ];
  return `<div class="mock terminal">
    <p class="prompt"><span class="dollar">$</span> <span class="typed" data-text="getman run posts -e public">getman run posts -e public</span></p>
    <ol class="stagger">${out.map((line, i) => `<li style="--i:${i}">${line}</li>`).join("")}</ol>
  </div>`;
}

function importMock() {
  return `<div class="mock terminal import">
    <p class="prompt"><span class="dollar">$</span> <span class="typed" data-text="getman import shop.postman_collection.json">getman import shop.postman_collection.json</span></p>
    <div class="ticks" aria-hidden="true">${Array.from({ length: 16 }, (_, i) => `<i style="--i:${i}"></i>`).join("")}</div>
    <p class="result">Imported Shop API: <span class="count" data-to="3">3</span> requests in 2 folders</p>
    <p class="script"><code>pm.test('ok', …)</code><span class="chip">runs as is</span></p>
  </div>`;
}

const features = [
  {
    size: "wide",
    demo: "files",
    mock: filesMock(),
    title: "Your requests are files",
    text: "One YAML file per request, reviewed in pull requests and versioned with your code. Tokens and personal values stay in git-ignored <code>*.local.yaml</code> files.",
  },
  {
    size: "wide",
    demo: "protocols",
    mock: protocolsMock(),
    title: "Every protocol you test",
    text: "HTTP/1.1, HTTP/2 and HTTP/3, gRPC from <code>.proto</code> files or server reflection, WebSocket, Server-Sent Events and Socket.IO, side by side in one workspace.",
  },
  {
    size: "narrow",
    demo: "checks",
    mock: checksMock(),
    title: "Tests without code",
    text: "Checks are lines in the request file: a target, an operator and a value. Scripts are there when you need them.",
  },
  {
    size: "narrow",
    demo: "run",
    mock: runMock(),
    title: "The same run in CI",
    text: "<code>getman run</code> sends a folder or the whole workspace and writes JUnit, HTML or JSON reports.",
  },
  {
    size: "narrow",
    demo: "import",
    mock: importMock(),
    title: "Bring your Postman work",
    text: "Import Postman collections and environments. <code>pm</code> is an alias of Getman's script API, so most scripts run unchanged.",
  },
];

// Measured on Windows 11 x64 with the 0.3.0 release files (README, "Fast").
const facts = [
  { value: 7, unit: "ms", label: "to start the command" },
  { value: 9.5, unit: "MiB", label: "download, browser UI included" },
  { value: 15.2, unit: "MiB", label: "desktop app setup" },
  { value: 0, unit: "accounts", label: "no sign-in, no cloud, no telemetry" },
];

export function landing({ root, assets, site, repo, pages }) {
  const title = "Getman: fast, open-source Postman alternative in Rust";
  const description =
    "Getman is a local-first, offline API client with HTTP/2, HTTP/3, gRPC and WebSocket, compatible with Postman, for the browser, desktop, terminal and CI, with no account, no cloud and no telemetry.";
  const app = {
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    name: "Getman",
    description,
    url: site,
    applicationCategory: "DeveloperApplication",
    operatingSystem: "Windows, macOS, Linux",
    offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
    license: [`${repo}/blob/main/LICENSE-MIT`, `${repo}/blob/main/LICENSE-APACHE`],
    downloadUrl: `${repo}/releases/latest`,
    screenshot: `${site}images/getman-dark.png`,
    image: `${site}favicon.svg`,
    sameAs: repo,
  };
  const jsonLd = `<script type="application/ld+json">${JSON.stringify(app).replace(/</g, "\\u003c")}</script>`;

  const buttons = systems
    .map(
      ({ system, name, file }) =>
        `<a class="btn filled large" href="${repo}/releases/latest/download/${file}" data-for="${system}">${icon("download")}<span>Download for ${name}</span></a>`,
    )
    .join("");
  const files = systems
    .map(({ system, file }) => `<code data-for="${system}">${file}</code>`)
    .join("");
  // The desktop guide is linked once the build has it.
  const guide = pages.includes("desktop")
    ? `<a href="${root}docs/desktop.html">Desktop app guide</a>`
    : "";
  const download = `<div class="download">
      ${buttons}
      <a class="btn filled large" href="${repo}/releases/latest" data-for="other">${icon("download")}<span>Download</span></a>
      <p class="download-note">
        ${files}<span data-for="other">For Windows, macOS and Linux</span>
        <span class="download-links"><a href="${repo}/releases" data-for="windows macos linux">Other downloads</a>${guide}</span>
      </p>
    </div>`;

  return `${head({ root, assets, title, description, canonical: site, site, home: true, extra: jsonLd })}
<body class="home">
${bar({ root, current: "" })}
<main id="content">

<section class="hero">
  <div class="hero-copy">
    <p class="eyebrow">Open source · No account · No cloud</p>
    <h1>Getman, the fast <span class="accent">open-source Postman alternative</span></h1>
    <p class="lead">Send, test and run your API requests from plain YAML files in your repository. On the desktop, in the browser, in the terminal and in CI.</p>
    ${download}
    <div class="cli">
      <div class="command" role="group" aria-label="Install the command-line tool">
        <span class="command-label">CLI</span>
        ${commands.map(({ systems, text }) => `<code data-for="${systems}" data-command>${breakable(text)}</code>`).join("")}
        <button class="btn text" type="button" data-copy-command>${icon("copy")}<span>Copy</span></button>
      </div>
      <a class="btn" href="${root}docs/getting-started.html"><span>Read the docs</span>${icon("right")}</a>
    </div>
  </div>
  <div class="hero-art">${orbitMark()}</div>
</section>

<figure class="shot">
  <img class="light" src="${root}images/getman-light.png" width="1280" height="800" fetchpriority="high" decoding="async" alt="Getman with a request's four checks beside its response: 200 OK in green, with the time, size and 4 of 4 tests passed">
  <img class="dark" src="${root}images/getman-dark.png" width="1280" height="800" decoding="async" alt="Getman in the dark theme, with a request's four checks beside its response: 200 OK in green, with the time, size and 4 of 4 tests passed">
</figure>

<section class="features" aria-labelledby="features">
  <h2 id="features">An API client that lives with your code</h2>
  <div class="bento">${features
    .map(
      (f) => `
    <article class="card ${f.size}" data-demo="${f.demo}">
      ${f.mock}
      <div class="card-text">
        <h3>${f.title}</h3>
        <p>${f.text}</p>
      </div>
    </article>`,
    )
    .join("")}
  </div>
  <p class="more"><a href="${root}docs/why-getman.html">How Getman compares with Postman</a> · <a href="${root}docs/compare.html">and with other API clients</a></p>
</section>

<section class="closing" aria-labelledby="closing">
  <h2 id="closing">Small, quick and yours</h2>
  <dl class="facts">${facts
    .map(
      (f) =>
        `<div><dt>${f.label}</dt><dd><span class="count" data-to="${f.value}">${f.value}</span> <span class="unit">${f.unit}</span></dd></div>`,
    )
    .join("")}</dl>
  <p class="facts-note">Measured on Windows 11 x64 with the 0.3.0 release files. <a href="${repo}#fast">How it was measured</a></p>
  <div class="closing-get">
    ${download}
    <p class="licence">Free and open source under MIT or Apache-2.0.</p>
  </div>
</section>

</main>
${footer({ root })}
${palette()}
</body>
</html>
`;
}
