//! Custom images (`image.<name>` in shaders.properties): textures a pack's
//! programs write with `imageStore` and read with `imageLoad`, or sample
//! under the declaration's sampler name. See docs/shader-pack-format-notes.md.
//!
//! A declaration is `image.<name>=<sampler|none> <pixel format> <internal
//! format> <pixel type> <clear> <relative> <size…>`: with `relative` true the
//! size is two factors of the screen size, else one, two or three texel
//! counts (1D, 2D, 3D).

use crate::plan::{BufferFormat, FormatKind, StorageFormat};

/// An image's size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageSize {
    /// Factors of the screen size (2D), recreated when the window resizes.
    Relative(f32, f32),
    D1(u32),
    D2(u32, u32),
    D3(u32, u32, u32),
}

/// An image's dimension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImageDim {
    D1,
    D2,
    D3,
}

impl ImageSize {
    pub fn dim(self) -> ImageDim {
        match self {
            Self::Relative(..) | Self::D2(..) => ImageDim::D2,
            Self::D1(_) => ImageDim::D1,
            Self::D3(..) => ImageDim::D3,
        }
    }

    /// Texels (width, height, depth) for a screen of `width`×`height`.
    pub fn extent(self, width: u32, height: u32) -> [u32; 3] {
        match self {
            Self::Relative(x, y) => [
                ((width as f32 * x) as u32).max(1),
                ((height as f32 * y) as u32).max(1),
                1,
            ],
            Self::D1(w) => [w, 1, 1],
            Self::D2(w, h) => [w, h, 1],
            Self::D3(w, h, d) => [w, h, d],
        }
    }
}

/// One `image.<name>` declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageSpec {
    /// The uniform the programs declare (`image2D <name>`).
    pub name: String,
    /// The sampler name for sampled reads (`none`: no sampler).
    pub sampler: Option<String>,
    /// The internal format as declared.
    pub format: BufferFormat,
    /// Cleared to zero at the start of every frame; else kept across frames.
    pub clear: bool,
    pub size: ImageSize,
}

/// Bytes one image may take (the client creates it whole, every texel).
pub const MAX_IMAGE_BYTES: u64 = 256 << 20;

impl ImageSpec {
    /// The WebGPU format the client creates it in, which every WebGPU device
    /// can use as a storage texture (see [`storage_format`]).
    pub fn storage(&self) -> StorageFormat {
        storage_format(self.format)
    }

    /// Bytes for a screen of `width`×`height`.
    pub fn bytes(&self, width: u32, height: u32) -> u64 {
        let [w, h, d] = self.size.extent(width, height);
        u64::from(w) * u64::from(h) * u64::from(d) * u64::from(self.storage().bytes())
    }
}

/// The storage format of an image's internal format. Core WebGPU stores only
/// `rgba8unorm`, `rgba8{s,u}int`, `rgba16{float,sint,uint}` and the 32-bit
/// `r`, `rg` and `rgba` formats, so the others widen: 8-bit normalized to
/// `rgba8unorm`, 16-bit normalized, signed normalized and packed floats to a
/// float format, 1- and 2-channel 8/16-bit integers to 32 bits, 3-channel
/// to 4. Values keep their range; 16-bit normalized ones in `rgba16float`
/// keep 11 bits of precision.
pub fn storage_format(format: BufferFormat) -> StorageFormat {
    use StorageFormat as S;
    let channels = format.channels();
    let pick = |one, two, four| match channels {
        1 => one,
        2 => two,
        _ => four,
    };
    let bits8 = matches!(
        format.storage(),
        S::R8Sint | S::Rg8Sint | S::Rgba8Sint | S::R8Uint | S::Rg8Uint | S::Rgba8Uint
    );
    let bits16 = matches!(
        format.storage(),
        S::R16Sint | S::Rg16Sint | S::Rgba16Sint | S::R16Uint | S::Rg16Uint | S::Rgba16Uint
    );
    match format.kind() {
        FormatKind::Unorm
            if matches!(format.storage(), S::R8Unorm | S::Rg8Unorm | S::Rgba8Unorm) =>
        {
            S::Rgba8Unorm
        }
        FormatKind::Unorm if format == BufferFormat::Rgb10A2 => S::Rgba16Float,
        FormatKind::Unorm | FormatKind::Snorm => pick(S::R32Float, S::Rg32Float, S::Rgba16Float),
        FormatKind::Float => match format {
            BufferFormat::R32F | BufferFormat::R16F => S::R32Float,
            BufferFormat::Rg32F | BufferFormat::Rg16F => S::Rg32Float,
            BufferFormat::Rgb32F | BufferFormat::Rgba32F => S::Rgba32Float,
            _ => S::Rgba16Float,
        },
        FormatKind::Sint if channels > 2 && bits8 => S::Rgba8Sint,
        FormatKind::Sint if channels > 2 && bits16 => S::Rgba16Sint,
        FormatKind::Sint => pick(S::R32Sint, S::Rg32Sint, S::Rgba32Sint),
        FormatKind::Uint if format == BufferFormat::Rgb10A2UI => S::Rgba16Uint,
        FormatKind::Uint if channels > 2 && bits8 => S::Rgba8Uint,
        FormatKind::Uint if channels > 2 && bits16 => S::Rgba16Uint,
        FormatKind::Uint => pick(S::R32Uint, S::Rg32Uint, S::Rgba32Uint),
    }
}

/// The GLSL `layout(…)` name of a storage format (naga's GLSL front end
/// reads these).
pub fn glsl_format(format: StorageFormat) -> &'static str {
    use StorageFormat as S;
    match format {
        S::Rgba8Unorm => "rgba8",
        S::Rgba16Float => "rgba16f",
        S::R32Float => "r32f",
        S::Rg32Float => "rg32f",
        S::Rgba32Float => "rgba32f",
        S::Rgba8Sint => "rgba8i",
        S::Rgba16Sint => "rgba16i",
        S::R32Sint => "r32i",
        S::Rg32Sint => "rg32i",
        S::Rgba32Sint => "rgba32i",
        S::Rgba8Uint => "rgba8ui",
        S::Rgba16Uint => "rgba16ui",
        S::R32Uint => "r32ui",
        S::Rg32Uint => "rg32ui",
        S::Rgba32Uint => "rgba32ui",
        // Not produced by `storage_format`.
        S::R8Unorm | S::Rg8Unorm => "rgba8",
        S::R16Float | S::Rg16Float | S::Rg11b10Ufloat | S::Rgb10a2Unorm => "rgba16f",
        S::Rgb10a2Uint => "rgba16ui",
        S::R8Sint | S::R16Sint => "r32i",
        S::Rg8Sint | S::Rg16Sint => "rg32i",
        S::R8Uint | S::R16Uint => "r32ui",
        S::Rg8Uint | S::Rg16Uint => "rg32ui",
    }
}

/// Whether core WebGPU lets a shader read and write a texture of this format
/// in one binding (`read_write`): only the 32-bit one-channel formats.
pub fn read_write_in_core(format: StorageFormat) -> bool {
    matches!(
        format,
        StorageFormat::R32Float | StorageFormat::R32Sint | StorageFormat::R32Uint
    )
}

/// One declaration, or why it is not one.
fn parse_one(key: &str, value: &str) -> Result<ImageSpec, String> {
    let name = key.strip_prefix("image.").unwrap_or(key).trim();
    let words: Vec<&str> = value.split_whitespace().collect();
    if words.len() < 7 {
        return Err(format!(
            "image.{name}: expected at least 7 values, found {}",
            words.len()
        ));
    }
    let format = BufferFormat::parse(words[2])
        .ok_or_else(|| format!("image.{name}: unknown format {}", words[2]))?;
    let flag = |w: &str| match w.to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("image.{name}: `{other}` is not true or false")),
    };
    let clear = flag(words[4])?;
    let relative = flag(words[5])?;
    let sizes = &words[6..];
    let size = if relative {
        let f = |w: &str| {
            w.trim_end_matches(['f', 'F'])
                .parse::<f32>()
                .ok()
                .filter(|v| *v > 0.0 && *v <= 16.0)
                .ok_or_else(|| format!("image.{name}: bad relative size {w}"))
        };
        match sizes {
            [x, y, ..] => ImageSize::Relative(f(x)?, f(y)?),
            _ => return Err(format!("image.{name}: a relative size needs two factors")),
        }
    } else {
        let n = |w: &str| {
            w.parse::<u32>()
                .ok()
                .filter(|v| (1..=16384).contains(v))
                .ok_or_else(|| format!("image.{name}: bad size {w}"))
        };
        match sizes {
            [x] => ImageSize::D1(n(x)?),
            [x, y] => ImageSize::D2(n(x)?, n(y)?),
            [x, y, z, ..] => ImageSize::D3(n(x)?, n(y)?, n(z)?),
            [] => unreachable!(),
        }
    };
    Ok(ImageSpec {
        name: name.to_string(),
        sampler: Some(words[0])
            .filter(|s| !s.eq_ignore_ascii_case("none"))
            .map(str::to_string),
        format,
        clear,
        size,
    })
}

/// Every `image.*` declaration of shaders.properties (the last of a name
/// wins), and the reasons the malformed ones are left out.
pub fn parse(entries: &[(String, String)]) -> (Vec<ImageSpec>, Vec<String>) {
    let mut images: Vec<ImageSpec> = Vec::new();
    let mut errors = Vec::new();
    for (key, value) in entries.iter().filter(|(k, _)| k.starts_with("image.")) {
        match parse_one(key, value) {
            Ok(spec) => match images.iter_mut().find(|i| i.name == spec.name) {
                Some(old) => *old = spec,
                None => images.push(spec),
            },
            Err(e) => errors.push(e),
        }
    }
    (images, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn declarations() {
        let (images, errors) = parse(&entries(&[
            (
                "image.lit",
                "litSampler rgba rgba16f float true true 0.5 1.0",
            ),
            (
                "image.voxels",
                "none RED_INTEGER R32UI UNSIGNED_INT true false 256 128 256",
            ),
            (
                "image.voxels",
                "none RED_INTEGER R32UI UNSIGNED_INT true false 64 32 64",
            ),
            (
                "image.lut",
                "lutTex RGBA RGBA16F HALF_FLOAT false false 256 64",
            ),
            ("image.line", "lineTex RED R32F FLOAT false false 512"),
            ("image.bad", "x RGBA NOPE FLOAT true true 1.0 1.0"),
            ("image.short", "x RGBA RGBA8 BYTE true"),
        ]));
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert_eq!(images.len(), 4);
        assert_eq!(images[0].sampler.as_deref(), Some("litSampler"));
        assert_eq!(images[0].size, ImageSize::Relative(0.5, 1.0));
        assert_eq!(images[0].size.extent(1280, 720), [640, 720, 1]);
        assert!(images[0].clear);
        assert_eq!(images[1].sampler, None);
        assert_eq!(images[1].size, ImageSize::D3(64, 32, 64));
        assert_eq!(images[1].bytes(1, 1), 64 * 32 * 64 * 4);
        assert_eq!(images[2].size.dim(), ImageDim::D2);
        assert!(!images[2].clear);
        assert_eq!(images[3].size.dim(), ImageDim::D1);
    }

    #[test]
    fn every_format_has_a_storage_format() {
        use StorageFormat as S;
        let f = |n: &str| storage_format(BufferFormat::parse(n).unwrap());
        assert_eq!(f("R8UI"), S::R32Uint);
        assert_eq!(f("R16UI"), S::R32Uint);
        assert_eq!(f("RGBA8"), S::Rgba8Unorm);
        assert_eq!(f("RG8"), S::Rgba8Unorm);
        assert_eq!(f("RGBA16"), S::Rgba16Float);
        assert_eq!(f("R16"), S::R32Float);
        assert_eq!(f("R16F"), S::R32Float);
        assert_eq!(f("RGBA16F"), S::Rgba16Float);
        assert_eq!(f("RGB32F"), S::Rgba32Float);
        assert_eq!(f("RGB10_A2"), S::Rgba16Float);
        assert_eq!(f("RGBA8_SNORM"), S::Rgba16Float);
        assert_eq!(f("RGB8I"), S::Rgba8Sint);
        assert_eq!(f("RG16I"), S::Rg32Sint);
        assert_eq!(f("RGB10_A2UI"), S::Rgba16Uint);
        for row in [
            "R8",
            "RG8",
            "RGB8",
            "RGBA8",
            "R16",
            "RGBA16",
            "R11F_G11F_B10F",
            "RGB9_E5",
            "RGBA4",
            "R8_SNORM",
            "R8I",
            "RGBA32I",
            "R32UI",
            "RGBA16UI",
        ] {
            let s = f(row);
            // Every one is a core WebGPU storage format with a GLSL name.
            assert!(!glsl_format(s).is_empty());
            assert!(
                [
                    S::Rgba8Unorm,
                    S::Rgba16Float,
                    S::R32Float,
                    S::Rg32Float,
                    S::Rgba32Float,
                    S::Rgba8Sint,
                    S::Rgba16Sint,
                    S::R32Sint,
                    S::Rg32Sint,
                    S::Rgba32Sint,
                    S::Rgba8Uint,
                    S::Rgba16Uint,
                    S::R32Uint,
                    S::Rg32Uint,
                    S::Rgba32Uint
                ]
                .contains(&s),
                "{row} → {s:?}"
            );
        }
        assert!(read_write_in_core(S::R32Uint) && !read_write_in_core(S::Rgba16Float));
    }
}
