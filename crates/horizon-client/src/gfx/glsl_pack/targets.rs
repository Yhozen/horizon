//! The pack's render targets: `colortexN` as main/alternate pairs (full-screen
//! passes read one side and write the other), the depth buffer and its
//! copies (`depthtex1`, `depthtex2`), and the shadow map's textures.

use horizon_glsl_pack::plan::{BufferFormat, ShadowSpec, StorageFormat};

use super::mips;

use horizon_glsl_pack::schedule::Side;

/// The wgpu format a pack format is stored in. 16-bit normalized formats are
/// not render targets without adapter-specific features (WebGPU never has
/// them), so they are stored as 32-bit floats where those filter and blend,
/// else as half floats; the programs clamp what they write (`horizon_glsl_pack`'s
/// host). Half floats hold `k / 65535` exactly only for small `k`: packs that
/// pack two bytes into one 16-bit channel need the 32-bit floats. colortex0 in
/// an 8-bit format takes the world's format, so Horizon's own pipelines can
/// draw into it for slots the pack has no program for.
pub(super) fn wgpu_format(
    format: BufferFormat,
    world: Option<wgpu::TextureFormat>,
    features: wgpu::Features,
) -> wgpu::TextureFormat {
    use wgpu::TextureFormat as T;
    let float32 =
        features.contains(wgpu::Features::FLOAT32_FILTERABLE | wgpu::Features::FLOAT32_BLENDABLE);
    // wgpu only guarantees 16-bit normalized formats as storage: rendering and
    // blending into them come with the adapter's own format features.
    let norm16 = features.contains(
        wgpu::Features::TEXTURE_FORMAT_16BIT_NORM
            | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
    );
    match format {
        BufferFormat::R8 => T::R8Unorm,
        BufferFormat::Rg8 => T::Rg8Unorm,
        BufferFormat::Rgb8 | BufferFormat::Rgba8 => world.unwrap_or(T::Rgba8Unorm),
        // 16-bit normalized buffers often carry bit-packed data (normals, material
        // ids): stored exactly where the device can, else as half floats (lossy).
        BufferFormat::R16 if norm16 => T::R16Unorm,
        BufferFormat::Rg16 if norm16 => T::Rg16Unorm,
        BufferFormat::Rgb16 | BufferFormat::Rgba16 if norm16 => T::Rgba16Unorm,
        BufferFormat::R16 if float32 => T::R32Float,
        BufferFormat::Rg16 if float32 => T::Rg32Float,
        BufferFormat::Rgb16 | BufferFormat::Rgba16 if float32 => T::Rgba32Float,
        BufferFormat::R16 | BufferFormat::R16F => T::R16Float,
        BufferFormat::Rg16 | BufferFormat::Rg16F => T::Rg16Float,
        BufferFormat::Rgb16
        | BufferFormat::Rgba16
        | BufferFormat::Rgb16F
        | BufferFormat::Rgba16F => T::Rgba16Float,
        BufferFormat::R32F if float32 => T::R32Float,
        BufferFormat::Rg32F if float32 => T::Rg32Float,
        BufferFormat::Rgb32F | BufferFormat::Rgba32F if float32 => T::Rgba32Float,
        BufferFormat::R32F => T::R16Float,
        BufferFormat::Rg32F => T::Rg16Float,
        BufferFormat::Rgb32F | BufferFormat::Rgba32F => T::Rgba16Float,
        BufferFormat::R11G11B10F if features.contains(wgpu::Features::RG11B10UFLOAT_RENDERABLE) => {
            T::Rg11b10Ufloat
        }
        BufferFormat::R11G11B10F => T::Rgba16Float,
        BufferFormat::Rgb10A2 => T::Rgb10a2Unorm,
        other => storage_format(other.storage(), float32),
    }
}

/// The wgpu format of the crate's storage choice for every other format
/// (snorm and packed small formats widened, integers as integer targets).
fn storage_format(storage: StorageFormat, float32: bool) -> wgpu::TextureFormat {
    use StorageFormat as S;
    use wgpu::TextureFormat as T;
    match storage {
        S::R8Unorm => T::R8Unorm,
        S::Rg8Unorm => T::Rg8Unorm,
        S::Rgba8Unorm => T::Rgba8Unorm,
        S::R16Float => T::R16Float,
        S::Rg16Float => T::Rg16Float,
        S::Rgba16Float => T::Rgba16Float,
        S::R32Float if float32 => T::R32Float,
        S::Rg32Float if float32 => T::Rg32Float,
        S::Rgba32Float if float32 => T::Rgba32Float,
        S::R32Float => T::R16Float,
        S::Rg32Float => T::Rg16Float,
        S::Rgba32Float => T::Rgba16Float,
        S::Rg11b10Ufloat => T::Rg11b10Ufloat,
        S::Rgb10a2Unorm => T::Rgb10a2Unorm,
        S::Rgb10a2Uint => T::Rgb10a2Uint,
        S::R8Sint => T::R8Sint,
        S::Rg8Sint => T::Rg8Sint,
        S::Rgba8Sint => T::Rgba8Sint,
        S::R16Sint => T::R16Sint,
        S::Rg16Sint => T::Rg16Sint,
        S::Rgba16Sint => T::Rgba16Sint,
        S::R32Sint => T::R32Sint,
        S::Rg32Sint => T::Rg32Sint,
        S::Rgba32Sint => T::Rgba32Sint,
        S::R8Uint => T::R8Uint,
        S::Rg8Uint => T::Rg8Uint,
        S::Rgba8Uint => T::Rgba8Uint,
        S::R16Uint => T::R16Uint,
        S::Rg16Uint => T::Rg16Uint,
        S::Rgba16Uint => T::Rgba16Uint,
        S::R32Uint => T::R32Uint,
        S::Rg32Uint => T::Rg32Uint,
        S::Rgba32Uint => T::Rgba32Uint,
    }
}

/// One side of a colour buffer.
pub(super) struct Surface {
    pub texture: wgpu::Texture,
    /// Every mip level, for sampling.
    pub view: wgpu::TextureView,
    /// Level 0, for drawing.
    pub target: wgpu::TextureView,
}

pub(super) struct ColorBuffer {
    /// Main, then alternate (when a full-screen pass writes the buffer).
    pub sides: Vec<Surface>,
}

impl ColorBuffer {
    pub fn side(&self, side: Side) -> &Surface {
        let i = usize::from(side == Side::Alt);
        &self.sides[i.min(self.sides.len() - 1)]
    }
}

/// A texture with its default view.
pub(super) struct Image {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

pub(super) fn image(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    mips: u32,
    usage: wgpu::TextureUsages,
) -> Image {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size[0].max(1),
            height: size[1].max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    Image { texture, view }
}

/// What a buffer needs: whether a full-screen pass writes it (two sides) and
/// whether a pass samples its mipmaps.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Need {
    pub used: bool,
    pub two_sides: bool,
    pub mipmaps: bool,
}

/// The screen-sized targets.
pub(super) struct Targets {
    pub size: [u32; 2],
    pub buffers: Vec<Option<ColorBuffer>>,
    /// The gbuffers' depth (`depthtex0`).
    pub depth: Image,
    /// Copies: `depthtex1` (no translucent terrain) and `depthtex2` (no hand).
    pub depth_copies: [Image; 2],
    /// `depth` upright (pack targets are stored bottom row first), for
    /// Horizon's overlays after `final` (flip.rs).
    pub overlay_depth: Image,
}

const COLOR_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT
    .union(wgpu::TextureUsages::TEXTURE_BINDING)
    .union(wgpu::TextureUsages::COPY_SRC)
    .union(wgpu::TextureUsages::COPY_DST);

impl Targets {
    pub fn new(
        device: &wgpu::Device,
        size: [u32; 2],
        needs: &[Need; 16],
        formats: &[wgpu::TextureFormat; 16],
        sizes: &[horizon_glsl_pack::plan::BufferSize; 16],
        mips: &mut mips::Mips,
    ) -> Self {
        let screen = size;
        let buffers = (0..16)
            .map(|i| {
                let need = needs[i];
                // `size.buffer.colortexN`: a fixed or relative size.
                let (w, h) = sizes[i].pixels(screen[0], screen[1]);
                let size = [w, h];
                need.used.then(|| {
                    let levels = if need.mipmaps { mips::levels(size) } else { 1 };
                    if levels > 1 {
                        mips.prepare(device, formats[i]);
                    }
                    let sides = (0..1 + usize::from(need.two_sides))
                        .map(|side| {
                            let label =
                                format!("colortex{i}{}", if side == 0 { "" } else { " (alt)" });
                            let img = image(device, &label, size, formats[i], levels, COLOR_USAGE);
                            let target = img.texture.create_view(&wgpu::TextureViewDescriptor {
                                mip_level_count: Some(1),
                                ..Default::default()
                            });
                            Surface {
                                texture: img.texture,
                                view: img.view,
                                target,
                            }
                        })
                        .collect();
                    ColorBuffer { sides }
                })
            })
            .collect();
        let depth_usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC;
        let copy_usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST;
        let depth32 = wgpu::TextureFormat::Depth32Float;
        Self {
            size: screen,
            buffers,
            depth: image(device, "depthtex0", size, depth32, 1, depth_usage),
            depth_copies: [
                image(device, "depthtex1", size, depth32, 1, copy_usage),
                image(device, "depthtex2", size, depth32, 1, copy_usage),
            ],
            overlay_depth: image(
                device,
                "Shader pack overlay depth",
                size,
                depth32,
                1,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            ),
        }
    }

    pub fn buffer(&self, i: u8) -> Option<&ColorBuffer> {
        self.buffers.get(usize::from(i))?.as_ref()
    }

    /// Copies the depth buffer into `depthtex1` (0) or `depthtex2` (1).
    pub fn copy_depth(&self, encoder: &mut wgpu::CommandEncoder, copy: usize) {
        copy_whole(
            encoder,
            &self.depth.texture,
            &self.depth_copies[copy].texture,
        );
    }
}

/// Copies a whole texture (level 0) into another of the same size and format.
pub(super) fn copy_whole(
    encoder: &mut wgpu::CommandEncoder,
    from: &wgpu::Texture,
    to: &wgpu::Texture,
) {
    encoder.copy_texture_to_texture(
        from.as_image_copy(),
        to.as_image_copy(),
        wgpu::Extent3d {
            width: from.width(),
            height: from.height(),
            depth_or_array_layers: 1,
        },
    );
}

/// The shadow map's textures.
pub(super) struct ShadowTargets {
    /// `shadowtex0`: everything (the attachment).
    pub depth: Image,
    /// `shadowtex1`: opaque casters only (copied before translucent terrain).
    pub depth_opaque: Image,
    pub colors: [Option<(Image, wgpu::TextureView)>; 2],
    /// Level 0 of the colours a `shadowcomp` pass draws into, copied before
    /// it so it reads what the shadow map held.
    pub copies: [Option<Image>; 2],
}

impl ShadowTargets {
    pub fn new(
        device: &wgpu::Device,
        spec: &ShadowSpec,
        colors_used: [bool; 2],
        copied: [bool; 2],
        features: wgpu::Features,
        mips: &mut mips::Mips,
    ) -> Self {
        let resolution = spec
            .resolution
            .clamp(16, device.limits().max_texture_dimension_2d);
        let size = [resolution; 2];
        let depth32 = wgpu::TextureFormat::Depth32Float;
        let colors = std::array::from_fn(|i| {
            colors_used[i].then(|| {
                let format = wgpu_format(spec.color_format[i], None, features);
                let levels = if spec.color_mipmap[i] {
                    mips::levels(size)
                } else {
                    1
                };
                if levels > 1 {
                    mips.prepare(device, format);
                }
                let img = image(
                    device,
                    &format!("shadowcolor{i}"),
                    size,
                    format,
                    levels,
                    COLOR_USAGE,
                );
                let target = img.texture.create_view(&wgpu::TextureViewDescriptor {
                    mip_level_count: Some(1),
                    ..Default::default()
                });
                (img, target)
            })
        });
        let copies = std::array::from_fn(|i| {
            (colors_used[i] && copied[i]).then(|| {
                image(
                    device,
                    &format!("shadowcolor{i} (shadowcomp's copy)"),
                    size,
                    wgpu_format(spec.color_format[i], None, features),
                    1,
                    wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                )
            })
        });
        Self {
            depth: image(
                device,
                "shadowtex0",
                size,
                depth32,
                1,
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
            ),
            depth_opaque: image(
                device,
                "shadowtex1",
                size,
                depth32,
                1,
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            ),
            colors,
            copies,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::{Features as F, TextureFormat as T};

    #[test]
    fn sixteen_bit_unorm_buffers_keep_their_precision_where_they_can() {
        let norm16 = F::TEXTURE_FORMAT_16BIT_NORM | F::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        let float32 = F::FLOAT32_FILTERABLE | F::FLOAT32_BLENDABLE;
        let rgba16 = BufferFormat::Rgba16;
        // Native: exact unorm targets.
        assert_eq!(wgpu_format(rgba16, None, norm16 | float32), T::Rgba16Unorm);
        // WebGPU: 32-bit floats hold every k / 65535 (bytes packed into one channel too).
        assert_eq!(wgpu_format(rgba16, None, float32), T::Rgba32Float);
        assert_eq!(
            wgpu_format(BufferFormat::Rgb16, None, float32),
            T::Rgba32Float
        );
        assert_eq!(wgpu_format(BufferFormat::Rg16, None, float32), T::Rg32Float);
        assert_eq!(wgpu_format(BufferFormat::R16, None, float32), T::R32Float);
        // Neither: half floats.
        assert_eq!(wgpu_format(rgba16, None, F::empty()), T::Rgba16Float);
        // Float formats are unchanged.
        assert_eq!(
            wgpu_format(BufferFormat::Rgba16F, None, float32),
            T::Rgba16Float
        );
    }
}
