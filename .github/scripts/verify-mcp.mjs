#!/usr/bin/env node
// Talks to `getman mcp` with the official MCP Inspector command line, the way
// a client would: lists the tools, then calls list_requests and send_request
// against a local server, and checks that a GETMAN_* value is scrubbed from
// what the agent gets back.
//
//   node .github/scripts/verify-mcp.mjs --getman target/debug/getman
//
// It needs Node.js and npm, and downloads the Inspector the first time.

import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";

// Pinned, so a new Inspector release cannot fail an unrelated change.
const INSPECTOR = "@modelcontextprotocol/inspector@2.12.0";
const TOOLS = ["list_requests", "read_request", "send_request", "run_folder", "list_environments", "save_request", "import_curl"];
const SECRET = "s3cr3t-token-value";

const { values } = parseArgs({ options: { getman: { type: "string", default: "getman" } } });
const getman = values.getman.includes("/") || values.getman.includes("\\") ? resolve(values.getman) : values.getman;
const isWindows = process.platform === "win32";

const seen = [];
const server = createServer((req, res) => {
  const chunks = [];
  req.on("data", (c) => chunks.push(c));
  req.on("end", () => {
    seen.push({ method: req.method, url: req.url, token: req.headers["x-token"], body: Buffer.concat(chunks).toString() });
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ echoed: req.headers["x-token"] ?? null }));
  });
});
await new Promise((ready) => server.listen(0, "127.0.0.1", ready));
const base = `http://127.0.0.1:${server.address().port}`;

const dir = mkdtempSync(join(tmpdir(), "getman-mcp-"));
const workspace = join(dir, "ws");
const init = spawnSync(getman, ["init", workspace, "--name", "McpCheck"], { encoding: "utf8" });
if (init.status !== 0) throw new Error(`getman init failed: ${init.stderr}`);
writeFileSync(
  join(workspace, "hello.yaml"),
  JSON.stringify({
    name: "Hello",
    method: "GET",
    url: `${base}/hello?x=1`,
    headers: [{ name: "X-Token", value: "{{$env.GETMAN_MCP_TOKEN}}" }],
    checks: ["status == 200"],
  }),
);

/** Calls the Inspector command line; resolves with the JSON it prints. */
function inspect(...args) {
  return new Promise((done, fail) => {
    // The Inspector gives the server only a few variables of its own, so
    // the one under test is passed with -e.
    const child = spawn(
      "npx",
      ["-y", INSPECTOR, "--cli", "-e", `GETMAN_MCP_TOKEN=${SECRET}`, getman, "mcp", workspace, ...args],
      { stdio: ["ignore", "pipe", "pipe"], shell: isWindows },
    );
    let out = "";
    let err = "";
    child.stdout.on("data", (d) => (out += d));
    child.stderr.on("data", (d) => (err += d));
    const timer = setTimeout(() => child.kill(), 180_000);
    child.on("close", (code) => {
      clearTimeout(timer);
      try {
        done({ code, json: JSON.parse(out), text: out });
      } catch {
        fail(new Error(`the Inspector printed no JSON (exit ${code}):\n${out}\n${err}`));
      }
    });
  });
}

const problems = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) problems.push(what);
};

try {
  const listed = await inspect("--method", "tools/list");
  const names = listed.json.tools.map((t) => t.name);
  check(JSON.stringify(names) === JSON.stringify(TOOLS), `tools/list names the seven tools (${names.join(", ")})`);
  check(listed.json.tools.every((t) => t.inputSchema?.type === "object"), "every tool has an object input schema");

  const requests = await inspect("--method", "tools/call", "--tool-name", "list_requests");
  const paths = requests.json.structuredContent.tree.map((n) => n.path);
  check(requests.json.isError === false && paths.includes("hello.yaml"), "list_requests lists hello.yaml");

  const sent = await inspect("--method", "tools/call", "--tool-name", "send_request", "--tool-arg", "path=hello.yaml");
  const result = sent.json.structuredContent;
  check(sent.json.isError === false && result.status === 200 && result.passed === true, "send_request gets 200 and the checks pass");
  check(seen.length === 1 && seen[0].token === SECRET, "the server received the GETMAN_* value in X-Token");
  check(!sent.text.includes(SECRET) && result.body.includes("[secret]"), "the GETMAN_* value comes back as [secret]");

  const missing = await inspect("--method", "tools/call", "--tool-name", "read_request", "--tool-arg", "path=../outside.yaml");
  check(missing.json.isError === true, "read_request refuses a path outside the workspace");
} finally {
  server.close();
  rmSync(dir, { recursive: true, force: true });
}

console.log(problems.length ? `\n${problems.length} failed` : "\nall passed");
process.exit(problems.length ? 1 : 0);
