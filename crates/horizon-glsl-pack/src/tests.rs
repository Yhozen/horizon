//! `compile` on synthetic in-memory packs.

use super::*;
use std::collections::BTreeSet;

const GBUFFERS_VSH: &str = "#version 120
varying vec2 uv;
varying vec4 tint;
void main() {
    gl_Position = ftransform();
    uv = gl_MultiTexCoord0.st;
    tint = gl_Color;
}
";

/// No samplers: every geometry's host can run it.
const GBUFFERS_FSH: &str = "#version 120
varying vec2 uv;
varying vec4 tint;
void main() {
    gl_FragData[0] = vec4(uv, 1.0, 1.0) * tint;
}
";

const PASS_VSH: &str = "#version 120
varying vec2 uv;
void main() {
    gl_Position = ftransform();
    uv = gl_MultiTexCoord0.st;
}
";

const PASS_FSH: &str = "#version 120
uniform sampler2D colortex0;
varying vec2 uv;
void main() {
    gl_FragColor = texture2D(colortex0, uv);
}
";

/// A fragment stage that never translates (an undeclared name).
const BROKEN_FSH: &str = "#version 120
void main() {
    gl_FragData[0] = vec4(not_declared_anywhere);
}
";

fn pack(files: &[(&str, &str)]) -> PackFiles {
    PackFiles(
        files
            .iter()
            .map(|(p, t)| (p.to_string(), t.as_bytes().to_vec()))
            .collect(),
    )
}

/// `name.vsh` + `name.fsh` under `shaders/`.
fn program<'a>(name: &str, vsh: &'a str, fsh: &'a str) -> [(String, &'a str); 2] {
    [
        (format!("shaders/{name}.vsh"), vsh),
        (format!("shaders/{name}.fsh"), fsh),
    ]
}

fn pack_of(programs: &[[(String, &str); 2]], extra: &[(&str, &str)]) -> PackFiles {
    let mut files: Vec<(&str, &str)> = programs
        .iter()
        .flatten()
        .map(|(p, t)| (p.as_str(), *t))
        .collect();
    files.extend_from_slice(extra);
    pack(&files)
}

fn compile_all(files: &PackFiles, options: &PackOptions) -> Result<CompiledPack, PackError> {
    compile(
        files,
        "minecraft:overworld",
        options,
        &HostParams::default(),
    )
}

#[test]
fn slots() {
    let all = ProgramKind::all();
    assert_eq!(all.len(), 17 + 15 + 3 * 100 + 1);
    assert_eq!(
        [
            ProgramKind::Composite(0).name(),
            ProgramKind::Deferred(3).name(),
            ProgramKind::Water.name(),
            ProgramKind::ShadowEntities.name(),
        ],
        [
            "composite",
            "deferred3",
            "gbuffers_water",
            "shadow_entities"
        ]
    );
    assert!(ProgramKind::Composite(5).is_essential() && !ProgramKind::Deferred(0).is_essential());
}

#[test]
fn every_slot_present_compiles_and_failures_are_skipped() {
    let files = pack_of(
        &[
            program("gbuffers_terrain", GBUFFERS_VSH, GBUFFERS_FSH),
            program("gbuffers_entities", GBUFFERS_VSH, BROKEN_FSH),
            program("composite", PASS_VSH, PASS_FSH),
            program("composite2", PASS_VSH, PASS_FSH),
            program("final", PASS_VSH, PASS_FSH),
        ],
        &[],
    );
    let pack = compile_all(&files, &PackOptions::default()).unwrap();
    // Water, block and damagedblock fall back to gbuffers_terrain; nothing
    // else falls back to it (or to anything the pack has).
    use ProgramKind as K;
    let slots: Vec<_> = pack.programs.keys().copied().collect();
    assert_eq!(
        slots,
        [
            K::Terrain,
            K::Water,
            K::Block,
            K::DamagedBlock,
            K::Composite(0),
            K::Composite(2),
            K::Final
        ]
    );
    assert_eq!(
        pack.plan.programs.keys().copied().collect::<Vec<_>>(),
        slots
    );
    assert_eq!(pack.plan.programs[&K::Water], "shaders/gbuffers_terrain");
    assert_eq!(pack.programs[&K::Water].source, "shaders/gbuffers_terrain");
    // Same file and geometry, but water's alpha test is its own.
    assert_ne!(
        pack.programs[&K::Terrain].fragment_wgsl,
        pack.programs[&K::Water].fragment_wgsl
    );
    assert_eq!(
        pack.programs[&K::Terrain].vertex_wgsl,
        pack.programs[&K::Water].vertex_wgsl
    );
    assert_eq!(pack.skipped.len(), 1);
    assert_eq!(pack.skipped[0].0, K::Entities);
    assert!(
        pack.skipped[0].1.contains("gbuffers_entities"),
        "{:?}",
        pack.skipped
    );
    assert!(!pack.plan.draw_buffers.contains_key(&K::Entities));
    assert_eq!(pack.plan.draw_buffers[&K::Final], [0]);
    assert_eq!(
        pack.plan.fullscreen_passes(),
        [K::Composite(0), K::Composite(2), K::Final]
    );
    assert_eq!(pack.shadow, ShadowSettings::default());
}

#[test]
fn essential_failures_fail_the_pack() {
    // (A broken gbuffers_water falls back to gbuffers_terrain: see session_tests.)
    for name in ["composite1", "final"] {
        let files = pack_of(
            &[
                program("gbuffers_terrain", GBUFFERS_VSH, GBUFFERS_FSH),
                program("final", PASS_VSH, PASS_FSH),
                program(name, PASS_VSH, BROKEN_FSH),
            ],
            &[],
        );
        let e = compile_all(&files, &PackOptions::default()).unwrap_err();
        assert!(e.file.contains(name), "{name}: {e}");
    }
    // A missing include in a non-essential program skips it too.
    let files = pack_of(
        &[
            program("gbuffers_terrain", GBUFFERS_VSH, GBUFFERS_FSH),
            program(
                "gbuffers_hand",
                "#include \"/missing.glsl\"\n",
                GBUFFERS_FSH,
            ),
        ],
        &[],
    );
    let pack = compile_all(&files, &PackOptions::default()).unwrap();
    assert_eq!(pack.skipped[0].0, ProgramKind::Hand);
    assert!(pack.skipped[0].1.contains("missing"), "{:?}", pack.skipped);
}

#[test]
fn subsets_are_strict() {
    let files = pack_of(
        &[
            program("gbuffers_terrain", GBUFFERS_VSH, GBUFFERS_FSH),
            program("gbuffers_entities", GBUFFERS_VSH, BROKEN_FSH),
            program("final", PASS_VSH, PASS_FSH),
            program(
                "composite",
                PASS_VSH,
                "#version 120\nconst int colortex3Format = RGBA16F;\nvoid main() { gl_FragData[0] = vec4(1.0); }\n",
            ),
        ],
        &[],
    );
    let kinds = |k: &[ProgramKind]| {
        compile_kinds(
            &files,
            "minecraft:overworld",
            k,
            &PackOptions::default(),
            &HostParams::default(),
        )
    };
    let pack = kinds(&[ProgramKind::Terrain, ProgramKind::Final]).unwrap();
    assert_eq!(pack.programs.len(), 2);
    assert_eq!(pack.plan.programs.len(), 2);
    // Pack-wide directives come from programs outside the subset too.
    assert_eq!(pack.plan.buffers[3].format, plan::BufferFormat::Rgba16F);
    assert!(kinds(&[ProgramKind::Entities]).is_err());
    let e = kinds(&[ProgramKind::Shadow]).unwrap_err();
    assert!(e.message.contains("Shadow"), "{e}");
}

#[test]
fn options_and_directives_reach_the_plan() {
    let lib = "#define FANCY_ENTITIES // [deliberately unused list]\n\
               const int shadowMapResolution = 2048; // [1024 2048 4096]\n\
               const float sunPathRotation = -30.0;\n";
    let entities_fsh = "#version 120\n#include \"/lib/settings.glsl\"\n\
                        varying vec2 uv;\nvarying vec4 tint;\n\
                        void main() {\n#ifdef FANCY_ENTITIES\n    gl_FragData[0] = vec4(not_declared);\n\
                        #else\n    gl_FragData[0] = vec4(uv, 0.0, 1.0) * tint;\n#endif\n}\n";
    let shadow_fsh = "#version 120\n#include \"/lib/settings.glsl\"\nvoid main() {\n    gl_FragData[0] = vec4(1.0);\n}\n";
    let composite_fsh = "#version 120\nuniform sampler2D colortex0;\nvarying vec2 uv;\n\
                         const bool colortex0MipmapEnabled = true;\n/* const int colortex2Format = RGB16; */\n\
                         void main() {\n    gl_FragData[0] = texture2D(colortex0, uv);\n    gl_FragData[2] = vec4(0.0);\n}\n";
    let properties = "oldLighting=true\nseparateAo=true\ntexture.gbuffers.gaux1=textures/r.png\n\
                      #ifndef FANCY_ENTITIES\nprogram.gbuffers_hand.enabled=false\n#endif\n";
    let files = pack_of(
        &[
            program("gbuffers_terrain", GBUFFERS_VSH, GBUFFERS_FSH),
            program("gbuffers_entities", GBUFFERS_VSH, entities_fsh),
            program("gbuffers_hand", GBUFFERS_VSH, GBUFFERS_FSH),
            program("shadow", GBUFFERS_VSH, shadow_fsh),
            program("composite", PASS_VSH, composite_fsh),
            program("final", PASS_VSH, PASS_FSH),
        ],
        &[
            ("shaders/lib/settings.glsl", lib),
            ("shaders/shaders.properties", properties),
        ],
    );
    use ProgramKind as K;
    let pack = compile_all(&files, &PackOptions::default()).unwrap();
    assert_eq!(
        pack.skipped.iter().map(|s| s.0).collect::<Vec<_>>(),
        [K::Entities]
    );
    assert_eq!(pack.plan.programs[&K::Hand], "shaders/gbuffers_hand");
    assert_eq!(pack.plan.shadow.resolution, 2048);
    assert_eq!(pack.shadow.sun_path_rotation, -30.0);
    assert_eq!(pack.plan.draw_buffers[&K::Shadow], [0, 1]);
    assert_eq!(pack.plan.draw_buffers[&K::Composite(0)], [0, 1, 2]);
    assert_eq!(pack.plan.mipmaps[&K::Composite(0)], BTreeSet::from([0]));
    assert!(!pack.plan.mipmaps.contains_key(&K::Final));
    assert_eq!(pack.plan.buffers[2].format, plan::BufferFormat::Rgb16);
    assert!(pack.plan.properties.old_lighting && pack.plan.properties.separate_ao);
    assert_eq!(pack.plan.custom_textures[0].sampler, "colortex4");
    let names: Vec<_> = pack.plan.options.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["FANCY_ENTITIES", "shadowMapResolution"]);

    // Off: entities compile, the properties' `#ifndef` disables the hand
    // program (Hand falls back to nothing the pack has… but terrain is not
    // in its chain), and the const option takes the new value.
    let options = PackOptions {
        overrides: vec![
            ("FANCY_ENTITIES".into(), None),
            ("shadowMapResolution".into(), Some("4096".into())),
        ],
        ..PackOptions::default()
    };
    let pack = compile_all(&files, &options).unwrap();
    assert!(pack.skipped.is_empty(), "{:?}", pack.skipped);
    assert!(pack.programs.contains_key(&K::Entities));
    assert!(!pack.plan.programs.contains_key(&K::Hand));
    assert_eq!(pack.plan.shadow.resolution, 4096);
    // The plan lists the defaults, not the overridden values.
    let o = &pack.plan.options;
    assert!(o[0].enabled && o[1].value.as_deref() == Some("2048"));
}

#[test]
fn world_folders_and_disabled_world_programs() {
    let files = pack_of(
        &[
            program("gbuffers_terrain", GBUFFERS_VSH, GBUFFERS_FSH),
            program("world0/gbuffers_terrain", GBUFFERS_VSH, GBUFFERS_FSH),
            program("world0/gbuffers_water", GBUFFERS_VSH, GBUFFERS_FSH),
            program("final", PASS_VSH, PASS_FSH),
        ],
        &[(
            "shaders/shaders.properties",
            "program.world0/gbuffers_water.enabled=false\n",
        )],
    );
    let pack = compile_all(&files, &PackOptions::default()).unwrap();
    use ProgramKind as K;
    assert_eq!(
        pack.plan.programs[&K::Terrain],
        "shaders/world0/gbuffers_terrain"
    );
    assert_eq!(
        pack.plan.programs[&K::Water],
        "shaders/world0/gbuffers_terrain"
    );
    let nether = compile(
        &files,
        "minecraft:the_nether",
        &PackOptions::default(),
        &HostParams::default(),
    )
    .unwrap();
    assert_eq!(
        nether.plan.programs[&K::Terrain],
        "shaders/gbuffers_terrain"
    );
}

#[test]
fn the_plan_without_compiling() {
    let files = pack_of(
        &[
            program("gbuffers_terrain", GBUFFERS_VSH, BROKEN_FSH),
            program(
                "composite1",
                PASS_VSH,
                "/* DRAWBUFFERS:37 */\nconst bool colortex3MipmapEnabled = true;\n",
            ),
        ],
        &[],
    );
    let plan = read_plan(&files, "minecraft:overworld", &PackOptions::default()).unwrap();
    use ProgramKind as K;
    // Terrain, water, block, damagedblock (all gbuffers_terrain) and composite1.
    assert_eq!(plan.programs.len(), 5, "{:?}", plan.programs);
    assert_eq!(plan.draw_buffers[&K::Composite(1)], [3, 7]);
    assert_eq!(plan.mipmaps[&K::Composite(1)], BTreeSet::from([3]));
}

/// A deferred pass with `body` as its `main` after `decls`, the pack's only
/// non-essential full-screen program.
fn sampling_pack(decls: &str, body: &str, properties: &str, extra: &[(&str, &str)]) -> PackFiles {
    let fsh = format!(
        "#version 130\n{decls}\nvarying vec2 uv;\nvoid main() {{\n    vec4 c = vec4(0.0);\n{body}\n    gl_FragData[0] = c;\n}}\n"
    );
    let fsh: &'static str = Box::leak(fsh.into_boxed_str());
    let mut files = vec![("shaders/shaders.properties", properties)];
    files.extend_from_slice(extra);
    pack_of(
        &[
            program("gbuffers_terrain", GBUFFERS_VSH, GBUFFERS_FSH),
            program("deferred", PASS_VSH, fsh),
            program("final", PASS_VSH, PASS_FSH),
        ],
        &files,
    )
}

/// The deferred program, or the reason it was skipped.
fn deferred(files: &PackFiles) -> Result<CompiledProgram, String> {
    let pack = compile_all(files, &PackOptions::default()).unwrap();
    match pack.programs.get(&ProgramKind::Deferred(0)) {
        Some(p) => Ok(p.clone()),
        None => Err(format!("{:?}", pack.skipped)),
    }
}

#[test]
fn every_2d_sampling_form_translates() {
    let decls = "uniform sampler2D colortex1;\nuniform sampler2D noisetex;\nuniform sampler2D depthtex0;\n\
                 uniform sampler2DShadow shadowtex0;\nuniform sampler2D shadowtex1;\n\
                 const bool shadowHardwareFiltering0 = true;";
    let body = "    ivec2 o = ivec2(int(uv.x * 4.0), 1);\n\
        c += texture(colortex1, uv) + texture(colortex1, uv, 1.0) + texture2D(colortex1, uv, -1.0);\n\
        c += textureLod(colortex1, uv, 0.0) + texture2DLod(colortex1, uv, 2.0);\n\
        c += textureGrad(colortex1, uv, dFdx(uv), dFdy(uv)) + texture2DGradARB(noisetex, uv, vec2(0.0), vec2(0.0));\n\
        c += texelFetch(colortex1, ivec2(2, 3), 0) + texelFetchOffset(colortex1, ivec2(2), 0, ivec2(1, -1));\n\
        c += textureOffset(colortex1, uv, ivec2(1, 0)) + textureOffset(colortex1, uv, o, 0.5);\n\
        c += textureLodOffset(noisetex, uv, 0.0, ivec2(-1, 2)) + textureGradOffset(noisetex, uv, vec2(0.0), vec2(0.0), o);\n\
        c += textureGather(colortex1, uv) + textureGather(noisetex, uv, 2) + textureGatherOffset(colortex1, uv, o, 1);\n\
        c += textureGather(depthtex0, uv, 0);\n\
        c.xy += vec2(textureSize(colortex1, 0)) + textureQueryLod(colortex1, uv) + vec2(textureSize(shadowtex0, 0));\n\
        c += textureProj(colortex1, vec3(uv, 2.0)) + texture2DProjLod(noisetex, vec4(uv, 0.0, 2.0), 0.0);\n\
        float s = texture(shadowtex0, vec3(uv, 0.5)) + textureLod(shadowtex0, vec3(uv, 0.5), 0.0);\n\
        s += textureProj(shadowtex0, vec4(uv, 0.5, 1.0)) + texture(shadowtex0, vec3(uv, 0.5)).x;\n\
        s += textureOffset(shadowtex0, vec3(uv, 0.5), ivec2(1)) + shadow2D(shadowtex1, vec3(uv, 0.5)).r;\n\
        s += shadow2DLod(shadowtex1, vec3(uv, 0.5), 0.0).r + texture(shadowtex1, uv).r;\n\
        c += textureGather(shadowtex0, uv, 0.5) + textureGatherOffset(shadowtex0, uv, 0.5, ivec2(1)) + vec4(s);";
    let p = deferred(&sampling_pack(decls, body, "", &[])).unwrap();
    let got: Vec<(TextureSource, SamplerKind, SampleKind)> = p
        .textures
        .iter()
        .map(|t| (t.source.clone(), t.sampler, t.sample))
        .collect();
    use TextureSource::*;
    assert_eq!(
        got,
        [
            (Buffer(1), SamplerKind::Linear, SampleKind::Float),
            (Depth(0), SamplerKind::Point, SampleKind::Float),
            (ShadowTex(0), SamplerKind::Point, SampleKind::Float),
            (ShadowTex(1), SamplerKind::Point, SampleKind::Float),
            (Noise, SamplerKind::LinearRepeat, SampleKind::Float),
            (ShadowTex(0), SamplerKind::Compare, SampleKind::Depth),
        ]
    );
    assert!(p.fragment_wgsl.contains("texture_depth_2d"));
    assert!(p.notes.is_empty(), "{:?}", p.notes);
}

#[test]
fn integer_3d_and_custom_samplers() {
    let decls = "uniform sampler3D cloudNoise;\nuniform usampler2D ids;\nuniform isampler3D volume;\n\
                 uniform sampler2D lut;\nuniform sampler3D missing3d;\nuniform usampler2D missingIds;\n\
                 uniform sampler2D mysteryTexture;\nuniform sampler2D gaux2;\nuniform sampler2D shadowcolor1;";
    let body = "    c += texture(cloudNoise, vec3(uv, 0.5)) + texture3D(cloudNoise, vec3(uv, 0.5), 1.0);\n\
        c += textureLod(cloudNoise, vec3(uv, 0.5), 0.0) + texelFetch(cloudNoise, ivec3(1, 2, 3), 0);\n\
        c += textureOffset(cloudNoise, vec3(uv, 0.5), ivec3(1)) + textureGrad(cloudNoise, vec3(uv, 0.5), vec3(0.0), vec3(0.0));\n\
        c.xyz += vec3(textureSize(cloudNoise, 0)) + vec3(textureSize(volume, 0));\n\
        uvec4 i = texelFetch(ids, ivec2(uv * 16.0), 0) + texture(ids, uv) + textureLod(ids, uv, 0.0);\n\
        ivec4 v = texelFetch(volume, ivec3(1), 0) + texture(volume, vec3(uv, 0.0));\n\
        c += vec4(i) + vec4(v) + vec4(textureGather(ids, uv, 1));\n\
        c += texture(lut, uv) + textureGather(lut, uv) + texture(missing3d, vec3(uv, 0.0));\n\
        c += vec4(texture(missingIds, uv)) + texture(mysteryTexture, uv) + texture(gaux2, uv);\n\
        c.xy += textureQueryLod(lut, uv) + textureQueryLod(cloudNoise, vec3(uv, 0.0));\n\
        c += texture(shadowcolor1, uv) + vec4(textureSize(mysteryTexture, 0), 0.0, 0.0);";
    let properties = "customTexture.cloudNoise=tex/noise.dat TEXTURE_3D RGBA8 32 32 32 RGBA UNSIGNED_BYTE\n\
                      customTexture.ids=tex/ids.bin TEXTURE_2D R16UI 16 16 RED_INTEGER UNSIGNED_SHORT\n\
                      customTexture.volume=tex/v.bin TEXTURE_3D RGBA16I 8 8 8 RGBA_INTEGER SHORT\n\
                      texture.deferred.lut=tex/lut.png\n\
                      texture.deferred.gaux2=minecraft:textures/block/stone.png\n";
    let mcmeta = (
        "shaders/tex/lut.png.mcmeta",
        "{\"texture\":{\"blur\":true,\"clamp\":true}}",
    );
    let files = sampling_pack(decls, body, properties, &[mcmeta]);
    let p = deferred(&files).unwrap();
    let got: Vec<(u32, TextureSource, SamplerKind, TexDim, SampleKind)> = p
        .textures
        .iter()
        .map(|t| {
            (
                t.binding,
                t.source.clone(),
                t.sampler,
                t.dimension,
                t.sample,
            )
        })
        .collect();
    use TextureSource::*;
    assert_eq!(
        got,
        [
            (
                7,
                Custom(4),
                SamplerKind::PointRepeat,
                TexDim::D2,
                SampleKind::Float
            ),
            (
                24,
                ShadowColor(1),
                SamplerKind::Linear,
                TexDim::D2,
                SampleKind::Float
            ),
            (
                64,
                Custom(0),
                SamplerKind::Linear,
                TexDim::D3,
                SampleKind::Float
            ),
            (
                65,
                Custom(1),
                SamplerKind::Point,
                TexDim::D2,
                SampleKind::Uint
            ),
            (
                66,
                Custom(2),
                SamplerKind::Point,
                TexDim::D3,
                SampleKind::Sint
            ),
            (
                67,
                Custom(3),
                SamplerKind::Linear,
                TexDim::D2,
                SampleKind::Float
            ),
        ]
    );
    let wgsl = &p.fragment_wgsl;
    for ty in ["texture_3d<f32>", "texture_2d<u32>", "texture_3d<i32>"] {
        assert!(wgsl.contains(ty), "{ty}\n{wgsl}");
    }
    // Three samplers with no texture, each a 1×1 default and a note; never colortex0.
    assert_eq!(p.notes.len(), 3, "{:?}", p.notes);
    assert!(p.notes.iter().any(|n| n.contains("`mysteryTexture`")));
    assert!(!p.textures.iter().any(|t| t.source == Buffer(0)));
}

#[test]
fn sampling_in_the_vertex_stage_and_terrain_gtexture() {
    let vsh = "#version 130\nuniform sampler2D colortex2;\nuniform sampler2D noisetex;\nuniform sampler2D gtexture;\n\
               varying vec2 uv;\nvarying vec4 tint;\n\
               void main() {\n    gl_Position = ftransform();\n    uv = gl_MultiTexCoord0.st;\n    \
               tint = texture(colortex2, uv) + texture2D(noisetex, uv, 1.0) + textureGather(noisetex, uv) \
               + texelFetch(gtexture, ivec2(0), 0) + vec4(textureQueryLod(noisetex, uv), 0.0, 0.0);\n}\n";
    let fsh = "#version 130\nuniform sampler2D gtexture;\nvarying vec2 uv;\nvarying vec4 tint;\n\
               void main() {\n    vec4 c = texelFetch(gtexture, ivec2(uv * 16.0), 0) + textureGather(gtexture, uv, 3);\n    \
               c += textureOffset(gtexture, uv, ivec2(1, 0)) + vec4(textureQueryLod(gtexture, uv), 0.0, 0.0);\n    \
               gl_FragData[0] = c * tint;\n}\n";
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
        let files = pack_of(&[program("gbuffers_terrain", vsh, fsh)], &[]);
        let pack = compile_kinds(
            &files,
            "minecraft:overworld",
            &[ProgramKind::Terrain],
            &PackOptions::default(),
            &host,
        )
        .unwrap();
        let p = &pack.programs[&ProgramKind::Terrain];
        assert!(p.textures.iter().all(|t| t.vertex && !t.fragment));
        assert!(
            !p.vertex_wgsl.contains("textureSample("),
            "{}",
            p.vertex_wgsl
        );
    }
}

#[test]
fn too_many_textures_fail_only_their_program() {
    let decls: String = (0..16)
        .map(|n| format!("uniform sampler2D colortex{n};\n"))
        .collect();
    let body: String = (0..16)
        .map(|n| format!("    c += texture(colortex{n}, uv);\n"))
        .collect();
    let ok = format!("{body}    c += texture(depthtex2, uv);\n");
    // 16 fit; depthtex1 and depthtex2 share one binding without a hand program.
    let p = deferred(&sampling_pack(&decls, &body, "", &[])).unwrap();
    assert_eq!(p.textures.len(), 16);
    let both = format!("{decls}uniform sampler2D depthtex1;\nuniform sampler2D depthtex2;");
    let two = "    c += texture(depthtex1, uv) + texture(depthtex2, uv);";
    let p = deferred(&sampling_pack(&both, two, "", &[])).unwrap();
    assert_eq!(p.textures.len(), 1, "{:?}", p.textures);
    let err = deferred(&sampling_pack(
        &format!("{decls}uniform sampler2D depthtex2;"),
        &ok,
        "",
        &[],
    ))
    .unwrap_err();
    assert!(err.contains("17 textures in the fragment stage"), "{err}");
    assert!(err.contains("depthtex2"), "{err}");
}

#[test]
fn unsupported_sampler_types_fail_their_program() {
    for (decl, call, msg) in [
        (
            "uniform samplerCube sky;",
            "texture(sky, vec3(1.0))",
            "samplerCube sky",
        ),
        (
            "uniform sampler2DArray layers;",
            "texture(layers, vec3(1.0))",
            "sampler2DArray",
        ),
        (
            "uniform sampler3D v;",
            "textureGather(v, vec3(1.0))",
            "3D sampler",
        ),
    ] {
        let body = format!("    c += {call};");
        let err = deferred(&sampling_pack(decl, &body, "", &[])).unwrap_err();
        assert!(err.contains(msg), "{decl}: {err}");
    }
    // Declared and never sampled: harmless.
    let p = deferred(&sampling_pack("uniform samplerCube sky;", "", "", &[])).unwrap();
    assert!(p.textures.is_empty());
}

#[test]
fn entity_atlas_and_lightmap_forms() {
    let fsh = "#version 130\nuniform sampler2D gtexture;\nuniform sampler2D lightmap;\nvarying vec2 uv;\nvarying vec4 tint;\n\
               void main() {\n    vec4 c = textureGather(gtexture, uv, 3) + textureGather(lightmap, uv);\n    \
               c += textureOffset(gtexture, uv, ivec2(1, 0)) + vec4(textureQueryLod(gtexture, uv), textureQueryLod(lightmap, uv));\n    \
               gl_FragData[0] = c * tint;\n}\n";
    let files = pack_of(&[program("gbuffers_entities", GBUFFERS_VSH, fsh)], &[]);
    let pack = compile_kinds(
        &files,
        "minecraft:overworld",
        &[ProgramKind::Entities],
        &PackOptions::default(),
        &HostParams::default(),
    )
    .unwrap();
    assert!(pack.programs[&ProgramKind::Entities].textures.is_empty());
}

#[test]
fn integer_levels_and_biases() {
    let decls = "uniform sampler2D colortex1;\nuniform sampler2DShadow shadowtex0;";
    let body = "    c += textureLod(colortex1, uv, 0) + texture(colortex1, uv, 1) + vec4(textureLod(shadowtex0, vec3(uv, 0.5), 0));";
    deferred(&sampling_pack(decls, body, "", &[])).unwrap();
}

#[test]
fn a_3d_override_leaves_the_2d_buffer() {
    let decls =
        "uniform sampler3D colortex6;\nuniform sampler2D colortex7;\nuniform sampler3D colortex8;";
    let body = "    c += texture(colortex6, vec3(uv, 0.5)) + texture(colortex7, uv) + texture(colortex8, vec3(uv, 0.5));";
    let properties = "texture.deferred.colortex6.1=a.dat TEXTURE_3D R8 4 4 4 RED UNSIGNED_BYTE\n\
                      texture.deferred.colortex7=a.dat TEXTURE_3D R8 4 4 4 RED UNSIGNED_BYTE\n";
    let p = deferred(&sampling_pack(decls, body, properties, &[])).unwrap();
    let got: Vec<(u32, TextureSource, TexDim)> = p
        .textures
        .iter()
        .map(|t| (t.binding, t.source.clone(), t.dimension))
        .collect();
    assert_eq!(
        got,
        [
            (8, TextureSource::Custom(0), TexDim::D3),
            (9, TextureSource::Buffer(7), TexDim::D2),
        ]
    );
    // colortex8 has no 3D texture: the default, with a note.
    assert_eq!(p.notes.len(), 1, "{:?}", p.notes);
}

#[test]
fn custom_uniforms_reach_the_programs() {
    let fsh = "#version 120
uniform sampler2D colortex0;
uniform float dayFactor;
uniform bool isRaining;
uniform vec3 tint;
uniform float rainStrength;
varying vec2 uv;
void main() {
    vec4 c = texture2D(colortex0, uv) * dayFactor;
    if (isRaining) { c.rgb *= tint; }
    gl_FragColor = c + rainStrength;
}
";
    let properties = "#ifdef TINTED\nuniform.vec3.tint=vec3(1.0, 0.9, 0.8)\n#else\nuniform.vec3.tint=vec3(1.0)\n#endif\n\
                      variable.float.day=clamp(sin(sunAngle * 2.0 * pi) * 4.0, 0.0, 1.0)\n\
                      uniform.float.dayFactor=smooth(day, 2, 2)\nuniform.bool.isRaining=rainStrength > 0.5\n";
    let files = pack_of(
        &[program("final", PASS_VSH, fsh)],
        &[
            ("shaders/shaders.properties", properties),
            (
                "shaders/lib/o.glsl",
                "//#define TINTED\n#ifdef TINTED\n#endif\n",
            ),
        ],
    );
    let mut pack = compile_all(&files, &PackOptions::default()).unwrap();
    let program = &pack.programs[&ProgramKind::Final];
    assert!(
        program.unknown_uniforms.is_empty(),
        "{:?}",
        program.unknown_uniforms
    );
    assert!(program.fragment_wgsl.contains("@group(1) @binding(1)"));
    let custom = &mut pack.custom_uniforms;
    assert_eq!(custom.size(), 32);
    assert!(custom.notes.is_empty(), "{:?}", custom.notes);
    let standard = crate::PackUniforms {
        sun_angle: 0.25,
        rain_strength: 1.0,
        ..crate::PackUniforms::default()
    };
    let mut bytes = Vec::new();
    custom.evaluate(&|n| standard.get(n), 0.05, &mut bytes);
    let f = |b: &[u8], o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    // tint (vec3 at 0), dayFactor at 12, isRaining at 16.
    assert_eq!(
        [f(&bytes, 0), f(&bytes, 4), f(&bytes, 8), f(&bytes, 12)],
        [1.0; 4]
    );
    assert_eq!(i32::from_le_bytes(bytes[16..20].try_into().unwrap()), 1);
    // The option's macro reaches shaders.properties.
    let options = PackOptions {
        overrides: vec![("TINTED".into(), Some("true".into()))],
        ..PackOptions::default()
    };
    let mut tinted = compile_all(&files, &options).unwrap();
    tinted
        .custom_uniforms
        .evaluate(&|n| standard.get(n), 0.05, &mut bytes);
    assert_eq!(f(&bytes, 4), 0.9);
}

#[test]
fn integer_colour_buffers_bind_as_integers() {
    let decls = "/*\nconst int colortex5Format = R32UI;\n*/\nuniform usampler2D colortex5;\nuniform sampler2D colortex4;\n\
                 /*\nconst int colortex4Format = RGBA16I;\n*/";
    let body = "    uvec4 id = texelFetch(colortex5, ivec2(0), 0) + texture(colortex5, uv);\n    \
                c += vec4(id) + texture(colortex4, uv);";
    let p = deferred(&sampling_pack(decls, body, "", &[])).unwrap();
    let got: Vec<(TextureSource, SamplerKind, SampleKind)> = p
        .textures
        .iter()
        .map(|t| (t.source.clone(), t.sampler, t.sample))
        .collect();
    assert_eq!(
        got,
        [
            (
                TextureSource::Buffer(4),
                SamplerKind::Point,
                SampleKind::Sint
            ),
            (
                TextureSource::Buffer(5),
                SamplerKind::Point,
                SampleKind::Uint
            ),
        ]
    );
    assert!(
        p.fragment_wgsl.contains("texture_2d<u32>") && p.fragment_wgsl.contains("texture_2d<i32>")
    );
}
