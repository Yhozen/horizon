//! Textures a pack reads that are not render targets: `noisetex`, the PNGs
//! of `texture.<stage>.<name>`, and 1×1 stand-ins (`normals`, `specular`,
//! and what a missing target reads).

use horizon_glsl_pack::{PackFiles, PackPlan};

pub(super) struct Textures {
    pub noise: wgpu::TextureView,
    pub custom: Vec<wgpu::TextureView>,
    pub normals: wgpu::TextureView,
    pub black: wgpu::TextureView,
    pub white: wgpu::TextureView,
    /// A depth texture at the far plane (0 in reversed Z): shadow reads
    /// with no shadow map are lit.
    pub far_depth: wgpu::TextureView,
}

fn rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    size: [u32; 2],
    pixels: &[u8],
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * size[0]),
            rows_per_image: Some(size[1]),
        },
        wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&Default::default())
}

/// A PNG as RGBA8, rows top first (as GL packs expect: the first row at v = 0).
fn decode_png(bytes: &[u8]) -> Result<([u32; 2], Vec<u8>), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width, info.height);
    let src = &buf[..info.buffer_size()];
    let pixels = match info.color_type {
        png::ColorType::Rgba => src.to_vec(),
        png::ColorType::Rgb => src
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => src
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => src.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("indexed PNG after expansion".into()),
    };
    Ok(([w, h], pixels))
}

impl Textures {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        plan: &PackPlan,
        files: &PackFiles,
    ) -> Result<Self, String> {
        let n = plan.noise_resolution.clamp(1, 2048);
        let noise = horizon_glsl_pack::noise::noise_rgba(n);
        let mut custom = Vec::new();
        for t in &plan.custom_textures {
            let bytes = files
                .0
                .get(&t.path)
                .ok_or_else(|| format!("{}: no such texture", t.path))?;
            let (size, pixels) = decode_png(bytes).map_err(|e| format!("{}: {e}", t.path))?;
            custom.push(rgba(device, queue, &t.path, size, &pixels));
        }
        let far_depth = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("Shader pack far depth"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            // wgpu zero-initialises it: the far plane in reversed Z.
            .create_view(&Default::default());
        Ok(Self {
            noise: rgba(device, queue, "noisetex", [n, n], &noise),
            custom,
            normals: rgba(device, queue, "normals", [1, 1], &[127, 127, 255, 255]),
            black: rgba(device, queue, "Shader pack black", [1, 1], &[0, 0, 0, 0]),
            white: rgba(device, queue, "Shader pack white", [1, 1], &[255; 4]),
            far_depth,
        })
    }
}
