use horizon_glsl_pack::naga_out::{Stage, glsl_to_wgsl};

const VS: &str = r#"#version 450
layout(std140, set = 1, binding = 0) uniform HzUniforms {
    mat4 hz_viewProj;
    mat4 gbufferModelView;
    mat4 gbufferModelViewInverse;
    vec4 sunPosition4;
    float rainStrength;
    float screenBrightness;
    int isEyeInWater;
    int worldTime;
    ivec2 eyeBrightnessSmooth;
};
layout(set = 0, binding = 3) uniform texture2D hz_lightmap;
layout(set = 0, binding = 4) uniform sampler hz_lightmap_sampler;

layout(location = 0) in vec3 hz_position;
layout(location = 1) in uvec2 hz_uv;
layout(location = 2) in uint hz_layer;
layout(location = 3) in vec4 hz_color;
layout(location = 4) in uvec2 hz_light;
layout(location = 5) in vec4 hz_origin;

vec4 gl_Vertex;
vec4 gl_Color;
vec3 gl_Normal;
vec4 gl_MultiTexCoord0;
vec4 gl_MultiTexCoord1;
mat4 gl_TextureMatrix[2];
vec4 mc_Entity;

float fogMix;
vec2 texUV;
vec4 ambient;
vec4 color;
float lightSourceLevel;

layout(location = 0) out float hz_v_fogMix;
layout(location = 1) out vec2 hz_v_texUV;
layout(location = 2) out vec4 hz_v_ambient;
layout(location = 3) out vec4 hz_v_color;
layout(location = 4) flat out float hz_v_lightSourceLevel;
layout(location = 5) flat out uvec4 hz_sprite;

vec4 ftransform() { return hz_viewProj * gl_Vertex; }

float rescale(float x, float a, float b) { return clamp((x - a) / (b - a), 0.0, 1.0); }
float hz_round(float x) { return floor(x + 0.5); }
const vec3 TABLE[2] = vec3[](vec3(0.0), vec3(0.6, 0.05, 0.07));

void pack_main() {
    gl_Position = ftransform();
    color = gl_Color;
    texUV = (gl_TextureMatrix[0] * gl_MultiTexCoord0).st;
    vec2 lightUV = (gl_TextureMatrix[1] * gl_MultiTexCoord1).st;
    ambient = textureLod(sampler2D(hz_lightmap, hz_lightmap_sampler), lightUV, 0.0);
    lightSourceLevel = float(mc_Entity.x == 10068.0 || mc_Entity.x == 10072);
    vec3 r = TABLE[int(max(0.0, mc_Entity.x - 20000.0))];
    if (isEyeInWater == 0) { fogMix = rescale(float(worldTime), 0.0, 1.0) + r.x; }
    fogMix += hz_round(rainStrength);
    float x = worldTime * (1.0/24000.0);
    fogMix += x;
}

void main() {
    gl_Vertex = vec4(hz_position + hz_origin.xyz, 1.0);
    gl_Color = vec4(hz_color.rgb, 1.0);
    gl_MultiTexCoord0 = vec4(vec2(hz_uv) * hz_origin.w, 0.0, 1.0);
    gl_MultiTexCoord1 = vec4(float(hz_light.y), float(hz_light.x), 0.0, 1.0);
    gl_TextureMatrix[0] = mat4(1.0);
    gl_TextureMatrix[1] = mat4(vec4(1.0/256.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0/256.0, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(1.0/32.0, 1.0/32.0, 0.0, 1.0));
    mc_Entity = vec4(10008.0, 0.0, 0.0, 0.0);
    gl_Normal = vec3(0.0, 1.0, 0.0);
    pack_main();
    hz_v_fogMix = fogMix;
    hz_v_texUV = texUV;
    hz_v_ambient = ambient;
    hz_v_color = color;
    hz_v_lightSourceLevel = lightSourceLevel;
    hz_sprite = uvec4(hz_layer, 0u, 0u, 0u);
}
"#;

const FS: &str = r#"#version 450
layout(set = 0, binding = 1) uniform texture2DArray hz_sprites;
layout(set = 0, binding = 2) uniform sampler hz_sprite_sampler;
layout(set = 1, binding = 1) uniform sampler hz_point;
layout(set = 1, binding = 2) uniform texture2D colortex0_t;
layout(set = 1, binding = 5) uniform texture2D depthtex0_t;

layout(location = 0) in float hz_v_fogMix;
layout(location = 1) in vec2 hz_v_texUV;
layout(location = 5) flat in uvec4 hz_sprite;

layout(location = 0) out vec4 hz_out0;
layout(location = 1) out vec4 hz_out1;

float fogMix;
vec2 texUV;
vec4 gl_FragData[2];

vec4 hz_gtexture(vec2 uv) { return texture(sampler2DArray(hz_sprites, hz_sprite_sampler), vec3(uv, float(hz_sprite.x))); }

void pack_main() {
    if (fogMix > 0.999) { discard; }
    vec4 albedo = hz_gtexture(texUV);
    float d = 1.0 - textureLod(sampler2D(depthtex0_t, hz_point), texUV, 0.0).x;
    for (int i = 0; i < 4; i++) {
        if (d < 0.5) break;
        albedo += textureLod(sampler2D(colortex0_t, hz_point), texUV, 0.0);
    }
    gl_FragData[0] = albedo;
    gl_FragData[1] = vec4(d);
}

void main() {
    fogMix = hz_v_fogMix;
    texUV = hz_v_texUV;
    pack_main();
    if (gl_FragData[0].a < 0.1) { discard; }
    hz_out0 = gl_FragData[0];
    hz_out1 = gl_FragData[1];
}
"#;

#[test]
fn spike_vertex() {
    match glsl_to_wgsl(VS, Stage::Vertex) {
        Ok(w) => println!("{w}"),
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn spike_fragment() {
    match glsl_to_wgsl(FS, Stage::Fragment) {
        Ok(w) => println!("{w}"),
        Err(e) => panic!("{e}"),
    }
}
