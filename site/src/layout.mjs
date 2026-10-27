// The frame every page shares: head, top bar, search palette and footer,
// and the documentation page around a rendered guide.

import { icon } from "./icons.mjs";

const repo = "https://github.com/fzn0x/getman";

const escape = (text) =>
  String(text)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");

export function head({ root, assets, title, description, canonical, site }) {
  const full = title === "Getman" ? "Getman: a local-first API client" : `${title} · Getman`;
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escape(full)}</title>
<meta name="description" content="${escape(description)}">
<link rel="canonical" href="${canonical}">
<meta property="og:type" content="website">
<meta property="og:title" content="${escape(full)}">
<meta property="og:description" content="${escape(description)}">
<meta property="og:url" content="${canonical}">
<meta property="og:image" content="${site}images/getman-light.png">
<meta name="twitter:card" content="summary_large_image">
<meta name="theme-color" content="#f2f0eb" media="(prefers-color-scheme: light)">
<meta name="theme-color" content="#0e0e0d" media="(prefers-color-scheme: dark)">
<link rel="icon" href="${root}favicon.svg" type="image/svg+xml">
<link rel="preload" href="${root}fonts/DepartureMono-Regular.woff2" as="font" type="font/woff2" crossorigin>
<link rel="stylesheet" href="${root}${assets.css}">
<script>
  // The saved theme, before the page paints.
  try {
    var theme = localStorage.getItem("getman.theme");
    if (theme === "light" || theme === "dark") document.documentElement.dataset.theme = theme;
  } catch (e) {}
  document.documentElement.classList.add("js");
  // The visitor's system, so downloads and install commands match it.
  var platform = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
  // Phones, tablets (iPadOS says Mac) and ChromeOS (which says Linux) get the general links.
  var other = /Android|iPhone|iPad|iPod|CrOS/.test(navigator.userAgent) || (/Mac/.test(platform) && navigator.maxTouchPoints > 1);
  document.documentElement.dataset.system = other ? "other"
    : /Win/.test(platform) ? "windows"
    : /Mac/.test(platform) ? "macos"
    : /Linux/.test(platform) ? "linux"
    : "other";
</script>
<script src="${root}${assets.js}" defer></script>
</head>`;
}

export function bar({ root, current }) {
  const link = (href, label, name) =>
    `<a href="${href}"${current === name ? ' aria-current="page"' : ""}>${label}</a>`;
  return `<a class="skip" href="#content">Skip to content</a>
<header class="bar">
  <a class="mark" href="${root}" aria-label="Getman home"><span class="square" aria-hidden="true"></span><span class="bitmap">Getman</span></a>
  <nav class="links" aria-label="Site">
    ${link(`${root}docs/getting-started.html`, "Docs", "docs")}
    ${link(`${root}docs/why-getman.html`, "Why Getman", "why")}
    ${link(repo, "GitHub", "github")}
  </nav>
  <button class="find" type="button" data-palette aria-haspopup="dialog">
    ${icon("search")}<span class="find-label">Search the docs</span><span class="keys"><kbd data-mod>Ctrl</kbd><kbd>K</kbd></span>
  </button>
  <button class="icon-button" type="button" data-theme-toggle aria-label="Colour theme: follows your system">${icon("contrast")}</button>
</header>`;
}

export function palette() {
  return `<dialog class="palette" aria-label="Search the documentation">
  <div class="palette-input">
    ${icon("search")}
    <input type="text" role="combobox" aria-expanded="true" aria-controls="palette-results" aria-autocomplete="list" placeholder="Search pages and sections" autocomplete="off" spellcheck="false">
  </div>
  <ul id="palette-results" role="listbox" aria-label="Results"></ul>
  <p class="palette-foot"><span><kbd>↑</kbd><kbd>↓</kbd> move</span><span><kbd>Enter</kbd> open</span><span><kbd>Esc</kbd> close</span></p>
</dialog>`;
}

export function footer({ root }) {
  return `<footer class="foot">
  <div class="foot-inner">
    <p class="foot-mark"><span class="square" aria-hidden="true"></span><span class="bitmap">Getman</span></p>
    <p class="foot-line">A local-first API client. Free and open source under MIT or Apache-2.0.</p>
    <nav class="foot-links" aria-label="More">
      <a href="${root}docs/getting-started.html">Documentation</a>
      <a href="${repo}/releases">Releases</a>
      <a href="${repo}">Source code</a>
      <a href="${repo}/blob/main/CONTRIBUTING.md">Contributing</a>
      <a href="${repo}/blob/main/SECURITY.md">Report a vulnerability</a>
    </nav>
    <p class="foot-note">Postman is a trademark of Postman, Inc. Getman is not affiliated with or endorsed by Postman.</p>
  </div>
</footer>`;
}

export function page(p) {
  const nav = p.sections
    .map(
      (section) => `<p class="nav-label bitmap">${section.title}</p>
      <ul>${section.pages
        .map(
          (item) =>
            `<li><a href="${item.href}"${item.current ? ' aria-current="page"' : ""}>${escape(item.title)}</a></li>`,
        )
        .join("")}</ul>`,
    )
    .join("");
  const toc = p.headings.length
    ? `<aside class="toc" aria-label="On this page">
        <p class="nav-label bitmap">On this page</p>
        <ol>${p.headings.map((h) => `<li><a href="#${h.id}">${escape(h.text)}</a></li>`).join("")}</ol>
      </aside>`
    : "";
  const pager = [
    p.previous
      ? `<a class="prev" href="${p.previous.href}" rel="prev"><span class="bitmap">${icon("left", 7)} Previous</span><span class="pager-title">${escape(p.previous.title)}</span></a>`
      : "<span></span>",
    p.next
      ? `<a class="next" href="${p.next.href}" rel="next"><span class="bitmap">Next ${icon("right", 7)}</span><span class="pager-title">${escape(p.next.title)}</span></a>`
      : "<span></span>",
  ].join("");
  return `${head(p)}
<body class="doc-page">
${bar({ root: p.root, current: p.title === "Why Getman" ? "why" : "docs" })}
<div class="docs">
  <nav class="sidebar" aria-label="Documentation">
    <button class="sidebar-toggle" type="button" aria-expanded="false" aria-controls="sidebar-list">${icon("menu")}<span class="bitmap">Guides</span><span class="sidebar-current">${escape(p.title)}</span></button>
    <div class="sidebar-list" id="sidebar-list">
      <p class="nav-home"><a href="index.html"${p.title === "Documentation" ? ' aria-current="page"' : ""}>Overview</a></p>
      ${nav}
    </div>
  </nav>
  <main id="content" class="doc">
    <article class="prose">
${p.body}
    </article>
    <nav class="pager" aria-label="Previous and next">${pager}</nav>
    <p class="edit"><a href="${p.edit}" rel="noopener">Edit this page on GitHub</a></p>
  </main>
  ${toc}
</div>
${footer(p)}
${palette()}
</body>
</html>
`;
}

export function notFound({ root, assets, site }) {
  return `${head({ root, assets, title: "Not found", description: "This page does not exist.", canonical: site, site })}
<body class="lost-page">
${bar({ root, current: "" })}
<main id="content" class="lost">
  <div class="lost-plate">
    <svg class="halftone" aria-hidden="true" data-time="10000"></svg>
    <p class="status"><span class="numeral bitmap">404</span><span class="reason bitmap">Not found</span></p>
  </div>
  <p class="lost-text">There is no page at this address. It may have moved when the documentation was reorganised.</p>
  <p class="lost-links"><a class="btn" href="${root}">Getman home</a><a class="btn" href="${root}docs/">Documentation</a><button class="btn" type="button" data-palette>${icon("search")} Search</button></p>
</main>
${footer({ root })}
${palette()}
</body>
</html>
`;
}
