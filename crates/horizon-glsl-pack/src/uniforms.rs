//! The one std140 uniform block every program shares, and its Rust mirror.
//!
//! One macro writes the Rust struct, the GLSL block and the member table from
//! the same list, so they cannot drift apart; a test checks naga's std140
//! offsets against the struct's.

use bytemuck::{Pod, Zeroable};

macro_rules! pack_uniforms {
    ($($field:ident: $rust:ty = $glsl_ty:literal $glsl:literal,)*) => {
        /// Mirror of [`UNIFORM_BLOCK`] (std140, 1376 bytes), filled by the
        /// client every frame. Members are ordered so each `vec3` shares its
        /// 16 bytes with a `float`: the only padding is the tail.
        ///
        /// Matrices are column-major (`[column][row]`), as GLSL reads them.
        #[repr(C)]
        #[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
        pub struct PackUniforms {
            $(#[doc = concat!("GLSL `", $glsl_ty, " ", $glsl, "`.")] pub $field: $rust,)*
        }

        /// The GLSL declaration of the block (set 1, binding 0). Anonymous, so
        /// the pack reads members by their plain names.
        pub const UNIFORM_BLOCK: &str = concat!(
            "layout(std140, set = 1, binding = 0) uniform HzUniforms {\n",
            $("    ", $glsl_ty, " ", $glsl, ";\n",)*
            "};\n"
        );

        /// `(GLSL name, GLSL type, byte offset in PackUniforms)` of every member.
        pub(crate) const MEMBERS: &[(&str, &str, usize)] =
            &[$(($glsl, $glsl_ty, std::mem::offset_of!(PackUniforms, $field)),)*];
    };
}

type Mat4 = [[f32; 4]; 4];

pack_uniforms! {
    hz_view_proj: Mat4 = "mat4" "hz_viewProj",
    gbuffer_model_view: Mat4 = "mat4" "gbufferModelView",
    gbuffer_model_view_inverse: Mat4 = "mat4" "gbufferModelViewInverse",
    gbuffer_projection: Mat4 = "mat4" "gbufferProjection",
    gbuffer_projection_inverse: Mat4 = "mat4" "gbufferProjectionInverse",
    gbuffer_previous_model_view: Mat4 = "mat4" "gbufferPreviousModelView",
    gbuffer_previous_projection: Mat4 = "mat4" "gbufferPreviousProjection",
    shadow_model_view: Mat4 = "mat4" "shadowModelView",
    shadow_model_view_inverse: Mat4 = "mat4" "shadowModelViewInverse",
    shadow_projection: Mat4 = "mat4" "shadowProjection",
    shadow_projection_inverse: Mat4 = "mat4" "shadowProjectionInverse",
    model_view_matrix: Mat4 = "mat4" "modelViewMatrix",
    projection_matrix: Mat4 = "mat4" "projectionMatrix",
    camera_position: [f32; 3] = "vec3" "cameraPosition",
    fog_start: f32 = "float" "fogStart",
    previous_camera_position: [f32; 3] = "vec3" "previousCameraPosition",
    fog_end: f32 = "float" "fogEnd",
    sun_position: [f32; 3] = "vec3" "sunPosition",
    fog_density: f32 = "float" "fogDensity",
    moon_position: [f32; 3] = "vec3" "moonPosition",
    near: f32 = "float" "near",
    shadow_light_position: [f32; 3] = "vec3" "shadowLightPosition",
    far: f32 = "float" "far",
    up_position: [f32; 3] = "vec3" "upPosition",
    rain_strength: f32 = "float" "rainStrength",
    fog_color: [f32; 3] = "vec3" "fogColor",
    wetness: f32 = "float" "wetness",
    sky_color: [f32; 3] = "vec3" "skyColor",
    screen_brightness: f32 = "float" "screenBrightness",
    chunk_offset: [f32; 3] = "vec3" "chunkOffset",
    frame_time_counter: f32 = "float" "frameTimeCounter",
    entity_color: [f32; 4] = "vec4" "entityColor",
    eye_brightness: [i32; 2] = "ivec2" "eyeBrightness",
    eye_brightness_smooth: [i32; 2] = "ivec2" "eyeBrightnessSmooth",
    atlas_size: [i32; 2] = "ivec2" "atlasSize",
    view_width: f32 = "float" "viewWidth",
    view_height: f32 = "float" "viewHeight",
    aspect_ratio: f32 = "float" "aspectRatio",
    sun_angle: f32 = "float" "sunAngle",
    shadow_angle: f32 = "float" "shadowAngle",
    night_vision: f32 = "float" "nightVision",
    blindness: f32 = "float" "blindness",
    darkness_factor: f32 = "float" "darknessFactor",
    is_eye_in_water: i32 = "int" "isEyeInWater",
    world_time: i32 = "int" "worldTime",
    world_day: i32 = "int" "worldDay",
    moon_phase: i32 = "int" "moonPhase",
    frame_counter: i32 = "int" "frameCounter",
    fog_shape: i32 = "int" "fogShape",
    fog_mode: i32 = "int" "fogMode",
    held_block_light_value: i32 = "int" "heldBlockLightValue",
    held_block_light_value2: i32 = "int" "heldBlockLightValue2",
    held_item_id: i32 = "int" "heldItemId",
    held_item_id2: i32 = "int" "heldItemId2",
    entity_id: i32 = "int" "entityId",
    block_entity_id: i32 = "int" "blockEntityId",
    hide_gui: i32 = "int" "hideGUI",
    render_stage: i32 = "int" "renderStage",
    frame_time: f32 = "float" "frameTime",
    eye_altitude: f32 = "float" "eyeAltitude",
    // Iris's standard uniforms (the public uniform list); booleans are ints
    // here, which the host converts for a pack that declares `bool`.
    biome: i32 = "int" "biome",
    camera_position_fract: [f32; 3] = "vec3" "cameraPositionFract",
    temperature: f32 = "float" "temperature",
    previous_camera_position_fract: [f32; 3] = "vec3" "previousCameraPositionFract",
    rainfall: f32 = "float" "rainfall",
    eye_position: [f32; 3] = "vec3" "eyePosition",
    thunder_strength: f32 = "float" "thunderStrength",
    relative_eye_position: [f32; 3] = "vec3" "relativeEyePosition",
    current_player_health: f32 = "float" "currentPlayerHealth",
    player_look_vector: [f32; 3] = "vec3" "playerLookVector",
    max_player_health: f32 = "float" "maxPlayerHealth",
    player_body_vector: [f32; 3] = "vec3" "playerBodyVector",
    current_player_hunger: f32 = "float" "currentPlayerHunger",
    camera_position_int: [i32; 3] = "ivec3" "cameraPositionInt",
    max_player_hunger: f32 = "float" "maxPlayerHunger",
    previous_camera_position_int: [i32; 3] = "ivec3" "previousCameraPositionInt",
    current_player_air: f32 = "float" "currentPlayerAir",
    lightning_bolt_position: [f32; 4] = "vec4" "lightningBoltPosition",
    gtexture_size: [i32; 2] = "ivec2" "gtextureSize",
    max_player_air: f32 = "float" "maxPlayerAir",
    player_mood: f32 = "float" "playerMood",
    constant_mood: f32 = "float" "constantMood",
    center_depth_smooth: f32 = "float" "centerDepthSmooth",
    alpha_test_ref: f32 = "float" "alphaTestRef",
    dh_far_plane: f32 = "float" "dhFarPlane",
    dh_near_plane: f32 = "float" "dhNearPlane",
    dh_render_distance: f32 = "float" "dhRenderDistance",
    biome_category: i32 = "int" "biome_category",
    biome_precipitation: i32 = "int" "biome_precipitation",
    is_spectator: i32 = "int" "isSpectator",
    is_sneaking: i32 = "int" "is_sneaking",
    is_sprinting: i32 = "int" "is_sprinting",
    is_hurt: i32 = "int" "is_hurt",
    is_invisible: i32 = "int" "is_invisible",
    is_burning: i32 = "int" "is_burning",
    is_on_ground: i32 = "int" "is_on_ground",
    darkness_light_factor: f32 = "float" "darknessLightFactor",
    end_flash_intensity: f32 = "float" "endFlashIntensity",
    cloud_time: f32 = "float" "cloudTime",
    ambient_light: f32 = "float" "ambientLight",
    pad0: i32 = "int" "hz_pad0",
    // `atlasSize` / `gtextureSize` of entity geometry, which samples the
    // shared atlas; terrain reads the members above (its sprite's size,
    // since terrain samples sprite arrays with sprite-local UVs).
    entity_atlas_size: [i32; 2] = "ivec2" "hz_entityAtlasSize",
    // std140 rounds the block up to 16 bytes; spelt out so the struct is Pod.
    pad1: i32 = "int" "hz_pad1",
    pad2: i32 = "int" "hz_pad2",
}

const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Every matrix the identity (so an unset matrix leaves positions where they
/// are rather than collapsing them to the origin), everything else zero.
impl Default for PackUniforms {
    fn default() -> Self {
        Self {
            hz_view_proj: IDENTITY,
            gbuffer_model_view: IDENTITY,
            gbuffer_model_view_inverse: IDENTITY,
            gbuffer_projection: IDENTITY,
            gbuffer_projection_inverse: IDENTITY,
            gbuffer_previous_model_view: IDENTITY,
            gbuffer_previous_projection: IDENTITY,
            shadow_model_view: IDENTITY,
            shadow_model_view_inverse: IDENTITY,
            shadow_projection: IDENTITY,
            shadow_projection_inverse: IDENTITY,
            model_view_matrix: IDENTITY,
            projection_matrix: IDENTITY,
            // The documented default reference of the alpha test.
            alpha_test_ref: 0.1,
            max_player_health: 20.0,
            max_player_hunger: 20.0,
            max_player_air: 300.0,
            ..Zeroable::zeroed()
        }
    }
}

/// The GLSL type of a uniform the block provides, by the pack's name
/// (`"mat4"`, `"vec3"`, `"ivec2"`, `"float"`, `"int"`…).
pub fn uniform_type(name: &str) -> Option<&'static str> {
    MEMBERS
        .iter()
        .find(|(glsl, _, _)| *glsl == name && !glsl.starts_with("hz_pad"))
        .map(|&(_, ty, _)| ty)
}

impl PackUniforms {
    /// A member by its GLSL name as four floats (scalars in `x`, ints and
    /// booleans converted), for custom uniforms
    /// ([`crate::custom_uniforms::CustomUniforms::evaluate`]): `name.N` is a
    /// matrix's column N. `None` for a name the block lacks.
    pub fn get(&self, name: &str) -> Option<[f32; 4]> {
        let (base, column) = match name.split_once('.') {
            Some((b, c)) => (b, Some(c.parse::<usize>().ok().filter(|c| *c < 4)?)),
            None => (name, None),
        };
        let &(_, ty, offset) = MEMBERS
            .iter()
            .find(|(n, _, _)| *n == base && !n.starts_with("hz_"))?;
        let bytes = bytemuck::bytes_of(self);
        let word = |i: usize| -> [u8; 4] {
            bytes[offset + 4 * i..offset + 4 * i + 4]
                .try_into()
                .unwrap()
        };
        let (count, int, start) = match (ty, column) {
            ("mat4", Some(c)) => (4, false, 4 * c),
            ("mat4", None) => (4, false, 0),
            (_, Some(_)) => return None,
            ("float", _) => (1, false, 0),
            ("int", _) => (1, true, 0),
            ("vec2", _) => (2, false, 0),
            ("vec3", _) => (3, false, 0),
            ("vec4", _) => (4, false, 0),
            ("ivec2", _) => (2, true, 0),
            ("ivec3", _) => (3, true, 0),
            _ => (4, true, 0),
        };
        let mut out = [0.0; 4];
        for (i, o) in out.iter_mut().enumerate().take(count) {
            let w = word(start + i);
            *o = if int {
                i32::from_le_bytes(w) as f32
            } else {
                f32::from_le_bytes(w)
            };
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn std140_offsets_match_the_rust_struct() {
        let source = format!("#version 450\n{UNIFORM_BLOCK}void main() {{}}\n");
        let module = naga::front::glsl::Frontend::default()
            .parse(
                &naga::front::glsl::Options::from(naga::ShaderStage::Vertex),
                &source,
            )
            .expect("block parses");
        let mut layouter = naga::proc::Layouter::default();
        layouter.update(module.to_ctx()).unwrap();
        let block = module
            .global_variables
            .iter()
            .find(|(_, g)| {
                g.binding
                    == Some(naga::ResourceBinding {
                        group: 1,
                        binding: 0,
                    })
            })
            .expect("block at set 1 binding 0")
            .1;
        let naga::TypeInner::Struct { members, span } = &module.types[block.ty].inner else {
            panic!("not a struct")
        };
        assert_eq!(members.len(), MEMBERS.len());
        for (member, &(name, _, offset)) in members.iter().zip(MEMBERS) {
            assert_eq!(member.name.as_deref(), Some(name));
            assert_eq!(member.offset as usize, offset, "{name}");
        }
        assert_eq!(*span as usize, size_of::<PackUniforms>());
        assert_eq!(layouter[block.ty].size as usize, size_of::<PackUniforms>());
        assert_eq!(size_of::<PackUniforms>(), 1376);
    }

    #[test]
    fn types_by_name() {
        assert_eq!(uniform_type("gbufferModelView"), Some("mat4"));
        assert_eq!(uniform_type("eyeBrightnessSmooth"), Some("ivec2"));
        assert_eq!(uniform_type("worldTime"), Some("int"));
        assert_eq!(uniform_type("frameTime"), Some("float"));
        assert_eq!(uniform_type("eyeAltitude"), Some("float"));
        assert_eq!(uniform_type("hz_pad0"), None);
        assert_eq!(uniform_type("sunPathRotation"), None);
        assert_eq!(PackUniforms::default().shadow_projection, IDENTITY);
        assert_eq!(uniform_type("is_sneaking"), Some("int"));
        assert_eq!(uniform_type("cameraPositionInt"), Some("ivec3"));
    }

    #[test]
    fn members_by_name() {
        let mut u = PackUniforms {
            world_time: 6000,
            camera_position: [1.0, 2.0, 3.0],
            eye_brightness_smooth: [10, 240],
            ..PackUniforms::default()
        };
        u.gbuffer_model_view_inverse[3] = [7.0, 8.0, 9.0, 1.0];
        assert_eq!(u.get("worldTime"), Some([6000.0, 0.0, 0.0, 0.0]));
        assert_eq!(u.get("cameraPosition"), Some([1.0, 2.0, 3.0, 0.0]));
        assert_eq!(u.get("eyeBrightnessSmooth"), Some([10.0, 240.0, 0.0, 0.0]));
        assert_eq!(
            u.get("gbufferModelViewInverse.3"),
            Some([7.0, 8.0, 9.0, 1.0])
        );
        assert_eq!(u.get("alphaTestRef"), Some([0.1, 0.0, 0.0, 0.0]));
        assert_eq!(u.get("gbufferModelView.4"), None);
        assert_eq!(u.get("worldTime.1"), None);
        assert_eq!(u.get("hz_pad0"), None);
        assert_eq!(u.get("nope"), None);
    }
}
