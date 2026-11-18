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
use crate::{SamplerKind, TextureBinding, TextureSource};

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

/// The host sampling operations the rewriter emits, as `<op><sampler>`.
pub const SAMPLING_OPS: [&str; 8] = [
    "hz_sampleLod_",
    "hz_sampleGrad_",
    "hz_sampleBias_",
    "hz_sample_",
    "hz_fetch_",
    "hz_size_",
    "hz_shadowLod_",
    "hz_shadow_",
];

/// `hz_sampleLod_colortex0` → `("hz_sampleLod_", "colortex0")`.
pub fn split_sampling_call(call: &str) -> Option<(&'static str, &str)> {
    SAMPLING_OPS
        .iter()
        .find_map(|op| call.strip_prefix(op).map(|name| (*op, name)))
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

/// Where a pack sampler's texture comes from.
pub(super) enum Source {
    Sprites,
    Atlas,
    Lightmap,
    Constant(&'static str),
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
    !matches!(
        format,
        BufferFormat::R32F | BufferFormat::Rg32F | BufferFormat::Rgb32F | BufferFormat::Rgba32F
    )
}

/// Whether a `texture.<stage>.<sampler>` override applies to this program.
fn custom_applies(stage: TextureStage, sampler: &str, spec: &ProgramSpec) -> bool {
    let kind = spec.kind;
    let applies = match stage {
        TextureStage::Gbuffers => !kind.is_fullscreen(),
        TextureStage::Shadow => kind.is_shadow(),
        TextureStage::Deferred => matches!(kind, ProgramKind::Deferred(_)),
        TextureStage::Composite => matches!(kind, ProgramKind::Composite(_)),
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
        _ => false,
    };
    !spec
        .plan
        .draw_buffers
        .iter()
        .any(|(k, buffers)| earlier(k) && buffers.contains(&n))
}

/// What `name` reads for the operation `op` in this program.
pub(super) fn resolve(
    name: &str,
    op: &str,
    spec: &ProgramSpec,
    watershadow: bool,
) -> Result<Source, String> {
    let fullscreen = spec.geometry == Geometry::Fullscreen;
    let canon = match canonical(name, watershadow) {
        Some(c) => c,
        // Full-screen passes read colortex0 through samplers they do not know.
        None if fullscreen => "colortex0".into(),
        None => return Err(format!("unknown sampler {name}")),
    };
    let canon = match canon.as_str() {
        "gtexture" | "lightmap" if fullscreen => "colortex0".to_string(),
        "gtexture" if spec.geometry == Geometry::Terrain => return Ok(Source::Sprites),
        "gtexture" => return Ok(Source::Atlas),
        "lightmap" => return Ok(Source::Lightmap),
        _ => canon,
    };
    let base = sampler_binding(&canon).ok_or_else(|| format!("unknown sampler {name}"))?;
    let custom = spec
        .plan
        .custom_textures
        .iter()
        .position(|c| c.sampler == canon && custom_applies(c.stage, &canon, spec));
    let entry = |source, kind| Entry {
        binding: base,
        source,
        kind,
        depth: false,
        compare: false,
        mips: false,
        manual: false,
        channels: 4,
        snap8: false,
    };
    if let Some(i) = custom {
        return Ok(Source::Pack(entry(
            TextureSource::Custom(i),
            SamplerKind::PointRepeat,
        )));
    }
    let shadow = spec.plan.shadow;
    let index = |prefix: &str| {
        canon
            .strip_prefix(prefix)
            .and_then(|n| n.parse::<u8>().ok())
    };
    if let Some(n) = index("colortex") {
        let format = spec.plan.buffers[usize::from(n)].format;
        // Mip sampling lasts from the first pass that regenerates the buffer's mips to the frame's end.
        let mips = fullscreen
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
            manual: !hardware,
            channels: format.channels(),
            snap8: is_8bit(format),
            ..entry(TextureSource::Buffer(n), kind)
        }));
    }
    if let Some(n) = index("depthtex") {
        return Ok(Source::Pack(Entry {
            depth: true,
            ..entry(TextureSource::Depth(n), SamplerKind::Point)
        }));
    }
    if let Some(n) = index("shadowtex") {
        let i = usize::from(n);
        let compared = matches!(op, "hz_shadow_" | "hz_shadowLod_");
        if compared && shadow.hardware_filtering[i] {
            return Ok(Source::Pack(Entry {
                binding: COMPARE_BINDING + u32::from(n),
                depth: true,
                compare: true,
                ..entry(TextureSource::ShadowTex(n), SamplerKind::Compare)
            }));
        }
        let linear = !shadow.tex_nearest[i];
        if shadow.tex_mipmap[i] {
            return Ok(Source::Pack(Entry {
                binding: MIPS_BINDING + u32::from(n),
                depth: true,
                mips: true,
                manual: linear,
                ..entry(TextureSource::ShadowTexMips(n), SamplerKind::Point)
            }));
        }
        return Ok(Source::Pack(Entry {
            depth: true,
            manual: linear,
            ..entry(TextureSource::ShadowTex(n), SamplerKind::Point)
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
        return Ok(Source::Pack(Entry {
            manual: !hardware,
            channels: format.channels(),
            snap8: is_8bit(format),
            ..entry(TextureSource::ShadowColor(n), kind)
        }));
    }
    match canon.as_str() {
        "noisetex" => Ok(Source::Pack(entry(
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

/// Bindings and functions for a stage's sampling calls.
pub(super) fn sampling(
    calls: &BTreeSet<String>,
    stage: Stage,
    spec: &ProgramSpec,
    extras: &Extras,
) -> Result<Sampling, String> {
    let watershadow = extras.watershadow;
    let mut bindings = BTreeMap::new();
    let mut functions = String::new();
    let mut entries: BTreeMap<u32, Entry> = BTreeMap::new();
    let fragment = stage == Stage::Fragment;
    for call in calls {
        let (op, name) =
            split_sampling_call(call).ok_or_else(|| format!("unknown sampling call {call}"))?;
        let unsupported = || format!("{call}: unsupported on {name}");
        let (signature, body) = match resolve(name, op, spec, watershadow)? {
            Source::Sprites => {
                sprite_function(call, op, stage, spec.host, &mut bindings, &mut functions)
                    .ok_or_else(unsupported)?
            }
            Source::Atlas => {
                bindings.insert((0, 1), "uniform texture2D hz_atlas;".to_string());
                bindings.insert((0, 2), "uniform sampler hz_atlas_sampler;".to_string());
                let t = "sampler2D(hz_atlas, hz_atlas_sampler)";
                let (signature, body) =
                    plain_function(call, op, t, fragment).ok_or_else(unsupported)?;
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
                )
                .ok_or_else(unsupported)?
            }
            Source::Constant(value) => {
                constant_function(call, op, value).ok_or_else(unsupported)?
            }
            Source::Pack(entry) => {
                let b = entry.binding;
                match entries.entry(b) {
                    std::collections::btree_map::Entry::Occupied(old) => {
                        let old = old.get();
                        if old.source != entry.source || old.kind != entry.kind {
                            return Err(format!("{name}: binding {b} read two ways"));
                        }
                    }
                    std::collections::btree_map::Entry::Vacant(slot) => {
                        // Compare reads become a `texture_depth_2d` through their `sampler2DShadow` use.
                        bindings.insert((1, b), format!("uniform texture2D hz_t{b};"));
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
                            functions += &entry_helpers(&entry);
                        }
                        slot.insert(entry.clone());
                    }
                }
                pack_function(call, op, &entry, fragment).ok_or_else(unsupported)?
            }
        };
        writeln!(functions, "{signature} {{\n    {body}\n}}").unwrap();
    }
    let sampled = bindings
        .values()
        .filter(|d| d.contains("texture2D"))
        .count();
    if sampled > 16 {
        return Err(format!("{sampled} textures in one stage, WebGPU allows 16"));
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

/// The combined sampler expression of a group 1 entry.
fn combined(e: &Entry) -> String {
    let name = sampler_name(e.kind);
    if e.compare {
        format!("sampler2DShadow(hz_t{}, {name})", e.binding)
    } else {
        format!("sampler2D(hz_t{}, {name})", e.binding)
    }
}

/// Per entry: `hz_post_<b>` (what the pack sees of a texel), `hz_read_<b>`
/// (a filtered read at an explicit level) and `hz_lod_<b>` (the level for
/// derivatives).
fn entry_helpers(e: &Entry) -> String {
    let b = e.binding;
    let t = combined(e);
    let post = if e.depth {
        "float d = 1.0 - c.x;\n    return vec4(d, d, d, 1.0);"
    } else {
        match e.channels {
            1 => "return vec4(c.r, 0.0, 0.0, 1.0);",
            2 => "return vec4(c.rg, 0.0, 1.0);",
            3 => "return vec4(c.rgb, 1.0);",
            _ => "return c;",
        }
    };
    // A true division: the multiply by a reciprocal is what goes wrong.
    let snap = if e.snap8 {
        "c = round(c * 255.0) / 255.0;\n    "
    } else {
        ""
    };
    let mut s = format!("vec4 hz_post_{b}(vec4 c) {{\n    {snap}{post}\n}}\n");
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
fn pack_function(call: &str, op: &str, e: &Entry, fragment: bool) -> Option<(String, String)> {
    let b = e.binding;
    let t = combined(e);
    // Implicit levels need derivatives, which only the fragment stage has.
    let derivatives = fragment && e.mips;
    if e.compare {
        // GL's LEQUAL on GL depth is GreaterEqual against `1 - ref` on Horizon's reversed depth.
        let compare = format!("return vec4(textureLod({t}, vec3(p.xy, 1.0 - p.z), 0.0));");
        return Some(match op {
            "hz_shadow_" => (format!("vec4 {call}(vec3 p)"), compare),
            "hz_shadowLod_" => (format!("vec4 {call}(vec3 p, float lod)"), compare),
            _ => return None,
        });
    }
    Some(match op {
        "hz_sample_" => (
            format!("vec4 {call}(vec2 uv)"),
            if derivatives {
                format!("return hz_read_{b}(uv, hz_lod_{b}(dFdx(uv), dFdy(uv)));")
            } else {
                format!("return hz_read_{b}(uv, 0.0);")
            },
        ),
        "hz_sampleLod_" => (
            format!("vec4 {call}(vec2 uv, float lod)"),
            format!("return hz_read_{b}(uv, lod);"),
        ),
        "hz_sampleBias_" => (
            format!("vec4 {call}(vec2 uv, float bias)"),
            if derivatives {
                format!("return hz_read_{b}(uv, hz_lod_{b}(dFdx(uv), dFdy(uv)) + bias);")
            } else {
                format!("return hz_read_{b}(uv, 0.0);")
            },
        ),
        "hz_sampleGrad_" => (
            format!("vec4 {call}(vec2 uv, vec2 dx, vec2 dy)"),
            if e.mips {
                format!("return hz_read_{b}(uv, hz_lod_{b}(dx, dy));")
            } else {
                format!("return hz_read_{b}(uv, 0.0);")
            },
        ),
        "hz_fetch_" => (
            format!("vec4 {call}(ivec2 p, int lod)"),
            format!("return hz_post_{b}(texelFetch({t}, p, lod));"),
        ),
        "hz_size_" => (
            format!("ivec2 {call}(int lod)"),
            format!("return textureSize({t}, lod);"),
        ),
        // shadow2D without hardware filtering: GL's LEQUAL on the read depth.
        "hz_shadow_" if e.depth => (
            format!("vec4 {call}(vec3 p)"),
            format!("return vec4(step(p.z, hz_read_{b}(p.xy, 0.0).x));"),
        ),
        "hz_shadowLod_" if e.depth => (
            format!("vec4 {call}(vec3 p, float lod)"),
            format!("return vec4(step(p.z, hz_read_{b}(p.xy, lod).x));"),
        ),
        _ => return None,
    })
}

/// A sampling function on a group 0 texture the hardware filters as it is
/// configured (the entity atlas, the lightmap); `implicit`: the fragment
/// stage, where `texture()` takes its level from derivatives.
fn plain_function(call: &str, op: &str, t: &str, implicit: bool) -> Option<(String, String)> {
    Some(match op {
        "hz_sample_" => (
            format!("vec4 {call}(vec2 uv)"),
            if implicit {
                format!("return texture({t}, uv);")
            } else {
                format!("return textureLod({t}, uv, 0.0);")
            },
        ),
        "hz_sampleLod_" => (
            format!("vec4 {call}(vec2 uv, float lod)"),
            format!("return textureLod({t}, uv, lod);"),
        ),
        "hz_sampleBias_" => (
            format!("vec4 {call}(vec2 uv, float bias)"),
            if implicit {
                format!("return texture({t}, uv, bias);")
            } else {
                format!("return textureLod({t}, uv, 0.0);")
            },
        ),
        "hz_sampleGrad_" => (
            format!("vec4 {call}(vec2 uv, vec2 dx, vec2 dy)"),
            format!("return textureGrad({t}, uv, dx, dy);"),
        ),
        "hz_fetch_" => (
            format!("vec4 {call}(ivec2 p, int lod)"),
            format!("return texelFetch({t}, p, lod);"),
        ),
        "hz_size_" => (
            format!("ivec2 {call}(int lod)"),
            format!("return textureSize({t}, lod);"),
        ),
        _ => return None,
    })
}

/// A sampling function on a 1×1 default texture.
fn constant_function(call: &str, op: &str, value: &str) -> Option<(String, String)> {
    let ret = format!("return {value};");
    Some(match op {
        "hz_sample_" => (format!("vec4 {call}(vec2 uv)"), ret),
        "hz_sampleLod_" => (format!("vec4 {call}(vec2 uv, float lod)"), ret),
        "hz_sampleBias_" => (format!("vec4 {call}(vec2 uv, float bias)"), ret),
        "hz_sampleGrad_" => (format!("vec4 {call}(vec2 uv, vec2 dx, vec2 dy)"), ret),
        "hz_fetch_" => (format!("vec4 {call}(ivec2 p, int lod)"), ret),
        "hz_size_" => (format!("ivec2 {call}(int lod)"), "return ivec2(1);".into()),
        _ => return None,
    })
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
    let body = match op {
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
