import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, readFile, rm, readdir } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { packageOpenRelease, checkBundle } from "../scripts/open.mjs";
import { validateRelease } from "../release.js";

async function fixture(t, { profile = "open-creative", generator = "tables" } = {}) {
  const root = await mkdtemp(join(tmpdir(), "horizon-open-package-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const web = join(root, "web"), dist = join(root, "dist"), resources = join(root, "resources");
  await mkdir(join(web, "open"), { recursive: true });
  for (const file of ["open/index.html", "cache.js", "release.js"]) await writeFile(join(web, file), `fixture ${file}`);
  await mkdir(dist);
  await writeFile(join(dist, "horizon-client.js"), "export default () => {};");
  await writeFile(join(dist, "horizon-client_bg.wasm"), Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]));
  await mkdir(join(resources, "credits/licenses"), { recursive: true });
  await writeFile(join(resources, "horizon-resources.json"), JSON.stringify({ content_profile: profile, content_revision: 1 }));
  await writeFile(join(resources, "horizon-open-provenance.json"), JSON.stringify({ content_profile: profile,
    files: { "facts.json": { sha256: "0", generator } } }));
  await writeFile(join(resources, "credits/credits.json"), JSON.stringify({ title: "fixture credits" }));
  for (const file of ["CREDITS.md", "licenses/CC-BY-SA-3.0.txt", "licenses/CC0-1.0.txt", "licenses/OFL-PixelifySans.txt"]) {
    await writeFile(join(resources, "credits", file), file);
  }
  const archive = join(root, "horizon-data.bin");
  await writeFile(archive, "open archive");
  return { web, dist, resources, archive, output: join(root, "static") };
}

test("the open site holds the page, credits and immutable assets with an open release manifest", async (t) => {
  const paths = await fixture(t);
  const release = await packageOpenRelease(paths.web, paths);
  assert.equal(release.edition, "open-creative");
  assert.equal(release.content_revision, 1);
  validateRelease(release);
  assert.deepEqual(JSON.parse(await readFile(join(paths.output, "release.json"), "utf8")), release);
  const top = (await readdir(paths.output)).sort();
  assert.deepEqual(top, ["assets", "cache.js", "credits", "credits.json", "index.html", "menu.css", "menu.js", "release.js", "release.json"]);
  // The open site's menu has only its own art: none of the private site's Minecraft pictures.
  const css = await readFile(join(paths.output, "menu.css"), "utf8");
  assert.match(css, /\.art-meadow\{/);
  assert.doesNotMatch(css, /\.art-(village|night|sunset)\{/);
  assert.equal(await readFile(join(paths.output, release.assets.resources.url), "utf8"), "open archive");
  // Repackaging the same inputs gives the same release.
  assert.deepEqual(await packageOpenRelease(paths.web, paths), release);
  // Changing only the page changes the release, not the asset URLs.
  await writeFile(join(paths.web, "open/index.html"), "changed page");
  const page = await packageOpenRelease(paths.web, paths);
  assert.notEqual(page.id, release.id);
  assert.deepEqual(page.assets, release.assets);
});

test("only an open bundle with a complete provenance record is packaged", async (t) => {
  const vanilla = await fixture(t, { profile: "vanilla" });
  await assert.rejects(checkBundle(vanilla.resources), /not a Horizon Open Creative bundle/);
  await assert.rejects(packageOpenRelease(vanilla.web, vanilla), /not a Horizon Open Creative bundle/);
  const unattributed = await fixture(t, { generator: null });
  await assert.rejects(packageOpenRelease(unattributed.web, unattributed), /provenance/);
});

test("the open Worker serves its pages and opens /connect only for its own page's WebSocket, by world", async () => {
  const { route } = await import("../src/open-route.ts");
  const pages = { "/index.html": { body: "page", contentType: "text/html" } };
  const bucket = {};
  const opened = [];
  const worlds = { shared: async () => { opened.push("shared"); return new Response("shared"); } };
  const page = await route(new Request("https://open.example/"), bucket, pages, worlds);
  assert.equal(page.status, 200);
  assert.equal(await page.text(), "page");
  assert.equal((await route(new Request("https://open.example/missing"), bucket, pages, worlds)).status, 404);
  const upgrade = (origin, query = "") => new Request(`https://open.example/connect${query}`,
    { headers: { Upgrade: "websocket", ...(origin ? { Origin: origin } : {}) } });
  assert.equal((await route(new Request("https://open.example/connect"), bucket, pages, worlds)).status, 426);
  assert.equal((await route(upgrade("https://elsewhere.example"), bucket, pages, worlds)).status, 403);
  assert.equal((await route(upgrade(), bucket, pages, worlds)).status, 403);
  assert.deepEqual(opened, []);
  // No world named: the shared one, as older pages expect.
  assert.equal(await (await route(upgrade("https://open.example"), bucket, pages, worlds)).text(), "shared");
  assert.equal(await (await route(upgrade("https://open.example", "?world=shared"), bucket, pages, worlds)).text(), "shared");
  // A world this deployment doesn't serve, or any other name, is refused.
  assert.equal((await route(upgrade("https://open.example", "?world=survival"), bucket, pages, worlds)).status, 404);
  assert.equal((await route(upgrade("https://open.example", "?world=10.0.0.1:25565"), bucket, pages, worlds)).status, 404);
  assert.deepEqual(opened, ["shared", "shared"]);
  assert.equal((await route(upgrade("https://open.example"), bucket, pages)).status, 404);
  // The menu learns what is configured.
  const list = await route(new Request("https://open.example/worlds.json"), bucket, pages, worlds);
  assert.deepEqual(await list.json(), { worlds: ["shared"] });
  assert.equal((await route(new Request("https://open.example/", { method: "POST" }), bucket, pages)).status, 405);
});

test("a release may carry the shared open bundle, and it is checked like the others", async () => {
  const { validateRelease } = await import("../release.js");
  const hash = (c) => c.repeat(64);
  const asset = (c, filename, contentType) => ({ url: `/assets/${hash(c)}/${filename}`, sha256: hash(c), size: 1, contentType });
  const release = { format: 1, id: hash("a"), assets: {
    javascript: asset("b", "horizon-client.js", "text/javascript"),
    wasm: asset("c", "horizon-client_bg.wasm", "application/wasm"),
    resources: asset("d", "horizon-data.bin", "application/octet-stream"),
  } };
  validateRelease(release);
  release.assets.sharedResources = asset("e", "horizon-open-data.bin", "application/octet-stream");
  validateRelease(release);
  release.assets.sharedResources.url = "/assets/elsewhere/horizon-open-data.bin";
  assert.throws(() => validateRelease(release), /sharedResources/);
});
