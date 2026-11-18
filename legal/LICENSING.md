# Licensing

Horizon's code is MIT. A few folders hold work by other people under their own terms, and the
Horizon Open bundle, which adapts share-alike artwork, is distributed under CC BY-SA 4.0. This page
maps which terms apply where.

This page is a map, not legal advice. Where it and a licence text disagree, the licence text wins.

## How it works

- The root [LICENSE](../LICENSE) (MIT) covers everything in the repository unless a folder or file
  is listed below with its own licence.
- A folder with its own licence file, or a file named in one of the tables below, keeps those terms.
  Third-party code adapted into Horizon's own files is credited in
  [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md).
- Dependencies fetched by Cargo or pnpm keep their own licences and are not part of this repository.
- Mojang's files are never in the repository: no game jars, assets, decompiled source or prepared
  resources. They stay in each player's `.local/`, and `tools/check_public_tree.py` rejects them
  on commit. Lists of Minecraft's identifiers (names) are committed as data.
- This fork is MIT. Upstream Horizon moved to LGPL-3.0-or-later; nothing from upstream after that
  change is taken ([UPSTREAM.md](../UPSTREAM.md#licence-boundary-imports-stopped)).

## Licence files in the repository

| Path | SPDX | Covers |
| --- | --- | --- |
| [`LICENSE`](../LICENSE) | `MIT` | Everything not listed below: the game, the server, the launcher, the tools, the docs, the website code, and the Horizon Open generator (`crates/horizon-open-content`) |
| [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md) | (notices) | Credits and licence texts for third-party work inside Horizon's files: CullLeaves (MIT), wgpu-hal, Pixelify Sans, Pixelarticons |
| [`third_party/wgpu-hal/LICENSE.MIT`](../third_party/wgpu-hal/LICENSE.MIT), [`LICENSE.APACHE`](../third_party/wgpu-hal/LICENSE.APACHE) | `MIT OR Apache-2.0` | `third_party/wgpu-hal/` (wgpu-hal 30.0.1 by the gfx-rs developers, with one upstream fix) |
| [`crates/horizon-theme/fonts/OFL-Geist.txt`](../crates/horizon-theme/fonts/OFL-Geist.txt) | `OFL-1.1` | `crates/horizon-theme/fonts/Geist-*.ttf`, `GeistMono-Regular.ttf` |
| [`crates/horizon-theme/fonts/OFL-PixelifySans.txt`](../crates/horizon-theme/fonts/OFL-PixelifySans.txt) | `OFL-1.1` | `crates/horizon-theme/fonts/PixelifySans-*.ttf` |
| [`crates/horizon-theme/icons/pixel/LICENSE-pixelarticons.txt`](../crates/horizon-theme/icons/pixel/LICENSE-pixelarticons.txt) | `MIT` | `crates/horizon-theme/icons/pixel/*.svg` (Pixelarticons by Gerrit Halfmann) |
| [`website/public/fonts/OFL-Geist.txt`](../website/public/fonts/OFL-Geist.txt), [`OFL-PixelifySans.txt`](../website/public/fonts/OFL-PixelifySans.txt) | `OFL-1.1` | `website/public/fonts/*.woff2` (the same fonts converted to WOFF2) |
| [`website/LEGAL.md`](../website/LEGAL.md) | (map) | Every file the website serves and its terms |
| [`content/open/LICENSE.md`](../content/open/LICENSE.md) | (per file) | `content/open/`: each media file under the licence recorded for it in `sources.json` |
| [`content/open/licenses/CC-BY-SA-3.0.txt`](../content/open/licenses/CC-BY-SA-3.0.txt) | `CC-BY-SA-3.0` | The licence text, for the media that use it |
| [`content/open/licenses/CC-BY-SA-4.0.txt`](../content/open/licenses/CC-BY-SA-4.0.txt) | `CC-BY-SA-4.0` | The licence text, for the generated Horizon Open bundle (below) and for media that use it |
| [`content/open/licenses/CC0-1.0.txt`](../content/open/licenses/CC0-1.0.txt) | `CC0-1.0` | The dedication text, for the media that use it |
| `content/open/packs/pixel_perfection_legacy/pack.json` | `CC-BY-SA-4.0` | `content/open/packs/pixel_perfection_legacy/` (Pixel Perfection Legacy textures; the manifest records source, author, licence evidence and every file; `pack.txt` is the pack's own credits) |
| `content/open/notices/minetest_game-*-license.txt`, `*-README.md` | (upstream notices) | Minetest Game's own licence and credit files, kept unchanged for attribution. They give Minetest Game's code (LGPL-2.1 for `default`, MIT for `flowers` and `wool`) and media (CC BY-SA 3.0) licences; only media is taken from it |

The Cargo manifests say `license = "MIT"` (or `license.workspace = true`) for every crate except
`third_party/wgpu-hal`, which says `MIT OR Apache-2.0`.

## Horizon Open content

Horizon Open is the edition that runs with no Mojang files ([docs/open-creative.md](../docs/open-creative.md)).
Its licensing has three parts.

### The inputs: `content/open/`

The media in `content/open/` (121 textures from Minetest Game under `media/`, and a vendored
texture pack under `packs/`) keep their own licences, file by file.
[`content/open/sources.json`](../content/open/sources.json) records for each Minetest Game file its
origin (repository, revision, path), author, licence id, SHA-256 and how it is transformed; a
pack's `pack.json` records the same for the pack and the SHA-256 of each of its files. The
licences in use:

| SPDX | Files today | Text |
| --- | --- | --- |
| `CC-BY-SA-3.0` | 119 | `content/open/licenses/CC-BY-SA-3.0.txt` |
| `CC0-1.0` | 2 (`default_gravel.png`, `default_glass.png`) | `content/open/licenses/CC0-1.0.txt` |
| `CC-BY-SA-4.0` | 3,056 textures of Pixel Perfection Legacy under `packs/` (not used by the bundle yet) | `content/open/licenses/CC-BY-SA-4.0.txt` |

Building the bundle fails if a media file is not listed in `sources.json`, if its checksum
changed, or if its licence text is not vendored (`crates/horizon-open-content/src/sources.rs`).

### The generated bundle

`horizon-prepare open` writes the Horizon Open resource folder, and `--archive` writes the browser's
data archive from it. That bundle combines and adapts share-alike artwork (composited textures,
renamed files) with Horizon's own generated art and data. It is distributed under
**CC BY-SA 4.0** as a whole ([text](../content/open/licenses/CC-BY-SA-4.0.txt)):

- CC BY-SA 3.0 allows an adaptation to be shared under "a later version of this License with the
  same License Elements" (section 4(b) of the 3.0 text), which CC BY-SA 4.0 is. CC0 files carry
  no conditions.
- A file taken unchanged from an input also stays available under that input's own licence, as
  recorded in the bundle's `horizon-open-provenance.json`.
- The fonts in the bundle stay under the SIL Open Font License 1.1, which they require; the OFL
  allows bundling them with other content.
- The bundle carries its attribution: `credits/CREDITS.md`, `credits/credits.json`, the licence
  texts and the upstream notices under `credits/`, and the in-game Credits screen.

### The generator stays MIT

The code that builds the bundle (`crates/horizon-open-content`, `crates/horizon-prepare`, the web
packaging) is MIT, like the rest of the repository. Share-alike does not reach it: the generator
reads the media files as data (decodes, composites and re-encodes the pixels) and writes the
bundle; no code in the repository is derived from the artwork or from the projects it comes from.
The art and data the generator draws itself (`art.rs`, `art_survival.rs`, the block and item
tables) are Horizon's original work, MIT in the repository and part of the CC BY-SA 4.0 bundle
once generated.
