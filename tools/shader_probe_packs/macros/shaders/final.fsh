#version 120
// Probe pack (Horizon, MIT): paints predefined macros and a few uniforms as rows of bits so one
// screenshot measures them. Row r starts at the top-left: a marker cell (green = defined, red = not),
// then 24 cells, most significant bit first (white = 1). Values are round(value * scale); a macro
// defined empty reads 0 (`NAME + 0` is `+0`).
// tools/iris_oracle.py decode reads the rows back using the `// row` comments below.

uniform sampler2D colortex0;
uniform float viewWidth;
uniform float viewHeight;
uniform int worldTime;
uniform int frameCounter;
varying vec2 texcoord;

const float CELL = 8.0;
const int ROWS = 38;

void value(int row, out bool def, out float v) {
    def = false;
    v = 0.0;
    // row 0: MC_VERSION x1
    if (row == 0) {
#ifdef MC_VERSION
        def = true; v = float(MC_VERSION + 0);
#endif
    }
    // row 1: IRIS_VERSION x1
    if (row == 1) {
#ifdef IRIS_VERSION
        def = true; v = float(IRIS_VERSION + 0);
#endif
    }
    // row 2: MC_GL_VERSION x1
    if (row == 2) {
#ifdef MC_GL_VERSION
        def = true; v = float(MC_GL_VERSION + 0);
#endif
    }
    // row 3: MC_GLSL_VERSION x1
    if (row == 3) {
#ifdef MC_GLSL_VERSION
        def = true; v = float(MC_GLSL_VERSION + 0);
#endif
    }
    // row 4: __VERSION__ x1
    if (row == 4) {
#ifdef __VERSION__
        def = true; v = float(__VERSION__ + 0);
#endif
    }
    // row 5: IS_IRIS x1
    if (row == 5) {
#ifdef IS_IRIS
        def = true; v = float(IS_IRIS + 0);
#endif
    }
    // row 6: IRIS_TAG_SUPPORT x1
    if (row == 6) {
#ifdef IRIS_TAG_SUPPORT
        def = true; v = float(IRIS_TAG_SUPPORT + 0);
#endif
    }
    // row 7: MC_RENDER_QUALITY x1000
    if (row == 7) {
#ifdef MC_RENDER_QUALITY
        def = true; v = float(MC_RENDER_QUALITY + 0) * 1000.0;
#endif
    }
    // row 8: MC_SHADOW_QUALITY x1000
    if (row == 8) {
#ifdef MC_SHADOW_QUALITY
        def = true; v = float(MC_SHADOW_QUALITY + 0) * 1000.0;
#endif
    }
    // row 9: MC_HAND_DEPTH x1000
    if (row == 9) {
#ifdef MC_HAND_DEPTH
        def = true; v = float(MC_HAND_DEPTH + 0) * 1000.0;
#endif
    }
    // row 10: MC_OS_LINUX x1
    if (row == 10) {
#ifdef MC_OS_LINUX
        def = true; v = float(MC_OS_LINUX + 0);
#endif
    }
    // row 11: MC_GL_VENDOR_MESA x1
    if (row == 11) {
#ifdef MC_GL_VENDOR_MESA
        def = true; v = float(MC_GL_VENDOR_MESA + 0);
#endif
    }
    // row 12: MC_GL_VENDOR_OTHER x1
    if (row == 12) {
#ifdef MC_GL_VENDOR_OTHER
        def = true; v = float(MC_GL_VENDOR_OTHER + 0);
#endif
    }
    // row 13: MC_GL_RENDERER_GALLIUM x1
    if (row == 13) {
#ifdef MC_GL_RENDERER_GALLIUM
        def = true; v = float(MC_GL_RENDERER_GALLIUM + 0);
#endif
    }
    // row 14: MC_GL_RENDERER_MESA x1
    if (row == 14) {
#ifdef MC_GL_RENDERER_MESA
        def = true; v = float(MC_GL_RENDERER_MESA + 0);
#endif
    }
    // row 15: MC_GL_RENDERER_OTHER x1
    if (row == 15) {
#ifdef MC_GL_RENDERER_OTHER
        def = true; v = float(MC_GL_RENDERER_OTHER + 0);
#endif
    }
    // row 16: MC_NORMAL_MAP x1
    if (row == 16) {
#ifdef MC_NORMAL_MAP
        def = true; v = float(MC_NORMAL_MAP + 0);
#endif
    }
    // row 17: MC_SPECULAR_MAP x1
    if (row == 17) {
#ifdef MC_SPECULAR_MAP
        def = true; v = float(MC_SPECULAR_MAP + 0);
#endif
    }
    // row 18: MC_TEXTURE_FORMAT_LAB_PBR x1
    if (row == 18) {
#ifdef MC_TEXTURE_FORMAT_LAB_PBR
        def = true; v = float(MC_TEXTURE_FORMAT_LAB_PBR + 0);
#endif
    }
    // row 19: MC_ANISOTROPIC_FILTERING x1
    if (row == 19) {
#ifdef MC_ANISOTROPIC_FILTERING
        def = true; v = float(MC_ANISOTROPIC_FILTERING + 0);
#endif
    }
    // row 20: MC_MIPMAP_LEVEL x1
    if (row == 20) {
#ifdef MC_MIPMAP_LEVEL
        def = true; v = float(MC_MIPMAP_LEVEL + 0);
#endif
    }
    // row 21: MC_FXAA_LEVEL x1
    if (row == 21) {
#ifdef MC_FXAA_LEVEL
        def = true; v = float(MC_FXAA_LEVEL + 0);
#endif
    }
    // row 22: MC_OLD_LIGHTING x1
    if (row == 22) {
#ifdef MC_OLD_LIGHTING
        def = true; v = float(MC_OLD_LIGHTING + 0);
#endif
    }
    // row 23: MC_OLD_HAND_LIGHT x1
    if (row == 23) {
#ifdef MC_OLD_HAND_LIGHT
        def = true; v = float(MC_OLD_HAND_LIGHT + 0);
#endif
    }
    // row 24: IRIS_FEATURE_CUSTOM_IMAGES x1
    if (row == 24) {
#ifdef IRIS_FEATURE_CUSTOM_IMAGES
        def = true; v = float(IRIS_FEATURE_CUSTOM_IMAGES + 0);
#endif
    }
    // row 25: IRIS_FEATURE_SSBO x1
    if (row == 25) {
#ifdef IRIS_FEATURE_SSBO
        def = true; v = float(IRIS_FEATURE_SSBO + 0);
#endif
    }
    // row 26: IRIS_FEATURE_COMPUTE_SHADERS x1
    if (row == 26) {
#ifdef IRIS_FEATURE_COMPUTE_SHADERS
        def = true; v = float(IRIS_FEATURE_COMPUTE_SHADERS + 0);
#endif
    }
    // row 27: IRIS_FEATURE_SEPARATE_HARDWARE_SAMPLERS x1
    if (row == 27) {
#ifdef IRIS_FEATURE_SEPARATE_HARDWARE_SAMPLERS
        def = true; v = float(IRIS_FEATURE_SEPARATE_HARDWARE_SAMPLERS + 0);
#endif
    }
    // row 28: IRIS_FEATURE_HIGHER_SHADOWCOLOR x1
    if (row == 28) {
#ifdef IRIS_FEATURE_HIGHER_SHADOWCOLOR
        def = true; v = float(IRIS_FEATURE_HIGHER_SHADOWCOLOR + 0);
#endif
    }
    // row 29: IRIS_FEATURE_ENTITY_TRANSLUCENT x1
    if (row == 29) {
#ifdef IRIS_FEATURE_ENTITY_TRANSLUCENT
        def = true; v = float(IRIS_FEATURE_ENTITY_TRANSLUCENT + 0);
#endif
    }
    // row 30: IRIS_FEATURE_FADE_VARIABLE x1
    if (row == 30) {
#ifdef IRIS_FEATURE_FADE_VARIABLE
        def = true; v = float(IRIS_FEATURE_FADE_VARIABLE + 0);
#endif
    }
    // row 31: IRIS_FEATURE_BLOCK_EMISSION_ATTRIBUTE x1
    if (row == 31) {
#ifdef IRIS_FEATURE_BLOCK_EMISSION_ATTRIBUTE
        def = true; v = float(IRIS_FEATURE_BLOCK_EMISSION_ATTRIBUTE + 0);
#endif
    }
    // row 32: IRIS_FEATURE_CHUNK_OFFSET x1
    if (row == 32) {
#ifdef IRIS_FEATURE_CHUNK_OFFSET
        def = true; v = float(IRIS_FEATURE_CHUNK_OFFSET + 0);
#endif
    }
    // row 33: IRIS_FEATURE_TEXTURE_SHAPED_SAMPLERS x1
    if (row == 33) {
#ifdef IRIS_FEATURE_TEXTURE_SHAPED_SAMPLERS
        def = true; v = float(IRIS_FEATURE_TEXTURE_SHAPED_SAMPLERS + 0);
#endif
    }
    // row 34: uniform viewWidth x1
    if (row == 34) { def = true; v = float(viewWidth); }
    // row 35: uniform viewHeight x1
    if (row == 35) { def = true; v = float(viewHeight); }
    // row 36: uniform worldTime x1
    if (row == 36) { def = true; v = float(worldTime); }
    // row 37: uniform frameCounter>0 x1
    if (row == 37) { def = true; v = float(frameCounter > 0); }
}

void main() {
    vec3 scene = texture2D(colortex0, texcoord).rgb;
    vec2 p = vec2(gl_FragCoord.x, viewHeight - gl_FragCoord.y);
    int col = int(floor(p.x / CELL));
    int row = int(floor(p.y / CELL));
    vec2 inCell = mod(p, CELL);
    if (row >= ROWS || col > 24) {
        gl_FragData[0] = vec4(scene, 1.0);
        return;
    }
    if (inCell.x < 1.0 || inCell.y < 1.0) {
        gl_FragData[0] = vec4(0.0, 0.0, 1.0, 1.0);
        return;
    }
    bool def;
    float v;
    value(row, def, v);
    v = floor(v + 0.5);
    vec3 c;
    if (col == 0) {
        c = def ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0);
    } else {
        float bit = mod(floor(v / exp2(float(24 - col))), 2.0);
        c = vec3(bit);
    }
    gl_FragData[0] = vec4(c, 1.0);
}
