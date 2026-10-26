//! Animated block sprites, drawn on the GPU as vanilla 26.3 draws them:
//! `SpriteContents.AnimatedTexture` keeps every frame in video memory, and
//! each tick `animate_sprite_blit` or `animate_sprite_interpolate` draws the
//! frame shown (mixed with the next one for `interpolate`) into the atlas.
//!
//! Every frame's mip chain is uploaded once, at the sprite's own size. A tick
//! that changes sprites writes one small job per changed sprite and runs one
//! compute dispatch per mip level (`animate.wgsl`), which repeats the chain
//! into the array's larger layers by whole pixels and mixes interpolated
//! frames. Blending on the CPU at the array's size and copying every changed
//! sprite level by level (a copy command each, a blit each in Mesa) was about
//! a fifth of the render thread on an integrated GPU.
use super::upload::Uploads;
use crate::mipmap::Settings;
use crate::sprites::{FrameBlend, SpriteAnimation};
#[allow(unused_imports)]
use horizon_core::fs::PathExt as _;
use wgpu::util::DeviceExt;

/// Bytes of `animate.wgsl`'s `Job`.
const JOB_BYTES: usize = 24;
/// `animate.wgsl`'s workgroup edge.
const WORKGROUP: u32 = 8;

struct Sprite {
    animation: SpriteAnimation,
    /// What its layer shows (`None`: the frame the array was created with,
    /// rewritten on the first tick).
    shown: Option<FrameBlend>,
    /// Its layer in its sprite array, and its tile in the layer's grid.
    layer: u32,
    tile: u32,
    /// Each frame's mip chain: its offset in words in the group's frames.
    frames: Vec<u32>,
    /// Edge of the chains' first level.
    edge: u32,
    group: usize,
}

/// The animated sprites of one sprite array whose frames share one buffer.
struct Group {
    /// Where its jobs start in `SpriteAnimator::jobs`, in bytes.
    offset: u64,
    /// One bind group per mip level of the array.
    levels: Vec<wgpu::BindGroup>,
    /// This tick's jobs.
    jobs: Vec<u8>,
}

pub(super) struct SpriteAnimator {
    sprites: Vec<Sprite>,
    groups: Vec<Group>,
    jobs: wgpu::Buffer,
    pipeline: wgpu::ComputePipeline,
    /// Edge of the arrays' layers.
    size: u32,
    /// The sprites `collect` found changed this tick.
    changed: Vec<usize>,
    /// The browser build's CPU animation (`Cpu`).
    cpu: Option<Cpu>,
}

/// The browser build blends the changed sprites on the CPU and writes them
/// into their layers through the queue: on a software adapter the compute
/// pass, however small its dispatches, cost more than all of the writes.
struct Cpu {
    /// Every sprite's frames' chains, as the frames buffer holds them.
    frames: Vec<Vec<Vec<u8>>>,
    /// Each group's sprite array.
    textures: Vec<wgpu::Texture>,
    grid: u32,
}

impl SpriteAnimator {
    /// `textures` are the sprite arrays, `split` layers each of `grid` by
    /// `grid` sprites `size` pixels wide; `animations` hold their frames at
    /// that size.
    pub fn new(
        device: &wgpu::Device,
        animations: &[SpriteAnimation],
        size: u32,
        settings: &[Settings],
        textures: &[wgpu::Texture],
        split: u32,
        grid: u32,
    ) -> Self {
        let cpu = cfg!(target_arch = "wasm32");
        Self::with_path(
            device, animations, size, settings, textures, split, grid, cpu,
        )
    }

    /// `new`, blending on the CPU (`Cpu`) or in the compute pass.
    #[allow(clippy::too_many_arguments)]
    fn with_path(
        device: &wgpu::Device,
        animations: &[SpriteAnimation],
        size: u32,
        settings: &[Settings],
        textures: &[wgpu::Texture],
        split: u32,
        grid: u32,
        cpu_path: bool,
    ) -> Self {
        let per = grid * grid;
        let limits = device.limits();
        let limit = limits
            .max_storage_buffer_binding_size
            .min(limits.max_buffer_size);
        let mut frames: Vec<Vec<u8>> = Vec::new();
        // Each group's sprite array and frames buffer.
        let mut keys: Vec<(u32, usize)> = Vec::new();
        let mut counts: Vec<usize> = Vec::new();
        let mut sprites = Vec::with_capacity(animations.len());
        let mut cpu_frames = Vec::new();
        for animation in animations {
            let chains: Vec<_> = (0..animation.frame_count())
                .map(|frame| source_chain(animation.pixels(frame), size, settings, animation.layer))
                .collect();
            let bytes: u64 = chains.iter().map(|(chain, _)| chain.len() as u64).sum();
            if bytes > limit {
                log::warn!(
                    "Horizon: layer {}'s animation ({bytes} bytes) does not fit in a storage buffer; it stays still",
                    animation.layer
                );
                continue;
            }
            if frames
                .last()
                .is_none_or(|data| data.len() as u64 + bytes > limit)
            {
                frames.push(Vec::new());
            }
            let buffer = frames.len() - 1;
            let data = &mut frames[buffer];
            let offsets = chains
                .iter()
                .map(|(chain, _)| {
                    let offset = (data.len() / 4) as u32;
                    data.extend_from_slice(chain);
                    offset
                })
                .collect();
            if cpu_path {
                cpu_frames.push(chains.iter().map(|(chain, _)| chain.clone()).collect());
            }
            let key = (animation.layer / (split * per), buffer);
            let group = keys.iter().position(|&k| k == key).unwrap_or_else(|| {
                keys.push(key);
                counts.push(0);
                keys.len() - 1
            });
            counts[group] += 1;
            sprites.push(Sprite {
                animation: animation.clone(),
                shown: None,
                layer: animation.layer % (split * per) / per,
                tile: animation.layer % per,
                frames: offsets,
                edge: chains[0].1,
                group,
            });
        }
        let frames: Vec<_> = frames
            .iter()
            .map(|data| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Animated sprite frames"),
                    contents: data,
                    usage: wgpu::BufferUsages::STORAGE,
                })
            })
            .collect();
        let align = u64::from(limits.min_storage_buffer_offset_alignment);
        let mut offsets = Vec::with_capacity(counts.len());
        let mut total = 0;
        for &count in &counts {
            offsets.push(total);
            total = (total + (count * JOB_BYTES) as u64).next_multiple_of(align);
        }
        let jobs = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animated sprite jobs"),
            size: total.max(align),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Animated sprites"),
            source: wgpu::ShaderSource::Wgsl(include_str!("animate.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Animated sprites"),
            layout: None,
            module: &module,
            entry_point: Some("animate"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[("GRID", f64::from(grid))],
                ..Default::default()
            },
            cache: None,
        });
        let layout = pipeline.get_bind_group_layout(0);
        let groups = keys
            .iter()
            .zip(&counts)
            .zip(offsets)
            .map(|((&(array, buffer), &count), offset)| {
                let texture = &textures[array as usize];
                let levels = (0..texture.mip_level_count())
                    .map(|level| {
                        let view = texture.create_view(&wgpu::TextureViewDescriptor {
                            label: Some("Animated sprites"),
                            dimension: Some(wgpu::TextureViewDimension::D2Array),
                            base_mip_level: level,
                            mip_level_count: Some(1),
                            ..Default::default()
                        });
                        device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("Animated sprites"),
                            layout: &layout,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: frames[buffer].as_entire_binding(),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                        buffer: &jobs,
                                        offset,
                                        size: wgpu::BufferSize::new((count * JOB_BYTES) as u64),
                                    }),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 2,
                                    resource: wgpu::BindingResource::TextureView(&view),
                                },
                            ],
                        })
                    })
                    .collect();
                Group {
                    offset,
                    levels,
                    jobs: Vec::with_capacity(count * JOB_BYTES),
                }
            })
            .collect();
        let cpu = cpu_path.then(|| Cpu {
            frames: cpu_frames,
            textures: keys
                .iter()
                .map(|&(array, _)| textures[array as usize].clone())
                .collect(),
            grid,
        });
        Self {
            sprites,
            groups,
            jobs,
            pipeline,
            size,
            changed: Vec::new(),
            cpu,
        }
    }

    /// Show every sprite's frame at `tick`, rewriting only the sprites whose
    /// frame or mix changed (`SpriteContents.AnimationState.tick`).
    pub fn animate(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        uploads: &mut Uploads,
        queue: &wgpu::Queue,
        tick: u64,
    ) {
        if self.collect(tick) == 0 {
            return;
        }
        if let Some(cpu) = &self.cpu {
            for group in &mut self.groups {
                group.jobs.clear();
            }
            for &i in &self.changed {
                let sprite = &self.sprites[i];
                let blend = sprite.shown.expect("changed sprites show a blend");
                let frames = &cpu.frames[i];
                let chain = blend_chain(
                    &frames[blend.frame],
                    &frames[blend.next],
                    blend.millis,
                    sprite.edge,
                    self.size,
                );
                let texture = &cpu.textures[sprite.group];
                let mut start = 0;
                for level in 0..texture.mip_level_count() {
                    let width = self.size >> level;
                    let bytes = (width * width * 4) as usize;
                    // Written at once, ahead of this frame's commands, as the
                    // copies at its start would be; the browser streams queue
                    // writes without a staging buffer of their own.
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture,
                            mip_level: level,
                            origin: wgpu::Origin3d {
                                x: sprite.tile % cpu.grid * width,
                                y: sprite.tile / cpu.grid * width,
                                z: sprite.layer,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        &chain[start..start + bytes],
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(width * 4),
                            rows_per_image: Some(width),
                        },
                        wgpu::Extent3d {
                            width,
                            height: width,
                            depth_or_array_layers: 1,
                        },
                    );
                    start += bytes;
                }
            }
            return;
        }
        for group in &self.groups {
            uploads.buffer(encoder, &self.jobs, group.offset, &group.jobs);
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Animated sprites"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        for group in &mut self.groups {
            let count = (group.jobs.len() / JOB_BYTES) as u32;
            if count == 0 {
                continue;
            }
            for (level, bind) in group.levels.iter().enumerate() {
                let groups = (self.size >> level).div_ceil(WORKGROUP);
                pass.set_bind_group(0, bind, &[]);
                pass.dispatch_workgroups(groups, groups, count);
            }
            group.jobs.clear();
        }
    }

    /// Queue a job for every sprite whose blend at `tick` differs from what
    /// its layer shows; the number of jobs.
    fn collect(&mut self, tick: u64) -> usize {
        let mut count = 0;
        self.changed.clear();
        for (i, sprite) in self.sprites.iter_mut().enumerate() {
            let blend = sprite.animation.blend(tick);
            if sprite.shown == Some(blend) {
                continue;
            }
            sprite.shown = Some(blend);
            self.changed.push(i);
            let job = [
                sprite.layer,
                sprite.tile,
                sprite.frames[blend.frame],
                sprite.frames[blend.next],
                blend.millis,
                sprite.edge,
            ];
            let jobs = &mut self.groups[sprite.group].jobs;
            for word in job {
                jobs.extend_from_slice(&word.to_le_bytes());
            }
            count += 1;
        }
        count
    }
}

/// The mip chain a frame's layer is made of, and its first level's edge.
/// `mipmap::generate` makes a sprite scaled up by whole pixels to the
/// array's size from its own size's chain, its first level repeated into the
/// larger levels: that chain, which the shader repeats. Any other sprite: its
/// chain at the array's size.
fn source_chain(pixels: &[u8], size: u32, settings: &[Settings], layer: u32) -> (Vec<u8>, u32) {
    let native = settings.get(layer as usize).map_or(0, |s| s.native_size);
    let edge = if native.is_power_of_two() && native < size {
        native
    } else {
        size
    };
    let frame = crate::sprites::resample(pixels, size, edge);
    (super::mip_chain(&frame, edge, settings, layer, 1).0, edge)
}

/// `animate.wgsl` on the CPU: `chain` (first level `edge` wide) as the
/// full chain of a `size` layer, mixed `millis` thousandths of the way
/// towards `next`. The chain is mixed once, at its own size, then its first
/// level repeated by whole pixels into the larger levels.
fn blend_chain(chain: &[u8], next: &[u8], millis: u32, edge: u32, size: u32) -> Vec<u8> {
    let mixed: Vec<u8>;
    let source = if millis == 0 {
        chain
    } else {
        mixed = chain
            .iter()
            .zip(next)
            .map(|(&a, &b)| {
                ((u32::from(a) * (1000 - millis) + u32::from(b) * millis + 500) / 1000) as u8
            })
            .collect();
        &mixed
    };
    let levels = size.trailing_zeros() + 1;
    let mut out = Vec::with_capacity((0..levels).map(|l| ((size >> l).pow(2) * 4) as usize).sum());
    for level in 0..levels {
        let width = (size >> level) as usize;
        let edge = edge as usize;
        if width >= edge {
            let scale = width / edge;
            for y in 0..edge {
                let row = out.len();
                for texel in source[y * edge * 4..(y + 1) * edge * 4].as_chunks::<4>().0 {
                    for _ in 0..scale {
                        out.extend_from_slice(texel);
                    }
                }
                for _ in 1..scale {
                    out.extend_from_within(row..row + width * 4);
                }
            }
        } else {
            let first = 4 * (edge * edge - width * width) / 3;
            out.extend_from_slice(&source[first * 4..(first + width * width) * 4]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plain per-texel form of `blend_chain` (and of `animate.wgsl`).
    /// `animate.wgsl` on the CPU: `chain` (first level `edge` wide) as the
    /// full chain of a `size` layer, mixed `millis` thousandths of the way
    /// towards `next`.
    fn reference(chain: &[u8], next: &[u8], millis: u32, edge: u32, size: u32) -> Vec<u8> {
        let levels = size.trailing_zeros() + 1;
        let mut out = Vec::new();
        for level in 0..levels {
            let width = size >> level;
            for y in 0..width {
                for x in 0..width {
                    let texel = if width >= edge {
                        let scale = width / edge;
                        y / scale * edge + x / scale
                    } else {
                        4 * (edge * edge - width * width) / 3 + y * width + x
                    } as usize;
                    for c in 0..4 {
                        let (a, b) = (
                            u32::from(chain[texel * 4 + c]),
                            u32::from(next[texel * 4 + c]),
                        );
                        out.push(((a * (1000 - millis) + b * millis + 500) / 1000) as u8);
                    }
                }
            }
        }
        out
    }

    #[test]
    fn blend_chain_matches_the_per_texel_reference() {
        for (edge, size) in [(8u32, 32u32), (16, 16), (4, 64), (32, 32)] {
            let chain_len: usize = (0..edge.trailing_zeros() + 1)
                .map(|l| ((edge >> l) * (edge >> l) * 4) as usize)
                .sum();
            let chain: Vec<u8> = (0..chain_len).map(|i| (i * 37 % 251) as u8).collect();
            let next: Vec<u8> = (0..chain_len).map(|i| (i * 91 % 241) as u8).collect();
            for millis in [0, 1, 333, 500, 999] {
                assert_eq!(
                    blend_chain(&chain, &next, millis, edge, size),
                    reference(&chain, &next, millis, edge, size),
                    "edge {edge} size {size} millis {millis}"
                );
            }
        }
    }
    use crate::mipmap::Strategy;

    fn frame(size: u32, seed: u32) -> Vec<u8> {
        (0..size * size)
            .flat_map(|i| {
                let v = i
                    .wrapping_mul(2654435761)
                    .wrapping_add(seed.wrapping_mul(40503));
                let alpha = match v >> 29 {
                    0 => 0,
                    1 => 77,
                    _ => 255,
                };
                [(v >> 3) as u8, (v >> 11) as u8, (v >> 19) as u8, alpha]
            })
            .collect()
    }

    fn settings(native: u32, strategy: Strategy) -> Settings {
        Settings {
            native_size: native,
            strategy,
            alpha_cutoff_bias: -0.1,
            ..Default::default()
        }
    }

    #[test]
    fn source_chains_repeat_into_the_array_chain() {
        // Sprites scaled up by whole pixels, at the array's size, and of a
        // size that is not a power of two (scaled up to the next one).
        for (native, size) in [(16, 16), (16, 64), (8, 32), (32, 64), (1, 16), (24, 32)] {
            for strategy in [
                Strategy::Auto,
                Strategy::Mean,
                Strategy::Cutout,
                Strategy::StrictCutout,
                Strategy::DarkCutout,
            ] {
                for item in [false, true] {
                    let settings = [
                        Settings::default(),
                        Settings {
                            item,
                            ..settings(native, strategy)
                        },
                    ];
                    let pixels = crate::sprites::resample(&frame(native, 3), native, size);
                    let (expected, _) = super::super::mip_chain(&pixels, size, &settings, 1, 1);
                    let (chain, edge) = source_chain(&pixels, size, &settings, 1);
                    let fits = native.is_power_of_two() && native < size;
                    assert_eq!(edge, if fits { native } else { size });
                    assert_eq!(
                        blend_chain(&chain, &chain, 0, edge, size),
                        expected,
                        "{native} in {size}, {strategy:?}, item {item}"
                    );
                }
            }
        }
    }

    #[test]
    fn real_animation_chains_repeat_into_the_array_chain() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/horizon-client")
            .join(horizon_core::VANILLA_VERSION);
        if !root
            .join("assets/minecraft/textures/block/water_still.png")
            .exists_vfs()
        {
            return;
        }
        let mut sprites = crate::sprites::Sprites::default();
        for name in [
            "water_still",
            "water_flow",
            "lava_still",
            "fire_0",
            "nether_portal",
            "sea_lantern",
            "prismarine",
            "magma",
            "kelp",
            "straw_bed",
        ] {
            sprites
                .insert(&root, &format!("minecraft:block/{name}"))
                .unwrap();
        }
        sprites.finish();
        assert_eq!(sprites.size, 64);
        assert_eq!(sprites.animations.len(), 9);
        for animation in &sprites.animations {
            for frame in 0..animation.frame_count() {
                let pixels = animation.pixels(frame);
                let (expected, _) =
                    super::super::mip_chain(pixels, 64, &sprites.mips, animation.layer, 1);
                let (chain, edge) = source_chain(pixels, 64, &sprites.mips, animation.layer);
                assert!(edge < 64, "layer {}", animation.layer);
                assert_eq!(blend_chain(&chain, &chain, 0, edge, 64), expected);
            }
        }
    }

    #[test]
    fn rounding_matches_f32_mix_but_for_exact_halves() {
        // The CPU blend this replaced: f32 mix, then rounded. They differ
        // only where the exact mix is a half, which f32 rounded either way.
        for millis in 1..1000u32 {
            let t = millis as f32 / 1000.0;
            for a in 0..=255u32 {
                for b in (0..=255u32).step_by(3) {
                    let f = ((a as f32 / 255.0) * (1.0 - t) + (b as f32 / 255.0) * t) * 255.0;
                    let exact = a * (1000 - millis) + b * millis;
                    let ours = (exact + 500) / 1000;
                    if exact % 1000 != 500 {
                        assert_eq!(ours, f.round() as u32, "{a} {b} {millis}");
                    }
                }
            }
        }
    }

    fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
        pollster::block_on(adapter.request_device(&Default::default())).ok()
    }

    /// Every layer's full mip chain, layer major.
    fn read_back(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
    ) -> Vec<Vec<u8>> {
        let layers = texture.depth_or_array_layers();
        let mut out = vec![Vec::new(); layers as usize];
        for level in 0..texture.mip_level_count() {
            let edge = texture.width() >> level;
            let padded = (edge * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: u64::from(padded * edge * layers),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: level,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded),
                        rows_per_image: Some(edge),
                    },
                },
                wgpu::Extent3d {
                    width: edge,
                    height: edge,
                    depth_or_array_layers: layers,
                },
            );
            queue.submit([encoder.finish()]);
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, |r| r.unwrap());
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            let data = buffer.slice(..).get_mapped_range().unwrap();
            for (layer, image) in data.chunks((padded * edge) as usize).enumerate() {
                for row in image.chunks(padded as usize) {
                    out[layer].extend_from_slice(&row[..edge as usize * 4]);
                }
            }
        }
        out
    }

    /// The full mip chain of the sprite in `tile` of a `grid` by `grid`
    /// layer's chain (`read_back`'s), sprites `size` wide.
    fn untile(layer: &[u8], size: u32, grid: u32, tile: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut level_start = 0;
        for level in 0..size.trailing_zeros() + 1 {
            let edge = (size >> level) as usize;
            let row = edge * grid as usize * 4;
            let (x, y) = ((tile % grid) as usize * edge, (tile / grid) as usize * edge);
            for r in 0..edge {
                let at = level_start + (y + r) * row + x * 4;
                out.extend_from_slice(&layer[at..at + edge * 4]);
            }
            level_start += row * edge * grid as usize;
        }
        out
    }

    #[test]
    fn gpu_layers_match_the_cpu_chains_every_tick() {
        for grid in [1, 2] {
            for cpu in [false, true] {
                gpu_layers_match_the_cpu_chains(grid, cpu);
            }
        }
    }

    fn gpu_layers_match_the_cpu_chains(grid: u32, cpu: bool) {
        let Some((device, queue)) = device() else {
            return;
        };
        let size = 32;
        // Layers 1 and 3: 8-pixel sprites with and without interpolation;
        // layer 4: one at the array's size. Two layers per array.
        let clips = [
            (
                1,
                8,
                r#"{"frametime":3,"interpolate":true,"frames":[0,2,1]}"#,
            ),
            (3, 8, r#"{"frametime":2}"#),
            (4, 32, r#"{"frametime":5,"interpolate":true}"#),
        ];
        let mut mips = vec![Settings::default(); 6];
        let animations: Vec<_> = clips
            .iter()
            .map(|&(layer, native, metadata)| {
                mips[layer as usize] = settings(native, Strategy::Cutout);
                let frames = (0..3)
                    .map(|f| crate::sprites::resample(&frame(native, layer * 7 + f), native, size))
                    .collect();
                crate::sprites::test_frames(layer, frames, metadata)
            })
            .collect();
        let split = 2;
        let textures: Vec<_> = (0..3)
            .map(|_| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d {
                        width: size * grid,
                        height: size * grid,
                        depth_or_array_layers: split,
                    },
                    mip_level_count: size.trailing_zeros() + 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::STORAGE_BINDING
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                })
            })
            .collect();
        let mut animator = SpriteAnimator::with_path(
            &device,
            &animations,
            size,
            &mips,
            &textures,
            split,
            grid,
            cpu,
        );
        let mut uploads = Uploads::new(&device);
        let mut writes = Vec::new();
        for tick in [0, 0, 1, 2, 3, 4, 7, 7, 12, 30] {
            let mut encoder = device.create_command_encoder(&Default::default());
            let shown: Vec<_> = animator.sprites.iter().map(|s| s.shown).collect();
            animator.animate(&mut encoder, &mut uploads, &queue, tick);
            writes.push(
                animator
                    .sprites
                    .iter()
                    .zip(shown)
                    .filter(|(s, shown)| s.shown != *shown)
                    .count(),
            );
            uploads.finish(&encoder);
            queue.submit([encoder.finish()]);
            for animation in &animations {
                let blend = animation.blend(tick);
                let chain = |f| source_chain(animation.pixels(f), size, &mips, animation.layer);
                let ((current, edge), (next, _)) = (chain(blend.frame), chain(blend.next));
                let per = grid * grid;
                let layers = read_back(
                    &device,
                    &queue,
                    &textures[(animation.layer / (split * per)) as usize],
                );
                let layer = &layers[(animation.layer % (split * per) / per) as usize];
                assert_eq!(
                    untile(layer, size, grid, animation.layer % per),
                    blend_chain(&current, &next, blend.millis, edge, size),
                    "layer {} at tick {tick} in a grid of {grid} (CPU {cpu})",
                    animation.layer
                );
            }
        }
        // Every sprite on the first tick, then only those whose frame or mix
        // changed since they were written: the interpolated ones every tick
        // but layer 1 at tick 30 (two 9-tick loops after 12), layer 3 when
        // its frame turns over (every 2 ticks, but 12 shows 7's frame).
        assert_eq!(writes, [3, 0, 2, 3, 2, 3, 3, 0, 2, 1]);
    }
}
