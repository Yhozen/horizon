//! Compute programs (`.csh`) and storage buffers (`bufferObject.N`).
//!
//! From the format docs (format notes §18): a composite-style pass may have
//! compute programs beside its full-screen one: `composite2.csh`, then
//! `composite2_a.csh` … `composite2_z.csh`, in that order, all before the
//! pass's own vertex and fragment stages. `setup*` programs are compute only
//! and run once, when the pack loads; `begin*` run at the start of every
//! frame, before the shadow map; `shadowcomp*` after it. A program says its
//! workgroup size with `layout(local_size_x = …) in;` and how many groups to
//! dispatch with `const ivec3 workGroups = ivec3(…);` or, relative to the
//! render size, `const vec2 workGroupsRender = vec2(…);`.

use std::collections::BTreeMap;

use crate::PackFiles;

/// The passes that may have compute programs, in the order they run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComputePass {
    /// Once, when the pack loads (`setup`, `setup1` … `setup99`).
    Setup,
    /// Every frame, before the shadow map (`begin` …).
    Begin,
    /// After the shadow map (`shadowcomp` …).
    ShadowComp,
    Prepare,
    Deferred,
    Composite,
}

impl ComputePass {
    pub const ALL: [ComputePass; 6] = [
        Self::Setup,
        Self::Begin,
        Self::ShadowComp,
        Self::Prepare,
        Self::Deferred,
        Self::Composite,
    ];

    pub fn base(self) -> &'static str {
        match self {
            Self::Setup => "setup",
            Self::Begin => "begin",
            Self::ShadowComp => "shadowcomp",
            Self::Prepare => "prepare",
            Self::Deferred => "deferred",
            Self::Composite => "composite",
        }
    }
}

/// One compute program slot: `composite2_a` is `(Composite, 2, Some(0))`.
/// The derived order is the order they run in: by pass, by number, the
/// unlettered program first and then `_a` … `_z`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComputeSlot {
    pub pass: ComputePass,
    /// The pass number (`composite` is 0, `composite12` is 12).
    pub n: u8,
    /// `_a` is 0 … `_z` is 25.
    pub letter: Option<u8>,
}

impl ComputeSlot {
    /// `composite2_a`, `setup`, `begin1`.
    pub fn name(self) -> String {
        let mut s = self.pass.base().to_string();
        if self.n > 0 {
            s += &self.n.to_string();
        }
        if let Some(l) = self.letter {
            s.push('_');
            s.push(char::from(b'a' + l));
        }
        s
    }

    /// The slot named `name`, if it is one.
    pub fn from_name(name: &str) -> Option<Self> {
        let (stem, letter) = match name.rsplit_once('_') {
            Some((stem, l)) if l.len() == 1 && l.as_bytes()[0].is_ascii_lowercase() => {
                (stem, Some(l.as_bytes()[0] - b'a'))
            }
            _ => (name, None),
        };
        for pass in ComputePass::ALL {
            let Some(rest) = stem.strip_prefix(pass.base()) else {
                continue;
            };
            let n = if rest.is_empty() {
                0
            } else if rest.starts_with('0') {
                return None;
            } else {
                rest.parse::<u8>()
                    .ok()
                    .filter(|&n| n <= crate::program::MAX_PASS)?
            };
            return Some(Self { pass, n, letter });
        }
        None
    }

    /// The full-screen slot this program runs before, if the pass is one
    /// Horizon runs (prepare, deferred, composite).
    pub fn pass_kind(self) -> Option<crate::ProgramKind> {
        use crate::ProgramKind as K;
        match self.pass {
            ComputePass::Prepare => Some(K::Prepare(self.n)),
            ComputePass::Deferred => Some(K::Deferred(self.n)),
            ComputePass::Composite => Some(K::Composite(self.n)),
            _ => None,
        }
    }
}

/// How many workgroups a compute program dispatches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dispatch {
    /// `const ivec3 workGroups = ivec3(x, y, z);`
    Fixed([u32; 3]),
    /// `const vec2 workGroupsRender = vec2(x, y);`: enough groups to cover
    /// the render size times `(x, y)` (one in z).
    Render([f32; 2]),
}

impl Default for Dispatch {
    /// With neither constant: one invocation per pixel (inferred; not probed).
    fn default() -> Self {
        Self::Render([1.0, 1.0])
    }
}

impl Dispatch {
    /// The workgroup counts for a `width`×`height` render with groups of `local`.
    pub fn groups(self, width: u32, height: u32, local: [u32; 3]) -> [u32; 3] {
        match self {
            Self::Fixed(g) => g,
            Self::Render([sx, sy]) => {
                let px = |size: u32, s: f32| (size as f32 * s).ceil().max(0.0) as u32;
                [
                    px(width, sx).div_ceil(local[0].max(1)),
                    px(height, sy).div_ceil(local[1].max(1)),
                    1,
                ]
            }
        }
    }

    /// The dispatch a preprocessed compute source declares (the last
    /// `workGroups`/`workGroupsRender` constant wins), if any.
    pub fn parse(source: &str) -> Result<Option<Self>, String> {
        let mut found = None;
        for (name, n) in [("workGroups", 3), ("workGroupsRender", 2)] {
            let mut rest = source;
            while let Some(at) = find_word(rest, name) {
                let after = &rest[at + name.len()..];
                rest = after;
                let Some(value) = after.trim_start().strip_prefix('=') else {
                    continue;
                };
                let end = value.find(';').unwrap_or(value.len());
                let value = value[..end].trim();
                let inner = value
                    .find('(')
                    .and_then(|o| value.rfind(')').map(|c| &value[o + 1..c]))
                    .ok_or_else(|| format!("`{name} = {value}`: not a constructor"))?;
                let parts: Vec<f32> = inner
                    .split(',')
                    .map(|p| {
                        p.trim()
                            .trim_end_matches(['f', 'F', 'u', 'U'])
                            .parse::<f32>()
                    })
                    .collect::<Result<_, _>>()
                    .map_err(|_| format!("`{name} = {value}`: not literal numbers"))?;
                let parts = if parts.len() == 1 {
                    vec![parts[0]; n]
                } else {
                    parts
                };
                if parts.len() != n {
                    return Err(format!("`{name} = {value}`: {n} components expected"));
                }
                found = Some(if n == 3 {
                    Self::Fixed([parts[0] as u32, parts[1] as u32, parts[2] as u32])
                } else {
                    Self::Render([parts[0], parts[1]])
                });
            }
        }
        Ok(found)
    }
}

/// The byte offset of `word` as a whole identifier in `text`.
fn find_word(text: &str, word: &str) -> Option<usize> {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut from = 0;
    while let Some(i) = text[from..].find(word).map(|i| i + from) {
        let before = text[..i].chars().next_back();
        let after = text[i + word.len()..].chars().next();
        if !before.is_some_and(ident) && !after.is_some_and(ident) {
            return Some(i);
        }
        from = i + word.len();
    }
    None
}

/// `bufferObject.N` of shaders.properties: a storage buffer, zeroed when
/// created and kept across frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BufferObject {
    /// Bytes; for a relative buffer, bytes per pixel.
    pub size: u64,
    /// `bufferObject.N = <bytes> true <x> <y>`: `size` per pixel of a
    /// render-sized grid scaled by `(x, y)`, recreated (zeroed) on resize.
    pub relative: Option<[f32; 2]>,
}

impl BufferObject {
    /// The buffer's bytes for a `width`×`height` render: at least 16 (an
    /// empty binding is not allowed), a multiple of 4.
    pub fn bytes(&self, width: u32, height: u32) -> u64 {
        let n = match self.relative {
            None => self.size,
            Some([sx, sy]) => {
                let px = |s: u32, k: f32| (s as f32 * k).ceil().max(1.0) as u64;
                self.size * px(width, sx) * px(height, sy)
            }
        };
        n.max(16).div_ceil(4) * 4
    }

    /// The `bufferObject.*` entries, by index (a later line wins).
    pub fn parse_all(entries: &[(String, String)]) -> (BTreeMap<u32, BufferObject>, Vec<String>) {
        let mut out = BTreeMap::new();
        let mut notes = Vec::new();
        for (key, value) in entries {
            let Some(index) = key.strip_prefix("bufferObject.") else {
                continue;
            };
            let Ok(index) = index.trim().parse::<u32>() else {
                notes.push(format!("{key}: not a buffer index"));
                continue;
            };
            let words: Vec<&str> = value.split_whitespace().collect();
            let size = words.first().and_then(|w| w.parse::<u64>().ok());
            let relative = match words.get(1).copied() {
                Some("true") => {
                    let f = |i: usize| words.get(i).and_then(|w| w.parse::<f32>().ok());
                    match (f(2), f(3)) {
                        (Some(x), Some(y)) => Some([x, y]),
                        _ => {
                            notes.push(format!("{key}={value}: a relative size needs two scales"));
                            continue;
                        }
                    }
                }
                _ => None,
            };
            match size {
                Some(size) => {
                    out.insert(index, BufferObject { size, relative });
                }
                None => notes.push(format!("{key}={value}: not a size in bytes")),
            }
        }
        (out, notes)
    }
}

/// Every compute program file for the dimension's `folder`, by slot: a
/// program of the world folder wins over `shaders/`'s of the same name.
/// `usable` filters paths (disabled programs, ones the GPU rejected).
pub fn find(
    files: &PackFiles,
    folder: &str,
    usable: &dyn Fn(&str) -> bool,
) -> BTreeMap<ComputeSlot, String> {
    let mut out = BTreeMap::new();
    let dirs = if folder.is_empty() {
        vec!["shaders/".to_string()]
    } else {
        vec!["shaders/".to_string(), format!("shaders/{folder}/")]
    };
    // shaders/ first, then the world folder overwrites.
    for dir in dirs {
        for path in files.0.keys() {
            let Some(rest) = path.strip_prefix(&dir) else {
                continue;
            };
            let Some(name) = rest.strip_suffix(".csh") else {
                continue;
            };
            if name.contains('/') {
                continue;
            }
            let Some(slot) = ComputeSlot::from_name(name) else {
                continue;
            };
            let program = format!("{dir}{name}");
            if usable(&program) {
                out.insert(slot, program);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_order_and_names() {
        let s = |n: &str| ComputeSlot::from_name(n).unwrap();
        for n in [
            "setup",
            "setup3_b",
            "begin1_a",
            "shadowcomp",
            "prepare_d",
            "deferred4_a",
            "composite99",
            "composite_z",
        ] {
            assert_eq!(s(n).name(), n);
        }
        assert!(ComputeSlot::from_name("composite100").is_none());
        assert!(ComputeSlot::from_name("composite02").is_none());
        assert!(ComputeSlot::from_name("final").is_none());
        assert!(ComputeSlot::from_name("voxel_photons").is_none());
        let mut v = vec![
            s("composite2_b"),
            s("composite2"),
            s("deferred"),
            s("composite2_a"),
            s("begin"),
            s("setup1"),
            s("composite10"),
        ];
        v.sort();
        let names: Vec<String> = v.into_iter().map(ComputeSlot::name).collect();
        assert_eq!(
            names,
            [
                "setup1",
                "begin",
                "deferred",
                "composite2",
                "composite2_a",
                "composite2_b",
                "composite10"
            ]
        );
    }

    #[test]
    fn dispatch_sizes() {
        let src = "layout(local_size_x = 8, local_size_y = 8) in;\nconst vec2 workGroupsRender = vec2(0.5, 0.5);\n";
        let d = Dispatch::parse(src).unwrap().unwrap();
        assert_eq!(d, Dispatch::Render([0.5, 0.5]));
        assert_eq!(d.groups(1280, 721, [8, 8, 1]), [80, 46, 1]);
        let d = Dispatch::parse("const ivec3 workGroups = ivec3(32, 16, 32);")
            .unwrap()
            .unwrap();
        assert_eq!(d.groups(10, 10, [8, 8, 8]), [32, 16, 32]);
        assert_eq!(Dispatch::parse("void main() {}").unwrap(), None);
        assert_eq!(Dispatch::default().groups(100, 50, [16, 16, 1]), [7, 4, 1]);
    }

    #[test]
    fn buffer_objects() {
        let e = |k: &str, v: &str| (k.to_string(), v.to_string());
        let (b, notes) = BufferObject::parse_all(&[
            e("bufferObject.0", "4"),
            e("bufferObject.3", "16 true 0.5 0.5"),
            e("bufferObject.x", "4"),
            e("bufferObject.5", "0"),
        ]);
        assert_eq!(notes.len(), 1);
        assert_eq!(b[&0].bytes(100, 100), 16);
        assert_eq!(b[&3].bytes(100, 101), 16 * 50 * 51);
        assert_eq!(b[&5].bytes(1, 1), 16);
    }
}
