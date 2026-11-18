//! [`Session`], fallbacks after a failure and the report, on synthetic packs.

use super::*;
use crate::plan::{AlphaFunc, AlphaTest, Blend, BlendFactor, BufferSize, ParticleOrdering};

const VSH: &str = "#version 120
varying vec2 uv;
void main() {
    gl_Position = ftransform();
    uv = gl_MultiTexCoord0.st;
}
";

const FSH: &str = "#version 120
varying vec2 uv;
void main() {
    gl_FragData[0] = vec4(uv, 1.0, 1.0);
}
";

/// Reads a uniform Horizon does not have.
const ODD_FSH: &str = "#version 120
uniform float oddUniform;
varying vec2 uv;
void main() {
    gl_FragData[0] = vec4(uv, oddUniform, 1.0);
}
";

const BROKEN_FSH: &str = "#version 120
void main() {
    gl_FragData[0] = vec4(not_declared_anywhere);
}
";

/// `shaders/<name>.vsh|fsh` for each `(name, fsh)`, plus `extra` files.
fn programs(list: &[(&str, &str)], extra: &[(&str, &str)]) -> PackFiles {
    let mut files: Vec<(String, String)> = Vec::new();
    for (name, fsh) in list {
        files.push((format!("shaders/{name}.vsh"), VSH.to_string()));
        files.push((format!("shaders/{name}.fsh"), fsh.to_string()));
    }
    for (p, t) in extra {
        files.push((p.to_string(), t.to_string()));
    }
    PackFiles(
        files
            .into_iter()
            .map(|(p, t)| (p, t.into_bytes()))
            .collect(),
    )
}

fn report(files: &PackFiles, options: &PackOptions) -> (Option<CompiledPack>, PackReport) {
    compile_report(
        files,
        "minecraft:overworld",
        options,
        &HostParams::default(),
    )
}

fn outcome<'a>(r: &'a PackReport, slot: &str) -> &'a ProgramOutcome {
    r.programs
        .iter()
        .find(|p| p.slot == slot)
        .unwrap_or_else(|| panic!("no {slot} in {:?}", r.programs))
}

#[test]
fn sessions_are_send_and_step_in_bounded_pieces() {
    fn send<T: Send>() {}
    send::<Session>();
    let files = programs(
        &[
            ("gbuffers_terrain", FSH),
            ("composite", FSH),
            ("final", FSH),
        ],
        &[],
    );
    let mut s = Session::new(
        &files,
        "minecraft:overworld",
        &PackOptions::default(),
        &HostParams::default(),
    );
    let mut steps = 0;
    let mut last = (0, 0);
    while !s.step() {
        steps += 1;
        let p = s.progress();
        assert!(p.0 >= last.0 && p.0 <= p.1, "{p:?}");
        last = p;
        assert!(steps < 100);
    }
    // Setup, three preprocesses, the plan, then one step per slot
    // (terrain, water, block, damagedblock, composite, final).
    assert_eq!(s.progress(), (11, 11));
    let (pack, r) = s.finish();
    let pack = pack.unwrap();
    assert_eq!(r.status, PackStatus::Works, "{}", r.text("t"));
    assert_eq!(pack.programs.len(), 6);
    assert_eq!(r.programs.len(), 6);
    assert_eq!(
        outcome(&r, "gbuffers_water").file.as_deref(),
        Some("shaders/gbuffers_terrain")
    );
}

#[test]
fn a_failing_candidate_falls_back_to_the_next() {
    let files = programs(
        &[
            ("gbuffers_terrain", FSH),
            ("gbuffers_water", BROKEN_FSH),
            ("gbuffers_entities", BROKEN_FSH),
            ("gbuffers_textured_lit", ODD_FSH),
            ("gbuffers_clouds", BROKEN_FSH),
            ("final", FSH),
        ],
        &[],
    );
    let (pack, r) = report(&files, &PackOptions::default());
    let pack = pack.unwrap();
    use ProgramKind as K;
    // Water falls back to terrain, entities to textured_lit.
    assert_eq!(pack.plan.programs[&K::Water], "shaders/gbuffers_terrain");
    assert_eq!(pack.programs[&K::Water].source, "shaders/gbuffers_terrain");
    assert_eq!(
        pack.plan.programs[&K::Entities],
        "shaders/gbuffers_textured_lit"
    );
    let water = outcome(&r, "gbuffers_water");
    assert_eq!(water.file.as_deref(), Some("shaders/gbuffers_terrain"));
    assert_eq!(water.failed.len(), 1);
    assert_eq!(water.failed[0].0, "shaders/gbuffers_water.fsh");
    assert!(water.error.is_none());
    // Clouds: gbuffers_clouds fails, gbuffers_textured and basic do not exist.
    let clouds = outcome(&r, "gbuffers_clouds");
    assert!(
        clouds.file.is_none() && clouds.error.is_some(),
        "{clouds:?}"
    );
    assert_eq!(pack.skipped.len(), 1);
    assert_eq!(pack.skipped[0].0, K::Clouds);
    assert_eq!(r.status, PackStatus::Partial);
    assert_eq!(r.unknown_uniforms, ["oddUniform"]);
    let text = r.text("Test");
    assert!(
        text.contains("gbuffers_water: runs shaders/gbuffers_terrain"),
        "{text}"
    );
    assert!(text.contains("shaders/gbuffers_water.fsh failed"), "{text}");
    assert!(text.contains("Uniforms reading zero: oddUniform"), "{text}");
    // `compile` keeps its result: the pack compiles with the fallbacks.
    let old = compile(
        &files,
        "minecraft:overworld",
        &PackOptions::default(),
        &HostParams::default(),
    )
    .unwrap();
    assert_eq!(old.plan.programs, pack.plan.programs);
}

#[test]
fn an_exhausted_essential_chain_fails_the_pack() {
    let files = programs(
        &[("gbuffers_terrain", FSH), ("composite2", BROKEN_FSH)],
        &[],
    );
    let (pack, r) = report(&files, &PackOptions::default());
    assert!(pack.is_none());
    assert_eq!(r.status, PackStatus::Failed);
    assert!(outcome(&r, "composite2").error.is_some());
    let e = compile(
        &files,
        "minecraft:overworld",
        &PackOptions::default(),
        &HostParams::default(),
    )
    .unwrap_err();
    assert_eq!(e.file, "shaders/composite2.fsh");
}

#[test]
fn excluded_programs_are_missing() {
    let files = programs(
        &[
            ("gbuffers_terrain", FSH),
            ("gbuffers_water", FSH),
            ("world0/gbuffers_water", FSH),
            ("final", FSH),
        ],
        &[],
    );
    let options = PackOptions {
        excluded: ["shaders/world0/gbuffers_water".to_string()].into(),
        ..PackOptions::default()
    };
    let (pack, r) = report(&files, &options);
    assert_eq!(
        pack.unwrap().plan.programs[&ProgramKind::Water],
        "shaders/gbuffers_water"
    );
    assert!(
        r.notes.iter().any(|n| n.contains("world0/gbuffers_water")),
        "{r:?}"
    );
}

#[test]
fn required_features_refuse_and_compile_does_not() {
    let files = programs(
        &[("gbuffers_terrain", FSH), ("final", FSH)],
        &[
            (
                "shaders/shaders.properties",
                "iris.features.required=COMPUTE_SHADERS\n",
            ),
            ("shaders/composite.csh", "#version 430\nvoid main() {}\n"),
        ],
    );
    let (pack, r) = report(&files, &PackOptions::default());
    assert!(pack.is_none());
    assert_eq!(r.status, PackStatus::Refused);
    assert_eq!(
        r.refusal.as_deref(),
        Some("This pack needs compute shaders, which Horizon does not support yet.")
    );
    assert!(
        r.text("x")
            .contains("compute shaders (required; shaders/composite.csh)")
    );
    // The old entry point still compiles what it can.
    assert!(
        compile(
            &files,
            "minecraft:overworld",
            &PackOptions::default(),
            &HostParams::default()
        )
        .is_ok()
    );
    // Nothing to run at all.
    let empty = programs(&[("gbuffers_hand", FSH)], &[]);
    let (pack, r) = report(&empty, &PackOptions::default());
    assert!(pack.is_none());
    assert_eq!(r.status, PackStatus::Refused);
    // Optional features are only listed: the pack works without them.
    let optional = programs(
        &[("gbuffers_terrain", FSH)],
        &[(
            "shaders/shaders.properties",
            "iris.features.optional=SSBO ENTITY_TRANSLUCENT\n\
                 #ifdef IRIS_FEATURE_BLOCK_EMISSION_ATTRIBUTE\nsun=false\n#endif\n\
                 iris.features.optional=BLOCK_EMISSION_ATTRIBUTE\n",
        )],
    );
    let (pack, r) = report(&optional, &PackOptions::default());
    let pack = pack.unwrap();
    assert_eq!(r.refusal, None);
    // The flag Horizon has is a macro, for the properties too.
    assert!(!pack.plan.properties.sun);
    assert!(
        r.features
            .iter()
            .all(|f| f.name != "the gbuffers_entities_translucent program")
    );
}

#[test]
fn feature_macros_reach_the_sources() {
    let fsh = "#version 120\nvoid main() {\n#ifdef IRIS_FEATURE_BLOCK_EMISSION_ATTRIBUTE\n\
               gl_FragData[0] = vec4(1.0);\n#else\n    gl_FragData[0] = vec4(nope);\n#endif\n}\n";
    let files = programs(
        &[("gbuffers_terrain", fsh)],
        &[(
            "shaders/shaders.properties",
            "iris.features.optional=BLOCK_EMISSION_ATTRIBUTE\n",
        )],
    );
    let (pack, r) = report(&files, &PackOptions::default());
    assert!(pack.is_some(), "{}", r.text("x"));
    assert_eq!(r.status, PackStatus::Works);
}

#[test]
fn iris_slots_run_only_with_their_own_file() {
    let files = programs(
        &[
            ("gbuffers_terrain", FSH),
            ("gbuffers_terrain_cutout", FSH),
            ("gbuffers_entities", FSH),
            ("shadow", FSH),
            ("shadow_water", FSH),
            ("prepare", FSH),
            ("composite57", FSH),
            ("final", FSH),
        ],
        &[],
    );
    let (pack, r) = report(&files, &PackOptions::default());
    let pack = pack.unwrap();
    use ProgramKind as K;
    let slots: Vec<K> = pack.plan.programs.keys().copied().collect();
    for k in [
        K::TerrainCutout,
        K::ShadowWater,
        K::Prepare(0),
        K::Composite(57),
    ] {
        assert!(slots.contains(&k), "{k:?}: {slots:?}");
    }
    // Without their own file, the split slots are left to their parents.
    for k in [
        K::TerrainSolid,
        K::Lightning,
        K::ShadowSolid,
        K::EntitiesTranslucent,
    ] {
        assert!(!slots.contains(&k), "{k:?}");
    }
    assert_eq!(pack.plan.programs[&K::ShadowEntities], "shaders/shadow");
    assert_eq!(
        pack.plan.fullscreen_passes(),
        [K::Prepare(0), K::Composite(57), K::Final]
    );
    assert_eq!(r.status, PackStatus::Works, "{}", r.text("x"));
}

#[test]
fn program_enabled_expressions() {
    let props = "program.composite.enabled = A && !B\n\
                 program.composite1.enabled = (B || C) && QUALITY > 1\n\
                 program.composite2.enabled = defined(B) || !A\n\
                 program.gbuffers_water.enabled = false\n\
                 program.world0/gbuffers_water.enabled = true\n\
                 program.gbuffers_hand.enabled = false\n";
    let lib = "#define A\n//#define B\n#define C\n#define QUALITY 2 // [1 2 3]\n\
               #ifdef A\n#endif\n#ifdef B\n#endif\n#ifdef C\n#endif\n";
    let files = programs(
        &[
            ("gbuffers_terrain", FSH),
            ("gbuffers_water", FSH),
            ("world0/gbuffers_water", FSH),
            ("gbuffers_hand", FSH),
            ("world0/gbuffers_hand", FSH),
            ("composite", FSH),
            ("composite1", FSH),
            ("composite2", FSH),
            ("final", FSH),
        ],
        &[
            ("shaders/shaders.properties", props),
            ("shaders/lib.glsl", lib),
            ("shaders/options.fsh", lib),
        ],
    );
    use ProgramKind as K;
    let plan = read_plan(&files, "minecraft:overworld", &PackOptions::default()).unwrap();
    assert!(plan.programs.contains_key(&K::Composite(0)));
    assert!(plan.programs.contains_key(&K::Composite(1)));
    assert!(!plan.programs.contains_key(&K::Composite(2)));
    // The folder's own key wins; a key without folder covers world0 copies.
    assert_eq!(plan.programs[&K::Water], "shaders/world0/gbuffers_water");
    assert!(!plan.programs.contains_key(&K::Hand), "{:?}", plan.programs);
    // Options change the outcome.
    let options = PackOptions {
        overrides: vec![
            ("B".into(), Some("true".into())),
            ("QUALITY".into(), Some("1".into())),
        ],
        ..PackOptions::default()
    };
    let plan = read_plan(&files, "minecraft:overworld", &options).unwrap();
    assert!(!plan.programs.contains_key(&K::Composite(0)));
    assert!(!plan.programs.contains_key(&K::Composite(1)));
    assert!(plan.programs.contains_key(&K::Composite(2)));
}

#[test]
fn pass_state_from_the_properties() {
    let props = "blend.gbuffers_water = SRC_ALPHA ONE_MINUS_SRC_ALPHA ONE ZERO\n\
                 blend.gbuffers_water.colortex2 = off\n\
                 blend.gbuffers_textured = ONE ONE ONE ONE\n\
                 alphaTest.gbuffers_terrain = GREATER 0.5\n\
                 alphaTest.gbuffers_entities = off\n\
                 scale.composite1 = 0.5 0.25 0.25\n\
                 flip.composite1.gaux1 = false\n\
                 size.buffer.colortex4 = 0.5 0.5\n\
                 size.buffer.colortex5 = 256 128\n\
                 shadow.enabled = false\nshadowEntities = false\nshadow.culling = reversed\n\
                 particles.before.deferred = true\nrain.depth = true\nbeacon.beam.depth=true\n\
                 frustum.culling = false\nweather = false true\n";
    let files = programs(
        &[
            ("gbuffers_terrain", FSH),
            ("gbuffers_entities", FSH),
            ("gbuffers_textured", FSH),
            ("shadow", FSH),
            ("composite1", FSH),
        ],
        &[("shaders/shaders.properties", props)],
    );
    let (pack, r) = report(&files, &PackOptions::default());
    let pack = pack.unwrap();
    let plan = &pack.plan;
    use BlendFactor as F;
    use ProgramKind as K;
    let water = plan.state(K::Water);
    assert_eq!(
        water.blend,
        Some(Blend::On {
            src: F::SrcAlpha,
            dst: F::OneMinusSrcAlpha,
            src_alpha: F::One,
            dst_alpha: F::Zero
        })
    );
    assert_eq!(water.blend_of(2), Some(Blend::Off));
    assert_eq!(water.blend_of(0), water.blend);
    // Clouds run gbuffers_textured: its state follows the file.
    assert_eq!(plan.programs[&K::Clouds], "shaders/gbuffers_textured");
    assert!(matches!(
        plan.state(K::Clouds).blend,
        Some(Blend::On { src: F::One, .. })
    ));
    assert_eq!(
        plan.alpha_test(K::Terrain),
        Some(AlphaTest::Test(AlphaFunc::Greater, 0.5))
    );
    let c1 = plan.state(K::Composite(1));
    assert_eq!(
        c1.scale.map(|s| (s.scale, s.offset)),
        Some((0.5, [0.25, 0.25]))
    );
    assert_eq!(c1.flip.get(&4), Some(&false));
    assert_eq!(plan.buffers[4].size, BufferSize::Relative(0.5, 0.5));
    assert_eq!(plan.buffers[5].size, BufferSize::Pixels(256, 128));
    assert_eq!(BufferSize::Relative(0.5, 0.5).pixels(1001, 600), (501, 300));
    let p = plan.properties;
    assert!(!plan.shadow_enabled() && !p.shadow_entities && p.shadow_terrain);
    assert_eq!(p.shadow_culling, crate::plan::ShadowCulling::Reversed);
    assert_eq!(plan.particle_ordering(), ParticleOrdering::Before);
    assert!(p.rain_depth && p.beacon_beam_depth && !p.frustum_culling);
    assert!(!p.weather && p.weather_particles);
    // The terrain's alpha test is the pack's; entities have none.
    let terrain = &pack.programs[&K::Terrain].fragment_wgsl;
    assert!(terrain.contains("0.5"), "{terrain}");
    let entities = &pack.programs[&K::Entities].fragment_wgsl;
    assert!(!entities.contains("discard"), "{entities}");
    assert!(
        r.features
            .iter()
            .any(|f| f.name == "buffer sizes" && f.supported)
    );
}

#[test]
fn particle_ordering_defaults() {
    let mut plan = PackPlan::default();
    assert_eq!(plan.particle_ordering(), ParticleOrdering::Mixed);
    plan.programs
        .insert(ProgramKind::Deferred(3), "shaders/deferred3".into());
    assert_eq!(plan.particle_ordering(), ParticleOrdering::After);
}

#[test]
fn draw_buffers_come_from_live_branches() {
    let fsh = "#version 120\n#ifdef WIDE\n/* RENDERTARGETS: 0,9,12 */\n#else\n/* DRAWBUFFERS:03 */\n#endif\n\
               /*\n#define WIDE\n*/\nvoid main() {\n    gl_FragData[0] = vec4(1.0);\n    gl_FragData[1] = vec4(0.5);\n}\n";
    let wide = format!(
        "#version 120\n#define WIDE\n{}",
        &fsh["#version 120\n".len()..]
    );
    let files = programs(&[("composite", fsh), ("composite1", &wide)], &[]);
    let plan = read_plan(&files, "minecraft:overworld", &PackOptions::default()).unwrap();
    use ProgramKind as K;
    assert_eq!(plan.draw_buffers[&K::Composite(0)], [0, 3]);
    assert_eq!(plan.draw_buffers[&K::Composite(1)], [0, 9, 12]);
}

#[test]
fn target_limits_fail_the_candidate() {
    let many = "#version 120\n/* RENDERTARGETS: 0,1,2,3,4,5,6,7,8 */\nvoid main() { gl_FragData[0] = vec4(1.0); }\n";
    // 8 + 2 × 16 bytes against a device limit of 32.
    let heavy = "#version 120\n/* RENDERTARGETS: 0,1,2 */\n\
                 const int colortex1Format = RGBA32F;\nconst int colortex2Format = rgba32f;\n\
                 void main() { gl_FragData[0] = vec4(1.0); }\n";
    let high =
        "#version 120\n/* RENDERTARGETS: 0,16 */\nvoid main() { gl_FragData[0] = vec4(1.0); }\n";
    let files = programs(
        &[
            ("gbuffers_terrain", FSH),
            ("gbuffers_entities", many),
            ("gbuffers_hand", heavy),
            ("gbuffers_weather", high),
        ],
        &[],
    );
    let host = HostParams {
        max_attachment_bytes: 32,
        ..HostParams::default()
    };
    let (pack, r) = compile_report(
        &files,
        "minecraft:overworld",
        &PackOptions::default(),
        &host,
    );
    assert!(pack.is_some());
    let err = |slot: &str| outcome(&r, slot).failed[0].1.clone();
    assert!(err("gbuffers_entities").contains("9 draw buffers"), "{r:?}");
    assert!(err("gbuffers_hand").contains("bytes a pixel"), "{r:?}");
    assert!(err("gbuffers_weather").contains("colortex16"), "{r:?}");
}

#[test]
fn outputs_by_location_and_size() {
    let fsh = "#version 330\nlayout(location = 1) out vec2 normalData;\nlayout(location = 0) out vec3 albedo;\n\
               out float extra;\nvoid main() {\n    albedo = vec3(1.0, 0.5, 0.25);\n    normalData = vec2(0.5);\n    extra = 1.0;\n}\n";
    let int_fsh = "#version 330\n/* RENDERTARGETS: 0,6 */\n/* const int colortex6Format = R32UI; */\n\
                   layout(location = 0) out vec4 color;\nlayout(location = 1) out uint id;\n\
                   void main() {\n    color = vec4(1.0);\n    id = 7u;\n}\n";
    let files = programs(
        &[
            ("gbuffers_terrain", FSH),
            ("composite", fsh),
            ("composite1", int_fsh),
            ("final", FSH),
        ],
        &[],
    );
    let (pack, r) = report(&files, &PackOptions::default());
    let pack = pack.unwrap_or_else(|| panic!("{}", r.text("x")));
    use ProgramKind as K;
    let outs = &pack.plan.outputs[&K::Composite(0)];
    assert_eq!(
        outs.iter()
            .map(|o| (o.name.as_str(), o.slot()))
            .collect::<Vec<_>>(),
        [("normalData", 1), ("albedo", 0), ("extra", 2)]
    );
    // No directive: every output up to the highest location.
    assert_eq!(pack.plan.draw_buffers[&K::Composite(0)], [0, 1, 2]);
    assert_eq!(pack.plan.buffers[6].format, plan::BufferFormat::R32UI);
    let wgsl = &pack.programs[&K::Composite(1)].fragment_wgsl;
    assert!(wgsl.contains("vec4<u32>"), "{wgsl}");
    assert_eq!(r.status, PackStatus::Works);
}
