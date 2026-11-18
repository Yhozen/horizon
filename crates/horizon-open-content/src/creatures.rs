//! More of the open edition's creatures: the villagers, golems, illagers, the
//! tameable and wild animals, the fish and the Nether's mobs, beside the
//! hand-built ones in [`crate::mobs`].
//!
//! Each is built from `data/mobs/creatures.txt` (written by
//! `tools/open_mob_layouts.py`): its entity type's facts (hit box, category,
//! default attributes) and its model layers in Minecraft's standard box layout,
//! with Minecraft's part names, poses, boxes, texture origins, growth and
//! mirrored boxes, the young's own models included; only numbers and names. So
//! a skin in Minecraft's layout fits: each texture the renderer may ask for is
//! the vendored pack's when it has one at that path (`Sources::image`), and
//! otherwise painted here, face by face on the model's boxes, by Horizon's own
//! painter. Nothing comes from Minecraft's art.
use crate::image::{Rgba, noise};
use crate::sources::Sources;
use crate::{Bundle, Origin, Result};
use serde_json::{Map, Value, json};

const DATA: &str = include_str!("../data/mobs/creatures.txt");
const GENERATOR: &str = "crates/horizon-open-content/src/creatures.rs";

/// A box in Minecraft's layout: its corner and size (model units, +Y down), its
/// texture origin, its growth on each axis (the texture keeps the size), whether it
/// is mirrored, and which of its faces (top, bottom, right, front, left, back) it has.
#[derive(Debug, Clone)]
struct Box3 {
    min: [f32; 3],
    size: [f32; 3],
    uv: [f32; 2],
    grow: [f32; 3],
    mirror: bool,
    faces: [bool; 6],
}

#[derive(Debug, Clone)]
struct Part {
    name: String,
    /// x, y, z, x/y/z rotation (radians), x/y/z scale.
    pose: [f32; 9],
    boxes: Vec<Box3>,
    children: Vec<Part>,
}

#[derive(Debug)]
struct Model {
    key: String,
    size: [u32; 2],
    root: Part,
}

#[derive(Debug)]
struct Texture {
    /// Under `textures/`, without `.png`.
    path: String,
    /// Minecraft's texture size: a pack's may be a multiple of it.
    size: [u32; 2],
    /// The model key whose layout it wears.
    model: String,
}

#[derive(Debug)]
pub struct Creature {
    pub name: String,
    width: f64,
    height: f64,
    eye_height: f64,
    category: String,
    fire_immune: bool,
    attributes: Vec<(String, f64)>,
    models: Vec<Model>,
    textures: Vec<Texture>,
}

fn numbers<const N: usize>(fields: &[&str]) -> [f32; N] {
    let mut out = [0.0; N];
    for (o, f) in out.iter_mut().zip(fields) {
        *o = f
            .parse()
            .unwrap_or_else(|_| panic!("creatures.txt: {f} is not a number"));
    }
    out
}

fn find<'a>(part: &'a mut Part, path: &[&str]) -> &'a mut Part {
    match path.split_first() {
        None => part,
        Some((name, rest)) => {
            let child = part
                .children
                .iter_mut()
                .position(|c| c.name == *name)
                .unwrap_or_else(|| panic!("creatures.txt: no part {name}"));
            find(&mut part.children[child], rest)
        }
    }
}

/// Every creature `creatures.txt` lists, in its order.
pub fn creatures() -> Vec<Creature> {
    let mut out: Vec<Creature> = Vec::new();
    let mut path: Vec<String> = Vec::new();
    for line in DATA.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let creature = out.last_mut();
        match f[0] {
            "mob" => out.push(Creature {
                name: f[1].into(),
                width: f[2].parse().expect("width"),
                height: f[3].parse().expect("height"),
                eye_height: f[4].parse().expect("eye height"),
                category: f[5].into(),
                fire_immune: f[6] == "1",
                attributes: Vec::new(),
                models: Vec::new(),
                textures: Vec::new(),
            }),
            "attr" => creature
                .expect("a mob")
                .attributes
                .push((f[1].into(), f[2].parse().expect("an attribute value"))),
            "model" => creature.expect("a mob").models.push(Model {
                key: f[1].into(),
                size: [f[2].parse().expect("width"), f[3].parse().expect("height")],
                root: Part {
                    name: "root".into(),
                    pose: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                    boxes: Vec::new(),
                    children: Vec::new(),
                },
            }),
            "part" => {
                let model = creature.and_then(|c| c.models.last_mut()).expect("a model");
                path = if f[1] == "." {
                    Vec::new()
                } else {
                    f[1].split('/').map(str::to_owned).collect()
                };
                let pose = numbers::<9>(&f[2..11]);
                match path.split_last() {
                    None => model.root.pose = pose,
                    Some((name, parent)) => {
                        let parent: Vec<&str> = parent.iter().map(String::as_str).collect();
                        find(&mut model.root, &parent).children.push(Part {
                            name: name.clone(),
                            pose,
                            boxes: Vec::new(),
                            children: Vec::new(),
                        });
                    }
                }
            }
            "box" => {
                let model = creature.and_then(|c| c.models.last_mut()).expect("a model");
                let at: Vec<&str> = path.iter().map(String::as_str).collect();
                let n = numbers::<11>(&f[1..12]);
                let faces: Vec<char> = f[13].chars().collect();
                find(&mut model.root, &at).boxes.push(Box3 {
                    min: [n[0], n[1], n[2]],
                    size: [n[3], n[4], n[5]],
                    uv: [n[6], n[7]],
                    grow: [n[8], n[9], n[10]],
                    mirror: f[12] == "1",
                    faces: std::array::from_fn(|i| faces[i] != '-'),
                });
            }
            "texture" => creature.expect("a mob").textures.push(Texture {
                path: f[1].into(),
                size: [f[2].parse().expect("width"), f[3].parse().expect("height")],
                model: f[4].into(),
            }),
            other => panic!("creatures.txt: unknown line {other}"),
        }
    }
    out
}

/// The entity types' names.
pub fn names() -> Vec<String> {
    creatures().into_iter().map(|c| c.name).collect()
}

/// A face of a box (as `mobs::Face`): which slot, its corners, its pixel rectangle
/// (u0, v0, u1, v1) and outward normal, in Minecraft's order (top, bottom, right,
/// front, left, back) and corner order.
type Quad = (usize, [[f32; 3]; 4], [f32; 4], [f32; 3]);

fn faces(b: &Box3) -> Vec<Quad> {
    let [mut x0, y0, z0] = std::array::from_fn(|i| b.min[i] - b.grow[i]);
    let [mut x1, y1, z1] = std::array::from_fn(|i| b.min[i] + b.size[i] + b.grow[i]);
    let mut sx = 1.0;
    if b.mirror {
        (x0, x1) = (x1, x0);
        sx = -1.0;
    }
    let [w, h, d] = b.size;
    let [u, v] = b.uv;
    let all: [Quad; 6] = [
        (
            0,
            [[x1, y0, z1], [x0, y0, z1], [x0, y0, z0], [x1, y0, z0]],
            [u + d, v, u + d + w, v + d],
            [0.0, -1.0, 0.0],
        ),
        (
            1,
            // Minecraft's bottom face reads from the back edge.
            [[x1, y1, z0], [x0, y1, z0], [x0, y1, z1], [x1, y1, z1]],
            [u + d + w, v, u + d + 2.0 * w, v + d],
            [0.0, 1.0, 0.0],
        ),
        (
            2,
            [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
            [u, v + d, u + d, v + d + h],
            [-sx, 0.0, 0.0],
        ),
        (
            3,
            [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
            [u + d, v + d, u + d + w, v + d + h],
            [0.0, 0.0, -1.0],
        ),
        (
            4,
            [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
            [u + d + w, v + d, u + 2.0 * d + w, v + d + h],
            [sx, 0.0, 0.0],
        ),
        (
            5,
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            [u + 2.0 * d + w, v + d, u + 2.0 * d + 2.0 * w, v + d + h],
            [0.0, 0.0, 1.0],
        ),
    ];
    all.into_iter().filter(|q| b.faces[q.0]).collect()
}

/// One part as `entity-models.json` holds it.
fn part_json(p: &Part, size: [u32; 2]) -> Value {
    let (tw, th) = (size[0] as f32, size[1] as f32);
    let mut polygons = Vec::new();
    let mut normals = Vec::new();
    for b in &p.boxes {
        for (slot, corners, [u0, v0, u1, v1], normal) in faces(b) {
            let uvs = if slot == 1 {
                [[u1, v1], [u0, v1], [u0, v0], [u1, v0]]
            } else {
                [[u1, v0], [u0, v0], [u0, v1], [u1, v1]]
            };
            let mut quad: Vec<Value> = corners
                .iter()
                .zip(uvs)
                .map(|(c, [u, v])| json!([c[0], c[1], c[2], u / tw, v / th]))
                .collect();
            // A mirrored box's faces turn the other way, as Minecraft's do.
            if b.mirror {
                quad.reverse();
            }
            polygons.push(Value::Array(quad));
            normals.push(json!(normal));
        }
    }
    let children: Map<String, Value> = p
        .children
        .iter()
        .map(|c| (c.name.clone(), part_json(c, size)))
        .collect();
    json!({ "pose": p.pose, "polygons": polygons, "normals": normals, "children": children })
}

// PAINTING: Horizon's own skins, for the textures the pack doesn't have.

/// A creature's colours: coat, a second coat colour, and eyes.
fn palette(name: &str) -> ([u8; 3], [u8; 3], [u8; 3]) {
    let named: &[(&str, [u8; 3], [u8; 3], [u8; 3])] = &[
        ("villager", [176, 128, 98], [104, 74, 52], [40, 120, 60]),
        ("zombie_villager", [96, 140, 82], [92, 64, 48], [24, 30, 24]),
        (
            "wandering_trader",
            [176, 128, 98],
            [52, 74, 140],
            [40, 60, 120],
        ),
        ("iron_golem", [206, 198, 186], [92, 120, 60], [160, 30, 30]),
        ("ghast", [236, 236, 236], [200, 200, 204], [40, 40, 40]),
        (
            "tropical_fish",
            [236, 120, 40],
            [244, 240, 230],
            [20, 20, 20],
        ),
    ];
    if let Some((_, a, b, e)) = named.iter().find(|(n, ..)| *n == name) {
        return (*a, *b, *e);
    }
    let h = noise(name.len() as u32, name.bytes().map(u32::from).sum(), 7);
    let base = [
        60 + (h % 150) as u8,
        60 + (h / 150 % 150) as u8,
        60 + (h / 22500 % 150) as u8,
    ];
    (base, base.map(|v| v / 2 + 20), [20, 20, 20])
}

/// Whether a texture is an overlay drawn over the creature's base skin: mostly clear.
fn is_overlay(path: &str) -> bool {
    [
        "/profession",
        "/type/",
        "pattern",
        "crackiness",
        "collar",
        "markings",
        "overlay",
    ]
    .iter()
    .any(|k| path.contains(k))
}

fn shade(c: [u8; 3], slot: usize) -> [u8; 3] {
    let k = match slot {
        0 => 1.08,
        1 => 0.82,
        5 => 0.94,
        _ => 1.0,
    };
    c.map(|v| ((v as f32) * k).min(255.0) as u8)
}

/// One pixel of a face: `x`, `y` from the face's top left, of `w` x `h`.
#[allow(clippy::too_many_arguments)]
fn pixel(
    part: &str,
    slot: usize,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    texture: &str,
    colours: ([u8; 3], [u8; 3], [u8; 3]),
    seed: u32,
) -> Option<[u8; 3]> {
    let (coat, second, eye) = colours;
    if is_overlay(texture) {
        let seed = seed ^ texture.bytes().map(u32::from).sum::<u32>();
        let tint = [
            90 + (noise(1, 2, seed) % 150) as u8,
            60 + (noise(3, 4, seed) % 150) as u8,
            40 + (noise(5, 6, seed) % 150) as u8,
        ];
        let draped = ["body", "jacket", "robe", "arms"]
            .iter()
            .any(|p| part.contains(p));
        return if texture.contains("crackiness") {
            (noise(x, y, seed) % 9 == 0).then_some([60, 56, 52])
        } else if draped && (texture.contains("profession") || y % 4 < 2) {
            Some(shade(tint, slot))
        } else {
            None
        };
    }
    // The face: on the head's front, or the body's when there is no head.
    if slot == 3
        && (part == "head" || part.ends_with("/head") || part == "body")
        && w >= 4
        && h >= 4
    {
        let ey = h / 2 - 1;
        let (l, r) = (w / 4, w - 1 - w / 4);
        if y == ey && (x == l || x == r) {
            return Some(eye);
        }
    }
    let limb = [
        "leg", "arm", "fin", "tail", "tentacle", "wing", "ear", "horn",
    ]
    .iter()
    .any(|p| part.contains(p));
    let c = if limb && y * 4 >= h * 3 || noise(x / 2, y / 2, seed) % 7 == 0 {
        second
    } else {
        coat
    };
    Some(shade(c, slot))
}

fn paint(model: &Model, size: [u32; 2], texture: &str, name: &str, seed: u32) -> Rgba {
    let mut image = Rgba::new(size[0], size[1]);
    let colours = palette(name);
    let (kx, ky) = (
        size[0] as f32 / model.size[0] as f32,
        size[1] as f32 / model.size[1] as f32,
    );
    fn each<'a>(p: &'a Part, path: String, out: &mut Vec<(String, &'a Box3)>) {
        for b in &p.boxes {
            out.push((path.clone(), b));
        }
        for c in &p.children {
            let at = if path.is_empty() {
                c.name.clone()
            } else {
                format!("{path}/{}", c.name)
            };
            each(c, at, out);
        }
    }
    let mut boxes = Vec::new();
    each(&model.root, String::new(), &mut boxes);
    for (part, b) in boxes {
        let leaf = part.rsplit('/').next().unwrap_or("");
        for (slot, _, [u0, v0, u1, v1], _) in faces(b) {
            let (x0, y0) = (
                (u0 * kx).floor().max(0.0) as u32,
                (v0 * ky).floor().max(0.0) as u32,
            );
            let (x1, y1) = (
                ((u1 * kx).ceil() as u32).min(size[0]),
                ((v1 * ky).ceil() as u32).min(size[1]),
            );
            let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
            for y in 0..h {
                for x in 0..w {
                    let Some(c) = pixel(leaf, slot, x, y, w, h, texture, colours, seed) else {
                        continue;
                    };
                    let n = (noise(x0 + x, y0 + y, seed) % 13) as i32 - 6;
                    let f = |v: u8| (v as i32 + n).clamp(0, 255) as u8;
                    image.set(x0 + x, y0 + y, [f(c[0]), f(c[1]), f(c[2]), 255]);
                }
            }
        }
    }
    image
}

/// Whether a pack's image fits Minecraft's texture of `size`: the same or a whole
/// multiple of it, so the layout's texture coordinates land on the same pixels.
fn fits(image: &Rgba, size: [u32; 2]) -> bool {
    image.width % size[0] == 0
        && image.height % size[1] == 0
        && image.width / size[0] == image.height / size[1]
}

/// Writes every creature's skins and returns its models and types, for
/// `entity-models.json` and `entity-types.json`.
pub fn write(
    bundle: &mut Bundle,
    sources: &Sources,
) -> Result<(Map<String, Value>, Map<String, Value>)> {
    let mut models = Map::new();
    let mut types = Map::new();
    for (i, c) in creatures().into_iter().enumerate() {
        for m in &c.models {
            models.insert(format!("minecraft:{}", m.key), part_json(&m.root, m.size));
        }
        for t in &c.textures {
            let destination = format!("assets/minecraft/textures/{}.png", t.path);
            if bundle.contains(&destination) {
                continue;
            }
            if let Some((bytes, origin)) = sources.raw(&destination)
                && sources
                    .image(&destination)
                    .is_ok_and(|(image, _)| fits(&image, t.size))
            {
                bundle.add(destination, bytes.to_vec(), origin)?;
                continue;
            }
            let model = c
                .models
                .iter()
                .find(|m| m.key == t.model)
                .ok_or_else(|| format!("{}: {} wears no model {}", c.name, t.path, t.model))?;
            let image = paint(model, t.size, &t.path, &c.name, 200 + i as u32);
            bundle.png(destination, &image, Origin::Generator(GENERATOR))?;
        }
        let attributes: Map<String, Value> = c
            .attributes
            .iter()
            .map(|(k, v)| (format!("minecraft:{k}"), json!(v)))
            .collect();
        types.insert(
            format!("minecraft:{}", c.name),
            json!({
                "width": c.width,
                "height": c.height,
                "eye_height": c.eye_height,
                "category": c.category,
                "fire_immune": c.fire_immune,
                "attributes": attributes,
            }),
        );
    }
    Ok((models, types))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn every_creature_has_models_and_textures_on_its_layouts() {
        let all = creatures();
        assert!(all.len() >= 50, "{}", all.len());
        let own = crate::mobs::own_names();
        for c in &all {
            assert!(
                !own.contains(&c.name.as_str()),
                "{} is also in mobs.rs",
                c.name
            );
            assert!(!c.models.is_empty() && !c.textures.is_empty(), "{}", c.name);
            for t in &c.textures {
                let m = c.models.iter().find(|m| m.key == t.model);
                let m = m.unwrap_or_else(|| panic!("{}: no model {}", t.path, t.model));
                assert_eq!(t.size[0] * m.size[1], t.size[1] * m.size[0], "{}", t.path);
            }
        }
    }

    /// The open models are Minecraft's: with the prepared vanilla resources at hand
    /// (`.local/horizon-client/26.3`, docs/parity-workflow.md), every layer's part
    /// tree, poses and polygons (corners, texture coordinates, normals) equal
    /// `entity-models.json`'s, corner for corner. Without them it checks nothing.
    #[test]
    fn creature_models_equal_minecraft_layouts() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/horizon-client/26.3/entity-models.json");
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("no {}: skipped", path.display());
            return;
        };
        let vanilla: Value = serde_json::from_slice(&bytes).unwrap();
        fn close(a: &Value, b: &Value) -> bool {
            match (a, b) {
                (Value::Array(a), Value::Array(b)) => {
                    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| close(a, b))
                }
                (Value::Number(a), Value::Number(b)) => {
                    (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-4
                }
                _ => a == b,
            }
        }
        fn degenerate(poly: &Value) -> bool {
            let corners: Vec<_> = poly
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    let v = v.as_array().unwrap();
                    (0..3)
                        .map(|i| (v[i].as_f64().unwrap() * 1e4).round() as i64)
                        .collect::<Vec<_>>()
                })
                .collect();
            let mut unique = corners.clone();
            unique.sort();
            unique.dedup();
            unique.len() < 4
        }
        fn compare(key: &str, at: &str, want: &Value, got: &Value, bad: &mut Vec<String>) {
            if !close(&want["pose"], &got["pose"]) {
                bad.push(format!(
                    "{key} {at}: pose {} vs {}",
                    want["pose"], got["pose"]
                ));
            }
            let polys = |v: &Value| -> Vec<(Value, Value)> {
                let p = v["polygons"].as_array().unwrap();
                let n = v["normals"].as_array().unwrap();
                p.iter()
                    .cloned()
                    .zip(n.iter().cloned())
                    .filter(|(p, _)| !degenerate(p))
                    .collect()
            };
            let (w, g) = (polys(want), polys(got));
            if w.len() != g.len() {
                bad.push(format!("{key} {at}: {} polygons vs {}", w.len(), g.len()));
            } else if let Some(i) = (0..w.len()).find(|&i| !close(&w[i].0, &g[i].0)) {
                bad.push(format!("{key} {at}: polygon {i} {} vs {}", w[i].0, g[i].0));
            } else if let Some(i) = (0..w.len()).find(|&i| !close(&w[i].1, &g[i].1)) {
                bad.push(format!("{key} {at}: normal {i} {} vs {}", w[i].1, g[i].1));
            }
            let names = |v: &Value| -> Vec<String> {
                let mut k: Vec<String> =
                    v["children"].as_object().unwrap().keys().cloned().collect();
                k.sort();
                k
            };
            if names(want) != names(got) {
                bad.push(format!(
                    "{key} {at}: parts {:?} vs {:?}",
                    names(want),
                    names(got)
                ));
                return;
            }
            for name in names(want) {
                let at = format!("{at}/{name}");
                compare(
                    key,
                    &at,
                    &want["children"][&name],
                    &got["children"][&name],
                    bad,
                );
            }
        }
        let mut bad = Vec::new();
        for c in creatures() {
            for m in &c.models {
                let key = format!("minecraft:{}", m.key);
                compare(
                    &key,
                    "",
                    &vanilla[&key],
                    &part_json(&m.root, m.size),
                    &mut bad,
                );
            }
        }
        assert!(
            bad.is_empty(),
            "{} differences:\n{}",
            bad.len(),
            bad[..bad.len().min(20)].join("\n")
        );
    }
}
