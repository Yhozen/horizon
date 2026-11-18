//! End to end on a real pack, which is never in the repository:
//! `HORIZON_TEST_SHADERPACK=<pack dir> cargo test -p horizon-glsl-pack --test real_pack -- --ignored`.
//! With `HORIZON_TEST_SHADERPACK_DUMP=<dir>` the WGSL lands in `<dir>/<kind>.{vs,fs}.wgsl`.

use std::path::Path;

use horizon_glsl_pack::{HostParams, PackFiles, PackOptions, ProgramKind, compile};

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

#[test]
#[ignore = "needs HORIZON_TEST_SHADERPACK and the preprocessor and rewriter"]
fn compiles_terrain_water_and_final() {
    let Some(dir) = std::env::var_os("HORIZON_TEST_SHADERPACK") else {
        eprintln!("HORIZON_TEST_SHADERPACK is not set: skipped");
        return;
    };
    let mut files = PackFiles::default();
    read_dir(Path::new(&dir), Path::new(&dir), &mut files);
    let options = PackOptions {
        overrides: vec![("ENABLE_SHADOWS".into(), None)],
        ..Default::default()
    };
    let kinds = [ProgramKind::Terrain, ProgramKind::Water, ProgramKind::Final];
    let pack = compile(
        &files,
        "minecraft:overworld",
        &kinds,
        &options,
        &HostParams::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(pack.programs.len(), kinds.len());
    if let Some(out) = std::env::var_os("HORIZON_TEST_SHADERPACK_DUMP") {
        let out = Path::new(&out);
        std::fs::create_dir_all(out).unwrap();
        for (kind, program) in &pack.programs {
            let name = format!("{kind:?}").to_lowercase();
            std::fs::write(out.join(format!("{name}.vs.wgsl")), &program.vertex_wgsl).unwrap();
            std::fs::write(out.join(format!("{name}.fs.wgsl")), &program.fragment_wgsl).unwrap();
        }
    }
}
