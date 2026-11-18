//! A shader pack's programs in the world pass (`--shader-pack`, off by
//! default): the pack's `gbuffers_terrain` and `gbuffers_water` draw Horizon's
//! sections into its colour targets (colortex0, 6, 7… per `DRAWBUFFERS`), the
//! rest of the world draws into colortex0 with Horizon's own pipelines, and
//! the pack's `final` composes the targets into the world's view.
//!
//! The WGSL comes from `horizon_glsl_pack`, which also fixes the bindings:
//! group 0 is the terrain group (`Renderer::group`), group 1 the pack's, with
//! the uniform block at binding 0, a non-filtering sampler at 1 and textures at
//! `horizon_glsl_pack::host::sampler_binding`.

use horizon_glsl_pack::{CompiledPack, CompiledProgram, PackUniforms, ProgramKind, host};
use std::collections::BTreeMap;

use super::TERRAIN_ATTRIBUTES;

/// Pack colour targets other than colortex0 (which is the world's format, so
/// Horizon's pipelines can draw into it): OptiFine's default RGBA8.
const AUX_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The textures the passes draw into and `final` reads, at the world's size.
struct Targets {
    size: [u32; 2],
    /// colortex index → view.
    color: BTreeMap<u8, wgpu::TextureView>,
    depth: wgpu::TextureView,
    final_group: wgpu::BindGroup,
}

pub(crate) struct PackRenderer {
    terrain: wgpu::RenderPipeline,
    water: wgpu::RenderPipeline,
    final_pipeline: wgpu::RenderPipeline,
    terrain_buffers: Vec<u8>,
    water_buffers: Vec<u8>,
    final_samplers: Vec<(String, u32)>,
    final_layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    gbuffer_group: wgpu::BindGroup,
    point: wgpu::Sampler,
    color_format: wgpu::TextureFormat,
    targets: Option<Targets>,
}

fn uniform_entry() -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// The program's group 1 textures, by binding: only colour targets and
/// depthtex0 exist in this proof of concept.
fn textures(program: &CompiledProgram) -> Result<Vec<(String, u32)>, String> {
    program
        .samplers
        .iter()
        .filter(|s| !matches!(s.as_str(), "gtexture" | "texture" | "tex" | "lightmap"))
        .map(|s| match host::sampler_binding(s) {
            Some(b) if (2..18).contains(&b) || b == 18 => Ok((s.clone(), b)),
            _ => Err(format!("{}: sampler {s} is not supported", program.source)),
        })
        .collect()
}

impl PackRenderer {
    pub fn new(
        device: &wgpu::Device,
        terrain_layout: &wgpu::BindGroupLayout,
        color_format: wgpu::TextureFormat,
        pack: &CompiledPack,
    ) -> Result<Self, String> {
        let program = |kind| {
            pack.programs
                .get(&kind)
                .ok_or_else(|| format!("the pack has no {kind:?} program"))
        };
        let (terrain, water, fin) = (
            program(ProgramKind::Terrain)?,
            program(ProgramKind::Water)?,
            program(ProgramKind::Final)?,
        );
        for p in [terrain, water] {
            if let Some(t) = textures(p)?.first() {
                return Err(format!("{}: reads {} while it draws", p.source, t.0));
            }
        }
        let gbuffer_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shader pack gbuffers"),
            entries: &[uniform_entry()],
        });
        let final_samplers = textures(fin)?;
        let mut entries = vec![
            uniform_entry(),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                count: None,
            },
        ];
        entries.extend(
            final_samplers
                .iter()
                .map(|&(_, binding)| wgpu::BindGroupLayoutEntry {
                    binding,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }),
        );
        let final_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shader pack final"),
            entries: &entries,
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shader pack uniforms"),
            size: std::mem::size_of::<PackUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let gbuffer_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shader pack gbuffers"),
            layout: &gbuffer_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let point = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Shader pack point"),
            ..Default::default()
        });
        let gbuffers = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shader pack gbuffers"),
            bind_group_layouts: &[Some(terrain_layout), Some(&gbuffer_layout)],
            ..Default::default()
        });
        let composite = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shader pack final"),
            bind_group_layouts: &[Some(terrain_layout), Some(&final_layout)],
            ..Default::default()
        });
        let instance_attributes = wgpu::vertex_attr_array![5=>Float32x4];
        let terrain_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<crate::mesher::TerrainVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &TERRAIN_ATTRIBUTES,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: 16,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &instance_attributes,
            }),
        ];
        let target_format = |index: u8| if index == 0 { color_format } else { AUX_FORMAT };
        let build = |p: &CompiledProgram,
                     layout: &wgpu::PipelineLayout,
                     buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
                     targets: &[Option<wgpu::ColorTargetState>],
                     depth: Option<wgpu::DepthStencilState>| {
            let module = |label: &str, wgsl: &str| {
                device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(label),
                    source: wgpu::ShaderSource::Wgsl(wgsl.into()),
                })
            };
            let vs = module(&p.source, &p.vertex_wgsl);
            let fs = module(&p.source, &p.fragment_wgsl);
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&p.source),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &vs,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: depth.as_ref().map(|_| wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: depth,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &fs,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let depth = || wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
            stencil: Default::default(),
            bias: Default::default(),
        };
        let gbuffer_targets = |p: &CompiledProgram, blend: Option<wgpu::BlendState>| {
            p.draw_buffers
                .iter()
                .map(|&i| {
                    Some(wgpu::ColorTargetState {
                        format: target_format(i),
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })
                })
                .collect::<Vec<_>>()
        };
        let terrain_pipeline = build(
            terrain,
            &gbuffers,
            &terrain_buffers,
            &gbuffer_targets(terrain, None),
            Some(depth()),
        );
        let water_pipeline = build(
            water,
            &gbuffers,
            &terrain_buffers,
            &gbuffer_targets(water, Some(wgpu::BlendState::ALPHA_BLENDING)),
            Some(depth()),
        );
        if fin.draw_buffers.first() != Some(&0) {
            return Err(format!("{}: final must write colortex0", fin.source));
        }
        let final_pipeline = build(
            fin,
            &composite,
            &[],
            &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            None,
        );
        Ok(Self {
            terrain: terrain_pipeline,
            water: water_pipeline,
            final_pipeline,
            terrain_buffers: terrain.draw_buffers.clone(),
            water_buffers: water.draw_buffers.clone(),
            final_samplers,
            final_layout,
            uniforms,
            gbuffer_group,
            point,
            color_format,
            targets: None,
        })
    }

    /// (Re)creates the targets for a world of `size`.
    pub fn prepare(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let texture = |label: &str, format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size[0].max(1),
                        height: size[1].max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let mut indices: Vec<u8> = self
            .terrain_buffers
            .iter()
            .chain(&self.water_buffers)
            .copied()
            .chain(
                self.final_samplers
                    .iter()
                    .filter(|(_, b)| (2..18).contains(b))
                    .map(|(_, b)| (b - 2) as u8),
            )
            .collect();
        indices.push(0);
        indices.sort_unstable();
        indices.dedup();
        let color: BTreeMap<u8, wgpu::TextureView> = indices
            .into_iter()
            .map(|i| {
                let format = if i == 0 {
                    self.color_format
                } else {
                    AUX_FORMAT
                };
                (i, texture(&format!("colortex{i}"), format))
            })
            .collect();
        let depth = texture("depthtex0", wgpu::TextureFormat::Depth32Float);
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: self.uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&self.point),
            },
        ];
        for &(_, binding) in &self.final_samplers {
            let view = if binding == 18 {
                &depth
            } else {
                &color[&((binding - 2) as u8)]
            };
            entries.push(wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::TextureView(view),
            });
        }
        let final_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shader pack final"),
            layout: &self.final_layout,
            entries: &entries,
        });
        self.targets = Some(Targets {
            size,
            color,
            depth,
            final_group,
        });
    }

    pub fn upload(&self, queue: &wgpu::Queue, uniforms: &PackUniforms) {
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(uniforms));
    }

    fn targets(&self) -> &Targets {
        self.targets.as_ref().expect("PackRenderer::prepare first")
    }

    /// colortex0 and the depth the rest of the world draws with.
    pub fn world_targets(&self) -> (&wgpu::TextureView, &wgpu::TextureView) {
        let t = self.targets();
        (&t.color[&0], &t.depth)
    }

    /// A pass over the targets of a gbuffers program: the first one of the
    /// frame clears them (colortex0 to `clear`, the others to zero) and the
    /// depth (to 0, reversed Z); later ones load.
    fn gbuffer_pass<'e>(
        &self,
        encoder: &'e mut wgpu::CommandEncoder,
        buffers: &[u8],
        clear: Option<wgpu::Color>,
        label: &str,
    ) -> wgpu::RenderPass<'e> {
        let t = self.targets();
        let attachments: Vec<_> = buffers
            .iter()
            .map(|i| {
                Some(wgpu::RenderPassColorAttachment {
                    view: &t.color[i],
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: match clear {
                            Some(c) if *i == 0 => wgpu::LoadOp::Clear(c),
                            Some(_) => wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            None => wgpu::LoadOp::Load,
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })
            })
            .collect();
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &attachments,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &t.depth,
                depth_ops: Some(wgpu::Operations {
                    load: if clear.is_some() {
                        wgpu::LoadOp::Clear(0.0)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        })
    }

    /// The solid terrain pass: clears every target, binds the pack's terrain
    /// program; the caller draws the sections.
    pub fn terrain_pass<'e>(
        &self,
        encoder: &'e mut wgpu::CommandEncoder,
        clear: wgpu::Color,
        group: &wgpu::BindGroup,
    ) -> wgpu::RenderPass<'e> {
        let mut pass = self.gbuffer_pass(
            encoder,
            &self.terrain_buffers,
            Some(clear),
            "gbuffers_terrain",
        );
        pass.set_pipeline(&self.terrain);
        pass.set_bind_group(0, group, &[]);
        pass.set_bind_group(1, &self.gbuffer_group, &[]);
        pass
    }

    /// The translucent terrain pass, loading the targets.
    pub fn water_pass<'e>(
        &self,
        encoder: &'e mut wgpu::CommandEncoder,
        group: &wgpu::BindGroup,
    ) -> wgpu::RenderPass<'e> {
        let mut pass = self.gbuffer_pass(encoder, &self.water_buffers, None, "gbuffers_water");
        pass.set_pipeline(&self.water);
        pass.set_bind_group(0, group, &[]);
        pass.set_bind_group(1, &self.gbuffer_group, &[]);
        pass
    }

    /// The world pass again on colortex0 and the depth, loading both.
    pub fn world_pass<'e>(&self, encoder: &'e mut wgpu::CommandEncoder) -> wgpu::RenderPass<'e> {
        self.gbuffer_pass(encoder, &[0], None, "Horizon world (shader pack)")
    }

    /// The pack's `final` into `view` (the world's own target).
    pub fn finish(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        group: &wgpu::BindGroup,
    ) {
        let t = self.targets();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("final"),
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
        pass.set_pipeline(&self.final_pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.set_bind_group(1, &t.final_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// What `uniforms` reads besides the frame (worked out in `draw_frame`).
pub(crate) struct PackUniformInputs<'a> {
    pub state: &'a super::PackFrame,
    /// Horizon's clip matrix (projection × bob × view), camera-relative.
    pub view_proj: glam::Mat4,
    pub size: [u32; 2],
    /// The brightness option, 0..1 (`screenBrightness`).
    pub gamma: f32,
    pub fog_color: [f32; 4],
    /// Environmental start/end, render-distance start/end (`Globals::fog`).
    pub fog: [f32; 4],
    pub frame_counter: u32,
}

/// GL clip space from Horizon's: y flipped (textures are y-down here) and
/// reversed-Z 0..1 depth to GL's -1..1 with 1 - d (`horizon_glsl_pack::host`'s
/// `hz_to_gl`). Packs reconstruct positions with `gbufferProjectionInverse`,
/// so their projection is this times Horizon's.
pub(crate) fn gl_clip() -> glam::Mat4 {
    glam::Mat4::from_cols(
        glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, -1.0, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, -2.0, 0.0),
        glam::Vec4::new(0.0, 0.0, 1.0, 1.0),
    )
}

impl PackUniformInputs<'_> {
    pub fn uniforms(&self, frame: &super::Frame) -> PackUniforms {
        use glam::{Mat4, Vec3};
        let s = self.state;
        let model_view = frame.bob * crate::camera::view_matrix(frame.yaw, frame.pitch);
        // view_proj = P × model_view, so P comes back exactly enough.
        let projection = gl_clip() * self.view_proj * model_view.inverse();
        let rotate = |v: Vec3| model_view.transform_vector3(v);
        let sun_angle = frame.sky.sun_angle.to_radians();
        let sun_world = Vec3::new(0.0, sun_angle.cos(), -sun_angle.sin()) * 100.0;
        let sun = rotate(sun_world);
        let moon = -sun;
        // OptiFine's celestial angle: 0 at sunrise, 0.25 at noon.
        let sun_fraction = (frame.sky.sun_angle / 360.0 + 0.25).rem_euclid(1.0);
        let mat = |m: Mat4| m.to_cols_array_2d();
        let v3 = |v: Vec3| v.to_array();
        let identity = Mat4::IDENTITY.to_cols_array_2d();
        let [w, h] = self.size;
        PackUniforms {
            hz_view_proj: mat(self.view_proj),
            gbuffer_model_view: mat(model_view),
            gbuffer_model_view_inverse: mat(model_view.inverse()),
            gbuffer_projection: mat(projection),
            gbuffer_projection_inverse: mat(projection.inverse()),
            gbuffer_previous_model_view: mat(model_view),
            gbuffer_previous_projection: mat(projection),
            shadow_model_view: identity,
            shadow_model_view_inverse: identity,
            shadow_projection: identity,
            shadow_projection_inverse: identity,
            model_view_matrix: mat(model_view),
            projection_matrix: mat(projection),
            camera_position: frame.camera.as_vec3().to_array(),
            previous_camera_position: frame.camera.as_vec3().to_array(),
            sun_position: v3(sun),
            moon_position: v3(moon),
            shadow_light_position: v3(if sun_world.y >= 0.0 { sun } else { moon }),
            up_position: v3(rotate(Vec3::Y * 100.0)),
            fog_color: [self.fog_color[0], self.fog_color[1], self.fog_color[2]],
            sky_color: [
                frame.sky.sky_color[0],
                frame.sky.sky_color[1],
                frame.sky.sky_color[2],
            ],
            entity_color: [0.0; 4],
            eye_brightness: s.eye_brightness,
            eye_brightness_smooth: s.eye_brightness,
            fog_start: self.fog[0].min(self.fog[2]),
            fog_end: self.fog[1].min(self.fog[3]),
            near: 0.05,
            far: frame.distance,
            rain_strength: s.rain_strength,
            wetness: s.rain_strength,
            screen_brightness: self.gamma,
            frame_time_counter: (frame.ticks % 72000) as f32 / 20.0,
            view_width: w as f32,
            view_height: h as f32,
            aspect_ratio: w as f32 / h.max(1) as f32,
            sun_angle: sun_fraction,
            shadow_angle: sun_fraction % 0.5,
            night_vision: s.night_vision,
            blindness: s.blindness,
            is_eye_in_water: i32::from(frame.underwater),
            world_time: s.world_time,
            world_day: s.world_day,
            moon_phase: s.moon_phase,
            frame_counter: (self.frame_counter % 720_720) as i32,
            // Cylindrical fog, as vanilla's terrain fog.
            fog_shape: 1,
            // GL_LINEAR.
            fog_mode: 9729,
            held_block_light_value: s.held_light[0],
            held_block_light_value2: s.held_light[1],
            hide_gui: i32::from(s.hide_gui),
            ..Default::default()
        }
    }
}
