//! A render pipeline per pack program: its bind group 1 layout (from the
//! textures the program reads), its vertex buffers (by geometry) and the
//! fixed-function state of its slot (blending, depth, culling).

use horizon_glsl_pack::host::Geometry;
use horizon_glsl_pack::{
    CompiledProgram, ImageBinding, ProgramKind, SampleKind, SamplerKind, TexDim, TextureBinding,
    TextureSource,
};

/// A pipeline and what its bind group 1 must hold.
pub(super) struct Program {
    pub kind: ProgramKind,
    pub pipeline: wgpu::RenderPipeline,
    pub layout: wgpu::BindGroupLayout,
    pub textures: Vec<TextureBinding>,
    pub samplers: Vec<SamplerKind>,
    /// Storage images, by binding.
    pub images: Vec<ImageBinding>,
    /// colortex (gbuffers, full-screen) or shadowcolor (shadow) indices, by output location.
    pub draw_buffers: Vec<u8>,
}

/// Group 0 layouts, by geometry, and the target formats.
pub(super) struct Layouts<'a> {
    pub terrain: &'a wgpu::BindGroupLayout,
    pub entity: &'a wgpu::BindGroupLayout,
    pub empty: &'a wgpu::BindGroupLayout,
    /// colortexN's format.
    pub color: &'a dyn Fn(u8) -> wgpu::TextureFormat,
    /// shadowcolorN's format.
    pub shadow_color: &'a dyn Fn(u8) -> wgpu::TextureFormat,
    /// The world view's format (`final` draws into it).
    pub screen: wgpu::TextureFormat,
    /// The pack has custom uniforms: every group 1 has their block at
    /// binding 1.
    pub custom_uniforms: bool,
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Whether a texture source is a depth texture.
pub(super) fn is_depth(source: &TextureSource) -> bool {
    matches!(
        source,
        TextureSource::Depth(_) | TextureSource::ShadowTex(_) | TextureSource::ShadowTexMips(_)
    )
}

/// A binding's texel type: depth for compared reads, integers as they are,
/// floats filterable only when sampled with a filtering sampler (depth
/// textures and 32-bit float formats are read with point samplers).
fn sample_type(t: &TextureBinding) -> wgpu::TextureSampleType {
    match t.sample {
        SampleKind::Depth => wgpu::TextureSampleType::Depth,
        SampleKind::Sint => wgpu::TextureSampleType::Sint,
        SampleKind::Uint => wgpu::TextureSampleType::Uint,
        SampleKind::Float => wgpu::TextureSampleType::Float {
            filterable: !is_depth(&t.source)
                && !matches!(t.sampler, SamplerKind::Point | SamplerKind::PointRepeat),
        },
    }
}

fn stages(vertex: bool, fragment: bool) -> wgpu::ShaderStages {
    let mut s = wgpu::ShaderStages::NONE;
    if vertex {
        s |= wgpu::ShaderStages::VERTEX;
    }
    if fragment {
        s |= wgpu::ShaderStages::FRAGMENT;
    }
    s
}

/// The vanilla-like blending of a slot, `None` for opaque.
fn blend(kind: ProgramKind) -> Option<wgpu::BlendState> {
    use wgpu::{BlendComponent, BlendFactor as F, BlendOperation as Op, BlendState};
    let additive = BlendState {
        color: BlendComponent {
            src_factor: F::SrcAlpha,
            dst_factor: F::One,
            operation: Op::Add,
        },
        alpha: BlendComponent::OVER,
    };
    match kind {
        // Full-screen passes (`prepare`, `deferred`, `composite`, `final`)
        // write their outputs as they are: a pass that stores alpha 0 (Bloop's
        // `prepare2` keeps last frame's cloud alpha there) must still write
        // its colour.
        k if k.is_fullscreen() => None,
        ProgramKind::Shadow | ProgramKind::ShadowEntities | ProgramKind::Terrain => None,
        ProgramKind::SkyTextured | ProgramKind::ArmorGlint => Some(additive),
        ProgramKind::DamagedBlock => Some(BlendState {
            color: BlendComponent {
                src_factor: F::Dst,
                dst_factor: F::Src,
                operation: Op::Add,
            },
            alpha: BlendComponent::OVER,
        }),
        _ => Some(BlendState::ALPHA_BLENDING),
    }
}

/// A `blend.<program>[.<buffer>]` setting as wgpu state (`None`: off).
fn pack_blend(blend: horizon_glsl_pack::plan::Blend) -> Option<wgpu::BlendState> {
    use horizon_glsl_pack::plan::{Blend, BlendFactor as P};
    use wgpu::{BlendComponent, BlendFactor as F, BlendOperation as Op};
    let factor = |f: P| match f {
        P::Zero => F::Zero,
        P::One => F::One,
        P::SrcColor => F::Src,
        P::OneMinusSrcColor => F::OneMinusSrc,
        P::DstColor => F::Dst,
        P::OneMinusDstColor => F::OneMinusDst,
        P::SrcAlpha => F::SrcAlpha,
        P::OneMinusSrcAlpha => F::OneMinusSrcAlpha,
        P::DstAlpha => F::DstAlpha,
        P::OneMinusDstAlpha => F::OneMinusDstAlpha,
        P::SrcAlphaSaturate => F::SrcAlphaSaturated,
    };
    match blend {
        Blend::Off => None,
        Blend::On {
            src,
            dst,
            src_alpha,
            dst_alpha,
        } => Some(wgpu::BlendState {
            color: BlendComponent {
                src_factor: factor(src),
                dst_factor: factor(dst),
                operation: Op::Add,
            },
            alpha: BlendComponent {
                src_factor: factor(src_alpha),
                dst_factor: factor(dst_alpha),
                operation: Op::Add,
            },
        }),
    }
}

/// Whether pipelines may blend into `format`: never into integer targets, and
/// into 32-bit float targets in the browser only with `FLOAT32_BLENDABLE`
/// (WebGPU rejects the pipeline otherwise).
fn blendable(format: wgpu::TextureFormat, features: wgpu::Features) -> bool {
    if cfg!(not(target_arch = "wasm32")) {
        return true;
    }
    let integer = matches!(
        format.sample_type(None, None),
        Some(wgpu::TextureSampleType::Sint | wgpu::TextureSampleType::Uint)
    );
    let float32 = matches!(
        format,
        wgpu::TextureFormat::R32Float
            | wgpu::TextureFormat::Rg32Float
            | wgpu::TextureFormat::Rgba32Float
    );
    !integer && (!float32 || features.contains(wgpu::Features::FLOAT32_BLENDABLE))
}

pub(super) fn build(
    device: &wgpu::Device,
    kind: ProgramKind,
    program: &CompiledProgram,
    state: &horizon_glsl_pack::plan::ProgramState,
    layouts: &Layouts,
) -> Result<Program, String> {
    let geometry = kind.geometry();
    let mut entries = vec![uniform_entry(0)];
    if layouts.custom_uniforms {
        entries.push(uniform_entry(
            horizon_glsl_pack::custom_uniforms::CUSTOM_BINDING,
        ));
    }
    let mut samplers: Vec<SamplerKind> = Vec::new();
    for t in &program.textures {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: t.binding,
            visibility: stages(t.vertex, t.fragment),
            ty: wgpu::BindingType::Texture {
                sample_type: sample_type(t),
                view_dimension: match t.dimension {
                    TexDim::D2 => wgpu::TextureViewDimension::D2,
                    TexDim::D3 => wgpu::TextureViewDimension::D3,
                },
                multisampled: false,
            },
            count: None,
        });
        if !samplers.contains(&t.sampler) {
            samplers.push(t.sampler);
        }
    }
    for b in &program.images {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: b.binding,
            visibility: stages(b.vertex, b.fragment),
            ty: wgpu::BindingType::StorageTexture {
                access: super::images::storage_access(b.access),
                format: super::images::wgpu_format(b.format).0,
                view_dimension: super::images::view_dimension(b.dimension),
            },
            count: None,
        });
    }
    samplers.sort();
    for &s in &samplers {
        let vertex = program.textures.iter().any(|t| t.sampler == s && t.vertex);
        let fragment = program
            .textures
            .iter()
            .any(|t| t.sampler == s && t.fragment);
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: s.binding(),
            visibility: stages(vertex, fragment),
            ty: wgpu::BindingType::Sampler(match s {
                SamplerKind::Compare => wgpu::SamplerBindingType::Comparison,
                SamplerKind::Point | SamplerKind::PointRepeat => {
                    wgpu::SamplerBindingType::NonFiltering
                }
                _ => wgpu::SamplerBindingType::Filtering,
            }),
            count: None,
        });
    }
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(&program.source),
        entries: &entries,
    });
    let group0 = match geometry {
        Geometry::Terrain => layouts.terrain,
        Geometry::Entity => layouts.entity,
        Geometry::Fullscreen => layouts.empty,
    };
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&program.source),
        bind_group_layouts: &[Some(group0), Some(&layout)],
        ..Default::default()
    });
    let instance_attributes = wgpu::vertex_attr_array![5=>Float32x4];
    // mid_uv, (block, cell), normal with emission: the host's `TERRAIN_INPUTS`.
    let pack_attributes = wgpu::vertex_attr_array![6=>Uint16x2,7=>Uint16x2,8=>Snorm8x4];
    let entity_attributes = wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x2,2=>Uint32,3=>Unorm8x4,4=>Uint8x4,5=>Snorm8x4];
    let buffers: Vec<Option<wgpu::VertexBufferLayout>> = match geometry {
        Geometry::Terrain => vec![
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<crate::mesher::TerrainVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &super::super::TERRAIN_ATTRIBUTES,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: 16,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &instance_attributes,
            }),
            // The arena's `PackVertex` stream (`Renderer::set_pack_vertex_stream`).
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<crate::mesher::PackVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &pack_attributes,
            }),
        ],
        Geometry::Entity => vec![Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<crate::mesher::Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &entity_attributes,
        })],
        Geometry::Fullscreen => Vec::new(),
    };
    let blend = blend(kind);
    // `final` draws the one screen target, whatever else its source writes.
    let outputs = if kind == ProgramKind::Final {
        &program.draw_buffers[..program.draw_buffers.len().min(1)]
    } else {
        &program.draw_buffers[..]
    };
    let targets: Vec<Option<wgpu::ColorTargetState>> = outputs
        .iter()
        .map(|&i| {
            let format = if kind == ProgramKind::Final {
                layouts.screen
            } else if kind.is_shadow() {
                (layouts.shadow_color)(i)
            } else {
                (layouts.color)(i)
            };
            // `blend.<program>[.<buffer>]` wins over the slot's default.
            let blend = state.blend_of(i).map_or(blend, pack_blend);
            Some(wgpu::ColorTargetState {
                format,
                blend: blend.filter(|_| blendable(format, device.features())),
                write_mask: wgpu::ColorWrites::ALL,
            })
        })
        .collect();
    // WebGPU's limits, checked here so a pack over them is refused rather than
    // failing every frame (the browser cannot catch pipeline errors in time).
    let limits = device.limits();
    let bytes: u32 = targets
        .iter()
        .flatten()
        .map(|t| t.format.target_pixel_byte_cost().unwrap_or(16))
        .sum();
    if bytes > limits.max_color_attachment_bytes_per_sample {
        return Err(format!(
            "{}: {bytes} bytes of draw buffers per pixel, the device allows {}",
            program.source, limits.max_color_attachment_bytes_per_sample
        ));
    }
    for (stage, vertex) in [("vertex", true), ("fragment", false)] {
        let used = |t: &&TextureBinding| if vertex { t.vertex } else { t.fragment };
        // Group 0's own textures: sprites and lightmap, or the atlas and lightmap.
        let horizon = match geometry {
            Geometry::Fullscreen => 0,
            _ => 2,
        };
        let textures = program.textures.iter().filter(used).count() as u32 + horizon;
        if textures > limits.max_sampled_textures_per_shader_stage {
            return Err(format!(
                "{}: {textures} textures in the {stage} stage, the device allows {}",
                program.source, limits.max_sampled_textures_per_shader_stage
            ));
        }
        let images = program
            .images
            .iter()
            .filter(|b| if vertex { b.vertex } else { b.fragment })
            .count() as u32;
        if images > limits.max_storage_textures_per_shader_stage {
            return Err(format!(
                "{}: {images} storage images in the {stage} stage, the device allows {}",
                program.source, limits.max_storage_textures_per_shader_stage
            ));
        }
    }
    let sky = matches!(kind, ProgramKind::SkyBasic | ProgramKind::SkyTextured);
    let depth_stencil = (!kind.is_fullscreen()).then(|| wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth32Float,
        depth_write_enabled: Some(!sky),
        depth_compare: Some(if sky {
            wgpu::CompareFunction::Always
        } else {
            wgpu::CompareFunction::GreaterEqual
        }),
        stencil: Default::default(),
        bias: Default::default(),
    });
    let module = |wgsl: &str| {
        device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&program.source),
            source: wgpu::ShaderSource::Wgsl(wgsl.into()),
        })
    };
    let (vs, fs) = (module(&program.vertex_wgsl), module(&program.fragment_wgsl));
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&program.source),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vs,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            buffers: &buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: if kind.is_fullscreen() {
                wgpu::PrimitiveTopology::TriangleStrip
            } else {
                wgpu::PrimitiveTopology::TriangleList
            },
            cull_mode: (kind == ProgramKind::Terrain || kind == ProgramKind::Water)
                .then_some(wgpu::Face::Back),
            // The host negates clip y (targets are stored bottom row first), which
            // mirrors the winding; `final` alone draws upright.
            front_face: if kind == ProgramKind::Final {
                wgpu::FrontFace::Ccw
            } else {
                wgpu::FrontFace::Cw
            },
            ..Default::default()
        },
        depth_stencil,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &fs,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    });
    Ok(Program {
        kind,
        pipeline,
        layout,
        textures: program.textures.clone(),
        samplers,
        images: program.images.clone(),
        draw_buffers: program.draw_buffers.clone(),
    })
}

/// The sampler of each kind.
pub(super) fn samplers(device: &wgpu::Device) -> Vec<wgpu::Sampler> {
    SamplerKind::ALL
        .iter()
        .map(|&kind| {
            let linear = !matches!(kind, SamplerKind::Point | SamplerKind::PointRepeat);
            let repeat = matches!(kind, SamplerKind::LinearRepeat | SamplerKind::PointRepeat);
            let filter = if linear {
                wgpu::FilterMode::Linear
            } else {
                wgpu::FilterMode::Nearest
            };
            let address = if repeat {
                wgpu::AddressMode::Repeat
            } else {
                wgpu::AddressMode::ClampToEdge
            };
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Shader pack sampler"),
                address_mode_u: address,
                address_mode_v: address,
                address_mode_w: address,
                mag_filter: filter,
                min_filter: filter,
                mipmap_filter: if kind == SamplerKind::Trilinear {
                    wgpu::MipmapFilterMode::Linear
                } else {
                    wgpu::MipmapFilterMode::Nearest
                },
                // Reversed Z: a point is lit when it is no farther than the
                // stored depth, i.e. its reversed depth is greater or equal.
                compare: (kind == SamplerKind::Compare)
                    .then_some(wgpu::CompareFunction::GreaterEqual),
                ..Default::default()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_screen_passes_do_not_blend() {
        for kind in [
            ProgramKind::Prepare(0),
            ProgramKind::Prepare(2),
            ProgramKind::Deferred(0),
            ProgramKind::Composite(5),
            ProgramKind::Final,
        ] {
            assert_eq!(blend(kind), None, "{kind:?}");
        }
        assert_eq!(
            blend(ProgramKind::Water),
            Some(wgpu::BlendState::ALPHA_BLENDING)
        );
    }
}
