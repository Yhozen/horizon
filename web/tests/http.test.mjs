import test from "node:test";
import assert from "node:assert/strict";
import { gateway as handleGateway, serveAsset } from "../src/http.ts";
import { allowedEmails } from "../scripts/config.mjs";

const path = `/assets/${"a".repeat(64)}/horizon-data.bin`;
const request = (suffix = path, init) => new Request(`https://game.test${suffix}`, init);
const env = { ALLOWED_EMAILS: allowedEmails.join(",") };
const shell = Object.fromEntries([["index.html", "text/html"], ["release.json", "application/json"],
  ["loader.js", "text/javascript"], ["cache.js", "text/javascript"], ["release.js", "text/javascript"]]
  .map(([name, contentType]) => [`/${name}`, { body: "shell ✓", contentType }]));
const gateway = (request, env, ctx) => handleGateway(request, env, ctx, shell);
const identity = { getIdentity: async () => ({ email: allowedEmails[0] }) };

test("anonymous and forged-header requests never reach assets or a warm internal cache", async () => {
  let calls = 0;
  const ctx = { exports: { Assets: { fetch: async () => { calls++; return new Response("cached"); } } } };
  for (const url of [path, "/", ...Object.keys(shell)]) {
    const response = await gateway(request(url, { headers: { "Cf-Access-Authenticated-User-Email": allowedEmails[0] } }), env, ctx);
    assert.equal(response.status, 403);
    assert.equal(response.headers.get("Cache-Control"), "no-store");
  }
  assert.equal(calls, 0);
  assert.equal((await gateway(request(), env, { ...ctx, access: { getIdentity: async () => ({ email: "other@test.net" }) } })).status, 403);
});

test("every invited viewer can retrieve the shell and cached assets; other identities remain denied", async () => {
  let assetRequests = 0;
  const exports = { Assets: { fetch: async () => { assetRequests++; return new Response("cached"); } } };
  for (const email of [...allowedEmails, allowedEmails[1].toUpperCase()]) {
    const ctx = { exports, access: { getIdentity: async () => ({ email }) } };
    assert.equal((await gateway(request("/"), env, ctx)).status, 200, email);
    assert.equal((await gateway(request(path), env, ctx)).status, 200, email);
  }
  const before = assetRequests;
  for (const email of ["other@example.com", "vichudo+other@gmail.com", "zneel.yevenes@example.com"]) {
    const ctx = { exports, access: { getIdentity: async () => ({ email }) } };
    assert.equal((await gateway(request("/"), env, ctx)).status, 403, email);
    assert.equal((await gateway(request(path), env, ctx)).status, 403, email);
  }
  assert.equal(assetRequests, before);
});

test("native Access identity can load the complete bundled shell without a Static Assets binding", async () => {
  const ctx = { access: identity, exports: {} };
  for (const [url, page] of [["/", shell["/index.html"]], ...Object.entries(shell)]) {
    const response = await gateway(request(url), env, ctx);
    assert.equal(response.status, 200);
    assert.equal(await response.text(), page.body);
    assert.equal(response.headers.get("Content-Type"), `${page.contentType}; charset=utf-8`);
    assert.equal(response.headers.get("Cache-Control"), "no-store");
    assert.equal(response.headers.get("X-Content-Type-Options"), "nosniff");
    assert.equal(Number(response.headers.get("Content-Length")), Buffer.byteLength(page.body));
    const head = await gateway(request(url, { method: "HEAD" }), env, ctx);
    assert.equal(head.status, 200);
    assert.equal(head.body, null);
    assert.deepEqual([...head.headers], [...response.headers]);
  }
});

test("authorized asset responses preserve streaming and expose cache diagnostics with private browser headers", async () => {
  const body = new ReadableStream({ start(c) { c.enqueue(new Uint8Array([1, 2])); c.close(); } });
  const response = await gateway(request(path, { headers: { Cookie: "private", Authorization: "private" } }), env, {
    access: identity, exports: { Assets: { fetch: async (incoming) => {
      assert.equal(incoming.headers.get("Cookie"), null);
      assert.equal(incoming.headers.get("Authorization"), null);
      return new Response(body, { headers: { "Cf-Cache-Status": "HIT", "Content-Type": "application/octet-stream", "Content-Length": "2" } });
    } } },
  });
  assert.equal(response.body, body);
  assert.equal(response.headers.get("X-Horizon-Asset-Cache"), "HIT");
  assert.equal(response.headers.get("Cache-Control"), "private, max-age=31536000, immutable");
  assert.equal(response.headers.get("Content-Length"), "2");
  assert.deepEqual([...new Uint8Array(await response.arrayBuffer())], [1, 2]);
});

test("shell, error, invalid asset and unsupported-method responses cannot become immutable cache entries", async () => {
  const ctx = { access: identity, exports: { Assets: { fetch: async () => new Response("missing", { status: 404 }) } } };
  for (const [url, method, status] of [["/", "GET", 200], ["/release.json", "GET", 200],
    [path, "GET", 404], [`${path}?version=bad`, "GET", 404], ["/horizon-data.bin", "GET", 404], [path, "PUT", 405]]) {
    const response = await gateway(request(url, { method }), env, ctx);
    assert.equal(response.status, status);
    assert.equal(response.headers.get("Cache-Control"), "no-store");
  }
});

test("R2 GET streams the body, HEAD omits it, and ETag revalidation returns 304", async () => {
  let heads = 0, gets = 0;
  const metadata = { size: 3, httpEtag: '"etag"', writeHttpMetadata: (h) => h.set("Content-Type", "application/octet-stream") };
  const bucket = { head: async () => { heads++; return metadata; }, get: async () => {
    gets++; return { ...metadata, body: new Response(new Uint8Array([1, 2, 3])).body };
  } };
  const response = await serveAsset(request(), bucket);
  assert.equal(response.headers.get("Content-Type"), "application/octet-stream");
  assert.equal(response.headers.get("Content-Length"), "3");
  assert.match(response.headers.get("Cache-Control"), /^public/);
  assert.equal((await response.arrayBuffer()).byteLength, 3);
  assert.equal((await serveAsset(request(path, { method: "HEAD" }), bucket)).body, null);
  assert.equal(heads, 1);
  assert.equal((await serveAsset(request(path, { headers: { "If-None-Match": '"etag"' } }), bucket)).status, 304);
  assert.equal(gets, 2);
  assert.equal((await serveAsset(request(), { get: async () => null })).status, 404);
  const failure = await serveAsset(request(), { get: async () => { throw new Error("offline"); } });
  assert.equal(failure.status, 502);
  assert.equal(failure.headers.get("Cache-Control"), "no-store");
});

test("unavailable identity and internal cache fail closed without caching errors", async () => {
  const fetch = async () => { throw new Error("unavailable"); };
  const ctx = { access: identity, exports: { Assets: { fetch } } };
  const unavailable = await gateway(request(path), env, ctx);
  assert.equal(unavailable.status, 502);
  assert.equal(unavailable.headers.get("Cache-Control"), "no-store");
  const response = await gateway(request(), env, { ...ctx, access: { getIdentity: fetch } });
  assert.equal(response.status, 503);
  assert.equal(response.headers.get("Cache-Control"), "no-store");
});
