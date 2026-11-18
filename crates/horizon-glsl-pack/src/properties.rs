//! `.properties` files and the source comments the format reads:
//! `block.properties`, `dimension.properties`, `DRAWBUFFERS`.

use std::collections::{BTreeMap, BTreeSet};

use crate::{PackError, PackFiles, preprocess};

/// `block.properties`: block names → `mc_Entity.x` ids, compressed to at most
/// 31 material indices (the vertex tag's 5 bits; 31 = none).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockIds {
    /// Material index → id, ascending ids.
    pub ids: Vec<u32>,
    /// Block name without namespace (`water`) → material index.
    pub by_name: BTreeMap<String, u8>,
}

impl BlockIds {
    /// The material index of a block (`water` or `minecraft:water`), or
    /// [`crate::NO_MATERIAL`].
    pub fn material(&self, block: &str) -> u8 {
        let name = block.strip_prefix("minecraft:").unwrap_or(block);
        self.by_name
            .get(name)
            .copied()
            .unwrap_or(crate::NO_MATERIAL)
    }
}

/// Reads `shaders/block.properties` (preprocessed: it may hold `#if` lines).
/// No file is an empty table. Block states (`name:prop=value`) map to their
/// block's id whatever the state; the first id listing a name wins; ids past
/// the 31st ascending get no material.
pub fn block_ids(
    files: &PackFiles,
    predefined: &BTreeMap<String, String>,
) -> Result<BlockIds, PackError> {
    const PATH: &str = "shaders/block.properties";
    let Some(text) = files.text(PATH) else {
        return Ok(BlockIds::default());
    };
    let text = preprocess::preprocess_text(&text, PATH, files, predefined, &[])?;
    let mut entries = Vec::new();
    for (key, value) in parse_properties(&text) {
        let Some(id) = key
            .strip_prefix("block.")
            .and_then(|id| id.parse::<u32>().ok())
        else {
            continue;
        };
        entries.push((id, value));
    }
    let ids: Vec<u32> = entries
        .iter()
        .map(|e| e.0)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(crate::NO_MATERIAL as usize)
        .collect();
    let mut by_name = BTreeMap::new();
    for (id, value) in &entries {
        let Ok(index) = ids.binary_search(id) else {
            continue;
        };
        for token in value.split_whitespace() {
            let token = token.strip_prefix("minecraft:").unwrap_or(token);
            let name: Vec<_> = token
                .split(':')
                .filter(|part| !part.contains('='))
                .collect();
            by_name.entry(name.join(":")).or_insert(index as u8);
        }
    }
    Ok(BlockIds { ids, by_name })
}

/// `key=value` lines (`\` continuations joined, `#` comments skipped).
pub fn parse_properties(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let mut line = line.trim().to_string();
        while line.ends_with('\\') {
            line.pop();
            line.push_str(lines.next().unwrap_or("").trim_start());
        }
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        // Java's separator is the first `=` or `:`; block states use `:` only in values.
        let (key, value) = match line.find(['=', ':']) {
            Some(i) => (&line[..i], &line[i + 1..]),
            None => (line.as_str(), ""),
        };
        out.push((key.trim().to_string(), value.trim().to_string()));
    }
    out
}

/// `/* DRAWBUFFERS:067 */` → `[0, 6, 7]`, `/* RENDERTARGETS: 0,6,7 */` → same.
/// The last such comment in the source wins; `None` when there is none.
pub fn draw_buffers(source: &str) -> Option<Vec<u8>> {
    let draw = source.rfind("DRAWBUFFERS:").map(|i| (i, true));
    let targets = source.rfind("RENDERTARGETS:").map(|i| (i, false));
    let (at, digits) = draw.into_iter().chain(targets).max()?;
    let rest = &source[at..];
    let rest = rest[rest.find(':')? + 1..].trim_start();
    Some(if digits {
        // One character per buffer, `A`-`F` for 10-15.
        rest.chars()
            .map_while(|c| c.to_digit(16))
            .map(|d| d as u8)
            .collect()
    } else {
        let list = rest.split(['*', '\n']).next().unwrap_or("");
        list.split(',')
            .map_while(|n| n.trim().parse().ok())
            .collect()
    })
}

/// The folder under `shaders/` whose programs a dimension uses: from
/// `dimension.properties` when present, else `world0` for the overworld,
/// `world-1` for the nether, `world1` for the end. Any other dimension gets
/// an empty folder: its programs come from `shaders/` itself.
pub fn dimension_folder(files: &PackFiles, dimension: &str) -> String {
    let id = |s: &str| {
        if s.contains(':') {
            s.to_string()
        } else {
            format!("minecraft:{s}")
        }
    };
    let dimension = id(dimension);
    const PATH: &str = "shaders/dimension.properties";
    if let Some(text) = files.text(PATH) {
        let predefined = preprocess::predefined(crate::PackOptions::default().mc_version);
        let text =
            preprocess::preprocess_text(&text, PATH, files, &predefined, &[]).unwrap_or(text);
        let mut wildcard = None;
        for (key, value) in parse_properties(&text) {
            let Some(folder) = key.strip_prefix("dimension.") else {
                continue;
            };
            for name in value.split_whitespace() {
                if name == "*" {
                    wildcard.get_or_insert_with(|| folder.to_string());
                } else if id(name) == dimension {
                    return folder.to_string();
                }
            }
        }
        return wildcard.unwrap_or_default();
    }
    match dimension.as_str() {
        "minecraft:overworld" => "world0",
        "minecraft:the_nether" => "world-1",
        "minecraft:the_end" => "world1",
        _ => "",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(files: &[(&str, &str)]) -> PackFiles {
        PackFiles(
            files
                .iter()
                .map(|(p, t)| (p.to_string(), t.as_bytes().to_vec()))
                .collect(),
        )
    }

    #[test]
    fn properties_lines() {
        let text = "# comment\n! also\na = 1\nb=two \\\n   three\\\n four\n\nc: x=y\nflag\n";
        let want = [
            ("a", "1"),
            ("b", "two threefour"),
            ("c", "x=y"),
            ("flag", ""),
        ];
        assert_eq!(
            parse_properties(text),
            want.map(|(k, v)| (k.to_string(), v.to_string()))
        );
    }

    #[test]
    fn block_ids_compress_and_strip() {
        let text = "block.30=minecraft:stone dirt:snowy=true\n#if MC_VERSION >= 99999\nblock.5=never\n#endif\n\
                    block.10=water \\\n  minecraft:oak_leaves:persistent=true:distance=1 mod:thing stone\nblock.x=bad\n";
        let files = pack(&[("shaders/block.properties", text)]);
        let ids = block_ids(&files, &preprocess::predefined(12111)).unwrap();
        assert_eq!(ids.ids, [10, 30]);
        assert_eq!(
            [
                ids.material("stone"),
                ids.material("minecraft:dirt"),
                ids.material("water")
            ],
            [1, 1, 0]
        );
        assert_eq!(
            [ids.material("oak_leaves"), ids.material("mod:thing")],
            [0, 0]
        );
        assert_eq!(
            [ids.material("never"), ids.material("air")],
            [crate::NO_MATERIAL; 2]
        );
        let many: String = (0..40)
            .map(|i| format!("block.{}=b{i}\n", 100 - i))
            .collect();
        let ids = block_ids(
            &pack(&[("shaders/block.properties", &many)]),
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!((ids.ids.len(), ids.ids[0], ids.ids[30]), (31, 61, 91));
        assert_eq!(
            [ids.material("b39"), ids.material("b9"), ids.material("b0")],
            [0, 30, crate::NO_MATERIAL]
        );
        assert_eq!(
            block_ids(&PackFiles::default(), &BTreeMap::new()).unwrap(),
            BlockIds::default()
        );
    }

    #[test]
    fn draw_buffer_comments() {
        assert_eq!(
            draw_buffers("x /* DRAWBUFFERS:067 */ y"),
            Some(vec![0, 6, 7])
        );
        assert_eq!(
            draw_buffers("/* DRAWBUFFERS:0 */ /* RENDERTARGETS: 0, 6,7 */"),
            Some(vec![0, 6, 7])
        );
        assert_eq!(
            draw_buffers("/* RENDERTARGETS: 1 */\n/* DRAWBUFFERS:1AF */"),
            Some(vec![1, 10, 15])
        );
        assert_eq!(draw_buffers("void main() {}"), None);
    }

    #[test]
    fn dimension_folders() {
        let props = "dimension.world0 = *\ndimension.world-1 = minecraft:the_nether nether\ndimension.world7=mod:sky\n";
        let files = pack(&[("shaders/dimension.properties", props)]);
        let f = |d| dimension_folder(&files, d);
        assert_eq!(
            [
                f("minecraft:overworld"),
                f("the_nether"),
                f("minecraft:nether"),
                f("mod:sky"),
                f("x:y")
            ],
            ["world0", "world-1", "world-1", "world7", "world0"]
        );
        let no_star = pack(&[(
            "shaders/dimension.properties",
            "dimension.world1=minecraft:the_end\n",
        )]);
        assert_eq!(dimension_folder(&no_star, "minecraft:overworld"), "");
        let none = PackFiles::default();
        let f = |d| dimension_folder(&none, d);
        assert_eq!(
            [
                f("minecraft:overworld"),
                f("minecraft:the_nether"),
                f("minecraft:the_end"),
                f("mod:x")
            ],
            ["world0", "world-1", "world1", ""]
        );
    }
}
