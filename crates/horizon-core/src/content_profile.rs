//! Which content a resource folder holds: the player's own vanilla data, or
//! Horizon Open Creative's independently authored bundle (`horizon-prepare
//! open`, docs/open-creative.md).
//!
//! `horizon-resources.json` names the profile. A manifest without one is
//! vanilla, as every folder `horizon-prepare` made before profiles existed.

/// The manifest key naming the profile.
pub const MANIFEST_KEY: &str = "content_profile";
/// The manifest key holding the open bundle's own content revision; it is not
/// Minecraft's data version and is never compared with it.
pub const REVISION_KEY: &str = "content_revision";
/// The open content revision this build reads. Raise it when the bundle's
/// layout or meaning changes; `horizon-prepare open` writes the same value.
pub const OPEN_CONTENT_REVISION: i64 = 1;
/// The world-folder marker naming the content a world was made with
/// (`horizon-content.json`). Vanilla worlds have none.
pub const WORLD_MARKER: &str = "horizon-content.json";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ContentProfile {
    /// Prepared from the player's own Minecraft install.
    #[default]
    Vanilla,
    /// Horizon Open Creative: open artwork and original data; single player
    /// and open servers in the overworld, no Minecraft compatibility.
    OpenCreative,
}

impl ContentProfile {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::OpenCreative => "open-creative",
        }
    }
    /// The profile a manifest value names; `None` (no key) is vanilla.
    pub fn from_manifest(value: Option<&str>) -> Result<Self, String> {
        match value {
            None | Some("vanilla") => Ok(Self::Vanilla),
            Some("open-creative") => Ok(Self::OpenCreative),
            Some(other) => Err(format!(
                "unknown content profile {other:?} in horizon-resources.json"
            )),
        }
    }
    pub const fn is_open(self) -> bool {
        matches!(self, Self::OpenCreative)
    }
    /// What a player sees for it.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Vanilla => "Minecraft Java Edition resources",
            Self::OpenCreative => "Horizon Open Creative",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_without_a_profile_is_vanilla() {
        assert_eq!(
            ContentProfile::from_manifest(None),
            Ok(ContentProfile::Vanilla)
        );
        assert_eq!(
            ContentProfile::from_manifest(Some("open-creative")),
            Ok(ContentProfile::OpenCreative)
        );
        assert!(ContentProfile::from_manifest(Some("bedrock")).is_err());
        for profile in [ContentProfile::Vanilla, ContentProfile::OpenCreative] {
            assert_eq!(
                ContentProfile::from_manifest(Some(profile.id())),
                Ok(profile)
            );
        }
    }
}
