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
            ..self.pack_player_state(eye)
        })
    }

    /// The biome, weather and player parts of [`Self::pack_frame`]: a few
    /// field reads and one ray along the view.
    fn pack_player_state(&self, eye: [f64; 3]) -> PackFrame {
        use horizon_glsl_pack::custom_uniforms::{biome_category, biome_id};
        let mut f = PackFrame::default();
        let [x, y, z] = eye.map(|v| v.floor() as i32);
        if let Some(chunk) = self.world.chunks.get(&(x >> 4, z >> 4)) {
            let info = &self.res.biomes().list[chunk.biome(x, y, z) as usize];
            f.biome = biome_id(&info.name);
            f.biome_category = biome_category(&info.name);
            // Biome.getPrecipitationAt without the height adjustment.
            f.biome_precipitation = match (info.precipitation, info.temperature < 0.15) {
                (false, _) => 0,
                (true, false) => 1,
                (true, true) => 2,
            };
            f.temperature = info.temperature;
            f.rainfall = info.downfall;
        }
        let partial = self.clock.fraction() as f32;
        f.thunder_strength = self.world.weather.levels(partial).1;
        f.mood = self.ambience.moodiness.clamp(0.0, 1.0);
        f.darkness = [
            self.darkness_fog().unwrap_or(0.0),
            self.darkness_lighting(partial).1,
        ];
        if let Some(bolt) = self.bolts.first() {
            let at = std::array::from_fn::<f32, 3, _>(|i| (bolt.at[i] - eye[i]) as f32);
            f.lightning = [at[0], at[1], at[2], 1.0];
        }
        // The first block along the view, as far as the shadows reach at most.
        let dir = super::view_vector(self.yaw, self.pitch);
        f.center_distance = self.clip_ray(eye, dir, 256.0).map_or(f32::INFINITY, |hit| {
            let d: f64 = (0..3).map(|i| (hit.point[i] - eye[i]) * dir[i]).sum();
            d as f32
        });
        f.spectator = self.spectator();
        let unbounded = self.creative || f.spectator;
        f.health = [if unbounded { -1.0 } else { 0.0 }, 20.0];
        f.hunger = [if unbounded { -1.0 } else { 0.0 }, 20.0];
        f.air = [if unbounded { -1.0 } else { 0.0 }, 300.0];
        if let Some(p) = &self.player {
            // Health boost adds 4 per level to the base 20 (the attribute
            // needs a mutable borrow; nothing else changes it in vanilla).
            let boost = self
                .potions
                .effects
                .get("health_boost")
                .map_or(0.0, |e| 4.0 * (e.amplifier + 1) as f32);
            f.health[1] = 20.0 + boost;
            if !unbounded {
                f.health[0] = (p.health.health() / f.health[1]).clamp(0.0, 1.0);
                f.hunger[0] = p.health.food.food as f32 / 20.0;
                f.air[0] = (p.air as f32 / 300.0).clamp(0.0, 1.0);
            }
            f.sneaking = p.pose == horizon_client::pose::Pose::Crouching;
            f.sprinting = p.sprinting;
            f.hurt = p.health.hurt.hurt_time > 0;
            f.burning = p.fire_ticks > 0;
            f.on_ground = p.ground.on_ground && !p.flying;
        }
        f.invisible = f.spectator || self.potions.effects.get("invisibility").is_some();
        f
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
