//! The open edition's mobs: the common creatures and monsters of a survival world, so
//! open players see what shares their world (through the relay, a Java server's mobs).
//!
//! Each mob is a model of boxes under the part names the client's animations drive
//! (`head`, `right_arm`, `left_hind_leg`...), and a skin painted here, Horizon's own:
//! every box gets its own area of the texture (a shelf packer lays them out) and a
//! painter colours each face. Nothing comes from Minecraft's art.
use crate::image::{Rgba, noise};
use crate::{Bundle, Origin, Result};
use serde_json::{Map, Value, json};
use std::f32::consts::{FRAC_PI_4, FRAC_PI_8};

const GENERATOR: &str = "crates/horizon-open-content/src/mobs.rs";

/// A face of a box, as the entity sees it: the model looks toward -Z, and model Y
/// grows downward (feet at y = 24).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Face {
    Top,
    Bottom,
    Front,
    Back,
    /// The entity's right side (-X).
    Right,
    /// The entity's left side (+X).
    Left,
}

/// A face pixel's colour, from the face, the pixel (x from the viewer's left, y from the
/// top) and the face's size. Alpha 0 leaves the pixel clear.
type Paint = Box<dyn Fn(Face, u32, u32, u32, u32) -> [u8; 4]>;

/// One part: a box at `min` with `size` (model units), posed at `pivot` with `rot`
/// (radians), grown by `grow` on every side (the texture keeps the unscaled size), and
/// optional child parts that move with it.
struct Cube {
    name: &'static str,
    pivot: [f32; 3],
    rot: [f32; 3],
    min: [f32; 3],
    size: [f32; 3],
    grow: f32,
    paint: Paint,
    children: Vec<Cube>,
}

fn cube(name: &'static str, pivot: [f32; 3], min: [f32; 3], size: [f32; 3], paint: Paint) -> Cube {
    Cube {
        name,
        pivot,
        rot: [0.0; 3],
        min,
        size,
        grow: 0.0,
        paint,
        children: Vec::new(),
    }
}

impl Cube {
    fn rot(mut self, rot: [f32; 3]) -> Self {
        self.rot = rot;
        self
    }
    fn grow(mut self, grow: f32) -> Self {
        self.grow = grow;
        self
    }
    fn child(mut self, child: Cube) -> Self {
        self.children.push(child);
        self
    }
}

/// A model and its skin: the boxes, laid out on a texture `width` pixels wide.
struct Skin {
    width: u32,
    height: u32,
    /// Each box's texture origin, in the order `walk` visits them.
    origins: Vec<[u32; 2]>,
}

/// Visits every box, parents before children, in a stable order.
fn walk<'a>(cubes: &'a [Cube], out: &mut Vec<&'a Cube>) {
    for c in cubes {
        out.push(c);
        walk(&c.children, out);
    }
}

/// A box's texture area in box layout: (2 × (depth + width)) × (depth + height).
fn area(c: &Cube) -> [u32; 2] {
    let [w, h, d] = c.size.map(|v| v.ceil() as u32);
    [2 * (d + w), d + h]
}

/// Shelf packing, left to right in rows, on a texture 64 pixels wide (wider for wide boxes);
/// the height is the next power of two that holds every row.
fn pack(cubes: &[Cube]) -> Skin {
    let mut all = Vec::new();
    walk(cubes, &mut all);
    let width = all
        .iter()
        .map(|c| area(c)[0])
        .max()
        .unwrap_or(0)
        .max(64)
        .next_power_of_two();
    let (mut x, mut y, mut row) = (0u32, 0u32, 0u32);
    let mut origins = Vec::new();
    for c in &all {
        let [w, h] = area(c);
        if x + w > width {
            (x, y, row) = (0, y + row, 0);
        }
        origins.push([x, y]);
        x += w;
        row = row.max(h);
    }
    let height = (y + row).max(32).next_power_of_two();
    Skin {
        width,
        height,
        origins,
    }
}

/// The six faces of a box with UV origin `[u, v]`: corners counter-clockwise seen from
/// outside, the face's pixel rectangle (u0, v0, u1, v1) and its outward normal. The same
/// box layout as the player's model in `art.rs`.
fn faces(
    min: [f32; 3],
    size: [f32; 3],
    grow: f32,
    uv: [u32; 2],
) -> [(Face, [[f32; 3]; 4], [u32; 4], [f32; 3]); 6] {
    let [x0, y0, z0] = min.map(|v| v - grow);
    let [x1, y1, z1] = [
        min[0] + size[0] + grow,
        min[1] + size[1] + grow,
        min[2] + size[2] + grow,
    ];
    let [w, h, d] = size.map(|v| v.ceil() as u32);
    let [u, v] = uv;
    [
        (
            Face::Top,
            [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
            [u + d, v, u + d + w, v + d],
            [0.0, -1.0, 0.0],
        ),
        (
            Face::Bottom,
            [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
            [u + d + w, v, u + d + 2 * w, v + d],
            [0.0, 1.0, 0.0],
        ),
        (
            Face::Front,
            [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
            [u + d, v + d, u + d + w, v + d + h],
            [0.0, 0.0, -1.0],
        ),
        (
            Face::Back,
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            [u + 2 * d + w, v + d, u + 2 * d + 2 * w, v + d + h],
            [0.0, 0.0, 1.0],
        ),
        (
            Face::Right,
            [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
            [u + d + w, v + d, u + 2 * d + w, v + d + h],
            [-1.0, 0.0, 0.0],
        ),
        (
            Face::Left,
            [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
            [u, v + d, u + d, v + d + h],
            [1.0, 0.0, 0.0],
        ),
    ]
}

/// One part as `entity-models.json` holds it.
/// One part, its geometry scaled by `scale` about its pivot (a baby's model: see `Baby`),
/// at `pivot`. The UVs stay the adult's, so a baby wears the adult's skin layout.
fn part_json(c: &Cube, skin: &Skin, index: &mut usize, scale: f32, pivot: [f32; 3]) -> Value {
    let uv = skin.origins[*index];
    *index += 1;
    let (tw, th) = (skin.width as f32, skin.height as f32);
    let mut polygons = Vec::new();
    let mut normals = Vec::new();
    for (_, corners, [u0, v0, u1, v1], normal) in faces(c.min, c.size, c.grow, uv) {
        let corners = corners.map(|p| p.map(|v| v * scale));
        // Corners 0..3 take the rectangle's (u1,v0) (u0,v0) (u0,v1) (u1,v1).
        let uvs = [[u1, v0], [u0, v0], [u0, v1], [u1, v1]];
        let mut quad: Vec<Value> = corners
            .iter()
            .zip(uvs)
            .map(|(p, [uu, vv])| json!([p[0], p[1], p[2], uu as f32 / tw, vv as f32 / th]))
            .collect();
        // Keep the winding consistent with the outward normal.
        let (a, b, cc) = (corners[0], corners[1], corners[2]);
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [cc[0] - a[0], cc[1] - a[1], cc[2] - a[2]];
        let cross = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        if cross[0] * normal[0] + cross[1] * normal[1] + cross[2] * normal[2] < 0.0 {
            quad.reverse();
        }
        polygons.push(Value::Array(quad));
        normals.push(json!(normal));
    }
    let mut children = Map::new();
    for child in &c.children {
        let at = child.pivot.map(|v| v * scale);
        children.insert(child.name.into(), part_json(child, skin, index, scale, at));
    }
    json!({
        "pose": [pivot[0], pivot[1], pivot[2], c.rot[0], c.rot[1], c.rot[2], 1.0, 1.0, 1.0],
        "polygons": polygons,
        "normals": normals,
        "children": children,
    })
}

/// A baby's proportions: the body scaled down about the feet (y = 24), the head less, as
/// young animals' heads are large for their bodies.
#[derive(Clone, Copy)]
struct Baby {
    body: f32,
    head: f32,
}
const ADULT: Baby = Baby {
    body: 1.0,
    head: 1.0,
};
const BABY: Baby = Baby {
    body: 0.5,
    head: 0.75,
};

/// The model's root: every top-level box a child, at the origin.
fn model_json(cubes: &[Cube], skin: &Skin, baby: Baby) -> Value {
    let mut index = 0;
    let mut children = Map::new();
    for c in cubes {
        let scale = if c.name == "head" {
            baby.head
        } else {
            baby.body
        };
        let [x, y, z] = c.pivot;
        let at = [x * baby.body, 24.0 - (24.0 - y) * baby.body, z * baby.body];
        children.insert(c.name.into(), part_json(c, skin, &mut index, scale, at));
    }
    json!({ "pose": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0], "polygons": [], "children": children })
}

/// The skin: every face painted, with a little per-pixel shading so flat colours read as
/// material rather than plastic.
fn paint(cubes: &[Cube], skin: &Skin, seed: u32) -> Rgba {
    let mut image = Rgba::new(skin.width, skin.height);
    let mut all = Vec::new();
    walk(cubes, &mut all);
    for (c, &uv) in all.iter().zip(&skin.origins) {
        for (face, _, [u0, v0, u1, v1], _) in faces(c.min, c.size, 0.0, uv) {
            let (fw, fh) = (u1 - u0, v1 - v0);
            for y in 0..fh {
                for x in 0..fw {
                    let [r, g, b, a] = (c.paint)(face, x, y, fw, fh);
                    if a == 0 {
                        continue;
                    }
                    let n = (noise(u0 + x, v0 + y, seed) % 13) as i32 - 6;
                    let f = |v: u8| (v as i32 + n).clamp(0, 255) as u8;
                    image.set(u0 + x, v0 + y, [f(r), f(g), f(b), a]);
                }
            }
        }
    }
    image
}

// PAINTERS

const CLEAR: [u8; 4] = [0, 0, 0, 0];
const fn rgb(c: [u8; 3]) -> [u8; 4] {
    [c[0], c[1], c[2], 255]
}
fn solid(c: [u8; 3]) -> Paint {
    Box::new(move |_, _, _, _, _| rgb(c))
}
/// A colour that is lighter on top and darker underneath.
fn lit(c: [u8; 3]) -> Paint {
    Box::new(move |face, _, _, _, _| shade(c, face))
}
fn shade(c: [u8; 3], face: Face) -> [u8; 4] {
    let k = match face {
        Face::Top => 1.08,
        Face::Bottom => 0.82,
        Face::Back => 0.94,
        _ => 1.0,
    };
    rgb(c.map(|v| ((v as f32) * k).min(255.0) as u8))
}
/// A face with two eyes (and optionally a mouth) on the front, `skin` elsewhere.
fn face(skin: [u8; 3], eye: [u8; 3], mouth: Option<[u8; 3]>) -> Paint {
    Box::new(move |f, x, y, w, h| {
        if f == Face::Front {
            let ey = h / 2 - 1;
            let (l, r) = (w / 4, w - 1 - w / 4);
            if y == ey
                && (x == l || x == l - 1 + (w / 8).min(1) || x == r || x == r + 1 - (w / 8).min(1))
            {
                return rgb(eye);
            }
            if let Some(m) = mouth
                && y == h - 2
                && x >= w / 2 - 1
                && x <= w / 2
            {
                return rgb(m);
            }
        }
        shade(skin, f)
    })
}
/// Bands down a limb or body: `top` above `split` (a share of the height), `bottom` below.
fn banded(top: [u8; 3], bottom: [u8; 3], split: f32) -> Paint {
    Box::new(move |f, _, y, _, h| {
        let c = if f == Face::Top {
            top
        } else if f == Face::Bottom {
            bottom
        } else if (y as f32) < h as f32 * split {
            top
        } else {
            bottom
        };
        shade(c, f)
    })
}
/// Patches of `spot` over `base`, seeded so each mob's coat differs.
fn spotted(base: [u8; 3], spot: [u8; 3], seed: u32) -> Paint {
    Box::new(move |f, x, y, _, _| {
        let blob = noise(x / 3, y / 3, seed) % 5 == 0;
        shade(if blob { spot } else { base }, f)
    })
}
/// Eyes only, glowing, for the emissive eye layers.
fn eyes_only(eye: [u8; 3]) -> Paint {
    Box::new(move |f, x, y, w, h| {
        let ey = h / 2 - 1;
        let (l, r) = (w / 4, w - 1 - w / 4);
        if f == Face::Front && y == ey && (x == l || x == r) {
            rgb(eye)
        } else {
            CLEAR
        }
    })
}

// MODELS

/// A humanoid: head, body, arms and legs, `limb` thick (4 for zombies, 2 for skeletons).
fn humanoid(
    head: Paint,
    body: Paint,
    arm: impl Fn() -> Paint,
    leg: impl Fn() -> Paint,
    limb: f32,
) -> Vec<Cube> {
    let half = limb / 2.0;
    let leg_x = if limb < 4.0 { 2.0 } else { 1.9 };
    vec![
        cube(
            "head",
            [0.0, 0.0, 0.0],
            [-4.0, -8.0, -4.0],
            [8.0, 8.0, 8.0],
            head,
        ),
        cube(
            "body",
            [0.0, 0.0, 0.0],
            [-4.0, 0.0, -2.0],
            [8.0, 12.0, 4.0],
            body,
        ),
        // Arms hang from the shoulders: the right one outward to -X, the left to +X.
        cube(
            "right_arm",
            [-5.0, 2.0, 0.0],
            [1.0 - limb, -2.0, -half],
            [limb, 12.0, limb],
            arm(),
        ),
        cube(
            "left_arm",
            [5.0, 2.0, 0.0],
            [-1.0, -2.0, -half],
            [limb, 12.0, limb],
            arm(),
        ),
        cube(
            "right_leg",
            [-leg_x, 12.0, 0.0],
            [-half, 0.0, -half],
            [limb, 12.0, limb],
            leg(),
        ),
        cube(
            "left_leg",
            [leg_x, 12.0, 0.0],
            [-half, 0.0, -half],
            [limb, 12.0, limb],
            leg(),
        ),
    ]
}

/// A four-legged animal: a head forward (-Z), a long body, four legs `leg` tall.
#[allow(clippy::too_many_arguments)]
fn quadruped(
    head: Cube,
    body_min: [f32; 3],
    body_size: [f32; 3],
    body: Paint,
    leg: impl Fn() -> Paint,
    leg_height: f32,
    leg_x: f32,
    leg_z: [f32; 2],
) -> Vec<Cube> {
    let y = 24.0 - leg_height;
    let legs = [
        ("right_hind_leg", -leg_x, leg_z[1]),
        ("left_hind_leg", leg_x, leg_z[1]),
        ("right_front_leg", -leg_x, leg_z[0]),
        ("left_front_leg", leg_x, leg_z[0]),
    ];
    let mut cubes = vec![
        head,
        cube("body", [0.0, 0.0, 0.0], body_min, body_size, body),
    ];
    for (name, x, z) in legs {
        cubes.push(cube(
            name,
            [x, y, z],
            [-2.0, 0.0, -2.0],
            [4.0, leg_height, 4.0],
            leg(),
        ));
    }
    cubes
}

/// One mob: its entity type, its layers (model key → boxes, texture) and the extra
/// texture names that share a layer's skin.
pub struct Mob {
    pub name: &'static str,
    category: &'static str,
    /// Width, height and eye height in blocks: the hit box players aim at.
    size: [f64; 3],
    attributes: &'static [(&'static str, f64)],
    layers: Vec<Layer>,
}

struct Layer {
    /// `entity-models.json` keys that use this model (variants share one).
    models: Vec<&'static str>,
    cubes: Vec<Cube>,
    /// Keys of this model's baby version (the same skin layout, smaller).
    babies: Vec<&'static str>,
    /// Texture paths under `textures/` that take this skin (baby textures too: a baby
    /// draws the adult model with the `_baby` texture).
    textures: Vec<String>,
    seed: u32,
}

fn layer(models: &[&'static str], cubes: Vec<Cube>, textures: &[&str], seed: u32) -> Layer {
    Layer {
        models: models.to_vec(),
        babies: Vec::new(),
        cubes,
        textures: textures.iter().map(|t| t.to_string()).collect(),
        seed,
    }
}

/// `path` and its `_baby` twin.
fn with_baby(paths: &[&str]) -> Vec<String> {
    paths
        .iter()
        .flat_map(|p| [p.to_string(), format!("{p}_baby")])
        .collect()
}
/// A layer whose kind has young: each model key `x#layer` also gets `x_baby#layer`, and
/// each texture its `_baby` twin.
fn baby_layer(models: &[&'static str], cubes: Vec<Cube>, textures: &[&str], seed: u32) -> Layer {
    Layer {
        models: models.to_vec(),
        babies: models
            .iter()
            .map(|m| {
                let (kind, layer) = m.split_once('#').expect("a model#layer key");
                leak(format!("{kind}_baby#{layer}"))
            })
            .collect(),
        cubes,
        textures: with_baby(textures),
        seed,
    }
}

const MONSTER: &[(&str, f64)] = &[
    ("max_health", 20.0),
    ("movement_speed", 0.23),
    ("attack_damage", 3.0),
    ("follow_range", 35.0),
];
const UNDEAD_ARCHER: &[(&str, f64)] = &[
    ("max_health", 20.0),
    ("movement_speed", 0.25),
    ("follow_range", 16.0),
];

fn zombie_like(
    name: &'static str,
    texture: &'static str,
    skin: [u8; 3],
    shirt: [u8; 3],
    trousers: [u8; 3],
    seed: u32,
) -> Mob {
    let eye = [24, 30, 24];
    Mob {
        name,
        category: "monster",
        size: [0.6, 1.95, 1.74],
        attributes: MONSTER,
        layers: vec![baby_layer(
            &[leak(format!("{name}#main"))],
            humanoid(
                face(skin, eye, Some([40, 30, 30])),
                banded(shirt, trousers, 0.75),
                || banded(shirt, skin, 0.4),
                || banded(trousers, [50, 46, 52], 0.85),
                4.0,
            ),
            &[texture],
            seed,
        )],
    }
}

fn skeleton_like(name: &'static str, texture: &'static str, bone: [u8; 3], seed: u32) -> Mob {
    let dark = bone.map(|v| v / 3);
    let ribs = move |f: Face, _x: u32, y: u32, _w: u32, _h: u32| {
        if (f == Face::Front || f == Face::Back) && y % 3 == 1 && y < 8 {
            shade(dark, f)
        } else {
            shade(bone, f)
        }
    };
    Mob {
        name,
        category: "monster",
        size: [0.6, 1.99, 1.74],
        attributes: UNDEAD_ARCHER,
        layers: vec![layer(
            &[leak(format!("{name}#main"))],
            humanoid(
                face(bone, [20, 20, 20], Some(dark)),
                Box::new(ribs),
                || lit(bone),
                || lit(bone),
                2.0,
            ),
            &[texture],
            seed,
        )],
    }
}

fn spider_like(
    name: &'static str,
    texture: &'static str,
    // The glowing eyes' texture; both spiders share one, so only one writes it.
    eyes: Option<&'static str>,
    body: [u8; 3],
    seed: u32,
) -> Mob {
    let leg_rot =
        |right: bool, y: f32, z: f32| [0.0, if right { y } else { -y }, if right { -z } else { z }];
    const MID: f32 = 0.581_194_6;
    let legs = [
        ("hind", 2.0, FRAC_PI_4, FRAC_PI_4),
        ("middle_hind", 1.0, FRAC_PI_8, MID),
        ("middle_front", 0.0, -FRAC_PI_8, MID),
        ("front", -1.0, -FRAC_PI_4, FRAC_PI_4),
    ];
    let red = [200, 30, 30];
    let build = |head: Paint| {
        let mut cubes = vec![
            cube(
                "head",
                [0.0, 15.0, -3.0],
                [-4.0, -4.0, -8.0],
                [8.0, 8.0, 8.0],
                head,
            ),
            cube(
                "body0",
                [0.0, 15.0, 0.0],
                [-3.0, -3.0, -3.0],
                [6.0, 6.0, 6.0],
                lit(body),
            ),
            cube(
                "body1",
                [0.0, 15.0, 9.0],
                [-5.0, -4.0, -6.0],
                [10.0, 8.0, 12.0],
                spotted(body, body.map(|v| v.saturating_add(18)), seed),
            ),
        ];
        for (n, z, yr, zr) in legs {
            for right in [true, false] {
                let name = leak(format!("{}_{n}_leg", if right { "right" } else { "left" }));
                let min = if right {
                    [-15.0, -1.0, -1.0]
                } else {
                    [-1.0, -1.0, -1.0]
                };
                cubes.push(
                    cube(
                        name,
                        [if right { -4.0 } else { 4.0 }, 15.0, z],
                        min,
                        [16.0, 2.0, 2.0],
                        banded(body, body.map(|v| v / 2), 0.5),
                    )
                    .rot(leg_rot(right, yr, zr)),
                );
            }
        }
        cubes
    };
    let model = leak(format!("{name}#main"));
    Mob {
        name,
        category: "monster",
        size: if name == "spider" {
            [1.4, 0.9, 0.65]
        } else {
            [0.7, 0.5, 0.45]
        },
        attributes: &[
            ("max_health", 16.0),
            ("movement_speed", 0.3),
            ("attack_damage", 2.0),
            ("follow_range", 16.0),
        ],
        layers: [layer(
            &[model],
            build(face(body, red, None)),
            &[texture],
            seed,
        )]
        .into_iter()
        // The glowing eyes: the same layout, clear but for the eyes.
        .chain(eyes.map(|eyes| layer(&[], build(eyes_only(red)), &[eyes], seed)))
        .collect(),
    }
}

/// Leaks a string for a `'static` model key (a few dozen, once per process).
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Every mob, in the network registry's order after the four basic entity types.
pub fn mobs() -> Vec<Mob> {
    let mut out = vec![
        zombie_like(
            "zombie",
            "entity/zombie/zombie",
            [96, 140, 82],
            [44, 120, 140],
            [62, 58, 120],
            11,
        ),
        zombie_like(
            "husk",
            "entity/zombie/husk",
            [170, 150, 104],
            [120, 100, 70],
            [96, 82, 60],
            12,
        ),
        zombie_like(
            "drowned",
            "entity/zombie/drowned",
            [80, 150, 140],
            [60, 110, 100],
            [40, 80, 90],
            13,
        ),
        skeleton_like("skeleton", "entity/skeleton/skeleton", [196, 196, 188], 21),
        skeleton_like("stray", "entity/skeleton/stray", [168, 196, 204], 22),
        spider_like(
            "spider",
            "entity/spider/spider",
            Some("entity/spider/spider_eyes"),
            [58, 48, 44],
            31,
        ),
        spider_like(
            "cave_spider",
            "entity/spider/cave_spider",
            None,
            [30, 62, 72],
            32,
        ),
    ];

    // The creeper: a mottled green column on four short legs.
    let green = [82, 160, 70];
    let creeper_face: Paint = Box::new(move |f, x, y, _, _| {
        if f == Face::Front
            && ((y == 2 || y == 3) && (x == 1 || x == 2 || x == 5 || x == 6)
                || (4..=6).contains(&y) && (3..=4).contains(&x)
                || y == 6 && (x == 2 || x == 5))
        {
            return rgb([16, 24, 16]);
        }
        shade(
            if noise(x, y, 41) % 4 == 0 {
                [120, 190, 100]
            } else {
                green
            },
            f,
        )
    });
    let mut creeper = vec![
        cube(
            "head",
            [0.0, 6.0, 0.0],
            [-4.0, -8.0, -4.0],
            [8.0, 8.0, 8.0],
            creeper_face,
        ),
        cube(
            "body",
            [0.0, 6.0, 0.0],
            [-4.0, 0.0, -2.0],
            [8.0, 12.0, 4.0],
            spotted(green, [52, 120, 48], 42),
        ),
    ];
    for (name, x, z) in [
        ("right_hind_leg", -2.0, 4.0),
        ("left_hind_leg", 2.0, 4.0),
        ("right_front_leg", -2.0, -4.0),
        ("left_front_leg", 2.0, -4.0),
    ] {
        creeper.push(cube(
            name,
            [x, 18.0, z],
            [-2.0, 0.0, -2.0],
            [4.0, 6.0, 4.0],
            spotted(green, [52, 120, 48], 43),
        ));
    }
    out.push(Mob {
        name: "creeper",
        category: "monster",
        size: [0.6, 1.7, 1.445],
        attributes: &[
            ("max_health", 20.0),
            ("movement_speed", 0.25),
            ("follow_range", 16.0),
        ],
        layers: vec![layer(
            &["creeper#main"],
            creeper,
            &["entity/creeper/creeper"],
            44,
        )],
    });

    // The enderman: a tall dark figure with long thin limbs and violet eyes.
    let black = [22, 20, 28];
    let violet = [200, 90, 240];
    let enderman = |head: Paint, rest: &dyn Fn() -> Paint| {
        vec![
            cube(
                "head",
                [0.0, -14.0, 0.0],
                [-4.0, -8.0, -4.0],
                [8.0, 8.0, 8.0],
                head,
            ),
            cube(
                "body",
                [0.0, -14.0, 0.0],
                [-4.0, 0.0, -2.0],
                [8.0, 12.0, 4.0],
                rest(),
            ),
            cube(
                "right_arm",
                [-5.0, -12.0, 0.0],
                [-1.0, -2.0, -1.0],
                [2.0, 30.0, 2.0],
                rest(),
            ),
            cube(
                "left_arm",
                [5.0, -12.0, 0.0],
                [-1.0, -2.0, -1.0],
                [2.0, 30.0, 2.0],
                rest(),
            ),
            cube(
                "right_leg",
                [-2.0, -6.0, 0.0],
                [-1.0, 0.0, -1.0],
                [2.0, 30.0, 2.0],
                rest(),
            ),
            cube(
                "left_leg",
                [2.0, -6.0, 0.0],
                [-1.0, 0.0, -1.0],
                [2.0, 30.0, 2.0],
                rest(),
            ),
        ]
    };
    out.push(Mob {
        name: "enderman",
        category: "monster",
        size: [0.6, 2.9, 2.55],
        attributes: &[
            ("max_health", 40.0),
            ("movement_speed", 0.3),
            ("attack_damage", 7.0),
            ("follow_range", 64.0),
        ],
        layers: vec![
            layer(
                &["enderman#main"],
                enderman(face(black, violet, None), &|| lit(black)),
                &["entity/enderman/enderman"],
                51,
            ),
            layer(
                &[],
                enderman(eyes_only(violet), &|| Box::new(|_, _, _, _, _| CLEAR)),
                &["entity/enderman/enderman_eyes"],
                51,
            ),
        ],
    });

    // Farm animals. Each climate variant shares its kind's model, with its own colours.
    let animal = |name: &'static str, size: [f64; 3], health: f64, layers: Vec<Layer>| Mob {
        name,
        category: "creature",
        size,
        attributes: leak_attrs(health),
        layers,
    };
    let cow = |coat: [u8; 3], patch: [u8; 3], seed: u32| {
        let head = cube(
            "head",
            [0.0, 4.0, -8.0],
            [-4.0, -4.0, -6.0],
            [8.0, 8.0, 6.0],
            face(coat, [20, 20, 20], Some([220, 160, 160])),
        )
        .child(cube(
            "right_horn",
            [0.0, 0.0, 0.0],
            [-5.0, -5.0, -4.0],
            [1.0, 3.0, 1.0],
            solid([220, 214, 196]),
        ))
        .child(cube(
            "left_horn",
            [0.0, 0.0, 0.0],
            [4.0, -5.0, -4.0],
            [1.0, 3.0, 1.0],
            solid([220, 214, 196]),
        ));
        quadruped(
            head,
            [-6.0, 5.0, -8.0],
            [12.0, 10.0, 18.0],
            spotted(coat, patch, seed),
            move || spotted(coat, patch, seed + 1),
            12.0,
            4.0,
            [-6.0, 7.0],
        )
    };
    out.push(animal(
        "cow",
        [0.9, 1.4, 1.3],
        10.0,
        vec![
            baby_layer(
                &["cow#main"],
                cow([92, 62, 44], [236, 230, 222], 61),
                &["entity/cow/cow_temperate"],
                61,
            ),
            baby_layer(
                &["warm_cow#main"],
                cow([150, 96, 60], [214, 170, 120], 62),
                &["entity/cow/cow_warm"],
                62,
            ),
            baby_layer(
                &["cold_cow#main"],
                cow([70, 64, 70], [190, 186, 190], 63),
                &["entity/cow/cow_cold"],
                63,
            ),
        ],
    ));
    let pig = |skin: [u8; 3]| {
        let snout = [
            skin[0].saturating_sub(30),
            skin[1].saturating_sub(40),
            skin[2].saturating_sub(40),
        ];
        let head = cube(
            "head",
            [0.0, 12.0, -6.0],
            [-4.0, -4.0, -8.0],
            [8.0, 8.0, 8.0],
            face(skin, [20, 20, 20], None),
        )
        .child(cube(
            "snout",
            [0.0, 0.0, 0.0],
            [-2.0, 0.0, -9.0],
            [4.0, 3.0, 1.0],
            solid(snout),
        ));
        quadruped(
            head,
            [-5.0, 7.0, -8.0],
            [10.0, 8.0, 16.0],
            lit(skin),
            move || lit(skin),
            6.0,
            3.0,
            [-5.0, 7.0],
        )
    };
    out.push(animal(
        "pig",
        [0.9, 0.9, 0.765],
        10.0,
        vec![
            baby_layer(
                &["pig#main"],
                pig([236, 160, 160]),
                &["entity/pig/pig_temperate", "entity/pig/pig_warm"],
                71,
            ),
            baby_layer(
                &["cold_pig#main"],
                pig([200, 150, 140]),
                &["entity/pig/pig_cold"],
                72,
            ),
        ],
    ));
    let sheep_head = || {
        cube(
            "head",
            [0.0, 6.0, -8.0],
            [-3.0, -4.0, -6.0],
            [6.0, 6.0, 8.0],
            face([224, 196, 170], [20, 20, 20], None),
        )
    };
    let sheep_body = |paint: Paint, grow: f32| {
        cube(
            "body",
            [0.0, 0.0, 0.0],
            [-4.0, 5.0, -8.0],
            [8.0, 8.0, 16.0],
            paint,
        )
        .grow(grow)
    };
    let sheep_legs = |paint: &dyn Fn() -> Paint, height: f32, grow: f32| -> Vec<Cube> {
        [
            ("right_hind_leg", -3.0, 7.0),
            ("left_hind_leg", 3.0, 7.0),
            ("right_front_leg", -3.0, -5.0),
            ("left_front_leg", 3.0, -5.0),
        ]
        .into_iter()
        .map(|(n, x, z)| {
            cube(
                n,
                [x, 12.0, z],
                [-2.0, 0.0, -2.0],
                [4.0, height, 4.0],
                paint(),
            )
            .grow(grow)
        })
        .collect()
    };
    let wool = [244, 242, 236];
    let fleece = move |shade_by: u32| -> Paint {
        Box::new(move |f, x, y, _, _| {
            let curl = noise(x / 2, y / 2, shade_by) % 3 == 0;
            shade(if curl { [222, 220, 214] } else { wool }, f)
        })
    };
    let mut sheep_main = vec![sheep_head(), sheep_body(lit([224, 196, 170]), 0.0)];
    sheep_main.extend(sheep_legs(
        &|| banded([224, 196, 170], [90, 80, 70], 0.85),
        12.0,
        0.0,
    ));
    let wool_model = |grow: f32| {
        let mut cubes = vec![
            cube(
                "head",
                [0.0, 6.0, -8.0],
                [-3.0, -4.0, -4.0],
                [6.0, 6.0, 6.0],
                fleece(81),
            )
            .grow(0.6),
            sheep_body(fleece(82), grow),
        ];
        cubes.extend(sheep_legs(&|| fleece(83), 6.0, 0.5));
        cubes
    };
    out.push(animal(
        "sheep",
        [0.9, 1.3, 1.235],
        8.0,
        vec![
            baby_layer(&["sheep#main"], sheep_main, &["entity/sheep/sheep"], 80),
            baby_layer(
                &["sheep#wool"],
                wool_model(1.75),
                &["entity/sheep/sheep_wool"],
                84,
            ),
            layer(
                &["sheep#wool_undercoat"],
                wool_model(1.0),
                &["entity/sheep/sheep_wool_undercoat"],
                85,
            ),
        ],
    ));
    let chicken = |feathers: [u8; 3], seed: u32| {
        let head = cube(
            "head",
            [0.0, 15.0, -4.0],
            [-2.0, -6.0, -2.0],
            [4.0, 6.0, 3.0],
            face(feathers, [20, 20, 20], None),
        )
        .child(cube(
            "beak",
            [0.0, 0.0, 0.0],
            [-2.0, -4.0, -4.0],
            [4.0, 2.0, 2.0],
            solid([236, 170, 40]),
        ))
        .child(cube(
            "red_thing",
            [0.0, 0.0, 0.0],
            [-1.0, -2.0, -3.0],
            [2.0, 2.0, 2.0],
            solid([200, 40, 40]),
        ));
        let legs = [("right_leg", -1.5), ("left_leg", 1.5)].map(|(n, x)| {
            cube(
                n,
                [x, 19.0, 1.0],
                [-1.0, 0.0, -1.0],
                [2.0, 5.0, 2.0],
                solid([236, 170, 40]),
            )
        });
        let wings = [("right_wing", -3.0, -1.0), ("left_wing", 3.0, 0.0)].map(|(n, x, min_x)| {
            cube(
                n,
                [x, 13.0, 0.0],
                [min_x, 0.0, -3.0],
                [1.0, 4.0, 6.0],
                lit(feathers.map(|v| v.saturating_sub(16))),
            )
        });
        let mut cubes = vec![
            head,
            cube(
                "body",
                [0.0, 0.0, 0.0],
                [-3.0, 11.0, -3.0],
                [6.0, 6.0, 8.0],
                spotted(feathers, feathers.map(|v| v.saturating_sub(20)), seed),
            ),
        ];
        cubes.extend(legs);
        cubes.extend(wings);
        cubes
    };
    out.push(animal(
        "chicken",
        [0.4, 0.7, 0.644],
        4.0,
        vec![
            baby_layer(
                &["chicken#main"],
                chicken([246, 244, 238], 91),
                &[
                    "entity/chicken/chicken_temperate",
                    "entity/chicken/chicken_warm",
                ],
                91,
            ),
            baby_layer(
                &["cold_chicken#main"],
                chicken([150, 120, 96], 92),
                &["entity/chicken/chicken_cold"],
                92,
            ),
        ],
    ));
    out
}

fn leak_attrs(health: f64) -> &'static [(&'static str, f64)] {
    Box::leak(vec![("max_health", health), ("movement_speed", 0.25)].into_boxed_slice())
}

/// The entity types' names, for the network registry.
pub fn names() -> Vec<&'static str> {
    mobs().into_iter().map(|m| m.name).collect()
}

/// Writes every mob's skins and returns its models and types, for `entity-models.json`
/// and `entity-types.json`.
pub fn write(bundle: &mut Bundle) -> Result<(Map<String, Value>, Map<String, Value>)> {
    let mut models = Map::new();
    let mut types = Map::new();
    for mob in mobs() {
        for layer in &mob.layers {
            let skin = pack(&layer.cubes);
            let model = model_json(&layer.cubes, &skin, ADULT);
            for key in &layer.models {
                models.insert(format!("minecraft:{key}"), model.clone());
            }
            let baby = model_json(&layer.cubes, &skin, BABY);
            for key in &layer.babies {
                models.insert(format!("minecraft:{key}"), baby.clone());
            }
            let image = paint(&layer.cubes, &skin, layer.seed);
            for texture in &layer.textures {
                bundle.png(
                    format!("assets/minecraft/textures/{texture}.png"),
                    &image,
                    Origin::Generator(GENERATOR),
                )?;
            }
        }
        let attributes: Map<String, Value> = mob
            .attributes
            .iter()
            .map(|(k, v)| (format!("minecraft:{k}"), json!(v)))
            .collect();
        types.insert(
            format!("minecraft:{}", mob.name),
            json!({
                "width": mob.size[0],
                "height": mob.size[1],
                "eye_height": mob.size[2],
                "category": mob.category,
                "fire_immune": false,
                "attributes": attributes,
            }),
        );
    }
    Ok((models, types))
}
