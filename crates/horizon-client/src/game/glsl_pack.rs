//! `--shader-pack PATH`: an OptiFine-format shader pack (zip or folder) that
//! draws the overworld's terrain and the final image (a proof of concept,
//! docs/shader-packs.md). Off unless the argument is given; nothing is saved.

use super::{Dim, Game, PackResources};
use horizon_client::gfx::{PackFrame, Renderer};
use horizon_core::fs::{self, PathExt as _};
use horizon_glsl_pack::{PackFiles, PackOptions, ProgramKind};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The pack's path, from `--shader-pack`.
static PATH: OnceLock<PathBuf> = OnceLock::new();
/// The pack's files, read once (a renderer is rebuilt on every resource reload).
static FILES: OnceLock<Result<PackFiles, String>> = OnceLock::new();

pub(super) fn set_path(path: PathBuf) {
    let _ = PATH.set(path);
}

/// Every file of a folder or zip, by its path inside the pack.
fn read_files(path: &Path) -> Result<PackFiles, String> {
    let mut files = PackFiles::default();
    if path.is_dir_vfs() {
        let mut stack = vec![path.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
                let entry = entry.map_err(|e| e.to_string())?.path();
                if entry.is_dir_vfs() {
                    stack.push(entry);
                } else if let Ok(name) = entry.strip_prefix(path) {
                    let name = name.to_string_lossy().replace('\\', "/");
                    let bytes = fs::read(&entry).map_err(|e| format!("{name}: {e}"))?;
                    files.0.insert(name, bytes);
                }
            }
        }
    } else {
        let zip =
            horizon_client::zip::Zip::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        for name in zip.names() {
            let bytes = zip.read(name).map_err(|e| format!("{name}: {e}"))?;
            files.0.insert(name.to_string(), bytes);
        }
    }
    // Packs zipped from their folder keep it as the top level.
    if !files.0.keys().any(|k| k.starts_with("shaders/"))
        && let Some(top) = files
            .0
            .keys()
            .find_map(|k| k.split_once("/shaders/").map(|(t, _)| t.to_string()))
    {
        let prefix = format!("{top}/");
        files.0 = std::mem::take(&mut files.0)
            .into_iter()
            .filter_map(|(k, v)| Some((k.strip_prefix(&prefix)?.to_string(), v)))
            .collect();
    }
    Ok(files)
}

/// Compiles the pack for `renderer` and installs it, with the mesher's vertex
/// tags; a pack that fails leaves Horizon's own shaders (and says why).
pub(super) fn install(renderer: &mut Renderer, pack: &PackResources) {
    let Some(path) = PATH.get() else {
        return;
    };
    let files = match FILES.get_or_init(|| read_files(path)) {
        Ok(files) => files,
        Err(e) => {
            log::warn!("Horizon: shader pack {}: {e}", path.display());
            return;
        }
    };
    let options = PackOptions {
        // Shadows need a shadow pass this proof of concept does not have yet.
        overrides: vec![("ENABLE_SHADOWS".into(), None)],
        ..Default::default()
    };
    let kinds = [ProgramKind::Terrain, ProgramKind::Water, ProgramKind::Final];
    let started = web_time::Instant::now();
    let compiled = horizon_glsl_pack::compile(
        files,
        "minecraft:overworld",
        &kinds,
        &options,
        &renderer.shader_pack_host(),
    )
    .map_err(|e| e.to_string())
    .and_then(|compiled| renderer.set_shader_pack(Some(&compiled)).map(|()| compiled));
    match compiled {
        Ok(compiled) => {
            pack.context
                .tables
                .set_pack_tags(horizon_client::mesher::PackTags::new(
                    pack.assets.registry,
                    &compiled.blocks,
                ));
            log::info!(
                "Horizon: shader pack {} ready in {} ms",
                path.display(),
                started.elapsed().as_millis()
            );
        }
        Err(e) => log::warn!("Horizon: shader pack {} not used: {e}", path.display()),
    }
}

impl Game<'_> {
    /// The frame's state for the pack's uniforms; `None` draws with
    /// Horizon's shaders (no pack, another dimension, the title screen).
    pub(super) fn pack_frame(&self, eye: [f64; 3], title: bool) -> Option<PackFrame> {
        if PATH.get().is_none() || title || self.dim != Dim::Overworld {
            return None;
        }
        let packed = self.world.light(
            eye[0].floor() as i32,
            eye[1].floor() as i32,
            eye[2].floor() as i32,
        );
        let day = self.world.day_time;
        let (rain0, rain1) = self.world.weather.rain_level;
        Some(PackFrame {
            world_time: day.rem_euclid(24000) as i32,
            world_day: day.div_euclid(24000) as i32,
            moon_phase: day.div_euclid(24000).rem_euclid(8) as i32,
            rain_strength: rain0 + (rain1 - rain0) * 0.5,
            eye_brightness: [i32::from(packed & 15) * 16, i32::from(packed >> 4) * 16],
            held_light: [0, 0],
            night_vision: self.night_vision_intensity(),
            blindness: f32::from(u8::from(self.potions.effects.get("blindness").is_some())),
            hide_gui: self.ui.hide_gui,
        })
    }
}
