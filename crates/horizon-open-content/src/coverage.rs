//! `horizon-open-coverage.json`: which of the names `registries.json` lists
//! have content in this bundle. Every Minecraft name is registered so the
//! numbering is Minecraft's ([`crate::numbering`]); a name without content
//! still loads (an inert block, an item or entity with no model), and the
//! relay keeps a stand-in for it. It is also the count of what is finished.
//!
//! The shape (docs/open-creative.md, "Coverage"):
//!
//! ```json
//! {"schema": 1, "registries": {
//!   "minecraft:block": {"total": 1286, "content": ["minecraft:air", ...]},
//!   "minecraft:menu": {"total": 25, "missing": []}
//! }}
//! ```
//!
//! Each registry has exactly one of `content` (the names with content) or
//! `missing` (the names without), whichever is shorter (`missing` on a tie),
//! in protocol order; every other name of the registry is in the other set.
use crate::{
    Bundle, Result,
    blocks::{Block, Form, Tier},
    network::{self, ENTITY_TYPES},
    numbering,
};
use serde_json::{Map, Value, json};
use std::collections::HashSet;

pub const PATH: &str = "horizon-open-coverage.json";
const GENERATOR: &str = "coverage";

/// Registries whose every name the client handles from the shared code
/// alone: codecs and numbers it decodes, statistics it counts by name.
const COMPLETE: &[&str] = &[
    "attribute",
    "command_argument_type",
    "custom_stat",
    "data_component_type",
    "recipe_book_category",
    "recipe_display",
    "slot_display",
    "stat_type",
];

/// Each menu's screen background (under `textures/gui/`): a menu has content
/// when the bundle has it.
const MENU_SCREENS: &[(&str, &str)] = &[
    ("generic_9x1", "container/generic_54"),
    ("generic_9x2", "container/generic_54"),
    ("generic_9x3", "container/generic_54"),
    ("generic_9x4", "container/generic_54"),
    ("generic_9x5", "container/generic_54"),
    ("generic_9x6", "container/generic_54"),
    ("generic_3x3", "container/dispenser"),
    ("crafter_3x3", "container/crafter"),
    ("anvil", "container/anvil"),
    ("beacon", "container/beacon"),
    ("blast_furnace", "container/blast_furnace"),
    ("brewing_stand", "container/brewing_stand"),
    ("crafting", "container/crafting_table"),
    ("enchantment", "container/enchanting_table"),
    ("furnace", "container/furnace"),
    ("grindstone", "container/grindstone"),
    ("hopper", "container/hopper"),
    ("lectern", "book"),
    ("loom", "container/loom"),
    ("merchant", "container/villager"),
    ("shulker_box", "container/shulker_box"),
    ("smithing", "container/smithing"),
    ("smoker", "container/smoker"),
    ("cartography_table", "container/cartography_table"),
    ("stonecutter", "container/stonecutter"),
];

/// Whether a block has content: defined here, or an air the shared code
/// treats as air (cave and void air), which looks and behaves as Minecraft's.
fn block_content(block: &Block) -> bool {
    block.tier != Tier::Support || block.form == Form::Air
}

/// The names with content, by registry (without `minecraft:`).
fn content(
    registry: &str,
    bundle: &Bundle,
    blocks: &[Block],
    items: &HashSet<String>,
) -> HashSet<String> {
    let names = numbering::names(registry).iter();
    let set: HashSet<String> = match registry {
        "block" => {
            let with: HashSet<&str> = blocks
                .iter()
                .filter(|b| block_content(b))
                .map(|b| b.id)
                .collect();
            names
                .filter(|n| with.contains(*n))
                .map(|n| n.to_string())
                .collect()
        }
        "item" => names
            .filter(|n| items.contains(&network::full(n)))
            .map(|n| n.to_string())
            .collect(),
        "entity_type" => ENTITY_TYPES
            .iter()
            .copied()
            .chain(crate::mobs::names())
            .map(str::to_owned)
            .collect(),
        "menu" => MENU_SCREENS
            .iter()
            .filter(|(_, screen)| {
                bundle.contains(&format!("assets/minecraft/textures/gui/{screen}.png"))
            })
            .map(|(menu, _)| menu.to_string())
            .collect(),
        r if COMPLETE.contains(&r) => names.map(|n| n.to_string()).collect(),
        _ => HashSet::new(),
    };
    set
}

pub fn write(bundle: &mut Bundle, blocks: &[Block]) -> Result<()> {
    let items: Value = serde_json::from_slice(
        bundle
            .get("items.json")
            .ok_or("coverage: items.json is written first")?,
    )
    .map_err(|e| format!("coverage: items.json: {e}"))?;
    let items: HashSet<String> = items
        .as_object()
        .ok_or("coverage: items.json is not an object")?
        .keys()
        .cloned()
        .collect();
    let mut registries = Map::new();
    for registry in numbering::registries() {
        let names = numbering::names(registry);
        let with = content(registry, bundle, blocks, &items);
        if let Some(stray) = with.iter().find(|n| !names.contains(&n.as_str())) {
            return Err(format!(
                "coverage: {stray} has content but is not in the {registry} registry"
            ));
        }
        let (key, listed): (&str, Vec<String>) = if with.len() * 2 < names.len() {
            (
                "content",
                names
                    .iter()
                    .filter(|n| with.contains(**n))
                    .map(|n| network::full(n))
                    .collect(),
            )
        } else {
            (
                "missing",
                names
                    .iter()
                    .filter(|n| !with.contains(**n))
                    .map(|n| network::full(n))
                    .collect(),
            )
        };
        registries.insert(
            format!("minecraft:{registry}"),
            json!({"total": names.len(), key: listed}),
        );
    }
    bundle.json(
        PATH,
        &json!({"schema": 1, "registries": registries}),
        GENERATOR,
    )
}
