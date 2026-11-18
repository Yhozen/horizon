//! `HORIZON_SHADER_PACK_DEBUG=colortexN|depth|shadow|shadowcolorN`: shows one
//! of the pack's textures instead of its image, for finding the pass that
//! goes wrong without a GPU debugger. Depth is shown as GL depth (white far).

const SHADER: &str = "
@group(0) @binding(0) var source: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

@group(0) @binding(1) var<uniform> params: vec4<f32>;

@fragment
fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(source));
    let at = vec2<i32>(p.xy / params.xy * size);
    let c = textureLoad(source, at, 0);
    if (params.z > 0.5) {
        // Reversed Z: 1 - d is GL depth; stretch the near range to see it.
        let d = pow(1.0 - c.x, 64.0);
        return vec4<f32>(d, d, d, 1.0);
    }
    return vec4<f32>(c.rgb, 1.0);
}
";

/// What the variable asks to see.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Show {
    Color(u8),
    Depth,
    Shadow,
    ShadowColor(u8),
}

impl Show {
    pub fn from_env() -> Option<Self> {
        let v = std::env::var("HORIZON_SHADER_PACK_DEBUG").ok()?;
        let v = v.trim();
        Some(match v {
            "depth" => Self::Depth,
            "shadow" => Self::Shadow,
            _ => {
                if let Some(n) = v.strip_prefix("shadowcolor") {
                    Self::ShadowColor(n.parse().ok()?)
                } else {
                    Self::Color(v.strip_prefix("colortex")?.parse().ok()?)
                }
            }
        })
    }

    pub fn is_depth(self) -> bool {
        matches!(self, Self::Depth | Self::Shadow)
    }
}

pub(super) struct Debug {
    pub show: Show,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
}

impl Debug {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Option<Self> {
        let show = Show::from_env()?;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader pack debug view"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shader pack debug view"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shader pack debug view"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shader pack debug view"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shader pack debug view"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        log::info!("Horizon: shader pack debug view: {show:?}");
        Some(Self {
            show,
            pipeline,
            layout,
            params,
        })
    }

    /// Draws `source` over the whole of `view` (`size` pixels).
    pub fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        view: &wgpu::TextureView,
        size: [u32; 2],
    ) {
        let depth = if self.show.is_depth() { 1.0f32 } else { 0.0 };
        queue.write_buffer(
            &self.params,
            0,
            bytemuck::cast_slice(&[size[0] as f32, size[1] as f32, depth, 0.0]),
        );
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shader pack debug view"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.params.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shader pack debug view"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}
