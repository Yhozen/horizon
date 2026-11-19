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

pub mod directives;
pub mod frame_math;
pub mod host;
pub mod legacy_ids;
pub mod naga_out;
pub mod noise;
pub mod options;
pub mod plan;
pub mod preprocess;
pub mod properties;
pub mod rewrite;
pub mod schedule;
pub mod uniforms;

pub use naga_out::Stage;
pub use plan::PackPlan;
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
}

impl Default for PackOptions {
    fn default() -> Self {
        Self {
            mc_version: 12111,
            overrides: Vec::new(),
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
}

impl Default for HostParams {
    fn default() -> Self {
        Self {
            sprite_arrays: 1,
            sprite_split: 2048,
            sprite_grid: 1,
            max_varyings: 16,
        }
    }
}

/// A program slot of the frame, by OptiFine's program names. A slot without
/// its own file runs its fallback ([`ProgramKind::candidates`]); a slot with
/// no file at all draws nothing (gbuffers) or is skipped (full-screen passes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProgramKind {
    /// Shadow map, terrain (`shadow`).
    Shadow,
    /// Shadow map, entities and block entities (`shadow`, entity geometry).
    ShadowEntities,
    /// Sky dome, horizon, void (`gbuffers_skybasic`), entity geometry.
    SkyBasic,
    /// Sun and moon (`gbuffers_skytextured`), entity geometry.
    SkyTextured,
    /// Clouds (`gbuffers_clouds`), entity geometry.
    Clouds,
    /// Solid and cutout terrain (`gbuffers_terrain`).
    Terrain,
    /// Translucent terrain (`gbuffers_water`).
    Water,
    /// Block entities (`gbuffers_block`), entity geometry.
    Block,
    /// Entities and name tags (`gbuffers_entities`).
    Entities,
    /// Particles (`gbuffers_textured_lit`).
    TexturedLit,
    /// Unlit particles, world border (`gbuffers_textured`).
    Textured,
    /// Lines (`gbuffers_basic`).
    Basic,
    /// The hand and held items (`gbuffers_hand`).
    Hand,
    /// Translucent held items (`gbuffers_hand_water`).
    HandWater,
    /// Rain and snow (`gbuffers_weather`).
    Weather,
    /// Enchantment glint (`gbuffers_armor_glint`).
    ArmorGlint,
    /// Block-breaking cracks (`gbuffers_damagedblock`).
    DamagedBlock,
    /// `deferred`, `deferred1` … `deferred15` (0 is `deferred`).
    Deferred(u8),
    /// `composite`, `composite1` … `composite15` (0 is `composite`).
    Composite(u8),
    /// The last full-screen pass, into the screen (`final`).
    Final,
}

impl ProgramKind {
    /// Every gbuffers and shadow slot, in no particular order.
    pub const GEOMETRY: [ProgramKind; 17] = [
        Self::Shadow,
        Self::ShadowEntities,
        Self::SkyBasic,
        Self::SkyTextured,
        Self::Clouds,
        Self::Terrain,
        Self::Water,
        Self::Block,
        Self::Entities,
        Self::TexturedLit,
        Self::Textured,
        Self::Basic,
        Self::Hand,
        Self::HandWater,
        Self::Weather,
        Self::ArmorGlint,
        Self::DamagedBlock,
    ];

    /// The program file names to try, best first (OptiFine's fallbacks).
    pub fn candidates(self) -> Vec<String> {
        let chain: &[&str] = match self {
            Self::Shadow | Self::ShadowEntities => &["shadow"],
            Self::Basic => &["gbuffers_basic"],
            Self::Textured => &["gbuffers_textured", "gbuffers_basic"],
            Self::TexturedLit => &[
                "gbuffers_textured_lit",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::SkyBasic => &["gbuffers_skybasic", "gbuffers_basic"],
            Self::SkyTextured => &[
                "gbuffers_skytextured",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::Clouds => &["gbuffers_clouds", "gbuffers_textured", "gbuffers_basic"],
            Self::Terrain => &[
                "gbuffers_terrain",
                "gbuffers_textured_lit",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::DamagedBlock | Self::Block | Self::Water => {
                let first = match self {
                    Self::DamagedBlock => "gbuffers_damagedblock",
                    Self::Block => "gbuffers_block",
                    _ => "gbuffers_water",
                };
                return [first]
                    .into_iter()
                    .chain(Self::Terrain.candidates().iter().map(String::as_str))
                    .map(str::to_string)
                    .collect();
            }
            Self::Entities => &[
                "gbuffers_entities",
                "gbuffers_textured_lit",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::ArmorGlint => &[
                "gbuffers_armor_glint",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::Hand => &[
                "gbuffers_hand",
                "gbuffers_textured_lit",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::HandWater => &[
                "gbuffers_hand_water",
                "gbuffers_hand",
                "gbuffers_textured_lit",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::Weather => &[
                "gbuffers_weather",
                "gbuffers_textured_lit",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::Deferred(n) | Self::Composite(n) => {
                let base = if matches!(self, Self::Deferred(_)) {
                    "deferred"
                } else {
                    "composite"
                };
                return vec![if n == 0 {
                    base.to_string()
                } else {
                    format!("{base}{n}")
                }];
            }
            Self::Final => &["final"],
        };
        chain.iter().map(|s| s.to_string()).collect()
    }

    /// The geometry the program draws.
    pub fn geometry(self) -> host::Geometry {
        match self {
            Self::Terrain | Self::Water | Self::Shadow => host::Geometry::Terrain,
            Self::Deferred(_) | Self::Composite(_) | Self::Final => host::Geometry::Fullscreen,
            _ => host::Geometry::Entity,
        }
    }

    /// Every slot, in frame-independent order: the gbuffers and shadow
    /// slots, `deferred..deferred15`, `composite..composite15`, `final`.
    pub fn all() -> Vec<ProgramKind> {
        let mut all = Self::GEOMETRY.to_vec();
        all.extend((0..16).map(Self::Deferred));
        all.extend((0..16).map(Self::Composite));
        all.push(Self::Final);
        all
    }

    /// The slot's OptiFine program name (`gbuffers_terrain`, `shadow`,
    /// `composite3`); the shadow slot for entities is `shadow_entities`.
    pub fn name(self) -> String {
        match self {
            Self::ShadowEntities => "shadow_entities".to_string(),
            _ => self.candidates().swap_remove(0),
        }
    }

    /// Whether the frame cannot run without this slot's program when the
    /// pack has one: a failure to compile it fails the pack.
    pub fn is_essential(self) -> bool {
        matches!(
            self,
            Self::Terrain | Self::Water | Self::Final | Self::Composite(_)
        )
    }

    /// Whether the program draws into the shadow map.
    pub fn is_shadow(self) -> bool {
        matches!(self, Self::Shadow | Self::ShadowEntities)
    }

    /// Whether the program is a full-screen pass (`deferred`, `composite`, `final`).
    pub fn is_fullscreen(self) -> bool {
        self.geometry() == host::Geometry::Fullscreen
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

/// One texture a program reads from group 1, at `binding` (see
/// [`host::sampler_binding`]); the WGSL declares it as a `texture_2d<f32>`
/// (`texture_depth_2d` when `sampler` is [`SamplerKind::Compare`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureBinding {
    pub binding: u32,
    pub source: TextureSource,
    pub sampler: SamplerKind,
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

/// The world folder a dimension's programs come from, before `shaders/`.
pub fn world_folder(files: &PackFiles, dimension: &str) -> String {
    properties::dimension_folder(files, dimension)
}

/// Compiles every slot of the pack whose program exists for the dimension
/// `dimension` (`minecraft:overworld`, …): the gbuffers and shadow slots,
/// `deferred*`, `composite*` and `final`, and reads the pack's [`PackPlan`].
///
/// A program that fails to compile is an error for the slots the frame
/// cannot do without ([`ProgramKind::is_essential`]: terrain, water, the
/// composites and final); any other slot is left out and listed in
/// [`CompiledPack::skipped`] with the reason.
pub fn compile(
    files: &PackFiles,
    dimension: &str,
    options: &PackOptions,
    host: &HostParams,
) -> Result<CompiledPack, PackError> {
    compile_slots(files, dimension, &ProgramKind::all(), false, options, host)
}

/// Compiles only the slots `kinds`; each must exist and compile. The plan's
/// pack-wide settings still come from every program the pack has.
pub fn compile_kinds(
    files: &PackFiles,
    dimension: &str,
    kinds: &[ProgramKind],
    options: &PackOptions,
    host: &HostParams,
) -> Result<CompiledPack, PackError> {
    compile_slots(files, dimension, kinds, true, options, host)
}

/// The pack path (without extension) of the program `kind` runs, looking in
/// the world folder first and `shaders/` second, best candidate first.
pub fn find_program(files: &PackFiles, folder: &str, kind: ProgramKind) -> Option<String> {
    find_enabled(files, folder, kind, &BTreeSet::new())
}

/// [`find_program`] skipping the programs `shaders.properties` disables
/// (pack paths without extension).
fn find_enabled(
    files: &PackFiles,
    folder: &str,
    kind: ProgramKind,
    disabled: &BTreeSet<String>,
) -> Option<String> {
    let dirs = if folder.is_empty() {
        vec!["shaders/".to_string()]
    } else {
        vec![format!("shaders/{folder}/"), "shaders/".to_string()]
    };
    for name in kind.candidates() {
        for dir in &dirs {
            let path = format!("{dir}{name}");
            if !disabled.contains(&path)
                && files.contains(&format!("{path}.vsh"))
                && files.contains(&format!("{path}.fsh"))
            {
                return Some(path);
            }
        }
    }
    None
}

/// A program's two stages, preprocessed and with includes only expanded.
struct Prepared {
    vsh: String,
    fsh: String,
    raw_vsh: String,
    raw_fsh: String,
}

impl Prepared {
    fn new(
        files: &PackFiles,
        program: &str,
        predefined: &BTreeMap<String, String>,
    ) -> Result<Self, PackError> {
        let (vsh_path, fsh_path) = (format!("{program}.vsh"), format!("{program}.fsh"));
        Ok(Self {
            vsh: preprocess::preprocess(files, &vsh_path, predefined, &[])?,
            fsh: preprocess::preprocess(files, &fsh_path, predefined, &[])?,
            // Before preprocessing removes comments: DRAWBUFFERS and some
            // directives live in one.
            raw_vsh: preprocess::expand_includes(files, &vsh_path)?,
            raw_fsh: preprocess::expand_includes(files, &fsh_path)?,
        })
    }

    /// The draw buffers of `kind` running this program: the directive, else
    /// shadowcolor0/1 for shadow programs, every `gl_FragData[i]` up to the
    /// highest written for full-screen passes, colortex0 for gbuffers.
    fn draw_buffers(&self, kind: ProgramKind) -> Vec<u8> {
        if let Some(list) = properties::draw_buffers(&self.raw_fsh) {
            return list;
        }
        if kind.is_shadow() {
            return vec![0, 1];
        }
        if kind.is_fullscreen() {
            let highest = (1..8u8)
                .rev()
                .find(|i| self.fsh.contains(&format!("gl_FragData[{i}]")));
            return (0..=highest.unwrap_or(0)).collect();
        }
        vec![0]
    }
}

/// A pack read up to compiling: the plan and the preprocessed programs.
struct Read {
    plan: PackPlan,
    blocks: BlockIds,
    /// By pack path (no extension).
    prepared: BTreeMap<String, Prepared>,
    skipped: Vec<(ProgramKind, String)>,
}

/// Reads what a pack asks of the renderer without compiling its programs:
/// every slot present (after fallbacks and `program.*.enabled`), draw
/// buffers, buffer formats, mipmaps, shadow map, custom textures,
/// `shaders.properties` settings and options. Slots whose sources do not
/// preprocess are left out (an error for essential ones).
pub fn read_plan(
    files: &PackFiles,
    dimension: &str,
    options: &PackOptions,
) -> Result<PackPlan, PackError> {
    Ok(read(files, dimension, &ProgramKind::all(), false, options)?.plan)
}

fn read(
    files: &PackFiles,
    dimension: &str,
    kinds: &[ProgramKind],
    strict: bool,
    options: &PackOptions,
) -> Result<Read, PackError> {
    let predefined = preprocess::predefined(options.mc_version);
    let defaults = options::discover(files);
    let files = &options::apply(files, &options.overrides);
    let mut macros = predefined.clone();
    macros.extend(options::macros(&options::discover(files)));
    let props = directives::shaders_properties(files, &macros)?;
    let blocks = properties::block_ids(files, &predefined)?;
    let folder = world_folder(files, dimension);

    // Every slot the pack has: pack-wide directives come from all of them.
    let resolved: BTreeMap<ProgramKind, String> = ProgramKind::all()
        .into_iter()
        .filter_map(|k| Some((k, find_enabled(files, &folder, k, &props.disabled)?)))
        .collect();
    let mut prepared: BTreeMap<String, Result<Prepared, PackError>> = BTreeMap::new();
    for path in resolved.values() {
        if !prepared.contains_key(path) {
            prepared.insert(path.clone(), Prepared::new(files, path, &predefined));
        }
    }
    let mut plan = PackPlan {
        properties: props.properties,
        custom_textures: props.textures,
        options: defaults,
        ..PackPlan::default()
    };
    directives::read_globals(
        &mut plan,
        prepared
            .values()
            .filter_map(|p| p.as_ref().ok())
            .map(|p| directives::ProgramText {
                raw: [&p.raw_vsh, &p.raw_fsh],
                preprocessed: [&p.vsh, &p.fsh],
            }),
    );

    let mut skipped = Vec::new();
    let mut wanted: Vec<ProgramKind> = kinds.to_vec();
    wanted.sort();
    wanted.dedup();
    for kind in wanted {
        let Some(path) = resolved.get(&kind) else {
            if strict {
                return Err(PackError::new(
                    format!("shaders/{folder}"),
                    format!("no program for {kind:?}"),
                ));
            }
            continue;
        };
        match &prepared[path] {
            Err(e) if strict || kind.is_essential() => return Err(e.clone()),
            Err(e) => skipped.push((kind, e.to_string())),
            Ok(p) => {
                plan.programs.insert(kind, path.clone());
                plan.draw_buffers.insert(kind, p.draw_buffers(kind));
                if kind.is_fullscreen() {
                    let mips = directives::mipmaps(&p.raw_fsh, &p.fsh);
                    if !mips.is_empty() {
                        plan.mipmaps.insert(kind, mips);
                    }
                }
            }
        }
    }
    Ok(Read {
        plan,
        blocks,
        prepared: prepared
            .into_iter()
            .filter_map(|(k, p)| Some((k, p.ok()?)))
            .collect(),
        skipped,
    })
}

fn compile_slots(
    files: &PackFiles,
    dimension: &str,
    kinds: &[ProgramKind],
    strict: bool,
    options: &PackOptions,
    host: &HostParams,
) -> Result<CompiledPack, PackError> {
    let Read {
        mut plan,
        blocks,
        prepared,
        mut skipped,
    } = read(files, dimension, kinds, strict, options)?;
    // One compile per (file, geometry, shadow or not, draw buffers, host variant).
    type Key<'k> = (
        &'k str,
        host::Geometry,
        bool,
        &'k [u8],
        (Option<&'static str>, bool),
    );
    let mut done: Vec<(Key, Result<CompiledProgram, PackError>)> = Vec::new();
    let mut results = Vec::new();
    for (&kind, path) in &plan.programs {
        let draw_buffers = &plan.draw_buffers[&kind];
        let key: Key = (
            path,
            kind.geometry(),
            kind.is_shadow(),
            draw_buffers,
            host::variant(kind),
        );
        let result = match done.iter().find(|(k, _)| *k == key) {
            Some((_, r)) => r.clone(),
            None => {
                // Only prepared programs are in the plan.
                let p = &prepared[path];
                let r = compile_program(path, p, kind, draw_buffers, host, &blocks, &plan);
                done.push((key, r.clone()));
                r
            }
        };
        results.push((kind, result));
    }
    let mut programs = BTreeMap::new();
    for (kind, result) in results {
        match result {
            Ok(program) => {
                programs.insert(kind, program);
            }
            Err(e) if strict || kind.is_essential() => return Err(e),
            Err(e) => {
                skipped.push((kind, e.to_string()));
                plan.programs.remove(&kind);
                plan.draw_buffers.remove(&kind);
                plan.mipmaps.remove(&kind);
            }
        }
    }
    let shadow = ShadowSettings::from(&plan.shadow);
    Ok(CompiledPack {
        programs,
        plan,
        skipped,
        blocks,
        shadow,
    })
}

fn compile_program(
    program: &str,
    p: &Prepared,
    kind: ProgramKind,
    draw_buffers: &[u8],
    host: &HostParams,
    blocks: &BlockIds,
    plan: &PackPlan,
) -> Result<CompiledProgram, PackError> {
    let vsh_path = format!("{program}.vsh");
    let fsh_path = format!("{program}.fsh");
    let vertex =
        rewrite::rewrite(&p.vsh, Stage::Vertex).map_err(|m| PackError::new(&vsh_path, m))?;
    let fragment =
        rewrite::rewrite(&p.fsh, Stage::Fragment).map_err(|m| PackError::new(&fsh_path, m))?;
    let spec = host::ProgramSpec {
        kind,
        geometry: kind.geometry(),
        draw_buffers,
        host,
        blocks,
        plan,
    };
    let host::HostOutput {
        vertex: vs_glsl,
        fragment: fs_glsl,
        textures,
        unknown_uniforms,
    } = host::program_sources(&vertex, &fragment, &spec).map_err(|m| PackError::new(program, m))?;
    let vertex_wgsl = naga_out::glsl_to_wgsl(&vs_glsl, Stage::Vertex)
        .map_err(|m| PackError::new(&vsh_path, m))?;
    let fragment_wgsl = naga_out::glsl_to_wgsl(&fs_glsl, Stage::Fragment)
        .map_err(|m| PackError::new(&fsh_path, m))?;
    // The samplers the program reads (a pack may declare more than it uses).
    let samplers = vertex
        .sample_calls
        .iter()
        .chain(&fragment.sample_calls)
        .filter_map(|call| host::split_sampling_call(call).map(|(_, name)| name.to_string()))
        .collect();
    Ok(CompiledProgram {
        source: program.to_string(),
        vertex_wgsl,
        fragment_wgsl,
        draw_buffers: draw_buffers.to_vec(),
        samplers,
        textures,
        unknown_uniforms,
    })
}

#[cfg(test)]
mod tests;
