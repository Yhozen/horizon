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
    /// Its lit light by candle count (candles), else its one lit light.
    pub lit_by_count: &'static [u8],
    /// The default state's facts the open tables cannot derive for a
    /// `shaped` block: piston reaction, replaceable, random ticks, catches
    /// fire from lava, has a block entity, and its item.
    pub push: &'static str,
    pub replaceable: bool,
    pub random_ticks: bool,
    pub ignited_by_lava: bool,
    pub block_entity: bool,
    pub item: &'static str,
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

/// A model of `cube_variants.txt`: whether it uses ambient occlusion, and its
/// elements, each `from;to;rotation;shade;down;up;north;south;west;east`
/// (see `tools/open_block_facts.py`).
struct BoxModel {
    ao: bool,
    elements: Vec<&'static str>,
}

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
                assert!(f.len() >= 3, "cube_variants.txt: {line}");
                let model = BoxModel {
                    ao: f[2] == "1",
                    elements: f[3..].to_vec(),
                };
                models.insert(f[1], model);
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

/// An element's faces (down, up, north, south, west, east; `-` for one it
/// lacks).
fn element_faces(element: &'static str) -> impl Iterator<Item = &'static str> {
    element.split(';').skip(4).filter(|s| *s != "-")
}

/// Where a `variants` model is written (`block/cube/<block>/<model>`: its
/// names are Minecraft's, some of them other blocks' names or shared).
fn variant_model_path(block: &str, model: &str) -> String {
    format!("block/cube/{block}/{model}")
}

fn variant_model(model: &BoxModel) -> Value {
    let numbers = |s: &str| -> Vec<f64> { s.split(',').map(|v| v.parse().unwrap()).collect() };
    let dirs = ["down", "up", "north", "south", "west", "east"];
    // One texture variable per texture, in the order the faces name them.
    let mut names: Vec<&str> = Vec::new();
    let mut elements = Vec::new();
    for element in &model.elements {
        let p: Vec<&'static str> = element.split(';').collect();
        assert_eq!(p.len(), 10, "cube_variants.txt: element {element}");
        let mut faces = serde_json::Map::new();
        for (dir, spec) in dirs.iter().zip(&p[4..]) {
            if *spec == "-" {
                continue;
            }
            let (texture, mut face) = variant_face(spec);
            let i = names.iter().position(|n| *n == texture).unwrap_or_else(|| {
                names.push(texture);
                names.len() - 1
            });
            face["texture"] = json!(format!("#t{i}"));
            faces.insert((*dir).into(), face);
        }
        let mut out = json!({"from": numbers(p[0]), "to": numbers(p[1]), "faces": faces});
        if p[2] != "-" {
            let r: Vec<&str> = p[2].split(',').collect();
            out["rotation"] = json!({
                "origin": numbers(&r[..3].join(",")),
                "axis": r[3],
                "angle": r[4].parse::<f64>().unwrap(),
                "rescale": r[5] == "1",
            });
        }
        if p[3] == "0" {
            out["shade"] = json!(false);
        }
        elements.push(out);
    }
    let mut textures: serde_json::Map<String, Value> = names
        .iter()
        .enumerate()
        .map(|(i, t)| (format!("t{i}"), json!(format!("minecraft:block/{t}"))))
        .collect();
    let particle = textures.get("t0").cloned().unwrap_or(Value::Null);
    textures.insert("particle".into(), particle);
    let mut out = json!({
        "parent": "minecraft:block/open_base",
        "textures": textures,
        "elements": elements,
    });
    if !model.ao {
        out["ambientocclusion"] = json!(false);
    }
    out
}

/// A multipart condition: `k=v,k=v`, alternatives joined by ` or `.
fn condition(key: &str) -> Value {
    let alternatives: Vec<Value> = key
        .split(" or ")
        .map(|a| {
            let map: serde_json::Map<String, Value> = a
                .split(',')
                .filter_map(|c| c.split_once('='))
                .map(|(k, val)| (k.to_owned(), json!(val)))
                .collect();
            Value::Object(map)
        })
        .collect();
    if alternatives.len() == 1 {
        alternatives.into_iter().next().unwrap()
    } else {
        json!({ "OR": alternatives })
    }
}

/// A `variants` block's models (its default one again as `block/<id>`, for
/// its item) and its block-state definition; None for other blocks.
pub fn variant_models(block: &Block) -> Option<(Vec<(String, Value)>, Option<Value>)> {
    if !matches!(layouts().get(block.id), Some(&"variants" | &"shaped")) {
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
            let mut part = json!({ "apply": entry });
            if !v.key.is_empty() {
                part["when"] = condition(v.key);
            }
            parts.push(part);
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
            let specs: Vec<String> = ["down", "up", "north", "south", "west", "east"]
                .iter()
                .zip(faces)
                .map(|(d, t)| format!("{t}/-/0/-1/{d}"))
                .collect();
            let element = format!("0,0,0;16,16,16;-;1;{}", specs.join(";"));
            variant_model(&BoxModel {
                ao: true,
                elements: vec![Box::leak(element.into_boxed_str())],
            })
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
        .flat_map(|v| {
            models[v.model]
                .elements
                .iter()
                .copied()
                .flat_map(element_faces)
        })
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
            assert_eq!(f.len(), 28, "cubes.txt: {line}");
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
                lit_light: f[5].split(',').next().unwrap().parse().unwrap(),
                lit_by_count: leak(f[5].split(',').map(|v| v.parse().unwrap()).collect()),
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
                push: f[22],
                replaceable: f[23] == "1",
                random_ticks: f[24] == "1",
                ignited_by_lava: f[25] == "1",
                block_entity: f[26] == "1",
                item: f[27],
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

/// The facts of a `shaped` block's states the open tables cannot derive
/// (Minecraft's, from `cubes.txt`).
pub fn patch_facts(row: &mut Value, id: &str, properties: &[(&str, &str)]) {
    static SHAPED: OnceLock<HashMap<&'static str, Cube>> = OnceLock::new();
    let shaped = SHAPED.get_or_init(|| {
        cubes()
            .into_iter()
            .filter(|c| c.layout == "shaped")
            .map(|c| (c.id, c))
            .collect()
    });
    let Some(cube) = shaped.get(id) else {
        return;
    };
    row["push"] = json!(cube.push);
    row["replaceable"] = json!(cube.replaceable);
    row["random_ticks"] = json!(cube.random_ticks);
    row["ignited_by_lava"] = json!(cube.ignited_by_lava);
    row["block_entity"] = json!(cube.block_entity);
    row["item"] = json!(format!("minecraft:{}", cube.item));
    let value = |key: &str| crate::shapes::value(properties, key);
    if cube.classes.contains(&"MossyCarpetBlock") {
        // It lets sky light through even where its outline is a whole
        // block (`MossyCarpetBlock.propagatesSkylightDown`).
        row["skylight_down"] = json!(true);
        row["dampening"] = json!(0);
    }
    if cube.lit_by_count.len() > 1 && value("lit") == "true" {
        let count: usize = value("candles").parse().unwrap_or(1);
        row["emission"] = json!(cube.lit_by_count[count - 1]);
    }
}

/// The shapes of a `shaped` block's state, by its class (Minecraft's
/// boxes, in sixteenths).
pub fn shape(block: &Block, properties: &[(&str, &str)]) -> Option<crate::shapes::StateShape> {
    use crate::shapes::{StateShape, cuboid, empty, union, value};
    if layouts().get(block.id) != Some(&"shaped") {
        return None;
    }
    let class = |name: &str| block.classes.contains(&name);
    let waterlogged = value(properties, "waterlogged") == "true";
    // Not occluding neighbours' faces (`noOcclusion`); the occlusion shape
    // is still the outline.
    let open = |collision, outline: crate::shapes::Shape| StateShape {
        collision,
        occlusion: outline.clone(),
        outline,
        can_occlude: false,
        shape_occlusion: false,
        waterlogged,
    };
    let flat = |height: f64| cuboid(0.0, 0.0, 0.0, 16.0, height, 16.0);
    Some(if class("RedstoneWireBlock") {
        let mut boxes = vec![cuboid(3.0, 0.0, 3.0, 13.0, 1.0, 13.0)];
        for (side, floor, up) in [
            (
                "north",
                [3.0, 0.0, 0.0, 13.0, 1.0, 13.0],
                [3.0, 0.0, 0.0, 13.0, 16.0, 1.0],
            ),
            (
                "south",
                [3.0, 0.0, 3.0, 13.0, 1.0, 16.0],
                [3.0, 0.0, 15.0, 13.0, 16.0, 16.0],
            ),
            (
                "east",
                [3.0, 0.0, 3.0, 16.0, 1.0, 13.0],
                [15.0, 0.0, 3.0, 16.0, 16.0, 13.0],
            ),
            (
                "west",
                [0.0, 0.0, 3.0, 13.0, 1.0, 13.0],
                [0.0, 0.0, 3.0, 1.0, 16.0, 13.0],
            ),
        ] {
            let b = |c: [f64; 6]| cuboid(c[0], c[1], c[2], c[3], c[4], c[5]);
            match value(properties, side) {
                "side" => boxes.push(b(floor)),
                "up" => boxes.extend([b(floor), b(up)]),
                _ => {}
            }
        }
        open(empty(), union(&boxes))
    } else if class("RodBlock") {
        let rod = match value(properties, "facing") {
            "north" | "south" => cuboid(6.0, 6.0, 0.0, 10.0, 10.0, 16.0),
            "east" | "west" => cuboid(0.0, 6.0, 6.0, 16.0, 10.0, 10.0),
            _ => cuboid(6.0, 0.0, 6.0, 10.0, 16.0, 10.0),
        };
        open(rod.clone(), rod)
    } else if class("LadderBlock") {
        let ladder = match value(properties, "facing") {
            "south" => cuboid(0.0, 0.0, 0.0, 16.0, 16.0, 3.0),
            "west" => cuboid(13.0, 0.0, 0.0, 16.0, 16.0, 16.0),
            "east" => cuboid(0.0, 0.0, 0.0, 3.0, 16.0, 16.0),
            _ => cuboid(0.0, 0.0, 13.0, 16.0, 16.0, 16.0),
        };
        open(ladder.clone(), ladder)
    } else if class("BaseRailBlock") {
        let ascending = value(properties, "shape").starts_with("ascending");
        open(empty(), flat(if ascending { 8.0 } else { 2.0 }))
    } else if class("LeverBlock") {
        let facing = value(properties, "facing");
        let along_x = matches!(facing, "east" | "west");
        let lever = match (value(properties, "face"), facing) {
            ("floor", _) if along_x => cuboid(4.0, 0.0, 5.0, 12.0, 6.0, 11.0),
            ("floor", _) => cuboid(5.0, 0.0, 4.0, 11.0, 6.0, 12.0),
            ("ceiling", _) if along_x => cuboid(4.0, 10.0, 5.0, 12.0, 16.0, 11.0),
            ("ceiling", _) => cuboid(5.0, 10.0, 4.0, 11.0, 16.0, 12.0),
            (_, "south") => cuboid(5.0, 4.0, 0.0, 11.0, 12.0, 6.0),
            (_, "west") => cuboid(10.0, 4.0, 5.0, 16.0, 12.0, 11.0),
            (_, "east") => cuboid(0.0, 4.0, 5.0, 6.0, 12.0, 11.0),
            _ => cuboid(5.0, 4.0, 10.0, 11.0, 12.0, 16.0),
        };
        open(empty(), lever)
    } else if class("CakeBlock") {
        let bites: f64 = value(properties, "bites").parse().unwrap_or(0.0);
        StateShape::solid(
            cuboid(1.0 + 2.0 * bites, 0.0, 1.0, 15.0, 8.0, 15.0),
            false,
            false,
        )
    } else if class("AnvilBlock") {
        let along_x = matches!(value(properties, "facing"), "east" | "west");
        let anvil = if along_x {
            union(&[
                cuboid(2.0, 0.0, 2.0, 14.0, 4.0, 14.0),
                cuboid(3.0, 4.0, 4.0, 13.0, 5.0, 12.0),
                cuboid(4.0, 5.0, 6.0, 12.0, 10.0, 10.0),
                cuboid(0.0, 10.0, 3.0, 16.0, 16.0, 13.0),
            ])
        } else {
            union(&[
                cuboid(2.0, 0.0, 2.0, 14.0, 4.0, 14.0),
                cuboid(4.0, 4.0, 3.0, 12.0, 5.0, 13.0),
                cuboid(6.0, 5.0, 4.0, 10.0, 10.0, 12.0),
                cuboid(3.0, 10.0, 0.0, 13.0, 16.0, 16.0),
            ])
        };
        StateShape::solid(anvil, false, false)
    } else if class("CandleBlock") {
        let candles = match value(properties, "candles") {
            "2" => cuboid(5.0, 0.0, 6.0, 11.0, 6.0, 9.0),
            "3" => cuboid(5.0, 0.0, 6.0, 10.0, 6.0, 11.0),
            "4" => cuboid(5.0, 0.0, 5.0, 11.0, 6.0, 10.0),
            _ => cuboid(7.0, 0.0, 7.0, 9.0, 6.0, 9.0),
        };
        open(candles.clone(), candles)
    } else if class("CandleCakeBlock") {
        let cake = union(&[
            cuboid(1.0, 0.0, 1.0, 15.0, 8.0, 15.0),
            cuboid(7.0, 8.0, 7.0, 9.0, 14.0, 9.0),
        ]);
        StateShape::solid(cake, false, false)
    } else if class("AbstractCauldronBlock") {
        // A block less its feet's gaps and its inside (`AbstractCauldronBlock.SHAPE`).
        let hollow = union(&[
            cuboid(0.0, 0.0, 4.0, 16.0, 3.0, 12.0),
            cuboid(4.0, 0.0, 0.0, 12.0, 3.0, 16.0),
            cuboid(2.0, 0.0, 2.0, 14.0, 3.0, 14.0),
            cuboid(2.0, 4.0, 2.0, 14.0, 16.0, 14.0),
        ]);
        let cauldron = crate::shapes::full()
            .join(&hollow, horizon_core::geometry::ShapeOp::ONLY_FIRST)
            .expect("a cauldron's shape");
        open(cauldron.clone(), cauldron)
    } else if class("MossyCarpetBlock") {
        let bottom = value(properties, "bottom") == "true";
        let mut boxes = Vec::new();
        if bottom {
            boxes.push(flat(1.0));
        }
        for side in ["north", "south", "west", "east"] {
            let h = match value(properties, side) {
                "low" => 10.0,
                "tall" => 16.0,
                _ => continue,
            };
            boxes.push(match side {
                "north" => cuboid(0.0, 0.0, 0.0, 16.0, h, 1.0),
                "south" => cuboid(0.0, 0.0, 15.0, 16.0, h, 16.0),
                "west" => cuboid(0.0, 0.0, 0.0, 1.0, h, 16.0),
                _ => cuboid(15.0, 0.0, 0.0, 16.0, h, 16.0),
            });
        }
        let collision = if bottom { flat(1.0) } else { empty() };
        // With nothing to draw, its outline is a whole block.
        let outline = if boxes.is_empty() {
            crate::shapes::full()
        } else {
            union(&boxes)
        };
        open(collision, outline)
    } else if class("CarpetBlock") {
        StateShape::solid(flat(1.0), false, waterlogged)
    } else if class("DiodeBlock") {
        StateShape::solid(flat(2.0), false, waterlogged)
    } else if class("SnowLayerBlock") {
        let layers: f64 = value(properties, "layers").parse().unwrap_or(1.0);
        StateShape {
            collision: if layers > 1.0 {
                flat(2.0 * (layers - 1.0))
            } else {
                empty()
            },
            ..StateShape::solid(flat(2.0 * layers), true, waterlogged)
        }
    } else {
        // Farmland and dirt paths.
        StateShape::solid(flat(15.0), true, waterlogged)
    })
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
    if cube.layout == "shaped" {
        block.form = Form::Shaped;
        block.layer = Layer::Cutout;
        if cube.item != cube.id {
            // Placed by another item (redstone wire by redstone): no item
            // of its own.
            block.tier = Tier::Terrain;
            block.category = None;
            block.drop = Drop::Nothing;
        }
    }
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
            if cube.item == cube.id {
                assert_eq!(block.tier, Tier::Palette, "{}", cube.id);
            }
            let form = if matches!(cube.layout, "sunken" | "shaped") {
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
