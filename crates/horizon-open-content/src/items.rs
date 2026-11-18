//! The open content's items that are not blocks: sticks, tools, food and the
//! materials ores give, with their stats (this edition's own, chosen so the
//! shared survival code plays as players expect), and their crafting recipes.
use crate::blocks::{Block, Category, Drop, Form, Tier};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Pickaxe,
    Axe,
    Shovel,
    Sword,
}
impl ToolKind {
    pub const ALL: [ToolKind; 4] = [Self::Pickaxe, Self::Axe, Self::Shovel, Self::Sword];
    pub fn id(self) -> &'static str {
        match self {
            Self::Pickaxe => "pickaxe",
            Self::Axe => "axe",
            Self::Shovel => "shovel",
            Self::Sword => "sword",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    Wood,
    Stone,
    Iron,
    Diamond,
}
impl Material {
    pub const ALL: [Material; 4] = [Self::Wood, Self::Stone, Self::Iron, Self::Diamond];
    pub fn id(self) -> &'static str {
        match self {
            Self::Wood => "wooden",
            Self::Stone => "stone",
            Self::Iron => "iron",
            Self::Diamond => "diamond",
        }
    }
    fn title(self) -> &'static str {
        match self {
            Self::Wood => "Wooden",
            Self::Stone => "Stone",
            Self::Iron => "Iron",
            Self::Diamond => "Diamond",
        }
    }
    pub fn durability(self) -> u32 {
        match self {
            Self::Wood => 59,
            Self::Stone => 131,
            Self::Iron => 250,
            Self::Diamond => 1561,
        }
    }
    pub fn speed(self) -> f64 {
        match self {
            Self::Wood => 2.0,
            Self::Stone => 4.0,
            Self::Iron => 6.0,
            Self::Diamond => 8.0,
        }
    }
    /// The block tag of what this material cannot harvest.
    pub fn incorrect(self) -> &'static str {
        match self {
            Self::Wood => "incorrect_for_wooden_tool",
            Self::Stone => "incorrect_for_stone_tool",
            Self::Iron => "incorrect_for_iron_tool",
            Self::Diamond => "incorrect_for_diamond_tool",
        }
    }
    /// The highest `needs` level ([`crate::blocks::Mining::needs`]) it harvests.
    pub fn level(self) -> u8 {
        match self {
            Self::Wood => 0,
            Self::Stone => 1,
            Self::Iron => 2,
            Self::Diamond => 3,
        }
    }
    /// The recipe ingredient its tools are made of.
    fn ingredient(self) -> &'static str {
        match self {
            Self::Wood => "#minecraft:planks",
            Self::Stone => "#minecraft:stone_tool_materials",
            Self::Iron => "minecraft:iron_ingot",
            Self::Diamond => "minecraft:diamond",
        }
    }
    /// The Minetest Game texture prefix.
    fn texture(self) -> &'static str {
        match self {
            Self::Wood => "wood",
            Self::Stone => "stone",
            Self::Iron => "steel",
            Self::Diamond => "diamond",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Plain,
    Food { nutrition: u32, saturation: f64 },
    Tool(ToolKind, Material),
}

#[derive(Clone, Debug)]
pub struct Item {
    /// The path of its `minecraft:` identifier.
    pub id: String,
    pub name: String,
    /// Its texture under `textures/` (`item/stick`).
    pub texture: String,
    pub kind: Kind,
    pub category: Category,
}

impl Item {
    pub fn key(&self) -> String {
        format!("minecraft:{}", self.id)
    }
    pub fn stack_size(&self) -> u32 {
        if matches!(self.kind, Kind::Tool(..)) {
            1
        } else {
            64
        }
    }
    /// Its default components, beyond the name and model `assets` adds.
    pub fn components(&self) -> serde_json::Map<String, Value> {
        let mut c = serde_json::Map::new();
        match self.kind {
            Kind::Plain => {}
            Kind::Food {
                nutrition,
                saturation,
            } => {
                c.insert(
                    "minecraft:food".into(),
                    json!({"nutrition": nutrition, "saturation": saturation}),
                );
                c.insert(
                    "minecraft:consumable".into(),
                    json!({"consume_seconds": 1.6, "animation": "eat", "sound": "minecraft:entity.generic.eat", "has_consume_particles": true}),
                );
            }
            Kind::Tool(tool, material) => {
                c.insert("minecraft:max_damage".into(), json!(material.durability()));
                c.insert("minecraft:damage".into(), json!(0));
                let (damage, speed) = attack(tool, material);
                c.insert(
                    "minecraft:attribute_modifiers".into(),
                    json!([
                        {"type": "minecraft:attack_damage", "id": "minecraft:base_attack_damage", "amount": damage, "operation": "add_value", "slot": "mainhand"},
                        {"type": "minecraft:attack_speed", "id": "minecraft:base_attack_speed", "amount": speed, "operation": "add_value", "slot": "mainhand"},
                    ]),
                );
                let rules = if tool == ToolKind::Sword {
                    json!([{"blocks": "#minecraft:sword_efficient", "speed": 1.5}])
                } else {
                    json!([
                        {"blocks": format!("#minecraft:{}", material.incorrect()), "correct_for_drops": false},
                        {"blocks": format!("#minecraft:mineable/{}", tool.id()), "correct_for_drops": true, "speed": material.speed()},
                    ])
                };
                c.insert(
                    "minecraft:tool".into(),
                    json!({"rules": rules, "damage_per_block": if tool == ToolKind::Sword { 2 } else { 1 }}),
                );
                if tool == ToolKind::Sword {
                    c.insert(
                        "minecraft:weapon".into(),
                        json!({"item_damage_per_attack": 1}),
                    );
                } else {
                    c.insert(
                        "minecraft:weapon".into(),
                        json!({"item_damage_per_attack": 2}),
                    );
                }
            }
        }
        c
    }
}

/// Attack damage (added to the hand's 1) and attack speed (added to 4).
fn attack(tool: ToolKind, material: Material) -> (f64, f64) {
    use Material::*;
    use ToolKind::*;
    match (tool, material) {
        (Sword, m) => (3.0 + f64::from(m.level()), -2.4),
        (Pickaxe, m) => (1.0 + f64::from(m.level()), -2.8),
        (Shovel, m) => (1.5 + f64::from(m.level()), -3.0),
        (Axe, Wood) => (6.0, -3.2),
        (Axe, Stone) => (8.0, -3.2),
        (Axe, Iron) => (8.0, -3.1),
        (Axe, Diamond) => (8.0, -3.0),
    }
}

/// Every item that is not a block, in palette order.
pub fn all() -> Vec<Item> {
    let plain = |id: &str, name: &str, texture: &str, category| Item {
        id: id.into(),
        name: name.into(),
        texture: format!("item/{texture}"),
        kind: Kind::Plain,
        category,
    };
    let mut items = Vec::new();
    for tool in ToolKind::ALL {
        for material in Material::ALL {
            let id = format!("{}_{}", material.id(), tool.id());
            let name = format!(
                "{} {}",
                material.title(),
                match tool {
                    ToolKind::Pickaxe => "Pickaxe",
                    ToolKind::Axe => "Axe",
                    ToolKind::Shovel => "Shovel",
                    ToolKind::Sword => "Sword",
                }
            );
            items.push(Item {
                texture: format!("item/{id}"),
                id,
                name,
                kind: Kind::Tool(tool, material),
                category: if tool == ToolKind::Sword {
                    Category::Combat
                } else {
                    Category::Tools
                },
            });
        }
    }
    items.push(Item {
        kind: Kind::Food {
            nutrition: 4,
            saturation: 2.4,
        },
        ..plain("apple", "Apple", "apple", Category::Food)
    });
    for (id, name) in [
        ("stick", "Stick"),
        ("coal", "Coal"),
        ("raw_copper", "Copper Lump"),
        ("raw_iron", "Iron Lump"),
        ("raw_gold", "Gold Lump"),
        ("copper_ingot", "Copper Ingot"),
        ("iron_ingot", "Iron Ingot"),
        ("gold_ingot", "Gold Ingot"),
        ("diamond", "Diamond"),
        ("clay_ball", "Clay Lump"),
        ("brick", "Clay Brick"),
    ] {
        items.push(plain(id, name, id, Category::Ingredients));
    }
    items
}

/// The Minetest Game file name (under `mods/default/textures/`) each item
/// texture comes from.
pub fn upstream_texture(item: &Item) -> String {
    match item.kind {
        Kind::Tool(tool, material) => {
            let short = match tool {
                ToolKind::Pickaxe => "pick",
                other => other.id(),
            };
            format!("default_tool_{}{short}.png", material.texture())
        }
        _ => match item.id.as_str() {
            "coal" => "default_coal_lump.png",
            "raw_copper" => "default_copper_lump.png",
            "raw_iron" => "default_iron_lump.png",
            "raw_gold" => "default_gold_lump.png",
            "iron_ingot" => "default_steel_ingot.png",
            "clay_ball" => "default_clay_lump.png",
            "brick" => "default_clay_brick.png",
            other => return format!("default_{other}.png"),
        }
        .to_owned(),
    }
}

/// The item tags recipes and the shared code read
/// (`data/minecraft/tags/item/<name>.json`).
pub fn item_tags(blocks: &[Block], items: &[Item]) -> Vec<(String, Vec<String>)> {
    let ids = |f: &dyn Fn(&Block) -> bool| -> Vec<String> {
        blocks
            .iter()
            .filter(|b| b.has_item() && f(b))
            .map(Block::key)
            .collect()
    };
    let tools = |kind: ToolKind| -> Vec<String> {
        items
            .iter()
            .filter(|i| matches!(i.kind, Kind::Tool(k, _) if k == kind))
            .map(Item::key)
            .collect()
    };
    vec![
        ("planks".into(), ids(&|b| b.id.ends_with("_planks"))),
        ("logs".into(), ids(&|b| b.id.ends_with("_log"))),
        ("leaves".into(), ids(&|b| b.id.ends_with("_leaves"))),
        ("saplings".into(), ids(&|b| b.id.ends_with("_sapling"))),
        ("wool".into(), ids(&|b| b.id.ends_with("_wool"))),
        (
            "stone_tool_materials".into(),
            vec![
                "minecraft:cobblestone".into(),
                "minecraft:mossy_cobblestone".into(),
            ],
        ),
        ("coals".into(), vec!["minecraft:coal".into()]),
        ("pickaxes".into(), tools(ToolKind::Pickaxe)),
        ("axes".into(), tools(ToolKind::Axe)),
        ("shovels".into(), tools(ToolKind::Shovel)),
        ("swords".into(), tools(ToolKind::Sword)),
    ]
}

fn shaped(
    pattern: &[&str],
    key: &[(char, &str)],
    result: &str,
    count: u32,
    category: &str,
) -> Value {
    let key: serde_json::Map<String, Value> =
        key.iter().map(|(c, v)| (c.to_string(), json!(v))).collect();
    json!({
        "type": "minecraft:crafting_shaped",
        "category": category,
        "pattern": pattern,
        "key": key,
        "result": {"id": format!("minecraft:{result}"), "count": count},
    })
}

fn shapeless(ingredients: &[&str], result: &str, count: u32, category: &str) -> Value {
    json!({
        "type": "minecraft:crafting_shapeless",
        "category": category,
        "ingredients": ingredients,
        "result": {"id": format!("minecraft:{result}"), "count": count},
    })
}

/// Every crafting recipe, by file name. There is no furnace in this edition:
/// what smelting would make is crafted with a piece of coal as the fuel.
pub fn recipes(blocks: &[Block]) -> Vec<(String, Value)> {
    let mut out: Vec<(String, Value)> = Vec::new();
    let m = |id: &str| format!("minecraft:{id}");
    for b in blocks
        .iter()
        .filter(|b| b.has_item() && b.id.ends_with("_log"))
    {
        let species = b.id.trim_end_matches("_log");
        // Stripped logs have no planks of their own.
        let planks = format!("{species}_planks");
        if !blocks.iter().any(|p| p.id == planks && p.has_item()) {
            continue;
        }
        out.push((
            format!("{species}_planks"),
            shapeless(&[&m(b.id)], &format!("{species}_planks"), 4, "building"),
        ));
    }
    out.push((
        "stick".into(),
        shaped(
            &["#", "#"],
            &[('#', "#minecraft:planks")],
            "stick",
            4,
            "misc",
        ),
    ));
    out.push((
        "crafting_table".into(),
        shaped(
            &["##", "##"],
            &[('#', "#minecraft:planks")],
            "crafting_table",
            1,
            "misc",
        ),
    ));
    for material in Material::ALL {
        let x = material.ingredient();
        let s = "minecraft:stick";
        for tool in ToolKind::ALL {
            let pattern: &[&str] = match tool {
                ToolKind::Pickaxe => &["XXX", " # ", " # "],
                ToolKind::Axe => &["XX", "X#", " #"],
                ToolKind::Shovel => &["X", "#", "#"],
                ToolKind::Sword => &["X", "X", "#"],
            };
            let id = format!("{}_{}", material.id(), tool.id());
            out.push((
                id.clone(),
                shaped(pattern, &[('X', x), ('#', s)], &id, 1, "equipment"),
            ));
        }
    }
    // Two-by-two building blocks.
    for (from, to) in [
        ("stone", "stone_bricks"),
        ("sand", "sandstone"),
        ("sandstone", "cut_sandstone"),
        ("cut_sandstone", "chiseled_sandstone"),
        ("red_sand", "red_sandstone"),
        ("red_sandstone", "cut_red_sandstone"),
        ("cut_red_sandstone", "chiseled_red_sandstone"),
        ("granite", "polished_granite"),
        ("clay_ball", "clay"),
        ("brick", "bricks"),
    ] {
        out.push((
            to.into(),
            shaped(
                &["##", "##"],
                &[('#', &m(from))],
                to,
                if from == "clay_ball" || from == "brick" {
                    1
                } else {
                    4
                },
                "building",
            ),
        ));
    }
    // Storage blocks and back.
    for (unit, block) in [
        ("coal", "coal_block"),
        ("copper_ingot", "copper_block"),
        ("iron_ingot", "iron_block"),
        ("gold_ingot", "gold_block"),
        ("diamond", "diamond_block"),
    ] {
        out.push((
            block.into(),
            shaped(
                &["###", "###", "###"],
                &[('#', &m(unit))],
                block,
                1,
                "building",
            ),
        ));
        out.push((
            format!("{unit}_from_{block}"),
            shapeless(&[&m(block)], unit, 9, "misc"),
        ));
    }
    // What a furnace would make, fired with coal.
    for (from, to) in [
        ("raw_copper", "copper_ingot"),
        ("raw_iron", "iron_ingot"),
        ("raw_gold", "gold_ingot"),
        ("sand", "glass"),
        ("red_sand", "glass"),
        ("cobblestone", "stone"),
        ("stone", "smooth_stone"),
        ("clay_ball", "brick"),
    ] {
        out.push((
            format!("{to}_from_{from}_with_coal"),
            shapeless(&[&m(from), "#minecraft:coals"], to, 1, "misc"),
        ));
    }
    out.push((
        "mossy_cobblestone".into(),
        shapeless(
            &["minecraft:cobblestone", "#minecraft:leaves"],
            "mossy_cobblestone",
            1,
            "building",
        ),
    ));
    out.push((
        "bookshelf".into(),
        shaped(
            &["###", "SSS", "###"],
            &[('#', "#minecraft:planks"), ('S', "minecraft:stick")],
            "bookshelf",
            1,
            "building",
        ),
    ));
    out.push((
        "glowstone".into(),
        shaped(
            &[" G ", "GCG", " G "],
            &[('G', "minecraft:glass"), ('C', "#minecraft:coals")],
            "glowstone",
            1,
            "building",
        ),
    ));
    out.push((
        "tinted_glass".into(),
        shaped(
            &[" O ", "OGO", " O "],
            &[('G', "minecraft:glass"), ('O', "minecraft:obsidian")],
            "tinted_glass",
            2,
            "building",
        ),
    ));
    out.push((
        "snow_block".into(),
        shapeless(
            &["minecraft:ice", "minecraft:ice"],
            "snow_block",
            2,
            "building",
        ),
    ));
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// One advancement that teaches every recipe on the first tick, so the
/// recipe book lists them all from the start.
pub fn advancement(recipes: &[(String, Value)]) -> Value {
    json!({
        "criteria": {"tick": {"trigger": "minecraft:tick"}},
        "requirements": [["tick"]],
        "rewards": {"recipes": recipes.iter().map(|(id, _)| format!("minecraft:{id}")).collect::<Vec<_>>()},
    })
}

/// The block's survival loot table, in the shared evaluator's format
/// (conditions name their kind under `type`, functions sit under `modifier`).
pub fn loot_table(block: &Block) -> Option<Value> {
    if block.tier == Tier::Support || block.is_air() || block.form == Form::Fluid {
        return None;
    }
    let explosion = json!({"type": "minecraft:survives_explosion"});
    let item = |id: String| json!({"type": "minecraft:item", "name": id});
    let pools = match block.drop {
        Drop::Nothing => return None,
        Drop::Itself => {
            let mut entry = item(block.key());
            if block.classes.contains(&"SlabBlock") {
                // A double slab gives two.
                entry["modifier"] = json!([{"type": "minecraft:set_count", "count": 2,
                    "conditions": [{"type": "minecraft:block_state_property", "block": block.key(), "properties": {"type": "double"}}]}]);
            }
            json!([{"rolls": 1.0, "entries": [entry], "conditions": [explosion]}])
        }
        Drop::Item { id, min, max, ore } => {
            let mut entry = item(format!("minecraft:{id}"));
            let mut modifiers = Vec::new();
            if min != 1 || max != 1 {
                modifiers.push(json!({"type": "minecraft:set_count", "count": if min == max { json!(min) } else { json!({"type": "minecraft:uniform", "min": min, "max": max}) }}));
            }
            if ore {
                modifiers.push(json!({"type": "minecraft:apply_bonus", "enchantment": "minecraft:fortune", "formula": "minecraft:ore_drops"}));
            }
            modifiers.push(json!({"type": "minecraft:explosion_decay"}));
            entry["modifier"] = json!(modifiers);
            json!([{"rolls": 1.0, "entries": [entry]}])
        }
        Drop::Leaves { sapling, apple } => {
            let mut pools = vec![
                json!({"rolls": 1.0, "entries": [{"type": "minecraft:item", "name": format!("minecraft:{sapling}"),
                    "condition": {"type": "minecraft:random_chance", "chance": 0.05}}], "conditions": [explosion]}),
                json!({"rolls": 1.0, "entries": [{"type": "minecraft:item", "name": "minecraft:stick",
                    "condition": {"type": "minecraft:random_chance", "chance": 0.04},
                    "modifier": [{"type": "minecraft:set_count", "count": {"type": "minecraft:uniform", "min": 1, "max": 2}}, {"type": "minecraft:explosion_decay"}]}]}),
            ];
            if apple {
                // Apples are the only food: oak leaves give them often enough to live on.
                pools.push(json!({"rolls": 1.0, "entries": [{"type": "minecraft:item", "name": "minecraft:apple",
                    "condition": {"type": "minecraft:random_chance", "chance": 0.05}}], "conditions": [explosion]}));
            }
            json!(pools)
        }
    };
    Some(json!({"type": "minecraft:block", "pools": pools}))
}

const GENERATOR: &str = "items";

/// The recipes, the item tags and the advancement that teaches the recipes.
pub fn write(bundle: &mut crate::Bundle, blocks: &[Block], items: &[Item]) -> crate::Result<()> {
    let recipes = recipes(blocks);
    for (name, recipe) in &recipes {
        bundle.json(
            format!("data/minecraft/recipe/{name}.json"),
            recipe,
            GENERATOR,
        )?;
    }
    bundle.json(
        "data/minecraft/advancement/recipes/open_all.json",
        &advancement(&recipes),
        GENERATOR,
    )?;
    for (name, values) in item_tags(blocks, items) {
        bundle.json(
            format!("data/minecraft/tags/item/{name}.json"),
            &json!({ "values": values }),
            GENERATOR,
        )?;
    }
    Ok(())
}

const DEFAULTS: &str = include_str!("../data/items/defaults.json");

/// One of Minecraft's items that is not a block, with Minecraft's default
/// components (`data/items/defaults.json`, written by
/// `tools/open_item_defaults.py`).
#[derive(Clone, Debug)]
pub struct VanillaItem {
    /// The path of its `minecraft:` identifier.
    pub id: &'static str,
    /// Drawn as a tool in the hand (`item/handheld`), else flat.
    pub handheld: bool,
    /// Its texture under `textures/` (`item/rotten_flesh`).
    pub texture: String,
    /// Its default components, without the name and model `assets` adds.
    pub components: serde_json::Map<String, Value>,
}

impl VanillaItem {
    pub fn key(&self) -> String {
        format!("minecraft:{}", self.id)
    }
    /// Its English name, from its identifier ("Rotten Flesh").
    pub fn name(&self) -> String {
        self.id
            .split('_')
            .map(|w| {
                let mut c = w.chars();
                c.next()
                    .map(|f| f.to_uppercase().chain(c).collect())
                    .unwrap_or_default()
            })
            .collect::<Vec<String>>()
            .join(" ")
    }
}

/// Minecraft's items that are not blocks and that [`all`] does not define,
/// in registry order.
pub fn vanilla() -> Vec<VanillaItem> {
    let data: Value = serde_json::from_str(DEFAULTS).expect("data/items/defaults.json");
    let base = data["base"].as_object().cloned().unwrap_or_default();
    let own: std::collections::HashSet<String> = all().into_iter().map(|i| i.id).collect();
    crate::numbering::names("item")
        .iter()
        .filter(|id| !own.contains(**id))
        .filter_map(|id| {
            let entry = data["items"].get(*id)?;
            let mut components = base.clone();
            for (k, v) in entry["components"].as_object()? {
                if v.is_null() {
                    components.remove(k);
                } else {
                    components.insert(k.clone(), v.clone());
                }
            }
            Some(VanillaItem {
                id,
                handheld: entry["handheld"].as_bool().unwrap_or(false),
                texture: entry["texture"]
                    .as_str()
                    .map_or_else(|| format!("item/{id}"), str::to_owned),
                components,
            })
        })
        .collect()
}

/// Minecraft's default components of a block's item (`blocks` in
/// `data/items/defaults.json`), without the name and model `assets` adds;
/// none for a name that has no block item there.
pub fn block_item_components(id: &str) -> Option<serde_json::Map<String, Value>> {
    static DATA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    let data =
        DATA.get_or_init(|| serde_json::from_str(DEFAULTS).expect("data/items/defaults.json"));
    let entry = data["blocks"].get(id)?;
    let mut components = data["base"].as_object().cloned().unwrap_or_default();
    for (k, v) in entry["components"].as_object()? {
        if v.is_null() {
            components.remove(k);
        } else {
            components.insert(k.clone(), v.clone());
        }
    }
    Some(components)
}

#[cfg(test)]
mod vanilla_tests {
    #[test]
    fn block_items_have_minecraft_defaults() {
        let planks = super::block_item_components("oak_planks").unwrap();
        assert_eq!(planks["minecraft:max_stack_size"], 64);
        assert_eq!(
            planks["minecraft:cooking_fuel"]["burn_time"],
            "minecraft:cooking/time_wood_blocks"
        );
        assert_eq!(
            planks["minecraft:break_sound"],
            "minecraft:entity.item.break"
        );
        let bed = super::block_item_components("white_bed").unwrap();
        assert_eq!(bed["minecraft:max_stack_size"], 1);
        assert!(
            super::block_item_components("stick").is_none(),
            "not a block"
        );
    }

    #[test]
    fn rotten_flesh_has_minecraft_food_values() {
        let items = super::vanilla();
        let flesh = items.iter().find(|i| i.id == "rotten_flesh").unwrap();
        assert_eq!(flesh.name(), "Rotten Flesh");
        assert_eq!(flesh.texture, "item/rotten_flesh");
        assert!(!flesh.handheld);
        let food = &flesh.components["minecraft:food"];
        assert_eq!(
            (food["nutrition"].as_i64(), food["saturation"].as_f64()),
            (Some(4), Some(0.8))
        );
        assert_eq!(flesh.components["minecraft:max_stack_size"], 64);
        let sword = items.iter().find(|i| i.id == "golden_sword").unwrap();
        assert!(sword.handheld);
        assert_eq!(sword.components["minecraft:max_stack_size"], 1);
        // The open edition's own items keep their definitions.
        assert!(items.iter().all(|i| i.id != "apple" && i.id != "stick"));
        assert!(items.len() > 450, "{}", items.len());
    }
}
