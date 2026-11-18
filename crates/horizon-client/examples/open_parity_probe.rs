//! The tables a resource folder gives the game, keyed by name, for the
//! `open_tables` harness (tools/test_open_tables_parity.py): it runs this on
//! the vanilla resources and on the Horizon Open bundle and compares them name
//! by name.
//!
//!   open_parity_probe ROOT OUT.json     a prepared resource folder
//!   open_parity_probe --open OUT.json   the open bundle, built here from the generator
//!
//! Every table goes through the loaders the game uses. Each category is a map
//! from a name to its facts; numbers (registry ids, global state ids) are facts
//! of a name, never keys, so two folders compare however they number things.
use horizon_client::{assets::Assets, blocks::BlockFacts, items::Items};
use horizon_core::geometry::{Aabb, VoxelShape};
use horizon_protocol::registry::Builtin;
use horizon_world::{
    attributes::entity::Catalog,
    collision::CachedCollisionRegistry,
    registry::{BlockRegistry, BlockStateId, BlockTableFile},
};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Instant,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Table = BTreeMap<String, Value>;

const USAGE: &str = "usage: open_parity_probe ROOT OUT.json | --open OUT.json";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root, out] = args.as_slice() else {
        return Err(USAGE.into());
    };
    let started = Instant::now();
    let (root, temporary) = if root == "--open" {
        (build_open()?, true)
    } else {
        (PathBuf::from(root), false)
    };
    let built = started.elapsed().as_secs_f64();
    let tables = dump(&root);
    if temporary {
        let _ = std::fs::remove_dir_all(&root);
    }
    let mut doc = Map::new();
    for (category, table) in tables? {
        doc.insert(category.to_owned(), json!(table));
    }
    doc.insert(
        "_seconds".into(),
        json!({ "build": built, "total": started.elapsed().as_secs_f64() }),
    );
    std::fs::write(out, serde_json::to_vec(&Value::Object(doc))?)?;
    Ok(())
}

/// The open bundle as `horizon-prepare open` writes it, from this checkout's
/// generator and content (so the parity runner traces the generator sources).
fn build_open() -> Result<PathBuf> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = std::env::temp_dir().join(format!("horizon-open-parity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let bundle = horizon_open_content::build(&horizon_open_content::Inputs::at(&repo))?;
    horizon_open_content::write(&bundle, &out)?;
    Ok(out)
}

fn dump(root: &Path) -> Result<Vec<(&'static str, Table)>> {
    let assets = Assets::load(root)?;
    let registry = assets.registry;
    let facts = BlockFacts::load(root, registry)?;
    let (_, flags) = BlockRegistry::from_facts_json_with_flags(horizon_core::fs::File::open(
        root.join("facts.json"),
    )?)?;
    let (attributes, entity_attributes) = attributes(root)?;
    let (textures, screens) = textures(root)?;
    Ok(vec![
        ("registries", registries(root)?),
        ("blocks", blocks(registry)),
        ("states", states(registry, &facts, &flags)),
        ("shapes", shapes(root, registry, &facts)?),
        ("items", items(root, registry, &facts)?),
        ("entity_types", entity_types(root)?),
        ("model_layers", model_layers(root)?),
        ("attributes", attributes),
        ("entity_attributes", entity_attributes),
        ("screens", screens),
        ("textures", textures),
        ("models", models(root, &assets)?),
    ])
}

/// Every built-in registry entry (`registry entry`) with its network id.
fn registries(root: &Path) -> Result<Table> {
    let builtin = Builtin::load(&root.join("registries.json"))?;
    let mut out = Table::new();
    for (registry, ids) in &builtin.registries {
        for (id, name) in ids.names.iter().enumerate() {
            out.insert(format!("{registry} {name}"), json!(id));
        }
    }
    Ok(out)
}

/// `name[k=v,...]` with the keys in the registry's (alphabetical) order.
fn state_key(registry: &BlockRegistry, id: BlockStateId) -> String {
    let state = registry.state(id).expect("state in registry");
    if state.properties.is_empty() {
        return state.name.clone();
    }
    let props: Vec<String> = state
        .properties
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    format!("{}[{}]", state.name, props.join(","))
}

/// Per block: its properties with their values in numbering order (the order
/// each value first appears among the block's states), and its default state.
fn blocks(registry: &BlockRegistry) -> Table {
    let mut props: BTreeMap<&str, Vec<(&str, Vec<&str>)>> = BTreeMap::new();
    for i in 0..registry.len() {
        let state = registry.state(BlockStateId(i as u32)).unwrap();
        let block = props.entry(&state.name).or_default();
        for (k, v) in &state.properties {
            let at = match block.iter().position(|(name, _)| name == k) {
                Some(at) => at,
                None => {
                    block.push((k, Vec::new()));
                    block.len() - 1
                }
            };
            if !block[at].1.contains(&v.as_str()) {
                block[at].1.push(v);
            }
        }
    }
    props
        .into_iter()
        .map(|(name, properties)| {
            let default = registry.default_state(name).expect("default state");
            let properties: Vec<Value> = properties.iter().map(|(k, v)| json!([k, v])).collect();
            (
                name.to_owned(),
                json!({ "properties": properties, "default": state_key(registry, default) }),
            )
        })
        .collect()
}

/// Per state: its global id and the facts the game reads by state.
fn states(registry: &BlockRegistry, facts: &BlockFacts, flags: &[(bool, bool)]) -> Table {
    (0..registry.len())
        .map(|i| {
            let id = BlockStateId(i as u32);
            let f = facts.get(id);
            let (air, fluid) = flags.get(i).copied().unwrap_or_default();
            (
                state_key(registry, id),
                json!({
                    "id": i,
                    "air": air,
                    "fluid": fluid,
                    "fluid_state": format!("{:?} {} {}", f.fluid, f.fluid_level, f.fluid_falling),
                    "emission": f.emission,
                    "opacity": f.dampening,
                    "hardness": f.hardness,
                    "explosion_resistance": f.explosion_resistance,
                    "friction": f.friction,
                    "requires_tool": f.requires_tool,
                    "replaceable": f.replaceable,
                    "block_entity": f.block_entity,
                    "render_shape": format!("{:?}", f.render_shape),
                }),
            )
        })
        .collect()
}

/// Per state: collision (`cached-collision.nbt`), outline (`getShape`) and
/// light occlusion (`occlusion.nbt`) boxes. A state whose shape vanilla
/// computes in code (no cached shape) is `dynamic`.
fn shapes(root: &Path, registry: &BlockRegistry, facts: &BlockFacts) -> Result<Table> {
    let table = |file: &str, name: &str| -> Result<CachedCollisionRegistry<'_>> {
        let file = BlockTableFile::open(&root.join(file), registry)?;
        Ok(CachedCollisionRegistry::from_table(file.table(name)?)?)
    };
    let collision = table("cached-collision.nbt", "collision")?;
    let occlusion = table("occlusion.nbt", "occlusion")?;
    let boxes = |shape: Option<&VoxelShape>| -> Value {
        shape.map_or(json!("dynamic"), |s| {
            json!(s.boxes().iter().map(|b| aabb(*b)).collect::<Vec<_>>())
        })
    };
    let mut out = Table::new();
    for i in 0..registry.len() {
        let id = BlockStateId(i as u32);
        let f = facts.get(id);
        out.insert(
            state_key(registry, id),
            json!({
                "collision": boxes(collision.cached_shape(id)?),
                "outline": f.outline.iter().map(round).collect::<Vec<_>>(),
                "occlusion": boxes(occlusion.cached_shape(id)?),
                "can_occlude": f.can_occlude,
                "solid_render": f.solid_render,
            }),
        );
    }
    Ok(out)
}

fn aabb(b: Aabb) -> Value {
    use horizon_core::geometry::Axis::{X, Y, Z};
    json!(round(&[
        b.min(X),
        b.min(Y),
        b.min(Z),
        b.max(X),
        b.max(Y),
        b.max(Z)
    ]))
}

/// Box corners to 1/1000000 of a block: the same shape written by two
/// exporters can differ in the last bit of a float.
fn round(b: &[f64; 6]) -> [f64; 6] {
    b.map(|v| (v * 1e6).round() / 1e6)
}

/// Per item: its default components, as `items.json` gives them to `Items::load`.
fn items(root: &Path, registry: &BlockRegistry, facts: &BlockFacts) -> Result<Table> {
    let loaded = Items::load(root, registry, facts)?;
    let raw: BTreeMap<String, Value> =
        serde_json::from_slice(&horizon_core::fs::read(root.join("items.json"))?)?;
    Ok(raw
        .into_iter()
        .filter(|(name, _)| loaded.get(name).is_some())
        .collect())
}

/// Per entity type: its dimensions (`entity-types.json`, as the spawner reads it).
fn entity_types(root: &Path) -> Result<Table> {
    let types: BTreeMap<String, Value> =
        serde_json::from_slice(&horizon_core::fs::read(root.join("entity-types.json"))?)?;
    Ok(types
        .into_iter()
        .map(|(name, t)| {
            let facts = json!({ "width": t["width"], "height": t["height"], "eye_height": t["eye_height"] });
            (name, facts)
        })
        .collect())
}

/// Per entity model layer (`model#layer`): its part names, depth first.
fn model_layers(root: &Path) -> Result<Table> {
    let models: BTreeMap<String, Value> =
        serde_json::from_slice(&horizon_core::fs::read(root.join("entity-models.json"))?)?;
    fn parts(part: &Value, path: &str, out: &mut Vec<String>) {
        if let Some(children) = part["children"].as_object() {
            for (name, child) in children {
                let path = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{path}/{name}")
                };
                out.push(path.clone());
                parts(child, &path, out);
            }
        }
    }
    Ok(models
        .into_iter()
        .map(|(name, model)| {
            let mut names = Vec::new();
            parts(&model, "", &mut names);
            names.sort();
            (name, json!(names))
        })
        .collect())
}

/// The attribute catalog (`Catalog::from_json`): per attribute its default,
/// range and wire id; per entity type and attribute its default base value.
fn attributes(root: &Path) -> Result<(Table, Table)> {
    let raw: Value =
        serde_json::from_slice(&horizon_core::fs::read(root.join("attributes.json"))?)?;
    let catalog = Catalog::from_json(&raw)?;
    let mut attributes = Table::new();
    for (id, a) in catalog.attributes() {
        let d = a.definition;
        attributes.insert(
            a.name.clone(),
            json!({
                "id": id.registry_id(),
                "default": d.default_value(),
                "min": d.sanitize(f64::NEG_INFINITY),
                "max": d.sanitize(f64::INFINITY),
                "syncable": a.syncable,
            }),
        );
    }
    let mut defaults = Table::new();
    for entity in catalog.entities() {
        for (id, a) in catalog.attributes() {
            if let Some(base) = catalog.default_base(entity, id) {
                defaults.insert(format!("{entity} {}", a.name), json!(base));
            }
        }
    }
    Ok((attributes, defaults))
}

/// Every texture under `assets/minecraft/textures` with its pixel size (from
/// the PNG header), and the container screens' textures on their own.
fn textures(root: &Path) -> Result<(Table, Table)> {
    let base = root.join("assets/minecraft/textures");
    let mut textures = Table::new();
    let mut stack = vec![base.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = horizon_core::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "png") {
                let name = path
                    .strip_prefix(&base)?
                    .to_string_lossy()
                    .replace('\\', "/");
                textures.insert(name, json!(png_size(&path)));
            }
        }
    }
    let screens = textures
        .iter()
        .filter(|(name, _)| name.starts_with("gui/container/"))
        .map(|(name, _)| (name.clone(), json!(true)))
        .collect();
    Ok((textures, screens))
}

fn png_size(path: &Path) -> Option<[u32; 2]> {
    let mut header = [0u8; 24];
    std::io::Read::read_exact(&mut horizon_core::fs::File::open(path).ok()?, &mut header).ok()?;
    let word = |at: usize| u32::from_be_bytes(header[at..at + 4].try_into().unwrap());
    (header[..8] == *b"\x89PNG\r\n\x1a\n").then(|| [word(16), word(20)])
}

/// Per state: its baked block-model quads (`Assets::reload`, the model
/// catalog at the origin), compared by structure: positions, UVs, facing,
/// cull face, tint index and texture name, never pixels. Kept as one digest
/// per state (and the quad count) so the file stays small.
fn models(root: &Path, assets: &Assets) -> Result<Table> {
    let render = assets.reload(root)?;
    let names: BTreeMap<u32, &str> = render
        .sprite_layers
        .iter()
        .map(|(name, layer)| (*layer, name.trim_start_matches("minecraft:")))
        .collect();
    let registry = assets.registry;
    let r = |v: f32| (v * 1e4).round() / 1e4;
    let mut out = Table::new();
    for i in 0..registry.len() {
        let id = BlockStateId(i as u32);
        let Ok(models) = render
            .model_catalog
            .collect(id, horizon_core::position::BlockPos::new(0, 0, 0))
        else {
            out.insert(state_key(registry, id), json!("unbaked"));
            continue;
        };
        let mut quads: Vec<String> = Vec::new();
        for (_, model) in &models {
            for face in &model.faces {
                let q = &face.quad;
                quads.push(format!(
                    "{:?} {:?} {:?} {:?} {} {}",
                    q.positions.map(|p| p.map(r)),
                    q.uv.map(|p| p.map(r)),
                    q.facing,
                    q.cull,
                    q.tint_index,
                    names.get(&face.layer).copied().unwrap_or("?"),
                ));
            }
        }
        quads.sort();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&quads, &mut hasher);
        let digest = std::hash::Hasher::finish(&hasher);
        out.insert(
            state_key(registry, id),
            json!({ "quads": quads.len(), "digest": format!("{digest:016x}") }),
        );
    }
    Ok(out)
}
