# Shader packs (experimental)

Horizon can draw the world with an OptiFine- or Iris-format shader pack: the `shaders/` folder of `.vsh`,
`.fsh` and `.properties` files those mods load, in a zip or a folder. It is off by default and looks exactly
like vanilla until a pack is chosen.

## Choosing a pack

**Options → Horizon Settings… → Shader Packs…** lists the packs in the game folder's `shaderpacks` folder
(zips, and folders holding a `shaders` folder). `Shaders: ON/OFF` turns packs on; clicking a pack chooses
it; `Done` applies. The pack compiles while the world keeps running ("Compiling 12/41"), then replaces
Horizon's own shaders; a chat line says whether it loaded fully, partly, or not at all.

- **Adding packs**: `Open Shader Pack Folder…`, or drop zips on the window while the Shader Packs page
  shows (as Iris takes drops only on its shader screen). In the browser, `Add Shader Pack…` opens a file
  picker, and zips can be dropped on the page while that page shows. Only a pack's `shaders/` folder is
  read; a pack with more than 10,000 files there or more than 256 MiB unpacked is refused (its report says
  so), and the browser takes zips up to 128 MB. A pack added again under the same name is read and
  compiled again.
- **Settings…**: the pack's own option screens (`screen=`, sub-screens, sliders, profiles, the labels of its
  `lang/en_us.lang`). Values are saved in `shaderpacks/<pack file>.txt`, the `NAME=VALUE` file Iris and
  OptiFine keep beside a pack, so a settings file copied from Iris gives the same configuration.
- **Report…**: what the pack uses that Horizon does not have, every program that was skipped and why
  (pack file and line), the GPU compiler's messages, uniforms that read zero. `Copy Report` puts it on the
  clipboard; the same text is in the log (`logs/latest.log`, or the browser console).
- The choice is saved in `horizon-shaders.txt` (`enabled:`, `pack:`) beside `options.txt`. While a newly
  installed pack has not yet drawn the world for 10 seconds the file also says `trial:true`; a game that
  starts and finds it there (the last run stopped in that time, perhaps crashed by the pack) leaves the pack
  off, says so in chat and in its report, and `Done` turns it on again.
- A GPU error the pack causes at any time (natively; in the browser in its first frames) turns it off with
  the error in its report, rather than stopping the game.
- The pack stays compiled on the title and loading screens, and the last two compiles (say the Overworld's
  and the Nether's) are kept, so a portal round trip or a new world does not compile again.

Statuses: **Works** (every program the pack enables compiled), **Partial** (it runs; the report lists what is
missing), **Not supported** (it requires something Horizon lacks, such as compute shaders; the report says
what), **Failed** (an essential program could not be compiled, or the GPU rejected it: Horizon's own shaders
stay).

### In the browser

The page keeps the `shaderpacks` folder and `horizon-shaders.txt` in the browser's private storage (the
Origin Private File System), so packs and the choice survive a reload. At boot only the choice, the option
files and the chosen pack (when shaders are on) are put in the game's memory; another pack is read from
storage when the page first shows it or it is chosen. A write the storage refuses (it is full, say) is
said in chat. `?shaderpack=<url of a zip>` adds that
pack to the folder and chooses it. A pack whose shaders the browser's own compiler rejects is turned off
during its first frames, and its report gives the browser's message; other programs of the pack fall back
when they can.

### From the command line

```sh
horizon-client --resources … --world W --shader-pack path/to/pack.zip     # a zip or a folder, not saved
horizon-client … --shader-pack pack.zip --shader-pack-option TAA_ENABLED=false --shader-pack-option shadowMapResolution=1024
```

`--shader-pack-option NAME=VALUE` (repeatable) changes one of the pack's options on top of its saved ones:
`true`/`false` for a `#define` toggle, a value for `#define NAME value` and for the `const` options.

The code is a clean-room rewrite: it is written from the pack format, its public documentation and the packs
themselves, not from Iris, OptiFine, or the shader-pack code Horizon removed in `4e7c25bb`. What a pack
observes of its host is written down in [shader-pack-format-notes.md](shader-pack-format-notes.md). How each measured pack fares: [shader-pack-compat.md](shader-pack-compat.md).

## How it works

`crates/horizon-glsl-pack` turns a pack into WGSL and a plan of its passes, with no wgpu and no file access.
A `Session` (`session.rs`) does it in small steps, so the browser can compile across frames; natively it
runs on a thread. Its result is the compiled pack and a `PackReport` (`report.rs`).

1. **Options and screens** (`options.rs`, `screens.rs`): options are found in the sources (`#define` toggles
   tested anywhere, value lists, `const` options with a list) and changed by rewriting the source text, as
   the pack's own options screen would; `screens.rs` reads `screen=`, `sliders=`, `profile.*` and the
   pack's `lang/` labels for the Settings page, and the `<pack>.txt` option files.
2. **Features** (`features.rs`): compute, geometry and tessellation stages, custom images, storage buffers,
   `iris.features.required/optional`. A pack that requires one Horizon lacks is refused with that reason;
   an optional one is simply not offered (its `IRIS_FEATURE_*` macro stays undefined), so the pack takes
   its own fallback.
3. **Plan** (`program.rs`, `directives.rs`, `shaders_properties.rs`, `plan.rs`): the program slots after
   the documented fallback chains (Iris's split slots such as `gbuffers_terrain_cutout` or `shadow_water`
   run when the pack has them), `deferredN`/`compositeN` up to 99, `prepare` passes, each buffer's format
   (every format name, integer and signed ones included), size, clear and mipmaps, the shadow map's
   constants, and `shaders.properties`: `blend.*`, `alphaTest.*`, `scale.*`, `flip.*`, `size.buffer.*`,
   `program.*.enabled` (as expressions), custom textures, shadow and culling switches.
4. **Ids** (`properties.rs`): `block.properties` by block state (`oak_log:axis=y`), `entity.properties`,
   `item.properties`, each preprocessed with the options' macros; without a `block.properties`, the
   pre-1.13 numeric ids old packs compare (`legacy_ids.rs`).
5. **Custom uniforms** (`custom_uniforms.rs`): `uniform.*` and `variable.*` expressions, evaluated on the CPU
   every frame from the standard uniforms, uploaded in a second block.
6. **Preprocess** (`preprocess.rs`): `#include`, `#define` (function-like, across lines, `##`), `#if`, and the
   macros Iris defines. Horizon says it is Iris 1.11.7 (`IRIS_VERSION` 11107, `MC_VERSION` 260300, values
   measured on a real Iris) at the OpenGL 4.1 level (`MC_GL_VERSION`/`MC_GLSL_VERSION` 410): the level of
   macOS, with no compute shaders, images or storage buffers, which mainstream packs keep a path for.
7. **Rewrite** (`rewrite/`): GLSL 120–460 (compatibility or core profile) to plain GLSL 450, in token passes:
   - functions `main` does not reach are dropped (packs keep invalid code in unused functions);
   - functions that take a sampler are cloned per sampler passed to them (`mono.rs`), as WGSL
     cannot pass textures around;
   - locals that shadow a sampler, a uniform or a built-in function are renamed (`scope.rs`);
   - `const` on locals and parameters is dropped, returns are converted to the function's type, global
     initialisers that need run-time values move into `main`, uniforms with initialisers become globals,
     built-ins naga lacks (`modf`, `isnan`…) get small polyfills (`fixes.rs`, `mod.rs`);
   - core-profile inputs (`vaPosition`, `modelViewMatrix`…) and `gl_TexCoord[]` become the compatibility
     ones (`compat.rs`);
   - interface declarations are recorded and taken out; legacy built-ins become `hz_gl_*` globals; every
     sampling call (2D, 3D, shadow and integer samplers; `Lod`, `Grad`, `Proj`, `Offset`, `Gather`,
     `texelFetch`, `textureSize`, `textureQueryLod`…) becomes a call to a host function per sampler.
8. **Host** (`host/`, `uniforms.rs`): a generated `main` per stage around the pack's, per geometry:
   - *terrain*: Horizon's `TerrainVertex` and, while a pack draws, a second vertex stream with each
     vertex's block id (`mc_Entity`), true normal, `mc_midTexCoord` and block cell (`at_midBlock`);
   - *entity*: Horizon's entity `Vertex` (entities, block entities, particles, the hand, the sky box, the sun
     and moon), with `entityColor` from the hurt and flash overlays;
   - *full screen*: a quad from (0, 0) to (1, 1).

   Varyings travel as WGSL locations, matrices as columns, packed into `vec4`s past the device's limit.
   Every texture is read through a host function that picks its sampler (linear, trilinear, comparison…),
   fills the channels its format lacks, converts depth, and filters in the shader what WebGPU cannot filter;
   a sampler nothing provides reads a constant texel, as GL's incomplete textures do. Outputs follow
   `layout(location)` and are clamped for normalized targets. Uniforms come from one std140 block,
   `PackUniforms` on the Rust side; a uniform declared with another type is converted; unknown uniforms
   read zero and are listed in the report. A program over the device's limits (textures per stage, draw
   buffers, attachment bytes, varyings) fails in the translator, so its slot falls back.
9. **naga** (`naga_out.rs`): GLSL to a validated module to WGSL, with `diagnostic(off,
   derivative_uniformity);` in front, plus the fixes the browser's compiler needs (long `else if` chains
   flattened; struct fields passed to `inout` parameters go through a temporary for Vulkan).
10. **Schedule and frame maths** (`schedule.rs`, `frame_math.rs`, `noise.rs`): the frame's clears, which side
   of each buffer every full-screen pass reads and writes (`flip.*` honoured), mipmap generation and the
   copy-back at the end of the frame; the shadow camera, the sun, the moon, smoothed wetness and eye
   brightness, the timers, the previous frame's matrices; the noise texture.

A program that fails at any step is recorded and its slot's next fallback is tried; only an exhausted chain
skips the slot, or fails the pack when the frame cannot do without it (terrain, water, the composites,
`final`).

The client (`gfx/glsl_pack/`, `game/shader_packs.rs`, `game/glsl_pack.rs`) builds the targets and one pipeline per program, and
draws the world when the pack is on:

| Step | Program | Draws |
|---|---|---|
| Clears | | the cleared buffers, both sides, to their clear colours |
| Shadow | `shadow` | the sections in the light's box and entities; `shadowtex1` is copied, then translucent sections |
| Sky | `gbuffers_skybasic`, `gbuffers_skytextured` | a sky box, the sun and moon |
| Opaque | `gbuffers_terrain`, `gbuffers_entities` | solid and cutout sections, entities and particles |
| | `gbuffers_hand` | `depthtex2` is copied, the held item drawn, `depthtex1` copied |
| Deferred | `deferred`… | full-screen passes |
| Translucent | `gbuffers_water` | translucent sections |
| Composite | `composite`…, `final` | full-screen passes, `final` into the world's view |
| Overlays | Horizon's own | block outline, cracks, glint, beacon beams, lightning, the world border, weather, see-through text |

Then Horizon's own passes follow as usual (the HUD). A full-screen pass reads every buffer as it was before
the pass and writes the other side of the buffers it draws; buffers that are not cleared every frame keep
their last result for the next one (temporal anti-aliasing).

### Conventions

Horizon renders with reversed Z (1 at the near plane) and wgpu's clip space, and its textures start at the
top row; packs expect OpenGL's. The host converts at the edges so the pack's maths stays as written:

- `ftransform()` returns GL clip space, `hz_to_gl(c) = (c.x, c.y, c.w - 2c.z, c.w)`, and `gbufferProjection`
  is that conversion times Horizon's projection: GL's own projection (a positive `[1][1]`), so
  `gbufferProjectionInverse` takes the pack's `(uv, depth)` back to view space and packs that read its
  elements directly (FOV, lens flares) see what they expect.
- The vertex host negates clip y on output, so pack targets are stored bottom row first, as GL stores them:
  texture coordinates, `gl_FragCoord.y` and `dFdy` then follow GL's window y with no conversion. Pack
  pipelines that cull face the other way. `final` alone draws upright into the screen (no negation; its
  `gl_FragCoord.y` and `dFdy` are flipped back), and Horizon's overlays draw after it against an upright copy
  of the pack's depth.
- Depth reads return `1 - d`, and so does `gl_FragCoord.z`; hardware shadow compares use `GreaterEqual`
  against `1 - ref`.
- The hand's depth is squeezed as `MC_HAND_DEPTH` does (GL depth 0.44 to 0.56), in front of the world.
- 8-bit targets read back exact multiples of 1/255: packs compare material ids with `==`, and some drivers
  decode 8-bit texels a bit off.
- The shadow camera follows the format notes: an orthographic box of `shadowDistance` around the player,
  turned by the sun's angle and `sunPathRotation`, its origin snapped to `shadowIntervalSize`.

### Block ids and normals

`mc_Entity` and the face normal travel in the alpha byte of `TerrainVertex::color`, which is otherwise
always 255: `normal << 5 | material`. `material` indexes up to 31 `(id, fluid)` entries: the pack's
`block.properties` ids, or, for packs without one (SEUS), the pre-1.13 numeric ids old packs compare
(`legacy_ids.rs`: stone 1, grass block 2, water 9, leaves 18, wool 35, flowers 37 and 38…; others -1).
`mc_Entity.y` is 1 on fluid surfaces. Quads that are not axis-aligned (cross plants) carry a diagonal
normal. The mesher writes the tag only when a pack compiled (`MeshTables::set_pack_tags`), and applies the
pack's `oldLighting` and `ambientOcclusionLevel`; without a pack the vertex is unchanged.

## Finding what goes wrong

`HORIZON_SHADER_PACK_DEBUG=colortex4` (or `depth`, `shadow`, `shadowcolor0`) shows one of the pack's
textures instead of its image, after the frame's passes: enough to bisect a pack's passes without a GPU
debugger. `HORIZON_SHADER_PACK_SHADOWS=0` runs the pack without its shadow program. Programs that do not
compile are logged and their slots skipped.

## Limits

- The overworld only; other dimensions and the title screen draw as usual.
- Weather, clouds, glint, cracks, beacon beams, lightning and the border stay Horizon's, drawn after `final`
  (so the pack does not light them and temporal anti-aliasing does not see them). Block entities draw with
  the entities program. Stars are not drawn. Horizon's clouds and entity blob shadows are not drawn (the pack
  has its own sky, and a shadow map replaces the blobs).
- No options screen: options come from `--shader-pack-option`.
- `gbuffers_hand_water`, compute shaders, `shadowcomp` and `prepare` passes, custom uniforms (`uniform.*`
  in shaders.properties), 3D and raw custom textures are not supported.
- Item lighting stays baked into entity colours.
- A pack whose programs need more textures per stage or more draw-buffer bytes than the device allows is
  refused with a message in the log.

## Performance

SEUS Renewed is heavy: global illumination, ray-traced reflections, volumetric clouds, a 2048² shadow map
and temporal anti-aliasing. Compiling it takes about 1.5 s natively.

## Tests

- `cargo test -p horizon-glsl-pack`: the preprocessor, options, plan, rewriter passes, hosts, schedule and
  frame maths with small synthetic shaders and packs, the std140 offsets against naga's layouter, and the
  GL/wgpu conversion.
- With a pack on disk (never committed): `HORIZON_TEST_SHADERPACK=<pack folder> cargo test -p
  horizon-glsl-pack -- --include-ignored` compiles every program end to end;
  `HORIZON_TEST_SHADERPACK_DUMP=<dir>` also writes their WGSL, which `pnpm --dir web check:wgsl <dir>`
  compiles in Chrome's WebGPU (Tint).
