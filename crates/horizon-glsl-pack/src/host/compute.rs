//! The generated source of a compute program: the uniform block, the
//! textures it samples (explicit level, as in a vertex stage: compute has no
//! derivatives), the pack's body with its storage buffers and `shared`
//! variables, and a `main` that calls the pack's.

use super::*;
use crate::plan::StorageFormat;

/// GLSL's memory barriers without an execution barrier: WebGPU has none, and
/// within one invocation memory order already holds; packs pair them with
/// `barrier()`, which is a storage and workgroup barrier here (inferred).
pub(super) const MEMORY_BARRIERS: &str = "\
void memoryBarrierShared() {}
void memoryBarrierBuffer() {}
void memoryBarrierImage() {}
void memoryBarrierAtomicCounter() {}
void groupMemoryBarrier() {}
";

/// Group 1 binding of `colorimgN`: 224 + N.
pub const COLOR_IMAGE_BINDING: u32 = 224;

/// The GLSL layout name of a colortex format WebGPU can bind as a storage
/// image, and the `vec4` prefix of its texels.
fn storage_layout(format: StorageFormat) -> Option<(&'static str, &'static str)> {
    use StorageFormat::*;
    Some(match format {
        Rgba8Unorm => ("rgba8", ""),
        Rgba16Float => ("rgba16f", ""),
        R32Float => ("r32f", ""),
        Rg32Float => ("rg32f", ""),
        Rgba32Float => ("rgba32f", ""),
        Rgba8Sint => ("rgba8i", "i"),
        Rgba16Sint => ("rgba16i", "i"),
        R32Sint => ("r32i", "i"),
        Rg32Sint => ("rg32i", "i"),
        Rgba32Sint => ("rgba32i", "i"),
        Rgba8Uint => ("rgba8ui", "u"),
        Rgba16Uint => ("rgba16ui", "u"),
        R32Uint => ("r32ui", "u"),
        Rg32Uint => ("rg32ui", "u"),
        Rgba32Uint => ("rgba32ui", "u"),
        _ => return None,
    })
}

/// The declarations and `hz_image*_colorimgN` functions of the program's
/// `colorimgN` images, and their bindings. Custom images (`image.*`) are
/// not bound yet.
fn color_images(
    stage: &Rewritten,
    spec: &ProgramSpec,
) -> Result<(String, Vec<crate::ColorImageBinding>), String> {
    use crate::rewrite::ImageAccess;
    let mut out = String::new();
    let mut bindings = Vec::new();
    for u in &stage.images {
        let n = u
            .name
            .strip_prefix("colorimg")
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|&n| n < 16)
            .ok_or_else(|| format!("image `{}`: custom images are not supported yet", u.name))?;
        let base =
            u.ty.strip_prefix(['i', 'u'])
                .filter(|r| r.starts_with("image"))
                .unwrap_or(&u.ty);
        if base != "image2D" {
            return Err(format!("`{} {}`: colorimg is a 2D image", u.ty, u.name));
        }
        let format = spec.plan.buffers[usize::from(n)].format.storage();
        let (layout, prefix) = storage_layout(format).ok_or_else(|| {
            format!(
                "{}: colortex{n} is {format:?}, which WebGPU cannot bind as a storage image",
                u.name
            )
        })?;
        if u.calls.iter().any(|c| c.starts_with("imageAtomic")) {
            return Err(format!("{}: image atomics on a colortex", u.name));
        }
        let read = u.calls.contains("imageLoad") && u.access != ImageAccess::WriteOnly;
        let write = u.calls.contains("imageStore") && u.access != ImageAccess::ReadOnly;
        let r32 = matches!(
            format,
            StorageFormat::R32Float | StorageFormat::R32Sint | StorageFormat::R32Uint
        );
        if read && write && !r32 {
            return Err(format!(
                "{} is read and written: WebGPU allows that for r32 formats only, colortex{n} is {format:?}",
                u.name
            ));
        }
        let access = if read && write {
            ""
        } else if read {
            "readonly "
        } else {
            "writeonly "
        };
        let binding = COLOR_IMAGE_BINDING + u32::from(n);
        let name = &u.name;
        let ty = format!("{prefix}image2D");
        let texel = format!("{prefix}vec4");
        writeln!(
            out,
            "layout({layout}, set = 1, binding = {binding}) {access}uniform {ty} {name};"
        )
        .unwrap();
        if u.calls.contains("imageStore") {
            if write {
                writeln!(
                    out,
                    "void hz_imageStore_{name}(ivec2 p, {texel} v) {{ imageStore({name}, p, v); }}"
                )
                .unwrap();
            } else {
                return Err(format!("imageStore on the readonly {name}"));
            }
        }
        if u.calls.contains("imageLoad") {
            if read {
                writeln!(
                    out,
                    "{texel} hz_imageLoad_{name}(ivec2 p) {{ return imageLoad({name}, p); }}"
                )
                .unwrap();
            } else {
                return Err(format!("imageLoad on the writeonly {name}"));
            }
        }
        if u.calls.contains("imageSize") {
            writeln!(
                out,
                "ivec2 hz_imageSize_{name}() {{ return imageSize({name}); }}"
            )
            .unwrap();
        }
        bindings.push(crate::ColorImageBinding {
            binding,
            buffer: n,
            format,
            read,
            write,
        });
    }
    Ok((out, bindings))
}

/// A compute program's GLSL 450 source, its textures, the uniforms nothing
/// provides and report notes.
pub fn compute_source(
    stage: &Rewritten,
    spec: &ProgramSpec,
) -> Result<(HostOutput, Vec<crate::ColorImageBinding>), String> {
    let none = Rewritten::default();
    let uniforms = UniformPlan::new(stage, &none, spec)?;
    let mut extras = Extras::new(stage, &none, spec);
    extras.converted = uniforms.converted.clone();
    if !uniforms.customs.is_empty() {
        extras.custom_block = Some(spec.plan.custom_uniforms.block());
        extras.customs = uniforms.customs.clone();
    }
    let stage = &uniforms.unknown_only(stage);
    let mut out = format!("#version 450\n{}", uniform_block(&extras));
    let sampled = sampling(&stage.sample_calls, Stage::Compute, spec, &extras)?;
    out += &sampled.decls;
    declare_unknown_uniforms(&mut out, stage);
    let mut init = String::new();
    declare_builtins(
        &mut out,
        &mut init,
        &builtins_of(stage, []),
        Stage::Compute,
        Geometry::Fullscreen,
    )?;
    let (images, color_images) = color_images(stage, spec)?;
    out += &images;
    let (body, atomics) = crate::naga_atomics::rename_calls(&stage.body)?;
    out += MEMORY_BARRIERS;
    if atomics {
        out += &crate::naga_atomics::atomic_stubs();
    }
    out += &sampled.functions;
    out += &body;
    out += "\nvoid main() {\n";
    out += &init;
    out += "    hz_pack_main();\n}\n";
    let notes = sampling_notes(&stage.sample_calls, spec, extras.watershadow);
    Ok((
        HostOutput {
            vertex: String::new(),
            fragment: out,
            textures: sampled.textures,
            unknown_uniforms: uniforms.unknown,
            notes: notes.into_iter().collect(),
        },
        color_images,
    ))
}
