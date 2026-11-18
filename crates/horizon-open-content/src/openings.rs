//! Doors, trapdoors, buttons and pressure plates, from
//! `data/blocks/openings.txt` (written by `tools/open_block_facts.py`, in
//! `connected.txt`'s format): their shapes per state and their models. The
//! blocks themselves are added with the connected ones
//! ([`crate::connected::add`]).
use crate::blocks::{Block, Faces};
use crate::connected::{FaceSpec, Family, Line, Shape, element, template, tex, value};
use serde_json::{Map, Value, json};

const OPENINGS: &str = include_str!("../data/blocks/openings.txt");

/// Every line, in block registry order.
pub fn lines() -> Vec<Line> {
    crate::connected::parse(OPENINGS)
}

/// A horizontal direction's quarter turns clockwise from north.
fn quarter(side: &str) -> u32 {
    match side {
        "north" => 0,
        "east" => 1,
        "south" => 2,
        _ => 3,
    }
}

const SIDES: [&str; 4] = ["north", "east", "south", "west"];

fn turn(side: &str, quarters: u32) -> &'static str {
    SIDES[((quarter(side) + quarters) % 4) as usize]
}

/// A slab three sixteenths thick against one side of the block.
fn against(side: &str) -> [f64; 6] {
    match side {
        "north" => [0.0, 0.0, 0.0, 16.0, 16.0, 3.0],
        "south" => [0.0, 0.0, 13.0, 16.0, 16.0, 16.0],
        "west" => [0.0, 0.0, 0.0, 3.0, 16.0, 16.0],
        _ => [13.0, 0.0, 0.0, 16.0, 16.0, 16.0],
    }
}

/// A state's boxes in sixteenths of a block.
pub fn boxes(family: Family, state: &[(&str, &str)], shape: Shape) -> Vec<[f64; 6]> {
    let facing = value(state, "facing");
    let open = value(state, "open") == "true";
    match family {
        // Closed, against the side behind the player who placed it; open,
        // swung to the side of its hinge.
        Family::Door => {
            let side = match (open, value(state, "hinge")) {
                (false, _) => turn(facing, 2),
                (true, "left") => turn(facing, 3),
                (true, _) => turn(facing, 1),
            };
            vec![against(side)]
        }
        Family::Trapdoor | Family::OrientableTrapdoor => vec![if open {
            against(turn(facing, 2))
        } else if value(state, "half") == "top" {
            [0.0, 13.0, 0.0, 16.0, 16.0, 16.0]
        } else {
            [0.0, 0.0, 0.0, 16.0, 3.0, 16.0]
        }],
        _ if shape == Shape::Collision => Vec::new(),
        Family::Button => {
            let depth = if value(state, "powered") == "true" {
                1.0
            } else {
                2.0
            };
            let across = matches!(facing, "east" | "west");
            let (a0, a1, b0, b1) = if across {
                (6.0, 10.0, 5.0, 11.0)
            } else {
                (5.0, 11.0, 6.0, 10.0)
            };
            vec![match value(state, "face") {
                "floor" => [a0, 0.0, b0, a1, depth, b1],
                "ceiling" => [a0, 16.0 - depth, b0, a1, 16.0, b1],
                _ => match facing {
                    "north" => [5.0, 6.0, 16.0 - depth, 11.0, 10.0, 16.0],
                    "south" => [5.0, 6.0, 0.0, 11.0, 10.0, depth],
                    "west" => [16.0 - depth, 6.0, 5.0, 16.0, 10.0, 11.0],
                    _ => [0.0, 6.0, 5.0, depth, 10.0, 11.0],
                },
            }]
        }
        _ => vec![[
            1.0,
            0.0,
            1.0,
            15.0,
            if pressed(state) { 0.5 } else { 1.0 },
            15.0,
        ]],
    }
}

/// Whether a plate is pressed (`powered`, or a weighted plate's `power`).
fn pressed(state: &[(&str, &str)]) -> bool {
    value(state, "powered") == "true" || !matches!(value(state, "power"), "" | "0")
}

/// One face with a rotation of its texture.
fn turned_face(face: FaceSpec, rotation: u32) -> (String, Value) {
    let (dir, uv, texture, cull) = face;
    let mut value = element([0.0; 3], [1.0; 3], &[face])["faces"][dir].clone();
    let _ = (uv, texture, cull);
    if rotation != 0 {
        value["rotation"] = json!(rotation);
    }
    (dir.to_owned(), value)
}

fn door(top: bool, right: bool, open: bool) -> Value {
    let t = if top { "top" } else { "bottom" };
    let s = |a: f64, b: f64, c: f64, d: f64| Some([a, b, c, d]);
    let (n, so, w, e) = match (right, open) {
        (false, false) => ([3.0, 0.0], [0.0, 3.0], [0.0, 16.0], [16.0, 0.0]),
        (false, true) => ([0.0, 3.0], [0.0, 3.0], [16.0, 0.0], [0.0, 16.0]),
        (true, false) => ([3.0, 0.0], [0.0, 3.0], [16.0, 0.0], [0.0, 16.0]),
        (true, true) => ([3.0, 0.0], [3.0, 0.0], [0.0, 16.0], [16.0, 0.0]),
    };
    let side = |u: [f64; 2]| s(u[0], 0.0, u[1], 16.0);
    let (end, uv, rotation) = match (top, right, open) {
        (false, false, false) => ("down", [16.0, 13.0, 0.0, 16.0], 90),
        (false, false, true) => ("down", [0.0, 16.0, 16.0, 13.0], 90),
        (false, true, false) => ("down", [0.0, 13.0, 16.0, 16.0], 90),
        (false, true, true) => ("down", [16.0, 16.0, 0.0, 13.0], 90),
        (true, false, false) => ("up", [0.0, 3.0, 16.0, 0.0], 90),
        (true, false, true) => ("up", [0.0, 3.0, 16.0, 0.0], 270),
        (true, true, false) => ("up", [0.0, 0.0, 16.0, 3.0], 270),
        (true, true, true) => ("up", [0.0, 0.0, 16.0, 3.0], 90),
    };
    let mut faces = Map::new();
    for face in [
        (end, Some(uv), t, Some(end)),
        ("north", side(n), t, Some("north")),
        ("south", side(so), t, Some("south")),
        ("west", side(w), t, Some("west")),
        ("east", side(e), t, None),
    ] {
        let (dir, value) = turned_face(face, if face.0 == end { rotation } else { 0 });
        faces.insert(dir, value);
    }
    let mut model = template(Vec::new(), t, false);
    model["elements"] = json!([{"from": [0, 0, 0], "to": [3, 16, 16], "faces": faces}]);
    model
}

fn slab(from: [f64; 3], to: [f64; 3], faces: &[(FaceSpec, u32)]) -> Value {
    let mut map = Map::new();
    for (face, rotation) in faces {
        let (dir, value) = turned_face(*face, *rotation);
        map.insert(dir, value);
    }
    let mut model = template(Vec::new(), "texture", true);
    model["elements"] = json!([{"from": from, "to": to, "faces": map}]);
    model
}

/// The shared templates, by model path (`block/open_<name>`).
pub fn templates() -> Vec<(&'static str, Value)> {
    let t = "texture";
    let s = |a: f64, b: f64, c: f64, d: f64| Some([a, b, c, d]);
    let full = s(0.0, 0.0, 16.0, 16.0);
    let mut list = Vec::new();
    for (name, top, right, open) in [
        ("door_bottom_left", false, false, false),
        ("door_bottom_left_open", false, false, true),
        ("door_bottom_right", false, true, false),
        ("door_bottom_right_open", false, true, true),
        ("door_top_left", true, false, false),
        ("door_top_left_open", true, false, true),
        ("door_top_right", true, true, false),
        ("door_top_right_open", true, true, true),
    ] {
        list.push((name, door(top, right, open)));
    }
    // Trapdoors: the plain ones keep the texture upright; the orientable
    // ones turn it with the trapdoor.
    for orientable in [false, true] {
        let edge = if orientable {
            s(0.0, 0.0, 16.0, 3.0)
        } else {
            s(0.0, 16.0, 16.0, 13.0)
        };
        let lid = if orientable {
            s(0.0, 16.0, 16.0, 0.0)
        } else {
            full
        };
        let edges = |cull: bool| {
            ["north", "south", "west", "east"].map(|d| ((d, edge, t, cull.then_some(d)), 0))
        };
        let mut bottom = vec![
            (("down", full, t, Some("down")), 0),
            (("up", lid, t, None), 0),
        ];
        bottom.extend(edges(true));
        let mut top = vec![
            (("down", full, t, None), 0),
            (("up", lid, t, Some("up")), 0),
        ];
        top.extend(edges(true));
        let open = if orientable {
            vec![
                (("down", s(0.0, 0.0, 16.0, 3.0), t, Some("down")), 0),
                (("up", s(0.0, 3.0, 16.0, 0.0), t, Some("up")), 0),
                (("north", s(0.0, 16.0, 16.0, 0.0), t, None), 0),
                (("south", s(0.0, 16.0, 16.0, 0.0), t, Some("south")), 0),
                (("west", s(0.0, 0.0, 16.0, 3.0), t, Some("west")), 90),
                (("east", s(0.0, 3.0, 16.0, 0.0), t, Some("east")), 90),
            ]
        } else {
            vec![
                (("down", s(0.0, 13.0, 16.0, 16.0), t, Some("down")), 0),
                (("up", s(0.0, 16.0, 16.0, 13.0), t, Some("up")), 0),
                (("north", full, t, None), 0),
                (("south", full, t, Some("south")), 0),
                (("west", s(16.0, 0.0, 13.0, 16.0), t, Some("west")), 0),
                (("east", s(13.0, 0.0, 16.0, 16.0), t, Some("east")), 0),
            ]
        };
        let names: [&'static str; 3] = if orientable {
            [
                "orientable_trapdoor_bottom",
                "orientable_trapdoor_top",
                "orientable_trapdoor_open",
            ]
        } else {
            ["trapdoor_bottom", "trapdoor_top", "trapdoor_open"]
        };
        list.push((names[0], slab([0.0; 3], [16.0, 3.0, 16.0], &bottom)));
        list.push((names[1], slab([0.0, 13.0, 0.0], [16.0; 3], &top)));
        list.push((names[2], slab([0.0, 0.0, 13.0], [16.0; 3], &open)));
    }
    for (name, high) in [("button", 2.0), ("button_pressed", 1.02)] {
        let v = if high > 1.5 { 16.0 } else { 15.0 };
        list.push((
            name,
            slab(
                [5.0, 0.0, 6.0],
                [11.0, high, 10.0],
                &[
                    (("down", s(5.0, 6.0, 11.0, 10.0), t, Some("down")), 0),
                    (("up", s(5.0, 6.0, 11.0, 10.0), t, None), 0),
                    (("north", s(5.0, 14.0, 11.0, v), t, None), 0),
                    (("south", s(5.0, 14.0, 11.0, v), t, None), 0),
                    (("west", s(6.0, 14.0, 10.0, v), t, None), 0),
                    (("east", s(6.0, 14.0, 10.0, v), t, None), 0),
                ],
            ),
        ));
    }
    for (name, high) in [("pressure_plate_up", 1.0), ("pressure_plate_down", 0.5)] {
        let side = s(1.0, 15.0, 15.0, 15.0 + high);
        list.push((
            name,
            slab(
                [1.0, 0.0, 1.0],
                [15.0, high, 15.0],
                &[
                    (("down", s(1.0, 1.0, 15.0, 15.0), t, Some("down")), 0),
                    (("up", s(1.0, 1.0, 15.0, 15.0), t, None), 0),
                    (("north", side, t, None), 0),
                    (("south", side, t, None), 0),
                    (("west", side, t, None), 0),
                    (("east", side, t, None), 0),
                ],
            ),
        ));
    }
    list
}

/// The block's models (children of the templates) and its block-state
/// definition.
pub fn models(block: &Block, family: Family) -> (Vec<(String, Value)>, Value) {
    let Faces::Connected { texture, edge } = block.faces else {
        unreachable!("{} has connected faces", block.id)
    };
    let id = block.id;
    let (textures, parts): (Value, Vec<(String, String)>) = match family {
        Family::Door => (
            json!({"top": tex(texture), "bottom": tex(edge)}),
            ["bottom", "top"]
                .iter()
                .flat_map(|half| {
                    ["left", "left_open", "right", "right_open"].map(|hinge| {
                        let name = format!("door_{half}_{hinge}");
                        (format!("_{half}_{hinge}"), name)
                    })
                })
                .collect(),
        ),
        Family::Trapdoor | Family::OrientableTrapdoor => {
            let prefix = if family == Family::OrientableTrapdoor {
                "orientable_"
            } else {
                ""
            };
            (
                json!({"texture": tex(texture)}),
                ["bottom", "top", "open"]
                    .map(|p| (format!("_{p}"), format!("{prefix}trapdoor_{p}")))
                    .to_vec(),
            )
        }
        Family::Button => (
            json!({"texture": tex(texture)}),
            vec![
                (String::new(), "button".into()),
                ("_pressed".into(), "button_pressed".into()),
            ],
        ),
        _ => (
            json!({"texture": tex(texture)}),
            vec![
                (String::new(), "pressure_plate_up".into()),
                ("_down".into(), "pressure_plate_down".into()),
            ],
        ),
    };
    let models = parts
        .iter()
        .map(|(suffix, template)| {
            (
                format!("block/{id}{suffix}"),
                json!({"parent": format!("minecraft:block/open_{template}"), "textures": textures}),
            )
        })
        .collect();
    let variant = |suffix: &str, x: u32, y: u32, uvlock: bool| {
        let mut v = json!({"model": format!("minecraft:block/{id}{suffix}")});
        if x != 0 {
            v["x"] = json!(x);
        }
        if y % 360 != 0 {
            v["y"] = json!(y % 360);
        }
        if uvlock {
            v["uvlock"] = json!(true);
        }
        v
    };
    // Quarter turns, as y rotations, from north.
    let y = |side: &str| quarter(side) * 90;
    let mut variants = Map::new();
    match family {
        Family::Door => {
            for facing in SIDES {
                for half in ["lower", "upper"] {
                    for hinge in ["left", "right"] {
                        for open in ["false", "true"] {
                            let part = if half == "lower" { "bottom" } else { "top" };
                            let suffix = if open == "true" {
                                format!("_{part}_{hinge}_open")
                            } else {
                                format!("_{part}_{hinge}")
                            };
                            // Closed, east is unturned; open, a quarter
                            // more towards the hinge.
                            let turn = match (open, hinge) {
                                ("false", _) => 0,
                                (_, "left") => 90,
                                _ => 270,
                            };
                            variants.insert(
                                format!("facing={facing},half={half},hinge={hinge},open={open}"),
                                variant(&suffix, 0, y(facing) + 270 + turn, false),
                            );
                        }
                    }
                }
            }
        }
        Family::Trapdoor | Family::OrientableTrapdoor => {
            let orientable = family == Family::OrientableTrapdoor;
            for facing in SIDES {
                for half in ["bottom", "top"] {
                    for open in ["false", "true"] {
                        let v = match (open, half, orientable) {
                            ("true", "top", true) => variant("_open", 180, y(facing) + 180, false),
                            ("true", _, _) => variant("_open", 0, y(facing), false),
                            (_, _, true) => variant(&format!("_{half}"), 0, y(facing), false),
                            _ => variant(&format!("_{half}"), 0, 0, false),
                        };
                        variants.insert(format!("facing={facing},half={half},open={open}"), v);
                    }
                }
            }
        }
        Family::Button => {
            for face in ["ceiling", "floor", "wall"] {
                for facing in SIDES {
                    for powered in ["false", "true"] {
                        let suffix = if powered == "true" { "_pressed" } else { "" };
                        let v = match face {
                            "floor" => variant(suffix, 0, y(facing), false),
                            "wall" => variant(suffix, 90, y(facing), true),
                            _ => variant(suffix, 180, y(facing) + 180, false),
                        };
                        variants
                            .insert(format!("face={face},facing={facing},powered={powered}"), v);
                    }
                }
            }
        }
        Family::WeightedPlate => {
            for power in 0..16 {
                let suffix = if power == 0 { "" } else { "_down" };
                variants.insert(format!("power={power}"), variant(suffix, 0, 0, false));
            }
        }
        _ => {
            variants.insert("powered=false".into(), variant("", 0, 0, false));
            variants.insert("powered=true".into(), variant("_down", 0, 0, false));
        }
    }
    (models, json!({"variants": variants}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_families_are_listed_with_minecraft_numbers() {
        let lines = lines();
        let count = |f: Family| lines.iter().filter(|l| l.family == f).count();
        assert_eq!(count(Family::Door), 22);
        assert_eq!(
            count(Family::Trapdoor) + count(Family::OrientableTrapdoor),
            22
        );
        assert_eq!(count(Family::Button), 15);
        assert_eq!(count(Family::Plate) + count(Family::WeightedPlate), 17);
        let door = lines.iter().find(|l| l.id == "iron_door").unwrap();
        assert_eq!(door.textures, ("iron_door_top", "iron_door_bottom"));
    }

    #[test]
    fn doors_swing_to_their_hinge_and_plates_never_collide() {
        let state = [("facing", "north"), ("hinge", "left"), ("open", "true")];
        assert_eq!(
            boxes(Family::Door, &state, Shape::Collision),
            vec![[0.0, 0.0, 0.0, 3.0, 16.0, 16.0]]
        );
        assert!(boxes(Family::Plate, &[("powered", "true")], Shape::Collision).is_empty());
        assert_eq!(
            boxes(Family::WeightedPlate, &[("power", "3")], Shape::Outline),
            vec![[1.0, 0.0, 1.0, 15.0, 0.5, 15.0]]
        );
    }
}
