import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { build } from "rolldown";

const web = fileURLToPath(new URL("..", import.meta.url));
const { default: workerd, compatibilityDate } = createRequire(import.meta.url)("workerd");
const { values } = parseArgs({ options: {
  server: { type: "string", default: "127.0.0.1:25565" },
  port: { type: "string", default: "8081" }, "assets-port": { type: "string", default: "8080" },
} });
const port = Number(values.port), assetsPort = Number(values["assets-port"]);
if (![port, assetsPort].every((p) => Number.isInteger(p) && p > 0 && p <= 65535) || port === assetsPort) {
  throw new Error("Choose two distinct TCP ports between 1 and 65535");
}
const directory = new URL("../.build/multiplayer-dev/", import.meta.url);
await mkdir(directory, { recursive: true });
await build({ input: new URL("./multiplayer-dev.worker.ts", import.meta.url).pathname,
  external: ["cloudflare:workers", "cloudflare:sockets"],
  output: { format: "esm", file: new URL("worker.mjs", directory).pathname } });
const config = new URL("config.capnp", directory);
await writeFile(config, `using Workerd = import "/workerd/workerd.capnp";
const config :Workerd.Config = (
  services = [
    (name = "main", worker = (
      modules = [(name = "worker.mjs", esModule = embed "worker.mjs")],
      compatibilityDate = "${compatibilityDate}",
      bindings = [(name = "TARGET", text = ${JSON.stringify(values.server)}),
        (name = "ASSET_ORIGIN", text = "http://127.0.0.1:${assetsPort}")]
    )),
    (name = "internet", network = (allow = ["public", "private"], tlsOptions = (trustBrowserCas = true)))
  ],
  sockets = [(name = "http", address = "127.0.0.1:${port}", http = (), service = "main")]
);`);

const children = [];
let stop;
const stopped = new Promise((resolve) => { stop = resolve; });
for (const signal of ["SIGINT", "SIGTERM"]) process.once(signal, () => stop());
try {
  children.push(spawn(process.execPath, ["scripts/serve.mjs"], { cwd: web, stdio: "inherit",
    env: { ...process.env, HORIZON_WEB_PORT: String(assetsPort) } }));
  children.push(spawn(workerd, ["serve", fileURLToPath(config)], { cwd: web, stdio: "inherit" }));
  const exited = children.map((child) => new Promise((_, reject) => {
    child.once("error", reject);
    child.once("exit", (code, signal) => reject(new Error(`Local server stopped (${code ?? signal})`)));
  }));
  console.log(`Multiplayer: http://127.0.0.1:${port}/?mode=multiplayer (local only; server ${values.server})`);
  await Promise.race([stopped, ...exited]);
} finally {
  for (const child of children) if (child.exitCode === null) child.kill();
}
