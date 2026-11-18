//! The pack's render targets: `colortexN` as main/alternate pairs (full-screen
//! passes read one side and write the other), the depth buffer and its
//! copies (`depthtex1`, `depthtex2`), and the shadow map's textures.

use horizon_glsl_pack::plan::{BufferFormat, ShadowSpec};

use super::mips;

use horizon_glsl_pack::schedule::Side;

/// The wgpu format a pack format is stored in. 16-bit normalized formats are
/// not render targets without adapter-specific features, so they are stored
/// as half floats; the programs clamp what they write (`horizon_glsl_pack`'s
/// host). colortex0 in an 8-bit format takes the world's format, so Horizon's
/// own pipelines can draw into it for slots the pack has no program for.
pub(super) fn wgpu_format(
    format: BufferFormat,
    world: Option<wgpu::TextureFormat>,
    features: wgpu::Features,
) -> wgpu::TextureFormat {
    use wgpu::TextureFormat as T;
    let float32 =
        features.contains(wgpu::Features::FLOAT32_FILTERABLE | wgpu::Features::FLOAT32_BLENDABLE);
    match format {
        BufferFormat::R8 => T::R8Unorm,
        BufferFormat::Rg8 => T::Rg8Unorm,
        BufferFormat::Rgb8 | BufferFormat::Rgba8 => world.unwrap_or(T::Rgba8Unorm),
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
        mips: &mut mips::Mips,
    ) -> Self {
        let buffers = (0..16)
            .map(|i| {
                let need = needs[i];
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
            size,
            buffers,
            depth: image(device, "depthtex0", size, depth32, 1, depth_usage),
            depth_copies: [
                image(device, "depthtex1", size, depth32, 1, copy_usage),
                image(device, "depthtex2", size, depth32, 1, copy_usage),
            ],
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
}

impl ShadowTargets {
    pub fn new(
        device: &wgpu::Device,
        spec: &ShadowSpec,
        colors_used: [bool; 2],
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
        }
    }
}
