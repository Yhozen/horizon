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
    assert_eq!(all.len(), 17 + 16 + 16 + 1);
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
    for name in ["gbuffers_water", "composite1", "final"] {
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
