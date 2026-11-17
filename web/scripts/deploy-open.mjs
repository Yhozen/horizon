// pnpm deploy:open: publish Horizon Open Creative (docs/open-creative.md) as a public
// Worker on its own domain, with the stored Alchemy profile's Cloudflare credentials:
//   1. the static build's immutable assets (web/.build-open, from pnpm build:open or
//      package:open) to its own R2 bucket, uploaded only when missing;
//   2. the shared open server's image (web/open-server: Horizon's dedicated server on the
//      open bundle) and the relay's image (crates/horizon-open-relay/deploy: open players'
//      way into the Java server), built with Docker;
//   3. the Worker (src/open-worker.ts), its two containers and its custom domain, with
//      wrangler. It touches neither the private preview nor any Alchemy state.
import { spawnSync } from "node:child_process";
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import * as Redacted from "effect/Redacted";
import { api, profileCredentials } from "./cloudflare.ts";
import { uploadAssets } from "./artifacts.mjs";

const { values } = parseArgs({ options: {
  domain: { type: "string", default: "open.rccpr.tech" },
  profile: { type: "string", default: "default" },
  // The server binary: `cargo build --profile fast -p horizon-client --bin horizon-client
  // --no-default-features` for x86_64 Linux (glibc 2.34 or older).
  "server-binary": { type: "string", default: "../target/server/fast/horizon-client" },
  // The relay: `cargo build --profile fast -p horizon-open-relay` (glibc 2.39 or older).
  "relay-binary": { type: "string", default: "../target/fast/horizon-open-relay" },
  // The Java server open players reach through the relay ("" leaves the relay out).
  survival: { type: "string", default: "survival.minecraft.rccpr.tech:25565" },
  "dry-run": { type: "boolean", default: false },
} });
export const WORKER = "horizon-open", BUCKET = "horizon-open-assets", WRANGLER = "wrangler@4.162.0";
const web = fileURLToPath(new URL("..", import.meta.url));
const build = join(web, ".build-open");

const credentials = await profileCredentials(values.profile);
if (credentials.type !== "apiToken") throw new Error("deploy:open needs an API token profile");
const request = api(credentials);
const json = async (path, init) => (await (await request(path, init)).json()).result;

// 1. The bucket and the immutable assets.
const upload = JSON.parse(await readFile(join(build, "upload.json"), "utf8"));
if (!values["dry-run"]) {
  const buckets = (await json("r2/buckets")).buckets ?? [];
  if (!buckets.some((b) => b.name === BUCKET)) {
    await request("r2/buckets", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ name: BUCKET }) });
    console.log(`Created bucket ${BUCKET}`);
  }
  await uploadAssets(request, BUCKET, upload.files);
  console.log(`Assets of release ${upload.releaseId} in ${BUCKET}`);
}

// 2. The server image's build context: the Dockerfile and settings, the binary, the
// same open bundle the page's archive was packed from.
const binary = resolve(web, values["server-binary"]);
if (!existsSync(binary)) throw new Error(`${binary} is missing: build the server binary (see --server-binary)`);
const image = join(build, "server-image");
await rm(image, { recursive: true, force: true });
await mkdir(image, { recursive: true });
for (const file of ["Dockerfile", "server.properties", "eula.txt"]) await cp(join(web, "open-server", file), join(image, file));
await cp(binary, join(image, "horizon-client"));
await cp(join(build, "resources"), join(image, "resources"), { recursive: true });

// The relay's build context: its Dockerfile, the binary, the same open bundle (players
// check the brand derived from it) and the Java server's address.
const relayImage = join(build, "relay-image");
await rm(relayImage, { recursive: true, force: true });
if (values.survival) {
  if (!/^[a-z0-9.-]+:\d{1,5}$/i.test(values.survival)) throw new Error(`--survival must be host:port, not ${values.survival}`);
  const relayBinary = resolve(web, values["relay-binary"]);
  if (!existsSync(relayBinary)) throw new Error(`${relayBinary} is missing: build the relay (see --relay-binary)`);
  await mkdir(relayImage, { recursive: true });
  const dockerfile = await readFile(join(web, "../crates/horizon-open-relay/deploy/Dockerfile"), "utf8");
  await writeFile(join(relayImage, "Dockerfile"), `${dockerfile}ENV HORIZON_RELAY_UPSTREAM=${values.survival}\n`);
  await cp(relayBinary, join(relayImage, "horizon-open-relay"));
  await cp(join(build, "resources"), join(relayImage, "resources"), { recursive: true });
}

// 3. The Worker, its Durable Object and container, and the domain.
const config = {
  name: WORKER,
  main: join(web, "src/open-worker.ts"),
  compatibility_date: "2026-10-24",
  workers_dev: false,
  routes: [{ pattern: values.domain, custom_domain: true }],
  r2_buckets: [{ binding: "BUCKET", bucket_name: BUCKET }],
  durable_objects: { bindings: [{ name: "OPEN_SERVER", class_name: "OpenServer" },
    ...(values.survival ? [{ name: "SURVIVAL_RELAY", class_name: "SurvivalRelay" }] : [])] },
  migrations: [{ tag: "v1", new_sqlite_classes: ["OpenServer"] }, { tag: "v2", new_sqlite_classes: ["SurvivalRelay"] }],
  containers: [{ class_name: "OpenServer", image: join(image, "Dockerfile"), image_build_context: image,
    max_instances: 1, instance_type: "standard-1" },
  ...(values.survival ? [{ class_name: "SurvivalRelay", image: join(relayImage, "Dockerfile"),
    image_build_context: relayImage, max_instances: 1, instance_type: "standard-1" }] : [])],
  observability: { enabled: true },
};
await writeFile(join(build, "wrangler.json"), JSON.stringify(config, null, 2) + "\n");
const args = ["dlx", WRANGLER, "deploy", "--config", join(build, "wrangler.json"), ...(values["dry-run"] ? ["--dry-run"] : [])];
const result = spawnSync("pnpm", args, { cwd: build, stdio: "inherit", env: { ...process.env,
  CLOUDFLARE_API_TOKEN: Redacted.value(credentials.apiToken), CLOUDFLARE_ACCOUNT_ID: credentials.accountId,
  WRANGLER_SEND_METRICS: "false" } });
// wrangler logs Docker in to Cloudflare's registry to push the image: leave no credential behind.
spawnSync("docker", ["logout", "registry.cloudflare.com"], { stdio: "ignore" });
if (result.status !== 0) throw new Error(`wrangler deploy failed (${result.status ?? result.error?.message})`);
console.log(`Horizon Open Creative: https://${values.domain}/ (release ${upload.releaseId})`);
