// pnpm serve:open: the open build's static site on 127.0.0.1 (any static file
// server works as well; this one sends the immutable cache headers).
import { createServer } from "node:http";
import { createReadStream } from "node:fs";
import { stat } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { resolve, sep } from "node:path";

const root = resolve(process.env.HORIZON_OPEN_SITE ?? fileURLToPath(new URL("../.build-open/static", import.meta.url)));
const port = Number(process.env.HORIZON_WEB_PORT ?? 8081);
const types = { html: "text/html", js: "text/javascript", css: "text/css", json: "application/json", wasm: "application/wasm",
  bin: "application/octet-stream", md: "text/markdown; charset=utf-8", txt: "text/plain; charset=utf-8" };
createServer(async (request, response) => {
  const path = decodeURIComponent(new URL(request.url, "http://localhost").pathname);
  const file = resolve(root, "." + (path === "/" ? "/index.html" : path));
  if (!file.startsWith(root + sep)) { response.writeHead(404); response.end(); return; }
  try {
    const info = await stat(file);
    if (!info.isFile()) throw new Error("not a file");
    const immutable = path.startsWith("/assets/");
    response.writeHead(200, { "Content-Type": types[file.split(".").pop()] ?? "application/octet-stream",
      "Content-Length": info.size, "Cache-Control": immutable ? "public, max-age=31536000, immutable" : "no-store" });
    if (request.method === "HEAD") response.end();
    else createReadStream(file).pipe(response);
  } catch { response.writeHead(404); response.end(); }
}).listen(port, "127.0.0.1", () => console.log(`Horizon Open Creative: http://127.0.0.1:${port}`));
