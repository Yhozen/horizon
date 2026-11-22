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
mod flip;
mod frame;
mod mips;
mod pipelines;
mod probation;
mod targets;
mod textures;
mod uniforms;

use std::collections::BTreeMap;

use horizon_glsl_pack::{
    CompiledPack, PackFiles, PackPlan, PackUniforms, ProgramKind, SamplerKind, TextureSource,
};

pub(crate) use probation::Probation;
pub(super) use uniforms::PackUniformInputs;

use horizon_glsl_pack::schedule::Side;
use pipelines::{ComputeProgram, Program};
use targets::{Need, ShadowTargets, Targets};

/// Features a pack's targets can use (32-bit float targets that filter and
/// blend, exact 16-bit normalized targets, 11/11/10 float targets). The stock
/// renderer requires none of these; without them a pack's targets fall back to
/// wider or lossier formats (`targets::wgpu_format`).
pub(super) fn device_features(shader_pack_requested: bool) -> wgpu::Features {
    if shader_pack_requested {
        wgpu::Features::FLOAT32_FILTERABLE
            | wgpu::Features::FLOAT32_BLENDABLE
            | wgpu::Features::TEXTURE_FORMAT_16BIT_NORM
            // 16-bit normalized formats render and blend only with these.
            | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
            | wgpu::Features::RG11B10UFLOAT_RENDERABLE
    } else {
        wgpu::Features::empty()
    }
}

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
    /// `size.buffer.colortexN`.
    sizes: [horizon_glsl_pack::plan::BufferSize; 16],
    /// Copies colortex0 to the screen when the pack has no `final`.
    blit: Option<debug::Debug>,
    /// `scale.<program>` of full-screen passes.
    pub(super) scales: BTreeMap<ProgramKind, horizon_glsl_pack::plan::Scale>,
    mips: mips::Mips,
    schedule: horizon_glsl_pack::schedule::FrameSchedule,
    /// `HORIZON_SHADER_PACK_DEBUG`: one texture shown instead of the image.
    debug: Option<debug::Debug>,
    /// Pack depth upright for Horizon's overlays.
    depth_flip: flip::DepthFlip,
    targets: Option<Targets>,
    shadow: Option<ShadowTargets>,
    textures: textures::Textures,
    empty_group: wgpu::BindGroup,
    /// The sky box (entity vertices) for `gbuffers_skybasic`.
    sky: wgpu::Buffer,
    /// The custom uniform block (group 1, binding 1) and what computes it,
    /// when the pack has custom uniforms.
    custom: Option<CustomBlock>,
    /// Compute programs, by slot (run where the schedule says).
    computes: BTreeMap<horizon_glsl_pack::compute::ComputeSlot, ComputeProgram>,
    /// `bufferObject.N`: zeroed when made, kept across frames; the relative
    /// ones are made again (zeroed) when the size changes.
    storage: BTreeMap<u32, StorageBuffer>,
    /// The `setup` programs ran.
    pub(super) setup_done: bool,
    /// Frames drawn, for `frameCounter`.
    pub(super) frames: u32,
    pub(super) history: uniforms::History,
}

/// The custom uniforms' buffer, their state between frames and a scratch
/// buffer for their bytes.
struct StorageBuffer {
    object: horizon_glsl_pack::compute::BufferObject,
    buffer: Option<wgpu::Buffer>,
}

impl StorageBuffer {
    fn make(&mut self, device: &wgpu::Device, index: u32, size: [u32; 2]) {
        let bytes = self.object.bytes(size[0], size[1]);
        self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("Shader pack bufferObject.{index}")),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }
}

/// What a pipeline's group 1 holds: a render program's or a compute program's.
struct Binds<'a> {
    layout: &'a wgpu::BindGroupLayout,
    textures: &'a [horizon_glsl_pack::TextureBinding],
    samplers: &'a [SamplerKind],
    storage: &'a [horizon_glsl_pack::StorageBinding],
    images: &'a [horizon_glsl_pack::ColorImageBinding],
    /// The side of each colortex the images are (the textures read `sides`).
    image_sides: [Side; 16],
    /// Gbuffers programs (not full-screen passes, not compute).
    gbuffers: bool,
    shadow: bool,
    draw_buffers: &'a [u8],
}

struct CustomBlock {
    buffer: wgpu::Buffer,
    uniforms: horizon_glsl_pack::custom_uniforms::CustomUniforms,
    bytes: Vec<u8>,
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
        resources: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) -> Result<Self, String> {
        let plan = pack.plan.clone();
        if !pack.programs.contains_key(&ProgramKind::Terrain) {
            return Err("the pack has no Terrain program".into());
        }
        let (sizes, skip, mut size_notes) = buffer_sizes(&plan);
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
        for c in pack.computes.values() {
            for img in &c.color_images {
                if let Some(n) = needs.get_mut(usize::from(img.buffer)) {
                    n.used = true;
                    n.storage = true;
                    // Sampled and written in one dispatch: it samples a copy on the other side.
                    n.two_sides |= c
                        .textures
                        .iter()
                        .any(|t| t.source == TextureSource::Buffer(img.buffer));
                }
            }
            for t in &c.textures {
                match t.source {
                    TextureSource::Buffer(b) => {
                        if let Some(n) = needs.get_mut(usize::from(b)) {
                            n.used = true;
                        }
                    }
                    TextureSource::ShadowTex(_)
                    | TextureSource::ShadowTexMips(_)
                    | TextureSource::ShadowColor(_) => shadow_used = true,
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
        let custom = (!pack.custom_uniforms.is_empty()).then(|| CustomBlock {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Shader pack custom uniforms"),
                size: pack.custom_uniforms.size() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            uniforms: pack.custom_uniforms.clone(),
            bytes: Vec::new(),
        });
        let layouts = pipelines::Layouts {
            custom_uniforms: custom.is_some(),
            terrain: group0.terrain,
            entity: group0.entity,
            empty: &empty,
            color: &color,
            shadow_color: &shadow_color,
            screen: screen_format,
        };
        let mut programs = BTreeMap::new();
        for (&kind, program) in pack.programs.iter().filter(|(k, _)| !skip.contains(k)) {
            let state = pack.plan.state(kind);
            programs.insert(
                kind,
                pipelines::build(device, kind, program, &state, &layouts)?,
            );
        }
        let mut computes = BTreeMap::new();
        let float32 = features
            .contains(wgpu::Features::FLOAT32_FILTERABLE | wgpu::Features::FLOAT32_BLENDABLE);
        for (&slot, c) in &pack.computes {
            // A colorimg must be its colortex's own format (colortex0 may be the world's).
            let mismatch = c.color_images.iter().find(|img| {
                targets::storage_format(img.format, float32) != formats[usize::from(img.buffer)]
            });
            if let Some(img) = mismatch {
                log::warn!(
                    "shader pack: {} left out: colortex{} is {:?} here, not {:?}",
                    c.source,
                    img.buffer,
                    formats[usize::from(img.buffer)],
                    img.format
                );
                continue;
            }
            match pipelines::build_compute(device, c, &layouts) {
                Ok(p) => {
                    computes.insert(slot, p);
                }
                Err(e) => log::warn!("shader pack: compute program left out: {e}"),
            }
        }
        let limit = device.limits().max_storage_buffer_binding_size;
        let mut storage = BTreeMap::new();
        for (&index, object) in &plan.buffer_objects {
            let mut b = StorageBuffer {
                object: *object,
                buffer: None,
            };
            if object.relative.is_none() {
                if object.bytes(1, 1) > limit {
                    return Err(format!(
                        "bufferObject.{index}: {} bytes, the device binds at most {limit}",
                        object.size
                    ));
                }
                b.make(device, index, [1, 1]);
            }
            storage.insert(index, b);
        }
        let mut textures = textures::Textures::new(device, queue, &plan, files, resources);
        textures.notes.append(&mut size_notes);
        // No `final`: colortex0 goes to the screen, as OptiFine shows it.
        let blit = (!programs.contains_key(&ProgramKind::Final))
            .then(|| debug::Debug::copy(device, screen_format));
        for note in &textures.notes {
            log::warn!("shader pack texture: {note}");
        }
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
            sizes,
            blit,
            scales: pack
                .programs
                .keys()
                .filter(|k| k.is_fullscreen())
                .filter_map(|&k| Some((k, pack.plan.state(k).scale?)))
                .collect(),
            mips,
            schedule: horizon_glsl_pack::schedule::FrameSchedule::new(&plan),
            debug: debug::Debug::new(device, screen_format),
            depth_flip: flip::DepthFlip::new(device),
            targets: None,
            shadow,
            textures,
            empty_group,
            sky: frame::sky_box(device),
            custom,
            computes,
            storage,
            setup_done: false,
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

    /// Computes the custom uniforms for this frame from the camera's block
    /// (`main`) and writes them; one copy serves every pass.
    pub(super) fn write_custom_uniforms(&mut self, queue: &wgpu::Queue, main: &PackUniforms) {
        let Some(custom) = &mut self.custom else {
            return;
        };
        custom
            .uniforms
            .evaluate(&|name| main.get(name), main.frame_time, &mut custom.bytes);
        queue.write_buffer(&custom.buffer, 0, &custom.bytes);
    }

    /// The terrain sprites' size and the shared atlas's (entities'), for
    /// `atlasSize` / `gtextureSize`.
    pub fn set_atlas_sizes(&mut self, sprite: [u32; 2], entity: [u32; 2]) {
        self.history.atlas = [sprite.map(|v| v as i32), entity.map(|v| v as i32)];
    }

    pub fn has(&self, kind: ProgramKind) -> bool {
        self.programs.contains_key(&kind)
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
            &self.sizes,
            &mut self.mips,
        ));
        for (&index, b) in &mut self.storage {
            if b.object.relative.is_some() {
                b.make(device, index, size);
            }
        }
    }

    fn targets(&self) -> &Targets {
        self.targets.as_ref().expect("PackRenderer::prepare first")
    }

    /// Custom textures that did not load (each reads a 1×1 stand-in), for
    /// the pack's report (not shown there yet: logged when the pack loads).
    pub fn texture_notes(&self) -> &[String] {
        &self.textures.notes
    }

    /// The pack's depth buffer upright, which Horizon's overlays draw against
    /// (filled by [`flip::DepthFlip`] after `final`).
    pub fn depth(&self) -> &wgpu::TextureView {
        &self.targets().overlay_depth.view
    }

    /// Group 1 of `program` for a pass that reads each buffer's `sides`.
    fn group(
        &self,
        device: &wgpu::Device,
        program: &Program,
        uniforms: &wgpu::Buffer,
        sides: &[Side; 16],
    ) -> wgpu::BindGroup {
        let binds = Binds {
            layout: &program.layout,
            textures: &program.textures,
            samplers: &program.samplers,
            storage: &program.storage,
            images: &[],
            image_sides: *sides,
            gbuffers: !program.kind.is_fullscreen(),
            shadow: program.kind.is_shadow(),
            draw_buffers: &program.draw_buffers,
        };
        self.bind_group(device, &binds, uniforms, sides)
    }

    fn compute_group(
        &self,
        device: &wgpu::Device,
        program: &ComputeProgram,
        sides: &[Side; 16],
        image_sides: &[Side; 16],
    ) -> wgpu::BindGroup {
        let binds = Binds {
            layout: &program.layout,
            textures: &program.textures,
            samplers: &program.samplers,
            storage: &program.storage,
            images: &program.color_images,
            image_sides: *image_sides,
            gbuffers: false,
            shadow: false,
            draw_buffers: &[],
        };
        self.bind_group(device, &binds, &self.uniforms.main, sides)
    }

    fn bind_group(
        &self,
        device: &wgpu::Device,
        program: &Binds,
        uniforms: &wgpu::Buffer,
        sides: &[Side; 16],
    ) -> wgpu::BindGroup {
        let targets = self.targets();
        let gbuffers = program.gbuffers;
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: uniforms.as_entire_binding(),
        }];
        if let Some(custom) = &self.custom {
            entries.push(wgpu::BindGroupEntry {
                binding: horizon_glsl_pack::custom_uniforms::CUSTOM_BINDING,
                resource: custom.buffer.as_entire_binding(),
            });
        }
        for t in program.textures {
            // What a binding reads when its texture is missing: zeros of its own type.
            let missing = self.textures.fallback(t.dimension, t.sample);
            let view = match t.source {
                TextureSource::Buffer(b) => match targets.buffer(b) {
                    // A pass cannot read a texture it draws into: the other
                    // side when there is one, else nothing.
                    Some(buf) if gbuffers && program.draw_buffers.contains(&b) => {
                        if buf.sides.len() > 1 {
                            &buf.side(sides[usize::from(b)].other()).view
                        } else {
                            missing
                        }
                    }
                    Some(buf) => &buf.side(sides[usize::from(b)]).view,
                    None => missing,
                },
                // depthtex0 is the gbuffers' attachment: they read last frame's copy.
                TextureSource::Depth(0) if gbuffers => &targets.depth_copies[0].view,
                TextureSource::Depth(0) => &targets.depth.view,
                TextureSource::Depth(n) => &targets.depth_copies[usize::from(n.min(2)) - 1].view,
                TextureSource::ShadowTex(n) => match &self.shadow {
                    Some(s) if n == 0 && !program.shadow => &s.depth.view,
                    Some(s) if !program.shadow => &s.depth_opaque.view,
                    _ => &self.textures.far_depth,
                },
                // Level 0 of the map stands in for the mipmapped copy.
                TextureSource::ShadowTexMips(n) => match &self.shadow {
                    Some(s) if n == 0 && !program.shadow => &s.depth.view,
                    Some(s) if !program.shadow => &s.depth_opaque.view,
                    _ => &self.textures.far_depth,
                },
                TextureSource::ShadowColor(n) => {
                    match self
                        .shadow
                        .as_ref()
                        .and_then(|s| s.colors[usize::from(n.min(1))].as_ref())
                    {
                        Some((img, _)) if !program.shadow => &img.view,
                        _ if t.sample != horizon_glsl_pack::SampleKind::Float => missing,
                        _ => &self.textures.white,
                    }
                }
                TextureSource::Noise => &self.textures.noise,
                TextureSource::Normals => &self.textures.normals,
                TextureSource::Specular => &self.textures.black,
                // At 64 + i, or at the built-in binding it overrides.
                TextureSource::Custom(i) => self
                    .textures
                    .custom
                    .get(i)
                    .and_then(Option::as_ref)
                    .unwrap_or(missing),
            };
            entries.push(wgpu::BindGroupEntry {
                binding: t.binding,
                resource: wgpu::BindingResource::TextureView(view),
            });
        }
        for b in program.storage {
            if let Some(buffer) = self.storage.get(&b.buffer).and_then(|s| s.buffer.as_ref()) {
                entries.push(wgpu::BindGroupEntry {
                    binding: b.binding,
                    resource: buffer.as_entire_binding(),
                });
            }
        }
        for img in program.images {
            if let Some(buf) = targets.buffer(img.buffer) {
                entries.push(wgpu::BindGroupEntry {
                    binding: img.binding,
                    resource: wgpu::BindingResource::TextureView(
                        &buf.side(program.image_sides[usize::from(img.buffer)])
                            .target,
                    ),
                });
            }
        }
        for &s in program.samplers {
            let index = SamplerKind::ALL.iter().position(|&k| k == s).unwrap_or(0);
            entries.push(wgpu::BindGroupEntry {
                binding: s.binding(),
                resource: wgpu::BindingResource::Sampler(&self.samplers[index]),
            });
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shader pack pass"),
            layout: program.layout,
            entries: &entries,
        })
    }
}

/// Each colortex's size, the full-screen passes that cannot run, and report
/// notes. A buffer a gbuffers program draws into keeps the screen's size
/// whatever `size.buffer` says (gbuffers passes share the screen-sized
/// depth buffer); only buffers that full-screen passes alone write take
/// their own size. A full-screen pass whose targets would differ in size is
/// skipped: WebGPU needs one size for all attachments of a pass.
fn buffer_sizes(
    plan: &PackPlan,
) -> (
    [horizon_glsl_pack::plan::BufferSize; 16],
    std::collections::BTreeSet<ProgramKind>,
    Vec<String>,
) {
    use horizon_glsl_pack::plan::BufferSize;
    let mut sizes: [BufferSize; 16] = std::array::from_fn(|i| plan.buffers[i].size);
    let mut notes = Vec::new();
    // colortex0 is also where Horizon's own pipelines draw.
    let mut screen = [false; 16];
    screen[0] = true;
    for (kind, buffers) in &plan.draw_buffers {
        if kind.is_fullscreen() || kind.is_shadow() {
            continue;
        }
        for &b in buffers {
            if let Some(s) = screen.get_mut(usize::from(b)) {
                *s = true;
            }
        }
    }
    for (b, size) in sizes.iter_mut().enumerate() {
        if screen[b] && *size != BufferSize::Screen {
            notes.push(format!(
                "size.buffer.colortex{b} ignored: gbuffers programs draw into it, so it keeps the screen's size"
            ));
            *size = BufferSize::Screen;
        }
    }
    let mut skip = std::collections::BTreeSet::new();
    for (kind, buffers) in &plan.draw_buffers {
        if !kind.is_fullscreen() || *kind == ProgramKind::Final {
            continue;
        }
        let mut distinct: Vec<BufferSize> = Vec::new();
        for &b in buffers {
            let s = sizes[usize::from(b).min(15)];
            if !distinct.contains(&s) {
                distinct.push(s);
            }
        }
        if distinct.len() > 1 {
            notes.push(format!(
                "{kind:?} skipped: its draw buffers {buffers:?} have different sizes ({distinct:?})"
            ));
            skip.insert(*kind);
        }
    }
    (sizes, skip, notes)
}
