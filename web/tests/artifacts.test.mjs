import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { fetchResources, packageRelease, uploadAssets } from "../scripts/artifacts.mjs";
import { validateRelease } from "../release.js";

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), "horizon-package-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(join(root, "dist"));
  for (const file of ["index.html", "loader.js", "cache.js", "release.js"]) await writeFile(join(root, file), "fixture");
  await writeFile(join(root, "dist/horizon-client.js"), "export default () => {};");
  await writeFile(join(root, "dist/horizon-client_bg.wasm"), Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]));
  const archive = join(root, "archive.bin");
  await writeFile(archive, "synthetic test resources");
  return { root, archive };
}

test("file hashes survive rebuilds and code-only releases; data changes invalidate only data", async (t) => {
  const { root, archive } = await fixture(t);
  const initial = validateRelease(await packageRelease(root, archive));
  assert.deepEqual(await packageRelease(root, archive), initial);
  await writeFile(join(root, "dist/horizon-client.js"), "export default () => 1;");
  const code = await packageRelease(root, archive);
  assert.notEqual(code.id, initial.id);
  assert.equal(code.assets.resources.url, initial.assets.resources.url);
  assert.equal(code.assets.wasm.url, initial.assets.wasm.url);
  await writeFile(archive, "different test resources");
  const data = await packageRelease(root, archive);
  assert.notEqual(data.assets.resources.url, code.assets.resources.url);
  assert.equal(data.assets.javascript.url, code.assets.javascript.url);
  assert.throws(() => validateRelease({ ...data, assets: { ...data.assets, resources: {
    ...data.assets.resources, url: "https://elsewhere.test/archive.bin",
  } } }));
});

test("packaging bundles the shell and matching manifest as importable Worker code", async (t) => {
  const { root, archive } = await fixture(t);
  const html = '<!doctype html><title>Horizon ✓</title><script>const example = "quoted";</script>';
  await writeFile(join(root, "index.html"), html);
  const release = await packageRelease(root, archive);
  const code = await readFile(join(root, ".build/shell.ts"));
  const { shell } = await import(`data:text/javascript;base64,${code.toString("base64")}`);
  assert.deepEqual(Object.keys(shell).sort(), ["/cache.js", "/index.html", "/loader.js", "/release.js", "/release.json"]);
  assert.equal(shell["/index.html"].body, html);
  assert.equal(shell["/index.html"].contentType, "text/html");
  assert.equal(shell["/loader.js"].contentType, "text/javascript");
  assert.equal(shell["/release.json"].contentType, "application/json");
  assert.deepEqual(validateRelease(JSON.parse(shell["/release.json"].body)), release);
});

function fakeR2() {
  const objects = new Map();
  let puts = 0, gets = 0;
  const request = async (path, init = {}) => {
    const url = new URL(path, "https://example.test");
    if (url.searchParams.has("prefix")) {
      const prefix = url.searchParams.get("prefix");
      return Response.json({ result: [...objects.values()].filter((o) => o.key.startsWith(prefix)) });
    }
    const key = decodeURIComponent(url.pathname.split("/objects/")[1]);
    if (init.method === "PUT") {
      const bytes = Buffer.from(await new Response(init.body).arrayBuffer());
      objects.set(key, { key, bytes, size: bytes.length,
        etag: createHash("md5").update(bytes).digest("hex"), http_metadata: { contentType: init.headers["Content-Type"] } });
      puts++;
      return Response.json({ success: true });
    }
    gets++;
    const object = objects.get(key);
    if (init.headers?.["If-Match"]) assert.equal(init.headers["If-Match"], `"${object.etag}"`);
    return new Response(object.bytes);
  };
  return { objects, request, puts: () => puts, gets: () => gets };
}

test("immutable uploads are verified and reused; corruption and changed build files fail closed", async (t) => {
  const { root, archive } = await fixture(t);
  await packageRelease(root, archive);
  const { files } = JSON.parse(await readFile(join(root, ".build/upload.json")));
  const r2 = fakeR2();
  await uploadAssets(r2.request, "bucket", files);
  assert.equal(r2.puts(), 3);
  await uploadAssets(r2.request, "bucket", files);
  assert.equal(r2.puts(), 3);
  r2.objects.get(files[0].key).etag = "wrong";
  await assert.rejects(uploadAssets(r2.request, "bucket", files), /Immutable object/);
  r2.objects.get(files[0].key).etag = files[0].md5;
  await writeFile(files[0].path, "tampered");
  await assert.rejects(uploadAssets(r2.request, "bucket", files), /artifact changed/);
});

test("source downloads pin the ETag, reuse verified files and reject incomplete or changing objects", async (t) => {
  const { root } = await fixture(t);
  const r2 = fakeR2(), bytes = Buffer.from("synthetic archive");
  const object = { key: "horizon-data.bin", bytes, size: bytes.length,
    etag: createHash("md5").update(bytes).digest("hex") };
  r2.objects.set(object.key, object);
  const destination = join(root, "download.bin"), source = { bucket: "bucket", key: object.key };
  await fetchResources(r2.request, source, destination);
  await fetchResources(r2.request, source, destination);
  assert.equal(r2.gets(), 1);
  await writeFile(destination, "damaged");
  await fetchResources(r2.request, source, destination);
  assert.equal(r2.gets(), 2);
  object.size++;
  await assert.rejects(fetchResources(r2.request, source, destination), /changed or was incomplete/);
  assert.deepEqual(await readFile(destination), bytes);
});
