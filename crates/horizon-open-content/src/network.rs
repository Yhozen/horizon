//! The network registries an open server and its players share
//! (`registries.json`, the shape of the data generator's registries report).
//! Every registry lists all of Minecraft 26.3's names under Minecraft's ids
//! ([`crate::numbering`]), so the numbers on the wire are Minecraft's; which
//! names have content yet is in the coverage file ([`crate::coverage`]).
use crate::{Bundle, Result, numbering};
use serde_json::{Map, Value, json};

const GENERATOR: &str = "network";

/// Entities an open world can hold beside the mobs (`crate::mobs`).
pub const ENTITY_TYPES: &[&str] = &[
    "player",
    "item",
    "experience_orb",
    "falling_block",
    "lightning_bolt",
];

/// A name with its namespace (`minecraft:` when it has none).
pub fn full(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

fn registry(names: &[&str]) -> Value {
    let entries: Map<String, Value> = names
        .iter()
        .enumerate()
        .map(|(id, name)| (full(name), json!({ "protocol_id": id })))
        .collect();
    json!({ "entries": entries })
}

pub fn write(bundle: &mut Bundle) -> Result<()> {
    let report: Map<String, Value> = numbering::registries()
        .map(|name| {
            (
                format!("minecraft:{name}"),
                registry(numbering::names(name)),
            )
        })
        .collect();
    bundle.compact_json("registries.json", &Value::Object(report), GENERATOR)
}
