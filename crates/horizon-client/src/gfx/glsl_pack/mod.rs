//! A shader pack's programs draw the overworld (`--shader-pack`, off by
//! default; docs/shader-packs.md). `horizon_glsl_pack` translates the pack and
//! says what it needs (`PackPlan`); this module builds the targets and
//! pipelines and runs the frame: the shadow map, the gbuffers programs over
//! Horizon's geometry, the full-screen passes with their main/alternate
//! buffers, `final` into the world's view, then Horizon's own overlays.
//!
//! Bindings: group 0 is Horizon's own for the geometry (the terrain group,
//! the entity group, nothing for full-screen passes); group 1 is the pack's,
//! with the uniform block at binding 0, textures at
//! `horizon_glsl_pack::host::sampler_binding` and one sampler per
//! `SamplerKind`.

mod debug;
mod frame;
mod mips;
mod pipelines;
mod targets;
mod textures;
mod uniforms;

use std::collections::BTreeMap;

use horizon_glsl_pack::{
    CompiledPack, PackFiles, PackPlan, PackUniforms, ProgramKind, SamplerKind, TextureSource,
};

pub(super) use uniforms::PackUniformInputs;

use horizon_glsl_pack::schedule::Side;
use pipelines::Program;
use targets::{Need, ShadowTargets, Targets};

/// Features a pack's targets use when the adapter has them (32-bit float
/// targets that filter and blend, 11/11/10 float targets).
pub(crate) const FEATURES: wgpu::Features = wgpu::Features::FLOAT32_FILTERABLE
    .union(wgpu::Features::FLOAT32_BLENDABLE)
    .union(wgpu::Features::RG11B10UFLOAT_RENDERABLE);

/// The uniform block, once per kind of pass (the camera's, the light's, the hand's).
pub(super) struct UniformBuffers {
    pub main: wgpu::Buffer,
    pub shadow: wgpu::Buffer,
    pub hand: wgpu::Buffer,
}

pub(crate) struct PackRenderer {
    plan: PackPlan,
    programs: BTreeMap<ProgramKind, Program>,
    samplers: Vec<wgpu::Sampler>,
    uniforms: UniformBuffers,
    formats: [wgpu::TextureFormat; 16],
    needs: [Need; 16],
    screen_format: wgpu::TextureFormat,
    mips: mips::Mips,
    schedule: horizon_glsl_pack::schedule::FrameSchedule,
    /// `HORIZON_SHADER_PACK_DEBUG`: one texture shown instead of the image.
    debug: Option<debug::Debug>,
    targets: Option<Targets>,
    shadow: Option<ShadowTargets>,
    textures: textures::Textures,
    empty_group: wgpu::BindGroup,
    /// The sky box (entity vertices) for `gbuffers_skybasic`.
    sky: wgpu::Buffer,
    /// Frames drawn, for `frameCounter`.
    pub(super) frames: u32,
    pub(super) history: uniforms::History,
}

fn uniform_buffer(device: &wgpu::Device, label: &str) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: std::mem::size_of::<PackUniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Horizon's group 0 layouts, by geometry.
pub(crate) struct Group0Layouts<'a> {
    pub terrain: &'a wgpu::BindGroupLayout,
    pub entity: &'a wgpu::BindGroupLayout,
}

impl PackRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        group0: Group0Layouts,
        screen_format: wgpu::TextureFormat,
        pack: &CompiledPack,
        files: &PackFiles,
    ) -> Result<Self, String> {
        let plan = pack.plan.clone();
        for essential in [ProgramKind::Terrain, ProgramKind::Final] {
            if !pack.programs.contains_key(&essential) {
                return Err(format!("the pack has no {essential:?} program"));
            }
        }
        let features = device.features();
        // colortex0 shares the world's format when the pack's is 8-bit, so
        // Horizon's pipelines can draw what the pack has no program for.
        let formats: [wgpu::TextureFormat; 16] = std::array::from_fn(|i| {
            targets::wgpu_format(
                plan.buffers[i].format,
                (i == 0).then_some(screen_format),
                features,
            )
        });
        let shadow_formats: [wgpu::TextureFormat; 2] = std::array::from_fn(|i| {
            targets::wgpu_format(plan.shadow.color_format[i], None, features)
        });
        let mut needs = [Need::default(); 16];
        needs[0].used = true;
        let mut shadow_colors = [false; 2];
        let mut shadow_used = false;
        for (&kind, program) in &pack.programs {
            for &b in &program.draw_buffers {
                if kind.is_shadow() {
                    if let Some(c) = shadow_colors.get_mut(usize::from(b)) {
                        *c = true;
                    }
                } else if let Some(n) = needs.get_mut(usize::from(b)) {
                    n.used = true;
                    n.two_sides |= kind.is_fullscreen() && kind != ProgramKind::Final;
                }
            }
            for t in &program.textures {
                match t.source {
                    TextureSource::Buffer(b) => {
                        if let Some(n) = needs.get_mut(usize::from(b)) {
                            n.used = true;
                        }
                    }
                    TextureSource::ShadowTex(_) | TextureSource::ShadowTexMips(_) => {
                        shadow_used = true
                    }
                    TextureSource::ShadowColor(c) => {
                        shadow_used = true;
                        if let Some(c) = shadow_colors.get_mut(usize::from(c)) {
                            *c = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        for buffers in plan.mipmaps.values() {
            for &b in buffers {
                if let Some(n) = needs.get_mut(usize::from(b)) {
                    n.mipmaps = true;
                }
            }
        }
        let mut mips = mips::Mips::new(device);
        let has_shadow = pack.programs.contains_key(&ProgramKind::Shadow);
        let shadow = (has_shadow || shadow_used)
            .then(|| ShadowTargets::new(device, &plan.shadow, shadow_colors, features, &mut mips));
        let empty = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shader pack (no group 0)"),
            entries: &[],
        });
        let empty_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shader pack (no group 0)"),
            layout: &empty,
            entries: &[],
        });
        let color = |i: u8| formats[usize::from(i).min(15)];
        let shadow_color = |i: u8| shadow_formats[usize::from(i).min(1)];
        let layouts = pipelines::Layouts {
            terrain: group0.terrain,
            entity: group0.entity,
            empty: &empty,
            color: &color,
            shadow_color: &shadow_color,
            screen: screen_format,
        };
        let mut programs = BTreeMap::new();
        for (&kind, program) in &pack.programs {
            programs.insert(kind, pipelines::build(device, kind, program, &layouts)?);
        }
        let textures = textures::Textures::new(device, queue, &plan, files)?;
        Ok(Self {
            programs,
            samplers: pipelines::samplers(device),
            uniforms: UniformBuffers {
                main: uniform_buffer(device, "Shader pack uniforms"),
                shadow: uniform_buffer(device, "Shader pack shadow uniforms"),
                hand: uniform_buffer(device, "Shader pack hand uniforms"),
            },
            formats,
            needs,
            screen_format,
            mips,
            schedule: horizon_glsl_pack::schedule::FrameSchedule::new(&plan),
            debug: debug::Debug::new(device, screen_format),
            targets: None,
            shadow,
            textures,
            empty_group,
            sky: frame::sky_box(device),
            frames: 0,
            history: Default::default(),
            plan,
        })
    }

    /// The shadow map's clip matrix this frame (Horizon's conventions,
    /// camera-relative), for choosing its casters; `None` without a shadow
    /// program or when the pack does not draw this frame.
    pub fn shadow_clip(&self, frame: &crate::gfx::Frame) -> Option<glam::Mat4> {
        if frame.shader_pack.is_none() || !self.has(ProgramKind::Shadow) {
            return None;
        }
        Some(uniforms::shadow_clip(frame, &self.plan))
    }

    pub fn has(&self, kind: ProgramKind) -> bool {
        self.programs.contains_key(&kind)
    }

    /// colortex0's format, which Horizon's pipelines can draw into when it is
    /// the world's.
    pub fn draws_horizon_pipelines(&self) -> bool {
        self.formats[0] == self.screen_format
    }

    /// (Re)creates the screen-sized targets.
    pub fn prepare(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        self.targets = Some(Targets::new(
            device,
            size,
            &self.needs,
            &self.formats,
            &mut self.mips,
        ));
    }

    fn targets(&self) -> &Targets {
        self.targets.as_ref().expect("PackRenderer::prepare first")
    }

    /// The pack's depth buffer, which Horizon's overlays draw against.
    pub fn depth(&self) -> &wgpu::TextureView {
        &self.targets().depth.view
    }

    /// Group 1 of `program` for a pass that reads each buffer's `sides`.
    fn group(
        &self,
        device: &wgpu::Device,
        program: &Program,
        uniforms: &wgpu::Buffer,
        sides: &[Side; 16],
    ) -> wgpu::BindGroup {
        let targets = self.targets();
        let gbuffers = !program.kind.is_fullscreen();
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: uniforms.as_entire_binding(),
        }];
        for t in &program.textures {
            let view = match t.source {
                TextureSource::Buffer(b) => match targets.buffer(b) {
                    // A pass cannot read a texture it draws into: the other
                    // side when there is one, else nothing.
                    Some(buf) if gbuffers && program.draw_buffers.contains(&b) => {
                        if buf.sides.len() > 1 {
                            &buf.side(sides[usize::from(b)].other()).view
                        } else {
                            &self.textures.black
                        }
                    }
                    Some(buf) => &buf.side(sides[usize::from(b)]).view,
                    None => &self.textures.black,
                },
                // depthtex0 is the gbuffers' attachment: they read last frame's copy.
                TextureSource::Depth(0) if gbuffers => &targets.depth_copies[0].view,
                TextureSource::Depth(0) => &targets.depth.view,
                TextureSource::Depth(n) => &targets.depth_copies[usize::from(n.min(2)) - 1].view,
                TextureSource::ShadowTex(n) => match &self.shadow {
                    Some(s) if n == 0 && !program.kind.is_shadow() => &s.depth.view,
                    Some(s) if !program.kind.is_shadow() => &s.depth_opaque.view,
                    _ => &self.textures.far_depth,
                },
                // Level 0 of the map stands in for the mipmapped copy.
                TextureSource::ShadowTexMips(n) => match &self.shadow {
                    Some(s) if n == 0 && !program.kind.is_shadow() => &s.depth.view,
                    Some(s) if !program.kind.is_shadow() => &s.depth_opaque.view,
                    _ => &self.textures.far_depth,
                },
                TextureSource::ShadowColor(n) => {
                    match self
                        .shadow
                        .as_ref()
                        .and_then(|s| s.colors[usize::from(n.min(1))].as_ref())
                    {
                        Some((img, _)) if !program.kind.is_shadow() => &img.view,
                        _ => &self.textures.white,
                    }
                }
                TextureSource::Noise => &self.textures.noise,
                TextureSource::Normals => &self.textures.normals,
                TextureSource::Specular => &self.textures.black,
                TextureSource::Custom(i) => {
                    self.textures.custom.get(i).unwrap_or(&self.textures.black)
                }
            };
            entries.push(wgpu::BindGroupEntry {
                binding: t.binding,
                resource: wgpu::BindingResource::TextureView(view),
            });
        }
        for &s in &program.samplers {
            let index = SamplerKind::ALL.iter().position(|&k| k == s).unwrap_or(0);
            entries.push(wgpu::BindGroupEntry {
                binding: s.binding(),
                resource: wgpu::BindingResource::Sampler(&self.samplers[index]),
            });
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shader pack pass"),
            layout: &program.layout,
            entries: &entries,
        })
    }
}
