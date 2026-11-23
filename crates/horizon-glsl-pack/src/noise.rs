//! The default `noisetex`: a square RGBA8 image whose RGB are independent
//! uniform random bytes and whose alpha is 255 (format notes §10). The seed
//! is fixed so a pack looks the same on every run.

/// Seed of the generator; any fixed value does.
const SEED: u64 = 0x853c_49e6_748f_ea9b;
/// PCG's stream increment (must be odd).
const STREAM: u64 = 0xda3e_39cb_94b9_5bdb;

/// A PCG32 (XSH-RR) generator: small, fast, and good enough for noise.
struct Pcg32 {
    state: u64,
}

impl Pcg32 {
    fn new(seed: u64) -> Self {
        let mut rng = Self { state: 0 };
        rng.next();
        rng.state = rng.state.wrapping_add(seed);
        rng.next();
        rng
    }

    fn next(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(STREAM);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        xorshifted.rotate_right((old >> 59) as u32)
    }
}

/// `size × size` RGBA8 pixels, rows top first (row 0 at v = 0, no flip),
/// `size * size * 4` bytes. `size` is `PackPlan::noise_resolution`.
pub fn noise_rgba(size: u32) -> Vec<u8> {
    let pixels = size as usize * size as usize;
    let mut rng = Pcg32::new(SEED);
    let mut out = Vec::with_capacity(pixels * 4);
    for _ in 0..pixels {
        let [r, g, b, _] = rng.next().to_le_bytes();
        out.extend_from_slice(&[r, g, b, 255]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_fixed_opaque_and_uniform() {
        let a = noise_rgba(64);
        assert_eq!(a.len(), 64 * 64 * 4);
        assert_eq!(a, noise_rgba(64));
        assert!(a.chunks(4).all(|p| p[3] == 255));
        // Each channel's mean is near 127.5 and every value occurs.
        for ch in 0..3 {
            let values: Vec<u8> = a.chunks(4).map(|p| p[ch]).collect();
            let mean = values.iter().map(|&v| f64::from(v)).sum::<f64>() / values.len() as f64;
            assert!((mean - 127.5).abs() < 4.0, "channel {ch}: {mean}");
            let mut seen = [false; 256];
            for v in values {
                seen[usize::from(v)] = true;
            }
            assert!(seen.iter().all(|&s| s));
        }
        // Channels are not copies of each other.
        assert!(a.chunks(4).filter(|p| p[0] == p[1]).count() < 64);
        assert!(noise_rgba(0).is_empty());
    }
}
