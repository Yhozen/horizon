//! `shaders/shaders.properties`: the pack-wide settings, read after its own
//! `#if` lines with the options' macros. [`ShadersProperties::entries`] keeps
//! every live key and value, so each part of the crate can read its own keys.

use std::collections::{BTreeMap, BTreeSet};

use crate::directives::{canonical_sampler, is_ident};
use crate::plan::{Clouds, CustomTexture, PackProperties, TextureStage};
use crate::{PackError, PackFiles, preprocess, properties};

/// What `shaders/shaders.properties` says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShadersProperties {
    pub properties: PackProperties,
    pub textures: Vec<CustomTexture>,
    /// `program.<name>.enabled=false`: pack paths (`shaders/world0/gbuffers_clouds`).
    pub disabled: BTreeSet<String>,
    /// Every live `key=value`, in file order (continuation lines joined).
    pub entries: Vec<(String, String)>,
}

fn property_bool(value: &str) -> Option<bool> {
    match value.trim() {
        "true" | "on" => Some(true),
        "false" | "off" => Some(false),
        _ => None,
    }
}

/// Reads `shaders/shaders.properties`, preprocessed with `macros` (the
/// predefined macros and the options' values: the file may hold `#if`).
pub fn shaders_properties(
    files: &PackFiles,
    macros: &BTreeMap<String, String>,
) -> Result<ShadersProperties, PackError> {
    const PATH: &str = "shaders/shaders.properties";
    let mut out = ShadersProperties::default();
    let Some(text) = files.text(PATH) else {
        return Ok(out);
    };
    let text = live_lines(&text, PATH, files, macros)?;
    let p = &mut out.properties;
    out.entries = properties::parse_properties(&text);
    for (key, value) in out.entries.clone() {
        let set = |to: &mut bool| {
            if let Some(b) = property_bool(&value) {
                *to = b;
            }
        };
        match key.as_str() {
            "clouds" => {
                p.clouds = match value.as_str() {
                    "off" => Clouds::Off,
                    "fast" => Clouds::Fast,
                    "fancy" => Clouds::Fancy,
                    _ => Clouds::Default,
                }
            }
            "oldLighting" => set(&mut p.old_lighting),
            "underwaterOverlay" => set(&mut p.underwater_overlay),
            "sun" => set(&mut p.sun),
            "moon" => set(&mut p.moon),
            "separateAo" => set(&mut p.separate_ao),
            "oldHandLight" => set(&mut p.old_hand_light),
            "texture.noise" => {
                for stage in [
                    TextureStage::Gbuffers,
                    TextureStage::Shadow,
                    TextureStage::Deferred,
                    TextureStage::Composite,
                ] {
                    out.textures.push(texture(stage, "noisetex", &value));
                }
            }
            _ => {
                if let Some(rest) = key.strip_prefix("texture.") {
                    let Some((stage, sampler)) = rest.split_once('.') else {
                        continue;
                    };
                    let stage = match stage {
                        "gbuffers" => TextureStage::Gbuffers,
                        "shadow" => TextureStage::Shadow,
                        "deferred" => TextureStage::Deferred,
                        "composite" => TextureStage::Composite,
                        // prepare, shadowcomp, begin …: passes Horizon does not run.
                        _ => continue,
                    };
                    out.textures.push(texture(stage, sampler, &value));
                } else if let Some(name) = key
                    .strip_prefix("program.")
                    .and_then(|k| k.strip_suffix(".enabled"))
                    && !enabled(&value, macros)
                {
                    out.disabled.insert(format!("shaders/{name}"));
                }
            }
        }
    }
    Ok(out)
}

/// The lines of a `.properties` text that its `#if`s keep, unexpanded (a
/// value such as `!FANCY` must keep its option name): each logical line goes
/// through the preprocessor as a marker identifier and comes back as itself.
fn live_lines(
    text: &str,
    file: &str,
    files: &PackFiles,
    macros: &BTreeMap<String, String>,
) -> Result<String, PackError> {
    let mut lines = Vec::new();
    let mut raw = text.lines();
    while let Some(line) = raw.next() {
        let mut line = line.to_string();
        while line.ends_with('\\') {
            line.pop();
            line.push_str(raw.next().unwrap_or("").trim_start());
        }
        lines.push(line);
    }
    let marked: String = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if l.trim_start().starts_with('#') {
                format!("{l}\n")
            } else {
                format!("hz_line_{i}\n")
            }
        })
        .collect();
    let kept = preprocess::preprocess_text(&marked, file, files, macros, &[])?;
    Ok(kept
        .lines()
        .filter_map(|m| m.trim().strip_prefix("hz_line_")?.parse::<usize>().ok())
        .map(|i| format!("{}\n", lines[i]))
        .collect())
}

fn texture(stage: TextureStage, sampler: &str, value: &str) -> CustomTexture {
    // Raw textures carry a type and sizes after the path; the path comes first.
    let path = value.split_whitespace().next().unwrap_or("");
    let path = path.trim_start_matches('/');
    CustomTexture {
        stage,
        sampler: canonical_sampler(sampler),
        path: format!("shaders/{path}"),
    }
}

/// `program.X.enabled` values: `true`/`false`, or an option name with an
/// optional `!`. Anything else counts as enabled.
fn enabled(value: &str, macros: &BTreeMap<String, String>) -> bool {
    let v = value.trim();
    if let Some(b) = property_bool(v) {
        return b;
    }
    let (negate, name) = match v.strip_prefix('!') {
        Some(n) => (true, n.trim()),
        None => (false, v),
    };
    if !name.is_empty() && name.chars().all(is_ident) {
        return macros.contains_key(name) != negate;
    }
    true
}
