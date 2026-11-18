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
if (worldTime > 12000) { level += rainStrength * frameTimeCounter; }
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
    assert!(rw.body.contains("return x; return -x;}") && !rw.body.contains("unreachable"));
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
        "hz_sample_gtexture",
        "hz_sample_colortex0",
        "hz_sample_colortex1",
        "hz_size_colortex0",
        "hz_fetch_colortex1",
        "hz_shadow_shadowtex0",
    ];
    assert_eq!(rw.sample_calls, set(&calls));
    assert_eq!(rw.builtins, set(&["hz_gl_FragData"]));
    assert!(
        rw.body.contains("hz_sample_gtexture( texUV)"),
        "{}",
        rw.body
    );
    assert!(rw.body.contains("hz_gl_FragData[0] = albedo"));
    // naga scopes the local after its initializer (as GLSL does), so this reads the varying.
    assert!(rw.body.contains("vec4 ambient = ambient;"));
    naga(&rw, Stage::Fragment);
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
            "uniform sampler2D s;\nvec4 f(sampler2D t) { return vec4(0.0); }",
            "sampler type",
        ),
        (
            "uniform sampler2D s;\nvoid main() { g(s); }",
            "passed to `g`",
        ),
        (
            "uniform sampler2D s;\nvoid main() { float x = s; }",
            "used other",
        ),
        (
            "uniform sampler2D s;\nvoid main() { texture2DGrad(s, vec2(0.0), vec2(0.0), vec2(0.0)); }",
            "passed",
        ),
        ("void main() { gl_TexCoord[0] = vec4(0.0); }", "gl_TexCoord"),
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
    for path in paths {
        let stage = match path.extension().and_then(|e| e.to_str()) {
            Some("vsh") => Stage::Vertex,
            Some("fsh") => Stage::Fragment,
            _ => continue,
        };
        let src = std::fs::read_to_string(&path).unwrap();
        let rw = rewrite(&src, stage).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        naga(&rw, stage);
        println!("ok {}", path.display());
    }
}
