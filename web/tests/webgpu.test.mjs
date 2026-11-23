import test from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { extname, join, resolve } from "node:path";
import { chromium } from "playwright";

// WebKit bug 317110 rejects an override declared in the module but unused by
// the entry point. Inspect what the built game actually passes to the browser.
function checkShaderOverrides(stages) {
  const constantsFor = (entry) => {
    const values = stages.filter((stage) => stage.entry === entry).map((stage) => stage.constants);
    assert.ok(values.length, `missing pipeline stage ${entry}`);
    return values;
  };
  const [sprites] = constantsFor("fs_solid");
  assert.deepEqual(Object.keys(sprites).sort(), ["SPRITE_ARRAYS", "SPRITE_GRID", "SPRITE_SPLIT"]);
  assert.ok(Object.values(sprites).every((v) => Number.isInteger(v) && v > 0));
  for (const entry of ["fs_solid", "fs_translucent", "vs_terrain", "vs_terrain_flat"]) {
    const expected = entry.startsWith("fs_") ? sprites : { SPRITE_GRID: sprites.SPRITE_GRID };
    for (const constants of constantsFor(entry)) assert.deepEqual(constants, expected, entry);
  }
  for (const entry of ["vs_sky", "fs_sky", "vs_sunrise", "fs_sunrise", "fs_dark_disc",
    "vs_line", "fs_line", "vs_debug_line", "fs_debug_line", "vs_screen"]) {
    for (const constants of constantsFor(entry)) assert.deepEqual(constants, {}, entry);
  }
  for (const entry of ["fs_screen", "fs_vignette"]) {
    const modes = constantsFor(entry).map((constants) => {
      assert.deepEqual(Object.keys(constants).sort(), ["BLUR", "CLAMP"], entry);
      return [constants.BLUR, constants.CLAMP];
    });
    assert.deepEqual(modes.sort(), [[0, 0], [0, 1], [1, 0], [1, 1]], entry);
  }
}

// Run against a built site with HORIZON_OPEN_SITE=.build-open/static pnpm test.
const site = process.env.HORIZON_OPEN_SITE;
test("WebGPU startup and upload fallback in Open Creative", { skip: !site, timeout: 120_000 }, async (t) => {
  const root = resolve(site);
  const types = { ".html": "text/html", ".js": "text/javascript", ".json": "application/json",
    ".css": "text/css", ".wasm": "application/wasm", ".bin": "application/octet-stream" };
  const server = createServer(async (req, res) => {
    const path = new URL(req.url, "http://localhost").pathname;
    const file = path === "/" ? "/index.html" : path;
    try {
      const data = await readFile(root + file);
      res.setHeader("Content-Type", types[extname(file)]);
      res.end(data);
    } catch {
      res.writeHead(404).end();
    }
  });
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  const home = await mkdtemp(join(tmpdir(), "horizon-webgpu-"));
  const env = { ...process.env, HORIZON_LAUNCHER_NO_SELF_UPDATE: "1", HORIZON_NO_AUDIO: "1" };
  let browser;
  t.after(async () => {
    await browser?.close();
    await new Promise((done) => server.close(done));
    await rm(home, { recursive: true, force: true });
  });
  for (const key of ["HORIZON_HOME", "HORIZON_LAUNCHER_HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME"]) {
    env[key] = join(home, key);
    await mkdir(env[key]);
  }
  browser = await chromium.launch({ executablePath: process.env.CHROME_PATH ?? "/usr/bin/google-chrome",
    headless: true, env, args: ["--no-sandbox", "--disable-dev-shm-usage", "--enable-unsafe-webgpu",
      "--enable-features=Vulkan", "--use-webgpu-adapter=swiftshader", "--enable-unsafe-swiftshader",
      "--use-angle=swiftshader", "--use-gl=angle"] });
  const url = `http://127.0.0.1:${server.address().port}/?name=test&world=solo&mode=singleplayer`;
  const open = async (t, fault) => {
    const page = await browser.newPage({ viewport: { width: 640, height: 360 } });
    t.after(() => page.close());
    await page.addInitScript((fault) => {
      const state = window.gpuTest = { attempts: 0, transfers: { buffer: 0, texture: 0 }, mappedTransfers: 0 };
      if (fault === "noPointerLock") {
        Object.defineProperty(Element.prototype, "requestPointerLock", { value: undefined });
        Object.defineProperty(Document.prototype, "exitPointerLock", { value: undefined });
      }
      if (fault === "adapter") {
        GPU.prototype.requestAdapter = async () => { state.attempts++; return null; };
        const delay = window.setTimeout;
        window.setTimeout = (fn, ms, ...args) => delay(fn, ms === 500 ? 0 : ms, ...args);
        return;
      }
      const request = GPUAdapter.prototype.requestDevice;
      GPUAdapter.prototype.requestDevice = async function (...args) {
        const device = await request.apply(this, args);
        state.device = device;
        return device;
      };
      if (fault === "panorama") {
        // Simulate Dawn returning the phone's internal error for just the
        // panorama. All other scopes and pipeline compilation run normally.
        const scopes = [];
        const push = GPUDevice.prototype.pushErrorScope;
        GPUDevice.prototype.pushErrorScope = function (filter) {
          scopes.push({ filter });
          return push.call(this, filter);
        };
        const pop = GPUDevice.prototype.popErrorScope;
        GPUDevice.prototype.popErrorScope = function () {
          const scope = scopes.pop();
          const verdict = pop.call(this);
          if (!scope?.error) return verdict;
          return verdict.then((error) => {
            state.returnedPanoramaError = true;
            return error ?? scope.error;
          });
        };
        const create = GPUDevice.prototype.createRenderPipeline;
        GPUDevice.prototype.createRenderPipeline = function (descriptor) {
          if (descriptor.label === "Title panorama") {
            const error = new GPUInternalError("CreateGraphicsPipelines failed with VK_ERROR_UNKNOWN");
            const scope = scopes.findLast((scope) => scope.filter === "internal");
            if (scope) scope.error = error;
            else queueMicrotask(() => this.dispatchEvent(new GPUUncapturedErrorEvent("uncapturederror", { error })));
          }
          return create.call(this, descriptor);
        };
        const set = GPURenderPassEncoder.prototype.setPipeline;
        GPURenderPassEncoder.prototype.setPipeline = function (pipeline) {
          if (pipeline.label === "Title panorama") state.usedPanorama = true;
          return set.call(this, pipeline);
        };
      }
      state.stages = [];
      const createPipeline = GPUDevice.prototype.createRenderPipeline;
      GPUDevice.prototype.createRenderPipeline = function (descriptor) {
        for (const stage of [descriptor.vertex, descriptor.fragment]) {
          if (stage) state.stages.push({ entry: stage.entryPoint, constants: { ...stage.constants } });
        }
        return createPipeline.call(this, descriptor);
      };
      window.addEventListener("horizon-render-ready", () => { state.ready = true; });
      const configure = GPUCanvasContext.prototype.configure;
      GPUCanvasContext.prototype.configure = function (options) {
        return configure.call(this, { ...options,
          usage: (options.usage ?? GPUTextureUsage.RENDER_ATTACHMENT) | GPUTextureUsage.COPY_SRC });
      };
      const getTexture = GPUCanvasContext.prototype.getCurrentTexture;
      GPUCanvasContext.prototype.getCurrentTexture = function () {
        return state.texture = getTexture.call(this);
      };
      for (const [kind, method] of [["buffer", "copyBufferToBuffer"], ["texture", "copyBufferToTexture"]]) {
        const copy = GPUCommandEncoder.prototype[method];
        GPUCommandEncoder.prototype[method] = function (source, ...args) {
          const buffer = kind === "buffer" ? source : source.buffer;
          if (buffer.label === "web staging transfer") {
            state.transfers[kind]++;
            if (buffer.mapState !== "unmapped") state.mappedTransfers++;
          }
          return copy.call(this, source, ...args);
        };
      }
      const submit = GPUQueue.prototype.submit;
      GPUQueue.prototype.submit = function (commands) {
        const { device, texture } = state;
        state.texture = undefined;
        const reject = fault === "probe" ? commands.some((command) => command.label === "Web staging probe") : fault === "frame" && texture;
        if (!state.injected && reject) {
          state.injected = true;
          // Resubmitting a command buffer raises a real WebGPU validation error in the active scope.
          submit.call(this, commands);
        }
        submit.call(this, commands);
        if (state.readFrame && texture) {
          state.readFrame = false;
          const { width, height } = texture;
          const bytesPerRow = Math.ceil(width * 4 / 256) * 256;
          const buffer = device.createBuffer({ size: bytesPerRow * height,
            usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
          const encoder = device.createCommandEncoder();
          encoder.copyTextureToBuffer({ texture }, { buffer, bytesPerRow }, { width, height });
          submit.call(this, [encoder.finish()]);
          buffer.mapAsync(GPUMapMode.READ).then(() => {
            const bytes = new Uint8Array(buffer.getMappedRange());
            state.visiblePixels = bytes.some((byte, index) => index % 4 !== 3 && byte !== 0);
            buffer.unmap();
            buffer.destroy();
          }, (error) => { state.visiblePixels = String(error); });
        }
      };
    }, fault);
    await page.goto(url);
    await page.getByRole("button", { name: "Play single player" }).click();
    return page;
  };

  await t.test("adapter exhaustion stops before downloading the game", async (t) => {
    const page = await open(t, "adapter");
    await page.getByRole("alert").filter({ hasText: "WebGPU could not find a usable graphics adapter." }).waitFor();
    assert.deepEqual(await page.evaluate(() => [window.gpuTest.attempts, window.horizonRelease]), [20, undefined]);
  });

  await t.test("a player spawns and rendering continues without the Pointer Lock API", async (t) => {
    const page = await open(t, "noPointerLock");
    await page.waitForFunction(() => window.horizonGame?.player, null, { timeout: 30_000 });
    const frame = await page.evaluate(() => window.horizonGame.frame);
    await page.waitForFunction((previous) => window.horizonGame.frame > previous, frame);
  });

  await t.test("the world renders when the Android driver rejects the title panorama", async (t) => {
    const page = await open(t, "panorama");
    await page.waitForFunction(() => window.horizonError ||
      (window.horizonStarted && window.horizonGame?.meshed > 0), null, { timeout: 50_000 });
    const result = await page.evaluate(() => ({
      error: window.horizonError || null,
      captured: !!window.gpuTest.returnedPanoramaError,
      used: !!window.gpuTest.usedPanorama,
    }));
    assert.deepEqual(result, { error: null, captured: true, used: false });
  });

  await t.test("first-frame validation failure reaches the menu", async (t) => {
    const page = await open(t, "frame");
    await page.getByRole("alert").filter({ hasText: /WebGPU startup failed/ }).waitFor({ timeout: 45_000 });
    assert.deepEqual(await page.evaluate(() => [window.horizonStarted, window.gpuTest.ready]), [false, undefined]);
    assert(await page.getByRole("button", { name: "Reload page" }).isVisible());
  });

  await t.test("probe rejection selects unmapped uploads and device loss restores the menu", async (t) => {
    const page = await open(t, "probe");
    await page.waitForFunction(() => window.horizonStarted && window.horizonGame?.meshed > 0 &&
      window.gpuTest.transfers.buffer > 0 && window.gpuTest.transfers.texture > 0, null, { timeout: 50_000 });
    await t.test("shader overrides match each pipeline entry point", async () => {
      checkShaderOverrides(await page.evaluate(() => window.gpuTest.stages));
    });
    assert.equal(await page.evaluate(() => window.gpuTest.mappedTransfers), 0);
    await page.evaluate(() => { window.gpuTest.readFrame = true; });
    await page.waitForFunction(() => window.gpuTest.visiblePixels !== undefined);
    assert.equal(await page.evaluate(() => window.gpuTest.visiblePixels), true);
    await page.evaluate(() => window.gpuTest.device.destroy());
    await page.getByRole("alert").filter({ hasText: /WebGPU device lost/ }).waitFor();
  });
});
