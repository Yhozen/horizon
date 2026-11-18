//! Texture bindings and the host sampling functions.
//!
//! Every pack sampler resolves, per sampling operation, to one of: Horizon's
//! block textures in group 0 (terrain sprite arrays, the entity atlas, the
//! lightmap), a constant (`normals`, `specular`), or a group 1 texture with
//! its [`SamplerKind`] (format-notes §1). Filtering WebGPU cannot do in
//! hardware (depth textures and 32-bit float formats are not filterable) is
//! done in the shader from `texelFetch`, so those textures take the
//! non-filtering [`SamplerKind::Point`].

use super::*;
use crate::plan::{BufferFormat, TextureStage};
use crate::{SampleKind, SamplerKind, TexDim, TextureBinding, TextureSource};

/// Group 1 binding of a pack sampler's texture (`colortex0` …), if supported:
/// `colortex0..15` → 2..17, `depthtex0..2` → 18..20, `shadowtex0..1` → 21..22,
/// `shadowcolor0..1` → 23..24, `noisetex` → 25, `normals` → 26,
/// `specular` → 27, and OptiFine's old aliases. Hardware-compared reads of
/// `shadowtexN` bind at [`COMPARE_BINDING`] + N, its mipmapped copy at
/// [`MIPS_BINDING`] + N.
pub fn sampler_binding(name: &str) -> Option<u32> {
    let indexed = |prefix: &str, count: u32, base: u32| {
        let digits = name.strip_prefix(prefix)?;
        let n: u32 = digits.parse().ok()?;
        (n < count && n.to_string() == digits).then_some(base + n)
    };
    let alias = match name {
        "gcolor" => Some(2),
        "gdepth" => Some(3),
        "gnormal" => Some(4),
        "composite" => Some(5),
        "gaux1" => Some(6),
        "gaux2" => Some(7),
        "gaux3" => Some(8),
        "gaux4" => Some(9),
        "gdepthtex" => Some(18),
        "shadow" | "watershadow" => Some(21),
        "shadowcolor" => Some(23),
        "noisetex" => Some(25),
        "normals" => Some(26),
        "specular" => Some(27),
        _ => None,
    };
    alias
        .or_else(|| indexed("colortex", 16, 2))
        .or_else(|| indexed("depthtex", 3, 18))
        .or_else(|| indexed("shadowtex", 2, 21))
        .or_else(|| indexed("shadowcolor", 2, 23))
}

/// `shadowtexN` read with a hardware depth compare (`texture_depth_2d`).
pub const COMPARE_BINDING: u32 = 28;
/// `shadowtexN`'s `R32Float` copy with a full mip chain (`shadowtexNMipmap`).
pub const MIPS_BINDING: u32 = 30;

/// The host sampling operations the rewriter emits, as `<op>[<tag>]<sampler>`
/// (see [`sampling_call`]).
pub const SAMPLING_OPS: [&str; 10] = [
    "hz_sampleLod_",
    "hz_sampleGrad_",
    "hz_sampleBias_",
    "hz_sample_",
    "hz_fetch_",
    "hz_size_",
    "hz_shadowLod_",
    "hz_shadow_",
    "hz_gather_",
    "hz_queryLod_",
];

/// A parsed host sampling call: the operation, the sampler's declared
/// dimension and texel type, and its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SamplingCall<'a> {
    pub op: &'static str,
    pub dim: TexDim,
    /// `Float` for `sampler2D`, `sampler3D` and `sampler2DShadow`.
    pub sample: SampleKind,
    pub name: &'a str,
}

/// The dimension and texel type of a GLSL sampler type; `None` for the ones
/// Horizon does not bind (`samplerCube`, `sampler2DArray`, `sampler1D`…).
pub fn sampler_type(ty: &str) -> Option<(TexDim, SampleKind)> {
    Some(match ty {
        "sampler2D" | "sampler2DShadow" => (TexDim::D2, SampleKind::Float),
        "sampler3D" => (TexDim::D3, SampleKind::Float),
        "isampler2D" => (TexDim::D2, SampleKind::Sint),
        "usampler2D" => (TexDim::D2, SampleKind::Uint),
        "isampler3D" => (TexDim::D3, SampleKind::Sint),
        "usampler3D" => (TexDim::D3, SampleKind::Uint),
        _ => return None,
    })
}

/// The call's tag between the operation and the name: none for 2D float
/// samplers, else a tag that starts with a digit (so it cannot be part of a
/// sampler name): `3_`, `2i_`, `2u_`, `3i_`, `3u_`.
fn tag(dim: TexDim, sample: SampleKind) -> &'static str {
    match (dim, sample) {
        (TexDim::D2, SampleKind::Float | SampleKind::Depth) => "",
        (TexDim::D3, SampleKind::Float | SampleKind::Depth) => "3_",
        (TexDim::D2, SampleKind::Sint) => "2i_",
        (TexDim::D2, SampleKind::Uint) => "2u_",
        (TexDim::D3, SampleKind::Sint) => "3i_",
        (TexDim::D3, SampleKind::Uint) => "3u_",
    }
}

/// `hz_fetch_` on a `usampler3D ids` → `hz_fetch_3u_ids`.
pub fn sampling_call(op: &str, dim: TexDim, sample: SampleKind, name: &str) -> String {
    format!("{op}{}{name}", tag(dim, sample))
}

/// The inverse of [`sampling_call`].
pub fn parse_sampling_call(call: &str) -> Option<SamplingCall<'_>> {
    let (op, rest) = SAMPLING_OPS
        .iter()
        .find_map(|op| call.strip_prefix(op).map(|rest| (*op, rest)))?;
    let tagged = [
        ("3_", TexDim::D3, SampleKind::Float),
        ("2i_", TexDim::D2, SampleKind::Sint),
        ("2u_", TexDim::D2, SampleKind::Uint),
        ("3i_", TexDim::D3, SampleKind::Sint),
        ("3u_", TexDim::D3, SampleKind::Uint),
    ];
    let (dim, sample, name) = tagged
        .iter()
        .find_map(|(t, d, k)| rest.strip_prefix(t).map(|n| (*d, *k, n)))
        .unwrap_or((TexDim::D2, SampleKind::Float, rest));
    Some(SamplingCall {
        op,
        dim,
        sample,
        name,
    })
}

/// `hz_sampleLod_colortex0` → `("hz_sampleLod_", "colortex0")` (the tag dropped).
pub fn split_sampling_call(call: &str) -> Option<(&'static str, &str)> {
    parse_sampling_call(call).map(|c| (c.op, c.name))
}

/// The canonical name of a sampler (`gaux1` → `colortex4`, `tex` → `gtexture`).
/// `shadow` is `shadowtex1` when the program also declares `watershadow`.
pub(super) fn canonical(name: &str, watershadow: bool) -> Option<String> {
    Some(
        match name {
            // `texture` may reach us renamed, as it is a GLSL 450 built-in.
            "gtexture" | "texture" | "hz_u_texture" | "tex" => "gtexture",
            "lightmap" => "lightmap",
            "shadow" if watershadow => "shadowtex1",
            "watershadow" | "shadow" => "shadowtex0",
            "gdepthtex" => "depthtex0",
            "shadowcolor" => "shadowcolor0",
            "noisetex" => "noisetex",
            "normals" => "normals",
            "specular" => "specular",
            _ => {
                let binding = sampler_binding(name)?;
                return Some(match binding {
                    2..=17 => format!("colortex{}", binding - 2),
                    18..=20 => format!("depthtex{}", binding - 18),
                    21..=22 => format!("shadowtex{}", binding - 21),
                    _ => format!("shadowcolor{}", binding - 23),
                });
            }
        }
        .to_string(),
    )
}

/// A group 1 texture as one stage reads it.
#[derive(Clone, Debug)]
pub(super) struct Entry {
    binding: u32,
    source: TextureSource,
    kind: SamplerKind,
    dim: TexDim,
    /// The bound texture's texels (`Depth` only for compare reads).
    sample: SampleKind,
    /// Reads `1 - stored` (reversed Z) as `(d, d, d, 1)`.
    depth: bool,
    /// Sampled through `sampler2DShadow` (a `texture_depth_2d`).
    compare: bool,
    /// Mip levels are sampled (the texture has a full chain).
    mips: bool,
    /// Linear filtering done in the shader (the format is not filterable).
    manual: bool,
    /// Channels the format has; missing ones read 0, alpha 1.
    channels: u8,
    /// An 8-bit normalized format: reads are snapped to exact multiples of
    /// 1/255, as packs compare `value * 255.0 == id` (some drivers decode
    /// unorm texels with a reciprocal multiply, a bit off).
    snap8: bool,
}

impl Entry {
    fn new(binding: u32, source: TextureSource, kind: SamplerKind) -> Self {
        Self {
            binding,
            source,
            kind,
            dim: TexDim::D2,
            sample: SampleKind::Float,
            depth: false,
            compare: false,
            mips: false,
            manual: false,
            channels: 4,
            snap8: false,
        }
    }

    fn repeat(&self) -> bool {
        matches!(
            self.kind,
            SamplerKind::LinearRepeat | SamplerKind::PointRepeat
        )
    }

    fn filtered(&self) -> bool {
        !matches!(self.kind, SamplerKind::Point | SamplerKind::PointRepeat)
    }
}

/// Where a pack sampler's texture comes from.
pub(super) enum Source {
    Sprites,
    Atlas,
    Lightmap,
    Constant(&'static str),
    /// Nothing provides the sampler: a 1×1 texel of (0, 0, 0, 1), as GL reads
    /// an incomplete texture, without spending a binding.
    Default,
    Pack(Entry),
}

fn is_8bit(format: BufferFormat) -> bool {
    matches!(
        format,
        BufferFormat::R8 | BufferFormat::Rg8 | BufferFormat::Rgb8 | BufferFormat::Rgba8
    )
}

/// Whether WebGPU can filter the client's texture for this format.
fn filterable(format: BufferFormat) -> bool {
    buffer_sample(format) == SampleKind::Float
        && !matches!(
            format,
            BufferFormat::R32F | BufferFormat::Rg32F | BufferFormat::Rgb32F | BufferFormat::Rgba32F
        )
}

/// The texel type of a buffer format: integer formats (`…UI`, `…I`, by their
/// names) read as integers.
fn buffer_sample(format: BufferFormat) -> SampleKind {
    let name = format!("{format:?}").to_ascii_uppercase();
    if name.ends_with("UI") {
        SampleKind::Uint
    } else if name.ends_with('I') {
        SampleKind::Sint
    } else {
        SampleKind::Float
    }
}

/// Whether a `texture.<stage>.<sampler>` override applies to this program.
fn custom_applies(stage: TextureStage, sampler: &str, spec: &ProgramSpec) -> bool {
    let kind = spec.kind;
    let applies = match stage {
        TextureStage::Gbuffers => !kind.is_fullscreen(),
        TextureStage::Shadow => kind.is_shadow(),
        TextureStage::Deferred => matches!(kind, ProgramKind::Deferred(_)),
        TextureStage::Composite => matches!(kind, ProgramKind::Composite(_) | ProgramKind::Final),
        TextureStage::All => true,
        // Passes Horizon does not run.
        TextureStage::Prepare
        | TextureStage::Shadowcomp
        | TextureStage::Begin
        | TextureStage::Setup => false,
    };
    if !applies {
        return false;
    }
    // In deferred and composite, a buffer an earlier pass of the stage wrote is the buffer again.
    let Some(n) = sampler
        .strip_prefix("colortex")
        .and_then(|n| n.parse::<u8>().ok())
    else {
        return true;
    };
    let earlier = |k: &ProgramKind| match (kind, k) {
        (ProgramKind::Deferred(a), ProgramKind::Deferred(b))
        | (ProgramKind::Composite(a), ProgramKind::Composite(b)) => b < &a,
        (ProgramKind::Final, ProgramKind::Composite(_)) => true,
        _ => false,
    };
    !spec
        .plan
        .draw_buffers
        .iter()
        .any(|(k, buffers)| earlier(k) && buffers.contains(&n))
}

/// Group 1 binding of custom texture `index` bound under its own name.
pub const CUSTOM_BINDING: u32 = 64;

/// The plan's custom texture for `sampler` in this program, as an entry at `binding`
/// (`None`: at [`CUSTOM_BINDING`] + its index).
/// Only a texture of the declared dimension: GL binds 2D and 3D textures to
/// separate targets of a unit, so a 3D override leaves the 2D buffer in place.
fn custom_entry(
    sampler: &str,
    dim: TexDim,
    binding: Option<u32>,
    spec: &ProgramSpec,
) -> Option<Entry> {
    let (i, c) = spec
        .plan
        .custom_textures
        .iter()
        .enumerate()
        .find(|(_, c)| {
            c.sampler == sampler && c.dim() == dim && custom_applies(c.stage, sampler, spec)
        })?;
    let binding = binding.unwrap_or(CUSTOM_BINDING + i as u32);
    let float = c.sample() == SampleKind::Float;
    Some(Entry {
        dim: c.dim(),
        sample: c.sample(),
        // A blurred 2D float texture the hardware cannot filter: bilinear in the shader.
        manual: float && c.dim() == TexDim::D2 && c.blur && !c.filterable(),
        ..Entry::new(binding, TextureSource::Custom(i), c.sampler())
    })
}

/// What `name` (declared with dimension `dim`) reads for the operation `op`
/// in this program. `depth2_is_1`: `depthtex2` reads `depthtex1` (they hold
/// the same without a hand program).
pub(super) fn resolve(
    name: &str,
    dim: TexDim,
    op: &str,
    spec: &ProgramSpec,
    watershadow: bool,
    depth2_is_1: bool,
) -> Result<Source, String> {
    let fullscreen = spec.geometry == Geometry::Fullscreen;
    let Some(canon) = canonical(name, watershadow) else {
        // The pack's own name: a custom texture, else the 1×1 default.
        return Ok(custom_entry(name, dim, None, spec).map_or(Source::Default, Source::Pack));
    };
    let canon = match canon.as_str() {
        "gtexture" | "lightmap" if fullscreen => "colortex0".to_string(),
        "gtexture" if spec.geometry == Geometry::Terrain => return Ok(Source::Sprites),
        "gtexture" => return Ok(Source::Atlas),
        "lightmap" => return Ok(Source::Lightmap),
        "depthtex2" if depth2_is_1 => "depthtex1".to_string(),
        _ => canon,
    };
    let base = sampler_binding(&canon).ok_or_else(|| format!("unknown sampler {name}"))?;
    if let Some(entry) = custom_entry(&canon, dim, Some(base), spec) {
        return Ok(Source::Pack(entry));
    }
    if dim == TexDim::D3 {
        // Every built-in texture is 2D: nothing is bound to the unit's 3D target.
        return Ok(Source::Default);
    }
    let entry = Entry::new;
    let shadow = spec.plan.shadow;
    let index = |prefix: &str| {
        canon
            .strip_prefix(prefix)
            .and_then(|n| n.parse::<u8>().ok())
    };
    if let Some(n) = index("colortex") {
        let format = spec.plan.buffers[usize::from(n)].format;
        let sample = buffer_sample(format);
        // Mip sampling lasts from the first pass that regenerates the buffer's mips to the frame's end.
        let mips = fullscreen
            && sample == SampleKind::Float
            && spec
                .plan
                .mipmaps
                .iter()
                .any(|(k, set)| k.is_fullscreen() && *k <= spec.kind && set.contains(&n));
        let hardware = filterable(format);
        let kind = match (hardware, mips) {
            (false, _) => SamplerKind::Point,
            (true, true) => SamplerKind::Trilinear,
            (true, false) => SamplerKind::Linear,
        };
        return Ok(Source::Pack(Entry {
            mips,
            sample,
            manual: !hardware && sample == SampleKind::Float,
            channels: format.channels(),
            snap8: is_8bit(format),
            ..entry(base, TextureSource::Buffer(n), kind)
        }));
    }
    if let Some(n) = index("depthtex") {
        return Ok(Source::Pack(Entry {
            depth: true,
            ..entry(base, TextureSource::Depth(n), SamplerKind::Point)
        }));
    }
    if let Some(n) = index("shadowtex") {
        let i = usize::from(n);
        let compared = matches!(op, "hz_shadow_" | "hz_shadowLod_");
        if compared && shadow.hardware_filtering[i] {
            return Ok(Source::Pack(Entry {
                depth: true,
                compare: true,
                sample: SampleKind::Depth,
                ..entry(
                    COMPARE_BINDING + u32::from(n),
                    TextureSource::ShadowTex(n),
                    SamplerKind::Compare,
                )
            }));
        }
        let linear = !shadow.tex_nearest[i];
        if shadow.tex_mipmap[i] {
            return Ok(Source::Pack(Entry {
                depth: true,
                mips: true,
                manual: linear,
                ..entry(
                    MIPS_BINDING + u32::from(n),
                    TextureSource::ShadowTexMips(n),
                    SamplerKind::Point,
                )
            }));
        }
        return Ok(Source::Pack(Entry {
            depth: true,
            manual: linear,
            ..entry(base, TextureSource::ShadowTex(n), SamplerKind::Point)
        }));
    }
    if let Some(n) = index("shadowcolor") {
        // A fixed linear sampler: shadowcolor mips are generated but never sampled.
        let format = shadow.color_format[usize::from(n)];
        let hardware = filterable(format);
        let kind = if hardware {
            SamplerKind::Linear
        } else {
            SamplerKind::Point
        };
        let sample = buffer_sample(format);
        return Ok(Source::Pack(Entry {
            sample,
            manual: !hardware && sample == SampleKind::Float,
            channels: format.channels(),
            snap8: is_8bit(format),
            ..entry(base, TextureSource::ShadowColor(n), kind)
        }));
    }
    match canon.as_str() {
        "noisetex" => Ok(Source::Pack(entry(
            base,
            TextureSource::Noise,
            SamplerKind::LinearRepeat,
        ))),
        // The 1×1 defaults: sampling them is a constant.
        "normals" => Ok(Source::Constant(
            "vec4(127.0 / 255.0, 127.0 / 255.0, 1.0, 1.0)",
        )),
        "specular" => Ok(Source::Constant("vec4(0.0)")),
        _ => Err(format!("unknown sampler {name}")),
    }
}

/// The GLSL identifier of a sampler kind's group 1 sampler.
fn sampler_name(kind: SamplerKind) -> String {
    format!("hz_s_{kind:?}").to_lowercase()
}

/// A stage's sampling: declarations (by `(set, binding)`), functions, and the
/// group 1 textures it reads.
pub(super) struct Sampling {
    pub(super) decls: String,
    pub(super) functions: String,
    pub(super) textures: Vec<TextureBinding>,
}

/// Whether `depthtex2` reads go to `depthtex1` in this stage: the pack has no
/// hand program (so the two hold the same) and the stage reads both, which
/// saves a binding.
fn depth2_is_1(calls: &BTreeSet<String>, spec: &ProgramSpec, watershadow: bool) -> bool {
    let reads = |want: &str| {
        calls.iter().any(|c| {
            split_sampling_call(c)
                .and_then(|(_, n)| canonical(n, watershadow))
                .is_some_and(|n| n == want)
        })
    };
    !spec.plan.programs.contains_key(&ProgramKind::Hand) && reads("depthtex1") && reads("depthtex2")
}

/// Textures of Horizon's own group 0 that every stage of this geometry's
/// layout counts: terrain's sprite arrays and lightmap, the entity atlas and
/// lightmap.
fn horizon_textures(spec: &ProgramSpec) -> u32 {
    match spec.geometry {
        Geometry::Terrain => spec.host.sprite_arrays + 1,
        Geometry::Entity => 2,
        Geometry::Fullscreen => 0,
    }
}

/// Things worth a line in the pack report about a stage's sampling: samplers
/// nothing provides.
pub(super) fn sampling_notes(
    calls: &BTreeSet<String>,
    spec: &ProgramSpec,
    watershadow: bool,
) -> BTreeSet<String> {
    let alias = depth2_is_1(calls, spec, watershadow);
    calls
        .iter()
        .filter_map(|c| parse_sampling_call(c))
        .filter(|c| {
            matches!(
                resolve(c.name, c.dim, c.op, spec, watershadow, alias),
                Ok(Source::Default)
            )
        })
        .map(|c| {
            format!(
                "sampler `{}` has no texture (not a built-in name, no customTexture): reads a 1×1 (0, 0, 0, 1)",
                c.name
            )
        })
        .collect()
}

/// The WGSL-side GLSL texture type of an entry.
fn texture_type(e: &Entry) -> &'static str {
    match (e.dim, e.sample) {
        (TexDim::D2, SampleKind::Float | SampleKind::Depth) => "texture2D",
        (TexDim::D3, SampleKind::Float | SampleKind::Depth) => "texture3D",
        (TexDim::D2, SampleKind::Sint) => "itexture2D",
        (TexDim::D2, SampleKind::Uint) => "utexture2D",
        (TexDim::D3, SampleKind::Sint) => "itexture3D",
        (TexDim::D3, SampleKind::Uint) => "utexture3D",
    }
}

/// Bindings and functions for a stage's sampling calls.
pub(super) fn sampling(
    calls: &BTreeSet<String>,
    stage: Stage,
    spec: &ProgramSpec,
    extras: &Extras,
) -> Result<Sampling, String> {
    let watershadow = extras.watershadow;
    let alias = depth2_is_1(calls, spec, watershadow);
    let mut bindings = BTreeMap::new();
    let mut functions = String::new();
    let mut entries: BTreeMap<u32, Entry> = BTreeMap::new();
    let mut names: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    let fragment = stage == Stage::Fragment;
    for call in calls {
        let c = parse_sampling_call(call).ok_or_else(|| format!("unknown sampling call {call}"))?;
        let (op, name) = (c.op, c.name);
        let unsupported = || format!("{call}: unsupported on {name}");
        let declared_2d_float = c.dim == TexDim::D2 && c.sample == SampleKind::Float;
        let group0 = |what: &str| {
            format!(
                "{name} is {what}, a 2D float texture, but the pack declares it {}",
                declared_type(c.dim, c.sample)
            )
        };
        let (signature, body) = match resolve(name, c.dim, op, spec, watershadow, alias)? {
            Source::Sprites if !declared_2d_float => return Err(group0("the block atlas")),
            Source::Atlas | Source::Lightmap if !declared_2d_float => {
                return Err(group0("Horizon's own texture"));
            }
            Source::Sprites => {
                sprite_function(call, op, stage, spec.host, &mut bindings, &mut functions)
                    .ok_or_else(unsupported)?
            }
            Source::Atlas => {
                bindings.insert((0, 1), "uniform texture2D hz_atlas;".to_string());
                bindings.insert((0, 2), "uniform sampler hz_atlas_sampler;".to_string());
                let t = "sampler2D(hz_atlas, hz_atlas_sampler)";
                let (signature, body) = plain_function(call, op, t, fragment, &mut functions)
                    .ok_or_else(unsupported)?;
                if fragment && extras.uv_wrap {
                    if !functions.contains("hz_atlas_uv") {
                        // entity.wgsl's per-fragment wrap: (origin, size) per axis, size 0 = none.
                        functions += "vec2 hz_atlas_uv(vec2 uv) {\n    vec2 o = hz_uvWrap.xy;\n    \
                            vec2 s = hz_uvWrap.zw;\n    \
                            return vec2(s.x > 0.0 ? o.x + fract(uv.x) * s.x : uv.x, \
                            s.y > 0.0 ? o.y + fract(uv.y) * s.y : uv.y);\n}\n";
                    }
                    let wrapped = format!("{t}, hz_atlas_uv(uv)");
                    (signature, body.replace(&format!("{t}, uv"), &wrapped))
                } else {
                    (signature, body)
                }
            }
            Source::Lightmap => {
                bindings.insert((0, 3), "uniform texture2D hz_lightmap;".to_string());
                bindings.insert((0, 4), "uniform sampler hz_lightmap_sampler;".to_string());
                plain_function(
                    call,
                    op,
                    "sampler2D(hz_lightmap, hz_lightmap_sampler)",
                    false,
                    &mut functions,
                )
                .ok_or_else(unsupported)?
            }
            Source::Constant(value) => {
                constant_function(&c, call, value).ok_or_else(unsupported)?
            }
            Source::Default => {
                constant_function(&c, call, "vec4(0.0, 0.0, 0.0, 1.0)").ok_or_else(unsupported)?
            }
            Source::Pack(entry) => {
                let b = entry.binding;
                if entry.dim != c.dim {
                    return Err(format!(
                        "{name} is a {} texture, but the pack declares it {}",
                        if entry.dim == TexDim::D3 { "3D" } else { "2D" },
                        declared_type(c.dim, c.sample)
                    ));
                }
                names.entry(b).or_default().insert(name.to_string());
                match entries.entry(b) {
                    std::collections::btree_map::Entry::Occupied(old) => {
                        let old = old.get();
                        if old.source != entry.source || old.kind != entry.kind {
                            return Err(format!("{name}: binding {b} read two ways"));
                        }
                    }
                    std::collections::btree_map::Entry::Vacant(slot) => {
                        // Compare reads become a `texture_depth_2d` through their `sampler2DShadow` use.
                        bindings
                            .insert((1, b), format!("uniform {} hz_t{b};", texture_type(&entry)));
                        let sampler = if entry.compare {
                            "samplerShadow"
                        } else {
                            "sampler"
                        };
                        bindings.insert(
                            (1, entry.kind.binding()),
                            format!("uniform {sampler} {};", sampler_name(entry.kind)),
                        );
                        if !entry.compare {
                            addressing(&mut functions);
                            functions += &entry_helpers(&entry);
                        }
                        slot.insert(entry.clone());
                    }
                }
                pack_function(&c, call, &entry, fragment).ok_or_else(unsupported)?
            }
        };
        writeln!(functions, "{signature} {{\n    {body}\n}}").unwrap();
        // GLSL converts an `int` level or bias implicitly; naga needs the overload.
        if let Some(head) = signature
            .strip_suffix(", float lod)")
            .or_else(|| signature.strip_suffix(", float bias)"))
        {
            let ret = if signature.starts_with("void") {
                ""
            } else {
                "return "
            };
            let arg = if signature.ends_with("lod)") {
                "lod"
            } else {
                "bias"
            };
            let first = head.rsplit_once('(').map_or("", |(_, a)| a);
            let first = first.split_whitespace().last().unwrap_or("uv");
            writeln!(
                functions,
                "{head}, int {arg}) {{\n    {ret}{call}({first}, float({arg}));\n}}"
            )
            .unwrap();
        }
    }
    let own = horizon_textures(spec);
    let total = entries.len() as u32 + own;
    if total > spec.host.max_textures {
        let list: Vec<String> = names
            .values()
            .map(|n| n.iter().cloned().collect::<Vec<_>>().join("/"))
            .collect();
        let stage = if fragment { "fragment" } else { "vertex" };
        return Err(format!(
            "{total} textures in the {stage} stage, the device allows {}: {} and {own} of Horizon's",
            spec.host.max_textures,
            list.join(", ")
        ));
    }
    let mut decls = String::new();
    for ((set, binding), decl) in bindings {
        writeln!(decls, "layout(set = {set}, binding = {binding}) {decl}").unwrap();
    }
    let textures = entries
        .into_values()
        .map(|e| TextureBinding {
            binding: e.binding,
            source: e.source,
            sampler: e.kind,
            dimension: e.dim,
            sample: e.sample,
            vertex: !fragment,
            fragment,
        })
        .collect();
    Ok(Sampling {
        decls,
        functions,
        textures,
    })
}

/// The GLSL sampler type a call was declared with, for messages.
fn declared_type(dim: TexDim, sample: SampleKind) -> &'static str {
    match (dim, sample) {
        (TexDim::D2, SampleKind::Sint) => "isampler2D",
        (TexDim::D2, SampleKind::Uint) => "usampler2D",
        (TexDim::D3, SampleKind::Sint) => "isampler3D",
        (TexDim::D3, SampleKind::Uint) => "usampler3D",
        (TexDim::D3, _) => "sampler3D",
        (TexDim::D2, _) => "sampler2D",
    }
}

/// The combined sampler expression of a group 1 entry.
fn combined(e: &Entry) -> String {
    let name = sampler_name(e.kind);
    let ty = match (e.compare, e.dim, e.sample) {
        (true, _, _) => "sampler2DShadow",
        (_, TexDim::D2, SampleKind::Sint) => "isampler2D",
        (_, TexDim::D2, SampleKind::Uint) => "usampler2D",
        (_, TexDim::D3, SampleKind::Sint) => "isampler3D",
        (_, TexDim::D3, SampleKind::Uint) => "usampler3D",
        (_, TexDim::D3, _) => "sampler3D",
        (_, TexDim::D2, _) => "sampler2D",
    };
    format!("{ty}(hz_t{}, {name})", e.binding)
}

/// GLSL vector types of a sampler: (texel, float coordinate, integer coordinate).
fn vector_types(dim: TexDim, sample: SampleKind) -> (&'static str, &'static str, &'static str) {
    let texel = match sample {
        SampleKind::Sint => "ivec4",
        SampleKind::Uint => "uvec4",
        _ => "vec4",
    };
    match dim {
        TexDim::D2 => (texel, "vec2", "ivec2"),
        TexDim::D3 => (texel, "vec3", "ivec3"),
    }
}

/// `expr` (texels of `from`) as the texel type of `to`.
fn convert(expr: String, from: SampleKind, to: SampleKind) -> String {
    let int = |k| matches!(k, SampleKind::Sint | SampleKind::Uint);
    if from == to || (!int(from) && !int(to)) {
        expr
    } else {
        format!("{}({expr})", vector_types(TexDim::D2, to).0)
    }
}

/// The signature of a host sampling function, by its declared sampler type.
fn signature(c: &SamplingCall, call: &str) -> Option<String> {
    let (r, v, i) = vector_types(c.dim, c.sample);
    Some(match c.op {
        "hz_sample_" => format!("{r} {call}({v} uv)"),
        "hz_sampleLod_" => format!("{r} {call}({v} uv, float lod)"),
        "hz_sampleBias_" => format!("{r} {call}({v} uv, float bias)"),
        "hz_sampleGrad_" => format!("{r} {call}({v} uv, {v} dx, {v} dy)"),
        "hz_fetch_" => format!("{r} {call}({i} p, int lod)"),
        "hz_size_" => format!("{i} {call}(int lod)"),
        "hz_gather_" if c.dim == TexDim::D2 => format!("{r} {call}(vec2 uv, int comp)"),
        "hz_queryLod_" => format!("vec2 {call}({v} uv)"),
        "hz_shadow_" if c.dim == TexDim::D2 => format!("vec4 {call}(vec3 p)"),
        "hz_shadowLod_" if c.dim == TexDim::D2 => format!("vec4 {call}(vec3 p, float lod)"),
        _ => return None,
    })
}

/// `textureGather` from four texels at level 0: `texel(p)` reads the texel at
/// `ivec2 p`, in range. GL's order: (i0, j1), (i1, j1), (i1, j0), (i0, j0).
fn gather_body(ty: &str, size: &str, wrap: &str, texel: &dyn Fn(&str) -> String) -> String {
    format!(
        "ivec2 size = max({size}, ivec2(1));\n    vec2 f = uv * vec2(size) - 0.5;\n    \
         ivec2 i = ivec2(floor(f));\n    \
         ivec2 a = {wrap}(i, size);\n    ivec2 b = {wrap}(i + ivec2(1), size);\n    \
         {ty} t00 = {};\n    {ty} t10 = {};\n    {ty} t01 = {};\n    {ty} t11 = {};\n    \
         return {ty}(t01[comp], t11[comp], t10[comp], t00[comp]);",
        texel("a"),
        texel("ivec2(b.x, a.y)"),
        texel("ivec2(a.x, b.y)"),
        texel("b"),
    )
}

/// Integer texel addressing: `hz_wrap(i, size)` (repeat) and `hz_clamp(i, size)`, 2D and 3D.
const ADDRESSING: &str = "ivec2 hz_wrap(ivec2 i, ivec2 s) { return ((i % s) + s) % s; }\n\
    ivec3 hz_wrap(ivec3 i, ivec3 s) { return ((i % s) + s) % s; }\n\
    ivec2 hz_clamp(ivec2 i, ivec2 s) { return clamp(i, ivec2(0), max(s - 1, ivec2(0))); }\n\
    ivec3 hz_clamp(ivec3 i, ivec3 s) { return clamp(i, ivec3(0), max(s - 1, ivec3(0))); }\n";

fn addressing(functions: &mut String) {
    if !functions.contains("ivec2 hz_wrap(") {
        functions.insert_str(0, ADDRESSING);
    }
}

/// `textureQueryLod` from derivatives: (the level read, the computed level).
/// `size` is the texture's level-0 size as a float vector, `top` its last level.
fn query_lod_body(fragment: bool, size: &str, top: &str) -> String {
    if !fragment {
        return "return vec2(0.0);".into();
    }
    format!(
        "float l = 0.5 * log2(max(dot(dFdx(uv) * {size}, dFdx(uv) * {size}), \
         dot(dFdy(uv) * {size}, dFdy(uv) * {size})));\n    \
         return vec2(clamp(l, 0.0, {top}), l);"
    )
}

/// Per entry: `hz_post_<b>` (what the pack sees of a texel), `hz_read_<b>`
/// (a filtered read at an explicit level), `hz_fetch_<b>` (one texel),
/// `hz_texel_<b>` (one level-0 texel, addressed as the sampler would) and,
/// for 2D float entries, `hz_lod_<b>` (the level for derivatives).
fn entry_helpers(e: &Entry) -> String {
    let b = e.binding;
    let t = combined(e);
    let (r, v, i) = vector_types(e.dim, e.sample);
    let wrap = if e.repeat() { "hz_wrap" } else { "hz_clamp" };
    let mut s = String::new();
    let snap = if e.snap8 {
        // A true division: the multiply by a reciprocal is what goes wrong.
        "c = round(c * 255.0) / 255.0;\n    "
    } else {
        ""
    };
    let post = if e.depth {
        "float d = 1.0 - c.x;\n    return vec4(d, d, d, 1.0);".to_string()
    } else {
        let one = if r == "vec4" { "1.0" } else { "1" };
        let zero = if r == "vec4" { "0.0" } else { "0" };
        match e.channels {
            1 => format!("return {r}(c.r, {zero}, {zero}, {one});"),
            2 => format!("return {r}(c.rg, {zero}, {one});"),
            3 => format!("return {r}(c.rgb, {one});"),
            _ => "return c;".into(),
        }
    };
    let snap = if r == "vec4" { snap } else { "" };
    writeln!(s, "{r} hz_post_{b}({r} c) {{\n    {snap}{post}\n}}").unwrap();
    writeln!(
        s,
        "{r} hz_fetch_{b}({i} p, int lod) {{\n    return hz_post_{b}(texelFetch({t}, p, lod));\n}}"
    )
    .unwrap();
    writeln!(
        s,
        "{r} hz_texel_{b}({i} p) {{\n    return hz_fetch_{b}({wrap}(p, textureSize({t}, 0)), 0);\n}}"
    )
    .unwrap();
    // The nearest texel at a level, as a Point sampler reads.
    writeln!(
        s,
        "{r} hz_nearest_{b}({v} uv, int level) {{\n    {i} size = max(textureSize({t}, level), {i}(1));\n    \
         return hz_fetch_{b}({wrap}({i}(floor(uv * {v}(size))), size), level);\n}}"
    )
    .unwrap();
    if r != "vec4" || e.dim == TexDim::D3 {
        // Integer textures: nearest only; 3D textures have no mips.
        let read = if r == "vec4" && e.filtered() {
            format!("hz_post_{b}(textureLod({t}, uv, 0.0))")
        } else if r == "vec4" {
            format!("hz_nearest_{b}(uv, 0)")
        } else {
            format!("hz_nearest_{b}(uv, int(lod))")
        };
        writeln!(
            s,
            "{r} hz_read_{b}({v} uv, float lod) {{\n    return {read};\n}}"
        )
        .unwrap();
        return s;
    }
    writeln!(
        s,
        "float hz_lod_{b}(vec2 dx, vec2 dy) {{\n    vec2 size = vec2(textureSize({t}, 0));\n    \
         vec2 x = dx * size;\n    vec2 y = dy * size;\n    \
         return 0.5 * log2(max(dot(x, x), dot(y, y)));\n}}"
    )
    .unwrap();
    if e.manual {
        // Bilinear from four texels, clamped to the edge (unfilterable formats).
        writeln!(
            s,
            "vec4 hz_bilinear_{b}(vec2 uv, int level) {{\n    \
             ivec2 size = textureSize({t}, level);\n    \
             vec2 p = uv * vec2(size) - 0.5;\n    vec2 f = fract(p);\n    \
             ivec2 i = ivec2(floor(p));\n    ivec2 hi = max(size - 1, ivec2(0));\n    \
             vec4 a = texelFetch({t}, clamp(i, ivec2(0), hi), level);\n    \
             vec4 b = texelFetch({t}, clamp(i + ivec2(1, 0), ivec2(0), hi), level);\n    \
             vec4 c = texelFetch({t}, clamp(i + ivec2(0, 1), ivec2(0), hi), level);\n    \
             vec4 d = texelFetch({t}, clamp(i + ivec2(1, 1), ivec2(0), hi), level);\n    \
             return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);\n}}"
        )
        .unwrap();
    }
    let read = match (e.manual, e.mips) {
        (false, false) => format!("textureLod({t}, uv, 0.0)"),
        (false, true) => format!("textureLod({t}, uv, max(lod, 0.0))"),
        (true, false) => format!("hz_bilinear_{b}(uv, 0)"),
        // Trilinear between the two nearest levels of the full chain.
        (true, true) => {
            writeln!(
                s,
                "vec4 hz_trilinear_{b}(vec2 uv, float lod) {{\n    \
                 ivec2 size = textureSize({t}, 0);\n    \
                 float top = floor(log2(float(max(max(size.x, size.y), 1))));\n    \
                 float l = lod > 0.0 ? min(lod, top) : 0.0;\n    int l0 = int(l);\n    \
                 return mix(hz_bilinear_{b}(uv, l0), hz_bilinear_{b}(uv, min(l0 + 1, int(top))), fract(l));\n}}"
            )
            .unwrap();
            format!("hz_trilinear_{b}(uv, lod)")
        }
    };
    writeln!(
        s,
        "vec4 hz_read_{b}(vec2 uv, float lod) {{\n    return hz_post_{b}({read});\n}}"
    )
    .unwrap();
    s
}

/// A sampling function on a group 1 entry.
fn pack_function(
    c: &SamplingCall,
    call: &str,
    e: &Entry,
    fragment: bool,
) -> Option<(String, String)> {
    let b = e.binding;
    let t = combined(e);
    let sig = signature(c, call)?;
    let op = c.op;
    if e.compare {
        // GL's LEQUAL on GL depth is GreaterEqual against `1 - ref` on Horizon's reversed depth.
        let compare = format!("return vec4(textureLod({t}, vec3(p.xy, 1.0 - p.z), 0.0));");
        return matches!(op, "hz_shadow_" | "hz_shadowLod_").then_some((sig, compare));
    }
    let float2 = e.dim == TexDim::D2 && e.sample == SampleKind::Float;
    // Implicit levels need derivatives, which only the fragment stage has.
    let derivatives = fragment && e.mips && float2;
    let ret = |expr: String| format!("return {};", convert(expr, e.sample, c.sample));
    let (_, v, _) = vector_types(e.dim, e.sample);
    let body = match op {
        "hz_sample_" if derivatives => {
            ret(format!("hz_read_{b}(uv, hz_lod_{b}(dFdx(uv), dFdy(uv)))"))
        }
        "hz_sampleBias_" if derivatives => ret(format!(
            "hz_read_{b}(uv, hz_lod_{b}(dFdx(uv), dFdy(uv)) + bias)"
        )),
        "hz_sample_" | "hz_sampleBias_" => ret(format!("hz_read_{b}(uv, 0.0)")),
        "hz_sampleLod_" => ret(format!("hz_read_{b}(uv, lod)")),
        "hz_sampleGrad_" if e.mips && float2 => ret(format!("hz_read_{b}(uv, hz_lod_{b}(dx, dy))")),
        "hz_sampleGrad_" => ret(format!("hz_read_{b}(uv, 0.0)")),
        "hz_fetch_" => ret(format!("hz_fetch_{b}(p, lod)")),
        "hz_size_" => format!("return textureSize({t}, lod);"),
        "hz_gather_" => {
            let (texel, ..) = vector_types(e.dim, e.sample);
            let body = gather_body(texel, &format!("textureSize({t}, 0)"), "hz_clamp", &|p| {
                format!("hz_texel_{b}({p})")
            });
            if texel == vector_types(c.dim, c.sample).0 {
                body
            } else {
                // The texels are converted once gathered.
                body.replacen(
                    "return ",
                    &format!("return {}(", vector_types(c.dim, c.sample).0),
                    1,
                )
                .replacen("[comp]);", "[comp]));", 1)
            }
        }
        "hz_queryLod_" => {
            let top = if e.mips {
                format!(
                    "floor(log2(float(max(max(textureSize({t}, 0).x, textureSize({t}, 0).y), 1))))"
                )
            } else {
                "0.0".into()
            };
            query_lod_body(fragment, &format!("{v}(textureSize({t}, 0))"), &top)
        }
        // shadow2D without hardware filtering: GL's LEQUAL on the read value.
        "hz_shadow_" if e.sample == SampleKind::Float => {
            format!("return vec4(step(p.z, hz_read_{b}(p.xy, 0.0).x));")
        }
        "hz_shadowLod_" if e.sample == SampleKind::Float => {
            format!("return vec4(step(p.z, hz_read_{b}(p.xy, lod).x));")
        }
        _ => return None,
    };
    Some((sig, body))
}

/// A sampling function on a group 0 texture the hardware filters as it is
/// configured (the entity atlas, the lightmap); `implicit`: the fragment
/// stage, where `texture()` takes its level from derivatives.
fn plain_function(
    call: &str,
    op: &str,
    t: &str,
    implicit: bool,
    functions: &mut String,
) -> Option<(String, String)> {
    let c = SamplingCall {
        op: SAMPLING_OPS.iter().copied().find(|o| *o == op)?,
        dim: TexDim::D2,
        sample: SampleKind::Float,
        name: "",
    };
    let sig = signature(&c, call)?;
    let body = match op {
        "hz_sample_" if implicit => format!("return texture({t}, uv);"),
        "hz_sampleBias_" if implicit => format!("return texture({t}, uv, bias);"),
        "hz_sample_" | "hz_sampleBias_" => format!("return textureLod({t}, uv, 0.0);"),
        "hz_sampleLod_" => format!("return textureLod({t}, uv, lod);"),
        "hz_sampleGrad_" => format!("return textureGrad({t}, uv, dx, dy);"),
        "hz_fetch_" => format!("return texelFetch({t}, p, lod);"),
        "hz_size_" => format!("return textureSize({t}, lod);"),
        "hz_gather_" => {
            addressing(functions);
            gather_body("vec4", &format!("textureSize({t}, 0)"), "hz_clamp", &|p| {
                format!("texelFetch({t}, {p}, 0)")
            })
        }
        "hz_queryLod_" => {
            let top = format!("float(textureQueryLevels({t}) - 1)");
            query_lod_body(implicit, &format!("vec2(textureSize({t}, 0))"), &top)
        }
        _ => return None,
    };
    Some((sig, body))
}

/// A sampling function on a 1×1 texture that reads `value` (a `vec4`
/// expression) everywhere.
fn constant_function(c: &SamplingCall, call: &str, value: &str) -> Option<(String, String)> {
    let sig = signature(c, call)?;
    let (r, _, i) = vector_types(c.dim, c.sample);
    let texel = convert(value.to_string(), SampleKind::Float, c.sample);
    let body = match c.op {
        "hz_size_" => format!("return {i}(1);"),
        "hz_queryLod_" => "return vec2(0.0);".into(),
        "hz_gather_" => format!("return {r}({texel}[comp]);"),
        // A depth of the texel's red channel, compared as GL's LEQUAL.
        "hz_shadow_" | "hz_shadowLod_" => format!("return vec4(step(p.z, {value}.x));"),
        _ => format!("return {texel};"),
    };
    Some((sig, body))
}

/// A sampling function on terrain's sprite arrays (group 0).
fn sprite_function(
    call: &str,
    op: &str,
    stage: Stage,
    p: &HostParams,
    bindings: &mut BTreeMap<(u32, u32), String>,
    functions: &mut String,
) -> Option<(String, String)> {
    let fragment = stage == Stage::Fragment;
    if fragment || op == "hz_size_" {
        bindings.insert((0, 1), "uniform texture2DArray hz_sprites0;".to_string());
        bindings.insert((0, 2), "uniform sampler hz_sprite_sampler;".to_string());
        for k in 1..p.sprite_arrays {
            bindings.insert((0, 4 + k), format!("uniform texture2DArray hz_sprites{k};"));
        }
    }
    let array = |k: u32| format!("sampler2DArray(hz_sprites{k}, hz_sprite_sampler)");
    let signature = |args: &str| format!("vec4 {call}({args})");
    let args = match op {
        "hz_sample_" => "vec2 uv",
        "hz_sampleLod_" => "vec2 uv, float lod",
        "hz_sampleBias_" => "vec2 uv, float bias",
        "hz_sampleGrad_" => "vec2 uv, vec2 dx, vec2 dy",
        "hz_fetch_" => "ivec2 p, int lod",
        "hz_gather_" => "vec2 uv, int comp",
        "hz_queryLod_" if !fragment => {
            return Some((format!("vec2 {call}(vec2 uv)"), "return vec2(0.0);".into()));
        }
        "hz_queryLod_" => {
            // The sprite's own size, as the pack's mip level of the atlas would follow it.
            let size = format!("vec2(textureSize({}, 0).xy / {})", array(0), p.sprite_grid);
            return Some((
                format!("vec2 {call}(vec2 uv)"),
                query_lod_body(true, &size, "4.0"),
            ));
        }
        "hz_size_" => {
            return Some((
                format!("ivec2 {call}(int lod)"),
                format!(
                    "return textureSize({}, lod).xy / {};",
                    array(0),
                    p.sprite_grid
                ),
            ));
        }
        _ => return None,
    };
    if !fragment {
        // No sprite per vertex: white, so the pack's colour maths still works.
        return Some((signature(args), "return vec4(1.0);".into()));
    }
    if !functions.contains("hz_sprite_lod") {
        *functions += &sprite_helpers(p, &array);
    }
    if matches!(op, "hz_fetch_" | "hz_gather_") && !functions.contains("hz_sprite_fetch") {
        addressing(functions);
        *functions += &sprite_fetch(p, &array);
    }
    if op == "hz_gather_" {
        let size = format!("textureSize({}, 0).xy / {}", array(0), p.sprite_grid);
        let body = gather_body("vec4", &size, "hz_wrap", &|q| {
            format!("hz_sprite_fetch({q}, 0)")
        });
        return Some((signature(args), body));
    }
    let body = match op {
        "hz_fetch_" => "return hz_sprite_fetch(p, lod);",
        "hz_sample_" => "return hz_sprite_grad(uv, dFdx(uv), dFdy(uv));",
        "hz_sampleLod_" => "return hz_sprite_lod(uv, lod);",
        "hz_sampleBias_" => {
            "float k = exp2(bias);\n    return hz_sprite_grad(uv, dFdx(uv) * k, dFdy(uv) * k);"
        }
        _ => "return hz_sprite_grad(uv, dx, dy);",
    };
    Some((signature(args), body.into()))
}

/// `hz_sprite_lod(uv, lod)` and `hz_sprite_grad(uv, dx, dy)`: the fragment's
/// sprite (`hz_sprite`), as `sprite()` in terrain.wgsl picks it.
fn sprite_helpers(p: &HostParams, array: &dyn Fn(u32) -> String) -> String {
    let grid = format!("{}.0", p.sprite_grid);
    if p.sprite_grid > 1 {
        // tiled_sprite(): Repeat addressing within the sprite's tile, kept off its far edge.
        return format!(
            "float hz_tile_size() {{ return float(textureSize({a}, 0).x) / {grid}; }}\n\
             vec4 hz_sprite_lod(vec2 uv, float lod) {{\n    \
             vec2 wrapped = min(uv - floor(uv), vec2(1.0 - 0.5 / hz_tile_size()));\n    \
             return textureLod({a}, vec3((vec2(hz_sprite.yz) + wrapped) / {grid}, float(hz_sprite.w)), lod);\n}}\n\
             vec4 hz_sprite_grad(vec2 uv, vec2 dx, vec2 dy) {{\n    \
             vec2 x = dx * hz_tile_size();\n    vec2 y = dy * hz_tile_size();\n    \
             return hz_sprite_lod(uv, 0.5 * log2(max(dot(x, x), dot(y, y))));\n}}\n",
            a = array(0)
        );
    }
    if p.sprite_arrays == 1 {
        return format!(
            "vec4 hz_sprite_lod(vec2 uv, float lod) {{\n    \
             return textureLod({a}, vec3(uv, float(hz_sprite.x & 0x7fffu)), lod);\n}}\n\
             vec4 hz_sprite_grad(vec2 uv, vec2 dx, vec2 dy) {{\n    \
             return textureGrad({a}, vec3(uv, float(hz_sprite.x & 0x7fffu)), dx, dy);\n}}\n",
            a = array(0)
        );
    }
    // The array is chosen per fragment; derivatives come from the caller, before the switch.
    let switch = |lookup: &dyn Fn(&str) -> String| {
        let mut s = format!(
            "uint layer = hz_sprite.x & 0x7fffu;\n    float l = float(layer % {}u);\n    vec4 c;\n    \
             switch (int(layer / {}u)) {{\n",
            p.sprite_split, p.sprite_split
        );
        for k in 0..p.sprite_arrays {
            let label = if k + 1 == p.sprite_arrays {
                "default:".to_string()
            } else {
                format!("case {k}:")
            };
            writeln!(s, "    {label} c = {}; break;", lookup(&array(k))).unwrap();
        }
        s + "    }\n    return c;"
    };
    format!(
        "vec4 hz_sprite_lod(vec2 uv, float lod) {{\n    {}\n}}\n\
         vec4 hz_sprite_grad(vec2 uv, vec2 dx, vec2 dy) {{\n    {}\n}}\n",
        switch(&|a| format!("textureLod({a}, vec3(uv, l), lod)")),
        switch(&|a| format!("textureGrad({a}, vec3(uv, l), dx, dy)")),
    )
}

/// `hz_sprite_fetch(p, lod)`: `texelFetch` on terrain's `gtexture`, a texel of
/// the fragment's sprite (`p` wraps within it, as the pack's atlas coordinate
/// would stay inside the sprite).
fn sprite_fetch(p: &HostParams, array: &dyn Fn(u32) -> String) -> String {
    let grid = p.sprite_grid;
    let a = array(0);
    let head = format!(
        "vec4 hz_sprite_fetch(ivec2 p, int lod) {{\n    \
         ivec2 tile = max(textureSize({a}, lod).xy / {grid}, ivec2(1));\n    \
         ivec2 q = hz_wrap(p, tile);\n    "
    );
    if grid > 1 {
        return format!(
            "{head}return texelFetch({a}, ivec3(q + ivec2(hz_sprite.yz) * tile, int(hz_sprite.w)), lod);\n}}\n"
        );
    }
    if p.sprite_arrays == 1 {
        return format!(
            "{head}return texelFetch({a}, ivec3(q, int(hz_sprite.x & 0x7fffu)), lod);\n}}\n"
        );
    }
    let mut s = format!(
        "{head}uint layer = hz_sprite.x & 0x7fffu;\n    int l = int(layer % {}u);\n    vec4 c;\n    \
         switch (int(layer / {}u)) {{\n",
        p.sprite_split, p.sprite_split
    );
    for k in 0..p.sprite_arrays {
        let label = if k + 1 == p.sprite_arrays {
            "default:".to_string()
        } else {
            format!("case {k}:")
        };
        writeln!(
            s,
            "    {label} c = texelFetch({}, ivec3(q, l), lod); break;",
            array(k)
        )
        .unwrap();
    }
    s + "    }\n    return c;\n}\n"
}
