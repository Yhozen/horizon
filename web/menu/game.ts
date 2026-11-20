// Loading and starting the game once the player presses Play: the release, the WASM
// game, its resources, then `start` on canvas#horizon. Ported from the pages' old
// loaders; window.horizonStatus / horizonStarted / horizonError stay for the dev tools
// (scripts/bench.mjs, scripts/test-multiplayer.mjs, tools/web/open_proof.sh).
import { inflateArchive, loadAsset, retainRelease } from "../cache.js";
import { validateRelease } from "../release.js";
import type { Edition, WorldId } from "./edition.ts";

export type Progress =
  | { readonly _tag: "Step"; readonly step: string; readonly index: number }
  | { readonly _tag: "Started" }
  | { readonly _tag: "Failed"; readonly error: string; readonly reload: boolean };

/** The loading steps, in order; the progress bar moves one step at a time. */
export const STEPS = 4;

interface Telemetry {
  capture(event: string, data?: object): void;
  captureException(error: unknown, data?: object): void;
  logInfo(message: string, data?: object): void;
  setRelease(): void;
}
type Window = typeof globalThis & {
  horizonStatus?: string; horizonStarted?: boolean; horizonError?: string; horizonRelease?: string;
  horizonTelemetry?: Telemetry;
};
const page = globalThis as Window;

function say(text: string) {
  page.horizonStatus = text;
  console.log("[page] " + text);
}

/** The game's address for a world: this page's own /connect, which the Worker routes by name. */
export function serverAddress(world: WorldId, location: { protocol: string; host: string }) {
  return `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/connect?world=${world}`;
}

/** The game's arguments. `custom` (the page's ?args=) replaces them, for development. */
export function gameArgs(edition: Edition, world: WorldId, name: string, address: string, custom?: string) {
  if (custom) return custom.split(" ").filter(Boolean);
  const run = ["--render-distance", "4", "--in-frame", "--world-frames", "18446744073709551615",
    "--timeout-seconds", "1000000000", "--name", name];
  if (world !== "solo") return ["--server", address, ...run];
  return ["--world", edition.solo.world, ...(edition.solo.creative ? ["--creative"] : []),
    ...(edition.id === "horizon" ? ["--test-terrain"] : []), ...run];
}

/** Where the page's ?shaderpack= lands in the game's files (relative: the game folder). */
export const SHADER_PACK = "shaderpacks/pack.zip";

/** The page's ?shaderpack=, an OptiFine-format zip to fetch and load (`--shader-pack`). */
export function shaderPackUrl(search: string) {
  return new URLSearchParams(search).get("shaderpack") || undefined;
}

const message = (error: unknown) => String((error as Error)?.message ?? error);

/** Start the watchdog at handoff and stop counting while the tab is hidden. */
function renderSignals(signal: AbortSignal) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const watching = new AbortController();
  const stopWatching = () => {
    watching.abort();
    clearTimeout(timer);
  };
  signal.addEventListener("abort", stopWatching, { once: true });
  const ready = new Promise<void>((resolve) => {
    window.addEventListener("horizon-render-ready", () => {
      stopWatching();
      resolve();
    }, { once: true, signal });
  });
  const failed = new Promise<Error>((resolve) => {
    const watch = () => {
      clearTimeout(timer);
      if (!document.hidden) timer = setTimeout(() => resolve(new Error("Timed out waiting for the first game frame")), 45_000);
    };
    document.addEventListener("visibilitychange", watch, { signal: watching.signal });
    window.addEventListener("horizon-render-error", (event) => {
      resolve(new Error((event as CustomEvent<string>).detail));
    }, { once: true, signal });
    watch();
  });
  return { ready, failed };
}

/** Loads the game, waits for the first frame, and delivers fatal errors for the session. */
export async function* boot(edition: Edition, world: WorldId, name: string, custom?: string): AsyncGenerator<Progress> {
  const telemetry = page.horizonTelemetry;
  const step = (index: number, text: string): Progress => { say(text); return { _tag: "Step", step: text, index }; };
  let renderer: AbortController | undefined;
  delete page.horizonError;
  delete page.horizonStarted;
  try {
    yield step(0, "Checking your graphics");
    const gpu = (navigator as Navigator & { gpu?: { requestAdapter(options: { powerPreference: string }): Promise<unknown> } }).gpu;
    if (!gpu) throw new Error("This browser does not provide WebGPU.");
    // A GPU process that is still starting answers the first requests with nothing.
    for (let attempt = 1; !(await gpu.requestAdapter({ powerPreference: "high-performance" })); attempt++) {
      if (attempt === 20) throw new Error("WebGPU could not find a usable graphics adapter.");
      await new Promise((resolve) => setTimeout(resolve, 500));
    }

    yield step(1, "Downloading the game");
    const response = await fetch("./release.json", { cache: "no-store", redirect: "error" });
    if (!response.ok) throw new Error(`Could not load the game (${response.status})`);
    const release = validateRelease(await response.json());
    if (edition.release && release.edition !== edition.release) throw new Error("This page runs Horizon Open Creative releases only");
    page.horizonRelease = release.id;
    telemetry?.setRelease();
    telemetry?.capture("game_load_started", { phase: "wasm", world });
    const game = await import(/* @vite-ignore */ release.assets.javascript.url);
    const wasm = await loadAsset(release.assets.wasm);
    await game.default({ module_or_path: new Response(wasm, { headers: { "Content-Type": "application/wasm" } }) });

    yield step(2, edition.id === "open" ? "Loading the artwork" : "Loading resources");
    const report = (state: string) =>
      telemetry?.capture("resources_cache_resolved", { state, elapsed_ms: Math.round(performance.now()) });
    const data: ArrayBuffer = await inflateArchive(
      await loadAsset(release.assets.resources, { report: report as () => void }));
    // The horizon world plays on the open bundle, which open.rccpr.tech's players share,
    // wearing this site's art where it has the same files (docs/open-creative.md).
    const overlaid = edition.id === "horizon" && world === "shared";
    const shared: ArrayBuffer | undefined = overlaid ? await inflateArchive(await loadAsset(release.assets.sharedResources ?? (() => {
      throw new Error("This release can't join the Horizon world");
    })())) : undefined;

    const packUrl = shaderPackUrl(location.search);
    if (packUrl) {
      const pack = await fetch(packUrl);
      if (!pack.ok) throw new Error(`Could not load the shader pack (${pack.status})`);
      game.install_file(SHADER_PACK, new Uint8Array(await pack.arrayBuffer()));
    }

    const address = serverAddress(world, location);
    const args = [...gameArgs(edition, world, name, address, custom), ...(packUrl ? ["--shader-pack", SHADER_PACK] : [])];
    yield step(3, world === "solo" ? "Creating your world" : "Joining the world");
    renderer = new AbortController();
    const { ready, failed } = renderSignals(renderer.signal);
    const started = shared ? game.start_overlaid(new Uint8Array(shared), new Uint8Array(data), args)
      : game.start(new Uint8Array(data), args);
    const error = await Promise.race([Promise.all([started, ready]).then(() => undefined), failed]);
    if (error) throw error;
    page.horizonStarted = true;
    say(world === "solo" ? "Playing" : "Joining");
    void retainRelease(release).catch((error: unknown) => console.warn("Keeping the release cached failed", error));
    yield { _tag: "Started" };
    throw await failed;
  } catch (error) {
    telemetry?.captureException(error, { phase: page.horizonStatus });
    if (document.pointerLockElement) document.exitPointerLock();
    page.horizonStarted = false;
    page.horizonError = message(error);
    say("Failed: " + message(error));
    console.error(error);
    yield { _tag: "Failed", error: message(error), reload: renderer !== undefined };
  } finally {
    renderer?.abort();
  }
}
