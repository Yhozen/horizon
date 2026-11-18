//! Walls, fences, fence gates, glass panes and bars, from
//! `data/blocks/connected.txt` (written by `tools/open_block_facts.py`):
//! Minecraft's numbers and texture names, with shapes and models written
//! here. Every state has Minecraft's collision, outline and light occlusion
//! boxes (fences, walls and closed gates collide 1.5 blocks high), and the
//! models are drawn as multipart block states that join the parts each
//! state's connections name. The doors, trapdoors, buttons and pressure
//! plates of `data/blocks/openings.txt` share the format; their shapes and
//! models are in [`crate::openings`].
use crate::blocks::{Block, Category, Drop, Faces, Form, Layer, Mining, Tier, Tool};
use horizon_core::geometry::{Axis, VoxelShape};
use serde_json::{Map, Value, json};

const CONNECTED: &str = include_str!("../data/blocks/connected.txt");

/// Which shapes and models a block has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Wall,
    Fence,
    Gate,
    Pane,
    Bars,
    Door,
    Trapdoor,
    /// A trapdoor whose texture turns with `facing`.
    OrientableTrapdoor,
    Button,
    Plate,
    /// A weighted plate: `power` rather than `powered`.
    WeightedPlate,
}
impl Family {
    /// Whether it is one of [`crate::openings`]'s.
    pub fn opening(self) -> bool {
        !matches!(
            self,
            Self::Wall | Self::Fence | Self::Gate | Self::Pane | Self::Bars
        )
    }
}

/// One line of `connected.txt`.
#[derive(Debug)]
pub struct Line {
    pub id: &'static str,
    pub classes: &'static [&'static str],
    pub hardness: f32,
    pub resistance: f32,
    pub map_color: u32,
    pub sound: &'static str,
    pub mining: Mining,
    pub family: Family,
    /// The texture, and the edge texture of panes and bars.
    pub textures: (&'static str, &'static str),
}

/// Every line, in block registry order.
pub fn lines() -> Vec<Line> {
    parse(CONNECTED)
}

/// The lines of `connected.txt` or `openings.txt`.
pub fn parse(text: &'static str) -> Vec<Line> {
    text.lines()
        .skip(1)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let f: Vec<&'static str> = line.split('\t').collect();
            assert_eq!(f.len(), 11, "connected.txt: {line}");
            let num = |i: usize| -> f32 {
                f[i].parse()
                    .unwrap_or_else(|_| panic!("connected.txt: {} field {i}", f[0]))
            };
            let tool = match f[6] {
                "pickaxe" => Tool::Pickaxe,
                "axe" => Tool::Axe,
                "shovel" => Tool::Shovel,
                "hoe" => Tool::Hoe,
                _ => Tool::None,
            };
            let family = match f[9] {
                "wall" => Family::Wall,
                "fence" => Family::Fence,
                "gate" => Family::Gate,
                "pane" => Family::Pane,
                "bars" => Family::Bars,
                "door" => Family::Door,
                "trapdoor" => Family::Trapdoor,
                "orientable_trapdoor" => Family::OrientableTrapdoor,
                "button" => Family::Button,
                "plate" => Family::Plate,
                "weighted_plate" => Family::WeightedPlate,
                other => panic!("connected.txt: {} has the family {other}", f[0]),
            };
            let textures = f[10].split_once(',').unwrap_or((f[10], f[10]));
            Line {
                id: f[0],
                classes: Box::leak(f[1].split(',').collect::<Vec<_>>().into_boxed_slice()),
                hardness: num(2),
                resistance: num(3),
                map_color: u32::from_str_radix(f[4], 16).expect("connected.txt: map colour"),
                sound: f[5],
                mining: Mining {
                    tool,
                    requires_tool: f[8] == "1",
                    needs: num(7) as u8,
                },
                family,
                textures,
            }
        })
        .collect()
}

/// Adds the families to the palette, in the file's order, each dropping
/// itself.
pub fn add(blocks: &mut Vec<Block>) {
    for line in lines().into_iter().chain(crate::openings::lines()) {
        assert!(
            blocks.iter().all(|b| b.id != line.id),
            "{} is authored twice",
            line.id
        );
        let colored = crate::cubes::COLORS
            .iter()
            .any(|c| line.id.starts_with(&format!("{c}_")));
        blocks.push(Block {
            id: line.id,
            name: crate::cubes::title(line.id),
            tier: Tier::Palette,
            form: Form::Connected(line.family),
            layer: match line.family {
                Family::Pane => Layer::Translucent,
                Family::Bars | Family::Door | Family::Trapdoor | Family::OrientableTrapdoor => {
                    Layer::Cutout
                }
                _ => Layer::Solid,
            },
            faces: Faces::Connected {
                texture: line.textures.0,
                edge: line.textures.1,
            },
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
            category: Some(if colored {
                Category::Colored
            } else {
                Category::Construction
            }),
            mining: line.mining,
            drop: Drop::Itself,
            friction: 0.6,
        });
    }
}

/// Which of a state's shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Collision,
    Outline,
    Occlusion,
}

type Boxes = Vec<[f64; 6]>;

/// A box in sixteenths of a block.
fn px(b: [f64; 6]) -> [f64; 6] {
    b.map(|v| v / 16.0)
}

pub(crate) fn value<'a>(state: &[(&str, &'a str)], key: &str) -> &'a str {
    state.iter().find(|(k, _)| *k == key).map_or("", |(_, v)| v)
}

/// The arm towards each side, from a box reaching north, turned as
/// Minecraft turns horizontal shapes (east is the north box turned a
/// quarter clockwise seen from above).
fn arm(side: &str, [x0, y0, z0, x1, y1, z1]: [f64; 6]) -> [f64; 6] {
    match side {
        "north" => [x0, y0, z0, x1, y1, z1],
        "south" => [16.0 - x1, y0, 16.0 - z1, 16.0 - x0, y1, 16.0 - z0],
        "east" => [16.0 - z1, y0, x0, 16.0 - z0, y1, x1],
        _ => [z0, y0, 16.0 - x1, z1, y1, 16.0 - x0],
    }
}

const SIDES: [&str; 4] = ["north", "east", "south", "west"];

/// A state's boxes (in blocks), unmerged.
pub fn boxes(family: Family, state: &[(&str, &str)], shape: Shape) -> Boxes {
    if family.opening() {
        return crate::openings::boxes(family, state, shape)
            .into_iter()
            .map(px)
            .collect();
    }
    let mut out = Vec::new();
    match family {
        Family::Fence => {
            let high = if shape == Shape::Collision {
                24.0
            } else {
                16.0
            };
            out.push([6.0, 0.0, 6.0, 10.0, 16.0, 10.0]);
            if shape != Shape::Occlusion {
                out[0][4] = high;
            }
            for side in SIDES.iter().filter(|s| value(state, s) == "true") {
                out.push(arm(
                    side,
                    match shape {
                        Shape::Occlusion => [7.0, 6.0, 0.0, 9.0, 15.0, 6.0],
                        _ => [6.0, 0.0, 0.0, 10.0, high, 6.0],
                    },
                ));
            }
        }
        Family::Wall => {
            let collision = shape == Shape::Collision;
            if value(state, "up") == "true" {
                out.push([
                    4.0,
                    0.0,
                    4.0,
                    12.0,
                    if collision { 24.0 } else { 16.0 },
                    12.0,
                ]);
            }
            for side in SIDES {
                let high = match value(state, side) {
                    _ if collision && value(state, side) != "none" => 24.0,
                    "low" => 14.0,
                    "tall" => 16.0,
                    _ => continue,
                };
                out.push(arm(side, [5.0, 0.0, 0.0, 11.0, high, 11.0]));
            }
        }
        Family::Pane | Family::Bars => {
            out.push([7.0, 0.0, 7.0, 9.0, 16.0, 9.0]);
            for side in SIDES.iter().filter(|s| value(state, s) == "true") {
                out.push(arm(side, [7.0, 0.0, 0.0, 9.0, 16.0, 9.0]));
            }
        }
        Family::Gate => {
            let in_wall = value(state, "in_wall") == "true";
            match shape {
                Shape::Collision if value(state, "open") == "true" => {}
                Shape::Collision => out.push([0.0, 0.0, 6.0, 16.0, 24.0, 10.0]),
                Shape::Outline => {
                    out.push([0.0, 0.0, 6.0, 16.0, if in_wall { 13.0 } else { 16.0 }, 10.0])
                }
                Shape::Occlusion => {
                    let (low, high) = if in_wall { (2.0, 13.0) } else { (5.0, 16.0) };
                    out.push([0.0, low, 7.0, 2.0, high, 9.0]);
                    out.push([14.0, low, 7.0, 16.0, high, 9.0]);
                }
            }
            if matches!(value(state, "facing"), "east" | "west") {
                out = out.into_iter().map(|b| arm("east", b)).collect();
            }
        }
        _ => unreachable!("openings shape through crate::openings"),
    }
    out.into_iter().map(px).collect()
}

/// The voxel shape of a union of boxes: a grid on their sorted corners.
pub fn voxel(boxes: &[[f64; 6]]) -> VoxelShape {
    if boxes.is_empty() {
        return VoxelShape::empty();
    }
    let coords: [Vec<f64>; 3] = std::array::from_fn(|axis| {
        let mut c: Vec<f64> = boxes.iter().flat_map(|b| [b[axis], b[axis + 3]]).collect();
        c.sort_by(f64::total_cmp);
        c.dedup();
        c
    });
    let sizes = coords.clone().map(|c| c.len() - 1);
    let mut occupied = vec![false; sizes[0] * sizes[1] * sizes[2]];
    for x in 0..sizes[0] {
        for y in 0..sizes[1] {
            for z in 0..sizes[2] {
                let centre = [
                    (coords[0][x] + coords[0][x + 1]) / 2.0,
                    (coords[1][y] + coords[1][y + 1]) / 2.0,
                    (coords[2][z] + coords[2][z + 1]) / 2.0,
                ];
                occupied[(x * sizes[1] + y) * sizes[2] + z] = boxes
                    .iter()
                    .any(|b| (0..3).all(|a| b[a] < centre[a] && centre[a] < b[a + 3]));
            }
        }
    }
    VoxelShape::from_grid(coords, &occupied).expect("a valid grid")
}

/// A state's merged boxes, as Minecraft lists a shape's boxes.
pub fn merged(family: Family, state: &[(&str, &str)], shape: Shape) -> Boxes {
    voxel(&boxes(family, state, shape))
        .boxes()
        .iter()
        .map(|b| {
            [
                b.min(Axis::X),
                b.min(Axis::Y),
                b.min(Axis::Z),
                b.max(Axis::X),
                b.max(Axis::Y),
                b.max(Axis::Z),
            ]
        })
        .collect()
}

/// Whether a state holds water.
pub fn waterlogged(state: &[(&str, &str)]) -> bool {
    value(state, "waterlogged") == "true"
}

// Models: templates with a texture variable, and per block a child naming
// its textures, as Minecraft's are organised.

/// One face: its UV (or the default from the element's bounds), texture
/// variable and cull face.
pub(crate) type FaceSpec<'a> = (&'a str, Option<[f64; 4]>, &'a str, Option<&'a str>);

pub(crate) fn element(from: [f64; 3], to: [f64; 3], faces: &[FaceSpec]) -> Value {
    let mut map = Map::new();
    for (dir, uv, texture, cull) in faces {
        let mut face = json!({"texture": format!("#{texture}")});
        if let Some(uv) = uv {
            face["uv"] = json!(uv);
        }
        if let Some(cull) = cull {
            face["cullface"] = json!(cull);
        }
        map.insert((*dir).to_owned(), face);
    }
    json!({"from": from, "to": to, "faces": map})
}

pub(crate) fn template(elements: Vec<Value>, particle: &str, occlusion: bool) -> Value {
    let mut model = json!({
        "parent": "minecraft:block/open_base",
        "textures": {"particle": format!("#{particle}")},
        "elements": elements,
    });
    if !occlusion {
        model["ambientocclusion"] = json!(false);
    }
    model
}

/// The gate's closed leaves, then (`open`) the leaves swung back; both
/// with the two posts, `drop` sixteenths lower in a wall.
fn gate(open: bool, drop: f64) -> Vec<Value> {
    let t = "texture";
    let lower = |e: (Value, [f64; 3], [f64; 3])| {
        let (mut v, from, to) = e;
        v["from"] = json!([from[0], from[1] - drop, from[2]]);
        v["to"] = json!([to[0], to[1] - drop, to[2]]);
        v
    };
    let mut parts: Vec<(Value, [f64; 3], [f64; 3])> = Vec::new();
    let mut push = |from: [f64; 3], to: [f64; 3], faces: &[FaceSpec]| {
        parts.push((element(from, to, faces), from, to));
    };
    // The posts.
    let s = |a: f64, b: f64, c: f64, d: f64| Some([a, b, c, d]);
    push(
        [0.0, 5.0, 7.0],
        [2.0, 16.0, 9.0],
        &[
            ("down", s(0.0, 7.0, 2.0, 9.0), t, None),
            ("up", s(0.0, 7.0, 2.0, 9.0), t, None),
            ("north", s(0.0, 0.0, 2.0, 11.0), t, None),
            ("south", s(0.0, 0.0, 2.0, 11.0), t, None),
            ("west", s(7.0, 0.0, 9.0, 11.0), t, Some("west")),
            ("east", s(7.0, 0.0, 9.0, 11.0), t, None),
        ],
    );
    push(
        [14.0, 5.0, 7.0],
        [16.0, 16.0, 9.0],
        &[
            ("down", s(14.0, 7.0, 16.0, 9.0), t, None),
            ("up", s(14.0, 7.0, 16.0, 9.0), t, None),
            ("north", s(14.0, 0.0, 16.0, 11.0), t, None),
            ("south", s(14.0, 0.0, 16.0, 11.0), t, None),
            ("west", s(7.0, 0.0, 9.0, 11.0), t, None),
            ("east", s(7.0, 0.0, 9.0, 11.0), t, Some("east")),
        ],
    );
    if !open {
        for x in [6.0, 8.0] {
            push(
                [x, 6.0, 7.0],
                [x + 2.0, 15.0, 9.0],
                &[
                    ("down", s(x, 7.0, x + 2.0, 9.0), t, None),
                    ("up", s(x, 7.0, x + 2.0, 9.0), t, None),
                    ("north", s(x, 1.0, x + 2.0, 10.0), t, None),
                    ("south", s(x, 1.0, x + 2.0, 10.0), t, None),
                    ("west", s(7.0, 1.0, 9.0, 10.0), t, None),
                    ("east", s(7.0, 1.0, 9.0, 10.0), t, None),
                ],
            );
        }
        for x in [2.0, 10.0] {
            for (y, v) in [(6.0, 7.0), (12.0, 1.0)] {
                push(
                    [x, y, 7.0],
                    [x + 4.0, y + 3.0, 9.0],
                    &[
                        ("down", s(x, 7.0, x + 4.0, 9.0), t, None),
                        ("up", s(x, 7.0, x + 4.0, 9.0), t, None),
                        ("north", s(x, v, x + 4.0, v + 3.0), t, None),
                        ("south", s(x, v, x + 4.0, v + 3.0), t, None),
                    ],
                );
            }
        }
    } else {
        for x in [0.0, 14.0] {
            push(
                [x, 6.0, 13.0],
                [x + 2.0, 15.0, 15.0],
                &[
                    ("down", s(x, 13.0, x + 2.0, 15.0), t, None),
                    ("up", s(x, 13.0, x + 2.0, 15.0), t, None),
                    ("north", s(x, 1.0, x + 2.0, 10.0), t, None),
                    ("south", s(x, 1.0, x + 2.0, 10.0), t, None),
                    ("west", s(13.0, 1.0, 15.0, 10.0), t, None),
                    ("east", s(13.0, 1.0, 15.0, 10.0), t, None),
                ],
            );
        }
        for x in [0.0, 14.0] {
            for (y, v) in [(6.0, 7.0), (12.0, 1.0)] {
                push(
                    [x, y, 9.0],
                    [x + 2.0, y + 3.0, 13.0],
                    &[
                        ("down", s(x, 9.0, x + 2.0, 13.0), t, None),
                        ("up", s(x, 9.0, x + 2.0, 13.0), t, None),
                        ("west", s(13.0, v, 15.0, v + 3.0), t, None),
                        ("east", s(13.0, v, 15.0, v + 3.0), t, None),
                    ],
                );
            }
        }
    }
    parts.into_iter().map(lower).collect()
}

/// The shared templates, by model path (`block/open_<name>`).
pub fn templates() -> Vec<(&'static str, Value)> {
    let s = |a: f64, b: f64, c: f64, d: f64| Some([a, b, c, d]);
    let (t, w) = ("texture", "wall");
    let sides = |uv: Option<[f64; 4]>, tex| -> [FaceSpec; 4] {
        [
            ("north", uv, tex, None),
            ("south", uv, tex, None),
            ("west", uv, tex, None),
            ("east", uv, tex, None),
        ]
    };
    // A square post standing on the block's floor.
    let post = |from: [f64; 3], to: [f64; 3], tex| {
        let top = Some([from[0], from[2], to[0], to[2]]);
        let mut faces = vec![
            ("down", top, tex, Some("down")),
            ("up", top, tex, Some("up")),
        ];
        faces.extend(sides(Some([from[0], 0.0, to[0], 16.0]), tex));
        element(from, to, &faces)
    };
    let fence_bar = |y: f64, v: f64| {
        element(
            [7.0, y, 0.0],
            [9.0, y + 3.0, 9.0],
            &[
                ("down", s(7.0, 0.0, 9.0, 9.0), t, None),
                ("up", s(7.0, 0.0, 9.0, 9.0), t, None),
                ("north", s(7.0, v, 9.0, v + 3.0), t, Some("north")),
                ("west", s(0.0, v, 9.0, v + 3.0), t, None),
                ("east", s(0.0, v, 9.0, v + 3.0), t, None),
            ],
        )
    };
    let wall_side = |high: f64| {
        element(
            [5.0, 0.0, 0.0],
            [11.0, high, 8.0],
            &[
                ("down", None, w, Some("down")),
                ("up", None, w, (high == 16.0).then_some("up")),
                ("north", None, w, Some("north")),
                ("west", None, w, None),
                ("east", None, w, None),
            ],
        )
    };
    let (p, e, b) = ("pane", "edge", "bars");
    let flat = |from: [f64; 3], to: [f64; 3], faces: &[FaceSpec]| element(from, to, faces);
    let ends = |z0: f64, z1: f64, down: [f64; 4], up: [f64; 4]| {
        [0.001, 15.999].map(|y| {
            flat(
                [7.0, y, z0],
                [9.0, y, z1],
                &[("down", Some(down), e, None), ("up", Some(up), e, None)],
            )
        })
    };
    let mut bars_side = vec![
        flat(
            [8.0, 0.0, 0.0],
            [8.0, 16.0, 8.0],
            &[
                ("west", s(16.0, 0.0, 8.0, 16.0), b, None),
                ("east", s(8.0, 0.0, 16.0, 16.0), b, None),
            ],
        ),
        flat(
            [7.0, 0.0, 0.0],
            [9.0, 16.0, 7.0],
            &[("north", s(7.0, 0.0, 9.0, 16.0), e, Some("north"))],
        ),
    ];
    bars_side.extend(ends(0.0, 7.0, [9.0, 0.0, 7.0, 7.0], [7.0, 0.0, 9.0, 7.0]));
    let mut bars_side_alt = vec![
        flat(
            [8.0, 0.0, 8.0],
            [8.0, 16.0, 16.0],
            &[
                ("west", s(8.0, 0.0, 0.0, 16.0), b, None),
                ("east", s(0.0, 0.0, 8.0, 16.0), b, None),
            ],
        ),
        flat(
            [7.0, 0.0, 9.0],
            [9.0, 16.0, 16.0],
            &[
                ("south", s(7.0, 0.0, 9.0, 16.0), e, Some("south")),
                ("down", s(9.0, 9.0, 7.0, 16.0), e, None),
                ("up", s(7.0, 9.0, 9.0, 16.0), e, None),
            ],
        ),
    ];
    bars_side_alt.extend(ends(
        9.0,
        16.0,
        [9.0, 9.0, 7.0, 16.0],
        [7.0, 9.0, 9.0, 16.0],
    ));
    let cap = |alt: bool| {
        let (z0, z1, z) = if alt {
            (7.0, 8.0, 7.0)
        } else {
            (8.0, 9.0, 9.0)
        };
        let (w0, w1) = if alt { (8.0, 9.0) } else { (8.0, 7.0) };
        let (n0, n1) = if alt { (7.0, 9.0) } else { (9.0, 7.0) };
        vec![
            flat(
                [8.0, 0.0, z0],
                [8.0, 16.0, z1],
                &[
                    ("west", s(w0, 0.0, w1, 16.0), b, None),
                    ("east", s(w1, 0.0, w0, 16.0), b, None),
                ],
            ),
            flat(
                [7.0, 0.0, z],
                [9.0, 16.0, z],
                &[
                    ("north", s(n0, 0.0, n1, 16.0), b, None),
                    ("south", s(n1, 0.0, n0, 16.0), b, None),
                ],
            ),
        ]
    };
    let mut list = vec![
        (
            "fence_post",
            template(vec![post([6.0, 0.0, 6.0], [10.0, 16.0, 10.0], t)], t, true),
        ),
        (
            "fence_side",
            template(vec![fence_bar(12.0, 1.0), fence_bar(6.0, 7.0)], t, true),
        ),
        (
            "fence_inventory",
            template(
                vec![
                    post([6.0, 0.0, 0.0], [10.0, 16.0, 4.0], t),
                    post([6.0, 0.0, 12.0], [10.0, 16.0, 16.0], t),
                    element([7.0, 13.0, 0.0], [9.0, 15.0, 16.0], &sides(None, t)),
                    element([7.0, 5.0, 0.0], [9.0, 7.0, 16.0], &sides(None, t)),
                ],
                t,
                true,
            ),
        ),
        (
            "wall_post",
            template(vec![post([4.0, 0.0, 4.0], [12.0, 16.0, 12.0], w)], w, true),
        ),
        ("wall_side", template(vec![wall_side(14.0)], w, true)),
        ("wall_side_tall", template(vec![wall_side(16.0)], w, true)),
        (
            "wall_inventory",
            template(
                vec![
                    post([4.0, 0.0, 4.0], [12.0, 16.0, 12.0], w),
                    element([5.0, 0.0, 0.0], [11.0, 13.0, 16.0], &{
                        let mut f = vec![("down", None, w, None), ("up", None, w, None)];
                        f.extend(sides(None, w));
                        f
                    }),
                ],
                w,
                true,
            ),
        ),
        ("fence_gate", template(gate(false, 0.0), t, true)),
        ("fence_gate_open", template(gate(true, 0.0), t, true)),
        ("fence_gate_wall", template(gate(false, 3.0), t, true)),
        ("fence_gate_wall_open", template(gate(true, 3.0), t, true)),
        (
            "glass_pane_post",
            template(
                vec![flat(
                    [7.0, 0.0, 7.0],
                    [9.0, 16.0, 9.0],
                    &[
                        ("down", s(7.0, 7.0, 9.0, 9.0), e, None),
                        ("up", s(7.0, 7.0, 9.0, 9.0), e, None),
                    ],
                )],
                p,
                false,
            ),
        ),
        (
            "glass_pane_side",
            template(
                vec![flat(
                    [7.0, 0.0, 0.0],
                    [9.0, 16.0, 7.0],
                    &[
                        ("down", s(7.0, 0.0, 9.0, 7.0), e, None),
                        ("up", s(7.0, 0.0, 9.0, 7.0), e, None),
                        ("north", s(7.0, 0.0, 9.0, 16.0), e, Some("north")),
                        ("west", s(16.0, 0.0, 9.0, 16.0), p, None),
                        ("east", s(9.0, 0.0, 16.0, 16.0), p, None),
                    ],
                )],
                p,
                false,
            ),
        ),
        (
            "glass_pane_side_alt",
            template(
                vec![flat(
                    [7.0, 0.0, 9.0],
                    [9.0, 16.0, 16.0],
                    &[
                        ("down", s(7.0, 0.0, 9.0, 7.0), e, None),
                        ("up", s(7.0, 0.0, 9.0, 7.0), e, None),
                        ("south", s(7.0, 0.0, 9.0, 16.0), e, Some("south")),
                        ("west", s(7.0, 0.0, 0.0, 16.0), p, None),
                        ("east", s(0.0, 0.0, 7.0, 16.0), p, None),
                    ],
                )],
                p,
                false,
            ),
        ),
        (
            "glass_pane_noside",
            template(
                vec![flat(
                    [7.0, 0.0, 7.0],
                    [9.0, 16.0, 9.0],
                    &[("north", s(9.0, 0.0, 7.0, 16.0), p, None)],
                )],
                p,
                false,
            ),
        ),
        (
            "glass_pane_noside_alt",
            template(
                vec![flat(
                    [7.0, 0.0, 7.0],
                    [9.0, 16.0, 9.0],
                    &[("east", s(7.0, 0.0, 9.0, 16.0), p, None)],
                )],
                p,
                false,
            ),
        ),
        (
            "bars_post_ends",
            template(
                ends(7.0, 9.0, [7.0, 7.0, 9.0, 9.0], [7.0, 7.0, 9.0, 9.0]).to_vec(),
                b,
                false,
            ),
        ),
        (
            "bars_post",
            template(
                vec![
                    flat(
                        [8.0, 0.0, 7.0],
                        [8.0, 16.0, 9.0],
                        &[
                            ("west", s(7.0, 0.0, 9.0, 16.0), b, None),
                            ("east", s(9.0, 0.0, 7.0, 16.0), b, None),
                        ],
                    ),
                    flat(
                        [7.0, 0.0, 8.0],
                        [9.0, 16.0, 8.0],
                        &[
                            ("north", s(7.0, 0.0, 9.0, 16.0), b, None),
                            ("south", s(9.0, 0.0, 7.0, 16.0), b, None),
                        ],
                    ),
                ],
                b,
                false,
            ),
        ),
        ("bars_side", template(bars_side, b, false)),
        ("bars_side_alt", template(bars_side_alt, b, false)),
        ("bars_cap", template(cap(false), b, false)),
        ("bars_cap_alt", template(cap(true), b, false)),
    ];
    list.extend(crate::openings::templates());
    list
}

/// The templates each family's blocks have a child of, by suffix.
fn parts(family: Family) -> &'static [(&'static str, &'static str)] {
    match family {
        Family::Door
        | Family::Trapdoor
        | Family::OrientableTrapdoor
        | Family::Button
        | Family::Plate
        | Family::WeightedPlate => &[],
        Family::Fence => &[
            ("_post", "fence_post"),
            ("_side", "fence_side"),
            ("_inventory", "fence_inventory"),
        ],
        Family::Wall => &[
            ("_post", "wall_post"),
            ("_side", "wall_side"),
            ("_side_tall", "wall_side_tall"),
            ("_inventory", "wall_inventory"),
        ],
        Family::Gate => &[
            ("", "fence_gate"),
            ("_open", "fence_gate_open"),
            ("_wall", "fence_gate_wall"),
            ("_wall_open", "fence_gate_wall_open"),
        ],
        Family::Pane => &[
            ("_post", "glass_pane_post"),
            ("_side", "glass_pane_side"),
            ("_side_alt", "glass_pane_side_alt"),
            ("_noside", "glass_pane_noside"),
            ("_noside_alt", "glass_pane_noside_alt"),
        ],
        Family::Bars => &[
            ("_post_ends", "bars_post_ends"),
            ("_post", "bars_post"),
            ("_side", "bars_side"),
            ("_side_alt", "bars_side_alt"),
            ("_cap", "bars_cap"),
            ("_cap_alt", "bars_cap_alt"),
        ],
    }
}

pub(crate) fn tex(name: &str) -> String {
    format!("minecraft:block/{name}")
}

/// The block's models (children of the templates) and its block-state
/// definition.
pub fn models(block: &Block, family: Family) -> (Vec<(String, Value)>, Value) {
    if family.opening() {
        return crate::openings::models(block, family);
    }
    let Faces::Connected { texture, edge } = block.faces else {
        unreachable!("{} has connected faces", block.id)
    };
    let id = block.id;
    let textures = match family {
        Family::Fence | Family::Gate => json!({"texture": tex(texture)}),
        Family::Wall => json!({"wall": tex(texture)}),
        // Panes draw translucent whatever their pixels.
        Family::Pane => json!({
            "pane": {"sprite": tex(texture), "force_translucent": true},
            "edge": {"sprite": tex(edge), "force_translucent": true},
        }),
        Family::Bars => json!({"bars": tex(texture), "edge": tex(edge)}),
        _ => unreachable!("openings draw through crate::openings"),
    };
    let models = parts(family)
        .iter()
        .map(|(suffix, template)| {
            (
                format!("block/{id}{suffix}"),
                json!({"parent": format!("minecraft:block/open_{template}"), "textures": textures}),
            )
        })
        .collect();
    let model = |suffix: &str| format!("minecraft:block/{id}{suffix}");
    let turned = |suffix: &str, y: u32, uvlock: bool| {
        let mut apply = json!({"model": model(suffix)});
        if uvlock {
            apply["uvlock"] = json!(true);
        }
        if y != 0 {
            apply["y"] = json!(y);
        }
        apply
    };
    let part = |apply: Value, when: Option<Value>| match when {
        Some(when) => json!({"apply": apply, "when": when}),
        None => json!({"apply": apply}),
    };
    let quarter = |side: &str| match side {
        "north" => 0,
        "east" => 90,
        "south" => 180,
        _ => 270,
    };
    let definition = match family {
        Family::Fence => {
            let mut list = vec![part(json!({"model": model("_post")}), None)];
            for side in SIDES {
                list.push(part(
                    turned("_side", quarter(side), true),
                    Some(json!({side: "true"})),
                ));
            }
            json!({"multipart": list})
        }
        Family::Wall => {
            let mut list = vec![part(
                json!({"model": model("_post")}),
                Some(json!({"up": "true"})),
            )];
            for (height, suffix) in [("low", "_side"), ("tall", "_side_tall")] {
                for side in SIDES {
                    list.push(part(
                        turned(suffix, quarter(side), true),
                        Some(json!({side: height})),
                    ));
                }
            }
            json!({"multipart": list})
        }
        Family::Gate => {
            let mut variants = Map::new();
            for (facing, y) in [("east", 270), ("north", 180), ("south", 0), ("west", 90)] {
                for in_wall in ["false", "true"] {
                    for open in ["false", "true"] {
                        let suffix = match (in_wall, open) {
                            ("false", "false") => "",
                            ("false", _) => "_open",
                            (_, "false") => "_wall",
                            _ => "_wall_open",
                        };
                        variants.insert(
                            format!("facing={facing},in_wall={in_wall},open={open}"),
                            turned(suffix, y, true),
                        );
                    }
                }
            }
            json!({"variants": variants})
        }
        Family::Pane => {
            let mut list = vec![part(json!({"model": model("_post")}), None)];
            for (side, suffix, y) in [
                ("north", "_side", 0),
                ("east", "_side", 90),
                ("south", "_side_alt", 0),
                ("west", "_side_alt", 90),
            ] {
                list.push(part(turned(suffix, y, false), Some(json!({side: "true"}))));
            }
            for (side, suffix, y) in [
                ("north", "_noside", 0),
                ("east", "_noside_alt", 0),
                ("south", "_noside_alt", 90),
                ("west", "_noside", 270),
            ] {
                list.push(part(turned(suffix, y, false), Some(json!({side: "false"}))));
            }
            json!({"multipart": list})
        }
        Family::Bars => {
            let only = |side: &str| {
                let mut when = Map::new();
                for s in ["east", "north", "south", "west"] {
                    when.insert(s.into(), json!(if s == side { "true" } else { "false" }));
                }
                Value::Object(when)
            };
            let mut list = vec![
                part(json!({"model": model("_post_ends")}), None),
                part(json!({"model": model("_post")}), Some(only(""))),
            ];
            for (side, suffix, y) in [
                ("north", "_cap", 0),
                ("east", "_cap", 90),
                ("south", "_cap_alt", 0),
                ("west", "_cap_alt", 90),
            ] {
                list.push(part(turned(suffix, y, false), Some(only(side))));
            }
            for (side, suffix, y) in [
                ("north", "_side", 0),
                ("east", "_side", 90),
                ("south", "_side_alt", 0),
                ("west", "_side_alt", 90),
            ] {
                list.push(part(turned(suffix, y, false), Some(json!({side: "true"}))));
            }
            json!({"multipart": list})
        }
        _ => unreachable!("openings draw through crate::openings"),
    };
    (models, definition)
}

/// The item's model: the inventory model of fences and walls, the closed
/// gate; `None` for panes and bars, whose items are their flat texture.
pub fn item_model(block: &Block, family: Family) -> Option<String> {
    match family {
        Family::Fence | Family::Wall => Some(format!("minecraft:block/{}_inventory", block.id)),
        Family::Gate => Some(format!("minecraft:block/{}", block.id)),
        Family::Pane | Family::Bars | Family::Door => None,
        Family::Trapdoor | Family::OrientableTrapdoor => {
            Some(format!("minecraft:block/{}_bottom", block.id))
        }
        Family::Button => Some(format!("minecraft:block/{}", block.id)),
        Family::Plate | Family::WeightedPlate => Some(format!("minecraft:block/{}", block.id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s<'a>(pairs: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        pairs.to_vec()
    }

    #[test]
    fn the_families_are_listed_with_minecraft_numbers() {
        let lines = lines();
        let count = |f: Family| lines.iter().filter(|l| l.family == f).count();
        assert_eq!(count(Family::Wall), 32);
        assert_eq!(count(Family::Fence), 13);
        assert_eq!(count(Family::Gate), 12);
        assert_eq!(count(Family::Pane), 17);
        assert_eq!(count(Family::Bars), 9);
        let fence = lines.iter().find(|l| l.id == "nether_brick_fence").unwrap();
        assert_eq!((fence.hardness, fence.resistance), (2.0, 6.0));
        let pane = lines.iter().find(|l| l.id == "glass_pane").unwrap();
        assert_eq!(pane.textures, ("glass", "glass_pane_top"));
    }

    #[test]
    fn fences_and_walls_collide_a_block_and_a_half_high() {
        let fence = merged(
            Family::Fence,
            &s(&[("east", "true"), ("north", "false")]),
            Shape::Collision,
        );
        assert_eq!(fence, vec![[0.375, 0.0, 0.375, 1.0, 1.5, 0.625]]);
        let wall = merged(
            Family::Wall,
            &s(&[("north", "low"), ("up", "false")]),
            Shape::Outline,
        );
        assert_eq!(wall, vec![[0.3125, 0.0, 0.0, 0.6875, 0.875, 0.6875]]);
        let open = s(&[("facing", "east"), ("open", "true")]);
        assert!(merged(Family::Gate, &open, Shape::Collision).is_empty());
        assert_eq!(
            merged(Family::Gate, &open, Shape::Outline),
            vec![[0.375, 0.0, 0.0, 0.625, 1.0, 1.0]]
        );
    }
}
