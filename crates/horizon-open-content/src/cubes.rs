//! The full-cube families, from `data/blocks/cubes.txt` (written by
//! `tools/open_block_facts.py`): every single-state block of class `Block`,
//! every `RotatedPillarBlock` and the three furnaces, with Minecraft's
//! numbers (destroy time, resistance, light, map colour, friction, sound
//! type, mining tool) and the texture names of their faces. A block this
//! crate already authors keeps its name, faces, tab and drop and takes the
//! numbers; the others are added to the palette, dropping themselves.
use crate::blocks::{Block, Category, Drop, Faces, Form, Layer, Mining, Sound, Tier, Tool};

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
            }
        })
        .collect()
}

/// The sound types the authored blocks name by variant.
fn sound(name: &'static str) -> Sound {
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
fn title(id: &str) -> &'static str {
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

const COLORS: &[&str] = &[
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
    } else if cube.classes[0] == "RotatedPillarBlock" && cube.mining.tool == Tool::Hoe {
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
        set_facts(&mut block, &cube);
        blocks.push(block);
    }
}

fn set_facts(block: &mut Block, cube: &Cube) {
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
    }

    #[test]
    fn every_family_block_is_content_in_the_palette() {
        let blocks = crate::blocks::defined();
        for cube in cubes() {
            let block = blocks.iter().find(|b| b.id == cube.id).unwrap();
            assert_eq!(block.tier, Tier::Palette, "{}", cube.id);
            assert!(block.form == Form::Cube, "{}", cube.id);
            assert!(!block.textures().is_empty(), "{}", cube.id);
        }
        assert_eq!(title("blast_furnace"), "Blast Furnace");
    }
}
