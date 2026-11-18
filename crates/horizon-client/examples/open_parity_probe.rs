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
use horizon_client::{assets::Assets, blocks::BlockFacts};
use horizon_protocol::registry::Builtin;
use horizon_world::registry::{BlockRegistry, BlockStateId};
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
    Ok(vec![
        ("registries", registries(root)?),
        ("blocks", blocks(registry)),
        ("states", states(registry, &facts, &flags)),
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
