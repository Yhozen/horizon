//! Texture bindings and the host sampling functions.

use super::*;

/// Group 1 binding of a pack sampler's texture (`colortex0` …), if supported:
/// `colortex0..15` → 2..17, `depthtex0..2` → 18..20, `shadowtex0..1` → 21..22,
/// `shadowcolor0..1` → 23..24, `noisetex` → 25, and OptiFine's old aliases.
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
        "noisetex" => Some(25),
        _ => None,
    };
    alias
        .or_else(|| indexed("colortex", 16, 2))
        .or_else(|| indexed("depthtex", 3, 18))
        .or_else(|| indexed("shadowtex", 2, 21))
        .or_else(|| indexed("shadowcolor", 2, 23))
}

/// Where a pack sampler's texture comes from.
pub(super) enum Source {
    Sprites,
    Lightmap,
    Pack { binding: u32, depth: bool },
}

pub(super) fn resolve(name: &str, geometry: Geometry) -> Result<Source, String> {
    let terrain = geometry == Geometry::Terrain;
    // `texture` may reach us renamed, as it is a GLSL 450 built-in.
    if terrain
        && matches!(
            name,
            "gtexture" | "texture" | "hz_u_texture" | "tex" | "gcolor"
        )
    {
        return Ok(Source::Sprites);
    }
    if terrain && name == "lightmap" {
        return Ok(Source::Lightmap);
    }
    let binding = sampler_binding(name).ok_or_else(|| format!("unknown sampler {name}"))?;
    Ok(Source::Pack {
        binding,
        depth: (18..=22).contains(&binding),
    })
}

/// Bindings (by `(set, binding)`) and functions for a stage's sampling calls.
pub(super) fn sampling(
    calls: &BTreeSet<String>,
    stage: Stage,
    spec: &ProgramSpec,
) -> Result<(String, String), String> {
    let mut bindings = BTreeMap::new();
    let mut functions = String::new();
    let p = spec.host;
    let grid = format!("{}.0", p.sprite_grid);
    let array = |k: u32| format!("sampler2DArray(hz_sprites{k}, hz_sprite_sampler)");
    // The array is chosen per fragment, as `sprite()` in terrain.wgsl does.
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
    for call in calls {
        let (op, name) = [
            "hz_sampleLod_",
            "hz_sample_",
            "hz_fetch_",
            "hz_size_",
            "hz_shadow_",
        ]
        .iter()
        .find_map(|op| call.strip_prefix(op).map(|name| (*op, name)))
        .ok_or_else(|| format!("unknown sampling call {call}"))?;
        let unsupported = || format!("{call}: unsupported on {name}");
        let (signature, body) = match resolve(name, spec.geometry)? {
            Source::Sprites => {
                let fragment = stage == Stage::Fragment;
                if fragment || op == "hz_size_" {
                    bindings.insert((0, 1), "uniform texture2DArray hz_sprites0;".to_string());
                    bindings.insert((0, 2), "uniform sampler hz_sprite_sampler;".to_string());
                    for k in 1..p.sprite_arrays {
                        bindings
                            .insert((0, 4 + k), format!("uniform texture2DArray hz_sprites{k};"));
                    }
                }
                let tiled = p.sprite_grid > 1;
                if fragment && tiled && !functions.contains("hz_tiled_sprite") {
                    // tiled_sprite(): Repeat addressing within the sprite's tile, kept off its far edge.
                    writeln!(
                        functions,
                        "float hz_tile_size() {{ return float(textureSize({a}, 0).x) / {grid}; }}\n\
                         vec4 hz_tiled_sprite(vec2 uv, float lod) {{\n    \
                         vec2 wrapped = min(uv - floor(uv), vec2(1.0 - 0.5 / hz_tile_size()));\n    \
                         return textureLod({a}, vec3((vec2(hz_sprite.yz) + wrapped) / {grid}, float(hz_sprite.w)), lod);\n}}",
                        a = array(0)
                    )
                    .unwrap();
                }
                match op {
                    "hz_sample_" | "hz_sampleLod_" if !fragment => {
                        // No sprite per vertex: white, so the pack's colour maths still works.
                        let args = if op == "hz_sample_" { "vec2 uv" } else { "vec2 uv, float lod" };
                        (format!("vec4 {call}({args})"), "return vec4(1.0);".into())
                    }
                    "hz_sample_" if tiled => (
                        format!("vec4 {call}(vec2 uv)"),
                        "vec2 dx = dFdx(uv) * hz_tile_size();\n    vec2 dy = dFdy(uv) * hz_tile_size();\n    \
                         return hz_tiled_sprite(uv, 0.5 * log2(max(dot(dx, dx), dot(dy, dy))));"
                            .into(),
                    ),
                    "hz_sampleLod_" if tiled => {
                        (format!("vec4 {call}(vec2 uv, float lod)"), "return hz_tiled_sprite(uv, lod);".into())
                    }
                    "hz_sample_" if p.sprite_arrays == 1 => (
                        format!("vec4 {call}(vec2 uv)"),
                        format!("return texture({}, vec3(uv, float(hz_sprite.x & 0x7fffu)));", array(0)),
                    ),
                    "hz_sampleLod_" if p.sprite_arrays == 1 => (
                        format!("vec4 {call}(vec2 uv, float lod)"),
                        format!("return textureLod({}, vec3(uv, float(hz_sprite.x & 0x7fffu)), lod);", array(0)),
                    ),
                    // Derivatives taken before the array is chosen.
                    "hz_sample_" => (
                        format!("vec4 {call}(vec2 uv)"),
                        "vec2 dx = dFdx(uv);\n    vec2 dy = dFdy(uv);\n    ".to_string()
                            + &switch(&|a| format!("textureGrad({a}, vec3(uv, l), dx, dy)")),
                    ),
                    "hz_sampleLod_" => (
                        format!("vec4 {call}(vec2 uv, float lod)"),
                        switch(&|a| format!("textureLod({a}, vec3(uv, l), lod)")),
                    ),
                    "hz_size_" => (
                        format!("ivec2 {call}(int lod)"),
                        format!("return textureSize({}, lod).xy / {};", array(0), p.sprite_grid),
                    ),
                    _ => return Err(unsupported()),
                }
            }
            Source::Lightmap => {
                bindings.insert((0, 3), "uniform texture2D hz_lightmap;".to_string());
                bindings.insert((0, 4), "uniform sampler hz_lightmap_sampler;".to_string());
                pack_function(
                    call,
                    op,
                    "sampler2D(hz_lightmap, hz_lightmap_sampler)",
                    false,
                )
                .ok_or_else(unsupported)?
            }
            Source::Pack { binding, depth } => {
                bindings.insert((1, 1), "uniform sampler hz_point;".to_string());
                bindings.insert((1, binding), format!("uniform texture2D hz_t{binding};"));
                let texture = format!("sampler2D(hz_t{binding}, hz_point)");
                pack_function(call, op, &texture, depth).ok_or_else(unsupported)?
            }
        };
        writeln!(functions, "{signature} {{\n    {body}\n}}").unwrap();
    }
    let mut decls = String::new();
    for ((set, binding), decl) in bindings {
        writeln!(decls, "layout(set = {set}, binding = {binding}) {decl}").unwrap();
    }
    Ok((decls, functions))
}

/// A sampling function on a plain 2D texture; depth textures read `1 - stored`
/// because Horizon's depth is reversed-Z and the pack expects GL's.
pub(super) fn pack_function(
    call: &str,
    op: &str,
    texture: &str,
    depth: bool,
) -> Option<(String, String)> {
    let ret = |value: String| {
        if depth {
            format!("float d = 1.0 - {value}.x;\n    return vec4(d, d, d, 1.0);")
        } else {
            format!("return {value};")
        }
    };
    Some(match op {
        "hz_sample_" => (
            format!("vec4 {call}(vec2 uv)"),
            ret(format!("textureLod({texture}, uv, 0.0)")),
        ),
        "hz_sampleLod_" => (
            format!("vec4 {call}(vec2 uv, float lod)"),
            ret(format!("textureLod({texture}, uv, lod)")),
        ),
        "hz_fetch_" => (
            format!("vec4 {call}(ivec2 p, int lod)"),
            ret(format!("texelFetch({texture}, p, lod)")),
        ),
        "hz_size_" => (
            format!("ivec2 {call}(int lod)"),
            format!("return textureSize({texture}, lod);"),
        ),
        // shadow2D's compare (LEQUAL): lit where the point is no farther than the stored depth.
        "hz_shadow_" if depth => (
            format!("vec4 {call}(vec3 p)"),
            format!(
                "float d = 1.0 - textureLod({texture}, p.xy, 0.0).x;\n    return vec4(step(p.z, d));"
            ),
        ),
        _ => return None,
    })
}
