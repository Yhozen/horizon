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
        writeln!(
            out,
            "layout(location = {location}) flat out uvec4 hz_sprite;"
        )
        .unwrap();
        out += "vec4 hz_ft;\n";
        writeln!(out, "{NORMALS}").unwrap();
        // Material → mc_Entity.xy (`BlockIds::entity`; 31 = unlisted).
        let ids: Vec<String> = (0..32)
            .map(|i| {
                let (id, fluid) = spec.blocks.entity(i);
                format!("vec2({id}.0, {}.0)", u8::from(fluid))
            })
            .collect();
        writeln!(
            out,
            "const vec2 hz_mc_entity[32] = vec2[]({});",
            ids.join(", ")
        )
        .unwrap();
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
        out += "    uint hz_tag = uint(round(hz_in_color.a * 255.0));\n";
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
        let value = match a.name.as_str() {
            // (id, fluid, 0, 1) on terrain; GL's default attribute elsewhere.
            "mc_Entity" => {
                let entity = if terrain {
                    "vec4(hz_mc_entity[int(hz_tag & 31u)], 0.0, 1.0)"
                } else {
                    "vec4(0.0, 0.0, 0.0, 1.0)"
                };
                let swizzle = match a.ty.as_bytes().last() {
                    Some(b'2') => ".xy",
                    Some(b'3') => ".xyz",
                    Some(b'4') => "",
                    _ => ".x",
                };
                format!("{}({entity}{swizzle})", a.ty)
            }
            // Entity models carry no tangent: a unit vector across the normal.
            "at_tangent" if entity && a.ty == "vec4" => ENTITY_TANGENT.to_string(),
            _ => format!("{}({zero})", a.ty),
        };
        writeln!(out, "    {} = {value};", a.name).unwrap();
    }
    out += "    hz_pack_main();\n    vec4 hz_p = gl_Position;\n";
    // The pack's untouched ftransform() goes out bit-exact, so its depth equals Horizon's own pipelines'.
    let back = "gl_Position = vec4(hz_p.x, -hz_p.y, 0.5 * (hz_p.w - hz_p.z), hz_p.w);";
    if !fullscreen {
        writeln!(out, "    if (all(equal(hz_p, hz_to_gl(hz_ft)))) {{ gl_Position = hz_ft; }} else {{ {back} }}").unwrap();
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
    }
    out += "}\n";
    Ok((out, sampled.textures))
}
