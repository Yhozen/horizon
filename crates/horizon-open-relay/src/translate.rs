//! Packet translation between a vanilla server and an open player, one
//! direction each. Every clientbound packet is either passed as it came
//! (only packets with no built-in registry ids in them), rewritten into the
//! open numbering, or dropped: nothing the open client would misread reaches
//! it. Serverbound packets carry few ids (item stacks); the rest pass.
//!
//! What changes (see docs/open-creative.md, "Joining a Java server through
//! the relay"): block states in chunks and block updates, items everywhere a
//! stack travels, entity types (a creature the bundle lacks shows as its
//! stand-in, `rules::stand_in`, or is never spawned, its later packets dropped
//! too), menu types, attributes, and the server brand.
use crate::rules;
use crate::tables::Tables;
use horizon_protocol::{
    buf::{Reader, Writer},
    chunk::{ChunkData, Paletted, Section, Strategy},
    entity_data::{Value, serializer},
    ids::{configuration::clientbound as cfg_cb, play::clientbound as cb, play::serverbound as sb},
    item::{Component, Hashed, Stack, component},
    packets::{configuration as cfg, play},
    registry::{DimensionType, Dynamic, ceil_log2},
};
use horizon_world::nbt::{JavaString, Tag};
use std::collections::{BTreeMap, HashSet};

/// The connection phase a direction is in after login.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Configuration,
    Play,
}

/// A packet body: VarInt id, then the payload.
pub fn body(id: i32, payload: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(id);
    w.bytes(payload);
    w.data
}

/// How many packets of each kind were dropped or failed, for the debug log.
#[derive(Default, Debug)]
pub struct Counts {
    pub dropped: BTreeMap<(Phase, i32), u64>,
    pub failed: BTreeMap<(Phase, i32), u64>,
}
impl Phase {
    fn key(self) -> u8 {
        match self {
            Self::Configuration => 0,
            Self::Play => 1,
        }
    }
}
impl PartialOrd for Phase {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Phase {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key().cmp(&other.key())
    }
}

/// Server → open player.
pub struct Clientbound<'t> {
    t: &'t Tables,
    pub phase: Phase,
    registries: Dynamic,
    dimension: DimensionType,
    biome_bits: u8,
    /// Entities the open client never got (types it lacks, with no stand-in).
    hidden: HashSet<i32>,
    /// Entities shown as a stand-in (`rules::stand_in`): only their shared data passes.
    standing_in: HashSet<i32>,
    pub counts: Counts,
}

/// Clientbound play packets passed unchanged: none of them holds a built-in
/// registry id (dynamic registry ids are fine: the server's own registries
/// reach the client).
const PLAY_PASS: &[i32] = &[
    cb::BUNDLE_DELIMITER,
    cb::BLOCK_CHANGED_ACK,
    cb::BLOCK_DESTRUCTION,
    cb::CHANGE_DIFFICULTY,
    cb::CHUNK_BATCH_FINISHED,
    cb::CHUNK_BATCH_START,
    cb::CHUNKS_BIOMES,
    cb::CLEAR_TITLES,
    cb::COMMAND_SUGGESTIONS,
    cb::CONTAINER_CLOSE,
    cb::CONTAINER_SET_DATA,
    cb::DISCONNECT,
    cb::DISGUISED_CHAT,
    cb::FORGET_LEVEL_CHUNK,
    cb::GAME_EVENT,
    cb::GAME_RULE_VALUES,
    cb::INITIALIZE_BORDER,
    cb::KEEP_ALIVE,
    cb::LIGHT_UPDATE,
    cb::OPEN_BOOK,
    cb::OPEN_SIGN_EDITOR,
    cb::PING,
    cb::PONG_RESPONSE,
    cb::PLAYER_ABILITIES,
    cb::PLAYER_CHAT,
    cb::PLAYER_COMBAT_KILL,
    cb::PLAYER_INFO_REMOVE,
    cb::PLAYER_INFO_UPDATE,
    cb::PLAYER_POSITION,
    cb::PLAYER_ROTATION,
    cb::RESET_SCORE,
    cb::RESOURCE_PACK_PUSH,
    cb::SERVER_DATA,
    cb::SET_ACTION_BAR_TEXT,
    cb::SET_BORDER_CENTER,
    cb::SET_BORDER_LERP_SIZE,
    cb::SET_BORDER_SIZE,
    cb::SET_BORDER_WARNING_DELAY,
    cb::SET_BORDER_WARNING_DISTANCE,
    cb::SET_CHUNK_CACHE_CENTER,
    cb::SET_CHUNK_CACHE_RADIUS,
    cb::SET_DEFAULT_SPAWN_POSITION,
    cb::SET_DISPLAY_OBJECTIVE,
    cb::SET_EXPERIENCE,
    cb::SET_HEALTH,
    cb::SET_HELD_SLOT,
    cb::SET_OBJECTIVE,
    cb::MOVE_VEHICLE,
    cb::SET_PLAYER_TEAM,
    cb::SET_SCORE,
    cb::SET_SIMULATION_DISTANCE,
    cb::SET_SUBTITLE_TEXT,
    cb::SET_TIME,
    cb::SET_TITLE_TEXT,
    cb::SET_TITLES_ANIMATION,
    cb::SYSTEM_CHAT,
    cb::TAB_LIST,
    cb::TICKING_STATE,
    cb::TICKING_STEP,
];

/// Play packets about one entity whose id is their first VarInt: dropped when
/// that entity is hidden.
const ENTITY_FIRST_VARINT: &[i32] = &[
    cb::ANIMATE,
    cb::ENTITY_POSITION_SYNC,
    cb::HURT_ANIMATION,
    cb::DAMAGE_EVENT,
    cb::MOVE_ENTITY_POS,
    cb::MOVE_ENTITY_POS_ROT,
    cb::MOVE_ENTITY_ROT,
    cb::ROTATE_HEAD,
    cb::SET_ENTITY_MOTION,
    cb::TAKE_ITEM_ENTITY,
    cb::TELEPORT_ENTITY,
    cb::PROJECTILE_POWER,
    cb::SWING_ANIMATION,
    cb::SET_PASSENGERS,
];
/// The same, with the entity id as a 32-bit Int.
const ENTITY_FIRST_INT: &[i32] = &[cb::ENTITY_EVENT, cb::SET_ENTITY_LINK];

/// Configuration packets passed unchanged.
const CONFIG_PASS: &[i32] = &[
    cfg_cb::COOKIE_REQUEST,
    cfg_cb::DISCONNECT,
    cfg_cb::KEEP_ALIVE,
    cfg_cb::PING,
    cfg_cb::RESET_CHAT,
    cfg_cb::RESOURCE_PACK_POP,
    cfg_cb::RESOURCE_PACK_PUSH,
    cfg_cb::UPDATE_ENABLED_FEATURES,
    cfg_cb::SELECT_KNOWN_PACKS,
    cfg_cb::CLEAR_DIALOG,
    cfg_cb::CODE_OF_CONDUCT,
];

/// LevelEvent 2001: a block broke (particles and sound); its data is a state.
const PARTICLES_AND_SOUND_FOR_BLOCK_DESTROYED: i32 = 2001;

impl<'t> Clientbound<'t> {
    pub fn new(t: &'t Tables) -> Self {
        Self {
            t,
            phase: Phase::Configuration,
            registries: Dynamic::default(),
            dimension: DimensionType::vanilla("minecraft:overworld"),
            biome_bits: 0,
            hidden: HashSet::new(),
            standing_in: HashSet::new(),
            counts: Counts::default(),
        }
    }

    /// The body to send the open player for this server packet, if any.
    pub fn translate(&mut self, id: i32, payload: &[u8]) -> Option<Vec<u8>> {
        let phase = self.phase;
        let out = match phase {
            Phase::Configuration => self.configuration(id, payload),
            Phase::Play => self.play(id, payload),
        };
        match &out {
            Ok(None) => *self.counts.dropped.entry((phase, id)).or_default() += 1,
            Err(_) => *self.counts.failed.entry((phase, id)).or_default() += 1,
            Ok(Some(_)) => {}
        }
        out.ok().flatten()
    }

    fn configuration(&mut self, id: i32, payload: &[u8]) -> Result<Option<Vec<u8>>, String> {
        let e = |e: horizon_protocol::Error| e.to_string();
        Ok(match id {
            cfg_cb::CUSTOM_PAYLOAD => match cfg::Clientbound::decode(id, payload).map_err(e)? {
                cfg::Clientbound::CustomPayload(p) if p.channel == "minecraft:brand" => {
                    Some(self.brand_payload(|p| cfg::Clientbound::CustomPayload(p).encode()))
                }
                _ => None,
            },
            cfg_cb::REGISTRY_DATA => {
                if let cfg::Clientbound::RegistryData(r) =
                    cfg::Clientbound::decode(id, payload).map_err(e)?
                {
                    self.registries.add(r);
                }
                Some(body(id, payload))
            }
            cfg_cb::FINISH_CONFIGURATION => {
                self.biome_bits = ceil_log2(self.registries.len("minecraft:worldgen/biome"));
                self.phase = Phase::Play;
                Some(body(id, payload))
            }
            id if CONFIG_PASS.contains(&id) => Some(body(id, payload)),
            // Tags (vanilla ids), transfers to other servers, cookies, report
            // details, server links, dialogs and post effects.
            _ => None,
        })
    }

    fn brand_payload(&self, encode: impl Fn(cfg::CustomPayload) -> Vec<u8>) -> Vec<u8> {
        let mut w = Writer::new();
        w.string(&self.t.brand);
        encode(cfg::CustomPayload {
            channel: "minecraft:brand".into(),
            data: w.data,
        })
    }

    fn play(&mut self, id: i32, payload: &[u8]) -> Result<Option<Vec<u8>>, String> {
        if PLAY_PASS.contains(&id) {
            return Ok(Some(body(id, payload)));
        }
        if ENTITY_FIRST_VARINT.contains(&id) || ENTITY_FIRST_INT.contains(&id) {
            let mut r = Reader::new(payload);
            let entity = if ENTITY_FIRST_INT.contains(&id) {
                r.i32()
            } else {
                r.varint()
            }
            .map_err(|e| e.to_string())?;
            return Ok((!self.hidden.contains(&entity)).then(|| body(id, payload)));
        }
        let typed = || play::Clientbound::decode(id, payload).map_err(|e| e.to_string());
        use play::Clientbound as C;
        Ok(match id {
            cb::LOGIN => {
                if let C::Login(p) = typed()? {
                    self.dimension = self.registries.dimension(p.spawn.dimension_type);
                    self.hidden.clear();
                    self.standing_in.clear();
                }
                Some(body(id, payload))
            }
            cb::RESPAWN => {
                if let C::Respawn(p) = typed()? {
                    self.dimension = self.registries.dimension(p.spawn.dimension_type);
                    self.hidden.clear();
                    self.standing_in.clear();
                }
                Some(body(id, payload))
            }
            cb::START_CONFIGURATION => {
                self.phase = Phase::Configuration;
                self.registries = Dynamic::default();
                Some(body(id, payload))
            }
            cb::CUSTOM_PAYLOAD => match typed()? {
                C::CustomPayload(p) if p.channel == "minecraft:brand" => {
                    Some(self.brand_payload(|p| C::CustomPayload(p).encode()))
                }
                _ => None,
            },
            cb::LEVEL_CHUNK_WITH_LIGHT => match typed()? {
                C::LevelChunkWithLight(mut p) => {
                    p.chunk = self.chunk(&p.chunk)?;
                    Some(C::LevelChunkWithLight(p).encode())
                }
                _ => None,
            },
            cb::BLOCK_UPDATE => match typed()? {
                C::BlockUpdate(mut p) => {
                    p.state = self.t.state(p.state);
                    Some(C::BlockUpdate(p).encode())
                }
                _ => None,
            },
            cb::SECTION_BLOCKS_UPDATE => match typed()? {
                C::SectionBlocksUpdate(mut p) => {
                    for c in &mut p.changes {
                        let state = self.t.state((*c >> 12) as i32);
                        *c = (i64::from(state) << 12) | (*c & 0xFFF);
                    }
                    Some(C::SectionBlocksUpdate(p).encode())
                }
                _ => None,
            },
            cb::LEVEL_EVENT => match typed()? {
                C::LevelEvent(mut p) if p.kind == PARTICLES_AND_SOUND_FOR_BLOCK_DESTROYED => {
                    p.data = self.t.state(p.data);
                    Some(C::LevelEvent(p).encode())
                }
                _ => None,
            },
            cb::ADD_ENTITY => match typed()? {
                C::AddEntity(mut p) => {
                    let falling = Some(p.kind) == self.t.falling_block;
                    let stand_in = self.t.stand_in(p.kind);
                    match self.t.entity_type(p.kind).or(stand_in) {
                        Some(kind) => {
                            self.hidden.remove(&p.id);
                            if self.t.entity_type(p.kind).is_none() {
                                self.standing_in.insert(p.id);
                            } else {
                                self.standing_in.remove(&p.id);
                            }
                            p.kind = kind;
                            if falling {
                                p.data = self.t.state(p.data);
                            }
                            Some(C::AddEntity(p).encode())
                        }
                        None => {
                            self.hidden.insert(p.id);
                            None
                        }
                    }
                }
                _ => None,
            },
            cb::REMOVE_ENTITIES => {
                if let C::RemoveEntities(p) = typed()? {
                    for id in &p.ids {
                        self.hidden.remove(id);
                        self.standing_in.remove(id);
                    }
                }
                Some(body(id, payload))
            }
            cb::SET_ENTITY_DATA => match typed()? {
                C::SetEntityData(mut p) if !self.hidden.contains(&p.id) => {
                    let shared_only = self.standing_in.contains(&p.id);
                    p.values = p
                        .values
                        .into_iter()
                        .filter(|(index, _, _)| !shared_only || *index < rules::ENTITY_DATA_SHARED)
                        .filter_map(|(index, kind, value)| {
                            self.entity_value(kind, value).map(|v| (index, kind, v))
                        })
                        .collect();
                    Some(C::SetEntityData(p).encode())
                }
                _ => None,
            },
            cb::SET_EQUIPMENT => match typed()? {
                C::SetEquipment(mut p) if !self.hidden.contains(&p.id) => {
                    for (_, item) in &mut p.slots {
                        *item = self.stack(item.take());
                    }
                    Some(C::SetEquipment(p).encode())
                }
                _ => None,
            },
            // Attributes by the bundle's own numbers (`attributes.json`); the
            // ones it lacks are dropped, so none is read as another (a vanilla
            // oxygen bonus as the open scale shrank players).
            cb::UPDATE_ATTRIBUTES => match typed()? {
                C::UpdateAttributes(mut p) if !self.hidden.contains(&p.id) => {
                    p.values = p
                        .values
                        .into_iter()
                        .filter_map(|mut a| {
                            a.attribute = self.t.attribute(a.attribute)?;
                            Some(a)
                        })
                        .collect();
                    Some(C::UpdateAttributes(p).encode())
                }
                _ => None,
            },
            cb::CONTAINER_SET_CONTENT => match typed()? {
                C::ContainerSetContent(mut p) => {
                    for item in &mut p.items {
                        *item = self.stack(item.take());
                    }
                    p.carried = self.stack(p.carried.take());
                    Some(C::ContainerSetContent(p).encode())
                }
                _ => None,
            },
            cb::CONTAINER_SET_SLOT => match typed()? {
                C::ContainerSetSlot(mut p) => {
                    p.item = self.stack(p.item.take());
                    Some(C::ContainerSetSlot(p).encode())
                }
                _ => None,
            },
            cb::SET_CURSOR_ITEM => match typed()? {
                C::SetCursorItem(mut p) => {
                    p.item = self.stack(p.item.take());
                    Some(C::SetCursorItem(p).encode())
                }
                _ => None,
            },
            cb::SET_PLAYER_INVENTORY => match typed()? {
                C::SetPlayerInventory(mut p) => {
                    p.item = self.stack(p.item.take());
                    Some(C::SetPlayerInventory(p).encode())
                }
                _ => None,
            },
            cb::OPEN_SCREEN => match typed()? {
                C::OpenScreen(mut p) => self.t.menu(p.menu).map(|menu| {
                    p.menu = menu;
                    C::OpenScreen(p).encode()
                }),
                _ => None,
            },
            cb::COMMANDS if self.t.commands_compatible => Some(body(id, payload)),
            // Everything else names something by a vanilla number the open
            // client would misread (sounds, particles, block entities, mob
            // effects, recipes, advancements, tags, statistics, maps, trades,
            // explosions, waypoints, debug data) or is a packet it does not
            // decode at all.
            _ => None,
        })
    }

    /// A chunk column in open numbering: every section's block states
    /// remapped and repacked (the open global palette is its own width), the
    /// counts recomputed for open air and fluids, biomes kept as sent, and
    /// block entities removed (the bundle has no block entity types).
    fn chunk(&self, data: &ChunkData) -> Result<ChunkData, String> {
        let sections = data
            .sections(
                self.dimension.sections(),
                self.t.vanilla_state_bits,
                self.biome_bits,
            )
            .map_err(|e| e.to_string())?;
        let sections: Vec<Section> = sections.into_iter().map(|s| self.section(s)).collect();
        Ok(ChunkData {
            heightmaps: data.heightmaps.clone(),
            buffer: ChunkData::from_sections(&sections),
            block_entities: Vec::new(),
        })
    }

    pub fn section(&self, mut s: Section) -> Section {
        let mut values = vec![0i32; horizon_protocol::chunk::BLOCKS];
        let (mut non_empty, mut fluids) = (0i16, 0i16);
        if s.states.bits == 0 || !s.states.palette.is_empty() {
            // Paletted: map each palette entry once.
            let palette: Vec<i32> = s.states.palette.iter().map(|&v| self.t.state(v)).collect();
            let single = s.states.bits == 0;
            for (i, v) in values.iter_mut().enumerate() {
                let index = if single {
                    0
                } else {
                    horizon_protocol::chunk::storage_get(&s.states.data, s.states.bits, i) as usize
                };
                *v = palette.get(index).copied().unwrap_or(0);
            }
        } else {
            for (i, v) in values.iter_mut().enumerate() {
                *v = self.t.state(s.states.get(i));
            }
        }
        for &v in &values {
            if !self.t.is_air(v as u32) {
                non_empty += 1;
            }
            if self.t.is_fluid(v as u32) {
                fluids += 1;
            }
        }
        s.states = Paletted::from_values(&values, Strategy::Blocks, self.t.open_state_bits);
        s.non_empty = non_empty;
        s.fluids = fluids;
        s
    }

    /// An entity data value as the open client gets it: stacks and block
    /// states remapped, particles removed.
    fn entity_value(&self, kind: i32, value: Value) -> Option<Value> {
        Some(match value {
            Value::Item(stack) => Value::Item(self.stack(stack)),
            Value::Id(state) if kind == serializer::BLOCK_STATE => Value::Id(self.t.state(state)),
            Value::Id(state) if kind == serializer::OPTIONAL_BLOCK_STATE => {
                Value::Id(if state == 0 {
                    0
                } else {
                    self.t.state(state - 1) + 1
                })
            }
            Value::Particle(_) | Value::Particles(_) => return None,
            v => v,
        })
    }

    /// A vanilla stack in open numbering, or none when it maps to air.
    pub fn stack(&self, stack: Option<Stack>) -> Option<Stack> {
        let s = stack?;
        let target = self.t.item(s.item)?;
        let mut added: Vec<(i32, Component)> = s
            .added
            .into_iter()
            .filter(|(kind, _)| *kind != component::ITEM_MODEL)
            .filter_map(|(kind, c)| {
                let open = self.t.component(kind)?;
                let c = match c {
                    Component::TooltipDisplay { hide, hidden } => Component::TooltipDisplay {
                        hide,
                        hidden: hidden
                            .into_iter()
                            .filter_map(|k| self.t.component(k))
                            .collect(),
                    },
                    // Stacks inside stacks: the bundle names no component
                    // that holds them, so these never reach here remapped
                    // wrong; drop them to be sure.
                    Component::Stacks(_) => return None,
                    c => c,
                };
                Some((open, c))
            })
            .collect();
        let removed = s
            .removed
            .into_iter()
            .filter(|k| *k != component::ITEM_MODEL)
            .filter_map(|k| self.t.component(k))
            .collect();
        let names = &self.t.open_component_names;
        let named = ["minecraft:custom_name", "minecraft:item_name"]
            .iter()
            .filter_map(|n| names.get(*n))
            .any(|id| added.iter().any(|(k, _)| k == id));
        if let (Some(shown), false, Some(&item_name)) =
            (&target.shown_as, named, names.get("minecraft:item_name"))
        {
            added.push((
                item_name,
                Component::Text(Tag::String(JavaString::from(shown.as_str()))),
            ));
        }
        Some(Stack {
            item: target.id,
            count: s.count,
            added,
            removed,
        })
    }
}

/// Open player → server.
pub struct Serverbound<'t> {
    t: &'t Tables,
    pub phase: Phase,
}
impl<'t> Serverbound<'t> {
    pub fn new(t: &'t Tables) -> Self {
        Self {
            t,
            phase: Phase::Configuration,
        }
    }

    pub fn translate(&mut self, id: i32, payload: &[u8]) -> Option<Vec<u8>> {
        match self.phase {
            Phase::Configuration => {
                if id == horizon_protocol::ids::configuration::serverbound::FINISH_CONFIGURATION {
                    self.phase = Phase::Play;
                }
                Some(body(id, payload))
            }
            Phase::Play => self.play(id, payload),
        }
    }

    fn play(&mut self, id: i32, payload: &[u8]) -> Option<Vec<u8>> {
        use play::Serverbound as S;
        match id {
            sb::CONFIGURATION_ACKNOWLEDGED => {
                self.phase = Phase::Configuration;
                Some(body(id, payload))
            }
            sb::CONTAINER_CLICK => match S::decode(id, payload).ok()? {
                S::ContainerClick(mut p) => {
                    for (_, h) in &mut p.changed {
                        *h = self.hashed(h.take());
                    }
                    p.carried = self.hashed(p.carried.take());
                    Some(S::ContainerClick(p).encode())
                }
                _ => None,
            },
            sb::SET_CREATIVE_MODE_SLOT => match S::decode(id, payload).ok()? {
                S::SetCreativeModeSlot(mut p) => {
                    p.item = p.item.take().and_then(|s| self.stack_back(s));
                    Some(S::SetCreativeModeSlot(p).encode())
                }
                _ => None,
            },
            _ => Some(body(id, payload)),
        }
    }

    /// What the client believes a slot holds, in vanilla numbering. Hashes
    /// of components cannot be translated; the client sends none.
    fn hashed(&self, h: Option<Hashed>) -> Option<Hashed> {
        let mut h = h?;
        h.item = self.t.item_back(h.item)?;
        Some(h)
    }

    /// A stack an open player made (creative): the vanilla item of the same
    /// name, keeping the components both sides share.
    fn stack_back(&self, s: Stack) -> Option<Stack> {
        let back = |k: i32| self.t.component_back(k);
        Some(Stack {
            item: self.t.item_back(s.item)?,
            count: s.count,
            added: s
                .added
                .into_iter()
                // The relay's own label for stand-ins.
                .filter(|(k, _)| *k != component::ITEM_NAME)
                .filter_map(|(k, c)| Some((back(k)?, c)))
                .collect(),
            removed: s.removed.into_iter().filter_map(back).collect(),
        })
    }
}
