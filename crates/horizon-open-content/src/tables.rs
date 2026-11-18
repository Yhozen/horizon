//! The per-state tables the shared loaders read, generated from the block
//! definitions in [`crate::blocks`], in the formats the loaders define.
//!
//! The NBT tables carry `DataVersion` because their readers check it as the
//! table schema; the bundle's own content revision is in its manifest.
use crate::{
    Bundle, Origin, Result,
    blocks::{Block, Form, Layer, Sound, Tier, Tool},
    shapes::{self, StateShape},
};
use horizon_world::{
    DATA_VERSION,
    nbt::{self, JavaString, Limits, NamedTag, Tag},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const GENERATOR: &str = "tables";

/// One registry row: a block and its state's properties.
pub struct State<'b> {
    pub block: &'b Block,
    pub properties: Vec<(&'static str, &'static str)>,
}

pub fn states(blocks: &[Block]) -> Vec<State<'_>> {
    blocks
        .iter()
        .flat_map(|block| {
            block
                .states()
                .into_iter()
                .map(move |properties| State { block, properties })
        })
        .collect()
}

fn description(state: &State) -> String {
    if state.properties.is_empty() {
        format!("Block{{{}}}", state.block.key())
    } else {
        let props: Vec<String> = state
            .properties
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        format!("Block{{{}}}[{}]", state.block.key(), props.join(","))
    }
}

fn state_json(state: &State) -> Value {
    if state.properties.is_empty() {
        json!(state.block.key())
    } else {
        let props: serde_json::Map<String, Value> = state
            .properties
            .iter()
            .map(|(k, v)| ((*k).to_owned(), json!(v)))
            .collect();
        json!({"id": state.block.key(), "properties": props})
    }
}

fn state_nbt(state: &State) -> Tag {
    if state.properties.is_empty() {
        Tag::String(state.block.key().as_str().into())
    } else {
        Tag::compound([
            ("id", Tag::String(state.block.key().as_str().into())),
            (
                "properties",
                Tag::compound(
                    state
                        .properties
                        .iter()
                        .map(|(k, v)| (*k, Tag::String((*v).into()))),
                ),
            ),
        ])
    }
}

/// Whether a state holds a fluid: a fluid block, or a waterlogged one.
fn has_fluid(state: &State) -> bool {
    fluid(state).0 != 0
}

/// The state's fluid as `block-behaviours.json` writes it: the fluid's
/// registry id (1 flowing water, 2 water, 3 flowing lava, 4 lava), its
/// amount and whether it falls. Level 0 is the source, 1 to 7 flow with
/// 8 - level, 8 and up fall; a waterlogged state holds a water source.
fn fluid(state: &State) -> (u8, u8, bool) {
    if state.properties.contains(&("waterlogged", "true")) {
        return (2, 8, false);
    }
    let (flowing, source) = match state.block.fluid() {
        Some("lava") => (3, 4),
        Some(_) => (1, 2),
        None => return (0, 0, false),
    };
    let level: u8 = state
        .properties
        .iter()
        .find(|(k, _)| *k == "level")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    match level {
        0 => (source, 8, false),
        8.. => (flowing, 8, true),
        n => (flowing, 8 - n, false),
    }
}

fn sound(block: &Block) -> Value {
    let name = block.sound.name();
    let id = |event: &str| json!({"id": format!("minecraft:block.{name}.{event}")});
    json!({
        "volume": 1.0, "pitch": 1.0,
        "break": id("break"), "step": id("step"), "place": id("place"), "hit": id("hit"), "fall": id("fall"),
    })
}

/// A `gameplay-facts.json` row without the fields whose value is what the
/// readers (`horizon_client::blocks`, `horizon_world::level::data`) assume
/// when a field is missing: false flags, zero numbers, no item, a model
/// render shape, the push-pull reaction, no sound. A support block's row is
/// then its id, friction, outline and the few facts that differ, a fifth
/// of the full row, and every state has one.
fn without_defaults(row: Value) -> Value {
    let Value::Object(map) = row else {
        return row;
    };
    let keep = |key: &str, value: &Value| match (key, value) {
        ("id" | "friction" | "outline" | "outline_edges", _) => true,
        (_, Value::Null | Value::Bool(false)) => false,
        (_, Value::Number(n)) => n.as_f64() != Some(0.0),
        ("item", v) => v != "minecraft:air",
        ("render_shape", v) => v != "MODEL",
        ("push", v) => v != "PUSH_PULL",
        _ => true,
    };
    Value::Object(map.into_iter().filter(|(k, v)| keep(k, v)).collect())
}

fn map_color(block: &Block) -> u32 {
    block.map_color
}

fn render_shape(block: &Block) -> &'static str {
    match block.form {
        Form::Air | Form::Fluid | Form::Inert => "INVISIBLE",
        _ => "MODEL",
    }
}

pub fn write(bundle: &mut Bundle, blocks: &[Block]) -> Result<()> {
    let states = states(blocks);
    let mut defaults = BTreeMap::new();
    let mut first = 0usize;
    for block in blocks {
        defaults.insert(block.key(), first + block.default_index());
        first += block.state_count();
    }
    if states.len() != first {
        return Err("state count mismatch".into());
    }
    // The shapes of the states whose block is not a plain cube or plant.
    let shaped: Vec<Option<StateShape>> = states
        .iter()
        .map(|s| shapes::of(s.block, &s.properties))
        .collect();

    // facts.json: [name, air, fluid, description, state].
    let rows: Vec<Value> = states
        .iter()
        .map(|s| {
            json!([
                s.block.key(),
                s.block.is_air(),
                has_fluid(s),
                description(s),
                state_json(s)
            ])
        })
        .collect();
    bundle.compact_json(
        "facts.json",
        &json!({"states": rows, "defaults": defaults}),
        GENERATOR,
    )?;

    // gameplay-facts.json: one object per state.
    let gameplay: Vec<Value> = states
        .iter()
        .enumerate()
        .map(|(id, s)| {
            let b = s.block;
            let full = b.collides();
            let mut row = json!({
                "id": id,
                "emission": b.emission(&s.properties),
                "dampening": if b.opaque() { 15 } else { b.opacity },
                "skylight_down": !b.opaque() && b.opacity == 0,
                "shape_occlusion": false,
                "can_occlude": b.opaque(),
                "solid_render": b.opaque(),
                "hardness": b.hardness,
                "requires_tool": b.mining.requires_tool,
                "replaceable": matches!(b.form, Form::Air | Form::Fluid | Form::Inert) || b.id == "short_grass",
                "random_ticks": b.id == "lava",
                "block_entity": b.classes.contains(&"BaseEntityBlock"),
                "liquid": b.form == Form::Fluid,
                "ignited_by_lava": false,
                "push": if b.id == "obsidian" || b.id == "bedrock" { "IMMOVEABLE" } else if matches!(b.form, Form::Cross | Form::Fluid | Form::Air | Form::Inert) { "POPPED" } else { "PUSH_PULL" },
                "render_shape": render_shape(b),
                "full_collision": full,
                "friction": b.friction,
                "explosion_resistance": b.resistance,
                "item": if b.has_item() { b.key() } else { "minecraft:air".to_owned() },
                "sound": if b.sound == Sound::None { Value::Null } else { sound(b) },
                "offset": b.id == "short_grass",
                "outline_offset": b.id == "short_grass",
                "map_color": map_color(b),
                "outline": b.outline(),
            });
            let edges = edges(&b.outline());
            row["outline_edges"] = json!(edges);
            if let Some(sh) = &shaped[id] {
                shaped_facts(&mut row, b, sh);
            }
            without_defaults(row)
        })
        .collect();
    bundle.compact_json(
        "gameplay-facts.json",
        &json!({"states": gameplay}),
        GENERATOR,
    )?;

    // block-behaviours.json.
    let behaviour_blocks: Vec<Value> = blocks
        .iter()
        .map(|b| {
            json!({
                "name": b.key(),
                "class": b.classes,
                "interfaces": [],
                "fields": {},
                "dynamic_shape": false,
                "item": if b.has_item() { b.key() } else { "minecraft:air".to_owned() },
            })
        })
        .collect();
    let behaviour_states: Vec<Value> = states
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let b = s.block;
            if let Some(sh) = &shaped[i] {
                return shaped_behaviour(b, sh);
            }
            let sturdy: u32 = if b.opaque() || b.form == Form::SeeThroughCube {
                (1 << 18) - 1
            } else {
                0
            };
            let mut flags = 0u32;
            if b.opaque() || b.form == Form::SeeThroughCube {
                flags |= 1; // isSolid
            }
            if b.is_air() {
                flags |= 2;
            }
            if b.opaque() {
                flags |= 4 | 64; // conductor, suffocating
            }
            if !b.collides() {
                flags |= 32;
            }
            let (kind, amount, falling) = fluid(s);
            json!([sturdy, flags, kind, amount, falling, 0, 0, false, "HARP"])
        })
        .collect();
    let items: BTreeMap<String, Value> = blocks
        .iter()
        .filter(|b| b.has_item())
        .map(|b| {
            (
                b.key(),
                json!({"class": ["BlockItem"], "fields": {"block": b.key()}}),
            )
        })
        .collect();
    bundle.compact_json(
        "block-behaviours.json",
        &json!({
            "blocks": behaviour_blocks,
            "states": behaviour_states,
            "items": items,
            "fluids": ["minecraft:empty", "minecraft:flowing_water", "minecraft:water", "minecraft:flowing_lava", "minecraft:lava"],
            "fire": {"igniteOdds": {}, "burnOdds": {}},
        }),
        GENERATOR,
    )?;

    // Tags, of the blocks with content (inert support blocks are in none).
    let names = |filter: &dyn Fn(&Block) -> bool| -> Vec<String> {
        blocks
            .iter()
            .filter(|b| b.form != Form::Inert && filter(b))
            .map(Block::key)
            .collect()
    };
    let solid = |b: &Block| b.collides();
    let mut tags: BTreeMap<String, Vec<String>> = BTreeMap::new();
    tags.insert("blocks_motion_in_heightmap".into(), names(&solid));
    tags.insert(
        "blocks_motion_in_heightmap_no_leaves".into(),
        names(&|b: &Block| solid(b) && !b.id.ends_with("_leaves")),
    );
    tags.insert("blocks_fluid_flow".into(), Vec::new());
    for empty in [
        "fences",
        "walls",
        "suppresses_bounce",
        "climbable",
        "fall_damage_resetting",
    ] {
        tags.insert(empty.into(), Vec::new());
    }
    tags.insert(
        "leaves".into(),
        names(&|b: &Block| b.id.ends_with("_leaves")),
    );
    tags.insert("logs".into(), names(&|b: &Block| b.id.ends_with("_log")));
    tags.insert(
        "planks".into(),
        names(&|b: &Block| b.id.ends_with("_planks")),
    );
    tags.insert(
        "saplings".into(),
        names(&|b: &Block| b.id.ends_with("_sapling")),
    );
    // Mining: the fast tool of each block, and the material it needs.
    for tool in Tool::ALL {
        tags.insert(
            format!("mineable/{}", tool.tag()),
            names(&|b: &Block| b.tier == Tier::Palette && b.mining.tool == tool),
        );
    }
    for (level, name) in [(1, "stone"), (2, "iron"), (3, "diamond")] {
        tags.insert(
            format!("needs_{name}_tool"),
            names(&|b: &Block| b.tier == Tier::Palette && b.mining.needs == level),
        );
    }
    for material in crate::items::Material::ALL {
        tags.insert(
            material.incorrect().into(),
            names(&|b: &Block| b.tier == Tier::Palette && b.mining.needs > material.level()),
        );
    }
    tags.insert(
        "sword_efficient".into(),
        names(&|b: &Block| {
            b.tier == Tier::Palette && (b.form == Form::Cross || b.id.ends_with("_leaves"))
        }),
    );
    tags.insert("wool".into(), names(&|b: &Block| b.id.ends_with("_wool")));
    tags.insert(
        "dirt".into(),
        vec!["minecraft:dirt".into(), "minecraft:grass_block".into()],
    );
    tags.insert(
        "replaceable".into(),
        names(&|b: &Block| {
            matches!(b.form, Form::Air | Form::Fluid) && b.id != "bubble_column"
                || b.id == "short_grass"
        }),
    );
    tags.insert(
        "impermeable".into(),
        vec!["minecraft:glass".into(), "minecraft:tinted_glass".into()],
    );
    let bundle_tags: BTreeMap<String, String> = tags
        .iter()
        .map(|(name, values)| {
            (
                format!("minecraft:{name}"),
                serde_json::to_string(&json!({"values": values})).unwrap(),
            )
        })
        .collect();
    bundle.json("block-tags.json", &json!(bundle_tags), GENERATOR)?;
    for (name, values) in &tags {
        bundle.json(
            format!("data/minecraft/tags/block/{name}.json"),
            &json!({"values": values}),
            GENERATOR,
        )?;
    }
    // Fluid tags: the two fluids, and the water sources mobs, plants, boats
    // and bubble columns look for.
    for (name, values) in [
        ("water", &["minecraft:water", "minecraft:flowing_water"][..]),
        ("lava", &["minecraft:lava", "minecraft:flowing_lava"]),
        ("axolotl_tries_to_find", &["minecraft:water"]),
        ("bubble_column_can_occupy", &["minecraft:water"]),
        ("dolphin_tries_to_find", &["minecraft:water"]),
        ("entity_floatable", &["#minecraft:water"]),
        ("frog_tries_to_find_land_near", &["minecraft:water"]),
        ("supports_frogspawn", &["minecraft:water"]),
        ("supports_lily_pad", &["minecraft:water"]),
        ("supports_sugar_cane_adjacently", &["#minecraft:water"]),
    ] {
        bundle.json(
            format!("data/minecraft/tags/fluid/{name}.json"),
            &json!({"values": values}),
            GENERATOR,
        )?;
    }

    // render-definitions.json: how faces against neighbours are skipped.
    let render: BTreeMap<String, Value> = blocks
        .iter()
        .map(|b| {
            let rule = match (b.form, b.id) {
                (Form::Fluid, id) if id != "bubble_column" => "liquid",
                (_, id) if id.ends_with("_leaves") => "leaves",
                (_, "glass" | "tinted_glass") => "same_block",
                _ => "never",
            };
            (
                b.key(),
                json!({"rule": rule, "can_occlude": b.opaque() || b.form == Form::Shaped, "bars_tag": false}),
            )
        })
        .collect();
    bundle.compact_json("render-definitions.json", &json!(render), GENERATOR)?;

    // model-states.json: each block's property schema and state order.
    let model_states: BTreeMap<String, Value> = blocks
        .iter()
        .filter(|b| b.tier != Tier::Support)
        .map(|b| {
            let schema: BTreeMap<&str, Value> = b
                .properties
                .iter()
                .map(|(k, values)| {
                    // As Minecraft's export writes them: integer properties
                    // as their range, the others' values sorted.
                    let numeric: Option<Vec<i64>> = values.iter().map(|v| v.parse().ok()).collect();
                    let value = match numeric {
                        Some(n) => json!({"integer": [n[0], n[n.len() - 1]]}),
                        None => {
                            let mut sorted = values.to_vec();
                            sorted.sort_unstable();
                            json!({"named": sorted})
                        }
                    };
                    (*k, value)
                })
                .collect();
            let states: Vec<BTreeMap<&str, &str>> = b
                .states()
                .into_iter()
                .map(|s| s.into_iter().collect())
                .collect();
            (b.key(), json!({"schema": schema, "states": states}))
        })
        .collect();
    bundle.json("model-states.json", &json!(model_states), GENERATOR)?;

    // Shapes: each state's collision and light occlusion, each shape once.
    let table_states: Vec<Tag> = states.iter().map(state_nbt).collect();
    let (mut collision, mut occlusion) = (shapes::Table::new(), shapes::Table::new());
    let (full, none) = (shapes::full(), shapes::empty());
    for (s, sh) in states.iter().zip(&shaped) {
        let pick = |yes: bool| if yes { &full } else { &none };
        match sh {
            Some(sh) => {
                collision.push(&sh.collision);
                occlusion.push(&sh.occlusion);
            }
            None => {
                collision.push(pick(s.block.collides()));
                occlusion.push(pick(s.block.opaque()));
            }
        }
    }
    nbt_file(
        bundle,
        "cached-collision.nbt",
        collision.tag(table_states.clone()),
    )?;
    nbt_file(bundle, "occlusion.nbt", occlusion.tag(table_states.clone()))?;

    // movement-properties.nbt: one entry per block.
    let movement = Tag::compound([
        ("DataVersion", Tag::Int(DATA_VERSION)),
        ("states", Tag::List(table_states.clone())),
        (
            "blocks",
            Tag::compound(blocks.iter().map(|b| {
                (
                    JavaString::from(b.key().as_str()),
                    Tag::compound([
                        ("friction", Tag::Float(b.friction.to_bits())),
                        ("speedFactor", Tag::Float(1.0f32.to_bits())),
                        ("jumpFactor", Tag::Float(1.0f32.to_bits())),
                        ("bounceRestitution", Tag::Float(0.0f32.to_bits())),
                        ("fallDistanceReduction", Tag::Float(0.0f32.to_bits())),
                        ("fenceGate", Tag::Byte(0)),
                    ]),
                )
            })),
        ),
    ]);
    nbt_file(bundle, "movement-properties.nbt", movement)?;
    let _ = Layer::Solid;
    Ok(())
}

/// The `gameplay-facts.json` fields that follow from a state's shapes.
fn shaped_facts(row: &mut Value, b: &Block, sh: &StateShape) {
    let line = crate::slabs::line(b.id);
    row["dampening"] = json!(sh.dampening());
    row["skylight_down"] = json!(sh.skylight_down());
    row["shape_occlusion"] = json!(sh.shape_occlusion);
    row["can_occlude"] = json!(sh.can_occlude);
    row["solid_render"] = json!(sh.solid_render());
    row["full_collision"] = json!(sh.full_collision());
    row["ignited_by_lava"] = json!(line.is_some_and(|l| l.ignited_by_lava));
    // Copper that can still age ticks randomly.
    row["random_ticks"] = json!(
        b.classes.iter().any(|c| c.starts_with("Weathering")) && !b.id.starts_with("oxidized")
    );
    row["outline"] = json!(shapes::boxes(&sh.outline));
    row["outline_edges"] = json!(shapes::edges(&sh.outline));
}

/// A `block-behaviours.json` state row from a state's shapes: sturdy
/// faces, flags (legacy solid, redstone conductor), its water and its
/// note block instrument.
fn shaped_behaviour(b: &Block, sh: &StateShape) -> Value {
    let mut flags = 0u32;
    if shapes::legacy_solid(&sh.collision) {
        flags |= 1;
    }
    if sh.full_collision() {
        flags |= 4;
    }
    if sh.collision.is_empty() {
        flags |= 32;
    }
    let (kind, amount) = if sh.waterlogged { (2, 8) } else { (0, 0) };
    let instrument = crate::slabs::line(b.id).map_or("HARP", |l| l.instrument);
    json!([
        shapes::sturdy(&sh.collision),
        flags,
        kind,
        amount,
        false,
        0,
        0,
        false,
        instrument
    ])
}

pub fn nbt_file(bundle: &mut Bundle, path: &str, tag: Tag) -> Result<()> {
    let bytes = nbt::write_named(
        &NamedTag {
            name: JavaString::from(""),
            tag,
        },
        Limits::tables(),
    )
    .map_err(|e| format!("{path}: {e}"))?;
    bundle.add(path, bytes, Origin::Generator(GENERATOR))
}

/// The 12 edges of each box, as the outline renderer draws them.
fn edges(boxes: &[[f64; 6]]) -> Vec<[f64; 6]> {
    let mut out = Vec::new();
    for b in boxes {
        let (lo, hi) = ([b[0], b[1], b[2]], [b[3], b[4], b[5]]);
        for axis in 0..3 {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            for (a, c) in [
                (lo[u], lo[v]),
                (hi[u], lo[v]),
                (lo[u], hi[v]),
                (hi[u], hi[v]),
            ] {
                let mut from = [0.0; 3];
                from[u] = a;
                from[v] = c;
                from[axis] = lo[axis];
                let mut to = from;
                to[axis] = hi[axis];
                out.push([from[0], from[1], from[2], to[0], to[1], to[2]]);
            }
        }
    }
    out
}

/// The single biome, the one dimension and the day cycle, authored here.
pub fn environment(bundle: &mut Bundle) -> Result<()> {
    const ENV: &str = "environment";
    // Grass and foliage take their colour from the colormaps (the pack's
    // textures are grey, as Minecraft's are); dry foliage and water are
    // already coloured, so their tint stays white.
    bundle.json(
        "data/minecraft/worldgen/biome/plains.json",
        &json!({
            "temperature": 0.8,
            "downfall": 0.4,
            "has_precipitation": false,
            "effects": {
                "dry_foliage_color": 0xFFFFFF,
                "water_color": 0xFFFFFF,
            },
            "attributes": {
                "minecraft:visual/sky_color": "#7fb0ff",
                "minecraft:visual/fog_color": "#c4dcff",
                "minecraft:visual/water_fog_color": "#1d3b6b",
                "minecraft:gameplay/creature_world_gen_spawn_probability": 0.0,
            },
        }),
        ENV,
    )?;
    bundle.json(
        "data/minecraft/dimension_type/overworld.json",
        &json!({
            "has_skylight": true,
            "has_ceiling": false,
            "natural": true,
            "coordinate_scale": 1.0,
            "min_y": -64,
            "height": 384,
            "logical_height": 384,
            "ambient_light": 0.0,
            "attributes": {
                "minecraft:visual/sky_color": "#7fb0ff",
                "minecraft:visual/fog_color": "#c4dcff",
                "minecraft:visual/cloud_color": "#f4f8ffd0",
                "minecraft:visual/fog_start_distance": 0.0,
                "minecraft:visual/fog_end_distance": 1024.0,
            },
        }),
        ENV,
    )?;
    // The sun crosses overhead at tick 6000; nights are dim, not black.
    let day = |value: Value, night: Value| {
        json!([
            {"ticks": 0, "value": value.clone()},
            {"ticks": 11000, "value": value},
            {"ticks": 13500, "value": night.clone()},
            {"ticks": 22500, "value": night},
        ])
    };
    bundle.json(
        "data/minecraft/timeline/day.json",
        &json!({
            "period_ticks": 24000,
            "tracks": {
                "minecraft:visual/sun_angle": {"keyframes": [{"ticks": 0, "value": -90.0}, {"ticks": 24000, "value": 270.0}]},
                "minecraft:visual/moon_angle": {"keyframes": [{"ticks": 0, "value": 90.0}, {"ticks": 24000, "value": 450.0}]},
                "minecraft:visual/star_angle": {"keyframes": [{"ticks": 0, "value": -90.0}, {"ticks": 24000, "value": 270.0}]},
                "minecraft:visual/star_brightness": {"keyframes": day(json!(0.0), json!(0.6))},
                "minecraft:visual/sky_light_factor": {"keyframes": day(json!(1.0), json!(0.3))},
                "minecraft:visual/sky_color": {"modifier": "multiply", "keyframes": day(json!("#ffffff"), json!("#141a33"))},
                "minecraft:visual/fog_color": {"modifier": "multiply", "keyframes": day(json!("#ffffff"), json!("#1a2038"))},
                "minecraft:visual/cloud_color": {"modifier": "multiply", "keyframes": day(json!("#ffffffff"), json!("#303850ff"))},
                "minecraft:gameplay/sky_light_level": {"modifier": "multiply", "keyframes": day(json!(1.0), json!(0.3))},
                "minecraft:visual/sunrise_sunset_color": {"keyframes": [
                    {"ticks": 0, "value": "#00ff9040"}, {"ticks": 600, "value": "#80ff9040"},
                    {"ticks": 1800, "value": "#00ff9040"}, {"ticks": 11600, "value": "#00ff7030"},
                    {"ticks": 12600, "value": "#a0ff7030"}, {"ticks": 13600, "value": "#00ff7030"},
                    {"ticks": 23400, "value": "#00ff9040"}
                ]},
            },
        }),
        ENV,
    )?;
    let white = crate::image::Rgba::filled(256, 256, [255, 255, 255, 255]);
    for map in ["grass", "foliage", "dry_foliage"] {
        let (image, origin) = match crate::art::colormap(map) {
            Some(image) => (image, Origin::Generator("art")),
            None => (white.clone(), Origin::Generator(ENV)),
        };
        bundle.png(
            format!("assets/minecraft/textures/colormap/{map}.png"),
            &image,
            origin,
        )?;
    }
    Ok(())
}

/// The attributes a player has: the movement, reach and health values the
/// shared player code reads, chosen for this edition.
pub const PLAYER_ATTRIBUTES: &[(&str, f64, Option<(f64, f64)>, bool)] = &[
    ("air_drag_modifier", 1.0, Some((0.0, 2048.0)), true),
    ("armor", 0.0, Some((0.0, 30.0)), true),
    ("armor_toughness", 0.0, Some((0.0, 20.0)), true),
    ("attack_damage", 1.0, Some((0.0, 2048.0)), false),
    ("attack_knockback", 0.0, Some((0.0, 5.0)), false),
    ("attack_speed", 4.0, Some((0.0, 1024.0)), true),
    ("block_break_speed", 1.0, Some((0.0, 1024.0)), true),
    ("bounciness", 0.0, Some((0.0, 1.0)), true),
    ("block_interaction_range", 4.5, Some((0.0, 64.0)), true),
    ("burning_time", 1.0, Some((0.0, 1024.0)), true),
    ("camera_distance", 4.0, Some((0.0, 32.0)), true),
    ("entity_interaction_range", 3.0, Some((0.0, 64.0)), true),
    (
        "explosion_knockback_resistance",
        0.0,
        Some((0.0, 1.0)),
        true,
    ),
    ("fall_damage_multiplier", 1.0, Some((0.0, 100.0)), true),
    ("flying_speed", 0.4, Some((0.0, 1024.0)), true),
    ("follow_range", 32.0, Some((0.0, 2048.0)), false),
    ("friction_modifier", 1.0, Some((0.0, 2048.0)), true),
    ("gravity", 0.08, Some((-1.0, 1.0)), true),
    ("jump_strength", 0.42, Some((0.0, 32.0)), true),
    ("knockback_resistance", 0.0, Some((0.0, 1.0)), false),
    ("luck", 0.0, Some((-1024.0, 1024.0)), true),
    ("max_absorption", 0.0, Some((0.0, 2048.0)), true),
    ("max_health", 20.0, Some((1.0, 1024.0)), true),
    ("mining_efficiency", 0.0, Some((0.0, 1024.0)), true),
    ("movement_efficiency", 0.0, Some((0.0, 1.0)), true),
    ("movement_speed", 0.1, Some((0.0, 1024.0)), true),
    ("oxygen_bonus", 0.0, Some((0.0, 1024.0)), true),
    ("safe_fall_distance", 3.0, Some((-1024.0, 1024.0)), true),
    ("scale", 1.0, Some((0.0625, 16.0)), true),
    ("sneaking_speed", 0.3, Some((0.0, 1.0)), true),
    ("step_height", 0.6, Some((0.0, 10.0)), true),
    ("submerged_mining_speed", 0.2, Some((0.0, 20.0)), true),
    ("sweeping_damage_ratio", 0.0, Some((0.0, 1.0)), true),
    ("water_movement_efficiency", 0.0, Some((0.0, 1.0)), true),
    ("waypoint_receive_range", 0.0, Some((0.0, 6.0e7)), false),
    ("waypoint_transmit_range", 0.0, Some((0.0, 6.0e7)), false),
    // Name tag distances: a player's name shows within 64 blocks, its
    // scoreboard line below the name within 10.
    ("below_name_distance", 10.0, Some((0.0, 256.0)), true),
    ("name_tag_distance", 64.0, Some((0.0, 256.0)), true),
];

/// Attributes only mobs have; the player's own table leaves them out.
pub const MOB_ATTRIBUTES: &[(&str, f64, Option<(f64, f64)>, bool)] = &[
    ("spawn_reinforcements", 0.0, Some((0.0, 1.0)), false),
    ("tempt_range", 10.0, Some((0.0, 2048.0)), false),
];

fn bits(v: f64) -> String {
    format!("{:016x}", v.to_bits())
}

/// `attributes.json`: every attribute in Minecraft's registry order (the
/// index is the network id, as `Catalog::from_json` reads it), and the
/// player's values.
pub fn attributes(bundle: &mut Bundle) -> Result<()> {
    let all: BTreeMap<&str, _> = PLAYER_ATTRIBUTES
        .iter()
        .chain(MOB_ATTRIBUTES)
        .map(|a| (a.0, a))
        .collect();
    let list: Vec<Value> = crate::numbering::names("attribute")
        .iter()
        .map(|name| {
            let (id, default, range, sync) = all
                .get(name)
                .ok_or_else(|| format!("attributes: no values for {name}"))?;
            let mut a = json!({"id": format!("minecraft:{id}"), "default": bits(*default), "syncable": sync});
            if let Some((min, max)) = range {
                a["min"] = json!(bits(*min));
                a["max"] = json!(bits(*max));
            }
            Ok(a)
        })
        .collect::<Result<_>>()?;
    if list.len() != all.len() {
        return Err("attributes: a value set names no Minecraft attribute".into());
    }
    let player: BTreeMap<String, String> = PLAYER_ATTRIBUTES
        .iter()
        .map(|(id, default, _, _)| (format!("minecraft:{id}"), bits(*default)))
        .collect();
    bundle.json(
        "attributes.json",
        &json!({"data_version": DATA_VERSION, "attributes": list, "entities": {"minecraft:player": player}}),
        GENERATOR,
    )
}

/// Damage types the shared player code names. Creative players take none,
/// but the health and landing code resolves these at startup.
const DAMAGE_TYPES: &[(&str, &str, &str)] = &[
    ("cactus", "cactus", "poking"),
    ("cramming", "cramming", "hurt"),
    ("drown", "drown", "drowning"),
    ("dry_out", "dryout", "hurt"),
    ("explosion", "explosion", "hurt"),
    ("fall", "fall", "hurt"),
    ("falling_block", "fallingBlock", "hurt"),
    ("fly_into_wall", "flyIntoWall", "hurt"),
    ("freeze", "freeze", "freezing"),
    ("generic", "generic", "hurt"),
    ("generic_kill", "genericKill", "hurt"),
    ("hot_floor", "hotFloor", "burning"),
    ("in_fire", "inFire", "burning"),
    ("in_wall", "inWall", "hurt"),
    ("lava", "lava", "burning"),
    ("magic", "magic", "hurt"),
    ("mob_attack", "mob", "hurt"),
    ("on_fire", "onFire", "burning"),
    ("out_of_world", "outOfWorld", "hurt"),
    ("player_attack", "player", "hurt"),
    ("stalagmite", "stalagmite", "hurt"),
    ("starve", "starve", "hurt"),
    ("sweet_berry_bush", "sweetBerryBush", "poking"),
];

pub fn health(bundle: &mut Bundle, blocks: &[Block]) -> Result<()> {
    let types: Vec<Value> = DAMAGE_TYPES
        .iter()
        .map(|(id, message, effects)| {
            json!({
                "id": format!("minecraft:{id}"),
                "message_id": message,
                "exhaustion": format!("{:08x}", 0.0f32.to_bits()),
                "scaling": "never",
                "effects": effects,
                "death_message_type": if *id == "fall" { "fall_variants" } else { "default" },
            })
        })
        .collect();
    bundle.json(
        "damage-catalog.json",
        &json!({"data_version": DATA_VERSION, "types": types}),
        GENERATOR,
    )?;
    let tag = |values: &[&str]| -> String {
        serde_json::to_string(
            &json!({"values": values.iter().map(|v| format!("minecraft:{v}")).collect::<Vec<_>>()}),
        )
        .unwrap()
    };
    let tags = json!({
        "minecraft:bypasses_invulnerability": tag(&["out_of_world", "generic_kill"]),
        "minecraft:bypasses_armor": tag(&["out_of_world", "generic_kill", "drown", "starve", "in_wall", "cramming", "fall", "fly_into_wall", "freeze", "magic"]),
        "minecraft:is_fall": tag(&["fall", "stalagmite"]),
        "minecraft:is_fire": tag(&["in_fire", "on_fire", "lava", "hot_floor"]),
        "minecraft:is_drowning": tag(&["drown"]),
        "minecraft:is_freezing": tag(&["freeze"]),
        "minecraft:no_knockback": tag(&["out_of_world", "generic_kill", "drown", "starve", "in_wall", "fall", "freeze", "magic"]),
    });
    bundle.json("damage-tags.json", &tags, GENERATOR)?;
    // The raw resources the catalog was decoded from, as data packs hold them.
    let raw_types: BTreeMap<String, String> = DAMAGE_TYPES
        .iter()
        .map(|(id, message, effects)| {
            (
                format!("minecraft:{id}"),
                serde_json::to_string(&json!({"message_id": message, "exhaustion": 0.0, "scaling": "never", "effects": effects})).unwrap(),
            )
        })
        .collect();
    bundle.json("damage-types.json", &json!(raw_types), GENERATOR)?;
    bundle.json(
        "player-float-ids.json",
        &json!({"health": 9, "absorption": 15}),
        GENERATOR,
    )?;
    let landing: BTreeMap<String, Value> = blocks
        .iter()
        .map(|b| (b.key(), json!({"fall": "Block", "bounce": false})))
        .collect();
    bundle.compact_json("landing-definitions.json", &json!(landing), GENERATOR)?;
    // What each block drops in survival.
    for b in blocks {
        if let Some(table) = crate::items::loot_table(b) {
            bundle.json(
                format!("data/minecraft/loot_table/blocks/{}.json", b.id),
                &table,
                GENERATOR,
            )?;
        }
    }
    Ok(())
}
