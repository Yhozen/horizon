//! Storage buffer blocks (`layout(std430, binding = N) buffer Name { … } inst;`):
//! kept in the body, rebound to Horizon's binding of `bufferObject.N`.

use super::*;

/// Group 1 binding of `bufferObject.N` in every stage: 160 + N.
pub const STORAGE_BINDING: u32 = 160;
/// Buffer object indices Horizon binds (0 to 63).
pub const STORAGE_BUFFERS: u32 = 64;

/// One storage buffer a stage declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferUse {
    /// `N` of `binding = N` (`bufferObject.N`).
    pub index: u32,
    /// Declared `readonly` (or made so: the vertex stage cannot write storage in WebGPU).
    pub read_only: bool,
}

/// Words that may qualify a buffer block, and whether each is kept.
const BUFFER_QUALIFIERS: [(&str, bool); 6] = [
    ("readonly", true),
    ("writeonly", false),
    ("coherent", false),
    ("volatile", false),
    ("restrict", false),
    ("buffer", true),
];

impl Ctx<'_> {
    /// Emits a storage buffer block with its binding replaced and returns
    /// true; false when the item is not one.
    pub(super) fn buffer_block(
        &mut self,
        sig: &[usize],
        end: usize,
        out: &mut String,
    ) -> Result<bool, String> {
        let brace = match sig.iter().position(|&i| self.toks[i].text == "{") {
            Some(b) => b,
            None => return Ok(false),
        };
        let header: Vec<&str> = sig[..brace].iter().map(|&i| self.toks[i].text).collect();
        let Some(at) = header.iter().position(|w| *w == "buffer") else {
            return Ok(false);
        };
        let name = header.get(at + 1).copied().unwrap_or("");
        let mut packing = "std430".to_string();
        let mut index = 0u32;
        let mut words = &header[..at];
        if words.first() == Some(&"layout") {
            let close = words
                .iter()
                .position(|w| *w == ")")
                .ok_or("an unclosed layout(…)")?;
            let args = words[2..close].join(" ");
            for arg in args.split(',') {
                let arg: String = arg.split_whitespace().collect::<Vec<_>>().join("");
                if let Some(n) = arg.strip_prefix("binding=") {
                    index = n
                        .trim_end_matches(['u', 'U'])
                        .parse()
                        .map_err(|_| format!("buffer `{name}`: binding `{n}` is not a literal"))?;
                } else if matches!(arg.as_str(), "std430" | "std140" | "shared" | "packed") {
                    packing = if arg == "std140" {
                        arg
                    } else {
                        "std430".into()
                    };
                }
            }
            words = &words[close + 1..];
        }
        if index >= STORAGE_BUFFERS {
            return Err(format!(
                "buffer `{name}` at binding {index}: Horizon binds 0 to {}",
                STORAGE_BUFFERS - 1
            ));
        }
        let mut read_only = false;
        for w in words {
            match BUFFER_QUALIFIERS.iter().find(|(q, _)| q == w) {
                Some(("readonly", _)) => read_only = true,
                Some(_) => {}
                None => return Err(format!("buffer `{name}`: qualifier `{w}`")),
            }
        }
        let forced = self.stage == Stage::Vertex && !read_only;
        let read_only = read_only || forced;
        out.push_str(&format!(
            "layout({packing}, set = 1, binding = {}) {}buffer {name} ",
            STORAGE_BINDING + index,
            if read_only { "readonly " } else { "" }
        ));
        self.emit(sig[brace], end, out)?;
        match self.out.buffers.iter_mut().find(|b| b.index == index) {
            Some(b) => b.read_only &= read_only,
            None => self.out.buffers.push(BufferUse { index, read_only }),
        }
        Ok(true)
    }

    /// Whether `name` is a word GLSL 450 reserves that the pack uses as its
    /// own name (renamed `hz_u_…`). Compute programs are GLSL 430 and up:
    /// there `shared`, `buffer` and the memory qualifiers are keywords.
    pub(super) fn reserved(&self, name: &str) -> bool {
        listed(KEYWORDS, name)
            && !(self.stage == Stage::Compute
                && matches!(
                    name,
                    "shared"
                        | "buffer"
                        | "layout"
                        | "coherent"
                        | "volatile"
                        | "restrict"
                        | "readonly"
                        | "writeonly"
                ))
    }
}
