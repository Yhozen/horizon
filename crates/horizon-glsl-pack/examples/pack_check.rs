//! Compiles every program slot of shader packs one by one and writes what
//! happened, for the corpus runner (`tools/shader_pack_corpus.py check`).
//!
//! ```text
//! cargo run --example pack_check -p horizon-glsl-pack -- <pack.zip|folder>... --out <dir>
//!     [--dimension minecraft:overworld|minecraft:the_nether|minecraft:the_end|all]
//!     [--option NAME=VALUE]... [--max-varyings N] [--max-textures N] [--terrain-textures N] [--jobs N]
//! ```
//!
//! For each pack it writes `<out>/<pack>/<dimension>/<slot>.{vs,fs}.wgsl` for
//! the programs that compiled and `<out>/<pack>/result.json`: per dimension
//! and slot the source program, ok or the error, unknown uniforms, the
//! texture count and naga's SPIR-V writer result; plus the pack's features
//! the translator does not handle yet, found by listing its files.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use horizon_glsl_pack::{
    HostParams, PackFiles, PackOptions, ProgramKind, compile_kinds, find_program, read_plan,
    world_folder,
};
use serde_json::{Value, json};

const DIMENSIONS: [&str; 3] = [
    "minecraft:overworld",
    "minecraft:the_nether",
    "minecraft:the_end",
];

/// OptiFine's documented gbuffers programs; any other `gbuffers_*` name is an
/// Iris addition.
const OPTIFINE_GBUFFERS: [&str; 23] = [
    "gbuffers_basic",
    "gbuffers_line",
    "gbuffers_textured",
    "gbuffers_textured_lit",
    "gbuffers_skybasic",
    "gbuffers_skytextured",
    "gbuffers_clouds",
    "gbuffers_terrain",
    "gbuffers_terrain_solid",
    "gbuffers_terrain_cutout_mip",
    "gbuffers_terrain_cutout",
    "gbuffers_damagedblock",
    "gbuffers_block",
    "gbuffers_beaconbeam",
    "gbuffers_item",
    "gbuffers_entities",
    "gbuffers_entities_glowing",
    "gbuffers_armor_glint",
    "gbuffers_spidereyes",
    "gbuffers_hand",
    "gbuffers_weather",
    "gbuffers_water",
    "gbuffers_hand_water",
];

fn main() {
    let mut packs = Vec::new();
    let mut out = None;
    let mut dimensions = vec![DIMENSIONS[0].to_string()];
    let mut options = PackOptions::default();
    let mut host = HostParams::default();
    let mut jobs: usize = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = args.next().map(PathBuf::from),
            "--dimension" => {
                let d = args.next().unwrap_or_else(|| usage());
                dimensions = if d == "all" {
                    DIMENSIONS.iter().map(|s| s.to_string()).collect()
                } else {
                    vec![d]
                };
            }
            "--max-varyings" => {
                host.max_varyings = args
                    .next()
                    .and_then(|j| j.parse().ok())
                    .unwrap_or_else(|| usage());
            }
            // Sampled textures per stage (16 = WebGPU's minimum; desktop adapters allow more).
            "--max-textures" => {
                host.max_textures = args
                    .next()
                    .and_then(|j| j.parse().ok())
                    .unwrap_or_else(|| usage());
            }
            // Horizon's own textures in terrain programs: the sprite arrays in use and the lightmap.
            "--terrain-textures" => {
                host.terrain_textures = args
                    .next()
                    .and_then(|j| j.parse().ok())
                    .unwrap_or_else(|| usage());
            }
            "--jobs" => {
                jobs = args
                    .next()
                    .and_then(|j| j.parse().ok())
                    .unwrap_or_else(|| usage())
            }
            "--option" => {
                let o = args.next().unwrap_or_else(|| usage());
                let (name, value) = o.split_once('=').unwrap_or_else(|| usage());
                options
                    .overrides
                    .push((name.to_string(), Some(value.to_string())));
            }
            "-h" | "--help" => usage(),
            _ => packs.push(PathBuf::from(arg)),
        }
    }
    let Some(out) = out else { usage() };
    if packs.is_empty() {
        usage();
    }
    // Packs run in parallel, biggest first so the slow ones do not finish last.
    packs.sort_by_key(|p| std::cmp::Reverse(fs::metadata(p).map(|m| m.len()).unwrap_or(0)));
    let queue = std::sync::Mutex::new(packs.into_iter());
    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1) {
            scope.spawn(|| {
                loop {
                    let Some(pack) = queue.lock().unwrap().next() else {
                        break;
                    };
                    check_one(&pack, &out, &dimensions, &options, &host);
                }
            });
        }
    });
}

fn check_one(
    pack: &Path,
    out: &Path,
    dimensions: &[String],
    options: &PackOptions,
    host: &HostParams,
) {
    let name = pack_name(pack);
    let started = std::time::Instant::now();
    let result = match read_files(pack) {
        Ok(files) => check_pack(&files, &name, &out.join(&name), dimensions, options, host),
        Err(e) => json!({ "pack": name, "read_error": e }),
    };
    let dir = out.join(&name);
    fs::create_dir_all(&dir).expect("create the output folder");
    let text = serde_json::to_string_pretty(&result).expect("JSON");
    fs::write(dir.join("result.json"), text).expect("write result.json");
    eprintln!("{name}: {:.1}s", started.elapsed().as_secs_f32());
}

fn usage() -> ! {
    eprintln!(
        "usage: pack_check <pack.zip|folder>... --out <dir> [--dimension <id>|all] [--option NAME=VALUE]... [--max-varyings N] [--max-textures N] [--terrain-textures N] [--jobs N]"
    );
    std::process::exit(2)
}

/// The pack's name: the zip's stem or the folder's name.
fn pack_name(path: &Path) -> String {
    let name = if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        path.file_stem()
    } else {
        path.file_name()
    };
    name.map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pack".to_string())
}

/// A zip or a folder as [`PackFiles`], with the top-level folder of packs
/// zipped from their folder stripped (as the client does).
fn read_files(path: &Path) -> Result<PackFiles, String> {
    let mut files = PackFiles::default();
    if path.is_dir() {
        let mut stack = vec![path.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
                let entry = entry.map_err(|e| e.to_string())?.path();
                if entry.is_dir() {
                    stack.push(entry);
                } else if let Ok(name) = entry.strip_prefix(path) {
                    let name = name.to_string_lossy().replace('\\', "/");
                    let bytes = fs::read(&entry).map_err(|e| format!("{name}: {e}"))?;
                    files.0.insert(name, bytes);
                }
            }
        }
    } else {
        let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().replace('\\', "/");
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut bytes)
                .map_err(|e| format!("{name}: {e}"))?;
            files.0.insert(name, bytes);
        }
    }
    if !files.0.keys().any(|k| k.starts_with("shaders/"))
        && let Some(top) = files
            .0
            .keys()
            .find_map(|k| k.split_once("/shaders/").map(|(t, _)| t.to_string()))
    {
        let prefix = format!("{top}/");
        files.0 = std::mem::take(&mut files.0)
            .into_iter()
            .filter_map(|(k, v)| Some((k.strip_prefix(&prefix)?.to_string(), v)))
            .collect();
    }
    Ok(files)
}

fn check_pack(
    files: &PackFiles,
    name: &str,
    dir: &Path,
    dimensions: &[String],
    options: &PackOptions,
    host: &HostParams,
) -> Value {
    let host = *host;
    let mut dims = serde_json::Map::new();
    for dimension in dimensions {
        let folder = world_folder(files, dimension);
        let plan_error = read_plan(files, dimension, options)
            .err()
            .map(|e| e.to_string());
        let wgsl_dir = dir.join(dimension.replace(':', "_"));
        let _ = fs::remove_dir_all(&wgsl_dir);
        let mut programs = Vec::new();
        for kind in ProgramKind::all() {
            let Some(source) = find_program(files, &folder, kind) else {
                continue;
            };
            let mut entry = json!({
                "slot": kind.name(),
                "essential": kind.is_essential(),
                "source": source,
            });
            match compile_kinds(files, dimension, &[kind], options, &host) {
                Err(e) if e.message.starts_with("no program for") => {
                    entry["status"] = json!("disabled");
                }
                Err(e) => {
                    entry["status"] = json!("error");
                    entry["file"] = json!(e.file);
                    entry["error"] = json!(e.message);
                }
                Ok(pack) => {
                    let program = &pack.programs[&kind];
                    fs::create_dir_all(&wgsl_dir).expect("create the WGSL folder");
                    let mut spirv = serde_json::Map::new();
                    for (stage, text, naga_stage) in [
                        ("vs", &program.vertex_wgsl, naga::ShaderStage::Vertex),
                        ("fs", &program.fragment_wgsl, naga::ShaderStage::Fragment),
                    ] {
                        let file = wgsl_dir.join(format!("{}.{stage}.wgsl", kind.name()));
                        fs::write(&file, text).expect("write WGSL");
                        spirv.insert(
                            stage.to_string(),
                            json!(write_spirv(text, naga_stage).err()),
                        );
                    }
                    entry["status"] = json!("ok");
                    entry["unknown_uniforms"] = json!(program.unknown_uniforms);
                    entry["textures"] = json!(program.textures.len());
                    entry["spirv_errors"] = Value::Object(spirv);
                }
            }
            programs.push(entry);
        }
        dims.insert(
            dimension.clone(),
            json!({ "folder": folder, "plan_error": plan_error, "programs": programs }),
        );
    }
    json!({
        "pack": name,
        "files": files.0.len(),
        // 0: no program sources at all (a resource pack's core shaders, say).
        "shader_sources": files
            .0
            .keys()
            .filter(|k| k.starts_with("shaders/") && [".vsh", ".fsh", ".csh", ".gsh"].iter().any(|e| k.ends_with(e)))
            .count(),
        "features": features(files),
        "dimensions": dims,
    })
}

/// naga's SPIR-V writer (wgpu's Vulkan path) on the WGSL a program compiled to.
fn write_spirv(wgsl: &str, stage: naga::ShaderStage) -> Result<(), String> {
    use naga::valid::{Capabilities, ValidationFlags, Validator};
    let module = naga::front::wgsl::parse_str(wgsl).map_err(|e| e.emit_to_string(wgsl))?;
    let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .map_err(|e| e.emit_to_string(wgsl))?;
    let entry_point = module
        .entry_points
        .iter()
        .find(|e| e.stage == stage)
        .map(|e| e.name.clone())
        .ok_or("no entry point")?;
    let pipeline = naga::back::spv::PipelineOptions {
        shader_stage: stage,
        entry_point,
    };
    naga::back::spv::write_vec(
        &module,
        &info,
        &naga::back::spv::Options::default(),
        Some(&pipeline),
    )
    .map(drop)
    .map_err(|e| e.to_string())
}

/// What the pack uses that the translator does not handle yet, from its file
/// list and its `shaders.properties` keys (every branch of its `#if`s).
fn features(files: &PackFiles) -> Value {
    let mut flags: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let known: BTreeSet<String> = ProgramKind::all()
        .into_iter()
        .flat_map(|k| k.candidates())
        .collect();
    for path in files.0.keys() {
        let Some(rest) = path.strip_prefix("shaders/") else {
            continue;
        };
        let file = rest.rsplit('/').next().unwrap_or(rest);
        let (stem, ext) = file.rsplit_once('.').unwrap_or((file, ""));
        let ext = ext.to_ascii_lowercase();
        let is_program = matches!(ext.as_str(), "vsh" | "fsh" | "csh" | "gsh" | "tcs" | "tes");
        match ext.as_str() {
            "csh" => add(&mut flags, "compute_shaders", stem),
            "gsh" => add(&mut flags, "geometry_shaders", stem),
            "tcs" | "tes" => add(&mut flags, "tessellation_shaders", stem),
            "dat" | "bin" => add(&mut flags, "raw_textures", rest),
            _ => {}
        }
        // Programs sit directly in `shaders/` or in a world folder.
        let top_level = rest.split('/').count() == 1
            || (rest.split('/').count() == 2 && rest.starts_with("world"));
        if !is_program || !top_level {
            continue;
        }
        let base = stem.trim_end_matches(|c: char| c.is_ascii_digit());
        let flag = match base {
            "prepare" => Some("prepare_programs"),
            "shadowcomp" => Some("shadowcomp_programs"),
            "begin" => Some("begin_programs"),
            "setup" => Some("setup_programs"),
            _ => None,
        };
        if let Some(flag) = flag {
            add(&mut flags, flag, stem);
        }
        if stem.starts_with("gbuffers_") && !OPTIFINE_GBUFFERS.contains(&stem) {
            add(&mut flags, "iris_gbuffers", stem);
        }
        let numbered = ["composite", "deferred"].contains(&base);
        if (stem.starts_with("gbuffers_") || stem.starts_with("shadow") || numbered)
            && !known.contains(stem)
            && stem != "shadow_entities"
        {
            add(&mut flags, "unhandled_programs", stem);
        }
    }
    for path in ["shaders/shaders.properties"] {
        let Some(text) = files.text(path) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            for (prefix, flag) in [
                ("iris.features.required", "iris_features_required"),
                ("iris.features.optional", "iris_features_optional"),
            ] {
                if key == prefix {
                    for f in value.split_whitespace() {
                        add(&mut flags, flag, f);
                    }
                }
            }
            for (prefix, flag) in [
                ("image.", "images"),
                ("bufferObject.", "buffer_objects"),
                ("uniform.", "custom_uniforms"),
                ("variable.", "custom_variables"),
                ("customTexture.", "custom_textures"),
            ] {
                if let Some(name) = key.strip_prefix(prefix) {
                    add(&mut flags, flag, name);
                }
            }
            if key.starts_with("texture.") && value.contains("TEXTURE_") {
                add(&mut flags, "raw_textures", key);
            }
        }
    }
    json!(flags)
}

fn add(flags: &mut BTreeMap<&'static str, BTreeSet<String>>, flag: &'static str, item: &str) {
    flags.entry(flag).or_default().insert(item.to_string());
}
