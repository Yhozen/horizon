//! The browser build's entry (WEB.md): the page fetches the prepared resources
//! as one archive and calls [`start`] with it and the game's arguments. The archive
//! goes into the in-memory file system, the WebGPU device opens, and the game starts
//! as `game::run` does on the desktop, its event loop driven by the page's frames.
use wasm_bindgen::prelude::*;

/// Panics and the log go to the browser console.
pub fn install_hooks() {
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            console_error_panic_hook::hook(info);
            horizon_client::gfx::web_gpu::report_error(&format!("Horizon stopped: {info}"));
        }));
    });
    if log::set_logger(&CONSOLE).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

struct Console;
static CONSOLE: Console = Console;
impl log::Log for Console {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        // wgpu and naga describe every pipeline at info.
        let level =
            if metadata.target().starts_with("wgpu") || metadata.target().starts_with("naga") {
                log::Level::Warn
            } else {
                log::Level::Info
            };
        metadata.level() <= level
    }
    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "[{}] {}: {}",
            record.level(),
            record.target(),
            record.args()
        );
        let line = JsValue::from_str(&line);
        match record.level() {
            log::Level::Error => web_sys::console::error_1(&line),
            log::Level::Warn => web_sys::console::warn_1(&line),
            _ => web_sys::console::log_1(&line),
        }
    }
    fn flush(&self) {}
}

/// The records of a web archive (tools/web/pack.py): path and data.
fn archive_entries(archive: &[u8]) -> Result<Vec<(&str, &[u8])>, JsValue> {
    let bad = || JsValue::from_str("resources: truncated web archive");
    let mut at = 0usize;
    let mut out = Vec::new();
    let u32_at = |at: &mut usize| -> Result<usize, JsValue> {
        let b = archive.get(*at..*at + 4).ok_or_else(bad)?;
        *at += 4;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    while at < archive.len() {
        let n = u32_at(&mut at)?;
        let name =
            std::str::from_utf8(archive.get(at..at + n).ok_or_else(bad)?).map_err(|_| bad())?;
        at += n;
        let len = u32_at(&mut at)?;
        out.push((name, archive.get(at..at + len).ok_or_else(bad)?));
        at += len;
    }
    Ok(out)
}

/// Lays `art`'s look over the resources mounted at `root`: every texture the
/// mounted bundle also has (with `art`'s animation metadata, or none), and
/// `art`'s English text merged over the bundle's. Only names the bundle already
/// uses are taken, so its data, models and network numbering stay its own; the
/// files replaced are listed in `OVERLAY` for the bundle's integrity check.
/// The open bundle's adult mobs use Minecraft's box layout (crates/horizon-open-content's
/// `mobs`), so another bundle's skins fit them and are laid over. Its babies are the
/// adults' models scaled down, which a Minecraft baby skin (a layout of its own)
/// wouldn't fit: those stay the bundle's own.
fn is_own_layout(name: &str) -> bool {
    name.starts_with("assets/minecraft/textures/entity/") && name.ends_with("_baby.png")
}

pub fn overlay_art(root: &std::path::Path, art: &[u8]) -> Result<usize, JsValue> {
    use horizon_core::fs::PathExt as _;
    let entries = archive_entries(art)?;
    let names: std::collections::HashSet<&str> = entries.iter().map(|(n, _)| *n).collect();
    let mut replaced = Vec::new();
    for (name, data) in &entries {
        let target = root.join(name);
        if name.starts_with("assets/minecraft/textures/") && name.ends_with(".png") {
            if !target.is_file_vfs() {
                continue;
            }
            let stem = name
                .trim_start_matches("assets/minecraft/textures/block/")
                .trim_end_matches(".png");
            if stem.starts_with("water_") {
                // Water's colour is the biome's in both; keep the bundle's own.
                continue;
            }
            if is_own_layout(name) {
                continue;
            }
            // Grey grass and foliage textures stay grey: the bundle's models tint
            // them, with the colormaps this archive also lays over.
            horizon_core::fs::install(&target, data.to_vec());
            replaced.push(name.to_string());
            let meta = format!("{name}.mcmeta");
            if !names.contains(meta.as_str()) && root.join(&meta).is_file_vfs() {
                let _ = horizon_core::fs::remove_file(root.join(&meta));
                replaced.push(meta);
            }
        } else if let Some(png) = name.strip_suffix(".mcmeta") {
            if png.starts_with("assets/minecraft/textures/")
                && !is_own_layout(png)
                && root.join(png).is_file_vfs()
            {
                horizon_core::fs::install(&target, data.to_vec());
                replaced.push(name.to_string());
            }
        } else if *name == "assets/minecraft/lang/en_us.json" && target.is_file_vfs() {
            let mut ours: serde_json::Map<String, serde_json::Value> =
                serde_json::from_slice(&horizon_core::fs::read(&target).unwrap_or_default())
                    .unwrap_or_default();
            if let Ok(serde_json::Value::Object(theirs)) = serde_json::from_slice(data) {
                ours.extend(theirs);
                horizon_core::fs::install(&target, serde_json::to_vec(&ours).unwrap_or_default());
                replaced.push(name.to_string());
            }
        }
    }
    let count = replaced.len();
    horizon_core::fs::install(
        &root.join(OVERLAY),
        serde_json::to_vec(&replaced).unwrap_or_default(),
    );
    Ok(count)
}

/// The list of files `overlay_art` replaced, beside the resources.
pub const OVERLAY: &str = "horizon-overlay.json";

/// Like `start`, with `archive` an open bundle (Horizon Open Creative) wearing the
/// look of `art`, a Minecraft-backed archive (`overlay_art`): a player of the private
/// preview on a shared open server, with the same numbering as the open players.
#[wasm_bindgen]
pub async fn start_overlaid(
    archive: Vec<u8>,
    art: Vec<u8>,
    args: Vec<String>,
) -> Result<(), JsValue> {
    install_hooks();
    let root = std::path::Path::new(".local/horizon-client").join(horizon_core::VANILLA_VERSION);
    let files = horizon_core::fs::mount_archive(&root, &archive)
        .map_err(|e| JsValue::from_str(&format!("resources: {e}")))?;
    drop(archive);
    let replaced = overlay_art(&root, &art)?;
    drop(art);
    log::info!(
        "Horizon: {files} resource files mounted at {}, {replaced} wearing the given art",
        root.display()
    );
    crate::game::run_web(args).await
}

/// Puts one file into the game's file system, before `start` or while the game
/// runs: a relative `path` starts from the game folder (`/horizon`), as the
/// game's own do. The page restores the files it keeps (shader packs, their
/// settings) this way, and adds the packs the player picks or drops.
#[wasm_bindgen]
pub fn install_file(path: String, bytes: Vec<u8>) {
    horizon_core::fs::install(std::path::Path::new(&path), bytes);
}

/// The page added shader packs: the Shader Packs screen lists them again.
#[wasm_bindgen]
pub fn shader_packs_changed() {
    crate::game::shader_packs_changed();
}

/// A line for the player from the page (a shader pack it could not store or load).
#[wasm_bindgen]
pub fn shader_pack_notice(line: String) {
    crate::game::shader_pack_notice(line);
}

/// Whether the Shader Packs page shows: the page takes dropped files only then.
#[wasm_bindgen]
pub fn shader_pack_page_open() -> bool {
    crate::game::shader_pack_page_open()
}

/// Mounts `archive` (tools/web/pack.py) as the prepared resources, opens WebGPU and
/// starts the game with `args`, the desktop game's command line.
#[wasm_bindgen]
pub async fn start(archive: Vec<u8>, args: Vec<String>) -> Result<(), JsValue> {
    install_hooks();
    let root = std::path::Path::new(".local/horizon-client").join(horizon_core::VANILLA_VERSION);
    let files = horizon_core::fs::mount_archive(&root, &archive)
        .map_err(|e| JsValue::from_str(&format!("resources: {e}")))?;
    drop(archive);
    log::info!(
        "Horizon: {files} resource files mounted at {}",
        root.display()
    );
    crate::game::run_web(args).await
}
