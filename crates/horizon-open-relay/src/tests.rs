//! Translation tests on the invented fixture registries (`tables::fixture`).
use crate::{
    tables::fixture::tables,
    translate::{Clientbound, Phase, Serverbound, body},
};
use horizon_protocol::{
    buf::{Reader, Writer},
    chunk::{ChunkData, LightData, Paletted, Section, Strategy},
    ids::{configuration::clientbound as cfg_cb, play::clientbound as cb},
    item::{Component, Hashed, Stack},
    packets::{
        configuration::{self as cfg, RegistryData, RegistryEntry},
        play,
    },
};

fn split(body: &[u8]) -> (i32, Vec<u8>) {
    let mut r = Reader::new(body);
    let id = r.varint().unwrap();
    (id, r.rest().to_vec())
}

/// A client translator already in play, with a 1-section dimension and
/// one biome.
fn playing(t: &crate::tables::Tables) -> Clientbound<'_> {
    let mut c = Clientbound::new(t);
    let dim = RegistryData {
        registry: "minecraft:dimension_type".into(),
        entries: vec![RegistryEntry {
            id: "minecraft:tiny".into(),
            data: Some(horizon_world::nbt::Tag::Compound({
                let mut m = horizon_world::nbt::Compound::new();
                m.insert("min_y".into(), horizon_world::nbt::Tag::Int(0));
                m.insert("height".into(), horizon_world::nbt::Tag::Int(16));
                m
            })),
        }],
    };
    let biomes = RegistryData {
        registry: "minecraft:worldgen/biome".into(),
        entries: vec![RegistryEntry {
            id: "minecraft:plains".into(),
            data: None,
        }],
    };
    for p in [dim, biomes] {
        let (id, payload) = split(&cfg::Clientbound::RegistryData(p).encode());
        assert!(c.translate(id, &payload).is_some());
    }
    assert!(c.translate(cfg_cb::FINISH_CONFIGURATION, &[]).is_some());
    assert_eq!(c.phase, Phase::Play);
    let login = play::Clientbound::Login(play::Login {
        player_id: 1,
        hardcore: false,
        levels: vec!["minecraft:overworld".into()],
        max_players: 10,
        chunk_radius: 4,
        simulation_distance: 4,
        reduced_debug_info: false,
        show_death_screen: true,
        limited_crafting: false,
        spawn: play::SpawnInfo {
            dimension_type: 0,
            dimension: "minecraft:overworld".into(),
            seed: 0,
            game_type: 0,
            previous_game_type: None,
            debug: false,
            flat: false,
            last_death: None,
            portal_cooldown: 0,
            sea_level: 63,
        },
        online_mode: false,
        enforces_secure_chat: false,
    });
    let (id, payload) = split(&login.encode());
    assert!(c.translate(id, &payload).is_some());
    c
}

fn decode(out: Option<Vec<u8>>) -> play::Clientbound {
    let (id, payload) = split(&out.expect("sent"));
    play::Clientbound::decode(id, &payload).unwrap()
}

fn send(c: &mut Clientbound, p: play::Clientbound) -> Option<Vec<u8>> {
    let (id, payload) = split(&p.encode());
    c.translate(id, &payload)
}

#[test]
fn brand_is_the_open_brand_and_tags_are_dropped() {
    let t = tables();
    let mut c = Clientbound::new(&t);
    let mut w = Writer::new();
    w.string("Paper");
    let (id, payload) = split(
        &cfg::Clientbound::CustomPayload(cfg::CustomPayload {
            channel: "minecraft:brand".into(),
            data: w.data,
        })
        .encode(),
    );
    let (id, payload) = split(&c.translate(id, &payload).unwrap());
    let cfg::Clientbound::CustomPayload(p) = cfg::Clientbound::decode(id, &payload).unwrap() else {
        panic!()
    };
    assert_eq!(Reader::new(&p.data).string(64).unwrap(), t.brand);
    assert!(t.brand.starts_with("horizon-open/"));
    let tags = cfg::Clientbound::UpdateTags(cfg::UpdateTags { registries: vec![] });
    let (id, payload) = split(&tags.encode());
    assert!(c.translate(id, &payload).is_none());
}

#[test]
fn chunk_sections_are_remapped_and_counted() {
    let t = tables();
    let mut c = playing(&t);
    // Vanilla: stone, dark oak logs on z, a torch and lava, the rest air.
    let mut values = vec![0i32; 4096];
    values[0] = 1;
    values[1] = 8;
    values[2] = 9;
    values[3] = 14;
    values[4] = 13;
    let section = Section {
        non_empty: 4,
        fluids: 1,
        states: Paletted::from_values(&values, Strategy::Blocks, t.vanilla_state_bits),
        biomes: Paletted::from_values(&[0; 64], Strategy::Biomes, 0),
    };
    let chunk = play::LevelChunkWithLight {
        x: 0,
        z: 0,
        chunk: ChunkData {
            heightmaps: vec![],
            buffer: ChunkData::from_sections(&[section]),
            block_entities: vec![],
        },
        light: LightData {
            sky_mask: vec![],
            block_mask: vec![],
            empty_sky_mask: vec![],
            empty_block_mask: vec![],
            sky: vec![],
            block: vec![],
        },
    };
    let play::Clientbound::LevelChunkWithLight(out) =
        decode(send(&mut c, play::Clientbound::LevelChunkWithLight(chunk)))
    else {
        panic!()
    };
    let s = &out.chunk.sections(1, t.open_state_bits, 0).unwrap()[0];
    assert_eq!(
        (0..5).map(|i| s.states.get(i)).collect::<Vec<_>>(),
        vec![1, 6, 0, 8, 7],
        "stone, spruce log z, torch → air, lava → orange wool, water"
    );
    assert_eq!((s.non_empty, s.fluids), (4, 1));
}

#[test]
fn global_palette_sections_are_repacked() {
    let t = tables();
    let c = playing(&t);
    let values: Vec<i32> = (0..4096).map(|i| i % 16).collect();
    let section = Section {
        non_empty: 0,
        fluids: 0,
        states: Paletted {
            bits: 16,
            palette: vec![],
            data: Paletted::from_values(&values, Strategy::Blocks, 16).data,
        },
        biomes: Paletted::from_values(&[0; 64], Strategy::Biomes, 0),
    };
    // from_values of 16 values chooses a palette; rebuild a global one.
    let global = Paletted {
        bits: 16,
        palette: vec![],
        data: {
            let per = 4;
            values
                .chunks(per)
                .map(|w| {
                    w.iter()
                        .enumerate()
                        .fold(0u64, |acc, (k, v)| acc | (*v as u64) << (k * 16))
                        as i64
                })
                .collect()
        },
    };
    let out = c.section(Section {
        states: global,
        ..section
    });
    for i in 0..16 {
        assert_eq!(out.states.get(i), t.state(i as i32));
    }
}

#[test]
fn block_updates_are_remapped() {
    let t = tables();
    let mut c = playing(&t);
    let play::Clientbound::BlockUpdate(p) = decode(send(
        &mut c,
        play::Clientbound::BlockUpdate(play::BlockUpdate {
            pos: [1, 2, 3],
            state: 7,
        }),
    )) else {
        panic!()
    };
    assert_eq!(p.state, 5);
    let change = (8i64 << 12) | 0x123;
    let play::Clientbound::SectionBlocksUpdate(p) = decode(send(
        &mut c,
        play::Clientbound::SectionBlocksUpdate(play::SectionBlocksUpdate {
            section: [0, 0, 0],
            changes: vec![change],
        }),
    )) else {
        panic!()
    };
    assert_eq!(p.changes, vec![(6i64 << 12) | 0x123]);
}

fn add_entity(id: i32, kind: i32, data: i32) -> play::Clientbound {
    play::Clientbound::AddEntity(play::AddEntity {
        id,
        uuid: id as u128,
        kind,
        position: [0.0; 3],
        velocity: [0.0; 3],
        pitch: 0,
        yaw: 0,
        head_yaw: 0,
        data,
    })
}

#[test]
fn creatures_the_bundle_lacks_stay_invisible() {
    let t = tables();
    let mut c = playing(&t);
    // A zombie (vanilla 0): never spawned, and its movement is dropped.
    assert!(send(&mut c, add_entity(5, 0, 0)).is_none());
    let mut w = Writer::new();
    w.varint(5);
    w.bytes(&[0; 7]);
    assert!(c.translate(cb::MOVE_ENTITY_POS, &w.data).is_none());
    let mut w = Writer::new();
    w.i32(5);
    w.i8(3);
    assert!(c.translate(cb::ENTITY_EVENT, &w.data).is_none());
    // A player (vanilla 3 → open 0) and a falling block whose state maps.
    let play::Clientbound::AddEntity(p) = decode(send(&mut c, add_entity(6, 3, 0))) else {
        panic!()
    };
    assert_eq!(p.kind, 0);
    let play::Clientbound::AddEntity(p) = decode(send(&mut c, add_entity(7, 2, 7))) else {
        panic!()
    };
    assert_eq!((p.kind, p.data), (3, 5));
    let mut w = Writer::new();
    w.varint(6);
    w.bytes(&[0; 7]);
    assert!(c.translate(cb::MOVE_ENTITY_POS, &w.data).is_some());
    // Sounds and particles never reach the open client.
    assert!(c.translate(cb::SOUND, &[0; 8]).is_none());
    assert!(c.translate(cb::LEVEL_PARTICLES, &[0; 8]).is_none());
}

#[test]
fn stacks_are_remapped_and_stand_ins_named() {
    let t = tables();
    let c = playing(&t);
    let golden = Stack {
        item: 2,
        count: 3,
        added: vec![
            (1, Component::Int(16)),
            (2, Component::Ident("minecraft:golden_apple".into())),
        ],
        removed: vec![3],
    };
    let out = c.stack(Some(golden)).unwrap();
    assert_eq!((out.item, out.count), (4, 3));
    // max_stack_size kept; item_model and the food removal dropped; the
    // fixture bundle has no item_name component, so no label.
    assert_eq!(out.added, vec![(1, Component::Int(16))]);
    assert!(out.removed.is_empty());
    assert!(c.stack(Some(Stack::new(0, 1))).is_none(), "air is empty");
}

#[test]
fn clicks_and_creative_slots_go_back_to_vanilla_items() {
    let t = tables();
    let mut s = Serverbound::new(&t);
    assert!(s.translate(3, &[]).is_some(), "finish configuration");
    assert_eq!(s.phase, Phase::Play);
    let click = play::Serverbound::ContainerClick(play::ContainerClick {
        container: 0,
        state: 1,
        slot: 36,
        button: 0,
        input: 0,
        changed: vec![(
            36,
            Some(Hashed {
                item: 4,
                count: 1,
                added: vec![],
                removed: vec![],
            }),
        )],
        carried: Some(Hashed {
            item: 3,
            count: 1,
            added: vec![],
            removed: vec![],
        }),
    });
    let (id, payload) = split(&click.encode());
    let (id, payload) = split(&s.translate(id, &payload).unwrap());
    let play::Serverbound::ContainerClick(p) = play::Serverbound::decode(id, &payload).unwrap()
    else {
        panic!()
    };
    assert_eq!(p.changed[0].1.as_ref().unwrap().item, 7, "apple");
    assert!(
        p.carried.is_none(),
        "spruce log has no vanilla twin in the fixture"
    );
    let creative = play::Serverbound::SetCreativeModeSlot(play::SetCreativeModeSlot {
        slot: 36,
        item: Some(Stack::new(1, 64)),
    });
    let (id, payload) = split(&creative.encode());
    let (id, payload) = split(&s.translate(id, &payload).unwrap());
    let play::Serverbound::SetCreativeModeSlot(p) =
        play::Serverbound::decode(id, &payload).unwrap()
    else {
        panic!()
    };
    assert_eq!(p.item.unwrap().item, 1);
    // Other packets pass as they came.
    assert_eq!(
        s.translate(sb_chat(), &[1, 2]),
        Some(body(sb_chat(), &[1, 2]))
    );
}

fn sb_chat() -> i32 {
    horizon_protocol::ids::play::serverbound::CHAT
}

#[test]
fn creatures_the_bundle_lacks_show_as_a_stand_in_with_only_shared_data() {
    use horizon_protocol::entity_data::{Value, serializer as s};
    let t = tables();
    let mut c = playing(&t);
    // A parched (vanilla 5) spawns as the bundle's skeleton (open 4).
    let play::Clientbound::AddEntity(p) = decode(send(&mut c, add_entity(9, 5, 0))) else {
        panic!()
    };
    assert_eq!(p.kind, 4);
    // Its health (Living data, 9) passes; its own data (16 and above) doesn't.
    let data = play::Clientbound::SetEntityData(play::SetEntityData {
        id: 9,
        values: vec![
            (9, s::FLOAT, Value::Float(12.0)),
            (16, s::BOOLEAN, Value::Bool(true)),
        ],
    });
    let play::Clientbound::SetEntityData(p) = decode(send(&mut c, data)) else {
        panic!()
    };
    assert_eq!(p.values, vec![(9, s::FLOAT, Value::Float(12.0))]);
    // It moves, as any shown entity does.
    let mut w = Writer::new();
    w.varint(9);
    w.bytes(&[0; 7]);
    assert!(c.translate(cb::MOVE_ENTITY_POS, &w.data).is_some());
}

#[test]
fn attributes_take_the_bundle_numbers() {
    let t = tables();
    let mut c = playing(&t);
    let snapshot = |attribute, base| play::AttributeSnapshot {
        attribute,
        base,
        modifiers: vec![],
    };
    // Vanilla armor (0), below_name_distance (1), oxygen_bonus (2), scale (3).
    let play::Clientbound::UpdateAttributes(p) = decode(send(
        &mut c,
        play::Clientbound::UpdateAttributes(play::UpdateAttributes {
            id: 1,
            values: vec![
                snapshot(0, 4.0),
                snapshot(1, 64.0),
                snapshot(2, 1.0),
                snapshot(3, 1.0),
            ],
        }),
    )) else {
        panic!()
    };
    assert_eq!(
        p.values
            .iter()
            .map(|a| (a.attribute, a.base))
            .collect::<Vec<_>>(),
        vec![(2, 4.0), (1, 1.0), (0, 1.0)],
        "open armor 2, oxygen_bonus 1, scale 0; below_name_distance dropped"
    );
    // A hidden entity's attributes stay dropped.
    assert!(send(&mut c, add_entity(5, 0, 0)).is_none());
    assert!(
        send(
            &mut c,
            play::Clientbound::UpdateAttributes(play::UpdateAttributes {
                id: 5,
                values: vec![snapshot(0, 2.0)],
            }),
        )
        .is_none()
    );
}

/// Every packet id after login, as `phase/direction/name`, with its rule.
fn every_rule() -> Vec<(String, crate::policy::Rule)> {
    use horizon_protocol::{Direction, ids};
    let tables = [
        (
            Phase::Configuration,
            Direction::Clientbound,
            "configuration/clientbound",
            ids::configuration::clientbound::NAMES,
        ),
        (
            Phase::Configuration,
            Direction::Serverbound,
            "configuration/serverbound",
            ids::configuration::serverbound::NAMES,
        ),
        (
            Phase::Play,
            Direction::Clientbound,
            "play/clientbound",
            ids::play::clientbound::NAMES,
        ),
        (
            Phase::Play,
            Direction::Serverbound,
            "play/serverbound",
            ids::play::serverbound::NAMES,
        ),
    ];
    let mut out = Vec::new();
    for (phase, direction, prefix, names) in tables {
        for (id, name) in names.iter().enumerate() {
            let rule = crate::policy::policy(phase, direction, id as i32);
            out.push((format!("{prefix}/{name}"), rule));
        }
    }
    out
}

fn listed(text: &str) -> Vec<String> {
    let lines: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect();
    let mut sorted = lines.clone();
    sorted.sort();
    assert_eq!(lines, sorted, "keep the list sorted");
    lines
}

/// The dropped packets are exactly `dropped.txt`, and every built-in registry
/// a forwarded packet carries is renumbered or a known gap (`gaps.txt`): a
/// packet newly passed or dropped, or a new gap, shows up here.
#[test]
fn every_packet_has_a_policy_and_the_lists_match() {
    use crate::policy::{Policy, TRANSLATED};
    let rules = every_rule();
    assert_eq!(
        rules.len(),
        21 + 10 + 144 + 69,
        "protocol 777 packet counts"
    );
    let mut dropped: Vec<String> = rules
        .iter()
        .filter(|(_, r)| r.policy == Policy::Drop)
        .map(|(n, _)| n.clone())
        .collect();
    dropped.sort();
    let mut gaps: Vec<String> = Vec::new();
    for (name, rule) in &rules {
        if rule.policy == Policy::Drop {
            assert!(
                rule.carries.is_empty(),
                "{name} is dropped: it carries nothing"
            );
        }
        for registry in rule.carries {
            let renumbered = rule.policy == Policy::Translate && TRANSLATED.contains(registry);
            if !renumbered {
                gaps.push(format!("{name} {registry}"));
            }
        }
    }
    gaps.sort();
    let show = |v: &[String]| v.join("\n") + "\n";
    assert_eq!(
        show(&dropped),
        show(&listed(include_str!("dropped.txt"))),
        "dropped packets differ from src/dropped.txt; the left side is the new list"
    );
    assert_eq!(
        show(&gaps),
        show(&listed(include_str!("gaps.txt"))),
        "registry gaps differ from src/gaps.txt; the left side is the new list"
    );
}

#[test]
fn clicks_send_component_ids_back() {
    let t = tables();
    let mut s = Serverbound::new(&t);
    assert!(s.translate(3, &[]).is_some(), "finish configuration");
    let click = play::Serverbound::ContainerClick(play::ContainerClick {
        container: 0,
        state: 1,
        slot: 36,
        button: 0,
        input: 0,
        changed: vec![],
        carried: Some(Hashed {
            item: 4,
            count: 1,
            // max_stack_size (1) has a vanilla twin; 7 is no open component.
            added: vec![(1, 123), (7, 456)],
            removed: vec![0, 7],
        }),
    });
    let (id, payload) = split(&click.encode());
    let (id, payload) = split(&s.translate(id, &payload).unwrap());
    let play::Serverbound::ContainerClick(p) = play::Serverbound::decode(id, &payload).unwrap()
    else {
        panic!()
    };
    let carried = p.carried.unwrap();
    assert_eq!((carried.added, carried.removed), (vec![(1, 123)], vec![0]));
}

/// Recipe book displays take the id of the bundle's recipe that draws the
/// same, the rest are left out, and the player's pick goes back by the
/// server's id.
#[test]
fn recipe_displays_take_the_bundle_ids_and_picks_go_back() {
    use horizon_protocol::packets::recipes::{
        PlaceRecipe, RecipeBookAdd, RecipeBookEntry, RecipeBookRemove, RecipeDisplay, SlotDisplay,
    };
    let mut t = tables();
    let recipes = [
        (
            "minecraft:a_stone",
            serde_json::json!({"type": "minecraft:crafting_shapeless",
                "ingredients": ["minecraft:oak_log"], "result": {"id": "minecraft:stone"}}),
        ),
        (
            "minecraft:b_other",
            serde_json::json!({"type": "minecraft:crafting_shapeless",
                "ingredients": ["minecraft:stone"], "result": {"id": "minecraft:oak_log"}}),
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    t.book = crate::recipes::OpenBook::from_recipes(&recipes);
    let displays = crate::recipes::Displays::default();
    let mut c = playing(&t).with_displays(displays.clone());
    let mut s = Serverbound::new(&t).with_displays(displays);
    assert!(s.translate(3, &[]).is_some(), "finish configuration");
    // Vanilla items: 1 stone, 3 oak_log, 4 torch; open: 1 stone, 2 oak_log.
    let entry = |id, ingredient, result| RecipeBookEntry {
        id,
        display: RecipeDisplay::Shapeless {
            ingredients: vec![SlotDisplay::Item(ingredient)],
            result: SlotDisplay::ItemStack(Stack::new(result, 1)),
            station: SlotDisplay::Empty,
        },
        group: 0,
        category: 2,
        requirements: None,
        flags: 0,
    };
    let add = play::Clientbound::RecipeBookAdd(RecipeBookAdd {
        entries: vec![entry(7, 3, 1), entry(8, 4, 1), entry(9, 1, 3)],
        replace: true,
    });
    let play::Clientbound::RecipeBookAdd(out) = decode(send(&mut c, add)) else {
        panic!()
    };
    assert_eq!(out.entries.len(), 2, "the torch recipe is not the bundle's");
    assert_eq!((out.entries[0].id, out.entries[1].id), (0, 1));
    let RecipeDisplay::Shapeless { ingredients, .. } = &out.entries[0].display else {
        panic!()
    };
    assert_eq!(ingredients, &vec![SlotDisplay::Item(2)], "open oak_log");
    let pick = |s: &mut Serverbound, recipe| {
        let (id, payload) = split(
            &play::Serverbound::PlaceRecipe(PlaceRecipe {
                container: 1,
                recipe,
                use_max: false,
            })
            .encode(),
        );
        s.translate(id, &payload).map(|out| {
            let (id, payload) = split(&out);
            match play::Serverbound::decode(id, &payload).unwrap() {
                play::Serverbound::PlaceRecipe(p) => p.recipe,
                _ => panic!(),
            }
        })
    };
    assert_eq!(pick(&mut s, 0), Some(7));
    assert_eq!(pick(&mut s, 1), Some(9));
    assert_eq!(pick(&mut s, 5), None, "never sent");
    let remove = play::Clientbound::RecipeBookRemove(RecipeBookRemove { ids: vec![8, 9] });
    let play::Clientbound::RecipeBookRemove(out) = decode(send(&mut c, remove)) else {
        panic!()
    };
    assert_eq!(out.ids, vec![1]);
    assert_eq!(pick(&mut s, 1), None, "removed");
}

#[test]
fn advancement_icons_take_the_bundle_items() {
    let t = tables();
    let mut c = playing(&t);
    let text = |s: &str| horizon_world::nbt::Tag::String(s.into());
    let advancement = |id: &str, icon: Option<Stack>| play::Advancement {
        id: id.into(),
        parent: None,
        display: icon.map(|icon| play::AdvancementDisplay {
            title: text("Title"),
            description: text("Description"),
            icon,
            frame: 2,
            show_toast: true,
            hidden: false,
            background: Some("minecraft:gui/advancements/backgrounds/stone".into()),
        }),
        requirements: vec![vec!["done".into()]],
        telemetry: false,
        x: 1.0,
        y: 0.5,
    };
    let packet = play::UpdateAdvancements {
        reset: true,
        added: vec![
            // Vanilla's golden apple (2) is the bundle's apple (4) in the fixture.
            advancement("minecraft:story/root", Some(Stack::new(2, 1))),
            advancement("minecraft:recipes/root", None),
        ],
        removed: vec!["minecraft:story/gone".into()],
        progress: vec![(
            "minecraft:story/root".into(),
            vec![("done".into(), Some(7))],
        )],
        show: true,
    };
    let play::Clientbound::UpdateAdvancements(p) = decode(send(
        &mut c,
        play::Clientbound::UpdateAdvancements(packet.clone()),
    )) else {
        panic!("advancements pass")
    };
    let icon = &p.added[0].display.as_ref().unwrap().icon;
    assert_eq!((icon.item, icon.count), (4, 1));
    // Everything but the icon is as the server sent it.
    let mut expected = packet;
    expected.added[0].display.as_mut().unwrap().icon = icon.clone();
    assert_eq!(p, expected);
    // The selected tab carries no number: it passes as it came.
    let mut w = Writer::new();
    w.bool(true);
    w.string("minecraft:story/root");
    assert_eq!(
        c.translate(cb::SELECT_ADVANCEMENTS_TAB, &w.data),
        Some(body(cb::SELECT_ADVANCEMENTS_TAB, &w.data))
    );
}
