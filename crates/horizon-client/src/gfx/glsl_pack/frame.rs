//! One frame of the world with a pack: `Renderer::draw_pack_world`, in the
//! order of `horizon_glsl_pack::schedule` (format notes: shadow, sky, opaque
//! terrain, entities and particles, depthtex2, hand, depthtex1, deferred,
//! translucent terrain, composites, final), then Horizon's overlays.

use horizon_glsl_pack::schedule::{BufferClear, ClearColor, GbufferPhase, Side, Step};
use horizon_glsl_pack::{ProgramKind, host::Geometry};

use super::PackRenderer;
use super::pipelines::Program;
use crate::gfx::{Frame, Renderer, WorldCounts};
use crate::mesher::Vertex;

/// The sky box `gbuffers_skybasic` draws: six inward quads around the camera.
const SKY_QUADS: usize = 6;

pub(super) fn sky_box(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Shader pack sky box"),
        size: (SKY_QUADS * 4 * std::mem::size_of::<Vertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// The sky box's vertices in the sky colour, 256 blocks from the camera.
fn sky_vertices(color: [f32; 3]) -> Vec<Vertex> {
    let r = 256.0;
    let rgba = u32::from_le_bytes([
        (color[0].clamp(0.0, 1.0) * 255.0) as u8,
        (color[1].clamp(0.0, 1.0) * 255.0) as u8,
        (color[2].clamp(0.0, 1.0) * 255.0) as u8,
        255,
    ]);
    // Each face's corners, in an order that faces inward.
    let faces: [[[f32; 3]; 4]; SKY_QUADS] = [
        [[-r, r, -r], [r, r, -r], [r, r, r], [-r, r, r]],
        [[-r, -r, r], [r, -r, r], [r, -r, -r], [-r, -r, -r]],
        [[-r, -r, -r], [r, -r, -r], [r, r, -r], [-r, r, -r]],
        [[r, -r, r], [-r, -r, r], [-r, r, r], [r, r, r]],
        [[-r, -r, r], [-r, -r, -r], [-r, r, -r], [-r, r, r]],
        [[r, -r, -r], [r, -r, r], [r, r, r], [r, r, -r]],
    ];
    faces
        .iter()
        .flat_map(|face| {
            face.iter().map(move |&pos| Vertex {
                pos,
                uv: [0.0, 0.0],
                layer: 2,
                color: rgba,
                light: 0xF0,
                normal: [0, 127, 0, 0],
            })
        })
        .collect()
}

fn attachment(
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> Option<wgpu::RenderPassColorAttachment<'_>> {
    Some(wgpu::RenderPassColorAttachment {
        view,
        resolve_target: None,
        depth_slice: None,
        ops: wgpu::Operations {
            load,
            store: wgpu::StoreOp::Store,
        },
    })
}

fn color(c: [f32; 4]) -> wgpu::Color {
    wgpu::Color {
        r: f64::from(c[0]),
        g: f64::from(c[1]),
        b: f64::from(c[2]),
        a: f64::from(c[3]),
    }
}

impl PackRenderer {
    /// A pass over a gbuffers program's draw buffers (on `sides`) and the
    /// pack's depth, loading both.
    fn gbuffer_pass<'e>(
        &self,
        encoder: &'e mut wgpu::CommandEncoder,
        program: &Program,
        sides: &[Side; 16],
    ) -> wgpu::RenderPass<'e> {
        let t = self.targets();
        let attachments: Vec<_> = program
            .draw_buffers
            .iter()
            .map(|&b| {
                let buffer = t.buffer(b).expect("draw buffers have targets");
                attachment(
                    &buffer.side(sides[usize::from(b)]).target,
                    wgpu::LoadOp::Load,
                )
            })
            .collect();
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(&format!("{:?}", program.kind)),
            color_attachments: &attachments,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &t.depth.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        })
    }

    /// Dispatches the compute program of `slot` (if it built) over a
    /// `size` render, sampling each colortex from `sides`.
    fn dispatch(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        slot: horizon_glsl_pack::compute::ComputeSlot,
        sides: &[Side; 16],
        size: [u32; 2],
    ) {
        let Some(c) = self.computes.get(&slot) else {
            return;
        };
        let groups = c.dispatch.groups(size[0], size[1], c.workgroup_size);
        if groups.contains(&0) {
            return;
        }
        let max = device.limits().max_compute_workgroups_per_dimension;
        // A colortex the program writes as colorimgN and also samples: WebGPU
        // forbids both in one dispatch, so it samples a copy on the other side.
        let mut sample_sides = *sides;
        for img in &c.color_images {
            let b = img.buffer;
            if !c
                .textures
                .iter()
                .any(|t| t.source == horizon_glsl_pack::TextureSource::Buffer(b))
            {
                continue;
            }
            if let Some(buffer) = self.targets().buffer(b)
                && buffer.sides.len() > 1
            {
                let side = sides[usize::from(b)];
                super::targets::copy_whole(
                    encoder,
                    &buffer.side(side).texture,
                    &buffer.side(side.other()).texture,
                );
                sample_sides[usize::from(b)] = side.other();
            }
        }
        let group = self.compute_group(device, c, &sample_sides, sides);
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(&c.source),
            timestamp_writes: None,
        });
        pass.set_pipeline(&c.pipeline);
        pass.set_bind_group(0, &self.empty_group, &[]);
        pass.set_bind_group(1, &group, &[]);
        pass.dispatch_workgroups(groups[0].min(max), groups[1].min(max), groups[2].min(max));
    }

    fn clear(&self, encoder: &mut wgpu::CommandEncoder, clears: &[BufferClear], fog: [f32; 4]) {
        let t = self.targets();
        for c in clears {
            let Some(buffer) = t.buffer(c.buffer) else {
                continue;
            };
            let value = match c.color {
                ClearColor::Fog => color([fog[0], fog[1], fog[2], 1.0]),
                ClearColor::Rgba(rgba) => color(rgba),
            };
            for side in &buffer.sides {
                drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Shader pack clear"),
                    color_attachments: &[attachment(&side.target, wgpu::LoadOp::Clear(value))],
                    ..Default::default()
                }));
            }
        }
    }
}

/// Splits `quads` (from `first`) into runs of plain entity quads, which the
/// pack draws, and Horizon's special ones (beacon beams, glint, cracks),
/// which stay Horizon's: `(start quad, quads, special)`.
fn entity_runs(vertices: &[Vertex]) -> Vec<(u32, u32, bool)> {
    let quads = vertices.as_chunks::<4>().0;
    let mut runs = Vec::new();
    let mut start = 0;
    while start < quads.len() {
        let special = (6..=12).contains(&quads[start][0].layer);
        let run = quads[start..]
            .iter()
            .take_while(|q| (6..=12).contains(&q[0].layer) == special)
            .count();
        runs.push((start as u32, run as u32, special));
        start += run;
    }
    runs
}

impl Renderer {
    /// Draws the world with the pack into `final_view` (the world's own
    /// target) and leaves the pack's depth for the overlays, which it then
    /// draws. The caller skips Horizon's world pass.
    pub(in crate::gfx) fn draw_pack_world(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        frame: &Frame,
        counts: &WorldCounts,
        final_view: &wgpu::TextureView,
    ) {
        let Some(mut pack) = self.glsl_pack.take() else {
            return;
        };
        let Some(state) = frame.shader_pack else {
            self.glsl_pack = Some(pack);
            return;
        };
        let size = [self.config.width, self.config.height];
        let fresh = pack.targets.as_ref().is_none_or(|t| t.size != size);
        pack.prepare(&self.device, size);
        let uniforms = super::PackUniformInputs {
            state: &state,
            view_proj: counts.view_proj,
            hand_view_proj: counts.hand_view_proj,
            size,
            gamma: self.gamma,
            fog_color: counts.fog_color,
            fog: counts.fog,
        }
        .uniforms(frame, &pack.plan, &mut pack.history);
        self.queue
            .write_buffer(&pack.uniforms.main, 0, bytemuck::bytes_of(&uniforms.main));
        pack.write_custom_uniforms(&self.queue, &uniforms.main);
        self.queue.write_buffer(
            &pack.uniforms.shadow,
            0,
            bytemuck::bytes_of(&uniforms.shadow),
        );
        self.queue
            .write_buffer(&pack.uniforms.hand, 0, bytemuck::bytes_of(&uniforms.hand));
        self.queue.write_buffer(
            &pack.sky,
            0,
            bytemuck::cast_slice(&sky_vertices(frame.sky.sky_color)),
        );
        pack.frames = pack.frames.wrapping_add(1);
        pack.images.clear(&self.device, encoder);
        if fresh {
            let initial = pack.schedule.initial_clears.clone();
            pack.clear(encoder, &initial, counts.fog_color);
        }
        // `setup*` programs: once, before the pack's first frame.
        if !pack.setup_done {
            for &slot in &pack.schedule.setup {
                pack.dispatch(&self.device, encoder, slot, &[Side::Main; 16], size);
            }
            pack.setup_done = true;
        }
        let steps = pack.schedule.steps.clone();
        for step in &steps {
            match step {
                Step::Clear(clears) => pack.clear(encoder, clears, counts.fog_color),
                Step::Shadow => self.pack_shadow(&pack, encoder, counts),
                Step::Gbuffers {
                    phase: GbufferPhase::Opaque,
                    sides,
                } => self.pack_opaque(&pack, encoder, frame, counts, sides),
                Step::Gbuffers {
                    phase: GbufferPhase::Translucent,
                    sides,
                } => self.pack_translucent(&pack, encoder, counts, sides),
                Step::Pass(p) => {
                    let Some(program) = pack.programs.get(&p.kind) else {
                        continue;
                    };
                    let t = pack.targets();
                    for &(b, side) in &p.regenerate_mips {
                        if let Some(buffer) = t.buffer(b) {
                            pack.mips
                                .generate(&self.device, encoder, &buffer.side(side).texture);
                        }
                    }
                    let group = pack.group(&self.device, program, &pack.uniforms.main, &p.reads);
                    let attachments: Vec<_> = if p.kind == ProgramKind::Final {
                        vec![attachment(final_view, wgpu::LoadOp::Load)]
                    } else {
                        p.writes
                            .iter()
                            .map(|&(b, side)| {
                                let buffer = t.buffer(b).expect("written buffers have targets");
                                attachment(&buffer.side(side).target, wgpu::LoadOp::Load)
                            })
                            .collect()
                    };
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some(&format!("{:?}", p.kind)),
                        color_attachments: &attachments,
                        ..Default::default()
                    });
                    // `scale.<program>`: the pass draws into part of its targets.
                    if let Some(scale) = pack.scales.get(&p.kind) {
                        let size = match p.writes.first() {
                            Some(&(b, side)) if p.kind != ProgramKind::Final => t
                                .buffer(b)
                                .map(|buf| buf.side(side).texture.size())
                                .map(|e| [e.width, e.height]),
                            _ => Some(t.size),
                        };
                        if let Some([w, h]) = size {
                            let (w, h) = (w as f32, h as f32);
                            let s = scale.scale.clamp(0.0, 1.0);
                            let x = (scale.offset[0].clamp(0.0, 1.0) * w).min(w - 1.0);
                            let y = (scale.offset[1].clamp(0.0, 1.0) * h).min(h - 1.0);
                            let vw = (s * w).min(w - x).max(1.0);
                            let vh = (s * h).min(h - y).max(1.0);
                            // GL's offset counts from the bottom: so do the rows of pack
                            // targets (stored bottom row first), not the upright screen.
                            let y = if p.kind == ProgramKind::Final {
                                (h - y - vh).max(0.0)
                            } else {
                                y
                            };
                            pass.set_viewport(x, y, vw, vh, 0.0, 1.0);
                        }
                    }
                    pass.set_pipeline(&program.pipeline);
                    pass.set_bind_group(0, &pack.empty_group, &[]);
                    pass.set_bind_group(1, &group, &[]);
                    pass.draw(0..4, 0..1);
                }
                Step::Compute { slot, reads } => {
                    pack.dispatch(&self.device, encoder, *slot, reads, size);
                }
                Step::ShadowComp { kind, writes } => {
                    let (Some(program), Some(shadow)) = (pack.programs.get(kind), &pack.shadow)
                    else {
                        continue;
                    };
                    let mut attachments = Vec::new();
                    for &b in writes {
                        let i = usize::from(b.min(1));
                        let Some((img, target)) = &shadow.colors[i] else {
                            continue;
                        };
                        if let Some(copy) = &shadow.copies[i] {
                            super::targets::copy_whole(encoder, &img.texture, &copy.texture);
                        }
                        attachments.push(attachment(target, wgpu::LoadOp::Load));
                    }
                    if attachments.len() != program.draw_buffers.len() {
                        continue;
                    }
                    let main = [horizon_glsl_pack::schedule::Side::Main; 16];
                    let group = pack.group(&self.device, program, &pack.uniforms.main, &main);
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some(&format!("{kind:?}")),
                        color_attachments: &attachments,
                        ..Default::default()
                    });
                    pass.set_pipeline(&program.pipeline);
                    pass.set_bind_group(0, &pack.empty_group, &[]);
                    pass.set_bind_group(1, &group, &[]);
                    pass.draw(0..4, 0..1);
                }
                Step::CopyBack(buffers) => {
                    let t = pack.targets();
                    for &b in buffers {
                        if let Some(buffer) = t.buffer(b)
                            && buffer.sides.len() > 1
                        {
                            super::targets::copy_whole(
                                encoder,
                                &buffer.sides[1].texture,
                                &buffer.sides[0].texture,
                            );
                        }
                    }
                }
            }
        }
        // No `final`: colortex0 (its latest side) goes to the screen.
        if let Some(blit) = &pack.blit
            && let Some(b) = pack.targets().buffer(0)
        {
            let view = &b.side(pack.schedule.latest[0]).view;
            blit.draw(&self.device, &self.queue, encoder, view, final_view, size);
        }
        if let Some(debug) = &pack.debug {
            let t = pack.targets();
            let source = match debug.show {
                super::debug::Show::Color(n) => t
                    .buffer(n)
                    .map(|b| &b.side(pack.schedule.latest[usize::from(n).min(15)]).view),
                super::debug::Show::Depth => Some(&t.depth.view),
                super::debug::Show::Shadow => pack.shadow.as_ref().map(|s| &s.depth.view),
                super::debug::Show::ShadowColor(n) => pack
                    .shadow
                    .as_ref()
                    .and_then(|s| s.colors.get(usize::from(n))?.as_ref())
                    .map(|(img, _)| &img.view),
            };
            if let Some(source) = source {
                debug.draw(&self.device, &self.queue, encoder, source, final_view, size);
            }
        }
        {
            let t = pack.targets();
            pack.depth_flip
                .run(&self.device, encoder, &t.depth.view, &t.overlay_depth.view);
        }
        self.pack_overlays(&pack, encoder, frame, counts, final_view);
        self.glsl_pack = Some(pack);
    }

    fn bind_geometry(&self, pass: &mut wgpu::RenderPass<'_>, geometry: Geometry, hand: bool) {
        match geometry {
            Geometry::Terrain => {
                pass.set_bind_group(0, &self.pack_group, &[]);
                pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.set_vertex_buffer(1, self.instances.slice(..));
            }
            Geometry::Entity => {
                pass.set_bind_group(
                    0,
                    if hand {
                        &self.hand_group
                    } else {
                        &self.entity_group
                    },
                    &[],
                );
                pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.set_vertex_buffer(0, self.entity_buffer.slice(..));
            }
            Geometry::Fullscreen => {}
        }
    }

    /// The light's sections, `count` draws from `first`. In the browser each is
    /// its own draw call: Chrome on a Galaxy S23+ (Adreno 740) loses the device a
    /// few seconds into a shadow pass drawn with `multi_draw_indexed_indirect`,
    /// even in runs of 32, and keeps drawing when the same draws go one by one.
    /// The camera's passes keep the indirect draws. A shadow map holds a few
    /// dozen sections, so the extra calls cost little.
    fn shadow_sections(&self, pass: &mut wgpu::RenderPass<'_>, first: u32, count: u32) {
        if !cfg!(target_arch = "wasm32") || !self.multi_draw {
            self.draw_sections(pass, &self.indirect, 0, first, count);
            return;
        }
        for i in first as usize..(first + count) as usize {
            let (page, _) = self.draw_pages[i];
            let d = self.draws[i];
            pass.set_vertex_buffer(0, self.arena.buffer(page).slice(..));
            if let Some(extra) = self.arena.extra_buffer(page) {
                pass.set_vertex_buffer(2, extra.slice(..));
            }
            pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(d[2]..d[2] + d[0], d[3] as i32, d[4]..d[4] + 1);
        }
    }

    fn pack_shadow(
        &self,
        pack: &PackRenderer,
        encoder: &mut wgpu::CommandEncoder,
        counts: &WorldCounts,
    ) {
        let Some(shadow) = &pack.shadow else {
            return;
        };
        let spec = &pack.plan.shadow;
        let colors: Vec<_> = shadow
            .colors
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                c.as_ref().map(|(_, target)| {
                    let load = if spec.color_clear[i] {
                        wgpu::LoadOp::Clear(color(spec.color_clear_color[i]))
                    } else {
                        wgpu::LoadOp::Load
                    };
                    (i, target, load)
                })
            })
            .collect();
        let sides = [Side::Main; 16];
        fn begin<'e>(
            encoder: &'e mut wgpu::CommandEncoder,
            shadow: &super::targets::ShadowTargets,
            colors: &[(usize, &wgpu::TextureView, wgpu::LoadOp<wgpu::Color>)],
            first: bool,
            program: &Program,
        ) -> wgpu::RenderPass<'e> {
            let attachments: Vec<_> = program
                .draw_buffers
                .iter()
                .filter_map(|&b| {
                    colors
                        .iter()
                        .find(|c| c.0 == usize::from(b))
                        .map(|&(_, target, load)| {
                            attachment(target, if first { load } else { wgpu::LoadOp::Load })
                        })
                })
                .collect();
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &attachments,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &shadow.depth.view,
                    depth_ops: Some(wgpu::Operations {
                        load: if first {
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
        let Some(terrain) = pack.programs.get(&ProgramKind::Shadow) else {
            return;
        };
        let group = pack.group(&self.device, terrain, &pack.uniforms.shadow, &sides);
        {
            let mut pass = begin(encoder, shadow, &colors, true, terrain);
            pass.set_pipeline(&terrain.pipeline);
            self.bind_geometry(&mut pass, Geometry::Terrain, false);
            pass.set_bind_group(1, &group, &[]);
            self.shadow_sections(&mut pass, counts.shadow_first, counts.shadow_solid);
        }
        // Its own pass: the entities program may draw other buffers.
        if let Some(entities) = pack.programs.get(&ProgramKind::ShadowEntities)
            && counts.entity_quads > 0
        {
            let group = pack.group(&self.device, entities, &pack.uniforms.shadow, &sides);
            let mut pass = begin(encoder, shadow, &colors, false, entities);
            pass.set_pipeline(&entities.pipeline);
            self.bind_geometry(&mut pass, Geometry::Entity, false);
            pass.set_bind_group(1, &group, &[]);
            pass.draw_indexed(0..counts.entity_quads * 6, 0, 0..1);
        }
        // shadowtex1 keeps only the opaque casters.
        super::targets::copy_whole(encoder, &shadow.depth.texture, &shadow.depth_opaque.texture);
        if counts.shadow_translucent > 0 {
            let mut pass = begin(encoder, shadow, &colors, false, terrain);
            pass.set_pipeline(&terrain.pipeline);
            self.bind_geometry(&mut pass, Geometry::Terrain, false);
            pass.set_bind_group(1, &group, &[]);
            self.shadow_sections(
                &mut pass,
                counts.shadow_first + counts.shadow_solid,
                counts.shadow_translucent,
            );
        }
        for (i, c) in shadow.colors.iter().enumerate() {
            if let Some((img, _)) = c
                && spec.color_mipmap[i]
            {
                pack.mips.generate(&self.device, encoder, &img.texture);
            }
        }
    }

    fn pack_opaque(
        &self,
        pack: &PackRenderer,
        encoder: &mut wgpu::CommandEncoder,
        frame: &Frame,
        counts: &WorldCounts,
        sides: &[Side; 16],
    ) {
        let t = pack.targets();
        // Reversed Z: the far plane is 0.
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shader pack depth clear"),
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &t.depth.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        }));
        let main = &pack.uniforms.main;
        let program = |kind| pack.programs.get(&kind);
        // The sky box only where vanilla draws a sky (the overworld): the
        // Nether has none, the End its own textured box below.
        if !frame.title
            && frame.skybox
            && let Some(sky) = program(ProgramKind::SkyBasic)
        {
            let group = pack.group(&self.device, sky, main, sides);
            let mut pass = pack.gbuffer_pass(encoder, sky, sides);
            pass.set_pipeline(&sky.pipeline);
            pass.set_bind_group(0, &self.entity_group, &[]);
            pass.set_bind_group(1, &group, &[]);
            pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_vertex_buffer(0, pack.sky.slice(..));
            pass.draw_indexed(0..SKY_QUADS as u32 * 6, 0, 0..1);
        }
        // Textured sky: the End's sky box, then the sun and moon (or the End flash).
        let end_sky = frame.end_sky && !frame.title;
        if (counts.celestial_vertices > 0 || end_sky)
            && let Some(sky) = program(ProgramKind::SkyTextured)
        {
            let group = pack.group(&self.device, sky, main, sides);
            let mut pass = pack.gbuffer_pass(encoder, sky, sides);
            pass.set_pipeline(&sky.pipeline);
            pass.set_bind_group(0, &self.entity_group, &[]);
            pass.set_bind_group(1, &group, &[]);
            pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
            if end_sky {
                self.end_sky.draw(&mut pass);
            }
            if counts.celestial_vertices > 0 {
                pass.set_vertex_buffer(0, self.celestial_buffer.slice(..));
                pass.draw_indexed(0..(counts.celestial_vertices / 4) * 6, 0, 0..1);
            }
        }
        if let Some(terrain) = program(ProgramKind::Terrain) {
            let group = pack.group(&self.device, terrain, main, sides);
            let mut pass = pack.gbuffer_pass(encoder, terrain, sides);
            pass.set_pipeline(&terrain.pipeline);
            self.bind_geometry(&mut pass, Geometry::Terrain, false);
            pass.set_bind_group(1, &group, &[]);
            self.draw_sections(&mut pass, &self.indirect, 0, 0, counts.solid_draws);
        }
        if counts.entity_quads > 0
            && let Some(entities) = program(ProgramKind::Entities)
        {
            let group = pack.group(&self.device, entities, main, sides);
            let mut pass = pack.gbuffer_pass(encoder, entities, sides);
            pass.set_pipeline(&entities.pipeline);
            self.bind_geometry(&mut pass, Geometry::Entity, false);
            pass.set_bind_group(1, &group, &[]);
            let vertices = &self.entity_batch.combined[..counts.entity_quads as usize * 4];
            for (start, quads, special) in entity_runs(vertices) {
                if !special {
                    pass.draw_indexed(start * 6..(start + quads) * 6, 0, 0..1);
                }
            }
        }
        t.copy_depth(encoder, 1);
        if counts.hand_quads > 0
            && let Some(hand) = program(ProgramKind::Hand)
        {
            let group = pack.group(&self.device, hand, &pack.uniforms.hand, sides);
            let mut pass = pack.gbuffer_pass(encoder, hand, sides);
            pass.set_pipeline(&hand.pipeline);
            self.bind_geometry(&mut pass, Geometry::Entity, true);
            pass.set_bind_group(1, &group, &[]);
            let base = ((counts.world_quads + counts.additive_quads) * 4) as i32;
            for (start, quads, special) in entity_runs(frame.hand) {
                if !special {
                    pass.draw_indexed(start * 6..(start + quads) * 6, base, 0..1);
                }
            }
        }
        t.copy_depth(encoder, 0);
    }

    fn pack_translucent(
        &self,
        pack: &PackRenderer,
        encoder: &mut wgpu::CommandEncoder,
        counts: &WorldCounts,
        sides: &[Side; 16],
    ) {
        // BoatRenderer's water patch (Horizon's depth-only pipeline) is not drawn: it draws
        // upright, and pack targets are stored bottom row first.
        if counts.translucent_draws > 0
            && let Some(water) = pack.programs.get(&ProgramKind::Water)
        {
            let group = pack.group(&self.device, water, &pack.uniforms.main, sides);
            let mut pass = pack.gbuffer_pass(encoder, water, sides);
            pass.set_pipeline(&water.pipeline);
            self.bind_geometry(&mut pass, Geometry::Terrain, false);
            pass.set_bind_group(1, &group, &[]);
            self.draw_sections(
                &mut pass,
                &self.indirect,
                0,
                counts.solid_draws,
                counts.translucent_draws,
            );
        }
    }

    /// Horizon's own overlays over the pack's image, against its depth:
    /// outlines, cracks, glint, beacon beams, lightning, the border, weather,
    /// see-through text.
    fn pack_overlays(
        &self,
        pack: &PackRenderer,
        encoder: &mut wgpu::CommandEncoder,
        frame: &Frame,
        counts: &WorldCounts,
        view: &wgpu::TextureView,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Horizon overlays (shader pack)"),
            color_attachments: &[attachment(view, wgpu::LoadOp::Load)],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: pack.depth(),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        if counts.entity_quads > 0 {
            pass.set_bind_group(0, &self.entity_group, &[]);
            pass.set_vertex_buffer(0, self.entity_buffer.slice(..));
            let vertices = &self.entity_batch.combined[..counts.entity_quads as usize * 4];
            let entities = !pack.has(ProgramKind::Entities);
            for (start, quads, special) in entity_runs(vertices) {
                if special || entities {
                    pass.set_pipeline(if special {
                        &self.entity_pipeline
                    } else {
                        &self.plain_entity_pipeline
                    });
                    pass.draw_indexed(start * 6..(start + quads) * 6, 0, 0..1);
                }
            }
        }
        // The hand's glint and other special quads (the pack drew the rest).
        if counts.hand_quads > 0 && pack.has(ProgramKind::Hand) {
            pass.set_bind_group(0, &self.hand_group, &[]);
            pass.set_vertex_buffer(0, self.entity_buffer.slice(..));
            pass.set_pipeline(&self.entity_pipeline);
            let base = ((counts.world_quads + counts.additive_quads) * 4) as i32;
            for (start, quads, special) in entity_runs(frame.hand) {
                if special {
                    pass.draw_indexed(start * 6..(start + quads) * 6, base, 0..1);
                }
            }
        }
        self.weather.draw(&mut pass, &self.entity_group);
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        let crumbling = [
            (counts.crumbling_quads, counts.entity_quads),
            (
                counts.translucent_crumbling_quads,
                counts.entity_quads + counts.crumbling_quads,
            ),
        ];
        for (quads, base) in crumbling {
            if quads > 0 {
                pass.set_pipeline(&self.crumbling_pipeline);
                pass.set_bind_group(0, &self.entity_group, &[]);
                pass.set_vertex_buffer(0, self.entity_buffer.slice(..));
                pass.draw_indexed(0..quads * 6, (base * 4) as i32, 0..1);
            }
        }
        if !frame.lightning.bolts.is_empty() {
            pass.set_pipeline(&self.lightning_pipeline);
            pass.set_bind_group(0, &self.entity_group, &[]);
            self.lightning.draw(&mut pass, &frame.lightning);
            pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        }
        if counts.additive_quads > 0 {
            pass.set_pipeline(&self.additive_pipeline);
            pass.set_bind_group(0, &self.entity_group, &[]);
            pass.set_vertex_buffer(0, self.entity_buffer.slice(..));
            pass.draw_indexed(
                0..counts.additive_quads * 6,
                (counts.world_quads * 4) as i32,
                0..1,
            );
        }
        pass.set_bind_group(0, &self.group, &[]);
        if counts.outline_edges > 0 {
            pass.set_pipeline(&self.lines);
            pass.set_vertex_buffer(0, self.outline.slice(..));
            pass.draw(0..4, 0..counts.outline_edges);
        }
        if counts.debug_depth > 0 {
            pass.set_pipeline(&self.debug_lines);
            pass.set_vertex_buffer(0, self.debug_line_buffer.slice(..));
            pass.draw(0..4, 0..counts.debug_depth);
        }
        if counts.debug_top > 0 {
            pass.set_pipeline(&self.debug_lines_on_top);
            pass.set_vertex_buffer(0, self.debug_line_buffer.slice(..));
            pass.draw(
                0..4,
                counts.debug_depth..counts.debug_depth + counts.debug_top,
            );
        }
        if let Some(b) = frame.border.as_ref() {
            pass.set_pipeline(&self.border_pipeline);
            pass.set_bind_group(0, &self.border_group, &[]);
            pass.set_vertex_buffer(0, self.border_vertices.slice(..));
            for &side in &b.sides {
                let first = side as u32 * 6;
                pass.draw_indexed(first..first + 6, 0, 0..1);
            }
            pass.set_bind_group(0, &self.group, &[]);
        }
        if counts.see_through_quads > 0 {
            pass.set_pipeline(&self.see_through_pipeline);
            pass.set_bind_group(0, &self.entity_group, &[]);
            pass.set_vertex_buffer(0, self.entity_buffer.slice(..));
            pass.draw_indexed(
                0..counts.see_through_quads * 6,
                ((counts.world_quads + counts.additive_quads + counts.hand_quads) * 4) as i32,
                0..1,
            );
        }
    }
}
