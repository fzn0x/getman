// Builds the documentation site into dist/: a landing page, one page per
// file in ../docs, a search index and the install scripts. Everything is
// static and self-hosted, so the site makes no third-party requests.

import { createHash } from "node:crypto";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Marked } from "marked";
import { createCssVariablesTheme, createHighlighter } from "shiki";
import { landing } from "./src/landing.mjs";
import { page, notFound } from "./src/layout.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const docsDir = join(here, "..", "docs");
const out = join(here, "dist");
const site = "https://fzn0x.github.io/getman/";
const repo = "https://github.com/fzn0x/getman";

// The order of the sidebar, and of previous and next.
export const sections = [
  { title: "Start", pages: ["getting-started", "why-getman"] },
  {
    title: "Guides",
    pages: [
      "workspaces",
      "variables",
      "checks",
      "scripts",
      "http",
      "runner",
      "examples-and-mocks",
      "mcp",
      "import-export",
      "browser-ui",
      "terminal",
    ],
  },
  { title: "Reference", pages: ["cli", "security"] },
];

const theme = createCssVariablesTheme({
  name: "getman",
  variablePrefix: "--sh-",
  variableDefaults: {},
  fontStyle: true,
});
const languages = ["yaml", "sh", "powershell", "js", "json", "csv", "python", "xml", "toml"];
const highlighter = await createHighlighter({ themes: [theme], langs: languages });
const aliases = {
  bash: "sh",
  shell: "sh",
  console: "sh",
  javascript: "js",
  yml: "yaml",
  ps1: "powershell",
};

const escape = (text) =>
  text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** Highlights code and marks `{{variables}}` the way the app does. */
export function highlight(code, lang) {
  const language = aliases[lang] ?? lang;
  const html = languages.includes(language)
    ? highlighter.codeToHtml(code, { lang: language, theme: "getman" })
    : `<pre class="shiki"><code>${escape(code)}</code></pre>`;
  return html
    .replace(/ style="background-color:var\(--sh-background\);color:var\(--sh-foreground\)"/, "")
    .replace(/ tabindex="0"/, "")
    .replace(/\{\{([^{}<]+)\}\}/g, '<span class="v">{{$1}}</span>');
}

const LABELS = {
  sh: "Shell",
  powershell: "PowerShell",
  yaml: "YAML",
  js: "JavaScript",
  json: "JSON",
  csv: "CSV",
  text: "Output",
  terminal: "Terminal",
  python: "Python",
  xml: "XML",
};

/**
 * A code block with its language and a copy button. With `wrap`, long
 * lines wrap under a hanging indent instead of scrolling, for recordings
 * whose last words are the point.
 */
export function codeBlock(code, lang = "text", { wrap = false } = {}) {
  const language = aliases[lang] ?? lang;
  const label = LABELS[language] ?? language.toUpperCase();
  const text = code.replace(/\n$/, "");
  const body = wrap
    ? `<pre class="shiki"><code>${text
        .split("\n")
        .map((line) => `<span class="line">${escape(line) || " "}</span>`)
        .join("")
        .replace(/\{\{([^{}<]+)\}\}/g, '<span class="v">{{$1}}</span>')}</code></pre>`
    : highlight(text, language);
  return `<figure class="code${wrap ? " wrap" : ""}"><figcaption><span>${label}</span><button class="copy" type="button" data-copy aria-label="Copy the ${label} code">Copy</button></figcaption>${body}</figure>`;
}

export function slug(text) {
  return text
    .toLowerCase()
    .replace(/<[^>]+>/g, "")
    .replace(/&[a-z]+;/g, "")
    .replace(/[^a-z0-9 -]/g, "")
    .trim()
    .replace(/\s+/g, "-");
}

function render(markdown) {
  const headings = [];
  const used = new Set();
  const marked = new Marked({
    gfm: true,
    renderer: {
      code({ text, lang }) {
        return codeBlock(text, (lang || "text").split(/\s/)[0]);
      },
      heading({ tokens, depth, text }) {
        const html = this.parser.parseInline(tokens);
        if (depth === 1) return `<h1>${html}</h1>\n`;
        let id = slug(text) || "section";
        while (used.has(id)) id += "-";
        used.add(id);
        if (depth <= 3) headings.push({ depth, id, text: html.replace(/<[^>]+>/g, "") });
        return `<h${depth} id="${id}"><a class="anchor" href="#${id}" aria-hidden="true" tabindex="-1"></a>${html}</h${depth}>\n`;
      },
      link({ href, title, tokens }) {
        const text = this.parser.parseInline(tokens);
        let target = href;
        const external = /^https?:/.test(href);
        if (!external) {
          // Pages link to each other as .md files; the site serves .html.
          target = href.replace(/^README\.md/, "index.html").replace(/\.md(#|$)/, ".html$1");
        }
        const rel = external ? ' rel="noopener"' : "";
        return `<a href="${escape(target)}"${title ? ` title="${escape(title)}"` : ""}${rel}>${text}</a>`;
      },
    },
  });
  // Wide tables scroll inside their own box rather than the page.
  const html = marked
    .parse(markdown)
    .replace(/<table>/g, '<div class="table"><table>')
    .replace(/<\/table>/g, "</table></div>");
  return { html, headings };
}

/** The first paragraph, as plain text, for the page description. */
function describe(markdown) {
  const paragraph = markdown
    .split(/\n\s*\n/)
    .map((block) => block.trim())
    .find((block) => block && !/^(#|```|\||-|\*|<|\d+\.)/.test(block));
  return (paragraph ?? "")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/[`*_]/g, "")
    .replace(/<[^>]+>/g, "")
    .replace(/\s+/g, " ")
    .slice(0, 200);
}

function stamp(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex").slice(0, 10);
}

// Start clean.
rmSync(out, { recursive: true, force: true });
mkdirSync(join(out, "docs"), { recursive: true });

// Assets shared with the app, and the site's own.
cpSync(join(here, "static"), out, { recursive: true });
cpSync(join(here, "..", "web", "public", "fonts"), join(out, "fonts"), { recursive: true });
cpSync(join(here, "..", "web", "public", "favicon.svg"), join(out, "favicon.svg"));
cpSync(join(docsDir, "images"), join(out, "images"), { recursive: true });
cpSync(join(here, "src", "site.css"), join(out, "site.css"));
cpSync(join(here, "src", "site.js"), join(out, "site.js"));
const assets = {
  css: `site.css?v=${stamp(join(out, "site.css"))}`,
  js: `site.js?v=${stamp(join(out, "site.js"))}`,
};

// Documentation pages.
const order = sections.flatMap((section) => section.pages);
const files = readdirSync(docsDir).filter((name) => name.endsWith(".md"));
for (const name of order) {
  if (!files.includes(`${name}.md`))
    throw new Error(`docs/${name}.md is in the sidebar but does not exist`);
}
for (const file of files) {
  const name = file.replace(/\.md$/, "");
  if (name !== "README" && !order.includes(name))
    throw new Error(`docs/${file} is missing from the sidebar`);
}

const index = [];
const docs = {};
for (const name of ["README", ...order]) {
  const markdown = readFileSync(join(docsDir, `${name}.md`), "utf8");
  const title = (markdown.match(/^# (.+)$/m)?.[1] ?? name).trim();
  const { html, headings } = render(markdown);
  docs[name] = { name, title, html, headings, description: describe(markdown) };
}

const titleOf = (name) => (name === "README" ? "Documentation" : docs[name].title);
for (const name of ["README", ...order]) {
  const doc = docs[name];
  const at = order.indexOf(name);
  const file = name === "README" ? "index.html" : `${name}.html`;
  const previous = at > 0 ? { href: `${order[at - 1]}.html`, title: titleOf(order[at - 1]) } : null;
  const next =
    at < order.length - 1 ? { href: `${order[at + 1]}.html`, title: titleOf(order[at + 1]) } : null;
  const html = page({
    root: "../",
    assets,
    site,
    sections: sections.map((section) => ({
      title: section.title,
      pages: section.pages.map((p) => ({
        href: `${p}.html`,
        title: titleOf(p),
        current: p === name,
      })),
    })),
    title: titleOf(name),
    description: doc.description,
    canonical: `${site}docs/${file === "index.html" ? "" : file}`,
    body: doc.html,
    headings: doc.headings.filter((h) => h.depth === 2),
    previous,
    next: name === "README" ? { href: `${order[0]}.html`, title: titleOf(order[0]) } : next,
    edit: `${repo}/edit/main/docs/${name}.md`,
  });
  writeFileSync(join(out, "docs", file), html);
  index.push({ title: titleOf(name), url: `docs/${file}`, kind: "page" });
  for (const heading of doc.headings) {
    index.push({ title: heading.text, url: `docs/${file}#${heading.id}`, kind: titleOf(name) });
  }
}

// The landing page and the rest.
writeFileSync(join(out, "index.html"), landing({ root: "./", assets, site, repo, pages: order }));
writeFileSync(join(out, "404.html"), notFound({ root: site, assets, site }));
writeFileSync(join(out, "search.json"), JSON.stringify(index));
writeFileSync(
  join(out, "sitemap.xml"),
  `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${[
    site,
    `${site}docs/`,
    ...order.map((name) => `${site}docs/${name}.html`),
  ]
    .map((url) => `  <url><loc>${url}</loc></url>`)
    .join("\n")}\n</urlset>\n`,
);
writeFileSync(join(out, "robots.txt"), `User-agent: *\nAllow: /\nSitemap: ${site}sitemap.xml\n`);

if (!existsSync(join(out, ".nojekyll"))) writeFileSync(join(out, ".nojekyll"), "");

// Every link between pages must land on a page, and every #anchor on a
// heading, so a renamed page or heading fails the build instead of a reader.
const broken = [];
const idsOf = new Map();
const ids = (file) => {
  if (!idsOf.has(file)) {
    const html = readFileSync(file, "utf8");
    idsOf.set(file, new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1])));
  }
  return idsOf.get(file);
};
const pages = [
  join(out, "index.html"),
  ...readdirSync(join(out, "docs")).map((name) => join(out, "docs", name)),
];
for (const from of pages) {
  const html = readFileSync(from, "utf8");
  for (const [, href] of html.matchAll(/\shref="([^"]+)"/g)) {
    if (/^(https?:|mailto:)/.test(href)) continue;
    const [path, anchor] = href.replace(/\?.*?(#|$)/, "$1").split("#");
    let target = path ? join(dirname(from), path) : from;
    if (path.endsWith("/")) target = join(target, "index.html");
    if (!existsSync(target)) {
      broken.push(`${from.slice(out.length + 1)}: ${href} does not exist`);
    } else if (anchor && target.endsWith(".html") && !ids(target).has(anchor)) {
      broken.push(`${from.slice(out.length + 1)}: ${href} has no #${anchor}`);
    }
  }
}
if (broken.length) {
  console.error(`Broken links:\n  ${broken.join("\n  ")}`);
  process.exit(1);
}

console.log(`Built ${order.length + 2} pages into ${out}`);
