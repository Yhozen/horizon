//! Horizon Open Creative's resource bundle (docs/open-creative.md).
//!
//! [`build`] reads only `content/open/` (the vendored open artwork, its
//! notices and `sources.json`) and the repository's openly licensed fonts,
//! and generates everything else here: block states and their facts, models,
//! item definitions, the creative palette, translations, interface artwork and
//! the environment. Nothing is copied from prepared Minecraft resources.
//!
//! Every output file is either an attributed open input (named with its
//! source in `horizon-open-provenance.json`) or the output of a named
//! generator in this crate; [`write`] refuses anything else, and
//! [`verify`] checks a written bundle against its provenance record.
pub mod art;
pub mod art_survival;
pub mod assets;
pub mod blocks;
pub mod coverage;
pub mod cubes;
pub mod image;
pub mod items;
pub mod lang;
pub mod mobs;
pub mod network;
pub mod numbering;
pub mod screens;
pub mod sources;
pub mod tables;

use horizon_core::content_profile::{
    ContentProfile, MANIFEST_KEY, OPEN_CONTENT_REVISION, REVISION_KEY,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, String>;

/// The provenance record's name in the bundle.
pub const PROVENANCE: &str = "horizon-open-provenance.json";
/// The manifest every resource folder has.
pub const MANIFEST: &str = "horizon-resources.json";

/// Where one output file came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// An open input, by its path under `content/open/` (or the repository,
    /// for fonts), possibly transformed as `sources.json` records.
    Input(String),
    /// Written by this crate's generator of that name.
    Generator(&'static str),
}

/// The bundle in memory: relative path to bytes, with each file's origin.
#[derive(Default)]
pub struct Bundle {
    files: BTreeMap<String, (Vec<u8>, Origin)>,
}

impl Bundle {
    pub fn add(&mut self, path: impl Into<String>, bytes: Vec<u8>, origin: Origin) -> Result<()> {
        let path = path.into();
        if path.starts_with('/') || path.split('/').any(|p| p == ".." || p.is_empty()) {
            return Err(format!("invalid bundle path {path}"));
        }
        if self.files.insert(path.clone(), (bytes, origin)).is_some() {
            return Err(format!("{path} is generated twice"));
        }
        Ok(())
    }
    pub fn json(
        &mut self,
        path: impl Into<String>,
        value: &Value,
        generator: &'static str,
    ) -> Result<()> {
        let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        self.add(path, bytes, Origin::Generator(generator))
    }
    /// [`Self::json`] without the indentation, for the tables with a row per
    /// block state or registry name.
    pub fn compact_json(
        &mut self,
        path: impl Into<String>,
        value: &Value,
        generator: &'static str,
    ) -> Result<()> {
        let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        self.add(path, bytes, Origin::Generator(generator))
    }
    pub fn text(
        &mut self,
        path: impl Into<String>,
        text: &str,
        generator: &'static str,
    ) -> Result<()> {
        self.add(path, text.as_bytes().to_vec(), Origin::Generator(generator))
    }
    pub fn png(
        &mut self,
        path: impl Into<String>,
        image: &image::Rgba,
        origin: Origin,
    ) -> Result<()> {
        self.add(path, image.encode(), origin)
    }
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(|(bytes, _)| bytes.as_slice())
    }
    pub fn contains(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
    pub fn len(&self) -> usize {
        self.files.len()
    }
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// The repository root's open inputs.
pub struct Inputs {
    /// The repository root (fonts are read from `crates/horizon-theme`).
    pub repo: PathBuf,
    /// `content/open`.
    pub content: PathBuf,
}
impl Inputs {
    pub fn at(repo: &Path) -> Self {
        Self {
            repo: repo.to_path_buf(),
            content: repo.join("content").join("open"),
        }
    }
}

/// Build the whole bundle from the open inputs.
pub fn build(inputs: &Inputs) -> Result<Bundle> {
    let sources = sources::Sources::load(inputs)?;
    let blocks = blocks::all();
    let items = items::all();
    let mut bundle = Bundle::default();
    tables::write(&mut bundle, &blocks)?;
    tables::environment(&mut bundle)?;
    tables::attributes(&mut bundle)?;
    tables::health(&mut bundle, &blocks)?;
    assets::write(&mut bundle, &blocks, &items, &sources)?;
    items::write(&mut bundle, &blocks, &items)?;
    art::write(&mut bundle)?;
    network::write(&mut bundle)?;
    coverage::write(&mut bundle, &blocks)?;
    sources.write_notices(&mut bundle)?;
    bundle.json(
        MANIFEST,
        &json!({
            MANIFEST_KEY: ContentProfile::OpenCreative.id(),
            REVISION_KEY: OPEN_CONTENT_REVISION,
            "schema": 1,
            "title": ContentProfile::OpenCreative.title(),
            "generator": concat!("horizon-open-content ", env!("CARGO_PKG_VERSION")),
            "features": {
                "available": [
                    "creative", "survival", "single_player", "multiplayer", "overworld",
                    "first_person", "third_person", "crafting", "tools", "food"
                ],
                "unavailable": [
                    "nether", "end", "mobs", "sound", "saving_in_browser", "structures",
                    "weather", "smelting", "fluid_flow", "random_ticks"
                ],
            },
            "starter_hotbar": [
                "minecraft:grass_block", "minecraft:stone", "minecraft:cobblestone",
                "minecraft:oak_planks", "minecraft:oak_log", "minecraft:crafting_table",
                "minecraft:glass", "minecraft:glowstone", "minecraft:red_wool"
            ],
            "blocks": blocks.iter().filter(|b| b.tier != blocks::Tier::Support).count(),
            "items": items.len() + blocks.iter().filter(|b| b.has_item()).count(),
            "recipes": items::recipes(&blocks).len(),
            "support_blocks": blocks.iter().filter(|b| b.tier == blocks::Tier::Support).count(),
        }),
        "manifest",
    )?;
    validate(&bundle, &blocks)?;
    let provenance = provenance(&bundle, &sources)?;
    bundle.json(PROVENANCE, &provenance, "provenance")?;
    Ok(bundle)
}

/// Every reference the bundle makes resolves inside it.
fn validate(bundle: &Bundle, blocks: &[blocks::Block]) -> Result<()> {
    for block in blocks {
        if block.tier == blocks::Tier::Support {
            continue;
        }
        if !block.is_air() && block.form != blocks::Form::Fluid {
            let path = format!("assets/minecraft/blockstates/{}.json", block.id);
            if !bundle.contains(&path) {
                return Err(format!("{} has no block state definition", block.id));
            }
            // Every state picks a variant, as Minecraft matches them: a
            // variant's `key=value` pairs all hold in the state ("" holds
            // in every state).
            let definition: Value = serde_json::from_slice(bundle.get(&path).unwrap())
                .map_err(|e| format!("{path}: {e}"))?;
            let variants = definition["variants"]
                .as_object()
                .ok_or_else(|| format!("{path} has no variants"))?;
            let predicates: Vec<Vec<(&str, &str)>> = variants
                .keys()
                .map(|key| {
                    key.split(',')
                        .filter(|p| !p.is_empty())
                        .map(|p| p.split_once('=').unwrap_or((p, "")))
                        .collect()
                })
                .collect();
            for state in block.states() {
                let matched = predicates.iter().any(|predicate| {
                    predicate
                        .iter()
                        .all(|(k, v)| state.iter().any(|(sk, sv)| sk == k && sv == v))
                });
                if !matched {
                    let key: Vec<String> = state.iter().map(|(k, v)| format!("{k}={v}")).collect();
                    return Err(format!(
                        "{path} has no variant for state [{}]",
                        key.join(",")
                    ));
                }
            }
        }
        for texture in block.textures() {
            let path = format!("assets/minecraft/textures/block/{texture}.png");
            if !bundle.contains(&path) {
                return Err(format!("{} needs the missing texture {path}", block.id));
            }
        }
        if block.has_item()
            && !bundle.contains(&format!("assets/minecraft/items/{}.json", block.id))
        {
            return Err(format!("{} has no item definition", block.id));
        }
    }
    // Models name only textures and parents that exist.
    for path in bundle
        .paths()
        .filter(|p| p.starts_with("assets/minecraft/models/"))
    {
        let model: Value = serde_json::from_slice(bundle.get(path).unwrap())
            .map_err(|e| format!("{path}: {e}"))?;
        if let Some(parent) = model["parent"]
            .as_str()
            .filter(|p| *p != "minecraft:builtin/generated")
        {
            let file = resource_path(parent, "models", "json");
            if !bundle.contains(&file) {
                return Err(format!("{path}: missing parent {parent}"));
            }
        }
        for texture in model["textures"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(_, v)| v.as_str())
        {
            if texture.starts_with('#') {
                continue;
            }
            let file = resource_path(texture, "textures", "png");
            if !bundle.contains(&file) {
                return Err(format!("{path}: missing texture {texture}"));
            }
        }
    }
    for path in bundle.paths().filter(|p| {
        p.starts_with("assets/minecraft/blockstates/") || p.starts_with("assets/minecraft/items/")
    }) {
        let text = std::str::from_utf8(bundle.get(path).unwrap()).map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_str(text).map_err(|e| format!("{path}: {e}"))?;
        let mut models = Vec::new();
        collect_models(&value, &mut models);
        for model in models {
            let file = resource_path(&model, "models", "json");
            if !bundle.contains(&file) {
                return Err(format!("{path}: missing model {model}"));
            }
        }
    }
    // Recipes, loot tables and tags name only items and blocks that exist.
    let prototypes: Value =
        serde_json::from_slice(bundle.get("items.json").ok_or("no items.json")?)
            .map_err(|e| e.to_string())?;
    let item_exists = |id: &str| prototypes.get(id).is_some();
    let tag_exists = |kind: &str, id: &str| {
        let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
        bundle.contains(&format!("data/{namespace}/tags/{kind}/{path}.json"))
    };
    for path in bundle.paths().filter(|p| {
        p.starts_with("data/minecraft/recipe/")
            || p.starts_with("data/minecraft/loot_table/")
            || p.starts_with("data/minecraft/tags/item/")
            || p.starts_with("data/minecraft/advancement/")
    }) {
        let value: Value = serde_json::from_slice(bundle.get(path).unwrap())
            .map_err(|e| format!("{path}: {e}"))?;
        let mut names = Vec::new();
        item_references(&value, &mut names);
        for name in names {
            let ok = match name.strip_prefix('#') {
                Some(tag) => tag_exists("item", tag),
                None => item_exists(&name),
            };
            if !ok {
                return Err(format!("{path}: names the missing item {name}"));
            }
        }
        if path.starts_with("data/minecraft/advancement/") {
            for recipe in value["rewards"]["recipes"].as_array().into_iter().flatten() {
                let file = recipe
                    .as_str()
                    .unwrap_or("")
                    .replace("minecraft:", "data/minecraft/recipe/")
                    + ".json";
                if !bundle.contains(&file) {
                    return Err(format!("{path}: rewards the missing recipe {recipe}"));
                }
            }
        }
    }
    let block_names: std::collections::HashSet<String> = blocks.iter().map(|b| b.key()).collect();
    for path in bundle
        .paths()
        .filter(|p| p.starts_with("data/minecraft/tags/block/"))
    {
        let value: Value = serde_json::from_slice(bundle.get(path).unwrap())
            .map_err(|e| format!("{path}: {e}"))?;
        for v in value["values"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let known = match v.strip_prefix('#') {
                Some(tag) => tag_exists("block", tag),
                None => block_names.contains(v),
            };
            if !known {
                return Err(format!("{path}: names the missing block {v}"));
            }
        }
    }
    // Tool rules name block tags that exist.
    for (id, prototype) in prototypes.as_object().into_iter().flatten() {
        for rule in prototype["minecraft:tool"]["rules"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(tag) = rule["blocks"].as_str().and_then(|b| b.strip_prefix('#')) {
                if !tag_exists("block", tag) {
                    return Err(format!("{id}: its tool rule names the missing tag #{tag}"));
                }
            }
        }
    }
    Ok(())
}

/// The item names in a recipe, loot table, item tag or advancement: recipe
/// keys, ingredients and results, loot entries' names, tag values.
fn item_references(value: &Value, out: &mut Vec<String>) {
    let mut push = |v: &Value| match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(list) => out.extend(list.iter().filter_map(Value::as_str).map(str::to_owned)),
        _ => {}
    };
    if value["type"]
        .as_str()
        .is_some_and(|t| t.starts_with("minecraft:crafting"))
    {
        for v in value["key"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(_, v)| v)
        {
            push(v);
        }
        for v in value["ingredients"].as_array().into_iter().flatten() {
            push(v);
        }
        push(&value["result"]["id"]);
        return;
    }
    if value["type"] == "minecraft:block" {
        fn entries(v: &Value, out: &mut Vec<String>) {
            match v {
                Value::Object(map) => {
                    if map.get("type").and_then(Value::as_str) == Some("minecraft:item") {
                        if let Some(name) = map.get("name").and_then(Value::as_str) {
                            out.push(name.to_owned());
                        }
                    }
                    map.values().for_each(|v| entries(v, out));
                }
                Value::Array(list) => list.iter().for_each(|v| entries(v, out)),
                _ => {}
            }
        }
        entries(value, out);
        return;
    }
    push(&value["values"]);
}

fn collect_models(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if key == "model" {
                    if let Some(name) = value.as_str() {
                        out.push(name.to_owned());
                        continue;
                    }
                }
                collect_models(value, out);
            }
        }
        Value::Array(list) => list.iter().for_each(|v| collect_models(v, out)),
        _ => {}
    }
}

/// `minecraft:block/stone` → `assets/minecraft/<kind>/block/stone.<ext>`.
pub fn resource_path(id: &str, kind: &str, ext: &str) -> String {
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
    format!("assets/{namespace}/{kind}/{path}.{ext}")
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn provenance(bundle: &Bundle, sources: &sources::Sources) -> Result<Value> {
    let mut files = serde_json::Map::new();
    for (path, (bytes, origin)) in &bundle.files {
        let entry = match origin {
            Origin::Input(input) => {
                let source = sources.describe(input).ok_or_else(|| {
                    format!("{path} comes from {input}, which sources.json does not attribute")
                })?;
                json!({"sha256": sha256_hex(bytes), "input": input, "source": source})
            }
            Origin::Generator(name) => json!({"sha256": sha256_hex(bytes), "generator": name}),
        };
        files.insert(path.clone(), entry);
    }
    Ok(json!({
        "schema": 1,
        MANIFEST_KEY: ContentProfile::OpenCreative.id(),
        REVISION_KEY: OPEN_CONTENT_REVISION,
        "generators": generators(),
        "files": files,
    }))
}

/// Each generator's description, for the provenance record.
fn generators() -> Value {
    json!({
        "manifest": "horizon-open-content: the bundle manifest",
        "provenance": "horizon-open-content: this record",
        "tables": "horizon-open-content::tables: per-state block tables from the block definitions in blocks.rs",
        "assets": "horizon-open-content::assets: block states, models, items, palette and translations from blocks.rs",
        "art": "horizon-open-content::art and art_survival: original procedural artwork (interface, status bars, screens, bedrock, crafting table, sky, title, player arm)",
        "notices": "horizon-open-content::sources: the credits and notices assembled from sources.json",
        "environment": "horizon-open-content::tables: the biome, dimension and day cycle",
        "network": "horizon-open-content::network: the registries an open server and its players share, numbered from the name lists in data/",
        "coverage": "horizon-open-content::coverage: which registered names have content in this bundle",
        "items": "horizon-open-content::items: recipes, item tags and the recipe advancement from items.rs",
    })
}

/// Write `bundle` to `out`, which must be empty, missing, or an earlier open
/// bundle (whose files are replaced). Returns the number of files.
pub fn write(bundle: &Bundle, out: &Path) -> Result<usize> {
    if out.exists() {
        let mut entries = fs::read_dir(out).map_err(|e| format!("{}: {e}", out.display()))?;
        if entries.next().is_some() {
            let manifest = fs::read(out.join(MANIFEST))
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
            let open = manifest
                .as_ref()
                .and_then(|m| m[MANIFEST_KEY].as_str())
                .is_some_and(|p| p == ContentProfile::OpenCreative.id());
            if !open {
                return Err(format!(
                    "{} is not empty and holds no open bundle; choose an empty folder",
                    out.display()
                ));
            }
            fs::remove_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
        }
    }
    for (path, (bytes, _)) in &bundle.files {
        let target = out.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::write(&target, bytes).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    Ok(bundle.files.len())
}

/// Check a written bundle: an open-creative manifest, and exactly the files
/// its provenance record lists, each with its recorded checksum.
pub fn verify(dir: &Path) -> Result<usize> {
    let read = |p: &Path| fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
    let manifest: Value =
        serde_json::from_slice(&read(&dir.join(MANIFEST))?).map_err(|e| e.to_string())?;
    if manifest[MANIFEST_KEY].as_str() != Some(ContentProfile::OpenCreative.id()) {
        return Err(format!("{} is not an open-creative bundle", dir.display()));
    }
    if manifest[REVISION_KEY].as_i64() != Some(OPEN_CONTENT_REVISION) {
        return Err("the bundle's content revision differs from this build's".into());
    }
    let record: Value =
        serde_json::from_slice(&read(&dir.join(PROVENANCE))?).map_err(|e| e.to_string())?;
    let files = record["files"]
        .as_object()
        .ok_or("the provenance record lists no files")?;
    let mut found = Vec::new();
    walk(dir, dir, &mut found)?;
    for path in &found {
        if path == PROVENANCE {
            continue;
        }
        let entry = files
            .get(path)
            .ok_or_else(|| format!("{path} is in the bundle but has no provenance"))?;
        if entry.get("input").is_none() && entry.get("generator").is_none() {
            return Err(format!("{path} has neither an input nor a generator"));
        }
        if entry["sha256"].as_str() != Some(&sha256_hex(&read(&dir.join(path))?)) {
            return Err(format!("{path} differs from its provenance checksum"));
        }
    }
    for path in files.keys() {
        if !found.iter().any(|f| f == path) {
            return Err(format!("{path} is recorded but missing from the bundle"));
        }
    }
    Ok(found.len())
}

/// The browser's data archive of the verified bundle at `dir`: [`records`]
/// compressed with gzip (the page inflates it before the game mounts it,
/// `inflateArchive` in web/cache.js). The per-state tables repeat almost
/// every row, so it is a few percent of the bundle's size.
pub fn archive(dir: &Path) -> Result<Vec<u8>> {
    use std::io::Write as _;
    let records = records(dir)?;
    let mut encoder = flate2::GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), flate2::Compression::best());
    encoder.write_all(&records).map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())
}

/// The verified bundle at `dir` as one stream of records (WEB.md's format:
/// per file a little-endian u32 path length, the path, a u32 data length and
/// the data), files in path order.
pub fn records(dir: &Path) -> Result<Vec<u8>> {
    verify(dir)?;
    let mut files = Vec::new();
    walk(dir, dir, &mut files)?;
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let data = fs::read(dir.join(&path)).map_err(|e| format!("{path}: {e}"))?;
        let len = |n: usize| u32::try_from(n).map_err(|_| format!("{path} is too large"));
        out.extend_from_slice(&len(path.len())?.to_le_bytes());
        out.extend_from_slice(path.as_bytes());
        out.extend_from_slice(&len(data.len())?.to_le_bytes());
        out.extend_from_slice(&data);
    }
    Ok(out)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .collect::<std::io::Result<_>>()
        .map_err(|e| e.to_string())?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out)?;
        } else {
            let relative = path.strip_prefix(root).unwrap();
            out.push(
                relative
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn file(bundle: &Bundle, path: &str) -> Value {
        serde_json::from_slice(bundle.get(path).unwrap()).unwrap()
    }

    /// Every name the bundle defines has Minecraft's id, and every state of
    /// every block Minecraft's state id (from the committed lists in data/).
    #[test]
    fn the_bundle_numbers_everything_as_minecraft_does() {
        let bundle = build(&Inputs::at(&repo())).unwrap();
        let registries = file(&bundle, "registries.json");
        let id = |registry: &str, name: &str| {
            registries[format!("minecraft:{registry}")]["entries"][name]["protocol_id"].as_u64()
        };
        let expect = |registry: &str, name: &str| {
            let short = name.strip_prefix("minecraft:").unwrap_or(name);
            let want = numbering::names(registry).iter().position(|n| *n == short);
            assert!(want.is_some(), "{name} is not a Minecraft {registry}");
            assert_eq!(
                id(registry, name),
                want.map(|i| i as u64),
                "{registry} {name}"
            );
        };
        for registry in numbering::registries() {
            let entries = registries[format!("minecraft:{registry}")]["entries"]
                .as_object()
                .unwrap();
            assert_eq!(
                entries.len(),
                numbering::names(registry).len(),
                "{registry}"
            );
        }
        for block in blocks::all() {
            expect("block", &block.key());
        }
        for name in file(&bundle, "items.json").as_object().unwrap().keys() {
            expect("item", name);
        }
        for name in network::ENTITY_TYPES.iter().copied().chain(mobs::names()) {
            expect("entity_type", &format!("minecraft:{name}"));
        }
        for (i, attribute) in file(&bundle, "attributes.json")["attributes"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let name = attribute["id"].as_str().unwrap();
            assert_eq!(id("attribute", name), Some(i as u64), "{name}");
        }
        assert_eq!(
            file(&bundle, "attributes.json")["attributes"]
                .as_array()
                .unwrap()
                .len(),
            40
        );

        // States: the product of each block's properties (keys sorted, last
        // fastest), blocks in registry order; defaults where Minecraft's are.
        let facts = file(&bundle, "facts.json");
        let rows = facts["states"].as_array().unwrap();
        let mut at = 0usize;
        for block in numbering::blocks() {
            let first = at;
            let mut state = vec![0usize; block.properties.len()];
            for _ in 0..numbering::state_count(block.properties) {
                let row = &rows[at][4];
                let name = format!("minecraft:{}", block.name);
                let got_name = row.as_str().or(row["id"].as_str()).unwrap();
                assert_eq!(got_name, name, "state {at}");
                for (k, (key, values)) in block.properties.iter().enumerate() {
                    assert_eq!(
                        row["properties"][*key].as_str(),
                        Some(values[state[k]]),
                        "state {at} of {name}"
                    );
                }
                assert_eq!(
                    row.get("properties")
                        .and_then(Value::as_object)
                        .map_or(0, |p| p.len()),
                    block.properties.len()
                );
                // The next state: the last property varies fastest.
                for k in (0..state.len()).rev() {
                    state[k] += 1;
                    if state[k] < block.properties[k].1.len() {
                        break;
                    }
                    state[k] = 0;
                }
                at += 1;
            }
            let default = block.properties.iter().fold(0, |index, (key, values)| {
                let value = block.defaults.iter().find(|(k, _)| k == key).unwrap().1;
                index * values.len() + values.iter().position(|v| *v == value).unwrap()
            });
            assert_eq!(
                facts["defaults"][format!("minecraft:{}", block.name)].as_u64(),
                Some((first + default) as u64),
                "{}",
                block.name
            );
        }
        assert_eq!(at, 35_723);
        assert_eq!(rows.len(), at, "states beyond Minecraft's");

        // Coverage: content is a subset of each registry, and every block
        // defined here has content.
        let coverage = file(&bundle, coverage::PATH);
        let block = &coverage["registries"]["minecraft:block"];
        let listed: Vec<&str> = block["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for b in blocks::defined() {
            assert!(listed.contains(&b.key().as_str()), "{}", b.id);
        }
        assert!(!listed.contains(&"minecraft:lava"));
        for (name, entry) in coverage["registries"].as_object().unwrap() {
            let one = entry.get("content").is_some() != entry.get("missing").is_some();
            assert!(one, "{name} lists exactly one of content and missing");
        }
    }

    #[test]
    fn the_bundle_builds_twice_to_the_same_bytes_and_verifies() {
        let a = build(&Inputs::at(&repo())).unwrap();
        let b = build(&Inputs::at(&repo())).unwrap();
        assert_eq!(a.files, b.files);
        let out = std::env::temp_dir().join(format!("horizon-open-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&out);
        write(&a, &out).unwrap();
        assert_eq!(verify(&out).unwrap(), a.len());
        let compressed = archive(&out).unwrap();
        assert_eq!(compressed, archive(&out).unwrap());
        // A gzip stream of the records.
        assert_eq!(compressed[..2], [0x1f, 0x8b]);
        let mut packed = Vec::new();
        std::io::Read::read_to_end(
            &mut flate2::read::GzDecoder::new(compressed.as_slice()),
            &mut packed,
        )
        .unwrap();
        assert_eq!(packed, records(&out).unwrap());
        // Every file is one record, in path order.
        let (mut at, mut paths) = (0usize, Vec::new());
        while at < packed.len() {
            let n = u32::from_le_bytes(packed[at..at + 4].try_into().unwrap()) as usize;
            paths.push(
                std::str::from_utf8(&packed[at + 4..at + 4 + n])
                    .unwrap()
                    .to_owned(),
            );
            at += 4 + n;
            let d = u32::from_le_bytes(packed[at..at + 4].try_into().unwrap()) as usize;
            at += 4 + d;
        }
        assert_eq!(paths.len(), a.len());
        assert!(paths.windows(2).all(|w| w[0] < w[1]));
        // A stray file, or a changed one, fails verification.
        fs::write(out.join("stray.txt"), b"x").unwrap();
        assert!(verify(&out).unwrap_err().contains("stray.txt"));
        fs::remove_file(out.join("stray.txt")).unwrap();
        fs::write(out.join("facts.json"), b"{}").unwrap();
        assert!(verify(&out).unwrap_err().contains("facts.json"));
        // A folder that is not an open bundle is never replaced.
        let other = out.with_extension("other");
        let _ = fs::remove_dir_all(&other);
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("keep.txt"), b"x").unwrap();
        assert!(write(&a, &other).is_err());
        assert!(other.join("keep.txt").exists());
        let _ = fs::remove_dir_all(&out);
        let _ = fs::remove_dir_all(&other);
    }
}
