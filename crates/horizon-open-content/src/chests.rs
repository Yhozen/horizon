//! Chests and shulker boxes, drawn by the client's block-entity renderer:
//! the `chest`, `double_chest_left`, `double_chest_right` and `shulker_box`
//! layers of `entity-models.json`, in Minecraft's box layout (the same part
//! names, boxes, pivots and texture origins as its models, on 64x64 skins),
//! and their skins under `textures/entity/chest/` and `entity/shulker/` from
//! the pack (a plain placeholder where it has none). The blocks themselves
//! and their facts are in [`crate::plants`]'s facts file.
use crate::mobs::{Box3, Face, faces};
use crate::sources::Sources;
use crate::{Bundle, Origin, Result};
use serde_json::{Map, Value, json};

/// One box: `min` and `size` (pixels), texture origin `uv`, and the face it
/// leaves out (a double chest's half is open toward the other half).
struct Part {
    name: &'static str,
    pivot: [f32; 3],
    min: [f32; 3],
    size: [f32; 3],
    uv: [u32; 2],
    open: Option<Face>,
}

/// A chest: its bottom, lid and lock. `x0` and `width` place the bottom and
/// lid across the block, `lock_x` the lock; `open` is the side a double
/// chest's half leaves out.
fn chest(x0: f32, width: f32, lock_x: f32, lock_width: f32, open: Option<Face>) -> Vec<Part> {
    vec![
        Part {
            name: "bottom",
            pivot: [0.0; 3],
            min: [x0, 0.0, 1.0],
            size: [width, 10.0, 14.0],
            uv: [0, 19],
            open,
        },
        Part {
            name: "lid",
            pivot: [0.0, 9.0, 1.0],
            min: [x0, 0.0, 0.0],
            size: [width, 5.0, 14.0],
            uv: [0, 0],
            open,
        },
        Part {
            name: "lock",
            pivot: [0.0, 9.0, 1.0],
            min: [lock_x, -2.0, 14.0],
            size: [lock_width, 4.0, 1.0],
            uv: [0, 0],
            open,
        },
    ]
}

/// The layer as `entity-models.json` holds it: a root with one child per
/// part, each a quad per face with UVs on a 64x64 skin.
fn layer(parts: &[Part]) -> Value {
    let mut children = Map::new();
    for p in parts {
        let b = Box3 {
            min: p.min,
            size: p.size,
            uv: p.uv,
            grow: 0.0,
            mirror: false,
            paint: Box::new(|_, _, _, _, _| [0; 4]),
        };
        let (mut polygons, mut normals) = (Vec::new(), Vec::new());
        for (face, corners, [u0, v0, u1, v1], normal) in faces(&b) {
            if Some(face) == p.open {
                continue;
            }
            let uvs = [[u1, v0], [u0, v0], [u0, v1], [u1, v1]];
            let quad: Vec<Value> = corners
                .iter()
                .zip(uvs)
                .map(|(c, [u, v])| json!([c[0], c[1], c[2], u as f32 / 64.0, v as f32 / 64.0]))
                .collect();
            polygons.push(Value::Array(quad));
            normals.push(json!(normal));
        }
        let [x, y, z] = p.pivot;
        children.insert(
            p.name.into(),
            json!({
                "pose": [x, y, z, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                "polygons": polygons,
                "normals": normals,
                "children": {},
            }),
        );
    }
    json!({"pose": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0], "polygons": [], "children": children})
}

/// The chest layers for `entity-models.json`.
pub fn layers() -> Map<String, Value> {
    let mut out = Map::new();
    out.insert(
        "minecraft:chest#main".into(),
        layer(&chest(1.0, 14.0, 7.0, 2.0, None)),
    );
    // The left half's open side is the right one (-X), the right half's +X.
    out.insert(
        "minecraft:double_chest_left#main".into(),
        layer(&chest(0.0, 15.0, 0.0, 1.0, Some(Face::Right))),
    );
    out.insert(
        "minecraft:double_chest_right#main".into(),
        layer(&chest(1.0, 15.0, 15.0, 1.0, Some(Face::Left))),
    );
    // A shulker box: its lid over its base, both about the block's centre.
    out.insert(
        "minecraft:shulker_box#main".into(),
        layer(&[
            Part {
                name: "lid",
                pivot: [0.0, 24.0, 0.0],
                min: [-8.0, -16.0, -8.0],
                size: [16.0, 12.0, 16.0],
                uv: [0, 0],
                open: None,
            },
            Part {
                name: "base",
                pivot: [0.0, 24.0, 0.0],
                min: [-8.0, -8.0, -8.0],
                size: [16.0, 8.0, 16.0],
                uv: [0, 28],
                open: None,
            },
        ]),
    );
    out
}

const DYES: &[&str] = &[
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

/// The skins the chest blocks name (`entity/chest/<name>.png`), with the
/// halves of a double chest.
const SKINS: &[&str] = &[
    "normal",
    "trapped",
    "ender",
    "copper",
    "copper_exposed",
    "copper_weathered",
    "copper_oxidized",
];

/// The pack's skin, else a plain placeholder of that colour.
fn skin(bundle: &mut Bundle, sources: &Sources, destination: String, color: [u8; 4]) -> Result<()> {
    let (image, origin) = if sources.provides(&destination) {
        sources.image(&destination)?
    } else {
        (
            crate::image::Rgba::filled(64, 64, color),
            Origin::Generator("art"),
        )
    };
    bundle.png(destination, &image, origin)
}

/// Writes the chest and shulker box skins.
pub fn write(bundle: &mut Bundle, sources: &Sources) -> Result<()> {
    for dye in std::iter::once("").chain(DYES.iter().copied()) {
        let name = if dye.is_empty() {
            "shulker".to_owned()
        } else {
            format!("shulker_{dye}")
        };
        let destination = format!("assets/minecraft/textures/entity/shulker/{name}.png");
        skin(bundle, sources, destination, [150, 100, 150, 255])?;
    }
    for name in SKINS {
        let halves: &[&str] = if *name == "ender" {
            &[""]
        } else {
            &["", "_left", "_right"]
        };
        for half in halves {
            let destination = format!("assets/minecraft/textures/entity/chest/{name}{half}.png");
            let color = if name.starts_with("copper") {
                [184, 104, 72, 255]
            } else if *name == "ender" {
                [24, 32, 32, 255]
            } else {
                [150, 104, 48, 255]
            };
            skin(bundle, sources, destination, color)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_chest_layers_have_minecraft_parts() {
        let layers = super::layers();
        for key in [
            "minecraft:chest#main",
            "minecraft:double_chest_left#main",
            "minecraft:double_chest_right#main",
        ] {
            let children = layers[key]["children"].as_object().unwrap();
            let mut names: Vec<_> = children.keys().cloned().collect();
            names.sort();
            assert_eq!(names, ["bottom", "lid", "lock"], "{key}");
        }
        let lid = &layers["minecraft:chest#main"]["children"]["lid"];
        assert_eq!(lid["polygons"].as_array().unwrap().len(), 6);
        let half = &layers["minecraft:double_chest_left#main"]["children"]["bottom"];
        assert_eq!(half["polygons"].as_array().unwrap().len(), 5);
    }
}
