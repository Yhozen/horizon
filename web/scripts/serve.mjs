import { createServer } from "node:http";
import { createReadStream } from "node:fs";
import { stat } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { assetPath } from "../src/http.ts";

const root = fileURLToPath(new URL("../.build", import.meta.url));
const port = Number(process.env.HORIZON_WEB_PORT ?? 8080);
const types = { html: "text/html", js: "text/javascript", json: "application/json",
  wasm: "application/wasm", bin: "application/octet-stream" };
createServer(async (request, response) => {
  const path = new URL(request.url, "http://localhost").pathname;
  const asset = assetPath.test(path);
  const name = path === "/" ? "index.html" : path.slice(1);
  if (!asset && !["index.html", "loader.js", "cache.js", "release.js", "release.json"].includes(name)) {
    response.writeHead(404); response.end(); return;
  }
  try {
    const file = resolve(root, asset ? "objects" : "site", name);
    const info = await stat(file);
    response.writeHead(200, { "Content-Type": types[name.split(".").pop()], "Content-Length": info.size,
      "Cache-Control": asset ? "private, max-age=31536000, immutable" : "no-store" });
    if (request.method === "HEAD") response.end();
    else createReadStream(file).pipe(response);
  } catch { response.writeHead(404); response.end(); }
}).listen(port, "127.0.0.1", () => console.log(`Horizon: http://127.0.0.1:${port} (local only; no Access login)`));
