//! The fragment stage's generated source.

use super::*;
use crate::plan::BufferFormat;

/// What a GL target does with a written value: unorm clamps 0..1 (NaN → 0),
/// float flushes NaN and infinities to 0 (Horizon's float targets would keep
/// them and spread them through every later blur).
const OUTPUT_HELPERS: &str = "vec4 hz_unorm(vec4 v) {
    bvec4 nan = greaterThan(floatBitsToUint(v) & uvec4(0x7fffffffu), uvec4(0x7f800000u));
    return mix(clamp(v, 0.0, 1.0), vec4(0.0), nan);
}
vec4 hz_finite(vec4 v) {
    bvec4 bad = greaterThanEqual(floatBitsToUint(v) & uvec4(0x7fffffffu), uvec4(0x7f800000u));
    return mix(v, vec4(0.0), bad);
}
";

/// The format of output location `i`'s target; `None` for the screen.
fn output_format(spec: &ProgramSpec, i: usize) -> Option<BufferFormat> {
    let n = usize::from(*spec.draw_buffers.get(i)?);
    if spec.kind == ProgramKind::Final {
        None
    } else if spec.kind.is_shadow() {
        spec.plan.shadow.color_format.get(n).copied()
    } else {
        spec.plan.buffers.get(n).map(|b| b.format)
    }
}

/// Which variant of the host a program kind gets beyond its geometry and
/// draw buffers: two kinds with the same file, geometry, shadow-ness, draw
/// buffers and variant compile to the same WGSL. (Its alpha test, and
/// whether atlas UVs wrap per fragment.)
pub fn variant(kind: ProgramKind) -> (Option<&'static str>, bool) {
    (
        alpha_test(kind),
        matches!(kind, ProgramKind::DamagedBlock | ProgramKind::Textured),
    )
}

/// The alpha test GL keeps on for the program's family (format-notes §4):
/// output 0 is discarded unless its alpha is above the reference.
fn alpha_test(kind: ProgramKind) -> Option<&'static str> {
    use ProgramKind::*;
    Some(match kind {
        // Horizon draws solid and cutout terrain in one pass: 0.1 unless the sprite is forced opaque (bit 15).
        Terrain | Shadow => {
            "    if (!(hz_gl_FragData[0].a >= 0.1) && (hz_sprite.x & 0x8000u) == 0u) { discard; }\n"
        }
        Water | ArmorGlint => "    if (!(hz_gl_FragData[0].a > 0.0001)) { discard; }\n",
        ShadowEntities | Block | Entities | TexturedLit | Textured | Hand | HandWater | Weather
        | DamagedBlock | Clouds => "    if (!(hz_gl_FragData[0].a > 0.1)) { discard; }\n",
        SkyBasic | SkyTextured | Basic | Deferred(_) | Composite(_) | Final => return None,
    })
}

pub(super) fn fragment_source(
    fragment: &Rewritten,
    shared: &Interface,
    spec: &ProgramSpec,
    extras: &Extras,
) -> Result<(String, Vec<crate::TextureBinding>), String> {
    let terrain = spec.geometry == Geometry::Terrain;
    let mut out = format!("#version 450\n{}", uniform_block(extras));
    let sampled = sampling(&fragment.sample_calls, Stage::Fragment, spec, extras)?;
    out += &sampled.decls;
    declare_unknown_uniforms(&mut out, fragment);
    shared.declare(&mut out, "in");
    let location = shared.next_location();
    if terrain {
        writeln!(
            out,
            "layout(location = {location}) flat in uvec4 hz_sprite;"
        )
        .unwrap();
    }
    if extras.entity_color {
        writeln!(
            out,
            "layout(location = {location}) flat in vec4 hz_v_entityColor;\nvec4 entityColor;"
        )
        .unwrap();
    }
    if extras.uv_wrap {
        let at = location + u32::from(extras.entity_color);
        writeln!(
            out,
            "layout(location = {at}) flat in vec4 hz_v_uvWrap;\nvec4 hz_uvWrap;"
        )
        .unwrap();
    }
    for i in 0..spec.draw_buffers.len() {
        writeln!(out, "layout(location = {i}) out vec4 hz_out_{i};").unwrap();
    }
    out += "vec4 hz_gl_FragData[8];\n";
    out += OUTPUT_HELPERS;
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
    out += &sampled.functions;
    out += &fragment.body;
    out += "\nvoid main() {\n";
    shared.read(&mut out);
    if extras.entity_color {
        out += "    entityColor = hz_v_entityColor;\n";
    }
    if extras.uv_wrap {
        out += "    hz_uvWrap = hz_v_uvWrap;\n";
    }
    out += &init;
    out += "    hz_pack_main();\n";
    // Only a program that writes output 0 is tested.
    if fragment.builtins.contains("hz_gl_FragData")
        && let Some(test) = alpha_test(spec.kind)
    {
        out += test;
    }
    for i in 0..spec.draw_buffers.len() {
        let wrap = match output_format(spec, i) {
            Some(f) if !f.is_unorm() => "hz_finite",
            _ => "hz_unorm",
        };
        writeln!(out, "    hz_out_{i} = {wrap}(hz_gl_FragData[{i}]);").unwrap();
    }
    out += "}\n";
    Ok((out, sampled.textures))
}
