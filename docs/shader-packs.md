# Shader packs (proof of concept)

Horizon can draw the overworld with an OptiFine-format shader pack, the `shaders/` folder of `.vsh`, `.fsh`
and `.properties` files that OptiFine and Iris load. This is an experiment behind a command-line argument:
nothing in the menus turns it on, nothing is saved, and only one pack, **Miniature**, is known to work.

```sh
horizon-client --resources … --world W --shader-pack path/to/pack.zip     # a zip or a folder
```

In the browser build, add `?shaderpack=<url of the zip>` to the page's address (`web/menu/game.ts`).

The code is a clean-room rewrite: it is written from the pack format and the packs' own sources, not from
Iris, OptiFine, or the shader-pack code Horizon removed in `4e7c25bb`.

## How it works

`crates/horizon-glsl-pack` turns a pack's GLSL into WGSL, with no wgpu and no file access:

1. **Preprocess** (`preprocess.rs`): `#include`, `#define` (expanded in the text), `#if` expressions,
   option overrides, OptiFine's predefined macros (`MC_VERSION` is 1.21.11's `12111`).
2. **Rewrite** (`rewrite.rs`): GLSL 120's compatibility profile to plain GLSL 450. `attribute`, `varying`
   and `uniform` declarations are taken out and recorded; legacy built-ins become `hz_gl_*` globals;
   `texture2D(s, uv)` becomes a call to a host function per sampler; the pack's `main` becomes
   `hz_pack_main`.
3. **Host** (`host.rs`, `uniforms.rs`): a generated GLSL 450 `main` per stage. The vertex host reads
   Horizon's own `TerrainVertex` (the pack's terrain draws the same sections, from the same buffers) and fills
   `gl_Vertex`, `gl_Color`, `gl_MultiTexCoord0/1`, `gl_Normal` and `mc_Entity`; the fragment host samples
   Horizon's sprite arrays for `gtexture` and writes `gl_FragData` to the pack's `DRAWBUFFERS`. Every pack
   uniform comes from one std140 block, `PackUniforms` on the Rust side.
4. **naga** (`naga_out.rs`): GLSL to a validated module to WGSL, with
   `diagnostic(off, derivative_uniformity);` in front (packs sample textures in non-uniform control flow).

The client (`gfx/glsl_pack.rs`, `game/glsl_pack.rs`) compiles the pack when it builds a renderer and splits
the world pass when the pack draws:

| Pass | Targets | Draws |
|---|---|---|
| `gbuffers_terrain` | colortex0/6/7 + depth (cleared) | solid and cutout sections, with the pack's program |
| Horizon world | colortex0 + depth | sky, entities, particles, weather… with Horizon's pipelines |
| `gbuffers_water` | colortex0/6/7 + depth | translucent sections, with the pack's program |
| Horizon world | colortex0 + depth | clouds, world border, outlines, name tags |
| `final` | the world's view | the pack's composite of colortex0/6/7 and depthtex0 |

Then Horizon's own passes follow as usual (scaling and FXAA, hand, HUD).

### Conventions

Horizon renders with reversed Z (1 at the near plane) and wgpu's clip space, and its textures start at the
top row; packs expect OpenGL's. The host converts at the edges so the pack's math stays as written:

- `ftransform()` returns GL clip space, `hz_to_gl(c) = (c.x, -c.y, c.w - 2c.z, c.w)`, and the vertex host
  converts `gl_Position` back (exactly, when the pack kept `ftransform()`'s value, so depth matches Horizon's
  other pipelines).
- `gbufferProjection` is that conversion times Horizon's projection, so `gbufferProjectionInverse` takes the
  pack's `(uv, depth)` back to view space.
- Depth reads return `1 - d`; screen UVs are y-down, as wgpu's textures are.

### Block ids and normals

`block.properties` ids (`mc_Entity.x`) and the face normal (`gl_Normal`) travel in the alpha byte of
`TerrainVertex::color`, which is otherwise always 255: `normal << 5 | material`, where `material` indexes
the pack's distinct ids (at most 31). The mesher writes it only when a pack compiled
(`MeshTables::set_pack_tags`); without a pack the vertex is unchanged.

## Limits of the proof of concept

- Only `gbuffers_terrain`, `gbuffers_water` and `final` run, in the overworld. Entities, the hand, sky,
  clouds and particles keep Horizon's shaders; other dimensions and the title screen draw as usual.
- No shadow pass: packs are compiled with `ENABLE_SHADOWS` off.
- Only colour targets, `depthtex0`, `gtexture` and `lightmap` can be sampled; no `noisetex`, no custom
  textures, no `composite` or `deferred` passes.
- No options screen: a pack runs with its defaults.
- Ambient occlusion and face shade stay baked into the vertex colour (`gl_Color.a` is 1).
- Reflections can show on entities in front of reflective blocks, since entities do not write colortex6/7.
- With resolution scaling or FXAA on, the pack is skipped for that frame.

## Tests

- `cargo test -p horizon-glsl-pack`: the preprocessor, rewriter and host with small synthetic shaders, the
  std140 offsets against naga's layouter, and the GL/wgpu conversion.
- With a pack on disk (never committed): `HORIZON_TEST_SHADERPACK=<pack folder> cargo test -p
  horizon-glsl-pack -- --include-ignored` compiles its programs end to end;
  `HORIZON_TEST_SHADERPACK_DUMP=<dir>` also writes their WGSL, which
  `pnpm --dir web check:wgsl <dir>` compiles in Chrome's WebGPU (Tint).
