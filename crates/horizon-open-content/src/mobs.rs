//! The open edition's mobs: the common creatures and monsters of a survival world, so
//! open players see what shares their world (through the relay, a Java server's mobs).
//!
//! Each mob is a model of boxes under Minecraft's part names (`head`, `hat`,
//! `right_arm`, `left_hind_leg`...) in Minecraft's standard box layout: every box has
//! the texture origin, size, pivot and mirroring of the Java model, and the skin has
//! the Java texture's size, so a skin from a Minecraft-layout resource pack fits the
//! open model. The skins painted here are Horizon's own: a painter colours each face
//! of each box. Nothing comes from Minecraft's art.
use crate::image::{Rgba, noise};
use crate::{Bundle, Origin, Result};
use serde_json::{Map, Value, json};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, FRAC_PI_8};

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
pub(crate) type Paint = Box<dyn Fn(Face, u32, u32, u32, u32) -> [u8; 4]>;

/// One box of a part: at `min` with `size` (model units), its skin at texture origin
/// `uv` in the standard box layout, grown by `grow` on every side (the texture keeps
/// the unscaled size), and `mirror`ed left to right as Minecraft's mirrored boxes are.
pub(crate) struct Box3 {
    pub(crate) min: [f32; 3],
    pub(crate) size: [f32; 3],
    pub(crate) uv: [u32; 2],
    pub(crate) grow: f32,
    pub(crate) mirror: bool,
    pub(crate) paint: Paint,
}

/// One part: its boxes, posed at `pivot` with `rot` (radians), and optional child
/// parts that move with it.
struct Cube {
    name: &'static str,
    pivot: [f32; 3],
    rot: [f32; 3],
    boxes: Vec<Box3>,
    children: Vec<Cube>,
}

/// A part of one box with its texture origin `uv`.
fn cube(
    name: &'static str,
    pivot: [f32; 3],
    min: [f32; 3],
    size: [f32; 3],
    uv: [u32; 2],
    paint: Paint,
) -> Cube {
    Cube {
        name,
        pivot,
        rot: [0.0; 3],
        boxes: vec![Box3 {
            min,
            size,
            uv,
            grow: 0.0,
            mirror: false,
            paint,
        }],
        children: Vec::new(),
    }
}

impl Cube {
    fn rot(mut self, rot: [f32; 3]) -> Self {
        self.rot = rot;
        self
    }
    /// Grows the last box.
    fn grow(mut self, grow: f32) -> Self {
        self.boxes.last_mut().expect("a box").grow = grow;
        self
    }
    /// Mirrors the last box.
    fn mirror(mut self) -> Self {
        self.boxes.last_mut().expect("a box").mirror = true;
        self
    }
    /// Another box in the same part.
    fn with(mut self, min: [f32; 3], size: [f32; 3], uv: [u32; 2], paint: Paint) -> Self {
        self.boxes.push(Box3 {
            min,
            size,
            uv,
            grow: 0.0,
            mirror: false,
            paint,
        });
        self
    }
    fn child(mut self, child: Cube) -> Self {
        self.children.push(child);
        self
    }
}

/// Visits every part, parents before children, in a stable order.
fn walk<'a>(cubes: &'a [Cube], out: &mut Vec<&'a Cube>) {
    for c in cubes {
        out.push(c);
        walk(&c.children, out);
    }
}

/// The six faces of a box in Minecraft's box layout from texture origin `[u, v]`
/// (w, h, d the box's size): the top at (u + d, v), the bottom beside it, then the
/// right side, front, left side and back in a row below. Each face: its corners in
/// the order of the rectangle's (u1, v0) (u0, v0) (u0, v1) (u1, v1), the pixel
/// rectangle (u0, v0, u1, v1) and its outward normal. A mirrored box swaps its X
/// extent, so its sides and every face read the other way round.
pub(crate) fn faces(b: &Box3) -> [(Face, [[f32; 3]; 4], [u32; 4], [f32; 3]); 6] {
    let [mut x0, y0, z0] = b.min.map(|v| v - b.grow);
    let [mut x1, y1, z1] = [
        b.min[0] + b.size[0] + b.grow,
        b.min[1] + b.size[1] + b.grow,
        b.min[2] + b.size[2] + b.grow,
    ];
    let mut sx = 1.0;
    if b.mirror {
        (x0, x1) = (x1, x0);
        sx = -1.0;
    }
    let [w, h, d] = b.size.map(|v| v.ceil() as u32);
    let [u, v] = b.uv;
    [
        (
            Face::Top,
            [[x1, y0, z1], [x0, y0, z1], [x0, y0, z0], [x1, y0, z0]],
            [u + d, v, u + d + w, v + d],
            [0.0, -1.0, 0.0],
        ),
        (
            Face::Bottom,
            [[x1, y1, z1], [x0, y1, z1], [x0, y1, z0], [x1, y1, z0]],
            [u + d + w, v, u + d + 2 * w, v + d],
            [0.0, 1.0, 0.0],
        ),
        (
            Face::Right,
            [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
            [u, v + d, u + d, v + d + h],
            [-sx, 0.0, 0.0],
        ),
        (
            Face::Front,
            [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
            [u + d, v + d, u + d + w, v + d + h],
            [0.0, 0.0, -1.0],
        ),
        (
            Face::Left,
            [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
            [u + d + w, v + d, u + 2 * d + w, v + d + h],
            [sx, 0.0, 0.0],
        ),
        (
            Face::Back,
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            [u + 2 * d + w, v + d, u + 2 * d + 2 * w, v + d + h],
            [0.0, 0.0, 1.0],
        ),
    ]
}

/// One part as `entity-models.json` holds it, its geometry scaled by `scale` about its
/// pivot (a baby's model: see `Baby`), at `pivot`. The UVs stay the adult's, so a baby
/// wears the adult's skin layout.
fn part_json(c: &Cube, size: [u32; 2], scale: f32, pivot: [f32; 3]) -> Value {
    let (tw, th) = (size[0] as f32, size[1] as f32);
    let mut polygons = Vec::new();
    let mut normals = Vec::new();
    for b in &c.boxes {
        for (_, corners, [u0, v0, u1, v1], normal) in faces(b) {
            let corners = corners.map(|p| p.map(|v| v * scale));
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
    }
    let mut children = Map::new();
    for child in &c.children {
        let at = child.pivot.map(|v| v * scale);
        children.insert(child.name.into(), part_json(child, size, scale, at));
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

/// The model's root: every top-level part a child, at the origin.
fn model_json(cubes: &[Cube], size: [u32; 2], baby: Baby) -> Value {
    let mut children = Map::new();
    for c in cubes {
        let scale = if c.name == "head" {
            baby.head
        } else {
            baby.body
        };
        let [x, y, z] = c.pivot;
        let at = [x * baby.body, 24.0 - (24.0 - y) * baby.body, z * baby.body];
        children.insert(c.name.into(), part_json(c, size, scale, at));
    }
    json!({ "pose": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0], "polygons": [], "children": children })
}

/// The skin: every face painted, with a little per-pixel shading so flat colours read as
/// material rather than plastic.
fn paint(cubes: &[Cube], size: [u32; 2], seed: u32) -> Rgba {
    let mut image = Rgba::new(size[0], size[1]);
    let mut all = Vec::new();
    walk(cubes, &mut all);
    for b in all.iter().flat_map(|c| &c.boxes) {
        for (face, _, [u0, v0, u1, v1], _) in faces(b) {
            let (fw, fh) = (u1 - u0, v1 - v0);
            for y in 0..fh {
                for x in 0..fw {
                    let [r, g, bl, a] = (b.paint)(face, x, y, fw, fh);
                    if a == 0 {
                        continue;
                    }
                    let n = (noise(u0 + x, v0 + y, seed) % 13) as i32 - 6;
                    let f = |v: u8| (v as i32 + n).clamp(0, 255) as u8;
                    image.set(u0 + x, v0 + y, [f(r), f(g), f(bl), a]);
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
fn clear() -> Paint {
    Box::new(|_, _, _, _, _| CLEAR)
}

// MODELS
//
// Box positions, sizes, pivots and texture origins are Minecraft's (layout facts, so a
// Minecraft-layout skin fits); the painters are Horizon's.

/// A humanoid's limbs, as Minecraft lays them out.
#[derive(Clone, Copy)]
enum Limbs {
    /// Zombies and husks: 4-wide limbs, the left ones mirroring the right ones' skin.
    Mirrored,
    /// Drowned: 4-wide limbs, the left ones with their own skin.
    Own,
    /// Skeletons: 2-wide limbs, mirrored.
    Thin,
}

/// A humanoid: head (with an empty `hat` overlay), body, arms and legs.
fn humanoid(
    head: Paint,
    body: Paint,
    arm: impl Fn() -> Paint,
    leg: impl Fn() -> Paint,
    limbs: Limbs,
) -> Vec<Cube> {
    let (limb, arm_x, leg_x) = match limbs {
        Limbs::Thin => (2.0, -1.0, 2.0),
        _ => (4.0, -3.0, 1.9),
    };
    let half = limb / 2.0;
    let left = |c: Cube, own: [u32; 2]| match limbs {
        Limbs::Own => Cube {
            boxes: c.boxes.into_iter().map(|b| Box3 { uv: own, ..b }).collect(),
            ..c
        },
        _ => c.mirror(),
    };
    vec![
        cube(
            "head",
            [0.0, 0.0, 0.0],
            [-4.0, -8.0, -4.0],
            [8.0; 3],
            [0, 0],
            head,
        )
        .child(
            cube(
                "hat",
                [0.0; 3],
                [-4.0, -8.0, -4.0],
                [8.0; 3],
                [32, 0],
                clear(),
            )
            .grow(0.5),
        ),
        cube(
            "body",
            [0.0; 3],
            [-4.0, 0.0, -2.0],
            [8.0, 12.0, 4.0],
            [16, 16],
            body,
        ),
        // Arms hang from the shoulders: the right one outward to -X, the left to +X.
        cube(
            "right_arm",
            [-5.0, 2.0, 0.0],
            [arm_x, -2.0, -half],
            [limb, 12.0, limb],
            [40, 16],
            arm(),
        ),
        left(
            cube(
                "left_arm",
                [5.0, 2.0, 0.0],
                [-1.0, -2.0, -half],
                [limb, 12.0, limb],
                [40, 16],
                arm(),
            ),
            [32, 48],
        ),
        cube(
            "right_leg",
            [-leg_x, 12.0, 0.0],
            [-half, 0.0, -half],
            [limb, 12.0, limb],
            [0, 16],
            leg(),
        ),
        left(
            cube(
                "left_leg",
                [leg_x, 12.0, 0.0],
                [-half, 0.0, -half],
                [limb, 12.0, limb],
                [0, 16],
                leg(),
            ),
            [16, 48],
        ),
    ]
}

/// A four-legged animal's legs: `height` tall at `x` either side, the front pair at
/// `z[0]`, the hind pair at `z[1]`; the left legs mirror the right ones' skin.
fn legs(height: f32, x: f32, z: [f32; 2], paint: impl Fn() -> Paint) -> Vec<Cube> {
    [
        ("right_hind_leg", -x, z[1], false),
        ("left_hind_leg", x, z[1], true),
        ("right_front_leg", -x, z[0], false),
        ("left_front_leg", x, z[0], true),
    ]
    .into_iter()
    .map(|(name, x, z, left)| {
        let leg = cube(
            name,
            [x, 24.0 - height, z],
            [-2.0, 0.0, -2.0],
            [4.0, height, 4.0],
            [0, 16],
            paint(),
        );
        if left { leg.mirror() } else { leg }
    })
    .collect()
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
    /// The skin's size in pixels: the Minecraft texture's.
    size: [u32; 2],
    /// Keys of this model's baby version (the same skin layout, smaller).
    babies: Vec<&'static str>,
    /// Texture paths under `textures/` that take this skin (baby textures too: a baby
    /// draws the adult model with the `_baby` texture).
    textures: Vec<String>,
    seed: u32,
    /// The adult model's root: a lift (model units, +Y down) and a scale, as Minecraft
    /// sizes a kind from another's model (a cave spider is a small spider).
    root: (f32, f32),
}

impl Layer {
    fn root(mut self, lift: f32, scale: f32) -> Self {
        self.root = (lift, scale);
        self
    }
}

fn layer(
    models: &[&'static str],
    cubes: Vec<Cube>,
    size: [u32; 2],
    textures: &[&str],
    seed: u32,
) -> Layer {
    Layer {
        models: models.to_vec(),
        babies: Vec::new(),
        cubes,
        size,
        textures: textures.iter().map(|t| t.to_string()).collect(),
        seed,
        root: (0.0, 1.0),
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
fn baby_layer(
    models: &[&'static str],
    cubes: Vec<Cube>,
    size: [u32; 2],
    textures: &[&str],
    seed: u32,
) -> Layer {
    Layer {
        babies: models
            .iter()
            .map(|m| {
                let (kind, layer) = m.split_once('#').expect("a model#layer key");
                leak(format!("{kind}_baby#{layer}"))
            })
            .collect(),
        textures: with_baby(textures),
        ..layer(models, cubes, size, &[], seed)
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
    let limbs = if name == "drowned" {
        Limbs::Own
    } else {
        Limbs::Mirrored
    };
    let main = baby_layer(
        &[leak(format!("{name}#main"))],
        humanoid(
            face(skin, eye, Some([40, 30, 30])),
            banded(shirt, trousers, 0.75),
            || banded(shirt, skin, 0.4),
            || banded(trousers, [50, 46, 52], 0.85),
            limbs,
        ),
        [64, 64],
        &[texture],
        seed,
    );
    Mob {
        name,
        category: "monster",
        size: [0.6, 1.95, 1.74],
        attributes: MONSTER,
        // The husk is a zombie at 1.0625, its feet kept on the ground.
        layers: vec![if name == "husk" {
            main.root(-24.0 * 0.0625 - 0.001, 1.0625)
        } else {
            main
        }],
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
                Limbs::Thin,
            ),
            [64, 32],
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
    let build = |head: Paint, eyes: bool| {
        let paint = |p: Paint| if eyes { clear() } else { p };
        let mut cubes = vec![
            cube(
                "head",
                [0.0, 15.0, -3.0],
                [-4.0, -4.0, -8.0],
                [8.0; 3],
                [32, 4],
                head,
            ),
            cube(
                "body0",
                [0.0, 15.0, 0.0],
                [-3.0; 3],
                [6.0; 3],
                [0, 0],
                paint(lit(body)),
            ),
            cube(
                "body1",
                [0.0, 15.0, 9.0],
                [-5.0, -4.0, -6.0],
                [10.0, 8.0, 12.0],
                [0, 12],
                paint(spotted(body, body.map(|v| v.saturating_add(18)), seed)),
            ),
        ];
        for (n, z, yr, zr) in legs {
            for right in [true, false] {
                let name = leak(format!("{}_{n}_leg", if right { "right" } else { "left" }));
                let leg = cube(
                    name,
                    [if right { -4.0 } else { 4.0 }, 15.0, z],
                    [if right { -15.0 } else { -1.0 }, -1.0, -1.0],
                    [16.0, 2.0, 2.0],
                    [18, 0],
                    paint(banded(body, body.map(|v| v / 2), 0.5)),
                )
                .rot(leg_rot(right, yr, zr));
                cubes.push(if right { leg } else { leg.mirror() });
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
        layers: [{
            let main = layer(
                &[model],
                build(face(body, red, None), false),
                [64, 32],
                &[texture],
                seed,
            );
            // The cave spider is the spider's model at 0.7, its feet kept on the ground.
            if name == "cave_spider" {
                main.root(24.0 * 0.3 + 0.0048, 0.7)
            } else {
                main
            }
        }]
        .into_iter()
        // The glowing eyes: the same layout, clear but for the eyes.
        .chain(eyes.map(|eyes| layer(&[], build(eyes_only(red), true), [64, 32], &[eyes], seed)))
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
            [8.0; 3],
            [0, 0],
            creeper_face,
        ),
        cube(
            "body",
            [0.0, 6.0, 0.0],
            [-4.0, 0.0, -2.0],
            [8.0, 12.0, 4.0],
            [16, 16],
            spotted(green, [52, 120, 48], 42),
        ),
    ];
    // The creeper's legs all share one skin, none mirrored.
    creeper.extend(
        legs(6.0, 2.0, [-4.0, 4.0], || spotted(green, [52, 120, 48], 43))
            .into_iter()
            .map(|mut leg| {
                leg.boxes[0].mirror = false;
                leg
            }),
    );
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
            [64, 32],
            &["entity/creeper/creeper"],
            44,
        )],
    });

    // The enderman: a tall dark figure with long thin limbs and violet eyes.
    let black = [22, 20, 28];
    let violet = [200, 90, 240];
    let enderman = |head: Paint, rest: &dyn Fn() -> Paint| {
        let limb = |name, pivot: [f32; 3], min_y: f32, left: bool| {
            let c = cube(
                name,
                pivot,
                [-1.0, min_y, -1.0],
                [2.0, 30.0, 2.0],
                [56, 0],
                rest(),
            );
            if left { c.mirror() } else { c }
        };
        vec![
            cube(
                "head",
                [0.0, -13.0, 0.0],
                [-4.0, -8.0, -4.0],
                [8.0; 3],
                [0, 0],
                head,
            )
            .child(
                cube(
                    "hat",
                    [0.0; 3],
                    [-4.0, -8.0, -4.0],
                    [8.0; 3],
                    [0, 16],
                    rest(),
                )
                .grow(-0.5),
            ),
            cube(
                "body",
                [0.0, -14.0, 0.0],
                [-4.0, 0.0, -2.0],
                [8.0, 12.0, 4.0],
                [32, 16],
                rest(),
            ),
            limb("right_arm", [-5.0, -12.0, 0.0], -2.0, false),
            limb("left_arm", [5.0, -12.0, 0.0], -2.0, true),
            limb("right_leg", [-2.0, -5.0, 0.0], 0.0, false),
            limb("left_leg", [2.0, -5.0, 0.0], 0.0, true),
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
                [64, 32],
                &["entity/enderman/enderman"],
                51,
            ),
            layer(
                &[],
                enderman(eyes_only(violet), &clear),
                [64, 32],
                &["entity/enderman/enderman_eyes"],
                51,
            ),
        ],
    });

    // Farm animals. Each climate variant has Minecraft's model for it, with its own
    // colours.
    let animal = |name: &'static str, size: [f64; 3], health: f64, layers: Vec<Layer>| Mob {
        name,
        category: "creature",
        size,
        attributes: leak_attrs(health),
        layers,
    };
    #[derive(Clone, Copy, PartialEq)]
    enum Climate {
        Temperate,
        Warm,
        Cold,
    }
    let cow = |climate: Climate, coat: [u8; 3], patch: [u8; 3], seed: u32| {
        let horn = [220, 214, 196];
        let muzzle = [220, 160, 160];
        let mut head = cube(
            "head",
            [0.0, 4.0, -8.0],
            [-4.0, -4.0, -6.0],
            [8.0, 8.0, 6.0],
            [0, 0],
            face(coat, [20, 20, 20], None),
        )
        .with(
            [-3.0, 1.0, -7.0],
            [6.0, 3.0, 1.0],
            if climate == Climate::Cold {
                [9, 33]
            } else {
                [1, 33]
            },
            solid(muzzle),
        );
        head = match climate {
            Climate::Temperate => head
                .with([-5.0, -5.0, -5.0], [1.0, 3.0, 1.0], [22, 0], solid(horn))
                .with([4.0, -5.0, -5.0], [1.0, 3.0, 1.0], [22, 0], solid(horn)),
            // Long horns curving up.
            Climate::Warm => head
                .with([-8.0, -3.0, -5.0], [4.0, 2.0, 2.0], [27, 0], solid(horn))
                .with([-8.0, -5.0, -5.0], [2.0, 2.0, 2.0], [39, 0], solid(horn))
                .with([4.0, -3.0, -5.0], [4.0, 2.0, 2.0], [27, 0], solid(horn))
                .mirror()
                .with([6.0, -5.0, -5.0], [2.0, 2.0, 2.0], [39, 0], solid(horn))
                .mirror(),
            Climate::Cold => head
                .child(
                    cube(
                        "right_horn",
                        [-4.5, -2.5, -3.5],
                        [-1.5, -4.5, -0.5],
                        [2.0, 6.0, 2.0],
                        [0, 40],
                        solid(horn),
                    )
                    .rot([FRAC_PI_2, 0.0, 0.0]),
                )
                .child(
                    cube(
                        "left_horn",
                        [5.5, -2.5, -5.0],
                        [-1.5, -3.0, -0.5],
                        [2.0, 6.0, 2.0],
                        [0, 32],
                        solid(horn),
                    )
                    .rot([FRAC_PI_2, 0.0, 0.0]),
                ),
        };
        let mut body = cube(
            "body",
            [0.0, 5.0, 2.0],
            [-6.0, -10.0, -7.0],
            [12.0, 18.0, 10.0],
            [18, 4],
            spotted(coat, patch, seed),
        )
        .rot([FRAC_PI_2, 0.0, 0.0]);
        if climate == Climate::Cold {
            // A shaggy coat over the body.
            body.boxes.insert(
                0,
                Box3 {
                    min: [-6.0, -10.0, -7.0],
                    size: [12.0, 18.0, 10.0],
                    uv: [20, 32],
                    grow: 0.5,
                    mirror: false,
                    paint: spotted(coat.map(|v| v.saturating_add(12)), patch, seed + 2),
                },
            );
        }
        let body = body.with(
            [-2.0, 2.0, -8.0],
            [4.0, 6.0, 1.0],
            [52, 0],
            solid([230, 170, 170]),
        );
        let mut cubes = vec![head, body];
        cubes.extend(legs(12.0, 4.0, [-5.0, 7.0], move || {
            spotted(coat, patch, seed + 1)
        }));
        cubes
    };
    out.push(animal(
        "cow",
        [0.9, 1.4, 1.3],
        10.0,
        vec![
            baby_layer(
                &["cow#main"],
                cow(Climate::Temperate, [92, 62, 44], [236, 230, 222], 61),
                [64, 64],
                &["entity/cow/cow_temperate"],
                61,
            ),
            baby_layer(
                &["warm_cow#main"],
                cow(Climate::Warm, [150, 96, 60], [214, 170, 120], 62),
                [64, 64],
                &["entity/cow/cow_warm"],
                62,
            ),
            baby_layer(
                &["cold_cow#main"],
                cow(Climate::Cold, [70, 64, 70], [190, 186, 190], 63),
                [64, 64],
                &["entity/cow/cow_cold"],
                63,
            ),
        ],
    ));
    let pig = |skin: [u8; 3], cold: bool| {
        let snout = [
            skin[0].saturating_sub(30),
            skin[1].saturating_sub(40),
            skin[2].saturating_sub(40),
        ];
        let head = cube(
            "head",
            [0.0, 12.0, -6.0],
            [-4.0, -4.0, -8.0],
            [8.0; 3],
            [0, 0],
            face(skin, [20, 20, 20], None),
        )
        .with([-2.0, 0.0, -9.0], [4.0, 3.0, 1.0], [16, 16], solid(snout));
        let mut body = cube(
            "body",
            [0.0, 11.0, 2.0],
            [-5.0, -10.0, -7.0],
            [10.0, 16.0, 8.0],
            [28, 8],
            lit(skin),
        )
        .rot([FRAC_PI_2, 0.0, 0.0]);
        if cold {
            // A woolly coat over the body.
            body = body
                .with(
                    [-5.0, -10.0, -7.0],
                    [10.0, 16.0, 8.0],
                    [28, 32],
                    spotted([236, 228, 214], [210, 200, 188], 73),
                )
                .grow(0.5);
        }
        let mut cubes = vec![head, body];
        cubes.extend(legs(6.0, 3.0, [-5.0, 7.0], move || lit(skin)));
        cubes
    };
    out.push(animal(
        "pig",
        [0.9, 0.9, 0.765],
        10.0,
        vec![
            baby_layer(
                &["pig#main"],
                pig([236, 160, 160], false),
                [64, 64],
                &["entity/pig/pig_temperate", "entity/pig/pig_warm"],
                71,
            ),
            baby_layer(
                &["cold_pig#main"],
                pig([200, 150, 140], true),
                [64, 64],
                &["entity/pig/pig_cold"],
                72,
            ),
        ],
    ));
    let sheep_body = |paint: Paint, grow: f32| {
        cube(
            "body",
            [0.0, 5.0, 2.0],
            [-4.0, -10.0, -7.0],
            [8.0, 16.0, 6.0],
            [28, 8],
            paint,
        )
        .rot([FRAC_PI_2, 0.0, 0.0])
        .grow(grow)
    };
    let wool = [244, 242, 236];
    let fleece = move |shade_by: u32| -> Paint {
        Box::new(move |f, x, y, _, _| {
            let curl = noise(x / 2, y / 2, shade_by) % 3 == 0;
            shade(if curl { [222, 220, 214] } else { wool }, f)
        })
    };
    // The bare sheep (and the undercoat, in the same layout).
    let sheep_main = |head: Paint, body: Paint, leg: &dyn Fn() -> Paint| {
        let mut cubes = vec![
            cube(
                "head",
                [0.0, 6.0, -8.0],
                [-3.0, -4.0, -6.0],
                [6.0, 6.0, 8.0],
                [0, 0],
                head,
            ),
            sheep_body(body, 0.0),
        ];
        // Minecraft mirrors the sheep's right legs rather than its left.
        cubes.extend(legs(12.0, 3.0, [-5.0, 7.0], leg).into_iter().map(|mut l| {
            l.boxes[0].mirror = l.name.starts_with("right");
            l
        }));
        cubes
    };
    let wool_model = || {
        let mut cubes = vec![
            cube(
                "head",
                [0.0, 6.0, -8.0],
                [-3.0, -4.0, -4.0],
                [6.0; 3],
                [0, 0],
                fleece(81),
            )
            .grow(0.6),
            sheep_body(fleece(82), 1.75),
        ];
        cubes.extend(
            legs(6.0, 3.0, [-5.0, 7.0], || fleece(83))
                .into_iter()
                .map(|leg| {
                    let mut leg = leg.grow(0.5);
                    leg.pivot[1] = 12.0;
                    leg.boxes[0].mirror = false;
                    leg
                }),
        );
        cubes
    };
    let tan = [224, 196, 170];
    out.push(animal(
        "sheep",
        [0.9, 1.3, 1.235],
        8.0,
        vec![
            baby_layer(
                &["sheep#main"],
                sheep_main(face(tan, [20, 20, 20], None), lit(tan), &|| {
                    banded(tan, [90, 80, 70], 0.85)
                }),
                [64, 32],
                &["entity/sheep/sheep"],
                80,
            ),
            baby_layer(
                &["sheep#wool"],
                wool_model(),
                [64, 32],
                &["entity/sheep/sheep_wool"],
                84,
            ),
            layer(
                &["sheep#wool_undercoat"],
                sheep_main(fleece(86), fleece(87), &|| fleece(88)),
                [64, 32],
                &["entity/sheep/sheep_wool_undercoat"],
                85,
            ),
        ],
    ));
    let chicken = |feathers: [u8; 3], seed: u32, cold: bool| {
        let mut head = cube(
            "head",
            [0.0, 15.0, -4.0],
            [-2.0, -6.0, -2.0],
            [4.0, 6.0, 3.0],
            [0, 0],
            face(feathers, [20, 20, 20], None),
        );
        if cold {
            // A ruff of feathers over the head.
            head = head.with(
                [-3.0, -7.0, -2.015],
                [6.0, 3.0, 4.0],
                [44, 0],
                lit(feathers.map(|v| v.saturating_add(20))),
            );
        }
        let head = head
            .child(cube(
                "beak",
                [0.0; 3],
                [-2.0, -4.0, -4.0],
                [4.0, 2.0, 2.0],
                [14, 0],
                solid([236, 170, 40]),
            ))
            .child(cube(
                "red_thing",
                [0.0; 3],
                [-1.0, -2.0, -3.0],
                [2.0; 3],
                [14, 4],
                solid([200, 40, 40]),
            ));
        let legs = [("right_leg", -2.0), ("left_leg", 1.0)].map(|(n, x)| {
            cube(
                n,
                [x, 19.0, 1.0],
                [-1.0, 0.0, -3.0],
                [3.0, 5.0, 3.0],
                [26, 0],
                solid([236, 170, 40]),
            )
        });
        let wings = [("right_wing", -4.0, 0.0), ("left_wing", 4.0, -1.0)].map(|(n, x, min_x)| {
            cube(
                n,
                [x, 13.0, 0.0],
                [min_x, 0.0, -3.0],
                [1.0, 4.0, 6.0],
                [24, 13],
                lit(feathers.map(|v| v.saturating_sub(16))),
            )
        });
        let mut body = cube(
            "body",
            [0.0, 16.0, 0.0],
            [-3.0, -4.0, -3.0],
            [6.0, 8.0, 6.0],
            [0, 9],
            spotted(feathers, feathers.map(|v| v.saturating_sub(20)), seed),
        )
        .rot([FRAC_PI_2, 0.0, 0.0]);
        if cold {
            // A flat tail.
            body = body.with(
                [0.0, 3.0, -1.0],
                [0.0, 3.0, 5.0],
                [38, 9],
                lit(feathers.map(|v| v.saturating_sub(30))),
            );
        }
        let mut cubes = vec![head, body];
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
                chicken([246, 244, 238], 91, false),
                [64, 32],
                &[
                    "entity/chicken/chicken_temperate",
                    "entity/chicken/chicken_warm",
                ],
                91,
            ),
            baby_layer(
                &["cold_chicken#main"],
                chicken([150, 120, 96], 92, true),
                [64, 32],
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
            let mut model = model_json(&layer.cubes, layer.size, ADULT);
            let (lift, scale) = layer.root;
            model["pose"] = json!([0.0, lift, 0.0, 0.0, 0.0, 0.0, scale, scale, scale]);
            for key in &layer.models {
                models.insert(format!("minecraft:{key}"), model.clone());
            }
            let baby = model_json(&layer.cubes, layer.size, BABY);
            for key in &layer.babies {
                models.insert(format!("minecraft:{key}"), baby.clone());
            }
            let image = paint(&layer.cubes, layer.size, layer.seed);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every face of every box lies on its skin, and the skins have Minecraft's sizes.
    #[test]
    fn every_face_lies_on_its_skin() {
        for mob in mobs() {
            for layer in &mob.layers {
                assert!(
                    [[64, 64], [64, 32]].contains(&layer.size),
                    "{}: {:?}",
                    mob.name,
                    layer.size
                );
                let mut all = Vec::new();
                walk(&layer.cubes, &mut all);
                for c in all {
                    for b in &c.boxes {
                        for (face, _, [u0, v0, u1, v1], _) in faces(b) {
                            assert!(
                                u0 <= u1 && v0 <= v1 && u1 <= layer.size[0] && v1 <= layer.size[1],
                                "{} {} {face:?}: {u0},{v0}..{u1},{v1}",
                                mob.name,
                                c.name
                            );
                        }
                    }
                }
            }
        }
    }

    /// The standard box layout, checked on the zombie's head (8x8x8 from 0,0): the top
    /// at (8, 0), the front at (8, 8), the left side (+X) at (16, 8), the back at (24, 8);
    /// and on its mirrored left leg, whose +X side takes the right side's area.
    #[test]
    fn boxes_follow_the_standard_layout() {
        let zombie = &mobs()[0].layers[0].cubes;
        let head = &zombie[0].boxes[0];
        let rects: Vec<(Face, [u32; 4])> = faces(head).iter().map(|f| (f.0, f.2)).collect();
        assert_eq!(
            rects,
            [
                (Face::Top, [8, 0, 16, 8]),
                (Face::Bottom, [16, 0, 24, 8]),
                (Face::Right, [0, 8, 8, 16]),
                (Face::Front, [8, 8, 16, 16]),
                (Face::Left, [16, 8, 24, 16]),
                (Face::Back, [24, 8, 32, 16]),
            ]
        );
        let leg = zombie.iter().find(|c| c.name == "left_leg").unwrap();
        let side = faces(&leg.boxes[0])[2];
        assert_eq!(side.2, [0, 20, 4, 32]);
        assert_eq!(
            side.3,
            [1.0, 0.0, 0.0],
            "the mirrored leg's outer side is +X"
        );
    }
}
