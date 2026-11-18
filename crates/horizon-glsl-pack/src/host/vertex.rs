//! The vertex stage's generated source.

use super::*;

pub(super) fn vertex_source(
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
