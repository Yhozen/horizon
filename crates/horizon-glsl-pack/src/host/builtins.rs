//! Legacy built-ins: declarations and the values `main` sets.

use super::*;

pub(super) const FOG_STRUCT: &str =
    "struct hz_FogParameters { vec4 color; float density; float start; float end; float scale; };";
pub(super) const TERRAIN_INPUTS: &str = "layout(location = 0) in vec3 hz_in_position;
layout(location = 1) in uvec2 hz_in_uv;
layout(location = 2) in uint hz_in_layer;
layout(location = 3) in vec4 hz_in_color;
layout(location = 4) in uvec2 hz_in_light;
layout(location = 5) in vec4 hz_in_origin;
";
/// `NormalCode` → normal; 6 and 7 (not axis-aligned, untagged) read as up.
pub(super) const NORMALS: &str = "const vec3 hz_normals[8] = vec3[](vec3(0.0, -1.0, 0.0), vec3(0.0, 1.0, 0.0), \
vec3(0.0, 0.0, -1.0), vec3(0.0, 0.0, 1.0), vec3(-1.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), \
vec3(0.0, 1.0, 0.0));";
/// GL's clip space from wgpu's: y flipped, reversed 0..1 depth to -1..1 (the matrix `M`).
pub(super) const TO_GL: &str =
    "vec4 hz_to_gl(vec4 c) { return vec4(c.x, -c.y, c.w - 2.0 * c.z, c.w); }";
/// The full-screen pass's fixed-function matrices: `gl_Vertex` is 0..1, the projection maps it to -1..1.
pub(super) const ORTHO: &str = "mat4(vec4(2.0, 0.0, 0.0, 0.0), vec4(0.0, 2.0, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(-1.0, -1.0, 0.0, 1.0))";
pub(super) const ORTHO_INVERSE: &str = "mat4(vec4(0.5, 0.0, 0.0, 0.0), vec4(0.0, 0.5, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(0.5, 0.5, 0.0, 1.0))";
/// `.st` of `gl_TextureMatrix[1] * gl_MultiTexCoord1` = (level + 0.5) / 16 from Horizon's 16 × level.
pub(super) const LIGHTMAP_MATRIX: &str = "mat4(vec4(1.0 / 256.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0 / 256.0, 0.0, 0.0), \
vec4(0.0, 0.0, 1.0, 0.0), vec4(1.0 / 32.0, 1.0 / 32.0, 0.0, 1.0))";

/// A legacy built-in's declaration and the statements of `main` that set it
/// before the pack's main runs.
pub(super) fn builtin(
    name: &str,
    stage: Stage,
    geometry: Geometry,
) -> Result<(String, String), String> {
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

/// The built-ins a stage declares: its own, plus (vertex) the ones the
/// fragment stage receives from it.
pub(super) fn builtins_of(
    rewritten: &Rewritten,
    extra: impl IntoIterator<Item = String>,
) -> BTreeSet<String> {
    let mut set: BTreeSet<String> = rewritten.builtins.iter().cloned().chain(extra).collect();
    set.remove("hz_ftransform");
    set.remove("hz_gl_FragData");
    set
}

pub(super) fn declare_builtins(
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
