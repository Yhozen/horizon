//! The one std140 uniform block every program shares, and its Rust mirror.
//!
//! One macro writes the Rust struct, the GLSL block and the member table from
//! the same list, so they cannot drift apart; a test checks naga's std140
//! offsets against the struct's.

use bytemuck::{Pod, Zeroable};

macro_rules! pack_uniforms {
    ($($field:ident: $rust:ty = $glsl_ty:literal $glsl:literal,)*) => {
        /// Mirror of [`UNIFORM_BLOCK`] (std140, 1120 bytes), filled by the
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
    // std140 rounds the block up to 16 bytes; spelt out so the struct is Pod.
    pad0: i32 = "int" "hz_pad0",
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
        assert_eq!(size_of::<PackUniforms>(), 1120);
    }

    #[test]
    fn types_by_name() {
        assert_eq!(uniform_type("gbufferModelView"), Some("mat4"));
        assert_eq!(uniform_type("eyeBrightnessSmooth"), Some("ivec2"));
        assert_eq!(uniform_type("worldTime"), Some("int"));
        assert_eq!(uniform_type("hz_pad0"), None);
        assert_eq!(uniform_type("sunPathRotation"), None);
        assert_eq!(PackUniforms::default().shadow_projection, IDENTITY);
    }
}
