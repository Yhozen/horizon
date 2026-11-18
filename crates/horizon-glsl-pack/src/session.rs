//! Reading a pack's plan and compiling its programs, in bounded steps.
//!
//! A [`Session`] does the work of [`compile`] one piece at a time (the setup,
//! one program's preprocessing, the plan, one slot's compile), so the browser
//! can run a step per frame and natively it runs whole on a thread. A slot
//! whose program fails tries the next candidate of its chain; every attempt
//! lands in the [`PackReport`].

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::program::find_candidates;
use crate::report::{PackStatus, ProgramOutcome};
use crate::{
    BlockIds, CompiledPack, CompiledProgram, HostParams, PackError, PackFiles, PackOptions,
    PackPlan, PackReport, ProgramKind, ShadowSettings, Stage, directives, features, host, naga_out,
    options, preprocess, properties, rewrite, world_folder,
};

/// Compiles every slot of the pack whose program exists for the dimension
/// `dimension` (`minecraft:overworld`, …): the gbuffers and shadow slots,
/// `prepare*`, `deferred*`, `composite*` and `final`, and reads the pack's
/// [`PackPlan`].
///
/// A slot whose program fails to compile runs the next candidate of its
/// chain. When none is left, the slot is left out and listed in
/// [`CompiledPack::skipped`], unless the frame cannot do without it
/// ([`ProgramKind::is_essential`]): then the pack fails with the error of
/// the slot's first candidate. Features the pack requires and Horizon lacks
/// do not stop `compile` (see [`compile_report`] for the refusing version).
pub fn compile(
    files: &PackFiles,
    dimension: &str,
    options: &PackOptions,
    host: &HostParams,
) -> Result<CompiledPack, PackError> {
    let mut s = Session::with(
        files,
        dimension,
        options,
        host,
        Mode {
            kinds: ProgramKind::all(),
            strict: false,
            compile: true,
            refuse: false,
        },
    );
    s.run();
    s.result().map(|(pack, _)| pack)
}

/// Compiles only the slots `kinds`; each must exist and its first usable
/// candidate must compile (no fallback after a failure). The plan's
/// pack-wide settings still come from every program the pack has.
pub fn compile_kinds(
    files: &PackFiles,
    dimension: &str,
    kinds: &[ProgramKind],
    options: &PackOptions,
    host: &HostParams,
) -> Result<CompiledPack, PackError> {
    let mut s = Session::with(
        files,
        dimension,
        options,
        host,
        Mode {
            kinds: kinds.to_vec(),
            strict: true,
            compile: true,
            refuse: false,
        },
    );
    s.run();
    s.result().map(|(pack, _)| pack)
}

/// Reads what a pack asks of the renderer without compiling its programs:
/// every slot present (after fallbacks and `program.*.enabled`), draw
/// buffers, buffer formats, mipmaps, shadow map, custom textures,
/// `shaders.properties` settings and options. A slot whose sources do not
/// preprocess falls back to its next candidate (an error for an essential
/// slot with none left).
pub fn read_plan(
    files: &PackFiles,
    dimension: &str,
    options: &PackOptions,
) -> Result<PackPlan, PackError> {
    let mut s = Session::with(
        files,
        dimension,
        options,
        &HostParams::default(),
        Mode {
            kinds: ProgramKind::all(),
            strict: false,
            compile: false,
            refuse: false,
        },
    );
    s.run();
    s.result().map(|(pack, _)| pack.plan)
}

/// [`Session::new`] run to the end: the pack (`None` when it failed or was
/// refused) and the report.
pub fn compile_report(
    files: &PackFiles,
    dimension: &str,
    options: &PackOptions,
    host: &HostParams,
) -> (Option<CompiledPack>, PackReport) {
    let mut s = Session::new(files, dimension, options, host);
    s.run();
    s.finish()
}

/// What a session does.
struct Mode {
    /// The slots it compiles (every slot's program still feeds the plan's
    /// pack-wide settings).
    kinds: Vec<ProgramKind>,
    /// Every slot must exist; the first failure is the result.
    strict: bool,
    /// Compile (false: stop after preprocessing, for [`read_plan`]).
    compile: bool,
    /// Stop when the pack needs a feature Horizon lacks.
    refuse: bool,
}

/// A program's two stages, preprocessed and with includes only expanded.
struct Prepared {
    vsh: String,
    fsh: String,
    raw_vsh: String,
    raw_fsh: String,
    /// The fragment stage's draw-buffer directive in a live branch, if any.
    directive: Option<Vec<u8>>,
}

impl Prepared {
    fn new(
        files: &PackFiles,
        program: &str,
        predefined: &BTreeMap<String, String>,
    ) -> Result<Self, PackError> {
        let (vsh_path, fsh_path) = (format!("{program}.vsh"), format!("{program}.fsh"));
        // Before preprocessing removes comments: DRAWBUFFERS and some
        // directives live in one.
        let raw_fsh = preprocess::expand_includes(files, &fsh_path)?;
        let directive = directives::live_draw_buffers(&raw_fsh, &fsh_path, files, predefined);
        Ok(Self {
            vsh: preprocess::preprocess(files, &vsh_path, predefined, &[])?,
            fsh: preprocess::preprocess(files, &fsh_path, predefined, &[])?,
            raw_vsh: preprocess::expand_includes(files, &vsh_path)?,
            raw_fsh,
            directive,
        })
    }

    /// The draw buffers of `kind` running this program: the directive, else
    /// shadowcolor0/1 for shadow programs, every output up to the highest
    /// written for full-screen passes, colortex0 for gbuffers.
    fn draw_buffers(&self, kind: ProgramKind) -> Vec<u8> {
        if let Some(list) = &self.directive {
            return list.clone();
        }
        if kind.is_shadow() {
            return vec![0, 1];
        }
        if kind.is_fullscreen() {
            let highest = directives::highest_output(&self.fsh);
            return (0..=highest.unwrap_or(0)).collect();
        }
        vec![0]
    }
}

/// One compile, shared by the slots that run the same file the same way.
type Key = (
    String,
    host::Geometry,
    bool,
    Vec<u8>,
    (Option<&'static str>, bool),
    Option<String>,
);

/// The state once the pack's properties are read.
struct State {
    files: PackFiles,
    predefined: BTreeMap<String, String>,
    blocks: BlockIds,
    entities: properties::IdMap,
    items: properties::IdMap,
    plan: PackPlan,
    /// Every slot the pack has (any of `ProgramKind::all()`), with its
    /// usable candidates, best first.
    chains: BTreeMap<ProgramKind, Vec<String>>,
    prepared: BTreeMap<String, Result<Prepared, PackError>>,
    to_prepare: VecDeque<String>,
    /// Wanted slots still to settle; the head may be on a later candidate.
    queue: VecDeque<ProgramKind>,
    cursor: BTreeMap<ProgramKind, usize>,
    failed: BTreeMap<ProgramKind, Vec<PackError>>,
    cache: Vec<(Key, Result<CompiledProgram, PackError>)>,
    programs: BTreeMap<ProgramKind, CompiledProgram>,
    skipped: Vec<(ProgramKind, String)>,
}

impl State {
    /// Runs `path` in slot `kind`: preprocessed if it is not yet, then
    /// compiled (or a compile of the same key reused) when `compile`.
    /// Returns the program and its draw buffers.
    fn attempt(
        &mut self,
        kind: ProgramKind,
        path: &str,
        host: &HostParams,
        compile: bool,
    ) -> Result<(Option<CompiledProgram>, Vec<u8>), PackError> {
        if !self.prepared.contains_key(path) {
            let p = Prepared::new(&self.files, path, &self.predefined);
            self.prepared.insert(path.to_string(), p);
        }
        let p = match &self.prepared[path] {
            Err(e) => return Err(e.clone()),
            Ok(p) => p,
        };
        let draw_buffers = p.draw_buffers(kind);
        self.plan
            .outputs
            .insert(kind, directives::fragment_outputs(&p.fsh));
        if !compile {
            return Ok((None, draw_buffers));
        }
        let key: Key = (
            path.to_string(),
            kind.geometry(),
            kind.is_shadow(),
            draw_buffers.clone(),
            host::variant(kind),
            Some(format!("{:?}", self.plan.alpha_test(kind))),
        );
        let r = match self.cache.iter().find(|(k, _)| *k == key) {
            Some((_, r)) => r.clone(),
            None => {
                let r =
                    compile_program(path, p, kind, &draw_buffers, host, &self.blocks, &self.plan);
                self.cache.push((key, r.clone()));
                r
            }
        };
        r.map(|c| (Some(c), draw_buffers))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Setup,
    Prepare,
    Plan,
    Slots,
    Done,
}

/// A pack compile in steps. `Send`: natively it may run on a thread.
pub struct Session {
    files: PackFiles,
    dimension: String,
    options: PackOptions,
    host: HostParams,
    mode: Mode,
    phase: Phase,
    state: Option<State>,
    report: PackReport,
    /// Why the session stopped (the setup, a refusal, a strict slot).
    error: Option<PackError>,
    /// The first essential slot whose chain was exhausted: the pack fails.
    essential: Option<PackError>,
    done: usize,
    total: usize,
}

impl Session {
    /// A session compiling every slot of the pack for `dimension`; nothing
    /// runs until [`Self::step`]. A pack that requires a feature Horizon
    /// lacks is refused (its report says why).
    pub fn new(
        files: &PackFiles,
        dimension: &str,
        options: &PackOptions,
        host: &HostParams,
    ) -> Session {
        Self::with(
            files,
            dimension,
            options,
            host,
            Mode {
                kinds: ProgramKind::all(),
                strict: false,
                compile: true,
                refuse: true,
            },
        )
    }

    fn with(
        files: &PackFiles,
        dimension: &str,
        options: &PackOptions,
        host: &HostParams,
        mode: Mode,
    ) -> Session {
        Session {
            files: files.clone(),
            dimension: dimension.to_string(),
            options: options.clone(),
            host: *host,
            mode,
            phase: Phase::Setup,
            state: None,
            report: PackReport::default(),
            error: None,
            essential: None,
            done: 0,
            total: 1,
        }
    }

    /// Does one bounded piece of work (at most one program compile or
    /// preprocess). Returns true once the session is done.
    pub fn step(&mut self) -> bool {
        match self.phase {
            Phase::Setup => self.setup(),
            Phase::Prepare => self.prepare_one(),
            Phase::Plan => self.read_globals(),
            Phase::Slots => self.settle_one(),
            Phase::Done => return true,
        }
        self.done += 1;
        if self.phase == Phase::Done {
            self.total = self.done;
        }
        self.phase == Phase::Done
    }

    /// `(done, total)` steps; the total grows when a slot falls back.
    pub fn progress(&self) -> (usize, usize) {
        (self.done.min(self.total), self.total)
    }

    /// The pack (`None` when it failed or was refused) and the report.
    /// Runs the remaining steps first if the session is not done.
    pub fn finish(mut self) -> (Option<CompiledPack>, PackReport) {
        self.run();
        match self.result() {
            Ok((pack, report)) => (Some(pack), report),
            Err(e) => {
                let (_, report) = self.outcome();
                let mut report = report;
                if report.status != PackStatus::Refused {
                    report.status = PackStatus::Failed;
                    if !report.programs.iter().any(|p| p.error.is_some()) {
                        report.notes.insert(0, e.to_string());
                    }
                }
                (None, report)
            }
        }
    }

    /// A session that only reads the pack (properties, options, every
    /// slot's best candidate preprocessed, the plan's pack-wide settings),
    /// run to its end: then [`Self::compile_slot`] compiles chosen slots on
    /// top of that one read (the corpus checker's way).
    pub fn read(
        files: &PackFiles,
        dimension: &str,
        options: &PackOptions,
        host: &HostParams,
    ) -> Session {
        let mut s = Self::with(
            files,
            dimension,
            options,
            host,
            Mode {
                kinds: Vec::new(),
                strict: false,
                compile: true,
                refuse: false,
            },
        );
        s.run();
        s
    }

    /// The candidates slot `kind` may run, best first (after
    /// `program.*.enabled` and [`PackOptions::excluded`]); empty when the
    /// pack has none or the session has not read the pack.
    pub fn candidates(&self, kind: ProgramKind) -> &[String] {
        self.state
            .as_ref()
            .and_then(|st| st.chains.get(&kind))
            .map_or(&[], Vec::as_slice)
    }

    /// Compiles candidate `index` of slot `kind` ([`Self::candidates`]) as
    /// that slot, with no fallback and without changing the session's
    /// outcome; `None` when there is no such candidate. Compiles are shared
    /// with the session's own.
    pub fn compile_slot(
        &mut self,
        kind: ProgramKind,
        index: usize,
    ) -> Option<Result<CompiledProgram, PackError>> {
        let host = self.host;
        let st = self.state.as_mut()?;
        let path = st.chains.get(&kind)?.get(index)?.clone();
        let outputs = st.plan.outputs.get(&kind).cloned();
        let r = st.attempt(kind, &path, &host, true);
        match outputs {
            Some(o) => st.plan.outputs.insert(kind, o),
            None => st.plan.outputs.remove(&kind),
        };
        Some(r.map(|(c, _)| c.expect("compiled")))
    }

    /// The plan as read so far (`None` before the setup step, or when the
    /// setup failed). Slots appear in `programs` once settled.
    pub fn plan(&self) -> Option<&PackPlan> {
        self.state.as_ref().map(|st| &st.plan)
    }

    /// The programs compiled so far, by slot (also when the pack fails).
    pub fn programs(&self) -> Option<&BTreeMap<ProgramKind, CompiledProgram>> {
        self.state.as_ref().map(|st| &st.programs)
    }

    /// The report as it stands (the final one comes from [`Self::finish`]).
    pub fn report(&self) -> PackReport {
        self.outcome().1
    }

    fn run(&mut self) {
        while !self.step() {}
    }

    fn fail(&mut self, e: PackError) {
        self.error = Some(e);
        self.phase = Phase::Done;
    }

    fn setup(&mut self) {
        let predefined = preprocess::predefined(self.options.mc_version);
        let defaults = options::discover(&self.files);
        let files = options::apply(&self.files, &self.options.overrides);
        let mut macros = predefined.clone();
        macros.extend(options::macros(&options::discover(&files)));
        let props = match directives::shaders_properties(&files, &macros) {
            Ok(p) => p,
            Err(e) => return self.fail(e),
        };
        // Optional Iris features Horizon has are macros, for the properties too.
        let flags = features::feature_macros(&props);
        let (props, predefined) = if flags.is_empty() {
            (props, predefined)
        } else {
            macros.extend(flags.clone());
            let props = match directives::shaders_properties(&files, &macros) {
                Ok(p) => p,
                Err(e) => return self.fail(e),
            };
            let mut predefined = predefined;
            predefined.extend(flags);
            (props, predefined)
        };
        // An id file that does not read leaves its ids unlisted: the pack
        // still runs, and the report says why its materials look off.
        let blocks = properties::block_ids(&files, &macros).unwrap_or_else(|e| {
            self.report
                .notes
                .push(format!("{e} (block ids left unlisted)"));
            BlockIds::default()
        });
        let mut ids = |kind: &str, unlisted: i32| {
            properties::id_map(&files, &macros, kind, unlisted).unwrap_or_else(|e| {
                self.report
                    .notes
                    .push(format!("{e} ({kind} ids left unlisted)"));
                properties::IdMap::default()
            })
        };
        let entities = ids("entity", properties::UNLISTED_ENTITY);
        let items = ids("item", properties::UNLISTED_ITEM);
        let folder = world_folder(&files, &self.dimension);
        self.report.features = features::detect(&files, &folder, &props);
        if self.mode.refuse
            && let Some(why) = features::refusal(&self.report.features)
        {
            self.report.refusal = Some(why.clone());
            return self.fail(PackError::new("shaders", why));
        }
        let excluded = &self.options.excluded;
        let usable = |path: &str| !excluded.contains(path) && !props.program_disabled(path);
        let chains: BTreeMap<ProgramKind, Vec<String>> = ProgramKind::all()
            .into_iter()
            .map(|k| (k, find_candidates(&files, &folder, k, &usable)))
            .filter(|(_, c)| !c.is_empty())
            .collect();
        for path in excluded {
            self.report
                .notes
                .push(format!("{path}: left out (the GPU's compiler rejected it)"));
        }
        if self.mode.refuse && !chains.keys().any(|k| runs_the_world(*k)) {
            let why = "This pack has no programs Horizon can run here.".to_string();
            self.report.refusal = Some(why.clone());
            return self.fail(PackError::new(format!("shaders/{folder}"), why));
        }
        let mut wanted = self.mode.kinds.clone();
        wanted.sort();
        wanted.dedup();
        if self.mode.strict
            && let Some(kind) = wanted.iter().find(|k| !chains.contains_key(k))
        {
            return self.fail(PackError::new(
                format!("shaders/{folder}"),
                format!("no program for {kind:?}"),
            ));
        }
        let queue: VecDeque<ProgramKind> = wanted
            .into_iter()
            .filter(|k| chains.contains_key(k))
            .collect();
        // Each slot's best candidate feeds the pack-wide settings.
        let mut to_prepare = VecDeque::new();
        for chain in chains.values() {
            if !to_prepare.contains(&chain[0]) {
                to_prepare.push_back(chain[0].clone());
            }
        }
        let plan = PackPlan {
            properties: props.properties,
            custom_textures: props.textures.clone(),
            options: defaults,
            ..PackPlan::default()
        };
        let mut plan = plan;
        crate::shaders_properties::apply_pass_state(&mut plan, &props);
        self.total = 1 + to_prepare.len() + 1 + queue.len();
        self.state = Some(State {
            files,
            predefined,
            blocks,
            entities,
            items,
            plan,
            chains,
            prepared: BTreeMap::new(),
            to_prepare,
            queue,
            cursor: BTreeMap::new(),
            failed: BTreeMap::new(),
            cache: Vec::new(),
            programs: BTreeMap::new(),
            skipped: Vec::new(),
        });
        self.phase = Phase::Prepare;
    }

    fn prepare_one(&mut self) {
        let st = self.state.as_mut().expect("set up");
        if let Some(path) = st.to_prepare.pop_front() {
            let p = Prepared::new(&st.files, &path, &st.predefined);
            st.prepared.insert(path, p);
        }
        if st.to_prepare.is_empty() {
            self.phase = Phase::Plan;
        }
    }

    fn read_globals(&mut self) {
        let st = self.state.as_mut().expect("set up");
        let firsts: Vec<&Prepared> = {
            let mut seen = BTreeSet::new();
            st.chains
                .values()
                .filter(|c| seen.insert(c[0].clone()))
                .filter_map(|c| st.prepared.get(&c[0])?.as_ref().ok())
                .collect()
        };
        directives::read_globals(
            &mut st.plan,
            firsts.into_iter().map(|p| directives::ProgramText {
                raw: [&p.raw_vsh, &p.raw_fsh],
                preprocessed: [&p.vsh, &p.fsh],
            }),
        );
        self.phase = if st.queue.is_empty() {
            Phase::Done
        } else {
            Phase::Slots
        };
    }

    /// Tries the head slot's current candidate: preprocess it if needed,
    /// then compile it (or reuse a compile of the same key).
    fn settle_one(&mut self) {
        let strict = self.mode.strict;
        let compile = self.mode.compile;
        let host = self.host;
        let st = self.state.as_mut().expect("set up");
        let Some(&kind) = st.queue.front() else {
            self.phase = Phase::Done;
            return;
        };
        let at = *st.cursor.get(&kind).unwrap_or(&0);
        let path = st.chains[&kind][at].clone();
        let result = st.attempt(kind, &path, &host, compile);
        match result {
            Ok((compiled, draw_buffers)) => {
                let p = st.prepared[&path].as_ref().expect("prepared");
                st.plan.programs.insert(kind, path.clone());
                st.plan.draw_buffers.insert(kind, draw_buffers);
                if kind.is_fullscreen() {
                    let mips = directives::mipmaps(&p.raw_fsh, &p.fsh);
                    if !mips.is_empty() {
                        st.plan.mipmaps.insert(kind, mips);
                    }
                }
                if let Some(c) = compiled {
                    st.programs.insert(kind, c);
                }
                st.queue.pop_front();
            }
            Err(e) => {
                st.plan.outputs.remove(&kind);
                if strict {
                    return self.fail(e);
                }
                st.failed.entry(kind).or_default().push(e);
                if at + 1 < st.chains[&kind].len() {
                    st.cursor.insert(kind, at + 1);
                    self.total += 1;
                } else {
                    st.queue.pop_front();
                    let first = st.failed[&kind][0].clone();
                    if kind.is_essential() {
                        // The pack fails; the other slots still go in the report.
                        self.essential.get_or_insert(first);
                    } else {
                        st.skipped.push((kind, first.to_string()));
                    }
                }
            }
        }
        if st.queue.is_empty() {
            self.phase = Phase::Done;
        }
    }

    /// The report so far, and the state's parts it needs.
    fn outcome(&self) -> (Option<&State>, PackReport) {
        let mut report = self.report.clone();
        let Some(st) = &self.state else {
            report.settle();
            return (None, report);
        };
        let mut unknown = BTreeSet::new();
        let mut slots: Vec<ProgramKind> = st
            .chains
            .keys()
            .copied()
            .filter(|k| self.mode.kinds.contains(k))
            .collect();
        slots.sort();
        for kind in slots {
            let failed: Vec<(String, String)> = st
                .failed
                .get(&kind)
                .into_iter()
                .flatten()
                .map(|e| (e.file.clone(), e.message.clone()))
                .collect();
            let file = st.plan.programs.get(&kind).cloned();
            let exhausted = file.is_none() && failed.len() >= st.chains[&kind].len();
            let error = exhausted.then(|| {
                let e = &st.failed[&kind][0];
                e.to_string()
            });
            if let Some(c) = st.programs.get(&kind) {
                unknown.extend(c.unknown_uniforms.iter().cloned());
            }
            report.programs.push(ProgramOutcome {
                slot: kind.name(),
                file,
                failed,
                error,
            });
        }
        report.unknown_uniforms = unknown.into_iter().collect();
        report.settle();
        if (self.error.is_some() || self.essential.is_some())
            && report.status != PackStatus::Refused
        {
            report.status = PackStatus::Failed;
        }
        (Some(st), report)
    }

    /// The compiled pack and the report, or the error that stopped it.
    fn result(&self) -> Result<(CompiledPack, PackReport), PackError> {
        if let Some(e) = self.error.as_ref().or(self.essential.as_ref()) {
            return Err(e.clone());
        }
        let (st, report) = self.outcome();
        let st = st.expect("a session without an error has a state");
        let plan = st.plan.clone();
        Ok((
            CompiledPack {
                programs: st.programs.clone(),
                shadow: ShadowSettings::from(&plan.shadow),
                plan,
                skipped: st.skipped.clone(),
                blocks: st.blocks.clone(),
                entities: st.entities.clone(),
                items: st.items.clone(),
            },
            report,
        ))
    }
}

/// Slots that draw the world or the picture: a pack with none of them has
/// nothing Horizon can run.
fn runs_the_world(kind: ProgramKind) -> bool {
    matches!(
        kind,
        ProgramKind::Terrain
            | ProgramKind::Final
            | ProgramKind::Deferred(_)
            | ProgramKind::Composite(_)
    )
}

/// The checks of a program's targets against the host's limits: they fail
/// the candidate in the translator, so its slot falls back.
fn check_targets(
    kind: ProgramKind,
    draw_buffers: &[u8],
    plan: &PackPlan,
    host: &HostParams,
) -> Result<(), String> {
    if kind.is_shadow() {
        if let Some(&b) = draw_buffers.iter().find(|&&b| b > 1) {
            return Err(format!("shadowcolor{b}: Horizon has shadowcolor0 and 1"));
        }
    } else if let Some(&b) = draw_buffers.iter().find(|&&b| b >= 16) {
        return Err(format!("colortex{b}: Horizon has colortex0 to colortex15"));
    }
    if kind == ProgramKind::Final {
        return Ok(());
    }
    if draw_buffers.len() > host.max_draw_buffers as usize {
        return Err(format!(
            "{} draw buffers, the device allows {}",
            draw_buffers.len(),
            host.max_draw_buffers
        ));
    }
    let bytes: u32 = draw_buffers
        .iter()
        .map(|&b| {
            if kind.is_shadow() {
                plan.shadow.color_format[usize::from(b)].bytes()
            } else {
                plan.buffers[usize::from(b)].format.bytes()
            }
        })
        .sum();
    if bytes > host.max_attachment_bytes {
        return Err(format!(
            "its draw buffers take {bytes} bytes a pixel, the device allows {}",
            host.max_attachment_bytes
        ));
    }
    Ok(())
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
    check_targets(kind, draw_buffers, plan, host).map_err(|m| PackError::new(&fsh_path, m))?;
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
        notes,
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
        notes,
    })
}
