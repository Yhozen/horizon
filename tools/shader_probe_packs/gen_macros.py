#!/usr/bin/env python3
"""Writes the `macros` and `macros-features` probe packs (MIT, Horizon's own).

Each pack's `final.fsh` paints one row per value: a marker cell (green = defined, red = not) and 24 bits,
most significant first (white = 1), 8x8 pixel cells from the top-left. `tools/iris_oracle.py decode` reads
them back using the `// row N: NAME xSCALE` comments. `macros-features` is the same pack with
`iris.features.optional` asking for every feature flag, to see which `IRIS_FEATURE_*` macros follow.
"""

from pathlib import Path

HERE = Path(__file__).resolve().parent

# (label, expression or macro name, scale). A bare macro name is tested with #ifdef and read as
# `float(NAME + 0)`, so a macro defined empty reads 0. An expression row is always "defined".
MACROS = [
    ("MC_VERSION", 1), ("IRIS_VERSION", 1), ("MC_GL_VERSION", 1), ("MC_GLSL_VERSION", 1), ("__VERSION__", 1),
    ("IS_IRIS", 1), ("IRIS_TAG_SUPPORT", 1), ("MC_RENDER_QUALITY", 1000), ("MC_SHADOW_QUALITY", 1000),
    ("MC_HAND_DEPTH", 1000), ("MC_OS_LINUX", 1), ("MC_GL_VENDOR_MESA", 1), ("MC_GL_VENDOR_OTHER", 1),
    ("MC_GL_RENDERER_GALLIUM", 1), ("MC_GL_RENDERER_MESA", 1), ("MC_GL_RENDERER_OTHER", 1), ("MC_NORMAL_MAP", 1),
    ("MC_SPECULAR_MAP", 1), ("MC_TEXTURE_FORMAT_LAB_PBR", 1), ("MC_ANISOTROPIC_FILTERING", 1), ("MC_MIPMAP_LEVEL", 1),
    ("MC_FXAA_LEVEL", 1), ("MC_OLD_LIGHTING", 1), ("MC_OLD_HAND_LIGHT", 1), ("IRIS_FEATURE_CUSTOM_IMAGES", 1),
    ("IRIS_FEATURE_SSBO", 1), ("IRIS_FEATURE_COMPUTE_SHADERS", 1), ("IRIS_FEATURE_SEPARATE_HARDWARE_SAMPLERS", 1),
    ("IRIS_FEATURE_HIGHER_SHADOWCOLOR", 1), ("IRIS_FEATURE_ENTITY_TRANSLUCENT", 1), ("IRIS_FEATURE_FADE_VARIABLE", 1),
    ("IRIS_FEATURE_BLOCK_EMISSION_ATTRIBUTE", 1), ("IRIS_FEATURE_CHUNK_OFFSET", 1),
    ("IRIS_FEATURE_TEXTURE_SHAPED_SAMPLERS", 1), ("PROBE_OPTION", 1), ("PROBE_TOGGLE", 1),
]
# Preprocessor conditions, painted 1 when the `#if` holds.
CONDITIONS = [
    "MC_GL_VERSION >= 320",
    "MC_GL_VERSION >= 45",
    "MC_VERSION >= 12111",
    "defined(MC_GL_VENDOR_MESA) || defined(MC_GL_VENDOR_OTHER)",
]
UNIFORMS = [("viewWidth", "float(viewWidth)"), ("viewHeight", "float(viewHeight)"),
            ("worldTime", "float(worldTime)"), ("frameCounter>0", "float(frameCounter > 0)")]
FEATURES = ("CUSTOM_IMAGES SSBO COMPUTE_SHADERS SEPARATE_HARDWARE_SAMPLERS HIGHER_SHADOWCOLOR ENTITY_TRANSLUCENT "
            "FADE_VARIABLE BLOCK_EMISSION_ATTRIBUTE CHUNK_OFFSET TEXTURE_SHAPED_SAMPLERS")


def fsh() -> str:
    body = []
    i = 0
    for name, scale in MACROS:
        val = f"float({name} + 0)" + (f" * {scale}.0" if scale != 1 else "")
        body.append(f"    // row {i}: {name} x{scale}\n    if (row == {i}) {{\n#ifdef {name}\n"
                    f"        def = true; v = {val};\n#endif\n    }}")
        i += 1
    for cond in CONDITIONS:
        body.append(f"    // row {i}: #if {cond} x1\n    if (row == {i}) {{\n        def = true;\n#if {cond}\n"
                    f"        v = 1.0;\n#endif\n    }}")
        i += 1
    for name, expr in UNIFORMS:
        body.append(f"    // row {i}: uniform {name} x1\n    if (row == {i}) {{ def = true; v = {expr}; }}")
        i += 1
    rows = "\n".join(body)
    return f"""#version 120
// Probe pack (Horizon, MIT; written by tools/shader_probe_packs/gen_macros.py): paints predefined macros,
// preprocessor conditions and a few uniforms as rows of bits so one screenshot measures them. Row r starts
// at the top-left: a marker cell (green = defined, red = not), then 24 cells, most significant bit first
// (white = 1). Values are round(value * scale); a macro defined empty reads 0 (`NAME + 0` is `+0`).

uniform sampler2D colortex0;
uniform float viewWidth;
uniform float viewHeight;
uniform int worldTime;
uniform int frameCounter;
varying vec2 texcoord;

// Pack options, to check that the option file (`shaderpacks/<pack>.txt`) is read.
#define PROBE_OPTION 2 // [1 2 3 4]
//#define PROBE_TOGGLE

const float CELL = 8.0;
const int ROWS = {i};

void value(int row, out bool def, out float v) {{
    def = false;
    v = 0.0;
{rows}
}}

void main() {{
    vec3 scene = texture2D(colortex0, texcoord).rgb;
    vec2 p = vec2(gl_FragCoord.x, viewHeight - gl_FragCoord.y);
    int col = int(floor(p.x / CELL));
    int row = int(floor(p.y / CELL));
    vec2 inCell = mod(p, CELL);
    if (row >= ROWS || col > 24) {{
        gl_FragData[0] = vec4(scene, 1.0);
        return;
    }}
    if (inCell.x < 1.0 || inCell.y < 1.0) {{
        gl_FragData[0] = vec4(0.0, 0.0, 1.0, 1.0);
        return;
    }}
    bool def;
    float v;
    value(row, def, v);
    v = floor(v + 0.5);
    vec3 c;
    if (col == 0) {{
        c = def ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0);
    }} else {{
        c = vec3(mod(floor(v / exp2(float(24 - col))), 2.0));
    }}
    gl_FragData[0] = vec4(c, 1.0);
}}
"""


VSH = """#version 120
// Probe pack (Horizon, MIT): a full-screen pass.
varying vec2 texcoord;

void main() {
    gl_Position = ftransform();
    texcoord = gl_MultiTexCoord0.xy;
}
"""


def main() -> None:
    for name, props in (("macros", None), ("macros-features", f"iris.features.optional={FEATURES}\n")):
        shaders = HERE / name / "shaders"
        shaders.mkdir(parents=True, exist_ok=True)
        (shaders / "final.fsh").write_text(fsh())
        (shaders / "final.vsh").write_text(VSH)
        if props:
            (shaders / "shaders.properties").write_text("# Probe pack (Horizon, MIT): ask for every Iris feature flag.\n" + props)


if __name__ == "__main__":
    main()
