# Shader pack compatibility

Every shader pack Horizon's translator has been measured on, and what came out. Experimental support:
[shader-packs.md](shader-packs.md). The format facts behind it: [shader-pack-format-notes.md](shader-pack-format-notes.md).

Packs are downloaded from Modrinth for testing only, by `tools/shader_pack_corpus.py fetch`, and are never
committed. This page lists names, links and results; it holds no pack code.

## What the columns mean

- **Version**: the version tested: the newest Modrinth version with Iris among its loaders when the pack was
  fetched (hold-out batches: 2026-11-19).
- **16 / 28 varyings**: the translation status (`tools/shader_pack_corpus.py check`) with a device that allows
  16 inter-stage varying locations (WebGPU's default) and with one that allows 28 (common desktop GPUs).
  Every program slot is translated on its own, validated by naga, written to SPIR-V and compiled by Chrome's
  WGSL compiler, in the overworld, the Nether and the End. One status means all three dimensions agree;
  otherwise each is listed (O, N, E).
  - **Works-candidate**: every program the pack enables compiled. Not yet a claim that it looks right.
  - **Partial**: an optional program failed; the pack runs without it.
  - **Fails**: an essential program (terrain, water, a pass the picture depends on) failed.
  - **Refused**: the pack requires an Iris feature Horizon does not have (the game shows Not supported).
  - **Not a pack**: the zip holds no shader programs.
- **Rendered natively**: the game drew the pack offscreen in the Iris oracle's test world, at noon
  (`tools/shader_pack_render.py`). Draws, Black, Suspect (large black areas), Looks vanilla (the pack did not
  load) or Timeout. "Iris diff" is the mean per-cell colour difference from Iris's own picture of the same
  pose (0 = identical); lower is closer. Blank: not rendered yet.

Builds, all 2026-11-19: the 16-varying columns are `gabriel/packs-corpus` at 2a28d2a4 (after the batch 2
fixes). The 28-varying columns of the train set and batch 1 are one integration merge later (6ce23c03),
which adds a fix to sampler declarations (e737596f); batch 2's 28 column is 2a28d2a4. The render column of
the train set is from 2026-11-21, `gabriel/packs-sampling` at 9d2b61eb (lavapipe, noon pose); the hold-out
batches' render column is from an earlier build of 2026-11-19.

**Instrument fix (77fc9958).** Until then `pack_check` counted 9 of Horizon's own textures against each terrain
program's 16. That was the old layout of 8 sprite arrays and the lightmap. The game binds the sprite
arrays in use plus the lightmap: 2 when the sprites fit in one array. Only programs that had failed on that
count can change. Those packs (Photon, MakeUp, Rethinking Voxels, Arc) were measured again at both limits
with the fix, on the same day's integration build (f479a1b7, reports `fix-tt-16`, `fix-tt-28`); their rows
show the new results. Rethinking Voxels stays Refused, and Arc's water passes, but its composite pass
still fails on its own textures.

**Re-measured 2026-11-21** (every pack, both limits, all dimensions, Chrome): `gabriel/packs-corpus` at
20953199 (`gabriel/packs-int` 35ed995a, which has the tighter varying packing of 5ce30a71, plus the Bloop fix
70fd317c). The 16 and 28 columns below are from this build; the render column is not. Reports: `c2-*-16`,
`c2-*-28` (35ed995a alone: `c1-*`). What changed, and why:
- MakeUp (O/E) and E-LITE (O/N/E) at 16: Fails → Works-candidate. Their water needs fewer locations once
  unread components stay home and pieces straddle slots: MakeUp 17 → 15, E-LITE 19 → 16.
- Bliss (O) at 16: Partial → Works-candidate, from the sampler fix e737596f (already in the 28 column).
- Bloop (O): Partial → Works-candidate. Its shadow vertex stage writes a varying it never declares; Iris
  patches the declaration in, and so does the translator now (70fd317c).
- Photon (O) stays Partial: its optional deferred pass went from 18 to 14 locations, and now stops at the next
  limit, 17 textures.
- Totals over all dimensions, at both 16 and 28: train 34 / 5 / 3 / 0, batch 1 24 / 3 / 6 / 0 (unchanged),
  batch 2 28 / 0 / 15 / 2 / 3.
- Arc compiles every program, Chrome included, with `--max-textures 32` (`c2-arc-t32`). The game asks the
  adapter for its own texture limit, so a desktop GPU that allows more than 16 could run it (not rendered yet).
- No status got worse. `final` left the essential lists (316126b6: final is not essential), which changed
  no status.

## Packs

### Train set

| rank | pack | version | 16 varyings | 28 varyings | rendered natively | notes |
|---|---|---|---|---|---|---|
| 1 | [Complementary Shaders - Reimagined](https://modrinth.com/shader/complementary-reimagined) | r5.10 | Works-candidate | Works-candidate | Draws, Iris diff 8 | optional compute and storage buffers skipped |
| 3 | [BSL Shaders](https://modrinth.com/shader/bsl-shaders) | 10.1.8 | Works-candidate | Works-candidate | Draws, Iris diff 10 | optional compute pass skipped |
| 4 | [Photon Shaders](https://modrinth.com/shader/photon-shader) | v1.4 | Partial (O/E), Works-candidate (N) | Partial (O/E), Works-candidate (N) | Draws, Iris diff 116 | an optional deferred pass fails (overworld: 17 textures; End: a sky function) |
| 5 | [Solas Shader](https://modrinth.com/shader/solas-shader) | 3.9 | Partial | Partial | Draws, Iris diff 16 | shadow programs use image writes |
| 6 | [Bliss Shaders](https://modrinth.com/shader/bliss-shader) | 2.1.2 | Works-candidate | Works-candidate | Draws, Iris diff 18 |  |
| 7 | [MakeUp - Ultra Fast](https://modrinth.com/shader/makeup-ultra-fast-shaders) | 9.6 | Works-candidate | Works-candidate | Draws, Iris diff 39 | water packs into 15 varying locations |
| 8 | [Rethinking Voxels](https://modrinth.com/shader/rethinking-voxels) | r0.1-beta9 | Refused | Refused | Looks vanilla | requires compute shaders, custom images, storage buffers |
| 9 | [Super Duper Vanilla](https://modrinth.com/shader/super-duper-vanilla) | 1.3.8 | Works-candidate | Works-candidate | Draws, Iris diff 11 |  |
| 10 | [Mellow](https://modrinth.com/shader/mellow) | 3.5 | Works-candidate | Works-candidate | Draws, Iris diff 7 | optional compute passes skipped |
| 11 | [Insanity Shader](https://modrinth.com/shader/insanity-shader) | 1.650 | Works-candidate | Works-candidate | Draws, Iris diff 30 |  |
| 12 | [AstraLex Shaders](https://modrinth.com/shader/astralex) | 93.0 | Works-candidate | Works-candidate | Draws, Iris diff 17 |  |
| 13 | [Miniature Shader](https://modrinth.com/shader/miniature-shader) | 2.19 | Works-candidate | Works-candidate | Draws, Iris diff 9 |  |
| 43 | [Sildur's Vibrant Shaders](https://modrinth.com/shader/sildurs-vibrant-shaders) | 2.02 | Works-candidate | Works-candidate | Draws, Iris diff 17 |  |
| – | SEUS Renewed (not from Modrinth) | not recorded | Works-candidate | Works-candidate | Draws, Iris diff 40 | a folder pack; drew in 539 s on 11-21 (timed out on 11-19) |

### Hold-out batch 1 (Modrinth ranks 14–24)

| rank | pack | version | 16 varyings | 28 varyings | rendered natively | notes |
|---|---|---|---|---|---|---|
| 14 | [Pastel Shaders](https://modrinth.com/shader/pastel-shaders) | 1.200 | Works-candidate | Works-candidate |  |  |
| 15 | [Nostalgia Shader](https://modrinth.com/shader/nostalgia-shader) | 5.1 | Works-candidate | Works-candidate |  |  |
| 16 | [Hysteria Shaders](https://modrinth.com/shader/hysteria-shaders) | 1.2.1 | Partial (O/E), Works-candidate (N) | Partial (O/E), Works-candidate (N) |  | one optional pass fails per dimension |
| 17 | [VanillAA](https://modrinth.com/shader/vanillaa) | 1.3 | Works-candidate | Works-candidate |  |  |
| 18 | [Kappa Shader](https://modrinth.com/shader/kappa-shader) | 5.3 | Partial (O), Works-candidate (N/E) | Partial (O), Works-candidate (N/E) |  | one optional prepare pass and one deferred pass fail |
| 19 | [Potato Shaders](https://modrinth.com/shader/potato-shaders) | 1.0d | Works-candidate | Works-candidate |  |  |
| 20 | [Spooklementary](https://modrinth.com/shader/spooklementary) | v2.0.4 | Works-candidate | Works-candidate |  |  |
| 21 | [I Like Vanilla](https://modrinth.com/shader/i-like-vanilla) | v1.5.0b | Works-candidate | Works-candidate |  |  |
| 22 | [Vanilla Plus Shader](https://modrinth.com/shader/vanilla-plus-shader) | 3.3 | Works-candidate | Works-candidate |  |  |
| 23 | [Visual Vibrance](https://modrinth.com/shader/visual-vibrance) | 0.3.5a | Refused | Refused |  | requires custom images, storage buffers, separate shadow samplers |
| 24 | [Noble Shaders](https://modrinth.com/shader/noble) | 1.9.9 | Refused | Refused |  | requires compute shaders and storage buffers |

### Hold-out batch 2 (Modrinth ranks 25–40)

| rank | pack | version | 16 varyings | 28 varyings | rendered natively | notes |
|---|---|---|---|---|---|---|
| 25 | [BSL Shaders - Classic](https://modrinth.com/shader/bsl-shaders-classic) | 10.0 | Works-candidate | Works-candidate |  |  |
| 26 | [Shrimple](https://modrinth.com/shader/shrimple) | v0.12 | Refused | Refused |  | requires compute shaders and custom images |
| 27 | [Bloop Shaders](https://modrinth.com/shader/bloop-shaders) | 1.8.0-Alpha-3 | Works-candidate | Works-candidate |  | its shadow program writes an undeclared varying, declared as Iris does |
| 28 | [FastPBR](https://modrinth.com/shader/fastpbr) | 2.0 | Refused | Refused |  | requires compute shaders, custom images, storage buffers |
| 29 | [E-LITE shaders (MakeUp edit)](https://modrinth.com/shader/lite-shaders) | ELT513 | Works-candidate | Works-candidate |  | water packs into exactly 16 varying locations |
| 30 | [Fantasy Shaders Reimagined](https://modrinth.com/shader/fantasy-shaders) | 1.11 | Works-candidate | Works-candidate |  |  |
| 31 | [Body Camera](https://modrinth.com/shader/body-camera-shader) | 1.6.2 | Works-candidate | Works-candidate |  |  |
| 32 | [RenderPearl](https://modrinth.com/shader/renderpearl) | 2.8.0-beta.4 | Refused | Refused |  | requires compute shaders, storage buffers and 5 more features |
| 33 | [Clarity](https://modrinth.com/shader/clarityshader) | 1.1.3 | Refused | Refused |  | requires compute shaders and custom images |
| 34 | [Lux V1](https://modrinth.com/shader/lux-v1) | 1.3 | Works-candidate | Works-candidate |  |  |
| 35 | [Fantasy Shaders Unbound](https://modrinth.com/shader/fantasy-shader-unbound) | 1.11 | Works-candidate | Works-candidate |  |  |
| 36 | [RedHat Shaders](https://modrinth.com/shader/redhat-shaders) | 34.3.1 | Works-candidate | Works-candidate |  |  |
| 37 | [No Fog](https://modrinth.com/shader/nofog) | 2.1.1 | Not a pack | Not a pack |  | a resource pack, not a shader pack |
| 38 | [Arc](https://modrinth.com/shader/arc-shader) | 0.15.2 | Fails (O/E), Works-candidate (N) | Fails (O/E), Works-candidate (N) |  | an essential composite pass reads 19 of its own textures (16 allowed); passes with a device allowing more (`--max-textures 32`) |
| 39 | [Pegasus Shaders](https://modrinth.com/shader/pegasus) | 0.8.1 | Refused | Refused |  | requires custom images |
| 40 | [Aurora's Shaders](https://modrinth.com/shader/auroras-shaders) | 1.9 | Works-candidate | Works-candidate |  |  |

## How the hold-out batches are measured

The train set is the packs the translator was developed against. Hold-out batches measure how it does on
packs nobody has looked at.

1. **Choose by rank, not by hand**: the next Modrinth shader projects by downloads, skipping slugs already
   measured. Fetch them into their own folder (`holdout-<n>/`, `batch: <n>` in its `index.json`).
2. **Merge the integration branch first** and note its commit, so the first try measures current code.
3. **Measure before opening any pack's source**: all three dimensions, Chrome on, at 16 and at 28 varyings.
   Write these first-try numbers down before changing anything.
4. **Then fix the errors** in the translator, never in the packs, each fix with a test of the construct
   (written from scratch, not taken from the pack).
5. **Measure again** with the same command, and rerun the train set and the earlier batches to catch
   regressions.

The first try is the honest number: it is what a new pack gets today. The after-fix number shows how much
of the gap was the translator's.

### Results

Overworld statuses; "renders" counts Works-candidate and Partial.

| Batch | packs | measurement | Works-candidate | Partial | Refused | Fails | Not a pack | renders |
|---|---|---|---|---|---|---|---|---|
| 1 (ranks 14–24) | 11 | first try, 16 and 28 varyings | 2 | 1 | 2 | 6 | – | 3 |
| | | after fixes, 16 and 28 varyings | 8 | 1 | 2 | 0 | – | 9 |
| 2 (ranks 25–40) | 16 | first try, 16 varyings | 5 | 0 | 5 | 6 | – | 5 |
| | | first try, 28 varyings | 6 | 0 | 5 | 5 | – | 6 |
| | | after fixes, 16 varyings | 7 | 1 | 5 | 2 | 1 | 8 |
| | | after fixes, 28 varyings | 8 | 1 | 5 | 1 | 1 | 9 |
| | | re-measured 2026-11-21, 16 and 28 varyings | 9 | 0 | 5 | 1 | 1 | 9 |

- **Batch 1** (rank 13 was already in the train set): 16 and 28 varyings gave the same statuses throughout.
  Over all three dimensions: 7 / 3 / 6 / 17 on the first try, 24 / 3 / 6 / 0 after.
  - After the fixes, every pack that requires no Iris-only feature renders (9 of 9). The two refused packs
    require storage buffers or compute shaders.
- **Batch 2**: no rank was already measured. The first try counted the resource pack as Fails; the Not a pack
  status was added afterwards. Over all three dimensions: 16 / 0 / 15 / 17 on the first try, 24 / 1 / 15 / 5 / 3
  after (27 / 1 / 15 / 2 / 3 at 28 varyings); 28 / 0 / 15 / 2 / 3 at both limits when re-measured on 2026-11-21.
  - What is left is a device limit: Arc reads more than 16 textures in a pass (overworld and End).
- The fixes in both batches were translator gaps: GLSL forms naga's front end does not accept, sampler
  overloads, preprocessor leniency, built-ins without a host form.
- No fix in either batch changed a train-set status. Photon's water failure in that period came from the
  instrument, which counted 9 of Horizon's textures where the game binds 2; see the instrument fix above.
