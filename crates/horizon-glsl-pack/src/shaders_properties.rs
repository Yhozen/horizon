//! `shaders/shaders.properties`: the pack-wide settings, read after its own
//! `#if` lines with the options' macros. [`ShadersProperties::entries`] keeps
//! every live key and value, so each part of the crate can read its own keys.

use std::collections::{BTreeMap, BTreeSet};

use crate::directives::{buffer_index, canonical_sampler, is_ident};
use crate::plan::{
    AlphaTest, Blend, BufferSize, Clouds, CustomTexture, PackPlan, PackProperties,
    ParticleOrdering, ProgramState, Scale, ShadowCulling, TextureStage,
};
use crate::{PackError, PackFiles, preprocess, properties};

/// What `shaders/shaders.properties` says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShadersProperties {
    pub properties: PackProperties,
    pub textures: Vec<CustomTexture>,
    /// `program.<name>.enabled=false`: pack paths (`shaders/world0/gbuffers_clouds`).
    pub disabled: BTreeSet<String>,
    /// `program.<name>.enabled=<expression>`, evaluated, by the key's name
    /// (`world0/gbuffers_clouds`, `composite3`).
    pub program_enabled: BTreeMap<String, bool>,
    /// `blend.*`, `alphaTest.*`, `scale.*`, `flip.*`, by program name.
    pub states: BTreeMap<String, ProgramState>,
    /// `size.buffer.<buffer>`, by colortex index.
    pub sizes: BTreeMap<u8, BufferSize>,
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
    let text = live_lines(&text, PATH, files, macros, false)?;
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
            "vignette" => set(&mut p.vignette),
            "sky" => set(&mut p.sky),
            "stars" => set(&mut p.stars),
            "weather" => {
                let mut words = value.split_whitespace().map(property_bool);
                if let Some(Some(b)) = words.next() {
                    p.weather = b;
                }
                if let Some(Some(b)) = words.next() {
                    p.weather_particles = b;
                }
            }
            "shadow.enabled" => p.shadow_enabled = property_bool(&value).or(p.shadow_enabled),
            "shadowTerrain" => set(&mut p.shadow_terrain),
            "shadowTranslucent" => set(&mut p.shadow_translucent),
            "shadowEntities" => set(&mut p.shadow_entities),
            "shadowBlockEntities" => set(&mut p.shadow_block_entities),
            "shadowPlayer" => set(&mut p.shadow_player),
            "shadow.culling" => {
                p.shadow_culling = match value.trim() {
                    "reversed" => ShadowCulling::Reversed,
                    v => match property_bool(v) {
                        Some(true) => ShadowCulling::On,
                        Some(false) => ShadowCulling::Off,
                        None => p.shadow_culling,
                    },
                }
            }
            "frustum.culling" => set(&mut p.frustum_culling),
            "rain.depth" => set(&mut p.rain_depth),
            "beacon.beam.depth" => set(&mut p.beacon_beam_depth),
            "particles.ordering" => {
                p.particles = match value.trim() {
                    "mixed" => Some(ParticleOrdering::Mixed),
                    "after" => Some(ParticleOrdering::After),
                    "before" => Some(ParticleOrdering::Before),
                    _ => p.particles,
                }
            }
            "particles.before.deferred" => match property_bool(&value) {
                Some(true) => p.particles = Some(ParticleOrdering::Before),
                Some(false) if p.particles == Some(ParticleOrdering::Before) => p.particles = None,
                _ => {}
            },
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
                {
                    let on = enabled(&value, files, macros);
                    out.program_enabled.insert(name.to_string(), on);
                    if !on {
                        out.disabled.insert(format!("shaders/{name}"));
                    }
                } else {
                    pass_state(&mut out.states, &mut out.sizes, &key, &value);
                }
            }
        }
    }
    Ok(out)
}

/// The lines of a text that its `#if`s keep, unexpanded (a `.properties`
/// value such as `!FANCY` must keep its option name, a `/* DRAWBUFFERS */`
/// comment its text): each logical line goes through the preprocessor as a
/// marker identifier and comes back as itself. With `glsl`, lines that start
/// inside a block comment are text (a `#define` there is no directive), and
/// a directive's trailing comment is dropped before it is evaluated.
pub(crate) fn live_lines(
    text: &str,
    file: &str,
    files: &PackFiles,
    macros: &BTreeMap<String, String>,
    glsl: bool,
) -> Result<String, PackError> {
    // Conditionals act on physical lines: a `\` before an `#else` must not
    // pull the directive into the line before it.
    let lines: Vec<&str> = text.lines().collect();
    let mut in_comment = false;
    let mut marked = String::new();
    for (i, l) in lines.iter().enumerate() {
        let directive = !in_comment && l.trim_start().starts_with('#');
        if glsl {
            in_comment = ends_in_comment(l, in_comment);
        }
        if directive {
            marked.push_str(if glsl { strip_comments(l) } else { l });
        } else {
            marked.push_str(&format!("hz_line_{i}"));
        }
        marked.push('\n');
    }
    let kept = preprocess::preprocess_text(&marked, file, files, macros, &[])?;
    // Then the live lines' continuations are joined.
    let mut out = String::new();
    let mut open = false;
    for i in kept
        .lines()
        .filter_map(|m| m.trim().strip_prefix("hz_line_")?.parse::<usize>().ok())
    {
        let line = if open {
            lines[i].trim_start()
        } else {
            lines[i]
        };
        match line.strip_suffix('\\') {
            Some(head) => {
                out.push_str(head);
                open = true;
            }
            None => {
                out.push_str(line);
                out.push('\n');
                open = false;
            }
        }
    }
    Ok(out)
}

/// Whether a block comment is open at the end of `line`, given whether one
/// was at its start (`//` outside a block comment ends the scan).
fn ends_in_comment(line: &str, mut open: bool) -> bool {
    let b = line.as_bytes();
    let mut i = 0;
    while i + 1 < b.len() {
        match (open, b[i], b[i + 1]) {
            (true, b'*', b'/') => {
                open = false;
                i += 2;
            }
            (false, b'/', b'*') => {
                open = true;
                i += 2;
            }
            (false, b'/', b'/') => return false,
            _ => i += 1,
        }
    }
    open
}

/// A directive line without its `//` or `/*` tail.
fn strip_comments(line: &str) -> &str {
    let cut = [line.find("//"), line.find("/*")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(line.len());
    &line[..cut]
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

/// `program.X.enabled` values: a boolean expression of option names
/// (`A && !B || (C && D)`, `true`, `false`, comparisons of value options),
/// evaluated by the preprocessor's `#if`: an option that is on but has no
/// value counts as 1, one that is off as 0. An expression that does not
/// evaluate counts as enabled.
fn enabled(value: &str, files: &PackFiles, macros: &BTreeMap<String, String>) -> bool {
    let v = value.trim();
    if let Some(b) = property_bool(v) {
        return b;
    }
    let mut expr = String::new();
    let mut word = String::new();
    let mut after_defined = false;
    let mut flush = |word: &mut String, expr: &mut String| {
        if word.is_empty() {
            return;
        }
        let w = std::mem::take(word);
        let keep = w.starts_with(|c: char| c.is_ascii_digit()) || w == "defined" || after_defined;
        after_defined = w == "defined";
        if keep {
            expr.push_str(&w);
        } else {
            expr.push_str(match (w.as_str(), macros.get(&w)) {
                ("true", _) => "1",
                ("false", _) => "0",
                (_, Some(body)) if !body.trim().is_empty() => &w,
                (_, Some(_)) => "1",
                (_, None) => "0",
            });
        }
    };
    for c in v.chars() {
        if is_ident(c) || (c == '.' && word.starts_with(|c: char| c.is_ascii_digit())) {
            word.push(c);
        } else {
            flush(&mut word, &mut expr);
            expr.push(c);
        }
    }
    flush(&mut word, &mut expr);
    let text = format!("#if {expr}\nhz_enabled\n#endif\n");
    match preprocess::preprocess_text(&text, "shaders/shaders.properties", files, macros, &[]) {
        Ok(out) => out.contains("hz_enabled"),
        Err(_) => true,
    }
}

impl ShadersProperties {
    /// Whether `program.<name>.enabled` turns off the program at pack path
    /// `path` (`shaders/world0/composite`): its own key, else (in a world
    /// folder) the key without the folder.
    pub fn program_disabled(&self, path: &str) -> bool {
        let rel = path.strip_prefix("shaders/").unwrap_or(path);
        if let Some(&on) = self.program_enabled.get(rel) {
            return !on;
        }
        match rel.rsplit_once('/') {
            Some((_, name)) => self.program_enabled.get(name) == Some(&false),
            None => false,
        }
    }
}

/// `blend.*`, `alphaTest.*`, `scale.*`, `flip.*`, `size.buffer.*`.
fn pass_state(
    states: &mut BTreeMap<String, ProgramState>,
    sizes: &mut BTreeMap<u8, BufferSize>,
    key: &str,
    value: &str,
) {
    let colortex = |name: &str| buffer_index(name.trim());
    if let Some(buffer) = key.strip_prefix("size.buffer.") {
        let Some(i) = colortex(buffer) else {
            return;
        };
        let words: Vec<&str> = value.split_whitespace().collect();
        let [w, h] = words[..] else {
            return;
        };
        let size = match (w.parse::<u32>(), h.parse::<u32>()) {
            (Ok(w), Ok(h)) => BufferSize::Pixels(w, h),
            _ => match (w.parse::<f32>(), h.parse::<f32>()) {
                (Ok(w), Ok(h)) if w > 0.0 && h > 0.0 => BufferSize::Relative(w, h),
                _ => return,
            },
        };
        sizes.insert(i, size);
        return;
    }
    let Some((what, rest)) = key.split_once('.') else {
        return;
    };
    let (program, buffer) = match rest.split_once('.') {
        Some((p, b)) => (p, Some(b)),
        None => (rest, None),
    };
    let state = states.entry(program.to_string()).or_default();
    match (what, buffer) {
        ("blend", None) => {
            if let Some(b) = Blend::parse(value) {
                state.blend = Some(b);
            }
        }
        ("blend", Some(buffer)) => {
            if let (Some(i), Some(b)) = (colortex(buffer), Blend::parse(value)) {
                state.buffer_blend.insert(i, b);
            }
        }
        ("alphaTest", None) => {
            if let Some(t) = AlphaTest::parse(value) {
                state.alpha_test = Some(t);
            }
        }
        ("scale", None) => {
            let n: Vec<f32> = value
                .split_whitespace()
                .filter_map(|w| w.parse().ok())
                .collect();
            let scale = match n[..] {
                [s] => Scale {
                    scale: s,
                    offset: [0.0; 2],
                },
                [s, x, y] => Scale {
                    scale: s,
                    offset: [x, y],
                },
                _ => return,
            };
            state.scale = Some(scale);
        }
        ("flip", Some(buffer)) => {
            if let (Some(i), Some(b)) = (colortex(buffer), property_bool(value)) {
                state.flip.insert(i, b);
            }
        }
        _ => {}
    }
}

/// Puts the per-program state and buffer sizes of `props` into `plan`.
pub fn apply_pass_state(plan: &mut PackPlan, props: &ShadersProperties) {
    plan.program_states = props.states.clone();
    for (&i, &size) in &props.sizes {
        if let Some(b) = plan.buffers.get_mut(usize::from(i)) {
            b.size = size;
        }
    }
}
