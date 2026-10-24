# wasm32 + WebGPU port: handoff

> 🤖 Agent: summary by Gabriel's AI agent (Grok Bot) of the steering research and the work on
> `gabriel/wasm-webgpu-singleplayer-b719`. How to build and run: [WEB.md](../WEB.md).

## Goal and agreed shortcuts

Single-player Horizon in desktop Chrome through `wasm32-unknown-unknown` + WebGPU, verified in a
Chromium with WebGPU (SwiftShader is fine) with a screenshot of terrain. The agreed shortcuts:

- single player only, with the in-frame single-threaded path (`--in-frame`); no Web Workers or
  `SharedArrayBuffer` yet
- no persistence (an in-memory file system)
- low render distance and no distant-terrain LOD
- texture-array layers clamped to WebGPU's 256
- Chrome desktop only
- native builds keep compiling

## What the read-only research found (and what was done about it)

| Finding | Resolution |
|---|---|
| `horizon-core`, `horizon-world` and `horizon-assets` already check clean on wasm32; `horizon-protocol` needs getrandom's `wasm_js` backend | `.cargo/config.toml` sets `getrandom_backend="wasm_js"`; the client depends on getrandom 0.2 `js` / 0.3 `wasm_js`, ring `wasm32_unknown_unknown_js`, rustls-pki-types `web` |
| `if-addrs`/`socket2` (horizon-friends), rustls/ring via ureq, `arboard` do not build | sockets and the clipboard are gated to native; ureq builds, and every request fails |
| ~71 `std::thread::spawn` sites | `Builder::spawn` returns `Err` on wasm, and the `.ok()` callers fall back; `thread::spawn`/`thread::scope` panic, so streaming workers, world loading, HUD sprite decode and mod-content tables run inline on wasm |
| `std::fs` in ~140 files | `horizon_core::fs`: `std::fs` on native, an in-memory store on wasm, with the data archive mounted at `.local/horizon-client/26.3`; `current_dir`/`current_exe` too |
| ~243 `Instant`/`SystemTime` uses (std panics on wasm) | `web-time` |
| `run_app` + `pollster::block_on(Renderer::new)` | the page awaits `gfx::open_web_gpu()` before the game starts, so `Renderer::new` never waits; winit's `spawn_app` runs the game from the page's event loop |
| wgpu built with Vulkan/Metal only | `webgpu` on wasm, `vulkan`/`metal` on native |
| `check_layers` wants up to 2048 array layers | sprite arrays split at 256 on wasm |
| compute shaders (`gfx/cull.wgsl`) | fine on WebGPU (not WebGL) |
| mods use wasmtime/cranelift | built without the `mods` feature |
| game data comes from `horizon-prepare` (Java exporters) | prepared locally from Mojang's public 26.3 client, packed by `tools/web/pack.py` into a gitignored archive |

## Problems found while making it run (in order)

1. The `std::fs` → `horizon_core::fs` rewrite: a `use std::fs` local to a function in `models.rs` read
   `render-definitions.json` through std, which answers "operation not supported on this platform".
2. Tint (Chrome's WGSL compiler) is stricter than naga. `textureSample` must be in uniform control flow
   (`hud.wgsl`), and mixing `<<` and `|` needs parentheses (`entity.wgsl`).
3. On a GPU-less VM, the canvas swap chain needs `--use-angle=swiftshader --use-gl=angle`. Without
   them the device is destroyed after the first frame, which showed up as `StagingBelt` mapping failures.
4. `StagingBelt` recalls its buffers through `map_async`/`on_submitted_work_done`. On wasm, `Uploads`
   now uses buffers mapped at creation, unmapped before the submit and kept for 4 frames.
5. Streaming scheduled work by `threads.len()`, which is 0 on wasm. Nothing was ever generated until it
   counted the frame as one worker.
6. Headless Chrome on the VM cannot composite WebGPU canvases, so screenshots came out blank. The proof
   screenshot reads the canvas texture back through WebGPU (see WEB.md).

## Result

The world loads and streams 81 chunks / ~515 meshed sections at render distance 4, the same as the
native smoke run, and terrain, trees, clouds and the HUD render
(`/workspace/horizon-artifacts/terrain.png` on the agent's box, not committed).

## Next steps

- Web Workers + `SharedArrayBuffer` for the chunk pool and the integrated server
- OPFS saves
- a WebSocket proxy for multiplayer
- Web Audio
- caching and trimming the data archive
- reworking the remaining blocking calls (`thread::sleep`, `recv_timeout`, `device.poll(Wait)`)
