//! The crate's shared types: a pack's files and options, the host's limits,
//! compiled programs and the errors they carry.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::ProgramKind;
use crate::plan::{self, PackPlan};
use crate::properties::BlockIds;

/// A pack's files, keyed by their path inside the pack with `/` separators
/// and no leading slash, e.g. `shaders/world0/gbuffers_terrain.fsh`.
#[derive(Clone, Debug, Default)]
pub struct PackFiles(pub BTreeMap<String, Vec<u8>>);

impl PackFiles {
    /// The file at `path` (pack-relative, `/` separators) as text.
    pub fn text(&self, path: &str) -> Option<String> {
        let path = path.trim_start_matches('/');
        self.0
            .get(path)
            .map(|b| String::from_utf8_lossy(b).into_owned())
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
    /// Option values, written into the sources before preprocessing (see
    /// [`options::apply`]): `(name, None)` or `Some("false")` turns a boolean
    /// `#define name` off (comments it out), `Some("true")` on, any other
    /// value replaces the value of a `#define name v` line or of a `const`
    /// option (`shadowMapResolution`, …).
    pub overrides: Vec<(String, Option<String>)>,
    /// Program files (pack paths without extension, e.g.
    /// `shaders/world0/composite3`) to treat as missing: ones the GPU's own
    /// compiler rejected, so their slot falls back to its next candidate.
    pub excluded: BTreeSet<String>,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self {
            mc_version: 12111,
            overrides: Vec::new(),
            excluded: BTreeSet::new(),
        }
    }
}

/// How Horizon's terrain bind group lays out block textures (gfx.rs's
/// `SPRITE_ARRAYS`, `SPRITE_SPLIT` and `SPRITE_GRID` pipeline constants).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostParams {
    pub sprite_arrays: u32,
    pub sprite_split: u32,
    pub sprite_grid: u32,
    /// Inter-stage locations the device allows (`maxInterStageShaderVariables`,
    /// at least 16 in WebGPU), counting Horizon's own (terrain's flat
    /// `hz_sprite`). Programs with more varyings pack them into `vec4`s.
    pub max_varyings: u32,
    /// Sampled textures one stage may bind (`maxSampledTexturesPerShaderStage`,
    /// 16 in WebGPU), Horizon's own included.
    pub max_textures: u32,
    /// Sampled textures Horizon's terrain group 0 layout declares for both
    /// stages, used or not (gfx.rs: 8 sprite arrays and the lightmap); they
    /// count against `max_textures` in terrain programs.
    pub terrain_textures: u32,
    /// Colour attachments one pass may write (`maxColorAttachments`, 8).
    pub max_draw_buffers: u32,
    /// Bytes per pixel of all colour attachments together
    /// (`maxColorAttachmentBytesPerSample`: 32 by default, 128 on most adapters).
    pub max_attachment_bytes: u32,
}

impl Default for HostParams {
    fn default() -> Self {
        Self {
            sprite_arrays: 1,
            sprite_split: 2048,
            sprite_grid: 1,
            max_varyings: 16,
            max_textures: 16,
            terrain_textures: 9,
            max_draw_buffers: 8,
            max_attachment_bytes: 128,
        }
    }
}

/// What a pack texture binding holds; the client binds it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextureSource {
    /// `colortexN`: the side the schedule says this pass reads.
    Buffer(u8),
    /// `depthtex0..2`.
    Depth(u8),
    /// `shadowtex0..1`.
    ShadowTex(u8),
    /// `shadowtex1` as an `R32Float` copy with mipmaps, for `texture2DLod` on it.
    ShadowTexMips(u8),
    ShadowColor(u8),
    Noise,
    /// `normals` and `specular` (1×1 defaults).
    Normals,
    Specular,
    /// A [`plan::CustomTexture`], by its index in `PackPlan::custom_textures`.
    Custom(usize),
}

/// How a pack texture is sampled; the client creates one sampler per kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SamplerKind {
    /// Nearest, clamp (depth, integer formats).
    Point,
    /// Linear, clamp (colour targets, shadow maps).
    Linear,
    /// Linear with linear mip filtering, clamp (mipmapped targets).
    Trilinear,
    /// Linear, repeat (noise).
    LinearRepeat,
    /// Nearest, repeat (PNG textures).
    PointRepeat,
    /// Depth comparison (`GreaterEqual` on Horizon's reversed depth), linear.
    Compare,
}

impl SamplerKind {
    pub const ALL: [SamplerKind; 6] = [
        Self::Point,
        Self::Linear,
        Self::Trilinear,
        Self::LinearRepeat,
        Self::PointRepeat,
        Self::Compare,
    ];

    /// Group 1 binding of the sampler: 40 + its index in [`Self::ALL`].
    pub fn binding(self) -> u32 {
        40 + Self::ALL.iter().position(|&k| k == self).unwrap() as u32
    }
}

/// A texture's dimension.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TexDim {
    #[default]
    D2,
    D3,
}

/// What a texture binding's texels are, which decides its WGSL type:
/// `texture_2d<f32>`, `texture_depth_2d`, `texture_2d<i32>`, `texture_2d<u32>`
/// (and the 3D forms).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SampleKind {
    /// Float, filterable unless its sampler is [`SamplerKind::Point`] or
    /// [`SamplerKind::PointRepeat`] (depth textures read as unfilterable float).
    #[default]
    Float,
    /// A depth texture read with a depth compare (`texture_depth_2d`).
    Depth,
    /// Signed integer texels, read with `textureLoad` only.
    Sint,
    /// Unsigned integer texels, read with `textureLoad` only.
    Uint,
}

/// One texture a program reads from group 1, at `binding` (see
/// [`host::sampler_binding`]; custom textures at 64 + their index in
/// `PackPlan::custom_textures`); the WGSL declares it by `dimension` and
/// `sample` (`texture_2d<f32>`, `texture_depth_2d` when `sampler` is
/// [`SamplerKind::Compare`], `texture_3d<f32>`, `texture_2d<u32>`…).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureBinding {
    pub binding: u32,
    pub source: TextureSource,
    pub sampler: SamplerKind,
    pub dimension: TexDim,
    pub sample: SampleKind,
    /// The stages that read it.
    pub vertex: bool,
    pub fragment: bool,
}

/// One program as WGSL: one module per stage, entry point `main` in each.
///
/// Bind groups: group 0 is Horizon's own (terrain: the terrain group;
/// entity geometry: the entity group; full-screen: empty); group 1 is the
/// pack's: binding 0 the uniform block, textures per `textures`, samplers at
/// [`SamplerKind::binding`].
#[derive(Clone, Debug)]
pub struct CompiledProgram {
    /// The pack file the program came from, e.g. `shaders/world0/gbuffers_terrain`.
    pub source: String,
    pub vertex_wgsl: String,
    pub fragment_wgsl: String,
    /// The colortex indices the fragment outputs go to, in output-location
    /// order (`/* DRAWBUFFERS:067 */` → `[0, 6, 7]`; `[0]` when absent).
    /// For shadow programs, shadowcolor indices.
    pub draw_buffers: Vec<u8>,
    /// Pack samplers the program reads (`gtexture`, `lightmap`, `colortex0`…).
    pub samplers: BTreeSet<String>,
    /// Group 1 textures, by binding.
    pub textures: Vec<TextureBinding>,
    /// Uniforms the pack declares that Horizon does not provide (read as zero).
    pub unknown_uniforms: BTreeSet<String>,
    /// Things that translated but may not look as in Iris (a sampler nothing
    /// provides reads a 1×1 default…), for the pack report.
    pub notes: Vec<String>,
}

/// A compiled pack.
#[derive(Clone, Debug)]
pub struct CompiledPack {
    /// Every slot that compiled, by slot (slots running the same file with
    /// the same geometry share one compile).
    pub programs: BTreeMap<ProgramKind, CompiledProgram>,
    /// What the client builds: slots, buffers, shadow map, textures, settings.
    /// `plan.programs` lists exactly the slots in `programs`.
    pub plan: PackPlan,
    /// Slots whose program exists but did not compile, with the reason; the
    /// frame runs without them (only non-essential slots: see [`compile`]).
    pub skipped: Vec<(ProgramKind, String)>,
    /// `block.properties`: block names to `mc_Entity.x` ids.
    pub blocks: BlockIds,
    /// `entity.properties`: entity names to `entityId` ids.
    pub entities: crate::properties::IdMap,
    /// `item.properties`: item names to `currentRenderedItemId` ids.
    pub items: crate::properties::IdMap,
    /// The shadow map's settings (derived from `plan.shadow`; kept until the
    /// client reads the plan).
    pub shadow: ShadowSettings,
}

/// The shadow map constants a pack declares (`const int shadowMapResolution
/// = 1024;` …), with OptiFine's defaults for the ones it does not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSettings {
    pub resolution: u32,
    /// Half the width of the shadow map's square, in blocks.
    pub distance: f32,
    /// The light's position snaps to this many blocks, so the map does not
    /// shimmer as the player moves.
    pub interval: f32,
    /// `sunPathRotation`, in degrees.
    pub sun_path_rotation: f32,
}

impl Default for ShadowSettings {
    fn default() -> Self {
        Self::from(&plan::ShadowSpec::default())
    }
}

impl From<&plan::ShadowSpec> for ShadowSettings {
    fn from(s: &plan::ShadowSpec) -> Self {
        Self {
            resolution: s.resolution,
            distance: s.distance,
            interval: s.interval,
            sun_path_rotation: s.sun_path_rotation,
        }
    }
}

/// What went wrong, with the pack file it happened in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackError {
    pub file: String,
    pub message: String,
}

impl PackError {
    pub fn new(file: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            message: message.into(),
        }
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
/// `material` is [`BlockIds::material`] (31 = no entry); `normal` is a [`NormalCode`].
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
    /// Not axis-aligned (cross plants, sloped fluid): decoded as the
    /// diagonal `normalize(1, 0, 1)`, a stand-in that is neither up nor a face.
    Other = 6,
    /// No tag (0xFF): decoded as up, material none.
    None = 7,
}

/// The material index meaning "no `block.properties` id".
pub const NO_MATERIAL: u8 = 31;
