//! Pre-1.13 numeric block ids, for packs without `block.properties`.
//!
//! Packs written for the numeric ids compare `mc_Entity.x` against them
//! (leaves, plants, water…). With no `block.properties`, a block name maps to
//! its old id by the rules below, written from the public pre-flattening id
//! list; a block the rules do not cover reads -1. Only ids such packs
//! compare are produced, so the whole table fits the vertex tag's 31
//! materials ([`LEGACY_IDS`]). Deviations from the old numbering follow
//! what packs observe on current loaders (format-notes §2, §13): water and
//! lava use their still ids 9 and 11 in every state, crops share wheat's 59,
//! and `emerald_block` reads -123 (133 read as a signed byte).

/// `mc_Entity.x` of a block without an id.
pub const UNLISTED: i32 = -1;

/// Every id [`legacy_id`] can return, ascending.
pub const LEGACY_IDS: [i32; 28] = [
    -123, 1, 2, 4, 9, 11, 12, 18, 24, 30, 31, 35, 37, 38, 41, 42, 50, 51, 57, 59, 79, 89, 95, 111,
    124, 160, 174, 175,
];

/// The old numeric id of a block (`stone` or `minecraft:stone`), if the
/// rules cover it.
pub fn legacy_id(block: &str) -> Option<i32> {
    let name = block.strip_prefix("minecraft:").unwrap_or(block);
    let id = match name {
        "stone" | "granite" | "polished_granite" | "diorite" | "polished_diorite" | "andesite"
        | "polished_andesite" => 1,
        "grass_block" => 2,
        "cobblestone" => 4,
        "water" => 9,
        "lava" => 11,
        "sand" | "red_sand" => 12,
        "sandstone" | "chiseled_sandstone" | "cut_sandstone" => 24,
        "cobweb" => 30,
        // `grass` is the pre-1.20.3 name of `short_grass`.
        "short_grass" | "grass" | "fern" | "seagrass" | "sweet_berry_bush" => 31,
        "dandelion" => 37,
        "poppy" | "blue_orchid" | "allium" | "azure_bluet" | "red_tulip" | "orange_tulip"
        | "white_tulip" | "pink_tulip" | "oxeye_daisy" | "cornflower" | "lily_of_the_valley"
        | "wither_rose" => 38,
        "gold_block" => 41,
        "iron_block" => 42,
        "torch" | "wall_torch" => 50,
        "fire" => 51,
        "diamond_block" => 57,
        "wheat" | "carrots" | "potatoes" => 59,
        "ice" => 79,
        "glowstone" => 89,
        "lily_pad" => 111,
        "redstone_lamp" => 124,
        "packed_ice" => 174,
        "sunflower" | "lilac" | "tall_grass" | "large_fern" | "rose_bush" | "peony"
        | "tall_seagrass" => 175,
        "emerald_block" => -123,
        _ if name.ends_with("_stained_glass_pane") => 160,
        _ if name.ends_with("_stained_glass") => 95,
        _ if name.ends_with("_wool") => 35,
        _ if name.ends_with("_leaves") => 18,
        _ => return None,
    };
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_and_quirks() {
        let cases = [
            ("minecraft:stone", Some(1)),
            ("polished_andesite", Some(1)),
            ("grass_block", Some(2)),
            ("water", Some(9)),
            ("lava", Some(11)),
            ("oak_leaves", Some(18)),
            ("cherry_leaves", Some(18)),
            ("red_wool", Some(35)),
            ("light_blue_stained_glass", Some(95)),
            ("light_blue_stained_glass_pane", Some(160)),
            ("short_grass", Some(31)),
            ("fern", Some(31)),
            ("dandelion", Some(37)),
            ("pink_tulip", Some(38)),
            ("peony", Some(175)),
            ("potatoes", Some(59)),
            ("emerald_block", Some(-123)),
            ("glass", None),
            ("dirt", None),
            ("tinted_glass", None),
            ("soul_fire", None),
        ];
        for (name, id) in cases {
            assert_eq!(legacy_id(name), id, "{name}");
        }
    }

    #[test]
    fn the_id_list_is_complete_and_sorted() {
        assert!(LEGACY_IDS.windows(2).all(|w| w[0] < w[1]));
        // Every name in the match produces a listed id.
        for name in [
            "stone",
            "grass_block",
            "cobblestone",
            "water",
            "lava",
            "sand",
            "sandstone",
            "cobweb",
            "fern",
            "dandelion",
            "poppy",
            "gold_block",
            "iron_block",
            "torch",
            "fire",
            "diamond_block",
            "wheat",
            "ice",
            "glowstone",
            "lily_pad",
            "redstone_lamp",
            "packed_ice",
            "lilac",
            "emerald_block",
            "x_stained_glass_pane",
            "x_stained_glass",
            "x_wool",
            "x_leaves",
        ] {
            let id = legacy_id(name).unwrap();
            assert!(LEGACY_IDS.contains(&id), "{name} {id}");
        }
    }
}
