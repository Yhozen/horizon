//! Generated GLSL 450 around a rewritten program: inputs from Horizon's
//! vertex data, bindings, sampling functions, outputs.
//! Contract: .context/glsl-pack-spec.md, section 4.
//!
//! Each stage is: the uniform block, the textures it samples, its
//! inter-stage interface, the plain globals the pack's body reads and writes
//! (varyings, attributes, `hz_gl_*` built-ins), the sampling functions, the
//! body, and a host `main` that fills the globals, calls `hz_pack_main` and
//! hands the results on.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::rewrite::{Rewritten, Var};
use crate::uniforms::{UNIFORM_BLOCK, uniform_type};
use crate::{BlockIds, HostParams, ProgramKind, Stage};

mod builtins;
mod fragment;
mod sampling;
#[cfg(test)]
mod tests;
mod varyings;
mod vertex;

use builtins::*;
use fragment::fragment_source;
pub use sampling::sampler_binding;
use sampling::*;
use varyings::*;
use vertex::vertex_source;

/// What a program draws, which decides its vertex inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Geometry {
    /// Horizon's `TerrainVertex` plus the per-section origin instance.
    Terrain,
    /// A full-screen triangle from `gl_VertexIndex` (no buffers).
    Fullscreen,
}

/// Everything [`sources`] needs besides the two rewritten stages.
pub struct ProgramSpec<'a> {
    pub kind: ProgramKind,
    pub geometry: Geometry,
    /// Colortex indices, one per fragment output location.
    pub draw_buffers: &'a [u8],
    pub host: &'a HostParams,
    pub blocks: &'a BlockIds,
}

/// WebGPU's guaranteed number of inter-stage locations.
const MAX_LOCATIONS: u32 = 16;

/// Built-ins GLSL 120 itself carries from the vertex to the fragment stage.
const BUILTIN_VARYINGS: [(&str, &str); 2] =
    [("hz_gl_Color", "vec4"), ("hz_gl_FogFragCoord", "float")];

/// The vertex and fragment GLSL 450 sources of a program.
pub fn sources(
    vertex: &Rewritten,
    fragment: &Rewritten,
    spec: &ProgramSpec,
) -> Result<(String, String), String> {
    for u in vertex.uniforms.iter().chain(&fragment.uniforms) {
        match uniform_type(&u.name) {
            Some(ty) if ty == u.ty => {}
            Some(ty) => {
                return Err(format!(
                    "uniform {} is {} in the pack but {ty} in Horizon's",
                    u.name, u.ty
                ));
            }
            None => return Err(format!("unknown uniform {}", u.name)),
        }
    }
    if spec.draw_buffers.len() > 8 {
        return Err(format!(
            "{} draw buffers, at most 8",
            spec.draw_buffers.len()
        ));
    }
    if !(1..=8).contains(&spec.host.sprite_arrays)
        || spec.host.sprite_grid == 0
        || spec.host.sprite_split == 0
    {
        return Err(format!("unsupported host parameters {:?}", spec.host));
    }
    let shared = shared_varyings(vertex, fragment, spec.geometry)?;
    Ok((
        vertex_source(vertex, fragment, &shared, spec)?,
        fragment_source(fragment, &shared, spec)?,
    ))
}
