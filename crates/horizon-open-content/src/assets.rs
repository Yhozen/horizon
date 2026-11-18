//! Client assets generated from the block definitions: textures (open inputs
//! or original art), block-state definitions, models with their own display
//! transforms, item definitions and prototypes, the creative palette and the
//! English translations.
use crate::{
    Bundle, Origin, Result,
    blocks::{Block, Category, Faces, Form, Layer, Tier},
    image::{Rgba, noise},
    items::{Item, Kind},
    sources::Sources,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const GENERATOR: &str = "assets";

/// The parent every block model in the bundle uses: its GUI, hand, ground and
/// frame transforms, chosen for this edition.
fn base_model() -> Value {
    json!({
        "gui_light": "side",
        "display": {
            "gui": {"rotation": [30, 45, 0], "translation": [0, 0, 0], "scale": [0.62, 0.62, 0.62]},
            "ground": {"rotation": [0, 0, 0], "translation": [0, 3, 0], "scale": [0.25, 0.25, 0.25]},
            "fixed": {"rotation": [0, 0, 0], "translation": [0, 0, 0], "scale": [0.5, 0.5, 0.5]},
            "head": {"rotation": [0, 180, 0], "translation": [0, 0, 0], "scale": [1, 1, 1]},
            "thirdperson_righthand": {"rotation": [70, 40, 0], "translation": [0, 2.5, 0], "scale": [0.37, 0.37, 0.37]},
            "thirdperson_lefthand": {"rotation": [70, 40, 0], "translation": [0, 2.5, 0], "scale": [0.37, 0.37, 0.37]},
            "firstperson_righthand": {"rotation": [0, 40, 0], "translation": [0, 0, 0], "scale": [0.42, 0.42, 0.42]},
            "firstperson_lefthand": {"rotation": [0, 220, 0], "translation": [0, 0, 0], "scale": [0.42, 0.42, 0.42]},
        }
    })
}

fn face(texture: &str, cull: Option<&str>) -> Value {
    let mut f = json!({"uv": [0, 0, 16, 16], "texture": texture});
    if let Some(cull) = cull {
        f["cullface"] = json!(cull);
    }
    f
}

/// A unit cube with one texture variable per face.
fn cube_model(textures: BTreeMap<&str, String>, faces: [(&str, &str); 6]) -> Value {
    let mut face_map = serde_json::Map::new();
    for (dir, var) in faces {
        face_map.insert(dir.to_owned(), face(&format!("#{var}"), Some(dir)));
    }
    let mut t: serde_json::Map<String, Value> = textures
        .into_iter()
        .map(|(k, v)| (k.to_owned(), json!(v)))
        .collect();
    if !t.contains_key("particle") {
        let first = t.values().next().cloned().unwrap_or(Value::Null);
        t.insert("particle".into(), first);
    }
    json!({
        "parent": "minecraft:block/open_base",
        "textures": t,
        "elements": [{"from": [0, 0, 0], "to": [16, 16, 16], "faces": face_map}],
    })
}

fn tex(name: &str) -> String {
    format!("minecraft:block/{name}")
}

/// The blocks Minecraft's `BlockColors` tint (the client tints them by name):
/// grass, ferns, sugar cane and leaves. Their grey pack textures need the
/// faces Minecraft's models tint to carry `tintindex` 0.
const TINTED: &[&str] = &[
    "grass_block",
    "short_grass",
    "fern",
    "sugar_cane",
    "oak_leaves",
    "spruce_leaves",
    "birch_leaves",
    "jungle_leaves",
    "acacia_leaves",
    "dark_oak_leaves",
    "mangrove_leaves",
];

/// Textures a pack must not replace: water's colour is in Minetest Game's
/// art, and the biome's water tint stays white.
const KEEP_MEDIA: &[&str] = &["water_still", "water_flow"];

/// The grass block's side overlay, drawn tinted over its side as Minecraft's
/// model does.
const GRASS_OVERLAY: &str = "grass_block_side_overlay";

/// How a block's grey textures are tinted.
#[derive(Clone, Copy, Default)]
struct Tinting {
    /// Its tinted faces carry `tintindex` 0.
    tint: bool,
    /// The grass block draws `GRASS_OVERLAY` over its sides.
    overlay: bool,
}

fn tinted(mut face: Value, tint: bool) -> Value {
    if tint {
        face["tintindex"] = json!(0);
    }
    face
}

/// The tint of a tinted block's item. Grass, ferns and sugar cane take the
/// grass colormap at a fixed climate; spruce and birch leaves the constants
/// the client tints them with in the world; other leaves the foliage
/// colormap's colour at that climate.
fn item_tint(id: &str) -> Value {
    let argb =
        |rgb: u32| json!({"type": "minecraft:constant", "value": (0xFF00_0000 | rgb) as i32});
    match id {
        "spruce_leaves" => argb(0x61_9961),
        "birch_leaves" => argb(0x80_A755),
        id if id.ends_with("_leaves") => {
            let map = crate::art::colormap("foliage").expect("foliage colormap");
            // temperature 0.5, downfall 1.0
            let [r, g, b, _] = map.get(127, 127);
            argb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
        }
        _ => json!({"type": "minecraft:grass", "temperature": 0.5, "downfall": 1.0}),
    }
}

/// The block's model (or models: logs have one per axis) and its block-state
/// definition.
fn models(block: &Block, tinting: Tinting) -> (Vec<(String, Value)>, Option<Value>) {
    let id = block.id;
    let all = ["down", "up", "north", "south", "west", "east"];
    match block.faces {
        Faces::All(t) => {
            let mut model = cube_model([("all", tex(t))].into(), all.map(|d| (d, "all")));
            if tinting.tint {
                // Leaves: every face, as Minecraft's `block/leaves`.
                for d in all {
                    model["elements"][0]["faces"][d]["tintindex"] = json!(0);
                }
            }
            (
                vec![(format!("block/{id}"), model)],
                Some(json!({"variants": {"": {"model": format!("minecraft:block/{id}")}}})),
            )
        }
        Faces::Grass { top, side, bottom } => {
            let model = cube_model(
                [
                    ("top", tex(top)),
                    ("side", tex(side)),
                    ("bottom", tex(bottom)),
                    ("particle", tex(bottom)),
                ]
                .into(),
                [
                    ("down", "bottom"),
                    ("up", "top"),
                    ("north", "side"),
                    ("south", "side"),
                    ("west", "side"),
                    ("east", "side"),
                ],
            );
            let mut model = model;
            if tinting.tint {
                // Minecraft's grass block: the top tinted, and a second cube
                // drawing the tinted overlay on the four sides.
                model["elements"][0]["faces"]["up"]["tintindex"] = json!(0);
            }
            if tinting.overlay {
                model["textures"]["overlay"] = json!(tex(GRASS_OVERLAY));
                let sides = ["north", "south", "west", "east"]
                    .map(|d| (d, tinted(face("#overlay", Some(d)), tinting.tint)));
                let faces: serde_json::Map<String, Value> =
                    sides.into_iter().map(|(d, f)| (d.to_owned(), f)).collect();
                model["elements"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"from": [0, 0, 0], "to": [16, 16, 16], "faces": faces}));
            }
            (
                vec![(format!("block/{id}"), model)],
                Some(json!({"variants": {"": {"model": format!("minecraft:block/{id}")}}})),
            )
        }
        Faces::Sided {
            top,
            bottom,
            front,
            side,
        } => {
            let model = cube_model(
                [
                    ("top", tex(top)),
                    ("bottom", tex(bottom)),
                    ("front", tex(front)),
                    ("side", tex(side)),
                    ("particle", tex(front)),
                ]
                .into(),
                [
                    ("down", "bottom"),
                    ("up", "top"),
                    ("north", "front"),
                    ("south", "front"),
                    ("west", "side"),
                    ("east", "side"),
                ],
            );
            (
                vec![(format!("block/{id}"), model)],
                Some(json!({"variants": {"": {"model": format!("minecraft:block/{id}")}}})),
            )
        }
        Faces::Column { side, end } => {
            // Vertical column; the x and z variants rotate it.
            let model = cube_model(
                [
                    ("side", tex(side)),
                    ("end", tex(end)),
                    ("particle", tex(side)),
                ]
                .into(),
                [
                    ("down", "end"),
                    ("up", "end"),
                    ("north", "side"),
                    ("south", "side"),
                    ("west", "side"),
                    ("east", "side"),
                ],
            );
            if !block.has_property("axis") {
                // A column that never turns (bookshelf, cactus).
                return (
                    vec![(format!("block/{id}"), model)],
                    Some(json!({"variants": {"": {"model": format!("minecraft:block/{id}")}}})),
                );
            }
            (
                vec![(format!("block/{id}"), model)],
                Some(json!({"variants": {
                    "axis=x": {"model": format!("minecraft:block/{id}"), "x": 90, "y": 90},
                    "axis=y": {"model": format!("minecraft:block/{id}")},
                    "axis=z": {"model": format!("minecraft:block/{id}"), "x": 90},
                }})),
            )
        }
        Faces::Cross(t) => {
            let plane = |from: [f64; 3], to: [f64; 3], angle: f64| {
                json!({
                    "from": from, "to": to, "shade": false,
                    "rotation": {"origin": [8, 8, 8], "axis": "y", "angle": angle, "rescale": true},
                    "faces": {"north": face("#cross", None), "south": face("#cross", None)},
                })
            };
            let model = json!({
                "parent": "minecraft:block/open_base",
                "ambientocclusion": false,
                "textures": {"cross": tex(t), "particle": tex(t)},
                "elements": [
                    plane([0.8, 0.0, 8.0], [15.2, 16.0, 8.0], 45.0),
                    plane([8.0, 0.0, 0.8], [8.0, 16.0, 15.2], 45.0),
                ],
            });
            let mut model = fix_cross(model);
            if tinting.tint {
                // Minecraft's `block/tinted_cross`: both sides of both planes.
                for element in model["elements"].as_array_mut().unwrap() {
                    for (_, f) in element["faces"].as_object_mut().unwrap() {
                        f["tintindex"] = json!(0);
                    }
                }
            }
            (
                vec![(format!("block/{id}"), model)],
                Some(json!({"variants": {"": {"model": format!("minecraft:block/{id}")}}})),
            )
        }
        Faces::Cube(faces) | Faces::Pillar(faces) => {
            let model = six_model(faces);
            let model_id = format!("minecraft:block/{id}");
            let mut models = vec![(format!("block/{id}"), model.clone())];
            let lying = if let Faces::Pillar(_) = block.faces {
                let mut lying = model;
                lying["elements"][0]["faces"]["up"]["rotation"] = json!(180);
                models.push((format!("block/{id}_horizontal"), lying));
                format!("{model_id}_horizontal")
            } else {
                model_id.clone()
            };
            let definition = if block.has_property("axis") {
                json!({"variants": {
                    "axis=x": {"model": lying, "x": 90, "y": 90},
                    "axis=y": {"model": model_id},
                    "axis=z": {"model": lying, "x": 90},
                }})
            } else {
                json!({"variants": {"": {"model": model_id}}})
            };
            (models, Some(definition))
        }
        Faces::Facing { faces, lit_north } => {
            let mut lit = faces;
            lit[2] = lit_north;
            let mut variants = serde_json::Map::new();
            for (facing, y) in [("north", 0), ("east", 90), ("south", 180), ("west", 270)] {
                for state in ["false", "true"] {
                    let model = if state == "true" {
                        format!("minecraft:block/{id}_on")
                    } else {
                        format!("minecraft:block/{id}")
                    };
                    let mut variant = json!({"model": model});
                    if y != 0 {
                        variant["y"] = json!(y);
                    }
                    let key = if block.has_property("lit") {
                        format!("facing={facing},lit={state}")
                    } else if state == "false" {
                        format!("facing={facing}")
                    } else {
                        continue;
                    };
                    variants.insert(key, variant);
                }
            }
            (
                vec![
                    (format!("block/{id}"), six_model(faces)),
                    (format!("block/{id}_on"), six_model(lit)),
                ],
                Some(json!({"variants": variants})),
            )
        }
        Faces::Fluid { still, .. } => {
            // Fluids draw through the fluid renderer; the model, one per
            // fluid as Minecraft's `block/water` and `block/lava`, gives
            // particles. A bubble column uses water's.
            let fluid = block.fluid().unwrap_or(id);
            let models = if fluid == id {
                let model = json!({"textures": {"particle": tex(still)}});
                vec![(format!("block/{id}"), model)]
            } else {
                Vec::new()
            };
            (
                models,
                Some(json!({"variants": {"": {"model": format!("minecraft:block/{fluid}")}}})),
            )
        }
        Faces::None => (Vec::new(), None),
    }
}

/// A unit cube with a texture per face: down, up, north, south, west, east.
fn six_model(faces: [&str; 6]) -> Value {
    let dirs = ["down", "up", "north", "south", "west", "east"];
    let mut textures: BTreeMap<&str, String> =
        dirs.iter().zip(faces).map(|(d, t)| (*d, tex(t))).collect();
    // Minecraft's particle is the side (north for the oriented blocks).
    textures.insert("particle", tex(faces[2]));
    cube_model(textures, dirs.map(|d| (d, d)))
}

/// The second plane of a cross faces east and west.
fn fix_cross(mut model: Value) -> Value {
    if let Some(second) = model["elements"].get_mut(1) {
        second["faces"] = json!({"west": face("#cross", None), "east": face("#cross", None)});
    }
    model
}

pub fn write(
    bundle: &mut Bundle,
    blocks: &[Block],
    items: &[Item],
    sources: &Sources,
) -> Result<()> {
    bundle.json(
        "assets/minecraft/models/block/open_base.json",
        &base_model(),
        GENERATOR,
    )?;
    // Flat items: the built-in extrusion with this edition's transforms;
    // tools are held along the hand.
    bundle.json(
        "assets/minecraft/models/item/generated.json",
        &generated_model(),
        GENERATOR,
    )?;
    bundle.json(
        "assets/minecraft/models/item/handheld.json",
        &handheld_model(),
        GENERATOR,
    )?;

    let block_texture = |t: &str| format!("assets/minecraft/textures/block/{t}.png");
    let from_pack = |t: &str| {
        !KEEP_MEDIA.contains(&t)
            && sources
                .input(&block_texture(t))
                .is_some_and(|m| m.pack.is_some())
    };
    let tinting = |block: &Block| {
        if !TINTED.contains(&block.id) {
            return Tinting::default();
        }
        match block.faces {
            Faces::Grass { top, .. } => Tinting {
                tint: from_pack(top),
                overlay: from_pack(GRASS_OVERLAY),
            },
            Faces::All(t) | Faces::Cross(t) => Tinting {
                tint: from_pack(t),
                overlay: false,
            },
            _ => Tinting::default(),
        }
    };
    let mut textures = BTreeMap::new();
    let mut colors = BTreeMap::new();
    for block in blocks.iter().filter(|b| b.tier != Tier::Support) {
        for texture in block.textures() {
            textures.insert(texture, block.layer);
            colors.entry(texture).or_insert(block.map_color);
        }
        let tinting = tinting(block);
        if tinting.overlay {
            textures.insert(GRASS_OVERLAY, block.layer);
        }
        let (models, definition) = models(block, tinting);
        for (path, model) in models {
            bundle.json(
                format!("assets/minecraft/models/{path}.json"),
                &model,
                GENERATOR,
            )?;
        }
        if let Some(definition) = definition {
            bundle.json(
                format!("assets/minecraft/blockstates/{}.json", block.id),
                &definition,
                GENERATOR,
            )?;
        }
    }
    // Block textures: an open input, else Horizon's own art.
    for texture in textures.keys() {
        let destination = block_texture(texture);
        let input = if KEEP_MEDIA.contains(texture) {
            sources.media_input(&destination)
        } else {
            sources.input(&destination)
        };
        if let Some(input) = input {
            let (mut image, origin) = Sources::decode(input)?;
            if input.pack.is_none()
                && let Some(base) = overlay_base(texture)
            {
                // Minetest Game draws these as overlays: the grass side on
                // dirt, the minerals on stone. A pack's are whole textures.
                let (base, _) = sources.image(&block_texture(base))?;
                image = over(&image, &base);
            }
            bundle.png(destination.clone(), &image, origin)?;
            let meta = format!("{destination}.mcmeta");
            if input.pack.is_some() {
                // A pack's animation metadata, as it ships it.
                if let Some((bytes, origin)) = sources.raw(&meta) {
                    bundle.add(meta, bytes.to_vec(), origin)?;
                }
            } else if image.height > image.width {
                bundle.json(
                    format!("{destination}.mcmeta"),
                    &json!({"animation": {"frametime": 2, "interpolate": false}}),
                    GENERATOR,
                )?;
            }
        } else if let Some(image) = crate::art::block_texture(texture) {
            bundle.png(destination, &image, Origin::Generator("art"))?;
        } else {
            // A placeholder in the block's map colour until there is art.
            let rgb = colors.get(texture).copied().unwrap_or(0x808080);
            let color = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255];
            bundle.png(destination, &speckle(color), Origin::Generator("art"))?;
        }
    }
    // Breaking overlays: original art.
    for stage in 0..10 {
        bundle.png(
            format!("assets/minecraft/textures/block/destroy_stage_{stage}.png"),
            &crate::art::destroy_stage(stage),
            Origin::Generator("art"),
        )?;
    }
    // The fluid renderer also reads water's overlay (water seen through
    // glass): a plain original fill, as the biome's water tint is white.
    bundle.png(
        "assets/minecraft/textures/block/water_overlay.png",
        &speckle([40, 80, 200, 160]),
        Origin::Generator("art"),
    )?;

    // Items: definitions, prototypes and translations.
    let mut prototypes = BTreeMap::new();
    prototypes.insert(
        "minecraft:air".to_owned(),
        json!({"minecraft:max_stack_size": 64, "minecraft:item_model": "minecraft:air"}),
    );
    let mut lang = BTreeMap::new();
    for block in blocks.iter().filter(|b| b.tier != Tier::Support) {
        lang.insert(
            format!("block.minecraft.{}", block.id),
            block.name.to_owned(),
        );
        if !block.has_item() {
            continue;
        }
        // Plants are drawn flat in the hand and the inventory.
        let model = if let Faces::Cross(texture) = block.faces {
            let path = format!("item/{}", block.id);
            bundle.json(
                format!("assets/minecraft/models/{path}.json"),
                &json!({"parent": "minecraft:item/generated", "textures": {"layer0": tex(texture)}}),
                GENERATOR,
            )?;
            format!("minecraft:{path}")
        } else {
            format!("minecraft:block/{}", block.id)
        };
        let mut item_model = json!({"type": "minecraft:model", "model": model});
        if tinting(block).tint {
            item_model["tints"] = json!([item_tint(block.id)]);
        }
        bundle.json(
            format!("assets/minecraft/items/{}.json", block.id),
            &json!({"model": item_model}),
            GENERATOR,
        )?;
        prototypes.insert(
            block.key(),
            json!({
                "minecraft:max_stack_size": 64,
                "minecraft:item_model": block.key(),
                "minecraft:item_name": {"translate": format!("block.minecraft.{}", block.id)},
                "minecraft:rarity": "common",
            }),
        );
        lang.insert(
            format!("item.minecraft.{}", block.id),
            block.name.to_owned(),
        );
    }
    let mut item_textures = Vec::new();
    for item in items {
        let path = format!("item/{}", item.id);
        let parent = if matches!(item.kind, Kind::Tool(..)) {
            "minecraft:item/handheld"
        } else {
            "minecraft:item/generated"
        };
        bundle.json(
            format!("assets/minecraft/models/{path}.json"),
            &json!({"parent": parent, "textures": {"layer0": format!("minecraft:{}", item.texture)}}),
            GENERATOR,
        )?;
        bundle.json(
            format!("assets/minecraft/items/{}.json", item.id),
            &json!({"model": {"type": "minecraft:model", "model": format!("minecraft:{path}")}}),
            GENERATOR,
        )?;
        let mut prototype = serde_json::Map::new();
        prototype.insert("minecraft:max_stack_size".into(), json!(item.stack_size()));
        prototype.insert("minecraft:item_model".into(), json!(item.key()));
        prototype.insert(
            "minecraft:item_name".into(),
            json!({"translate": format!("item.minecraft.{}", item.id)}),
        );
        prototype.insert("minecraft:rarity".into(), json!("common"));
        prototype.extend(item.components());
        prototypes.insert(item.key(), Value::Object(prototype));
        lang.insert(format!("item.minecraft.{}", item.id), item.name.clone());
        item_textures.push(item.texture.clone());
    }
    for texture in item_textures {
        let destination = format!("assets/minecraft/textures/{texture}.png");
        let (image, origin) = sources.image(&destination)?;
        bundle.png(destination, &image, origin)?;
    }
    vanilla_items(bundle, sources, &mut prototypes, &mut lang)?;
    bundle.json("items.json", &json!(prototypes), GENERATOR)?;

    // The creative palette: one tab per category, then search and the
    // player's inventory, in the creative screen's own format.
    let stack = |id: &str| format!("{{id:\"{id}\",count:1}}");
    // Within a tab: logs, then planks, then the rest, each in definition order.
    let rank = |id: &str| {
        if id.ends_with("_log") {
            0
        } else if id.ends_with("_planks") {
            1
        } else {
            2
        }
    };
    // Definition order is blocks.rs's, not the registry's.
    let defined: BTreeMap<&str, usize> = crate::blocks::defined()
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id, i))
        .collect();
    let mut palette: Vec<(Category, String, String)> = Vec::new();
    for category in Category::ALL {
        let mut entries: Vec<(&str, String)> = blocks
            .iter()
            .filter(|b| b.has_item() && b.category == Some(category))
            .map(|b| (b.id, b.name.to_owned()))
            .collect();
        entries.sort_by_key(|(id, _)| (rank(id), defined.get(id).copied()));
        entries.extend(
            items
                .iter()
                .filter(|i| i.category == category)
                .map(|i| (i.id.as_str(), i.name.clone())),
        );
        palette.extend(
            entries
                .into_iter()
                .map(|(id, name)| (category, format!("minecraft:{id}"), name)),
        );
    }
    let mut tabs: Vec<Value> = Category::ALL
        .iter()
        .map(|category| {
            let (row, column) = category.place();
            let items: Vec<String> = palette
                .iter()
                .filter(|(c, _, _)| c == category)
                .map(|(_, id, _)| stack(id))
                .collect();
            json!({
                "id": format!("minecraft:open_{}", category.id()),
                "type": "CATEGORY",
                "row": row,
                "column": column,
                "title": category.title(),
                "icon": stack(&format!("minecraft:{}", category.icon())),
                "items": items,
            })
        })
        .collect();
    tabs.push(json!({
        "id": "minecraft:search",
        "type": "SEARCH",
        "row": "TOP",
        "column": 6,
        "title": "Search",
        "icon": stack("minecraft:glass"),
        "items": palette.iter().map(|(_, id, _)| stack(id)).collect::<Vec<_>>(),
        "lines": palette.iter().map(|(_, id, _)| vec![crate::lang::display_name(id)]).collect::<Vec<_>>(),
        "tags": palette.iter().map(|_| Vec::<String>::new()).collect::<Vec<_>>(),
    }));
    tabs.push(json!({
        "id": "minecraft:inventory",
        "type": "INVENTORY",
        "row": "BOTTOM",
        "column": 6,
        "title": "Inventory",
        "icon": stack("minecraft:oak_planks"),
        "items": [],
    }));
    for category in Category::ALL {
        lang.insert(
            format!("itemGroup.open.{}", category.id()),
            category.title().to_owned(),
        );
    }
    bundle.json("creative-inventory.json", &json!({"tabs": tabs}), GENERATOR)?;
    for (key, value) in crate::art::ui_strings() {
        lang.insert(key.to_owned(), value.to_owned());
    }
    for (key, value) in crate::lang::LABELS.iter().chain(crate::lang::SURVIVAL) {
        lang.insert((*key).to_owned(), (*value).to_owned());
    }
    lang.extend(crate::lang::key_names());
    crate::lang::fill(&mut lang);
    bundle.json("assets/minecraft/lang/en_us.json", &json!(lang), GENERATOR)?;
    let _ = (Form::Air, Layer::Solid);
    Ok(())
}

/// The texture an overlay input is composited on, if it is one.
pub fn overlay_base(texture: &str) -> Option<&'static str> {
    if texture == "grass_block_side" {
        Some("dirt")
    } else if texture.ends_with("_ore") {
        Some("stone")
    } else {
        None
    }
}

/// The flat items' parent: the built-in extrusion, with this edition's
/// transforms (lying on the ground, upright in the inventory).
fn generated_model() -> Value {
    json!({
        "parent": "minecraft:builtin/generated",
        "gui_light": "front",
        "display": {
            "ground": {"rotation": [0, 0, 0], "translation": [0, 2, 0], "scale": [0.5, 0.5, 0.5]},
            "head": {"rotation": [0, 180, 0], "translation": [0, 13, 7], "scale": [1, 1, 1]},
            "thirdperson_righthand": {"rotation": [0, 0, 0], "translation": [0, 3, 1], "scale": [0.55, 0.55, 0.55]},
            "firstperson_righthand": {"rotation": [0, -90, 25], "translation": [1.13, 3.2, 1.13], "scale": [0.68, 0.68, 0.68]},
            "fixed": {"rotation": [0, 180, 0], "translation": [0, 0, 0], "scale": [1, 1, 1]},
        }
    })
}

/// Tools: held along the hand, head up.
fn handheld_model() -> Value {
    json!({
        "parent": "minecraft:item/generated",
        "display": {
            "thirdperson_righthand": {"rotation": [0, -90, 55], "translation": [0, 4.0, 0.5], "scale": [0.85, 0.85, 0.85]},
            "thirdperson_lefthand": {"rotation": [0, 90, -55], "translation": [0, 4.0, 0.5], "scale": [0.85, 0.85, 0.85]},
            "firstperson_righthand": {"rotation": [0, -90, 25], "translation": [1.13, 3.2, 1.13], "scale": [0.68, 0.68, 0.68]},
            "firstperson_lefthand": {"rotation": [0, 90, -25], "translation": [1.13, 3.2, 1.13], "scale": [0.68, 0.68, 0.68]},
        }
    })
}

fn speckle(color: [u8; 4]) -> Rgba {
    let mut image = Rgba::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            let n = (noise(x, y, 7) % 24) as i32 - 12;
            let c = |v: u8| (v as i32 + n).clamp(0, 255) as u8;
            image.set(x, y, [c(color[0]), c(color[1]), c(color[2]), color[3]]);
        }
    }
    image
}

/// `top` alpha-composited over `bottom` (same size), fully opaque.
fn over(top: &Rgba, bottom: &Rgba) -> Rgba {
    let mut out = bottom.clone();
    for y in 0..top.height.min(bottom.height) {
        for x in 0..top.width.min(bottom.width) {
            let (t, b) = (top.get(x, y), bottom.get(x, y));
            let a = u32::from(t[3]);
            let mix = |i: usize| ((u32::from(t[i]) * a + u32::from(b[i]) * (255 - a)) / 255) as u8;
            out.set(x, y, [mix(0), mix(1), mix(2), 255]);
        }
    }
    out
}

/// Minecraft's items that are not blocks (`items::vanilla`): a model on
/// `item/generated` or `item/handheld` with the item's own texture, the item
/// definition, a prototype with Minecraft's default components, and a name.
/// The texture comes from an open input (an enabled pack) when one provides
/// it, else a generated placeholder icon.
fn vanilla_items(
    bundle: &mut Bundle,
    sources: &Sources,
    prototypes: &mut BTreeMap<String, Value>,
    lang: &mut BTreeMap<String, String>,
) -> Result<()> {
    for item in crate::items::vanilla() {
        let path = format!("item/{}", item.id);
        let parent = if item.handheld {
            "minecraft:item/handheld"
        } else {
            "minecraft:item/generated"
        };
        bundle.json(
            format!("assets/minecraft/models/{path}.json"),
            &json!({"parent": parent, "textures": {"layer0": format!("minecraft:{}", item.texture)}}),
            GENERATOR,
        )?;
        bundle.json(
            format!("assets/minecraft/items/{}.json", item.id),
            &json!({"model": {"type": "minecraft:model", "model": format!("minecraft:{path}")}}),
            GENERATOR,
        )?;
        let destination = format!("assets/minecraft/textures/{}.png", item.texture);
        if !bundle.contains(&destination) {
            let (image, origin) = if sources.provides(&destination) {
                sources.image(&destination)?
            } else {
                (placeholder_icon(item.id), Origin::Generator("art"))
            };
            bundle.png(destination, &image, origin)?;
        }
        let mut prototype = item.components.clone();
        // Tool rules may only name block tags the bundle has.
        if let Some(rules) = prototype
            .get_mut("minecraft:tool")
            .and_then(|t| t.get_mut("rules"))
            .and_then(Value::as_array_mut)
        {
            rules.retain(|rule| {
                rule["blocks"]
                    .as_str()
                    .and_then(|b| b.strip_prefix('#'))
                    .is_none_or(|tag| {
                        let (namespace, path) = tag.split_once(':').unwrap_or(("minecraft", tag));
                        bundle.contains(&format!("data/{namespace}/tags/block/{path}.json"))
                    })
            });
        }
        prototype.insert("minecraft:item_model".into(), json!(item.key()));
        prototype.insert(
            "minecraft:item_name".into(),
            json!({"translate": format!("item.minecraft.{}", item.id)}),
        );
        prototypes.insert(item.key(), Value::Object(prototype));
        lang.insert(format!("item.minecraft.{}", item.id), item.name());
    }
    Ok(())
}

/// A flat 16x16 icon for an item no open input draws yet: a rounded tile in
/// a colour of its own, with a darker rim and a light corner.
fn placeholder_icon(id: &str) -> Rgba {
    let hash = id.bytes().fold(2166136261u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16777619)
    });
    let hue = (hash % 360) as f32;
    // HSV with fixed saturation and value, to RGB.
    let (s, v) = (0.55f32, 0.85f32);
    let c = v * s;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (hue / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let base = [r + m, g + m, b + m].map(|f| (f * 255.0) as u8);
    let shade = |k: f32| [base[0], base[1], base[2]].map(|v| (f32::from(v) * k) as u8);
    let (rim, light) = (shade(0.55), shade(1.2));
    let mut image = Rgba::new(16, 16);
    for y in 3..13u32 {
        for x in 3..13u32 {
            // Cut the corners for a rounded tile.
            let corner = (x == 3 || x == 12) && (y == 3 || y == 12);
            if corner {
                continue;
            }
            let edge = x == 3 || x == 12 || y == 3 || y == 12;
            let lit = (x == 5 || y == 5) && x < 8 && y < 8 && x >= 5 && y >= 5;
            let rgb = if edge {
                rim
            } else if lit {
                light
            } else {
                base
            };
            image.set(x, y, [rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    image
}

#[cfg(test)]
mod tests {
    use crate::{Inputs, Origin, build};
    use serde_json::Value;
    use std::path::Path;

    #[test]
    fn the_pack_textures_are_tinted_as_minecraft_tints_them() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bundle = build(&Inputs::at(&repo)).unwrap();
        let json = |path: &str| -> Value {
            serde_json::from_slice(bundle.get(path).unwrap_or_else(|| panic!("{path}"))).unwrap()
        };
        let input = |path: &str| match &bundle.files[path].1 {
            Origin::Input(input) => input.clone(),
            Origin::Generator(g) => format!("generator {g}"),
        };
        let grass = json("assets/minecraft/models/block/grass_block.json");
        assert_eq!(grass["elements"][0]["faces"]["up"]["tintindex"], 0);
        assert!(grass["elements"][0]["faces"]["north"]["tintindex"].is_null());
        assert_eq!(
            grass["elements"][1]["faces"]["north"]["texture"],
            "#overlay"
        );
        assert_eq!(grass["elements"][1]["faces"]["north"]["tintindex"], 0);
        let overlay = "assets/minecraft/textures/block/grass_block_side_overlay.png";
        assert!(input(overlay).starts_with("packs/"));
        // The pack's grass side is whole: not composited over dirt.
        let side = "assets/minecraft/textures/block/grass_block_side.png";
        assert!(input(side).starts_with("packs/"));
        let leaves = json("assets/minecraft/models/block/oak_leaves.json");
        for face in ["down", "up", "north", "south", "west", "east"] {
            assert_eq!(leaves["elements"][0]["faces"][face]["tintindex"], 0);
        }
        let fern = json("assets/minecraft/models/block/fern.json");
        assert_eq!(fern["elements"][1]["faces"]["east"]["tintindex"], 0);
        assert_eq!(
            json("assets/minecraft/items/fern.json")["model"]["tints"][0]["type"],
            "minecraft:grass"
        );
        assert_eq!(
            json("assets/minecraft/items/oak_leaves.json")["model"]["tints"][0]["type"],
            "minecraft:constant"
        );
        // Untinted blocks stay untinted; water keeps Minetest Game's art.
        let stone = json("assets/minecraft/models/block/stone.json");
        assert!(stone["elements"][0]["faces"]["up"]["tintindex"].is_null());
        assert!(input("assets/minecraft/textures/block/water_still.png").starts_with("media/"));
        // The biome reads its grass colour from the colormap.
        let plains = json("data/minecraft/worldgen/biome/plains.json");
        assert!(plains["effects"]["grass_color"].is_null());
    }
}
