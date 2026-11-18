//! GLSL 450 (Vulkan flavour) → validated naga module → WGSL text.

use naga::back::wgsl;
use naga::front::glsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

/// Which pipeline stage a GLSL source is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Vertex,
    Fragment,
}

/// Parses `source` as one stage, validates it, and writes it as WGSL with the
/// derivative-uniformity diagnostic switched off (packs sample textures in
/// non-uniform control flow everywhere).
pub fn glsl_to_wgsl(source: &str, stage: Stage) -> Result<String, String> {
    let stage = match stage {
        Stage::Vertex => naga::ShaderStage::Vertex,
        Stage::Fragment => naga::ShaderStage::Fragment,
    };
    let mut frontend = glsl::Frontend::default();
    let module = frontend
        .parse(&glsl::Options::from(stage), source)
        .map_err(|e| e.emit_to_string(source))?;
    let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .map_err(|e| e.emit_to_string(source))?;
    let text = wgsl::write_string(&module, &info, wgsl::WriterFlags::empty())
        .map_err(|e| e.to_string())?;
    let out = format!("diagnostic(off, derivative_uniformity);\n{text}");
    // The browser compiles this text again: make sure naga's own WGSL front end takes it.
    let reparsed = naga::front::wgsl::parse_str(&out).map_err(|e| e.emit_to_string(&out))?;
    Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&reparsed)
        .map_err(|e| e.emit_to_string(&out))?;
    Ok(out)
}
