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
    /// How much it slows walking (soul sand 0.4, else 1).
    pub speed_factor: f32,
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

const VARIANTS: &str = include_str!("../data/blocks/cube_variants.txt");

/// One block-state variant, or multipart part, of a `variants` block
/// (`cube_variants.txt`).
struct Variant {
    block: &'static str,
    /// A multipart part (mushroom blocks) rather than a variant.
    part: bool,
    /// The state key (`facing=north,open=true`) or the part's condition;
    /// empty for every state.
    key: &'static str,
    model: &'static str,
    x: u16,
    y: u16,
    uvlock: bool,
}

/// A model of `cube_variants.txt`: its box's corners and its faces (down,
/// up, north, south, west, east; `-` for a face it lacks).
type BoxModel = [&'static str; 8];

/// The `variants` blocks' variants, and their models by name.
type VariantTables = (Vec<Variant>, HashMap<&'static str, BoxModel>);

fn variant_tables() -> &'static VariantTables {
    static TABLES: OnceLock<VariantTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let (mut variants, mut models) = (Vec::new(), HashMap::new());
        for line in VARIANTS.lines().filter(|l| !l.is_empty()) {
            let f: Vec<&'static str> = line.split('\t').collect();
            let num = |s: &str| -> u16 { s.parse().expect("cube_variants.txt: a turn") };
            if f[0] == "model" {
                assert_eq!(f.len(), 10, "cube_variants.txt: {line}");
                models.insert(f[1], [f[2], f[3], f[4], f[5], f[6], f[7], f[8], f[9]]);
                continue;
            }
            assert_eq!(f.len(), 7, "cube_variants.txt: {line}");
            variants.push(Variant {
                block: f[1],
                part: f[0] == "part",
                key: if f[2] == "-" { "" } else { f[2] },
                model: f[3],
                x: num(f[4]),
                y: num(f[5]),
                uvlock: f[6] == "1",
            });
        }
        (variants, models)
    })
}

/// A face of `cube_variants.txt`: `texture/u1,v1,u2,v2/rotation/tintindex/cullface`
/// (`-` for an unset UV or cull face).
fn variant_face(spec: &'static str) -> (&'static str, Value) {
    let p: Vec<&str> = spec.split('/').collect();
    assert_eq!(p.len(), 5, "cube_variants.txt: face {spec}");
    let mut face = json!({});
    if p[1] != "-" {
        let uv: Vec<f64> = p[1].split(',').map(|v| v.parse().unwrap()).collect();
        face["uv"] = json!(uv);
    }
    let rotation: u16 = p[2].parse().unwrap();
    if rotation != 0 {
        face["rotation"] = json!(rotation);
    }
    let tint: i32 = p[3].parse().unwrap();
    if tint >= 0 {
        face["tintindex"] = json!(tint);
    }
    if p[4] != "-" {
        face["cullface"] = json!(p[4]);
    }
    (p[0], face)
}

/// Where a `variants` model is written (`block/cube/<block>/<model>`: its
/// names are Minecraft's, some of them other blocks' names or shared).
fn variant_model_path(block: &str, model: &str) -> String {
    format!("block/cube/{block}/{model}")
}

fn variant_model(model: &BoxModel) -> Value {
    let corner = |s: &str| -> Vec<f64> { s.split(',').map(|v| v.parse().unwrap()).collect() };
    let dirs = ["down", "up", "north", "south", "west", "east"];
    let mut textures = serde_json::Map::new();
    let mut faces = serde_json::Map::new();
    for (dir, spec) in dirs.iter().zip(&model[2..]) {
        if *spec == "-" {
            continue;
        }
        let (texture, mut face) = variant_face(spec);
        textures.insert((*dir).into(), json!(format!("minecraft:block/{texture}")));
        face["texture"] = json!(format!("#{dir}"));
        faces.insert((*dir).into(), face);
    }
    let particle = textures.values().next().cloned().unwrap_or(Value::Null);
    textures.insert("particle".into(), particle);
    json!({
        "parent": "minecraft:block/open_base",
        "textures": textures,
        "elements": [{"from": corner(model[0]), "to": corner(model[1]), "faces": faces}],
    })
}

/// A `variants` block's models (its default one again as `block/<id>`, for
/// its item) and its block-state definition; None for other blocks.
pub fn variant_models(block: &Block) -> Option<(Vec<(String, Value)>, Option<Value>)> {
    if layouts().get(block.id) != Some(&"variants") {
        return None;
    }
    let (variants, models) = variant_tables();
    let mine: Vec<&Variant> = variants.iter().filter(|v| v.block == block.id).collect();
    let mut out = Vec::new();
    let mut definition = serde_json::Map::new();
    let mut parts = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for v in &mine {
        if seen.insert(v.model) {
            out.push((
                variant_model_path(block.id, v.model),
                variant_model(&models[v.model]),
            ));
        }
        let path = variant_model_path(block.id, v.model);
        let mut entry = json!({ "model": format!("minecraft:{path}") });
        if v.x != 0 {
            entry["x"] = json!(v.x);
        }
        if v.y != 0 {
            entry["y"] = json!(v.y);
        }
        if v.uvlock {
            entry["uvlock"] = json!(true);
        }
        if v.part {
            let when: serde_json::Map<String, Value> = v
                .key
                .split(',')
                .filter_map(|c| c.split_once('='))
                .map(|(k, val)| (k.to_owned(), json!(val)))
                .collect();
            parts.push(json!({"when": when, "apply": entry}));
            continue;
        }
        match definition.get_mut(v.key) {
            Some(Value::Array(list)) => list.push(entry),
            Some(first) => *first = json!([first.clone(), entry]),
            None => {
                definition.insert(v.key.into(), entry);
            }
        }
    }
    // The item shows the default state's model, or (a multipart) the
    // block's faces as a cube.
    let default = mine.iter().find(|v| {
        !v.part
            && v.key.split(',').filter(|c| !c.is_empty()).all(|c| {
                c.split_once('=')
                    .is_some_and(|(k, val)| block.defaults.contains(&(k, val)))
            })
    });
    let item = match (default, block.faces) {
        (Some(v), _) => variant_model(&models[v.model]),
        (None, Faces::Cube(faces)) => {
            let specs: Vec<&'static str> = ["down", "up", "north", "south", "west", "east"]
                .iter()
                .zip(faces)
                .map(|(d, t)| &*Box::leak(format!("{t}/-/0/-1/{d}").into_boxed_str()))
                .collect();
            let mut model: BoxModel = ["0,0,0", "16,16,16", "", "", "", "", "", ""];
            model[2..].copy_from_slice(&specs);
            variant_model(&model)
        }
        (None, _) => return None,
    };
    out.push((format!("block/{}", block.id), item));
    let definition = if parts.is_empty() {
        json!({ "variants": definition })
    } else {
        json!({ "multipart": parts })
    };
    Some((out, Some(definition)))
}

/// Every texture a `variants` block's models use.
pub fn variant_textures(id: &str) -> Vec<&'static str> {
    let (variants, models) = variant_tables();
    let mut out: Vec<&'static str> = variants
        .iter()
        .filter(|v| v.block == id)
        .flat_map(|v| models[v.model][2..].iter().filter(|s| **s != "-"))
        .map(|spec| variant_face(spec).0)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
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
            assert_eq!(f.len(), 22, "cubes.txt: {line}");
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
                speed_factor: num(21),
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
        "mycelium",
        "podzol",
        "sculk",
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

/// The speed factor of a block of this file (1 for the others).
pub fn speed_factor(id: &str) -> f32 {
    static SPEEDS: OnceLock<HashMap<&'static str, f32>> = OnceLock::new();
    let speeds = SPEEDS.get_or_init(|| {
        cubes()
            .into_iter()
            .filter(|c| c.speed_factor != 1.0)
            .map(|c| (c.id, c.speed_factor))
            .collect()
    });
    speeds.get(id).copied().unwrap_or(1.0)
}

/// The shapes of a `sunken` cube (soul sand, mud): a full outline and
/// occlusion over a collision box 14 sixteenths tall
/// (`SoulSandBlock.SHAPE`, `MudBlock.SHAPE`).
pub fn sunken() -> crate::shapes::StateShape {
    use crate::shapes::{StateShape, cuboid, full};
    StateShape {
        collision: cuboid(0.0, 0.0, 0.0, 16.0, 14.0, 16.0),
        ..StateShape::solid(full(), false, false)
    }
}

fn set_facts(block: &mut Block, cube: &Cube) {
    block.faces = cube.faces;
    if cube.layout == "sunken" {
        block.form = Form::Shaped;
    }
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
            let form = if cube.layout == "sunken" {
                Form::Shaped
            } else if cube.clear() {
                Form::SeeThroughCube
            } else {
                Form::Cube
            };
            assert!(block.form == form, "{}", cube.id);
            assert!(!block.textures().is_empty(), "{}", cube.id);
        }
        assert_eq!(title("blast_furnace"), "Blast Furnace");
    }

    #[test]
    fn variant_blocks_keep_minecraft_states_models_and_turns() {
        assert_eq!(layouts().values().filter(|l| **l == "variants").count(), 50);
        let blocks = crate::blocks::defined();
        let block = |id: &str| blocks.iter().find(|b| b.id == id).unwrap();
        let (models, definition) = variant_models(block("white_glazed_terracotta")).unwrap();
        let definition = definition.unwrap();
        assert_eq!(definition["variants"]["facing=north"]["y"], 180);
        assert!(definition["variants"]["facing=south"]["y"].is_null());
        // Its own model, and the default one again for the item.
        assert_eq!(models.len(), 2);
        let (_, definition) = variant_models(block("observer")).unwrap();
        assert_eq!(
            definition.unwrap()["variants"].as_object().unwrap().len(),
            12
        );
        let (_, definition) = variant_models(block("sculk")).unwrap();
        assert_eq!(
            definition.unwrap()["variants"][""]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        assert!(variant_textures("mycelium").contains(&"grass_block_snow"));
        let (_, definition) = variant_models(block("red_mushroom_block")).unwrap();
        assert_eq!(
            definition.unwrap()["multipart"].as_array().unwrap().len(),
            12
        );
        assert!(variant_models(block("stone")).is_none());
    }
}
