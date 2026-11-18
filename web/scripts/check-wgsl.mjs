// Compiles WGSL files with Chrome's own compiler (Tint, through WebGPU's createShaderModule and
// getCompilationInfo) and prints its messages as FILE:LINE:COLUMN; exits 1 if any is an error.
//   node scripts/check-wgsl.mjs DIR_OR_FILE... [--gpu]
// Without --gpu, WebGPU runs on SwiftShader (a machine without a GPU), as in scripts/bench.mjs.
import { chromium } from "playwright";
import { readdir, readFile, stat } from "node:fs/promises";
import { join } from "node:path";
import { parseArgs } from "node:util";

const { values, positionals } = parseArgs({ allowPositionals: true,
  options: { gpu: { type: "boolean", default: false } } });
if (!positionals.length) throw new Error("usage: node scripts/check-wgsl.mjs DIR_OR_FILE... [--gpu]");
const files = [];
for (const path of positionals) {
  if ((await stat(path)).isDirectory()) {
    for (const name of (await readdir(path)).sort()) if (name.endsWith(".wgsl")) files.push(join(path, name));
  } else files.push(path);
}
const sources = await Promise.all(files.map(async (file) => ({ file, code: await readFile(file, "utf8") })));

const flags = values.gpu ? [] : ["--enable-features=Vulkan", "--use-webgpu-adapter=swiftshader",
  "--enable-unsafe-swiftshader", "--use-angle=swiftshader", "--use-gl=angle"];
const browser = await chromium.launch({ executablePath: process.env.CHROME_PATH ?? "/usr/bin/google-chrome",
  headless: true, args: ["--no-sandbox", "--disable-dev-shm-usage", "--enable-unsafe-webgpu", ...flags] });
let errors = 0;
try {
  const page = await browser.newPage();
  // An empty page on localhost: WebGPU needs a secure context.
  await page.route("http://localhost/", (route) => route.fulfill({ contentType: "text/html", body: "<!doctype html>" }));
  await page.goto("http://localhost/");
  const results = await page.evaluate(async (sources) => {
    let adapter = null;
    for (let i = 0; i < 20 && !(adapter = await navigator.gpu?.requestAdapter()); i++) await new Promise((r) => setTimeout(r, 500));
    if (!adapter) throw new Error("no WebGPU adapter");
    const device = await adapter.requestDevice({ requiredFeatures: [...adapter.features] });
    const out = [];
    for (const { file, code } of sources) {
      const info = await device.createShaderModule({ code, label: file }).getCompilationInfo();
      out.push({ file, messages: info.messages.map(({ type, lineNum, linePos, message }) => ({ type, lineNum, linePos, message })) });
    }
    return out;
  }, sources);
  console.log(`Chrome ${browser.version()}`);
  for (const { file, messages } of results) {
    for (const m of messages) console.log(`${file}:${m.lineNum}:${m.linePos}: ${m.type}: ${m.message}`);
    const failed = messages.some((m) => m.type === "error");
    errors += failed;
    console.log(`${failed ? "FAIL" : "ok  "} ${file}`);
  }
  console.log(`${results.length - errors} of ${results.length} compiled`);
} finally {
  await browser.close();
}
process.exit(errors ? 1 : 0);
