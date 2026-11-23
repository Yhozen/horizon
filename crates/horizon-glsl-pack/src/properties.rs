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
    /// `block.properties` by block state: bare block name → its entries in
    /// file order (see [`Self::state_id`]).
    pub states: BTreeMap<String, Vec<StateRule>>,
    /// What was read but not used, for the report: tags, other mods'
    /// blocks, malformed entries, names listed twice.
    pub notes: Vec<String>,
}

/// One `name:prop=v1,v2:prop2=w` entry of `block.properties`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateRule {
    pub id: i32,
    /// Property → the values it matches; empty matches every state.
    pub properties: Vec<(String, Vec<String>)>,
}

impl StateRule {
    fn matches(&self, state: &[(&str, &str)]) -> bool {
        self.properties.iter().all(|(p, values)| {
            state
                .iter()
                .any(|(k, v)| k == p && values.iter().any(|w| w == v))
        })
    }
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
            states: BTreeMap::new(),
            notes: Vec::new(),
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
            states: BTreeMap::new(),
            notes: Vec::new(),
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
    /// (`oak_log:axis=y`) match only those states; of the entries that match,
    /// the one naming the most properties wins, the first in the file on a
    /// tie. Blocks of other namespaces and unmatched states read
    /// [`Self::unlisted`].
    pub fn state_id(&self, block: &str, state: &[(&str, &str)]) -> i32 {
        if self.legacy {
            return self.id(block);
        }
        let name = block.strip_prefix("minecraft:").unwrap_or(block);
        let mut best: Option<&StateRule> = None;
        for rule in self.states.get(name).into_iter().flatten() {
            if rule.matches(state)
                && best.is_none_or(|b| rule.properties.len() > b.properties.len())
            {
                best = Some(rule);
            }
        }
        best.map_or(self.unlisted, |r| r.id)
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

/// Reads `shaders/block.properties`, preprocessed with `macros` (the
/// predefined macros and the options': the file may hold `#if` and
/// `#define`). No file is the legacy table ([`BlockIds::legacy`]). Every
/// entry goes into [`BlockIds::states`]; for the material path
/// ([`BlockIds::id`]) a state maps to its block's id whatever the state, the
/// first id listing a name wins, and ids that do not fit in the materials get
/// none.
pub fn block_ids(
    files: &PackFiles,
    macros: &BTreeMap<String, String>,
) -> Result<BlockIds, PackError> {
    const PATH: &str = "shaders/block.properties";
    let Some(text) = files.text(PATH) else {
        return Ok(BlockIds::legacy());
    };
    let text = preprocess_properties(&text, PATH, files, macros)?;
    let mut by_name = BTreeMap::new();
    let mut states: BTreeMap<String, Vec<StateRule>> = BTreeMap::new();
    let mut notes = Vec::new();
    let mut skipped = Skipped::default();
    for (key, value) in parse_properties(&text) {
        let Some(id) = key.strip_prefix("block.") else {
            continue;
        };
        let Ok(id) = id.parse::<i32>() else {
            notes.push(format!("block.properties: {key} is not a number"));
            continue;
        };
        for token in value.split_whitespace() {
            let stripped = token.strip_prefix("minecraft:").unwrap_or(token);
            let name: Vec<_> = stripped
                .split(':')
                .filter(|part| !part.contains('='))
                .collect();
            by_name.entry(name.join(":")).or_insert(id);
            match parse_block_token(token) {
                Ok((name, properties)) => {
                    let rules = states.entry(name.to_string()).or_default();
                    if rules
                        .iter()
                        .any(|r| r.properties == properties && r.id != id)
                    {
                        notes.push(format!(
                            "block.properties: {token} is listed again under block.{id}"
                        ));
                    }
                    rules.push(StateRule { id, properties });
                }
                Err(skip) => skipped.add("block.properties", token, skip, &mut notes),
            }
        }
    }
    skipped.finish("block.properties", &mut notes);
    let mut ids = BlockIds::from_names(by_name, 0);
    ids.states = states;
    ids.notes = notes;
    Ok(ids)
}

/// An id `.properties` file through the preprocessor as the packs expect:
/// conditionals act on physical lines (a `\` before `#else` does not pull the
/// directive into the entry), then continuations are joined and the live
/// `#define`s expanded (packs list block groups as macros).
pub fn preprocess_properties(
    text: &str,
    file: &str,
    files: &PackFiles,
    macros: &BTreeMap<String, String>,
) -> Result<String, PackError> {
    let lines: Vec<&str> = text.lines().collect();
    let directive = |l: &str| l.trim_start().starts_with('#') && !is_comment(l);
    // Pass 1: which lines survive the conditionals. Each content line, and
    // each `#define`/`#undef` (kept, so later `#if`s see it), is followed or
    // replaced by a marker the preprocessor keeps when the line is live.
    let mut marked = String::new();
    for (i, l) in lines.iter().enumerate() {
        if directive(l) {
            let d = l.trim_start()[1..].trim_start();
            marked.push_str(l);
            marked.push('\n');
            if d.starts_with("define") || d.starts_with("undef") {
                marked.push_str(&format!("hz_line_{i}\n"));
            }
        } else if !l.trim().is_empty() && !is_comment(l) {
            marked.push_str(&format!("hz_line_{i}\n"));
        }
    }
    let kept = preprocess::preprocess_text(&marked, file, files, macros, &[])?;
    let live: Vec<usize> = kept
        .split_whitespace()
        .filter_map(|m| m.strip_prefix("hz_line_")?.parse().ok())
        .collect();
    // Pass 2: the live lines, continuations joined, macros expanded.
    let mut joined = String::new();
    let mut open = false;
    for &i in &live {
        let l = lines[i];
        if directive(l) {
            if open {
                joined.push('\n');
            }
            open = false;
            joined.push_str(l);
            joined.push('\n');
            continue;
        }
        let body = if open { l.trim_start() } else { l };
        match body.trim_end().strip_suffix('\\') {
            Some(rest) => {
                joined.push_str(rest);
                joined.push(' ');
                open = true;
            }
            None => {
                joined.push_str(body);
                joined.push('\n');
                open = false;
            }
        }
    }
    preprocess::preprocess_text(&joined, file, files, macros, &[])
}

/// A `#` line that is a properties comment, not a directive (`# text`,
/// `#block.5=`): anything whose first word is not a preprocessor keyword.
fn is_comment(line: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix('#') else {
        return false;
    };
    let word = rest.trim_start();
    let end = word
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(word.len());
    !matches!(
        &word[..end],
        "if" | "ifdef"
            | "ifndef"
            | "elif"
            | "else"
            | "endif"
            | "define"
            | "undef"
            | "include"
            | "error"
            | "warning"
            | "pragma"
            | "line"
    )
}

/// Why an entry of an id file is not used.
enum Skip<'a> {
    Tag,
    Namespace(&'a str),
    Bad(&'static str),
}

/// Entries not used, summarised for the report: tags and other namespaces
/// are counted (packs list hundreds of mod blocks), malformed entries listed.
#[derive(Default)]
struct Skipped {
    tags: usize,
    namespaces: BTreeMap<String, usize>,
}

impl Skipped {
    fn add(&mut self, file: &str, token: &str, skip: Skip, notes: &mut Vec<String>) {
        match skip {
            Skip::Tag => self.tags += 1,
            Skip::Namespace(ns) => *self.namespaces.entry(ns.to_string()).or_default() += 1,
            Skip::Bad(why) => notes.push(format!("{file}: {token}: {why}")),
        }
    }

    fn finish(self, file: &str, notes: &mut Vec<String>) {
        if self.tags > 0 {
            notes.push(format!("{file}: {} tags ignored", self.tags));
        }
        if !self.namespaces.is_empty() {
            let total: usize = self.namespaces.values().sum();
            let names: Vec<&str> = self.namespaces.keys().map(String::as_str).collect();
            notes.push(format!(
                "{file}: {total} entries of other namespaces ignored ({})",
                names.join(", ")
            ));
        }
    }
}

/// `[minecraft:]name[:prop=v1,v2]…` → the bare name and its property
/// filters. Tags (`#…`, `%…`) and other namespaces are refused.
fn parse_block_token(token: &str) -> Result<(&str, Vec<(String, Vec<String>)>), Skip<'_>> {
    if token.starts_with(['#', '%']) {
        return Err(Skip::Tag);
    }
    let mut parts = token.split(':').peekable();
    let first = parts.next().unwrap_or("");
    let name = match parts.peek() {
        Some(second) if !second.contains('=') => {
            let second = parts.next().unwrap_or("");
            if first != "minecraft" {
                return Err(Skip::Namespace(first));
            }
            second
        }
        _ => first,
    };
    if name.is_empty() || name.contains('=') {
        return Err(Skip::Bad("no block name"));
    }
    let mut properties = Vec::new();
    for part in parts {
        let Some((p, values)) = part.split_once('=') else {
            return Err(Skip::Bad("a property without a value"));
        };
        let values: Vec<String> = values
            .split(',')
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .collect();
        if p.is_empty() || values.is_empty() {
            return Err(Skip::Bad("an empty property"));
        }
        properties.push((p.to_string(), values));
    }
    Ok((name, properties))
}

/// `entity.properties` or `item.properties`: names → `entityId` /
/// `currentRenderedItemId` ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdMap {
    /// Bare name (`zombie`) → id; the first id listing a name wins.
    pub by_name: BTreeMap<String, i32>,
    /// The id of a name the file does not list, or of every name when the
    /// pack has no file.
    pub unlisted: i32,
    /// Entries not used (tags, other namespaces, names listed twice).
    pub notes: Vec<String>,
}

impl Default for IdMap {
    /// No file: every id reads -1 (observed; format notes §9).
    fn default() -> Self {
        Self {
            by_name: BTreeMap::new(),
            unlisted: -1,
            notes: Vec::new(),
        }
    }
}

impl IdMap {
    /// The id of `name` (`zombie` or `minecraft:zombie`).
    pub fn id(&self, name: &str) -> i32 {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        self.by_name.get(name).copied().unwrap_or(self.unlisted)
    }
}

/// The documented id of an entity a present `entity.properties` does not
/// list.
pub const UNLISTED_ENTITY: i32 = 65535;
/// The documented id of an item a present `item.properties` does not list.
pub const UNLISTED_ITEM: i32 = -1;

/// Reads `shaders/<file>` (`entity.properties` with keys `entity.<id>`, or
/// `item.properties` with `item.<id>`), preprocessed with `macros`. No
/// file: [`IdMap::default`]; with one, unlisted names read `unlisted`.
pub fn id_map(
    files: &PackFiles,
    macros: &BTreeMap<String, String>,
    kind: &str,
    unlisted: i32,
) -> Result<IdMap, PackError> {
    let path = format!("shaders/{kind}.properties");
    let Some(text) = files.text(&path) else {
        return Ok(IdMap::default());
    };
    let text = preprocess_properties(&text, &path, files, macros)?;
    let mut map = IdMap {
        unlisted,
        ..IdMap::default()
    };
    let prefix = format!("{kind}.");
    let file = format!("{kind}.properties");
    let mut skipped = Skipped::default();
    for (key, value) in parse_properties(&text) {
        let Some(id) = key.strip_prefix(&prefix) else {
            continue;
        };
        let Ok(id) = id.parse::<i32>() else {
            map.notes
                .push(format!("{kind}.properties: {key} is not a number"));
            continue;
        };
        for token in value.split_whitespace() {
            let name = match parse_block_token(token) {
                Ok((name, props)) if props.is_empty() => name,
                Ok(_) => {
                    map.notes
                        .push(format!("{kind}.properties: {token}: properties"));
                    continue;
                }
                Err(skip) => {
                    skipped.add(&file, token, skip, &mut map.notes);
                    continue;
                }
            };
            if let Some(&first) = map.by_name.get(name) {
                if first == id {
                    continue;
                }
                map.notes.push(format!(
                    "{kind}.properties: {token} is listed again under {key}"
                ));
                continue;
            }
            map.by_name.insert(name.to_string(), id);
        }
    }
    skipped.finish(&file, &mut map.notes);
    Ok(map)
}

/// The macros `block.properties`, `entity.properties` and `item.properties`
/// are read with: the predefined ones and the options' at the values
/// `options` sets.
pub fn option_macros(files: &PackFiles, options: &crate::PackOptions) -> BTreeMap<String, String> {
    let mut macros = preprocess::predefined(options.mc_version);
    let files = crate::options::apply(files, &options.overrides);
    macros.extend(crate::options::macros(&crate::options::discover(&files)));
    macros
}

/// `entity.properties` at `options`' values (see [`id_map`]); a file that
/// fails to preprocess reads as none.
pub fn entity_ids(files: &PackFiles, options: &crate::PackOptions) -> IdMap {
    id_map(
        files,
        &option_macros(files, options),
        "entity",
        UNLISTED_ENTITY,
    )
    .unwrap_or_default()
}

/// `item.properties` at `options`' values (see [`id_map`]).
pub fn item_ids(files: &PackFiles, options: &crate::PackOptions) -> IdMap {
    id_map(files, &option_macros(files, options), "item", UNLISTED_ITEM).unwrap_or_default()
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
    fn block_states_most_specific_first_on_ties() {
        let text = "block.1=oak_log\nblock.2=minecraft:oak_log:axis=y\n\
                    block.3=oak_log:axis=x,z:stripped=true oak_log:axis=x\n\
                    block.4=oak_log:axis=y\nblock.5=#minecraft:logs %leaves mod:log other:thing:a=b\n\
                    block.6=:axis=y stone:axis=\nblock.70000=far_block\nblock.-2=neg\n";
        let files = pack(&[("shaders/block.properties", text)]);
        let ids = block_ids(&files, &BTreeMap::new()).unwrap();
        let s = |b: &str, st: &[(&str, &str)]| ids.state_id(b, st);
        assert_eq!(s("oak_log", &[("axis", "y")]), 2);
        assert_eq!(s("minecraft:oak_log", &[("axis", "z")]), 1);
        assert_eq!(s("oak_log", &[("axis", "x")]), 3);
        assert_eq!(s("oak_log", &[("axis", "z"), ("stripped", "true")]), 3);
        // No state: only the bare entry matches.
        assert_eq!(s("oak_log", &[]), 1);
        assert_eq!(s("far_block", &[]), 70000);
        assert_eq!(s("neg", &[]), -2);
        assert_eq!([s("mod:log", &[]), s("stone", &[]), s("air", &[])], [0; 3]);
        let notes = ids.notes.join("\n");
        for needle in [
            "oak_log:axis=y is listed again",
            "2 tags ignored",
            "2 entries of other namespaces ignored (mod, other)",
            ":axis=y: no block name",
            "stone:axis=: an empty property",
        ] {
            assert!(notes.contains(needle), "{needle} in {notes}");
        }
        // The material path still reads the first id of a name.
        assert_eq!(ids.id("oak_log"), 1);
        // The legacy table reads the same through state_id.
        assert_eq!(
            BlockIds::legacy().state_id("oak_leaves", &[("distance", "1")]),
            18
        );
    }

    #[test]
    fn block_properties_see_options() {
        let text = "#ifdef FANCY_LEAVES\nblock.10=oak_leaves\n#else\nblock.11=oak_leaves\n#endif\n\
                    #define PLANTS short_grass fern\nblock.12=PLANTS\n";
        let files = pack(&[
            ("shaders/block.properties", text),
            (
                "shaders/lib/settings.glsl",
                "//#define FANCY_LEAVES\n#ifdef FANCY_LEAVES\n#endif\n",
            ),
        ]);
        let ids = |overrides: Vec<(String, Option<String>)>| {
            let options = crate::PackOptions {
                overrides,
                ..crate::PackOptions::default()
            };
            block_ids(&files, &option_macros(&files, &options)).unwrap()
        };
        let off = ids(Vec::new());
        assert_eq!(off.state_id("oak_leaves", &[]), 11);
        assert_eq!(off.state_id("fern", &[]), 12);
        let on = ids(vec![("FANCY_LEAVES".into(), Some("true".into()))]);
        assert_eq!(on.state_id("oak_leaves", &[]), 10);
    }

    #[test]
    fn conditionals_act_on_physical_lines() {
        // The line before `#else` ends with a continuation, as packs write.
        let text = "# Plants\n#block.99=commented\n#ifdef OFF\nblock.1=a \\\n b \\\n#else\n\
                    block.2=c \\\n  d:age=1,2\n#endif\nblock.3=e\n";
        let files = pack(&[("shaders/block.properties", text)]);
        let ids = block_ids(&files, &BTreeMap::new()).unwrap();
        let s = |b: &str, st: &[(&str, &str)]| ids.state_id(b, st);
        assert_eq!(
            [
                s("a", &[]),
                s("b", &[]),
                s("c", &[]),
                s("d", &[("age", "2")]),
                s("e", &[])
            ],
            [0, 0, 2, 2, 3]
        );
        assert_eq!(s("commented", &[]), 0);
        assert!(ids.notes.is_empty(), "{:?}", ids.notes);
    }

    #[test]
    fn entity_and_item_ids() {
        let entities = "entity.10=minecraft:zombie pig\nentity.11=zombie #minecraft:skeletons mod:x \\\n  cow\n\
                        #if MC_VERSION > 1\nentity.12=sheep\n#endif\n";
        let files = pack(&[
            ("shaders/entity.properties", entities),
            (
                "shaders/item.properties",
                "item.5=diamond_sword minecraft:bow\n",
            ),
        ]);
        let options = crate::PackOptions::default();
        let e = entity_ids(&files, &options);
        assert_eq!(
            [
                e.id("zombie"),
                e.id("minecraft:pig"),
                e.id("cow"),
                e.id("sheep")
            ],
            [10, 10, 11, 12]
        );
        assert_eq!(e.id("creeper"), UNLISTED_ENTITY);
        assert_eq!(e.notes.len(), 3, "{:?}", e.notes);
        assert!(e.notes[2].contains("1 entries of other namespaces ignored (mod)"));
        let i = item_ids(&files, &options);
        assert_eq!([i.id("bow"), i.id("stick")], [5, UNLISTED_ITEM]);
        let none = entity_ids(&PackFiles::default(), &options);
        assert_eq!(
            (
                none.id("zombie"),
                item_ids(&PackFiles::default(), &options).id("bow")
            ),
            (-1, -1)
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
