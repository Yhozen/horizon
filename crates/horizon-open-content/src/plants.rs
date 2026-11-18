//! Plants and small decorations, from `data/blocks/plants.txt` (written by
//! `tools/open_plant_facts.py`): flowers, saplings, grass and ferns, crops,
//! mushrooms and fungi, seagrass, kelp, coral and coral fans, vines of the
//! Nether and the caves, amethyst buds and the like. Each state takes
//! Minecraft's facts (light, outline, collision, random offset, fluid,
//! map colour) and a model in Minecraft's layout for its kind (a cross, a
//! crop, a coral fan...), drawn here, with Minecraft's texture names and tint
//! indices. A block this crate already authors keeps its name and tab.
use crate::blocks::{Block, Category, Drop, Faces, Form, Layer, Mining, Sound, Tier, Tool};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::OnceLock;

const PLANTS: &str = include_str!("../data/blocks/plants.txt");

/// One state of a plant: its facts row and the models it picks among.
#[derive(Debug)]
pub struct PlantState {
    pub row: usize,
    pub models: Vec<Variant>,
}

/// A model with the variant's turn and its weight among the state's models.
#[derive(Debug)]
pub struct Variant {
    pub model: &'static str,
    pub x: i64,
    pub y: i64,
    pub uvlock: bool,
    pub weight: u32,
}

/// One `block` line.
#[derive(Debug)]
pub struct Plant {
    pub id: &'static str,
    pub classes: &'static [&'static str],
    /// The item its gameplay row names, if any (its own, seeds, a torch...).
    pub item: Option<&'static str>,
    pub hardness: f32,
    pub resistance: f32,
    pub friction: f32,
    pub sound: &'static str,
    pub mining: Mining,
    /// Its own item's texture (`block/poppy`, `item/kelp`) and tints.
    pub item_model: Option<(&'static str, Option<Value>)>,
    /// Or its own item's model as Minecraft defines it (a chest's special
    /// model, a bed's two halves).
    pub item_definition: Option<Value>,
    /// Its own item's components besides the base set.
    pub components: Map<String, Value>,
    pub states: Vec<PlantState>,
}

pub struct Facts {
    shapes: Vec<Value>,
    rows: Vec<Value>,
    /// Model name: its layout and texture variables.
    models: HashMap<&'static str, (&'static str, BTreeMap<&'static str, &'static str>)>,
    pub plants: Vec<Plant>,
    by_id: HashMap<&'static str, usize>,
    /// The first block using each model: it writes the model.
    owner: HashMap<&'static str, &'static str>,
    /// The blocks drawn in parts: each case's condition and models.
    multipart: HashMap<&'static str, Vec<(Value, Vec<Variant>)>>,
}

fn leak<T>(v: Vec<T>) -> &'static [T] {
    Box::leak(v.into_boxed_slice())
}

fn parse() -> Facts {
    let mut facts = Facts {
        shapes: Vec::new(),
        rows: Vec::new(),
        models: HashMap::new(),
        plants: Vec::new(),
        by_id: HashMap::new(),
        owner: HashMap::new(),
        multipart: HashMap::new(),
    };
    let json = |text: &str| -> Value { serde_json::from_str(text).expect("plants.txt: JSON") };
    for line in PLANTS
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let f: Vec<&'static str> = line.split('\t').collect();
        match f[0] {
            "shape" => facts.shapes.push(json(f[2])),
            "row" => facts.rows.push(json(f[2])),
            "model" => {
                let textures = f
                    .get(3)
                    .unwrap_or(&"")
                    .split(' ')
                    .filter(|v| !v.is_empty())
                    .map(|v| v.split_once('=').expect("plants.txt: texture variable"))
                    .collect();
                facts.models.insert(f[1], (f[2], textures));
            }
            "block" => {
                assert_eq!(f.len(), 14, "plants.txt: {line}");
                let num = |i: usize| -> f32 { f[i].parse().expect("plants.txt: number") };
                let tool = match f[8] {
                    "pickaxe" => Tool::Pickaxe,
                    "axe" => Tool::Axe,
                    "shovel" => Tool::Shovel,
                    "hoe" => Tool::Hoe,
                    _ => Tool::None,
                };
                let item_definition = f[11].strip_prefix('@').map(json);
                let item_model = (f[11] != "-" && item_definition.is_none()).then(|| {
                    match f[11].split_once('|') {
                        Some((texture, tints)) => (texture, Some(json(tints))),
                        None => (f[11], None),
                    }
                });
                let Value::Object(components) = json(f[12]) else {
                    panic!("plants.txt: components of {}", f[1]);
                };
                let states = f[13]
                    .split(' ')
                    .map(|entry| {
                        let (row, choices) = entry.split_once(':').expect("plants.txt: state");
                        PlantState {
                            row: row.parse().expect("plants.txt: row"),
                            models: variants(choices),
                        }
                    })
                    .collect::<Vec<PlantState>>();
                for variant in states.iter().flat_map(|s| &s.models) {
                    facts.owner.entry(variant.model).or_insert(f[1]);
                }
                facts.by_id.insert(f[1], facts.plants.len());
                facts.plants.push(Plant {
                    id: f[1],
                    classes: leak(f[2].split(',').collect()),
                    item: (f[3] != "-").then_some(f[3]),
                    hardness: num(4),
                    resistance: num(5),
                    friction: num(6),
                    sound: f[7],
                    mining: Mining {
                        tool,
                        requires_tool: f[10] == "1",
                        needs: num(9) as u8,
                    },
                    item_model,
                    item_definition,
                    components,
                    states,
                });
            }
            "multipart" => {
                let cases: Vec<(Value, Vec<Variant>)> = json(f[2])
                    .as_array()
                    .expect("plants.txt: multipart")
                    .iter()
                    .map(|case| {
                        let models = case[1].as_str().expect("plants.txt: models");
                        let models: &'static str = Box::leak(models.to_owned().into_boxed_str());
                        (case[0].clone(), variants(models))
                    })
                    .collect();
                for variant in cases.iter().flat_map(|(_, v)| v) {
                    facts.owner.entry(variant.model).or_insert(f[1]);
                }
                facts.multipart.insert(f[1], cases);
            }
            other => panic!("plants.txt: unknown record {other}"),
        }
    }
    facts
}

/// `MODEL:X:Y:UVLOCK:WEIGHT` entries separated by `|` (none when empty).
fn variants(choices: &'static str) -> Vec<Variant> {
    choices
        .split('|')
        .filter(|c| !c.is_empty())
        .map(|choice| {
            let p: Vec<&'static str> = choice.split(':').collect();
            Variant {
                model: p[0],
                x: p[1].parse().expect("plants.txt: x"),
                y: p[2].parse().expect("plants.txt: y"),
                uvlock: p[3] == "1",
                weight: p[4].parse().expect("plants.txt: weight"),
            }
        })
        .collect()
}

pub fn facts() -> &'static Facts {
    static FACTS: OnceLock<Facts> = OnceLock::new();
    FACTS.get_or_init(parse)
}

pub fn plant(id: &str) -> Option<&'static Plant> {
    let facts = facts();
    facts.by_id.get(id).map(|&i| &facts.plants[i])
}

/// The state's index among the block's states (the last property fastest).
fn state_index(block: &Block, properties: &[(&str, &str)]) -> usize {
    block.properties.iter().fold(0, |index, (key, values)| {
        let value = properties
            .iter()
            .find(|(k, _)| k == key)
            .map_or("", |(_, v)| v);
        index * values.len() + values.iter().position(|v| *v == value).unwrap_or(0)
    })
}

fn state<'a>(block: &Block, properties: &[(&str, &str)]) -> Option<(&'a PlantState, &'a Value)> {
    if block.faces != Faces::Plant {
        return None;
    }
    let facts = facts();
    let state = &plant(block.id)?.states[state_index(block, properties)];
    Some((state, &facts.rows[state.row]))
}

/// The `gameplay-facts.json` fields this state takes from Minecraft, to
/// lay over the row the shared code writes.
pub fn gameplay(block: &Block, properties: &[(&str, &str)]) -> Option<Map<String, Value>> {
    let (_, row) = state(block, properties)?;
    let mut out: Map<String, Value> = row
        .as_object()?
        .iter()
        .filter(|(k, _)| !matches!(k.as_str(), "collision" | "occlusion" | "behaviour"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let item = plant(block.id)?
        .item
        .map_or("minecraft:air".to_owned(), |i| format!("minecraft:{i}"));
    out.insert("item".into(), json!(item));
    Some(out)
}

/// The state's `block-behaviours.json` row (sturdy faces, flags, fluid).
pub fn behaviour(block: &Block, properties: &[(&str, &str)]) -> Option<Value> {
    state(block, properties).map(|(_, row)| row["behaviour"].clone())
}

/// Whether the state holds a fluid (seagrass, kelp, waterlogged states).
pub fn holds_fluid(block: &Block, properties: &[(&str, &str)]) -> bool {
    behaviour(block, properties).is_some_and(|row| row[2].as_u64().unwrap_or(0) != 0)
}

/// The state's collision or occlusion shape, by its index among
/// [`shapes`] (an empty or full shape gives the shared table's own 0 or 1).
/// `Some(None)` for a shape Minecraft computes in code (a shulker box's).
pub fn shape(block: &Block, properties: &[(&str, &str)], occlusion: bool) -> Option<Option<usize>> {
    let (_, row) = state(block, properties)?;
    let id = row[if occlusion { "occlusion" } else { "collision" }].as_i64()?;
    Some(usize::try_from(id).ok())
}

/// Every shape the plants use, in the shape table's terms: cell boundaries
/// per axis, the occupied cells (x slowest), the cube flags and bounds.
pub fn shapes() -> &'static [Value] {
    &facts().shapes
}

/// Shape `id` of [`shapes`] as the engine's `VoxelShape`, built from its
/// cells as the shape table's reader builds it (the empty shape and the full
/// block as the shared ones).
pub fn voxel(id: usize) -> crate::shapes::Shape {
    use crate::shapes::Shape;
    let shape = &shapes()[id];
    let axis = |name: &str| -> Vec<f64> {
        shape[name]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .collect()
    };
    let bounds = |name: &str| -> [i32; 3] {
        let v: Vec<i32> = shape[name]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_i64().map(|i| i as i32))
            .collect();
        [v[0], v[1], v[2]]
    };
    let occupied: Vec<bool> = shape["occupied"]
        .as_str()
        .unwrap_or("")
        .bytes()
        .map(|b| b == b'1')
        .collect();
    if occupied.is_empty() {
        return Shape::empty();
    }
    let coords = [axis("x"), axis("y"), axis("z")];
    let axes = shape["cube_axes"].as_i64().unwrap_or(0);
    let built = if shape["cube"].as_i64() == Some(1) {
        let sizes = std::array::from_fn(|i| coords[i].len().saturating_sub(1));
        Shape::from_cube(sizes, &occupied)
    } else {
        Shape::from_mixed_grid(
            coords,
            &occupied,
            [axes & 1 != 0, axes & 2 != 0, axes & 4 != 0],
        )
    };
    built
        .expect("plants.txt: shape")
        .with_discrete_bounds(bounds("min"), bounds("max"))
        .expect("plants.txt: shape bounds")
}

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

/// Plants Minecraft drops only to shears (or as something else): they drop
/// nothing here.
const SHEARS_ONLY: &[&str] = &[
    "TallGrassBlock",
    "DoublePlantBlock",
    "SeagrassBlock",
    "TallSeagrassBlock",
    "ShortDryGrassBlock",
    "TallDryGrassBlock",
    "NetherSproutsBlock",
    "HangingRootsBlock",
    "WebBlock",
    "DryVegetationBlock",
    "HangingMossBlock",
];

/// Applies the plants' facts to the authored blocks and appends the others,
/// in the file's order.
pub fn add(blocks: &mut Vec<Block>) {
    for plant in &facts().plants {
        let own = plant.item == Some(plant.id);
        if let Some(block) = blocks.iter_mut().find(|b| b.id == plant.id) {
            set_facts(block, plant);
            if own {
                block.tier = Tier::Palette;
                block.category.get_or_insert(Category::Plants);
            }
            continue;
        }
        let drop = match plant.item {
            _ if SHEARS_ONLY.contains(&plant.classes[0]) => Drop::Nothing,
            Some(item) if item == plant.id => Drop::Itself,
            Some(item) => Drop::Item {
                id: item,
                min: 1,
                max: 1,
                ore: false,
            },
            None => Drop::Nothing,
        };
        let mut block = Block {
            id: plant.id,
            name: title(plant.id),
            tier: if own { Tier::Palette } else { Tier::Terrain },
            form: Form::Cross,
            layer: Layer::Cutout,
            faces: Faces::Plant,
            properties: &[],
            defaults: &[],
            hardness: 0.0,
            resistance: 0.0,
            light: 0,
            lit_light: 0,
            opacity: 0,
            sound: Sound::Grass,
            classes: &[],
            map_color: 0,
            category: own.then(|| {
                if plant.classes[0] == "AmethystClusterBlock" {
                    Category::Minerals
                } else if plant.classes[0] == "BedBlock" {
                    Category::Colored
                } else if plant.classes.iter().any(|c| {
                    ["Torch", "Lantern", "Chain", "Chest", "Sign"]
                        .iter()
                        .any(|kind| c.contains(kind))
                }) {
                    Category::Construction
                } else {
                    Category::Plants
                }
            }),
            mining: plant.mining,
            drop,
            friction: 0.6,
        };
        set_facts(&mut block, plant);
        blocks.push(block);
    }
}

fn set_facts(block: &mut Block, plant: &Plant) {
    let first = &facts().rows[plant.states[0].row];
    block.form = Form::Cross;
    block.layer = Layer::Cutout;
    block.faces = Faces::Plant;
    block.classes = plant.classes;
    block.hardness = plant.hardness;
    block.resistance = plant.resistance;
    block.friction = plant.friction;
    block.sound = sound(plant.sound);
    block.mining = plant.mining;
    block.light = first["emission"].as_u64().unwrap_or(0) as u8;
    block.lit_light = block.light;
    block.opacity = first["dampening"].as_u64().unwrap_or(0) as u8;
    block.map_color = first["map_color"].as_u64().unwrap_or(0) as u32;
}

/// Every texture the block's models read (names under `textures/block/`).
pub fn textures(id: &str) -> Vec<&'static str> {
    let Some(plant) = plant(id) else {
        return Vec::new();
    };
    let facts = facts();
    let mut out = BTreeSet::new();
    let parts = facts.multipart.get(id).into_iter().flatten();
    let variants = plant
        .states
        .iter()
        .map(|s| &s.models)
        .chain(parts.map(|(_, models)| models))
        .flatten();
    for variant in variants {
        if let Some((_, textures)) = facts.models.get(variant.model) {
            out.extend(textures.values().copied());
        }
    }
    out.into_iter().collect()
}

/// The particle texture of the block's first model (a chest's planks).
pub fn particle(id: &str) -> Option<&'static str> {
    let model = plant(id)?.states.first()?.models.first()?.model;
    facts().models.get(model)?.1.get("particle").copied()
}

fn tex(name: &str) -> String {
    if name.contains('/') {
        format!("minecraft:{name}")
    } else {
        format!("minecraft:block/{name}")
    }
}

fn faces(entries: &[(&str, [f64; 4], &str, Option<i64>, bool)]) -> Value {
    let mut out = Map::new();
    for (dir, uv, texture, rotation, tint) in entries {
        let mut face = json!({"uv": uv, "texture": texture});
        if let Some(rotation) = rotation {
            face["rotation"] = json!(rotation);
        }
        if *tint {
            face["tintindex"] = json!(0);
        }
        out.insert((*dir).to_owned(), face);
    }
    Value::Object(out)
}

fn element(
    from: [f64; 3],
    to: [f64; 3],
    rotation: Option<([f64; 3], &str, f64, bool)>,
    faces: Value,
) -> Value {
    let mut e = json!({"from": from, "to": to, "shade_direction_override": "up", "faces": faces});
    if let Some((origin, axis, angle, rescale)) = rotation {
        e["rotation"] = json!({"origin": origin, "axis": axis, "angle": angle, "rescale": rescale});
    }
    e
}

const FULL: [f64; 4] = [0.0, 0.0, 16.0, 16.0];
const FLIPPED: [f64; 4] = [16.0, 0.0, 0.0, 16.0];

/// Two planes crossed at 45 degrees, drawn on both sides (`block/cross`).
fn cross(texture: &str, tint: bool, height: f64, uv: [f64; 4]) -> Vec<Value> {
    let turn = Some(([8.0, 8.0, 8.0], "y", 45.0, true));
    vec![
        element(
            [0.8, 0.0, 8.0],
            [15.2, height, 8.0],
            turn,
            faces(&[
                ("north", uv, texture, None, tint),
                ("south", uv, texture, None, tint),
            ]),
        ),
        element(
            [8.0, 0.0, 0.8],
            [8.0, height, 15.2],
            turn,
            faces(&[
                ("west", uv, texture, None, tint),
                ("east", uv, texture, None, tint),
            ]),
        ),
    ]
}

/// The geometry of a layout, and whether ambient occlusion is off.
fn layout(name: &str) -> Option<(Vec<Value>, bool)> {
    Some(match name {
        "cross" => (cross("#cross", false, 16.0, FULL), true),
        "tinted_cross" => (cross("#cross", true, 16.0, FULL), true),
        "cross_emissive" => {
            let mut elements = cross("#cross", false, 16.0, FULL);
            for mut glow in cross("#cross_emissive", false, 16.0, FULL) {
                glow["light_emission"] = json!(15);
                elements.push(glow);
            }
            (elements, true)
        }
        // Four planes in a hash, sunk a pixel into the farmland.
        "crop" => {
            let plane = |from, to, a: (&'static str, [f64; 4]), b: (&'static str, [f64; 4])| {
                element(
                    from,
                    to,
                    None,
                    faces(&[
                        (a.0, a.1, "#crop", None, false),
                        (b.0, b.1, "#crop", None, false),
                    ]),
                )
            };
            (
                vec![
                    plane(
                        [4.0, -1.0, 0.0],
                        [4.0, 15.0, 16.0],
                        ("west", FULL),
                        ("east", FLIPPED),
                    ),
                    plane(
                        [12.0, -1.0, 0.0],
                        [12.0, 15.0, 16.0],
                        ("west", FLIPPED),
                        ("east", FULL),
                    ),
                    plane(
                        [0.0, -1.0, 4.0],
                        [16.0, 15.0, 4.0],
                        ("north", FULL),
                        ("south", FLIPPED),
                    ),
                    plane(
                        [0.0, -1.0, 12.0],
                        [16.0, 15.0, 12.0],
                        ("north", FLIPPED),
                        ("south", FULL),
                    ),
                ],
                true,
            )
        }
        // Four flat blades leaning out from the middle.
        "coral_fan" => {
            let blade = |from,
                         to,
                         origin,
                         axis,
                         angle,
                         up: Option<i64>,
                         down: Option<i64>,
                         uv: [[f64; 4]; 2]| {
                element(
                    from,
                    to,
                    Some((origin, axis, angle, false)),
                    faces(&[
                        ("up", uv[0], "#fan", up, false),
                        ("down", uv[1], "#fan", down, false),
                    ]),
                )
            };
            let down = [0.0, 16.0, 16.0, 0.0];
            (
                vec![
                    blade(
                        [8.0, 0.0, 0.0],
                        [24.0, 0.0, 16.0],
                        [8.0, 0.0, 0.0],
                        "z",
                        22.5,
                        Some(90),
                        Some(270),
                        [FULL, down],
                    ),
                    blade(
                        [-8.0, 0.0, 0.0],
                        [8.0, 0.0, 16.0],
                        [8.0, 0.0, 0.0],
                        "z",
                        -22.5,
                        Some(270),
                        Some(90),
                        [FULL, down],
                    ),
                    blade(
                        [0.0, 0.0, 8.0],
                        [16.0, 0.0, 24.0],
                        [0.0, 0.0, 8.0],
                        "x",
                        -22.5,
                        None,
                        None,
                        [[16.0, 16.0, 0.0, 0.0], [16.0, 0.0, 0.0, 16.0]],
                    ),
                    blade(
                        [0.0, 0.0, -8.0],
                        [16.0, 0.0, 8.0],
                        [0.0, 0.0, 8.0],
                        "x",
                        22.5,
                        None,
                        None,
                        [FULL, down],
                    ),
                ],
                true,
            )
        }
        // Two blades out of the wall to the south.
        "coral_wall_fan" => {
            let blade = |angle| {
                element(
                    [0.0, 8.0, 0.0],
                    [16.0, 8.0, 16.0],
                    Some(([8.0, 8.0, 14.0], "x", angle, true)),
                    faces(&[
                        ("up", FULL, "#fan", None, false),
                        ("down", [16.0, 16.0, 0.0, 0.0], "#fan", None, false),
                    ]),
                )
            };
            (vec![blade(22.5), blade(-22.5)], true)
        }
        // Four planes in a hash, the full height.
        "template_seagrass" => {
            let plane = |from, to, a: &'static str, b: &'static str| {
                element(
                    from,
                    to,
                    None,
                    faces(&[
                        (a, FULL, "#texture", None, false),
                        (b, FULL, "#texture", None, false),
                    ]),
                )
            };
            (
                vec![
                    plane([0.0, 0.0, 4.0], [16.0, 16.0, 4.0], "north", "south"),
                    plane([12.0, 0.0, 0.0], [12.0, 16.0, 16.0], "west", "east"),
                    plane([4.0, 0.0, 0.0], [4.0, 16.0, 16.0], "west", "east"),
                    plane([0.0, 0.0, 12.0], [16.0, 16.0, 12.0], "north", "south"),
                ],
                false,
            )
        }
        // The sunflower's top: a short cross and the tilted flower head.
        "sunflower_top" => {
            let mut elements = cross("#cross", false, 8.0, [0.0, 8.0, 16.0, 16.0]);
            elements.push(element(
                [9.6, -1.0, 1.0],
                [9.6, 15.0, 15.0],
                Some(([8.0, 8.0, 8.0], "z", 22.5, true)),
                faces(&[
                    ("west", FULL, "#back", None, false),
                    ("east", FULL, "#front", None, false),
                ]),
            ));
            (elements, true)
        }
        "template_torch" | "template_torch_unlit" | "template_redstone_torch" => {
            let shade = name != "template_torch_unlit";
            let mut elements = vec![stick([7.0, 0.0, 7.0], [9.0, 10.0, 9.0], None, true, shade)];
            if name == "template_redstone_torch" {
                // The glowing tip: one face on each side of a small box.
                let tip =
                    |from, to, dir, uv| plain("#torch", from, to, None, &[(dir, uv, None)], true);
                elements.extend([
                    tip([6.5, 7.5, 6.5], [9.5, 7.5, 9.5], "up", [8.0, 5.0, 9.0, 6.0]),
                    tip(
                        [6.5, 10.5, 6.5],
                        [9.5, 10.5, 9.5],
                        "down",
                        [7.0, 5.0, 8.0, 6.0],
                    ),
                    tip(
                        [6.5, 7.5, 6.5],
                        [9.5, 10.5, 6.5],
                        "south",
                        [9.0, 6.0, 10.0, 7.0],
                    ),
                    tip(
                        [9.5, 7.5, 6.5],
                        [9.5, 10.5, 9.5],
                        "west",
                        [6.0, 7.0, 7.0, 8.0],
                    ),
                    tip(
                        [6.5, 7.5, 9.5],
                        [9.5, 10.5, 9.5],
                        "north",
                        [6.0, 6.0, 7.0, 7.0],
                    ),
                    tip(
                        [6.5, 7.5, 6.5],
                        [6.5, 10.5, 9.5],
                        "east",
                        [9.0, 7.0, 10.0, 8.0],
                    ),
                ]);
            }
            (elements, true)
        }
        "template_torch_wall" | "template_torch_wall_unlit" | "template_redstone_torch_wall" => {
            let lean = Some(([0.0, 3.5, 8.0], "z", -22.5, false));
            let shade = name != "template_torch_wall_unlit";
            let mut elements = vec![stick(
                [-1.0, 3.5, 7.0],
                [1.0, 13.5, 9.0],
                lean,
                false,
                shade,
            )];
            if name == "template_redstone_torch_wall" {
                let uv = [6.0, 5.0, 7.0, 6.0];
                let tip = |from, to, dir| plain("#torch", from, to, lean, &[(dir, uv, None)], true);
                elements.extend([
                    tip([-1.5, 8.0, 6.5], [1.5, 11.0, 9.5], "up"),
                    tip([-1.5, 14.0, 6.5], [1.5, 17.0, 9.5], "down"),
                    tip([-1.5, 11.0, 3.5], [1.5, 14.0, 6.5], "south"),
                    tip([1.5, 11.0, 6.5], [4.5, 14.0, 9.5], "west"),
                    tip([-1.5, 11.0, 9.5], [1.5, 14.0, 12.5], "north"),
                    tip([-4.5, 11.0, 6.5], [-1.5, 14.0, 9.5], "east"),
                ]);
            }
            (elements, true)
        }
        "template_lantern" | "template_hanging_lantern" => {
            let hanging = name == "template_hanging_lantern";
            let (lo, cull) = if hanging {
                (1.0, None)
            } else {
                (0.0, Some("down"))
            };
            let side = [0.0, 2.0, 6.0, 9.0];
            let lid = [1.0, 10.0, 5.0, 14.0];
            let cap = [1.0, 0.0, 5.0, 2.0];
            let turn = Some(([8.0, 8.0, 8.0], "y", 45.0, false));
            let mut top = vec![("up", lid, None)];
            if hanging {
                top.insert(0, ("down", lid, None));
            }
            top.extend(["north", "south", "west", "east"].map(|d| (d, cap, None)));
            let body: Vec<(&str, [f64; 4], Option<&str>)> = vec![
                ("down", [0.0, 9.0, 6.0, 15.0], cull),
                ("up", [0.0, 9.0, 6.0, 15.0], None),
                ("north", side, None),
                ("south", side, None),
                ("west", side, None),
                ("east", side, None),
            ];
            let (hook_a, hook_b): ([f64; 4], [f64; 4]) = if hanging {
                ([11.0, 1.0, 14.0, 5.0], [11.0, 6.0, 14.0, 12.0])
            } else {
                ([11.0, 1.0, 14.0, 3.0], [11.0, 10.0, 14.0, 12.0])
            };
            let flip = |uv: [f64; 4]| [uv[2], uv[1], uv[0], uv[3]];
            let (a_from, a_to, b_from, b_to) = if hanging {
                (
                    [6.5, 11.0, 8.0],
                    [9.5, 15.0, 8.0],
                    [8.0, 10.0, 6.5],
                    [8.0, 16.0, 9.5],
                )
            } else {
                (
                    [6.5, 9.0, 8.0],
                    [9.5, 11.0, 8.0],
                    [8.0, 9.0, 6.5],
                    [8.0, 11.0, 9.5],
                )
            };
            (
                vec![
                    plain(
                        "#lantern",
                        [5.0, lo, 5.0],
                        [11.0, lo + 7.0, 11.0],
                        None,
                        &body,
                        false,
                    ),
                    plain(
                        "#lantern",
                        [6.0, lo + 7.0, 6.0],
                        [10.0, lo + 9.0, 10.0],
                        None,
                        &top,
                        false,
                    ),
                    plain(
                        "#lantern",
                        a_from,
                        a_to,
                        turn,
                        &[("north", flip(hook_a), None), ("south", hook_a, None)],
                        true,
                    ),
                    plain(
                        "#lantern",
                        b_from,
                        b_to,
                        turn,
                        &[("west", flip(hook_b), None), ("east", hook_b, None)],
                        true,
                    ),
                ],
                false,
            )
        }
        // A flat leaf just above the ground, tinted.
        "lily_pad" => (
            vec![
                json!({"from": [0.0, 0.25, 0.0], "to": [16.0, 0.25, 16.0], "faces": {
                    "down": {"uv": [0.0, 16.0, 16.0, 0.0], "texture": "#texture", "tintindex": 0},
                    "up": {"uv": FULL, "texture": "#texture", "tintindex": 0},
                }}),
            ],
            true,
        ),
        // Drawn by the block-entity renderer (chests): only a particle.
        "none" => (Vec::new(), false),
        // A bed half: the mattress 3 to 9 pixels up, open toward the other
        // half, and two 3-pixel legs at its far corners. Each face names
        // the texture of its side; the legs' faces the side they show.
        "template_bed_head" | "template_bed_foot" => {
            let head = name == "template_bed_head";
            let end = if head { "north" } else { "south" };
            let leg = |u: f64| [u, 13.0, u + 3.0, 16.0];
            let mattress = [
                ("down", FULL, "#down", None),
                ("up", FULL, "#up", None),
                (
                    end,
                    [0.0, 7.0, 16.0, 13.0],
                    if head { "#north" } else { "#south" },
                    Some(end),
                ),
                ("west", [0.0, 7.0, 16.0, 13.0], "#west", Some("west")),
                ("east", [0.0, 7.0, 16.0, 13.0], "#east", Some("east")),
            ];
            // Each leg: its down, north, south, west and east faces.
            let legs: [(
                [f64; 3],
                [f64; 3],
                [(&str, [f64; 4], &str, Option<&str>); 5],
            ); 2] = if head {
                [
                    (
                        [0.0, 0.0, 0.0],
                        [3.0, 3.0, 3.0],
                        [
                            ("down", leg(6.0), "#west", Some("down")),
                            ("north", leg(13.0), "#north", Some("north")),
                            ("south", leg(3.0), "#west", None),
                            ("west", leg(0.0), "#west", Some("west")),
                            ("east", leg(10.0), "#north", None),
                        ],
                    ),
                    (
                        [13.0, 0.0, 0.0],
                        [16.0, 3.0, 3.0],
                        [
                            ("down", leg(7.0), "#east", Some("down")),
                            ("north", leg(0.0), "#north", Some("north")),
                            ("south", leg(10.0), "#east", None),
                            ("west", leg(3.0), "#north", None),
                            ("east", leg(13.0), "#east", Some("east")),
                        ],
                    ),
                ]
            } else {
                [
                    (
                        [0.0, 0.0, 13.0],
                        [3.0, 3.0, 16.0],
                        [
                            ("down", leg(7.0), "#west", Some("down")),
                            ("north", leg(10.0), "#west", None),
                            ("south", leg(0.0), "#south", Some("south")),
                            ("west", leg(13.0), "#west", Some("west")),
                            ("east", leg(3.0), "#south", None),
                        ],
                    ),
                    (
                        [13.0, 0.0, 13.0],
                        [16.0, 3.0, 16.0],
                        [
                            ("down", leg(6.0), "#east", Some("down")),
                            ("north", leg(3.0), "#east", None),
                            ("south", leg(13.0), "#south", Some("south")),
                            ("west", leg(10.0), "#south", None),
                            ("east", leg(0.0), "#east", Some("east")),
                        ],
                    ),
                ]
            };
            let mut elements = vec![mixed([0.0, 3.0, 0.0], [16.0, 9.0, 16.0], &mattress)];
            for (from, to, faces) in &legs {
                elements.push(mixed(*from, *to, faces));
            }
            (elements, false)
        }
        // A sign: Minecraft's sign board (24 by 12 by 2 pixels) and post (2
        // by 14 by 2) at two thirds of their size on the sign texture's
        // layout; standing, the board tops the post and the model turns a
        // sixteenth of a turn per step (the block state turns it further);
        // on a wall, the board alone against the wall's side.
        _ if name.starts_with("template_sign_rot_") || name == "template_wall_sign" => {
            // Written to five places, as Minecraft's model files write them
            // (a turned corner lands where Minecraft's does).
            let third = |v: f64| (v * 2.0 / 3.0 * 1e5).round() / 1e5;
            let board_faces = |wall: bool| {
                vec![
                    ("north", [0.0, 8.0, 12.0, 14.0], None),
                    ("east", [12.0, 1.0, 13.0, 7.0], wall.then_some("east")),
                    ("south", [0.0, 1.0, 12.0, 7.0], None),
                    ("west", [12.0, 8.0, 13.0, 14.0], wall.then_some("west")),
                    ("up", [0.0, 0.0, 12.0, 1.0], None),
                    ("down", [0.0, 14.0, 12.0, 15.0], None),
                ]
            };
            if name == "template_wall_sign" {
                let board = plain(
                    "#all",
                    [0.0, third(12.5) - third(6.0), 1.0 - third(1.0)],
                    [16.0, third(12.5) + third(6.0), 1.0 + third(1.0)],
                    None,
                    &board_faces(true),
                    false,
                );
                return Some((vec![board], false));
            }
            let step = f64::from(name.as_bytes()[name.len() - 1] - b'0');
            // Minecraft's first step turns by a hair, not by nothing.
            let angle = if step == 0.0 { 0.0001 } else { -22.5 * step };
            let turn = Some(([8.0, 0.0, 8.0], "y", angle, false));
            let post = plain(
                "#all",
                [8.0 - third(1.0), 0.0, 8.0 - third(1.0)],
                [8.0 + third(1.0), third(14.0), 8.0 + third(1.0)],
                (step != 0.0).then_some(turn).flatten(),
                &[
                    ("north", [14.0, 8.0, 15.0, 15.0], None),
                    ("east", [15.0, 0.0, 16.0, 7.0], None),
                    ("south", [14.0, 0.0, 15.0, 7.0], None),
                    ("west", [15.0, 8.0, 16.0, 15.0], None),
                    ("down", [14.0, 15.0, 15.0, 16.0], Some("down")),
                ],
                false,
            );
            let board = plain(
                "#all",
                [0.0, third(14.0), 8.0 - third(1.0)],
                [16.0, third(26.0), 8.0 + third(1.0)],
                turn,
                &board_faces(false),
                false,
            );
            (vec![post, board], false)
        }
        // A chain: two narrow strips of its texture crossed at 45 degrees.
        "template_chain" => {
            let turn = Some(([8.0, 8.0, 8.0], "y", 45.0, false));
            (
                vec![
                    plain(
                        "#all",
                        [6.5, 0.0, 8.0],
                        [9.5, 16.0, 8.0],
                        turn,
                        &[
                            ("north", [3.0, 0.0, 0.0, 16.0], None),
                            ("south", [0.0, 0.0, 3.0, 16.0], None),
                        ],
                        true,
                    ),
                    plain(
                        "#all",
                        [8.0, 0.0, 6.5],
                        [8.0, 16.0, 9.5],
                        turn,
                        &[
                            ("west", [6.0, 0.0, 3.0, 16.0], None),
                            ("east", [3.0, 0.0, 6.0, 16.0], None),
                        ],
                        true,
                    ),
                ],
                false,
            )
        }
        // Leaf litter: a tinted sheet just above the ground, over the
        // north-west quarter, the west half, the south-east quarter or the
        // whole block; the block state's cases combine and turn them.
        _ if name.starts_with("template_leaf_litter_") => {
            let (from, to): ([f64; 2], [f64; 2]) = match name.as_bytes()[name.len() - 1] {
                b'1' => ([0.0, 0.0], [8.0, 8.0]),
                b'2' => ([0.0, 0.0], [8.0, 16.0]),
                b'3' => ([8.0, 8.0], [16.0, 16.0]),
                _ => ([0.0, 0.0], [16.0, 16.0]),
            };
            let up = [from[0], from[1], to[0], to[1]];
            let down = [from[0], to[1], to[0], from[1]];
            (
                vec![
                    json!({"from": [from[0], 0.25, from[1]], "to": [to[0], 0.25, to[1]], "faces": {
                        "up": {"uv": up, "texture": "#texture", "tintindex": 0},
                        "down": {"uv": down, "texture": "#texture", "tintindex": 0},
                    }}),
                ],
                true,
            )
        }
        // One face of a vine or lichen, against the north side.
        "vine" | "glow_lichen" | "sculk_vein" => {
            let (z, tint) = if name == "vine" {
                (0.8, true)
            } else {
                (0.1, false)
            };
            let texture = format!("#{name}");
            let mut sheet = json!({"from": [0.0, 0.0, z], "to": [16.0, 16.0, z], "faces": faces(&[
                ("north", FLIPPED, texture.as_str(), None, tint),
                ("south", FULL, texture.as_str(), None, tint),
            ])});
            if tint {
                sheet["shade_direction_override"] = json!("up");
            }
            (vec![sheet], true)
        }
        // Sea pickles: one to four, each a body, its top and (alive) two
        // crossed tentacles.
        _ if name.ends_with("sea_pickle") || name.ends_with("sea_pickles") => {
            let pickles: &[(f64, f64, f64)] = if name.starts_with("two") {
                &[(3.0, 3.0, 6.0), (8.0, 8.0, 4.0)]
            } else if name.starts_with("three") {
                &[(6.0, 9.0, 6.0), (2.0, 2.0, 4.0), (8.0, 4.0, 6.0)]
            } else if name.starts_with("four") {
                &[
                    (2.0, 2.0, 6.0),
                    (9.0, 10.0, 4.0),
                    (9.0, 2.0, 6.0),
                    (2.0, 8.0, 7.0),
                ]
            } else {
                &[(6.0, 6.0, 6.0)]
            };
            let alive = !name.contains("dead");
            let mut elements = Vec::new();
            for &(x, z, h) in pickles {
                let side = |u: f64| [u, 5.0, u + 4.0, 5.0 + h];
                elements.push(plain(
                    "#all",
                    [x, 0.0, z],
                    [x + 4.0, h, z + 4.0],
                    None,
                    &[
                        ("down", [8.0, 1.0, 12.0, 5.0], Some("down")),
                        ("up", [4.0, 1.0, 8.0, 5.0], None),
                        ("north", side(4.0), None),
                        ("south", side(0.0), None),
                        ("west", side(8.0), None),
                        ("east", side(12.0), None),
                    ],
                    false,
                ));
                elements.push(plain(
                    "#all",
                    [x, h - 0.05, z],
                    [x + 4.0, h - 0.05, z + 4.0],
                    None,
                    &[("up", [8.0, 1.0, 12.0, 5.0], None)],
                    false,
                ));
                if alive {
                    let (cx, cz) = (x + 2.0, z + 2.0);
                    let turn = Some(([cx, 8.0, cz], "y", 45.0, true));
                    elements.push(plain(
                        "#all",
                        [cx - 0.5, h - 0.8, cz],
                        [cx + 0.5, h + 2.7, cz],
                        turn,
                        &[
                            ("north", [1.0, 0.0, 3.0, 5.0], None),
                            ("south", [3.0, 0.0, 1.0, 5.0], None),
                        ],
                        true,
                    ));
                    elements.push(plain(
                        "#all",
                        [cx, h - 0.8, cz - 0.5],
                        [cx, h + 2.7, cz + 0.5],
                        turn,
                        &[
                            ("west", [13.0, 0.0, 15.0, 5.0], None),
                            ("east", [15.0, 0.0, 13.0, 5.0], None),
                        ],
                        true,
                    ));
                }
            }
            (elements, false)
        }
        _ => return None,
    })
}

/// An element with untinted faces on one texture, each with its UV and
/// optional cull face.
fn plain(
    texture: &str,
    from: [f64; 3],
    to: [f64; 3],
    rotation: Option<([f64; 3], &str, f64, bool)>,
    entries: &[(&str, [f64; 4], Option<&str>)],
    shade_up: bool,
) -> Value {
    let mut faces = Map::new();
    for (dir, uv, cull) in entries {
        let mut face = json!({"uv": uv, "texture": texture});
        if let Some(cull) = cull {
            face["cullface"] = json!(cull);
        }
        faces.insert((*dir).to_owned(), face);
    }
    let mut e = json!({"from": from, "to": to, "faces": faces});
    if shade_up {
        e["shade_direction_override"] = json!("up");
    }
    if let Some((origin, axis, angle, rescale)) = rotation {
        e["rotation"] = json!({"origin": origin, "axis": axis, "angle": angle, "rescale": rescale});
    }
    e
}

/// An element whose faces each name their texture, UV and cull face.
fn mixed(from: [f64; 3], to: [f64; 3], entries: &[(&str, [f64; 4], &str, Option<&str>)]) -> Value {
    let mut faces = Map::new();
    for (dir, uv, texture, cull) in entries {
        let mut face = json!({"uv": uv, "texture": texture});
        if let Some(cull) = cull {
            face["cullface"] = json!(cull);
        }
        faces.insert((*dir).to_owned(), face);
    }
    json!({"from": from, "to": to, "faces": faces})
}

/// A torch's stick: a 2x10 box on `#torch`, its bottom culled when it stands.
fn stick(
    from: [f64; 3],
    to: [f64; 3],
    rotation: Option<([f64; 3], &str, f64, bool)>,
    standing: bool,
    shade_up: bool,
) -> Value {
    let side = [7.0, 6.0, 9.0, 16.0];
    plain(
        "#torch",
        from,
        to,
        rotation,
        &[
            ("down", [7.0, 13.0, 9.0, 15.0], standing.then_some("down")),
            ("up", [7.0, 6.0, 9.0, 8.0], None),
            ("north", side, None),
            ("east", side, None),
            ("south", side, None),
            ("west", side, None),
        ],
        shade_up,
    )
}

/// A model of the facts file in the bundle's format.
fn model(name: &str) -> Value {
    let (layout_name, textures) = &facts().models[name];
    let (elements, no_occlusion) =
        layout(layout_name).unwrap_or_else(|| panic!("plants.rs: no layout {layout_name}"));
    let textures: Map<String, Value> = textures
        .iter()
        .map(|(k, v)| ((*k).to_owned(), json!(tex(v))))
        .collect();
    let mut model = json!({
        "parent": "minecraft:block/open_base",
        "textures": textures,
        "elements": elements,
    });
    if no_occlusion {
        model["ambientocclusion"] = json!(false);
    }
    model
}

/// The models a variant or a multipart case picks among, in the block-state
/// format (one model, or a list to pick from at random).
fn choice(models: &[Variant]) -> Value {
    let mut choices: Vec<Value> = models
        .iter()
        .map(|choice| {
            let mut variant = json!({"model": format!("minecraft:block/{}", choice.model)});
            if choice.x != 0 {
                variant["x"] = json!(choice.x);
            }
            if choice.y != 0 {
                variant["y"] = json!(choice.y);
            }
            if choice.uvlock {
                variant["uvlock"] = json!(true);
            }
            if choice.weight != 1 {
                variant["weight"] = json!(choice.weight);
            }
            variant
        })
        .collect();
    if choices.len() == 1 {
        choices.remove(0)
    } else {
        Value::Array(choices)
    }
}

/// The block's models (those it is the first to use) and its block-state
/// definition.
pub fn models(block: &Block) -> (Vec<(String, Value)>, Option<Value>) {
    let Some(plant) = plant(block.id) else {
        return (Vec::new(), None);
    };
    let facts = facts();
    let owned: BTreeSet<&str> = facts
        .owner
        .iter()
        .filter(|(_, owner)| **owner == block.id)
        .map(|(name, _)| *name)
        .collect();
    let models = owned
        .into_iter()
        .map(|name| (format!("block/{name}"), model(name)))
        .collect();
    if let Some(cases) = facts.multipart.get(block.id) {
        let parts: Vec<Value> = cases
            .iter()
            .map(|(when, models)| json!({"when": when, "apply": choice(models)}))
            .collect();
        return (models, Some(json!({"multipart": parts})));
    }
    let mut variants = Map::new();
    for (state, properties) in plant.states.iter().zip(block.states()) {
        let key: Vec<String> = properties.iter().map(|(k, v)| format!("{k}={v}")).collect();
        variants.insert(key.join(","), choice(&state.models));
    }
    (models, Some(json!({"variants": variants})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_families_have_minecraft_facts_and_layouts() {
        let facts = facts();
        assert!(facts.plants.len() >= 100);
        for (name, (layout_name, _)) in &facts.models {
            assert!(layout(layout_name).is_some(), "{name}: {layout_name}");
        }
        let blocks = crate::blocks::defined();
        for plant in &facts.plants {
            let block = blocks.iter().find(|b| b.id == plant.id).unwrap();
            assert_eq!(block.state_count(), plant.states.len(), "{}", plant.id);
            assert!(!textures(plant.id).is_empty(), "{}", plant.id);
        }
        let wheat = plant("wheat").unwrap();
        assert_eq!(wheat.item, Some("wheat_seeds"));
        assert_eq!(wheat.states.len(), 8);
        let poppy = blocks.iter().find(|b| b.id == "poppy").unwrap();
        let row = gameplay(poppy, &[]).unwrap();
        assert_eq!(row["offset"], true);
        assert_eq!(
            row["outline"],
            json!([[0.3125, 0.0, 0.3125, 0.6875, 0.625, 0.6875]])
        );
        let kelp = blocks.iter().find(|b| b.id == "kelp").unwrap();
        assert_eq!(behaviour(kelp, &[("age", "0")]).unwrap()[2], 2);
    }
}
