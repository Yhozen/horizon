//! Per-frame maths behind the pack's uniforms: the shadow camera, the sun and
//! moon, smoothed values, frame timers and the previous frame's matrices.
//!
//! Everything is pure and in GL conventions (format notes §3 and §9):
//! matrices are column-major `[column][row]` like [`PackUniforms`], view
//! space is right-handed with the camera looking down −Z, projections map to
//! GL clip space (the client converts to Horizon's with `hz_to_gl` as for
//! every other GL matrix). Positions handed to the pack are camera-relative
//! world positions, so no matrix here carries the camera's translation.
//!
//! [`PackUniforms`]: crate::PackUniforms

use crate::plan::{PackPlan, ShadowSpec};

/// A column-major 4×4 matrix (`m[column][row]`).
pub type Mat4 = [[f32; 4]; 4];

pub const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// `a · b` (apply `b` first).
pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0; 4]; 4];
    for (c, col) in out.iter_mut().enumerate() {
        for (r, v) in col.iter_mut().enumerate() {
            *v = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    out
}

/// `m · v`.
pub fn transform(m: &Mat4, v: [f32; 4]) -> [f32; 4] {
    let mut out = [0.0; 4];
    for (r, o) in out.iter_mut().enumerate() {
        *o = (0..4).map(|k| m[k][r] * v[k]).sum();
    }
    out
}

/// Counter-clockwise rotation by `degrees` about X (GL's `glRotatef(a, 1, 0, 0)`).
pub fn rotation_x(degrees: f32) -> Mat4 {
    let (s, c) = degrees.to_radians().sin_cos();
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, c, s, 0.0],
        [0.0, -s, c, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Counter-clockwise rotation by `degrees` about Y.
pub fn rotation_y(degrees: f32) -> Mat4 {
    let (s, c) = degrees.to_radians().sin_cos();
    [
        [c, 0.0, -s, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [s, 0.0, c, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Counter-clockwise rotation by `degrees` about Z.
pub fn rotation_z(degrees: f32) -> Mat4 {
    let (s, c) = degrees.to_radians().sin_cos();
    [
        [c, s, 0.0, 0.0],
        [-s, c, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

pub fn translation(t: [f32; 3]) -> Mat4 {
    let mut m = IDENTITY;
    m[3] = [t[0], t[1], t[2], 1.0];
    m
}

/// GL's `glOrtho`: view-space z = −`near` maps to NDC −1, z = −`far` to +1.
pub fn ortho(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Mat4 {
    let (w, h, d) = (right - left, top - bottom, far - near);
    [
        [2.0 / w, 0.0, 0.0, 0.0],
        [0.0, 2.0 / h, 0.0, 0.0],
        [0.0, 0.0, -2.0 / d, 0.0],
        [
            -(right + left) / w,
            -(top + bottom) / h,
            -(far + near) / d,
            1.0,
        ],
    ]
}

/// The inverse of `m` (cofactors, in f64); the identity for a singular matrix
/// so a degenerate input never fills a uniform with NaN.
pub fn invert(m: &Mat4) -> Mat4 {
    // Row-major f64 copy: a[r][c].
    let a: [[f64; 4]; 4] = std::array::from_fn(|r| std::array::from_fn(|c| f64::from(m[c][r])));
    let minor = |r: usize, c: usize| -> f64 {
        let rows: Vec<usize> = (0..4).filter(|&i| i != r).collect();
        let cols: Vec<usize> = (0..4).filter(|&i| i != c).collect();
        let e = |i: usize, j: usize| a[rows[i]][cols[j]];
        e(0, 0) * (e(1, 1) * e(2, 2) - e(1, 2) * e(2, 1))
            - e(0, 1) * (e(1, 0) * e(2, 2) - e(1, 2) * e(2, 0))
            + e(0, 2) * (e(1, 0) * e(2, 1) - e(1, 1) * e(2, 0))
    };
    let cof: [[f64; 4]; 4] = std::array::from_fn(|r| {
        std::array::from_fn(|c| {
            let sign = if (r + c) % 2 == 0 { 1.0 } else { -1.0 };
            sign * minor(r, c)
        })
    });
    let det: f64 = (0..4).map(|c| a[0][c] * cof[0][c]).sum();
    if det.abs() < 1e-30 || !det.is_finite() {
        return IDENTITY;
    }
    // inverse[r][c] = cof[c][r] / det; stored column-major: out[c][r].
    std::array::from_fn(|c| std::array::from_fn(|r| (cof[c][r] / det) as f32))
}

// ---- Sun, moon and angles ------------------------------------------------

/// Vanilla's celestial angle of a day time, as a fraction of a turn: 0 at
/// noon (day time 6000), 0.5 at midnight, with vanilla's easing (the sun
/// spends a little longer near the horizon). `day_time` in ticks; any value
/// (the day number is ignored). Multiply by 360 for vanilla's sun angle in
/// degrees.
pub fn celestial_angle(day_time: i64) -> f32 {
    let d = (day_time.rem_euclid(24_000) as f64 / 24_000.0 - 0.25).rem_euclid(1.0);
    let eased = 0.5 - (d * std::f64::consts::PI).cos() / 2.0;
    ((d * 2.0 + eased) / 3.0) as f32
}

/// The pack's `sunAngle` from vanilla's sun angle in degrees: 0 at sunrise,
/// 0.25 at noon, 0.5 at sunset, in [0, 1).
pub fn sun_angle(vanilla_sun_degrees: f32) -> f32 {
    wrap01((vanilla_sun_degrees + 90.0) / 360.0)
}

/// The pack's `shadowAngle`: `sunAngle` by day, `sunAngle − 0.5` by night
/// (the moon's angle), in [0, 0.5).
pub fn shadow_angle(sun_angle: f32) -> f32 {
    if sun_angle < 0.5 {
        sun_angle
    } else {
        sun_angle - 0.5
    }
}

fn wrap01(x: f32) -> f32 {
    let w = x.rem_euclid(1.0);
    // rem_euclid can round up to exactly 1.0 for tiny negative inputs.
    if w >= 1.0 { 0.0 } else { w }
}

/// The celestial uniforms of a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Celestial {
    /// View-space vectors of length 100 (`sunPosition`, `moonPosition`,
    /// `shadowLightPosition`, `upPosition`).
    pub sun_position: [f32; 3],
    pub moon_position: [f32; 3],
    pub shadow_light_position: [f32; 3],
    pub up_position: [f32; 3],
    /// World-space unit vector toward the body that casts shadows.
    pub shadow_light_world: [f32; 3],
    pub sun_angle: f32,
    pub shadow_angle: f32,
}

/// The body at `degrees` (vanilla's sky rotation), turned by
/// `sun_path_rotation`, in world space with length 100.
fn body_world(degrees: f32, sun_path_rotation: f32) -> [f32; 3] {
    let m = mul(
        &mul(&rotation_y(-90.0), &rotation_z(sun_path_rotation)),
        &rotation_x(degrees),
    );
    let v = transform(&m, [0.0, 100.0, 0.0, 0.0]);
    [v[0], v[1], v[2]]
}

/// Sun, moon, up and shadow-light positions and the two angles.
///
/// - `sun_degrees`, `moon_degrees`: vanilla's sky angles of the bodies in
///   degrees (`celestial_angle(t) * 360`; the moon is the sun + 180 in the
///   overworld). The client can pass its environment's `sun_angle` /
///   `moon_angle` directly.
/// - `sun_path_rotation`: `ShadowSpec::sun_path_rotation`, degrees.
/// - `model_view`: `gbufferModelView` (camera rotation, no translation).
pub fn celestial(
    sun_degrees: f32,
    moon_degrees: f32,
    sun_path_rotation: f32,
    model_view: &Mat4,
) -> Celestial {
    let sun_world = body_world(sun_degrees, sun_path_rotation);
    let moon_world = body_world(moon_degrees, sun_path_rotation);
    let view = |v: [f32; 3], w: f32| {
        let o = transform(model_view, [v[0], v[1], v[2], w]);
        [o[0], o[1], o[2]]
    };
    let sun_angle = sun_angle(sun_degrees);
    let by_day = sun_angle < 0.5;
    let light = if by_day { sun_world } else { moon_world };
    Celestial {
        sun_position: view(sun_world, 1.0),
        moon_position: view(moon_world, 1.0),
        shadow_light_position: view(light, 1.0),
        up_position: view([0.0, 100.0, 0.0], 0.0),
        shadow_light_world: light.map(|c| c / 100.0),
        sun_angle,
        shadow_angle: shadow_angle(sun_angle),
    }
}

/// [`celestial`] from a day time in ticks, with the moon opposite the sun.
pub fn celestial_at(day_time: i64, sun_path_rotation: f32, model_view: &Mat4) -> Celestial {
    let sun = celestial_angle(day_time) * 360.0;
    celestial(sun, sun + 180.0, sun_path_rotation, model_view)
}

// ---- Shadow camera -------------------------------------------------------

/// View-space z of the shadow projection's near and far planes: the map
/// covers +100.05 (toward the light) to −156, as a camera 100 blocks toward
/// the light with a 256-block range would.
pub const SHADOW_NEAR_Z: f32 = 100.05;
pub const SHADOW_FAR_Z: f32 = -156.0;

/// `shadowModelView`, `shadowProjection` and their inverses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowCamera {
    pub model_view: Mat4,
    pub model_view_inverse: Mat4,
    pub projection: Mat4,
    pub projection_inverse: Mat4,
}

/// The snapping offset `o` of the shadow view: per world axis
/// `fmod(camera, interval) − interval / 2` (truncated `fmod`, so it keeps the
/// camera's sign); zero when `interval` <= 0.
pub fn shadow_snap_offset(camera: [f64; 3], interval: f32) -> [f32; 3] {
    let i = f64::from(interval);
    if i <= 0.0 {
        return [0.0; 3];
    }
    // Rust's `%` on floats is C's fmod.
    camera.map(|c| ((c % i) - i / 2.0) as f32)
}

/// The shadow camera (format notes §3).
///
/// - `camera`: the real (unshifted) camera position in world blocks, f64 so
///   the snapping stays exact far from the origin.
/// - `shadow_angle`: [`Celestial::shadow_angle`].
/// - `spec`: distance (half-width of the map), interval, sun-path rotation.
///
/// `model_view` = Rx(90°) · Rz(−360°·s) · Rx(sunPathRotation) · T(o) with
/// s = shadowAngle − 0.25 in [0, 1) and `o` = [`shadow_snap_offset`]; it
/// applies to camera-relative world positions and maps the direction toward
/// the light onto +Z (the light shines down −Z, the shadow camera's view
/// direction). The projection is an orthographic box of ±`distance` blocks
/// from [`SHADOW_NEAR_Z`] to [`SHADOW_FAR_Z`].
pub fn shadow_camera(camera: [f64; 3], shadow_angle: f32, spec: &ShadowSpec) -> ShadowCamera {
    let s = wrap01(shadow_angle - 0.25);
    let o = shadow_snap_offset(camera, spec.interval);
    let model_view = mul(
        &mul(
            &mul(&rotation_x(90.0), &rotation_z(-360.0 * s)),
            &rotation_x(spec.sun_path_rotation),
        ),
        &translation(o),
    );
    let d = spec.distance;
    let projection = ortho(-d, d, -d, d, -SHADOW_NEAR_Z, -SHADOW_FAR_Z);
    ShadowCamera {
        model_view,
        model_view_inverse: invert(&model_view),
        projection,
        projection_inverse: invert(&projection),
    }
}

// ---- Smoothing -----------------------------------------------------------

/// Moves `current` toward `target` so the gap halves every `half_life`
/// seconds. `dt` in seconds (the frame time). A half-life <= 0 jumps to the
/// target.
pub fn smooth(current: f32, target: f32, dt: f32, half_life: f32) -> f32 {
    if half_life <= 0.0 || !current.is_finite() {
        return target;
    }
    let keep = (-dt.max(0.0) / half_life).exp2();
    target + (current - target) * keep
}

/// A plan half-life (`wetnessHalflife`, `drynessHalflife`,
/// `eyeBrightnessHalflife`: units of 0.1 s) in seconds.
pub fn half_life_seconds(plan_value: f32) -> f32 {
    plan_value * 0.1
}

/// One frame of `wetness`: toward `rain_strength` (0..1) with the plan's
/// wetness half-life while rising and its dryness half-life while falling.
/// `dt` in seconds.
pub fn wetness_step(wetness: f32, rain_strength: f32, dt: f32, plan: &PackPlan) -> f32 {
    let half_life = if rain_strength > wetness {
        plan.wetness_half_life
    } else {
        plan.dryness_half_life
    };
    smooth(wetness, rain_strength, dt, half_life_seconds(half_life))
}

/// `eyeBrightnessSmooth`: `eyeBrightness` smoothed with
/// `eyeBrightnessHalflife` both ways and truncated to integers.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EyeBrightnessSmooth {
    value: Option<[f32; 2]>,
}

impl EyeBrightnessSmooth {
    /// One frame: `eye_brightness` (block, sky) in 0..=240, `dt` seconds,
    /// `plan_half_life` in plan units (0.1 s). The first call starts at the
    /// target.
    pub fn update(&mut self, eye_brightness: [i32; 2], dt: f32, plan_half_life: f32) -> [i32; 2] {
        let target = eye_brightness.map(|v| v as f32);
        let hl = half_life_seconds(plan_half_life);
        let v = match self.value {
            None => target,
            Some(v) => [
                smooth(v[0], target[0], dt, hl),
                smooth(v[1], target[1], dt, hl),
            ],
        };
        self.value = Some(v);
        v.map(|x| x as i32)
    }
}

// ---- Frame timers --------------------------------------------------------

/// `frameTimeCounter` resets to 0 once it reaches this many milliseconds.
const FRAME_TIME_COUNTER_RESET_MS: u64 = 3_600_000;
/// `frameCounter` wraps here.
pub const FRAME_COUNTER_WRAP: u32 = 720_720;

/// The timer uniforms of one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameTimes {
    /// `frameTime`: seconds since the previous frame, at 1 ms resolution
    /// (0 on the first frame).
    pub frame_time: f32,
    /// `frameTimeCounter`: seconds summed over frames, back to 0 at 3600.
    pub frame_time_counter: f32,
    /// `frameCounter`: 0 on the first frame, wrapping at 720720.
    pub frame_counter: i32,
}

/// Keeps `frameTime`, `frameTimeCounter` and `frameCounter` across frames.
/// Counting in whole milliseconds keeps the counter free of float drift.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameClock {
    last_ms: Option<u64>,
    counter_ms: u64,
    frames: u32,
}

impl FrameClock {
    /// Call once per frame with a monotonic clock in milliseconds (e.g.
    /// `Instant::elapsed().as_millis()` from a fixed start).
    pub fn advance(&mut self, now_ms: u64) -> FrameTimes {
        let dt = self.last_ms.map_or(0, |last| now_ms.saturating_sub(last));
        self.last_ms = Some(now_ms);
        self.counter_ms += dt;
        if self.counter_ms >= FRAME_TIME_COUNTER_RESET_MS {
            self.counter_ms = 0;
        }
        let frame_counter = self.frames as i32;
        self.frames = (self.frames + 1) % FRAME_COUNTER_WRAP;
        FrameTimes {
            frame_time: dt as f32 / 1000.0,
            frame_time_counter: self.counter_ms as f32 / 1000.0,
            frame_counter,
        }
    }
}

// ---- Camera position and the previous frame -------------------------------

/// The x/z shift kicks in beyond this distance from the current shift…
const CAMERA_SHIFT_RANGE: f64 = 30_000.0;
/// …or when the camera jumps further than this in one frame.
const CAMERA_JUMP: f64 = 1_000.0;

/// The camera uniforms that depend on the previous frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraFrame {
    /// `cameraPosition`: the camera, shifted in x/z by a multiple of 30000
    /// so it stays in f32 range of the origin.
    pub camera_position: [f32; 3],
    /// `previousCameraPosition`, with the same shift as this frame's (so
    /// the difference is the true motion). The current position on the
    /// first frame.
    pub previous_camera_position: [f32; 3],
    /// `eyeAltitude`: the camera's y.
    pub eye_altitude: f32,
    /// `gbufferPreviousModelView` / `gbufferPreviousProjection`: last
    /// frame's `gbufferModelView` / `gbufferProjection`; identity on the
    /// first frame.
    pub previous_model_view: Mat4,
    pub previous_projection: Mat4,
}

/// Remembers last frame's camera and matrices. Reset it (`default()`) when a
/// world opens or the pack reloads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreviousFrame {
    last: Option<([f64; 3], Mat4, Mat4)>,
    /// Current shift in x and z (multiples of 30000).
    shift: [f64; 2],
}

impl PreviousFrame {
    /// One frame: the real camera position (world blocks, f64) and this
    /// frame's `gbufferModelView` and `gbufferProjection`. Returns the
    /// uniforms; the inputs become next frame's "previous".
    pub fn advance(&mut self, camera: [f64; 3], model_view: Mat4, projection: Mat4) -> CameraFrame {
        let prev_camera = self.last.map_or(camera, |(c, _, _)| c);
        let jump = (0..3)
            .map(|i| (camera[i] - prev_camera[i]).powi(2))
            .sum::<f64>()
            .sqrt();
        let axes = [0, 2];
        let out_of_range = axes
            .iter()
            .zip(self.shift)
            .any(|(&a, s)| (camera[a] - s).abs() > CAMERA_SHIFT_RANGE);
        if out_of_range || jump > CAMERA_JUMP {
            self.shift =
                axes.map(|a| (camera[a] / CAMERA_SHIFT_RANGE).trunc() * CAMERA_SHIFT_RANGE);
        }
        let shifted = |c: [f64; 3]| {
            [
                (c[0] - self.shift[0]) as f32,
                c[1] as f32,
                (c[2] - self.shift[1]) as f32,
            ]
        };
        let (previous_model_view, previous_projection) =
            self.last.map_or((IDENTITY, IDENTITY), |(_, mv, p)| (mv, p));
        let frame = CameraFrame {
            camera_position: shifted(camera),
            previous_camera_position: shifted(prev_camera),
            eye_altitude: camera[1] as f32,
            previous_model_view,
            previous_projection,
        };
        self.last = Some((camera, model_view, projection));
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn close3(a: [f32; 3], b: [f32; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-3)
    }

    fn xyz(v: [f32; 4]) -> [f32; 3] {
        [v[0], v[1], v[2]]
    }

    #[test]
    fn matrix_helpers() {
        let m = mul(&rotation_x(30.0), &translation([1.0, 2.0, 3.0]));
        let back = mul(&invert(&m), &m);
        for c in 0..4 {
            for r in 0..4 {
                assert!(close(back[c][r], IDENTITY[c][r]), "{back:?}");
            }
        }
        // Rz(90) turns +X into +Y (counter-clockwise).
        assert!(close3(
            xyz(transform(&rotation_z(90.0), [1.0, 0.0, 0.0, 0.0])),
            [0.0, 1.0, 0.0]
        ));
        assert_eq!(invert(&[[0.0; 4]; 4]), IDENTITY);
    }

    #[test]
    fn celestial_angles_through_the_day() {
        assert!(close(celestial_angle(6000), 0.0));
        assert!(close(celestial_angle(18_000), 0.5));
        assert!(close(celestial_angle(30_000), 0.0));
        // Noon: sunAngle 0.25, the sun straight up.
        let c = celestial_at(6000, 0.0, &IDENTITY);
        assert!(close(c.sun_angle, 0.25));
        assert!(close(c.shadow_angle, 0.25));
        assert!(close3(c.sun_position, [0.0, 100.0, 0.0]));
        assert!(close3(c.moon_position, [0.0, -100.0, 0.0]));
        assert!(close3(c.up_position, [0.0, 100.0, 0.0]));
        // Sunrise (day time 0) is in the east (+X), sunAngle near 0.
        let c = celestial_at(0, 0.0, &IDENTITY);
        assert!(c.sun_angle < 0.05 || c.sun_angle > 0.95, "{}", c.sun_angle);
        assert!(c.sun_position[0] > 90.0 && c.sun_position[1].abs() < 25.0);
        // Midnight: the moon casts, shadowAngle 0.25 again.
        let c = celestial_at(18_000, 0.0, &IDENTITY);
        assert!(close(c.sun_angle, 0.75));
        assert!(close(c.shadow_angle, 0.25));
        assert_eq!(c.shadow_light_position, c.moon_position);
        assert!(close3(c.shadow_light_world, [0.0, 1.0, 0.0]));
    }

    #[test]
    fn positions_follow_the_view_rotation() {
        let mv = rotation_y(90.0);
        let c = celestial_at(6000, 0.0, &mv);
        assert!(close3(c.up_position, [0.0, 100.0, 0.0]));
        let world = celestial_at(1000, 0.0, &IDENTITY).sun_position;
        let c = celestial_at(1000, 0.0, &mv);
        let expected = xyz(transform(&mv, [world[0], world[1], world[2], 0.0]));
        assert!(close3(c.sun_position, expected));
    }

    #[test]
    fn shadow_view_puts_the_light_on_plus_z() {
        let mut spec = ShadowSpec {
            interval: 0.0,
            ..ShadowSpec::default()
        };
        for rotation in [0.0, 25.0, -40.0] {
            spec.sun_path_rotation = rotation;
            for t in [0, 2000, 6000, 11_000, 15_000, 18_000, 23_000] {
                let c = celestial_at(t, rotation, &IDENTITY);
                let cam = shadow_camera([0.0; 3], c.shadow_angle, &spec);
                let l = c.shadow_light_world;
                let v = xyz(transform(&cam.model_view, [l[0], l[1], l[2], 0.0]));
                assert!(close3(v, [0.0, 0.0, 1.0]), "t {t} rot {rotation}: {v:?}");
            }
        }
    }

    #[test]
    fn shadow_projection_covers_the_box() {
        let spec = ShadowSpec::default();
        let cam = shadow_camera([0.0; 3], 0.25, &spec);
        let p = |v: [f32; 3]| transform(&cam.projection, [v[0], v[1], v[2], 1.0]);
        assert!(close(p([0.0, 0.0, SHADOW_NEAR_Z])[2], -1.0));
        assert!(close(p([0.0, 0.0, SHADOW_FAR_Z])[2], 1.0));
        assert!(close(p([160.0, -160.0, 0.0])[0], 1.0));
        assert!(close(p([160.0, -160.0, 0.0])[1], -1.0));
        let id = mul(&cam.projection_inverse, &cam.projection);
        assert!(close(id[2][2], 1.0) && close(id[3][2], 0.0));
    }

    #[test]
    fn snapping_keeps_the_map_still_within_an_interval() {
        let spec = ShadowSpec::default();
        let world = [1003.7f64, 70.2, -2051.3];
        let at = |cam: [f64; 3]| {
            let s = shadow_camera(cam, 0.3, &spec);
            let rel = [0, 1, 2].map(|i| (world[i] - cam[i]) as f32);
            xyz(transform(&s.model_view, [rel[0], rel[1], rel[2], 1.0]))
        };
        let base = at([10.2, 64.1, -20.6]);
        // Sub-interval moves (interval 2) inside the same cell: same map position.
        for cam in [[10.9, 64.9, -20.1], [10.01, 65.5, -21.9]] {
            assert!(close3(at(cam), base), "{cam:?}");
        }
        // Crossing a cell boundary moves it by a whole interval.
        let moved = at([12.2, 64.1, -20.6]);
        assert!(!close3(moved, base));
        assert_eq!(shadow_snap_offset([-3.0, 0.5, 5.0], 2.0), [-2.0, -0.5, 0.0]);
        assert_eq!(shadow_snap_offset([7.0, 1.0, 2.0], 0.0), [0.0; 3]);
    }

    #[test]
    fn smoothing_halves_per_half_life() {
        assert!(close(smooth(0.0, 1.0, 2.0, 2.0), 0.5));
        assert!(close(smooth(0.0, 1.0, 4.0, 2.0), 0.75));
        assert_eq!(smooth(0.3, 1.0, 0.016, 0.0), 1.0);
        let plan = PackPlan::default();
        // Rising: 60 s half-life; falling: 20 s.
        assert!(close(wetness_step(0.0, 1.0, 60.0, &plan), 0.5));
        assert!(close(wetness_step(1.0, 0.0, 20.0, &plan), 0.5));
        let mut eye = EyeBrightnessSmooth::default();
        assert_eq!(eye.update([0, 240], 0.05, 10.0), [0, 240]);
        assert_eq!(eye.update([240, 0], 1.0, 10.0), [120, 120]);
    }

    #[test]
    fn frame_clock_counts_and_wraps() {
        let mut clock = FrameClock::default();
        let f = clock.advance(5000);
        assert_eq!(
            (f.frame_time, f.frame_time_counter, f.frame_counter),
            (0.0, 0.0, 0)
        );
        let f = clock.advance(5016);
        assert!(close(f.frame_time, 0.016) && close(f.frame_time_counter, 0.016));
        assert_eq!(f.frame_counter, 1);
        let f = clock.advance(5016 + 3_600_000);
        assert_eq!(f.frame_time_counter, 0.0);
        let mut clock = FrameClock {
            frames: FRAME_COUNTER_WRAP - 1,
            ..FrameClock::default()
        };
        assert_eq!(clock.advance(0).frame_counter, 720_719);
        assert_eq!(clock.advance(1).frame_counter, 0);
    }

    #[test]
    fn previous_frame_tracking() {
        let mut prev = PreviousFrame::default();
        let mv = rotation_y(10.0);
        let p = ortho(-1.0, 1.0, -1.0, 1.0, 0.0, 1.0);
        let f = prev.advance([1.0, 64.0, 2.0], mv, p);
        assert_eq!(f.previous_model_view, IDENTITY);
        assert_eq!(f.previous_projection, IDENTITY);
        assert_eq!(f.previous_camera_position, f.camera_position);
        assert_eq!(f.eye_altitude, 64.0);
        let f = prev.advance([1.5, 64.0, 2.0], IDENTITY, IDENTITY);
        assert_eq!(f.previous_model_view, mv);
        assert_eq!(f.previous_projection, p);
        assert_eq!(f.previous_camera_position, [1.0, 64.0, 2.0]);
        // Far out: shifted by 30000 in x, previous with the same shift.
        let mut prev = PreviousFrame::default();
        prev.advance([30_000.5, 70.0, 10.0], IDENTITY, IDENTITY);
        let f = prev.advance([30_001.0, 70.0, 10.0], IDENTITY, IDENTITY);
        assert_eq!(f.camera_position, [1.0, 70.0, 10.0]);
        assert_eq!(f.previous_camera_position, [0.5, 70.0, 10.0]);
    }
}
