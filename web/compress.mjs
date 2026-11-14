// Packs the built UI for getman to build in. Vite writes the UI to build/,
// and this writes every file of it Brotli-compressed to dist/, which getman
// embeds and serves as it is to browsers that take Brotli. That keeps about
// a megabyte out of the binary.

import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { brotliCompressSync, constants } from "node:zlib";

const root = fileURLToPath(new URL(".", import.meta.url));
const from = join(root, "build");
const to = join(root, "dist");

rmSync(to, { recursive: true, force: true });
let before = 0;
let after = 0;
for (const entry of readdirSync(from, { recursive: true, withFileTypes: true })) {
  if (!entry.isFile()) continue;
  const path = join(entry.parentPath, entry.name);
  const data = readFileSync(path);
  const packed = brotliCompressSync(data, {
    params: {
      [constants.BROTLI_PARAM_QUALITY]: constants.BROTLI_MAX_QUALITY,
      [constants.BROTLI_PARAM_SIZE_HINT]: data.length,
    },
  });
  const target = join(to, `${relative(from, path)}.br`);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, packed);
  before += data.length;
  after += packed.length;
}
const kib = (bytes) => `${(bytes / 1024).toFixed(0)} KiB`;
console.log(`dist/: the UI's ${kib(before)} compressed to ${kib(after)}`);
