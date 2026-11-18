//! Reading a pack's plan and compiling its programs.

use std::collections::BTreeMap;

use crate::program::find_enabled;
use crate::{
    BlockIds, CompiledPack, CompiledProgram, HostParams, PackError, PackFiles, PackOptions,
    PackPlan, ProgramKind, ShadowSettings, Stage, directives, host, naga_out, options, preprocess,
    properties, rewrite, world_folder,
};

/// Compiles every slot of the pack whose program exists for the dimension
/// `dimension` (`minecraft:overworld`, …): the gbuffers and shadow slots,
/// `deferred*`, `composite*` and `final`, and reads the pack's [`PackPlan`].
///
/// A program that fails to compile is an error for the slots the frame
/// cannot do without ([`ProgramKind::is_essential`]: terrain, water, the
/// composites and final); any other slot is left out and listed in
/// [`CompiledPack::skipped`] with the reason.
pub fn compile(
    files: &PackFiles,
    dimension: &str,
    options: &PackOptions,
    host: &HostParams,
) -> Result<CompiledPack, PackError> {
    compile_slots(files, dimension, &ProgramKind::all(), false, options, host)
}

/// Compiles only the slots `kinds`; each must exist and compile. The plan's
/// pack-wide settings still come from every program the pack has.
pub fn compile_kinds(
    files: &PackFiles,
    dimension: &str,
    kinds: &[ProgramKind],
    options: &PackOptions,
    host: &HostParams,
) -> Result<CompiledPack, PackError> {
    compile_slots(files, dimension, kinds, true, options, host)
}

/// A program's two stages, preprocessed and with includes only expanded.
struct Prepared {
    vsh: String,
    fsh: String,
    raw_vsh: String,
    raw_fsh: String,
}

impl Prepared {
    fn new(
        files: &PackFiles,
        program: &str,
        predefined: &BTreeMap<String, String>,
    ) -> Result<Self, PackError> {
        let (vsh_path, fsh_path) = (format!("{program}.vsh"), format!("{program}.fsh"));
        Ok(Self {
            vsh: preprocess::preprocess(files, &vsh_path, predefined, &[])?,
            fsh: preprocess::preprocess(files, &fsh_path, predefined, &[])?,
            // Before preprocessing removes comments: DRAWBUFFERS and some
            // directives live in one.
            raw_vsh: preprocess::expand_includes(files, &vsh_path)?,
            raw_fsh: preprocess::expand_includes(files, &fsh_path)?,
        })
    }

    /// The draw buffers of `kind` running this program: the directive, else
    /// shadowcolor0/1 for shadow programs, every `gl_FragData[i]` up to the
    /// highest written for full-screen passes, colortex0 for gbuffers.
    fn draw_buffers(&self, kind: ProgramKind) -> Vec<u8> {
        if let Some(list) = properties::draw_buffers(&self.raw_fsh) {
            return list;
        }
        if kind.is_shadow() {
            return vec![0, 1];
        }
        if kind.is_fullscreen() {
            let highest = (1..8u8)
                .rev()
                .find(|i| self.fsh.contains(&format!("gl_FragData[{i}]")));
            return (0..=highest.unwrap_or(0)).collect();
        }
        vec![0]
    }
}

/// A pack read up to compiling: the plan and the preprocessed programs.
struct Read {
    plan: PackPlan,
    blocks: BlockIds,
    /// By pack path (no extension).
    prepared: BTreeMap<String, Prepared>,
    skipped: Vec<(ProgramKind, String)>,
}

/// Reads what a pack asks of the renderer without compiling its programs:
/// every slot present (after fallbacks and `program.*.enabled`), draw
/// buffers, buffer formats, mipmaps, shadow map, custom textures,
/// `shaders.properties` settings and options. Slots whose sources do not
/// preprocess are left out (an error for essential ones).
pub fn read_plan(
    files: &PackFiles,
    dimension: &str,
    options: &PackOptions,
) -> Result<PackPlan, PackError> {
    Ok(read(files, dimension, &ProgramKind::all(), false, options)?.plan)
}

fn read(
    files: &PackFiles,
    dimension: &str,
    kinds: &[ProgramKind],
    strict: bool,
    options: &PackOptions,
) -> Result<Read, PackError> {
    let predefined = preprocess::predefined(options.mc_version);
    let defaults = options::discover(files);
    let files = &options::apply(files, &options.overrides);
    let mut macros = predefined.clone();
    macros.extend(options::macros(&options::discover(files)));
    let props = directives::shaders_properties(files, &macros)?;
    let blocks = properties::block_ids(files, &predefined)?;
    let folder = world_folder(files, dimension);

    // Every slot the pack has: pack-wide directives come from all of them.
    let resolved: BTreeMap<ProgramKind, String> = ProgramKind::all()
        .into_iter()
        .filter_map(|k| Some((k, find_enabled(files, &folder, k, &props.disabled)?)))
        .collect();
    let mut prepared: BTreeMap<String, Result<Prepared, PackError>> = BTreeMap::new();
    for path in resolved.values() {
        if !prepared.contains_key(path) {
            prepared.insert(path.clone(), Prepared::new(files, path, &predefined));
        }
    }
    let mut plan = PackPlan {
        properties: props.properties,
        custom_textures: props.textures,
        options: defaults,
        ..PackPlan::default()
    };
    directives::read_globals(
        &mut plan,
        prepared
            .values()
            .filter_map(|p| p.as_ref().ok())
            .map(|p| directives::ProgramText {
                raw: [&p.raw_vsh, &p.raw_fsh],
                preprocessed: [&p.vsh, &p.fsh],
            }),
    );

    let mut skipped = Vec::new();
    let mut wanted: Vec<ProgramKind> = kinds.to_vec();
    wanted.sort();
    wanted.dedup();
    for kind in wanted {
        let Some(path) = resolved.get(&kind) else {
            if strict {
                return Err(PackError::new(
                    format!("shaders/{folder}"),
                    format!("no program for {kind:?}"),
                ));
            }
            continue;
        };
        match &prepared[path] {
            Err(e) if strict || kind.is_essential() => return Err(e.clone()),
            Err(e) => skipped.push((kind, e.to_string())),
            Ok(p) => {
                plan.programs.insert(kind, path.clone());
                plan.draw_buffers.insert(kind, p.draw_buffers(kind));
                if kind.is_fullscreen() {
                    let mips = directives::mipmaps(&p.raw_fsh, &p.fsh);
                    if !mips.is_empty() {
                        plan.mipmaps.insert(kind, mips);
                    }
                }
            }
        }
    }
    Ok(Read {
        plan,
        blocks,
        prepared: prepared
            .into_iter()
            .filter_map(|(k, p)| Some((k, p.ok()?)))
            .collect(),
        skipped,
    })
}

fn compile_slots(
    files: &PackFiles,
    dimension: &str,
    kinds: &[ProgramKind],
    strict: bool,
    options: &PackOptions,
    host: &HostParams,
) -> Result<CompiledPack, PackError> {
    let Read {
        mut plan,
        blocks,
        prepared,
        mut skipped,
    } = read(files, dimension, kinds, strict, options)?;
    // One compile per (file, geometry, shadow or not, draw buffers, host variant).
    type Key<'k> = (
        &'k str,
        host::Geometry,
        bool,
        &'k [u8],
        (Option<&'static str>, bool),
    );
    let mut done: Vec<(Key, Result<CompiledProgram, PackError>)> = Vec::new();
    let mut results = Vec::new();
    for (&kind, path) in &plan.programs {
        let draw_buffers = &plan.draw_buffers[&kind];
        let key: Key = (
            path,
            kind.geometry(),
            kind.is_shadow(),
            draw_buffers,
            host::variant(kind),
        );
        let result = match done.iter().find(|(k, _)| *k == key) {
            Some((_, r)) => r.clone(),
            None => {
                // Only prepared programs are in the plan.
                let p = &prepared[path];
                let r = compile_program(path, p, kind, draw_buffers, host, &blocks, &plan);
                done.push((key, r.clone()));
                r
            }
        };
        results.push((kind, result));
    }
    let mut programs = BTreeMap::new();
    for (kind, result) in results {
        match result {
            Ok(program) => {
                programs.insert(kind, program);
            }
            Err(e) if strict || kind.is_essential() => return Err(e),
            Err(e) => {
                skipped.push((kind, e.to_string()));
                plan.programs.remove(&kind);
                plan.draw_buffers.remove(&kind);
                plan.mipmaps.remove(&kind);
            }
        }
    }
    let shadow = ShadowSettings::from(&plan.shadow);
    Ok(CompiledPack {
        programs,
        plan,
        skipped,
        blocks,
        shadow,
    })
}

fn compile_program(
    program: &str,
    p: &Prepared,
    kind: ProgramKind,
    draw_buffers: &[u8],
    host: &HostParams,
    blocks: &BlockIds,
    plan: &PackPlan,
) -> Result<CompiledProgram, PackError> {
    let vsh_path = format!("{program}.vsh");
    let fsh_path = format!("{program}.fsh");
    let vertex =
        rewrite::rewrite(&p.vsh, Stage::Vertex).map_err(|m| PackError::new(&vsh_path, m))?;
    let fragment =
        rewrite::rewrite(&p.fsh, Stage::Fragment).map_err(|m| PackError::new(&fsh_path, m))?;
    let spec = host::ProgramSpec {
        kind,
        geometry: kind.geometry(),
        draw_buffers,
        host,
        blocks,
        plan,
    };
    let host::HostOutput {
        vertex: vs_glsl,
        fragment: fs_glsl,
        textures,
        unknown_uniforms,
    } = host::program_sources(&vertex, &fragment, &spec).map_err(|m| PackError::new(program, m))?;
    let vertex_wgsl = naga_out::glsl_to_wgsl(&vs_glsl, Stage::Vertex)
        .map_err(|m| PackError::new(&vsh_path, m))?;
    let fragment_wgsl = naga_out::glsl_to_wgsl(&fs_glsl, Stage::Fragment)
        .map_err(|m| PackError::new(&fsh_path, m))?;
    // The samplers the program reads (a pack may declare more than it uses).
    let samplers = vertex
        .sample_calls
        .iter()
        .chain(&fragment.sample_calls)
        .filter_map(|call| host::split_sampling_call(call).map(|(_, name)| name.to_string()))
        .collect();
    Ok(CompiledProgram {
        source: program.to_string(),
        vertex_wgsl,
        fragment_wgsl,
        draw_buffers: draw_buffers.to_vec(),
        samplers,
        textures,
        unknown_uniforms,
    })
}
