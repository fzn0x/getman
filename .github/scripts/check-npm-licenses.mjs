#!/usr/bin/env node
// Checks the licences of every npm package in the lockfiles of web/ (the
// browser UI, built into the getman binary and the desktop app), site/ (the
// website) and crates/getman-desktop/ (the installer tooling).
//
//   node .github/scripts/check-npm-licenses.mjs
//
// Getman is MIT OR Apache-2.0, so what ends up in a build must be permissive.
// Packages that only run while building (`dev` in the lockfile) may also be
// MPL-2.0, which asks only that changes to its own files are shared: nothing
// of it is copied into what Getman ships. Anything else fails, and so does a
// package with no licence, so a new dependency is looked at by a person.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const folders = ["web", "site", "crates/getman-desktop"];

const permissive = new Set([
  "MIT",
  "ISC",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "0BSD",
  "BlueOak-1.0.0",
  "CC0-1.0",
  "Unlicense",
  "Python-2.0",
]);
// Why each extra licence is allowed for packages used only to build.
const buildOnly = new Map([["MPL-2.0", "file-level copyleft; used by the build tools and not shipped"]]);

/** Whether an SPDX expression such as `(MIT OR Apache-2.0)` is allowed. */
function allowed(expression, dev) {
  const ok = (id) => permissive.has(id) || (dev && buildOnly.has(id));
  return expression
    .replace(/[()]/g, " ")
    .split(/\s+OR\s+/)
    .some((alternative) => alternative.split(/\s+AND\s+/).every((id) => ok(id.trim())));
}

const failures = [];
for (const folder of folders) {
  const lock = JSON.parse(readFileSync(join(root, folder, "package-lock.json"), "utf8"));
  let prod = 0;
  let dev = 0;
  for (const [path, info] of Object.entries(lock.packages)) {
    if (!path) continue;
    const name = path.slice(path.lastIndexOf("node_modules/") + "node_modules/".length);
    const license = typeof info.license === "string" ? info.license : undefined;
    if (info.dev) dev++;
    else prod++;
    if (!license) {
      failures.push(`${folder}: ${name} ${info.version} declares no licence`);
    } else if (!allowed(license, Boolean(info.dev))) {
      failures.push(`${folder}: ${name} ${info.version} is ${license}${info.dev ? "" : ", which a shipped package may not be"}`);
    }
  }
  console.log(`${folder}: ${prod} packages that can ship, ${dev} for building only`);
}

if (failures.length) {
  console.error(`\n${failures.join("\n")}`);
  process.exit(1);
}
console.log("All npm licences are allowed.");
