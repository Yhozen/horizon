//! Slabs and stairs, from `data/blocks/slabs.txt` (written by
//! `tools/open_slab_facts.py`): every `SlabBlock` and `StairBlock` with
//! Minecraft's numbers and the texture names of its faces. Their shapes
//! (from [`crate::shapes`]), block-state definitions and models are this
//! crate's own, laid out as Minecraft's are so each state shows and collides
//! the same.
use crate::blocks::{Block, Category, Drop, Faces, Form, Layer, Mining, Tier, Tool};
use crate::shapes::{StateShape, cuboid, flip_y, full, rotate_y, turns, union, value};
use serde_json::{Value, json};

const SLABS: &str = include_str!("../data/blocks/slabs.txt");

/// One line of `slabs.txt`.
#[derive(Debug)]
pub struct Line {
    pub id: &'static str,
    pub stairs: bool,
    pub classes: &'static [&'static str],
    pub hardness: f32,
    pub resistance: f32,
    pub friction: f32,
    pub map_color: u32,
    pub sound: &'static str,
    pub mining: Mining,
    /// The note block instrument it gives (`NoteBlockInstrument`).
    pub instrument: &'static str,
    pub ignited_by_lava: bool,
    pub faces: Faces,
}

/// Every line, in block registry order.
pub fn lines() -> Vec<Line> {
    SLABS
        .lines()
        .skip(1)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let f: Vec<&'static str> = line.split('\t').collect();
            assert_eq!(f.len(), 22, "slabs.txt: {line}");
            let num = |i: usize| -> f32 {
                f[i].parse()
                    .unwrap_or_else(|_| panic!("slabs.txt: {} field {i}", f[0]))
            };
            let tool = match f[8] {
                "pickaxe" => Tool::Pickaxe,
                "axe" => Tool::Axe,
                "shovel" => Tool::Shovel,
                "hoe" => Tool::Hoe,
                _ => Tool::None,
            };
            let stairs = f[1] == "stairs";
            let (bottom, top, side) = (f[13], f[14], f[15]);
            let faces = if stairs {
                Faces::Stairs { bottom, top, side }
            } else {
                Faces::Slab {
                    bottom,
                    top,
                    side,
                    double: [f[16], f[17], f[18], f[19], f[20], f[21]],
                }
            };
            Line {
                id: f[0],
                stairs,
                classes: Box::leak(f[2].split(',').collect::<Vec<_>>().into_boxed_slice()),
                hardness: num(3),
                resistance: num(4),
                friction: num(5),
                map_color: u32::from_str_radix(f[6], 16).expect("slabs.txt: map colour"),
                sound: f[7],
                mining: Mining {
                    tool,
                    requires_tool: f[10] == "1",
                    needs: num(9) as u8,
                },
                instrument: f[11],
                ignited_by_lava: f[12] == "1",
                faces,
            }
        })
        .collect()
}

/// The line of a slab or stairs block.
pub fn line(id: &str) -> Option<&'static Line> {
    static LINES: std::sync::OnceLock<Vec<Line>> = std::sync::OnceLock::new();
    LINES.get_or_init(lines).iter().find(|l| l.id == id)
}

/// Appends every slab and stairs block, in the file's order.
pub fn add(blocks: &mut Vec<Block>) {
    for line in lines() {
        assert!(
            blocks.iter().all(|b| b.id != line.id),
            "{} is defined twice",
            line.id
        );
        blocks.push(Block {
            id: line.id,
            name: crate::cubes::title(line.id),
            tier: Tier::Palette,
            form: Form::Shaped,
            layer: Layer::Solid,
            faces: line.faces,
            properties: &[],
            defaults: &[],
            hardness: line.hardness,
            resistance: line.resistance,
            light: 0,
            lit_light: 0,
            opacity: 0,
            sound: crate::cubes::sound(line.sound),
            classes: line.classes,
            map_color: line.map_color,
            category: Some(if line.id.contains("wool") {
                Category::Colored
            } else {
                Category::Construction
            }),
            mining: line.mining,
            drop: Drop::Itself,
            friction: line.friction,
        });
    }
}

/// A slab's state: the lower or upper half, or both.
pub fn slab(properties: &[(&str, &str)]) -> StateShape {
    let kind = value(properties, "type");
    let shape = match kind {
        "double" => full(),
        "top" => cuboid(0.0, 8.0, 0.0, 16.0, 16.0, 16.0),
        _ => cuboid(0.0, 0.0, 0.0, 16.0, 8.0, 16.0),
    };
    StateShape::solid(
        shape,
        kind != "double",
        value(properties, "waterlogged") == "true",
    )
}

/// A stairs state: a slab with a step on top (below when `half=top`); a
/// quarter of a block for outer corners, three for inner ones.
pub fn stairs(properties: &[(&str, &str)]) -> StateShape {
    let facing = turns(value(properties, "facing"));
    // The step's part toward north, then turned to its direction:
    // StairBlock.getShape picks the map by shape and the direction by its
    // handedness.
    let north_half = cuboid(0.0, 8.0, 0.0, 16.0, 16.0, 8.0);
    let (step, turn) = match value(properties, "shape") {
        "outer_left" => (cuboid(0.0, 8.0, 0.0, 8.0, 16.0, 8.0), facing),
        "outer_right" => (cuboid(0.0, 8.0, 0.0, 8.0, 16.0, 8.0), facing + 1),
        "inner_left" => (
            union(&[north_half.clone(), rotate_y(&north_half, 1)]),
            facing + 3,
        ),
        "inner_right" => (
            union(&[north_half.clone(), rotate_y(&north_half, 1)]),
            facing,
        ),
        _ => (north_half, facing),
    };
    let bottom = union(&[
        cuboid(0.0, 0.0, 0.0, 16.0, 8.0, 16.0),
        rotate_y(&step, turn),
    ]);
    let shape = if value(properties, "half") == "top" {
        flip_y(&bottom)
    } else {
        bottom
    };
    StateShape::solid(shape, true, value(properties, "waterlogged") == "true")
}

fn tex(name: &str) -> String {
    format!("minecraft:block/{name}")
}

/// A model face: its UV, texture variable and the face it is culled by.
fn face(uv: [u8; 4], texture: &str, cull: Option<&str>) -> Value {
    let mut f = json!({"uv": uv, "texture": format!("#{texture}")});
    if let Some(cull) = cull {
        f["cullface"] = json!(cull);
    }
    f
}

/// The lower half of a slab or stairs.
fn lower() -> Value {
    json!({"from": [0, 0, 0], "to": [16, 8, 16], "faces": {
        "down": face([0, 0, 16, 16], "bottom", Some("down")),
        "up": face([0, 0, 16, 16], "top", None),
        "north": face([0, 8, 16, 16], "side", Some("north")),
        "south": face([0, 8, 16, 16], "side", Some("south")),
        "west": face([0, 8, 16, 16], "side", Some("west")),
        "east": face([0, 8, 16, 16], "side", Some("east")),
    }})
}

fn model(bottom: &str, top: &str, side: &str, elements: Vec<Value>, stairs: bool) -> Value {
    let mut model = json!({
        "parent": "minecraft:block/open_base",
        "textures": {"bottom": tex(bottom), "top": tex(top), "side": tex(side), "particle": tex(side)},
        "elements": elements,
    });
    if stairs {
        // Shown in the inventory with its steps toward the viewer.
        model["display"] = json!({
            "gui": {"rotation": [30, 135, 0], "translation": [0, 0, 0], "scale": [0.625, 0.625, 0.625]},
        });
    }
    model
}

/// The block's models and its block-state definition.
pub fn models(block: &Block) -> (Vec<(String, Value)>, Value) {
    let id = block.id;
    match block.faces {
        Faces::Slab {
            bottom,
            top,
            side,
            double,
        } => {
            let upper = json!({"from": [0, 8, 0], "to": [16, 16, 16], "faces": {
                "down": face([0, 0, 16, 16], "bottom", None),
                "up": face([0, 0, 16, 16], "top", Some("up")),
                "north": face([0, 0, 16, 8], "side", Some("north")),
                "south": face([0, 0, 16, 8], "side", Some("south")),
                "west": face([0, 0, 16, 8], "side", Some("west")),
                "east": face([0, 0, 16, 8], "side", Some("east")),
            }});
            let models = vec![
                (
                    format!("block/{id}"),
                    model(bottom, top, side, vec![lower()], false),
                ),
                (
                    format!("block/{id}_top"),
                    model(bottom, top, side, vec![upper], false),
                ),
                (
                    format!("block/{id}_double"),
                    crate::assets::six_model(double),
                ),
            ];
            let definition = json!({"variants": {
                "type=bottom": {"model": format!("minecraft:block/{id}")},
                "type=double": {"model": format!("minecraft:block/{id}_double")},
                "type=top": {"model": format!("minecraft:block/{id}_top")},
            }});
            (models, definition)
        }
        Faces::Stairs { bottom, top, side } => {
            let straight = json!({"from": [8, 8, 0], "to": [16, 16, 16], "faces": {
                "up": face([8, 0, 16, 16], "top", Some("up")),
                "north": face([0, 0, 8, 8], "side", Some("north")),
                "south": face([8, 0, 16, 8], "side", Some("south")),
                "west": face([0, 0, 16, 8], "side", None),
                "east": face([0, 0, 16, 8], "side", Some("east")),
            }});
            let inner = json!({"from": [0, 8, 8], "to": [8, 16, 16], "faces": {
                "up": face([0, 8, 8, 16], "top", Some("up")),
                "north": face([8, 0, 16, 8], "side", None),
                "south": face([0, 0, 8, 8], "side", Some("south")),
                "west": face([8, 0, 16, 8], "side", Some("west")),
            }});
            let outer = json!({"from": [8, 8, 8], "to": [16, 16, 16], "faces": {
                "up": face([8, 8, 16, 16], "top", Some("up")),
                "north": face([0, 0, 8, 8], "side", None),
                "south": face([8, 0, 16, 8], "side", Some("south")),
                "west": face([8, 0, 16, 8], "side", None),
                "east": face([0, 0, 8, 8], "side", Some("east")),
            }});
            let models = vec![
                (
                    format!("block/{id}"),
                    model(bottom, top, side, vec![lower(), straight.clone()], true),
                ),
                (
                    format!("block/{id}_inner"),
                    model(bottom, top, side, vec![lower(), straight, inner], true),
                ),
                (
                    format!("block/{id}_outer"),
                    model(bottom, top, side, vec![lower(), outer], true),
                ),
            ];
            let mut variants = serde_json::Map::new();
            // The models face east; the others turn by a quarter per step.
            for (facing, base) in [("east", 0), ("south", 90), ("west", 180), ("north", 270)] {
                for half in ["bottom", "top"] {
                    for shape in [
                        "inner_left",
                        "inner_right",
                        "outer_left",
                        "outer_right",
                        "straight",
                    ] {
                        let suffix = match shape {
                            "straight" => "",
                            s if s.starts_with("inner") => "_inner",
                            _ => "_outer",
                        };
                        let left = shape.ends_with("_left");
                        let right = shape.ends_with("_right");
                        let y = match half {
                            "bottom" if left => base + 270,
                            "top" if right => base + 90,
                            _ => base,
                        } % 360;
                        let x = if half == "top" { 180 } else { 0 };
                        let mut variant = json!({"model": format!("minecraft:block/{id}{suffix}")});
                        if x != 0 {
                            variant["x"] = json!(x);
                        }
                        if y != 0 {
                            variant["y"] = json!(y);
                        }
                        if x != 0 || y != 0 {
                            variant["uvlock"] = json!(true);
                        }
                        variants.insert(
                            format!("facing={facing},half={half},shape={shape}"),
                            variant,
                        );
                    }
                }
            }
            (models, json!({"variants": variants}))
        }
        _ => panic!("{id} is not a slab or stairs"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::boxes;

    #[test]
    fn every_slab_and_stairs_is_listed() {
        let lines = lines();
        assert_eq!(lines.iter().filter(|l| !l.stairs).count(), 101);
        assert_eq!(lines.iter().filter(|l| l.stairs).count(), 97);
        let oak = line("oak_stairs").unwrap();
        assert_eq!(
            (oak.hardness, oak.resistance, oak.instrument),
            (2.0, 3.0, "BASS")
        );
    }

    #[test]
    fn stairs_take_minecraft_shapes() {
        let state = |facing, half, shape| {
            boxes(
                &stairs(&[
                    ("facing", facing),
                    ("half", half),
                    ("shape", shape),
                    ("waterlogged", "false"),
                ])
                .outline,
            )
        };
        assert_eq!(
            state("north", "bottom", "straight"),
            vec![
                [0.0, 0.0, 0.0, 1.0, 0.5, 1.0],
                [0.0, 0.5, 0.0, 1.0, 1.0, 0.5]
            ]
        );
        assert_eq!(
            state("north", "top", "straight"),
            vec![
                [0.0, 0.0, 0.0, 1.0, 1.0, 0.5],
                [0.0, 0.5, 0.5, 1.0, 1.0, 1.0]
            ]
        );
        let slab = slab(&[("type", "bottom"), ("waterlogged", "true")]);
        assert_eq!(slab.dampening(), 1);
        assert!(!slab.full_collision());
    }
}
