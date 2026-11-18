//! The numbering tables between a vanilla server and open players, built at
//! start from two inputs read at runtime and never stored in this crate:
//!
//! - the vanilla server's data generator reports (`blocks.json`,
//!   `registries.json`; `java -DbundlerMainClass=net.minecraft.data.Main -jar
//!   server.jar --reports`, see `vanilla.rs`), from the operator's own server jar;
//! - the open bundle the players run (`registries.json`, `facts.json`,
//!   `horizon-open-provenance.json`, `attributes.json`), so the relay's
//!   numbers are the players'.
use crate::rules;
use horizon_protocol::registry::{Builtin, Ids};
use horizon_world::{chunk::BlockState, registry::BlockRegistry};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
};

type Result<T> = std::result::Result<T, String>;

/// Vanilla block states by protocol id, from the `blocks.json` report.
pub fn vanilla_states(report: &serde_json::Value) -> Result<Vec<BlockState>> {
    let blocks = report.as_object().ok_or("blocks.json is not an object")?;
    let mut states: Vec<Option<BlockState>> = Vec::new();
    for (name, block) in blocks {
        let list = block["states"]
            .as_array()
            .ok_or_else(|| format!("blocks.json: {name} has no states"))?;
        for state in list {
            let id = state["id"]
                .as_u64()
                .ok_or_else(|| format!("blocks.json: {name}: a state without an id"))?
                as usize;
            let properties = state["properties"]
                .as_object()
                .map(|p| {
                    p.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                        .collect()
                })
                .unwrap_or_default();
            if id >= states.len() {
                states.resize(id + 1, None);
            }
            states[id] = Some(BlockState {
                name: name.clone(),
                properties,
            });
        }
    }
    states
        .into_iter()
        .enumerate()
        .map(|(i, s)| s.ok_or_else(|| format!("blocks.json: no state {i}")))
        .collect()
}

/// The open bundle as the relay needs it.
pub struct Open {
    pub registries: Builtin,
    /// Attribute names by open wire id: the bundle's `attributes.json` order
    /// (`attributes::Catalog::from_json`), not a `registries.json` entry.
    pub attributes: Ids,
    pub states: BlockRegistry,
    /// Per open state: (air, fluid), as `facts.json` flags them.
    pub flags: Vec<(bool, bool)>,
    /// Blocks players can see: a block-state definition in the bundle (the
    /// rest are inert support blocks), plus the air and water every world has.
    pub visible: std::collections::HashSet<String>,
    /// `minecraft:brand` an open player expects (`assets::server_brand`).
    pub brand: String,
}
impl Open {
    pub fn load(root: &Path) -> Result<Self> {
        let read = |name: &str| {
            fs::read(root.join(name)).map_err(|e| format!("{}: {e}", root.join(name).display()))
        };
        let manifest: serde_json::Value = serde_json::from_slice(&read("horizon-resources.json")?)
            .map_err(|e| format!("horizon-resources.json: {e}"))?;
        if manifest["content_profile"].as_str() != Some("open-creative") {
            return Err(format!(
                "{} is not a Horizon Open Creative bundle (horizon-prepare open --out DIR)",
                root.display()
            ));
        }
        let registries = Builtin::parse(
            std::str::from_utf8(&read("registries.json")?).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let (states, flags) =
            BlockRegistry::from_facts_json_with_flags(read("facts.json")?.as_slice())
                .map_err(|e| format!("facts.json: {e}"))?;
        let attributes = attribute_names(&read("attributes.json")?)?;
        let mut visible: std::collections::HashSet<String> = ["minecraft:air", "minecraft:water"]
            .into_iter()
            .map(String::from)
            .collect();
        let definitions = root.join("assets/minecraft/blockstates");
        for entry in fs::read_dir(&definitions)
            .map_err(|e| format!("{}: {e}", definitions.display()))?
            .flatten()
        {
            if let Some(name) = entry
                .file_name()
                .to_str()
                .and_then(|n| n.strip_suffix(".json"))
            {
                visible.insert(format!("minecraft:{name}"));
            }
        }
        Ok(Self {
            registries,
            attributes,
            states,
            flags,
            visible,
            brand: brand(&read("horizon-open-provenance.json")?),
        })
    }
}

/// The attribute names of a bundle's `attributes.json`, in wire-id order.
pub fn attribute_names(json: &[u8]) -> Result<Ids> {
    let v: serde_json::Value =
        serde_json::from_slice(json).map_err(|e| format!("attributes.json: {e}"))?;
    let list = v["attributes"]
        .as_array()
        .ok_or("attributes.json has no attributes list")?;
    list.iter()
        .map(|a| {
            a["id"]
                .as_str()
                .map(String::from)
                .ok_or_else(|| "attributes.json: an attribute without an id".to_string())
        })
        .collect::<Result<Vec<_>>>()
        .map(Ids::from_names)
}

/// `horizon-open/` and the first six bytes of the provenance record's SHA-256
/// in hex: what `horizon_client::assets::server_brand` gives the same bundle.
pub fn brand(provenance: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(provenance);
    let hex: String = digest[..6].iter().map(|b| format!("{b:02x}")).collect();
    format!("horizon-open/{hex}")
}

/// Vanilla id → open id for one registry, by name; `None` where the bundle
/// has no entry of that name.
fn by_name(vanilla: &Ids, open: &Ids) -> Vec<Option<i32>> {
    vanilla.names.iter().map(|n| open.id(n)).collect()
}

/// An item as an open player gets it.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemTarget {
    /// The open item id (0 is air: no stack).
    pub id: i32,
    /// The vanilla item's own name when the open item stands in for another
    /// (shown as the stack's name, so "Golden Apple" reads right on an apple).
    pub shown_as: Option<String>,
}

/// Every table one relay uses, shared by its connections.
pub struct Tables {
    /// Vanilla block state → open block state.
    pub states: Vec<u32>,
    /// Open state flags (air, fluid) for section counts.
    pub open_flags: Vec<(bool, bool)>,
    /// Bits of the vanilla and open global block palettes.
    pub vanilla_state_bits: u8,
    pub open_state_bits: u8,
    /// Vanilla item → open item; `None` for air.
    pub items: Vec<Option<ItemTarget>>,
    /// Open item → the vanilla item of the same name.
    pub items_back: Vec<Option<i32>>,
    pub entity_types: Vec<Option<i32>>,
    /// Vanilla entity type → the open mob standing in for it (`rules::stand_in`), for
    /// types the bundle doesn't have.
    pub stand_ins: Vec<Option<i32>>,
    pub menus: Vec<Option<i32>>,
    /// Vanilla attribute → open attribute; `None` where the bundle has none.
    pub attributes: Vec<Option<i32>>,
    /// Vanilla data component id → open id, for the components the bundle names.
    pub components: Vec<Option<i32>>,
    pub components_back: Vec<Option<i32>>,
    pub open_component_names: HashMap<String, i32>,
    /// Both sides number command argument types alike, so the command tree
    /// passes unchanged (its parsers are read by those numbers).
    pub commands_compatible: bool,
    pub open_air: u32,
    /// The vanilla `minecraft:falling_block` entity type (its spawn data is a state).
    pub falling_block: Option<i32>,
    pub brand: String,
    /// Vanilla blocks shown as another block, and as air, for the start log.
    pub substituted_blocks: usize,
    pub hidden_blocks: Vec<String>,
}

impl Tables {
    /// Load from the vanilla reports folder (`blocks.json`, `registries.json`)
    /// and the open bundle.
    pub fn load(vanilla_reports: &Path, open_root: &Path) -> Result<Self> {
        let read_json = |name: &str| -> Result<serde_json::Value> {
            let path = vanilla_reports.join(name);
            let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
        };
        let blocks = vanilla_states(&read_json("blocks.json")?)?;
        let registries = Builtin::parse(&read_json("registries.json")?.to_string())
            .map_err(|e| e.to_string())?;
        Self::build(&blocks, &registries, &Open::load(open_root)?)
    }

    pub fn build(vanilla_states: &[BlockState], vanilla: &Builtin, open: &Open) -> Result<Self> {
        let get = |b: &Builtin, name: &str| -> Result<Ids> {
            b.get(name)
                .cloned()
                .ok_or_else(|| format!("registry {name} is missing"))
        };
        // Open states grouped by block name, for the best property match.
        let mut open_by_name: HashMap<&str, Vec<(u32, &BlockState)>> = HashMap::new();
        for i in 0..open.states.len() {
            let id = horizon_world::registry::BlockStateId(i as u32);
            if let Some(s) = open.states.state(id) {
                open_by_name
                    .entry(s.name.as_str())
                    .or_default()
                    .push((i as u32, s));
            }
        }
        let open_air = open
            .states
            .default_state("minecraft:air")
            .ok_or("the open bundle has no air")?
            .0;
        let pick = |name: &str, properties: &BTreeMap<String, String>| -> Option<u32> {
            let candidates = open_by_name.get(name)?;
            let default = open.states.default_state(name).map(|d| d.0);
            let score = |s: &BlockState| {
                properties
                    .iter()
                    .filter(|(k, v)| s.properties.get(*k) == Some(*v))
                    .count()
            };
            let best = candidates.iter().map(|(_, s)| score(s)).max()?;
            candidates
                .iter()
                .filter(|(_, s)| score(s) == best)
                .map(|(id, _)| *id)
                .min_by_key(|id| Some(*id) != default)
        };
        let mut substituted_blocks = 0;
        let mut hidden: std::collections::BTreeSet<String> = Default::default();
        let mut cache: HashMap<String, String> = HashMap::new();
        let states: Vec<u32> = vanilla_states
            .iter()
            .map(|s| {
                if open.visible.contains(&s.name)
                    && let Some(id) = pick(&s.name, &s.properties)
                {
                    return id;
                }
                substituted_blocks += 1;
                let short = s.name.strip_prefix("minecraft:").unwrap_or(&s.name);
                let wet = s.properties.get("waterlogged").is_some_and(|v| v == "true");
                let key = format!("{short}{wet}");
                let target = cache
                    .entry(key)
                    .or_insert_with(|| format!("minecraft:{}", rules::block(short, wet)))
                    .clone();
                if target == "minecraft:air" && !s.name.ends_with("air") {
                    hidden.insert(s.name.clone());
                }
                pick(&target, &s.properties).unwrap_or(open_air)
            })
            .collect();

        let vanilla_items = get(vanilla, "minecraft:item")?;
        let open_items = get(&open.registries, "minecraft:item")?;
        let vanilla_block_names: std::collections::HashSet<&str> =
            vanilla_states.iter().map(|s| s.name.as_str()).collect();
        let stick = open_items
            .id("minecraft:stick")
            .ok_or("the open bundle has no stick")?;
        let open_air_item = open_items.id("minecraft:air");
        let items = vanilla_items
            .names
            .iter()
            .map(|name| {
                if name == "minecraft:air" || name.is_empty() {
                    return None;
                }
                if let Some(id) = open_items.id(name) {
                    return Some(ItemTarget { id, shown_as: None });
                }
                let short = name.strip_prefix("minecraft:").unwrap_or(name);
                // A block item becomes the item of the block its block becomes.
                let as_block = vanilla_block_names
                    .contains(name.as_str())
                    .then(|| format!("minecraft:{}", rules::block(short, false)))
                    .and_then(|b| open_items.id(&b));
                let id = as_block
                    .filter(|id| Some(*id) != open_air_item)
                    .or_else(|| open_items.id(&format!("minecraft:{}", rules::item(short))))
                    .unwrap_or(stick);
                Some(ItemTarget {
                    id,
                    shown_as: Some(rules::title(short)),
                })
            })
            .collect();
        let items_back = open_items
            .names
            .iter()
            .map(|n| vanilla_items.id(n).filter(|_| n != "minecraft:air"))
            .collect();

        let vanilla_entities = get(vanilla, "minecraft:entity_type")?;
        let open_entities = get(&open.registries, "minecraft:entity_type")?;
        let components_open = get(&open.registries, "minecraft:data_component_type")?;
        let vanilla_components = get(vanilla, "minecraft:data_component_type")?;
        let open_component_names = components_open
            .names
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.is_empty())
            .map(|(i, n)| (n.clone(), i as i32))
            .collect();
        Ok(Self {
            states,
            open_flags: open.flags.clone(),
            vanilla_state_bits: horizon_protocol::registry::ceil_log2(vanilla_states.len()),
            // The open client reads a global palette at 16 bits or more
            // (`chunk::set_block_states`).
            open_state_bits: horizon_protocol::registry::ceil_log2(open.states.len()).max(16),
            items,
            items_back,
            entity_types: by_name(&vanilla_entities, &open_entities),
            stand_ins: vanilla_entities
                .names
                .iter()
                .map(|n| rules::stand_in(n).and_then(|open| open_entities.id(open)))
                .collect(),
            menus: by_name(
                &get(vanilla, "minecraft:menu")?,
                &get(&open.registries, "minecraft:menu")?,
            ),
            attributes: by_name(&get(vanilla, "minecraft:attribute")?, &open.attributes),
            // Components keep their number on both sides or are dropped: the
            // shared item codec reads each by its vanilla number.
            components: by_name(&vanilla_components, &components_open)
                .into_iter()
                .enumerate()
                .map(|(i, open)| open.filter(|o| *o == i as i32))
                .collect(),
            components_back: by_name(&components_open, &vanilla_components)
                .into_iter()
                .enumerate()
                .map(|(i, v)| v.filter(|v| *v == i as i32))
                .collect(),
            open_component_names,
            commands_compatible: vanilla
                .get("minecraft:command_argument_type")
                .zip(open.registries.get("minecraft:command_argument_type"))
                .is_some_and(|(v, o)| v.names == o.names),
            open_air,
            falling_block: vanilla_entities.id("minecraft:falling_block"),
            brand: open.brand.clone(),
            substituted_blocks,
            hidden_blocks: hidden.into_iter().collect(),
        })
    }

    /// The open state for a vanilla one (air for ids past the registry).
    pub fn state(&self, vanilla: i32) -> i32 {
        usize::try_from(vanilla)
            .ok()
            .and_then(|i| self.states.get(i))
            .copied()
            .unwrap_or(self.open_air) as i32
    }
    pub fn is_air(&self, open: u32) -> bool {
        self.open_flags.get(open as usize).is_none_or(|f| f.0)
    }
    pub fn is_fluid(&self, open: u32) -> bool {
        self.open_flags.get(open as usize).is_some_and(|f| f.1)
    }
    /// The open item for a vanilla one; `None` is an empty slot.
    pub fn item(&self, vanilla: i32) -> Option<&ItemTarget> {
        usize::try_from(vanilla)
            .ok()
            .and_then(|i| self.items.get(i))
            .and_then(Option::as_ref)
    }
    /// The vanilla item an open player's item stands for.
    pub fn item_back(&self, open: i32) -> Option<i32> {
        usize::try_from(open)
            .ok()
            .and_then(|i| self.items_back.get(i).copied().flatten())
    }
    pub fn entity_type(&self, vanilla: i32) -> Option<i32> {
        usize::try_from(vanilla)
            .ok()
            .and_then(|i| self.entity_types.get(i).copied().flatten())
    }
    /// The open mob standing in for a vanilla type the bundle lacks.
    pub fn stand_in(&self, vanilla: i32) -> Option<i32> {
        usize::try_from(vanilla)
            .ok()
            .and_then(|i| self.stand_ins.get(i).copied().flatten())
    }
    pub fn menu(&self, vanilla: i32) -> Option<i32> {
        usize::try_from(vanilla)
            .ok()
            .and_then(|i| self.menus.get(i).copied().flatten())
    }
    pub fn attribute(&self, vanilla: i32) -> Option<i32> {
        usize::try_from(vanilla)
            .ok()
            .and_then(|i| self.attributes.get(i).copied().flatten())
    }
    pub fn component(&self, vanilla: i32) -> Option<i32> {
        usize::try_from(vanilla)
            .ok()
            .and_then(|i| self.components.get(i).copied().flatten())
    }
    pub fn component_back(&self, open: i32) -> Option<i32> {
        usize::try_from(open)
            .ok()
            .and_then(|i| self.components_back.get(i).copied().flatten())
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    //! A tiny vanilla-shaped registry and open bundle, written for the tests
    //! (no vanilla data: invented ids in vanilla's report format).
    use super::*;

    pub fn state(name: &str, props: &[(&str, &str)]) -> BlockState {
        BlockState {
            name: format!("minecraft:{name}"),
            properties: props
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
        }
    }

    fn registry(names: &[&str]) -> Ids {
        Ids::from_names(names.iter().map(|n| format!("minecraft:{n}")).collect())
    }

    pub fn tables() -> Tables {
        let vanilla_states = vec![
            state("air", &[]),
            state("stone", &[]),
            state("granite", &[]),
            state("oak_log", &[("axis", "x")]),
            state("oak_log", &[("axis", "y")]),
            state("oak_log", &[("axis", "z")]),
            state("dark_oak_log", &[("axis", "x")]),
            state("dark_oak_log", &[("axis", "y")]),
            state("dark_oak_log", &[("axis", "z")]),
            state("torch", &[]),
            state("oak_slab", &[("type", "top"), ("waterlogged", "true")]),
            state("oak_button", &[("waterlogged", "true")]),
            state("cave_air", &[]),
            state("water", &[("level", "0")]),
            state("lava", &[("level", "0")]),
            state("lever", &[("waterlogged", "false")]),
        ];
        let mut vanilla = Builtin::default();
        vanilla.registries.insert(
            "minecraft:item".into(),
            registry(&[
                "air",
                "stone",
                "golden_apple",
                "oak_log",
                "torch",
                "netherite_pickaxe",
                "dark_oak_log",
                "apple",
            ]),
        );
        vanilla.registries.insert(
            "minecraft:entity_type".into(),
            registry(&[
                "zombie",
                "item",
                "falling_block",
                "player",
                "experience_orb",
                "wither_skeleton",
            ]),
        );
        vanilla.registries.insert(
            "minecraft:menu".into(),
            registry(&["generic_9x1", "crafting"]),
        );
        vanilla.registries.insert(
            "minecraft:data_component_type".into(),
            registry(&["custom_data", "max_stack_size", "item_model", "food"]),
        );
        vanilla.registries.insert(
            "minecraft:attribute".into(),
            registry(&["armor", "below_name_distance", "oxygen_bonus", "scale"]),
        );
        let mut open_registries = Builtin::default();
        open_registries.registries.insert(
            "minecraft:item".into(),
            registry(&[
                "air",
                "stone",
                "oak_log",
                "spruce_log",
                "apple",
                "stick",
                "diamond_pickaxe",
            ]),
        );
        open_registries.registries.insert(
            "minecraft:entity_type".into(),
            registry(&[
                "player",
                "item",
                "experience_orb",
                "falling_block",
                "skeleton",
            ]),
        );
        open_registries.registries.insert(
            "minecraft:menu".into(),
            registry(&["generic_9x1", "crafting"]),
        );
        open_registries.registries.insert(
            "minecraft:data_component_type".into(),
            registry(&["custom_data", "max_stack_size"]),
        );
        let facts = serde_json::json!({
            "defaults": {
                "minecraft:air": 0, "minecraft:stone": 1, "minecraft:oak_log": 3,
                "minecraft:spruce_log": 5, "minecraft:water": 7, "minecraft:orange_wool": 8,
                "minecraft:oak_planks": 9, "minecraft:cave_air": 10, "minecraft:torch": 11,
            },
            "states": [
                ["minecraft:air", true, false, "", "minecraft:air"],
                ["minecraft:stone", false, false, "", "minecraft:stone"],
                ["minecraft:oak_log", false, false, "", {"id": "minecraft:oak_log", "properties": {"axis": "x"}}],
                ["minecraft:oak_log", false, false, "", {"id": "minecraft:oak_log", "properties": {"axis": "y"}}],
                ["minecraft:spruce_log", false, false, "", {"id": "minecraft:spruce_log", "properties": {"axis": "x"}}],
                ["minecraft:spruce_log", false, false, "", {"id": "minecraft:spruce_log", "properties": {"axis": "y"}}],
                ["minecraft:spruce_log", false, false, "", {"id": "minecraft:spruce_log", "properties": {"axis": "z"}}],
                ["minecraft:water", false, true, "", "minecraft:water"],
                ["minecraft:orange_wool", false, false, "", "minecraft:orange_wool"],
                ["minecraft:oak_planks", false, false, "", "minecraft:oak_planks"],
                ["minecraft:cave_air", true, false, "", "minecraft:cave_air"],
                ["minecraft:torch", false, false, "", "minecraft:torch"],
            ],
        });
        let (states, flags) =
            BlockRegistry::from_facts_json_with_flags(facts.to_string().as_bytes()).unwrap();
        let visible = [
            "air",
            "stone",
            "oak_log",
            "spruce_log",
            "water",
            "orange_wool",
            "oak_planks",
        ]
        .iter()
        .map(|n| format!("minecraft:{n}"))
        .collect();
        let attributes = attribute_names(
            serde_json::json!({"attributes": [
                {"id": "minecraft:scale"},
                {"id": "minecraft:oxygen_bonus"},
                {"id": "minecraft:armor"},
            ]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        let open = Open {
            registries: open_registries,
            attributes,
            states,
            flags,
            visible,
            brand: brand(b"{}"),
        };
        Tables::build(&vanilla_states, &vanilla, &open).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::tables;

    #[test]
    fn same_names_keep_their_properties() {
        let t = tables();
        assert_eq!(t.state(0), 0);
        assert_eq!(t.state(1), 1);
        // oak_log axis=y is open state 3; axis=z has no open state: the
        // default (axis=y, id 3) wins the tie.
        assert_eq!(t.state(4), 3);
        assert_eq!(t.state(3), 2);
        assert_eq!(t.state(5), 3);
    }

    #[test]
    fn substitutes_keep_matching_properties() {
        let t = tables();
        // dark_oak_log → spruce_log on the same axis.
        assert_eq!([t.state(6), t.state(7), t.state(8)], [4, 5, 6]);
        // Granite is not in this bundle: air.
        assert_eq!(t.state(2), 0);
        // A torch the bundle holds only as an invisible support block is air.
        assert_eq!(t.state(9), 0);
        // A waterlogged slab is planks; a waterlogged button is water.
        assert_eq!(t.state(10), 9);
        assert_eq!(t.state(11), 7);
        assert_eq!(t.state(13), 7);
        assert_eq!(t.state(14), 8);
        assert_eq!(t.state(15), 0);
        // Unknown ids read as air.
        assert_eq!(t.state(9999), 0);
        assert!(t.hidden_blocks.contains(&"minecraft:torch".to_string()));
    }

    #[test]
    fn items_entities_menus_and_components() {
        let t = tables();
        assert_eq!(t.item(1).unwrap().id, 1);
        assert_eq!(t.item(1).unwrap().shown_as, None);
        let golden = t.item(2).unwrap();
        assert_eq!(
            (golden.id, golden.shown_as.as_deref()),
            (4, Some("Golden Apple"))
        );
        assert_eq!(t.item(4).unwrap().id, 5, "a torch is a stick");
        assert_eq!(t.item(5).unwrap().id, 6, "netherite → diamond pickaxe");
        assert_eq!(
            t.item(6).unwrap().id,
            3,
            "a dark oak log item is a spruce log"
        );
        assert_eq!(t.item_back(4), Some(7));
        assert_eq!(
            t.item_back(3),
            None,
            "no vanilla spruce_log in this fixture"
        );
        assert!(t.item(0).is_none(), "air is an empty slot");
        assert_eq!(t.entity_type(0), None);
        assert_eq!(t.entity_type(3), Some(0));
        assert_eq!(t.falling_block, Some(2));
        assert_eq!(t.menu(1), Some(1));
        assert_eq!(t.component(1), Some(1));
        assert_eq!(t.component(3), None);
        assert_eq!(t.component_back(1), Some(1));
        assert_eq!(
            (0..5).map(|i| t.attribute(i)).collect::<Vec<_>>(),
            vec![Some(2), None, Some(1), Some(0), None],
            "armor, below_name_distance (not in the bundle), oxygen_bonus, scale"
        );
        assert!(!t.commands_compatible, "the fixture has no argument types");
    }
}
