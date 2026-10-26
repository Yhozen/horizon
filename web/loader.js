import { loadAsset, retainRelease } from "./cache.js";
import { validateRelease } from "./release.js";

Error.stackTraceLimit = 200;
const status = document.getElementById("status");
const say = (text) => {
  status.textContent = text;
  window.horizonStatus = text;
  console.log("[page] " + text);
};
window.horizonStatus = "loading";
const params = new URLSearchParams(location.search);
const playerName = params.get("name") || `Web${crypto.getRandomValues(new Uint32Array(1))[0].toString(16)}`;
document.getElementById("player-name").value = playerName;
document.getElementById("multiplayer-note").hidden = params.get("mode") !== "multiplayer";
const args = params.has("args")
  ? params.get("args").split(" ").filter(Boolean)
  : ["--world", "Web World", "--creative", "--test-terrain", "--render-distance", "4",
    "--in-frame", "--world-frames", "18446744073709551615", "--timeout-seconds", "1000000000",
    "--name", playerName];
if (!params.has("args") && params.get("mode") === "multiplayer") {
  args.splice(args.indexOf("--world"), 2, "--server", "server");
}

try {
  if (!navigator.gpu) throw new Error("This browser has no WebGPU support");
  say("Loading the game…");
  const response = await fetch("./release.json", { cache: "no-store", redirect: "error" });
  if (!response.ok) throw new Error(`Could not load game release (${response.status})`);
  const release = validateRelease(await response.json());
  const { default: init, start } = await import(release.assets.javascript.url);
  const wasm = await loadAsset(release.assets.wasm);
  await init({ module_or_path: new Response(wasm, { headers: { "Content-Type": "application/wasm" } }) });
  say("Loading resources…");
  const data = await loadAsset(release.assets.resources, { report: (state) => {
    if (state === "cached") say("Using saved resources…");
    if (state === "downloading") say("Downloading resources…");
  } });
  say(`Starting (${(data.byteLength / 1e6).toFixed(0)} MB of resources)…`);
  await start(new Uint8Array(data), args);
  window.horizonStarted = true;
  window.horizonRelease = release.id;
  say(args.includes("--server") ? "Multiplayer is starting…" : "Single player is running.");
  await retainRelease(release);
} catch (error) {
  say("Failed: " + (error?.message ?? error));
  window.horizonError = String(error?.message ?? error);
  console.error(error);
}
