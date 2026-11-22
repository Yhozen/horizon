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
                "iris.features.required=TESSELATION_SHADERS\n",
            ),
            (
                "shaders/gbuffers_terrain.tcs",
                "#version 430\nvoid main() {}\n",
            ),
        ],
    );
    let (pack, r) = report(&files, &PackOptions::default());
    assert!(pack.is_none());
    assert_eq!(r.status, PackStatus::Refused);
    assert_eq!(
        r.refusal.as_deref(),
        Some("This pack needs tessellation shaders, which Horizon does not support yet.")
    );
    assert!(
        r.text("x")
            .contains("tessellation shaders (required; shaders/gbuffers_terrain.tcs)")
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

#[test]
fn targets_get_their_format_s_range_and_type() {
    let fsh = "#version 120\n/* RENDERTARGETS: 0,2,3 */\n\
               /* const int colortex2Format = RGBA8_SNORM; const int colortex3Format = rgba16i; */\n\
               void main() {\n    gl_FragData[0] = vec4(1.0);\n    gl_FragData[1] = vec4(-2.0);\n    gl_FragData[2] = vec4(3.0);\n}\n";
    let files = programs(
        &[
            ("gbuffers_terrain", FSH),
            ("composite", fsh),
            ("final", FSH),
        ],
        &[(
            "shaders/shaders.properties",
            "alphaTest.gbuffers_terrain = NEVER 0\n",
        )],
    );
    let (pack, r) = report(&files, &PackOptions::default());
    let pack = pack.unwrap_or_else(|| panic!("{}", r.text("x")));
    use ProgramKind as K;
    let c = &pack.programs[&K::Composite(0)].fragment_wgsl;
    assert!(c.contains("vec4<i32>"), "{c}");
    assert!(c.contains("-1f"), "snorm clamp: {c}");
    // NEVER discards every fragment.
    let t = &pack.programs[&K::Terrain].fragment_wgsl;
    assert!(t.contains("discard"), "{t}");
    assert_ne!(t, &pack.programs[&K::Water].fragment_wgsl);
}

#[test]
fn properties_continuations_stop_at_directives() {
    // A continued line before `#else` must not swallow it.
    let props = "#ifdef NOPE\nprogram.composite.enabled = \\\n  false \\\n#else\nsun = false\n#endif\nmoon = \\\n  false\n";
    let files = programs(
        &[("gbuffers_terrain", FSH), ("composite", FSH)],
        &[("shaders/shaders.properties", props)],
    );
    let plan = read_plan(&files, "minecraft:overworld", &PackOptions::default()).unwrap();
    assert!(plan.programs.contains_key(&ProgramKind::Composite(0)));
    assert!(!plan.properties.sun && !plan.properties.moon);
}

#[test]
fn one_read_then_chosen_slots() {
    let files = programs(
        &[
            ("gbuffers_terrain", FSH),
            ("gbuffers_water", BROKEN_FSH),
            ("composite", BROKEN_FSH),
            ("final", FSH),
        ],
        &[],
    );
    let mut s = Session::read(
        &files,
        "minecraft:overworld",
        &PackOptions::default(),
        &HostParams::default(),
    );
    use ProgramKind as K;
    assert_eq!(
        s.candidates(K::Water),
        ["shaders/gbuffers_water", "shaders/gbuffers_terrain"]
    );
    assert!(s.compile_slot(K::Water, 0).unwrap().is_err());
    let water = s.compile_slot(K::Water, 1).unwrap().unwrap();
    assert_eq!(water.source, "shaders/gbuffers_terrain");
    assert!(s.compile_slot(K::Water, 2).is_none());
    assert!(s.compile_slot(K::Hand, 0).is_none());
    assert!(s.plan().unwrap().programs.is_empty());

    // A failing essential slot fails the pack, and the report still has
    // every slot after it.
    let (pack, r) = report(&files, &PackOptions::default());
    assert!(pack.is_none());
    assert_eq!(r.status, PackStatus::Failed);
    assert!(outcome(&r, "composite").error.is_some());
    assert_eq!(outcome(&r, "final").file.as_deref(), Some("shaders/final"));
}

/// Two compute programs of `composite`: `_a` counts into a buffer with an
/// atomic and a `shared` reduction, `_b` reads the count back; the pass's
/// fragment stage reads the buffer too.
const COUNT_A: &str = "#version 430 compatibility
layout(local_size_x = 8, local_size_y = 8) in;
const vec2 workGroupsRender = vec2(0.5, 0.5);
uniform sampler2D colortex0;
uniform float viewWidth;
layout(std430, binding = 0) restrict buffer Counts {
    uint total;
    float average;
    float values[];
} counts;
shared float partial[64];
void main() {
    vec4 c = texelFetch(colortex0, ivec2(gl_GlobalInvocationID.xy), 0);
    partial[gl_LocalInvocationIndex] = c.r;
    memoryBarrierShared();
    barrier();
    if (gl_LocalInvocationIndex == 0u) {
        float s = 0.0;
        for (int i = 0; i < 64; i++) s += partial[i];
        counts.values[gl_WorkGroupID.x + gl_WorkGroupID.y * gl_NumWorkGroups.x] = s;
        atomicAdd(counts.total, 1u);
    }
}
";

const COUNT_B: &str = "#version 430
layout(local_size_x = 1) in;
const ivec3 workGroups = ivec3(1, 1, 1);
layout(std430, binding = 0) buffer Counts {
    uint total;
    float average;
    float values[];
} counts;
void main() {
    float s = 0.0;
    for (uint i = 0u; i < counts.total; i++) s += counts.values[i];
    counts.average = s / max(float(counts.total), 1.0);
}
";

const READS_COUNT: &str = "#version 430
layout(std430, binding = 0) readonly buffer Counts {
    uint total;
    float average;
} counts;
varying vec2 uv;
void main() {
    gl_FragData[0] = vec4(counts.average);
}
";

fn compute_pack(required: &str, extra: &[(&str, &str)]) -> PackFiles {
    let props = format!("{required}bufferObject.0=65536\n");
    let mut files = vec![
        ("shaders/shaders.properties", props.as_str()),
        ("shaders/composite_a.csh", COUNT_A),
        ("shaders/composite_b.csh", COUNT_B),
    ];
    files.extend_from_slice(extra);
    programs(
        &[("gbuffers_terrain", FSH), ("composite", READS_COUNT)],
        &files,
    )
}

#[test]
fn compute_chain_writes_and_reads_a_buffer() {
    use crate::compute::{ComputeSlot, Dispatch};
    use crate::schedule::{FrameSchedule, Step};
    let files = compute_pack(
        "iris.features.required=COMPUTE_SHADERS SSBO\n",
        &[(
            "shaders/setup.csh",
            "#version 430\nlayout(local_size_x = 1) in;\nconst ivec3 workGroups = ivec3(1, 1, 1);\nlayout(std430, binding = 0) buffer Counts { uint total; } counts;\nvoid main() { counts.total = 0u; }\n",
        )],
    );
    let (pack, r) = report(&files, &PackOptions::default());
    let pack = pack.unwrap_or_else(|| panic!("{}", r.text("x")));
    assert_eq!(r.refusal, None);
    assert_eq!(r.status, PackStatus::Works, "{}", r.text("x"));
    let names: Vec<String> = pack.computes.keys().map(|s| s.name()).collect();
    assert_eq!(names, ["setup", "composite_a", "composite_b"]);
    let a = &pack.computes[&ComputeSlot::from_name("composite_a").unwrap()];
    assert_eq!(a.workgroup_size, [8, 8, 1]);
    assert_eq!(a.dispatch, Dispatch::Render([0.5, 0.5]));
    assert!(a.wgsl.contains("atomicAdd"), "{}", a.wgsl);
    assert!(a.wgsl.contains("var<workgroup>"));
    assert!(a.wgsl.contains("@group(1) @binding(160)"));
    assert_eq!(a.storage.len(), 1);
    assert!(a.storage[0].compute && !a.storage[0].read_only);
    assert!(
        a.textures
            .iter()
            .any(|t| t.source == crate::TextureSource::Buffer(0))
    );
    let b = &pack.computes[&ComputeSlot::from_name("composite_b").unwrap()];
    assert_eq!(b.dispatch, Dispatch::Fixed([1, 1, 1]));
    // The fragment stage binds the same buffer, read only.
    let composite = &pack.programs[&ProgramKind::Composite(0)];
    assert_eq!(composite.storage.len(), 1);
    assert!(composite.storage[0].fragment && composite.storage[0].read_only);
    assert!(composite.fragment_wgsl.contains("var<storage>"));
    // The chain runs before its pass, `_a` then `_b`; setup once, outside the frame.
    let s = FrameSchedule::new(&pack.plan);
    let order: Vec<String> = s
        .steps
        .iter()
        .filter_map(|st| match st {
            Step::Compute { slot, .. } => Some(slot.name()),
            Step::Pass(p) => Some(format!("{:?}", p.kind)),
            _ => None,
        })
        .collect();
    assert_eq!(order, ["composite_a", "composite_b", "Composite(0)"]);
    assert_eq!(s.setup, [ComputeSlot::from_name("setup").unwrap()]);
}

#[test]
fn compute_programs_over_the_device_limits() {
    let big = "#version 430\nlayout(local_size_x = 16, local_size_y = 16) in;\nshared vec4 tile[512];\nvoid main() { tile[gl_LocalInvocationIndex] = vec4(1.0); barrier(); }\n";
    let wide = "#version 430\nlayout(local_size_x = 32, local_size_y = 32) in;\nvoid main() {}\n";
    let extra = [
        ("shaders/deferred.csh", big),
        ("shaders/composite1_a.csh", wide),
    ];
    // Optional: each over-limit program is left out with its reason.
    let files = compute_pack("", &extra);
    let (pack, r) = report(&files, &PackOptions::default());
    let pack = pack.unwrap();
    let names: Vec<String> = pack.computes.keys().map(|s| s.name()).collect();
    assert_eq!(names, ["deferred", "composite_a", "composite_b"]);
    let text = r.text("x");
    assert!(
        text.contains("1024 invocations, the device allows 256"),
        "{text}"
    );
    // 512 vec4 = 8 KiB: over a 4 KiB device, under WebGPU's 16 KiB default.
    let small = HostParams {
        max_workgroup_storage: 4096,
        ..HostParams::default()
    };
    let (_, r) = compile_report(
        &files,
        "minecraft:overworld",
        &PackOptions::default(),
        &small,
    );
    assert!(
        r.text("x")
            .contains("8192 bytes of shared memory, the device allows 4096"),
        "{}",
        r.text("x")
    );
    // Required: a program that cannot run fails the pack, with that reason.
    let files = compute_pack("iris.features.required=COMPUTE_SHADERS\n", &extra);
    let (pack, r) = report(&files, &PackOptions::default());
    assert!(pack.is_none());
    assert_eq!(r.status, PackStatus::Failed);
    assert!(r.text("x").contains("1024 invocations"), "{}", r.text("x"));
    // A buffer block without its bufferObject.
    let files = programs(
        &[("gbuffers_terrain", FSH), ("composite", READS_COUNT)],
        &[],
    );
    let (_, r) = report(&files, &PackOptions::default());
    assert!(
        r.text("x").contains("without bufferObject.0"),
        "{}",
        r.text("x")
    );
}

#[test]
fn compute_programs_write_colortex_images() {
    use crate::compute::ComputeSlot;
    let store = "#version 430
layout(local_size_x = 8, local_size_y = 8) in;
const vec2 workGroupsRender = vec2(1.0, 1.0);
layout(rgba16f) uniform image2D colorimg2;
uniform sampler2D colortex2;
void main() {
    ivec2 p = ivec2(gl_GlobalInvocationID.xy);
    if (any(greaterThanEqual(p, imageSize(colorimg2)))) return;
    imageStore(colorimg2, p, texelFetch(colortex2, p, 0) * 0.5);
}
";
    // colortex3 stays RGBA8: read and written, which WebGPU allows for r32 formats only.
    let both = "#version 430
layout(local_size_x = 8, local_size_y = 8) in;
layout(rgba8) uniform image2D colorimg3;
void main() {
    ivec2 p = ivec2(gl_GlobalInvocationID.xy);
    imageStore(colorimg3, p, imageLoad(colorimg3, p) + 0.1);
}
";
    let fsh = format!("{FSH}/*\nconst int colortex2Format = RGBA16F;\n*/\n");
    let files = programs(
        &[("gbuffers_terrain", FSH), ("composite", &fsh)],
        &[
            ("shaders/composite_a.csh", store),
            ("shaders/composite_b.csh", both),
        ],
    );
    let (pack, r) = report(&files, &PackOptions::default());
    let pack = pack.unwrap();
    let a = pack
        .computes
        .get(&ComputeSlot::from_name("composite_a").unwrap())
        .unwrap_or_else(|| panic!("{}", r.text("x")));
    assert_eq!(a.color_images.len(), 1);
    let img = &a.color_images[0];
    assert_eq!(
        (img.binding, img.buffer, img.read, img.write),
        (226, 2, false, true)
    );
    assert!(
        a.wgsl.contains("texture_storage_2d<rgba16float,write>"),
        "{}",
        a.wgsl
    );
    let text = r.text("x");
    assert!(
        text.contains("colorimg3 is read and written: WebGPU allows that for r32 formats only"),
        "{text}"
    );
}
