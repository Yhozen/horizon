//! Legacy built-ins: declarations and the values `main` sets.

use super::*;

pub(super) const FOG_STRUCT: &str =
    "struct hz_FogParameters { vec4 color; float density; float start; float end; float scale; };";
/// Horizon's `TerrainVertex` (slot 0), the section origin and UV scale
/// (slot 1, per instance) and the `PackVertex` stream (slot 2): the
/// quad's mean UV, `(block id, cell)` and the true normal with the block's
/// light emission in `w` (client `mesher.rs`).
pub(super) const TERRAIN_INPUTS: &str = "layout(location = 0) in vec3 hz_in_position;
layout(location = 1) in uvec2 hz_in_uv;
layout(location = 2) in uint hz_in_layer;
layout(location = 3) in vec4 hz_in_color;
layout(location = 4) in uvec2 hz_in_light;
layout(location = 5) in vec4 hz_in_origin;
layout(location = 6) in uvec2 hz_in_mid_uv;
layout(location = 7) in uvec2 hz_in_block;
layout(location = 8) in vec4 hz_in_normal;
";
/// `gl_Normal` on terrain: the vertex's true normal (up if it has none).
pub(super) const TERRAIN_NORMAL: &str = "dot(hz_in_normal.xyz, hz_in_normal.xyz) > 0.01 ? normalize(hz_in_normal.xyz) : vec3(0.0, 1.0, 0.0)";
/// `PackVertex::cell` flags: a fluid surface, no block cell (distant tiles).
pub(super) const CELL_FLUID: u32 = 1 << 12;
pub(super) const CELL_NONE: u32 = 1 << 13;
/// `PackVertex::block` of a block without an id.
pub(super) const BLOCK_UNLISTED: u32 = 0xFFFF;
/// `at_tangent` on terrain, from the normal `n` (vanilla's face UVs: `u` runs
/// along +x on up and down faces and on south faces, -x on north, +z on
/// west and -z on east, so the tangent is `(n.z, 0, -n.x)` across vertical
/// faces and +x, projected onto the face, across horizontal ones), `w` 1.
pub(super) const TERRAIN_TANGENT: &str = "vec4 hz_terrain_tangent(vec3 n) {
    vec3 t = abs(n.y) > 0.70710677 ? vec3(1.0, 0.0, 0.0) - n * n.x : vec3(n.z, 0.0, -n.x);
    return vec4(normalize(t), 1.0);
}
";
/// GL's clip space from wgpu's: reversed 0..1 depth to -1..1 (the matrix `M`).
/// y stays: packs see GL's own projection; the host flips y on output, so
/// targets are stored bottom row first, as GL stores them.
pub(super) const TO_GL: &str =
    "vec4 hz_to_gl(vec4 c) { return vec4(c.x, c.y, c.w - 2.0 * c.z, c.w); }";
/// The full-screen pass's fixed-function matrices: `gl_Vertex` is 0..1, the projection maps it to -1..1.
pub(super) const ORTHO: &str = "mat4(vec4(2.0, 0.0, 0.0, 0.0), vec4(0.0, 2.0, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(-1.0, -1.0, 0.0, 1.0))";
pub(super) const ORTHO_INVERSE: &str = "mat4(vec4(0.5, 0.0, 0.0, 0.0), vec4(0.0, 0.5, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(0.5, 0.5, 0.0, 1.0))";
/// `.st` of `gl_TextureMatrix[1] * gl_MultiTexCoord1` = (level + 0.5) / 16 from Horizon's 16 × level.
pub(super) const LIGHTMAP_MATRIX: &str = "mat4(vec4(1.0 / 256.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0 / 256.0, 0.0, 0.0), \
vec4(0.0, 0.0, 1.0, 0.0), vec4(1.0 / 32.0, 1.0 / 32.0, 0.0, 1.0))";

/// Horizon's entity `Vertex` (mesher.rs): position, uv, overlay code, colour,
/// light bytes (sky, block), normal.
pub(super) const ENTITY_INPUTS: &str = "layout(location = 0) in vec3 hz_in_position;
layout(location = 1) in vec2 hz_in_uv;
layout(location = 2) in uint hz_in_layer;
layout(location = 3) in vec4 hz_in_color;
layout(location = 4) in uvec4 hz_in_light;
layout(location = 5) in vec4 hz_in_normal;
";
/// `entityColor` from an entity vertex's overlay code (entity.wgsl's
/// `overlay_texel`, overlay.rs): `(overlay rgb, 1 - overlay alpha)`, zero
/// where the overlay is opaque white (no overlay).
pub(super) const ENTITY_COLOR: &str = "vec4 hz_entity_color(uint o) {
    if (o == 1u || o == 8u) { return vec4(1.0, 0.0, 0.0, 1.0 - 178.0 / 255.0); }
    uint u = o == 5u ? 15u : (o >= 16u && o < 32u ? o - 16u : 0u);
    if (u == 0u) { return vec4(0.0); }
    float a = float(int((1.0 - float(u) / 15.0 * 0.75) * 255.0)) / 255.0;
    return vec4(1.0, 1.0, 1.0, 1.0 - a);
}
";
/// A tangent for entity vertices, which carry none: across the normal, `w` 1.
pub(super) const ENTITY_TANGENT: &str = "vec4(normalize(cross(abs(hz_in_normal.y) < 0.9 ? vec3(0.0, 1.0, 0.0) \
: vec3(1.0, 0.0, 0.0), hz_in_normal.xyz + vec3(0.0, 0.0, 1e-4))), 1.0)";
/// The head of entity.wgsl's `Globals` (group 0, binding 0) up to
/// `crumbling_sprite`, the size of a breaking stage's sprite. Same std140
/// layout as the WGSL struct: matrices and `vec4`s only.
pub(super) const ENTITY_GLOBALS: &str = "layout(std140, set = 0, binding = 0) uniform HzEntityGlobals {
    mat4 view_proj; vec4 fog_color; vec4 fog; vec4 sky_color; vec4 misc; mat4 inverse_view;
    vec4 sunrise; vec4 sun; vec4 fog_ends; vec4 end_portal_sprites[2]; vec4 time; vec4 crumbling_sprite;
} hz_entity_globals;
";
/// Entity overlay codes drawn unlit (celestial bodies, clouds, beams, portals, cracks): their light bytes are not light.
pub(super) const ENTITY_UNLIT: &str = "(hz_in_layer == 2u || hz_in_layer == 4u || hz_in_layer == 6u || hz_in_layer == 9u \
|| hz_in_layer == 10u || hz_in_layer == 11u || hz_in_layer == 12u)";
/// A general 4×4 inverse (cofactors), for `gl_*MatrixInverse` of the per-pass matrices.
pub(super) const INVERSE: &str = "mat4 hz_inverse(mat4 m) {
    float a00 = m[0][0], a01 = m[0][1], a02 = m[0][2], a03 = m[0][3];
    float a10 = m[1][0], a11 = m[1][1], a12 = m[1][2], a13 = m[1][3];
    float a20 = m[2][0], a21 = m[2][1], a22 = m[2][2], a23 = m[2][3];
    float a30 = m[3][0], a31 = m[3][1], a32 = m[3][2], a33 = m[3][3];
    float b00 = a00 * a11 - a01 * a10, b01 = a00 * a12 - a02 * a10;
    float b02 = a00 * a13 - a03 * a10, b03 = a01 * a12 - a02 * a11;
    float b04 = a01 * a13 - a03 * a11, b05 = a02 * a13 - a03 * a12;
    float b06 = a20 * a31 - a21 * a30, b07 = a20 * a32 - a22 * a30;
    float b08 = a20 * a33 - a23 * a30, b09 = a21 * a32 - a22 * a31;
    float b10 = a21 * a33 - a23 * a31, b11 = a22 * a33 - a23 * a32;
    float det = b00 * b11 - b01 * b10 + b02 * b09 + b03 * b08 - b04 * b07 + b05 * b06;
    return mat4(
        a11 * b11 - a12 * b10 + a13 * b09, a02 * b10 - a01 * b11 - a03 * b09,
        a31 * b05 - a32 * b04 + a33 * b03, a22 * b04 - a21 * b05 - a23 * b03,
        a12 * b08 - a10 * b11 - a13 * b07, a00 * b11 - a02 * b08 + a03 * b07,
        a32 * b02 - a30 * b05 - a33 * b01, a20 * b05 - a22 * b02 + a23 * b01,
        a10 * b10 - a11 * b08 + a13 * b06, a01 * b08 - a00 * b10 - a03 * b06,
        a30 * b04 - a31 * b02 + a33 * b00, a21 * b02 - a20 * b04 - a23 * b00,
        a11 * b07 - a10 * b09 - a12 * b06, a00 * b09 - a01 * b07 + a02 * b06,
        a31 * b01 - a30 * b03 - a32 * b00, a20 * b03 - a21 * b01 + a22 * b00) / det;
}
";

/// A legacy built-in's declaration and the statements of `main` that set it
/// before the pack's main runs.
pub(super) fn builtin(
    name: &str,
    stage: Stage,
    geometry: Geometry,
) -> Result<(String, String), String> {
    let vertex = stage == Stage::Vertex;
    // Per geometry: (terrain, entity, full-screen).
    let pick = |t: &str, e: &str, f: &str| {
        match geometry {
            Geometry::Terrain => t,
            Geometry::Entity => e,
            Geometry::Fullscreen => f,
        }
        .to_string()
    };
    let vertex_only = |value: String| {
        if vertex {
            Ok(format!("{name} = {value};"))
        } else {
            Err(format!("{name} in a fragment shader"))
        }
    };
    let set = |value: String| format!("{name} = {value};");
    // Light × 16 as (block, sky), as terrain carries it.
    let entity_light = format!(
        "{ENTITY_UNLIT} ? vec4(240.0, 240.0, 0.0, 1.0) : vec4(float(hz_in_light.y), float(hz_in_light.x), 0.0, 1.0)"
    );
    let (decl, init) = match name {
        "hz_gl_Vertex" => (
            "vec4",
            vertex_only(pick(
                "vec4(hz_in_position + hz_in_origin.xyz, 1.0)",
                "vec4(hz_in_position, 1.0)",
                "vec4(hz_uv, 0.0, 1.0)",
            ))?,
        ),
        // In the fragment stage these come in as varyings.
        "hz_gl_Color" if vertex => (
            "vec4",
            set(pick(
                "vec4(hz_in_color.rgb, 1.0)",
                "hz_in_color",
                "vec4(1.0)",
            )),
        ),
        "hz_gl_Color" => ("vec4", String::new()),
        "hz_gl_FogFragCoord" => ("float", String::new()),
        // Writes are discarded (GLSL 120's per-face colours).
        "hz_gl_FrontColor"
        | "hz_gl_BackColor"
        | "hz_gl_FrontSecondaryColor"
        | "hz_gl_BackSecondaryColor" => ("vec4", String::new()),
        "hz_gl_SecondaryColor" => ("vec4", vertex_only("vec4(0.0)".into())?),
        "hz_gl_Normal" => (
            "vec3",
            vertex_only(pick(
                TERRAIN_NORMAL,
                // Unlit geometry (particles, the sky) may carry no normal: up.
                "dot(hz_in_normal.xyz, hz_in_normal.xyz) > 0.01 ? normalize(hz_in_normal.xyz) : vec3(0.0, 1.0, 0.0)",
                "vec3(0.0, 0.0, 1.0)",
            ))?,
        ),
        "hz_gl_MultiTexCoord0" => (
            "vec4",
            vertex_only(pick(
                "vec4(vec2(hz_in_uv) * hz_in_origin.w, 0.0, 1.0)",
                "vec4(hz_in_uv, 0.0, 1.0)",
                "vec4(hz_uv, 0.0, 1.0)",
            ))?,
        ),
        // 2 is an alias of 1.
        "hz_gl_MultiTexCoord1" | "hz_gl_MultiTexCoord2" if geometry != Geometry::Fullscreen => (
            "vec4",
            vertex_only(pick(
                "vec4(float(hz_in_light.y), float(hz_in_light.x), 0.0, 1.0)",
                &entity_light,
                "",
            ))?,
        ),
        n if n
            .strip_prefix("hz_gl_MultiTexCoord")
            .is_some_and(|i| matches!(i, "1" | "2" | "3" | "4" | "5" | "6" | "7")) =>
        {
            ("vec4", vertex_only("vec4(0.0, 0.0, 0.0, 1.0)".into())?)
        }
        "hz_gl_TextureMatrix" => {
            // gl_TextureMatrix[1] maps light × 16 to lightmap coordinates in every program.
            let init = format!(
                "for (int i = 0; i < 8; i++) {{ hz_gl_TextureMatrix[i] = mat4(1.0); }}\n    \
                 hz_gl_TextureMatrix[1] = {LIGHTMAP_MATRIX};"
            );
            return Ok(("mat4 hz_gl_TextureMatrix[8];".into(), init));
        }
        // The per-pass matrices (world: gbufferModelView / gbufferProjection; hand, shadow: their own).
        "hz_gl_ModelViewMatrix" => (
            "mat4",
            set(pick("modelViewMatrix", "modelViewMatrix", "mat4(1.0)")),
        ),
        "hz_gl_ModelViewMatrixInverse" => (
            "mat4",
            set(pick(
                "hz_inverse(modelViewMatrix)",
                "hz_inverse(modelViewMatrix)",
                "mat4(1.0)",
            )),
        ),
        "hz_gl_ProjectionMatrix" => (
            "mat4",
            set(pick("projectionMatrix", "projectionMatrix", ORTHO)),
        ),
        "hz_gl_ProjectionMatrixInverse" => (
            "mat4",
            set(pick(
                "hz_inverse(projectionMatrix)",
                "hz_inverse(projectionMatrix)",
                ORTHO_INVERSE,
            )),
        ),
        "hz_gl_ModelViewProjectionMatrix" => (
            "mat4",
            set(pick(
                "projectionMatrix * modelViewMatrix",
                "projectionMatrix * modelViewMatrix",
                ORTHO,
            )),
        ),
        "hz_gl_NormalMatrix" => (
            "mat3",
            set(pick(
                "mat3(modelViewMatrix)",
                "mat3(modelViewMatrix)",
                "mat3(1.0)",
            )),
        ),
        "hz_gl_Fog" => {
            let init = "hz_gl_Fog = hz_FogParameters(vec4(fogColor, 1.0), fogDensity, fogStart, fogEnd, \
                1.0 / max(fogEnd - fogStart, 1e-6));";
            return Ok((
                format!("{FOG_STRUCT}\nhz_FogParameters hz_gl_Fog;"),
                init.into(),
            ));
        }
        // GL window coordinates: depth from Horizon's reversed Z; x, y as wgpu has them: targets
        // are stored bottom row first, so the row is GL's y (`final` flips it back, fragment.rs).
        "hz_FragCoord" if !vertex => (
            "vec4",
            set("vec4(gl_FragCoord.xy, 1.0 - gl_FragCoord.z, gl_FragCoord.w)".into()),
        ),
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
    // The rewriter's `gl_FragCoord` rename, whether or not it lists it.
    if rewritten.body.contains("hz_FragCoord") {
        set.insert("hz_FragCoord".into());
    }
    set
}

pub(super) fn declare_builtins(
    out: &mut String,
    init: &mut String,
    names: &BTreeSet<String>,
    stage: Stage,
    geometry: Geometry,
) -> Result<(), String> {
    if names
        .iter()
        .any(|n| n.ends_with("MatrixInverse") && geometry != Geometry::Fullscreen)
    {
        out.push_str(INVERSE);
    }
    for name in names {
        let (decl, set) = builtin(name, stage, geometry)?;
        writeln!(out, "{decl}").unwrap();
        if !set.is_empty() {
            writeln!(init, "    {set}").unwrap();
        }
    }
    Ok(())
}
