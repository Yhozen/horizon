//! Program slots, their fallback chains, and finding a slot's file in a pack.
//!
//! The names and chains are the public format docs' (format notes §2): the
//! OptiFine programs, the Iris-only gbuffers programs and the shadow
//! sub-programs. Iris-era slots that only split an older slot's geometry
//! (`gbuffers_terrain_cutout` out of `gbuffers_terrain`, `shadow_water` out of
//! `shadow` …) have a [`ProgramKind::parent`]: they run only when the pack has
//! their own file, and otherwise the parent slot draws their geometry.

use std::collections::BTreeSet;

use crate::{PackFiles, host, properties};

/// The highest `N` of `deferredN`, `compositeN` and `prepareN`.
pub const MAX_PASS: u8 = 99;

/// A program slot of the frame, by OptiFine's program names. A slot without
/// its own file runs its fallback ([`ProgramKind::candidates`]); a slot with
/// no file at all draws nothing (gbuffers) or is skipped (full-screen passes).
///
/// The derived order puts the full-screen passes in the order they run:
/// `prepare*`, `deferred*`, `composite*`, `final`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProgramKind {
    /// Shadow map, terrain (`shadow`).
    Shadow,
    /// Shadow map, entities and block entities (`shadow_entities`, then `shadow`), entity geometry.
    ShadowEntities,
    /// Sky dome, horizon, void (`gbuffers_skybasic`), entity geometry.
    SkyBasic,
    /// Sun and moon (`gbuffers_skytextured`), entity geometry.
    SkyTextured,
    /// Clouds (`gbuffers_clouds`), entity geometry.
    Clouds,
    /// Solid and cutout terrain (`gbuffers_terrain`).
    Terrain,
    /// Translucent terrain (`gbuffers_water`).
    Water,
    /// Block entities (`gbuffers_block`), entity geometry.
    Block,
    /// Entities and name tags (`gbuffers_entities`).
    Entities,
    /// Particles (`gbuffers_textured_lit`).
    TexturedLit,
    /// Unlit particles, world border (`gbuffers_textured`).
    Textured,
    /// Lines (`gbuffers_basic`).
    Basic,
    /// The hand and held items (`gbuffers_hand`).
    Hand,
    /// Translucent held items (`gbuffers_hand_water`).
    HandWater,
    /// Rain and snow (`gbuffers_weather`).
    Weather,
    /// Enchantment glint (`gbuffers_armor_glint`).
    ArmorGlint,
    /// Block-breaking cracks (`gbuffers_damagedblock`).
    DamagedBlock,
    /// Block outline, fishing line (`gbuffers_line`; parent [`Self::Basic`]).
    Line,
    /// Beacon beams (`gbuffers_beaconbeam`; parent [`Self::Textured`]).
    BeaconBeam,
    /// Spider, enderman and dragon eyes (`gbuffers_spidereyes`; parent [`Self::Textured`]).
    SpiderEyes,
    /// Entities with the glowing effect (`gbuffers_entities_glowing`, OptiFine
    /// only: Iris does not run it; parent [`Self::Entities`]).
    EntitiesGlowing,
    /// Translucent entities (`gbuffers_entities_translucent`; parent [`Self::Entities`]).
    EntitiesTranslucent,
    /// Lightning, dragon death beams (`gbuffers_lightning`; parent [`Self::Entities`]).
    Lightning,
    /// Translucent block entities (`gbuffers_block_translucent`; parent [`Self::Block`]).
    BlockTranslucent,
    /// Particles (`gbuffers_particles`; parent [`Self::TexturedLit`]).
    Particles,
    /// Translucent particles (`gbuffers_particles_translucent`; parent [`Self::Particles`]).
    ParticlesTranslucent,
    /// Solid terrain only (`gbuffers_terrain_solid`; parent [`Self::Terrain`]).
    TerrainSolid,
    /// Cutout terrain only (`gbuffers_terrain_cutout`; parent [`Self::Terrain`]).
    TerrainCutout,
    /// Shadow map, solid terrain (`shadow_solid`; parent [`Self::Shadow`]).
    ShadowSolid,
    /// Shadow map, cutout terrain (`shadow_cutout`; parent [`Self::Shadow`]).
    ShadowCutout,
    /// Shadow map, translucent terrain (`shadow_water`; parent [`Self::Shadow`]).
    ShadowWater,
    /// Shadow map, block entities (`shadow_block`; parent [`Self::ShadowEntities`]).
    ShadowBlock,
    /// `prepare`, `prepare1` … `prepare99`: full-screen passes after the
    /// shadow map, before the opaque gbuffers.
    Prepare(u8),
    /// `deferred`, `deferred1` … `deferred99` (0 is `deferred`).
    Deferred(u8),
    /// `composite`, `composite1` … `composite99` (0 is `composite`).
    Composite(u8),
    /// The last full-screen pass, into the screen (`final`).
    Final,
}

impl ProgramKind {
    /// The gbuffers and shadow slots Horizon drew before the Iris-era slots,
    /// in no particular order.
    pub const GEOMETRY: [ProgramKind; 17] = [
        Self::Shadow,
        Self::ShadowEntities,
        Self::SkyBasic,
        Self::SkyTextured,
        Self::Clouds,
        Self::Terrain,
        Self::Water,
        Self::Block,
        Self::Entities,
        Self::TexturedLit,
        Self::Textured,
        Self::Basic,
        Self::Hand,
        Self::HandWater,
        Self::Weather,
        Self::ArmorGlint,
        Self::DamagedBlock,
    ];

    /// The slots that split an older slot's geometry ([`Self::parent`]).
    pub const SPLIT: [ProgramKind; 15] = [
        Self::Line,
        Self::BeaconBeam,
        Self::SpiderEyes,
        Self::EntitiesGlowing,
        Self::EntitiesTranslucent,
        Self::Lightning,
        Self::BlockTranslucent,
        Self::Particles,
        Self::ParticlesTranslucent,
        Self::TerrainSolid,
        Self::TerrainCutout,
        Self::ShadowSolid,
        Self::ShadowCutout,
        Self::ShadowWater,
        Self::ShadowBlock,
    ];

    /// The slot whose program draws this slot's geometry when the pack has no
    /// file of this slot's own name (Iris-era split slots only).
    pub fn parent(self) -> Option<ProgramKind> {
        Some(match self {
            Self::Line => Self::Basic,
            Self::BeaconBeam | Self::SpiderEyes => Self::Textured,
            Self::EntitiesGlowing | Self::EntitiesTranslucent | Self::Lightning => Self::Entities,
            Self::BlockTranslucent => Self::Block,
            Self::Particles => Self::TexturedLit,
            Self::ParticlesTranslucent => Self::Particles,
            Self::TerrainSolid | Self::TerrainCutout => Self::Terrain,
            Self::ShadowSolid | Self::ShadowCutout | Self::ShadowWater => Self::Shadow,
            Self::ShadowBlock => Self::ShadowEntities,
            _ => return None,
        })
    }

    /// The program file names to try, best first (the documented fallbacks).
    /// A split slot's chain goes on through its parent's.
    pub fn candidates(self) -> Vec<String> {
        let own = |first: &str, then: ProgramKind| -> Vec<String> {
            std::iter::once(first.to_string())
                .chain(then.candidates())
                .collect()
        };
        let chain: &[&str] = match self {
            Self::Shadow => &["shadow"],
            Self::ShadowEntities => return own("shadow_entities", Self::Shadow),
            Self::ShadowSolid => return own("shadow_solid", Self::Shadow),
            Self::ShadowCutout => return own("shadow_cutout", Self::Shadow),
            Self::ShadowWater => return own("shadow_water", Self::Shadow),
            Self::ShadowBlock => return own("shadow_block", Self::ShadowEntities),
            Self::Basic => &["gbuffers_basic"],
            Self::Line => return own("gbuffers_line", Self::Basic),
            Self::Textured => &["gbuffers_textured", "gbuffers_basic"],
            Self::TexturedLit => return own("gbuffers_textured_lit", Self::Textured),
            Self::SkyBasic => &["gbuffers_skybasic", "gbuffers_basic"],
            Self::SkyTextured => return own("gbuffers_skytextured", Self::Textured),
            Self::Clouds => return own("gbuffers_clouds", Self::Textured),
            Self::Terrain => return own("gbuffers_terrain", Self::TexturedLit),
            Self::TerrainSolid => return own("gbuffers_terrain_solid", Self::Terrain),
            Self::TerrainCutout => return own("gbuffers_terrain_cutout", Self::Terrain),
            Self::DamagedBlock => return own("gbuffers_damagedblock", Self::Terrain),
            Self::Block => return own("gbuffers_block", Self::Terrain),
            Self::BlockTranslucent => return own("gbuffers_block_translucent", Self::Block),
            Self::Water => return own("gbuffers_water", Self::Terrain),
            Self::Entities => return own("gbuffers_entities", Self::TexturedLit),
            Self::EntitiesGlowing => return own("gbuffers_entities_glowing", Self::Entities),
            Self::EntitiesTranslucent => {
                return own("gbuffers_entities_translucent", Self::Entities);
            }
            Self::Lightning => return own("gbuffers_lightning", Self::Entities),
            Self::Particles => return own("gbuffers_particles", Self::TexturedLit),
            Self::ParticlesTranslucent => {
                return own("gbuffers_particles_translucent", Self::Particles);
            }
            Self::ArmorGlint => return own("gbuffers_armor_glint", Self::Textured),
            Self::SpiderEyes => return own("gbuffers_spidereyes", Self::Textured),
            Self::BeaconBeam => return own("gbuffers_beaconbeam", Self::Textured),
            Self::Hand => return own("gbuffers_hand", Self::TexturedLit),
            Self::HandWater => return own("gbuffers_hand_water", Self::Hand),
            Self::Weather => return own("gbuffers_weather", Self::TexturedLit),
            Self::Prepare(n) | Self::Deferred(n) | Self::Composite(n) => {
                let base = match self {
                    Self::Prepare(_) => "prepare",
                    Self::Deferred(_) => "deferred",
                    _ => "composite",
                };
                return vec![if n == 0 {
                    base.to_string()
                } else {
                    format!("{base}{n}")
                }];
            }
            Self::Final => &["final"],
        };
        chain.iter().map(|s| s.to_string()).collect()
    }

    /// The geometry the program draws.
    pub fn geometry(self) -> host::Geometry {
        match self {
            Self::Terrain
            | Self::TerrainSolid
            | Self::TerrainCutout
            | Self::Water
            | Self::Shadow
            | Self::ShadowSolid
            | Self::ShadowCutout
            | Self::ShadowWater => host::Geometry::Terrain,
            Self::Prepare(_) | Self::Deferred(_) | Self::Composite(_) | Self::Final => {
                host::Geometry::Fullscreen
            }
            _ => host::Geometry::Entity,
        }
    }

    /// Every slot, in frame-independent order: the gbuffers and shadow
    /// slots (older ones first, then the split ones), `prepare..prepare99`,
    /// `deferred..deferred99`, `composite..composite99`, `final`.
    pub fn all() -> Vec<ProgramKind> {
        let mut all = Self::GEOMETRY.to_vec();
        all.extend(Self::SPLIT);
        all.extend((0..=MAX_PASS).map(Self::Prepare));
        all.extend((0..=MAX_PASS).map(Self::Deferred));
        all.extend((0..=MAX_PASS).map(Self::Composite));
        all.push(Self::Final);
        all
    }

    /// The slot's program name (`gbuffers_terrain`, `shadow_entities`, `composite3`).
    pub fn name(self) -> String {
        self.candidates().swap_remove(0)
    }

    /// The slot named `name` (`gbuffers_water`, `composite12`, `shadow`), if any.
    pub fn from_name(name: &str) -> Option<ProgramKind> {
        for (base, make) in [
            ("prepare", Self::Prepare as fn(u8) -> Self),
            ("deferred", Self::Deferred),
            ("composite", Self::Composite),
        ] {
            if let Some(rest) = name.strip_prefix(base) {
                if rest.is_empty() {
                    return Some(make(0));
                }
                if rest.starts_with('0') {
                    return None;
                }
                return rest.parse::<u8>().ok().filter(|&n| n <= MAX_PASS).map(make);
            }
        }
        Self::GEOMETRY
            .into_iter()
            .chain(Self::SPLIT)
            .chain([Self::Final])
            .find(|k| k.name() == name)
    }

    /// Whether the frame cannot run without this slot's program when the
    /// pack has one: when every candidate of its chain fails, the pack fails.
    /// `final` is not: without it colortex0 goes to the screen, as OptiFine
    /// shows colortex0 for a pack that has no `final`.
    pub fn is_essential(self) -> bool {
        matches!(self, Self::Terrain | Self::Water | Self::Composite(_))
    }

    /// Whether the program draws into the shadow map.
    pub fn is_shadow(self) -> bool {
        matches!(
            self,
            Self::Shadow
                | Self::ShadowEntities
                | Self::ShadowSolid
                | Self::ShadowCutout
                | Self::ShadowWater
                | Self::ShadowBlock
        )
    }

    /// Whether the program is a full-screen pass (`prepare`, `deferred`, `composite`, `final`).
    pub fn is_fullscreen(self) -> bool {
        self.geometry() == host::Geometry::Fullscreen
    }
}

/// The world folder a dimension's programs come from, before `shaders/`.
pub fn world_folder(files: &PackFiles, dimension: &str) -> String {
    properties::dimension_folder(files, dimension)
}

/// The pack path (without extension) of the program `kind` runs, looking in
/// the world folder first and `shaders/` second, best candidate first.
pub fn find_program(files: &PackFiles, folder: &str, kind: ProgramKind) -> Option<String> {
    find_candidates(files, folder, kind, &|_| true)
        .into_iter()
        .next()
}

/// Every pack path (without extension) `kind` may run, best first: each
/// candidate name in the world folder, then in `shaders/`, keeping those with
/// both stages present and for which `usable` holds (not disabled by
/// `shaders.properties`, not excluded). A split slot ([`ProgramKind::parent`])
/// keeps only files of its own name: its parent draws its geometry otherwise.
pub fn find_candidates(
    files: &PackFiles,
    folder: &str,
    kind: ProgramKind,
    usable: &dyn Fn(&str) -> bool,
) -> Vec<String> {
    let dirs = if folder.is_empty() {
        vec!["shaders/".to_string()]
    } else {
        vec![format!("shaders/{folder}/"), "shaders/".to_string()]
    };
    let mut names = kind.candidates();
    if kind.parent().is_some() {
        names.truncate(1);
    }
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for name in names {
        for dir in &dirs {
            let path = format!("{dir}{name}");
            if usable(&path)
                && files.contains(&format!("{path}.vsh"))
                && files.contains(&format!("{path}.fsh"))
                && seen.insert(path.clone())
            {
                out.push(path);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ProgramKind as K;

    #[test]
    fn documented_chains() {
        assert_eq!(
            K::HandWater.candidates(),
            [
                "gbuffers_hand_water",
                "gbuffers_hand",
                "gbuffers_textured_lit",
                "gbuffers_textured",
                "gbuffers_basic"
            ]
        );
        assert_eq!(
            K::ParticlesTranslucent.candidates()[..3],
            [
                "gbuffers_particles_translucent",
                "gbuffers_particles",
                "gbuffers_textured_lit"
            ]
        );
        assert_eq!(
            K::ShadowBlock.candidates(),
            ["shadow_block", "shadow_entities", "shadow"]
        );
        assert_eq!(K::Clouds.candidates()[1], "gbuffers_textured");
        assert_eq!(K::Lightning.candidates()[1], "gbuffers_entities");
        assert_eq!(K::Line.candidates(), ["gbuffers_line", "gbuffers_basic"]);
        assert_eq!(K::Composite(99).name(), "composite99");
        assert_eq!(K::Prepare(0).name(), "prepare");
        for k in K::all() {
            assert_eq!(K::from_name(&k.name()), Some(k), "{k:?}");
            assert_eq!(k.parent().is_some(), K::SPLIT.contains(&k), "{k:?}");
        }
        assert_eq!(K::from_name("composite100"), None);
        assert_eq!(K::from_name("composite07"), None);
        assert_eq!(K::from_name("gbuffers_item"), None);
        // Full-screen passes sort in the order they run.
        let mut passes = vec![
            K::Final,
            K::Composite(10),
            K::Deferred(2),
            K::Prepare(1),
            K::Composite(2),
        ];
        passes.sort();
        assert_eq!(
            passes,
            [
                K::Prepare(1),
                K::Deferred(2),
                K::Composite(2),
                K::Composite(10),
                K::Final
            ]
        );
    }

    #[test]
    fn candidates_in_world_folders_and_split_slots() {
        let files = PackFiles(
            [
                "shaders/gbuffers_textured",
                "shaders/world0/gbuffers_textured",
                "shaders/gbuffers_basic",
                "shaders/gbuffers_hand",
            ]
            .iter()
            .flat_map(|p| [format!("{p}.vsh"), format!("{p}.fsh")])
            .map(|p| (p, Vec::new()))
            .collect(),
        );
        assert_eq!(
            find_candidates(&files, "world0", K::Clouds, &|_| true),
            [
                "shaders/world0/gbuffers_textured",
                "shaders/gbuffers_textured",
                "shaders/gbuffers_basic"
            ]
        );
        assert_eq!(
            find_candidates(&files, "world0", K::Clouds, &|p| !p.contains("world0")),
            ["shaders/gbuffers_textured", "shaders/gbuffers_basic"]
        );
        // A split slot without its own file: its parent draws it.
        assert!(find_candidates(&files, "", K::BeaconBeam, &|_| true).is_empty());
        assert_eq!(
            find_program(&files, "", K::HandWater).as_deref(),
            Some("shaders/gbuffers_hand")
        );
    }
}
