//! Iris's render stages: the value of the `renderStage` uniform for each kind
//! of draw, numbered as the `MC_RENDER_STAGE_*` macros
//! ([`crate::preprocess::RENDER_STAGES`]).

use crate::ProgramKind;

/// What a draw renders, in the order of the public docs (`NONE` = 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum RenderStage {
    None,
    Sky,
    Sunset,
    CustomSky,
    Sun,
    Moon,
    Stars,
    Void,
    TerrainSolid,
    TerrainCutoutMipped,
    TerrainCutout,
    Entities,
    BlockEntities,
    Destroy,
    Outline,
    Debug,
    HandSolid,
    TerrainTranslucent,
    Tripwire,
    Particles,
    Clouds,
    RainSnow,
    WorldBorder,
    HandTranslucent,
}

impl RenderStage {
    /// The `renderStage` uniform's value (`MC_RENDER_STAGE_<NAME>`).
    pub fn value(self) -> i32 {
        self as i32
    }

    /// The stage of a draw through `kind`'s program when nothing more precise
    /// is known: the sky dome is `Sky` (a draw of the sunset fan, stars or
    /// void should say so), `SkyTextured` is `Sun` (the moon draw: `Moon`),
    /// `Textured` is `WorldBorder` (its only user in Horizon), cutout terrain
    /// is `TerrainSolid` as in Iris, shadow slots the stage of what they
    /// draw, full-screen passes and the glint `None`.
    pub fn of_slot(kind: ProgramKind) -> Self {
        match kind {
            ProgramKind::SkyBasic => Self::Sky,
            ProgramKind::SkyTextured => Self::Sun,
            ProgramKind::Clouds => Self::Clouds,
            ProgramKind::Terrain => Self::TerrainSolid,
            ProgramKind::Water => Self::TerrainTranslucent,
            ProgramKind::Block => Self::BlockEntities,
            ProgramKind::Entities => Self::Entities,
            ProgramKind::TexturedLit => Self::Particles,
            ProgramKind::Textured => Self::WorldBorder,
            ProgramKind::Basic => Self::Outline,
            ProgramKind::Hand => Self::HandSolid,
            ProgramKind::HandWater => Self::HandTranslucent,
            ProgramKind::Weather => Self::RainSnow,
            ProgramKind::DamagedBlock => Self::Destroy,
            ProgramKind::Line => Self::Outline,
            ProgramKind::BeaconBeam | ProgramKind::BlockTranslucent => Self::BlockEntities,
            ProgramKind::SpiderEyes
            | ProgramKind::EntitiesGlowing
            | ProgramKind::EntitiesTranslucent
            | ProgramKind::Lightning => Self::Entities,
            ProgramKind::Particles | ProgramKind::ParticlesTranslucent => Self::Particles,
            ProgramKind::TerrainSolid | ProgramKind::TerrainCutout => Self::TerrainSolid,
            // The shadow pass draws the same geometry with the same stages.
            ProgramKind::Shadow | ProgramKind::ShadowSolid | ProgramKind::ShadowCutout => {
                Self::TerrainSolid
            }
            ProgramKind::ShadowWater => Self::TerrainTranslucent,
            ProgramKind::ShadowEntities => Self::Entities,
            ProgramKind::ShadowBlock => Self::BlockEntities,
            _ => Self::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preprocess::{RENDER_STAGES, predefined};

    #[test]
    fn values_match_the_macros() {
        let macros = predefined(260300);
        assert_eq!(
            RenderStage::HandTranslucent.value() as usize,
            RENDER_STAGES.len() - 1
        );
        for (stage, name) in [
            (RenderStage::None, "NONE"),
            (RenderStage::Sun, "SUN"),
            (RenderStage::TerrainTranslucent, "TERRAIN_TRANSLUCENT"),
            (RenderStage::HandTranslucent, "HAND_TRANSLUCENT"),
        ] {
            assert_eq!(
                macros[&format!("MC_RENDER_STAGE_{name}")],
                stage.value().to_string()
            );
        }
        assert_eq!(
            RenderStage::of_slot(ProgramKind::Water),
            RenderStage::TerrainTranslucent
        );
        assert_eq!(RenderStage::of_slot(ProgramKind::Final), RenderStage::None);
    }
}
