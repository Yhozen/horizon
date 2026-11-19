use super::*;
use crate::naga_out::glsl_to_wgsl;

/// A stand-in for the real host (S3): every recorded name becomes a plain
/// global or a stub function, then naga must accept the whole stage.
fn naga(rw: &Rewritten, stage: Stage) -> String {
    let mut s = String::from("#version 450\n");
    if !rw.uniforms.is_empty() {
        s += "layout(std140, set = 1, binding = 0) uniform HzUniforms {\n";
        for u in &rw.uniforms {
            s += &format!("    {} {};\n", u.ty, u.name);
        }
        s += "};\n";
    }
    s += "struct HzFog { vec4 color; float density; float start; float end; float scale; };\n";
    for b in &rw.builtins {
        let decl = match b.as_str() {
            "hz_gl_TextureMatrix" => "mat4 hz_gl_TextureMatrix[8];".into(),
            "hz_gl_FragData" => "vec4 hz_gl_FragData[8];".into(),
            "hz_gl_Normal" => "vec3 hz_gl_Normal;".into(),
            "hz_gl_NormalMatrix" => "mat3 hz_gl_NormalMatrix;".into(),
            "hz_gl_FogFragCoord" => "float hz_gl_FogFragCoord;".into(),
            "hz_gl_Fog" => "HzFog hz_gl_Fog;".into(),
            "hz_ftransform" => "vec4 hz_ftransform() { return vec4(0.0); }".into(),
            m if m.ends_with("Matrix") || m.ends_with("Inverse") => format!("mat4 {m};"),
            v => format!("vec4 {v};"),
        };
        s += &decl;
        s += "\n";
    }
    for v in rw.attributes.iter().chain(&rw.varyings) {
        let len = v.array_len.map(|n| format!("[{n}]")).unwrap_or_default();
        s += &format!("{} {}{len};\n", v.ty, v.name);
    }
    for f in &rw.sample_calls {
        let (ty, sig, ret) = match f.split('_').nth(1) {
            Some("sample") => ("vec4", "vec2 a", "vec4(0.0)"),
            Some("sampleLod") => ("vec4", "vec2 a, float l", "vec4(0.0)"),
            Some("shadow") => ("vec4", "vec3 a", "vec4(0.0)"),
            Some("sampleBias") => ("vec4", "vec2 a, float b", "vec4(0.0)"),
            Some("sampleGrad") => ("vec4", "vec2 a, vec2 dx, vec2 dy", "vec4(0.0)"),
            Some("shadowLod") => ("vec4", "vec3 a, float l", "vec4(0.0)"),
            Some("fetch") => ("vec4", "ivec2 a, int l", "vec4(0.0)"),
            _ => ("ivec2", "int l", "ivec2(1)"),
        };
        s += &format!("{ty} {f}({sig}) {{ return {ret}; }}\n");
    }
    let write_out = stage == Stage::Fragment && rw.builtins.contains("hz_gl_FragData");
    if write_out {
        s += "layout(location = 0) out vec4 hz_out0;\n";
    }
    s += &rw.body;
    let out = if write_out {
        " hz_out0 = hz_gl_FragData[0];"
    } else {
        ""
    };
    s += &format!("\nvoid main() {{ hz_pack_main();{out} }}\n");
    glsl_to_wgsl(&s, stage).unwrap_or_else(|e| panic!("{e}\n--- source ---\n{s}"))
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| s.to_string()).collect()
}

const VS: &str = "precision highp float;
attribute vec4 mc_Entity;
uniform float rainStrength, frameTimeCounter;
uniform int worldTime;
uniform sampler2D lightmap;
flat varying float level;
varying vec2 texUV, lightUV;
varying vec3 tints[2];
varying vec4 ambient;
struct Light { vec3 color; float sample; };
const vec3 TABLE[2] = vec3[](vec3(0.0), vec3(0.25, 0.5, 0.75));
float round(float x) { return floor(x + 0.5); }
vec3 round(vec3 x) { return floor(x + 0.5); }
float early(float x) { if (x > 0.0) return x; return -x; float unreachable = x * 2.0; return unreachable; }
void main() {
gl_Position = ftransform();
texUV = (gl_TextureMatrix[0] * gl_MultiTexCoord0).st;
lightUV = (gl_TextureMatrix[1] * gl_MultiTexCoord1).st;
ambient = texture2D(lightmap, vec2(0.03, lightUV.t)) + texture2DLod(lightmap, lightUV, 0.0);
level = float(mc_Entity.x == 3.0 || mc_Entity.x == 4);
Light l = Light(gl_Color.rgb, 1.0e-3 + .5);
mediump float sample = round(l.sample) + float(gl_VertexID % 2);
tints[0] = round(TABLE[int(max(0.0, mc_Entity.x - 100.0))]) * sample;
tints[1] = gl_NormalMatrix * gl_Normal + (gl_ModelViewMatrix * gl_Vertex).xyz;
if (worldTime > 12000) { level += rainStrength * early(frameTimeCounter); }
}
";

#[test]
fn vertex_stage() {
    let rw = rewrite(VS, Stage::Vertex).unwrap();
    let var = |name: &str, ty: &str, flat, array_len| Var {
        name: name.into(),
        ty: ty.into(),
        flat,
        array_len,
    };
    assert_eq!(rw.attributes, [var("mc_Entity", "vec4", false, None)]);
    assert_eq!(
        rw.varyings,
        [
            var("level", "float", true, None),
            var("texUV", "vec2", false, None),
            var("lightUV", "vec2", false, None),
            var("tints", "vec3", false, Some(2)),
            var("ambient", "vec4", false, None),
        ]
    );
    let names: Vec<_> = rw.uniforms.iter().map(|u| u.name.as_str()).collect();
    assert_eq!(names, ["rainStrength", "frameTimeCounter", "worldTime"]);
    assert_eq!(rw.samplers, set(&["lightmap"]));
    assert_eq!(
        rw.sample_calls,
        set(&["hz_sample_lightmap", "hz_sampleLod_lightmap"])
    );
    let builtins = [
        "hz_ftransform",
        "hz_gl_Color",
        "hz_gl_ModelViewMatrix",
        "hz_gl_MultiTexCoord0",
    ];
    assert!(
        builtins.iter().all(|b| rw.builtins.contains(*b)),
        "{:?}",
        rw.builtins
    );
    for needle in [
        "void hz_pack_main()",
        "float hz_u_round(float x)",
        "l.hz_u_sample",
        "gl_VertexIndex",
    ] {
        assert!(
            rw.body.contains(needle),
            "{needle} missing from\n{}",
            rw.body
        );
    }
    assert!(
        rw.body.contains("return float( x); return float( -x);}")
            && !rw.body.contains("unreachable"),
        "{}",
        rw.body
    );
    assert!(
        !rw.body.contains("highp") && !rw.body.contains("mediump") && !rw.body.contains("varying")
    );
    naga(&rw, Stage::Vertex);
}

const FS: &str = "uniform sampler2D gtexture;
uniform sampler2D colortex0, colortex1;
uniform sampler2DShadow shadowtex0;
uniform vec4 entityColor;
varying vec2 texUV;
varying vec4 ambient;
varying vec4 color;
vec4 sum(vec4 a, vec4 b) { return a + b; }
void main() {
vec4 ambient = ambient;
vec4 color = color * entityColor;
vec4 albedo = texture2D(gtexture, texUV, -1.0)
    * texture(colortex0, texture2D(colortex1, sum(vec4(texUV, 0.0, 0.0), vec4(0.5)).xy).xy);
ivec2 size = textureSize(colortex0, 0);
albedo += texelFetch(colortex1, ivec2(gl_FragCoord.xy) % size, 0);
albedo *= shadow2D(shadowtex0, vec3(texUV, 0.5)).x * texture(shadowtex0, vec3(texUV, 0.25)).x;
gl_FragColor = albedo * ambient * color;
}
";

#[test]
fn fragment_stage() {
    let rw = rewrite(FS, Stage::Fragment).unwrap();
    assert_eq!(
        rw.samplers,
        set(&["gtexture", "colortex0", "colortex1", "shadowtex0"])
    );
    let calls = [
        "hz_sampleBias_gtexture",
        "hz_sample_colortex0",
        "hz_sample_colortex1",
        "hz_size_colortex0",
        "hz_fetch_colortex1",
        "hz_shadow_shadowtex0",
    ];
    assert_eq!(rw.sample_calls, set(&calls));
    assert_eq!(rw.builtins, set(&["hz_FragCoord", "hz_gl_FragData"]));
    assert!(
        rw.body.contains("hz_sampleBias_gtexture( texUV, -1.0)"),
        "{}",
        rw.body
    );
    assert!(rw.body.contains("hz_gl_FragData[0] = albedo"));
    // A local shadowing a varying is renamed; its initializer still reads the varying.
    assert!(
        rw.body.contains("vec4 hz_l_ambient = ambient;"),
        "{}",
        rw.body
    );
    naga(&rw, Stage::Fragment);
}

const MONO: &str = "uniform sampler2D colortex0, colortex1;
uniform sampler2D depthtex0;
uniform sampler2D texture;
varying vec2 uv;
vec4 fetch(sampler2D t, vec2 p);
vec4 fetch(in sampler2D t, vec2 p) { return texture2D(t, p) + vec4(textureSize(t, 0), 0.0, 0.0); }
vec4 fetch(sampler2D t, vec3 p) { return texture2D(t, p.xy / p.z); }
vec4 fetch(sampler2D t, vec2 p, float k) { return texture2DLod(t, p, k); }
vec4 never(sampler2D t) { return undefined_call(t); }
float pair(sampler2D a, float w, sampler2D b) { return fetch(a, uv).x * w + fetch(b, vec3(uv, 1.0)).y; }
vec4 nested(sampler2D t) { vec4 s = fetch(t, uv, 1.0); return s * pair(t, s.x, depthtex0); }
void main() {
gl_FragData[0] = nested(colortex0) + fetch(colortex1, fetch(colortex1, uv).xy) + nested(texture);
}
";

#[test]
fn sampler_parameters_are_monomorphised() {
    let rw = rewrite(MONO, Stage::Fragment).unwrap();
    for needle in [
        "vec4 fetch_hz_colortex1( vec2 p) {",
        "vec4 fetch_hz_colortex0( vec3 p) {",
        "vec4 fetch_hz_depthtex0( vec3 p) {",
        "vec4 fetch_hz_colortex0( vec2 p, float k) {",
        "float pair_hz_colortex0_hz_depthtex0( float w) {",
        "fetch_hz_depthtex0( vec3(uv, 1.0))",
        "fetch_hz_colortex1( fetch_hz_colortex1( uv).xy)",
        "nested_hz_texture()",
        "hz_sampleLod_gtexture",
    ] {
        assert!(
            rw.body.contains(needle),
            "{needle} missing from\n{}",
            rw.body
        );
    }
    // The generic originals, and a function nothing calls, are gone.
    assert!(
        !rw.body.contains("sampler2D") && !rw.body.contains("never"),
        "{}",
        rw.body
    );
    assert_eq!(
        rw.samplers,
        set(&["colortex0", "colortex1", "depthtex0", "gtexture"])
    );
    naga(&rw, Stage::Fragment);
}

const FORMS: &str = "uniform sampler2D normals;
uniform sampler2DShadow shadow;
varying vec4 uv;
void main() {
vec2 dx = dFdx(uv.xy), dy = dFdy(uv.xy);
vec4 c = texture2DGradARB(normals, uv.xy, dx, dy) + textureGrad(normals, uv.xy, dx, dy)
    + texture2DGrad(normals, uv.xy, dx, dy) + texture2DProj(normals, uv) + texture2DProj(normals, uv.xyz)
    + texture2D(normals, uv.xy, 2.0) + shadow2DLod(shadow, uv.xyz, 1.0) + shadow2D(shadow, uv.xyz, 0.5);
gl_FragCoord.z -= 0.0001;
gl_FragData[0] = c * gl_FragCoord.z;
}
";

#[test]
fn sampling_forms() {
    let rw = rewrite(FORMS, Stage::Fragment).unwrap();
    let calls = [
        "hz_sampleGrad_normals",
        "hz_sample_normals",
        "hz_sampleBias_normals",
        "hz_shadowLod_shadow",
        "hz_shadow_shadow",
    ];
    assert_eq!(rw.sample_calls, set(&calls));
    for needle in [
        "hz_sampleGrad_normals( uv.xy, dx, dy)",
        "hz_sample_normals(hz_proj( uv))",
        "hz_sampleBias_normals( uv.xy, 2.0)",
        "hz_shadowLod_shadow( uv.xyz, 1.0)",
        "hz_FragCoord.z -= 0.0001",
    ] {
        assert!(
            rw.body.contains(needle),
            "{needle} missing from\n{}",
            rw.body
        );
    }
    assert!(rw.builtins.contains("hz_FragCoord"));
    naga(&rw, Stage::Fragment);
}

#[test]
fn front_colour_is_discarded() {
    let vs = "varying vec4 color;\nvoid main() { gl_FrontColor = gl_Color; gl_BackColor = gl_Color; \
              color = gl_Color; gl_Position = ftransform(); }\n";
    let rw = rewrite(vs, Stage::Vertex).unwrap();
    assert!(
        rw.body
            .starts_with("vec4 hz_gl_BackColor;\nvec4 hz_gl_FrontColor;\n"),
        "{}",
        rw.body
    );
    assert!(!rw.builtins.iter().any(|b| b.contains("FrontColor")));
    naga(&rw, Stage::Vertex);
}

#[test]
fn glsl_130_interface() {
    let vs = "in vec3 position;\nout vec2 uv;\nflat out int id;\n\
              void main() { uv = position.xy; id = 1; gl_Position = vec4(position, 1.0); }\n";
    let rw = rewrite(vs, Stage::Vertex).unwrap();
    assert_eq!(rw.attributes.len(), 1);
    assert_eq!(rw.varyings.len(), 2);
    assert!(rw.varyings[1].flat);
    naga(&rw, Stage::Vertex);
    let fs = "in vec2 uv;\nlayout(location = 0) out vec4 outColor;\nout vec4 outNormal;\n\
              void main() { outColor = vec4(uv, 0.0, 1.0); outNormal = outColor; }\n";
    let rw = rewrite(fs, Stage::Fragment).unwrap();
    assert!(
        rw.body.contains("hz_gl_FragData[1] = hz_gl_FragData[0]"),
        "{}",
        rw.body
    );
    naga(&rw, Stage::Fragment);
}

#[test]
fn unsupported_input_is_an_error() {
    let cases = [
        (
            "uniform sampler2D s;\nvec4 f(sampler2D t) { return vec4(0.0); }\n\
             void main() { f(s + 1.0); }",
            "not a sampler name",
        ),
        (
            "uniform sampler2D s;\nvec4 f(sampler2D t) { return vec4(0.0); }\n\
             void main() { f(q); }",
            "`q` passed to `f` is not a global sampler",
        ),
        ("void main() { sampler2D t; }", "sampler type"),
        (
            "uniform sampler2D s;\nvoid main() { g(s); }",
            "passed to `g`",
        ),
        (
            "uniform sampler2D s;\nvoid main() { float x = s; }",
            "used other",
        ),
        (
            "uniform sampler2D s;\nvoid main() { texture3D(s, vec3(0.0)); }",
            "passed to `texture3D`",
        ),
        // Literal indices are supported (compat.rs), computed ones are not.
        (
            "void main() { int i = 0; gl_TexCoord[i] = vec4(0.0); }",
            "gl_TexCoord",
        ),
        ("uniform sampler2D s[2];", "sampler array"),
        ("void main() { {", "unbalanced"),
    ];
    for (src, msg) in cases {
        let err = rewrite(src, Stage::Fragment).unwrap_err();
        assert!(err.contains(msg), "{src}: {err}");
    }
}

#[test]
fn numbers_are_single_tokens() {
    let toks: Vec<_> = tokenize("1.0e-3+2u*.5-0x1F")
        .into_iter()
        .map(|t| t.text)
        .collect();
    assert_eq!(toks, ["1.0e-3", "+", "2u", "*", ".5", "-", "0x1F"]);
}

/// `HORIZON_TEST_SHADERPACK_PREPROCESSED=<dir>`: every `.vsh`/`.fsh` in it
/// (already preprocessed, made outside the repo) must rewrite and pass naga.
#[test]
fn preprocessed_pack_files() {
    let Ok(dir) = std::env::var("HORIZON_TEST_SHADERPACK_PREPROCESSED") else {
        return;
    };
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    paths.sort();
    let mut failed = Vec::new();
    for path in paths {
        let stage = match path.extension().and_then(|e| e.to_str()) {
            Some("vsh") => Stage::Vertex,
            Some("fsh") => Stage::Fragment,
            _ => continue,
        };
        let src = std::fs::read_to_string(&path).unwrap();
        // Every file is tried, so one run lists all the failures.
        let result = std::panic::catch_unwind(|| {
            let rw = rewrite(&src, stage).unwrap_or_else(|e| panic!("rewrite: {e}"));
            naga(&rw, stage);
        });
        match result {
            Ok(()) => println!("ok {}", path.display()),
            Err(_) => failed.push(path.display().to_string()),
        }
    }
    assert!(failed.is_empty(), "failed: {failed:?}");
}

#[test]
fn dead_functions_are_dropped() {
    let fs = "uniform sampler2D colortex0;
float helper(float x);
float scale(float x) { return x * 0.5; }
const float K = 2.0;
float helper(float x) { return scale(x) + K; }
float helper(vec2 x) { return x.x; }
vec4 unused(sampler2D s, vec2 uv) { return texture2D(s, uv) * gl_TexCoord[0]; }
float alsoUnused() { return unused(colortex0, vec2(0.0)).x; };
;
void main() { gl_FragColor = vec4(helper(1.0) + helper(vec2(0.0))); }
";
    let rw = rewrite(fs, Stage::Fragment).unwrap();
    for gone in [
        "unused",
        "alsoUnused",
        "gl_TexCoord",
        "};",
        "float helper(float x);",
    ] {
        assert!(!rw.body.contains(gone), "{gone} in\n{}", rw.body);
    }
    for kept in ["float scale(", "float helper(vec2 x)"] {
        assert!(rw.body.contains(kept), "{kept} missing from\n{}", rw.body);
    }
    naga(&rw, Stage::Fragment);
}

#[test]
fn shadowing_locals_are_renamed() {
    let fs = "uniform sampler2D tex;
uniform float frameTimeCounter;
varying vec2 uv;
float wave(float frameTimeCounter, const in float step) {
    return sin(frameTimeCounter * step);
}
vec3 gain(vec3 color, float gain) { return color * gain; }
void main() {
    vec4 tex = texture2D(tex, uv);
    float distance = distance(uv, vec2(0.5)), d2 = distance * 2.0;
    for (int step = 0; step < 2; step++) tex.x += float(step);
    for (float uv = 0.0; uv < 1.0; uv += 0.5) { tex.y += uv; }
    for (int i = 0; i < 2; i++) for (int step = 0; step < 2; step++) tex.x += float(step);
    tex.x += step(0.5, uv.y);
    {
        float frameTimeCounter = wave(frameTimeCounter, 1.0);
        tex.z += frameTimeCounter;
    }
    tex.w = uv.x + step(0.5, d2) + frameTimeCounter;
    gl_FragColor = tex + vec4(gain(vec3(1.0), 2.0), 0.0);
}
";
    let rw = rewrite(fs, Stage::Fragment).unwrap();
    for needle in [
        "vec3 gain(vec3 color, float hz_l_gain) { return vec3( color * hz_l_gain);}",
        "float wave(float hz_l_frameTimeCounter, in float hz_l_step)",
        "sin(hz_l_frameTimeCounter * hz_l_step)",
        "vec4 hz_l_tex = hz_sample_tex( uv);",
        "float hz_l_distance = distance(uv, vec2(0.5)), d2 = hz_l_distance * 2.0;",
        "for (int hz_l_step = 0; hz_l_step < 2; hz_l_step++) hz_l_tex.x += float(hz_l_step);",
        "for (float hz_l_uv = 0.0; hz_l_uv < 1.0; hz_l_uv += 0.5) { hz_l_tex.y += hz_l_uv; }",
        "for (int hz_l_step = 0; hz_l_step < 2; hz_l_step++) hz_l_tex.x += float(hz_l_step);\n    hz_l_tex.x += step(0.5, uv.y);",
        "float hz_l_frameTimeCounter = wave(frameTimeCounter, 1.0);",
        "hz_l_tex.z += hz_l_frameTimeCounter;",
        "hz_l_tex.w = uv.x + step(0.5, d2) + frameTimeCounter;",
    ] {
        assert!(
            rw.body.contains(needle),
            "{needle} missing from\n{}",
            rw.body
        );
    }
    naga(&rw, Stage::Fragment);
}

#[test]
fn naga_rejections_are_fixed() {
    let fs = "uniform float rainStrength;
varying vec2 uv;
float half(float x) { return x * 0.5; }
const float A = 2.0;
const float B = half(A), C = A * 3.0;
const float D = B + 1.0;
float E = rainStrength * C, F = 4.0;
const vec3 N = normalize(vec3(1.0, 2, 3));
int count(const in int n) { const int m = n * 2; return m; }
float one(float x) { if (x > 0.0) return 1; return count(2); }
vec3 up() { return vec3(0, 1, 0); }
void main() {
    gl_FragColor = vec4(N * D + up() * E * F, one(uv.x));
}
";
    let rw = rewrite(fs, Stage::Fragment).unwrap();
    for needle in [
        "const float A = 2.0;",
        "float B, C = A * 3.0;",
        "float D;",
        "float E, F = 4.0;",
        "const vec3 N = normalize(",
        "int count(in int n) { int m = n * 2; return int( m);}",
        "return float( 1); return float( count(2));",
        "void hz_pack_main() { B = half ( A ); D = B + 1.0; E = rainStrength * C;",
    ] {
        assert!(
            rw.body.contains(needle),
            "{needle} missing from\n{}",
            rw.body
        );
    }
    let wgsl = naga(&rw, Stage::Fragment);
    // The moved initializers really run (naga drops run-time global initializers).
    assert!(wgsl.contains("half("), "{wgsl}");
}

#[test]
fn dfdy_is_negated_in_fragment_shaders() {
    let fs = "varying vec3 viewPos;\nvoid main() {\n    vec3 n = normalize(cross(dFdx(viewPos), dFdy(viewPos)));\n    \
              float w = 1.0-dFdy(viewPos.x);\n    gl_FragData[0] = vec4(n, w);\n}\n";
    let rw = rewrite(fs, Stage::Fragment).unwrap();
    assert!(
        rw.body.contains("hz_dFdy(viewPos)") && rw.body.contains("1.0-hz_dFdy("),
        "{}",
        rw.body
    );
    assert!(
        rw.body
            .contains("vec3 hz_dFdy(vec3 v) { return -dFdy(v); }"),
        "{}",
        rw.body
    );
    naga(&rw, Stage::Fragment);
    // Vertex shaders have no derivatives to fix.
    let vs = "void main() { gl_Position = vec4(0.0); }\n";
    assert!(!rewrite(vs, Stage::Vertex).unwrap().body.contains("hz_dFdy"));
}

#[test]
fn sampler_types_and_tagged_calls() {
    let src = "uniform sampler3D vol;\nuniform usampler2D ids;\nuniform sampler2DShadow sh;\nuniform samplerCube sky;\n\
               void main() {\n    vec4 a = texture3D(vol, vec3(0.5)) + texelFetchOffset(vol, ivec3(1), 0, ivec3(1));\n    \
               uvec4 b = texture(ids, vec2(0.5));\n    float s = texture(sh, vec3(0.5));\n    \
               vec4 t = shadow2D(sh, vec3(0.5));\n    float u = texture(sh, vec3(0.5)).x;\n}\n";
    let out = rewrite(src, Stage::Fragment).unwrap();
    assert_eq!(out.sampler_types["vol"], "sampler3D");
    assert_eq!(out.sampler_types["sky"], "samplerCube");
    assert_eq!(
        out.sample_calls,
        set(&[
            "hz_fetch_3_vol",
            "hz_sample_2u_ids",
            "hz_sample_3_vol",
            "hz_shadow_sh"
        ])
    );
    // The fetch offset is added to the coordinate; modern shadow reads are scalars.
    assert!(
        out.body
            .contains("hz_fetch_3_vol((( ivec3(1)) + ( ivec3(1))), 0)"),
        "{}",
        out.body
    );
    assert!(
        out.body.contains("float s = hz_shadow_sh( vec3(0.5)).x;"),
        "{}",
        out.body
    );
    assert!(
        out.body.contains("vec4 t = hz_shadow_sh( vec3(0.5));"),
        "{}",
        out.body
    );
    assert!(
        out.body.contains("float u = hz_shadow_sh( vec3(0.5)).x;"),
        "{}",
        out.body
    );
    let parsed = crate::host::parse_sampling_call("hz_sample_2u_ids").unwrap();
    assert_eq!(
        (parsed.op, parsed.dim, parsed.sample, parsed.name),
        (
            "hz_sample_",
            crate::TexDim::D2,
            crate::SampleKind::Uint,
            "ids"
        )
    );
}

#[test]
fn a_plain_overload_beside_a_sampler_one() {
    let src = "uniform sampler2D colortex9;\nvec3 f(vec3 p, bool lod) { return p; }\n\
               vec3 f(vec3 p, sampler2D v) { return texelFetch(v, ivec2(p.xy), 0).xyz + f(p, false); }\n\
               void main() { gl_FragData[0] = vec4(f(vec3(0.5), colortex9) + f(vec3(0.5), true), 1.0); }\n";
    let rw = rewrite(src, Stage::Fragment).unwrap();
    assert!(rw.body.contains("f_hz_colortex9("), "{}", rw.body);
    assert!(rw.body.contains("f(vec3(0.5), true)"), "{}", rw.body);
}

#[test]
fn uniform_initializers() {
    // GLSL 330: Horizon's value wins for its own uniforms; others keep the pack's value.
    let src = "uniform float viewWidth = 1.0, heavyFog = max(0.25, 0.5);\n\
               uniform vec2 offset = vec2(1.0, -2.0);\n\
               out vec4 color;\n\
               void main() { color = vec4(viewWidth, heavyFog, offset); }\n";
    let rw = rewrite(src, Stage::Fragment).unwrap();
    let names: Vec<_> = rw.uniforms.iter().map(|u| u.name.as_str()).collect();
    assert_eq!(names, ["viewWidth"], "{}", rw.body);
    assert!(
        rw.body.contains("float heavyFog = max ( 0.25 , 0.5 );"),
        "{}",
        rw.body
    );
    assert!(
        rw.body.contains("vec2 offset = vec2 ( 1.0 , - 2.0 );"),
        "{}",
        rw.body
    );
    naga(&rw, Stage::Fragment);
}

#[test]
fn modf_and_frexp_are_polyfilled() {
    let src = "varying vec2 uv;\n\
               void main() { float i, f = modf(uv.x, i); vec2 w; vec2 g = modf(uv, w); ivec3 e;\n\
               vec3 m = frexp(vec3(uv, 1.0), e); gl_FragData[0] = vec4(f + i, g + w, 0.0) + vec4(m, e.x); }\n";
    let rw = rewrite(src, Stage::Fragment).unwrap();
    assert!(rw.body.contains("hz_modf(uv.x, i)"), "{}", rw.body);
    naga(&rw, Stage::Fragment);
    // A pack's own `modf` stays.
    let own = "float modf(float x, float y) { return x; }\nvoid main() { gl_FragData[0] = vec4(modf(1.0, 2.0)); }\n";
    assert!(
        !rewrite(own, Stage::Fragment)
            .unwrap()
            .body
            .contains("hz_modf")
    );
}

#[test]
fn isnan_and_isinf_are_polyfilled() {
    let src = "varying vec3 c;\n\
               void main() { bool a = isnan(c.x) || isinf(c.y); bvec3 b = isnan(c);\n\
               gl_FragData[0] = vec4(a || any(b) || any(isinf(vec2(c))) ? 1.0 : 0.0); }\n";
    let rw = rewrite(src, Stage::Fragment).unwrap();
    assert!(
        rw.body.contains("hz_isnan(c.x) || hz_isinf(c.y)"),
        "{}",
        rw.body
    );
    // The WGSL writer, which has no `isnan`, takes it.
    naga(&rw, Stage::Fragment);
}

#[test]
fn struct_varyings_are_declared_after_their_struct() {
    let src = "struct Fog { vec3 color; float density; };\nflat varying Fog fog;\n\
               void main() { gl_FragData[0] = vec4(fog.color, fog.density); }\n";
    let rw = rewrite(src, Stage::Fragment).unwrap();
    assert_eq!(rw.varyings[0].ty, "Fog");
    assert!(
        rw.body
            .contains("struct Fog { vec3 color; float density; }; Fog fog;"),
        "{}",
        rw.body
    );
}

#[test]
fn functions_returning_arrays() {
    let src = "vec3[2] f(vec3[2] a, vec3 k) { vec3 m = k; return vec3[2](a[0] * m, a[1]); }\n\
               void main() { if (true) { } { } vec3[2] q = f(vec3[2](vec3(0.0), vec3(1.0)), vec3(1.0)); \
               gl_FragData[0] = vec4(q[0], 1.0); }\n";
    let rw = rewrite(src, Stage::Fragment).unwrap();
    assert!(
        rw.body.contains("vec3[2] f(vec3[2] a, vec3 k)"),
        "{}",
        rw.body
    );
    naga(&rw, Stage::Fragment);
}

#[test]
fn vertex_id_is_an_int() {
    let src = "int f(int i) { return i % 2; }\nvoid main() { gl_Position = vec4(float(f(gl_VertexID)), 0.0, 0.0, 1.0); }\n";
    let rw = rewrite(src, Stage::Vertex).unwrap();
    assert!(rw.body.contains("f(int(gl_VertexIndex))"), "{}", rw.body);
}

#[test]
fn a_sampler_declared_below_its_first_use() {
    let src = "vec3 sky(sampler2D s) { return texture2D(s, vec2(0.5)).rgb; }\n\
               vec3 g() { return sky(colortex4); }\n\
               uniform sampler2D colortex4;\n\
               void main() { gl_FragData[0] = vec4(g(), 1.0); }\n";
    let rw = rewrite(src, Stage::Fragment).unwrap();
    assert!(
        rw.sample_calls.contains("hz_sample_colortex4"),
        "{}",
        rw.body
    );
}
