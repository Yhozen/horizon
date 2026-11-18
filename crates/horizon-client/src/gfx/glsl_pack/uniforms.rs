//! The pack's uniform block for a frame: one copy for the camera's passes,
//! one for the shadow map (the light's matrices as the camera's) and one for
//! the hand (its own projection). The maths is `horizon_glsl_pack::frame_math`.

use glam::Mat4;
use horizon_glsl_pack::frame_math::{self, EyeBrightnessSmooth, FrameClock, PreviousFrame};
use horizon_glsl_pack::{PackPlan, PackUniforms};

/// What the uniforms remember between frames.
pub(crate) struct History {
    clock: FrameClock,
    previous: PreviousFrame,
    eye: EyeBrightnessSmooth,
    wetness: Option<f32>,
    start: web_time::Instant,
}

impl Default for History {
    fn default() -> Self {
        Self {
            clock: FrameClock::default(),
            previous: PreviousFrame::default(),
            eye: EyeBrightnessSmooth::default(),
            wetness: None,
            start: web_time::Instant::now(),
        }
    }
}

/// What the uniforms read besides the frame (worked out in `draw_frame`).
pub(crate) struct PackUniformInputs<'a> {
    pub state: &'a crate::gfx::PackFrame,
    /// Horizon's clip matrix (projection × bob × view), camera-relative.
    pub view_proj: Mat4,
    /// The hand's (projection × bob × view).
    pub hand_view_proj: Mat4,
    pub size: [u32; 2],
    /// The brightness option, 0..1 (`screenBrightness`).
    pub gamma: f32,
    pub fog_color: [f32; 4],
    /// Environmental start/end, render-distance start/end (`Globals::fog`).
    pub fog: [f32; 4],
}

/// The three copies of a frame's block.
pub(crate) struct FrameUniforms {
    pub main: PackUniforms,
    pub shadow: PackUniforms,
    pub hand: PackUniforms,
}

/// GL clip space from Horizon's: y flipped (images are stored top row first)
/// and reversed-Z 0..1 depth to GL's -1..1 with 1 - d (`horizon_glsl_pack`'s
/// `hz_to_gl`). Packs reconstruct positions with `gbufferProjectionInverse`,
/// so their projection is this times Horizon's.
pub(crate) fn gl_clip() -> Mat4 {
    Mat4::from_cols(
        glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, -1.0, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, -2.0, 0.0),
        glam::Vec4::new(0.0, 0.0, 1.0, 1.0),
    )
}

/// The hand's depth squeezed as `MC_HAND_DEPTH` (0.125) does to GL's clip z:
/// in reversed Z, d' = 0.4375 + 0.125 d, so the hand reads as GL depth
/// 0.4375..0.5625, nearer than any world surface.
fn hand_depth() -> Mat4 {
    Mat4::from_cols(
        glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, 1.0, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, 0.125, 0.0),
        glam::Vec4::new(0.0, 0.0, 0.4375, 1.0),
    )
}

fn m4(m: Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}

fn glam4(m: [[f32; 4]; 4]) -> Mat4 {
    Mat4::from_cols_array_2d(&m)
}

impl PackUniformInputs<'_> {
    pub fn uniforms(
        &self,
        frame: &crate::gfx::Frame,
        plan: &PackPlan,
        history: &mut History,
    ) -> FrameUniforms {
        let s = self.state;
        // gbufferModelView is the camera's rotation; view bobbing lives in
        // the projection, as in vanilla.
        let model_view = crate::camera::view_matrix(frame.yaw, frame.pitch);
        let projection = gl_clip() * self.view_proj * model_view.inverse();
        let celestial = frame_math::celestial(
            frame.sky.sun_angle,
            frame.sky.moon_angle,
            plan.shadow.sun_path_rotation,
            &m4(model_view),
        );
        let camera = frame.camera.to_array();
        let shadow = frame_math::shadow_camera(camera, celestial.shadow_angle, &plan.shadow);
        let times = history
            .clock
            .advance(history.start.elapsed().as_millis() as u64);
        let dt = times.frame_time;
        let previous = history
            .previous
            .advance(camera, m4(model_view), m4(projection));
        let wetness = match history.wetness {
            Some(w) => frame_math::wetness_step(w, s.rain_strength, dt, plan),
            None => s.rain_strength,
        };
        history.wetness = Some(wetness);
        let eye_smooth = history
            .eye
            .update(s.eye_brightness, dt, plan.eye_brightness_half_life);
        let [w, h] = self.size;
        let main = PackUniforms {
            hz_view_proj: m4(self.view_proj),
            gbuffer_model_view: m4(model_view),
            gbuffer_model_view_inverse: m4(model_view.inverse()),
            gbuffer_projection: m4(projection),
            gbuffer_projection_inverse: m4(projection.inverse()),
            gbuffer_previous_model_view: previous.previous_model_view,
            gbuffer_previous_projection: previous.previous_projection,
            shadow_model_view: shadow.model_view,
            shadow_model_view_inverse: shadow.model_view_inverse,
            shadow_projection: shadow.projection,
            shadow_projection_inverse: shadow.projection_inverse,
            model_view_matrix: m4(model_view),
            projection_matrix: m4(projection),
            camera_position: previous.camera_position,
            previous_camera_position: previous.previous_camera_position,
            eye_altitude: previous.eye_altitude,
            sun_position: celestial.sun_position,
            moon_position: celestial.moon_position,
            shadow_light_position: celestial.shadow_light_position,
            up_position: celestial.up_position,
            fog_color: [self.fog_color[0], self.fog_color[1], self.fog_color[2]],
            sky_color: frame.sky.sky_color,
            entity_color: [0.0; 4],
            eye_brightness: s.eye_brightness,
            eye_brightness_smooth: eye_smooth,
            fog_start: self.fog[0].min(self.fog[2]),
            fog_end: self.fog[1].min(self.fog[3]),
            near: 0.05,
            far: frame.distance,
            rain_strength: s.rain_strength.clamp(0.0, 1.0),
            wetness,
            screen_brightness: self.gamma,
            frame_time: dt,
            frame_time_counter: times.frame_time_counter,
            frame_counter: times.frame_counter,
            view_width: w as f32,
            view_height: h as f32,
            aspect_ratio: w as f32 / h.max(1) as f32,
            sun_angle: celestial.sun_angle,
            shadow_angle: celestial.shadow_angle,
            night_vision: s.night_vision,
            blindness: s.blindness,
            is_eye_in_water: s.eye_in_fluid,
            world_time: s.world_time,
            world_day: s.world_day,
            moon_phase: s.moon_phase,
            // Cylindrical fog, as vanilla's terrain fog.
            fog_shape: 1,
            // GL_LINEAR.
            fog_mode: 9729,
            held_block_light_value: s.held_light[0],
            held_block_light_value2: s.held_light[1],
            held_item_id: -1,
            held_item_id2: -1,
            entity_id: -1,
            block_entity_id: -1,
            hide_gui: i32::from(s.hide_gui),
            atlas_size: [0, 0],
            ..Default::default()
        };
        // The light's matrices as the camera's; Horizon's clip matrix is the
        // one that gives GL's after `gl_clip`.
        let mut light = main;
        let (light_view, light_projection) = (glam4(shadow.model_view), glam4(shadow.projection));
        light.hz_view_proj = m4(gl_clip().inverse() * light_projection * light_view);
        light.gbuffer_model_view = shadow.model_view;
        light.gbuffer_model_view_inverse = shadow.model_view_inverse;
        light.gbuffer_projection = shadow.projection;
        light.gbuffer_projection_inverse = shadow.projection_inverse;
        light.model_view_matrix = shadow.model_view;
        light.projection_matrix = shadow.projection;
        // The hand: its own projection, depth squeezed in front of the world.
        let mut hand = main;
        let hand_clip = hand_depth() * self.hand_view_proj;
        hand.hz_view_proj = m4(hand_clip);
        hand.projection_matrix = m4(gl_clip() * hand_clip * model_view.inverse());
        FrameUniforms {
            main,
            shadow: light,
            hand,
        }
    }
}

/// The shadow map's clip matrix in Horizon's conventions (camera-relative,
/// reversed Z), the same the shadow pass draws with.
pub(super) fn shadow_clip(frame: &crate::gfx::Frame, plan: &PackPlan) -> Mat4 {
    let model_view = crate::camera::view_matrix(frame.yaw, frame.pitch);
    let celestial = frame_math::celestial(
        frame.sky.sun_angle,
        frame.sky.moon_angle,
        plan.shadow.sun_path_rotation,
        &m4(model_view),
    );
    let shadow = frame_math::shadow_camera(
        frame.camera.to_array(),
        celestial.shadow_angle,
        &plan.shadow,
    );
    gl_clip().inverse() * glam4(shadow.projection) * glam4(shadow.model_view)
}
