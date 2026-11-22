# Shader pack format notes

The facts about the OptiFine shader pack format that Horizon's shader pack support
([shader-packs.md](shader-packs.md)) relies on: what a pack observes from its host.

> **Provenance.** These notes describe pack-visible behaviour only. They come from the public format
> documentation (the shaders.properties docs and OptiFine's `shaders.txt`) and from a behaviour-only
> description prepared in isolation from implementation work. No code was copied from any loader or pack.
> Facts marked "(medium)" or "unknown" are less certain; unmarked facts are well established. Where the
> public docs disagree with each other, the entry says so.
> Sections 15–17 and the `block.properties` entries of section 9 add facts from black-box measurement of
> Iris (probe packs, pictures only) and from running Horizon on the shader pack corpus
> ([shader-pack-compat.md](shader-pack-compat.md)). Each says its source; "(unverified)" marks Horizon's
> own reading where neither the docs nor a measurement settle it.

Terms: *gbuffers* are the programs that draw world geometry; *composite-style* passes (`shadowcomp`,
`prepare`, `deferred`, `composite`, `final`) draw one full-screen quad.

## 1. Frame order

1. Per-frame clears (section 3).
2. `shadow`, then `shadowcomp`, `shadowcomp1`…`99`, then `prepare`, `prepare1`…`99`.
3. Sky: the horizon cone (in the fog colour), sky dome, sun, moon, stars.
4. Opaque and cutout terrain; then entities, block entities, opaque particles, block cracks (where outlines
   and cracks fall around the depthtex1 copy is unknown).
5. depthtex2 copy, the solid hand, depthtex1 copy, then `deferred`, `deferred1`…`99`.
6. Translucent terrain (`gbuffers_water`), translucent particles, weather, world border.
7. Translucent hand (`gbuffers_hand_water`).
8. `composite`, `composite1`…`99`, then `final`.

- The docs do not fix the order inside the gbuffers steps (medium for steps 3–7). What packs rely on: weather,
  water and translucent held items draw after `deferred`.
- Compute programs (`_a`…`_z`) run before their pass's vertex stage; gbuffers cannot have them. A missing or
  disabled program uses its fallback (section 2); with no fallback it draws nothing.

## 2. Programs and fallbacks

| Program | Draws | Falls back to |
|---|---|---|
| `basic` (`line` in newer packs, falling back to `basic`) | leashes, block outline, fishing line | none |
| `textured` | unlit particles, world border, beacon beams | `basic` |
| `textured_lit` | lit particles, most dropped items | `textured` |
| `skybasic` | sky dome, horizon cone, stars, void disc, sunrise fan | `basic` |
| `skytextured`, `clouds` | sun, moon; clouds | `textured` (for clouds the docs disagree: some say none) |
| `terrain` | solid and cutout blocks | `textured_lit` |
| `damagedblock` | block cracks | `terrain` |
| `block` | block entities, moving pistons, end portal and gateway | `terrain` |
| `entities` | entities, name tags, text, lightning | `textured_lit` |
| `armor_glint`, `spidereyes`, `beaconbeam` | glint; spider, enderman and dragon eyes; beacon beam | `textured` |
| `hand` | hand, opaque held items | `textured_lit` |
| `weather` | rain, snow | `textured_lit` |
| `water` | translucent terrain | `terrain` |
| `hand_water` | translucent held items | `hand` |

- With `clouds=off`, clouds are not drawn at all. Shadow sub-programs (`shadow_solid`, `shadow_cutout`,
  `shadow_water`, `shadow_entities`, `shadow_block`) fall back to `shadow`.

## 3. Render targets, clears and flips

| Target | Legacy name | Default clear | Default format |
|---|---|---|---|
| colortex0 | `gcolor` | (fogColor, 1) | RGBA8 |
| colortex1 | `gdepth` | (1, 1, 1, 1) | RGBA8; RGBA32F if any program declares `gdepth` |
| colortex2–15 | `gnormal`, `composite`, `gaux1`–`gaux4` (2–7) | (0, 0, 0, 0) | RGBA8 |
| shadowcolor0/1 | `shadowcolor` = 0 | (1, 1, 1, 1) | RGBA8 |
| depthtex, shadowtex | `gdepthtex` = depthtex0 | 1.0, fixed | 32-bit depth |

**Formats**
- Set with `const int colortexNFormat = RGBA16F;` inside a block comment, so GLSL still compiles. The legacy
  form `/* GAUX4FORMAT:RGBA32F */` (also `RGB32F`, `RGB16`) is honoured.
- RGB8 and RGB16 are true three-channel formats, so alpha reads 1.
- Accepted names: the GL normalized, `_SNORM`, float and integer formats (RGBA8, RGB10_A2, RGBA16F, R32F...).

**Clears**
- Every frame, both halves of every target with Clear on are cleared to its clear colour.
- `const vec4 colortexNClearColor = vec4(...)` sets the colour; all four components are needed.
- `const bool colortexNClear = false;` skips the per-frame clear. Legacy names work too (`gaux4Clear = false`
  means colortex7).
- A buffer with Clear off still gets its clear colour on the first frame and after every resize. The default
  clear alpha is 1, which avoids pink artefacts in older packs.

**Ping-pong (flips)**
- Each colour target has two halves, main and alternate. Gbuffers always read and write the main half.
- A `deferred` or `composite` pass reads every buffer as it stood before the pass. It writes the other half of
  the buffers it lists, then those buffers flip. So a pass that samples and writes the same colortex reads the
  previous contents.
- `final` reads the latest half of everything. A pass with no fragment program flips nothing.
  `flip.<program>.<buffer>=false` turns one flip off.
- At the end of the frame, a flipped buffer that is not cleared per frame is copied back to its main half.
  Next frame's gbuffers then see the last composite result.

**Draw buffers**
- `/* DRAWBUFFERS:017 */` lists single digits (targets 0–9); `/* RENDERTARGETS: 0,3,13 */` reaches every
  target. Either goes in the fragment source, on its own line.
- Output i (`gl_FragData[i]`, or `layout(location = i)`) goes to the i-th listed target. Outputs past the list
  are dropped. A listed target that is never written holds undefined values.
- With no directive, a gbuffers program writes colortex0 only. (The docs say 0–7; the observed behaviour is
  colortex0.) (medium)
- In gbuffers and shadow programs, units 0–3 are the atlas, lightmap, normals and specular. colortex0–3 cannot
  be sampled there; colortex4 and up can.

## 4. Depth textures

| Texture | Contains |
|---|---|
| depthtex0 | everything, including translucents and the translucent hand |
| depthtex1 | no translucents; includes the solid hand; valid from `deferred` on |
| depthtex2 | no translucents and no hand |

- All three are single-channel at window size and do not flip. Hand pixels: depthtex1 differs from depthtex2,
  or depth is below about 0.56 (section 8).

## 5. Sampler filtering

| Sampler | Filter | Wrap | Mipmaps |
|---|---|---|---|
| colortexN (float, normalized) | linear | clamp to edge | none, unless enabled (below) |
| colortexN (integer formats) | nearest | clamp to edge | none |
| depthtex0/1/2 | nearest | clamp | none |
| noisetex | linear | repeat | none |
| shadowtex0/1 | linear | clamp | none, unless enabled |
| shadowcolor0/1 | linear | clamp | never sampled with mips (medium) |
| custom PNG textures | nearest | repeat | none |
| `normals`, `specular` | nearest, mips | clamp | anisotropy from video settings (medium) |
| atlas (`gtexture`, `texture`, `tex`), `lightmap` | as vanilla | as vanilla | as vanilla |

**colortex mipmaps**
- `const bool colortexNMipmapEnabled = true;` is read from a pass's fragment source. It works in `deferred`,
  `composite` and `final`. (The docs disagree about `prepare` and `begin`.)
- Before each such pass the mip chain is rebuilt for the half that pass reads, changed or not, and the pass
  itself samples it trilinear.
- Later passes sample it trilinear for the rest of the frame while they read that half. Once a pass writes
  the buffer (it flips), the half just written has no fresh mips and reads at level 0 until a pass rebuilds
  them. (Horizon's model: GL keeps the filter on the texture object that was mipmapped. Not measured under
  Iris. Without it, Fantasy's FXAA read stale levels and drew black dots.)

**Shadow filtering keys**
- `shadowtexNNearest` (also `shadowtexNearest` for 0, `shadowNMinMagNearest`): nearest. `shadowtexNMipmap`
  (also `generateShadowMipmap` for both, `shadowtexMipmap` for 0): a full mip chain; sampling becomes
  trilinear, or nearest-mipmap-nearest with Nearest set. These mipmaps are built once per frame, after the
  shadow pass and before `shadowcomp`.
- `shadowHardwareFiltering` (or `…0`, `…1`): the shadowtex becomes a depth-compare sampler with the
  less-or-equal test. Off by default (medium: the docs only show `= true`).
- Shadow depth reads swizzle as (R, R, R, 1), so `.z` and `.b` also return depth.
- `shadowcolorNMipmap` still builds mips, but sampling ignores them; `shadowcolorNNearest` has no visible
  effect. (medium)
- Custom textures: a `.mcmeta` beside the PNG with `blur` gives linear, with `clamp` clamp to edge. Raw
  (non-PNG) textures default to linear and clamp.

## 6. Shadow camera

**Projection**
- Orthographic, half-extent `shadowDistance` blocks (default 160); render distance plays no part.
- Depth range: as if the camera sat 100 blocks toward the light with near 0.05 and far 256. `shadowNearPlane`
  and `shadowFarPlane` override these.
- `shadowMapFov` switches to a perspective projection. `shadowMapResolution` (default 1024) sets the square
  map size; `/* SHADOWRES:1024 */` is the legacy form.

**Model-view**
- The sun casts while `sunAngle` < 0.5, the moon otherwise.
- Rotation only, applied to camera-relative positions: Rx(90°) · Rz(−360° · s) · Rx(`sunPathRotation`), then a
  snapping offset, with s = `shadowAngle` − 0.25 wrapped into [0, 1).
- Snapping: per world axis, offset = fmod(cameraPosition, I) − I/2, with I = `shadowIntervalSize` (default 2;
  0 turns snapping off). fmod keeps the sign, so negative coordinates give offsets in (−1.5 I, 0.5 I).

**Pass contents, in order**
1. Depth cleared to 1; shadowcolors cleared (unless `shadowcolorNClear=false`).
2. Opaque and cutout terrain.
3. Entities (the local player included in practice) and block entities.
4. Depth copied into shadowtex1.
5. Translucent terrain, into shadowtex0 and the shadowcolors only.
6. Mipmaps, then `shadowcomp`.

- Alpha test: solid and translucent terrain none, cutout terrain > 0.5, entities and block entities > 0.1.
- `shadow` is shadowtex0; if the pack declares `watershadow`, then `watershadow` is shadowtex0 and `shadow` is
  shadowtex1.
- Blending is off: translucent geometry overwrites shadowcolor. Terrain back faces are not culled.
- A shadow program with no draw-buffer directive writes shadowcolor0 and 1.
- Keys: `shadowTerrain`, `shadowTranslucent`, `shadowEntities` (default true), `shadowBlockEntities`,
  `shadowPlayer` (default false, though entity shadows are on).
- Culling: with `shadowDistanceRenderMul` below 0 (the default), only chunks that can cast into the view are
  drawn. At 0 or above, chunks are limited to `shadowDistance × mul` blocks (0 draws nothing).
  `shadow.culling=false` gives distance-only culling. `entityShadowDistanceMul` limits entities.
- Vanilla's round entity shadows are off whenever a shadow map exists.

## 7. Built-in inputs per program

**Composite-style passes**
- A quad from (0, 0, 0) to (1, 1, 0); `gl_MultiTexCoord0` = (x, y, 0, 1).
- `gl_ModelViewMatrix` = identity; `gl_ProjectionMatrix` maps [0, 1] to [−1, 1], so `ftransform()` = (2x − 1,
  2y − 1, 0, 1).
- `gl_Color` = white; `gl_Normal` = (0, 0, 1); normal and texture matrices = identity. Viewport = the full
  buffer, changed by `scale.<program>`. Blending is off. Unknown sampler names read colortex0.

**Terrain**

| Input | Value |
|---|---|
| `gl_Vertex` | camera-relative world position |
| `gl_ModelViewMatrix`, `gl_ProjectionMatrix` | `gbufferModelView`, `gbufferProjection` |
| `gl_MultiTexCoord0` | atlas UV; `gl_TextureMatrix[0]` = identity |
| `gl_MultiTexCoord1` (and 2) | (block, sky) light, each clamped to 8…248 (level × 16, smooth values between) |
| `gl_TextureMatrix[1]` | scale 1/256, offset 1/32 on x, y, z (every program) |
| `gl_Color` | biome tint × smooth AO × face shade; AO in alpha with `separateAo=true` |
| `gl_Normal` | world-space face normal |
| `mc_Entity` | (id, fluid flag, 0, 1): section 9 |
| `mc_midTexCoord` | centre UV of the quad's sprite (also `gl_MultiTexCoord3`) |
| `at_tangent` | xyz tangent; the sign of w gives bitangent handedness |
| `at_midBlock` | offset to the block centre in 1/64 blocks; w = emitted light 0–15 |

- Face shade applies only with `oldLighting=true`; otherwise it is 1.

**Entities and other draws**
- Model-view is the draw's own matrix; projection is `gbufferProjection`. `gl_Normal` is the model normal,
  with the matching `gl_NormalMatrix`.
- `gl_Color` = vertex colour × colour modulator; entity directional lighting is left to the pack. With no
  lightmap input, `gl_MultiTexCoord1` = (240, 240).
- `gl_TextureMatrix[0]` is the draw's texture matrix (it scrolls for glint). (medium)
- `mc_Entity` is not supplied outside terrain; probably reads (0, 0, 0, 1). (unknown)

**Fog built-ins** (medium)
- `gl_Fog.color` = fog colour; start and end = the environment fog; scale = 1 / (end − start); density = 0.
  `gl_FogFragCoord` starts at 0; the vertex shader may write it.

## 8. Hand

- Projection: a fixed 70° field of view (not the FOV slider), clip z multiplied by `MC_HAND_DEPTH`.
- `MC_HAND_DEPTH` = 0.125, so hand depth lies in about [0.4375, 0.5625]. Undo it with ndc = (d × 2 − 1) /
  `MC_HAND_DEPTH`.
- Model-view: view bobbing and hurt tilt only. Depth is not cleared before the hand. The solid hand draws
  before `deferred`; translucent held items draw last, before `composite`.

## 9. Legacy block ids

With no `block.properties`, `mc_Entity.x` takes pre-1.13 numeric ids from a fixed table. Names match current
block names exactly, for every block state.

| Id | Blocks |
|---|---|
| 1, 2, 4, 12, 24 | stone (with granite, diorite, andesite); grass_block; cobblestone; sand; sandstone |
| 9, 11 | water and lava, every level (the still ids) |
| 18 | oak, birch, jungle, spruce, acacia and dark oak leaves |
| 31 | "grass", seagrass, sweet_berry_bush |
| 35, 95, 160 | the 16 wools; the 16 stained glass blocks; the 16 stained glass panes |
| 37 | dandelion, poppy, blue_orchid, allium, azure_bluet, the four tulips, oxeye_daisy, cornflower, lily_of_the_valley, wither_rose |
| 41, 42, 57 | gold_block, iron_block, diamond_block |
| 50, 51, 89, 124 | torch; fire; glowstone; redstone_lamp, lit and unlit |
| 59, 79, 111 | wheat, carrots, potatoes; ice; lily_pad |
| 175 | sunflower, lilac, tall_grass, large_fern, rose_bush, peony, tall_seagrass |
| −123 | emerald_block (some older packs expect it there) |

- No other ids appear: nothing for 8 and 10 (flowing fluids), 30, 38, 161, 174 or saplings.
- No id for mangrove, cherry, azalea or pale oak leaves, kelp, newer ores, tinted glass, soul fire, packed or
  blue ice, or ferns.
- Unmapped blocks get −1. The block called "grass" is now `short_grass`, so the "grass" entry probably matches
  nothing. (medium)
- `mc_Entity.y` = 1 on fluid surfaces (water and lava, including waterlogged water), otherwise 0. (The docs
  say −1 for non-fluids; the observed value is 0.) (medium)
**`block.properties` and block states**
- Each `block.<id>=` entry lists names: `[minecraft:]name`, optionally followed by `:property=v1,v2`
  filters (`oak_log:axis=y`, `farmland:moisture=7`). A filtered name matches only the states whose
  properties take one of the listed values. (public docs)
- Other namespaces (mod blocks) and tags (`#…`, `%…`) never match a vanilla block. Horizon skips them and
  counts them in its report.
- When several entries match one state, Horizon picks the entry naming the most properties, and the first
  in the file on a tie. (Horizon's rule; unverified against Iris)
- With a `block.properties`, a block it does not list reads 0; with none, the legacy table above applies and
  unlisted blocks read −1. (unverified)
- The file goes through the preprocessor. `#if` and `#define` act on physical lines, so a `\` continuation
  before an `#else` does not pull the directive into the entry. Continuations are joined afterwards, and
  live `#define`s are expanded: packs list block groups as macros. (observed in packs; unverified against Iris)
- Horizon's terrain vertices carry the id as 16 bits. Ids outside 0–65534 (negative ones included) reach the
  shader as unlisted.

- `entityId`, `blockEntityId` and `heldItemId` read −1 with no mapping. The docs give 0 when the properties
  file is absent and 65535 for an unlisted entity. (medium)

## 10. Uniforms: meaning and units

| Uniform | Meaning |
|---|---|
| `sunPosition`, `moonPosition` | view-space position, length 100 |
| `shadowLightPosition` | the sun while `sunAngle` < 0.5, else the moon |
| `upPosition` | view-space up, length 100 (`gbufferModelView[1]` × 100) |
| `sunAngle` | 0 sunrise, 0.25 noon, 0.5 sunset, 0.75 midnight, with vanilla's easing |
| `shadowAngle` | `sunAngle` by day, `sunAngle` − 0.5 by night (docs disagree on 0–0.5 vs 0–1) |
| `worldTime` | day time mod 24000 ticks; 0 in dimensions with fixed time other than Nether and End |
| `worldDay`, `moonPhase` | day time / 24000; 0–7 |
| `frameTime`, `frameTimeCounter`, `frameCounter` | last frame in seconds (1 ms steps); seconds, reset at 3600; frames, wrap at 720720 |
| `near`, `far` | 0.05; render distance × 16 blocks (not the clip plane) |
| `viewWidth`, `viewHeight`, `aspectRatio` | pixels; width / height |
| `rainStrength`, `wetness` | rain level 0–1; the same smoothed (below) |
| `eyeBrightness` | (block, sky) light × 16 at eye level, 0–240 |
| `eyeBrightnessSmooth` | the same, 1 s half-life both ways, truncated to integers |
| `isEyeInWater` | 0 none, 1 water, 2 lava (not for spectators), 3 powder snow |
| `nightVision` | effect strength, or conduit water vision, 0–1 |
| `blindness` | remaining effect seconds clamped to 0–1 (1 if infinite) |
| `screenBrightness` | the brightness option, 0–1 |
| `skyColor`, `fogColor` | upper sky colour; horizon and fog colour |
| `heldBlockLightValue` | main-hand emission 0–15; the brighter hand with `oldHandLight` (default true) |
| `entityColor` | (overlay rgb, 1 − overlay alpha); hurt ≈ (1, 0, 0, 0.3); creeper or TNT flash (1, 1, 1, 0.75 k); else 0 (medium) |
| `atlasSize` | size of the bound atlas, or 0 |
| `cameraPosition` | world position, shifted in x and z by multiples of 30000 past ±30000 or on a jump over 1000 |
| `previousCameraPosition`, `eyeAltitude` | last frame's position, with the same shift; camera y |
| `gbufferModelView` | the camera's rotation, then view bobbing and hurt tilt: player space to view space. `gbufferModelViewInverse[3]` is the way from the player's head to the bobbing camera (the public coordinate-spaces page) |
| `gbufferProjection` | the plain perspective projection of the world: no view bobbing (vanilla multiplies the bob into its projection; packs must not see it there), no jitter |
| `gbufferPrevious*` | last frame's matrices; identity on the first frame |
| `shadowModelView`, `shadowProjection` | the matrices of section 6 |

**Wetness smoothing**
- Exponential, per frame. Rising uses `wetnessHalflife` (default 600), falling `drynessHalflife` (200).
- Observed units: the value × 0.1 s, so 600 is 60 s. The docs call the values ticks and disagree on which key
  applies to which direction.
- Observed quirk: a `drynessHalflife` directive sets the rising half-life, and the falling one stays at 200.
  (medium)

## 11. Default and custom textures

- With no `_n` or `_s` texture, `normals` is 1×1 (127, 127, 255, 255) / 255 and `specular` 1×1 (0, 0, 0, 0).
- `noisetex`: square, `noiseTextureResolution` pixels (default 256), RGBA8. RGB are independent random bytes
  from a fixed seed; A = 255. It does not flip. `texture.noise` replaces it.
- PNGs load as RGBA8 with no sRGB conversion; the image's top row is at v = 0. (medium)
- `texture.<stage>.<sampler>=<path>` rebinds a sampler for a whole stage. Stages: `gbuffers` (shadow programs
  too), `shadowcomp`, `prepare`, `deferred`, `composite` (including `final`).
- `texture.gbuffers.gaux1` rebinds reads of colortex4 in gbuffers and shadow programs, while they still render
  into colortex4.
- In `deferred` and `composite`, a stage override for a buffer stops applying once an earlier pass of that
  stage has written the buffer.

## 12. Gbuffers state and alpha test

- Blending follows the draw type (translucent: src-alpha, one-minus-src-alpha), on every draw buffer.
  `blend.<program>` and `blend.<program>.<buffer>` override it. `gbuffers_spidereyes` defaults to (SRC_ALPHA,
  ONE, ZERO, ONE).
- Depth test, depth write and face culling follow the draw type. (medium)
- The alpha test is a `discard` at the end of `main` when output 0's alpha fails. It applies only when the
  program writes `gl_FragData[0]` or `gl_FragColor`. `alphaTest.<program>=off|<func> <ref>` overrides it.

| Geometry | Alpha test |
|---|---|
| solid terrain, solid entities, sky dome, sun and moon, lines, leashes, lightning, beacon beams | none |
| cutout terrain | > 0.5 |
| water, translucent terrain, textured without colour, armor glint, sunrise fan | > 0.0001 |
| cutout or translucent entities, block entities, moving blocks, particles, weather, hand, cracks, clouds, textured with colour | > 0.1 |

## 13. GLSL leniency packs rely on

**Versions**
- Each stage is judged on its own. Below 150 with no profile counts as the compatibility profile, so mixed
  `#version 120` and `130` programs behave alike. `attribute` and `varying` work; `1.0f` is legal.

**Built-ins that work**
- `gl_Vertex`, `gl_Normal`, `gl_Color`, `gl_MultiTexCoord0`–`7`, `gl_ModelViewMatrix`, `gl_ProjectionMatrix`,
  `gl_ModelViewProjectionMatrix`, `gl_NormalMatrix`, their inverses, `gl_TextureMatrix[0..2]` (constant
  index), `ftransform()`, `gl_FogFragCoord`, `gl_Fog.*`, `gl_FragData[const]`, `gl_FragColor` (=
  `gl_FragData[0]`). `gl_Fog` exists in every stage.
- `gl_FrontColor` writes compile and do nothing. Reading `gl_Color` in a fragment shader gives zero.
- `gl_TexCoord[]` is only partly supported. (medium)

**Samplers and functions**
- `uniform sampler2D texture;` (or `gcolor`) works with `texture2D()`; declaring both means one albedo
  sampler.
- `texture2D`, `texture3D` and their `Lod`, `Proj` and `Grad` forms (`texture2DGradARB` too), `texelFetch2D`,
  `textureSize2D` all work. `shadow2D` and `shadow2DLod` return the comparison as a vec4.

**Mistakes that compile anyway**
- Local variables named `texture` or `sample`; unused functions that would not compile; `const` locals
  initialised from parameters; stray semicolons.
- A varying the fragment stage reads but the vertex stage never writes reads zero. Mismatched out/in types of
  the same size are converted.
- Some older packs' vertex shaders take the fractional part of world y plus a 0.001 offset; hosts raise that
  offset to 0.01 so the result stays stable.

## 14. shaders.properties and options

**Rendering keys**

| Key | Effect |
|---|---|
| `clouds=fast\|fancy\|off` | `off` hides clouds; below render distance 4 the player's setting wins |
| `oldLighting` | default false: no face shade on terrain and fluids |
| `oldHandLight` | default true: held light takes the brighter hand |
| `separateAo` | default false: AO in `gl_Color.a` when true |
| `underwaterOverlay`, `sun`, `moon`, `vignette` | on or off; `underwaterOverlay` defaults to off |
| `frustum.culling`, `backFace.*`, `rain.depth`, `beacon.beam.depth` | defaults on, off, off, off |
| `particles.ordering=mixed\|after\|before` | translucent particles against `deferred` (default `after` with a deferred pass) |
| `alphaTest.<p>`, `blend.<p>[.<buf>]`, `scale.<p>`, `flip.<p>.<buf>`, `size.buffer.<buf>` | per-program state |
| `program.<[worldN/]name>.enabled=<expr>` | a disabled program uses its fallback |
| `uniform.<type>.<name>=<expr>`, `variable.*` | custom uniforms, computed on the CPU |
| `sliders`, `profile.*`, `screen` | how options appear in the options screen |

**Options in the source**
- `#define X` is an option that is on; `//#define X` is one that is off. Either counts only if the same file,
  after includes, tests X with `#ifdef` or `#ifndef` (an `#if` does not count). Toggling adds or removes the
  leading `//`.
- `#define N v // [a b c]` is a value option; changing it replaces the value on that line.
- `const` options exist only for a fixed list of names (`shadowMapResolution`, `shadowDistance`,
  `sunPathRotation`, `ambientOcclusionLevel`, the half-lives, `noiseTextureResolution`, the shadow filter and
  mipmap flags...); the value after `=` is replaced. An option must have the same default in every file.
- `ambientOcclusionLevel` L (0–1): each block's shade b becomes 1 − L × (1 − b) before smooth AO uses it.

**Macros** (inserted after `#version`)
- `MC_VERSION` is the version as a number (1.21.0 is 12100); `MC_GL_VERSION`, `MC_GLSL_VERSION`; `MC_OS_*`,
  `MC_GL_VENDOR_*`, `MC_GL_RENDERER_*` for the host.
- `MC_RENDER_QUALITY` and `MC_SHADOW_QUALITY` (0.5 to 2.0); `MC_HAND_DEPTH` = 0.125.
- `MC_NORMAL_MAP`, `MC_SPECULAR_MAP`, `MC_OLD_LIGHTING`, `MC_OLD_HAND_LIGHT`, `MC_ANISOTROPIC_FILTERING` are
  defined when on; `renderStage` is compared against `MC_RENDER_STAGE_*`.
- The values one Iris build actually defines are in section 15.
- `program.<name>.enabled` expressions: section 16. Custom uniforms: section 17.

## 15. Macros and feature flags, as measured

**Source: black-box measurement.** Two probe packs (`tools/shader_probe_packs/macros`, `macros-features`)
encode each macro's presence and value in the colour of `final`. They were run under Iris 1.11.7 with
Sodium 0.9.2 on Minecraft 26.3 (Mesa llvmpipe, OpenGL 4.5 core, Linux, 854×480), using `tools/iris_oracle.py`.
"Defined, empty or 0" means `#ifdef` holds and the value reads 0; the probe cannot tell empty from 0.

| Macro | Iris 1.11.7 |
|---|---|
| `MC_VERSION` | 260300 for 26.3: major × 10000 + minor × 100 + patch |
| `IRIS_VERSION` | 11107 (1.11.7) |
| `MC_GL_VERSION` | 45 on a GL 4.5 context, not 450: `#if MC_GL_VERSION >= 320` is false there |
| `MC_GLSL_VERSION` | 450 |
| `IS_IRIS`, `MC_OS_LINUX` | defined, empty or 0 |
| `IRIS_TAG_SUPPORT` | 2 |
| `MC_RENDER_QUALITY`, `MC_SHADOW_QUALITY`, `MC_HAND_DEPTH` | 1.0, 1.0, 0.125 |
| `MC_MIPMAP_LEVEL` | 4 |
| `MC_NORMAL_MAP`, `MC_SPECULAR_MAP` | defined (empty or 0) even for a pack with no normal or specular settings |
| `MC_GL_VENDOR_OTHER`, `MC_GL_RENDERER_OTHER` | defined, empty or 0, on Mesa; `MC_GL_VENDOR_MESA` and the `MC_GL_RENDERER_*` for Mesa or Gallium stay undefined |
| `MC_TEXTURE_FORMAT_LAB_PBR`, `MC_ANISOTROPIC_FILTERING`, `MC_FXAA_LEVEL`, `MC_OLD_LIGHTING`, `MC_OLD_HAND_LIGHT` | undefined |
| `__VERSION__` | undefined to `#ifdef`: the loader resolves the pack's conditionals itself, before the driver sees the text |

- The GL version, vendor and renderer follow the real driver. On a desktop GPU expect something like
  `MC_GL_VERSION` 46 and a named vendor. (unverified: only llvmpipe was measured)
- Horizon defines the measured values, except `MC_GL_VERSION` and `MC_GLSL_VERSION` (410 for both: a host
  without compute shaders or storage buffers) and the vendor and renderer (always `…_OTHER`). See
  `preprocess::predefined` in `horizon-glsl-pack`.

**Feature flags**
- With no `iris.features.*` line, no `IRIS_FEATURE_*` macro is defined. (measured)
- Iris defines `IRIS_FEATURE_<flag>` (empty or 0) for each flag a pack lists in `iris.features.optional` that it
  supports. Measured as defined: `CUSTOM_IMAGES`, `SSBO`, `COMPUTE_SHADERS`, `SEPARATE_HARDWARE_SAMPLERS`,
  `HIGHER_SHADOWCOLOR`, `ENTITY_TRANSLUCENT`, `FADE_VARIABLE`, `BLOCK_EMISSION_ATTRIBUTE`.
- Listed but still undefined: `CHUNK_OFFSET` and `TEXTURE_SHAPED_SAMPLERS`. The log says nothing about them;
  they are unknown or unsupported in that build. (measured)
- `iris.features.required` gets the same macros. A required flag the loader lacks refuses the whole pack.
  (public docs; the required case was not probed)
- Horizon defines a flag's macro only for the flags it has: today only `BLOCK_EMISSION_ATTRIBUTE`
  (`features::SUPPORTED_FLAGS`). A pack that requires any other flag is reported as Not supported.

**Option files** (measured)
- A folder pack's options are read from `shaderpacks/<folder>.txt`; a zip's from `shaderpacks/<file>.zip.txt`.
- A value option and a toggle both took effect in either case.
- Iris rewrites the file with a date comment and keeps the values.

## 16. Preprocessing details

- **A conditional left open at the end of the file closes there**: its lines are kept or dropped by its condition,
  and no error is raised. A pack in the corpus leaves an `#ifdef` open at the end of an included file and
  loads in Iris. (inferred from that pack; not probed)
- The conditional state runs through `#include`s: an `#if` opened in an included file can be closed, or left
  open, in the file that included it.
- **A condition that does not parse counts as false**, so its `#else` branch is taken. A pack in the corpus
  has an unbalanced parenthesis in a live `#if` and loads in Iris. (inferred; not probed)

**`program.<name>.enabled`** (key from the public docs; how expressions evaluate is Horizon's reading,
unverified against Iris)
- The value is `true`, `false`, or a boolean expression of option and macro names: `!`, `&&`, `||`,
  parentheses, `defined`, and comparisons of value options (`SHADOW_QUALITY > 1`).
- It evaluates like `#if`. An option that is on but has no value counts as 1, one that is off as 0, and an
  unknown name as 0.
- An expression that does not evaluate leaves the program enabled.
- `program.world0/composite3.enabled` applies to that dimension's program; a bare
  `program.composite3.enabled=false` also turns off the program of that name in a world folder.
- A disabled program is treated as missing, so its fallback (section 2) draws. Horizon's corpus counts a
  disabled program as neither found nor failed.

## 17. Custom uniforms

**Source:** the public format documentation. Horizon's implementation, `custom_uniforms.rs` in
`horizon-glsl-pack`, follows it; parts the docs leave open are marked.

**Declaring**
- `uniform.<type>.<name>=<expr>` is a uniform the programs can declare and read.
  `variable.<type>.<name>=<expr>` only feeds other expressions.
- Types: `float`, `int`, `bool`, `vec2`, `vec3`, `vec4`.
- Values are computed on the CPU once per frame, in file order.
- A name defined twice keeps its last definition, evaluated at that line's position. (unverified)

**Expressions**
- Literals: numbers, `true` / `false`, `pi`, and the `BIOME_*`, `CAT_*` and `PPT_*` constants.
- Names: other custom values, and standard uniforms one element at a time: `cameraPosition.y`, `skyColor.r`,
  and `gbufferModelView.3.0` for GLSL's `m[3][0]` (column, then row).
- Entity-state booleans (`is_sneaking`, `is_in_water`, `is_hurt`…) are standard values too. Horizon reads them
  as 0 until the client supplies them.
- Operators: `+ - * / %`, `! && ||`, comparisons.
- `/` divides as floats, even between two ints: `worldTime / 24000` is 0.25 at day time 6000, not 0. (From
  the docs, which list only floating-point constants and give `worldTime` and the other scalar uniforms as
  float parameters. Iris agrees black-box: it draws BSL Classic, whose sun position is `worldTime / 24000`,
  at noon at day time 6000; integer division would put it at sunrise. Iris's own expression types are not
  measured here.) Of the corpus, only BSL Classic changes value.

**Functions**
- `sin cos tan asin acos atan atan2 exp exp2 exp10 log log2 log10 sqrt pow torad todeg abs signum floor ceil
  round frac fmod min max clamp mix edge between equals in if ifb random randomInt vec2 vec3 vec4 smooth print`.
- Angles are in radians. `fmod` is the floored modulus, while `%` is Java's remainder (it keeps the sign of
  the left side). `round` rounds half up.
- `log(b, x)` is the logarithm of x in base b. `equals(a, b, eps)` compares within eps.
- `in(x, a, b, …)` tests x against each of the values. `if(c1, v1, c2, v2, …, else)` takes the first branch
  whose condition holds.
- `random()` is in [0, 1) and new every frame; `random(lo, hi)` and `randomInt(lo, hi)` scale it.
- `smooth([id], value, [up, [down]])` follows `value` exponentially, with half-lives in seconds for rising and
  falling. Horizon's defaults: up 1 s, down = up; it starts at the first frame's value; state is kept per call
  (the id is accepted and not needed). (defaults and the first frame unverified)

**Errors**
- A line that does not parse, or whose value does not fit its type, reads 0.
- The pack never fails because of a custom uniform; Horizon lists such lines in its report.

