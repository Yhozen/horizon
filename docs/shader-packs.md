# Shader packs (experimental)

Horizon can draw the world with an OptiFine- or Iris-format shader pack: the `shaders/` folder of `.vsh`,
`.fsh` and `.properties` files those mods load, in a zip or a folder. It is off by default and looks exactly
like vanilla until a pack is chosen.

## Choosing a pack

**Options → Horizon Settings… → Shader Packs…** lists the packs in the game folder's `shaderpacks` folder
(zips, and folders holding a `shaders` folder). `Shaders: ON/OFF` turns packs on; clicking a pack chooses
it; `Done` applies. The pack compiles while the world keeps running ("Compiling 12/41"), then replaces
Horizon's own shaders; a chat line says whether it loaded fully, partly, or not at all.

- **Adding packs**: `Open Shader Pack Folder…`, or drop zips on the window. In the browser, `Add Shader
  Pack…` opens a file picker, and zips can be dropped on the page.
- **Settings…**: the pack's own option screens (`screen=`, sub-screens, sliders, profiles, the labels of its
  `lang/en_us.lang`). Values are saved in `shaderpacks/<pack file>.txt`, the `NAME=VALUE` file Iris and
  OptiFine keep beside a pack, so a settings file copied from Iris gives the same configuration.
- **Report…**: what the pack uses that Horizon does not have, every program that was skipped and why
  (pack file and line), the GPU compiler's messages, uniforms that read zero. `Copy Report` puts it on the
  clipboard; the same text is in the log (`logs/latest.log`, or the browser console).
- The choice is saved in `horizon-shaders.txt` (`enabled:`, `pack:`) beside `options.txt`.

Statuses: **Works** (every program the pack enables compiled), **Partial** (it runs; the report lists what is
missing), **Not supported** (it requires something Horizon lacks, such as compute shaders; the report says
what), **Failed** (an essential program could not be compiled, or the GPU rejected it: Horizon's own shaders
stay).

### In the browser

The page keeps the `shaderpacks` folder and `horizon-shaders.txt` in the browser's private storage (the
Origin Private File System), so packs and the choice survive a reload. `?shaderpack=<url of a zip>` adds that
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
observes of its host is written down in [shader-pack-format-notes.md](shader-pack-format-notes.md).

## How it works

`crates/horizon-glsl-pack` turns a pack into WGSL and a plan of its passes, with no wgpu and no file access:

1. **Options** (`options.rs`): a pack's options are found in its sources and changed by rewriting the source
   text, as the pack's own options screen would.
2. **Plan** (`directives.rs`, `plan.rs`): the program slots present after OptiFine's fallbacks (an entities
   slot without `gbuffers_entities` runs `gbuffers_textured_lit`, and so on), each buffer's format, clear and
   mipmaps, the shadow map's constants, `shaders.properties` (`clouds`, `oldLighting`, custom textures,
   disabled programs), the noise size and the light levels.
3. **Preprocess** (`preprocess.rs`): `#include`, `#define` (expanded in the text), `#if`, OptiFine's
   predefined macros (`MC_VERSION` is 1.21.11's `12111`).
4. **Rewrite** (`rewrite/`): GLSL 120's compatibility profile to plain GLSL 450, in token passes:
   - functions `main` does not reach are dropped (packs keep invalid code in unused functions);
   - functions that take a `sampler2D` are cloned per sampler passed to them (`mono.rs`), as WGSL
     cannot pass textures around;
   - locals that shadow a sampler, a uniform or a built-in function are renamed (`scope.rs`);
   - `const` on locals and parameters is dropped, returns are converted to the function's type, global
     initialisers that need run-time values move into `main` (`fixes.rs`);
   - interface declarations are recorded and taken out; legacy built-ins become `hz_gl_*` globals; sampling
     calls (`texture2D`, `texture2DLod`, `textureGrad`, `shadow2D`, `texelFetch`, `textureSize`…) become
     calls to host functions per sampler.
5. **Host** (`host/`, `uniforms.rs`): a generated `main` per stage around the pack's, per geometry:
   - *terrain*: Horizon's `TerrainVertex`, with `mc_Entity` and the face normal from the vertex tag;
   - *entity*: Horizon's entity `Vertex` (entities, block entities, particles, the hand, the sky box, the sun
     and moon), with `entityColor` from the hurt and flash overlays;
   - *full screen*: a quad from (0, 0) to (1, 1).

   Varyings travel as WGSL locations, matrices as columns, packed into `vec4`s past the device's limit.
   Every texture is read through a host function that picks its sampler (linear, trilinear, comparison…),
   fills the channels its format lacks, converts depth, and filters in the shader what WebGPU cannot filter.
   Outputs to normalized targets are clamped, as GL's fixed-point targets would. Every pack uniform comes
   from one std140 block, `PackUniforms` on the Rust side; unknown uniforms read zero.
6. **naga** (`naga_out.rs`): GLSL to a validated module to WGSL, with `diagnostic(off,
   derivative_uniformity);` in front. Struct fields passed to `inout` parameters go through a temporary,
   which naga's SPIR-V writer (Vulkan) needs.
7. **Schedule and frame maths** (`schedule.rs`, `frame_math.rs`, `noise.rs`): the frame's clears, which side
   of each buffer every full-screen pass reads and writes, mipmap generation and the copy-back at the end of
   the frame; the shadow camera, the sun, the moon, smoothed wetness and eye brightness, the timers, the
   previous frame's matrices; the noise texture.

The client (`gfx/glsl_pack/`, `game/glsl_pack.rs`) builds the targets and one pipeline per program, and
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

- `ftransform()` returns GL clip space, `hz_to_gl(c) = (c.x, -c.y, c.w - 2c.z, c.w)`, and the vertex host
  converts `gl_Position` back.
- `gbufferProjection` is that conversion times Horizon's projection, so `gbufferProjectionInverse` takes the
  pack's `(uv, depth)` back to view space. The y flip lives in that projection, so texture coordinates and
  `gl_FragCoord` need no flip; but window y then runs against view-space y, so `dFdy` is negated in fragment
  shaders, which keeps normals rebuilt with `cross(dFdx(p), dFdy(p))` facing out.
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
