//! The generated source of a compute program: the uniform block, the
//! textures it samples (explicit level, as in a vertex stage: compute has no
//! derivatives), the pack's body with its storage buffers and `shared`
//! variables, and a `main` that calls the pack's.

use super::*;

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

/// A compute program's GLSL 450 source, its textures, the uniforms nothing
/// provides and report notes.
pub fn compute_source(stage: &Rewritten, spec: &ProgramSpec) -> Result<HostOutput, String> {
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
    Ok(HostOutput {
        vertex: String::new(),
        fragment: out,
        textures: sampled.textures,
        unknown_uniforms: uniforms.unknown,
        notes: notes.into_iter().collect(),
    })
}
