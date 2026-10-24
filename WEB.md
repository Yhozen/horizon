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

`web/dist/` is gitignored. The archive holds Mojang's files: keep it on your machine and do not publish
it.

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
cd web && python3 -m http.server 8080 --bind 127.0.0.1
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
| Rendering | wgpu `webgpu` backend; device opened before the game (`gfx::open_web_gpu`); `rgba8unorm` canvas; texture arrays split at 256 layers; per-frame mapped-at-creation staging buffers instead of `StagingBelt` (`gfx/upload.rs`) |
| World | single player, `--in-frame` only; generation, lighting and meshing run as inline jobs each frame (`Streaming::pump_inline`, 40 ms budget); world loading runs inline (`world_loading::Handle`) |
| Files | `horizon_core::fs`: an in-memory store on wasm (the data archive is mounted read-write; saves live until the tab closes) |
| Time | `web-time` everywhere `Instant`/`SystemTime` were used |
| Threads | none: `thread::Builder::spawn` returns an error on wasm, which callers that already handled a failed spawn (`.ok()`) absorb; the places that panicked (`thread::spawn`/`thread::scope`) run inline on wasm: streaming workers, world loading, HUD sprite decode, mod-content tables. Other spawn sites (region writer, LOD, saving) are not audited for the web yet |
| Off | audio, controllers, mods (wasmtime), networking (multiplayer, LAN, friends/relay, HTTPS), clipboard, telemetry upload, LOD/distant terrain, persistence |

## Known issues

- One thread: a frame that generates chunks takes up to ~40 ms more. Expect low frame rates with
  SwiftShader.
- `thread::sleep`, `recv_timeout` and blocking `device.poll(Wait)` paths (screenshots, F2, LOD) are not
  reworked yet. Avoid them on the web.
- Clicking into the game asks for pointer lock; Chrome refuses it without a user gesture (a
  harmless `NotAllowedError` in automated runs).
- About 8 frames a second under SwiftShader at render distance 4 (546 frames in ~65 s).
- The page is ~140–160 MB to download before the game starts. Nothing is cached yet.

## Next steps

Web Workers with `SharedArrayBuffer` (cross-origin isolation) for the chunk pool and the integrated
server; OPFS for saves; a WebSocket↔TCP proxy for multiplayer; Web Audio for sound; caching the data
archive (Cache API) and trimming it.
