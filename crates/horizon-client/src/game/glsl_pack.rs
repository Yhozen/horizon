//! `--shader-pack PATH`: an OptiFine-format shader pack (zip or folder) that
//! draws the overworld (docs/shader-packs.md), with `--shader-pack-option
//! NAME=VALUE` to change its options. Off unless the argument is given;
//! nothing is saved.

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
/// `--shader-pack-option NAME=VALUE` (repeatable), in order.
static OPTIONS: std::sync::Mutex<Vec<(String, Option<String>)>> = std::sync::Mutex::new(Vec::new());

#[derive(Default)]
pub(super) struct Args {
    pub path: Option<PathBuf>,
    pub overrides: Vec<String>,
}

impl Args {
    pub(super) fn apply(self) {
        if let Some(path) = self.path {
            let _ = PATH.set(path);
        }
        for arg in self.overrides {
            add_option(&arg);
        }
    }
}

pub(super) fn requested() -> bool {
    PATH.get().is_some()
}

/// `NAME=VALUE` (a boolean option takes `true` or `false`); `NAME` alone turns it on.
fn add_option(arg: &str) {
    let (name, value) = match arg.split_once('=') {
        Some((n, v)) => (n.trim().to_string(), v.trim().to_string()),
        None => (arg.trim().to_string(), "true".to_string()),
    };
    OPTIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((name, Some(value)));
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
    let started = web_time::Instant::now();
    let mut host = renderer.shader_pack_host();
    let mut try_compile = |shadows: bool| {
        let mut options = PackOptions::default();
        options.overrides.extend(
            OPTIONS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .cloned(),
        );
        if !shadows {
            // Without a shadow program the pack must not read a shadow map.
            options.overrides.push(("ENABLE_SHADOWS".into(), None));
        }
        host.max_varyings = renderer.max_varyings();
        horizon_glsl_pack::compile(files, "minecraft:overworld", &options, &host)
            .map_err(|e| e.to_string())
            .and_then(|mut compiled| {
                if !shadows {
                    compiled.programs.remove(&ProgramKind::Shadow);
                    compiled.programs.remove(&ProgramKind::ShadowEntities);
                    compiled.plan.programs.remove(&ProgramKind::Shadow);
                    compiled.plan.programs.remove(&ProgramKind::ShadowEntities);
                }
                for (kind, why) in &compiled.skipped {
                    log::warn!("Horizon: shader pack {kind:?} skipped: {why}");
                }
                renderer
                    .set_shader_pack(Some((&compiled, files)))
                    .map(|()| compiled)
            })
    };
    let shadows = std::env::var("HORIZON_SHADER_PACK_SHADOWS").map_or(true, |v| v != "0");
    let compiled = match try_compile(shadows) {
        Ok(compiled) => Ok(compiled),
        Err(e) if shadows => {
            log::warn!("Horizon: shader pack with shadows failed ({e}); trying without");
            try_compile(false)
        }
        Err(e) => Err(e),
    };
    match compiled {
        Ok(compiled) => {
            pack.context
                .tables
                .set_pack_tags(horizon_client::mesher::PackTags::new(
                    pack.assets.registry,
                    &compiled.blocks,
                    &compiled.plan,
                ));
            log::info!(
                "Horizon: shader pack {} ready in {} ms ({} programs)",
                path.display(),
                started.elapsed().as_millis(),
                compiled.programs.len()
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
            held_light: self.held_light_levels(),
            night_vision: self.night_vision_intensity(),
            blindness: f32::from(u8::from(self.potions.effects.get("blindness").is_some())),
            hide_gui: self.ui.hide_gui,
            eye_in_fluid: match self.camera_fluid(eye) {
                horizon_client::camera::CameraFluid::None => 0,
                horizon_client::camera::CameraFluid::Water => 1,
                horizon_client::camera::CameraFluid::Lava => 2,
                horizon_client::camera::CameraFluid::PowderSnow => 3,
            },
        })
    }
}

impl Game<'_> {
    /// The light the main and off hand's items give (`heldBlockLightValue`, `…2`).
    fn held_light_levels(&self) -> [i32; 2] {
        let Some(lum) = &self.luminance else {
            return [0, 0];
        };
        let held = |slot| {
            self.stack_in(slot)
                .map_or(0, |s| i32::from(lum.of(&s.item)))
        };
        [held(self.selected), held(super::shield::OFFHAND_SLOT)]
    }

    /// Builds the item light table the pack's held light reads, once.
    pub(super) fn prepare_pack_frame(&mut self) {
        if PATH.get().is_some() && self.luminance.is_none() {
            self.luminance = Some(horizon_client::dynamic_light::Luminance::new(
                self.res.facts(),
                self.pack.assets.registry,
            ));
        }
    }
}
