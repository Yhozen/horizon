//! OptiFine-format shader packs (`shaders/*.vsh|*.fsh|*.properties`) translated
//! into WGSL that Horizon's wgpu renderer runs, natively and in the browser.
//!
//! Written from the pack format and the packs' own sources (clean room: no
//! code from Iris or from the shader-pack code Horizon removed in 4e7c25bb).
//!
//! The pipeline per program: [`preprocess`] (`#include`, `#define`, `#if`,
//! option overrides) → [`rewrite`] (GLSL 120 compatibility profile → plain
//! GLSL 450 globals, uniform and sampler uses recorded) → [`host`] (a
//! generated GLSL 450 `main` per stage that feeds Horizon's vertex data in and
//! the pack's results out) → [`naga_out`] (naga: GLSL → validated module →
//! WGSL text). Nothing here touches wgpu or the file system.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub mod host;
pub mod naga_out;
pub mod preprocess;
pub mod properties;
pub mod rewrite;
pub mod uniforms;

pub use naga_out::Stage;
pub use properties::BlockIds;
pub use uniforms::PackUniforms;

/// A pack's files, keyed by their path inside the pack with `/` separators
/// and no leading slash, e.g. `shaders/world0/gbuffers_terrain.fsh`.
#[derive(Clone, Debug, Default)]
pub struct PackFiles(pub BTreeMap<String, Vec<u8>>);

impl PackFiles {
    /// The file at `path` (pack-relative, `/` separators) as text.
    pub fn text(&self, path: &str) -> Option<String> {
        let path = path.trim_start_matches('/');
        self.0.get(path).map(|b| String::from_utf8_lossy(b).into_owned())
    }

    /// Whether the pack has a file at `path`.
    pub fn contains(&self, path: &str) -> bool {
        self.0.contains_key(path.trim_start_matches('/'))
    }
}

/// How the pack is configured.
#[derive(Clone, Debug)]
pub struct PackOptions {
    /// OptiFine's `MC_VERSION` macro (1.21.11 = 12111).
    pub mc_version: u32,
    /// Option values: `(name, Some(value))` replaces the value of every
    /// `#define name …` line, `(name, None)` drops `#define name` lines (turns
    /// a boolean option off).
    pub overrides: Vec<(String, Option<String>)>,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self { mc_version: 12111, overrides: Vec::new() }
    }
}

/// How Horizon's terrain bind group lays out block textures (gfx.rs's
/// `SPRITE_ARRAYS`, `SPRITE_SPLIT` and `SPRITE_GRID` pipeline constants).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostParams {
    pub sprite_arrays: u32,
    pub sprite_split: u32,
    pub sprite_grid: u32,
}

impl Default for HostParams {
    fn default() -> Self {
        Self { sprite_arrays: 1, sprite_split: 2048, sprite_grid: 1 }
    }
}

/// The programs this proof of concept runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProgramKind {
    /// Solid and cutout terrain (`gbuffers_terrain`), into colortex0/6/7.
    Terrain,
    /// Translucent terrain (`gbuffers_water`), into colortex0/6/7.
    Water,
    /// The full-screen `final` pass: reads colortex0/6/7 and depthtex0.
    Final,
    /// The shadow map pass (`shadow`).
    Shadow,
}

impl ProgramKind {
    /// The program file names to try, best first (OptiFine's fallbacks).
    pub fn candidates(self) -> &'static [&'static str] {
        match self {
            Self::Terrain => &["gbuffers_terrain", "gbuffers_textured_lit", "gbuffers_textured", "gbuffers_basic"],
            Self::Water => &[
                "gbuffers_water",
                "gbuffers_terrain",
                "gbuffers_textured_lit",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::Final => &["final"],
            Self::Shadow => &["shadow"],
        }
    }

    /// The geometry the program draws.
    pub fn geometry(self) -> host::Geometry {
        match self {
            Self::Terrain | Self::Water | Self::Shadow => host::Geometry::Terrain,
            Self::Final => host::Geometry::Fullscreen,
        }
    }
}

/// One program as WGSL: one module per stage, entry point `main` in each.
#[derive(Clone, Debug)]
pub struct CompiledProgram {
    /// The pack file the program came from, e.g. `shaders/world0/gbuffers_terrain`.
    pub source: String,
    pub vertex_wgsl: String,
    pub fragment_wgsl: String,
    /// The colortex indices the fragment outputs go to, in output-location
    /// order (`/* DRAWBUFFERS:067 */` → `[0, 6, 7]`; `[0]` when absent).
    pub draw_buffers: Vec<u8>,
    /// Pack samplers the program reads (`gtexture`, `lightmap`, `colortex0`…).
    pub samplers: BTreeSet<String>,
}

/// A compiled pack.
#[derive(Clone, Debug)]
pub struct CompiledPack {
    pub programs: BTreeMap<ProgramKind, CompiledProgram>,
    /// `block.properties`: block names to `mc_Entity.x` ids.
    pub blocks: BlockIds,
}

/// What went wrong, with the pack file it happened in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackError {
    pub file: String,
    pub message: String,
}

impl PackError {
    pub fn new(file: impl Into<String>, message: impl Into<String>) -> Self {
        Self { file: file.into(), message: message.into() }
    }
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.file, self.message)
    }
}

impl std::error::Error for PackError {}

/// The `mc_Entity` and normal of a terrain vertex, carried in the alpha byte
/// of `TerrainVertex::color` while a pack is active: `normal << 5 | material`.
/// `material` indexes [`BlockIds`] (31 = no id); `normal` is a [`NormalCode`].
pub fn vertex_tag(normal: NormalCode, material: u8) -> u8 {
    ((normal as u8) << 5) | (material & 31)
}

/// The face normal a terrain vertex carries in its tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum NormalCode {
    Down = 0,
    Up = 1,
    North = 2,
    South = 3,
    West = 4,
    East = 5,
    /// Not axis-aligned (cross plants, sloped fluid): decoded as up.
    Other = 6,
    /// No tag (0xFF): decoded as up, material none.
    None = 7,
}

/// The material index meaning "no `block.properties` id".
pub const NO_MATERIAL: u8 = 31;

/// The world folder a dimension's programs come from, before `shaders/`.
pub fn world_folder(files: &PackFiles, dimension: &str) -> String {
    properties::dimension_folder(files, dimension)
}

/// Compiles the programs of `kinds` for the dimension `dimension`
/// (`minecraft:overworld`, …). Every program in `kinds` must compile.
pub fn compile(
    files: &PackFiles,
    dimension: &str,
    kinds: &[ProgramKind],
    options: &PackOptions,
    host: &HostParams,
) -> Result<CompiledPack, PackError> {
    let predefined = preprocess::predefined(options.mc_version);
    let blocks = properties::block_ids(files, &predefined)?;
    let folder = world_folder(files, dimension);
    let mut programs = BTreeMap::new();
    for &kind in kinds {
        let program = find_program(files, &folder, kind)
            .ok_or_else(|| PackError::new(format!("shaders/{folder}"), format!("no program for {kind:?}")))?;
        programs.insert(kind, compile_program(files, &program, kind, options, host, &blocks, &predefined)?);
    }
    Ok(CompiledPack { programs, blocks })
}

/// The pack path (without extension) of the program `kind` runs, looking in
/// the world folder first and `shaders/` second, best candidate first.
pub fn find_program(files: &PackFiles, folder: &str, kind: ProgramKind) -> Option<String> {
    for name in kind.candidates() {
        for dir in [format!("shaders/{folder}/"), "shaders/".to_string()] {
            let path = format!("{dir}{name}");
            if files.contains(&format!("{path}.vsh")) && files.contains(&format!("{path}.fsh")) {
                return Some(path);
            }
        }
    }
    None
}

fn compile_program(
    files: &PackFiles,
    program: &str,
    kind: ProgramKind,
    options: &PackOptions,
    host: &HostParams,
    blocks: &BlockIds,
    predefined: &BTreeMap<String, String>,
) -> Result<CompiledProgram, PackError> {
    let vsh_path = format!("{program}.vsh");
    let fsh_path = format!("{program}.fsh");
    let vsh = preprocess::preprocess(files, &vsh_path, predefined, &options.overrides)?;
    let fsh = preprocess::preprocess(files, &fsh_path, predefined, &options.overrides)?;
    // Before preprocessing removes comments: DRAWBUFFERS lives in one.
    let raw_fsh = preprocess::expand_includes(files, &fsh_path)?;
    let draw_buffers = properties::draw_buffers(&raw_fsh).unwrap_or_else(|| vec![0]);
    let vertex = rewrite::rewrite(&vsh, Stage::Vertex).map_err(|m| PackError::new(&vsh_path, m))?;
    let fragment = rewrite::rewrite(&fsh, Stage::Fragment).map_err(|m| PackError::new(&fsh_path, m))?;
    let spec = host::ProgramSpec { kind, geometry: kind.geometry(), draw_buffers: &draw_buffers, host, blocks };
    let (vs_glsl, fs_glsl) = host::sources(&vertex, &fragment, &spec).map_err(|m| PackError::new(program, m))?;
    let vertex_wgsl =
        naga_out::glsl_to_wgsl(&vs_glsl, Stage::Vertex).map_err(|m| PackError::new(&vsh_path, m))?;
    let fragment_wgsl =
        naga_out::glsl_to_wgsl(&fs_glsl, Stage::Fragment).map_err(|m| PackError::new(&fsh_path, m))?;
    let samplers = vertex.samplers.union(&fragment.samplers).cloned().collect();
    Ok(CompiledProgram { source: program.to_string(), vertex_wgsl, fragment_wgsl, draw_buffers, samplers })
}
