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
            let bytes = match biome_tint(stem) {
                // The bundle's models carry no tint: wear the texture as vanilla's
                // default biome colours would draw it.
                Some(color) => tinted(data, color).unwrap_or_else(|| data.to_vec()),
                None => data.to_vec(),
            };
            horizon_core::fs::install(&target, bytes);
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
/// The colour a grey, biome-tinted texture is drawn with in a plains-like biome
/// (grass and foliage colours), for block textures by name.
fn biome_tint(stem: &str) -> Option<u32> {
    const GRASS: u32 = 0x91BD59;
    const FOLIAGE: u32 = 0x77AB2F;
    match stem {
        "spruce_leaves" => Some(0x619961),
        "birch_leaves" => Some(0x80A755),
        "oak_leaves" | "jungle_leaves" | "acacia_leaves" | "dark_oak_leaves"
        | "mangrove_leaves" | "vine" => Some(FOLIAGE),
        "short_grass"
        | "fern"
        | "tall_grass_top"
        | "tall_grass_bottom"
        | "large_fern_top"
        | "large_fern_bottom"
        | "grass_block_top"
        | "grass_block_side_overlay"
        | "sugar_cane" => Some(GRASS),
        _ => None,
    }
}
/// `png` with every pixel's colour multiplied by `color` (0xRRGGBB).
fn tinted(png_bytes: &[u8], color: u32) -> Option<Vec<u8>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width, info.height);
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => buf[..info.buffer_size()]
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf[..info.buffer_size()]
            .chunks(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buf[..info.buffer_size()]
            .iter()
            .flat_map(|&g| [g, g, g, 255])
            .collect(),
        png::ColorType::Indexed => return None,
    };
    let tint = [(color >> 16) as u8, (color >> 8) as u8, color as u8];
    let out: Vec<u8> = rgba
        .chunks(4)
        .flat_map(|p| {
            let m = |c: u8, t: u8| ((u16::from(c) * u16::from(t)) / 255) as u8;
            [m(p[0], tint[0]), m(p[1], tint[1]), m(p[2], tint[2]), p[3]]
        })
        .collect();
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().ok()?.write_image_data(&out).ok()?;
    }
    Some(encoded)
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
    horizon_client::gfx::open_web_gpu()
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    crate::game::set_web_args(args);
    crate::game::run().map_err(|e| JsValue::from_str(&format!("Horizon: {e}")))
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
