//! Storage images: a program's `uniform image*` declarations bound as WebGPU
//! storage textures, and the per-image functions the rewriter's
//! `hz_imageLoad_<name>(…)`-style calls go to.
//!
//! WGSL has no image atomics, so `imageAtomic*` is a load and a store: exact
//! within one invocation, not across invocations racing on one texel.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::{ProgramSpec, split_sampling_call};
use crate::images::{ImageDim, ImageSpec, MAX_IMAGE_BYTES, glsl_format, read_write_in_core};
use crate::plan::{FormatKind, StorageFormat};
use crate::rewrite::{ImageAccess, ImageUniform, Rewritten};
use crate::{ImageBinding, ImageSource, Stage};

/// Group 1 binding of custom image `index` as a storage texture.
pub const IMAGE_BINDING: u32 = 240;
/// Group 1 binding of custom image `index` read through its sampler name.
pub const IMAGE_SAMPLED_BINDING: u32 = 304;

/// A program's images: each stage's declarations and functions, the bindings, notes.
pub(super) struct ProgramImages {
    pub(super) sources: [String; 2],
    pub(super) bindings: Vec<ImageBinding>,
    pub(super) notes: Vec<String>,
}

fn writes(u: &ImageUniform) -> bool {
    u.calls
        .iter()
        .any(|c| c == "imageStore" || c.starts_with("imageAtomic"))
}

/// The scalar GLSL type and vector prefix of a storage format's texels.
fn scalar(format: StorageFormat) -> (&'static str, &'static str) {
    use StorageFormat as S;
    match format {
        S::Rgba8Sint | S::Rgba16Sint | S::R32Sint | S::Rg32Sint | S::Rgba32Sint => ("int", "i"),
        S::Rgba8Uint | S::Rgba16Uint | S::R32Uint | S::Rg32Uint | S::Rgba32Uint => ("uint", "u"),
        _ => ("float", ""),
    }
}

/// Channels a storage format holds.
fn stored_channels(format: StorageFormat) -> u8 {
    use StorageFormat as S;
    match format {
        S::R32Float | S::R32Sint | S::R32Uint => 1,
        S::Rg32Float | S::Rg32Sint | S::Rg32Uint => 2,
        _ => 4,
    }
}

fn dim_suffix(dim: ImageDim) -> &'static str {
    match dim {
        ImageDim::D1 => "1D",
        ImageDim::D2 => "2D",
        ImageDim::D3 => "3D",
    }
}

fn coord_type(dim: ImageDim) -> &'static str {
    match dim {
        ImageDim::D1 => "int",
        ImageDim::D2 => "ivec2",
        ImageDim::D3 => "ivec3",
    }
}

/// The GLSL declaration and the `hz_<builtin>_<name>` functions of image
/// `u`, stored as `format` with `channels` of them visible to the pack, at
/// `binding` with `access`. `drop_writes`: stores do nothing and atomics
/// return 0 (a vertex stage that may not write); no declaration is made when
/// nothing else is left.
#[allow(clippy::too_many_arguments)]
pub fn image_functions(
    u: &ImageUniform,
    dim: ImageDim,
    format: StorageFormat,
    channels: u8,
    kind: FormatKind,
    binding: u32,
    access: ImageAccess,
    drop_writes: bool,
) -> String {
    let mut out = String::new();
    let (s, prefix) = scalar(format);
    let vec = format!("{prefix}vec4");
    let ty = format!("{prefix}image{}", dim_suffix(dim));
    let c = coord_type(dim);
    let name = &u.name;
    let declared = u
        .calls
        .iter()
        .any(|f| !drop_writes || !(f == "imageStore" || f.starts_with("imageAtomic")));
    if declared {
        let qualifier = match access {
            ImageAccess::ReadOnly => "readonly ",
            ImageAccess::WriteOnly => "writeonly ",
            ImageAccess::ReadWrite => "",
        };
        writeln!(
            out,
            "layout(set = 1, binding = {binding}, {}) {qualifier}uniform {ty} {name};",
            glsl_format(format)
        )
        .unwrap();
    }
    let zero = format!("{s}(0)");
    let one = if s == "float" {
        "1.0".into()
    } else {
        format!("{s}(1)")
    };
    for f in &u.calls {
        let atomic = |expr: &str| {
            if drop_writes {
                return format!("{s} hz_{f}_{name}({c} c, {s} v) {{ return {zero}; }}");
            }
            format!(
                "{s} hz_{f}_{name}({c} c, {s} v) {{ {s} o = imageLoad({name}, c).x; \
                 imageStore({name}, c, {vec}({expr})); return o; }}"
            )
        };
        let text = match f.as_str() {
            "imageLoad" => {
                let shown = channels.min(stored_channels(format));
                if shown >= 4 {
                    format!("{vec} hz_imageLoad_{name}({c} c) {{ return imageLoad({name}, c); }}")
                } else {
                    // GL reads a missing channel as 0, alpha as 1.
                    let part = |i: u8, xs: &str| {
                        if i < shown {
                            format!("v.{xs}")
                        } else if i == 3 {
                            one.clone()
                        } else {
                            zero.clone()
                        }
                    };
                    format!(
                        "{vec} hz_imageLoad_{name}({c} c) {{ {vec} v = imageLoad({name}, c); \
                         return {vec}({}, {}, {}, {}); }}",
                        part(0, "x"),
                        part(1, "y"),
                        part(2, "z"),
                        part(3, "w")
                    )
                }
            }
            "imageStore" if drop_writes => {
                format!("void hz_imageStore_{name}({c} c, {vec} v) {{}}")
            }
            "imageStore" => {
                // A normalized format stored as float keeps GL's clamp.
                let v = match (kind, s) {
                    (FormatKind::Unorm, "float") => "clamp(v, 0.0, 1.0)",
                    (FormatKind::Snorm, "float") => "clamp(v, -1.0, 1.0)",
                    _ => "v",
                };
                format!(
                    "void hz_imageStore_{name}({c} c, {vec} v) {{ imageStore({name}, c, {v}); }}"
                )
            }
            "imageSize" => format!("{c} hz_imageSize_{name}() {{ return imageSize({name}); }}"),
            "imageAtomicAdd" => atomic("o + v"),
            "imageAtomicMin" => atomic("min(o, v)"),
            "imageAtomicMax" => atomic("max(o, v)"),
            "imageAtomicAnd" => atomic("o & v"),
            "imageAtomicOr" => atomic("o | v"),
            "imageAtomicXor" => atomic("o ^ v"),
            "imageAtomicExchange" => atomic("v"),
            "imageAtomicCompSwap" if drop_writes => {
                format!("{s} hz_{f}_{name}({c} c, {s} k, {s} v) {{ return {zero}; }}")
            }
            "imageAtomicCompSwap" => format!(
                "{s} hz_{f}_{name}({c} c, {s} k, {s} v) {{ {s} o = imageLoad({name}, c).x; \
                 if (o == k) {{ imageStore({name}, c, {vec}(v)); }} return o; }}"
            ),
            _ => continue,
        };
        out += &text;
        out.push('\n');
    }
    out
}

/// The stage's declaration of image `name`, if the stage uses it.
fn stage_use<'a>(r: &'a Rewritten, name: &str) -> Option<&'a ImageUniform> {
    r.images
        .iter()
        .find(|u| u.name == name && !u.calls.is_empty())
}

/// Whether the pack's GLSL type fits the declared image.
fn type_fits(ty: &str, spec: &ImageSpec) -> bool {
    let kind = match spec.format.kind() {
        FormatKind::Sint => "i",
        FormatKind::Uint => "u",
        _ => "",
    };
    ty == format!("{kind}image{}", dim_suffix(spec.size.dim()))
}

/// The images a program declares, checked against the plan's declarations
/// and the device's limits.
pub(super) fn program_images(
    vertex: &Rewritten,
    fragment: &Rewritten,
    spec: &ProgramSpec,
) -> Result<ProgramImages, String> {
    let mut out = ProgramImages {
        sources: Default::default(),
        bindings: Vec::new(),
        notes: Vec::new(),
    };
    let host = spec.host;
    // Each image by name: (index, vertex use, fragment use).
    let mut names: BTreeSet<&str> = BTreeSet::new();
    for u in vertex.images.iter().chain(&fragment.images) {
        if !u.calls.is_empty() {
            names.insert(&u.name);
        }
    }
    let mut counts = [0u32; 2];
    for name in names {
        let (index, image) = spec
            .plan
            .images
            .iter()
            .enumerate()
            .find(|(_, i)| i.name == name)
            .ok_or_else(|| format!("image {name} has no image.{name} declaration"))?;
        let bytes = image.bytes(3840, 2160);
        if bytes > MAX_IMAGE_BYTES {
            return Err(format!(
                "image {name} takes {} MiB, more than Horizon's {} MiB for one image",
                bytes >> 20,
                MAX_IMAGE_BYTES >> 20
            ));
        }
        let format = image.storage();
        let (vs, fs) = (stage_use(vertex, name), stage_use(fragment, name));
        for u in vs.iter().chain(&fs) {
            if !type_fits(&u.ty, image) {
                return Err(format!(
                    "image {name} is a {} in the program but image.{name} is {} {:?}",
                    u.ty,
                    image.format.name(),
                    image.size.dim()
                ));
            }
        }
        let drop_vertex = !host.vertex_storage_writes && vs.is_some_and(writes);
        if drop_vertex {
            out.notes.push(format!(
                "image {name}: the vertex stage's writes are dropped (WebGPU has no storage writes in vertex shaders)"
            ));
        }
        // The calls each stage keeps.
        let is_write = |c: &String| c == "imageStore" || c.starts_with("imageAtomic");
        let is_read = |c: &String| c == "imageLoad" || c.starts_with("imageAtomic");
        let kept_vertex: Vec<&String> = vs
            .map(|u| {
                u.calls
                    .iter()
                    .filter(|c| !(drop_vertex && is_write(c)))
                    .collect()
            })
            .unwrap_or_default();
        let kept = || {
            kept_vertex
                .iter()
                .copied()
                .chain(fs.iter().flat_map(|u| &u.calls))
        };
        let read = kept().any(is_read);
        let write = kept().any(is_write);
        let vertex_bound = !kept_vertex.is_empty();
        let access = match (read, write) {
            (true, true) => ImageAccess::ReadWrite,
            (false, true) => ImageAccess::WriteOnly,
            _ => ImageAccess::ReadOnly,
        };
        if access == ImageAccess::ReadWrite
            && !host.read_write_storage
            && !read_write_in_core(format)
        {
            return Err(format!(
                "image {name} is read and written by one program, which WebGPU allows only for 32-bit one-channel formats, not {}",
                image.format.name()
            ));
        }
        if vertex_bound && access != ImageAccess::ReadOnly && !host.vertex_storage_writes {
            return Err(format!(
                "image {name} is written by the fragment stage and read by the vertex stage"
            ));
        }
        // A texture bound for writing may not be sampled in the same pass.
        if write && let Some(sampler) = &image.sampler {
            let sampled = vertex
                .sample_calls
                .iter()
                .chain(&fragment.sample_calls)
                .any(|c| split_sampling_call(c).is_some_and(|(_, n)| n == sampler));
            if sampled {
                return Err(format!(
                    "image {name} is written and read through {sampler} by one program"
                ));
            }
        }
        let binding = IMAGE_BINDING + index as u32;
        let channels = image.format.channels();
        let kind = image.format.kind();
        let dim = image.size.dim();
        if let Some(u) = vs {
            out.sources[0] +=
                &image_functions(u, dim, format, channels, kind, binding, access, drop_vertex);
            counts[0] += u32::from(vertex_bound);
        }
        if let Some(u) = fs {
            out.sources[1] +=
                &image_functions(u, dim, format, channels, kind, binding, access, false);
            counts[1] += 1;
        }
        if !vertex_bound && fs.is_none() {
            continue;
        }
        out.bindings.push(ImageBinding {
            binding,
            source: ImageSource::Custom(index),
            access,
            dimension: dim,
            format,
            vertex: vertex_bound,
            fragment: fs.is_some(),
            compute: false,
        });
    }
    for (stage, n) in [(Stage::Vertex, counts[0]), (Stage::Fragment, counts[1])] {
        if n > host.max_storage_textures {
            return Err(format!(
                "{n} storage images in the {} stage, the device allows {}",
                if stage == Stage::Vertex {
                    "vertex"
                } else {
                    "fragment"
                },
                host.max_storage_textures
            ));
        }
    }
    Ok(out)
}
