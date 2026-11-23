// The browser's shader pack store (menu/shaderpacks.ts) in real headless Chrome: OPFS
// round trips and ordering, a restart of a persistent profile, the picker, drag and
// drop, the clipboard, and a browser whose OPFS fails. The wasm game is replaced by a
// fake `installFile` that records what it was given.
import test from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "rolldown";
import { chromium } from "playwright";

const web = fileURLToPath(new URL("..", import.meta.url));

/** A tiny stored-only zip with one file, enough to look like a pack. */
function syntheticZip(name = "shaders/final.fsh", body = "void main() {}\n") {
  const data = Buffer.from(body), file = Buffer.from(name);
  const local = Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50, 0); local.writeUInt16LE(10, 4);
  local.writeUInt32LE(data.length, 18); local.writeUInt32LE(data.length, 22);
  local.writeUInt16LE(file.length, 26);
  const central = Buffer.alloc(46);
  central.writeUInt32LE(0x02014b50, 0); central.writeUInt16LE(10, 4); central.writeUInt16LE(10, 6);
  central.writeUInt32LE(data.length, 20); central.writeUInt32LE(data.length, 24);
  central.writeUInt16LE(file.length, 28);
  const head = Buffer.concat([local, file, data]), dir = Buffer.concat([central, file]);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0); end.writeUInt16LE(1, 8); end.writeUInt16LE(1, 10);
  end.writeUInt32LE(dir.length, 12); end.writeUInt32LE(head.length, 16);
  return Buffer.concat([head, dir, end]);
}

test("pure helpers sanitize names and URLs", async () => {
  const out = await build({ input: join(web, "menu/shaderpacks.ts"), platform: "browser", write: false, output: { format: "esm" } });
  const dir = await mkdtemp(join(tmpdir(), "horizon-shaderpacks-unit-"));
  try {
    await writeFile(join(dir, "m.mjs"), out.output[0].code);
    const m = await import(join(dir, "m.mjs"));
    assert.equal(m.sanitizeName("BSL_v8.2.09 [Iris] §6+.zip"), "BSL_v8.2.09 [Iris] §6+.zip");
    assert.equal(m.sanitizeName("../evil/pack\\x.zip"), "._evil_pack_x.zip");
    assert.equal(m.sanitizeName("a\u0000b\nc.zip"), "abc.zip");
    assert.equal(m.sanitizeName(".."), "pack.zip");
    assert.equal(m.sanitizeName("  "), "pack.zip");
    assert.equal(m.packPath("x/y.zip"), "shaderpacks/x_y.zip");
    assert.deepEqual(m.splitPath("/shaderpacks/a.zip.txt"), ["shaderpacks", "a.zip.txt"]);
    assert.throws(() => m.splitPath("shaderpacks/../x"));
    assert.throws(() => m.splitPath("a//b"));
    assert.equal(m.joinPath("shaderpacks", "", "a.zip"), "shaderpacks/a.zip");
    assert.equal(m.packNameFromUrl("https://cdn.example/files/Complementary%20Reimagined.zip?x=1"), "Complementary Reimagined.zip");
    assert.equal(m.packNameFromUrl("https://cdn.example/"), "pack.zip");
    assert.equal(m.packNameFromUrl("/packs/..%2F..%2Fx.zip"), "._._x.zip");
    assert(m.isZip({ name: "A.ZIP" }) && !m.isZip({ name: "notes.txt", type: "text/plain" }));
    assert.equal(m.selectedPack("enabled:true\npack:B.zip\ntrial:true\n"), "B.zip");
    assert.equal(m.selectedPack("enabled:false\npack:B.zip\n"), undefined);
    assert.equal(m.selectedPack("enabled:true\npack:\n"), undefined);
    assert.deepEqual(m.bootPlan(["horizon-shaders.txt", "shaderpacks/A.zip", "shaderpacks/A.zip.txt", "shaderpacks/B.zip",
      "shaderpacks/Folder/shaders/final.fsh"], "B.zip"), {
      now: ["horizon-shaders.txt", "shaderpacks/A.zip.txt", "shaderpacks/B.zip", "shaderpacks/Folder/shaders/final.fsh"],
      later: ["shaderpacks/A.zip"],
    });
    assert.deepEqual(m.bootPlan(["shaderpacks/A.zip"]).later, ["shaderpacks/A.zip"]);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("Chrome keeps shader packs in OPFS across restarts; picker, drop and clipboard work; a failing OPFS does not throw", { timeout: 120000 }, async (t) => {
  const profile = await mkdtemp(join(tmpdir(), "horizon-shaderpacks-browser-"));
  const bundle = (await build({ input: join(web, "menu/shaderpacks.ts"), platform: "browser", write: false,
    output: { format: "esm" } })).output[0].code;
  const html = `<!doctype html><title>Shader packs fixture</title><body><script type="module">
    import * as store from "/shaderpacks.js";
    window.store = store;
    window.installs = [];
    window.changes = 0;
    window.notices = [];
    window.acceptDrops = true;
    window.hooks = store.installHooks({
      installFile: (path, bytes) => installs.push([path, new TextDecoder().decode(bytes.slice(0, 4))]),
      onChanged: () => { changes++; },
      notice: (line) => notices.push(line),
      acceptsDrop: () => window.acceptDrops,
    });
    window.ready = true;
  </script></body>`;
  const server = createServer((req, res) => {
    if (req.url === "/shaderpacks.js") { res.writeHead(200, { "Content-Type": "text/javascript" }); res.end(bundle); return; }
    res.writeHead(200, { "Content-Type": "text/html", "Cache-Control": "no-store" });
    res.end(html);
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
    await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin });
    const page = context.pages()[0];
    await page.goto(origin);
    await page.waitForFunction(() => window.ready);
    return page;
  }
  let page = await open();

  // Round trip, nested folders, ordering of queued writes, delete.
  const roundTrip = await page.evaluate(async () => {
    const enc = (s) => new TextEncoder().encode(s), dec = (b) => b && new TextDecoder().decode(b);
    const shared = enc("first");
    const write = store.writeFile("shaderpacks/A.zip.txt", shared);
    shared.set(enc("XXXXX")); // the store copied the bytes synchronously
    await write;
    window.horizonPersist("shaderpacks/B.zip", enc("one"));
    window.horizonPersist("shaderpacks/B.zip", enc("two"));
    window.horizonPersist("gone.txt", enc("soon"));
    window.horizonPersist("gone.txt", null);
    window.horizonPersist("horizon-shaders.txt", enc("enabled:true\npack:B.zip\n"));
    window.horizonPersist("shaderpacks/Folder/shaders/final.fsh", enc("void main(){}"));
    await store.flush();
    const listed = await store.listFiles();
    const a = dec(await store.readFile("shaderpacks/A.zip.txt")), b = dec(await store.readFile("shaderpacks/B.zip"));
    const missing = await store.readFile("gone.txt");
    await store.deleteFile("nothing/here.txt");
    return { listed, a, b, missing: missing === undefined };
  });
  assert.deepEqual(roundTrip, {
    listed: ["horizon-shaders.txt", "shaderpacks/A.zip.txt", "shaderpacks/B.zip", "shaderpacks/Folder/shaders/final.fsh"],
    a: "first", b: "two", missing: true,
  });

  // The picker: a hidden file input opened from the click handler.
  const zip = syntheticZip();
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.evaluate(() => window.horizonPickShaderPack())]);
  assert(chooser.isMultiple());
  await chooser.setFiles([
    { name: "My Pack [v2] §a+.zip", mimeType: "application/zip", buffer: zip },
    { name: "notes.txt", mimeType: "text/plain", buffer: Buffer.from("not a pack") },
  ]);
  await page.waitForFunction(() => window.changes === 1);
  await page.evaluate(() => store.flush());
  assert.deepEqual(await page.evaluate(() => installs), [["shaderpacks/My Pack [v2] §a+.zip", "PK\u0003\u0004"]]);
  assert.equal(await page.locator("#horizon-shaderpack-input").getAttribute("accept"), ".zip,application/zip");
  // The player hears of each file (the game's chat and the Shader Packs page).
  assert.deepEqual(await page.evaluate(() => notices.splice(0)),
    ["Added shader pack My Pack [v2] §a+.zip.", "Not added: notes.txt is not a .zip file."]);

  // Drop: a synthetic drop of a zip (name with a slash) and a non-zip.
  const dropped = await page.evaluate(async (bytes) => {
    const data = new DataTransfer();
    data.items.add(new File([new Uint8Array(bytes)], "sub/Dropped.zip", { type: "application/zip" }));
    data.items.add(new File(["text"], "readme.md", { type: "text/markdown" }));
    const over = new DragEvent("dragover", { dataTransfer: data, cancelable: true, bubbles: true });
    document.body.dispatchEvent(over);
    const drop = new DragEvent("drop", { dataTransfer: data, cancelable: true, bubbles: true });
    document.body.dispatchEvent(drop);
    while (window.changes < 2) await new Promise((r) => setTimeout(r, 20));
    await store.flush();
    return { prevented: over.defaultPrevented && drop.defaultPrevented, installs: installs.slice(1), files: await store.listFiles() };
  }, [...zip]);
  assert.equal(dropped.prevented, true);
  assert.deepEqual(dropped.installs, [["shaderpacks/sub_Dropped.zip", "PK\u0003\u0004"]]);
  assert(dropped.files.includes("shaderpacks/sub_Dropped.zip") && dropped.files.includes("shaderpacks/My Pack [v2] §a+.zip"));
  assert(!dropped.files.some((f) => f.endsWith("readme.md") || f.endsWith("notes.txt")));

  // Away from the game's Shader Packs page a drop is not taken (nor opened by the browser).
  const ignored = await page.evaluate(async (bytes) => {
    window.acceptDrops = false;
    const data = new DataTransfer();
    data.items.add(new File([new Uint8Array(bytes)], "Elsewhere.zip", { type: "application/zip" }));
    const drop = new DragEvent("drop", { dataTransfer: data, cancelable: true, bubbles: true });
    document.body.dispatchEvent(drop);
    await new Promise((r) => setTimeout(r, 100));
    await store.flush();
    window.acceptDrops = true;
    return { prevented: drop.defaultPrevented, changes, files: await store.listFiles(), notices: notices.splice(0) };
  }, [...zip]);
  assert.equal(ignored.prevented, true);
  assert.deepEqual(ignored.notices.slice(-1), ["Not added: drop shader packs on Options > Horizon Settings > Shader Packs."]);
  assert.equal(ignored.changes, 2);
  assert(!ignored.files.includes("shaderpacks/Elsewhere.zip"));

  // A file the browser reads as empty is not added: the game would list a pack that can
  // never load ("Added", then "could not be loaded").
  const empty = await page.evaluate(async () => {
    const before = installs.length, changed = changes;
    const added = await hooks.addFiles([new File([], "Empty.zip", { type: "application/zip" })]);
    await store.flush();
    return { counts: [added, installs.length - before, changes - changed], files: await store.listFiles(), notices: notices.splice(0) };
  });
  assert.deepEqual(empty.notices, ["Not added: Empty.zip is empty (the browser read 0 bytes of it)."]);
  assert.deepEqual(empty.counts, [0, 0, 0]);
  assert(!empty.files.includes("shaderpacks/Empty.zip"));

  // Clipboard.
  await page.bringToFront();
  await page.evaluate(() => window.horizonCopyText("Status: Works\nline 2"));
  await page.waitForFunction(async () => (await navigator.clipboard.readText()) === "Status: Works\nline 2");

  // A browser restart keeps every file and mirrorInto puts them back.
  await context.close();
  page = await open();
  const mirrored = await page.evaluate(async () => {
    const count = await store.mirrorInto((path, bytes) => installs.push([path, bytes.length]));
    return { count, paths: installs.map(([p]) => p) };
  });
  assert.equal(mirrored.count, 6);
  assert.deepEqual(mirrored.paths, [
    "horizon-shaders.txt", "shaderpacks/A.zip.txt", "shaderpacks/B.zip", "shaderpacks/Folder/shaders/final.fsh",
    "shaderpacks/My Pack [v2] §a+.zip", "shaderpacks/sub_Dropped.zip",
  ]);
  const pack = await page.evaluate(async () => [...(await store.readFile("shaderpacks/sub_Dropped.zip"))]);
  assert.deepEqual(Buffer.from(pack), zip);

  // Boot: the selection names B.zip (shaders on), so only it of the packs is read; the
  // others are empty placeholders until the game asks for one.
  const booted = await page.evaluate(async () => {
    const got = [];
    const count = await store.mirrorAtBoot((path, bytes) => got.push([path, bytes.length]), await store.listFiles());
    return { count, got };
  });
  assert.equal(booted.count, 4);
  assert.deepEqual(booted.got, [
    ["shaderpacks/My Pack [v2] §a+.zip", 0], ["shaderpacks/sub_Dropped.zip", 0],
    ["horizon-shaders.txt", 24], ["shaderpacks/A.zip.txt", 5], ["shaderpacks/B.zip", 3],
    ["shaderpacks/Folder/shaders/final.fsh", 13],
  ]);
  const loaded = await page.evaluate(async () => {
    const before = installs.length, changed = changes;
    window.horizonLoadShaderPack("shaderpacks/sub_Dropped.zip");
    while (changes === changed) await new Promise((r) => setTimeout(r, 20));
    window.horizonLoadShaderPack("shaderpacks/Missing.zip");
    while (notices.length === 0) await new Promise((r) => setTimeout(r, 20));
    return { installs: installs.slice(before), notices: notices.splice(0) };
  });
  assert.deepEqual(loaded.installs, [["shaderpacks/sub_Dropped.zip", "PK\u0003\u0004"]]);
  assert.deepEqual(loaded.notices, ["Shader pack Missing.zip is not in this browser's storage: add it again."]);

  // A stored pack with no bytes (a write cut short) is removed at boot instead of listed,
  // chosen or not.
  const cut = await page.evaluate(async () => {
    await store.writeFile("shaderpacks/Cut.zip", new Uint8Array(0));
    await store.writeFile("shaderpacks/Chosen.zip", new Uint8Array(0));
    await store.writeFile("horizon-shaders.txt", new TextEncoder().encode("enabled:true\npack:Chosen.zip\n"));
    const got = [], empty = [];
    await store.mirrorAtBoot((path) => got.push(path), await store.listFiles(), (path) => empty.push(path));
    await store.flush();
    return { got, empty, files: await store.listFiles() };
  });
  assert.deepEqual(cut.empty, ["shaderpacks/Cut.zip", "shaderpacks/Chosen.zip"]);
  assert(!cut.got.some((path) => /Cut|Chosen/.test(path)) && cut.got.includes("shaderpacks/B.zip"));
  assert(!cut.files.some((path) => /Cut|Chosen/.test(path)));

  // A write the browser cuts short leaves no empty file behind.
  const cutShort = await page.evaluate(async () => {
    const write = FileSystemWritableFileStream.prototype.write;
    FileSystemWritableFileStream.prototype.write = () => Promise.reject(new DOMException("Full", "QuotaExceededError"));
    let error = "";
    try { await store.writeFile("shaderpacks/Full.zip", new Uint8Array([1, 2, 3])); } catch (e) { error = e.name; }
    FileSystemWritableFileStream.prototype.write = write;
    return { error, files: await store.listFiles() };
  });
  assert.equal(cutShort.error, "QuotaExceededError");
  assert(!cutShort.files.includes("shaderpacks/Full.zip"));

  // An OPFS that throws: mirrorInto and horizonPersist report nothing to the caller.
  const failed = await page.evaluate(async () => {
    navigator.storage.getDirectory = () => { throw new DOMException("Denied", "SecurityError"); };
    const before = installs.length;
    const count = await store.mirrorInto((path) => installs.push([path, 0]));
    window.horizonPersist("shaderpacks/x.zip", new Uint8Array([1]));
    await store.flush();
    let listThrew = false;
    try { await store.listFiles(); } catch { listThrew = true; }
    return { count, added: installs.length - before, listThrew, notices: notices.splice(0) };
  });
  // The player hears of the failed write (the game's chat).
  assert.equal(failed.notices.length, 1);
  assert.match(failed.notices[0], /shaderpacks\/x\.zip could not be kept in this browser/);
  delete failed.notices;
  assert.deepEqual(failed, { count: 0, added: 0, listThrew: true });
});
