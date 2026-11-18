//! Mipmaps for pack render targets: each level is the box filter of the one
//! above, drawn with `textureLoad` so it works for every colour format
//! (filterable or not) on every backend.

use std::collections::HashMap;

const SHADER: &str = "
@group(0) @binding(0) var source: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(source));
    let base = vec2<i32>(p.xy) * 2;
    let hi = size - 1;
    var sum = vec4<f32>(0.0);
    for (var y = 0; y < 2; y++) {
        for (var x = 0; x < 2; x++) {
            sum += textureLoad(source, min(base + vec2<i32>(x, y), hi), 0);
        }
    }
    return sum * 0.25;
}
";

pub(super) struct Mips {
    module: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
}

impl Mips {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader pack mipmaps"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shader pack mipmaps"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shader pack mipmaps"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        Self {
            module,
            layout,
            pipeline_layout,
            pipelines: HashMap::new(),
        }
    }

    /// Makes sure a pipeline exists for `format` (call while building targets).
    pub fn prepare(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        self.pipelines.entry(format).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Shader pack mipmaps"),
                layout: Some(&self.pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &self.module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &self.module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(format.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
    }

    /// Rebuilds levels 1.. of `texture` from level 0.
    pub fn generate(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
    ) {
        let Some(pipeline) = self.pipelines.get(&texture.format()) else {
            return;
        };
        let level = |mip: u32| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: mip,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        for mip in 1..texture.mip_level_count() {
            let source = level(mip - 1);
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Shader pack mipmap"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source),
                }],
            });
            let target = level(mip);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Shader pack mipmap"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

/// Levels of a full mip chain for a `size` texture.
pub(super) fn levels(size: [u32; 2]) -> u32 {
    32 - size[0].max(size[1]).max(1).leading_zeros()
}
