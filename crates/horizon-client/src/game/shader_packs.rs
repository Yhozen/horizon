//! The shader-pack manager: the packs in the `shaderpacks` folder, which one is
//! chosen and whether shaders are on (`horizon-shaders.txt`), each pack's
//! option values (`shaderpacks/<pack>.txt`, the layout Iris and OptiFine write),
//! compiling the chosen pack off the frame, and switching it in and out of the
//! renderer and the mesher while a world runs (docs/shader-packs.md).
//!
//! In the browser the same files live in the game's in-memory file system,
//! filled from the page's storage at boot; every write here is handed to the
//! page with [`persist`] so it survives a reload.

use super::{Dim, Game};
use horizon_core::fs::{self, PathExt as _};
use horizon_glsl_pack::report::{PackReport, PackStatus};
use horizon_glsl_pack::{CompiledPack, HostParams, PackFiles, PackOptions};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Set when the page added packs (browser): the folder is scanned again.
pub(super) static RESCAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The selection file, beside `options.txt`.
pub(super) const SELECTION_FILE: &str = "horizon-shaders.txt";
/// The packs' folder, in the game folder.
pub(super) const FOLDER: &str = "shaderpacks";

/// What the player chose, saved in [`SELECTION_FILE`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Selection {
    pub enabled: bool,
    /// The pack's file name in the folder (`BSL_v10.zip`, or a folder's name).
    pub pack: Option<String>,
}

impl Selection {
    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            match line.split_once(':') {
                Some(("enabled", v)) => s.enabled = v.trim() == "true",
                Some(("pack", v)) if !v.trim().is_empty() => s.pack = Some(v.trim().to_string()),
                _ => {}
            }
        }
        s
    }

    pub fn serialize(&self) -> String {
        format!(
            "enabled:{}\npack:{}\n",
            self.enabled,
            self.pack.as_deref().unwrap_or("")
        )
    }
}

/// One pack in the folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LibraryPack {
    /// File (or folder) name in the folder.
    pub file: String,
    /// Where it is (the folder, or the `--shader-pack` path).
    pub path: PathBuf,
}

/// The packs a folder holds: zips and folders with a `shaders` folder,
/// sorted by name. Option files (`<pack>.txt`) and hidden entries are not packs.
pub(super) fn scan(dir: &Path) -> Vec<LibraryPack> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut packs: Vec<LibraryPack> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            let file = e.file_name().to_string_lossy().into_owned();
            if file.starts_with('.') {
                return None;
            }
            let is_zip = file.to_ascii_lowercase().ends_with(".zip") && path.is_file_vfs();
            let is_pack_dir = path.is_dir_vfs() && path.join("shaders").is_dir_vfs();
            (is_zip || is_pack_dir).then_some(LibraryPack { file, path })
        })
        .collect();
    packs.sort_by_key(|p| p.file.to_lowercase());
    packs
}

/// `NAME=VALUE` lines (`#` comments), as Iris and OptiFine keep a pack's settings.
pub(super) fn parse_values(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            Some((k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}

pub(super) fn serialize_values(values: &[(String, String)]) -> String {
    values.iter().map(|(k, v)| format!("{k}={v}\n")).collect()
}

/// Every file of a folder or zip, by its path inside the pack; a pack zipped
/// with its folder as the top level loses that level.
pub(super) fn read_files(path: &Path) -> Result<PackFiles, String> {
    let mut files = PackFiles::default();
    if path.is_dir_vfs() {
        let mut stack = vec![path.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
                let entry = entry.map_err(|e| e.to_string())?.path();
                if entry.is_dir_vfs() {
                    stack.push(entry);
                } else if let Ok(name) = entry.strip_prefix(path) {
                    let name = name.to_string_lossy().replace('\\', "/");
                    let bytes = fs::read(&entry).map_err(|e| format!("{name}: {e}"))?;
                    files.0.insert(name, bytes);
                }
            }
        }
    } else {
        let zip =
            horizon_client::zip::Zip::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        for name in zip.names() {
            let bytes = zip.read(name).map_err(|e| format!("{name}: {e}"))?;
            files.0.insert(name.to_string(), bytes);
        }
    }
    strip_top_folder(&mut files);
    if !files.0.keys().any(|k| k.starts_with("shaders/")) {
        return Err("no shaders folder: this is not a shader pack".into());
    }
    Ok(files)
}

/// Packs zipped from their folder keep it as the top level: drop it.
fn strip_top_folder(files: &mut PackFiles) {
    if !files.0.keys().any(|k| k.starts_with("shaders/"))
        && let Some(top) = files
            .0
            .keys()
            .filter(|k| !k.starts_with("__MACOSX/"))
            .find_map(|k| k.split_once("/shaders/").map(|(t, _)| t.to_string()))
    {
        let prefix = format!("{top}/");
        files.0 = std::mem::take(&mut files.0)
            .into_iter()
            .filter_map(|(k, v)| Some((k.strip_prefix(&prefix)?.to_string(), v)))
            .collect();
    }
}

/// The dimension id a pack's world folders are chosen by.
pub(super) fn dimension_id(dim: Dim) -> &'static str {
    match dim {
        Dim::Overworld => "minecraft:overworld",
        Dim::Nether => "minecraft:the_nether",
        Dim::End => "minecraft:the_end",
    }
}

/// Hands a write (`Some`) or a delete (`None`) of a file under the game
/// folder to the page, which keeps it (browser only; natively the file
/// system already does).
pub(super) fn persist(path: &Path, bytes: Option<&[u8]>) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast as _;
        let root = fs::current_dir().unwrap_or_default();
        let rel = path.strip_prefix(&root).unwrap_or(path);
        let rel = rel.to_string_lossy().replace('\\', "/");
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(f) = js_sys::Reflect::get(&window, &"horizonPersist".into()) else {
            return;
        };
        let Some(f) = f.dyn_ref::<js_sys::Function>() else {
            return;
        };
        let data = match bytes {
            Some(b) => js_sys::Uint8Array::from(b).into(),
            None => wasm_bindgen::JsValue::NULL,
        };
        if let Err(e) = f.call2(&window, &rel.into(), &data) {
            log::warn!("Horizon: could not keep {}: {e:?}", path.display());
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (path, bytes);
}

/// Calls a page function with no arguments (browser only): the file picker
/// (`horizonPickShaderPack`) must run inside the click that asked for it.
pub(super) fn call_page(name: &str, arg: Option<&str>) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast as _;
        let Some(window) = web_sys::window() else {
            return false;
        };
        let Ok(f) = js_sys::Reflect::get(&window, &name.into()) else {
            return false;
        };
        let Some(f) = f.dyn_ref::<js_sys::Function>() else {
            return false;
        };
        let r = match arg {
            Some(a) => f.call1(&window, &a.into()),
            None => f.call0(&window),
        };
        r.is_ok()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (name, arg);
        false
    }
}

/// What a compile is for: the same key means the same compiled pack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Target {
    pub file: String,
    pub path: PathBuf,
    pub dimension: &'static str,
    pub overrides: Vec<(String, Option<String>)>,
}

/// A compiled pack, kept so a rebuilt renderer gets it back without compiling again.
pub(super) struct Applied {
    pub target: Target,
    pub compiled: Arc<CompiledPack>,
    pub files: Arc<PackFiles>,
}

/// A compile running off the frame.
pub(super) struct Job {
    pub target: Target,
    #[cfg(not(target_arch = "wasm32"))]
    handle: std::thread::JoinHandle<JobResult>,
    #[cfg(target_arch = "wasm32")]
    state: WebJob,
}

#[cfg(target_arch = "wasm32")]
enum WebJob {
    /// Compiled on the next frame, so the screen can first say "Compiling".
    Start(HostParams),
    Done,
}

pub(super) type JobResult = (Option<CompiledPack>, PackReport, Option<Arc<PackFiles>>);

impl Job {
    fn start(target: Target, host: HostParams) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let t = target.clone();
            let handle = std::thread::Builder::new()
                .name("Shader pack".into())
                .spawn(move || compile_target(&t, &host))
                .expect("thread");
            Self { target, handle }
        }
        #[cfg(target_arch = "wasm32")]
        Self {
            target,
            state: WebJob::Start(host),
        }
    }

    /// Advances the compile by a slice of work; the result once done.
    fn poll(&mut self) -> Option<JobResult> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if !self.handle.is_finished() {
                return None;
            }
            let handle = std::mem::replace(&mut self.handle, std::thread::spawn(|| (None, PackReport::default(), None)));
            Some(handle.join().unwrap_or_else(|_| {
                (None, failed_report("the compiler crashed"), None)
            }))
        }
        #[cfg(target_arch = "wasm32")]
        match std::mem::replace(&mut self.state, WebJob::Done) {
            WebJob::Start(host) => Some(compile_target(&self.target, &host)),
            WebJob::Done => None,
        }
    }

    pub fn progress(&self) -> Option<(usize, usize)> {
        None
    }
}

fn failed_report(why: &str) -> PackReport {
    let mut report = PackReport {
        status: PackStatus::Failed,
        ..PackReport::default()
    };
    report.notes.push(why.to_string());
    report
}

fn pack_options(target: &Target) -> PackOptions {
    PackOptions {
        overrides: target.overrides.clone(),
        ..PackOptions::default()
    }
}

/// Reads and compiles a pack (a worker thread natively).
fn compile_target(target: &Target, host: &HostParams) -> JobResult {
    let files = match read_files(&target.path) {
        Ok(f) => Arc::new(f),
        Err(e) => return (None, failed_report(&e), None),
    };
    let (compiled, report) = compile_report(&files, target.dimension, &pack_options(target), host);
    (compiled, report, Some(files))
}

/// Until the crate's staged session lands: one compile, its result as a report.
fn compile_report(
    files: &PackFiles,
    dimension: &str,
    options: &PackOptions,
    host: &HostParams,
) -> (Option<CompiledPack>, PackReport) {
    match horizon_glsl_pack::compile(files, dimension, options, host) {
        Ok(compiled) => {
            let mut report = PackReport::default();
            for (kind, why) in &compiled.skipped {
                report.programs.push(horizon_glsl_pack::report::ProgramOutcome {
                    slot: kind.name(),
                    file: None,
                    failed: Vec::new(),
                    error: Some(why.clone()),
                });
            }
            let mut unknown: Vec<String> = compiled
                .programs
                .values()
                .flat_map(|p| p.unknown_uniforms.iter().cloned())
                .collect();
            unknown.sort();
            unknown.dedup();
            report.unknown_uniforms = unknown;
            report.settle();
            (Some(compiled), report)
        }
        Err(e) => (None, failed_report(&e.to_string())),
    }
}

/// The manager's state, in `Game`.
#[derive(Default)]
pub(super) struct ShaderPacks {
    pub selection: Selection,
    pub library: Vec<LibraryPack>,
    /// `--shader-pack PATH`: chosen for this run, not saved.
    pub cli: Option<LibraryPack>,
    /// `--shader-pack-option NAME=VALUE`, on top of the saved values.
    pub cli_options: Vec<(String, String)>,
    /// Each pack's saved option values, read when first needed.
    pub values: BTreeMap<String, Vec<(String, String)>>,
    pub reports: BTreeMap<String, PackReport>,
    pub job: Option<Job>,
    pub applied: Option<Applied>,
    /// The last target that failed, not retried until something changes.
    pub failed: Option<Target>,
    loaded: bool,
}

impl ShaderPacks {
    pub fn folder() -> PathBuf {
        fs::current_dir().unwrap_or_default().join(FOLDER)
    }

    fn selection_path() -> PathBuf {
        fs::current_dir().unwrap_or_default().join(SELECTION_FILE)
    }

    fn values_path(file: &str) -> PathBuf {
        Self::folder().join(format!("{file}.txt"))
    }

    /// Reads the selection and scans the folder (once; [`Self::rescan`] again).
    pub fn load(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        let (cli, options) = super::glsl_pack::cli_pack();
        if let Some(path) = cli {
            let file = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            self.cli = Some(LibraryPack { file, path });
        }
        self.cli_options = options;
        if let Ok(text) = fs::read_to_string(Self::selection_path()) {
            self.selection = Selection::parse(&text);
        }
        self.rescan();
    }

    pub fn rescan(&mut self) {
        self.library = scan(&Self::folder());
    }

    pub fn save_selection(&self) {
        if self.cli.is_some() {
            return;
        }
        let path = Self::selection_path();
        let text = self.selection.serialize();
        if let Err(e) = fs::write(&path, &text) {
            log::warn!("Horizon: could not save {}: {e}", path.display());
        }
        persist(&path, Some(text.as_bytes()));
    }

    pub fn values_of(&mut self, file: &str) -> &Vec<(String, String)> {
        self.values.entry(file.to_string()).or_insert_with(|| {
            fs::read_to_string(Self::values_path(file))
                .map(|t| parse_values(&t))
                .unwrap_or_default()
        })
    }

    pub fn save_values(&mut self, file: &str, values: Vec<(String, String)>) {
        let path = Self::values_path(file);
        if values.is_empty() {
            let _ = fs::remove_file(&path);
            persist(&path, None);
        } else {
            let text = serialize_values(&values);
            if let Err(e) = fs::write(&path, &text) {
                log::warn!("Horizon: could not save {}: {e}", path.display());
            }
            persist(&path, Some(text.as_bytes()));
        }
        self.values.insert(file.to_string(), values);
    }

    /// The chosen pack, if shaders are on.
    pub fn chosen(&self) -> Option<LibraryPack> {
        if let Some(cli) = &self.cli {
            return Some(cli.clone());
        }
        if !self.selection.enabled {
            return None;
        }
        let file = self.selection.pack.as_ref()?;
        self.library.iter().find(|p| &p.file == file).cloned()
    }

    /// What should be compiled for `dim`, if anything.
    pub fn target(&mut self, dim: Dim) -> Option<Target> {
        let pack = self.chosen()?;
        let mut values = self.values_of(&pack.file).clone();
        values.extend(self.cli_options.iter().cloned());
        Some(Target {
            overrides: values
                .into_iter()
                .map(|(k, v)| (k, Some(v)))
                .collect(),
            file: pack.file,
            path: pack.path,
            dimension: dimension_id(dim),
        })
    }

    pub fn progress(&self) -> Option<(usize, usize)> {
        let job = self.job.as_ref()?;
        Some(job.progress().unwrap_or((0, 0)))
    }

    /// Deletes a pack and its settings from the folder.
    pub fn delete(&mut self, file: &str) {
        let Some(pack) = self.library.iter().find(|p| p.file == file).cloned() else {
            return;
        };
        let removed = if pack.path.is_dir_vfs() {
            fs::remove_dir_all(&pack.path)
        } else {
            fs::remove_file(&pack.path)
        };
        match removed {
            Ok(()) => {
                persist(&pack.path, None);
                let values = Self::values_path(file);
                if fs::remove_file(&values).is_ok() {
                    persist(&values, None);
                }
            }
            Err(e) => log::warn!("Horizon: could not delete {}: {e}", pack.path.display()),
        }
        if self.selection.pack.as_deref() == Some(file) {
            self.selection.pack = None;
            self.save_selection();
        }
        self.reports.remove(file);
        self.values.remove(file);
        self.rescan();
    }

    /// Copies a dropped zip or folder into the folder.
    pub fn add_dropped(&mut self, from: &Path) -> Result<String, String> {
        let name = from
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or("no file name")?;
        let dir = Self::folder();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        if from.is_dir_vfs() {
            return Err("drop a .zip file (folders can be put in the shaderpacks folder)".into());
        }
        if !name.to_ascii_lowercase().ends_with(".zip") {
            return Err(format!("{name} is not a .zip file"));
        }
        let bytes = fs::read(from).map_err(|e| e.to_string())?;
        let to = dir.join(&name);
        fs::write(&to, &bytes).map_err(|e| e.to_string())?;
        persist(&to, Some(&bytes));
        self.rescan();
        Ok(name)
    }
}

impl Game<'_> {
    /// Whether the pack can draw this frame's world: a world (not the title
    /// screen) and a compiled pack for this dimension, installed in the renderer.
    pub(super) fn shader_pack_draws(&self, title: bool) -> bool {
        !title
            && self
                .shader_packs
                .applied
                .as_ref()
                .is_some_and(|a| a.target.dimension == dimension_id(self.dim))
    }

    /// Once a frame: starts, polls and installs compiles so the running pack
    /// matches the selection, the dimension and the options; reinstalls the
    /// pack into a rebuilt renderer and its tags into a rebuilt mesher.
    pub(super) fn tick_shader_packs(&mut self, title: bool) {
        self.shader_packs.load();
        if RESCAN.swap(false, std::sync::atomic::Ordering::Relaxed) {
            self.shader_packs.rescan();
        }
        let want = if title {
            None
        } else {
            self.shader_packs.target(self.dim)
        };
        // A finished compile.
        if let Some(job) = &mut self.shader_packs.job
            && let Some((compiled, mut report, files)) = job.poll()
        {
            let target = self.shader_packs.job.take().map(|j| j.target).expect("job");
            let installed = match (compiled, files) {
                (Some(compiled), Some(files)) => {
                    let compiled = Arc::new(compiled);
                    match self.install_shader_pack(&compiled, &files) {
                        Ok(()) => Some(Applied {
                            target: target.clone(),
                            compiled,
                            files,
                        }),
                        Err(e) => {
                            report.status = PackStatus::Failed;
                            report.gpu_errors.push(("pipelines".into(), e));
                            None
                        }
                    }
                }
                _ => None,
            };
            log::info!(
                "Horizon: shader pack {} ({}): {}\n{}",
                target.file,
                target.dimension,
                report.status.label(),
                report.text(&target.file)
            );
            self.announce_shader_pack(&target.file, &report);
            self.shader_packs.reports.insert(target.file.clone(), report);
            match installed {
                Some(applied) => {
                    self.shader_packs.applied = Some(applied);
                    self.shader_packs.failed = None;
                }
                None => {
                    self.shader_packs.failed = Some(target);
                    self.uninstall_shader_pack();
                }
            }
        }
        if self.shader_packs.job.is_some() {
            return;
        }
        match want {
            None => {
                if self.shader_packs.applied.is_some() {
                    self.uninstall_shader_pack();
                }
            }
            Some(target) => {
                let current = self.shader_packs.applied.as_ref().map(|a| &a.target);
                if current == Some(&target) {
                    self.keep_shader_pack_installed();
                } else if self.shader_packs.failed.as_ref() != Some(&target) {
                    if let Some(host) = self.renderer.as_ref().map(|r| r.shader_pack_host()) {
                        self.shader_packs.job = Some(Job::start(target, host));
                    }
                }
            }
        }
    }

    /// Puts a compiled pack into the renderer and its tags into the mesher.
    fn install_shader_pack(
        &mut self,
        compiled: &Arc<CompiledPack>,
        files: &Arc<PackFiles>,
    ) -> Result<(), String> {
        let Some(renderer) = self.renderer.as_mut() else {
            return Err("no renderer".into());
        };
        let mut compiled_ref = (**compiled).clone();
        if std::env::var("HORIZON_SHADER_PACK_SHADOWS").is_ok_and(|v| v == "0") {
            for kind in [
                horizon_glsl_pack::ProgramKind::Shadow,
                horizon_glsl_pack::ProgramKind::ShadowEntities,
            ] {
                compiled_ref.programs.remove(&kind);
                compiled_ref.plan.programs.remove(&kind);
            }
        }
        renderer.set_shader_pack(Some((&compiled_ref, files)))?;
        self.set_shader_pack_tags(Some(compiled));
        Ok(())
    }

    /// Back to Horizon's own shaders.
    fn uninstall_shader_pack(&mut self) {
        if let Some(renderer) = self.renderer.as_mut() {
            let _ = renderer.set_shader_pack(None);
        }
        if self.shader_packs.applied.take().is_some() {
            self.set_shader_pack_tags(None);
        }
    }

    /// A renderer rebuilt by a resource reload starts without the pack.
    fn keep_shader_pack_installed(&mut self) {
        let Some(applied) = &self.shader_packs.applied else {
            return;
        };
        let missing = self
            .renderer
            .as_ref()
            .is_some_and(|r| !r.has_shader_pack());
        if missing {
            let (compiled, files) = (applied.compiled.clone(), applied.files.clone());
            if let Err(e) = self.install_shader_pack(&compiled, &files) {
                log::warn!("Horizon: shader pack could not be reinstalled: {e}");
                self.shader_packs.applied = None;
            }
        }
    }

    /// The mesher's per-block data for the pack (`None`: Horizon's own).
    fn set_shader_pack_tags(&mut self, compiled: Option<&Arc<CompiledPack>>) {
        // TODO(vertex stream): swappable tags; until then they can only be set once.
        if let Some(c) = compiled {
            self.pack
                .context
                .tables
                .set_pack_tags(horizon_client::mesher::PackTags::new(
                    self.pack.assets.registry,
                    &c.blocks,
                    &c.plan,
                ));
        }
    }

    /// A file dropped on the window: a zip goes into the packs folder.
    pub(super) fn shader_pack_dropped(&mut self, path: &Path) {
        self.shader_packs.load();
        let line = match self.shader_packs.add_dropped(path) {
            Ok(name) => format!("Added shader pack {name}."),
            Err(e) => format!("Not added: {e}"),
        };
        self.system_message(horizon_client::text::Text::literal(line));
    }

    /// A chat line when a pack finishes loading.
    fn announce_shader_pack(&mut self, file: &str, report: &PackReport) {
        let text = match report.status {
            PackStatus::Works => format!("Shader pack {file} loaded."),
            PackStatus::Partial => {
                format!("Shader pack {file} loaded with parts missing (Options > Horizon Settings > Shader Packs > Report).")
            }
            PackStatus::Refused => format!(
                "Shader pack {file} is not supported: {}",
                report.refusal.as_deref().unwrap_or("see its report")
            ),
            PackStatus::Failed => {
                format!("Shader pack {file} could not be loaded (see its report).")
            }
        };
        self.system_message(horizon_client::text::Text::literal(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_round_trips() {
        let s = Selection {
            enabled: true,
            pack: Some("BSL v10.zip".into()),
        };
        assert_eq!(Selection::parse(&s.serialize()), s);
        assert_eq!(Selection::parse("garbage\n"), Selection::default());
    }

    #[test]
    fn option_values_round_trip() {
        let values = vec![("SHADOWS".to_string(), "false".to_string()), ("sunPathRotation".to_string(), "-30.0".to_string())];
        assert_eq!(parse_values(&serialize_values(&values)), values);
        assert_eq!(parse_values("# comment\n\nA = 1\n"), vec![("A".into(), "1".into())]);
    }

    #[test]
    fn zipped_folders_lose_their_top_level() {
        let mut files = PackFiles::default();
        files.0.insert("Pack v1/shaders/final.fsh".into(), vec![]);
        files.0.insert("Pack v1/readme.txt".into(), vec![]);
        strip_top_folder(&mut files);
        assert!(files.0.contains_key("shaders/final.fsh"));
        assert!(files.0.contains_key("readme.txt"));
    }
}
