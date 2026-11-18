//! Textures a pack reads that are not render targets: `noisetex`, the custom
//! textures of `shaders.properties` (PNGs, raw texel files, resource-pack
//! textures), and 1×1 stand-ins (`normals`, `specular`, and what a missing
//! texture reads, one per dimension and texel type).
//!
//! A custom texture that cannot be loaded never stops the pack: its binding
//! reads the stand-in and the reason goes to [`Textures::notes`].

use horizon_glsl_pack::plan::CustomTexture;
use horizon_glsl_pack::textures::{CustomTextureKind, RawTexture};
use horizon_glsl_pack::{PackFiles, PackPlan, SampleKind, TexDim};

pub(super) struct Textures {
    pub noise: wgpu::TextureView,
    /// By index in `PackPlan::custom_textures`; `None` when it did not load.
    pub custom: Vec<Option<wgpu::TextureView>>,
    pub normals: wgpu::TextureView,
    pub black: wgpu::TextureView,
    pub white: wgpu::TextureView,
    /// A depth texture at the far plane (0 in reversed Z): shadow reads
    /// with no shadow map are lit.
    pub far_depth: wgpu::TextureView,
    /// 1×1×1 zero textures: `[float, sint, uint]` in 3D, `[sint, uint]` in 2D
    /// (2D float is `black`).
    zero3: [wgpu::TextureView; 3],
    zero2: [wgpu::TextureView; 2],
    /// Why custom textures did not load, for the pack report.
    pub notes: Vec<String>,
}

/// One texture with its pixels uploaded (rows tightly packed).
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    dim: TexDim,
    size: [u32; 3],
    format: wgpu::TextureFormat,
    pixels: &[u8],
) -> wgpu::TextureView {
    let extent = wgpu::Extent3d {
        width: size[0],
        height: size[1],
        depth_or_array_layers: size[2],
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: match dim {
            TexDim::D2 => wgpu::TextureDimension::D2,
            TexDim::D3 => wgpu::TextureDimension::D3,
        },
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let texel = format.block_copy_size(None).unwrap_or(4);
    queue.write_texture(
        texture.as_image_copy(),
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(texel * size[0]),
            rows_per_image: Some(size[1]),
        },
        extent,
    );
    texture.create_view(&Default::default())
}

fn rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    size: [u32; 2],
    pixels: &[u8],
) -> wgpu::TextureView {
    upload(
        device,
        queue,
        label,
        TexDim::D2,
        [size[0], size[1], 1],
        wgpu::TextureFormat::Rgba8Unorm,
        pixels,
    )
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

/// How texels are written into the texture's format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Encoding {
    Unorm8,
    Snorm8,
    F16,
    F32,
    Sint(u8),
    Uint(u8),
}

impl Encoding {
    fn bytes(self) -> usize {
        match self {
            Self::Unorm8 | Self::Snorm8 => 1,
            Self::F16 => 2,
            Self::F32 => 4,
            Self::Sint(b) | Self::Uint(b) => usize::from(b / 8),
        }
    }

    fn integer(self) -> bool {
        matches!(self, Self::Sint(_) | Self::Uint(_))
    }

    fn write(self, v: f64, out: &mut Vec<u8>) {
        match self {
            Self::Unorm8 => out.push((v.clamp(0.0, 1.0) * 255.0).round() as u8),
            Self::Snorm8 => out.push(((v.clamp(-1.0, 1.0) * 127.0).round() as i8) as u8),
            Self::F16 => out.extend(f16_bits(v as f32).to_le_bytes()),
            Self::F32 => out.extend((v as f32).to_le_bytes()),
            Self::Sint(8) => out.push(v as i8 as u8),
            Self::Sint(16) => out.extend((v as i16).to_le_bytes()),
            Self::Sint(_) => out.extend((v as i32).to_le_bytes()),
            Self::Uint(8) => out.push(v as u8),
            Self::Uint(16) => out.extend((v as u16).to_le_bytes()),
            Self::Uint(_) => out.extend((v as u32).to_le_bytes()),
        }
    }
}

/// IEEE half from a float (round to nearest even; overflow to infinity).
fn f16_bits(f: f32) -> u16 {
    let x = f.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let exp = ((x >> 23) & 0xff) as i32;
    let man = x & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if man != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        // A subnormal half: units of 2^-24, rounded half up.
        let shift = (14 - e) as u32;
        let full = u64::from(man | 0x80_0000);
        return sign | ((full + (1 << (shift - 1))) >> shift) as u16;
    }
    let half = sign | ((e as u16) << 10) | (man >> 13) as u16;
    let rest = man & 0x1fff;
    if rest > 0x1000 || (rest == 0x1000 && half & 1 == 1) {
        half + 1
    } else {
        half
    }
}

/// A float from IEEE half bits.
fn f16_value(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((h >> 10) & 0x1f);
    let man = f32::from(h & 0x3ff);
    sign * match exp {
        0 => man * 2f32.powi(-24),
        0x1f if man == 0.0 => f32::INFINITY,
        0x1f => f32::NAN,
        _ => (1.0 + man / 1024.0) * 2f32.powi(exp - 15),
    }
}

/// The wgpu format a GL internal format is stored in, its channel count
/// (RGB widened to RGBA) and how texels are written.
fn raw_format(internal: &str) -> Option<(wgpu::TextureFormat, usize, Encoding)> {
    use wgpu::TextureFormat as F;
    let name = internal.to_ascii_uppercase();
    let name = name.strip_prefix("GL_").unwrap_or(&name);
    match name {
        "R11F_G11F_B10F" | "RGB9_E5" | "RGB10_A2" => {
            return Some((F::Rgba16Float, 4, Encoding::F16));
        }
        "RGB10_A2UI" => return Some((F::Rgba16Uint, 4, Encoding::Uint(16))),
        "SRGB8" | "SRGB8_ALPHA8" => return Some((F::Rgba8Unorm, 4, Encoding::Unorm8)),
        _ => {}
    }
    let channels = ["RGBA", "RGB", "RG", "R"]
        .into_iter()
        .find(|p| name.starts_with(p))?;
    let rest = &name[channels.len()..];
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let bits: u8 = rest[..digits].parse().ok()?;
    let suffix = &rest[digits..];
    let n = match channels.len() {
        1 => 1,
        2 => 2,
        _ => 4,
    };
    let pick = |one, two, four| {
        Some(match n {
            1 => one,
            2 => two,
            _ => four,
        })
    };
    let (format, encoding) = match (bits, suffix) {
        (8, "") => (
            pick(F::R8Unorm, F::Rg8Unorm, F::Rgba8Unorm)?,
            Encoding::Unorm8,
        ),
        (8, "_SNORM") => (
            pick(F::R8Snorm, F::Rg8Snorm, F::Rgba8Snorm)?,
            Encoding::Snorm8,
        ),
        // 16-bit normalized formats are not portable: stored as half floats.
        (16, "" | "_SNORM" | "F") => (
            pick(F::R16Float, F::Rg16Float, F::Rgba16Float)?,
            Encoding::F16,
        ),
        (32, "F") => (
            pick(F::R32Float, F::Rg32Float, F::Rgba32Float)?,
            Encoding::F32,
        ),
        (8, "I") => (
            pick(F::R8Sint, F::Rg8Sint, F::Rgba8Sint)?,
            Encoding::Sint(8),
        ),
        (16, "I") => (
            pick(F::R16Sint, F::Rg16Sint, F::Rgba16Sint)?,
            Encoding::Sint(16),
        ),
        (32, "I") => (
            pick(F::R32Sint, F::Rg32Sint, F::Rgba32Sint)?,
            Encoding::Sint(32),
        ),
        (8, "UI") => (
            pick(F::R8Uint, F::Rg8Uint, F::Rgba8Uint)?,
            Encoding::Uint(8),
        ),
        (16, "UI") => (
            pick(F::R16Uint, F::Rg16Uint, F::Rgba16Uint)?,
            Encoding::Uint(16),
        ),
        (32, "UI") => (
            pick(F::R32Uint, F::Rg32Uint, F::Rgba32Uint)?,
            Encoding::Uint(32),
        ),
        _ => return None,
    };
    Some((format, n, encoding))
}

/// A GL pixel format: the source channel feeding each of R, G, B, A, and
/// whether the values are integers (`*_INTEGER`) rather than normalized.
fn pixel_format(name: &str) -> Option<(Vec<usize>, bool)> {
    let name = name.to_ascii_uppercase();
    let name = name.strip_prefix("GL_").unwrap_or(&name);
    let (base, integer) = match name.strip_suffix("_INTEGER") {
        Some(b) => (b, true),
        None => (name, false),
    };
    let order = match base {
        "RED" | "R" | "LUMINANCE" | "DEPTH_COMPONENT" => vec![0],
        "RG" => vec![0, 1],
        "RGB" => vec![0, 1, 2],
        "BGR" => vec![2, 1, 0],
        "RGBA" => vec![0, 1, 2, 3],
        "BGRA" => vec![2, 1, 0, 3],
        _ => return None,
    };
    Some((order, integer))
}

/// A GL pixel type: bytes per component and how to read one.
#[derive(Clone, Copy)]
enum Component {
    U(usize),
    S(usize),
    Half,
    Float,
}

fn pixel_type(name: &str) -> Option<Component> {
    let name = name.to_ascii_uppercase();
    Some(match name.strip_prefix("GL_").unwrap_or(&name) {
        "UNSIGNED_BYTE" => Component::U(1),
        "BYTE" => Component::S(1),
        "UNSIGNED_SHORT" => Component::U(2),
        "SHORT" => Component::S(2),
        "UNSIGNED_INT" => Component::U(4),
        "INT" => Component::S(4),
        "HALF_FLOAT" => Component::Half,
        "FLOAT" => Component::Float,
        _ => return None,
    })
}

impl Component {
    fn bytes(self) -> usize {
        match self {
            Self::U(b) | Self::S(b) => b,
            Self::Half => 2,
            Self::Float => 4,
        }
    }

    /// The value at `b`: integers as they are, or normalized (unsigned to
    /// 0…1, signed to -1…1) when `normalize`.
    fn read(self, b: &[u8], normalize: bool) -> f64 {
        let int = |v: i64, max: i64| {
            if normalize {
                (v as f64 / max as f64).max(-1.0)
            } else {
                v as f64
            }
        };
        match self {
            Self::U(1) => int(i64::from(b[0]), 0xff),
            Self::U(2) => int(i64::from(u16::from_le_bytes([b[0], b[1]])), 0xffff),
            Self::U(_) => int(
                i64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                0xffff_ffff,
            ),
            Self::S(1) => int(i64::from(b[0] as i8), 0x7f),
            Self::S(2) => int(i64::from(i16::from_le_bytes([b[0], b[1]])), 0x7fff),
            Self::S(_) => int(
                i64::from(i32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                0x7fff_ffff,
            ),
            Self::Half => f64::from(f16_value(u16::from_le_bytes([b[0], b[1]]))),
            Self::Float => f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        }
    }
}

/// The largest texture Horizon uploads (bytes), whatever the device allows.
const MAX_BYTES: usize = 256 << 20;

/// A raw texture's texels in the format it is stored in.
fn decode_raw(raw: &RawTexture, bytes: &[u8]) -> Result<(wgpu::TextureFormat, Vec<u8>), String> {
    let (format, channels, encoding) = raw_format(&raw.internal_format)
        .ok_or_else(|| format!("internal format {} is not supported", raw.internal_format))?;
    let (order, integer_source) = pixel_format(&raw.pixel_format)
        .ok_or_else(|| format!("pixel format {} is not supported", raw.pixel_format))?;
    let component = pixel_type(&raw.pixel_type)
        .ok_or_else(|| format!("pixel type {} is not supported", raw.pixel_type))?;
    let [w, h, d] = raw.size.map(|s| s as usize);
    let texels = w * h * d;
    if texels == 0 || texels.saturating_mul(channels * encoding.bytes()) > MAX_BYTES {
        return Err(format!("{w}×{h}×{d} is too large"));
    }
    let stride = order.len() * component.bytes();
    let row = w * stride;
    // Rows are tightly packed, or padded to 4 bytes (GL's default unpack alignment).
    let padded = row.div_ceil(4) * 4;
    let pitch = if bytes.len() >= row * h * d && bytes.len() != padded * h * d {
        row
    } else {
        padded
    };
    if bytes.len() < pitch * (h * d - 1) + row {
        return Err(format!(
            "{} bytes, {w}×{h}×{d} {} {} needs {}",
            bytes.len(),
            raw.pixel_format,
            raw.pixel_type,
            row * h * d
        ));
    }
    let normalize = !encoding.integer()
        && !integer_source
        && matches!(component, Component::U(_) | Component::S(_));
    let mut out = Vec::with_capacity(texels * channels * encoding.bytes());
    for r in 0..h * d {
        for x in 0..w {
            let at = r * pitch + x * stride;
            for c in 0..channels {
                let v = match order.get(c) {
                    Some(&src) => {
                        let o = at + src * component.bytes();
                        component.read(&bytes[o..o + component.bytes()], normalize)
                    }
                    None if c == 3 => 1.0,
                    None => 0.0,
                };
                encoding.write(v, &mut out);
            }
        }
    }
    Ok((format, out))
}

/// The pack file of a resource texture (`minecraft:textures/x.png` →
/// `assets/minecraft/textures/x.png`), when the pack bundles it.
fn resource_path(name: &str) -> String {
    let (namespace, path) = name.split_once(':').unwrap_or(("minecraft", name));
    format!("assets/{namespace}/{path}")
}

/// Loads one custom texture, checked against the device's limits.
fn load(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    t: &CustomTexture,
    files: &PackFiles,
) -> Result<wgpu::TextureView, String> {
    let limits = device.limits();
    let png = |path: &str| -> Result<wgpu::TextureView, String> {
        let bytes = files.0.get(path).ok_or("no such file")?;
        let (size, pixels) = decode_png(bytes)?;
        let max = limits.max_texture_dimension_2d;
        if size[0] > max || size[1] > max {
            return Err(format!(
                "{}×{} is over the device's {max}",
                size[0], size[1]
            ));
        }
        Ok(rgba(device, queue, path, size, &pixels))
    };
    match &t.kind {
        CustomTextureKind::Png => png(&t.path),
        CustomTextureKind::Resource(name) => png(&resource_path(name))
            .map_err(|_| format!("resource-pack texture {name} is not available")),
        CustomTextureKind::Raw(raw) => {
            let bytes = files.0.get(&t.path).ok_or("no such file")?;
            let max = match raw.dim {
                TexDim::D2 => limits.max_texture_dimension_2d,
                TexDim::D3 => limits.max_texture_dimension_3d,
            };
            if raw.size.iter().any(|&s| s > max) {
                let [w, h, d] = raw.size;
                return Err(format!("{w}×{h}×{d} is over the device's {max}"));
            }
            let (format, pixels) = decode_raw(raw, bytes)?;
            Ok(upload(
                device, queue, &t.path, raw.dim, raw.size, format, &pixels,
            ))
        }
    }
}

impl Textures {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        plan: &PackPlan,
        files: &PackFiles,
    ) -> Self {
        let n = plan.noise_resolution.clamp(1, 2048);
        let noise = horizon_glsl_pack::noise::noise_rgba(n);
        let mut notes = Vec::new();
        let mut custom = Vec::new();
        for t in &plan.custom_textures {
            match load(device, queue, t, files) {
                Ok(view) => custom.push(Some(view)),
                Err(e) => {
                    notes.push(format!(
                        "{} ({}): {e}; it reads a 1×1 stand-in",
                        t.path, t.sampler
                    ));
                    // Resource textures stand in as white, so tinted maths still shows.
                    custom.push(
                        matches!(t.kind, CustomTextureKind::Resource(_))
                            .then(|| rgba(device, queue, &t.path, [1, 1], &[255; 4])),
                    );
                }
            }
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
        use wgpu::TextureFormat as F;
        let zero = |dim, format: F| {
            let bytes = format.block_copy_size(None).unwrap_or(4) as usize;
            upload(
                device,
                queue,
                "Shader pack stand-in",
                dim,
                [1, 1, 1],
                format,
                &vec![0; bytes],
            )
        };
        Self {
            noise: rgba(device, queue, "noisetex", [n, n], &noise),
            custom,
            normals: rgba(device, queue, "normals", [1, 1], &[127, 127, 255, 255]),
            black: rgba(device, queue, "Shader pack black", [1, 1], &[0, 0, 0, 0]),
            white: rgba(device, queue, "Shader pack white", [1, 1], &[255; 4]),
            far_depth,
            zero3: [
                zero(TexDim::D3, F::Rgba8Unorm),
                zero(TexDim::D3, F::Rgba32Sint),
                zero(TexDim::D3, F::Rgba32Uint),
            ],
            zero2: [
                zero(TexDim::D2, F::Rgba32Sint),
                zero(TexDim::D2, F::Rgba32Uint),
            ],
            notes,
        }
    }

    /// What a binding of this dimension and texel type reads when its
    /// texture is missing: zeros (the far plane for depth).
    pub fn fallback(&self, dim: TexDim, sample: SampleKind) -> &wgpu::TextureView {
        match (dim, sample) {
            (_, SampleKind::Depth) => &self.far_depth,
            (TexDim::D2, SampleKind::Float) => &self.black,
            (TexDim::D2, SampleKind::Sint) => &self.zero2[0],
            (TexDim::D2, SampleKind::Uint) => &self.zero2[1],
            (TexDim::D3, SampleKind::Float) => &self.zero3[0],
            (TexDim::D3, SampleKind::Sint) => &self.zero3[1],
            (TexDim::D3, SampleKind::Uint) => &self.zero3[2],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(internal: &str, size: [u32; 3], pf: &str, pt: &str) -> RawTexture {
        RawTexture {
            dim: if size[2] > 1 { TexDim::D3 } else { TexDim::D2 },
            internal_format: internal.into(),
            size,
            pixel_format: pf.into(),
            pixel_type: pt.into(),
        }
    }

    #[test]
    fn raw_textures_convert_to_their_storage() {
        // RGB8 widens to RGBA with alpha 1.
        let (f, px) = decode_raw(
            &raw("RGB8", [2, 1, 1], "RGB", "UNSIGNED_BYTE"),
            &[1, 2, 3, 4, 5, 6],
        )
        .unwrap();
        assert_eq!(f, wgpu::TextureFormat::Rgba8Unorm);
        assert_eq!(px, [1, 2, 3, 255, 4, 5, 6, 255]);
        // BGRA swaps; R8 keeps one channel.
        let (_, px) = decode_raw(
            &raw("RGBA8", [1, 1, 1], "BGRA", "UNSIGNED_BYTE"),
            &[1, 2, 3, 4],
        )
        .unwrap();
        assert_eq!(px, [3, 2, 1, 4]);
        // Half floats into RGB16F → Rgba16Float with alpha 1.0.
        let one = f16_bits(1.0).to_le_bytes();
        let half = [one, one, one].concat();
        let (f, px) = decode_raw(&raw("RGB16F", [1, 1, 1], "RGB", "HALF_FLOAT"), &half).unwrap();
        assert_eq!(f, wgpu::TextureFormat::Rgba16Float);
        assert_eq!(px, [one, one, one, one].concat());
        // Unsigned shorts into an integer format stay integers.
        let (f, px) = decode_raw(
            &raw("R16UI", [2, 1, 1], "RED_INTEGER", "UNSIGNED_SHORT"),
            &[7, 0, 9, 1],
        )
        .unwrap();
        assert_eq!(f, wgpu::TextureFormat::R16Uint);
        assert_eq!(px, [7, 0, 9, 1]);
        // 3D, R8 from bytes; 16-bit unorm from shorts is stored as half.
        let (f, px) = decode_raw(&raw("R8", [2, 2, 2], "RED", "UNSIGNED_BYTE"), &[9; 8]).unwrap();
        assert_eq!((f, px.len()), (wgpu::TextureFormat::R8Unorm, 8));
        let (_, px) = decode_raw(
            &raw("R16", [1, 1, 1], "RED", "UNSIGNED_SHORT"),
            &[0xff, 0xff],
        )
        .unwrap();
        assert_eq!(px, f16_bits(1.0).to_le_bytes());
        // A file that is too short, an unknown format.
        assert!(decode_raw(&raw("RGBA8", [4, 4, 1], "RGBA", "UNSIGNED_BYTE"), &[0; 10]).is_err());
        assert!(decode_raw(&raw("RGB565", [1, 1, 1], "RGB", "UNSIGNED_BYTE"), &[0; 3]).is_err());
        // Rows padded to 4 bytes.
        let (_, px) = decode_raw(
            &raw("R8", [3, 2, 1], "RED", "UNSIGNED_BYTE"),
            &[1, 2, 3, 0, 4, 5, 6, 0],
        )
        .unwrap();
        assert_eq!(px, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn halves_round_trip() {
        for v in [0.0, 1.0, -2.5, 0.333, 65504.0, 1e-5] {
            let back = f16_value(f16_bits(v));
            assert!((back - v).abs() <= v.abs() * 1e-3 + 1e-7, "{v} → {back}");
        }
        assert_eq!(f16_value(f16_bits(1e9)), f32::INFINITY);
    }
}
