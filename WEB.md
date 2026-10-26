# Horizon in the browser (wasm32 + WebGPU)

> 🤖 Agent: written by Gabriel's AI agent (Grok Bot). This is an experimental single-player build for
> desktop Chrome. The native game is unchanged; everything browser-specific is behind
> `cfg(target_arch = "wasm32")`.

What it does: the page downloads the prepared resources, mounts them in an in-memory file system,
opens the WebGPU device, creates a fresh single-player world, ticks it on the page's own frame loop
(`--in-frame`), streams and meshes chunks on the main thread, and draws terrain on `<canvas id="horizon">`.
Choppy is expected: one thread does everything.

## 1. Toolchain

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129   # must match wasm-bindgen in Cargo.lock
# Linux, to build the ring crate for wasm32: clang
sudo apt-get install clang
```

`.cargo/config.toml` sets `--cfg getrandom_backend="wasm_js"` for the wasm32 target. Nothing else
needs configuring.

## 2. Game data (never commit it)

The browser build reads the same prepared resources as the desktop game
(`.local/horizon-client/26.3`, made by `horizon-prepare` from your own Minecraft 26.3 install, which needs a
JDK). If you already play Horizon on this machine, the launcher has prepared them; otherwise:

```sh
# Your Minecraft install (the launcher's .minecraft, holding versions/26.3/26.3.jar and assets/)
JAVA_HOME=/path/to/jdk-25 cargo run --release -p horizon-prepare -- --minecraft-dir ~/.minecraft
```

Then pack them into one archive the page fetches:

```sh
python3 tools/web/pack.py                       # -> web/dist/horizon-data.bin (~156 MB, 23k files)
```

`tools/web/pack.py --only LIST` packs only the files LIST names (one path per line), for example the
files a desktop run opened (`strace -f -e trace=openat -o open.trace target/fast/horizon-client ...`,
then keep the paths under the resources folder). That trimmed the archive to ~139 MB here; it is
optional.

`web/dist/` and `web/.build/` are gitignored. The archive holds Mojang's files: never commit it or
publish it publicly. The private preview below stores it in an R2 bucket behind an email allowlist.

## 3. Build

```sh
cargo build --target wasm32-unknown-unknown --profile fast \
  -p horizon-client --bin horizon-client --no-default-features
wasm-bindgen --target web --no-typescript --out-dir web/dist --out-name horizon-client \
  target/wasm32-unknown-unknown/fast/horizon-client.wasm
```

`--no-default-features` leaves out audio, controllers and the mod runtime (wasmtime cannot run in a
page). A clean build takes about 2 minutes, an incremental one under a minute.

## 4. Serve and open

```sh
cd web
pnpm install --frozen-lockfile
pnpm package --archive dist/horizon-data.bin
pnpm serve
```

Open <http://127.0.0.1:8080/> in desktop Chrome (113 or later, WebGPU on). The page passes these game
arguments by default:

```
--world "Web World" --creative --test-terrain --render-distance 4 --in-frame
--world-frames 18446744073709551615 --timeout-seconds 1000000000 --name WebPlayer
```

To pass others, add `?args=` with the arguments separated by spaces (URL-encoded), for example
`?args=--world%20W%20--creative%20--test-terrain%20--render-distance%202%20--in-frame`.
`--in-frame` is required. The browser always uses the synchronous startup path (no resource-loader
thread), and both `Game::resumed` and `Loading::resumed` bind winit to `canvas#horizon`, so a short
`?args=` list without `--world-frames` still draws on the page canvas.
`window.horizonStatus` shows what the page is doing. Every two seconds the console prints a progress line
(`Horizon web: frame … chunks … meshed … player …`).

## 5. Private Cloudflare preview

The deployment project is `web/`; the marketing website is a separate project. Node 24+, pnpm, the
Rust wasm target, clang on Linux, and the matching wasm-bindgen CLI are required. Alchemy dependencies
are pinned in the pnpm lockfile.

The `HorizonWeb` stack in stage `preview` references the existing bucket owned by
`HorizonCloudflare/live_vercel-sandbox/Bucket`. It never recreates or deletes that bucket. The source
object is `horizon-data.bin`; bucket coordinates and the preview's email allowlist are in
`web/scripts/config.mjs`.

Enable Cloudflare Zero Trust/Access for the account first. Configure credentials in Alchemy's stored
`default` profile (Workers, R2 and Access permissions), using its interactive prompts:

```sh
cd web
pnpm alchemy profile edit --profile default --add Cloudflare
pnpm alchemy profile show --profile default
```

The stack provisions a One-time PIN identity provider for emailed sign-in codes. If your account
already has one managed elsewhere, reference that provider instead of creating or adopting it.

Build and review:

```sh
pnpm build
pnpm test
pnpm check
pnpm alchemy plan --stage preview --profile default
```

The browser tests use `/usr/bin/google-chrome`; set `CHROME_PATH` to your Chrome executable when it
is installed elsewhere.

`pnpm build` reads the archive from the private bucket using the stored profile, pins its ETag while
downloading, builds WASM, and generates `.build/site/release.json` and immutable local objects. To use
an archive already on your machine, pass `pnpm build --archive /absolute/path/horizon-data.bin`.
`pnpm package` packages existing wasm-bindgen output without rebuilding Rust.

Review the plan, then run:

```sh
pnpm alchemy deploy --stage preview --profile default
```

Inherited Cloudflare environment credentials take precedence in Alchemy's CLI. When present, remove
those overrides from the command's environment so the reviewed `default` profile is used. The archive
fetch command explicitly uses the stored profile.

The publish Action verifies or uploads each immutable object before the Worker release can deploy.
Uploads stream through the R2 API (300 MB per-object limit); generated files never enter Git. Old
objects remain available for rollback. A failed upload does not publish the new Worker release.

Packaging embeds the small HTML shell, loader modules and manifest in the Worker bundle. This deliberately
avoids Cloudflare Static Assets: its internal router does not forward `ctx.access` to the user Worker
([Cloudflare's documented limitation](https://developers.cloudflare.com/workers/configuration/cloudflare-access/#ctxaccess-limitations)).
The Worker serves HTML and the manifest with `no-store`. Every public request first passes Cloudflare
Access and an uncached gateway that checks the platform's authenticated identity. The gateway then
calls an internal asset entrypoint with Workers Cache enabled across versions. It streams R2 objects
and converts their browser cache policy to `private, max-age=31536000, immutable`. The R2 bucket and
its `r2.dev` URL remain private, and Worker version preview URLs are disabled. The
`X-Horizon-Asset-Cache` response header reports the internal entrypoint's cache status.

The browser verifies SHA-256 and size before storing the WASM and resource archive in Cache Storage.
Hashes depend on each file's bytes, independently: changing game code does not change the resource
URL. Successful startup retains the current and previous release's browser entries. Quota failures
fall back to normal downloads; clearing browser data or storage eviction can require downloading
again. This caches assets, not world saves, and does not provide an offline app shell.

Before calling a release live, check an authenticated cold start, a reload and browser restart, then
an anonymous request to the same warm asset URL. Check the internal edge-cache header independently
of the browser cache. Keep later update/rollback checks local unless another deployment is approved.

### Chrome flags

A machine with a GPU needs no flags in current Chrome. To run on a VM with no GPU (SwiftShader), as
the screenshot here was made:

```
google-chrome --enable-unsafe-webgpu --enable-features=Vulkan \
  --use-webgpu-adapter=swiftshader --enable-unsafe-swiftshader \
  --use-angle=swiftshader --use-gl=angle http://127.0.0.1:8080/
```

Without `--use-angle=swiftshader --use-gl=angle`, Chrome's GPU process cannot create the canvas's swap
chain image on such a VM. The device is then destroyed after the first frame
("A valid external Instance reference no longer exists", then `createBuffer … too large … when
mappedAtCreation`).

Headless Chrome on a GPU-less VM still cannot *composite* a WebGPU canvas (`Unable to initialize
SkSurface`), so a page screenshot shows the canvas blank even while the game renders. That limits
screenshots only. To capture the frame, read the canvas's current texture back through WebGPU right after
the game's submit: configure the canvas with `COPY_SRC`, wrap `GPUQueue.prototype.submit`, then
`copyTextureToBuffer` → `mapAsync` → a 2D canvas → PNG. The capture script used for the screenshot does
exactly that. It is a test-harness wrapper injected with Puppeteer's `evaluateOnNewDocument`, not part of
the game.

## What runs, and what is stubbed on the web

| Area | Browser build |
|---|---|
| Rendering | wgpu `webgpu` backend; device opened before the game (`gfx::open_web_gpu`); `rgba8unorm` canvas; one block-sprite array, packing a grid of sprites per layer where the adapter has too few layers (`gfx::sprite_grid`); sections drawn flat, one draw per arena page (`gfx/flat.rs`); animated sprites blended on the CPU and written through the queue (`gfx/animate.rs`); per-frame mapped-at-creation staging buffers instead of `StagingBelt` (`gfx/upload.rs`) |
| World | single player, `--in-frame` only; generation, lighting and meshing run as inline jobs each frame (`Streaming::pump_inline`, 40 ms budget); world loading runs inline (`world_loading::Handle`) |
| Files | `horizon_core::fs`: an in-memory store on wasm (the data archive is mounted read-write; saves live until the tab closes). Files keep modification times (the Singleplayer list skips worlds whose `level.dat` has none); region and chunk data are compressed at zlib's fastest level, since they never leave memory |
| Time | `web-time` everywhere `Instant`/`SystemTime` were used; browser animation frames replace the blocking native frame limiter |
| Threads | none: `thread::Builder::spawn` returns an error on wasm, which callers that already handled a failed spawn (`.ok()`) absorb; the places that panicked (`thread::spawn`/`thread::scope`) run inline on wasm: streaming workers, world loading, HUD sprite decode, mod-content tables, region-file writes (`RegionWriter`). The integrated server always ticks in the frame (`--server-thread` is ignored), and world deletion does not sleep between retries |
| Off | audio, controllers, mods (wasmtime), networking (multiplayer, LAN, friends/relay, HTTPS), clipboard, telemetry upload, LOD/distant terrain, persistence |

## Frame rate

`pnpm bench` (in `web/`, after `pnpm package`) serves `.build/`, opens the page in headless Chrome and
reads the game's two-second status lines (frames and chunks, then the mean CPU and GPU frame times
with `--verbose`). It prints the frame rate over the last `--tail N` lines (15) of a `--seconds N` run
(60). `--args="..."` passes game arguments, `--shot FILE` saves the last frame (read back through
WebGPU, which headless Chrome without a GPU cannot composite), `--profile FILE` records a main-thread
CPU profile, `--trace FILE` a Chrome trace, and `--release FILE` serves another `release.json` (its
objects copied into `.build/objects`) for A/B runs. Without `--gpu` WebGPU runs on SwiftShader. For a
scene that repeats exactly, fix the spawn, view and time:

```
--world W --creative --test-terrain --render-distance 4 --in-frame --name WebPlayer --weather clear
--spawn-radius 0 --look 30,15 --time 6000      # --time 0 for dawn, the time a new world starts at
--world-frames 18446744073709551615 --timeout-seconds 1000000000
```

What limited the frame rate, measured on a 4-vCPU VM with SwiftShader (frames a second over 30 s,
the original build and this one run alternately):

| Scene | Before | After |
|---|---|---|
| noon (`--time 6000`) | 1.73 | 5.67 (3.3×) |
| dawn (`--time 0`) | 1.44 | 5.35 (3.7×) |

- **The page's main thread spun.** `ControlFlow::Poll` makes winit's web loop queue a
  `scheduler.postTask` (and an `AbortController`) back to back: ~70% of the main thread, with the
  animation frames it waits for starved. The browser now waits (`game.rs`'s `FRAME_FLOW`); the redraw
  request already schedules the next frame.
- **Multi-draw is emulated.** wgpu replays it on WebGPU as one validated `drawIndexedIndirect` per
  section group, each a command to the GPU process; flat draws (one per arena page) are on for the
  browser as for Metal, and arena pages fit a storage binding.
- **Sprites in several arrays.** WebGPU guarantees 256 texture layers; split across 6 arrays, every
  terrain fragment chose its array in a `switch` and sampled with explicit gradients, doubling the
  terrain's shading. The split now follows the adapter's limit (2048 on desktop GPUs: one array), and
  where it is lower a grid of sprites shares each layer (`tiled_sprite`, the tile worked out per vertex).
- **Branches cost both sides on a software rasterizer.** SwiftShader runs every side of a branch
  for every pixel, masked: the sunrise fan's 16 ray–triangle tests (now geometry, as vanilla draws it),
  the entity shader's End portal and glint code (now left out, through a pipeline constant, of the runs
  of quads without them), and the clouds going through the 11-output entity shader (now a shader of
  their own).
- **Overdraw.** The sky draws after solid terrain at the far plane, so only uncovered pixels shade it.
- **Per-quad cost.** Clouds out of view are left out of the cloud index list.
- **Animated sprites** went through a compute pass per mip level; the browser blends them on the CPU
  and writes them through `queue.write_texture` (no staging buffer).

On a GPU-less machine the canvas is read back for compositing, so a frame takes the CPU time plus the
GPU time. What remains is mostly shading: terrain about half of the frame, then the sky, the
hand, clouds and water. Horizon's resolution scale (0.25–2) trades sharpness for speed.

## Freezes while playing (fixed)

The game stopped (canvas frozen, `RuntimeError: unreachable` in the console) a few seconds to minutes
into play. Each one was a std call that panics on wasm32, reached from a path that only runs in play:

- **Unloading chunks** saved their mobs, and a mob without a UUID got one salted with
  `std::process::id()` (panics): ~20 s of walking froze the game. The browser salts with random bits
  and a counter; temp-file names use `horizon_core::fs::process_id()` (1 on wasm).
- **Region writes** started a writer thread (`thread::spawn` fails); they now write inline.
- **The log directory** fell back to `std::env::temp_dir()` (panics); it falls back to the VFS cwd.
- **The integrated server thread** and **world deletion's retry sleep** are off on wasm.
- **Saved worlds were missing from Singleplayer** after Save & Quit: the in-memory files had no
  modification times, and the list drops worlds whose `level.dat` returns an error. They now have
  them.

Checked by a 25-minute soak (walking, clicking and commands) and a 10-minute vanilla-terrain walk
with `/tp` jumps, then Save & Quit, re-entering the world from the list, walking and clicking.

## Known issues

- One thread: streaming runs inline with a 40 ms budget per frame, but each job runs whole. In a
  vanilla (not `--test-terrain`) world one chunk generation takes up to ~100-260 ms. The stronghold
  ring search (128 positions × 3,249 biome samples, ~3.8 s) is skipped near the origin, where no
  ring can reach, and is run one search per idle frame once the chunks in range are generated. A
  walk out past ~65 chunks (~1,000 blocks) before that finishes still pays the rest in one frame. Saves go in
  batches of 16 proto chunks. Jobs over 100 ms are logged
  (`Horizon web: generate x,z took N ms in the frame`).
- Worlds are not persisted: the file system is in memory, so a reload loses them.
- std on wasm32 panics in `std::process::id`, `thread::sleep`, `Instant::now`/`SystemTime::now`,
  `env::temp_dir` and `Condvar::wait`. A panic inside the animation-frame callback ends winit's loop
  and leaves the last frame on the canvas, which looks like a freeze; see above.
- `thread::sleep`, `recv_timeout` and blocking `device.poll(Wait)` paths (screenshots, F2, LOD) are not
  reworked yet. Avoid them on the web.
- Clicking into the game asks for pointer lock; Chrome refuses it without a user gesture (a
  harmless `NotAllowedError` in automated runs).
- The first visit downloads the resource archive and WASM before the game starts. Later visits reuse
  verified browser-cache entries while they remain available.

## Next steps

Web Workers with `SharedArrayBuffer` (cross-origin isolation) for the chunk pool and the integrated
server; OPFS for saves; a WebSocket↔TCP proxy for multiplayer; Web Audio for sound; trimming or splitting
the data archive.
