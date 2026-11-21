//! Per-frame uploads through one reused staging belt. `Queue::write_buffer`
//! and `Queue::write_texture` create a fresh staging buffer for every call (on
//! Metal a kernel call each), which made the animated sprites' few hundred
//! texture writes a spike on every tick that changes a frame. The belt keeps a
//! few mapped buffers and records the copies into the frame's encoder instead.
use std::num::NonZeroU64;

/// Large enough for a tick of animated sprites at 16x16 with every mip level.
const CHUNK: u64 = 1 << 20;

pub struct Uploads {
    #[cfg(not(target_arch = "wasm32"))]
    belt: wgpu::util::StagingBelt,
    #[cfg(target_arch = "wasm32")]
    belt: web::Belt,
}

impl Uploads {
    pub fn new(device: &wgpu::Device) -> Self {
        Self::with_chunk(device, CHUNK)
    }
    /// A belt whose staging buffers hold `chunk` bytes (larger writes get
    /// a buffer of their own).
    pub fn with_chunk(device: &wgpu::Device, chunk: u64) -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            belt: wgpu::util::StagingBelt::new(device.clone(), chunk),
            #[cfg(target_arch = "wasm32")]
            belt: web::Belt::new(device.clone(), chunk, web::SHARED.get()),
        }
    }
    /// Copy tightly packed RGBA8 `pixels` (`width` by `height`) into one mip
    /// level of one array layer, padding rows to the copy alignment.
    pub fn texture(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        destination: wgpu::TexelCopyTextureInfo<'_>,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) {
        let row = width as usize * 4;
        debug_assert_eq!(pixels.len(), row * height as usize);
        let padded = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize);
        let Some(size) = NonZeroU64::new((padded * height as usize) as u64) else {
            return;
        };
        let alignment = NonZeroU64::new(u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT))
            .expect("non-zero alignment");
        let slice = self.belt.allocate(size, alignment);
        {
            let mut view = slice
                .get_mapped_range_mut()
                .expect("staging belt slices are mapped");
            for (i, src) in pixels.chunks(row).enumerate() {
                view.slice(i * padded..i * padded + row)
                    .copy_from_slice(src);
            }
        }
        #[cfg(target_arch = "wasm32")]
        self.belt.prepare_copy(&slice);
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: slice.buffer(),
                layout: wgpu::TexelCopyBufferLayout {
                    offset: slice.offset(),
                    bytes_per_row: Some(padded as u32),
                    rows_per_image: Some(height),
                },
            },
            destination,
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
    /// Copy `bytes` into `target` at `offset`, as `Queue::write_buffer`
    /// would, in the encoder's order.
    pub fn buffer(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Buffer,
        offset: u64,
        bytes: &[u8],
    ) {
        let Some(size) = NonZeroU64::new(bytes.len() as u64) else {
            return;
        };
        #[cfg(not(target_arch = "wasm32"))]
        self.belt
            .write_buffer(encoder, target, offset, size)
            .copy_from_slice(bytes);
        #[cfg(target_arch = "wasm32")]
        self.belt.write_buffer(encoder, target, offset, size, bytes);
    }
    /// Close this frame's staging buffers: they come back for reuse once the
    /// encoder's submission finishes on the GPU.
    pub fn finish(&mut self, encoder: &wgpu::CommandEncoder) {
        self.belt.finish_and_recall_on_submit(encoder);
    }
}

/// Use the actual belt on the game device. WebKit can reject a second copy
/// encoded while the same source is mapped (gfx-rs/wgpu#10460).
#[cfg(target_arch = "wasm32")]
pub async fn probe_shared_staging(device: &wgpu::Device, queue: &wgpu::Queue) {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let targets = [wgpu::BufferUsages::UNIFORM, wgpu::BufferUsages::VERTEX].map(|usage| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Web staging probe target"),
            size: 8,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Web staging probe"),
    });
    let mut belt = web::Belt::new(device.clone(), CHUNK, true);
    let size = NonZeroU64::new(8).expect("non-zero size");
    belt.write_buffer(&mut encoder, &targets[0], 0, size, &[1; 8]);
    belt.write_buffer(&mut encoder, &targets[1], 0, size, &[2; 8]);
    belt.finish_and_recall_on_submit(&encoder);
    queue.submit([encoder.finish()]);
    let error = scope.pop().await;
    web::SHARED.set(error.is_none());
    if let Some(error) = error {
        log::warn!(
            "WebGPU staging probe rejected shared buffers; using individual uploads: {error}"
        );
    }
}

/// 🤖 Agent: the browser build's belt. `StagingBelt` recalls its buffers
/// through `map_async` and `on_submitted_work_done`, which fail in Chrome
/// once the frame loop runs inside the page's event loop; here every frame
/// gets buffers mapped at creation instead, unmapped before the submit and
/// kept alive for a few frames so the GPU is done with them when they drop.
#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::Cell;
    use std::num::NonZeroU64;

    const FRAMES: usize = 4;
    thread_local! {
        pub(super) static SHARED: Cell<bool> = const { Cell::new(true) };
    }

    pub struct Belt {
        device: wgpu::Device,
        chunk: u64,
        shared: bool,
        open: Option<(wgpu::Buffer, u64)>,
        frame: Vec<wgpu::Buffer>,
        old: std::collections::VecDeque<Vec<wgpu::Buffer>>,
        /// Bytes this frame's writes took so far, and the last frame's.
        used: u64,
        last: u64,
    }

    pub struct Slice {
        buffer: wgpu::Buffer,
        offset: u64,
        size: u64,
    }

    impl Slice {
        pub fn buffer(&self) -> &wgpu::Buffer {
            &self.buffer
        }
        pub fn offset(&self) -> u64 {
            self.offset
        }
        pub fn get_mapped_range_mut(&self) -> Option<wgpu::BufferViewMut> {
            self.buffer
                .slice(self.offset..self.offset + self.size)
                .get_mapped_range_mut()
                .ok()
        }
    }

    impl Belt {
        pub fn new(device: wgpu::Device, chunk: u64, shared: bool) -> Self {
            Self {
                device,
                chunk,
                shared,
                open: None,
                frame: Vec::new(),
                old: Default::default(),
                used: 0,
                last: 0,
            }
        }
        pub fn allocate(&mut self, size: NonZeroU64, alignment: NonZeroU64) -> Slice {
            let size = size.get().next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
            if !self.shared {
                let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("web staging transfer"),
                    size,
                    usage: wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: true,
                });
                return Slice {
                    buffer,
                    offset: 0,
                    size,
                };
            }
            let align = alignment.get().max(wgpu::MAP_ALIGNMENT);
            let fits = |(buffer, used): &(wgpu::Buffer, u64)| {
                used.next_multiple_of(align) + size <= buffer.size()
            };
            if !self.open.as_ref().is_some_and(fits) {
                if let Some((buffer, _)) = self.open.take() {
                    self.frame.push(buffer);
                }
                // Chrome zeroes and ships every byte of a buffer mapped at
                // creation: size it for what the last frame wrote (most
                // frames write a few kilobytes), not a whole chunk.
                let expected = self
                    .last
                    .saturating_sub(self.used)
                    .next_multiple_of(1 << 16);
                let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("web staging"),
                    size: size.max(expected.clamp(1 << 16, self.chunk)),
                    usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: true,
                });
                self.open = Some((buffer, 0));
            }
            let (buffer, used) = self.open.as_mut().expect("opened above");
            let offset = used.next_multiple_of(align);
            self.used += offset + size - *used;
            *used = offset + size;
            Slice {
                buffer: buffer.clone(),
                offset,
                size,
            }
        }
        pub fn prepare_copy(&mut self, slice: &Slice) {
            if !self.shared {
                slice.buffer.unmap();
                self.frame.push(slice.buffer.clone());
            }
        }
        pub fn write_buffer(
            &mut self,
            encoder: &mut wgpu::CommandEncoder,
            target: &wgpu::Buffer,
            offset: u64,
            size: NonZeroU64,
            bytes: &[u8],
        ) {
            let slice = self.allocate(size, NonZeroU64::new(4).expect("non-zero"));
            slice
                .get_mapped_range_mut()
                .expect("mapped at creation")
                .slice(..bytes.len())
                .copy_from_slice(bytes);
            self.prepare_copy(&slice);
            encoder.copy_buffer_to_buffer(&slice.buffer, slice.offset, target, offset, size.get());
        }
        pub fn finish_and_recall_on_submit(&mut self, _encoder: &wgpu::CommandEncoder) {
            if let Some((buffer, _)) = self.open.take() {
                self.frame.push(buffer);
            }
            if self.shared {
                for buffer in &self.frame {
                    buffer.unmap();
                }
            }
            self.old.push_back(std::mem::take(&mut self.frame));
            self.last = std::mem::take(&mut self.used);
            while self.old.len() > FRAMES {
                self.old.pop_front();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
        pollster::block_on(adapter.request_device(&Default::default())).ok()
    }

    /// Every mip level of one layer, read back as tightly packed RGBA8.
    fn read_back(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
        let mut out = Vec::new();
        for level in 0..texture.mip_level_count() {
            let edge = texture.width() >> level;
            let padded = (edge * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: u64::from(padded * edge),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: level,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: 1 },
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
                    depth_or_array_layers: 1,
                },
            );
            queue.submit([encoder.finish()]);
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, |r| r.unwrap());
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            let data = buffer.slice(..).get_mapped_range().unwrap();
            for row in data.chunks(padded as usize) {
                out.extend_from_slice(&row[..edge as usize * 4]);
            }
        }
        out
    }

    #[test]
    fn belt_uploads_match_queue_writes_on_every_mip_level() {
        let Some((device, queue)) = device() else {
            eprintln!("no GPU adapter: skipped");
            return;
        };
        let texture = || {
            device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 16,
                    height: 16,
                    depth_or_array_layers: 3,
                },
                mip_level_count: 5,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let (expected, actual) = (texture(), texture());
        let mut uploads = Uploads::new(&device);
        // Two frames, so the second reuses the belt's recalled buffers.
        for frame in 0..2u8 {
            let mut encoder = device.create_command_encoder(&Default::default());
            for level in 0..5 {
                let edge = 16 >> level;
                let pixels: Vec<u8> = (0..edge * edge * 4)
                    .map(|i| {
                        (i as u8)
                            .wrapping_mul(31)
                            .wrapping_add(level as u8 * 7 + frame)
                    })
                    .collect();
                let at = |texture| wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: level,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: 1 },
                    aspect: wgpu::TextureAspect::All,
                };
                queue.write_texture(
                    at(&expected),
                    &pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(edge * 4),
                        rows_per_image: Some(edge),
                    },
                    wgpu::Extent3d {
                        width: edge,
                        height: edge,
                        depth_or_array_layers: 1,
                    },
                );
                uploads.texture(&mut encoder, at(&actual), edge, edge, &pixels);
            }
            uploads.finish(&encoder);
            queue.submit([encoder.finish()]);
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            assert_eq!(
                read_back(&device, &queue, &actual),
                read_back(&device, &queue, &expected),
                "frame {frame}"
            );
        }
    }

    #[test]
    fn belt_buffer_writes_land_in_order_at_their_offsets() {
        let Some((device, queue)) = device() else {
            eprintln!("no GPU adapter: skipped");
            return;
        };
        let target = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut uploads = Uploads::new(&device);
        let mut expected = [0u8; 64];
        for frame in 0..3u8 {
            let mut encoder = device.create_command_encoder(&Default::default());
            // Overlapping writes: the later one wins, as with the queue.
            for (offset, len, value) in [(0, 32, frame + 1), (16, 32, frame + 10), (60, 4, 7)] {
                let bytes = vec![value; len];
                uploads.buffer(&mut encoder, &target, offset as u64, &bytes);
                expected[offset..offset + len].copy_from_slice(&bytes);
            }
            uploads.buffer(&mut encoder, &target, 0, &[]);
            encoder.copy_buffer_to_buffer(&target, 0, &readback, 0, 64);
            uploads.finish(&encoder);
            queue.submit([encoder.finish()]);
            readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, |r| r.unwrap());
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            assert_eq!(
                *readback.slice(..).get_mapped_range().unwrap(),
                expected[..]
            );
            readback.unmap();
        }
    }
}
