//! The vertex stage's generated source.

use super::*;

pub(super) fn vertex_source(
    vertex: &Rewritten,
    fragment: &Rewritten,
    shared: &Interface,
    spec: &ProgramSpec,
    extras: &Extras,
) -> Result<(String, Vec<crate::TextureBinding>), String> {
    let terrain = spec.geometry == Geometry::Terrain;
    let entity = spec.geometry == Geometry::Entity;
    let fullscreen = spec.geometry == Geometry::Fullscreen;
    let mut out = format!("#version 450\n{}", uniform_block(extras));
    let sampled = sampling(&vertex.sample_calls, Stage::Vertex, spec, extras)?;
    out += &sampled.decls;
    declare_unknown_uniforms(&mut out, vertex);
    match spec.geometry {
        Geometry::Terrain => out += TERRAIN_INPUTS,
        Geometry::Entity => out += ENTITY_INPUTS,
        Geometry::Fullscreen => {}
    }
    shared.declare(&mut out, "out");
    let location = shared.next_location();
    if terrain {
        if shared.sprite == SpriteCarrier::Own {
            writeln!(
                out,
                "layout(location = {location}) flat out uvec4 hz_sprite;"
            )
            .unwrap();
        } else {
            out += "uvec4 hz_sprite;\n";
        }
        out += "vec4 hz_ft;\n";
        out += TERRAIN_TANGENT;
    } else if entity {
        out += "vec4 hz_ft;\n";
        if extras.entity_color {
            writeln!(
                out,
                "layout(location = {location}) flat out vec4 hz_v_entityColor;\nvec4 entityColor;\n{ENTITY_COLOR}"
            )
            .unwrap();
        }
        if extras.uv_wrap {
            let at = location + u32::from(extras.entity_color);
            writeln!(
                out,
                "layout(location = {at}) flat out vec4 hz_v_uvWrap;\n{ENTITY_GLOBALS}"
            )
            .unwrap();
        }
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
    out += if fullscreen {
        "vec4 hz_ftransform() { return vec4(2.0 * hz_uv - 1.0, 0.0, 1.0); }\n"
    } else {
        "vec4 hz_ftransform() { return hz_to_gl(hz_ft); }\n"
    };
    out += &sampled.functions;
    out += &vertex.body;
    out += "\nvoid main() {\n";
    if terrain {
        out += "    hz_ft = hz_viewProj * vec4(hz_in_position + hz_in_origin.xyz, 1.0);\n";
    } else if entity {
        out += "    hz_ft = hz_viewProj * vec4(hz_in_position, 1.0);\n";
        if extras.entity_color {
            out += "    entityColor = hz_entity_color(hz_in_layer);\n";
        }
    } else {
        // The client draws a 4-vertex triangle strip: (0,0) (1,0) (0,1) (1,1).
        out += "    hz_uv = vec2(float(gl_VertexIndex & 1), float(gl_VertexIndex >> 1));\n";
    }
    out += &init;
    for a in &vertex.attributes {
        if a.array_len.is_some() {
            return Err(format!("attribute array {}", a.name));
        }
        let float = !matches!(a.ty.chars().next(), Some('i' | 'u'));
        let zero = if float { "0.0" } else { "0" };
        let value = match (a.name.as_str(), terrain) {
            ("mc_Entity", true) => fit(&terrain_entity(spec.blocks.unlisted), &a.ty),
            ("mc_midTexCoord", true) => {
                fit("vec4(vec2(hz_in_mid_uv) * hz_in_origin.w, 0.0, 1.0)", &a.ty)
            }
            ("at_midBlock", true) => fit(&terrain_mid_block(), &a.ty),
            ("at_tangent", true) => fit(&format!("hz_terrain_tangent({TERRAIN_NORMAL})"), &a.ty),
            // GL's default attribute elsewhere.
            ("mc_Entity" | "mc_midTexCoord", false) => fit("vec4(0.0, 0.0, 0.0, 1.0)", &a.ty),
            // Entity models carry no tangent: a unit vector across the normal.
            ("at_tangent", false) if entity && a.ty == "vec4" => ENTITY_TANGENT.to_string(),
            _ => format!("{}({zero})", a.ty),
        };
        writeln!(out, "    {} = {value};", a.name).unwrap();
    }
    out += "    hz_pack_main();\n    vec4 hz_p = gl_Position;\n";
    // GL clip to wgpu's, y negated: GL's bottom row lands in the target's first row, as GL
    // stores images. `final` draws the screen, which is upright (top row first): no flip.
    let back = if spec.kind == ProgramKind::Final {
        "gl_Position = vec4(hz_p.x, hz_p.y, 0.5 * (hz_p.w - hz_p.z), hz_p.w);"
    } else {
        "gl_Position = vec4(hz_p.x, -hz_p.y, 0.5 * (hz_p.w - hz_p.z), hz_p.w);"
    };
    if !fullscreen {
        // The pack's untouched ftransform() goes out bit-exact, so its depth equals Horizon's own pipelines'.
        writeln!(out, "    if (all(equal(hz_p, hz_to_gl(hz_ft)))) {{ gl_Position = vec4(hz_ft.x, -hz_ft.y, hz_ft.zw); }} else {{ {back} }}").unwrap();
    } else {
        writeln!(out, "    {back}").unwrap();
    }
    shared.write(&mut out);
    if extras.entity_color {
        out += "    hz_v_entityColor = entityColor;\n";
    }
    if extras.uv_wrap {
        // entity.wgsl's wrap: beams repeat v inside the sprite, cracks repeat inside the stage's sprite.
        out += "    vec2 hz_w = vec2(float(hz_in_light.x | (hz_in_light.y << 8u)), \
                float(hz_in_light.z | (hz_in_light.w << 8u))) / 65535.0;\n    \
                hz_v_uvWrap = hz_in_layer == 6u ? vec4(0.0, hz_w.x, 0.0, hz_w.y) \
                : (hz_in_layer == 11u || hz_in_layer == 12u) ? vec4(hz_w, hz_entity_globals.crumbling_sprite.zw) \
                : vec4(0.0);\n";
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
        shared.write_sprite(&mut out);
    }
    out += "}\n";
    Ok((out, sampled.textures))
}

/// `mc_Entity` on terrain: `(id, fluid, 0, 1)` from `PackVertex::block` and
/// `cell` (`unlisted` for a block without an id).
fn terrain_entity(unlisted: i32) -> String {
    format!(
        "vec4(hz_in_block.x == {BLOCK_UNLISTED}u ? {unlisted}.0 : float(hz_in_block.x), \
         (hz_in_block.y & {CELL_FLUID}u) != 0u ? 1.0 : 0.0, 0.0, 1.0)"
    )
}

/// `at_midBlock` on terrain: from the vertex to its block's centre in 1/64
/// block (zero without a cell), the block's light emission in `w`.
fn terrain_mid_block() -> String {
    format!(
        "vec4((hz_in_block.y & {CELL_NONE}u) != 0u ? vec3(0.0) \
         : (vec3(uvec3(hz_in_block.y, hz_in_block.y >> 4u, hz_in_block.y >> 8u) & 15u) + 0.5 - hz_in_position) * 64.0, \
         round(hz_in_normal.w * 127.0))"
    )
}

/// A `vec4` expression as an attribute of GLSL type `ty` (`float`, `vec2`,
/// `ivec3`...): its leading components, converted.
fn fit(value: &str, ty: &str) -> String {
    let swizzle = match ty.as_bytes().last() {
        Some(b'2') => ".xy",
        Some(b'3') => ".xyz",
        Some(b'4') => "",
        _ => ".x",
    };
    format!("{ty}({value}{swizzle})")
}
