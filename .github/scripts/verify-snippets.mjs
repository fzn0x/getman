#!/usr/bin/env node
// Runs the code Getman exports, for real.
//
// Each tricky request below is written into a throwaway workspace, exported
// in every code-export language with `getman export`, and the snippet is run
// against a local echo server. The method, the path and query, the headers
// that matter and the body (byte for byte, or part by part for a multipart
// form) that arrive must match what the request file says.
//
//   node .github/scripts/verify-snippets.mjs --getman target/debug/getman
//
//   --getman <path>    the getman binary (default: getman on the PATH)
//   --lang a,b         only these languages (default: all)
//   --case a,b         only these cases (default: all)
//   --skip-missing     report a language whose runtime is not installed as
//                      skipped instead of failing (CI never uses this)
//   --work <dir>       keep the generated workspace and snippets here
//   --list             print the languages and cases, then stop
//
// Runtimes are found on the PATH. These environment variables point the
// harness at the rest:
//
//   PYTHON             the interpreter that has `requests` (default: python3)
//   HTTP_BIN           the HTTPie command (default: http)
//   KOTLINC            the Kotlin compiler (default: kotlinc)
//   KOTLIN_CLASSPATH   OkHttp, Okio and the Kotlin standard library as jars,
//                      joined with the platform's path separator
//
// Run the shell snippets (curl, wget, HTTPie) on Linux or macOS. Git Bash
// on Windows drops carriage returns from arguments and turns non-ASCII text
// into question marks before the program sees it, so the text and unicode
// cases fail there for reasons that have nothing to do with the snippet.
//
// Nothing here touches the network beyond 127.0.0.1, except installing the
// libraries the snippets import (axios, reqwest, http, ...), which is done
// once into the work directory.

import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:http";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";
import { parseArgs } from "node:util";

const { values: options } = parseArgs({
  options: {
    getman: { type: "string", default: "getman" },
    lang: { type: "string" },
    case: { type: "string" },
    "skip-missing": { type: "boolean", default: false },
    work: { type: "string" },
    list: { type: "boolean", default: false },
  },
});

const isWindows = process.platform === "win32";

// ---------------------------------------------------------------------------
// The requests

const NOTE = "it's \"q\" \\ $HOME #{x} ${y} `tick` %s";
const JSON_BODY = JSON.stringify({
  q: NOTE,
  unicode: "Zoë ‘quoted’ 日本語 😀 \u2028 \u2029",
  newline: "line one\nline two\ttabbed",
  nested: { list: [1, 2.5, null, true], empty: "" },
});
const TEXT_BODY =
  "it's \"q\" \\ $HOME #{x} ${y} `tick` %s \\n not a newline\r\n" +
  "second line\nthird\ttabbed  \n" +
  "Zoë ‘’ 日本語 😀 \u2028 \u2029 \u0085 \u0001 end\n";
const BINARY = Buffer.from(Array.from({ length: 256 }, (_, i) => i));
const NOTE_FILE = Buffer.from("A note, with \"quotes\", $HOME and Zoë\nsecond line\n");

/**
 * Every case: the request file, and what must reach the server.
 * `expect.body` is the exact body; `expect.parts` the parts of a multipart
 * form. `files` are written into the workspace first.
 */
const CASES = [
  {
    name: "json-nasty",
    request: {
      method: "POST",
      url: "{{base}}/json?q=it%27s&x=a%20b&name=Zo%C3%AB&e=%24HOME",
      headers: [
        { name: "Content-Type", value: "application/json" },
        { name: "X-Note", value: NOTE },
        { name: "Authorization", value: "Bearer t0k.en-x_~+/=" },
      ],
      body: { type: "json", content: JSON_BODY },
    },
    expect: {
      headers: { "content-type": "application/json", "x-note": NOTE, authorization: "Bearer t0k.en-x_~+/=" },
      body: Buffer.from(JSON_BODY),
    },
  },
  {
    name: "text-nasty",
    request: {
      method: "POST",
      url: "{{base}}/text",
      headers: [{ name: "X-Note", value: NOTE }],
      body: { type: "text", content: TEXT_BODY },
    },
    expect: {
      headers: { "content-type": "text/plain", "x-note": NOTE },
      body: Buffer.from(TEXT_BODY),
    },
  },
  {
    name: "text-custom-type",
    request: {
      method: "PUT",
      url: "{{base}}/text",
      body: { type: "text", content: "plain 'single' and \"double\"", content_type: "text/csv" },
    },
    expect: {
      headers: { "content-type": "text/csv" },
      body: Buffer.from("plain 'single' and \"double\""),
    },
  },
  {
    name: "form-urlencoded",
    request: {
      method: "POST",
      url: "{{base}}/form",
      body: {
        type: "form",
        fields: [
          { name: "a b", value: "it's \"q\" \\ $HOME & more=1" },
          { name: "uni", value: "Zoë 日本語" },
          { name: "empty", value: "" },
        ],
      },
    },
    expect: {
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: Buffer.from("a+b=it%27s+%22q%22+%5C+%24HOME+%26+more%3D1&uni=Zo%C3%AB+%E6%97%A5%E6%9C%AC%E8%AA%9E&empty="),
    },
  },
  {
    name: "multipart-fields",
    request: {
      method: "POST",
      url: "{{base}}/multipart",
      body: {
        type: "multipart",
        fields: [
          { name: "title", value: "it's \"q\" \\ $HOME #{x} ${y} `tick`" },
          { name: "uni", value: "Zoë ‘’ 日本語 😀 \u2028" },
          { name: "lines", value: "one\ntwo\n" },
          { name: "empty", value: "" },
        ],
      },
    },
    expect: {
      parts: [
        { name: "title", value: Buffer.from("it's \"q\" \\ $HOME #{x} ${y} `tick`") },
        { name: "uni", value: Buffer.from("Zoë ‘’ 日本語 😀 \u2028") },
        { name: "lines", value: Buffer.from("one\ntwo\n") },
        { name: "empty", value: Buffer.alloc(0) },
      ],
    },
  },
  {
    name: "multipart-files",
    files: { "files/note.txt": NOTE_FILE, "files/blob.bin": BINARY },
    request: {
      method: "POST",
      url: "{{base}}/upload",
      headers: [{ name: "X-Note", value: "upload" }],
      body: {
        type: "multipart",
        fields: [
          { name: "title", value: "it's \"q\" $HOME" },
          { name: "note", file: "files/note.txt" },
          { name: "blob", file: "files/blob.bin", content_type: "application/x-custom" },
        ],
      },
    },
    expect: {
      headers: { "x-note": "upload" },
      parts: [
        { name: "title", value: Buffer.from("it's \"q\" $HOME") },
        { name: "note", filename: "note.txt", value: NOTE_FILE },
        { name: "blob", filename: "blob.bin", value: BINARY, type: "application/x-custom" },
      ],
    },
  },
  {
    name: "binary-file",
    files: { "files/blob.bin": BINARY },
    request: {
      method: "POST",
      url: "{{base}}/binary",
      body: { type: "binary", file: "files/blob.bin", content_type: "application/octet-stream" },
    },
    expect: {
      headers: { "content-type": "application/octet-stream" },
      body: BINARY,
    },
  },
  {
    name: "get-headers",
    request: {
      method: "GET",
      url: "{{base}}/items?filter=a%26b&tag=x&tag=y&path=%2Fa%2Fb",
      headers: [
        { name: "Accept", value: "application/json" },
        { name: "X-Note", value: NOTE },
        { name: "X-Empty", value: "" },
      ],
    },
    expect: {
      headers: { accept: "application/json", "x-note": NOTE },
      body: Buffer.alloc(0),
    },
  },
  {
    name: "patch-json",
    request: {
      method: "PATCH",
      url: "{{base}}/items/7",
      body: { type: "json", content: '{"name":"O\'Brien","note":"line\\nbreak","path":"C:\\\\temp"}' },
    },
    expect: {
      headers: { "content-type": "application/json" },
      body: Buffer.from('{"name":"O\'Brien","note":"line\\nbreak","path":"C:\\\\temp"}'),
    },
  },
  {
    name: "delete",
    request: { method: "DELETE", url: "{{base}}/items/7" },
    expect: { body: Buffer.alloc(0) },
  },
  {
    name: "head",
    request: { method: "HEAD", url: "{{base}}/items/7" },
    expect: { body: Buffer.alloc(0) },
  },
  {
    name: "options",
    request: { method: "OPTIONS", url: "{{base}}/items" },
    expect: { body: Buffer.alloc(0) },
  },
  {
    name: "post-empty",
    request: { method: "POST", url: "{{base}}/empty" },
    expect: { body: Buffer.alloc(0) },
  },
];

// ---------------------------------------------------------------------------
// The languages

const have = (command) => {
  if (command.includes("/") || command.includes("\\")) return existsSync(command);
  const probe = spawnSync(isWindows ? "where" : "which", [command], { encoding: "utf8" });
  return probe.status === 0;
};

const python = () => process.env.PYTHON ?? (isWindows ? "python" : "python3");
const powershell = () => (have("pwsh") ? "pwsh" : "powershell");

const rawHttpCases = new Set(["json-nasty", "text-nasty", "text-custom-type", "form-urlencoded", "patch-json", "get-headers", "delete", "head", "options", "post-empty"]);

/**
 * How to run each language's snippet. `file` is the snippet's name inside its
 * own directory; `needs` the commands that must exist; `skip(caseName)` a
 * reason the language cannot take the case; `prepare(ctx)` runs once; `run`
 * returns `{ command, args, cwd?, env? }`. Every command runs with the
 * workspace as its working directory, because the snippets read files
 * relative to it.
 */
const LANGUAGES = {
  curl: {
    file: "main.sh",
    needs: ["bash", "curl"],
    run: (f) => ({ command: "bash", args: [f] }),
  },
  wget: {
    file: "main.sh",
    needs: ["bash", "wget"],
    skip: (c) => (c.startsWith("multipart") ? "wget cannot send a multipart form" : null),
    run: (f) => ({ command: "bash", args: [f] }),
  },
  httpie: {
    file: "main.sh",
    needs: ["bash", () => process.env.HTTP_BIN ?? "http"],
    run: (f, ctx, c) => ({
      command: "bash",
      args: [f],
      // HTTPie reads a body from a piped stdin unless told not to; the
      // snippet's own `< file` redirect is the one body that comes from it.
      env: {
        PATH: `${dirname(process.env.HTTP_BIN ?? "http")}${delimiter}${process.env.PATH}`,
        HTTPIE_CONFIG_DIR: c === "binary-file" ? join(ctx.work, "httpie-plain") : join(ctx.work, "httpie-config"),
      },
    }),
    prepare: (ctx) => {
      mkdirSync(join(ctx.work, "httpie-config"), { recursive: true });
      mkdirSync(join(ctx.work, "httpie-plain"), { recursive: true });
      writeFileSync(join(ctx.work, "httpie-config", "config.json"), JSON.stringify({ default_options: ["--ignore-stdin"] }));
    },
  },
  powershell: {
    file: "main.ps1",
    needs: [powershell],
    // -Form needs PowerShell 7; Windows PowerShell 5.1 has no such parameter.
    skip: (c, ctx) =>
      c.startsWith("multipart") && powershell() === "powershell" ? "-Form needs PowerShell 7 (pwsh)" : null,
    // Invoke-RestMethod -Form has no way to set a part's type.
    noPartTypes: true,
    run: (f) => ({ command: powershell(), args: ["-NoProfile", "-NonInteractive", "-File", f] }),
  },
  http: {
    file: "main.http",
    needs: [],
    skip: (c) => (rawHttpCases.has(c) ? null : "the raw request names a file instead of holding it"),
    // Not run by a program: the text is sent down a socket.
    socket: true,
  },
  fetch: {
    file: "main.mjs",
    needs: ["node"],
    skip: (c) => (c === "multipart-files" ? "the snippet reads a file from a browser <input>" : null),
    // FormData writes every line break in a field as CRLF, as browsers do.
    formCrlf: true,
    run: (f) => ({ command: "node", args: [f] }),
  },
  axios: {
    file: "main.mjs",
    needs: ["node", "npm"],
    formCrlf: true,
    prepare: (ctx) => {
      const dir = join(ctx.work, "node-libs");
      mkdirSync(dir, { recursive: true });
      writeFileSync(join(dir, "package.json"), '{"private":true,"type":"module"}');
      must("npm", ["install", "--no-audit", "--no-fund", "axios@1"], { cwd: dir });
    },
    // ESM resolves imports from the snippet's own folder upwards, so the
    // snippets live under the folder holding node_modules.
    dir: (ctx) => join(ctx.work, "node-libs", "snippets"),
    run: (f) => ({ command: "node", args: [f] }),
  },
  node: {
    file: "main.mjs",
    needs: ["node"],
    formCrlf: true,
    run: (f) => ({ command: "node", args: [f] }),
  },
  python: {
    file: "main.py",
    needs: [python],
    run: (f) => ({ command: python(), args: [f] }),
  },
  go: {
    file: "main.go",
    needs: ["go"],
    run: (f) => ({ command: "go", args: ["run", f] }),
  },
  rust: {
    file: "main.rs",
    needs: ["cargo"],
    prepare: (ctx) => {
      const dir = join(ctx.work, "rust-project");
      mkdirSync(join(dir, "src", "bin"), { recursive: true });
      writeFileSync(
        join(dir, "Cargo.toml"),
        '[package]\nname = "snippets"\nversion = "0.0.0"\nedition = "2021"\n\n[dependencies]\n' +
          'reqwest = { version = "0.13", features = ["multipart"] }\ntokio = { version = "1", features = ["full"] }\n',
      );
      writeFileSync(join(dir, "src", "main.rs"), "fn main() {}\n");
      ctx.rustDir = dir;
    },
    // Each case is a binary of the same package, so reqwest builds once.
    stage: (ctx, source, c) => {
      writeFileSync(join(ctx.rustDir, "src", "bin", `${c.replace(/-/g, "_")}.rs`), readFileSync(source));
    },
    run: (f, ctx, c) => ({
      command: "cargo",
      args: ["run", "--quiet", "--manifest-path", join(ctx.rustDir, "Cargo.toml"), "--bin", c.replace(/-/g, "_")],
      env: { CARGO_TARGET_DIR: join(ctx.work, "rust-target") },
    }),
    timeout: 900_000,
  },
  java: {
    file: "Main.java",
    needs: ["java"],
    run: (f) => ({ command: "java", args: [f] }),
  },
  kotlin: {
    file: "Snippet.kt",
    needs: [() => process.env.KOTLINC ?? "kotlinc", "java"],
    prepare: (ctx) => {
      if (!process.env.KOTLIN_CLASSPATH) throw new Error("set KOTLIN_CLASSPATH to the OkHttp, Okio and Kotlin standard library jars");
    },
    // Compiled, then run: the file is Snippet.kt, so the class is SnippetKt.
    run: (f, ctx, c) => ({
      command: "bash",
      args: [
        "-c",
        '"$1" -nowarn -cp "$2" -d "$3" "$4" 2>&1 | grep -v "^warning" ; java -cp "$3$5$2" SnippetKt',
        "kotlin",
        process.env.KOTLINC ?? "kotlinc",
        process.env.KOTLIN_CLASSPATH,
        join(ctx.work, "kotlin-out", c),
        f,
        delimiter,
      ],
    }),
    timeout: 300_000,
  },
  csharp: {
    file: "Program.cs",
    needs: ["dotnet"],
    prepare: (ctx) => {
      const dir = join(ctx.work, "dotnet-project");
      mkdirSync(dir, { recursive: true });
      writeFileSync(
        join(dir, "snippets.csproj"),
        '<Project Sdk="Microsoft.NET.Sdk">\n  <PropertyGroup>\n    <OutputType>Exe</OutputType>\n' +
          "    <TargetFramework>net8.0</TargetFramework>\n    <ImplicitUsings>enable</ImplicitUsings>\n" +
          "    <Nullable>enable</Nullable>\n  </PropertyGroup>\n</Project>\n",
      );
      ctx.dotnetDir = dir;
    },
    // One project, with the case's snippet as its Program.cs.
    stage: (ctx, source) => writeFileSync(join(ctx.dotnetDir, "Program.cs"), readFileSync(source)),
    run: (f, ctx) => ({
      command: "dotnet",
      args: ["run", "--project", join(ctx.dotnetDir, "snippets.csproj"), "--nologo", "-v", "q"],
      env: { DOTNET_NOLOGO: "1", DOTNET_CLI_TELEMETRY_OPTOUT: "1" },
    }),
    timeout: 300_000,
  },
  swift: {
    file: "main.swift",
    needs: ["swiftc"],
    // Compiled, then run: top-level code needs the file to be main.swift.
    run: (f, ctx, c) => ({
      command: "bash",
      args: ["-c", '"$1" -o "$2" "$3" && "$2"', "swift", "swiftc", join(ctx.work, "swift-out", c), f],
    }),
    timeout: 300_000,
  },
  php: {
    file: "main.php",
    needs: ["php"],
    run: (f) => ({ command: "php", args: [f] }),
  },
  ruby: {
    file: "main.rb",
    needs: ["ruby"],
    run: (f) => ({ command: "ruby", args: [f] }),
  },
  dart: {
    file: "main.dart",
    needs: ["dart"],
    prepare: (ctx) => {
      const dir = join(ctx.work, "dart-project");
      mkdirSync(join(dir, "bin"), { recursive: true });
      writeFileSync(
        join(dir, "pubspec.yaml"),
        "name: snippets\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\ndependencies:\n  http: ^1.2.0\n  http_parser: ^4.0.0\n",
      );
      must("dart", ["pub", "get"], { cwd: dir });
      ctx.dartDir = dir;
    },
    // Packages resolve from the script's own folder upwards.
    stage: (ctx, source, c) => writeFileSync(join(ctx.dartDir, "bin", `${c.replace(/-/g, "_")}.dart`), readFileSync(source)),
    run: (f, ctx, c) => ({ command: "dart", args: [join(ctx.dartDir, "bin", `${c.replace(/-/g, "_")}.dart`)] }),
  },
};

// ---------------------------------------------------------------------------
// Helpers

// On Windows these are .cmd or .bat files, which only a shell starts.
const needsShell = (command) => isWindows && /^(npm|npx|dart)$/.test(command);

function must(command, args, opts = {}) {
  const done = spawnSync(command, args, { encoding: "utf8", shell: needsShell(command), ...opts });
  if (done.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed (${done.status}):\n${done.stdout}\n${done.stderr}`);
  }
  return done.stdout;
}

/** Runs a command to its end; resolves with its exit code and output. */
function run({ command, args, cwd, env }, timeout) {
  return new Promise((done) => {
    const child = spawn(command, args, {
      cwd,
      env: { ...process.env, ...env },
      stdio: ["ignore", "pipe", "pipe"],
      shell: needsShell(command),
    });
    let output = "";
    child.stdout.on("data", (d) => (output += d));
    child.stderr.on("data", (d) => (output += d));
    const timer = setTimeout(() => {
      output += `\n[timed out after ${timeout / 1000}s]`;
      child.kill();
    }, timeout);
    child.on("error", (error) => {
      clearTimeout(timer);
      done({ code: -1, output: String(error) });
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      done({ code, output });
    });
  });
}

const show = (buffer) => {
  const text = JSON.stringify(buffer.toString("utf8"));
  return buffer.length > 200 ? `${text.slice(0, 200)}... (${buffer.length} bytes)` : text;
};

// ---------------------------------------------------------------------------
// The echo server

const received = [];
const server = createServer((req, res) => {
  const chunks = [];
  req.on("data", (c) => chunks.push(c));
  req.on("end", () => {
    received.push({
      method: req.method,
      target: req.url,
      headers: req.headers,
      body: Buffer.concat(chunks),
    });
    res.writeHead(200, { "Content-Type": "text/plain" });
    res.end(req.method === "HEAD" ? undefined : "ok");
  });
});

/** Splits a multipart body into its parts. */
function parseMultipart(body, contentType) {
  const boundary = /boundary="?([^";]+)"?/i.exec(contentType ?? "")?.[1];
  if (!boundary) return null;
  const parts = [];
  const delimiterBytes = Buffer.from(`--${boundary}`);
  let at = body.indexOf(delimiterBytes);
  while (at !== -1) {
    const start = at + delimiterBytes.length;
    if (body.subarray(start, start + 2).toString() === "--") break;
    const next = body.indexOf(delimiterBytes, start);
    if (next === -1) return null;
    // Between the boundary line and the next: CRLF, headers, CRLF CRLF, data, CRLF.
    const chunk = body.subarray(start + 2, next - 2);
    const split = chunk.indexOf("\r\n\r\n");
    const head = chunk.subarray(0, split).toString("utf8");
    const disposition = /content-disposition:[^\r\n]*/i.exec(head)?.[0] ?? "";
    parts.push({
      // Clients quote the name or not, and some use filename*=UTF-8''...
      name: decodeURIComponent(/[ ;]name=(?:"([^"]*)"|([^;\s]*))/i.exec(disposition)?.slice(1).find((v) => v !== undefined) ?? ""),
      filename: /filename=(?:"([^"]*)"|([^;\s]*))/i.exec(disposition)?.slice(1).find((v) => v !== undefined),
      type: /content-type:\s*([^\r\n]*)/i.exec(head)?.[1],
      value: chunk.subarray(split + 4),
    });
    at = next;
  }
  return parts;
}

/** Compares what arrived with what the case expects; lists the differences. */
function compare(testCase, got, base, spec = {}) {
  const crlf = spec.formCrlf;
  const problems = [];
  const want = testCase.request;
  if (got.method !== want.method) problems.push(`method: got ${got.method}, want ${want.method}`);
  const wantTarget = want.url.replace("{{base}}", "");
  if (got.target !== wantTarget) problems.push(`target: got ${got.target}, want ${wantTarget}`);
  for (const [name, value] of Object.entries(testCase.expect.headers ?? {})) {
    if (got.headers[name] !== value) {
      problems.push(`header ${name}: got ${JSON.stringify(got.headers[name])}, want ${JSON.stringify(value)}`);
    }
  }
  if (testCase.expect.parts) {
    const contentType = got.headers["content-type"] ?? "";
    if (!/^multipart\/form-data;/i.test(contentType)) {
      problems.push(`content-type: got ${JSON.stringify(contentType)}, want multipart/form-data`);
    }
    const parts = parseMultipart(got.body, contentType);
    if (!parts) {
      problems.push(`body is not a multipart form: ${show(got.body)}`);
    } else if (parts.length !== testCase.expect.parts.length) {
      problems.push(`parts: got ${parts.length} (${parts.map((p) => p.name)}), want ${testCase.expect.parts.length}`);
    } else {
      testCase.expect.parts.forEach((part, i) => {
        const have = parts[i];
        if (have.name !== part.name) problems.push(`part ${i} name: got ${JSON.stringify(have.name)}, want ${JSON.stringify(part.name)}`);
        if (have.filename !== part.filename) problems.push(`part ${part.name} filename: got ${JSON.stringify(have.filename)}, want ${JSON.stringify(part.filename)}`);
        if (part.type && !spec.noPartTypes && have.type !== part.type) problems.push(`part ${part.name} type: got ${JSON.stringify(have.type)}, want ${JSON.stringify(part.type)}`);
        const wanted = crlf && !part.filename ? Buffer.from(part.value.toString("utf8").replace(/\r?\n/g, "\r\n")) : part.value;
        if (!have.value.equals(wanted)) problems.push(`part ${part.name} value: got ${show(have.value)}, want ${show(part.value)}`);
      });
    }
  } else if (!got.body.equals(testCase.expect.body)) {
    problems.push(`body: got ${show(got.body)}, want ${show(testCase.expect.body)}`);
  }
  return problems;
}

/** Sends a raw HTTP/1.1 snippet down a socket, as a client would. */
async function sendRaw(text, port) {
  const { connect } = await import("node:net");
  // The snippet breaks lines with LF to read well; the headers need CRLF.
  const split = text.indexOf("\n\n");
  const head = (split === -1 ? text : text.slice(0, split)).replace(/\n/g, "\r\n");
  const wire = split === -1 ? `${head}\r\n\r\n` : `${head}\r\n\r\n${text.slice(split + 2)}`;
  await new Promise((done, fail) => {
    const socket = connect(port, "127.0.0.1", () => socket.write(wire));
    socket.on("data", () => socket.end());
    socket.on("close", done);
    socket.on("error", fail);
  });
}

// ---------------------------------------------------------------------------
// The run

const only = (value) => (value ? new Set(value.split(",")) : null);
const wantLangs = only(options.lang);
const wantCases = only(options.case);

if (options.list) {
  console.log(`languages: ${Object.keys(LANGUAGES).join(", ")}`);
  console.log(`cases: ${CASES.map((c) => c.name).join(", ")}`);
  process.exit(0);
}

const getman = options.getman.includes("/") || options.getman.includes("\\") ? resolve(options.getman) : options.getman;
const work = options.work ? resolve(options.work) : mkdtempSync(join(tmpdir(), "getman-snippets-"));
mkdirSync(work, { recursive: true });
const workspace = join(work, "workspace");
const ctx = { work };

await new Promise((ready) => server.listen(0, "127.0.0.1", ready));
const port = server.address().port;
const base = `http://127.0.0.1:${port}`;

// The workspace holds the requests; `getman init` marks it as one.
for (const stale of [workspace, join(work, "snippets"), join(work, "node-libs", "snippets")]) {
  rmSync(stale, { recursive: true, force: true });
}
must(getman, ["init", workspace, "--name", "Snippets"]);
for (const testCase of CASES) {
  const request = { name: testCase.name, ...testCase.request, url: testCase.request.url.replace("{{base}}", base) };
  // JSON is YAML, but YAML reads a raw U+0085, U+2028 or U+2029 as a line
  // break, so those are written as escapes.
  const yaml = [...JSON.stringify(request, null, 2)]
    .map((c) => ([0x85, 0x2028, 0x2029].includes(c.charCodeAt(0)) ? "\\u" + c.charCodeAt(0).toString(16).padStart(4, "0") : c))
    .join("");
  writeFileSync(join(workspace, `${testCase.name}.yaml`), yaml);
  for (const [path, data] of Object.entries(testCase.files ?? {})) {
    mkdirSync(dirname(join(workspace, path)), { recursive: true });
    writeFileSync(join(workspace, path), data);
  }
}

const results = [];
const record = (language, name, status, detail = "") => {
  results.push({ language, name, status, detail });
  const mark = { pass: "ok  ", fail: "FAIL", skip: "skip" }[status];
  console.log(`${mark} ${language} / ${name}${detail ? `\n       ${detail.replace(/\n/g, "\n       ")}` : ""}`);
};

// Getman itself is the reference: what `getman send` puts on the wire
// must match the expectations too, or the expectations are wrong.
const selectedCases = CASES.filter((c) => !wantCases || wantCases.has(c.name));
if (!wantLangs || wantLangs.has("getman")) {
  for (const testCase of selectedCases) {
    received.length = 0;
    const done = await run({ command: getman, args: ["send", "-w", workspace, `${testCase.name}.yaml`], cwd: workspace }, 60_000);
    const problems = received.length === 1 ? compare(testCase, received[0], base) : [`${received.length} requests arrived`];
    if (done.code !== 0) problems.push(`getman send exited ${done.code}: ${done.output.trim()}`);
    record("getman", testCase.name, problems.length ? "fail" : "pass", problems.join("\n"));
  }
}

for (const [language, spec] of Object.entries(LANGUAGES)) {
  if (wantLangs && !wantLangs.has(language)) continue;
  const missing = (spec.needs ?? []).map((n) => (typeof n === "function" ? n() : n)).filter((n) => !have(n));
  if (missing.length) {
    for (const testCase of selectedCases) {
      const reason = `${missing.join(", ")} is not installed`;
      record(language, testCase.name, options["skip-missing"] ? "skip" : "fail", reason);
    }
    continue;
  }
  try {
    spec.prepare?.(ctx);
  } catch (error) {
    for (const testCase of selectedCases) record(language, testCase.name, "fail", `setup failed: ${error.message}`);
    continue;
  }
  for (const testCase of selectedCases) {
    const reason = spec.skip?.(testCase.name, ctx);
    if (reason) {
      record(language, testCase.name, "skip", reason);
      continue;
    }
    const folder = spec.dir ? spec.dir(ctx) : join(work, "snippets");
    const snippetDir = join(folder, language, testCase.name);
    mkdirSync(snippetDir, { recursive: true });
    const file = join(snippetDir, spec.file);
    const exported = await run({ command: getman, args: ["export", "-w", workspace, `${testCase.name}.yaml`, "--to", language, "-o", file], cwd: workspace }, 60_000);
    if (exported.code !== 0) {
      record(language, testCase.name, "fail", `getman export failed: ${exported.output.trim()}`);
      continue;
    }
    received.length = 0;
    let output = "";
    if (spec.socket) {
      await sendRaw(readFileSync(file, "utf8"), port);
    } else {
      spec.stage?.(ctx, file, testCase.name);
      const done = await run({ cwd: workspace, ...spec.run(file, ctx, testCase.name) }, spec.timeout ?? 120_000);
      output = done.output.trim();
      if (done.code !== 0) {
        record(language, testCase.name, "fail", `the snippet exited ${done.code}:\n${output.slice(-1500)}`);
        continue;
      }
    }
    if (received.length !== 1) {
      record(language, testCase.name, "fail", `${received.length} requests arrived, want 1\n${output.slice(-600)}`);
      continue;
    }
    const problems = compare(testCase, received[0], base, spec);
    record(language, testCase.name, problems.length ? "fail" : "pass", problems.join("\n"));
  }
}

server.close();
const count = (status) => results.filter((r) => r.status === status).length;
console.log(`\n${count("pass")} passed, ${count("fail")} failed, ${count("skip")} skipped (work directory: ${work})`);
if (!options.work && count("fail") === 0) rmSync(work, { recursive: true, force: true });
process.exit(count("fail") === 0 ? 0 : 1);
