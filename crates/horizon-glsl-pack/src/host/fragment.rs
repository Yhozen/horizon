//! The fragment stage's generated source.

use super::*;
use crate::plan::FragOutput;
use crate::plan::{AlphaFunc, AlphaTest, BufferFormat, FormatKind};

/// What a GL target does with a written value: unorm clamps 0..1 (NaN → 0);
/// a float target stores it as written, NaN and infinities included. Packs
/// count on that: Photon's clouds history stores a NaN distance on its first
/// frame, which `min` drops on the next; stored as 0 it reprojected by 0/0
/// every frame and the clouds' transmittance stayed 0 (terrain and sky black).
///
/// 16-bit unorm targets are half floats where the device cannot render into
/// R16/RG16/RGBA16 (WebGPU): there `k / 65535` reads back a hair under it, and
/// a pack that decodes with `int(x * 65535.0)` gets `k - 1` (every material id
/// off by one). `hz_unorm16` writes the step `k` a quarter step above it: a
/// real 16-bit unorm target still rounds it to `k`, and a half float keeps it
/// above `k` for every `k` up to 512. 0 and 1 stay exact.
const OUTPUT_HELPERS: &str = "vec4 hz_unorm(vec4 v) {
    bvec4 nan = greaterThan(floatBitsToUint(v) & uvec4(0x7fffffffu), uvec4(0x7f800000u));
    return mix(clamp(v, 0.0, 1.0), vec4(0.0), nan);
}
vec4 hz_unorm16(vec4 v) {
    vec4 k = round(hz_unorm(v) * 65535.0);
    return mix(min((k + 0.25) / 65535.0, vec4(1.0)), vec4(0.0), equal(k, vec4(0.0)));
}
vec4 hz_snorm(vec4 v) {
    bvec4 nan = greaterThan(floatBitsToUint(v) & uvec4(0x7fffffffu), uvec4(0x7f800000u));
    return mix(clamp(v, -1.0, 1.0), vec4(0.0), nan);
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
/// buffers, variant and plan alpha test ([`PackPlan::alpha_test`]) compile
/// to the same WGSL. (Its default alpha test, and whether atlas UVs wrap
/// per fragment.)
pub fn variant(kind: ProgramKind) -> (Option<&'static str>, bool) {
    (
        alpha_test(kind),
        matches!(
            kind,
            ProgramKind::DamagedBlock | ProgramKind::Textured | ProgramKind::BeaconBeam
        ),
    )
}

/// Terrain sprites Horizon forces opaque (bit 15 of the sprite) are never
/// discarded by an alpha test.
const TERRAIN_OPAQUE: &str = " && (hz_sprite.x & 0x8000u) == 0u";

/// The alpha test GL keeps on for the program's family (format-notes §12):
/// output 0 is discarded unless its alpha is above the reference.
fn alpha_test(kind: ProgramKind) -> Option<&'static str> {
    use ProgramKind::*;
    Some(match kind {
        // Horizon draws solid and cutout terrain in one pass: 0.1 unless the sprite is forced opaque (bit 15).
        Terrain | TerrainCutout | Shadow | ShadowSolid | ShadowCutout | ShadowWater => {
            "    if (!(hz_gl_FragData[0].a >= 0.1) && (hz_sprite.x & 0x8000u) == 0u) { discard; }\n"
        }
        Water | ArmorGlint => "    if (!(hz_gl_FragData[0].a > 0.0001)) { discard; }\n",
        ShadowEntities | ShadowBlock | Block | BlockTranslucent | Entities | EntitiesGlowing
        | EntitiesTranslucent | TexturedLit | Textured | Particles | ParticlesTranslucent
        | SpiderEyes | Hand | HandWater | Weather | DamagedBlock | Clouds => {
            "    if (!(hz_gl_FragData[0].a > 0.1)) { discard; }\n"
        }
        SkyBasic | SkyTextured | Basic | Line | Lightning | BeaconBeam | TerrainSolid
        | Prepare(_) | Deferred(_) | Composite(_) | Final => return None,
    })
}

/// The alpha test of `spec`'s slot: the plan's `alphaTest.*` when the pack
/// sets one, else the family's default.
fn alpha_test_of(spec: &ProgramSpec) -> Option<String> {
    let terrain = if spec.geometry == Geometry::Terrain {
        TERRAIN_OPAQUE
    } else {
        ""
    };
    match spec.plan.alpha_test(spec.kind) {
        None => alpha_test(spec.kind).map(str::to_string),
        Some(AlphaTest::Off) => None,
        Some(AlphaTest::Test(func, reference)) => {
            let op = match func {
                AlphaFunc::Always => return None,
                // Code after a bare `discard` is unreachable, which naga rejects.
                AlphaFunc::Never => return Some("    if (true) { discard; }\n".into()),
                AlphaFunc::Less => "<",
                AlphaFunc::Equal => "==",
                AlphaFunc::LessEqual => "<=",
                AlphaFunc::Greater => ">",
                AlphaFunc::NotEqual => "!=",
                AlphaFunc::GreaterEqual => ">=",
            };
            Some(format!(
                "    if (!(hz_gl_FragData[0].a {op} {reference:?}){terrain}) {{ discard; }}\n"
            ))
        }
    }
}

/// The scalar kind of a GLSL output type or of a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scalar {
    Float,
    Int,
    Uint,
    Bool,
}

impl Scalar {
    fn of_format(format: Option<BufferFormat>) -> Self {
        match format.map(BufferFormat::kind) {
            Some(FormatKind::Sint) => Self::Int,
            Some(FormatKind::Uint) => Self::Uint,
            _ => Self::Float,
        }
    }

    fn vec(self, n: u8) -> String {
        let (scalar, prefix) = match self {
            Self::Float => ("float", ""),
            Self::Int => ("int", "i"),
            Self::Uint => ("uint", "u"),
            Self::Bool => ("bool", "b"),
        };
        if n == 1 {
            scalar.to_string()
        } else {
            format!("{prefix}vec{n}")
        }
    }

    fn literals(self) -> (&'static str, &'static str) {
        match self {
            Self::Int => ("0", "1"),
            Self::Uint => ("0u", "1u"),
            _ => ("0.0", "1.0"),
        }
    }
}

/// The scalar kind and component count of a GLSL type (`vec3`, `uint`, `ivec2`…).
fn glsl_type(ty: &str) -> Result<(Scalar, u8), String> {
    let (scalar, rest) = match ty {
        "float" => return Ok((Scalar::Float, 1)),
        "int" => return Ok((Scalar::Int, 1)),
        "uint" => return Ok((Scalar::Uint, 1)),
        "bool" => return Ok((Scalar::Bool, 1)),
        _ => match ty.as_bytes().first() {
            Some(b'i') => (Scalar::Int, &ty[1..]),
            Some(b'u') => (Scalar::Uint, &ty[1..]),
            Some(b'b') => (Scalar::Bool, &ty[1..]),
            _ => (Scalar::Float, ty),
        },
    };
    match rest.strip_prefix("vec").and_then(|n| n.parse::<u8>().ok()) {
        Some(n @ 2..=4) => Ok((scalar, n)),
        _ => Err(format!("fragment output of type `{ty}`")),
    }
}

/// `value` (of GLSL type `ty`) as a 4-vector of `to`: converted, and
/// padded as GL does (missing components 0, alpha 1).
fn widen(value: &str, ty: &str, to: Scalar) -> Result<String, String> {
    let (from, n) = glsl_type(ty)?;
    let v = if from == to {
        value.to_string()
    } else {
        format!("{}({value})", to.vec(n))
    };
    let (zero, one) = to.literals();
    let v4 = to.vec(4);
    Ok(match n {
        1 => format!("{v4}({v}, {zero}, {zero}, {one})"),
        2 => format!("{v4}({v}, {zero}, {one})"),
        3 => format!("{v4}({v}, {one})"),
        _ => v,
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
        if shared.sprite == SpriteCarrier::Own {
            writeln!(
                out,
                "layout(location = {location}) flat in uvec4 hz_sprite;"
            )
            .unwrap();
        } else {
            out += "uvec4 hz_sprite;\n";
        }
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
    // The pack's own `out` declarations: globals of their type, copied to
    // their location's target after the pack's main.
    let outputs: &[FragOutput] = spec.plan.outputs.get(&spec.kind).map_or(&[], Vec::as_slice);
    let targets: Vec<Scalar> = (0..spec.draw_buffers.len())
        .map(|i| Scalar::of_format(output_format(spec, i)))
        .collect();
    for (i, target) in targets.iter().enumerate() {
        writeln!(
            out,
            "layout(location = {i}) out {} hz_out_{i};",
            target.vec(4)
        )
        .unwrap();
    }
    out += "vec4 hz_gl_FragData[8];\n";
    out += OUTPUT_HELPERS;
    // Atomics on storage buffers (naga_atomics), and GLSL's memory barriers.
    let (mut body, atomics) = crate::naga_atomics::rename_calls(&fragment.body)?;
    if !fragment.buffers.is_empty() {
        out += super::compute::MEMORY_BARRIERS;
    }
    if atomics {
        out += &crate::naga_atomics::atomic_stubs();
    }
    for o in outputs {
        glsl_type(&o.ty)?;
        writeln!(out, "{} hz_o_{};", o.ty, o.index).unwrap();
        body = body.replace(
            &format!("hz_gl_FragData[{}]", o.index),
            &format!("hz_o_{}", o.index),
        );
    }
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
    // `dFdy` along GL's window y: the target's rows run up (bottom row first), except
    // in `final`, which draws the upright screen.
    let final_pass = spec.kind == ProgramKind::Final;
    if body.contains("hz_dFdy") {
        let sign = if final_pass { "-1.0" } else { "1.0" };
        writeln!(out, "const float hz_dFdy_sign = {sign};").unwrap();
    }
    out += &sampled.functions;
    out += &body;
    out += "\nvoid main() {\n";
    shared.read(&mut out);
    shared.read_sprite(&mut out, spec.host.sprite_grid);
    if extras.entity_color {
        out += "    entityColor = hz_v_entityColor;\n";
    }
    if extras.uv_wrap {
        out += "    hz_uvWrap = hz_v_uvWrap;\n";
    }
    out += &init;
    if final_pass && fragment.builtins.contains("hz_FragCoord") {
        out += "    hz_FragCoord.y = viewHeight - hz_FragCoord.y;\n";
    }
    out += "    hz_pack_main();\n";
    // Outputs to float targets go through `hz_gl_FragData` (the alpha test
    // reads location 0 there); integer ones straight to their target.
    let mut direct = BTreeMap::new();
    for o in outputs {
        let slot = o.slot();
        let Some(&target) = targets.get(slot) else {
            continue;
        };
        if target == Scalar::Float {
            let v = widen(&format!("hz_o_{}", o.index), &o.ty, Scalar::Float)?;
            writeln!(out, "    hz_gl_FragData[{slot}] = {v};").unwrap();
        } else {
            direct.insert(slot, widen(&format!("hz_o_{}", o.index), &o.ty, target)?);
        }
    }
    // Only a program that writes `gl_FragData`/`gl_FragColor` is tested;
    // with `out` declarations the pack tests `alphaTestRef` itself.
    if outputs.is_empty()
        && fragment.builtins.contains("hz_gl_FragData")
        && let Some(test) = alpha_test_of(spec)
    {
        out += &test;
    }
    for (i, &target) in targets.iter().enumerate() {
        if let Some(v) = direct.get(&i) {
            writeln!(out, "    hz_out_{i} = {v};").unwrap();
            continue;
        }
        let value = format!("hz_gl_FragData[{i}]");
        let format = output_format(spec, i);
        let value = match format.map(BufferFormat::kind) {
            _ if target != Scalar::Float => widen(&value, "vec4", target)?,
            Some(FormatKind::Float) => value,
            Some(FormatKind::Snorm) => format!("hz_snorm({value})"),
            _ if format.is_some_and(BufferFormat::is_unorm16) => format!("hz_unorm16({value})"),
            _ => format!("hz_unorm({value})"),
        };
        writeln!(out, "    hz_out_{i} = {value};").unwrap();
    }
    out += "}\n";
    Ok((out, sampled.textures))
}
