import test from "node:test";
import assert from "node:assert/strict";
import { bundledPackPath, installBundledPack } from "../menu/game.ts";

const asset = { url: "/assets/abc/miniature-shader-2.19.zip" };

test("the bundled shader pack goes in shaderpacks/ under its file name", () => {
  assert.equal(bundledPackPath(asset.url), "shaderpacks/miniature-shader-2.19.zip");
});

test("a pack the player stored under the same name wins, and a release without one installs nothing", async () => {
  const installed = [];
  const install = (path, bytes) => installed.push(path);
  await installBundledPack(asset, ["shaderpacks/miniature-shader-2.19.zip"], install);
  await installBundledPack(undefined, [], install);
  assert.deepEqual(installed, []);
});

test("a pack that can't be loaded leaves the game to start without it", async (t) => {
  t.mock.method(console, "warn", () => {});
  const installed = [];
  // No page here to resolve the asset against, and no Cache Storage: the load fails.
  await installBundledPack({ ...asset, sha256: "0".repeat(64), size: 1 }, [], (path) => installed.push(path));
  assert.deepEqual(installed, []);
  assert.equal(console.warn.mock.callCount(), 1);
});
