//! What a pack asks of the renderer, read from its sources and properties:
//! programs present, render targets, the shadow map, custom textures and
//! settings. The client builds its passes from a [`PackPlan`]; nothing here
//! touches wgpu.
//!
//! CONTRACT (types frozen for the parallel work; bodies are filled by A1).

use std::collections::{BTreeMap, BTreeSet};

use crate::ProgramKind;

/// A colour target's format, as a pack names it (`RGBA16`, `R11F_G11F_B10F` …).
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
}

impl BufferFormat {
    /// The pack's name for it (`RGBA16`, `R11F_G11F_B10F`, `RGB10_A2` …), case-sensitive.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "R8" => Self::R8,
            "RG8" => Self::Rg8,
            "RGB8" => Self::Rgb8,
            "RGBA8" => Self::Rgba8,
            "R16" => Self::R16,
            "RG16" => Self::Rg16,
            "RGB16" => Self::Rgb16,
            "RGBA16" => Self::Rgba16,
            "R16F" => Self::R16F,
            "RG16F" => Self::Rg16F,
            "RGB16F" => Self::Rgb16F,
            "RGBA16F" => Self::Rgba16F,
            "R32F" => Self::R32F,
            "RG32F" => Self::Rg32F,
            "RGB32F" => Self::Rgb32F,
            "RGBA32F" => Self::Rgba32F,
            "R11F_G11F_B10F" => Self::R11G11B10F,
            "RGB10_A2" => Self::Rgb10A2,
            _ => return None,
        })
    }

    /// Channels the pack sees; reads of the missing ones return 0, alpha 1.
    pub fn channels(self) -> u8 {
        match self {
            Self::R8 | Self::R16 | Self::R16F | Self::R32F => 1,
            Self::Rg8 | Self::Rg16 | Self::Rg16F | Self::Rg32F => 2,
            Self::Rgb8 | Self::Rgb16 | Self::Rgb16F | Self::Rgb32F | Self::R11G11B10F => 3,
            Self::Rgba8 | Self::Rgba16 | Self::Rgba16F | Self::Rgba32F | Self::Rgb10A2 => 4,
        }
    }

    /// Normalized (0…1) formats: writes are clamped to 0…1, NaN becomes 0.
    /// The client may store them in a float format (16-bit unorm is not a
    /// render target without adapter-specific features).
    pub fn is_unorm(self) -> bool {
        matches!(
            self,
            Self::R8
                | Self::Rg8
                | Self::Rgb8
                | Self::Rgba8
                | Self::R16
                | Self::Rg16
                | Self::Rgb16
                | Self::Rgba16
                | Self::Rgb10A2
        )
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
}

impl Default for BufferSpec {
    fn default() -> Self {
        Self {
            format: BufferFormat::Rgba8,
            clear: true,
            clear_color: None,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextureStage {
    /// gbuffers programs (and, as the spec says, shadow programs).
    Gbuffers,
    Shadow,
    Deferred,
    Composite,
}

/// A PNG the pack binds in place of one of its samplers
/// (`texture.gbuffers.gaux1=textures/ripples.png`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomTexture {
    pub stage: TextureStage,
    /// The sampler it replaces, canonical (`colortex4`, not `gaux1`).
    pub sampler: String,
    /// Pack path of the image (`shaders/textures/ripples.png`).
    pub path: String,
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
        }
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
        }
    }
}

impl PackPlan {
    /// The full-screen passes in the order they run: deferred, composite, final.
    pub fn fullscreen_passes(&self) -> Vec<ProgramKind> {
        self.programs
            .keys()
            .copied()
            .filter(|k| k.is_fullscreen())
            .collect()
    }
}
