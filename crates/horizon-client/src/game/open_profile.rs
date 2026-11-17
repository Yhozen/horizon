//! Horizon Open Creative's bounds (docs/open-creative.md). The resources'
//! manifest names the profile; with the open bundle every world uses the open
//! terrain preset in the overworld, single player runs in the frame, and a
//! server or player only meets others on the same bundle (`assets::server_brand`).
//! These checks sit where a run is configured and where a player could leave
//! that scope, so the rest of the game needs none.
use super::*;
use horizon_core::content_profile::{ContentProfile, WORLD_MARKER};

/// Read the resources' profile into `o` and hold a run to it: arguments that
/// would leave the open edition's scope (content it does not have) are errors,
/// not silently ignored.
pub(super) fn apply(o: &mut Options) -> Result<()> {
    // Missing or unreadable resources fail later with their own message.
    o.profile = match horizon_client::assets::profile_at(&o.resources) {
        Ok(profile) => profile,
        Err(e) if o.resources.join("horizon-resources.json").exists_vfs() => return Err(e),
        Err(_) => ContentProfile::Vanilla,
    };
    if !o.profile.is_open() {
        return Ok(());
    }
    let unavailable =
        |what: &str| -> Result<()> { Err(format!("Horizon Open Creative has no {what}").into()) };
    if o.benchmark.is_some() {
        unavailable("benchmark scenes (--benchmark)")?;
    }
    if !o.dev_mods.is_empty() {
        unavailable("mods (--dev-mod)")?;
    }
    if o.weather.is_some() {
        unavailable("weather (--weather)")?;
    }
    if o.dimension
        .as_deref()
        .is_some_and(|d| d != "minecraft:overworld")
    {
        unavailable("other dimensions (--dimension)")?;
    }
    verify_bundle(&o.resources)?;
    // The open terrain, with single player's world in the frame.
    o.server_thread = false;
    o.test_terrain = true;
    Ok(())
}

/// Every file the bundle's provenance record lists is present with its
/// recorded checksum: a damaged or partial bundle stops here with what to do,
/// rather than loading with something missing.
fn verify_bundle(root: &std::path::Path) -> Result<()> {
    use sha2::Digest;
    let fix = "prepare them again with: horizon-prepare open --out PATH";
    let record: serde_json::Value = serde_json::from_slice(
        &horizon_core::fs::read(root.join("horizon-open-provenance.json")).map_err(|e| {
            format!("Horizon Open Creative resources have no provenance record ({e}); {fix}")
        })?,
    )
    .map_err(|e| format!("Horizon Open Creative's provenance record is unreadable ({e}); {fix}"))?;
    let files = record["files"]
        .as_object()
        .filter(|f| !f.is_empty())
        .ok_or_else(|| {
            format!("Horizon Open Creative's provenance record lists no files; {fix}")
        })?;
    // Files a browser player laid their own art over (`web::overlay_art`): the
    // bundle's data and numbering are untouched, its look is theirs.
    let overlaid: std::collections::HashSet<String> =
        horizon_core::fs::read(root.join("horizon-overlay.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
    for (path, entry) in files {
        if overlaid.contains(path) {
            continue;
        }
        let bytes = horizon_core::fs::read(root.join(path))
            .map_err(|_| format!("Horizon Open Creative resources are missing {path}; {fix}"))?;
        let digest: String = sha2::Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if entry["sha256"].as_str() != Some(digest.as_str()) {
            return Err(
                format!("Horizon Open Creative resources have a damaged {path}; {fix}").into(),
            );
        }
    }
    Ok(())
}

/// The open bundle's starting hotbar (`starter_hotbar` in its manifest).
pub(super) fn starter_hotbar(o: &Options) -> Option<Vec<String>> {
    if !o.profile.is_open() {
        return None;
    }
    let manifest: serde_json::Value = serde_json::from_slice(
        &horizon_core::fs::read(o.resources.join("horizon-resources.json")).ok()?,
    )
    .ok()?;
    Some(
        manifest["starter_hotbar"]
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
    )
}

/// What a player could try that Horizon Open Creative does not offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Scope {
    OtherDimensions,
    Commands,
}
impl Scope {
    fn describe(self) -> &'static str {
        match self {
            Self::OtherDimensions => "Horizon Open Creative has the overworld only",
            Self::Commands => "Horizon Open Creative does not have that command",
        }
    }
}
/// Commands the open edition keeps: those that only use what its bundle has.
const OPEN_COMMANDS: &[&str] = &[
    "clear",
    "clone",
    "defaultgamemode",
    "difficulty",
    "experience",
    "fill",
    "gamemode",
    "give",
    "help",
    "kill",
    "list",
    "me",
    "msg",
    "say",
    "seed",
    "setblock",
    "setworldspawn",
    "spawnpoint",
    "teleport",
    "tell",
    "time",
    "tp",
    "w",
    "xp",
];

/// The chat's command tree in the open edition: the vanilla tree cut to
/// `OPEN_COMMANDS`, so suggestions offer nothing the edition refuses.
pub(super) fn command_tree() -> &'static horizon_client::command::Dispatcher {
    static TREE: std::sync::OnceLock<horizon_client::command::Dispatcher> =
        std::sync::OnceLock::new();
    TREE.get_or_init(|| {
        horizon_client::command::vanilla::game_dispatcher().with_root_commands(OPEN_COMMANDS)
    })
}

impl Game<'_> {
    /// Whether the run may do `scope`; with the open bundle it may not, and
    /// the player is told so.
    pub(super) fn open_allows(&mut self, scope: Scope) -> bool {
        if !self.options.profile.is_open() {
            return true;
        }
        log::info!("Horizon: refused: {}", scope.describe());
        self.system_message(horizon_client::text::Text::literal(scope.describe()));
        false
    }
    /// The open edition runs a short list of commands (`OPEN_COMMANDS`).
    pub(super) fn open_allows_command(&mut self, command: &str) -> bool {
        let name = command.split_whitespace().next().unwrap_or("");
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        OPEN_COMMANDS.contains(&name) || self.open_allows(Scope::Commands)
    }
}

/// The game rules that keep unsupported simulation off in an open world: no
/// weather, mobs, random block changes, fire or source-water creation.
pub(super) fn hold_rules(rules: &mut horizon_client::gamerules::GameRules) {
    use horizon_client::gamerules::Value;
    for (rule, value) in [
        ("advance_weather", Value::Bool(false)),
        ("spawn_mobs", Value::Bool(false)),
        ("spawn_monsters", Value::Bool(false)),
        ("spawn_patrols", Value::Bool(false)),
        ("spawn_phantoms", Value::Bool(false)),
        ("spawn_wandering_traders", Value::Bool(false)),
        ("spawner_blocks_work", Value::Bool(false)),
        ("raids", Value::Bool(false)),
        ("random_tick_speed", Value::Int(0)),
        ("fire_spread_radius_around_player", Value::Int(0)),
        ("water_source_conversion", Value::Bool(false)),
        ("command_blocks_work", Value::Bool(false)),
    ] {
        rules.set(rule, value);
    }
}

/// The marker a world made with `profile` carries (`horizon-content.json`).
fn marker(profile: ContentProfile) -> String {
    format!(
        "{{\"content_profile\":\"{}\",\"content_revision\":{}}}\n",
        profile.id(),
        horizon_core::content_profile::OPEN_CONTENT_REVISION
    )
}

/// Check a world folder against the resources before it is read or written:
/// an open world opens only with the open bundle, and the open bundle opens
/// only worlds it made (a new folder becomes one). Vanilla worlds have no marker.
pub(super) fn check_world(profile: ContentProfile, world: &std::path::Path) -> Result<()> {
    let path = world.join(WORLD_MARKER);
    let recorded = horizon_core::fs::read(&path).ok().and_then(|bytes| {
        serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|v| v["content_profile"].as_str().map(str::to_owned))
    });
    let existing = world.join("level.dat").exists_vfs() || world.join("region").exists_vfs();
    match (profile.is_open(), recorded.as_deref()) {
        (true, Some(id)) if id == profile.id() => Ok(()),
        (true, None) if !existing => {
            horizon_core::fs::create_dir_all(world)?;
            horizon_core::fs::write(&path, marker(profile))?;
            Ok(())
        }
        (true, _) => Err(format!(
            "{} was not made with Horizon Open Creative; it cannot open here",
            world.display()
        )
        .into()),
        (false, Some(id)) if id != ContentProfile::Vanilla.id() => Err(format!(
            "{} is a Horizon Open Creative world; open it with the open resources",
            world.display()
        )
        .into()),
        (false, _) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worlds_open_only_with_the_content_they_were_made_with() {
        let dir = std::env::temp_dir().join(format!("horizon-open-world-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let open = ContentProfile::OpenCreative;
        // A new folder becomes an open world.
        check_world(open, &dir.join("new")).unwrap();
        assert!(dir.join("new").join(WORLD_MARKER).exists());
        check_world(open, &dir.join("new")).unwrap();
        assert!(check_world(ContentProfile::Vanilla, &dir.join("new")).is_err());
        // A vanilla world (no marker, a level.dat) never opens with the open bundle.
        std::fs::create_dir_all(dir.join("vanilla")).unwrap();
        std::fs::write(dir.join("vanilla/level.dat"), b"").unwrap();
        assert!(check_world(open, &dir.join("vanilla")).is_err());
        assert!(!dir.join("vanilla").join(WORLD_MARKER).exists());
        check_world(ContentProfile::Vanilla, &dir.join("vanilla")).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
