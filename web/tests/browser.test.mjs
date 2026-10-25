import test from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";

test("Chrome retains verified assets across restarts, updates and rollback; bad responses and quota failures recover", { timeout: 120000 }, async (t) => {
  const profile = await mkdtemp(join(tmpdir(), "horizon-cache-browser-"));
  const objects = new Map(), counts = new Map();
  const cacheModule = await readFile(new URL("../cache.js", import.meta.url));
  const asset = (name, value, contentType = "application/octet-stream") => {
    const bytes = Buffer.from(value), sha256 = createHash("sha256").update(bytes).digest("hex");
    const url = `/assets/${sha256}/${name}`;
    objects.set(url, { bytes, contentType, status: 200 });
    return { url, sha256, size: bytes.length, contentType };
  };
  const wasm = asset("horizon-client_bg.wasm", "synthetic wasm", "application/wasm");
  const release = (id, data) => ({ id, assets: { wasm, resources: asset("horizon-data.bin", data) } });
  const first = release("first", "archive one"), second = release("second", "archive two");
  const third = release("third", "archive three");
  const server = createServer((req, res) => {
    if (req.url === "/cache.js") { res.writeHead(200, { "Content-Type": "text/javascript" }); res.end(cacheModule); return; }
    const object = objects.get(req.url);
    if (object) {
      counts.set(req.url, (counts.get(req.url) ?? 0) + 1);
      res.writeHead(object.status, { "Content-Type": object.contentType,
        "Cache-Control": "private, max-age=31536000, immutable" });
      res.end(object.bytes); return;
    }
    res.writeHead(200, { "Content-Type": "text/html", "Cache-Control": "no-store" });
    res.end("<!doctype html><title>Cache fixture</title>");
  });
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  const origin = `http://127.0.0.1:${server.address().port}`;
  let context;
  t.after(async () => {
    await context?.close();
    await new Promise((done) => server.close(done));
    await rm(profile, { recursive: true, force: true });
  });
  async function open() {
    context = await chromium.launchPersistentContext(profile, {
      executablePath: process.env.CHROME_PATH ?? "/usr/bin/google-chrome", headless: true,
      args: ["--no-sandbox", "--disable-dev-shm-usage"],
    });
    const page = context.pages()[0];
    await page.goto(origin);
    return page;
  }
  let page = await open();
  const load = (r) => page.evaluate(async (r) => {
    const { loadAsset, retainRelease } = await import("/cache.js");
    const bytes = await loadAsset(r.assets.resources);
    await loadAsset(r.assets.wasm);
    await retainRelease(r);
    return bytes.byteLength;
  }, r);
  assert.equal(await load(first), first.assets.resources.size);
  await page.reload();
  await load(first);
  await context.close();
  page = await open();
  await load(first);
  assert.equal(counts.get(first.assets.resources.url), 1, "archive should survive browser restart");
  assert.equal(counts.get(wasm.url), 1);
  await load({ ...first, id: "code-only" });
  assert.equal(counts.get(first.assets.resources.url), 1, "code updates must reuse the archive");
  await load(second);
  assert.equal(counts.get(second.assets.resources.url), 1);
  assert.equal(counts.get(wasm.url), 1, "data updates must reuse WASM");
  await page.evaluate(async (a) => {
    const cache = await caches.open("horizon-assets-v1");
    await cache.put(a.url, new Response("broken", { headers: { "Content-Type": a.contentType } }));
  }, second.assets.resources);
  await load(second);
  assert.equal(counts.get(second.assets.resources.url), 2, "corrupt entries should be replaced");

  const uncached = asset("horizon-data.bin", "quota fixture");
  await page.evaluate(async (a) => {
    const { loadAsset } = await import("/cache.js");
    for (const storage of [
      { open: async () => { throw new DOMException("Denied", "SecurityError"); } },
      { open: async () => ({ match: async () => undefined, put: async () => { throw new DOMException("Full", "QuotaExceededError"); } }) },
    ]) {
      if ((await loadAsset(a, { storage })).byteLength !== a.size) throw new Error("Cache failure lost downloaded data");
    }
  }, uncached);
  assert.equal(counts.get(uncached.url), 2, "a cache write failure must not duplicate its network download");

  for (const [label, override] of [
    ["login", { contentType: "text/html" }], ["partial", { status: 206 }],
    ["truncated", { bytes: Buffer.from("short") }],
    ["checksum", { bytes: Buffer.from("X".repeat("bad checksum".length)) }],
  ]) {
    const broken = asset("horizon-data.bin", `bad ${label}`);
    Object.assign(objects.get(broken.url), override);
    const result = await page.evaluate(async (a) => {
      const { loadAsset } = await import("/cache.js");
      let rejected = false;
      try { await loadAsset(a); } catch { rejected = true; }
      return { rejected, cached: !!(await (await caches.open("horizon-assets-v1")).match(a.url)) };
    }, broken);
    assert.deepEqual(result, { rejected: true, cached: false }, label);
  }
  await load(third);
  const keys = await page.evaluate(async () => (await (await caches.open("horizon-assets-v1")).keys()).map((r) => new URL(r.url).pathname));
  assert(!keys.includes(first.assets.resources.url));
  assert(keys.includes(second.assets.resources.url));
  assert(keys.includes(third.assets.resources.url));
  assert(keys.includes(wasm.url));
  await load(second);
  assert.equal(counts.get(second.assets.resources.url), 2, "rollback should use the previous cached archive");
});
