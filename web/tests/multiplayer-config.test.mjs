import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { minecraftServer, parseMinecraftServer } from "../scripts/multiplayer-config.mjs";

test("shared Minecraft targets accept TCP endpoints and reject URLs or injected destinations", () => {
  assert.equal(parseMinecraftServer(), "");
  assert.equal(parseMinecraftServer("  MC.Example.com:25565  "), "mc.example.com:25565");
  assert.equal(parseMinecraftServer("127.0.0.1:25566"), "127.0.0.1:25566");
  assert.equal(parseMinecraftServer("[2001:db8::1]:25565"), "[2001:db8::1]:25565");
  for (const value of ["mc.example.com", "https://mc.example.com:25565", "user@mc.example.com:25565",
    "mc.example.com:25565/connect", "mc.example.com:25565?host=other", "mc.example.com:25565\nother:25565",
    "mc.example.com:0", "mc.example.com:65536", "[not-ipv6]:25565", "[::gg]:25565", "-bad.example:25565"]) {
    assert.throws(() => parseMinecraftServer(value), /must be HOST:PORT/, value);
  }
});

test("explicit local backend configuration overrides inherited targets and can select the container", async () => {
  const directory = await mkdtemp(join(tmpdir(), "horizon-multiplayer-config-"));
  const file = join(directory, ".env.local");
  try {
    assert.equal(await minecraftServer({}, file), "");
    assert.equal(await minecraftServer({}, file, "survival.minecraft.rccpr.tech:25565"),
      "survival.minecraft.rccpr.tech:25565");
    assert.equal(await minecraftServer({ HORIZON_MINECRAFT_SERVER: "inherited.example:25565" }, file), "inherited.example:25565");
    await writeFile(file, "HORIZON_MINECRAFT_SERVER=chosen.example:25566\n");
    assert.equal(await minecraftServer({ HORIZON_MINECRAFT_SERVER: "inherited.example:25565" }, file), "chosen.example:25566");
    await writeFile(file, "HORIZON_MINECRAFT_SERVER=\n");
    assert.equal(await minecraftServer({ HORIZON_MINECRAFT_SERVER: "inherited.example:25565" }, file), "");
  } finally { await rm(directory, { recursive: true, force: true }); }
});
