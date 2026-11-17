import test from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdtemp, readFile, writeFile, symlink, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { gunzipSync } from "node:zlib";
import { chromium } from "playwright";
import { bundleTelemetry } from "../scripts/telemetry.mjs";

function events(bytes) {
  let body = bytes;
  if (body[0] === 31 && body[1] === 139) body = gunzipSync(body);
  let text = body.toString();
  if (text.startsWith("data=")) text = Buffer.from(new URLSearchParams(text).get("data"), "base64").toString();
  const json = JSON.parse(text);
  return Array.isArray(json) ? json : json.batch ?? [json];
}

test("real SDK stays silent before consent, redacts events, and stops after revocation", { timeout: 45000 }, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "horizon-telemetry-"));
  let browser;
  const packets = [], external = [];
  await symlink(new URL("../node_modules", import.meta.url).pathname, join(root, "node_modules"), "dir");
  await writeFile(join(root, "telemetry.js"), await readFile(new URL("../telemetry.js", import.meta.url)));
  await writeFile(join(root, ".env.local"), "NEXT_PUBLIC_POSTHOG_PROJECT_TOKEN=phc_fixture\nNEXT_PUBLIC_POSTHOG_HOST=https://ph.invalid\n");
  const code = await bundleTelemetry(root);
  const html = '<!doctype html><form id="session"></form><input id="diagnostics" type="checkbox"><input id="recording" type="checkbox"><script>window.horizonIdentity={distinctId:"private-player@example.com",email:"private-player@example.com"};</script><script type="module" src="/telemetry.js"></script>';
  const server = createServer((req, res) => {
    res.writeHead(200, { "Content-Type": req.url === "/telemetry.js" ? "text/javascript" : "text/html" });
    res.end(req.url === "/telemetry.js" ? code : html);
  });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  t.after(async () => { await browser?.close(); await new Promise(resolve => server.close(resolve)); await rm(root, { recursive: true, force: true }); });
  browser = await chromium.launch({ executablePath: process.env.CHROME_PATH ?? "/usr/bin/google-chrome", headless: true,
    args: ["--no-sandbox", "--disable-dev-shm-usage"] });
  const page = await browser.newPage();
  await page.route("**/*", async route => {
    const request = route.request(), url = new URL(request.url());
    if (url.hostname === "127.0.0.1") return route.continue();
    external.push(request.url());
    assert.equal(url.hostname, "ph.invalid", "only fixture ingestion host is allowed");
    if (request.method() === "POST" && /\/e\/?$/.test(url.pathname)) packets.push(request.postDataBuffer());
    return route.fulfill({ status: 200, contentType: url.pathname.endsWith(".js") ? "text/javascript" : "application/json",
      headers: { "Access-Control-Allow-Origin": "*" }, body: url.pathname.endsWith(".js") ? "" : '{"status":1}' });
  });
  await page.goto(`http://127.0.0.1:${server.address().port}/?name=private-player`);
  await page.waitForFunction(() => window.horizonTelemetry);
  await page.waitForTimeout(300);
  assert.equal(external.length, 0, "no config or ingest connection before consent");
  await page.locator("#diagnostics").check();
  await page.evaluate(() => window.horizonTelemetry.capture("redaction check", {
    url: "https://game.example/?name=private-player&token=secret", chat: "private chat", name: "private-player", count: 3,
  }));
  let captured;
  for (let attempt = 0; attempt < 60 && !captured; attempt++) {
    for (const packet of packets) captured ??= events(packet).find(event => event.event === "redaction check");
    if (!captured) await page.waitForTimeout(100);
  }
  assert(captured, "custom event reached the SDK transport");
  assert.equal(captured.properties.url, "https://game.example/");
  assert.equal(captured.properties.count, 3);
  assert.equal(captured.properties.token, "phc_fixture", "redaction must preserve the SDK ingestion token");
  assert.equal(captured.properties.chat, undefined);
  assert.equal(captured.properties.name, undefined);
  assert(!JSON.stringify(captured).includes("private-player"));
  assert(!packets.flatMap(events).some(event => event.event === "$identify"), "Access identity must not become analytics identity");
  assert(!packets.some(packet => packet.toString().includes("private-player@example.com")));
  await page.locator("#diagnostics").uncheck();
  await page.waitForTimeout(3500);
  const afterRevocation = packets.length;
  await page.evaluate(() => {
    window.horizonTelemetry.capture("must not send");
    window.horizonTelemetry.captureException(new Error("must not send either"));
  });
  await page.waitForTimeout(3500);
  assert.equal(packets.length, afterRevocation);
  external.length = 0;
  await page.reload(); await page.waitForFunction(() => window.horizonTelemetry);
  await page.waitForTimeout(300);
  assert.equal(external.length, 0, "revocation persists after reload");
});
