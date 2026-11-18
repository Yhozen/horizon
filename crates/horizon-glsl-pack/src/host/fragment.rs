//! The fragment stage's generated source.

use super::*;

pub(super) fn fragment_source(
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
