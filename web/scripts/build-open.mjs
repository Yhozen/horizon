// pnpm build:open: the open bundle, the WASM game and the static site, with no
// Minecraft files, private archive or storage credentials (docs/open-creative.md).
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { buildWasm, packageOpenRelease, prepareOpen } from "./open.mjs";

const web = fileURLToPath(new URL("..", import.meta.url)), repo = resolve(web, "..");
const version = readFileSync(resolve(repo, "Cargo.lock"), "utf8")
  .match(/name = "wasm-bindgen"\nversion = "([^"]+)"/)[1];
const installed = spawnSync("wasm-bindgen", ["--version"], { encoding: "utf8" });
if (installed.stdout?.trim() !== `wasm-bindgen ${version}`) {
  throw new Error(`Install the matching CLI first: cargo install wasm-bindgen-cli --version ${version} --locked`);
}
prepareOpen(web);
buildWasm(web);
const release = await packageOpenRelease(web);
console.log(`Horizon Open Creative: release ${release.id} in web/.build-open/static`);
