//! The browser build's entry (WEB.md): the page fetches the prepared resources
//! as one archive and calls [`start`] with it and the game's arguments. The archive
//! goes into the in-memory file system, the WebGPU device opens, and the game starts
//! as `game::run` does on the desktop, its event loop driven by the page's frames.
use wasm_bindgen::prelude::*;

/// Panics and the log go to the browser console.
pub fn install_hooks() {
    console_error_panic_hook::set_once();
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
    horizon_client::gfx::open_web_gpu()
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    crate::game::set_web_args(args);
    crate::game::run().map_err(|e| JsValue::from_str(&format!("Horizon: {e}")))
}
