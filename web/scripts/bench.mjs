// Frame rate of the browser build (WEB.md, "Frame rate"): serves .build/ (pnpm package), or with
// --site open .build-open/static (pnpm build:open) on 8081, opens the page in Chrome and reads the
// game's two-second status lines. --shaderpack serves a local zip to the page as ?shaderpack=.
//   node scripts/bench.mjs [--site open] [--args="GAME ARGS"] [--shaderpack pack.zip] [--seconds 60]
//     [--tail 15] [--shot frame.png] [--profile main.cpuprofile] [--trace trace.json]
//     [--release other/release.json] [--gpu] [--verbose] [--strict] [--expect-chunks N]
// Without --gpu, WebGPU runs on SwiftShader (a machine without a GPU).
import { chromium } from "playwright";
import { spawn } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { parseArgs } from "node:util";
import { fileURLToPath } from "node:url";
import { watchFrameRate } from "./bench-frames.mjs";

const { values } = parseArgs({ options: {
  args: { type: "string" }, seconds: { type: "string", default: "60" }, tail: { type: "string", default: "15" },
  shot: { type: "string" }, profile: { type: "string" }, trace: { type: "string" }, release: { type: "string" },
  site: { type: "string", default: "horizon" }, shaderpack: { type: "string" },
  gpu: { type: "boolean", default: false }, verbose: { type: "boolean", default: false },
  strict: { type: "boolean", default: false }, "expect-chunks": { type: "string" },
} });
const seconds = Number(values.seconds), tail = Number(values.tail);
const web = fileURLToPath(new URL("..", import.meta.url));
if (values.site !== "horizon" && values.site !== "open") throw new Error("--site is horizon or open");
const open = values.site === "open", origin = `http://127.0.0.1:${open ? 8081 : 8080}`;
const server = spawn(process.execPath, [open ? "scripts/serve-open.mjs" : "scripts/serve.mjs"], { cwd: web, stdio: "ignore" });
await new Promise((done) => setTimeout(done, 700));
const flags = values.gpu ? [] : ["--enable-features=Vulkan", "--use-webgpu-adapter=swiftshader",
  "--enable-unsafe-swiftshader", "--use-angle=swiftshader", "--use-gl=angle"];
const browser = await chromium.launch({ executablePath: process.env.CHROME_PATH ?? "/usr/bin/google-chrome",
  headless: true, args: ["--no-sandbox", "--disable-dev-shm-usage", "--enable-unsafe-webgpu", ...flags] });
try {
  const page = await browser.newPage({ viewport: { width: 1300, height: 800 } });
  // Reads the canvas back through WebGPU right after a frame's submit: headless Chrome without a
  // GPU cannot composite a WebGPU canvas into a page screenshot.
  await page.addInitScript(() => {
    let device, texture, want = null;
    const configure = GPUCanvasContext.prototype.configure;
    GPUCanvasContext.prototype.configure = function (c) {
      device = c.device;
      return configure.call(this, { ...c, usage: (c.usage ?? GPUTextureUsage.RENDER_ATTACHMENT) | GPUTextureUsage.COPY_SRC });
    };
    const current = GPUCanvasContext.prototype.getCurrentTexture;
    GPUCanvasContext.prototype.getCurrentTexture = function () { return (texture = current.call(this)); };
    const submit = GPUQueue.prototype.submit;
    GPUQueue.prototype.submit = function (buffers) {
      const result = submit.call(this, buffers);
      if (want && texture) {
        const done = want; want = null;
        const { width, height } = texture, row = Math.ceil(width * 4 / 256) * 256;
        const buffer = device.createBuffer({ size: row * height, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
        const encoder = device.createCommandEncoder();
        encoder.copyTextureToBuffer({ texture }, { buffer, bytesPerRow: row }, { width, height });
        submit.call(this, [encoder.finish()]);
        buffer.mapAsync(GPUMapMode.READ).then(() => {
          const src = new Uint8Array(buffer.getMappedRange()), px = new Uint8ClampedArray(width * height * 4);
          const [r, b] = texture.format.startsWith("bgra") ? [2, 0] : [0, 2];
          for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
            const i = y * row + x * 4, o = (y * width + x) * 4;
            px[o] = src[i + r]; px[o + 1] = src[i + 1]; px[o + 2] = src[i + b]; px[o + 3] = 255;
          }
          const canvas = document.createElement("canvas");
          canvas.width = width; canvas.height = height;
          canvas.getContext("2d").putImageData(new ImageData(px, width, height), 0, 0);
          done(canvas.toDataURL("image/png"));
        });
      }
      return result;
    };
    window.horizonShot = () => new Promise((resolve) => { want = resolve; });
  });
  if (values.release) {
    const body = await readFile(values.release, "utf8");
    await page.route("**/release.json", (route) => route.fulfill({ contentType: "application/json", body }));
  }
  if (values.shaderpack) {
    const body = await readFile(values.shaderpack);
    await page.route(`${origin}/bench-shaderpack.zip`, (route) => route.fulfill({ contentType: "application/zip", body }));
  }
  const reportFrameRate = watchFrameRate(page, {
    tail, strict: values.strict, expectedChunks: values["expect-chunks"], verbose: values.verbose,
  });
  const query = new URLSearchParams({ name: "Bench" });
  if (values.args) query.set("args", values.args);
  if (values.shaderpack) query.set("shaderpack", `${origin}/bench-shaderpack.zip`);
  await page.goto(`${origin}/?${query}`);
  // The menu starts automatically with ?args=. Older pages can still need a click.
  const play = page.getByRole("button", { name: /^Play/ }).first();
  await Promise.race([
    play.waitFor({ timeout: 30000 }).then(() => play.click(), () => {}),
    page.waitForFunction(() => window.horizonStatus, null, { timeout: 30000 }).catch(() => {}),
  ]);
  await page.waitForFunction(() => window.horizonStarted || window.horizonError, null, { timeout: 300000 });
  const wait = (fraction) => new Promise((done) => setTimeout(done, seconds * 1000 * fraction));
  if (values.profile) {
    // The second half, once the world has loaded.
    await wait(0.5);
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Profiler.enable");
    await cdp.send("Profiler.setSamplingInterval", { interval: 200 });
    await cdp.send("Profiler.start");
    await wait(0.5);
    await writeFile(values.profile, JSON.stringify((await cdp.send("Profiler.stop")).profile));
  } else if (values.trace) {
    await wait(0.8);
    await browser.startTracing(page, { path: values.trace, categories: ["gpu", "viz", "cc", "blink", "toplevel",
      "disabled-by-default-gpu.dawn", "disabled-by-default-gpu.service", "v8", "devtools.timeline"] });
    await wait(0.2);
    await browser.stopTracing();
  } else {
    await wait(1);
  }
  if (values.shot) {
    const url = await page.evaluate(() => window.horizonShot());
    await writeFile(values.shot, Buffer.from(url.split(",")[1], "base64"));
  }
  reportFrameRate(await page.evaluate(() => window.horizonError ?? null));
} finally {
  await browser.close();
  server.kill();
}
