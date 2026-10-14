// Serves dist/ under /getman/, as GitHub Pages will, for a local preview.
import { createReadStream, existsSync, statSync } from "node:fs";
import { createServer } from "node:http";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const dist = fileURLToPath(new URL("dist/", import.meta.url));
const port = Number(process.env.PORT ?? 4000);
const types = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".woff2": "font/woff2",
  ".sh": "text/plain; charset=utf-8",
  ".ps1": "text/plain; charset=utf-8",
  ".txt": "text/plain; charset=utf-8",
  ".xml": "application/xml",
};

createServer((request, response) => {
  const url = new URL(request.url, "http://localhost");
  if (!url.pathname.startsWith("/getman/")) {
    response.writeHead(302, { location: "/getman/" }).end();
    return;
  }
  let path = normalize(join(dist, decodeURIComponent(url.pathname.slice("/getman/".length))));
  if (!path.startsWith(normalize(dist))) {
    response.writeHead(403).end();
    return;
  }
  if (existsSync(path) && statSync(path).isDirectory()) path = join(path, "index.html");
  if (!existsSync(path)) {
    response.writeHead(404, { "content-type": types[".html"] });
    createReadStream(join(dist, "404.html")).pipe(response);
    return;
  }
  response.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
  createReadStream(path).pipe(response);
}).listen(port, "127.0.0.1", () => console.log(`http://127.0.0.1:${port}/getman/`));
