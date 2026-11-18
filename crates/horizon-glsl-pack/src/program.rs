//! Program slots, their fallback chains, and finding a slot's file in a pack.

use std::collections::BTreeSet;

use crate::{PackFiles, host, properties};

/// A program slot of the frame, by OptiFine's program names. A slot without
/// its own file runs its fallback ([`ProgramKind::candidates`]); a slot with
/// no file at all draws nothing (gbuffers) or is skipped (full-screen passes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProgramKind {
    /// Shadow map, terrain (`shadow`).
    Shadow,
    /// Shadow map, entities and block entities (`shadow`, entity geometry).
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
    /// `deferred`, `deferred1` … `deferred15` (0 is `deferred`).
    Deferred(u8),
    /// `composite`, `composite1` … `composite15` (0 is `composite`).
    Composite(u8),
    /// The last full-screen pass, into the screen (`final`).
    Final,
}

impl ProgramKind {
    /// Every gbuffers and shadow slot, in no particular order.
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

    /// The program file names to try, best first (OptiFine's fallbacks).
    pub fn candidates(self) -> Vec<String> {
        let chain: &[&str] = match self {
            Self::Shadow | Self::ShadowEntities => &["shadow"],
            Self::Basic => &["gbuffers_basic"],
            Self::Textured => &["gbuffers_textured", "gbuffers_basic"],
            Self::TexturedLit => &[
                "gbuffers_textured_lit",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::SkyBasic => &["gbuffers_skybasic", "gbuffers_basic"],
            Self::SkyTextured => &[
                "gbuffers_skytextured",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::Clouds => &["gbuffers_clouds", "gbuffers_textured", "gbuffers_basic"],
            Self::Terrain => &[
                "gbuffers_terrain",
                "gbuffers_textured_lit",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::DamagedBlock | Self::Block | Self::Water => {
                let first = match self {
                    Self::DamagedBlock => "gbuffers_damagedblock",
                    Self::Block => "gbuffers_block",
                    _ => "gbuffers_water",
                };
                return [first]
                    .into_iter()
                    .chain(Self::Terrain.candidates().iter().map(String::as_str))
                    .map(str::to_string)
                    .collect();
            }
            Self::Entities => &[
                "gbuffers_entities",
                "gbuffers_textured_lit",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::ArmorGlint => &[
                "gbuffers_armor_glint",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::Hand => &[
                "gbuffers_hand",
                "gbuffers_textured_lit",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::HandWater => &[
                "gbuffers_hand_water",
                "gbuffers_hand",
                "gbuffers_textured_lit",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::Weather => &[
                "gbuffers_weather",
                "gbuffers_textured_lit",
                "gbuffers_textured",
                "gbuffers_basic",
            ],
            Self::Deferred(n) | Self::Composite(n) => {
                let base = if matches!(self, Self::Deferred(_)) {
                    "deferred"
                } else {
                    "composite"
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
            Self::Terrain | Self::Water | Self::Shadow => host::Geometry::Terrain,
            Self::Deferred(_) | Self::Composite(_) | Self::Final => host::Geometry::Fullscreen,
            _ => host::Geometry::Entity,
        }
    }

    /// Every slot, in frame-independent order: the gbuffers and shadow
    /// slots, `deferred..deferred15`, `composite..composite15`, `final`.
    pub fn all() -> Vec<ProgramKind> {
        let mut all = Self::GEOMETRY.to_vec();
        all.extend((0..16).map(Self::Deferred));
        all.extend((0..16).map(Self::Composite));
        all.push(Self::Final);
        all
    }

    /// The slot's OptiFine program name (`gbuffers_terrain`, `shadow`,
    /// `composite3`); the shadow slot for entities is `shadow_entities`.
    pub fn name(self) -> String {
        match self {
            Self::ShadowEntities => "shadow_entities".to_string(),
            _ => self.candidates().swap_remove(0),
        }
    }

    /// Whether the frame cannot run without this slot's program when the
    /// pack has one: a failure to compile it fails the pack.
    pub fn is_essential(self) -> bool {
        matches!(
            self,
            Self::Terrain | Self::Water | Self::Final | Self::Composite(_)
        )
    }

    /// Whether the program draws into the shadow map.
    pub fn is_shadow(self) -> bool {
        matches!(self, Self::Shadow | Self::ShadowEntities)
    }

    /// Whether the program is a full-screen pass (`deferred`, `composite`, `final`).
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
    find_enabled(files, folder, kind, &BTreeSet::new())
}

/// [`find_program`] skipping the programs `shaders.properties` disables
/// (pack paths without extension).
pub(crate) fn find_enabled(
    files: &PackFiles,
    folder: &str,
    kind: ProgramKind,
    disabled: &BTreeSet<String>,
) -> Option<String> {
    let dirs = if folder.is_empty() {
        vec!["shaders/".to_string()]
    } else {
        vec![format!("shaders/{folder}/"), "shaders/".to_string()]
    };
    for name in kind.candidates() {
        for dir in &dirs {
            let path = format!("{dir}{name}");
            if !disabled.contains(&path)
                && files.contains(&format!("{path}.vsh"))
                && files.contains(&format!("{path}.fsh"))
            {
                return Some(path);
            }
        }
    }
    None
}
