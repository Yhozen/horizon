//! Generated GLSL 450 around a rewritten program: inputs from Horizon's
//! vertex data, bindings, sampling functions, outputs.
//! Contract: .context/glsl-pack-spec.md, section 4.
//!
//! Each stage is: the uniform block, the textures it samples, its
//! inter-stage interface, the plain globals the pack's body reads and writes
//! (varyings, attributes, `hz_gl_*` built-ins), the sampling functions, the
//! body, and a host `main` that fills the globals, calls `hz_pack_main` and
//! hands the results on.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::rewrite::{Rewritten, Var};
use crate::uniforms::{UNIFORM_BLOCK, uniform_type};
use crate::{BlockIds, HostParams, ProgramKind, Stage};

#[cfg(test)]
mod tests;

/// What a program draws, which decides its vertex inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Geometry {
    /// Horizon's `TerrainVertex` plus the per-section origin instance.
    Terrain,
    /// A full-screen triangle from `gl_VertexIndex` (no buffers).
    Fullscreen,
}

/// Everything [`sources`] needs besides the two rewritten stages.
pub struct ProgramSpec<'a> {
    pub kind: ProgramKind,
    pub geometry: Geometry,
    /// Colortex indices, one per fragment output location.
    pub draw_buffers: &'a [u8],
    pub host: &'a HostParams,
    pub blocks: &'a BlockIds,
}

/// WebGPU's guaranteed number of inter-stage locations.
const MAX_LOCATIONS: u32 = 16;

/// Built-ins GLSL 120 itself carries from the vertex to the fragment stage.
const BUILTIN_VARYINGS: [(&str, &str); 2] =
    [("hz_gl_Color", "vec4"), ("hz_gl_FogFragCoord", "float")];

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

/// The vertex and fragment GLSL 450 sources of a program.
pub fn sources(
    vertex: &Rewritten,
    fragment: &Rewritten,
    spec: &ProgramSpec,
) -> Result<(String, String), String> {
    for u in vertex.uniforms.iter().chain(&fragment.uniforms) {
        match uniform_type(&u.name) {
            Some(ty) if ty == u.ty => {}
            Some(ty) => {
                return Err(format!(
                    "uniform {} is {} in the pack but {ty} in Horizon's",
                    u.name, u.ty
                ));
            }
            None => return Err(format!("unknown uniform {}", u.name)),
        }
    }
    if spec.draw_buffers.len() > 8 {
        return Err(format!(
            "{} draw buffers, at most 8",
            spec.draw_buffers.len()
        ));
    }
    if !(1..=8).contains(&spec.host.sprite_arrays)
        || spec.host.sprite_grid == 0
        || spec.host.sprite_split == 0
    {
        return Err(format!("unsupported host parameters {:?}", spec.host));
    }
    let shared = shared_varyings(vertex, fragment, spec.geometry)?;
    Ok((
        vertex_source(vertex, fragment, &shared, spec)?,
        fragment_source(fragment, &shared, spec)?,
    ))
}

/// A varying both stages declare, at `location` (arrays take one per element).
struct Shared {
    name: String,
    ty: String,
    flat: bool,
    len: Option<u32>,
    location: u32,
}

impl Shared {
    /// `(location, suffix of the interface variable, index of the global)`.
    fn elements(&self) -> Vec<(u32, String, String)> {
        match self.len {
            None => vec![(self.location, String::new(), String::new())],
            Some(n) => (0..n)
                .map(|k| (self.location + k, format!("_{k}"), format!("[{k}]")))
                .collect(),
        }
    }
}

/// Whether a varying type is integer (`Some(true)`), float (`Some(false)`) or
/// not allowed between stages in WGSL (matrices, booleans).
fn integer_type(ty: &str) -> Option<bool> {
    match ty {
        "float" | "vec2" | "vec3" | "vec4" => Some(false),
        "int" | "uint" | "ivec2" | "ivec3" | "ivec4" | "uvec2" | "uvec3" | "uvec4" => Some(true),
        _ => None,
    }
}

fn shared_varyings(
    vertex: &Rewritten,
    fragment: &Rewritten,
    geometry: Geometry,
) -> Result<Vec<Shared>, String> {
    let mut by_name = BTreeMap::new();
    for f in &fragment.varyings {
        let Some(v) = vertex.varyings.iter().find(|v| v.name == f.name) else {
            continue;
        };
        if v.ty != f.ty || v.array_len != f.array_len {
            return Err(format!(
                "varying {} has different types in the two stages",
                v.name
            ));
        }
        let integer =
            integer_type(&v.ty).ok_or_else(|| format!("varying {} of type {}", v.name, v.ty))?;
        // WGSL wants both ends to agree, and integers can only be flat.
        by_name.insert(
            v.name.clone(),
            (v.ty.clone(), v.flat || f.flat || integer, v.array_len),
        );
    }
    for (name, ty) in BUILTIN_VARYINGS {
        if fragment.builtins.contains(name) {
            by_name.insert(name.to_string(), (ty.to_string(), false, None));
        }
    }
    let mut location = 0;
    let mut shared = Vec::new();
    for (name, (ty, flat, len)) in by_name {
        shared.push(Shared {
            name,
            ty,
            flat,
            len,
            location,
        });
        location += len.unwrap_or(1);
    }
    // Terrain adds `hz_sprite` after the pack's varyings.
    let total = location + u32::from(geometry == Geometry::Terrain);
    if total > MAX_LOCATIONS {
        return Err(format!(
            "{total} varying locations, WebGPU allows {MAX_LOCATIONS}"
        ));
    }
    Ok(shared)
}

fn declare(out: &mut String, var: &Var) {
    match var.array_len {
        Some(n) => writeln!(out, "{} {}[{n}];", var.ty, var.name),
        None => writeln!(out, "{} {};", var.ty, var.name),
    }
    .unwrap();
}

fn interface(out: &mut String, shared: &[Shared], dir: &str) {
    for s in shared {
        let flat = if s.flat { "flat " } else { "" };
        for (location, suffix, _) in s.elements() {
            writeln!(
                out,
                "layout(location = {location}) {flat}{dir} {} hz_v_{}{suffix};",
                s.ty, s.name
            )
            .unwrap();
        }
    }
}

const FOG_STRUCT: &str =
    "struct hz_FogParameters { vec4 color; float density; float start; float end; float scale; };";
const TERRAIN_INPUTS: &str = "layout(location = 0) in vec3 hz_in_position;
layout(location = 1) in uvec2 hz_in_uv;
layout(location = 2) in uint hz_in_layer;
layout(location = 3) in vec4 hz_in_color;
layout(location = 4) in uvec2 hz_in_light;
layout(location = 5) in vec4 hz_in_origin;
";
/// `NormalCode` → normal; 6 and 7 (not axis-aligned, untagged) read as up.
const NORMALS: &str = "const vec3 hz_normals[8] = vec3[](vec3(0.0, -1.0, 0.0), vec3(0.0, 1.0, 0.0), \
vec3(0.0, 0.0, -1.0), vec3(0.0, 0.0, 1.0), vec3(-1.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), \
vec3(0.0, 1.0, 0.0));";
/// GL's clip space from wgpu's: y flipped, reversed 0..1 depth to -1..1 (the matrix `M`).
const TO_GL: &str = "vec4 hz_to_gl(vec4 c) { return vec4(c.x, -c.y, c.w - 2.0 * c.z, c.w); }";
/// The full-screen pass's fixed-function matrices: `gl_Vertex` is 0..1, the projection maps it to -1..1.
const ORTHO: &str = "mat4(vec4(2.0, 0.0, 0.0, 0.0), vec4(0.0, 2.0, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(-1.0, -1.0, 0.0, 1.0))";
const ORTHO_INVERSE: &str = "mat4(vec4(0.5, 0.0, 0.0, 0.0), vec4(0.0, 0.5, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(0.5, 0.5, 0.0, 1.0))";
/// `.st` of `gl_TextureMatrix[1] * gl_MultiTexCoord1` = (level + 0.5) / 16 from Horizon's 16 × level.
const LIGHTMAP_MATRIX: &str = "mat4(vec4(1.0 / 256.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0 / 256.0, 0.0, 0.0), \
vec4(0.0, 0.0, 1.0, 0.0), vec4(1.0 / 32.0, 1.0 / 32.0, 0.0, 1.0))";

/// A legacy built-in's declaration and the statements of `main` that set it
/// before the pack's main runs.
fn builtin(name: &str, stage: Stage, geometry: Geometry) -> Result<(String, String), String> {
    let terrain = geometry == Geometry::Terrain;
    let vertex = stage == Stage::Vertex;
    let pick = |t: &str, f: &str| {
        if terrain {
            t.to_string()
        } else {
            f.to_string()
        }
    };
    let vertex_only = |value: String| {
        if vertex {
            Ok(format!("{name} = {value};"))
        } else {
            Err(format!("{name} in a fragment shader"))
        }
    };
    let set = |value: String| format!("{name} = {value};");
    let (decl, init) = match name {
        "hz_gl_Vertex" => (
            "vec4",
            vertex_only(pick(
                "vec4(hz_in_position + hz_in_origin.xyz, 1.0)",
                "vec4(hz_uv, 0.0, 1.0)",
            ))?,
        ),
        // In the fragment stage these come in as varyings.
        "hz_gl_Color" if vertex => ("vec4", set(pick("vec4(hz_in_color.rgb, 1.0)", "vec4(1.0)"))),
        "hz_gl_Color" => ("vec4", String::new()),
        "hz_gl_FogFragCoord" => ("float", String::new()),
        "hz_gl_SecondaryColor" => ("vec4", vertex_only("vec4(0.0)".into())?),
        "hz_gl_Normal" => (
            "vec3",
            vertex_only(pick("hz_normals[int(hz_tag >> 5u)]", "vec3(0.0, 0.0, 1.0)"))?,
        ),
        "hz_gl_MultiTexCoord0" => (
            "vec4",
            vertex_only(pick(
                "vec4(vec2(hz_in_uv) * hz_in_origin.w, 0.0, 1.0)",
                "vec4(hz_uv, 0.0, 1.0)",
            ))?,
        ),
        "hz_gl_MultiTexCoord1" if terrain => (
            "vec4",
            vertex_only("vec4(float(hz_in_light.y), float(hz_in_light.x), 0.0, 1.0)".into())?,
        ),
        n if n
            .strip_prefix("hz_gl_MultiTexCoord")
            .is_some_and(|i| matches!(i, "1" | "2" | "3" | "4" | "5" | "6" | "7")) =>
        {
            ("vec4", vertex_only("vec4(0.0, 0.0, 0.0, 1.0)".into())?)
        }
        "hz_gl_TextureMatrix" => {
            let lightmap = if terrain {
                format!("\n    hz_gl_TextureMatrix[1] = {LIGHTMAP_MATRIX};")
            } else {
                String::new()
            };
            let init = format!(
                "for (int i = 0; i < 8; i++) {{ hz_gl_TextureMatrix[i] = mat4(1.0); }}{lightmap}"
            );
            return Ok(("mat4 hz_gl_TextureMatrix[8];".into(), init));
        }
        "hz_gl_ModelViewMatrix" => ("mat4", set(pick("gbufferModelView", "mat4(1.0)"))),
        "hz_gl_ModelViewMatrixInverse" => {
            ("mat4", set(pick("gbufferModelViewInverse", "mat4(1.0)")))
        }
        "hz_gl_ProjectionMatrix" => ("mat4", set(pick("gbufferProjection", ORTHO))),
        "hz_gl_ProjectionMatrixInverse" => {
            ("mat4", set(pick("gbufferProjectionInverse", ORTHO_INVERSE)))
        }
        "hz_gl_ModelViewProjectionMatrix" => (
            "mat4",
            set(pick("gbufferProjection * gbufferModelView", ORTHO)),
        ),
        "hz_gl_NormalMatrix" => ("mat3", set(pick("mat3(gbufferModelView)", "mat3(1.0)"))),
        "hz_gl_Fog" => {
            let init = "hz_gl_Fog = hz_FogParameters(vec4(fogColor, 1.0), fogDensity, fogStart, fogEnd, \
                1.0 / max(fogEnd - fogStart, 1e-6));";
            return Ok((
                format!("{FOG_STRUCT}\nhz_FogParameters hz_gl_Fog;"),
                init.into(),
            ));
        }
        _ => return Err(format!("unsupported built-in {name}")),
    };
    Ok((format!("{decl} {name};"), init))
}

/// Where a pack sampler's texture comes from.
enum Source {
    Sprites,
    Lightmap,
    Pack { binding: u32, depth: bool },
}

fn resolve(name: &str, geometry: Geometry) -> Result<Source, String> {
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
fn sampling(
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
fn pack_function(call: &str, op: &str, texture: &str, depth: bool) -> Option<(String, String)> {
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

/// The built-ins a stage declares: its own, plus (vertex) the ones the
/// fragment stage receives from it.
fn builtins_of(rewritten: &Rewritten, extra: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    let mut set: BTreeSet<String> = rewritten.builtins.iter().cloned().chain(extra).collect();
    set.remove("hz_ftransform");
    set.remove("hz_gl_FragData");
    set
}

fn declare_builtins(
    out: &mut String,
    init: &mut String,
    names: &BTreeSet<String>,
    stage: Stage,
    geometry: Geometry,
) -> Result<(), String> {
    for name in names {
        let (decl, set) = builtin(name, stage, geometry)?;
        writeln!(out, "{decl}").unwrap();
        if !set.is_empty() {
            writeln!(init, "    {set}").unwrap();
        }
    }
    Ok(())
}

fn vertex_source(
    vertex: &Rewritten,
    fragment: &Rewritten,
    shared: &[Shared],
    spec: &ProgramSpec,
) -> Result<String, String> {
    let terrain = spec.geometry == Geometry::Terrain;
    let mut out = format!("#version 450\n{UNIFORM_BLOCK}");
    let (bindings, functions) = sampling(&vertex.sample_calls, Stage::Vertex, spec)?;
    out += &bindings;
    if terrain {
        out += TERRAIN_INPUTS;
    }
    interface(&mut out, shared, "out");
    let location: u32 = shared.iter().map(|s| s.len.unwrap_or(1)).sum();
    if terrain {
        writeln!(
            out,
            "layout(location = {location}) flat out uvec4 hz_sprite;"
        )
        .unwrap();
        out += "vec4 hz_ft;\n";
        writeln!(out, "{NORMALS}").unwrap();
        let ids: Vec<String> = (0..32)
            .map(|i| match spec.blocks.ids.get(i) {
                Some(&id) if i < usize::from(crate::NO_MATERIAL) => format!("{id}.0"),
                _ => "0.0".into(),
            })
            .collect();
        writeln!(
            out,
            "const float hz_mc_ids[32] = float[]({});",
            ids.join(", ")
        )
        .unwrap();
    } else {
        out += "vec2 hz_uv;\n";
    }
    writeln!(out, "{TO_GL}").unwrap();
    for var in vertex.varyings.iter().chain(&vertex.attributes) {
        declare(&mut out, var);
    }
    let passed = BUILTIN_VARYINGS
        .iter()
        .filter(|(n, _)| fragment.builtins.contains(*n))
        .map(|(n, _)| n.to_string());
    let mut init = String::new();
    declare_builtins(
        &mut out,
        &mut init,
        &builtins_of(vertex, passed),
        Stage::Vertex,
        spec.geometry,
    )?;
    out += if terrain {
        "vec4 hz_ftransform() { return hz_to_gl(hz_ft); }\n"
    } else {
        "vec4 hz_ftransform() { return vec4(2.0 * hz_uv - 1.0, 0.0, 1.0); }\n"
    };
    out += &functions;
    out += &vertex.body;
    out += "\nvoid main() {\n";
    if terrain {
        out += "    hz_ft = hz_viewProj * vec4(hz_in_position + hz_in_origin.xyz, 1.0);\n";
        out += "    uint hz_tag = uint(round(hz_in_color.a * 255.0));\n";
    } else {
        out += "    hz_uv = vec2(float((gl_VertexIndex << 1) & 2), float(gl_VertexIndex & 2));\n";
    }
    out += &init;
    for a in &vertex.attributes {
        if a.array_len.is_some() {
            return Err(format!("attribute array {}", a.name));
        }
        let float = !matches!(a.ty.chars().next(), Some('i' | 'u'));
        let zero = if float { "0.0" } else { "0" };
        let value = match (a.name.as_str(), a.ty.as_str()) {
            ("mc_Entity", "float") if terrain => "hz_mc_ids[int(hz_tag & 31u)]".to_string(),
            ("mc_Entity", ty @ ("vec2" | "vec3" | "vec4")) if terrain => {
                let n = usize::from(ty.as_bytes()[3] - b'0');
                format!(
                    "{ty}(hz_mc_ids[int(hz_tag & 31u)]{})",
                    ", 0.0".repeat(n - 1)
                )
            }
            (_, ty) => format!("{ty}({zero})"),
        };
        writeln!(out, "    {} = {value};", a.name).unwrap();
    }
    out += "    hz_pack_main();\n    vec4 hz_p = gl_Position;\n";
    // The pack's untouched ftransform() goes out bit-exact, so its depth equals Horizon's own pipelines'.
    let back = "gl_Position = vec4(hz_p.x, -hz_p.y, 0.5 * (hz_p.w - hz_p.z), hz_p.w);";
    if terrain {
        writeln!(out, "    if (all(equal(hz_p, hz_to_gl(hz_ft)))) {{ gl_Position = hz_ft; }} else {{ {back} }}").unwrap();
    } else {
        writeln!(out, "    {back}").unwrap();
    }
    for s in shared {
        for (_, suffix, index) in s.elements() {
            writeln!(out, "    hz_v_{0}{suffix} = {0}{index};", s.name).unwrap();
        }
    }
    if terrain {
        let g = spec.host.sprite_grid;
        writeln!(
            out,
            "    uint hz_n = hz_in_layer & 0x7fffu;\n    \
             hz_sprite = uvec4(hz_in_layer, hz_n % {per}u % {g}u, hz_n % {per}u / {g}u, hz_n / {per}u);",
            per = g * g
        )
        .unwrap();
    }
    out += "}\n";
    Ok(out)
}

fn fragment_source(
    fragment: &Rewritten,
    shared: &[Shared],
    spec: &ProgramSpec,
) -> Result<String, String> {
    let terrain = spec.geometry == Geometry::Terrain;
    let mut out = format!("#version 450\n{UNIFORM_BLOCK}");
    let (bindings, functions) = sampling(&fragment.sample_calls, Stage::Fragment, spec)?;
    out += &bindings;
    interface(&mut out, shared, "in");
    if terrain {
        let location: u32 = shared.iter().map(|s| s.len.unwrap_or(1)).sum();
        writeln!(
            out,
            "layout(location = {location}) flat in uvec4 hz_sprite;"
        )
        .unwrap();
    }
    for i in 0..spec.draw_buffers.len() {
        writeln!(out, "layout(location = {i}) out vec4 hz_out_{i};").unwrap();
    }
    out += "vec4 hz_gl_FragData[8];\n";
    for var in &fragment.varyings {
        declare(&mut out, var);
    }
    let mut init = String::new();
    declare_builtins(
        &mut out,
        &mut init,
        &builtins_of(fragment, []),
        Stage::Fragment,
        spec.geometry,
    )?;
    out += &functions;
    out += &fragment.body;
    out += "\nvoid main() {\n";
    for s in shared {
        for (_, suffix, index) in s.elements() {
            writeln!(out, "    {0}{index} = hz_v_{0}{suffix};", s.name).unwrap();
        }
    }
    out += &init;
    out += "    hz_pack_main();\n";
    // OptiFine keeps GL's fixed-function alpha test on for these; bit 15 marks forced-opaque leaves.
    if matches!(spec.kind, ProgramKind::Terrain | ProgramKind::Shadow) {
        out += "    if (hz_gl_FragData[0].a < 0.1 && (hz_sprite.x & 0x8000u) == 0u) { discard; }\n";
    }
    for i in 0..spec.draw_buffers.len() {
        writeln!(out, "    hz_out_{i} = hz_gl_FragData[{i}];").unwrap();
    }
    out += "}\n";
    Ok(out)
}
