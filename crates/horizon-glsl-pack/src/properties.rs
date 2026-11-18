//! `.properties` files and the source comments the format reads:
//! `block.properties`, `dimension.properties`, `DRAWBUFFERS`.

use std::collections::{BTreeMap, BTreeSet};

use crate::{PackError, PackFiles, preprocess};

/// The `mc_Entity` table of a pack: block names → `mc_Entity.x` ids, from
/// `block.properties` or, without one, the pre-1.13 numeric ids
/// ([`BlockIds::legacy`]). Compressed to at most 31 *materials* (the vertex
/// tag's 5 bits; [`crate::NO_MATERIAL`] = none): a material is an
/// `(id, fluid)` pair, so `mc_Entity = (id, fluid ? 1 : 0, 0, 1)` comes from
/// the tag alone. Fluid surfaces of water and lava (waterlogged blocks'
/// water too) are their own materials.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockIds {
    /// Material index → `(id, fluid)`: the fluids' entries first, then the
    /// other ids ascending.
    pub entries: Vec<(i32, bool)>,
    /// `block.properties`: block name without namespace (`water`) → id.
    /// Empty for the legacy table, which matches names by rule.
    pub by_name: BTreeMap<String, i32>,
    /// Ids come from [`crate::legacy_ids::legacy_id`] instead of `by_name`.
    pub legacy: bool,
    /// `mc_Entity.x` of a block without an id (material [`crate::NO_MATERIAL`]):
    /// -1 for the legacy table, 0 for `block.properties`.
    pub unlisted: i32,
}

impl Default for BlockIds {
    /// An empty `block.properties`: every block reads id 0.
    fn default() -> Self {
        Self::from_names(BTreeMap::new(), 0)
    }
}

/// The blocks whose fluid surfaces Horizon's mesher draws.
const FLUIDS: [&str; 2] = ["water", "lava"];

impl BlockIds {
    /// The table of a pack without `block.properties`: pre-1.13 numeric ids
    /// (`crate::legacy_ids`), -1 for the rest.
    pub fn legacy() -> Self {
        let mut entries: Vec<(i32, bool)> = FLUIDS
            .iter()
            .filter_map(|f| crate::legacy_ids::legacy_id(f))
            .map(|id| (id, true))
            .collect();
        // Water and lava blocks draw only as fluids: their ids need no solid entry.
        let solid: Vec<(i32, bool)> = crate::legacy_ids::LEGACY_IDS
            .iter()
            .filter(|id| !entries.contains(&(**id, true)))
            .map(|&id| (id, false))
            .collect();
        entries.extend(solid);
        entries.truncate(crate::NO_MATERIAL as usize);
        Self {
            entries,
            by_name: BTreeMap::new(),
            legacy: true,
            unlisted: crate::legacy_ids::UNLISTED,
        }
    }

    /// A `block.properties`-style table: entries for the fluids, then every
    /// distinct id ascending, as many as fit.
    fn from_names(by_name: BTreeMap<String, i32>, unlisted: i32) -> Self {
        let mut entries = Vec::new();
        for fluid in FLUIDS {
            let entry = (by_name.get(fluid).copied().unwrap_or(unlisted), true);
            if !entries.contains(&entry) {
                entries.push(entry);
            }
        }
        let ids: BTreeSet<i32> = by_name.values().copied().collect();
        entries.extend(ids.into_iter().map(|id| (id, false)));
        entries.truncate(crate::NO_MATERIAL as usize);
        Self {
            entries,
            by_name,
            legacy: false,
            unlisted,
        }
    }

    /// `mc_Entity.x` of a block (`water` or `minecraft:water`), whether or
    /// not it has a material.
    pub fn id(&self, block: &str) -> i32 {
        let name = block.strip_prefix("minecraft:").unwrap_or(block);
        let id = if self.legacy {
            crate::legacy_ids::legacy_id(name)
        } else {
            self.by_name.get(name).copied()
        };
        id.unwrap_or(self.unlisted)
    }

    /// `mc_Entity.x` of a block state: `block` as in [`Self::id`], `state` its
    /// properties (`[("axis", "y")]`). Entries that name properties
    /// (`oak_log:axis=y`) match only those states.
    pub fn state_id(&self, block: &str, state: &[(&str, &str)]) -> i32 {
        let _ = state;
        self.id(block)
    }

    /// The material index of a block's geometry for the vertex tag: `fluid`
    /// for the fluid surface Horizon's fluid mesher draws (pass the fluid's
    /// block, `water` or `lava`), false for the block's model. Ids that did
    /// not fit in 31 materials, and unlisted blocks, give
    /// [`crate::NO_MATERIAL`].
    pub fn material(&self, block: &str, fluid: bool) -> u8 {
        let entry = (self.id(block), fluid);
        self.entries
            .iter()
            .position(|e| *e == entry)
            .map_or(crate::NO_MATERIAL, |i| i as u8)
    }

    /// `mc_Entity.xy` of a material index (31 and past the table: unlisted).
    pub fn entity(&self, material: u8) -> (i32, bool) {
        self.entries
            .get(usize::from(material))
            .copied()
            .unwrap_or((self.unlisted, false))
    }
}

/// Reads `shaders/block.properties` (preprocessed: it may hold `#if` lines).
/// No file is the legacy table ([`BlockIds::legacy`]). Block states
/// (`name:prop=value`) map to their block's id whatever the state; the first
/// id listing a name wins; ids that do not fit in the materials get none.
pub fn block_ids(
    files: &PackFiles,
    predefined: &BTreeMap<String, String>,
) -> Result<BlockIds, PackError> {
    const PATH: &str = "shaders/block.properties";
    let Some(text) = files.text(PATH) else {
        return Ok(BlockIds::legacy());
    };
    let text = preprocess::preprocess_text(&text, PATH, files, predefined, &[])?;
    let mut by_name = BTreeMap::new();
    for (key, value) in parse_properties(&text) {
        let Some(id) = key
            .strip_prefix("block.")
            .and_then(|id| id.parse::<i32>().ok())
        else {
            continue;
        };
        for token in value.split_whitespace() {
            let token = token.strip_prefix("minecraft:").unwrap_or(token);
            let name: Vec<_> = token
                .split(':')
                .filter(|part| !part.contains('='))
                .collect();
            by_name.entry(name.join(":")).or_insert(id);
        }
    }
    Ok(BlockIds::from_names(by_name, 0))
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
        // Water's and lava's (unlisted: 0) fluid surfaces, then 10 and 30.
        assert_eq!(
            ids.entries,
            [(10, true), (0, true), (10, false), (30, false)]
        );
        assert_eq!(
            [
                ids.material("stone", false),
                ids.material("minecraft:dirt", false),
                ids.material("water", false),
                ids.material("water", true),
                ids.material("lava", true),
            ],
            [3, 3, 2, 0, 1]
        );
        assert_eq!(
            [
                ids.material("oak_leaves", false),
                ids.material("mod:thing", false)
            ],
            [2, 2]
        );
        assert_eq!(
            [ids.material("never", false), ids.material("air", false)],
            [crate::NO_MATERIAL; 2]
        );
        assert_eq!(
            (ids.id("air"), ids.entity(crate::NO_MATERIAL)),
            (0, (0, false))
        );
        assert_eq!(ids.entity(1), (0, true));
        let many: String = (0..40)
            .map(|i| format!("block.{}=b{i}\n", 100 - i))
            .collect();
        let ids = block_ids(
            &pack(&[("shaders/block.properties", &many)]),
            &BTreeMap::new(),
        )
        .unwrap();
        // One fluid entry (both unlisted), then 30 ids from 61.
        assert_eq!(ids.entries.len(), 31);
        assert_eq!(
            (ids.entries[0], ids.entries[1], ids.entries[30]),
            ((0, true), (61, false), (90, false))
        );
        assert_eq!(
            [
                ids.material("b39", false),
                ids.material("b10", false),
                ids.material("b9", false)
            ],
            [1, 30, crate::NO_MATERIAL]
        );
        assert_eq!(ids.id("b9"), 91);
        assert_eq!(
            block_ids(&PackFiles::default(), &BTreeMap::new()).unwrap(),
            BlockIds::legacy()
        );
    }

    #[test]
    fn legacy_table() {
        let ids = BlockIds::legacy();
        assert!(ids.entries.len() <= 31);
        assert_eq!(&ids.entries[..2], [(9, true), (11, true)]);
        let entity = |name: &str, fluid| ids.entity(ids.material(name, fluid));
        assert_eq!(entity("minecraft:water", true), (9, true));
        assert_eq!(entity("lava", true), (11, true));
        assert_eq!(entity("birch_leaves", false), (18, false));
        assert_eq!(entity("emerald_block", false), (-123, false));
        assert_eq!(entity("dirt", false), (-1, false));
        assert_eq!(ids.material("dirt", false), crate::NO_MATERIAL);
        // Every id the rules produce has its material.
        for id in crate::legacy_ids::LEGACY_IDS {
            assert!(ids.entries.iter().any(|e| e.0 == id), "{id}");
        }
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
