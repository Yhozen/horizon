import { spawnSync } from "node:child_process";
import { readFileSync, mkdirSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { packageRelease } from "./artifacts.mjs";

const web = fileURLToPath(new URL("..", import.meta.url)), repo = resolve(web, "..");
const { values } = parseArgs({ options: { archive: { type: "string" } } });
const run = (command, args, cwd = repo) => {
  const result = spawnSync(command, args, { cwd, stdio: "inherit" });
  if (result.error || result.status !== 0) throw new Error(`${command} failed: ${result.error?.message ?? result.status}`);
};
const version = readFileSync(resolve(repo, "Cargo.lock"), "utf8")
  .match(/name = "wasm-bindgen"\nversion = "([^"]+)"/)[1];
const installed = spawnSync("wasm-bindgen", ["--version"], { encoding: "utf8" });
if (installed.stdout?.trim() !== `wasm-bindgen ${version}`) {
  throw new Error(`Install the matching CLI first: cargo install wasm-bindgen-cli --version ${version} --locked`);
}
if (!values.archive) run(process.execPath, ["scripts/fetch-resources.ts"], web);
run("cargo", ["build", "--locked", "--target", "wasm32-unknown-unknown", "--profile", "fast",
  "-p", "horizon-client", "--bin", "horizon-client", "--no-default-features"]);
mkdirSync(resolve(web, "dist"), { recursive: true });
run("wasm-bindgen", ["--target", "web", "--no-typescript", "--out-dir", resolve(web, "dist"),
  "--out-name", "horizon-client", resolve(repo, "target/wasm32-unknown-unknown/fast/horizon-client.wasm")]);
const release = await packageRelease(web, values.archive ? resolve(values.archive) : undefined);
console.log(`Ready to review: release ${release.id}`);
