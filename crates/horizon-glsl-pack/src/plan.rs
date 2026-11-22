//! What a pack asks of the renderer, read from its sources and properties:
//! programs present, render targets, the shadow map, custom textures and
//! settings. The client builds its passes from a [`PackPlan`]; nothing here
//! touches wgpu.

use std::collections::{BTreeMap, BTreeSet};

use crate::ProgramKind;

/// A colour target's format, as a pack names it (`RGBA16`, `R11F_G11F_B10F`,
/// `RGBA8_SNORM`, `R32UI` …): every name of the format docs.
///
/// For the client: store each in [`BufferFormat::storage`] (a renderable
/// WebGPU format; 16-bit normalized, 8-bit signed normalized and the packed
/// small formats are widened, the host clamps what programs write to the
/// pack's range). Integer formats are `*Sint`/`*Uint` targets, sampled as
/// `texture_2d<i32>`/`<u32>` with a point sampler, never blended or filtered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum BufferFormat {
    R8,
    Rg8,
    Rgb8,
    #[default]
    Rgba8,
    R16,
    Rg16,
    Rgb16,
    Rgba16,
    R16F,
    Rg16F,
    Rgb16F,
    Rgba16F,
    R32F,
    Rg32F,
    Rgb32F,
    Rgba32F,
    R11G11B10F,
    Rgb10A2,
    Rgba4,
    Rgb5A1,
    Rgb565,
    Rgba2,
    R3G3B2,
    Rgb9E5,
    R8Snorm,
    Rg8Snorm,
    Rgb8Snorm,
    Rgba8Snorm,
    R16Snorm,
    Rg16Snorm,
    Rgb16Snorm,
    Rgba16Snorm,
    R8I,
    Rg8I,
    Rgb8I,
    Rgba8I,
    R16I,
    Rg16I,
    Rgb16I,
    Rgba16I,
    R32I,
    Rg32I,
    Rgb32I,
    Rgba32I,
    R8UI,
    Rg8UI,
    Rgb8UI,
    Rgba8UI,
    R16UI,
    Rg16UI,
    Rgb16UI,
    Rgba16UI,
    R32UI,
    Rg32UI,
    Rgb32UI,
    Rgba32UI,
    Rgb10A2UI,
}

/// What a format's values are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatKind {
    /// 0…1.
    Unorm,
    /// -1…1.
    Snorm,
    Float,
    /// Signed integers (`ivec4` outputs, `isampler2D`).
    Sint,
    /// Unsigned integers (`uvec4` outputs, `usampler2D`).
    Uint,
}

/// The renderable format a [`BufferFormat`] is stored in (WebGPU names;
/// `*32Float` may go down to `*16Float` on devices without float32 blending).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StorageFormat {
    R8Unorm,
    Rg8Unorm,
    Rgba8Unorm,
    R16Float,
    Rg16Float,
    Rgba16Float,
    R32Float,
    Rg32Float,
    Rgba32Float,
    Rg11b10Ufloat,
    Rgb10a2Unorm,
    Rgb10a2Uint,
    R8Sint,
    Rg8Sint,
    Rgba8Sint,
    R16Sint,
    Rg16Sint,
    Rgba16Sint,
    R32Sint,
    Rg32Sint,
    Rgba32Sint,
    R8Uint,
    Rg8Uint,
    Rgba8Uint,
    R16Uint,
    Rg16Uint,
    Rgba16Uint,
    R32Uint,
    Rg32Uint,
    Rgba32Uint,
}

impl StorageFormat {
    /// WebGPU's render-target pixel byte cost (what
    /// `maxColorAttachmentBytesPerSample` counts).
    pub fn bytes(self) -> u32 {
        use StorageFormat::*;
        match self {
            R8Unorm | R8Sint | R8Uint => 1,
            Rg8Unorm | R16Float | Rg8Sint | Rg8Uint | R16Sint | R16Uint => 2,
            R32Float | Rg16Float | Rgba8Sint | Rgba8Uint | Rg16Sint | Rg16Uint | R32Sint
            | R32Uint | Rgb10a2Uint => 4,
            Rgba8Unorm | Rgba16Float | Rg32Float | Rg11b10Ufloat | Rgb10a2Unorm | Rgba16Sint
            | Rgba16Uint | Rg32Sint | Rg32Uint => 8,
            Rgba32Float | Rgba32Sint | Rgba32Uint => 16,
        }
    }
}

/// `(name, format, kind, channels, bits per channel)`; the bits of packed
/// formats are their widest channel's.
const FORMATS: [(&str, BufferFormat, FormatKind, u8, u8); 57] = {
    use BufferFormat as F;
    use FormatKind::*;
    [
        ("R8", F::R8, Unorm, 1, 8),
        ("RG8", F::Rg8, Unorm, 2, 8),
        ("RGB8", F::Rgb8, Unorm, 3, 8),
        ("RGBA8", F::Rgba8, Unorm, 4, 8),
        ("R16", F::R16, Unorm, 1, 16),
        ("RG16", F::Rg16, Unorm, 2, 16),
        ("RGB16", F::Rgb16, Unorm, 3, 16),
        ("RGBA16", F::Rgba16, Unorm, 4, 16),
        ("R16F", F::R16F, Float, 1, 16),
        ("RG16F", F::Rg16F, Float, 2, 16),
        ("RGB16F", F::Rgb16F, Float, 3, 16),
        ("RGBA16F", F::Rgba16F, Float, 4, 16),
        ("R32F", F::R32F, Float, 1, 32),
        ("RG32F", F::Rg32F, Float, 2, 32),
        ("RGB32F", F::Rgb32F, Float, 3, 32),
        ("RGBA32F", F::Rgba32F, Float, 4, 32),
        ("R11F_G11F_B10F", F::R11G11B10F, Float, 3, 11),
        ("RGB10_A2", F::Rgb10A2, Unorm, 4, 10),
        ("RGBA4", F::Rgba4, Unorm, 4, 4),
        ("RGB5_A1", F::Rgb5A1, Unorm, 4, 5),
        ("RGB565", F::Rgb565, Unorm, 3, 6),
        ("RGBA2", F::Rgba2, Unorm, 4, 2),
        ("R3_G3_B2", F::R3G3B2, Unorm, 3, 3),
        ("RGB9_E5", F::Rgb9E5, Float, 3, 9),
        ("R8_SNORM", F::R8Snorm, Snorm, 1, 8),
        ("RG8_SNORM", F::Rg8Snorm, Snorm, 2, 8),
        ("RGB8_SNORM", F::Rgb8Snorm, Snorm, 3, 8),
        ("RGBA8_SNORM", F::Rgba8Snorm, Snorm, 4, 8),
        ("R16_SNORM", F::R16Snorm, Snorm, 1, 16),
        ("RG16_SNORM", F::Rg16Snorm, Snorm, 2, 16),
        ("RGB16_SNORM", F::Rgb16Snorm, Snorm, 3, 16),
        ("RGBA16_SNORM", F::Rgba16Snorm, Snorm, 4, 16),
        ("R8I", F::R8I, Sint, 1, 8),
        ("RG8I", F::Rg8I, Sint, 2, 8),
        ("RGB8I", F::Rgb8I, Sint, 3, 8),
        ("RGBA8I", F::Rgba8I, Sint, 4, 8),
        ("R16I", F::R16I, Sint, 1, 16),
        ("RG16I", F::Rg16I, Sint, 2, 16),
        ("RGB16I", F::Rgb16I, Sint, 3, 16),
        ("RGBA16I", F::Rgba16I, Sint, 4, 16),
        ("R32I", F::R32I, Sint, 1, 32),
        ("RG32I", F::Rg32I, Sint, 2, 32),
        ("RGB32I", F::Rgb32I, Sint, 3, 32),
        ("RGBA32I", F::Rgba32I, Sint, 4, 32),
        ("R8UI", F::R8UI, Uint, 1, 8),
        ("RG8UI", F::Rg8UI, Uint, 2, 8),
        ("RGB8UI", F::Rgb8UI, Uint, 3, 8),
        ("RGBA8UI", F::Rgba8UI, Uint, 4, 8),
        ("R16UI", F::R16UI, Uint, 1, 16),
        ("RG16UI", F::Rg16UI, Uint, 2, 16),
        ("RGB16UI", F::Rgb16UI, Uint, 3, 16),
        ("RGBA16UI", F::Rgba16UI, Uint, 4, 16),
        ("R32UI", F::R32UI, Uint, 1, 32),
        ("RG32UI", F::Rg32UI, Uint, 2, 32),
        ("RGB32UI", F::Rgb32UI, Uint, 3, 32),
        ("RGBA32UI", F::Rgba32UI, Uint, 4, 32),
        ("RGB10_A2UI", F::Rgb10A2UI, Uint, 4, 10),
    ]
};

impl BufferFormat {
    /// The format named `name` (`RGBA16`, `r11f_g11f_b10f`, `RGBA8_SNORM`,
    /// `R32UI` …), in any case; an optional `GL_` prefix is accepted.
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim();
        let name = name
            .get(..3)
            .filter(|p| p.eq_ignore_ascii_case("GL_"))
            .map_or(name, |_| &name[3..]);
        FORMATS
            .iter()
            .find(|f| f.0.eq_ignore_ascii_case(name))
            .map(|f| f.1)
    }

    fn row(self) -> &'static (&'static str, BufferFormat, FormatKind, u8, u8) {
        FORMATS
            .iter()
            .find(|f| f.1 == self)
            .expect("every format has a row")
    }

    /// The pack's name for it (`RGBA16F`, `R11F_G11F_B10F` …).
    pub fn name(self) -> &'static str {
        self.row().0
    }

    /// Channels the pack sees; reads of the missing ones return 0, alpha 1.
    pub fn channels(self) -> u8 {
        self.row().3
    }

    pub fn kind(self) -> FormatKind {
        self.row().2
    }

    /// Normalized (0…1) formats: writes are clamped to 0…1, NaN becomes 0.
    /// The client may store them in a float format (16-bit unorm is not a
    /// render target without adapter-specific features).
    pub fn is_unorm(self) -> bool {
        self.kind() == FormatKind::Unorm
    }

    /// The 16-bit normalized formats (`R16` … `RGBA16`), which the client may
    /// store as half floats.
    pub fn is_unorm16(self) -> bool {
        self.is_unorm() && self.row().4 == 16
    }

    /// Integer formats (`R32UI`, `RGBA16I` …).
    pub fn is_integer(self) -> bool {
        matches!(self.kind(), FormatKind::Sint | FormatKind::Uint)
    }

    /// The format to store it in.
    pub fn storage(self) -> StorageFormat {
        use StorageFormat as S;
        let (_, _, kind, channels, bits) = *self.row();
        let pick = |one, two, four| match channels {
            1 => one,
            2 => two,
            _ => four,
        };
        match (self, kind) {
            (Self::Rgb10A2, _) => S::Rgb10a2Unorm,
            (Self::Rgb10A2UI, _) => S::Rgb10a2Uint,
            (Self::R11G11B10F, _) => S::Rg11b10Ufloat,
            (Self::Rgb9E5, _) => S::Rgba16Float,
            (_, FormatKind::Unorm) if bits <= 8 => pick(S::R8Unorm, S::Rg8Unorm, S::Rgba8Unorm),
            (_, FormatKind::Unorm | FormatKind::Snorm) => {
                pick(S::R16Float, S::Rg16Float, S::Rgba16Float)
            }
            (_, FormatKind::Float) if bits == 16 => pick(S::R16Float, S::Rg16Float, S::Rgba16Float),
            (_, FormatKind::Float) => pick(S::R32Float, S::Rg32Float, S::Rgba32Float),
            (_, FormatKind::Sint) => match bits {
                8 => pick(S::R8Sint, S::Rg8Sint, S::Rgba8Sint),
                16 => pick(S::R16Sint, S::Rg16Sint, S::Rgba16Sint),
                _ => pick(S::R32Sint, S::Rg32Sint, S::Rgba32Sint),
            },
            (_, FormatKind::Uint) => match bits {
                8 => pick(S::R8Uint, S::Rg8Uint, S::Rgba8Uint),
                16 => pick(S::R16Uint, S::Rg16Uint, S::Rgba16Uint),
                _ => pick(S::R32Uint, S::Rg32Uint, S::Rgba32Uint),
            },
        }
    }

    /// Bytes a pixel takes against the device's attachment-bytes limit.
    pub fn bytes(self) -> u32 {
        self.storage().bytes()
    }
}

/// A buffer's size (`size.buffer.<buffer>=<w> <h>`).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum BufferSize {
    /// The screen's (the default).
    #[default]
    Screen,
    /// Fractions of the screen's width and height.
    Relative(f32, f32),
    /// Fixed pixels.
    Pixels(u32, u32),
}

impl BufferSize {
    /// The size in pixels for a `width × height` screen (at least 1×1).
    pub fn pixels(self, width: u32, height: u32) -> (u32, u32) {
        let (w, h) = match self {
            Self::Screen => (width, height),
            Self::Relative(x, y) => (
                (width as f32 * x).round() as u32,
                (height as f32 * y).round() as u32,
            ),
            Self::Pixels(w, h) => (w, h),
        };
        (w.max(1), h.max(1))
    }
}

/// One `colortexN` buffer (both its main and alternate side).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BufferSpec {
    pub format: BufferFormat,
    /// Cleared every frame (`colortexNClear`, default true). A buffer that is
    /// not is still cleared on the first frame and after a resize.
    pub clear: bool,
    /// `colortexNClearColor`; `None` is the default (colortex0 the fog
    /// colour with alpha 1, colortex1 all ones, others zero).
    pub clear_color: Option<[f32; 4]>,
    /// `size.buffer.colortexN`. Gbuffers programs cannot draw into a buffer
    /// of another size than the screen's (the docs say so; the client skips
    /// such a draw buffer).
    pub size: BufferSize,
}

impl Default for BufferSpec {
    fn default() -> Self {
        Self {
            format: BufferFormat::Rgba8,
            clear: true,
            clear_color: None,
            size: BufferSize::Screen,
        }
    }
}

/// The shadow map's settings (`const` directives in the programs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSpec {
    pub resolution: u32,
    /// Half the width of the shadow map's square, in blocks.
    pub distance: f32,
    /// The light's position snaps to this many blocks (0: no snapping).
    pub interval: f32,
    /// `shadowDistanceRenderMul`: < 0 culls by the view, else draws up to
    /// `distance × mul` blocks.
    pub distance_render_mul: f32,
    /// `sunPathRotation`, in degrees.
    pub sun_path_rotation: f32,
    /// `shadowHardwareFiltering[N]`: `shadowtexN` is read with a depth compare.
    pub hardware_filtering: [bool; 2],
    /// `shadowtexNMipmap` / `generateShadowMipmap`.
    pub tex_mipmap: [bool; 2],
    /// `shadowtexNNearest` / `shadowNMinMagNearest`.
    pub tex_nearest: [bool; 2],
    /// `shadowcolorNMipmap` (generated; sampling ignores mips).
    pub color_mipmap: [bool; 2],
    pub color_format: [BufferFormat; 2],
    /// `shadowcolorNClear` (default true) and `shadowcolorNClearColor` (default ones).
    pub color_clear: [bool; 2],
    pub color_clear_color: [[f32; 4]; 2],
}

impl Default for ShadowSpec {
    fn default() -> Self {
        Self {
            resolution: 1024,
            distance: 160.0,
            interval: 2.0,
            distance_render_mul: -1.0,
            sun_path_rotation: 0.0,
            hardware_filtering: [false; 2],
            tex_mipmap: [false; 2],
            tex_nearest: [false; 2],
            color_mipmap: [false; 2],
            color_format: [BufferFormat::Rgba8; 2],
            color_clear: [true; 2],
            color_clear_color: [[1.0; 4]; 2],
        }
    }
}

/// Which program family a `texture.<stage>.<name>` override applies to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextureStage {
    /// gbuffers programs (and, as the spec says, shadow programs).
    #[default]
    Gbuffers,
    Shadow,
    Deferred,
    /// `composite*` and `final`.
    Composite,
    /// Stages Horizon does not run yet: stored so the report can name them.
    Prepare,
    Shadowcomp,
    Begin,
    Setup,
    /// `customTexture.<name>`: every program that declares the sampler.
    All,
}

/// A texture the pack binds to one of its samplers
/// (`texture.gbuffers.gaux1=textures/ripples.png`, `customTexture.lut=…`).
/// The client loads it (see [`crate::textures`]); nothing here decodes images.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomTexture {
    pub stage: TextureStage,
    /// The sampler it replaces, canonical (`colortex4`, not `gaux1`), or the
    /// pack's own sampler name.
    pub sampler: String,
    /// Pack path of the file (`shaders/textures/ripples.png`); for a
    /// [`crate::textures::CustomTextureKind::Resource`], the resource name.
    pub path: String,
    /// What the file is: a PNG, raw texels, or a resource-pack texture.
    pub kind: crate::textures::CustomTextureKind,
    /// Linear filtering (a `.mcmeta` with `"blur": true`; raw textures: true).
    pub blur: bool,
    /// Clamp to edge rather than repeat (`"clamp": true`; raw textures: true).
    pub clamp: bool,
}

/// `clouds=` in shaders.properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Clouds {
    #[default]
    Default,
    Off,
    Fast,
    Fancy,
}

/// `shadow.culling=` in shaders.properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ShadowCulling {
    /// Not set: the shadow map is culled to its own frustum.
    #[default]
    Default,
    On,
    Off,
    /// Iris: no culling within the voxel distance, culling beyond it.
    Reversed,
}

/// Where translucent particles draw (`particles.ordering`, or the legacy
/// `particles.before.deferred=true`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleOrdering {
    /// Opaque particles before `deferred`, translucent ones after.
    Mixed,
    /// All particles after `deferred`.
    After,
    /// All particles before `deferred`.
    Before,
}

/// Keys of `shaders.properties` the renderer follows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PackProperties {
    pub clouds: Clouds,
    /// `oldLighting`: keep vanilla face shading baked into terrain colour (default false).
    pub old_lighting: bool,
    pub underwater_overlay: bool,
    pub sun: bool,
    pub moon: bool,
    /// `separateAo`: AO in `gl_Color.a` instead of the RGB (default false).
    pub separate_ao: bool,
    pub old_hand_light: bool,
    /// `vignette` (default true).
    pub vignette: bool,
    /// Iris `sky`: the vanilla sky disc, horizon and void (default true).
    pub sky: bool,
    /// Iris `stars` (default true).
    pub stars: bool,
    /// Iris `weather=<geometry> <splashes>` (default both true).
    pub weather: bool,
    pub weather_particles: bool,
    /// `shadow.enabled`: `Some(false)` turns the shadow pass off even when
    /// the pack has a shadow program (default `None`: on when it has one).
    pub shadow_enabled: Option<bool>,
    /// `shadowTerrain`, `shadowTranslucent`, `shadowEntities`,
    /// `shadowBlockEntities` (default true): what the shadow map draws.
    pub shadow_terrain: bool,
    pub shadow_translucent: bool,
    pub shadow_entities: bool,
    pub shadow_block_entities: bool,
    /// `shadowPlayer` (default false): the player draws in the shadow map
    /// even when `shadowEntities` is off.
    pub shadow_player: bool,
    pub shadow_culling: ShadowCulling,
    /// `frustum.culling` (default true).
    pub frustum_culling: bool,
    /// `rain.depth` (default false): weather writes depthtex0.
    pub rain_depth: bool,
    /// `beacon.beam.depth` (default false): the inner beam is opaque.
    pub beacon_beam_depth: bool,
    /// `particles.ordering` / `particles.before.deferred`; `None` when the
    /// pack sets neither (see [`PackPlan::particle_ordering`]).
    pub particles: Option<ParticleOrdering>,
}

impl Default for PackProperties {
    fn default() -> Self {
        Self {
            clouds: Clouds::Default,
            old_lighting: false,
            underwater_overlay: false,
            sun: true,
            moon: true,
            separate_ao: false,
            old_hand_light: true,
            vignette: true,
            sky: true,
            stars: true,
            weather: true,
            weather_particles: true,
            shadow_enabled: None,
            shadow_terrain: true,
            shadow_translucent: true,
            shadow_entities: true,
            shadow_block_entities: true,
            shadow_player: false,
            shadow_culling: ShadowCulling::Default,
            frustum_culling: true,
            rain_depth: false,
            beacon_beam_depth: false,
            particles: None,
        }
    }
}

/// A blend factor of `blend.*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendFactor {
    Zero,
    One,
    SrcColor,
    OneMinusSrcColor,
    DstColor,
    OneMinusDstColor,
    SrcAlpha,
    OneMinusSrcAlpha,
    DstAlpha,
    OneMinusDstAlpha,
    SrcAlphaSaturate,
}

impl BlendFactor {
    /// `ZERO`, `ONE_MINUS_SRC_ALPHA` … (any case, optional `GL_` prefix).
    pub fn parse(name: &str) -> Option<Self> {
        let n = name.trim().to_ascii_uppercase();
        let n = n.strip_prefix("GL_").unwrap_or(&n);
        Some(match n {
            "ZERO" => Self::Zero,
            "ONE" => Self::One,
            "SRC_COLOR" => Self::SrcColor,
            "ONE_MINUS_SRC_COLOR" => Self::OneMinusSrcColor,
            "DST_COLOR" => Self::DstColor,
            "ONE_MINUS_DST_COLOR" => Self::OneMinusDstColor,
            "SRC_ALPHA" => Self::SrcAlpha,
            "ONE_MINUS_SRC_ALPHA" => Self::OneMinusSrcAlpha,
            "DST_ALPHA" => Self::DstAlpha,
            "ONE_MINUS_DST_ALPHA" => Self::OneMinusDstAlpha,
            "SRC_ALPHA_SATURATE" => Self::SrcAlphaSaturate,
            _ => return None,
        })
    }
}

/// `blend.<program>[.<buffer>]=off|<src> <dst> <srcA> <dstA>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Off,
    /// Colour and alpha factors; the operation is always add.
    On {
        src: BlendFactor,
        dst: BlendFactor,
        src_alpha: BlendFactor,
        dst_alpha: BlendFactor,
    },
}

impl Blend {
    pub fn parse(value: &str) -> Option<Self> {
        let v = value.trim();
        if v.eq_ignore_ascii_case("off") || v.eq_ignore_ascii_case("false") {
            return Some(Self::Off);
        }
        let f: Vec<BlendFactor> = v
            .split_whitespace()
            .map(BlendFactor::parse)
            .collect::<Option<_>>()?;
        match f[..] {
            [src, dst, src_alpha, dst_alpha] => Some(Self::On {
                src,
                dst,
                src_alpha,
                dst_alpha,
            }),
            _ => None,
        }
    }
}

/// An alpha test comparison: a fragment passes when `alpha <func> ref`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaFunc {
    Never,
    Less,
    Equal,
    LessEqual,
    Greater,
    NotEqual,
    GreaterEqual,
    Always,
}

/// `alphaTest.<program>=off|<func> <ref>`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AlphaTest {
    Off,
    Test(AlphaFunc, f32),
}

impl AlphaTest {
    pub fn parse(value: &str) -> Option<Self> {
        let v = value.trim();
        if v.eq_ignore_ascii_case("off") || v.eq_ignore_ascii_case("false") {
            return Some(Self::Off);
        }
        let mut words = v.split_whitespace();
        let func = words.next()?.to_ascii_uppercase();
        let func = func.strip_prefix("GL_").unwrap_or(&func);
        let func = match func {
            "NEVER" => AlphaFunc::Never,
            "LESS" => AlphaFunc::Less,
            "EQUAL" => AlphaFunc::Equal,
            "LEQUAL" => AlphaFunc::LessEqual,
            "GREATER" => AlphaFunc::Greater,
            "NOTEQUAL" => AlphaFunc::NotEqual,
            "GEQUAL" => AlphaFunc::GreaterEqual,
            "ALWAYS" => AlphaFunc::Always,
            _ => return None,
        };
        let reference = match words.next() {
            Some(r) => r.trim_end_matches(['f', 'F']).parse().ok()?,
            None => 0.0,
        };
        Some(Self::Test(func, reference))
    }
}

/// `scale.<program>=<scale> [<x> <y>]`: a full-screen pass draws into the
/// viewport `(x, y, scale, scale)` of its targets, in fractions of their size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    pub scale: f32,
    pub offset: [f32; 2],
}

/// The per-program state `shaders.properties` sets for one program name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProgramState {
    /// `blend.<program>`: every draw buffer (`None`: the slot's default).
    pub blend: Option<Blend>,
    /// `blend.<program>.<buffer>`, by colortex index: wins over `blend`.
    pub buffer_blend: BTreeMap<u8, Blend>,
    /// `alphaTest.<program>` (`None`: the slot's default, which the host applies).
    pub alpha_test: Option<AlphaTest>,
    /// `scale.<program>` (full-screen passes).
    pub scale: Option<Scale>,
    /// `flip.<program>.<buffer>`, by colortex index (default: flip what it writes).
    pub flip: BTreeMap<u8, bool>,
}

impl ProgramState {
    /// The blend of draw buffer `buffer` (`None`: the slot's default).
    pub fn blend_of(&self, buffer: u8) -> Option<Blend> {
        self.buffer_blend.get(&buffer).copied().or(self.blend)
    }
}

/// One fragment output the pack declares (`layout(location = 1) out vec3 x;`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FragOutput {
    /// Declaration order: the rewriter's `hz_gl_FragData` index for it.
    pub index: usize,
    pub name: String,
    /// GLSL type (`vec4`, `vec2`, `float`, `uvec4`, `int` …).
    pub ty: String,
    /// `layout(location = N)`; `None`: its declaration order.
    pub location: Option<u32>,
}

impl FragOutput {
    /// The draw-buffer position it writes (location, else declaration order).
    pub fn slot(&self) -> usize {
        self.location.map_or(self.index, |l| l as usize)
    }
}

/// A pack option the user can change (`#define X`, `//#define X`,
/// `#define X 2 // [1 2 4]`, `const int shadowMapResolution = 2048;`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackOption {
    pub name: String,
    /// `None` for a boolean option.
    pub value: Option<String>,
    /// Boolean: on by default. Value options: always true.
    pub enabled: bool,
    /// The values from a trailing `// [a b c]` comment.
    pub allowed: Vec<String>,
}

/// Everything the client needs to build a pack's passes.
#[derive(Clone, Debug, PartialEq)]
pub struct PackPlan {
    /// Every slot that runs, with the pack path (no extension) of the program
    /// it runs after fallbacks, e.g. `Water → shaders/gbuffers_water`.
    pub programs: BTreeMap<ProgramKind, String>,
    /// Draw buffers of each program (`/* DRAWBUFFERS:046 */` → `[0, 4, 6]`;
    /// gbuffers default `[0]`; shadow default `[0, 1]` meaning shadowcolor0/1).
    pub draw_buffers: BTreeMap<ProgramKind, Vec<u8>>,
    /// `colortex0..15`.
    pub buffers: [BufferSpec; 16],
    /// For each full-screen pass, the buffers it regenerates mipmaps for
    /// before it runs (`colortexNMipmapEnabled` in its fragment source).
    pub mipmaps: BTreeMap<ProgramKind, BTreeSet<u8>>,
    pub shadow: ShadowSpec,
    pub custom_textures: Vec<CustomTexture>,
    /// `noiseTextureResolution` (default 256).
    pub noise_resolution: u32,
    /// `ambientOcclusionLevel` (default 1), clamped to 0…1.
    pub ambient_occlusion_level: f32,
    /// `wetnessHalflife` / `drynessHalflife` in ticks-like units ×0.1 s (defaults 600 / 200).
    pub wetness_half_life: f32,
    pub dryness_half_life: f32,
    /// `eyeBrightnessHalflife` (default 10, i.e. 1 s).
    pub eye_brightness_half_life: f32,
    pub properties: PackProperties,
    /// Options found in the sources, for `--shader-pack-option` and logs.
    pub options: Vec<PackOption>,
    /// Per-program state from `shaders.properties`, by program name
    /// (`gbuffers_water`, `composite2`); read it through [`PackPlan::state`].
    pub program_states: BTreeMap<String, ProgramState>,
    /// The fragment outputs each slot's program declares (empty when it
    /// writes `gl_FragData`/`gl_FragColor`).
    pub outputs: BTreeMap<ProgramKind, Vec<FragOutput>>,
    /// `uniform.*` / `variable.*` of `shaders.properties` (group 1, binding 1).
    pub custom_uniforms: crate::custom_uniforms::CustomUniforms,
    /// `image.*` declarations (custom images), bound at
    /// [`crate::host::IMAGE_BINDING`] + their index.
    pub images: Vec<crate::images::ImageSpec>,
}

impl Default for PackPlan {
    fn default() -> Self {
        Self {
            programs: BTreeMap::new(),
            draw_buffers: BTreeMap::new(),
            buffers: [BufferSpec::default(); 16],
            mipmaps: BTreeMap::new(),
            shadow: ShadowSpec::default(),
            custom_textures: Vec::new(),
            noise_resolution: 256,
            ambient_occlusion_level: 1.0,
            wetness_half_life: 600.0,
            dryness_half_life: 200.0,
            eye_brightness_half_life: 10.0,
            properties: PackProperties::default(),
            options: Vec::new(),
            program_states: BTreeMap::new(),
            outputs: BTreeMap::new(),
            custom_uniforms: Default::default(),
            images: Vec::new(),
        }
    }
}

impl PackPlan {
    /// The `shaders.properties` state of slot `kind`: the entry of the slot's
    /// own name, else that of the program file it runs (a fallback's), field
    /// by field.
    pub fn state(&self, kind: ProgramKind) -> ProgramState {
        let own = self.program_states.get(&kind.name());
        let file = self
            .programs
            .get(&kind)
            .and_then(|p| p.rsplit('/').next())
            .filter(|f| *f != kind.name())
            .and_then(|f| self.program_states.get(f));
        let mut state = file.cloned().unwrap_or_default();
        if let Some(own) = own {
            state.blend = own.blend.or(state.blend);
            state.buffer_blend.extend(own.buffer_blend.clone());
            state.alpha_test = own.alpha_test.or(state.alpha_test);
            state.scale = own.scale.or(state.scale);
            state.flip.extend(own.flip.clone());
        }
        state
    }

    /// The alpha test override of slot `kind` (`None`: the slot's default).
    pub fn alpha_test(&self, kind: ProgramKind) -> Option<AlphaTest> {
        self.state(kind).alpha_test
    }

    /// Where particles draw: the pack's choice, else `After` when the pack
    /// has a deferred pass and `Mixed` when it has none.
    pub fn particle_ordering(&self) -> ParticleOrdering {
        self.properties.particles.unwrap_or_else(|| {
            if self
                .programs
                .keys()
                .any(|k| matches!(k, ProgramKind::Deferred(_)))
            {
                ParticleOrdering::After
            } else {
                ParticleOrdering::Mixed
            }
        })
    }

    /// Whether the shadow pass runs: the pack has a shadow program and does
    /// not turn it off with `shadow.enabled=false`.
    pub fn shadow_enabled(&self) -> bool {
        self.properties.shadow_enabled != Some(false) && self.programs.keys().any(|k| k.is_shadow())
    }

    /// The full-screen passes in the order they run: prepare, deferred, composite, final.
    pub fn fullscreen_passes(&self) -> Vec<ProgramKind> {
        self.programs
            .keys()
            .copied()
            .filter(|k| k.is_fullscreen())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_documented_format_name() {
        for row in FORMATS {
            assert_eq!(BufferFormat::parse(row.0), Some(row.1), "{}", row.0);
            assert_eq!(row.1.name(), row.0);
        }
        assert_eq!(BufferFormat::parse("rgba16f"), Some(BufferFormat::Rgba16F));
        assert_eq!(
            BufferFormat::parse("GL_RGB10_A2"),
            Some(BufferFormat::Rgb10A2)
        );
        assert_eq!(BufferFormat::parse(" r32ui "), Some(BufferFormat::R32UI));
        assert_eq!(BufferFormat::parse("RGBA"), None);
        use StorageFormat as S;
        let f = |n: &str| BufferFormat::parse(n).unwrap();
        assert_eq!(f("RGB8").storage(), S::Rgba8Unorm);
        assert_eq!(f("RGBA16").storage(), S::Rgba16Float);
        assert_eq!(f("RG8_SNORM").storage(), S::Rg16Float);
        assert_eq!(f("RGB9_E5").storage(), S::Rgba16Float);
        assert_eq!(f("RGB565").storage(), S::Rgba8Unorm);
        assert_eq!(f("RGB16I").storage(), S::Rgba16Sint);
        assert_eq!(f("R8UI").storage(), S::R8Uint);
        assert_eq!(f("RGB10_A2UI").storage(), S::Rgb10a2Uint);
        assert_eq!(f("RGBA32F").bytes(), 16);
        assert_eq!(f("RGBA8").bytes(), 8);
        assert_eq!(f("R32UI").bytes(), 4);
        assert!(f("RGBA16I").is_integer() && !f("RGBA16F").is_integer());
        assert_eq!(f("RGBA8_SNORM").kind(), FormatKind::Snorm);
        assert_eq!(f("R11F_G11F_B10F").channels(), 3);
    }

    #[test]
    fn pass_state_values() {
        assert_eq!(
            Blend::parse("one ZERO GL_SRC_ALPHA one_minus_dst_alpha"),
            Some(Blend::On {
                src: BlendFactor::One,
                dst: BlendFactor::Zero,
                src_alpha: BlendFactor::SrcAlpha,
                dst_alpha: BlendFactor::OneMinusDstAlpha,
            })
        );
        assert_eq!(Blend::parse("off"), Some(Blend::Off));
        assert_eq!(Blend::parse("ONE ZERO"), None);
        assert_eq!(
            AlphaTest::parse("GEQUAL 0.25f"),
            Some(AlphaTest::Test(AlphaFunc::GreaterEqual, 0.25))
        );
        assert_eq!(AlphaTest::parse("OFF"), Some(AlphaTest::Off));
        assert_eq!(AlphaTest::parse("BIGGER 1"), None);
        assert_eq!(BufferSize::Pixels(0, 9).pixels(10, 10), (1, 9));
    }

    #[test]
    fn a_slot_takes_its_own_state_over_its_file_s() {
        let mut plan = PackPlan::default();
        plan.programs.insert(
            ProgramKind::Clouds,
            "shaders/world0/gbuffers_textured".into(),
        );
        plan.program_states.insert(
            "gbuffers_textured".into(),
            ProgramState {
                blend: Some(Blend::Off),
                alpha_test: Some(AlphaTest::Off),
                ..ProgramState::default()
            },
        );
        plan.program_states.insert(
            "gbuffers_clouds".into(),
            ProgramState {
                alpha_test: Some(AlphaTest::Test(AlphaFunc::Greater, 0.5)),
                ..ProgramState::default()
            },
        );
        let s = plan.state(ProgramKind::Clouds);
        assert_eq!(s.blend, Some(Blend::Off));
        assert_eq!(s.alpha_test, Some(AlphaTest::Test(AlphaFunc::Greater, 0.5)));
        assert_eq!(plan.state(ProgramKind::Water), ProgramState::default());
    }
}
