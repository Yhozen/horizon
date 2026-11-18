//! The one std140 uniform block every program shares, and its Rust mirror.
//! OWNER: S3.

use bytemuck::{Pod, Zeroable};

/// Mirror of [`UNIFORM_BLOCK`] (std140). Filled by the client every frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct PackUniforms {
    pub hz_view_proj: [[f32; 4]; 4],
}

/// The GLSL declaration of the block (set 1, binding 0).
pub const UNIFORM_BLOCK: &str = "";

/// The GLSL type of a uniform the block provides, by the pack's name.
pub fn uniform_type(name: &str) -> Option<&'static str> {
    let _ = name;
    todo!("S3")
}
