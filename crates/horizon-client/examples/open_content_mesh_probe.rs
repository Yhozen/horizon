//! Exports Horizon Open Creative's baked block meshes for inspection in Blender
//! (tools/open_content_blender.py, docs/open-creative.md). It loads the open
//! bundle through the game's own loaders (`Assets::load`, the model catalog),
//! takes each state's baked quads (positions in block units, UVs per sprite,
//! the sprite each face uses) and writes them as JSON beside the textures.
//!
//!   cargo run --profile fast -p horizon-client --example open_content_mesh_probe -- RESOURCES OUT
//!
//! Water is not a baked model (the fluid renderer draws it): it is exported as
//! its still texture on the surface box the fluid renderer uses, marked so.
use horizon_client::{assets::Assets, mobs};
use horizon_core::position::BlockPos;
use horizon_world::{chunk::BlockState, registry::BlockStateId};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().ok_or("usage: RESOURCES OUT")?);
    let out = PathBuf::from(args.next().ok_or("usage: RESOURCES OUT")?);
    std::fs::create_dir_all(out.join("textures"))?;
    let assets = Assets::load(&root)?;
    if !assets.profile.is_open() {
        return Err("these are not Horizon Open Creative resources".into());
    }
    let render = assets.reload(&root)?;
    let names: BTreeMap<u32, String> = render
        .sprite_layers
        .iter()
        .map(|(name, layer)| (*layer, name.trim_start_matches("minecraft:").to_owned()))
        .collect();
    let registry = assets.registry;
    let state = |name: &str, props: &[(&str, &str)]| -> Result<BlockStateId> {
        Ok(registry.resolve(&BlockState {
            name: format!("minecraft:{name}"),
            properties: props
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        })?)
    };
    let palette = [
        "stone",
        "cobblestone",
        "dirt",
        "grass_block",
        "sand",
        "gravel",
        "sandstone",
        "smooth_stone",
        "obsidian",
        "bedrock",
        "oak_log",
        "oak_planks",
        "oak_leaves",
        "bricks",
        "glass",
        "glowstone",
        "white_wool",
        "black_wool",
        "red_wool",
        "orange_wool",
        "yellow_wool",
        "green_wool",
        "blue_wool",
        "purple_wool",
    ];
    let mut entries: Vec<(String, BlockStateId)> = Vec::new();
    for name in palette {
        entries.push((name.to_owned(), state(name, &[])?));
    }
    for axis in ["x", "y", "z"] {
        entries.push((
            format!("oak_log[axis={axis}]"),
            state("oak_log", &[("axis", axis)])?,
        ));
    }
    entries.push(("short_grass".into(), state("short_grass", &[])?));
    let mut textures = BTreeMap::new();
    let mut blocks = Vec::new();
    for (label, id) in &entries {
        let mut faces = Vec::new();
        for (_, model) in render.model_catalog.collect(*id, BlockPos::new(0, 0, 0))? {
            for face in &model.faces {
                let sprite = names
                    .get(&face.layer)
                    .cloned()
                    .ok_or("a face without a sprite")?;
                textures.insert(sprite.clone(), ());
                faces.push(json!({
                    "positions": face.quad.positions,
                    "uv": face.quad.uv,
                    "facing": format!("{:?}", face.quad.facing),
                    "cull": face.quad.cull.map(|d| format!("{d:?}")),
                    "sprite": sprite,
                    "translucent": face.translucent,
                    "opaque": face.opaque,
                }));
            }
        }
        if faces.is_empty() {
            return Err(format!("{label} baked no faces").into());
        }
        blocks.push(json!({"label": label, "state": id.0, "faces": faces}));
    }
    // Water: the still texture on the fluid renderer's source surface box.
    let water_top = 14.0 / 16.0;
    let quad = |p: [[f32; 3]; 4], facing: &str| {
        json!({"positions": p, "uv": [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            "facing": facing, "sprite": "block/water_still", "translucent": true, "opaque": false})
    };
    let t = water_top;
    blocks.push(json!({"label": "water (fluid renderer surface)", "state": state("water", &[])?.0, "faces": [
        quad([[0.0, t, 0.0], [0.0, t, 1.0], [1.0, t, 1.0], [1.0, t, 0.0]], "Up"),
        quad([[0.0, 0.0, 0.0], [0.0, t, 0.0], [1.0, t, 0.0], [1.0, 0.0, 0.0]], "North"),
        quad([[1.0, 0.0, 1.0], [1.0, t, 1.0], [0.0, t, 1.0], [0.0, 0.0, 1.0]], "South"),
        quad([[0.0, 0.0, 1.0], [0.0, t, 1.0], [0.0, t, 0.0], [0.0, 0.0, 0.0]], "West"),
        quad([[1.0, 0.0, 0.0], [1.0, t, 0.0], [1.0, t, 1.0], [1.0, 0.0, 1.0]], "East"),
    ]}));
    textures.insert("block/water_still".into(), ());
    // Copy each texture with its frame count (animated sprites stack frames).
    let mut texture_info = BTreeMap::new();
    for name in textures.keys() {
        let from = root.join(format!("assets/minecraft/textures/{name}.png"));
        let to = out
            .join("textures")
            .join(format!("{}.png", name.replace('/', "__")));
        std::fs::copy(&from, &to)?;
        let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&from)?));
        let info = decoder.read_info()?.info().clone();
        texture_info.insert(
            name.clone(),
            json!({"file": to.file_name().unwrap().to_string_lossy(), "width": info.width,
                "height": info.height, "frames": (info.height / info.width.max(1)).max(1)}),
        );
    }
    // The first-person arm: the player model's right arm, posed.
    let arm = mobs::load_model(&root, "minecraft:player#main")
        .and_then(|m| m.children.into_iter().find(|c| c.name == "right_arm"))
        .ok_or("the open bundle has no player arm")?;
    let polygons: Vec<Value> = arm
        .polygons
        .iter()
        .map(|p| json!({"vertices": p.vertices, "normal": p.normal}))
        .collect();
    std::fs::copy(
        root.join("assets/minecraft/textures/entity/player/wide/steve.png"),
        out.join("textures/player_skin.png"),
    )?;
    // The held block's first-person transform from the block models' parent.
    let base: Value = serde_json::from_slice(&std::fs::read(
        root.join("assets/minecraft/models/block/open_base.json"),
    )?)?;
    let doc = json!({
        "generator": "open_content_mesh_probe",
        "blocks": blocks,
        "textures": texture_info,
        "arm": {"pose": arm.pose, "polygons": polygons, "texture": "player_skin.png", "texture_size": [64, 64]},
        "held": {"display": base["display"]["firstperson_righthand"], "blocks": ["grass_block", "oak_log", "glass", "red_wool"]},
        "calibration": calibration(&out)?,
    });
    std::fs::write(out.join("meshes.json"), serde_json::to_vec_pretty(&doc)?)?;
    println!(
        "{}: {} block meshes, {} textures",
        out.display(),
        doc["blocks"].as_array().unwrap().len(),
        texture_info.len()
    );
    Ok(())
}

/// A 64x64 calibration image: four coloured quadrants and an "F" whose
/// mirror image or rotation shows at a glance on any face.
fn calibration(out: &std::path::Path) -> Result<String> {
    let size = 64u32;
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let quadrant = [
                [0xe0, 0x40, 0x40],
                [0x40, 0xc0, 0x40],
                [0x40, 0x60, 0xe0],
                [0xe0, 0xd0, 0x40],
            ][((y >= 32) as usize) * 2 + (x >= 32) as usize];
            let f = (16..48).contains(&y) && (20..26).contains(&x)
                || (16..22).contains(&y) && (20..46).contains(&x)
                || (29..35).contains(&y) && (20..40).contains(&x);
            let c = if f { [16, 16, 16] } else { quadrant };
            let i = ((y * size + x) * 4) as usize;
            pixels[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    let file = std::fs::File::create(out.join("textures/calibration.png"))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&pixels)?;
    Ok("calibration.png".into())
}
