//! OptiFine-format shader packs (`shaders/*.vsh|*.fsh|*.properties`) translated
//! into WGSL that Horizon's wgpu renderer runs, natively and in the browser.
//!
//! Written from the pack format and the packs' own sources (clean room: no
//! code from Iris or from the shader-pack code Horizon removed in 4e7c25bb).
//!
//! The pipeline per program: [`preprocess`] (`#include`, `#define`, `#if`,
//! option overrides) → [`rewrite`] (GLSL 120 compatibility profile → plain
//! GLSL 450 globals, uniform and sampler uses recorded) → [`host`] (a
//! generated GLSL 450 `main` per stage that feeds Horizon's vertex data in and
//! the pack's results out) → [`naga_out`] (naga: GLSL → validated module →
//! WGSL text). Nothing here touches wgpu or the file system.

pub mod custom_uniforms;
pub mod directives;
pub mod features;
pub mod frame_math;
pub mod host;
pub mod legacy_ids;
pub mod naga_out;
pub mod noise;
pub mod options;
pub mod plan;
pub mod preprocess;
pub mod program;
pub mod properties;
pub mod render_stage;
pub mod report;
pub mod rewrite;
pub mod schedule;
pub mod screens;
pub mod session;
pub mod shaders_properties;
pub mod textures;
pub mod types;
pub mod uniforms;

pub use naga_out::Stage;
pub use plan::PackPlan;
pub use program::{ProgramKind, find_program, world_folder};
pub use properties::BlockIds;
pub use render_stage::RenderStage;
pub use report::{Feature, PackReport, PackStatus, ProgramOutcome};
pub use session::{Session, compile, compile_kinds, compile_report, read_plan};
pub use types::*;
pub use uniforms::PackUniforms;

#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod tests;
