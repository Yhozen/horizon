// Two real Chrome clients against an isolated vanilla server. See WEB.md.
import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";

const { values } = parseArgs({ options: {
  origin: { type: "string", default: "http://127.0.0.1:8081" },
  operators: { type: "boolean", default: false }, "skip-background": { type: "boolean", default: false },
  screenshots: { type: "string" }, gpu: { type: "boolean", default: false },
} });
const names = ["BrowserOne", "BrowserTwo"], pages = [], sent = new Map();
const pause = (ms) => new Promise((done) => setTimeout(done, ms));
const isolated = await mkdtemp(join(tmpdir(), "horizon-web-multiplayer-"));
const env = { ...process.env, HORIZON_LAUNCHER_NO_SELF_UPDATE: "1", HORIZON_NO_AUDIO: "1" };
for (const name of ["HORIZON_HOME", "HORIZON_LAUNCHER_HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME"]) {
  env[name] = join(isolated, name); await mkdir(env[name]);
}
let browser, fail;
const failed = new Promise((_, reject) => { fail = reject; });
try {
  browser = await chromium.launch({ executablePath: process.env.CHROME_PATH ?? "/usr/bin/google-chrome",
    headless: true, env, args: ["--no-sandbox", "--disable-dev-shm-usage", "--enable-unsafe-webgpu",
      "--disable-background-timer-throttling", "--disable-renderer-backgrounding", ...(values.gpu ? [] : [
        "--enable-features=Vulkan", "--use-webgpu-adapter=swiftshader", "--enable-unsafe-swiftshader",
        "--use-angle=swiftshader", "--use-gl=angle",
      ])] });
  await Promise.race([exercise(), failed]);
} finally {
  await browser?.close();
  await rm(isolated, { recursive: true, force: true });
}

async function loaded(page) {
  await page.waitForFunction(() => window.horizonGame?.remote?.loaded || window.horizonError, null, { timeout: 180_000 });
  assert.equal(await page.evaluate(() => window.horizonError ?? null), null);
}
async function peers(page) {
  await page.waitForFunction((names) => names.every((n) => window.horizonGame?.remote?.players?.includes(n))
    && window.horizonGame?.remote?.entities?.["minecraft:player"] >= 1, names, { timeout: 30_000 });
}
async function chat(page, text) {
  await page.bringToFront(); await page.locator("#horizon").focus();
  await page.keyboard.press("t");
  await page.waitForFunction(() => window.horizonGame?.screen === "Chat", null, { timeout: 15_000 });
  // Canvas keyboard input needs key events; insertText alone does not reach winit.
  await page.keyboard.type(text, { delay: 15 }); await page.keyboard.press("Enter");
  await page.waitForFunction(() => window.horizonGame?.screen === "Playing", null, { timeout: 15_000 });
}
async function target(z) {
  for (const page of pages) await page.waitForFunction((z) => {
    const target = window.horizonGame?.target;
    return target?.block === "minecraft:diamond_block" && target.pos[2] === z;
  }, z, { timeout: 20_000 });
}

async function exercise() {
  for (const name of names) {
    const context = await browser.newContext({ viewport: { width: 660, height: 460 } });
    const page = await context.newPage(); pages.push(page); sent.set(page, 0);
    await page.addInitScript(browserHelpers, Boolean(values.screenshots));
    page.on("console", (message) => {
      const text = message.text();
      if (/panicked at|Horizon: disconnected:/.test(text)) fail(new Error(`${name}: ${text}`));
    });
    page.on("pageerror", (error) => {
      if (!/user gesture.*Pointer Lock/i.test(error.message)) fail(error);
    });
    page.on("websocket", (socket) => socket.on("framesent", () => sent.set(page, sent.get(page) + 1)));
    const args = `--server server --name ${name} --render-distance 2 --in-frame --width 640 --height 360 --no-pause`;
    await page.goto(`${values.origin}/?args=${encodeURIComponent(args)}`);
    await loaded(page);
  }
  for (const page of pages) {
    await peers(page);
    const state = await page.evaluate(() => window.horizonGame);
    assert(state.remote.joined && state.remote.loaded && state.chunks > 0 && state.meshed > 0);
    assert.equal(state.remote.decode_errors, 0);
  }
  console.log("Both clients loaded terrain and the other player's entity; no decode errors.");
  const [first, second] = pages, message = `Horizon multiplayer test ${Date.now()}`;
  await chat(first, message);
  await second.waitForFunction((text) => window.horizonGame.remote.chat.some((line) => line.includes(text)), message, { timeout: 20_000 });
  const before = await first.evaluate(() => window.horizonGame.player);
  await first.keyboard.down("w"); await pause(2000); await first.keyboard.up("w"); await pause(2500);
  const after = await first.evaluate(() => window.horizonGame.player);
  assert(Math.hypot(after[0] - before[0], after[2] - before[2]) > 0.5, "player moved");
  await second.waitForFunction((at) => window.horizonGame.remote.player_positions.some((p) =>
    Math.hypot(p[0] - at[0], p[2] - at[2]) < 0.3), after, { timeout: 20_000 });
  console.log("Chat and authoritative movement reached the other client.");

  if (values.operators) {
    // These commands are for the disposable flat test server, never a player's world.
    await chat(first, "/tp @s 0.5 -60 3.5 180 0");
    await chat(first, "/setblock 0 -59 0 minecraft:diamond_block");
    await chat(second, "/tp @s 1.5 -60 4.5 165.96376 0");
    await target(0);
    await first.bringToFront(); await first.locator("#horizon").focus();
    // The first press may capture the pointer; a second pick is harmless.
    // Pointer lock routes raw mouse events to the canvas; locator hit testing can
    // instead see the session form at the locked pointer's stale screen position.
    for (let i = 0; i < 2; i++) { await first.mouse.click(320, 220, { button: "middle" }); await pause(300); }
    await first.waitForFunction(() => window.horizonGame.held === "minecraft:diamond_block", null, { timeout: 15_000 });
    await first.mouse.down({ button: "right" }); await first.mouse.up({ button: "right" }); await target(1);
    await first.mouse.down({ button: "left" }); await first.mouse.up({ button: "left" }); await target(0);
    console.log("Server block updates, mouse placement and breaking reached both game worlds.");
  }

  if (!values["skip-background"]) {
    await second.evaluate(() => { window.__pauseTestFrames = true; }); await pause(3000);
    const frame = await second.evaluate(() => window.horizonGame.frame), count = sent.get(second);
    await pause(45_000);
    assert.equal(await second.evaluate(() => window.horizonGame.frame), frame, "animation frames stayed paused");
    assert(sent.get(second) > count, "network callbacks answered keep-alives without animation frames");
    await peers(first);
    await second.evaluate(() => window.__resumeTestFrames());
    await second.waitForFunction((frame) => window.horizonGame.frame > frame, frame, { timeout: 15_000 });
    console.log("The paused tab stayed connected for 45 seconds and resumed.");
  }

  await first.reload(); await loaded(first);
  for (const page of pages) await peers(page);
  if (values.screenshots) {
    await mkdir(values.screenshots, { recursive: true });
    for (let i = 0; i < pages.length; i++) {
      await pages[i].bringToFront(); await pages[i].locator("#horizon").focus();
      await pages[i].keyboard.down("Tab");
      const image = await pages[i].evaluate(() => window.__gameShot());
      await writeFile(join(values.screenshots, `${names[i]}.png`), Buffer.from(image.split(",")[1], "base64"));
      await pages[i].keyboard.up("Tab");
    }
  }
  const results = await Promise.all(pages.map((page) => page.evaluate(() => window.horizonGame)));
  for (const result of results) assert.equal(result.remote.decode_errors, 0);
  const destination = resolve(".build/multiplayer-test.json");
  await mkdir(resolve(".build"), { recursive: true });
  await writeFile(destination, JSON.stringify(results, null, 2));
  console.log(`Reload/rejoin passed. Results: ${destination}`);
}

function browserHelpers(screenshots) {
  const raf = window.requestAnimationFrame.bind(window); let held;
  window.requestAnimationFrame = (callback) => raf((time) => {
    if (window.__pauseTestFrames) held = callback; else callback(time);
  });
  window.__resumeTestFrames = () => { window.__pauseTestFrames = false; if (held) { raf(held); held = null; } };
  if (!screenshots) return;
  // Headless SwiftShader renders WebGPU but cannot composite its canvas. Read back a frame.
  let device, texture, shot;
  const configure = GPUCanvasContext.prototype.configure, current = GPUCanvasContext.prototype.getCurrentTexture;
  GPUCanvasContext.prototype.configure = function (config) {
    device = config.device;
    return configure.call(this, { ...config, usage: (config.usage ?? GPUTextureUsage.RENDER_ATTACHMENT) | GPUTextureUsage.COPY_SRC });
  };
  GPUCanvasContext.prototype.getCurrentTexture = function () { return (texture = current.call(this)); };
  const submit = GPUQueue.prototype.submit;
  GPUQueue.prototype.submit = function (commands) {
    const result = submit.call(this, commands);
    if (shot && texture) {
      const done = shot; shot = null;
      const { width, height } = texture, row = Math.ceil(width * 4 / 256) * 256;
      const buffer = device.createBuffer({ size: row * height, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
      const encoder = device.createCommandEncoder();
      encoder.copyTextureToBuffer({ texture }, { buffer, bytesPerRow: row }, { width, height });
      submit.call(this, [encoder.finish()]);
      buffer.mapAsync(GPUMapMode.READ).then(() => {
        const source = new Uint8Array(buffer.getMappedRange()), pixels = new Uint8ClampedArray(width * height * 4);
        const [red, blue] = texture.format.startsWith("bgra") ? [2, 0] : [0, 2];
        for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
          const from = y * row + x * 4, to = (y * width + x) * 4;
          pixels[to] = source[from + red]; pixels[to + 1] = source[from + 1]; pixels[to + 2] = source[from + blue]; pixels[to + 3] = 255;
        }
        buffer.unmap(); buffer.destroy();
        const canvas = document.createElement("canvas"); canvas.width = width; canvas.height = height;
        canvas.getContext("2d").putImageData(new ImageData(pixels, width, height), 0, 0); done(canvas.toDataURL("image/png"));
      });
    }
    return result;
  };
  window.__gameShot = () => new Promise((resolve) => { shot = resolve; });
}
