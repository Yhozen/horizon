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

/// The Shader Packs page shows (the page's drop handler asks, browser).
pub(super) static PAGE_OPEN: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Lines the page has for the player (browser: a pack that could not be
/// stored or loaded), shown in chat on the next frame.
static NOTICES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Queues a line for the player (`web.rs` `shader_pack_notice`).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(super) fn notice(line: String) {
    NOTICES.lock().unwrap_or_else(|e| e.into_inner()).push(line);
}

/// How long the Shader Packs page shows the last notice.
const NOTICE_SHOWN: std::time::Duration = std::time::Duration::from_secs(12);

fn take_notices() -> Vec<String> {
    std::mem::take(&mut *NOTICES.lock().unwrap_or_else(|e| e.into_inner()))
}

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
    /// The pack was installed but has not drawn for [`PROVEN_AFTER`] yet
    /// (`trial:true`): found at start, the game stopped in that time (it may
    /// have crashed it), so the pack is not turned on again by itself.
    pub trial: bool,
}

/// A pack's file name, one entry of the folder: never a path out of it
/// (`pack:../options` would reach `options.txt` through the option file).
pub(super) fn is_pack_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && !name.contains("..")
}

/// How long a pack draws before a crash is no longer blamed on it.
const PROVEN_AFTER: std::time::Duration = std::time::Duration::from_secs(10);

/// Shaders were left on with a pack chosen, before the device opens.
pub(super) fn saved_selection_on() -> bool {
    let path = fs::current_dir().unwrap_or_default().join(SELECTION_FILE);
    fs::read_to_string(path)
        .map(|text| Selection::parse(&text))
        .is_ok_and(|s| s.enabled && s.pack.is_some() && !s.trial)
}

impl Selection {
    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            match line.split_once(':') {
                Some(("enabled", v)) => s.enabled = v.trim() == "true",
                Some(("trial", v)) => s.trial = v.trim() == "true",
                Some(("pack", v)) if is_pack_name(v.trim()) => s.pack = Some(v.trim().to_string()),
                _ => {}
            }
        }
        s
    }

    pub fn serialize(&self) -> String {
        format!(
            "enabled:{}\npack:{}\n{}",
            self.enabled,
            self.pack.as_deref().unwrap_or(""),
            if self.trial { "trial:true\n" } else { "" }
        )
    }

    /// Whether this selection, read at start, turns its pack on: not when
    /// the last run stopped while the pack was on trial.
    pub fn starts_on(&self) -> bool {
        self.enabled && !self.trial
    }
}

/// One pack in the folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LibraryPack {
    /// File (or folder) name in the folder.
    pub file: String,
    /// Where it is (the folder, or the `--shader-pack` path).
    pub path: PathBuf,
    /// Its size and time (a zip's): a pack added again under the same name
    /// changes it, which drops what was read or compiled from the old one.
    pub stamp: u64,
}

/// [`LibraryPack::stamp`]; 0 for folders. In the browser a not yet loaded
/// pack is an empty file (`web/menu/shaderpacks.ts`).
fn stamp_of(path: &Path) -> u64 {
    let Ok(meta) = fs::metadata(path) else {
        return 0;
    };
    if meta.is_dir() {
        return 0;
    }
    let time = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as u64);
    meta.len() ^ time.rotate_left(17)
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
            (is_zip || is_pack_dir).then(|| LibraryPack {
                file,
                stamp: stamp_of(&path),
                path,
            })
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

/// How much of a pack is read: far above any real pack (the largest known
/// has under 1,000 files and 20 MB), well below what would exhaust memory.
#[derive(Clone, Copy)]
pub(super) struct ReadLimits {
    pub files: usize,
    pub bytes: u64,
}

pub(super) const READ_LIMITS: ReadLimits = ReadLimits {
    files: 10_000,
    bytes: 256 << 20,
};

/// Whether a path inside a pack is one of its shader files (the pack's own
/// `shaders/` folder, perhaps under one top folder), read at all.
fn is_pack_file(name: &str) -> bool {
    !name.starts_with("__MACOSX/") && name.split('/').take(2).any(|part| part == "shaders")
}

/// Counts what is read against [`ReadLimits`].
struct Budget {
    limits: ReadLimits,
    files: usize,
    bytes: u64,
}

impl Budget {
    /// One more file of `bytes`.
    fn take(&mut self, bytes: u64) -> Result<(), String> {
        self.files += 1;
        self.grow(bytes)
    }

    /// `bytes` more than counted.
    fn grow(&mut self, bytes: u64) -> Result<(), String> {
        self.bytes = self.bytes.saturating_add(bytes);
        if self.files > self.limits.files {
            return Err(format!(
                "the pack has more than {} files: not read",
                self.limits.files
            ));
        }
        if self.bytes > self.limits.bytes {
            return Err(format!(
                "the pack unpacks to more than {} MiB: not read",
                self.limits.bytes >> 20
            ));
        }
        Ok(())
    }
}

/// Every shader file of a folder or zip, by its path inside the pack; a pack
/// zipped with its folder as the top level loses that level.
pub(super) fn read_files(path: &Path) -> Result<PackFiles, String> {
    read_files_within(path, READ_LIMITS)
}

pub(super) fn read_files_within(path: &Path, limits: ReadLimits) -> Result<PackFiles, String> {
    let mut files = PackFiles::default();
    let mut budget = Budget {
        limits,
        files: 0,
        bytes: 0,
    };
    if path.is_dir_vfs() {
        let mut stack = vec![path.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
                let entry = entry.map_err(|e| e.to_string())?.path();
                let Ok(name) = entry.strip_prefix(path) else {
                    continue;
                };
                let name = name.to_string_lossy().replace('\\', "/");
                // A link back up the tree would never end.
                if std::fs::symlink_metadata(&entry).is_ok_and(|m| m.file_type().is_symlink()) {
                    continue;
                }
                if entry.is_dir_vfs() {
                    if name.split('/').count() < 2 || is_pack_file(&name) {
                        stack.push(entry);
                    }
                } else if is_pack_file(&name) {
                    let bytes = fs::read(&entry).map_err(|e| format!("{name}: {e}"))?;
                    budget.take(bytes.len() as u64)?;
                    files.0.insert(name, bytes);
                }
            }
        }
    } else {
        let zip =
            horizon_client::zip::Zip::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        for name in zip.names().filter(|n| is_pack_file(n)) {
            // The declared size first, so a bomb is refused before it inflates
            // (an entry inflates to at most about twice what it declares).
            let declared = zip.size(name).unwrap_or(0);
            budget.take(declared)?;
            let bytes = zip.read(name).map_err(|e| format!("{name}: {e}"))?;
            budget.grow((bytes.len() as u64).saturating_sub(declared))?;
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
    pub stamp: u64,
    pub dimension: &'static str,
    pub overrides: Vec<(String, Option<String>)>,
    /// Programs the GPU rejected earlier: their slots fall back.
    pub excluded: std::collections::BTreeSet<String>,
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
    handle: Option<std::thread::JoinHandle<JobResult>>,
    #[cfg(target_arch = "wasm32")]
    state: WebJob,
}

#[cfg(target_arch = "wasm32")]
enum WebJob {
    /// The files are read on the next frame, so the screen can first say "Compiling".
    Start(HostParams),
    Running(Box<horizon_glsl_pack::session::Session>, Arc<PackFiles>),
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
            Self {
                target,
                handle: Some(handle),
            }
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
            if !self.handle.as_ref().is_some_and(|h| h.is_finished()) {
                return None;
            }
            let handle = self.handle.take()?;
            Some(
                handle
                    .join()
                    .unwrap_or_else(|_| (None, failed_report("the compiler crashed"), None)),
            )
        }
        #[cfg(target_arch = "wasm32")]
        match std::mem::replace(&mut self.state, WebJob::Done) {
            WebJob::Start(host) => {
                let files = match read_files(&self.target.path) {
                    Ok(f) => Arc::new(f),
                    Err(e) => return Some((None, failed_report(&e), None)),
                };
                let session = horizon_glsl_pack::session::Session::new(
                    &files,
                    self.target.dimension,
                    &pack_options(&self.target),
                    &host,
                );
                self.state = WebJob::Running(Box::new(session), files);
                None
            }
            WebJob::Running(mut session, files) => {
                // About two frames' worth of work at a time, so the page stays alive.
                let started = web_time::Instant::now();
                while started.elapsed().as_millis() < 30 {
                    if session.step() {
                        let (compiled, report) = session.finish();
                        return Some((compiled, report, Some(files)));
                    }
                }
                self.state = WebJob::Running(session, files);
                None
            }
            WebJob::Done => None,
        }
    }

    pub fn progress(&self) -> Option<(usize, usize)> {
        #[cfg(target_arch = "wasm32")]
        if let WebJob::Running(session, _) = &self.state {
            return Some(session.progress());
        }
        None
    }
}

fn held_back_report() -> PackReport {
    failed_report(
        "Not turned on at start: the game stopped before this pack had drawn for 10 seconds \
         last time, so it may have crashed the game. Choose Done here to try it again.",
    )
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
        excluded: target.excluded.clone(),
        ..PackOptions::default()
    }
}

/// The programs a GPU error names (pipelines and shader modules are
/// labelled with their program's pack path).
fn programs_named(error: &str, compiled: &CompiledPack) -> Vec<String> {
    compiled
        .programs
        .values()
        .map(|p| p.source.clone())
        .filter(|source| error.contains(source.as_str()))
        .collect()
}

/// Reads and compiles a pack (a worker thread natively).
#[cfg(not(target_arch = "wasm32"))]
fn compile_target(target: &Target, host: &HostParams) -> JobResult {
    let files = match read_files(&target.path) {
        Ok(f) => Arc::new(f),
        Err(e) => return (None, failed_report(&e), None),
    };
    let (compiled, report) = horizon_glsl_pack::session::compile_report(
        &files,
        target.dimension,
        &pack_options(target),
        host,
    );
    (compiled, report, Some(files))
}

/// The manager's state, in `Game`.
#[derive(Default)]
pub(super) struct ShaderPacks {
    /// What the pages show and edit.
    pub selection: Selection,
    /// What the game runs: the selection and the option values as of the
    /// last Apply (or the saved ones at start).
    pub active: Selection,
    pub active_values: Option<Vec<(String, String)>>,
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
    /// Each pack's option screens, read when its settings are first shown
    /// (`None`: it could not be read; tried again after a rescan).
    pub models: BTreeMap<String, Option<Arc<horizon_glsl_pack::screens::OptionModel>>>,
    /// The pack compiled before the applied one, kept so going back (a
    /// portal round trip, the title screen) does not compile again.
    pub previous: Option<Applied>,
    /// Packs asked of the page (browser: not loaded at boot).
    requested: std::collections::BTreeSet<String>,
    /// The settings sub-screen shown (`None`: the root).
    pub settings_screen: Option<String>,
    /// Per pack, programs the GPU rejected (retried without them).
    pub excluded: BTreeMap<String, std::collections::BTreeSet<String>>,
    /// The running selection has drawn for [`PROVEN_AFTER`] this run.
    proven: bool,
    /// When the pack on trial first drew.
    trial_drawn: Option<web_time::Instant>,
    /// A pack left on trial by the last run, not turned on (said in chat once in a world).
    held_back: Option<String>,
    /// The last line about adding or loading packs, and when (shown on the page).
    pub notice: Option<(String, web_time::Instant)>,
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
        self.cli_options = options;
        if let Ok(text) = fs::read_to_string(Self::selection_path()) {
            self.selection = Selection::parse(&text);
        }
        self.rescan();
        self.active = self.selection.clone();
        if self.selection.enabled
            && !self.selection.starts_on()
            && let Some(file) = self.selection.pack.clone()
        {
            // The last run stopped before this pack had drawn for a while.
            self.active.enabled = false;
            self.reports.insert(file.clone(), held_back_report());
            self.held_back = Some(file);
        }
        if let Some(path) = cli {
            let file = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            // Natively only the game's own folder; in the browser the page's
            // `?shaderpack=` is the relative `shaderpacks/NAME`.
            let in_folder = path.parent().is_some_and(|p| {
                fs::current_dir().unwrap_or_default().join(p) == Self::folder()
                    || (cfg!(target_arch = "wasm32") && p.ends_with(FOLDER))
            });
            if in_folder && self.library.iter().any(|p| p.file == file) {
                // A pack the page put in the folder (`?shaderpack=`): chosen and kept.
                self.selection = Selection {
                    enabled: true,
                    pack: Some(file),
                    trial: false,
                };
                self.active = self.selection.clone();
                self.save_selection();
            } else {
                self.cli = Some(LibraryPack {
                    file,
                    stamp: stamp_of(&path),
                    path,
                });
            }
        }
    }

    pub fn rescan(&mut self) {
        let old = std::mem::replace(&mut self.library, scan(&Self::folder()));
        self.models.retain(|_, m| m.is_some());
        let changed: Vec<String> = self
            .library
            .iter()
            .filter(|p| !old.iter().any(|o| o.file == p.file && o.stamp == p.stamp))
            .map(|p| p.file.clone())
            .collect();
        for file in changed {
            self.forget(&file);
        }
    }

    /// Drops what was read or compiled from a pack (added again, or loaded in the browser).
    fn forget(&mut self, file: &str) {
        self.models.remove(file);
        self.reports.remove(file);
        self.excluded.remove(file);
        self.requested.remove(file);
        if self.failed.as_ref().is_some_and(|t| t.file == file) {
            self.failed = None;
        }
        if self
            .previous
            .as_ref()
            .is_some_and(|a| a.target.file == file)
        {
            self.previous = None;
        }
    }

    /// Whether `file` is a pack of the library (or the `--shader-pack` one).
    fn known(&self, file: &str) -> bool {
        self.library
            .iter()
            .chain(self.cli.iter())
            .any(|p| p.file == file)
    }

    /// Whether a pack's bytes are here. In the browser the page loads packs
    /// other than the selected one when first needed (an empty placeholder
    /// until then): this asks for it once, and the page's rescan follows.
    fn ready(&mut self, pack: &LibraryPack) -> bool {
        if !cfg!(target_arch = "wasm32")
            || pack.path.is_dir_vfs()
            || fs::metadata(&pack.path).is_ok_and(|m| m.len() > 0)
        {
            return true;
        }
        if self.requested.insert(pack.file.clone()) {
            let root = fs::current_dir().unwrap_or_default();
            let rel = pack.path.strip_prefix(&root).unwrap_or(&pack.path);
            let rel = rel.to_string_lossy().replace('\\', "/");
            if !call_page("horizonLoadShaderPack", Some(&rel)) {
                log::warn!("Horizon: shader pack {} could not be asked for", pack.file);
            }
        }
        false
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
        if !self.known(file) {
            return self.values.entry(file.to_string()).or_default();
        }
        self.values.entry(file.to_string()).or_insert_with(|| {
            fs::read_to_string(Self::values_path(file))
                .map(|t| parse_values(&t))
                .unwrap_or_default()
        })
    }

    pub fn save_values(&mut self, file: &str, values: Vec<(String, String)>) {
        if !self.known(file) || !is_pack_name(file) {
            return;
        }
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

    /// A pack's options and screens (read from its files once).
    /// A pack's options and screens (read from its files once; a pack that
    /// can't be read is not read again until a rescan).
    pub fn model_of(&mut self, file: &str) -> Option<Arc<horizon_glsl_pack::screens::OptionModel>> {
        if let Some(m) = self.models.get(file) {
            return m.clone();
        }
        let pack = self
            .library
            .iter()
            .chain(self.cli.iter())
            .find(|p| p.file == file)?
            .clone();
        if !self.ready(&pack) {
            return None;
        }
        let model = match read_files(&pack.path) {
            Ok(files) => Some(Arc::new(horizon_glsl_pack::screens::option_model(&files))),
            Err(e) => {
                log::warn!("Horizon: shader pack {file}: {e}");
                self.reports
                    .entry(file.to_string())
                    .or_insert_with(|| failed_report(&e));
                None
            }
        };
        self.models.insert(file.to_string(), model.clone());
        model
    }

    /// The chosen pack, if shaders are on.
    pub fn chosen(&self) -> Option<LibraryPack> {
        if let Some(cli) = &self.cli {
            return Some(cli.clone());
        }
        if !self.active.enabled {
            return None;
        }
        let file = self.active.pack.as_ref()?;
        self.library.iter().find(|p| &p.file == file).cloned()
    }

    /// Makes the pages' choices the running ones.
    pub fn apply(&mut self) {
        self.active = self.selection.clone();
        self.active_values = self.active.pack.clone().map(|f| self.values_of(&f).clone());
        self.failed = None;
        self.proven = false;
        self.trial_drawn = None;
        self.held_back = None;
        if !self.active.enabled {
            self.selection.trial = false;
            self.active.trial = false;
        }
        self.save_selection();
    }

    /// Once a frame while a pack is installed in a world: puts it on trial
    /// (saved before it draws), and ends the trial once it has drawn for [`PROVEN_AFTER`].
    fn tick_trial(&mut self, in_world: bool, draws: bool) {
        if in_world && self.trial_step(draws, web_time::Instant::now()) {
            self.save_selection();
        }
    }

    /// The pack was turned off without a crash: nothing to hold back at the next start.
    fn end_trial(&mut self) {
        self.trial_drawn = None;
        if self.selection.trial && self.cli.is_none() {
            self.selection.trial = false;
            self.active.trial = false;
            self.save_selection();
        }
    }

    /// [`Self::tick_trial`]'s bookkeeping; true when the saved flag changes.
    fn trial_step(&mut self, draws: bool, now: web_time::Instant) -> bool {
        if self.proven || self.cli.is_some() {
            return false;
        }
        let mut on = true;
        if draws {
            let first = *self.trial_drawn.get_or_insert(now);
            if now.duration_since(first) >= PROVEN_AFTER {
                self.proven = true;
                self.trial_drawn = None;
                on = false;
            }
        }
        let changed = self.selection.trial != on;
        self.selection.trial = on;
        self.active.trial = on;
        changed
    }

    /// What should be compiled for `dim`, if anything.
    pub fn target(&mut self, dim: Dim) -> Option<Target> {
        let pack = self.chosen()?;
        let mut values = match (&self.active_values, &self.cli) {
            (Some(v), None) => v.clone(),
            _ => self.values_of(&pack.file).clone(),
        };
        values.extend(self.cli_options.iter().cloned());
        Some(Target {
            overrides: values.into_iter().map(|(k, v)| (k, Some(v))).collect(),
            excluded: self.excluded.get(&pack.file).cloned().unwrap_or_default(),
            file: pack.file,
            path: pack.path,
            stamp: pack.stamp,
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
        self.forget(file);
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
    /// pack into a rebuilt renderer and its tags into a rebuilt mesher. On
    /// the title and loading screens the installed pack stays (it doesn't
    /// draw there) and nothing new is compiled.
    pub(super) fn tick_shader_packs(&mut self, title: bool) {
        self.shader_packs.load();
        if RESCAN.swap(false, std::sync::atomic::Ordering::Relaxed) {
            self.shader_packs.rescan();
        }
        for line in take_notices() {
            self.shader_packs.notice = Some((line.clone(), web_time::Instant::now()));
            self.system_message(horizon_client::text::Text::literal(line));
        }
        let want = self.shader_packs.target(self.dim);
        // A finished compile.
        if let Some(job) = &mut self.shader_packs.job
            && let Some(result) = job.poll()
        {
            let target = self.shader_packs.job.take().map(|j| j.target).expect("job");
            if want.as_ref() == Some(&target) {
                self.finish_shader_pack(target, result);
            } else {
                // Shaders turned off, or another pack or option chosen meanwhile.
                log::info!(
                    "Horizon: shader pack {} ({}) compiled but no longer wanted",
                    target.file,
                    target.dimension
                );
            }
        }
        // The GPU rejected the running pack (in the browser, frames later;
        // natively also any error the device reports while it is installed).
        if let Some(error) = self.renderer.as_mut().and_then(|r| r.shader_pack_error())
            && let Some(applied) = self.shader_packs.applied.take()
        {
            let file = applied.target.file.clone();
            self.set_shader_pack_tags(None);
            self.shader_packs.end_trial();
            if !self.exclude_rejected(&file, &error, &applied.compiled) {
                let report = self.shader_packs.reports.entry(file.clone()).or_default();
                report.status = PackStatus::Failed;
                report.gpu_errors.push(("GPU".into(), error.clone()));
                log::warn!("Horizon: shader pack {file} turned off by the GPU: {error}");
                self.shader_packs.failed = Some(applied.target);
                self.system_message(horizon_client::text::Text::literal(format!(
                    "Shader pack {file} was turned off: the GPU rejected it (see its report)."
                )));
            }
        }
        if self.shader_packs.applied.is_some() {
            let draws = self.shader_pack_draws(title);
            self.shader_packs.tick_trial(!title, draws);
        }
        if !title && let Some(file) = self.shader_packs.held_back.take() {
            self.system_message(horizon_client::text::Text::literal(format!(
                "Shader pack {file} was not turned on: the game stopped while it was new last time (see its report)."
            )));
        }
        let Some(target) = want else {
            // Off at once, even while a compile runs (natively its thread
            // finishes and the result is dropped; in the browser it stops).
            #[cfg(target_arch = "wasm32")]
            {
                self.shader_packs.job = None;
            }
            self.shader_packs.previous = None;
            if self.shader_packs.applied.is_some() {
                self.uninstall_shader_pack();
            }
            return;
        };
        if self.shader_packs.job.is_some() || title {
            return;
        }
        let current = self.shader_packs.applied.as_ref().map(|a| &a.target);
        if current == Some(&target) {
            self.keep_shader_pack_installed();
            return;
        }
        // Another dimension: the old pack can't draw it, so Horizon's own
        // shaders (and meshes) take over until the new one is ready.
        if self
            .shader_packs
            .applied
            .as_ref()
            .is_some_and(|a| a.target.dimension != target.dimension)
        {
            self.shader_packs.previous = self.uninstall_shader_pack();
        }
        if self.shader_packs.previous.as_ref().map(|a| &a.target) == Some(&target) {
            let back = self.shader_packs.previous.take().expect("previous");
            match self.install_shader_pack(&back.compiled, &back.files) {
                Ok(()) => {
                    self.shader_packs.previous = self.shader_packs.applied.replace(back);
                    return;
                }
                Err(e) => log::warn!(
                    "Horizon: shader pack {} could not be put back: {e}",
                    target.file
                ),
            }
        }
        if self.shader_packs.failed.as_ref() == Some(&target) {
            return;
        }
        let pack = LibraryPack {
            file: target.file.clone(),
            path: target.path.clone(),
            stamp: target.stamp,
        };
        if !self.shader_packs.ready(&pack) {
            return;
        }
        if let Some(host) = self.renderer.as_ref().map(|r| r.shader_pack_host()) {
            self.shader_packs.job = Some(Job::start(target, host));
        }
    }

    /// Installs a compile that is still wanted, or records why it can't be.
    fn finish_shader_pack(&mut self, target: Target, (compiled, mut report, files): JobResult) {
        let installed = match (compiled, files) {
            (Some(compiled), Some(files)) => {
                let compiled = Arc::new(compiled);
                for program in compiled.programs.values() {
                    report.notes.extend(
                        program
                            .notes
                            .iter()
                            .map(|n| format!("{}: {n}", program.source)),
                    );
                }
                report.notes.sort();
                report.notes.dedup();
                match self.install_shader_pack(&compiled, &files) {
                    Ok(()) => Some(Applied {
                        target: target.clone(),
                        compiled,
                        files,
                    }),
                    Err(e) => {
                        // The renderer has no pack now: neither may the meshes.
                        if let Some(old) = self.uninstall_shader_pack() {
                            self.shader_packs.previous = Some(old);
                        }
                        if self.exclude_rejected(&target.file, &e, &compiled) {
                            // Compiled again without them on the next frame.
                            report.gpu_errors.push(("pipelines".into(), e));
                            self.shader_packs
                                .reports
                                .insert(target.file.clone(), report);
                            return;
                        }
                        report.status = PackStatus::Failed;
                        report.gpu_errors.push(("pipelines".into(), e));
                        None
                    }
                }
            }
            _ => None,
        };
        if installed.is_some()
            && let Some(r) = self.renderer.as_ref()
        {
            report.notes.extend(r.shader_pack_notes());
        }
        log::info!(
            "Horizon: shader pack {} ({}): {}\n{}",
            target.file,
            target.dimension,
            report.status.label(),
            report.text(&target.file)
        );
        self.announce_shader_pack(&target.file, &report);
        self.shader_packs
            .reports
            .insert(target.file.clone(), report);
        match installed {
            Some(applied) => {
                let old = self.shader_packs.applied.replace(applied);
                if old.is_some() {
                    self.shader_packs.previous = old;
                }
                self.shader_packs.failed = None;
            }
            None => {
                self.shader_packs.failed = Some(target);
                self.uninstall_shader_pack();
            }
        }
    }

    /// Programs a GPU error names are left out of the next compile (their
    /// slots fall back); false when it names none that are not already out.
    fn exclude_rejected(&mut self, file: &str, error: &str, compiled: &CompiledPack) -> bool {
        let named = programs_named(error, compiled);
        let excluded = self
            .shader_packs
            .excluded
            .entry(file.to_string())
            .or_default();
        let before = excluded.len();
        excluded.extend(named);
        let more = excluded.len() > before;
        if more {
            log::warn!(
                "Horizon: shader pack {file}: the GPU rejected {excluded:?}; compiling without them: {error}"
            );
            self.shader_packs.failed = None;
        }
        more
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
        // The pack's `minecraft:textures/…` textures come from the game's own assets.
        let root = self.options.resources.clone();
        let resources = move |name: &str| {
            let (namespace, path) = name.split_once(':').unwrap_or(("minecraft", name));
            std::fs::read(root.join("assets").join(namespace).join(path)).ok()
        };
        renderer.set_shader_pack(Some((&compiled_ref, files)), &resources)?;
        self.set_shader_pack_tags(Some(compiled));
        Ok(())
    }

    /// Back to Horizon's own shaders; the pack that was applied.
    fn uninstall_shader_pack(&mut self) -> Option<Applied> {
        if let Some(renderer) = self.renderer.as_mut() {
            let _ = renderer.set_shader_pack(None, &|_| None);
        }
        let old = self.shader_packs.applied.take();
        if old.is_some() {
            self.set_shader_pack_tags(None);
        }
        self.shader_packs.end_trial();
        old
    }

    /// A renderer rebuilt by a resource reload starts without the pack.
    fn keep_shader_pack_installed(&mut self) {
        let Some(applied) = &self.shader_packs.applied else {
            return;
        };
        let missing = self.renderer.as_ref().is_some_and(|r| !r.has_shader_pack());
        if missing {
            let (compiled, files) = (applied.compiled.clone(), applied.files.clone());
            if let Err(e) = self.install_shader_pack(&compiled, &files) {
                log::warn!("Horizon: shader pack could not be reinstalled: {e}");
                // Its tags go too, so the meshes are Horizon's own again.
                self.uninstall_shader_pack();
            }
        }
    }

    /// The mesher's per-block data for the pack (`None`: Horizon's own):
    /// meshes carry the pack's `PackVertex` stream exactly while this is set,
    /// so a change remeshes every section (as a resource reload does).
    fn set_shader_pack_tags(&mut self, compiled: Option<&Arc<CompiledPack>>) {
        let tags = compiled.map(|c| {
            Arc::new(horizon_client::mesher::PackTags::new(
                self.pack.assets.registry,
                &c.blocks,
                &c.plan,
            ))
        });
        let stream = self
            .renderer
            .as_ref()
            .is_none_or(|r| r.pack_vertex_stream() == tags.is_some());
        if stream && self.pack.context.pack == tags && self.streaming.context().pack == tags {
            return;
        }
        // New streamers and distant terrain take the pack resources' context.
        self.pack.context = self.pack.context.with_pack(tags.clone());
        self.clear_lod();
        self.streaming
            .reload_resources(&mut self.world, self.pack.context.clone());
        self.pending_uploads.clear();
        if let Some(renderer) = &mut self.renderer {
            renderer.set_pack_vertex_stream(tags.is_some());
        }
    }

    /// Whether the Shader Packs page shows: dropped files are taken only there.
    pub(super) fn shader_pack_page_open(&self) -> bool {
        self.screen == super::Screen::Options
            && self.ui.options.page() == horizon_client::options_ui::Page::ShaderPacks
    }

    /// A file dropped on the window: a zip goes into the packs folder.
    pub(super) fn shader_pack_dropped(&mut self, path: &Path) {
        self.shader_packs.load();
        let line = match self.shader_packs.add_dropped(path) {
            Ok(name) => format!("Added shader pack {name}."),
            Err(e) => format!("Not added: {e}"),
        };
        self.shader_packs.notice = Some((line.clone(), web_time::Instant::now()));
        self.system_message(horizon_client::text::Text::literal(line));
    }

    /// A chat line when a pack finishes loading.
    fn announce_shader_pack(&mut self, file: &str, report: &PackReport) {
        let text = match report.status {
            PackStatus::Works => format!("Shader pack {file} loaded."),
            PackStatus::Partial => {
                format!(
                    "Shader pack {file} loaded with parts missing (Options > Horizon Settings > Shader Packs > Report)."
                )
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
            trial: false,
        };
        assert_eq!(Selection::parse(&s.serialize()), s);
        assert!(!s.serialize().contains("trial"), "the old two-line file");
        let t = Selection { trial: true, ..s };
        assert_eq!(Selection::parse(&t.serialize()), t);
        assert_eq!(Selection::parse("garbage\n"), Selection::default());
    }

    #[test]
    fn pack_names_never_leave_the_folder() {
        for bad in ["../options", "a/b.zip", "a\\b.zip", "..", "x..zip", ""] {
            assert!(!is_pack_name(bad), "{bad}");
            let s = Selection::parse(&format!("enabled:true\npack:{bad}\n"));
            assert_eq!(s.pack, None, "{bad}");
        }
        assert!(is_pack_name("BSL v10 [x] + §.zip"));
        // Option files only for packs of the library.
        let mut sp = ShaderPacks::default();
        sp.save_values("options", vec![("A".into(), "1".into())]);
        assert!(sp.values.is_empty());
        assert!(sp.values_of("options").is_empty());
    }

    #[test]
    fn forgetting_a_pack_drops_what_was_read_from_it() {
        let target = Target {
            file: "A.zip".into(),
            path: "shaderpacks/A.zip".into(),
            stamp: 1,
            dimension: "minecraft:overworld",
            overrides: Vec::new(),
            excluded: Default::default(),
        };
        let mut sp = ShaderPacks::default();
        sp.models.insert("A.zip".into(), None);
        sp.models.insert("B.zip".into(), None);
        sp.reports.insert("A.zip".into(), PackReport::default());
        sp.excluded.insert("A.zip".into(), ["x".to_string()].into());
        sp.failed = Some(target);
        sp.forget("A.zip");
        assert!(!sp.models.contains_key("A.zip") && sp.models.contains_key("B.zip"));
        assert!(sp.reports.is_empty() && sp.excluded.is_empty() && sp.failed.is_none());
    }

    #[test]
    fn a_pack_on_trial_is_not_turned_on_again_at_start() {
        let s = Selection::parse("enabled:true\npack:X.zip\ntrial:true\n");
        assert!(!s.starts_on());
        assert!(Selection::parse("enabled:true\npack:X.zip\n").starts_on());
    }

    #[test]
    fn the_trial_ends_after_drawing_for_a_while() {
        let mut sp = ShaderPacks::default();
        sp.selection = Selection {
            enabled: true,
            pack: Some("X.zip".into()),
            trial: false,
        };
        sp.active = sp.selection.clone();
        let t0 = web_time::Instant::now();
        // Installed, not drawing yet: on trial (saved), the clock not started.
        assert!(sp.trial_step(false, t0));
        assert!(sp.selection.trial && sp.trial_drawn.is_none());
        assert!(!sp.trial_step(true, t0), "already saved");
        assert!(!sp.trial_step(true, t0 + PROVEN_AFTER / 2));
        assert!(
            sp.trial_step(true, t0 + PROVEN_AFTER),
            "proven: the flag is cleared"
        );
        assert!(sp.proven && !sp.selection.trial && !sp.active.trial);
        assert!(
            !sp.trial_step(true, t0 + PROVEN_AFTER * 2),
            "nothing more to save"
        );
        // A `--shader-pack` run is never on trial.
        let mut cli = ShaderPacks {
            cli: Some(LibraryPack {
                file: "X.zip".into(),
                path: "X.zip".into(),
                stamp: 0,
            }),
            ..ShaderPacks::default()
        };
        assert!(!cli.trial_step(false, t0));
    }

    #[test]
    fn option_values_round_trip() {
        let values = vec![
            ("SHADOWS".to_string(), "false".to_string()),
            ("sunPathRotation".to_string(), "-30.0".to_string()),
        ];
        assert_eq!(parse_values(&serialize_values(&values)), values);
        assert_eq!(
            parse_values("# comment\n\nA = 1\n"),
            vec![("A".into(), "1".into())]
        );
    }

    /// A zip of `(name, bytes)` entries, deflated.
    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write as _;
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            out.start_file(*name, options).unwrap();
            out.write_all(bytes).unwrap();
        }
        out.finish().unwrap().into_inner()
    }

    fn temp_zip(tag: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "horizon-shader-pack-{tag}-{}.zip",
            std::process::id()
        ));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn only_the_shaders_folder_is_read() {
        let path = temp_zip(
            "filter",
            &zip_of(&[
                ("Pack/shaders/final.fsh", b"void main() {}"),
                ("Pack/readme.txt", b"hello"),
                ("__MACOSX/Pack/shaders/._final.fsh", b"junk"),
                ("Pack/extras/huge.bin", &[0; 4096]),
            ]),
        );
        let files = read_files(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(files.0.keys().collect::<Vec<_>>(), ["shaders/final.fsh"]);
    }

    #[test]
    fn a_pack_past_the_limits_is_refused_with_a_reason() {
        // A small zip that inflates far past what it is: refused by its declared size.
        let zeros = vec![0u8; 4 << 20];
        let path = temp_zip(
            "bomb",
            &zip_of(&[("shaders/a.fsh", &zeros), ("shaders/b.fsh", &zeros)]),
        );
        assert!(std::fs::metadata(&path).unwrap().len() < 64 << 10);
        let tight = ReadLimits {
            files: 100,
            bytes: 6 << 20,
        };
        let err = read_files_within(&path, tight).unwrap_err();
        assert!(err.contains("more than 6 MiB"), "{err}");
        let few = ReadLimits {
            files: 1,
            bytes: 1 << 30,
        };
        let err = read_files_within(&path, few).unwrap_err();
        let _ = std::fs::remove_file(&path);
        assert!(err.contains("more than 1 files"), "{err}");
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

// The Shader Packs pages (`horizon_client::shader_packs_ui`).
impl Game<'_> {
    /// What the pages show; set into `ui.options.shader_packs` every frame while one shows
    /// (`poll_control`, game/control.rs).
    pub(crate) fn shader_packs_view(&mut self) -> horizon_client::shader_packs_ui::ShaderPacksView {
        use horizon_client::shader_packs_ui::{PackRow, SettingsView, ShaderPacksView};
        let sp = &mut self.shader_packs;
        sp.load();
        let selected = match &sp.cli {
            Some(c) => Some(c.file.clone()),
            None => sp.selection.pack.clone(),
        };
        let packs = sp
            .library
            .iter()
            .chain(sp.cli.iter())
            .map(|p| PackRow {
                file: p.file.clone(),
                title: p.file.strip_suffix(".zip").unwrap_or(&p.file).to_string(),
                status: sp.reports.get(&p.file).map(|r| r.status),
            })
            .collect();
        let report = selected
            .as_ref()
            .and_then(|f| sp.reports.get(f).map(|r| r.text(f)))
            .map(|t| t.lines().map(str::to_string).collect())
            .unwrap_or_default();
        let settings = match selected.as_ref() {
            Some(f) => {
                let model = sp.model_of(f);
                model.map(|model| SettingsView {
                    model,
                    values: sp.values_of(f).clone(),
                    screen: sp.settings_screen.clone(),
                })
            }
            None => None,
        };
        ShaderPacksView {
            enabled: sp.cli.is_some() || sp.selection.enabled,
            packs,
            selected,
            applied: sp.applied.as_ref().map(|a| a.target.file.clone()),
            progress: sp.progress(),
            web: cfg!(target_arch = "wasm32"),
            report,
            settings,
            notice: sp
                .notice
                .as_ref()
                .filter(|(_, at)| at.elapsed() < NOTICE_SHOWN)
                .map(|(line, _)| line.clone()),
        }
    }

    /// A Shader Packs page event (`options_ui::Event::ShaderPacks`), handled while the click
    /// that caused it is (the browser's file picker needs that).
    pub(crate) fn shader_pack_event(
        &mut self,
        ev: horizon_client::shader_packs_ui::ShaderPackEvent,
    ) {
        use horizon_client::shader_packs_ui::ShaderPackEvent as E;
        self.shader_packs.load();
        let sp = &mut self.shader_packs;
        match ev {
            E::SetEnabled(on) => {
                sp.cli = None;
                sp.selection.enabled = on;
            }
            E::Select(file) => {
                sp.cli = None;
                if sp.selection.pack.as_deref() != Some(file.as_str()) {
                    sp.settings_screen = None;
                }
                sp.selection.pack = Some(file);
            }
            E::Apply => sp.apply(),
            E::OpenFolder => {
                let dir = ShaderPacks::folder();
                let _ = fs::create_dir_all(&dir);
                super::packs::open_folder(&dir);
            }
            E::AddPack => {
                if !call_page("horizonPickShaderPack", None) {
                    sp.rescan();
                }
            }
            E::Delete(file) => sp.delete(&file),
            E::CopyReport => {
                let text = sp
                    .selection
                    .pack
                    .as_ref()
                    .and_then(|f| sp.reports.get(f).map(|r| r.text(f)))
                    .unwrap_or_default();
                if !call_page("horizonCopyText", Some(&text)) {
                    use horizon_client::clipboard::Clipboard as _;
                    self.clipboard.set(&text);
                }
            }
            E::OpenScreen(screen) => sp.settings_screen = screen,
            E::SetOption { name, value } => {
                if let Some(file) = sp.selection.pack.clone()
                    && let Some(model) = sp.model_of(&file)
                {
                    let mut values = model.values_from(sp.values_of(&file));
                    values.insert(name, value);
                    let changed = model.changed(&values);
                    sp.save_values(&file, changed);
                }
            }
            E::ApplyProfile(profile) => {
                if let Some(file) = sp.selection.pack.clone()
                    && let Some(model) = sp.model_of(&file)
                {
                    let values = model.values_from(sp.values_of(&file));
                    let values =
                        horizon_glsl_pack::screens::apply_profile(&model, &profile, &values);
                    let changed = model.changed(&values);
                    sp.save_values(&file, changed);
                }
            }
            E::ResetOptions => {
                if let Some(file) = sp.selection.pack.clone() {
                    sp.save_values(&file, Vec::new());
                }
            }
        }
    }
}
