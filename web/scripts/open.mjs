// Horizon Open Creative's browser build (docs/open-creative.md): its resources
// come from `horizon-prepare open` alone, never from the private archive, and
// its output is a separate static site in .build-open/static.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { filenames } from "../release.js";
import { fingerprint } from "./artifacts.mjs";
import { bundleTelemetry } from "./telemetry.mjs";
import { bundleMenu } from "./menu.mjs";

export const EDITION = "open-creative";
const SITE_FILES = [["index.html", "open/index.html"], ["cache.js", "cache.js"], ["release.js", "release.js"]];

function run(command, args, cwd) {
  const result = spawnSync(command, args, { cwd, stdio: "inherit" });
  if (result.error || result.status !== 0) throw new Error(`${command} failed: ${result.error?.message ?? result.status}`);
}

/** The open bundle and its archive, prepared from the repository's open inputs. */
export function prepareOpen(web, { prepare = process.env.HORIZON_PREPARE } = {}) {
  const repo = resolve(web, "..");
  const out = join(web, ".build-open");
  const resources = join(out, "resources"), archive = join(out, "horizon-data.bin");
  if (prepare) {
    run(resolve(prepare), ["open", "--root", repo, "--out", resources, "--archive", archive], repo);
  } else {
    run("cargo", ["run", "--locked", "--quiet", "--profile", "fast", "-p", "horizon-prepare", "--",
      "open", "--root", repo, "--out", resources, "--archive", archive], repo);
  }
  return { resources, archive };
}

/** Checks a prepared bundle: the open profile, its revision and a provenance record. */
export async function checkBundle(resources) {
  const manifest = JSON.parse(await readFile(join(resources, "horizon-resources.json"), "utf8"));
  if (manifest.content_profile !== EDITION) throw new Error(`${resources} is not a Horizon Open Creative bundle`);
  const provenance = JSON.parse(await readFile(join(resources, "horizon-open-provenance.json"), "utf8"));
  if (provenance.content_profile !== EDITION || !provenance.files ||
      !Object.values(provenance.files).every((f) => f.input || f.generator)) {
    throw new Error("The open bundle's provenance record is missing or incomplete");
  }
  return manifest;
}

/**
 * The static site: the page, its scripts, the credits and release.json at the
 * root, and every game asset under assets/<sha256>/ (immutable URLs).
 */
export async function packageOpenRelease(web, { dist = join(web, ".build-open/dist"),
  resources = join(web, ".build-open/resources"), archive = join(web, ".build-open/horizon-data.bin"),
  output = join(web, ".build-open/static") } = {}) {
  const manifest = await checkBundle(resources);
  await rm(output, { recursive: true, force: true });
  await mkdir(output, { recursive: true });
  const assets = {};
  for (const [name, [filename, contentType]] of Object.entries(filenames)) {
    const input = name === "resources" ? archive : join(dist, filename);
    if (!existsSync(input)) throw new Error(`${input} is missing: run pnpm build:open first`);
    const digest = await fingerprint(input);
    if (!digest.size) throw new Error(`${filename} is empty`);
    const key = `assets/${digest.sha256}/${filename}`;
    await mkdir(join(output, "assets", digest.sha256), { recursive: true });
    await copyFile(input, join(output, key));
    assets[name] = { url: `/${key}`, sha256: digest.sha256, size: digest.size, contentType };
  }
  const sources = {};
  for (const [name, source] of SITE_FILES) {
    sources[name] = await readFile(join(web, source), "utf8");
    await writeFile(join(output, name), sources[name]);
  }
  // The landing menu (web/menu), bundled with this site's own art.
  const menu = await bundleMenu("open");
  for (const [name, body] of [["menu.js", menu.js], ["menu.css", menu.css]]) {
    sources[name] = body;
    await writeFile(join(output, name), body);
  }
  // The same opt-in diagnostics as the private preview (web/telemetry.js), as their own app.
  const telemetry = await bundleTelemetry(web, { app: "horizon-open", stage: "open" });
  if (telemetry) {
    sources["telemetry.js"] = telemetry;
    await writeFile(join(output, "telemetry.js"), telemetry);
  }
  const credits = await readFile(join(resources, "credits/credits.json"), "utf8");
  await writeFile(join(output, "credits.json"), credits);
  for (const file of ["CREDITS.md", "licenses/CC-BY-SA-3.0.txt", "licenses/CC0-1.0.txt", "licenses/OFL-PixelifySans.txt"]) {
    await mkdir(join(output, "credits", file, ".."), { recursive: true });
    await copyFile(join(resources, "credits", file), join(output, "credits", file));
  }
  const id = createHash("sha256").update(JSON.stringify({ edition: EDITION, assets, sources, credits })).digest("hex");
  const release = { format: 1, edition: EDITION, id, content_revision: manifest.content_revision, assets };
  await writeFile(join(output, "release.json"), JSON.stringify(release, null, 2) + "\n");
  // For the public deployment (scripts/deploy-open.mjs): the small files bundled into the
  // Worker, and the immutable assets to upload to R2.
  const types = { html: "text/html", js: "text/javascript", css: "text/css", json: "application/json",
    md: "text/markdown", txt: "text/plain" };
  const shell = {};
  for (const name of [...SITE_FILES.map(([n]) => n), "menu.js", "menu.css", ...(telemetry ? ["telemetry.js"] : []), "release.json", "credits.json", "credits/CREDITS.md",
    "credits/licenses/CC-BY-SA-3.0.txt", "credits/licenses/CC0-1.0.txt", "credits/licenses/OFL-PixelifySans.txt"]) {
    shell[`/${name}`] = { body: await readFile(join(output, name), "utf8"), contentType: types[name.split(".").pop()] };
  }
  const build = join(output, "..");
  await writeFile(join(build, "shell.ts"), `// Generated by pnpm package:open; do not edit.\nexport const shell = ${JSON.stringify(shell)};\n`);
  const files = [];
  for (const asset of Object.values(assets)) {
    const path = join(output, asset.url.slice(1));
    files.push({ key: asset.url.slice(1), path, ...(await fingerprint(path)), contentType: asset.contentType });
  }
  await writeFile(join(build, "upload.json"), JSON.stringify({ releaseId: id, files }, null, 2) + "\n");
  return release;
}

/** wasm-bindgen output for the open build (the same game binary as the private one). */
export function buildWasm(web) {
  const repo = resolve(web, "..");
  run("cargo", ["build", "--locked", "--target", "wasm32-unknown-unknown", "--profile", "fast",
    "-p", "horizon-client", "--bin", "horizon-client", "--no-default-features"], repo);
  const dist = join(web, ".build-open/dist");
  run("wasm-bindgen", ["--target", "web", "--no-typescript", "--out-dir", dist, "--out-name", "horizon-client",
    resolve(repo, "target/wasm32-unknown-unknown/fast/horizon-client.wasm")], repo);
  return dist;
}
