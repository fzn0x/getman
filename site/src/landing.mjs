// The landing page: the name and what it is, the download and the install
// command for the visitor's system, one screenshot and a few facts.

import { bar, head, palette } from "./layout.mjs";
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

const facts = [
  ["Files", "Plain YAML you commit, with secrets kept in git-ignored files."],
  ["Protocols", "HTTP/1.1, HTTP/2 and HTTP/3, WebSocket, SSE, Socket.IO and gRPC."],
  [
    "Tests",
    "Checks without code and <code>gm</code> scripts, with JUnit, HTML and JSON reports in CI.",
  ],
  ["Postman", "Opens Postman collections and environments, and runs <code>pm</code> scripts."],
  ["Privacy", "No account, no cloud, no telemetry."],
];

const escape = (text) =>
  String(text).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** A command that may wrap on narrow screens, but only after a slash or a space. */
function breakable(command) {
  return escape(command).replace(/\//g, "/<wbr>");
}

export function landing({ root, assets, site, repo, pages }) {
  const description =
    "Getman is a local-first API client whose requests are files in your repository, for the desktop, browser, terminal and CI. No account, no cloud, no telemetry.";

  const buttons = systems
    .map(
      ({ system, name, file }) =>
        `<a class="btn primary" href="${repo}/releases/latest/download/${file}" data-for="${system}">${icon("download")}<span>Download for ${name}</span></a>`,
    )
    .join("");
  const files = systems
    .map(({ system, file }) => `<code data-for="${system}">${file}</code>`)
    .join("");
  // The desktop guide is linked once the build has it.
  const guide = pages.includes("desktop")
    ? `<a href="${root}docs/desktop.html">Desktop app guide</a>`
    : "";

  return `${head({ root, assets, title: "Getman", description, canonical: site, site })}
<body class="home">
${bar({ root, current: "" })}
<main id="content">

<header class="intro">
  <h1 class="bitmap">Getman</h1>
  <p class="lead">A local-first API client whose requests are files in your repository. <span>An open-source Postman alternative for desktop, browser, terminal and CI.</span></p>
</header>

<div class="get">
  <div class="download">
    ${buttons}
    <a class="btn primary" href="${repo}/releases/latest" data-for="other">${icon("download")}<span>Download</span></a>
    <p class="download-note">
      ${files}<span data-for="other">For Windows, macOS and Linux</span>
      <span class="download-links"><a href="${repo}/releases" data-for="windows macos linux">Other downloads</a>${guide}</span>
    </p>
  </div>
  <div class="cli">
    <div class="command" role="group" aria-label="Install the command-line tool">
      <span class="command-label bitmap">CLI</span>
      ${commands.map(({ systems, text }) => `<code data-for="${systems}" data-command>${breakable(text)}</code>`).join("")}
      <button class="btn" type="button" data-copy-command>${icon("copy")}<span>Copy</span></button>
    </div>
    <a class="btn" href="${root}docs/getting-started.html">Docs ${icon("right", 7)}</a>
  </div>
</div>

<figure class="shot">
  <img class="light" src="${root}images/getman-light.png" width="1280" height="800" loading="lazy" decoding="async" alt="Getman with a request's four checks beside its response: a 200 on a green halftone field, with time, size and 4 of 4 tests passed">
  <img class="dark" src="${root}images/getman-dark.png" width="1280" height="800" loading="lazy" decoding="async" alt="Getman in the dark theme, with a request's four checks beside its response: a 200 on a green halftone field, with time, size and 4 of 4 tests passed">
</figure>

<section class="facts" aria-labelledby="facts">
  <h2 id="facts" class="sr-only">At a glance</h2>
  <dl>${facts.map(([term, line]) => `<div><dt class="bitmap">${term}</dt><dd>${line}</dd></div>`).join("")}</dl>
</section>

</main>
<footer class="foot">
  <div class="foot-inner">
    <nav class="foot-links" aria-label="More">
      <a href="${repo}">GitHub</a>
      <a href="${root}docs/">Docs</a>
      <a href="${repo}#license">License: MIT or Apache-2.0</a>
    </nav>
    <p class="foot-note">Postman is a trademark of Postman, Inc. Getman is not affiliated with or endorsed by Postman.</p>
  </div>
</footer>
${palette()}
</body>
</html>
`;
}
