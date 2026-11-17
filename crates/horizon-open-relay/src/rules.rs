//! What a vanilla block or item becomes for an open player when the open
//! bundle has nothing of the same name: the nearest open block or item by
//! material and colour, by name alone. These are rules over the names, not a
//! copy of any vanilla table: the vanilla names come from the server's own
//! data at runtime.
//!
//! Thin or non-solid decoration (buttons, torches, rails, carpets, signs ...)
//! becomes air, or water when it is waterlogged; everything else becomes a
//! full block, so the open player sees and collides with roughly the same
//! shapes the server has.

/// The sixteen dye colours, as vanilla names them.
const COLOURS: [&str; 16] = [
    "light_blue",
    "light_gray",
    "white",
    "orange",
    "magenta",
    "yellow",
    "lime",
    "pink",
    "gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

/// The open wool of a dye colour (the bundle has every colour but light blue).
fn wool(colour: &str) -> String {
    match colour {
        "light_blue" => "cyan_wool".into(),
        c => format!("{c}_wool"),
    }
}

/// Wood species in vanilla names and the open species that looks closest
/// (longer names first: `dark_oak` before `oak`).
const SPECIES: [(&str, &str); 15] = [
    ("dark_oak", "spruce"),
    ("pale_oak", "birch"),
    ("flowering_azalea", "oak"),
    ("azalea", "oak"),
    ("oak", "oak"),
    ("spruce", "spruce"),
    ("birch", "birch"),
    ("jungle", "jungle"),
    ("acacia", "acacia"),
    ("mangrove", "jungle"),
    ("cherry", "birch"),
    ("bamboo", "birch"),
    ("poplar", "birch"),
    ("crimson", "acacia"),
    ("warped", "spruce"),
];

/// Parts of names whose blocks are thin, small or not solid.
const THIN: &[&str] = &[
    "button",
    "pressure_plate",
    "sign",
    "banner",
    "carpet",
    "torch",
    "rail",
    "redstone_wire",
    "repeater",
    "comparator",
    "lever",
    "tripwire",
    "ladder",
    "vine",
    "lichen",
    "hanging_roots",
    "candle",
    "_head",
    "skull",
    "flower_pot",
    "potted_",
    "iron_bars",
    "end_rod",
    "lightning_rod",
    "sea_pickle",
    "frogspawn",
    "turtle_egg",
    "sniffer_egg",
    "petals",
    "wildflowers",
    "leaf_litter",
    "cobweb",
    "portal",
    "end_gateway",
    "structure_void",
    "moving_piston",
    "piston_head",
    "brewing_stand",
    "lantern",
    "_bed",
    "dripleaf",
    "spore_blossom",
    "amethyst_cluster",
    "amethyst_bud",
    "pointed_dripstone",
    "sculk_vein",
    "scaffolding",
    "conduit",
    "daylight_detector",
    "cactus_flower",
    "firefly_bush",
    "_fan",
    "resin_clump",
    "chain",
    "shelf",
    "copper_golem_statue",
];

/// Ground plants without collision: open short grass.
const PLANTS: &[&str] = &[
    "tall_grass",
    "bush",
    "wheat",
    "carrots",
    "potatoes",
    "beetroots",
    "melon_stem",
    "pumpkin_stem",
    "torchflower_crop",
    "pitcher_crop",
    "pitcher_plant",
    "nether_wart",
    "sweet_berry_bush",
    "cocoa",
    "nether_sprouts",
    "crimson_roots",
    "warped_roots",
    "short_dry_grass",
    "tall_dry_grass",
];

/// Flowers by the open flower that looks closest.
const FLOWERS: &[(&str, &str)] = &[
    ("blue_orchid", "cornflower"),
    ("azure_bluet", "oxeye_daisy"),
    ("lily_of_the_valley", "oxeye_daisy"),
    ("red_tulip", "poppy"),
    ("white_tulip", "oxeye_daisy"),
    ("pink_tulip", "allium"),
    ("wither_rose", "poppy"),
    ("torchflower", "orange_tulip"),
    ("open_eyeblossom", "orange_tulip"),
    ("closed_eyeblossom", "allium"),
    ("sunflower", "dandelion"),
    ("lilac", "allium"),
    ("rose_bush", "poppy"),
    ("peony", "allium"),
    ("large_fern", "fern"),
    ("crimson_fungus", "red_mushroom"),
    ("warped_fungus", "brown_mushroom"),
];

/// Exact names, checked before the patterns.
const EXACT: &[(&str, &str)] = &[
    ("cave_air", "air"),
    ("void_air", "air"),
    ("barrier", "air"),
    ("light", "air"),
    ("fire", "air"),
    ("soul_fire", "air"),
    ("snow", "air"),
    ("lily_pad", "air"),
    ("tripwire_hook", "air"),
    ("bell", "gold_block"),
    ("command_block", "iron_block"),
    ("chain_command_block", "iron_block"),
    ("repeating_command_block", "iron_block"),
    ("water", "water"),
    ("bubble_column", "water"),
    ("kelp", "water"),
    ("kelp_plant", "water"),
    ("seagrass", "water"),
    ("tall_seagrass", "water"),
    ("lava", "orange_wool"),
    ("magma_block", "orange_wool"),
    ("short_grass", "short_grass"),
    ("grass", "short_grass"),
    ("fern", "fern"),
    ("dead_bush", "dead_bush"),
    ("bamboo", "sugar_cane"),
    ("bamboo_sapling", "sugar_cane"),
    ("mangrove_propagule", "jungle_sapling"),
    ("moss_block", "grass_block"),
    ("pale_moss_block", "light_gray_wool"),
    ("mud", "dirt"),
    ("packed_mud", "dirt"),
    ("muddy_mangrove_roots", "dirt"),
    ("mangrove_roots", "dirt"),
    ("coarse_dirt", "dirt"),
    ("rooted_dirt", "dirt"),
    ("podzol", "dirt"),
    ("mycelium", "dirt"),
    ("farmland", "dirt"),
    ("dirt_path", "dirt"),
    ("soul_sand", "dirt"),
    ("soul_soil", "dirt"),
    ("suspicious_sand", "sand"),
    ("suspicious_gravel", "gravel"),
    ("powder_snow", "snow_block"),
    ("packed_ice", "ice"),
    ("blue_ice", "ice"),
    ("frosted_ice", "ice"),
    ("netherrack", "red_sandstone"),
    ("crimson_nylium", "red_sandstone"),
    ("warped_nylium", "cyan_wool"),
    ("end_stone", "sandstone"),
    ("end_portal_frame", "sandstone"),
    ("crying_obsidian", "obsidian"),
    ("respawn_anchor", "obsidian"),
    ("enchanting_table", "obsidian"),
    ("ender_chest", "obsidian"),
    ("dragon_egg", "obsidian"),
    ("reinforced_deepslate", "obsidian"),
    ("netherite_block", "coal_block"),
    ("ancient_debris", "coal_block"),
    ("emerald_block", "diamond_block"),
    ("lapis_block", "blue_wool"),
    ("redstone_block", "red_wool"),
    ("amethyst_block", "purple_wool"),
    ("budding_amethyst", "purple_wool"),
    ("pumpkin", "orange_wool"),
    ("carved_pumpkin", "orange_wool"),
    ("melon", "lime_wool"),
    ("hay_block", "yellow_wool"),
    ("dried_kelp_block", "green_wool"),
    ("honey_block", "orange_wool"),
    ("honeycomb_block", "yellow_wool"),
    ("slime_block", "lime_wool"),
    ("sponge", "yellow_wool"),
    ("wet_sponge", "yellow_wool"),
    ("target", "white_wool"),
    ("tnt", "red_wool"),
    ("cake", "white_wool"),
    ("bone_block", "white_wool"),
    ("mushroom_stem", "white_wool"),
    ("red_mushroom_block", "red_wool"),
    ("brown_mushroom_block", "brown_wool"),
    ("nether_wart_block", "red_wool"),
    ("warped_wart_block", "cyan_wool"),
    ("chorus_plant", "purple_wool"),
    ("chorus_flower", "purple_wool"),
    ("terracotta", "red_sandstone"),
    ("shulker_box", "purple_wool"),
    ("decorated_pot", "bricks"),
    ("creaking_heart", "birch_log"),
    ("resin_block", "orange_wool"),
    ("glowstone", "glowstone"),
    ("sea_lantern", "glowstone"),
    ("shroomlight", "glowstone"),
    ("jack_o_lantern", "glowstone"),
    ("redstone_lamp", "glowstone"),
    ("beacon", "glass"),
    ("spawner", "tinted_glass"),
    ("trial_spawner", "tinted_glass"),
    ("vault", "tinted_glass"),
    ("bookshelf", "bookshelf"),
    ("chiseled_bookshelf", "bookshelf"),
    ("crafter", "crafting_table"),
    ("calcite", "smooth_stone"),
    ("dripstone_block", "granite"),
    ("blackstone", "coal_block"),
    ("red_shrub", "dead_bush"),
    ("sulfur_spike", "air"),
];

/// Coral colours: the wool closest to each.
const CORALS: [(&str, &str); 5] = [
    ("tube", "blue_wool"),
    ("brain", "pink_wool"),
    ("bubble", "magenta_wool"),
    ("fire", "red_wool"),
    ("horn", "yellow_wool"),
];

/// Blocks that work like wood workstations and containers.
const WOODEN: &[&str] = &[
    "chest",
    "barrel",
    "lectern",
    "note_block",
    "jukebox",
    "composter",
    "beehive",
    "bee_nest",
    "loom",
    "cartography_table",
    "fletching_table",
    "smithing_table",
];
/// Blocks that work like stone machines.
const STONY: &[&str] = &[
    "furnace",
    "smoker",
    "dispenser",
    "dropper",
    "observer",
    "piston",
    "stonecutter",
    "grindstone",
    "lodestone",
];

/// The open block (name without namespace) a vanilla block becomes, or air.
/// `waterlogged`: the state holds water, so whatever becomes air becomes water.
pub fn block(name: &str, waterlogged: bool) -> String {
    let target = block_rule(name);
    if target == "air" && waterlogged {
        "water".into()
    } else {
        target
    }
}

fn block_rule(name: &str) -> String {
    if let Some((_, to)) = EXACT.iter().find(|(from, _)| *from == name) {
        return (*to).into();
    }
    if let Some((_, to)) = FLOWERS.iter().find(|(from, _)| *from == name) {
        return (*to).into();
    }
    if name.contains("coral") {
        if !name.ends_with("coral_block") {
            return "air".into();
        }
        if name.starts_with("dead_") {
            return "gray_wool".into();
        }
        let colour = CORALS.iter().find(|(c, _)| name.starts_with(c));
        return colour.map_or("pink_wool", |(_, w)| w).into();
    }
    if name.ends_with("_ore") {
        let base = name
            .trim_start_matches("deepslate_")
            .trim_start_matches("nether_");
        return match base {
            "coal_ore" | "copper_ore" | "iron_ore" | "gold_ore" | "diamond_ore" => base.into(),
            _ if name.starts_with("nether_") => "red_sandstone".into(),
            _ => "stone".into(),
        };
    }
    if let Some(metal) = name
        .strip_prefix("raw_")
        .and_then(|n| n.strip_suffix("_block"))
    {
        return format!("{metal}_block");
    }
    if name.contains("glass") {
        let dark = ["tinted", "black_", "gray_"]
            .iter()
            .any(|d| name.contains(d) && !name.contains("light_gray"));
        return if dark { "tinted_glass" } else { "glass" }.into();
    }
    if THIN.iter().any(|t| name.contains(t)) {
        return "air".into();
    }
    if PLANTS.iter().any(|p| name == *p || name.ends_with(p)) {
        return "short_grass".into();
    }
    if let Some(colour) = COLOURS.iter().find(|c| {
        name.strip_prefix(**c)
            .is_some_and(|rest| rest.starts_with('_'))
    }) {
        let rest = name[colour.len() + 1..]
            .trim_end_matches("_slab")
            .trim_end_matches("_stairs")
            .trim_end_matches("_wall");
        if [
            "wool",
            "concrete",
            "concrete_powder",
            "terracotta",
            "glazed_terracotta",
            "shulker_box",
        ]
        .contains(&rest)
        {
            return wool(colour);
        }
    }
    if name.contains("copper") {
        return "copper_block".into();
    }
    for (metal, block) in [
        ("anvil", "iron_block"),
        ("cauldron", "iron_block"),
        ("hopper", "iron_block"),
        ("heavy_core", "iron_block"),
        ("command_block", "iron_block"),
        ("structure_block", "iron_block"),
        ("jigsaw", "iron_block"),
        ("test_", "iron_block"),
        ("iron", "iron_block"),
        ("gold", "gold_block"),
        ("diamond", "diamond_block"),
        ("quartz", "smooth_stone"),
        ("purpur", "purple_wool"),
        ("prismarine", "cyan_wool"),
        ("sculk", "black_wool"),
        ("froglight", "glowstone"),
        ("cinnabar", "red_sandstone"),
        ("sulfur", "sandstone"),
    ] {
        if name.contains(metal) {
            return block.into();
        }
    }
    if name.contains("nylium") {
        return "red_sandstone".into();
    }
    if let Some((_, species)) = SPECIES.iter().find(|(s, _)| name.contains(s)) {
        let part = if name.ends_with("_log")
            || name.ends_with("_wood")
            || name.ends_with("_stem")
            || name.ends_with("_hyphae")
            || name == "bamboo_block"
            || name == "stripped_bamboo_block"
        {
            "log"
        } else if name.ends_with("_leaves") || name == "azalea" || name == "flowering_azalea" {
            "leaves"
        } else if name.ends_with("_sapling") {
            "sapling"
        } else {
            "planks"
        };
        return format!("{species}_{part}");
    }
    if WOODEN.iter().any(|w| name.ends_with(w)) {
        return "oak_planks".into();
    }
    if STONY.iter().any(|s| name.ends_with(s)) {
        return "cobblestone".into();
    }
    if name.starts_with("mossy_cobblestone") {
        return "mossy_cobblestone".into();
    }
    if name.contains("cobblestone") || name.contains("cobbled_") {
        return "cobblestone".into();
    }
    if name.contains("brick") {
        let red = name.contains("nether") || name.contains("mud") || name.starts_with("brick");
        return if red { "bricks" } else { "stone_bricks" }.into();
    }
    if name.contains("granite") {
        return if name.contains("polished") {
            "polished_granite"
        } else {
            "granite"
        }
        .into();
    }
    if name.contains("red_sandstone") {
        return "red_sandstone".into();
    }
    if name.contains("sandstone") {
        return "sandstone".into();
    }
    if name.starts_with("smooth_stone") {
        return "smooth_stone".into();
    }
    if name.contains("terracotta") {
        return "red_sandstone".into();
    }
    for ground in [
        "deepslate",
        "tuff",
        "andesite",
        "diorite",
        "basalt",
        "stone",
        "infested",
    ] {
        if name.contains(ground) {
            return "stone".into();
        }
    }
    if name.contains("dirt") || name.contains("mud") {
        return "dirt".into();
    }
    if name.contains("sand") {
        return "sand".into();
    }
    if name.contains("ice") {
        return "ice".into();
    }
    if name.contains("snow") {
        return "snow_block".into();
    }
    "air".into()
}

/// Edible items: all become the open apple, its only food.
const FOOD: &[&str] = &[
    "bread",
    "cookie",
    "carrot",
    "golden_carrot",
    "potato",
    "baked_potato",
    "poisonous_potato",
    "beetroot",
    "melon_slice",
    "glistering_melon_slice",
    "golden_apple",
    "enchanted_golden_apple",
    "pumpkin_pie",
    "mushroom_stew",
    "rabbit_stew",
    "beetroot_soup",
    "suspicious_stew",
    "sweet_berries",
    "glow_berries",
    "dried_kelp",
    "honey_bottle",
    "chorus_fruit",
    "rotten_flesh",
    "spider_eye",
    "tropical_fish",
    "pufferfish",
    "beef",
    "porkchop",
    "chicken",
    "mutton",
    "rabbit",
    "cod",
    "salmon",
];

/// The open item (name without namespace) a vanilla item that is not an open
/// item or block becomes: tools by tier and kind, food as the apple,
/// materials by their look, and a stick for everything else.
pub fn item(name: &str) -> String {
    if FOOD.contains(&name) || name.starts_with("cooked_") {
        return "apple".into();
    }
    for tool in ["pickaxe", "axe", "shovel", "sword", "hoe", "spear"] {
        if let Some(tier) = name.strip_suffix(tool).and_then(|t| t.strip_suffix('_')) {
            let tier = match tier {
                "wooden" | "stone" | "iron" | "diamond" => tier,
                "golden" | "copper" => "iron",
                "netherite" => "diamond",
                _ => "stone",
            };
            let tool = match tool {
                "hoe" => "shovel",
                "spear" => "sword",
                t => t,
            };
            return format!("{tier}_{tool}");
        }
    }
    match name {
        "mace" | "trident" => "iron_sword",
        "charcoal" | "flint" => "coal",
        "iron_nugget" => "iron_ingot",
        "gold_nugget" => "gold_ingot",
        "copper_nugget" => "copper_ingot",
        "netherite_ingot"
        | "netherite_scrap"
        | "emerald"
        | "lapis_lazuli"
        | "amethyst_shard"
        | "quartz"
        | "prismarine_crystals"
        | "echo_shard" => "diamond",
        "nether_brick" | "resin_brick" => "brick",
        _ => "stick",
    }
    .into()
}

/// "golden_apple" as players read it: "Golden Apple".
pub fn title(name: &str) -> String {
    name.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wood_keeps_its_part_and_nearest_species() {
        assert_eq!(block("dark_oak_log", false), "spruce_log");
        assert_eq!(block("stripped_cherry_wood", false), "birch_log");
        assert_eq!(block("mangrove_leaves", false), "jungle_leaves");
        assert_eq!(block("oak_stairs", false), "oak_planks");
        assert_eq!(block("pale_oak_slab", false), "birch_planks");
        assert_eq!(block("crimson_stem", false), "acacia_log");
        assert_eq!(block("cherry_sapling", false), "birch_sapling");
        assert_eq!(block("oak_sign", false), "air");
        assert_eq!(block("spruce_button", false), "air");
    }

    #[test]
    fn stone_ore_and_metal() {
        assert_eq!(block("deepslate_iron_ore", false), "iron_ore");
        assert_eq!(block("deepslate", false), "stone");
        assert_eq!(block("cobbled_deepslate_wall", false), "cobblestone");
        assert_eq!(block("andesite_stairs", false), "stone");
        assert_eq!(block("emerald_ore", false), "stone");
        assert_eq!(block("nether_gold_ore", false), "gold_ore");
        assert_eq!(block("raw_iron_block", false), "iron_block");
        assert_eq!(
            block("waxed_weathered_cut_copper_stairs", false),
            "copper_block"
        );
        assert_eq!(block("mossy_stone_brick_wall", false), "stone_bricks");
        assert_eq!(block("nether_bricks", false), "bricks");
        assert_eq!(block("polished_granite_slab", false), "polished_granite");
        assert_eq!(block("smooth_red_sandstone", false), "red_sandstone");
        assert_eq!(block("iron_door", false), "iron_block");
    }

    #[test]
    fn colours_and_decoration() {
        assert_eq!(block("light_blue_concrete", false), "cyan_wool");
        assert_eq!(block("red_glazed_terracotta", false), "red_wool");
        assert_eq!(block("light_gray_stained_glass", false), "glass");
        assert_eq!(block("black_stained_glass_pane", false), "tinted_glass");
        assert_eq!(block("white_carpet", false), "air");
        assert_eq!(block("red_bed", false), "air");
        assert_eq!(block("torch", false), "air");
        assert_eq!(block("rail", false), "air");
        assert_eq!(block("brain_coral_block", false), "pink_wool");
        assert_eq!(block("brain_coral_fan", true), "water");
        assert_eq!(block("oak_trapdoor", true), "oak_planks");
        assert_eq!(block("lantern", true), "water");
        assert_eq!(block("blue_wool_slab", false), "blue_wool");
        assert_eq!(block("red_poplar_leaves", false), "birch_leaves");
        assert_eq!(block("polished_cinnabar_wall", false), "red_sandstone");
    }

    #[test]
    fn ground_and_plants() {
        assert_eq!(block("podzol", false), "dirt");
        assert_eq!(block("tall_grass", false), "short_grass");
        assert_eq!(block("wheat", false), "short_grass");
        assert_eq!(block("blue_orchid", false), "cornflower");
        assert_eq!(block("kelp_plant", false), "water");
        assert_eq!(block("lava", false), "orange_wool");
        assert_eq!(block("cave_air", false), "air");
        assert_eq!(block("chest", false), "oak_planks");
        assert_eq!(block("blast_furnace", false), "cobblestone");
    }

    #[test]
    fn items_by_kind() {
        assert_eq!(item("netherite_pickaxe"), "diamond_pickaxe");
        assert_eq!(item("golden_hoe"), "iron_shovel");
        assert_eq!(item("cooked_beef"), "apple");
        assert_eq!(item("bread"), "apple");
        assert_eq!(item("emerald"), "diamond");
        assert_eq!(item("ender_pearl"), "stick");
        assert_eq!(title("enchanted_golden_apple"), "Enchanted Golden Apple");
    }
}
