//! Generated GLSL 450 around a rewritten program: inputs from Horizon's
//! vertex data, bindings, sampling functions, outputs. OWNER: S3.
//! Contract: .context/glsl-pack-spec.md.

use crate::rewrite::Rewritten;
use crate::{BlockIds, HostParams, ProgramKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Geometry {
    /// Horizon's `TerrainVertex` plus the per-section origin instance.
    Terrain,
    /// A full-screen triangle from `gl_VertexIndex` (no buffers).
    Fullscreen,
}

pub struct ProgramSpec<'a> {
    pub kind: ProgramKind,
    pub geometry: Geometry,
    pub draw_buffers: &'a [u8],
    pub host: &'a HostParams,
    pub blocks: &'a BlockIds,
}

/// Group 1 binding of a pack sampler's texture (`colortex0` …), if supported.
pub fn sampler_binding(name: &str) -> Option<u32> {
    let _ = name;
    todo!("S3")
}

/// The vertex and fragment GLSL 450 sources of a program.
pub fn sources(
    vertex: &Rewritten,
    fragment: &Rewritten,
    spec: &ProgramSpec,
) -> Result<(String, String), String> {
    let _ = (vertex, fragment, spec);
    todo!("S3")
}
