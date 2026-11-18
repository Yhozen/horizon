use super::*;
use crate::naga_out::glsl_to_wgsl;
use crate::rewrite::Uniform;
use crate::{SamplerKind, TextureSource};

fn var(name: &str, ty: &str, flat: bool) -> Var {
    Var {
        name: name.into(),
        ty: ty.into(),
        flat,
        array_len: None,
    }
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| s.to_string()).collect()
}

fn uniforms(names: &[(&str, &str)]) -> Vec<Uniform> {
    names
        .iter()
        .map(|(n, t)| Uniform {
            name: n.to_string(),
            ty: t.to_string(),
        })
        .collect()
}

/// A gbuffers-style pair in the rewriter's output conventions (spec section 3).
fn terrain_program() -> (Rewritten, Rewritten) {
    let mut varyings = vec![
        var("texUV", "vec2", false),
        var("color", "vec4", false),
        var("ambient", "vec4", false),
        var("normal", "vec3", false),
        var("id", "float", true),
        var("level", "int", false),
    ];
    varyings.push(Var {
        name: "weights".into(),
        ty: "vec2".into(),
        flat: false,
        array_len: Some(2),
    });
    let vertex = Rewritten {
        body: "float rescale(float x, float a, float b) { return clamp((x - a) / (b - a), 0.0, 1.0); }
void hz_pack_main() {
    gl_Position = hz_ftransform();
    texUV = (hz_gl_TextureMatrix[0] * hz_gl_MultiTexCoord0).st;
    vec2 lightUV = (hz_gl_TextureMatrix[1] * hz_gl_MultiTexCoord1).st;
    color = hz_gl_Color * hz_sample_gtexture(texUV);
    ambient = hz_sample_lightmap(lightUV) + hz_fetch_lightmap(ivec2(0), 0) * float(hz_size_lightmap(0).x);
    normal = hz_gl_NormalMatrix * hz_gl_Normal + (hz_gl_ModelViewProjectionMatrix * hz_gl_Vertex).xyz;
    id = mc_Entity.x + rescale(float(worldTime), 0.0, 24000.0) * rainStrength;
    level = int(at_midBlock.w);
    weights[0] = vec2(1.0);
    weights[1] = sunPosition.xy;
    hz_gl_FogFragCoord = length(hz_gl_Vertex.xyz);
}
"
        .into(),
        varyings: varyings.clone(),
        attributes: vec![var("mc_Entity", "vec4", false), var("at_midBlock", "vec4", false)],
        uniforms: uniforms(&[("worldTime", "int"), ("rainStrength", "float"), ("sunPosition", "vec3")]),
        samplers: set(&["lightmap", "gtexture"]),
        builtins: set(&[
            "hz_ftransform",
            "hz_gl_TextureMatrix",
            "hz_gl_MultiTexCoord0",
            "hz_gl_MultiTexCoord1",
            "hz_gl_Color",
            "hz_gl_NormalMatrix",
            "hz_gl_Normal",
            "hz_gl_ModelViewProjectionMatrix",
            "hz_gl_Vertex",
            "hz_gl_FogFragCoord",
        ]),
        sample_calls: set(&["hz_sample_gtexture", "hz_sample_lightmap", "hz_fetch_lightmap", "hz_size_lightmap"]),
    };
    // The fragment stage skips `level`; `weights` has an array.
    varyings.retain(|v| v.name != "level");
    let fragment = Rewritten {
        body: "void hz_pack_main() {
    if (id > 99999.0) { discard; }
    vec4 albedo = hz_sample_gtexture(texUV) * hz_sampleLod_texture(texUV, 0.0) * color * ambient;
    float shadow = hz_shadow_shadowtex1(vec3(texUV, 0.5)).x * hz_sample_shadowtex0(texUV).x;
    vec4 c = hz_fetch_colortex7(ivec2(1, 2), 0) + vec4(hz_size_gtexture(0), 0.0, 0.0);
    albedo.rgb = mix(albedo.rgb, hz_gl_Fog.color.rgb, clamp(hz_gl_FogFragCoord / hz_gl_Fog.end, 0.0, 1.0));
    hz_gl_FragData[0] = albedo * hz_gl_Color * weights[1].x;
    hz_gl_FragData[1] = vec4(normal * shadow, 1.0) + c;
    hz_gl_FragData[2] = vec4(float(entityId), screenBrightness, 0.0, 1.0);
}
"
        .into(),
        varyings,
        uniforms: uniforms(&[("entityId", "int"), ("screenBrightness", "float")]),
        samplers: set(&["gtexture", "texture", "shadowtex0", "shadowtex1", "colortex7"]),
        builtins: set(&["hz_gl_FragData", "hz_gl_Color", "hz_gl_Fog", "hz_gl_FogFragCoord"]),
        sample_calls: set(&[
            "hz_sample_gtexture",
            "hz_sampleLod_texture",
            "hz_shadow_shadowtex1",
            "hz_sample_shadowtex0",
            "hz_fetch_colortex7",
            "hz_size_gtexture",
        ]),
        ..Default::default()
    };
    (vertex, fragment)
}

/// A `final`-style full-screen pass.
fn final_program() -> (Rewritten, Rewritten) {
    let vertex = Rewritten {
        body: "void hz_pack_main() {\n    gl_Position = hz_ftransform();\n    texUV = hz_gl_MultiTexCoord0.st;\n}\n".into(),
        varyings: vec![var("texUV", "vec2", false)],
        builtins: set(&["hz_ftransform", "hz_gl_MultiTexCoord0"]),
        ..Default::default()
    };
    let fragment = Rewritten {
        body: "vec3 screen2view(vec2 uv, float depth) {
    vec4 v = gbufferProjectionInverse * vec4(2.0 * vec3(uv, depth) - 1.0, 1.0);
    return v.xyz / v.w;
}
void hz_pack_main() {
    float depth = hz_sample_depthtex0(texUV).x;
    vec3 view = screen2view(texUV, depth);
    vec4 color = hz_sample_colortex0(texUV) + hz_sample_gcolor(texUV) + hz_sampleLod_colortex6(texUV, 1.0);
    if (isEyeInWater == 1) { color.rgb *= 0.5; }
    hz_gl_FragData[0] = color * clamp(-view.z / far, 0.0, 1.0);
}
"
        .into(),
        varyings: vec![var("texUV", "vec2", false)],
        uniforms: uniforms(&[("gbufferProjectionInverse", "mat4"), ("far", "float"), ("isEyeInWater", "int")]),
        samplers: set(&["colortex0", "gcolor", "colortex6", "depthtex0"]),
        builtins: set(&["hz_gl_FragData"]),
        sample_calls: set(&["hz_sample_depthtex0", "hz_sample_colortex0", "hz_sample_gcolor", "hz_sampleLod_colortex6"]),
        ..Default::default()
    };
    (vertex, fragment)
}

fn compile(
    kind: ProgramKind,
    host: &HostParams,
    program: &(Rewritten, Rewritten),
) -> (String, String) {
    let blocks = BlockIds::legacy();
    let draw_buffers = [0, 6, 7];
    let spec = ProgramSpec {
        kind,
        geometry: kind.geometry(),
        draw_buffers: &draw_buffers,
        host,
        blocks: &blocks,
        plan: &PackPlan::default(),
    };
    let (vs, fs) = sources(&program.0, &program.1, &spec).unwrap();
    let vs_wgsl =
        glsl_to_wgsl(&vs, Stage::Vertex).unwrap_or_else(|e| panic!("{kind:?} vertex:\n{e}\n{vs}"));
    let fs_wgsl = glsl_to_wgsl(&fs, Stage::Fragment)
        .unwrap_or_else(|e| panic!("{kind:?} fragment:\n{e}\n{fs}"));
    (vs_wgsl, fs_wgsl)
}

#[test]
fn every_kind_translates() {
    for kind in [
        ProgramKind::Terrain,
        ProgramKind::Water,
        ProgramKind::Shadow,
        ProgramKind::Final,
    ] {
        let program = if kind == ProgramKind::Final {
            final_program()
        } else {
            terrain_program()
        };
        let (vs, fs) = compile(kind, &HostParams::default(), &program);
        assert!(vs.contains("@vertex") && fs.contains("@fragment"));
        // Alpha test on Terrain and Shadow only.
        assert_eq!(
            fs.contains("32768u"),
            matches!(kind, ProgramKind::Terrain | ProgramKind::Shadow),
            "{kind:?}"
        );
    }
}

#[test]
fn sprite_grid_and_arrays() {
    for host in [
        HostParams {
            sprite_grid: 4,
            ..Default::default()
        },
        HostParams {
            sprite_arrays: 3,
            sprite_split: 1024,
            ..Default::default()
        },
    ] {
        let (_, fs) = compile(ProgramKind::Terrain, &host, &terrain_program());
        assert_eq!(
            fs.contains("@group(0) @binding(6)"),
            host.sprite_arrays == 3
        );
        assert!(!fs.contains("@group(0) @binding(7)"));
    }
}

#[test]
fn locations_are_sorted_and_limited() {
    let (vertex, fragment) = terrain_program();
    let blocks = BlockIds::default();
    let spec = ProgramSpec {
        kind: ProgramKind::Terrain,
        geometry: Geometry::Terrain,
        draw_buffers: &[0],
        host: &HostParams::default(),
        blocks: &blocks,
        plan: &PackPlan::default(),
    };
    let (vs, fs) = sources(&vertex, &fragment, &spec).unwrap();
    // ambient color hz_gl_Color hz_gl_FogFragCoord id normal texUV weights[2], then hz_sprite.
    for (location, decl) in [
        (0, "out vec4 hz_v_ambient"),
        (2, "out vec4 hz_v_hz_gl_Color"),
        (4, "flat out float hz_v_id"),
        (7, "out vec2 hz_v_weights_0"),
        (8, "out vec2 hz_v_weights_1"),
        (9, "flat out uvec4 hz_sprite"),
    ] {
        assert!(
            vs.contains(&format!("layout(location = {location}) {decl};")),
            "{decl}\n{vs}"
        );
    }
    assert!(fs.contains("layout(location = 9) flat in uvec4 hz_sprite;"));

    let many: Vec<Var> = (0..16)
        .map(|i| var(&format!("v{i}"), "float", false))
        .collect();
    let wide = (
        Rewritten {
            varyings: many.clone(),
            ..vertex
        },
        Rewritten {
            varyings: many,
            builtins: set(&[]),
            ..fragment
        },
    );
    // 17 with hz_sprite: packed into four vec4s.
    let (vs, _) = sources(&wide.0, &wide.1, &spec).unwrap();
    assert!(vs.contains("layout(location = 3) out vec4 hz_vp3;"), "{vs}");
    assert!(
        vs.contains("layout(location = 4) flat out uvec4 hz_sprite;"),
        "{vs}"
    );
    // A full-screen pass has no hz_sprite: 16 fit unpacked.
    let fits = ProgramSpec {
        geometry: Geometry::Fullscreen,
        ..spec
    };
    let unpacked = Interface::new(&wide.0, &wide.1, fits.geometry, 16).unwrap();
    assert_eq!(unpacked.slots.len(), 16);
    assert_eq!(unpacked.slots[15].name, "hz_v_v9");
}

#[test]
fn unknown_uniforms_are_zero_and_mistyped_ones_fail() {
    let blocks = BlockIds::default();
    let spec = ProgramSpec {
        kind: ProgramKind::Final,
        geometry: Geometry::Fullscreen,
        draw_buffers: &[0],
        host: &HostParams::default(),
        blocks: &blocks,
        plan: &PackPlan::default(),
    };
    let (vertex, fragment) = final_program();
    let with = |u: &[(&str, &str)]| Rewritten {
        uniforms: uniforms(u),
        ..fragment.clone()
    };
    let unknown = with(&[("hz_test_unknown", "vec3"), ("hz_test_count", "int")]);
    let mut unknown = unknown;
    unknown.body = unknown.body.replace(
        "hz_gl_FragData[0] = color",
        "hz_gl_FragData[0] = vec4(hz_test_unknown, float(hz_test_count)) + color",
    );
    let out = program_sources(&vertex, &unknown, &spec).unwrap();
    assert_eq!(
        out.unknown_uniforms,
        set(&["hz_test_unknown", "hz_test_count"])
    );
    assert!(out.fragment.contains("vec3 hz_test_unknown;"));
    glsl_to_wgsl(&out.fragment, Stage::Fragment)
        .unwrap_or_else(|e| panic!("{e}\n{}", out.fragment));
    let err = sources(&vertex, &with(&[("worldTime", "float")]), &spec).unwrap_err();
    assert!(err.contains("worldTime"), "{err}");
}

#[test]
fn sampler_bindings() {
    let cases = [
        ("colortex0", Some(2)),
        ("colortex15", Some(17)),
        ("colortex16", None),
        ("colortex01", None),
        ("gaux4", Some(9)),
        ("depthtex2", Some(20)),
        ("shadowtex1", Some(22)),
        ("watershadow", Some(21)),
        ("shadowcolor1", Some(24)),
        ("noisetex", Some(25)),
        ("gtexture", None),
    ];
    for (name, binding) in cases {
        assert_eq!(sampler_binding(name), binding, "{name}");
    }
}

type Mat = [[f64; 4]; 4];

/// Column-major, like GLSL: `m[column][row]`.
fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut out = [[0.0; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            out[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    out
}

fn apply(m: &Mat, v: [f64; 4]) -> [f64; 4] {
    std::array::from_fn(|r| (0..4).map(|k| m[k][r] * v[k]).sum())
}

/// Gauss-Jordan with partial pivoting.
fn inverse(m: &Mat) -> Mat {
    // Rows of [m | I] (m transposed into row-major).
    let mut a: [[f64; 8]; 4] = std::array::from_fn(|r| {
        std::array::from_fn(|c| {
            if c < 4 {
                m[c][r]
            } else {
                f64::from(c - 4 == r)
            }
        })
    });
    for col in 0..4 {
        let pivot = (col..4)
            .max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))
            .unwrap();
        a.swap(col, pivot);
        let p = a[col][col];
        a[col].iter_mut().for_each(|x| *x /= p);
        for r in 0..4 {
            if r != col {
                let f = a[r][col];
                for c in 0..8 {
                    a[r][c] -= f * a[col][c];
                }
            }
        }
    }
    std::array::from_fn(|c| std::array::from_fn(|r| a[r][c + 4]))
}

/// `screen2view(uv_down, 1 - d)` with `inverse(M * P)` undoes wgpu's projection
/// of a view position to (texture uv with v down, stored reversed-Z depth).
#[test]
fn gl_convention_round_trips() {
    let (near, far, f, aspect) = (0.05, 512.0, 1.0 / (35f64.to_radians()).tan(), 16.0 / 9.0);
    // Reversed-Z perspective, wgpu clip: depth 1 at near, 0 at far.
    let p: Mat = [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, near / (far - near), -1.0],
        [0.0, 0.0, near * far / (far - near), 0.0],
    ];
    // hz_to_gl as a matrix: (x, -y, w - 2z, w).
    let m: Mat = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, -1.0, 0.0, 0.0],
        [0.0, 0.0, -2.0, 0.0],
        [0.0, 0.0, 1.0, 1.0],
    ];
    let gl_projection_inverse = inverse(&mul(&m, &p));
    for view in [[0.3, -1.2, -4.0], [-20.0, 7.5, -300.0], [0.0, 0.0, -0.06]] {
        let clip = apply(&p, [view[0], view[1], view[2], 1.0]);
        let ndc = [clip[0] / clip[3], clip[1] / clip[3], clip[2] / clip[3]];
        let uv_down = [0.5 * ndc[0] + 0.5, 0.5 - 0.5 * ndc[1]];
        let d = ndc[2];
        assert!((0.0..=1.0).contains(&d));
        // The pack's screen2view on what the host hands it.
        let depth = 1.0 - d;
        let v = apply(
            &gl_projection_inverse,
            [
                2.0 * uv_down[0] - 1.0,
                2.0 * uv_down[1] - 1.0,
                2.0 * depth - 1.0,
                1.0,
            ],
        );
        for i in 0..3 {
            let got = v[i] / v[3];
            assert!(
                (got - view[i]).abs() < 1e-6 * view[i].abs().max(1.0),
                "{view:?}: {i} {got}"
            );
        }
    }
}

/// Host sources and both stages' WGSL of a program under `plan`.
fn run(
    kind: ProgramKind,
    plan: &PackPlan,
    draw_buffers: &[u8],
    program: &(Rewritten, Rewritten),
) -> (HostOutput, String, String) {
    let blocks = BlockIds::default();
    let spec = ProgramSpec {
        kind,
        geometry: kind.geometry(),
        draw_buffers,
        host: &HostParams::default(),
        blocks: &blocks,
        plan,
    };
    let out =
        program_sources(&program.0, &program.1, &spec).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
    let vs = glsl_to_wgsl(&out.vertex, Stage::Vertex)
        .unwrap_or_else(|e| panic!("{kind:?} vertex:\n{e}\n{}", out.vertex));
    let fs = glsl_to_wgsl(&out.fragment, Stage::Fragment)
        .unwrap_or_else(|e| panic!("{kind:?} fragment:\n{e}\n{}", out.fragment));
    (out, vs, fs)
}

/// A `gbuffers_entities`-style pair on entity geometry.
fn entity_program() -> (Rewritten, Rewritten) {
    let varyings = vec![
        var("uv", "vec2", false),
        var("light", "vec2", false),
        var("tint", "vec4", false),
        var("normal", "vec3", false),
    ];
    let vertex = Rewritten {
        body: "void hz_pack_main() {
    uv = (hz_gl_TextureMatrix[0] * hz_gl_MultiTexCoord0).st;
    light = (hz_gl_TextureMatrix[1] * hz_gl_MultiTexCoord1).st;
    tint = hz_gl_Color + entityColor * 0.0;
    normal = normalize(hz_gl_NormalMatrix * hz_gl_Normal) + at_tangent.xyz * 0.0;
    vec4 view = hz_gl_ModelViewMatrix * hz_gl_Vertex;
    vec4 back = hz_gl_ModelViewMatrixInverse * view;
    gl_Position = hz_gl_ProjectionMatrix * view + back * 0.0;
    hz_gl_FogFragCoord = length(view.xyz);
}
"
        .into(),
        varyings: varyings.clone(),
        attributes: vec![
            var("at_tangent", "vec4", false),
            var("mc_Entity", "vec4", false),
        ],
        uniforms: uniforms(&[("entityColor", "vec4")]),
        builtins: set(&[
            "hz_gl_TextureMatrix",
            "hz_gl_MultiTexCoord0",
            "hz_gl_MultiTexCoord1",
            "hz_gl_Color",
            "hz_gl_NormalMatrix",
            "hz_gl_Normal",
            "hz_gl_ModelViewMatrix",
            "hz_gl_ModelViewMatrixInverse",
            "hz_gl_ProjectionMatrix",
            "hz_gl_Vertex",
            "hz_gl_FogFragCoord",
        ]),
        ..Default::default()
    };
    let fragment = Rewritten {
        body: "void hz_pack_main() {
    vec4 albedo = hz_sample_texture(uv) * tint;
    albedo.rgb = mix(albedo.rgb, entityColor.rgb, entityColor.a);
    albedo *= hz_sampleBias_tex(uv, 1.0) + hz_sampleGrad_gtexture(uv, dFdx(uv), dFdy(uv));
    albedo *= hz_sample_lightmap(light) * hz_sample_normals(uv) + hz_sample_specular(uv);
    albedo += hz_sample_gaux1(uv) * 0.0;
    albedo.rgb = mix(albedo.rgb, hz_gl_Fog.color.rgb, clamp(hz_gl_FogFragCoord / hz_gl_Fog.end, 0.0, 1.0));
    hz_gl_FragData[0] = albedo;
    hz_gl_FragData[1] = vec4(normal * 0.5 + 0.5, hz_FragCoord.z);
}
"
        .into(),
        varyings,
        uniforms: uniforms(&[("entityColor", "vec4")]),
        samplers: set(&["texture", "tex", "gtexture", "lightmap", "normals", "specular", "gaux1"]),
        builtins: set(&["hz_gl_FragData", "hz_gl_Fog", "hz_gl_FogFragCoord", "hz_FragCoord"]),
        sample_calls: set(&[
            "hz_sample_texture",
            "hz_sampleBias_tex",
            "hz_sampleGrad_gtexture",
            "hz_sample_lightmap",
            "hz_sample_normals",
            "hz_sample_specular",
            "hz_sample_gaux1",
        ]),
        ..Default::default()
    };
    (vertex, fragment)
}

#[test]
fn entity_geometry_reads_horizon_s_entity_vertex() {
    let mut plan = PackPlan::default();
    plan.custom_textures.push(crate::plan::CustomTexture {
        stage: crate::plan::TextureStage::Gbuffers,
        sampler: "colortex4".into(),
        path: "shaders/textures/a.png".into(),
    });
    plan.buffers[2].format = crate::plan::BufferFormat::Rgba16F;
    let (out, vs, fs) = run(ProgramKind::Entities, &plan, &[0, 2], &entity_program());
    // The entity vertex layout, every attribute by location.
    for input in [
        "@location(0) hz_in_position: vec3<f32>",
        "@location(1) hz_in_uv: vec2<f32>",
        "@location(2) hz_in_layer: u32",
        "@location(3) hz_in_color: vec4<f32>",
        "@location(4) hz_in_light: vec4<u32>",
        "@location(5) hz_in_normal: vec4<f32>",
    ] {
        assert!(vs.contains(input), "{input}\n{vs}");
    }
    // The atlas and lightmap of Horizon's entity group; nothing of the terrain group.
    assert!(
        fs.contains("@group(0) @binding(1) \nvar hz_atlas: texture_2d<f32>"),
        "{fs}"
    );
    assert!(fs.contains("@group(0) @binding(3) \nvar hz_lightmap: texture_2d<f32>"));
    assert!(!fs.contains("texture_2d_array"));
    // entityColor per vertex, not the block's.
    assert!(out.vertex.contains("hz_block_entityColor;"));
    assert!(
        vs.contains("@location(5) @interpolate(flat) hz_v_entityColor"),
        "{vs}"
    );
    // The custom texture replaces gaux1; normals and specular are constants.
    assert_eq!(
        out.textures,
        vec![TextureBinding {
            binding: 6,
            source: TextureSource::Custom(0),
            sampler: SamplerKind::PointRepeat,
            vertex: false,
            fragment: true,
        }]
    );
    assert!(fs.contains("@group(1) @binding(44)"));
    // Entities' alpha test, unorm output 0 and float output 1.
    assert!(out.fragment.contains("a > 0.1)"));
    assert!(out.fragment.contains("hz_out_0 = hz_unorm("));
    assert!(out.fragment.contains("hz_out_1 = hz_finite("));
    assert!(out.fragment.contains("1.0 - gl_FragCoord.z"));
    // Projection per pass.
    assert!(
        out.vertex
            .contains("hz_gl_ProjectionMatrix = projectionMatrix;")
    );
    assert!(out.vertex.contains("hz_inverse(modelViewMatrix)"));
}

/// A composite pass reading every kind of pack texture.
fn composite_program(calls: &[&str]) -> (Rewritten, Rewritten) {
    let (vertex, mut fragment) = final_program();
    let mut body = String::from("void hz_pack_main() {\n    vec4 c = vec4(0.0);\n");
    for call in calls {
        let (op, _) = split_sampling_call(call).unwrap();
        let args = match op {
            "hz_sample_" => "(texUV)",
            "hz_sampleLod_" => "(texUV, 2.0)",
            "hz_sampleBias_" => "(texUV, 1.0)",
            "hz_sampleGrad_" => "(texUV, dFdx(texUV), dFdy(texUV))",
            "hz_fetch_" => "(ivec2(3, 4), 0)",
            "hz_size_" => {
                body += &format!("    c.x += float({call}(0).x);\n");
                continue;
            }
            "hz_shadow_" => "(vec3(texUV, 0.5))",
            _ => "(vec3(texUV, 0.5), 0.0)",
        };
        body += &format!("    c += {call}{args};\n");
    }
    body += "    hz_gl_FragData[0] = c;\n    hz_gl_FragData[1] = c;\n}\n";
    fragment.body = body;
    fragment.uniforms.clear();
    fragment.samplers = calls
        .iter()
        .map(|c| split_sampling_call(c).unwrap().1.to_string())
        .collect();
    fragment.sample_calls = calls.iter().map(|s| s.to_string()).collect();
    (vertex, fragment)
}

#[test]
fn sampling_kinds_follow_the_plan() {
    use crate::plan::BufferFormat;
    let mut plan = PackPlan::default();
    plan.shadow.hardware_filtering[0] = true;
    plan.shadow.tex_mipmap[1] = true;
    plan.buffers[1].format = BufferFormat::Rgba32F;
    plan.buffers[2].format = BufferFormat::Rgb16;
    plan.buffers[5].format = BufferFormat::R11G11B10F;
    plan.mipmaps.insert(ProgramKind::Composite(1), [6].into());
    plan.draw_buffers.insert(ProgramKind::Composite(1), vec![4]);
    plan.custom_textures.push(crate::plan::CustomTexture {
        stage: crate::plan::TextureStage::Composite,
        sampler: "colortex4".into(),
        path: "shaders/textures/a.png".into(),
    });
    let calls = [
        "hz_shadow_shadow",
        "hz_shadowLod_shadow",
        "hz_sample_shadowtex0",
        "hz_sampleLod_shadowtex1",
        "hz_sample_shadowtex1",
        "hz_shadow_shadowtex1",
        "hz_sample_gaux3",
        "hz_sampleGrad_colortex6",
        "hz_sampleBias_colortex6",
        "hz_fetch_colortex6",
        "hz_sample_gdepth",
        "hz_sampleLod_gdepth",
        "hz_sample_gnormal",
        "hz_sample_shadowcolor",
        "hz_sample_noisetex",
        "hz_sample_gdepthtex",
        "hz_size_depthtex1",
        "hz_sample_gaux1",
        "hz_sample_mysteryTexture",
    ];
    let program = composite_program(&calls);
    let (out, _, fs) = run(ProgramKind::Composite(2), &plan, &[0, 5], &program);
    let got: Vec<(u32, TextureSource, SamplerKind)> = out
        .textures
        .iter()
        .map(|t| (t.binding, t.source.clone(), t.sampler))
        .collect();
    use TextureSource::*;
    assert_eq!(
        got,
        vec![
            (2, Buffer(0), SamplerKind::Linear),
            (3, Buffer(1), SamplerKind::Point),
            (4, Buffer(2), SamplerKind::Linear),
            // An earlier composite wrote colortex4: the custom texture no longer applies.
            (6, Buffer(4), SamplerKind::Linear),
            (8, Buffer(6), SamplerKind::Trilinear),
            (18, Depth(0), SamplerKind::Point),
            (19, Depth(1), SamplerKind::Point),
            (21, ShadowTex(0), SamplerKind::Point),
            (23, ShadowColor(0), SamplerKind::Linear),
            (25, Noise, SamplerKind::LinearRepeat),
            (28, ShadowTex(0), SamplerKind::Compare),
            (31, ShadowTexMips(1), SamplerKind::Point),
        ]
    );
    assert!(out.textures.iter().all(|t| t.fragment && !t.vertex));
    assert!(fs.contains("texture_depth_2d") && fs.contains("sampler_comparison"));
    assert!(fs.contains("textureSampleCompareLevel"));
    // RGB16 reads alpha 1; RGBA32F is filtered in the shader; colortex6 has mips.
    assert!(
        out.fragment
            .contains("vec4 hz_post_4(vec4 c) {\n    return vec4(c.rgb, 1.0);")
    );
    assert!(out.fragment.contains("hz_bilinear_3("));
    assert!(out.fragment.contains("hz_trilinear_31("));
    assert!(out.fragment.contains("hz_lod_8(dFdx(uv), dFdy(uv)) + bias"));
    // Output 1 is the R11G11B10F colortex5: a float target.
    assert!(out.fragment.contains("hz_out_1 = hz_finite("));
    assert!(!out.fragment.contains("discard"));

    // Before the pass that turns them on, colortex6 has no mip sampling; the custom texture applies.
    let (early, _, _) = run(ProgramKind::Composite(0), &plan, &[0], &program);
    let find = |out: &HostOutput, b: u32| {
        out.textures
            .iter()
            .find(|t| t.binding == b)
            .cloned()
            .unwrap()
    };
    assert_eq!(find(&early, 8).sampler, SamplerKind::Linear);
    assert_eq!(find(&early, 6).source, Custom(0));
    assert_eq!(find(&early, 6).sampler, SamplerKind::PointRepeat);
}

#[test]
fn watershadow_moves_shadow_to_shadowtex1() {
    let mut program = composite_program(&["hz_sample_shadow", "hz_sample_watershadow"]);
    program.1.samplers.insert("watershadow".into());
    let (out, _, _) = run(ProgramKind::Final, &PackPlan::default(), &[0], &program);
    let sources: Vec<_> = out.textures.iter().map(|t| t.source.clone()).collect();
    assert_eq!(
        sources,
        vec![TextureSource::ShadowTex(0), TextureSource::ShadowTex(1)]
    );
}

#[test]
fn fullscreen_is_a_four_vertex_strip() {
    let (out, vs, _) = run(
        ProgramKind::Final,
        &PackPlan::default(),
        &[0],
        &final_program(),
    );
    assert!(
        out.vertex
            .contains("vec2(float(gl_VertexIndex & 1), float(gl_VertexIndex >> 1))")
    );
    assert!(!vs.contains("@location(0) hz_in"));
    // Unknown samplers and the atlas names read colortex0 in a full-screen pass.
    let program = composite_program(&[
        "hz_sample_gtexture",
        "hz_sample_whatever",
        "hz_sample_colortex0",
    ]);
    let (out, _, _) = run(
        ProgramKind::Composite(0),
        &PackPlan::default(),
        &[0],
        &program,
    );
    assert_eq!(out.textures.len(), 1);
    assert_eq!(out.textures[0].source, TextureSource::Buffer(0));
}

#[test]
fn every_entity_kind_translates_with_its_alpha_test() {
    for kind in ProgramKind::GEOMETRY {
        if kind.geometry() != Geometry::Entity {
            continue;
        }
        let (out, _, _) = run(kind, &PackPlan::default(), &[0, 1], &entity_program());
        let tested = out.fragment.contains("discard");
        let untested = matches!(
            kind,
            ProgramKind::SkyBasic | ProgramKind::SkyTextured | ProgramKind::Basic
        );
        assert_eq!(tested, !untested, "{kind:?}");
        // Cracks and beacon beams wrap their atlas UVs per fragment, as entity.wgsl does.
        let wraps = matches!(kind, ProgramKind::DamagedBlock | ProgramKind::Textured);
        assert_eq!(out.fragment.contains("hz_atlas_uv(uv)"), wraps, "{kind:?}");
        assert_eq!(
            out.vertex.contains("crumbling_sprite.zw"),
            wraps,
            "{kind:?}"
        );
    }
}

#[test]
fn sprite_grad_and_bias() {
    for host in [
        HostParams::default(),
        HostParams {
            sprite_grid: 4,
            ..Default::default()
        },
        HostParams {
            sprite_arrays: 3,
            sprite_split: 1024,
            ..Default::default()
        },
    ] {
        let mut program = terrain_program();
        program.1.body = program.1.body.replace(
            "vec4 albedo = ",
            "vec4 albedo = hz_sampleGrad_gtexture(texUV, dFdx(texUV), dFdy(texUV)) * hz_sampleBias_tex(texUV, -1.0) * ",
        );
        program
            .1
            .sample_calls
            .insert("hz_sampleGrad_gtexture".into());
        program.1.sample_calls.insert("hz_sampleBias_tex".into());
        compile(ProgramKind::Terrain, &host, &program);
    }
}

/// A terrain vertex stage reading every attribute of the pack stream.
fn pack_stream_program() -> (Rewritten, Rewritten) {
    let (mut vertex, fragment) = terrain_program();
    vertex.attributes = vec![
        var("mc_Entity", "vec2", false),
        var("mc_midTexCoord", "vec2", false),
        var("at_midBlock", "vec4", false),
        var("at_tangent", "vec4", false),
    ];
    vertex.body = vertex.body.replace(
        "    level = int(at_midBlock.w);",
        "    level = int(at_midBlock.w + at_midBlock.x + mc_Entity.y);
    normal += at_tangent.xyz + vec3(mc_midTexCoord, 0.0);",
    );
    (vertex, fragment)
}

#[test]
fn terrain_reads_the_pack_vertex_stream() {
    for kind in [ProgramKind::Terrain, ProgramKind::Shadow] {
        let (out, vs, _) = run(kind, &PackPlan::default(), &[0], &pack_stream_program());
        // Slot 2's attributes, as the client's pipelines lay them out.
        for input in [
            "@location(6) hz_in_mid_uv: vec2<u32>",
            "@location(7) hz_in_block: vec2<u32>",
            "@location(8) hz_in_normal: vec4<f32>",
        ] {
            assert!(vs.contains(input), "{kind:?} {input}\n{vs}");
        }
        for line in [
            // block.properties' unlisted id is 0; the fluid bit is mc_Entity.y.
            "mc_Entity = vec2(vec4(hz_in_block.x == 65535u ? 0.0 : float(hz_in_block.x), \
             (hz_in_block.y & 4096u) != 0u ? 1.0 : 0.0, 0.0, 1.0).xy);",
            // The same transform as gl_MultiTexCoord0's.
            "mc_midTexCoord = vec2(vec4(vec2(hz_in_mid_uv) * hz_in_origin.w, 0.0, 1.0).xy);",
            "round(hz_in_normal.w * 127.0)",
            "(hz_in_block.y & 8192u) != 0u ? vec3(0.0)",
            "at_tangent = vec4(hz_terrain_tangent(",
            "hz_gl_Normal = dot(hz_in_normal.xyz, hz_in_normal.xyz) > 0.01 ? normalize(hz_in_normal.xyz)",
        ] {
            assert!(out.vertex.contains(line), "{kind:?} {line}\n{}", out.vertex);
        }
        // No alpha tag is read any more.
        assert!(!out.vertex.contains("hz_in_color.a"), "{}", out.vertex);
    }
}
