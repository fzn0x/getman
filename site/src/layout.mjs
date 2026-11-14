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

const ld = (data) =>
  `<script type="application/ld+json">${JSON.stringify(data).replace(/</g, "\\u003c")}</script>`;

/** The logo mark: a G whose bowl runs into a forward tip. */
export function mark(width = 24, className = "logo") {
  const height = Math.round((width * 36) / 40);
  return `<svg class="${className}" viewBox="4 6 40 36" width="${width}" height="${height}" aria-hidden="true"><path d="M32.42 12.33L27.82 16.93A10 10 0 1 0 27.82 31.07L31.64 27.25L19.5 27.25L26 20.75L40.83 20.75L42.95 22.87A1.6 1.6 0 0 1 42.95 25.13L32.42 35.67A16.5 16.5 0 1 1 32.42 12.33Z"/></svg>`;
}

/**
 * The logo mark circled by what Getman does: tools on the inner ring,
 * HTTP methods on the outer one, turning the other way. The same picture
 * as the app's 401 page; site.css draws the orbits.
 */
export function orbitMark() {
  const ring = (items, cls) =>
    items
      .map(
        (item, i) =>
          `<span class="orbiter ${cls}" style="--angle:${(360 / items.length) * i}"><span class="slot">${item}</span></span>`,
      )
      .join("");
  const tools = ["globe", "terminal", "code", "check", "folder", "play"].map(
    (name) => `<span class="tool">${icon(name, 18)}</span>`,
  );
  const methods = ["GET", "POST", "PUT", "PATCH", "DELETE"].map(
    (m) => `<span class="method m-${m.toLowerCase()}">${m}</span>`,
  );
  return `<div class="orbit-mark" aria-hidden="true">
    <svg class="rings"><circle class="inner" cx="50%" cy="50%"/><circle class="outer" cx="50%" cy="50%"/></svg>
    <div class="core">${mark(52, "core-logo")}</div>
    ${ring(tools, "in")}${ring(methods, "out")}
  </div>`;
}

/**
 * The page's head. `home` takes `title` as the whole title; `extra` is
 * markup for the end of the head, such as structured data. `lang` is the
 * language of the page's text, for a translated guide, and `alternates`
 * lists the page in every language it exists in.
 */
export function head({
  root,
  assets,
  title,
  description,
  canonical,
  site,
  home = false,
  extra = "",
  lang = "en",
  alternates = [],
}) {
  const full = home ? title : `${title} · Getman`;
  const image = `${site}images/getman-dark.png`;
  const alt =
    "Getman in the dark theme, with a request's four checks beside its response: 200 OK in green, with 4 of 4 tests passed";
  const locale = lang === "id" ? "id_ID" : "en_US";
  const hreflang = alternates.length
    ? [
        ...alternates.map((a) => `<link rel="alternate" hreflang="${a.lang}" href="${a.href}">`),
        `<link rel="alternate" hreflang="x-default" href="${alternates.find((a) => a.lang === "en")?.href ?? canonical}">`,
      ].join("\n") + "\n"
    : "";
  return `<!doctype html>
<html lang="${escape(lang)}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escape(full)}</title>
<meta name="description" content="${escape(description)}">
<link rel="canonical" href="${canonical}">
${hreflang}<meta property="og:type" content="${home ? "website" : "article"}">
<meta property="og:site_name" content="Getman">
<meta property="og:locale" content="${locale}">
<meta property="og:title" content="${escape(full)}">
<meta property="og:description" content="${escape(description)}">
<meta property="og:url" content="${canonical}">
<meta property="og:image" content="${image}">
<meta property="og:image:width" content="2560">
<meta property="og:image:height" content="1600">
<meta property="og:image:alt" content="${escape(alt)}">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="${escape(full)}">
<meta name="twitter:description" content="${escape(description)}">
<meta name="twitter:image" content="${image}">
<meta name="twitter:image:alt" content="${escape(alt)}">
<meta name="theme-color" content="#fbf8fe" media="(prefers-color-scheme: light)">
<meta name="theme-color" content="#131317" media="(prefers-color-scheme: dark)">
<link rel="icon" href="${root}favicon.svg" type="image/svg+xml">
<link rel="preload" href="${root}fonts/GoogleSansFlex.woff2" as="font" type="font/woff2" crossorigin>
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
${extra}</head>`;
}

export function bar({ root, current }) {
  const link = (href, label, name) =>
    `<a href="${href}"${current === name ? ' aria-current="page"' : ""}>${label}</a>`;
  return `<a class="skip" href="#content">Skip to content</a>
<header class="bar">
  <a class="brand" href="${root}" aria-label="Getman home">${mark(24)}<span class="word">Getman</span></a>
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
    <p class="foot-mark">${mark(20)}<span class="word">Getman</span></p>
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
      (section) => `<p class="nav-label">${section.title}</p>
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
        <p class="nav-label">On this page</p>
        <ol>${p.headings.map((h) => `<li><a href="#${h.id}">${escape(h.text)}</a></li>`).join("")}</ol>
      </aside>`
    : "";
  const pager = [
    p.previous
      ? `<a class="prev" href="${p.previous.href}" rel="prev"><span class="pager-label">${icon("left", 16)} Previous</span><span class="pager-title">${escape(p.previous.title)}</span></a>`
      : "<span></span>",
    p.next
      ? `<a class="next" href="${p.next.href}" rel="next"><span class="pager-label">Next ${icon("right", 16)}</span><span class="pager-title">${escape(p.next.title)}</span></a>`
      : "<span></span>",
  ].join("");
  // Search engines read the trail and the article from structured data.
  const docsUrl = `${p.site}docs/`;
  const trail = [
    { name: "Getman", item: p.site },
    { name: "Documentation", item: docsUrl },
    ...(p.canonical === docsUrl ? [] : [{ name: p.title, item: p.canonical }]),
  ];
  const data = ld([
    {
      "@context": "https://schema.org",
      "@type": "TechArticle",
      headline: p.title,
      description: p.description,
      url: p.canonical,
      inLanguage: p.lang ?? "en",
      image: `${p.site}images/getman-dark.png`,
      author: { "@type": "Organization", name: "The Getman contributors", url: repo },
      isPartOf: { "@type": "WebSite", name: "Getman", url: p.site },
    },
    {
      "@context": "https://schema.org",
      "@type": "BreadcrumbList",
      itemListElement: trail.map((t, i) => ({
        "@type": "ListItem",
        position: i + 1,
        name: t.name,
        item: t.item,
      })),
    },
  ]);
  // Copy page sits under the title and the paragraph that leads the guide.
  const actions = `<p class="doc-actions"><button class="btn small" type="button" data-copy-page="${p.markdown}">${icon("copy", 16)}<span>Copy page</span></button></p>`;
  const body = p.body.replace(
    /^(\s*<h1>[\s\S]*?<\/h1>\s*(?:<p>[\s\S]*?<\/p>)?)/,
    (lead) => `${lead}\n${actions}`,
  );
  return `${head({ ...p, extra: data })}
<body class="doc-page">
${bar({ root: p.root, current: p.title === "Why Getman" ? "why" : "docs" })}
<div class="docs">
  <nav class="sidebar" aria-label="Documentation">
    <button class="sidebar-toggle" type="button" aria-expanded="false" aria-controls="sidebar-list">${icon("menu")}<span class="sidebar-label">Guides</span><span class="sidebar-current">${escape(p.title)}</span></button>
    <div class="sidebar-list" id="sidebar-list">
      <p class="nav-home"><a href="index.html"${p.title === "Documentation" ? ' aria-current="page"' : ""}>Overview</a></p>
      ${nav}
    </div>
  </nav>
  <main id="content" class="doc">
    <article class="prose">
${body}
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
  ${orbitMark()}
  <p class="lost-code">Error 404</p>
  <h1>Not found</h1>
  <p class="lost-text">There is no page at this address. It may have moved when the documentation was reorganised.</p>
  <p class="lost-links"><a class="btn filled" href="${root}">Getman home</a><a class="btn" href="${root}docs/">Documentation</a><button class="btn" type="button" data-palette>${icon("search")} Search</button></p>
</main>
${footer({ root })}
${palette()}
</body>
</html>
`;
}
