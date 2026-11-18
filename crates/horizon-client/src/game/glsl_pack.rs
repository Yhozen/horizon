//! The shader pack's per-frame inputs, and `--shader-pack PATH` /
//! `--shader-pack-option NAME=VALUE` (a pack for this run, not saved; the
//! manager is `game/shader_packs.rs`).

use super::Game;
use horizon_client::gfx::PackFrame;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The pack's path, from `--shader-pack`.
static PATH: OnceLock<PathBuf> = OnceLock::new();
/// `--shader-pack-option NAME=VALUE` (repeatable), in order.
static OPTIONS: std::sync::Mutex<Vec<(String, Option<String>)>> = std::sync::Mutex::new(Vec::new());

pub(super) fn set_path(path: PathBuf) {
    let _ = PATH.set(path);
}

/// `NAME=VALUE` (a boolean option takes `true` or `false`); `NAME` alone turns it on.
pub(super) fn add_option(arg: &str) {
    let (name, value) = match arg.split_once('=') {
        Some((n, v)) => (n.trim().to_string(), v.trim().to_string()),
        None => (arg.trim().to_string(), "true".to_string()),
    };
    OPTIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((name, Some(value)));
}

/// The `--shader-pack` path and options given on the command line.
pub(super) fn cli_pack() -> (Option<PathBuf>, Vec<(String, String)>) {
    let options = OPTIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|(k, v)| (k.clone(), v.clone().unwrap_or_else(|| "true".into())))
        .collect();
    (PATH.get().cloned(), options)
}

impl Game<'_> {
    /// The frame's state for the pack's uniforms; `None` draws with
    /// Horizon's shaders (no pack, another dimension, the title screen).
    pub(super) fn pack_frame(&self, eye: [f64; 3], title: bool) -> Option<PackFrame> {
        if !self.shader_pack_draws(title) {
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

    /// Keeps the running pack in step with the selection, then builds the
    /// item light table the pack's held light reads, once.
    pub(super) fn prepare_pack_frame(&mut self, title: bool) {
        self.tick_shader_packs(title);
        if self.shader_packs.applied.is_some() && self.luminance.is_none() {
            self.luminance = Some(horizon_client::dynamic_light::Luminance::new(
                self.res.facts(),
                self.pack.assets.registry,
            ));
        }
    }
}
