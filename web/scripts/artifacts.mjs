import { createHash } from "node:crypto";
import { createReadStream, createWriteStream } from "node:fs";
import { copyFile, mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { existsSync } from "node:fs";
import { filenames, optionalFilenames } from "../release.js";
import { bundleTelemetry } from "./telemetry.mjs";
import { bundleMenu } from "./menu.mjs";

export async function fingerprint(path) {
  const sha256 = createHash("sha256"), md5 = createHash("md5");
  let size = 0;
  for await (const chunk of createReadStream(path)) {
    size += chunk.length; sha256.update(chunk); md5.update(chunk);
  }
  return { sha256: sha256.digest("hex"), md5: md5.digest("hex"), size };
}

export async function findObject(request, bucket, key) {
  const data = await (await request(`r2/buckets/${encodeURIComponent(bucket)}/objects?prefix=${encodeURIComponent(key)}`)).json();
  return data.result.find((object) => object.key === key);
}

export async function fetchResources(request, source, destination) {
  const object = await findObject(request, source.bucket, source.key);
  if (!object) throw new Error(`Missing resource archive: ${source.key}`);
  const stamp = `${destination}.source.json`;
  try {
    const previous = JSON.parse(await readFile(stamp, "utf8"));
    const local = await fingerprint(destination);
    if (previous.etag === object.etag && previous.bucket === source.bucket &&
        previous.key === source.key && local.sha256 === previous.sha256 && local.size === object.size) return local;
  } catch { /* Missing or corrupt local copy: fetch again. */ }
  await mkdir(dirname(destination), { recursive: true });
  const temporary = `${destination}.partial`;
  try {
    const response = await request(`r2/buckets/${encodeURIComponent(source.bucket)}/objects/${encodeURIComponent(source.key)}`,
      { headers: { "If-Match": `"${object.etag}"` } });
    await pipeline(Readable.fromWeb(response.body), createWriteStream(temporary));
    const digest = await fingerprint(temporary);
    const after = await findObject(request, source.bucket, source.key);
    if (digest.size !== object.size || after?.etag !== object.etag ||
        (!object.etag.includes("-") && digest.md5 !== object.etag)) {
      throw new Error("The resource archive changed or was incomplete during download; retry the build");
    }
    await rename(temporary, destination);
    await writeFile(stamp, JSON.stringify({ ...source, etag: object.etag, ...digest }));
    return digest;
  } finally { await rm(temporary, { force: true }); }
}

export async function packageRelease(webRoot, archive = join(webRoot, ".build/horizon-data.bin"),
  // The open bundle's archive (pnpm build:open or package:open): multiplayer players
  // use its numbering on the shared server, wearing this release's art.
  shared = join(webRoot, ".build-open/horizon-data.bin")) {
  const build = join(webRoot, ".build");
  const site = join(build, "site");
  await mkdir(site, { recursive: true });
  const assets = {}, files = [];
  const inputs = Object.entries(filenames).map(([name, spec]) =>
    [name, spec, name === "resources" ? archive : join(webRoot, "dist", spec[0])]);
  if (shared && existsSync(shared)) inputs.push(["sharedResources", optionalFilenames.sharedResources, shared]);
  else console.warn("No open bundle archive (pnpm package:open): multiplayer will be unavailable in this release");
  for (const [name, [filename, contentType], input] of inputs) {
    const digest = await fingerprint(input);
    if (!digest.size || digest.size > 300_000_000) throw new Error(`${filename}: expected a file of 1–300 MB for the R2 upload API`);
    const key = `assets/${digest.sha256}/${filename}`;
    const output = join(build, "objects", key);
    await mkdir(dirname(output), { recursive: true });
    await copyFile(input, output);
    assets[name] = { url: `/${key}`, sha256: digest.sha256, size: digest.size, contentType };
    files.push({ key, path: output, ...digest, contentType });
  }
  const telemetry = await bundleTelemetry(webRoot);
  const sources = {};
  for (const file of ["index.html", "cache.js", "release.js"]) sources[file] = await readFile(join(webRoot, file), "utf8");
  // The landing menu (web/menu), bundled for this site.
  const menu = await bundleMenu("horizon");
  sources["menu.js"] = menu.js;
  sources["menu.css"] = menu.css;
  const id = createHash("sha256").update(JSON.stringify({ assets, sources, telemetry })).digest("hex");
  const release = { format: 1, id, assets };
  for (const file of ["index.html", "cache.js", "release.js"]) await copyFile(join(webRoot, file), join(site, file));
  await writeFile(join(site, "menu.js"), menu.js);
  await writeFile(join(site, "menu.css"), menu.css);
  await writeFile(join(site, "release.json"), JSON.stringify(release, null, 2) + "\n");
  // Serve the small shell directly from the Worker. Cloudflare's Static Assets
  // router does not forward the native Access context to user Worker code.
  const shell = {};
  for (const [filename, contentType] of [["index.html", "text/html"], ["menu.js", "text/javascript"], ["menu.css", "text/css"],
    ["cache.js", "text/javascript"], ["release.js", "text/javascript"], ["release.json", "application/json"]]) {
    shell[`/${filename}`] = { body: await readFile(join(site, filename), "utf8"), contentType };
  }
  if (telemetry) {
    await writeFile(join(site, "telemetry.js"), telemetry);
    shell["/telemetry.js"] = { body: telemetry, contentType: "text/javascript" };
  }
  await writeFile(join(build, "shell.ts"), `// Generated by pnpm package; do not edit.\nexport const shell = ${JSON.stringify(shell)};\n`);
  await writeFile(join(build, "upload.json"), JSON.stringify({ releaseId: id, files }, null, 2) + "\n");
  return release;
}

export async function uploadAssets(request, bucket, files) {
  for (const file of files) {
    const local = await fingerprint(file.path);
    if (local.sha256 !== file.sha256 || local.size !== file.size || local.md5 !== file.md5) {
      throw new Error(`Build artifact changed: ${file.key}; rebuild before deploying`);
    }
    const existing = await findObject(request, bucket, file.key);
    if (existing) {
      if (existing.size !== file.size || existing.etag !== file.md5 ||
          existing.http_metadata?.contentType !== file.contentType) {
        throw new Error(`Immutable object does not match its build artifact: ${file.key}`);
      }
      continue;
    }
    const response = await request(`r2/buckets/${encodeURIComponent(bucket)}/objects/${file.key.split("/").map(encodeURIComponent).join("/")}`, {
      method: "PUT", duplex: "half", body: Readable.toWeb(createReadStream(file.path)),
      headers: { "Content-Type": file.contentType, "Content-Length": String(file.size),
        "Cache-Control": "public, max-age=31536000, immutable", "If-None-Match": "*" },
    });
    await response.arrayBuffer();
    const stored = await findObject(request, bucket, file.key);
    if (stored?.size !== file.size || stored?.etag !== file.md5 || stored?.http_metadata?.contentType !== file.contentType) {
      throw new Error(`Upload verification failed: ${file.key}`);
    }
  }
}
