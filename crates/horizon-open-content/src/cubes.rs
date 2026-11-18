//! The full-cube families, from `data/blocks/cubes.txt` (written by
//! `tools/open_block_facts.py`): every single-state block of class `Block`,
//! every `RotatedPillarBlock`, the three furnaces, and every other block all
//! of whose states draw one plain cube (ores, glass, leaves, copper, sand,
//! netherrack, pumpkins), with Minecraft's
//! numbers (destroy time, resistance, light, map colour, friction, sound
//! type, mining tool) and the texture names of their faces. A block this
//! crate already authors keeps its name, tab and drop and takes the numbers
//! and faces; the others are added to the palette, dropping themselves.
use crate::blocks::{Block, Category, Drop, Faces, Form, Layer, Mining, Sound, Tier, Tool};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::OnceLock;

const CUBES: &str = include_str!("../data/blocks/cubes.txt");

/// One line of `cubes.txt`.
#[derive(Debug)]
pub struct Cube {
    pub id: &'static str,
    pub classes: &'static [&'static str],
    pub hardness: f32,
    pub resistance: f32,
    pub light: u8,
    pub lit_light: u8,
    pub opacity: u8,
    pub friction: f32,
    pub map_color: u32,
    pub sound: &'static str,
    pub mining: Mining,
    pub faces: Faces,
    /// `cube`, `clear` (light shows through), `leaves` (and its faces carry
    /// a tint index), `glass` (and its sprites are
    /// drawn translucent), `spin_y` / `spin_xy` (turned at random as sand
    /// and netherrack), `axis`, `pillar` or `facing`.
    pub layout: &'static str,
}

impl Cube {
    /// Light shows through it (glass, leaves, ice, grates).
    pub fn clear(&self) -> bool {
        matches!(self.layout, "clear" | "leaves" | "glass")
    }
}

fn layouts() -> &'static HashMap<&'static str, &'static str> {
    static LAYOUTS: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    LAYOUTS.get_or_init(|| cubes().into_iter().map(|c| (c.id, c.layout)).collect())
}

/// The turns (x, y) of the models Minecraft picks among at random for the
/// block, in its order; empty for a block drawn unturned.
pub fn turns(id: &str) -> Vec<(u16, u16)> {
    let quarter = [0, 90, 180, 270];
    match layouts().get(id).copied() {
        Some("spin_y") => quarter.iter().map(|&y| (0, y)).collect(),
        Some("spin_xy") => quarter
            .iter()
            .flat_map(|&y| quarter.iter().map(move |&x| (x, y)))
            .collect(),
        _ => Vec::new(),
    }
}

/// Its sprites are drawn translucent (Minecraft's `force_translucent`).
pub fn translucent(id: &str) -> bool {
    layouts().get(id) == Some(&"glass")
}

/// Marks a cube model's faces tinted (leaves) and its sprites translucent
/// (glass), as Minecraft's models of the block do. Minecraft's leaves model
/// tints every face; the client colours only the leaves it names
/// (`colored`), and those only when `tint` (the pack's grey art).
pub fn dress(id: &str, model: &mut Value, colored: bool, tint: bool) {
    let leaves = layouts().get(id) == Some(&"leaves");
    if (tint || leaves && !colored)
        && let Some(faces) = model["elements"][0]["faces"].as_object_mut()
    {
        for face in faces.values_mut() {
            face["tintindex"] = json!(0);
        }
    }
    if translucent(id)
        && let Some(textures) = model["textures"].as_object_mut()
    {
        for texture in textures.values_mut() {
            *texture = json!({"sprite": texture.clone(), "force_translucent": true});
        }
    }
}

/// The block-state variant of an unturned cube: its model, or the list of
/// its turns Minecraft picks among.
pub fn variants(id: &str, model: &str) -> Value {
    let turns = turns(id);
    if turns.is_empty() {
        return json!({ "model": model });
    }
    let list: Vec<Value> = turns
        .into_iter()
        .map(|(x, y)| {
            let mut v = json!({ "model": model });
            if x != 0 {
                v["x"] = json!(x);
            }
            if y != 0 {
                v["y"] = json!(y);
            }
            v
        })
        .collect();
    Value::Array(list)
}

fn leak<T>(v: Vec<T>) -> &'static [T] {
    Box::leak(v.into_boxed_slice())
}

/// Every line, in block registry order.
pub fn cubes() -> Vec<Cube> {
    CUBES
        .lines()
        .skip(1)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let f: Vec<&'static str> = line.split('\t').collect();
            assert_eq!(f.len(), 21, "cubes.txt: {line}");
            let num = |i: usize| -> f32 {
                f[i].parse()
                    .unwrap_or_else(|_| panic!("cubes.txt: {} field {i}", f[0]))
            };
            let tool = match f[10] {
                "pickaxe" => Tool::Pickaxe,
                "axe" => Tool::Axe,
                "shovel" => Tool::Shovel,
                "hoe" => Tool::Hoe,
                _ => Tool::None,
            };
            let six = [f[14], f[15], f[16], f[17], f[18], f[19]];
            let faces = match f[13] {
                "facing" => Faces::Facing {
                    faces: six,
                    lit_north: f[20],
                },
                "pillar" => Faces::Pillar(six),
                _ => Faces::Cube(six),
            };
            Cube {
                id: f[0],
                classes: leak(f[1].split(',').collect()),
                hardness: num(2),
                resistance: num(3),
                light: num(4) as u8,
                lit_light: num(5) as u8,
                opacity: num(6) as u8,
                friction: num(7),
                map_color: u32::from_str_radix(f[8], 16).expect("cubes.txt: map colour"),
                sound: f[9],
                mining: Mining {
                    tool,
                    requires_tool: f[12] == "1",
                    needs: num(11) as u8,
                },
                faces,
                layout: f[13],
            }
        })
        .collect()
}

/// The sound types the authored blocks name by variant.
pub(crate) fn sound(name: &'static str) -> Sound {
    match name {
        "stone" => Sound::Stone,
        "gravel" => Sound::Gravel,
        "sand" => Sound::Sand,
        "grass" => Sound::Grass,
        "wood" => Sound::Wood,
        "glass" => Sound::Glass,
        "wool" => Sound::Wool,
        other => Sound::Named(other),
    }
}

/// `blast_furnace` as "Blast Furnace".
pub(crate) fn title(id: &str) -> &'static str {
    let words: Vec<String> = id
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|first| first.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        })
        .collect();
    Box::leak(words.join(" ").into_boxed_str())
}

pub(crate) const COLORS: &[&str] = &[
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

/// The palette tab of a block added here.
fn category(cube: &Cube) -> Category {
    let id = cube.id;
    let natural = [
        "granite",
        "diorite",
        "andesite",
        "deepslate",
        "tuff",
        "calcite",
        "netherrack",
        "end_stone",
        "basalt",
        "blackstone",
        "dripstone_block",
        "soul_soil",
        "packed_mud",
        "packed_ice",
        "blue_ice",
        "snow_block",
        "clay",
        "bedrock",
        "muddy_mangrove_roots",
        "pale_moss_block",
        "sand",
        "red_sand",
        "gravel",
        "magma_block",
        "crimson_nylium",
        "warped_nylium",
        "moss_block",
        "rooted_dirt",
        "ice",
    ];
    if COLORS.iter().any(|c| id.starts_with(&format!("{c}_"))) {
        Category::Colored
    } else if id.contains("ore") && !id.contains("core")
        || id.starts_with("raw_")
        || [
            "emerald_block",
            "lapis_block",
            "netherite_block",
            "amethyst_block",
            "ancient_debris",
        ]
        .contains(&id)
    {
        Category::Minerals
    } else if natural.contains(&id) {
        Category::Ground
    } else if id.ends_with("_leaves")
        || cube.classes[0] == "RotatedPillarBlock" && cube.mining.tool == Tool::Hoe
    {
        Category::Plants
    } else {
        Category::Construction
    }
}

/// Applies the families' numbers to the authored blocks and appends the
/// others, in the file's order.
pub fn add(blocks: &mut Vec<Block>) {
    for cube in cubes() {
        if let Some(block) = blocks.iter_mut().find(|b| b.id == cube.id) {
            set_facts(block, &cube);
            continue;
        }
        let mut block = Block {
            id: cube.id,
            name: title(cube.id),
            tier: Tier::Palette,
            form: Form::Cube,
            layer: Layer::Solid,
            faces: cube.faces,
            properties: &[],
            defaults: &[],
            hardness: 0.0,
            resistance: 0.0,
            light: 0,
            lit_light: 0,
            opacity: 15,
            sound: Sound::Stone,
            classes: &[],
            map_color: 0,
            category: Some(category(&cube)),
            mining: cube.mining,
            drop: Drop::Itself,
            friction: 0.6,
        };
        if let Some(base) = id_base(cube.id).and_then(|b| blocks.iter().find(|x| x.id == b)) {
            // A deepslate ore drops what its stone ore drops.
            block.drop = base.drop;
        }
        if cube.id.ends_with("_leaves") {
            // Its sapling is not content yet.
            block.drop = Drop::Nothing;
        }
        set_facts(&mut block, &cube);
        blocks.push(block);
    }
}

/// `iron_ore` for `deepslate_iron_ore`.
fn id_base(id: &str) -> Option<&str> {
    id.strip_prefix("deepslate_")
        .filter(|b| b.ends_with("_ore"))
}

fn set_facts(block: &mut Block, cube: &Cube) {
    block.faces = cube.faces;
    if cube.clear() {
        block.form = Form::SeeThroughCube;
        block.layer = if cube.layout == "glass" {
            Layer::Translucent
        } else {
            Layer::Cutout
        };
    }
    block.classes = cube.classes;
    block.hardness = cube.hardness;
    block.resistance = cube.resistance;
    block.light = cube.light;
    block.lit_light = cube.lit_light;
    block.opacity = cube.opacity;
    block.friction = cube.friction;
    block.map_color = cube.map_color;
    block.sound = sound(cube.sound);
    block.mining = cube.mining;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_families_are_all_listed_with_minecraft_numbers() {
        let cubes = cubes();
        let count = |class: &str| cubes.iter().filter(|c| c.classes[0] == class).count();
        assert_eq!(count("Block"), 182);
        assert_eq!(count("RotatedPillarBlock"), 60);
        let furnace = cubes.iter().find(|c| c.id == "furnace").unwrap();
        assert_eq!(
            (furnace.hardness, furnace.light, furnace.lit_light),
            (3.5, 0, 13)
        );
        assert!(furnace.classes.contains(&"BaseEntityBlock"));
        let obsidian = cubes.iter().find(|c| c.id == "obsidian").unwrap();
        assert_eq!((obsidian.resistance, obsidian.mining.needs), (1200.0, 3));
        let layout = |l: &str| cubes.iter().filter(|c| c.layout == l).count();
        assert_eq!(
            [
                layout("clear") + layout("leaves"),
                layout("glass"),
                layout("spin_y"),
                layout("spin_xy")
            ],
            [24, 17, 19, 1]
        );
        let ore = cubes.iter().find(|c| c.id == "redstone_ore").unwrap();
        assert_eq!((ore.light, ore.lit_light), (0, 9));
        assert_eq!(turns("netherrack").len(), 16);
        assert_eq!(turns("sand")[1], (0, 90));
        assert!(translucent("white_stained_glass") && !translucent("oak_leaves"));
    }

    #[test]
    fn every_family_block_is_content_in_the_palette() {
        let blocks = crate::blocks::defined();
        for cube in cubes() {
            let block = blocks.iter().find(|b| b.id == cube.id).unwrap();
            assert_eq!(block.tier, Tier::Palette, "{}", cube.id);
            let form = if cube.clear() {
                Form::SeeThroughCube
            } else {
                Form::Cube
            };
            assert!(block.form == form, "{}", cube.id);
            assert!(!block.textures().is_empty(), "{}", cube.id);
        }
        assert_eq!(title("blast_furnace"), "Blast Furnace");
    }
}
