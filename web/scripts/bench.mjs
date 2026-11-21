// Frame rate of the browser build (WEB.md, "Frame rate"): serves .build/ (pnpm package), or with
// --site open .build-open/static (pnpm build:open) on 8081, opens the page in Chrome and reads the
// game's two-second status lines. --shaderpack serves a local zip to the page as ?shaderpack=.
//   node scripts/bench.mjs [--site open] [--args="GAME ARGS"] [--shaderpack pack.zip] [--seconds 60]
//     [--tail 15] [--shot frame.png] [--profile main.cpuprofile] [--trace trace.json]
//     [--release other/release.json] [--gpu] [--verbose] [--strict] [--expect-chunks N]
//     [--profile-dir DIR] [--reload] [--port N] [--log console.txt] [--files DIR] [--size WxH]
// Without --gpu, WebGPU runs on SwiftShader (a machine without a GPU). --profile-dir keeps the
// browser profile (OPFS included) in DIR; --reload then opens the page a second time in it
// without ?shaderpack= (the saved pack and selection must carry over) and saves a second
// --shot as NAME-reload.png. Console lines about the shader pack are always printed.
// --files DIR puts DIR's files into the page's store (OPFS horizon-files/) before the first
// visit; the page installs them into the game folder, so DIR/saves/W can be played with
// --args="--world W …". --size sets the page's viewport (default 1300x800).
import { chromium } from "playwright";
import { spawn } from "node:child_process";
import { readdir, readFile, writeFile } from "node:fs/promises";
import { join, relative } from "node:path";
import { parseArgs } from "node:util";
import { fileURLToPath } from "node:url";
import { watchFrameRate } from "./bench-frames.mjs";

const { values } = parseArgs({ options: {
  args: { type: "string" }, seconds: { type: "string", default: "60" }, tail: { type: "string", default: "15" },
  shot: { type: "string" }, profile: { type: "string" }, trace: { type: "string" }, release: { type: "string" },
  site: { type: "string", default: "horizon" }, shaderpack: { type: "string" },
  gpu: { type: "boolean", default: false }, verbose: { type: "boolean", default: false },
  strict: { type: "boolean", default: false }, "expect-chunks": { type: "string" },
  "profile-dir": { type: "string" }, files: { type: "string" }, size: { type: "string" }, log: { type: "string" }, reload: { type: "boolean", default: false }, port: { type: "string" },
} });
if (values.reload && !values["profile-dir"]) throw new Error("--reload needs --profile-dir");
if (values.trace && values["profile-dir"]) throw new Error("--trace and --profile-dir do not mix");
const seconds = Number(values.seconds), tail = Number(values.tail);
const web = fileURLToPath(new URL("..", import.meta.url));
if (values.site !== "horizon" && values.site !== "open") throw new Error("--site is horizon or open");
const open = values.site === "open", port = values.port ?? (open ? "8081" : "8080");
const origin = `http://127.0.0.1:${port}`;
const server = spawn(process.execPath, [open ? "scripts/serve-open.mjs" : "scripts/serve.mjs"],
  { cwd: web, stdio: "ignore", env: { ...process.env, HORIZON_WEB_PORT: port } });
await new Promise((done) => setTimeout(done, 700));
const flags = values.gpu ? [] : ["--enable-features=Vulkan", "--use-webgpu-adapter=swiftshader",
  "--enable-unsafe-swiftshader", "--use-angle=swiftshader", "--use-gl=angle"];
const launch = { executablePath: process.env.CHROME_PATH ?? "/usr/bin/google-chrome",
  headless: true, args: ["--no-sandbox", "--disable-dev-shm-usage", "--enable-unsafe-webgpu", ...flags] };
const [width, height] = (values.size ?? "1300x800").split("x").map(Number);
if (!(width > 0 && height > 0)) throw new Error("--size is WIDTHxHEIGHT");
const viewport = { width, height };
// A persistent profile keeps OPFS between the two visits of --reload.
const context = values["profile-dir"]
  ? await chromium.launchPersistentContext(values["profile-dir"], { ...launch, viewport })
  : await (await chromium.launch(launch)).newContext({ viewport });
const browser = context.browser();
try {
  const page = context.pages()[0] ?? await context.newPage();
  // Reads the canvas back through WebGPU right after a frame's submit: headless Chrome without a
  // GPU cannot composite a WebGPU canvas into a page screenshot.
  await context.addInitScript(() => {
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
    // Logs what the game asks the page to keep (OPFS), for --reload runs.
    let persist;
    Object.defineProperty(window, "horizonPersist", { configurable: true,
      set(f) { persist = f; },
      get() {
        return persist && ((path, bytes) => {
          console.log(`horizonPersist ${path} ${bytes ? `${bytes.length} bytes` : "(delete)"}`);
          return persist(path, bytes);
        });
      } });
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
  // Shader pack lines (always) and the whole console (--log), beside the frame-rate watch.
  const t0 = Date.now(), log = values.log ? [] : null;
  page.on("console", (message) => {
    const text = message.text(), s = (Date.now() - t0) / 1000;
    if (log) log.push(`[${s.toFixed(1)}] ${text}`);
    const pack = /shader ?pack|probation|horizon-shaders|horizonPersist/i.test(text) && !/Horizon web: (cpu|frame)/.test(text);
    if (pack && !values.verbose) console.log(`[${s.toFixed(1)}] ${text.slice(0, 2000)}`);
  });
  // One visit: open the page, press Play, wait for the game.
  const visit = async (withPack) => {
    const query = new URLSearchParams({ name: "Bench" });
    if (values.args) query.set("args", values.args);
    if (withPack && values.shaderpack) query.set("shaderpack", `${origin}/bench-shaderpack.zip`);
    await page.goto(`${origin}/?${query}`);
    // The menu starts automatically with ?args=. Older pages can still need a click.
    const play = page.getByRole("button", { name: /^Play/ }).first();
    await Promise.race([
      play.waitFor({ timeout: 30000 }).then(() => play.click(), () => {}),
      page.waitForFunction(() => window.horizonStatus, null, { timeout: 30000 }).catch(() => {}),
    ]);
    await page.waitForFunction(() => window.horizonStarted || window.horizonError, null, { timeout: 300000 });
  };
  if (values.files) {
    // Any same-origin page gives access to the origin's OPFS.
    await page.goto(`${origin}/release.json`);
    const root = values.files;
    const walk = async (dir) => (await Promise.all((await readdir(dir, { withFileTypes: true })).map((e) =>
      e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]))).flat();
    for (const file of await walk(root)) {
      const path = relative(root, file).split("\\").join("/");
      const bytes = [...await readFile(file)];
      await page.evaluate(async ({ path, bytes }) => {
        let dir = await (await navigator.storage.getDirectory()).getDirectoryHandle("horizon-files", { create: true });
        const parts = path.split("/"), name = parts.pop();
        for (const part of parts) dir = await dir.getDirectoryHandle(part, { create: true });
        const writable = await (await dir.getFileHandle(name, { create: true })).createWritable();
        await writable.write(new Uint8Array(bytes));
        await writable.close();
      }, { path, bytes });
    }
    console.log(`files: ${values.files} stored`);
  }
  await visit(true);
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
    await browser?.startTracing(page, { path: values.trace, categories: ["gpu", "viz", "cc", "blink", "toplevel",
      "disabled-by-default-gpu.dawn", "disabled-by-default-gpu.service", "v8", "devtools.timeline"] });
    await wait(0.2);
    await browser?.stopTracing();
  } else {
    await wait(1);
  }
  const shot = async (path) => {
    const url = await page.evaluate(() => window.horizonShot());
    await writeFile(path, Buffer.from(url.split(",")[1], "base64"));
    console.log(`shot: ${path}`);
  };
  if (values.shot) await shot(values.shot);
  const stored = () => page.evaluate(async () => {
    const out = [];
    const walk = async (dir, prefix) => {
      for await (const [name, handle] of dir.entries()) {
        if (handle.kind === "directory") await walk(handle, `${prefix}${name}/`);
        else out.push(`${prefix}${name} (${(await handle.getFile()).size} bytes)`);
      }
    };
    try { await walk(await navigator.storage.getDirectory(), ""); } catch (e) { out.push(`OPFS: ${e}`); }
    return out;
  });
  if (values.reload) {
    console.log(`OPFS before the reload: ${(await stored()).join(", ")}`);
    const selection = await page.evaluate(async () => {
      try {
        const root = await (await navigator.storage.getDirectory()).getDirectoryHandle("horizon-files");
        return await (await (await root.getFileHandle("horizon-shaders.txt")).getFile()).text();
      } catch (e) { return `none (${e})`; }
    });
    console.log(`horizon-shaders.txt: ${JSON.stringify(selection)}`);
    console.log("reloading without ?shaderpack=");
    await visit(false);
    await wait(1);
    if (values.shot) await shot(values.shot.replace(/(\.png)?$/, "-reload.png"));
  }
  if (log) await writeFile(values.log, log.join("\n") + "\n");
  reportFrameRate(await page.evaluate(() => window.horizonError ?? null));
} finally {
  await context.close();
  await browser?.close();
  server.kill();
}
