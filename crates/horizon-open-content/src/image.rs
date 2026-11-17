//! RGBA images: decoding the vendored PNGs whatever their colour type, and
//! encoding byte-identical PNGs for the same pixels.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
        }
    }
    pub fn filled(width: u32, height: u32, color: [u8; 4]) -> Self {
        let mut image = Self::new(width, height);
        for pixel in image.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&color);
        }
        image
    }
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        self.pixels[i..i + 4].try_into().unwrap()
    }
    pub fn set(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x < self.width && y < self.height {
            let i = ((y * self.width + x) * 4) as usize;
            self.pixels[i..i + 4].copy_from_slice(&color);
        }
    }
    pub fn fill(&mut self, x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, color);
            }
        }
    }
    /// Copy `other` with its top left corner at (x, y).
    pub fn blit(&mut self, other: &Rgba, x: u32, y: u32) {
        for yy in 0..other.height {
            for xx in 0..other.width {
                self.set(x + xx, y + yy, other.get(xx, yy));
            }
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        let mut buffer = vec![
            0;
            reader
                .output_buffer_size()
                .ok_or("an image too large to decode")?
        ];
        let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
        let (width, height) = (info.width, info.height);
        let data = &buffer[..info.buffer_size()];
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        match info.color_type {
            png::ColorType::Rgba => pixels.extend_from_slice(data),
            png::ColorType::Rgb => {
                for p in data.chunks_exact(3) {
                    pixels.extend_from_slice(&[p[0], p[1], p[2], 255]);
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for p in data.chunks_exact(2) {
                    pixels.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
                }
            }
            png::ColorType::Grayscale => {
                for p in data {
                    pixels.extend_from_slice(&[*p, *p, *p, 255]);
                }
            }
            png::ColorType::Indexed => return Err("an indexed image was not expanded".into()),
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// 8-bit RGBA, no filtering choices left to heuristics, no metadata: the
    /// same pixels always give the same bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_compression(png::Compression::Balanced);
            encoder.set_filter(png::Filter::NoFilter);
            let mut writer = encoder.write_header().expect("PNG header");
            writer.write_image_data(&self.pixels).expect("PNG data");
        }
        out
    }
}

/// A small deterministic hash for procedural texture noise.
pub fn noise(x: u32, y: u32, seed: u32) -> u32 {
    let mut h =
        x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77) ^ seed.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_round_trips_and_is_stable() {
        let mut image = Rgba::new(3, 2);
        image.set(1, 1, [1, 2, 3, 4]);
        let bytes = image.encode();
        assert_eq!(bytes, image.encode());
        assert_eq!(Rgba::decode(&bytes).unwrap(), image);
    }
}
