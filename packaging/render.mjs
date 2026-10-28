#!/usr/bin/env node
// Writes the package manifests for one release: a Homebrew formula, a Scoop
// manifest, winget manifests and a Flatpak manifest. Each is a template
// beside this script (a file ending in .in) with the release's version,
// download URLs and SHA-256 sums filled in from the files the release holds.
//
//   node packaging/render.mjs --tag v0.2.0 --repo fzn0x/getman --dist dist
//
//   --tag <tag>     the release tag, such as v0.2.0
//   --repo <repo>   the GitHub repository (default: fzn0x/getman)
//   --dist <dir>    the folder with the release files, to hash
//   --out <dir>     where to write the manifests (default: the --dist folder)
//   --date <date>   the release date, YYYY-MM-DD (default: today, UTC)
//
// In a template:
//
//   {{version}} {{tag}} {{repo}} {{date}}
//   {{url:FILE}}      the release's download URL for FILE
//   {{sha256:FILE}}   the SHA-256 of FILE in --dist, in lower case
//   {{SHA256:FILE}}   the same in upper case, as winget writes it
//   {{cliurl:TARGET}} {{clisha256:TARGET}}   the same for the command-line
//                     archive of a Rust target, such as aarch64-apple-darwin
//
// A file the templates name but --dist lacks is an error, so a release with a
// missing installer fails here instead of publishing a broken manifest.
// Nothing is uploaded anywhere; the release workflow attaches the output to
// the GitHub release.

import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const { values: options } = parseArgs({
  options: {
    tag: { type: "string" },
    repo: { type: "string", default: "fzn0x/getman" },
    dist: { type: "string" },
    out: { type: "string" },
    date: { type: "string" },
  },
});

if (!options.tag || !options.dist) {
  console.error("usage: render.mjs --tag v0.2.0 --dist dist [--repo owner/name] [--out dir] [--date YYYY-MM-DD]");
  process.exit(2);
}
if (!/^v\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(options.tag)) {
  console.error(`${options.tag} is not a version tag such as v0.2.0`);
  process.exit(2);
}

const here = dirname(fileURLToPath(import.meta.url));
const dist = resolve(options.dist);
const out = resolve(options.out ?? options.dist);
const tag = options.tag;
const version = tag.slice(1);
const date = options.date ?? new Date().toISOString().slice(0, 10);
const download = `https://github.com/${options.repo}/releases/download/${tag}`;

const hashes = new Map();
const sha256 = (file) => {
  if (!hashes.has(file)) {
    const path = join(dist, file);
    if (!existsSync(path)) throw new Error(`${file} is not in ${dist}`);
    hashes.set(file, createHash("sha256").update(readFileSync(path)).digest("hex"));
  }
  return hashes.get(file);
};
const cliArchive = (target) => `getman-${tag}-${target}${target.includes("windows") ? ".zip" : ".tar.gz"}`;

function render(text) {
  return text.replace(/\{\{(\w+)(?::([^}]+))?\}\}/g, (whole, name, argument) => {
    switch (name) {
      case "version": return version;
      case "tag": return tag;
      case "repo": return options.repo;
      case "date": return date;
      case "url": return `${download}/${argument}`;
      case "sha256": return sha256(argument);
      case "SHA256": return sha256(argument).toUpperCase();
      case "cliurl": return `${download}/${cliArchive(argument)}`;
      case "clisha256": return sha256(cliArchive(argument));
      default: throw new Error(`unknown placeholder ${whole}`);
    }
  });
}

/** Every template under packaging/, as [path, output file name]. */
function templates(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return templates(path);
    return entry.name.endsWith(".in") ? [[path, entry.name.slice(0, -3)]] : [];
  });
}

mkdirSync(out, { recursive: true });
for (const [path, name] of templates(here)) {
  const text = render(readFileSync(path, "utf8"));
  writeFileSync(join(out, name), text);
  console.log(`wrote ${join(out, name)} from ${basename(dirname(path))}/${basename(path)}`);
}
