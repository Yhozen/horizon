//! Preprocessed GLSL 120 (compatibility profile) → GLSL 450 text without
//! interface declarations. OWNER: S2. Contract: .context/glsl-pack-spec.md.

use std::collections::BTreeSet;

use crate::Stage;

/// A variable the host declares: an attribute or a varying.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Var {
    pub name: String,
    /// GLSL type (`vec4`, `float`, `ivec2`…).
    pub ty: String,
    pub flat: bool,
    pub array_len: Option<u32>,
}

/// A loose `uniform` declaration the rewriter removed (non-sampler).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Uniform {
    pub name: String,
    pub ty: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rewritten {
    /// Everything but the removed declarations, with the renames applied.
    pub body: String,
    /// `varying` (and `in`/`out` between stages) declarations, in source order.
    pub varyings: Vec<Var>,
    /// `attribute` (vertex `in`) declarations, e.g. `mc_Entity`.
    pub attributes: Vec<Var>,
    pub uniforms: Vec<Uniform>,
    /// Samplers declared and/or sampled (`gtexture`, `lightmap`, `colortex0`…).
    pub samplers: BTreeSet<String>,
    /// Legacy built-ins the body uses, by their new names (`hz_gl_Vertex`…).
    pub builtins: BTreeSet<String>,
    /// Calls the body makes to host sampling functions, e.g. `hz_sample_gtexture`.
    pub sample_calls: BTreeSet<String>,
}

pub fn rewrite(source: &str, stage: Stage) -> Result<Rewritten, String> {
    let _ = (source, stage);
    todo!("S2")
}
