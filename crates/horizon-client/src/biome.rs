//! Biome colours from the pinned biome definitions and colormaps
//! (`Biome.getGrassColor`/`getFoliageColor`/..., `BlockTintSources`).
use crate::Result;
use horizon_core::fs;
use horizon_core::{noise::SimplexNoise, random::LegacyRandom};
use horizon_world::nbt::Tag;
use serde_json::Value;
use std::{collections::HashMap, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrassModifier {
    None,
    DarkForest,
    Swamp,
}
#[derive(Clone, Debug)]
pub struct SpawnEntry {
    pub entity: String,
    pub weight: u32,
    pub min: u32,
    pub max: u32,
    /// A `ConstantInt` count (sampling draws nothing).
    pub constant: bool,
}
#[derive(Clone, Debug)]
pub struct BiomeInfo {
    /// `natural_mob_spawns` by category (creature, monster, ...).
    pub spawns: HashMap<String, Vec<SpawnEntry>>,
    /// `natural_mob_spawns` spawn costs by entity type.
    pub spawn_costs: HashMap<String, crate::mobs::spawn_selection::SpawnCost>,
    /// The `gameplay/creature_world_gen_spawn_probability` attribute.
    pub creature_probability: f32,
    pub name: String,
    pub temperature: f32,
    pub downfall: f32,
    pub precipitation: bool,
    pub grass: u32,
    pub modifier: GrassModifier,
    pub foliage: u32,
    pub dry_foliage: u32,
    pub water: u32,
    pub sky: Option<u32>,
    /// The biome's own fog colour attribute; otherwise the dimension's applies.
    pub fog: Option<u32>,
    pub water_fog: u32,
}
/// Biome ids the chunk storage holds (`chunk::BIOME_STRATEGY`'s 7 bits).
const SERVER_ID_LIMIT: usize = 128;
/// A biome's sky, fog and water fog colours: what the client draws of a
/// server's biome these resources lack (`Biomes::server_ids`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerBiome {
    pub sky: Option<u32>,
    pub fog: Option<u32>,
    pub water_fog: u32,
}
impl ServerBiome {
    /// From a biome's definition (its synchronized data, as JSON).
    pub fn from_json(v: &Value) -> Self {
        let attributes = &v["attributes"];
        Self {
            sky: color(&attributes["minecraft:visual/sky_color"]),
            fog: color(&attributes["minecraft:visual/fog_color"]),
            water_fog: color(&attributes["minecraft:visual/water_fog_color"]).unwrap_or(0x050533),
        }
    }
}
pub struct Biomes {
    pub list: Vec<BiomeInfo>,
    pub index: HashMap<String, u16>,
    info_noise: SimplexNoise,
}
fn color(v: &Value) -> Option<u32> {
    match v {
        Value::String(s) => u32::from_str_radix(s.trim_start_matches('#'), 16)
            .ok()
            .map(|c| c & 0xFF_FFFF),
        Value::Number(n) => n.as_i64().map(|c| c as u32 & 0xFF_FFFF),
        _ => None,
    }
}
fn colormap(root: &Path, name: &str) -> Result<Vec<u32>> {
    let path = root.join(format!("assets/minecraft/textures/colormap/{name}.png"));
    let mut decoder = png::Decoder::new(std::io::BufReader::new(fs::File::open(path)?));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut bytes = vec![0; reader.output_buffer_size().ok_or("colormap too large")?];
    let info = reader.next_frame(&mut bytes)?;
    let channels = info.color_type.samples();
    Ok(bytes[..info.buffer_size()]
        .chunks_exact(channels)
        .map(|p| u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]))
        .collect())
}
/// `GrassColor.get`/`FoliageColor.get`: temperature and downfall index a 256x256 map.
fn sample(map: &[u32], temperature: f32, downfall: f32, fallback: u32) -> u32 {
    let temperature = f64::from(temperature.clamp(0.0, 1.0));
    let downfall = f64::from(downfall.clamp(0.0, 1.0)) * temperature;
    let x = ((1.0 - temperature) * 255.0) as i32;
    let y = ((1.0 - downfall) * 255.0) as i32;
    let index = (y << 8 | x) as usize;
    map.get(index).copied().unwrap_or(fallback)
}
impl Biomes {
    pub fn load(root: &Path) -> Result<Self> {
        let grass = colormap(root, "grass")?;
        let foliage = colormap(root, "foliage")?;
        let dry = colormap(root, "dry_foliage")?;
        let mut list = Vec::new();
        let mut names: Vec<_> = fs::read_dir(root.join("data/minecraft/worldgen/biome"))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        names.sort();
        for path in names {
            let name = format!(
                "minecraft:{}",
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or("bad biome")?
            );
            let v: Value = serde_json::from_slice(&fs::read(&path)?)?;
            let effects = &v["effects"];
            let attributes = &v["attributes"];
            let temperature = v["temperature"].as_f64().unwrap_or(0.5) as f32;
            let downfall = v["downfall"].as_f64().unwrap_or(0.5) as f32;
            let modifier = match effects["grass_color_modifier"].as_str() {
                Some("dark_forest") => GrassModifier::DarkForest,
                Some("swamp") => GrassModifier::Swamp,
                _ => GrassModifier::None,
            };
            let mut spawns: HashMap<String, Vec<SpawnEntry>> = HashMap::new();
            let natural = &attributes["minecraft:gameplay/natural_mob_spawns"];
            // A modifier object ({argument, modifier: overlay}) or the plain value;
            // the overlay over the dimension's empty default is the biome's value.
            let natural = if natural["argument"].is_object() {
                &natural["argument"]
            } else {
                natural
            };
            let categories = &natural["spawns_by_category"];
            for (category, entries) in categories.as_object().into_iter().flatten() {
                let list = crate::mobs::spawn_selection::parse_entries(entries);
                spawns.insert(category.clone(), list);
            }
            list.push(BiomeInfo {
                spawns,
                spawn_costs: crate::mobs::spawn_selection::parse_costs(&natural["spawn_costs"]),
                creature_probability:
                    attributes["minecraft:gameplay/creature_world_gen_spawn_probability"]
                        .as_f64()
                        .unwrap_or(0.1) as f32,
                temperature,
                downfall,
                precipitation: v["has_precipitation"].as_bool().unwrap_or(true),
                grass: color(&effects["grass_color"])
                    .unwrap_or_else(|| sample(&grass, temperature, downfall, 0xFF00FF)),
                modifier,
                foliage: color(&effects["foliage_color"])
                    .unwrap_or_else(|| sample(&foliage, temperature, downfall, 0x48B518)),
                dry_foliage: color(&effects["dry_foliage_color"])
                    .unwrap_or_else(|| sample(&dry, temperature, downfall, 0x5D4930)),
                water: color(&effects["water_color"]).unwrap_or(0x3F76E4),
                sky: color(&attributes["minecraft:visual/sky_color"]),
                fog: color(&attributes["minecraft:visual/fog_color"]),
                water_fog: color(&attributes["minecraft:visual/water_fog_color"])
                    .unwrap_or(0x050533),
                name,
            });
        }
        let index = list
            .iter()
            .enumerate()
            .map(|(i, b)| (b.name.clone(), i as u16))
            .collect();
        Ok(Self {
            list,
            index,
            info_noise: SimplexNoise::with_skipped_offsets(&mut LegacyRandom::new(2345), true),
        })
    }
    pub fn id(&self, name: &str) -> u16 {
        self.index
            .get(name)
            .or_else(|| self.index.get("minecraft:plains"))
            .copied()
            .unwrap_or(0)
    }
    /// A biome by id; a server's biome these resources lack (an id past
    /// `list`, `server_ids`) reads as plains, as its name would.
    pub fn get(&self, id: u16) -> &BiomeInfo {
        self.list.get(usize::from(id)).unwrap_or_else(|| {
            &self.list[usize::from(self.id("minecraft:plains")).min(self.list.len() - 1)]
        })
    }
    /// The local id of each biome of a server's registry (in its order) and
    /// the server biomes these resources lack, which take the ids after
    /// `list` in turn, with the colours their synchronized data gives
    /// (`server_colours`). Ids stop where the chunk storage does
    /// (`chunk::BIOME_STRATEGY`); past it a missing biome is plains.
    pub fn server_ids(&self, registry: &[(String, Option<Tag>)]) -> (Vec<u16>, Vec<ServerBiome>) {
        let mut missing = Vec::new();
        let mut ids = Vec::with_capacity(registry.len());
        for (name, data) in registry {
            let next = self.list.len() + missing.len();
            ids.push(match self.index.get(name) {
                Some(&id) => id,
                None if next < SERVER_ID_LIMIT => {
                    let json = data.as_ref().map(crate::item_render::tag_json);
                    missing.push(ServerBiome::from_json(&json.unwrap_or_default()));
                    next as u16
                }
                None => self.id(name),
            });
        }
        (ids, missing)
    }
    /// The sky, fog and water fog colours of biome `id`: these resources'
    /// own, or for a server's biome they lack, the server's (`server`).
    pub fn server_colours(&self, id: u16, server: &[ServerBiome]) -> ServerBiome {
        match usize::from(id)
            .checked_sub(self.list.len())
            .and_then(|i| server.get(i))
        {
            Some(s) => *s,
            None => {
                let b = self.get(id);
                ServerBiome {
                    sky: b.sky,
                    fog: b.fog,
                    water_fog: b.water_fog,
                }
            }
        }
    }
    pub fn grass(&self, id: u16, x: f64, z: f64) -> u32 {
        let b = self.get(id);
        match b.modifier {
            GrassModifier::None => b.grass,
            GrassModifier::DarkForest => ((b.grass & 0xFE_FEFE) + 0x28_340A) >> 1,
            GrassModifier::Swamp => {
                if f64::from(self.info_noise.get2(x * 0.0225, z * 0.0225)) < -0.1 {
                    0x4C_763C
                } else {
                    0x6A_7039
                }
            }
        }
    }
}

/// A 24-bit colour as sRGB components in 0..1.
pub fn rgb(c: u32) -> [f32; 3] {
    [16, 8, 0].map(|shift| (c >> shift & 255) as f32 / 255.0)
}

/// `GaussianSampler.GAUSSIAN_SAMPLE_KERNEL`.
const GAUSSIAN_KERNEL: [f64; 7] = [0.0, 1.0, 4.0, 6.0, 4.0, 1.0, 0.0];

/// `EnvironmentAttributeProbe.tick`: `GaussianSampler.sample` over the noise
/// biomes (quart cells) around `position`, in blocks. Each biome comes once
/// with its summed weight, in the order it was first sampled, as
/// `SpatialAttributeInterpolator` keeps them (zero weights included).
pub fn gaussian_weights(
    position: [f64; 3],
    mut biome_at_quart: impl FnMut(i32, i32, i32) -> u16,
) -> Vec<(u16, f64)> {
    let p = position.map(|c| c * 0.25 - 0.5);
    let integral = p.map(|c| c.floor() as i32);
    let relative: [f64; 3] = std::array::from_fn(|i| p[i] - f64::from(integral[i]));
    let weight = |axis: usize, i: usize| {
        let (start, end) = (GAUSSIAN_KERNEL[i + 1], GAUSSIAN_KERNEL[i]);
        start + relative[axis] * (end - start)
    };
    let mut weights: Vec<(u16, f64)> = Vec::new();
    for z in 0..6 {
        let wz = weight(2, z);
        for x in 0..6 {
            let wx = weight(0, x);
            for y in 0..6 {
                let wy = weight(1, y);
                let biome = biome_at_quart(
                    integral[0] - 2 + x as i32,
                    integral[1] - 2 + y as i32,
                    integral[2] - 2 + z as i32,
                );
                let w = wx * wy * wz;
                match weights.iter_mut().find(|(b, _)| *b == biome) {
                    Some((_, total)) => *total += w,
                    None => weights.push((biome, w)),
                }
            }
        }
    }
    weights
}

/// `SpatialAttributeInterpolator.applyAttributeLayer` for an RGB colour
/// (`ARGB.srgbLerp`: a component-wise lerp of the sRGB values).
pub fn blend_rgb(weights: &[(u16, f64)], mut value: impl FnMut(u16) -> [f32; 3]) -> [f32; 3] {
    let mut result: Option<[f32; 3]> = None;
    let mut accumulated = 0.0;
    for &(biome, weight) in weights {
        let v = value(biome);
        accumulated += weight;
        result = Some(match result {
            None => v,
            Some(r) => {
                let t = (weight / accumulated) as f32;
                std::array::from_fn(|i| r[i] + (v[i] - r[i]) * t)
            }
        });
    }
    result.unwrap_or([0.0; 3])
}

#[cfg(test)]
mod blend_tests {
    use super::*;

    #[test]
    fn one_biome_keeps_its_colour() {
        let weights = gaussian_weights([10.3, 64.0, -7.9], |_, _, _| 4);
        assert_eq!(weights.len(), 1);
        // The kernel sums to 16 per axis whatever the offset.
        assert!((weights[0].1 - 16f64.powi(3)).abs() < 1e-9);
        assert_eq!(blend_rgb(&weights, |_| [0.2, 0.4, 0.6]), [0.2, 0.4, 0.6]);
    }

    #[test]
    fn colour_changes_gradually_across_a_boundary() {
        // Biome 1 for quart x >= 0 (block x >= 0), biome 0 below.
        let at = |x: f64| {
            let w = gaussian_weights([x, 40.0, 0.0], |qx, _, _| u16::from(qx >= 0));
            blend_rgb(&w, |b| if b == 1 { [1.0, 0.0, 0.0] } else { [0.0; 3] })[0]
        };
        assert_eq!(at(-12.0), 0.0);
        assert_eq!(at(12.0), 1.0);
        // Halfway at the boundary, rising steadily through it.
        assert!((at(0.0) - 0.5).abs() < 1e-5, "{}", at(0.0));
        let mut last = 0.0;
        for i in -16..=16 {
            let v = at(f64::from(i) * 0.5);
            assert!(v >= last - 1e-6, "{v} < {last} at {i}");
            last = v;
        }
        assert!(at(-2.0) > 0.0 && at(-2.0) < 0.5);
    }
}

#[cfg(test)]
mod server_biome_tests {
    use super::*;
    use horizon_world::nbt::JavaString;

    fn info(name: &str, sky: u32) -> BiomeInfo {
        BiomeInfo {
            spawns: HashMap::new(),
            spawn_costs: HashMap::new(),
            creature_probability: 0.1,
            name: name.into(),
            temperature: 0.8,
            downfall: 0.4,
            precipitation: false,
            grass: 0x91BD59,
            modifier: GrassModifier::None,
            foliage: 0x77AB2F,
            dry_foliage: 0x5D4930,
            water: 0x3F76E4,
            sky: Some(sky),
            fog: Some(sky),
            water_fog: 0x050533,
        }
    }

    /// A biome's synchronized data: its attributes compound.
    fn data(attributes: &[(&str, &str)]) -> Option<Tag> {
        let compound = |pairs: Vec<(JavaString, Tag)>| Tag::Compound(pairs.into_iter().collect());
        Some(compound(vec![(
            JavaString::from("attributes"),
            compound(
                attributes
                    .iter()
                    .map(|(k, v)| (JavaString::from(*k), Tag::String(JavaString::from(*v))))
                    .collect(),
            ),
        )]))
    }

    #[test]
    fn a_servers_biomes_the_resources_lack_keep_their_colours() {
        // The open bundle: plains only.
        let biomes = Biomes {
            list: vec![info("minecraft:plains", 0x7FB0FF)],
            index: [("minecraft:plains".to_owned(), 0)].into_iter().collect(),
            info_noise: SimplexNoise::with_skipped_offsets(&mut LegacyRandom::new(2345), true),
        };
        let registry = vec![
            ("minecraft:badlands".to_owned(), data(&[])),
            (
                "minecraft:nether_wastes".to_owned(),
                data(&[("minecraft:visual/fog_color", "#330808")]),
            ),
            ("minecraft:plains".to_owned(), data(&[])),
        ];
        let (ids, missing) = biomes.server_ids(&registry);
        assert_eq!(ids, vec![1, 2, 0]);
        assert_eq!(missing.len(), 2);
        // The server's Nether fog, not the bundle's plains.
        let nether = biomes.server_colours(2, &missing);
        assert_eq!((nether.sky, nether.fog), (None, Some(0x330808)));
        assert_eq!(biomes.server_colours(0, &missing).sky, Some(0x7FB0FF));
        // Everything else about a missing biome reads as plains.
        assert_eq!(biomes.get(2).name, "minecraft:plains");
        // Past the chunk storage's ids, a missing biome is plains.
        let many: Vec<_> = (0..200)
            .map(|i| (format!("custom:b{i}"), data(&[])))
            .collect();
        let (ids, missing) = biomes.server_ids(&many);
        assert_eq!(missing.len(), SERVER_ID_LIMIT - 1);
        assert_eq!(ids[199], 0);
        assert!(ids.iter().all(|&id| usize::from(id) < SERVER_ID_LIMIT));
    }
}
