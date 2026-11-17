//! The network registries an open server and its players share
//! (`registries.json`, the shape of the data generator's registries report).
//! Both ends read this one file, so the numbers only have to agree with each
//! other: they are the bundle's own order and imply nothing about Minecraft
//! servers, which an open player cannot join (and the reverse).
use crate::{Bundle, Result, blocks::Block};
use serde_json::{Map, Value, json};

const GENERATOR: &str = "network";

/// Entities an open world can hold.
const ENTITY_TYPES: &[&str] = &["player", "item", "experience_orb", "falling_block"];

/// Menus (`MenuType`) by name; only the crafting table's opens in this
/// edition, the rest keep their names resolvable.
const MENUS: &[&str] = &[
    "generic_9x1",
    "generic_9x2",
    "generic_9x3",
    "generic_9x4",
    "generic_9x5",
    "generic_9x6",
    "generic_3x3",
    "crafter_3x3",
    "anvil",
    "beacon",
    "blast_furnace",
    "brewing_stand",
    "crafting",
    "enchantment",
    "furnace",
    "grindstone",
    "hopper",
    "lectern",
    "loom",
    "merchant",
    "shulker_box",
    "smithing",
    "smoker",
    "cartography_table",
    "stonecutter",
];

/// The data components the shared item codec reads by number
/// (`horizon_protocol::item::component`), so the player names what the server sent.
const COMPONENTS: &[(i32, &str)] = &[
    (0, "custom_data"),
    (1, "max_stack_size"),
    (2, "max_damage"),
    (3, "damage"),
    (4, "unbreakable"),
    (6, "custom_name"),
    (9, "item_name"),
    (10, "item_model"),
    (11, "lore"),
    (12, "rarity"),
    (13, "enchantments"),
    (17, "custom_model_data"),
    (18, "tooltip_display"),
    (19, "repair_cost"),
    (20, "creative_slot_lock"),
    (21, "enchantment_glint_override"),
    (23, "food"),
    (24, "consumable"),
    (30, "attack_range"),
    (45, "stored_enchantments"),
    (47, "dyed_color"),
];

const RECIPE_DISPLAYS: &[&str] = &[
    "crafting_shapeless",
    "crafting_shaped",
    "furnace",
    "stonecutter",
    "smithing",
];
const SLOT_DISPLAYS: &[&str] = &[
    "empty",
    "any_fuel",
    "with_any_potion",
    "only_with_component",
    "item",
    "item_stack",
    "tag",
    "dyed",
    "smithing_trim",
    "with_remainder",
    "composite",
];
/// The recipe book's categories (`horizon_client::recipe_book::Category`).
const BOOK_CATEGORIES: &[&str] = &[
    "crafting_building_blocks",
    "crafting_redstone",
    "crafting_equipment",
    "crafting_misc",
    "furnace_food",
    "furnace_blocks",
    "furnace_misc",
    "blast_furnace_blocks",
    "blast_furnace_misc",
    "smoker_food",
];

fn registry<'a>(names: impl IntoIterator<Item = (usize, &'a str)>) -> Value {
    let entries: Map<String, Value> = names
        .into_iter()
        .map(|(id, name)| {
            let name = if name.contains(':') {
                name.to_owned()
            } else {
                format!("minecraft:{name}")
            };
            (name, json!({ "protocol_id": id }))
        })
        .collect();
    json!({ "entries": entries })
}

pub fn write(bundle: &mut Bundle, blocks: &[Block]) -> Result<()> {
    let items: Value = serde_json::from_slice(
        bundle
            .get("items.json")
            .ok_or("network: items.json is written first")?,
    )
    .map_err(|e| format!("network: items.json: {e}"))?;
    let items: Vec<&str> = items
        .as_object()
        .ok_or("network: items.json is not an object")?
        .keys()
        .map(String::as_str)
        .collect();
    let report = json!({
        "minecraft:block": registry(blocks.iter().map(|b| b.id).enumerate()),
        "minecraft:item": registry(items.iter().copied().enumerate()),
        "minecraft:entity_type": registry(ENTITY_TYPES.iter().copied().enumerate()),
        "minecraft:menu": registry(MENUS.iter().copied().enumerate()),
        "minecraft:command_argument_type": registry(
            horizon_protocol::packets::play::ARGUMENT_TYPES.iter().copied().enumerate()),
        "minecraft:data_component_type": registry(
            COMPONENTS.iter().map(|&(id, name)| (id as usize, name))),
        // Numbered as the shared recipe codecs (horizon_protocol::packets::recipes) read them.
        "minecraft:recipe_display": registry(RECIPE_DISPLAYS.iter().copied().enumerate()),
        "minecraft:slot_display": registry(SLOT_DISPLAYS.iter().copied().enumerate()),
        "minecraft:recipe_book_category": registry(BOOK_CATEGORIES.iter().copied().enumerate()),
        "minecraft:block_entity_type": registry([]),
        "minecraft:mob_effect": registry([]),
        "minecraft:particle_type": registry([(0, "block")]),
        "minecraft:sound_event": registry([]),
    });
    bundle.json("registries.json", &report, GENERATOR)
}
