//! End to end on a real pack, which is never in the repository:
//! `HORIZON_TEST_SHADERPACK=<pack dir> cargo test -p horizon-glsl-pack --test real_pack -- --ignored`.
//! With `HORIZON_TEST_SHADERPACK_DUMP=<dir>` every compiled slot's WGSL lands in
//! `<dir>/<test>/<slot>.{vs,fs}.wgsl` (`gbuffers_terrain`, `composite2`, …).

use std::path::Path;

use horizon_glsl_pack::{
    CompiledPack, HostParams, PackFiles, PackOptions, PackPlan, ProgramKind, compile,
    compile_kinds, read_plan,
};

fn read_dir(root: &Path, dir: &Path, files: &mut PackFiles) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            read_dir(root, &path, files);
        } else {
            let relative = path.strip_prefix(root).unwrap();
            let key: Vec<_> = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect();
            files.0.insert(key.join("/"), std::fs::read(&path).unwrap());
        }
    }
}

fn load() -> Option<PackFiles> {
    let Some(dir) = std::env::var_os("HORIZON_TEST_SHADERPACK") else {
        eprintln!("HORIZON_TEST_SHADERPACK is not set: skipped");
        return None;
    };
    let mut files = PackFiles::default();
    read_dir(Path::new(&dir), Path::new(&dir), &mut files);
    Some(files)
}

fn dump(pack: &CompiledPack, test: &str) {
    let Some(out) = std::env::var_os("HORIZON_TEST_SHADERPACK_DUMP") else {
        return;
    };
    let out = Path::new(&out).join(test);
    std::fs::create_dir_all(&out).unwrap();
    for (kind, program) in &pack.programs {
        let name = kind.name();
        std::fs::write(out.join(format!("{name}.vs.wgsl")), &program.vertex_wgsl).unwrap();
        std::fs::write(out.join(format!("{name}.fs.wgsl")), &program.fragment_wgsl).unwrap();
    }
}

fn print_plan(plan: &PackPlan) {
    for (kind, path) in &plan.programs {
        eprintln!("{:<22} {path} → {:?}", kind.name(), plan.draw_buffers[kind]);
    }
    for (i, b) in plan.buffers.iter().enumerate() {
        eprintln!("colortex{i}: {b:?}");
    }
    eprintln!("mipmaps {:?}", plan.mipmaps);
    eprintln!("shadow {:?}", plan.shadow);
    eprintln!("properties {:?}", plan.properties);
    eprintln!("textures {:?}", plan.custom_textures);
    eprintln!(
        "noise {} ao {} wetness {} dryness {} eye {}",
        plan.noise_resolution,
        plan.ambient_occlusion_level,
        plan.wetness_half_life,
        plan.dryness_half_life,
        plan.eye_brightness_half_life
    );
    eprintln!("{} options", plan.options.len());
}

/// The plan alone (no program compiled).
#[test]
#[ignore = "needs HORIZON_TEST_SHADERPACK"]
fn reads_the_plan() {
    let Some(files) = load() else { return };
    let plan = read_plan(&files, "minecraft:overworld", &PackOptions::default())
        .unwrap_or_else(|e| panic!("{e}"));
    print_plan(&plan);
    for o in &plan.options {
        eprintln!(
            "option {} = {:?} on={} {:?}",
            o.name, o.value, o.enabled, o.allowed
        );
    }
    assert!(plan.programs.contains_key(&ProgramKind::Terrain));
    // A const option override reaches the plan.
    if let Some(o) = plan
        .options
        .iter()
        .find(|o| o.name == "shadowMapResolution")
    {
        let last = o.allowed.last().expect("a const option has a list").clone();
        let options = PackOptions {
            overrides: vec![(o.name.clone(), Some(last.clone()))],
            ..PackOptions::default()
        };
        let changed = read_plan(&files, "minecraft:overworld", &options).unwrap();
        assert_eq!(changed.shadow.resolution.to_string(), last);
    }
}

/// Every slot the pack has; slots that do not translate yet are listed.
#[test]
#[ignore = "needs HORIZON_TEST_SHADERPACK"]
fn compiles_every_slot() {
    let Some(files) = load() else { return };
    let pack = compile(
        &files,
        "minecraft:overworld",
        &PackOptions::default(),
        &HostParams::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    print_plan(&pack.plan);
    for (kind, why) in &pack.skipped {
        eprintln!(
            "SKIPPED {}: {}",
            kind.name(),
            why.lines().next().unwrap_or("")
        );
    }
    assert!(pack.programs.contains_key(&ProgramKind::Terrain));
    assert!(pack.programs.contains_key(&ProgramKind::Final));
    assert_eq!(
        pack.plan.programs.keys().collect::<Vec<_>>(),
        pack.programs.keys().collect::<Vec<_>>()
    );
    dump(&pack, "all");
}

/// What the client compiles today without a shadow map: terrain, water and
/// final, with the pack's `ENABLE_SHADOWS` option off.
#[test]
#[ignore = "needs HORIZON_TEST_SHADERPACK"]
fn compiles_terrain_water_and_final_without_shadows() {
    let Some(files) = load() else { return };
    let mut options = PackOptions::default();
    options.overrides.push(("ENABLE_SHADOWS".into(), None));
    let kinds = [ProgramKind::Terrain, ProgramKind::Water, ProgramKind::Final];
    let pack = compile_kinds(
        &files,
        "minecraft:overworld",
        &kinds,
        &options,
        &HostParams::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(pack.programs.len(), kinds.len());
    if pack.plan.options.iter().any(|o| o.name == "ENABLE_SHADOWS") {
        for (kind, program) in &pack.programs {
            assert!(
                !program.samplers.iter().any(|s| s.starts_with("shadow")),
                "{kind:?} still reads {:?}",
                program.samplers
            );
        }
    }
    dump(&pack, "plain");
}
