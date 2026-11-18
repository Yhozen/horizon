//! Minecraft 26.3's numbering, from the name lists in `data/` (written by
//! `tools/open_registry_names.py`): every registry the bundle writes, in
//! protocol order, and each block's properties with their values in
//! Minecraft's order and its default state. The bundle numbers everything by
//! these, so its ids are Minecraft's.
use std::{collections::HashMap, sync::OnceLock};

/// A registry's name in the `minecraft:` namespace and its list.
const REGISTRIES: &[(&str, &str)] = &[
    (
        "attribute",
        include_str!("../data/registries/attribute.txt"),
    ),
    ("block", include_str!("../data/registries/block.txt")),
    (
        "block_entity_type",
        include_str!("../data/registries/block_entity_type.txt"),
    ),
    (
        "command_argument_type",
        include_str!("../data/registries/command_argument_type.txt"),
    ),
    (
        "custom_stat",
        include_str!("../data/registries/custom_stat.txt"),
    ),
    (
        "data_component_type",
        include_str!("../data/registries/data_component_type.txt"),
    ),
    (
        "entity_type",
        include_str!("../data/registries/entity_type.txt"),
    ),
    ("item", include_str!("../data/registries/item.txt")),
    (
        "map_decoration_type",
        include_str!("../data/registries/map_decoration_type.txt"),
    ),
    ("menu", include_str!("../data/registries/menu.txt")),
    (
        "mob_effect",
        include_str!("../data/registries/mob_effect.txt"),
    ),
    (
        "particle_type",
        include_str!("../data/registries/particle_type.txt"),
    ),
    (
        "recipe_book_category",
        include_str!("../data/registries/recipe_book_category.txt"),
    ),
    (
        "recipe_display",
        include_str!("../data/registries/recipe_display.txt"),
    ),
    (
        "slot_display",
        include_str!("../data/registries/slot_display.txt"),
    ),
    (
        "sound_event",
        include_str!("../data/registries/sound_event.txt"),
    ),
    (
        "stat_type",
        include_str!("../data/registries/stat_type.txt"),
    ),
    (
        "villager_profession",
        include_str!("../data/registries/villager_profession.txt"),
    ),
    (
        "villager_type",
        include_str!("../data/registries/villager_type.txt"),
    ),
];

const STATES: &str = include_str!("../data/blocks/states.txt");

/// Every registry the bundle numbers, by name (without `minecraft:`).
pub fn registries() -> impl Iterator<Item = &'static str> {
    REGISTRIES.iter().map(|(name, _)| *name)
}

/// A registry's names in protocol order (line n is id n), without the
/// `minecraft:` namespace (other namespaces are kept: `brigadier:bool`).
pub fn names(registry: &str) -> &'static [&'static str] {
    static LISTS: OnceLock<HashMap<&'static str, Vec<&'static str>>> = OnceLock::new();
    LISTS
        .get_or_init(|| {
            REGISTRIES
                .iter()
                .map(|(name, text)| (*name, text.lines().collect()))
                .collect()
        })
        .get(registry)
        .map(Vec::as_slice)
        .unwrap_or_else(|| panic!("no name list for the registry {registry}"))
}

/// A name's id in a registry.
pub fn id(registry: &str, name: &str) -> Option<usize> {
    names(registry).iter().position(|n| *n == name)
}

/// One block as Minecraft numbers its states.
#[derive(Debug)]
pub struct BlockStates {
    pub name: &'static str,
    /// Keys sorted, values in Minecraft's order; the last key varies fastest.
    pub properties: &'static [(&'static str, &'static [&'static str])],
    /// The default state's value of each key.
    pub defaults: &'static [(&'static str, &'static str)],
}

/// Every block in registry order.
pub fn blocks() -> &'static [BlockStates] {
    static BLOCKS: OnceLock<Vec<BlockStates>> = OnceLock::new();
    BLOCKS.get_or_init(|| {
        STATES
            .lines()
            .map(|line| {
                let mut parts = line.split(' ');
                let name = parts.next().expect("a block name");
                let mut properties = Vec::new();
                let mut defaults = Vec::new();
                for part in parts {
                    let (key, values) = part.split_once('=').expect("key=values");
                    let values: Vec<&'static str> = values
                        .split(',')
                        .map(|v| match v.strip_prefix('*') {
                            Some(v) => {
                                defaults.push((key, v));
                                v
                            }
                            None => v,
                        })
                        .collect();
                    properties.push((key, &*Vec::leak(values)));
                }
                assert_eq!(
                    defaults.len(),
                    properties.len(),
                    "{name}: one default per key"
                );
                BlockStates {
                    name,
                    properties: Vec::leak(properties),
                    defaults: Vec::leak(defaults),
                }
            })
            .collect()
    })
}

/// The number of states a block's properties make.
pub fn state_count(properties: &[(&str, &[&str])]) -> usize {
    properties.iter().map(|(_, values)| values.len()).product()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lists_agree_with_each_other_and_number_every_state() {
        let blocks = blocks();
        let names: Vec<&str> = blocks.iter().map(|b| b.name).collect();
        assert_eq!(names, super::names("block"));
        assert_eq!(names[0], "air");
        let states: usize = blocks.iter().map(|b| state_count(b.properties)).sum();
        assert_eq!(states, 35_723);
        for block in blocks {
            assert!(block.properties.windows(2).all(|w| w[0].0 < w[1].0));
        }
        for registry in registries() {
            let list = super::names(registry);
            let mut sorted = list.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), list.len(), "{registry} repeats a name");
        }
        assert_eq!(super::names("attribute").len(), 40);
        assert_eq!(super::names("item")[0], "air");
    }
}
