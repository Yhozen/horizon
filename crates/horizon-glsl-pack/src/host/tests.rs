use super::*;
use crate::naga_out::glsl_to_wgsl;
use crate::rewrite::Uniform;

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
    let blocks = BlockIds {
        ids: vec![10008, 10068, 10072],
        ..Default::default()
    };
    let draw_buffers = [0, 6, 7];
    let spec = ProgramSpec {
        kind,
        geometry: kind.geometry(),
        draw_buffers: &draw_buffers,
        host,
        blocks: &blocks,
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
    let mut wide = (
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
    let err = sources(&wide.0, &wide.1, &spec).unwrap_err();
    assert!(err.contains("17 varying locations"), "{err}");
    // A full-screen pass has no hz_sprite: 16 fit.
    wide.0.varyings.pop();
    let fits = ProgramSpec {
        geometry: Geometry::Fullscreen,
        ..spec
    };
    wide.1.varyings.pop();
    assert!(shared_varyings(&wide.0, &wide.1, fits.geometry).is_ok());
}

#[test]
fn unknown_or_mistyped_names_fail() {
    let blocks = BlockIds::default();
    let spec = ProgramSpec {
        kind: ProgramKind::Final,
        geometry: Geometry::Fullscreen,
        draw_buffers: &[0],
        host: &HostParams::default(),
        blocks: &blocks,
    };
    let (vertex, fragment) = final_program();
    let with = |u: &[(&str, &str)]| Rewritten {
        uniforms: uniforms(u),
        ..fragment.clone()
    };
    let err = sources(&vertex, &with(&[("sunPathRotation", "float")]), &spec).unwrap_err();
    assert!(err.contains("sunPathRotation"), "{err}");
    let err = sources(&vertex, &with(&[("worldTime", "float")]), &spec).unwrap_err();
    assert!(err.contains("worldTime"), "{err}");
    let specular = Rewritten {
        sample_calls: set(&["hz_sample_specular"]),
        ..fragment.clone()
    };
    assert!(
        sources(&vertex, &specular, &spec)
            .unwrap_err()
            .contains("specular")
    );
    // gtexture is the block atlas: a full-screen pass has none.
    let gtexture = Rewritten {
        sample_calls: set(&["hz_sample_gtexture"]),
        ..fragment
    };
    assert!(sources(&vertex, &gtexture, &spec).is_err());
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
