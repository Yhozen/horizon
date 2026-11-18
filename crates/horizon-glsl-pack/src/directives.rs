//! The renderer settings a pack declares in its sources and in
//! `shaders.properties`, read into a [`PackPlan`].
//!
//! Source directives are `const` declarations with known names
//! (`const int colortex2Format = RGBA16;`, `const bool gaux4Clear = false;`,
//! `const int shadowMapResolution = 2048;` …). They are read from the
//! preprocessed text (so `#if` and option values apply) and from block
//! comments of the raw text (packs hide format names there so GLSL still
//! compiles), plus the old comment forms `/* GAUX4FORMAT:RGBA32F */` and
//! `/* SHADOWRES:1024 */`.

use std::collections::{BTreeMap, BTreeSet};

use crate::plan::{BufferFormat, FragOutput, PackPlan};

/// The texture index of a colour buffer name: `colortex0..15` and the
/// legacy names `gcolor`, `gdepth`, `gnormal`, `composite`, `gaux1..4`.
pub fn buffer_index(name: &str) -> Option<u8> {
    if let Some(n) = name.strip_prefix("colortex") {
        return n.parse().ok().filter(|&n: &u8| n < 16);
    }
    Some(match name {
        "gcolor" => 0,
        "gdepth" => 1,
        "gnormal" => 2,
        "composite" => 3,
        "gaux1" => 4,
        "gaux2" => 5,
        "gaux3" => 6,
        "gaux4" => 7,
        _ => return None,
    })
}

/// A sampler's canonical name (`gaux1` → `colortex4`, `gdepthtex` →
/// `depthtex0`, `shadowcolor` → `shadowcolor0`, `tex`/`texture` → `gtexture`).
pub fn canonical_sampler(name: &str) -> String {
    if let Some(i) = buffer_index(name) {
        return format!("colortex{i}");
    }
    match name {
        "gdepthtex" => "depthtex0",
        "shadowcolor" => "shadowcolor0",
        "tex" | "texture" => "gtexture",
        "noise" => "noisetex",
        other => other,
    }
    .to_string()
}

/// `const TYPE NAME = VALUE;` declarations in `text`, in order, as
/// `(NAME, VALUE)` with the value trimmed.
pub fn consts(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for statement in text.split(';') {
        let mut rest = statement;
        // The last `const` word of the statement (after `{`, `}` or another statement).
        let Some(at) = rest
            .match_indices("const")
            .filter(|(i, _)| {
                let before = rest[..*i].chars().next_back();
                let after = rest[i + 5..].chars().next();
                before.is_none_or(|c| !is_ident(c)) && after.is_some_and(char::is_whitespace)
            })
            .map(|(i, _)| i)
            .last()
        else {
            continue;
        };
        rest = rest[at + 5..].trim_start();
        let mut words = Vec::new();
        while words.len() < 3 {
            let end = rest.find(|c: char| !is_ident(c)).unwrap_or(rest.len());
            if end == 0 {
                break;
            }
            words.push(&rest[..end]);
            rest = rest[end..].trim_start();
            if rest.starts_with('=') {
                break;
            }
        }
        // `const [precision] TYPE NAME =`.
        let Some(value) = rest.strip_prefix('=') else {
            continue;
        };
        let Some(&name) = words.last().filter(|_| words.len() >= 2) else {
            continue;
        };
        out.push((name.to_string(), value.trim().to_string()));
    }
    out
}

pub(crate) fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The bodies of the `/* … */` comments of `text` (`//` comments skipped).
pub fn block_comments(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut i = 0;
    let b = text.as_bytes();
    while i + 1 < b.len() {
        match (b[i], b[i + 1]) {
            (b'/', b'/') => i = text[i..].find('\n').map_or(b.len(), |n| i + n),
            (b'/', b'*') => {
                let start = i + 2;
                let end = text[start..].find("*/").map_or(b.len(), |n| start + n);
                out.push(&text[start..end]);
                i = end + 2;
            }
            _ => i += 1,
        }
    }
    out
}

fn number(value: &str) -> Option<f32> {
    let v = value
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();
    let v = v.strip_suffix(['f', 'F']).unwrap_or(v);
    v.parse().ok()
}

fn boolean(value: &str) -> Option<bool> {
    match value.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// `vec4(r, g, b, a)` or `vec4(v)`.
fn vec4(value: &str) -> Option<[f32; 4]> {
    let inner = value.trim().strip_prefix("vec4")?.trim_start();
    let inner = inner.strip_prefix('(')?.strip_suffix(')')?;
    let parts: Vec<f32> = inner.split(',').map(number).collect::<Option<_>>()?;
    match parts[..] {
        [v] => Some([v; 4]),
        [r, g, b, a] => Some([r, g, b, a]),
        _ => None,
    }
}

/// One program's sources, as directives read them.
pub struct ProgramText<'a> {
    /// Includes expanded, comments kept (vertex and fragment).
    pub raw: [&'a str; 2],
    /// Preprocessed (vertex and fragment).
    pub preprocessed: [&'a str; 2],
}

/// The directives of one text, comment forms first, as `(name, value)`.
fn directives_of(raw: &str, preprocessed: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for comment in block_comments(raw) {
        out.extend(consts(comment));
        // `GAUX4FORMAT:RGBA32F`, `SHADOWRES:1024`.
        for word in comment.split_whitespace() {
            let Some((key, value)) = word.split_once(':') else {
                continue;
            };
            let key = key.to_ascii_lowercase();
            if let Some(buffer) = key.strip_suffix("format") {
                if let Some(i) = buffer_index(buffer) {
                    out.push((format!("colortex{i}Format"), value.to_string()));
                }
            } else if key == "shadowres" {
                out.push(("shadowMapResolution".into(), value.to_string()));
            } else if key == "shadowhpl" {
                out.push(("shadowDistance".into(), value.to_string()));
            }
        }
    }
    out.extend(consts(preprocessed));
    out
}

/// Whether `text` declares `uniform … name;`.
fn declares_uniform(text: &str, name: &str) -> bool {
    text.split(';').any(|s| {
        let words: Vec<_> = s.split_whitespace().collect();
        words.contains(&"uniform") && words.last() == Some(&name)
    })
}

/// Fills the pack-wide settings of `plan` (buffers, shadow map, noise, AO,
/// half-lives) from every program's sources, in order; a later program's
/// directive overrides an earlier one's.
pub fn read_globals<'a>(plan: &mut PackPlan, programs: impl IntoIterator<Item = ProgramText<'a>>) {
    let mut format_given = [false; 16];
    let mut gdepth = false;
    for program in programs {
        for stage in 0..2 {
            let pre = program.preprocessed[stage];
            gdepth |= declares_uniform(pre, "gdepth");
            // Formats name identifiers a pack may also `#define`: when the
            // preprocessed value is not a format, the raw text's is.
            let raw_consts: BTreeMap<String, String> =
                consts(program.raw[stage]).into_iter().collect();
            for (name, value) in directives_of(program.raw[stage], pre) {
                let value = match raw_consts.get(&name) {
                    Some(raw)
                        if name.ends_with("Format") && BufferFormat::parse(&value).is_none() =>
                    {
                        raw.clone()
                    }
                    _ => value,
                };
                global(plan, &mut format_given, &name, &value);
            }
        }
    }
    // A pack reading `gdepth` gets a float colortex1 unless it names a format.
    if gdepth && !format_given[1] {
        plan.buffers[1].format = BufferFormat::Rgba32F;
    }
}

fn global(plan: &mut PackPlan, format_given: &mut [bool; 16], name: &str, value: &str) {
    let shadow = &mut plan.shadow;
    let both = |b: Option<bool>, to: &mut [bool; 2]| {
        if let Some(b) = b {
            *to = [b; 2];
        }
    };
    let one = |b: Option<bool>, to: &mut bool| {
        if let Some(b) = b {
            *to = b;
        }
    };
    let num = |to: &mut f32| {
        if let Some(v) = number(value) {
            *to = v;
        }
    };
    let b = boolean(value);
    match name {
        "shadowMapResolution" => {
            if let Some(v) = number(value).filter(|v| *v >= 1.0) {
                shadow.resolution = v as u32;
            }
        }
        "shadowDistance" => num(&mut shadow.distance),
        "shadowIntervalSize" => num(&mut shadow.interval),
        "shadowDistanceRenderMul" => num(&mut shadow.distance_render_mul),
        "sunPathRotation" => num(&mut shadow.sun_path_rotation),
        "shadowHardwareFiltering" => both(b, &mut shadow.hardware_filtering),
        "shadowHardwareFiltering0" => one(b, &mut shadow.hardware_filtering[0]),
        "shadowHardwareFiltering1" => one(b, &mut shadow.hardware_filtering[1]),
        "generateShadowMipmap" => both(b, &mut shadow.tex_mipmap),
        "shadowtexMipmap" | "shadowtex0Mipmap" => one(b, &mut shadow.tex_mipmap[0]),
        "shadowtex1Mipmap" => one(b, &mut shadow.tex_mipmap[1]),
        "shadowtexNearest" | "shadowtex0Nearest" | "shadow0MinMagNearest" => {
            one(b, &mut shadow.tex_nearest[0])
        }
        "shadowtex1Nearest" | "shadow1MinMagNearest" => one(b, &mut shadow.tex_nearest[1]),
        "generateShadowColorMipmap" => both(b, &mut shadow.color_mipmap),
        "shadowcolor0Mipmap" => one(b, &mut shadow.color_mipmap[0]),
        "shadowcolor1Mipmap" => one(b, &mut shadow.color_mipmap[1]),
        "noiseTextureResolution" => {
            if let Some(v) = number(value).filter(|v| *v >= 1.0) {
                plan.noise_resolution = v as u32;
            }
        }
        "ambientOcclusionLevel" => {
            if let Some(v) = number(value) {
                plan.ambient_occlusion_level = v.clamp(0.0, 1.0);
            }
        }
        // The format's quirk (format-notes §9): `drynessHalflife` sets the
        // rising (wetness) half-life too, and the falling one stays 200.
        "wetnessHalflife" | "drynessHalflife" => num(&mut plan.wetness_half_life),
        "eyeBrightnessHalflife" => num(&mut plan.eye_brightness_half_life),
        _ => buffer_directive(plan, format_given, name, value),
    }
}

/// `<buffer>Format`, `<buffer>Clear`, `<buffer>ClearColor` for colortex
/// buffers and shadowcolor0/1.
fn buffer_directive(plan: &mut PackPlan, format_given: &mut [bool; 16], name: &str, value: &str) {
    let (buffer, what) = ["Format", "ClearColor", "Clear"]
        .iter()
        .find_map(|s| Some((name.strip_suffix(s)?, *s)))
        .unwrap_or(("", ""));
    let shadow = match buffer {
        "shadowcolor" | "shadowcolor0" => Some(0),
        "shadowcolor1" => Some(1),
        _ => None,
    };
    if let Some(n) = shadow {
        let s = &mut plan.shadow;
        match what {
            "Format" => {
                if let Some(f) = BufferFormat::parse(value.trim()) {
                    s.color_format[n] = f;
                }
            }
            "Clear" => {
                if let Some(b) = boolean(value) {
                    s.color_clear[n] = b;
                }
            }
            "ClearColor" => {
                if let Some(c) = vec4(value) {
                    s.color_clear_color[n] = c;
                }
            }
            _ => {}
        }
        return;
    }
    let Some(i) = buffer_index(buffer) else {
        return;
    };
    let spec = &mut plan.buffers[i as usize];
    match what {
        "Format" => {
            if let Some(f) = BufferFormat::parse(value.trim()) {
                spec.format = f;
                format_given[i as usize] = true;
            }
        }
        "Clear" => {
            if let Some(b) = boolean(value) {
                spec.clear = b;
            }
        }
        "ClearColor" => {
            if let Some(c) = vec4(value) {
                spec.clear_color = Some(c);
            }
        }
        _ => {}
    }
}

/// The buffers a full-screen pass regenerates mipmaps for: its fragment
/// source's `<buffer>MipmapEnabled = true` (the last one per buffer wins).
pub fn mipmaps(raw_fragment: &str, preprocessed_fragment: &str) -> BTreeSet<u8> {
    let mut out = BTreeSet::new();
    for (name, value) in directives_of(raw_fragment, preprocessed_fragment) {
        let Some(i) = name.strip_suffix("MipmapEnabled").and_then(buffer_index) else {
            continue;
        };
        match boolean(&value) {
            Some(true) => out.insert(i),
            Some(false) => out.remove(&i),
            None => false,
        };
    }
    out
}

/// The draw buffers a fragment source's live `DRAWBUFFERS` / `RENDERTARGETS`
/// directive names: `raw` is the source with includes expanded and comments
/// kept; a directive in a branch its `#if`s drop does not count. When the
/// branches cannot be evaluated, the last directive of the raw text wins.
pub fn live_draw_buffers(
    raw: &str,
    file: &str,
    files: &crate::PackFiles,
    macros: &BTreeMap<String, String>,
) -> Option<Vec<u8>> {
    match crate::shaders_properties::live_lines(raw, file, files, macros, true) {
        Ok(live) => crate::properties::draw_buffers(&live),
        Err(_) => crate::properties::draw_buffers(raw),
    }
}

/// The fragment outputs a preprocessed fragment source declares at top
/// level (`out vec4 color;`, `layout(location = 2) out vec2 data;`), in
/// declaration order.
pub fn fragment_outputs(preprocessed: &str) -> Vec<FragOutput> {
    let mut out = Vec::new();
    for statement in preprocessed.split(';') {
        let s = statement
            .rsplit(['{', '}'])
            .next()
            .unwrap_or(statement)
            .trim();
        let (location, rest) = match s.strip_prefix("layout") {
            Some(rest) => {
                let rest = rest.trim_start();
                let Some(inner) = rest.strip_prefix('(') else {
                    continue;
                };
                let Some(close) = inner.find(')') else {
                    continue;
                };
                let location = inner[..close].split(',').find_map(|q| {
                    let (k, v) = q.split_once('=')?;
                    (k.trim() == "location").then(|| v.trim().parse::<u32>().ok())?
                });
                (location, &inner[close + 1..])
            }
            None => (None, s),
        };
        if rest.contains(['(', '[', '=']) {
            continue;
        }
        let words: Vec<&str> = rest
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|w| !w.is_empty())
            .collect();
        let Some(at) = words.iter().position(|w| *w == "out") else {
            continue;
        };
        let before_ok = words[..at].iter().all(|w| {
            matches!(
                *w,
                "flat" | "smooth" | "noperspective" | "centroid" | "invariant"
            )
        });
        let mut rest = words[at + 1..]
            .iter()
            .skip_while(|w| matches!(**w, "highp" | "mediump" | "lowp"));
        let (Some(ty), true) = (rest.next(), before_ok) else {
            continue;
        };
        for (k, name) in rest.enumerate() {
            out.push(FragOutput {
                index: out.len(),
                name: name.to_string(),
                ty: ty.to_string(),
                // Further names of one declaration take the next locations.
                location: location.map(|l| l + k as u32),
            });
        }
    }
    out
}

/// The highest output a fragment source writes: `gl_FragData[i]` (i < 16)
/// or a declared output's location; `None` when it writes neither.
pub fn highest_output(preprocessed: &str) -> Option<u8> {
    let data = (0..16u8)
        .rev()
        .find(|i| preprocessed.contains(&format!("gl_FragData[{i}]")));
    let outs = fragment_outputs(preprocessed)
        .iter()
        .map(|o| o.slot().min(15) as u8)
        .max();
    data.max(outs)
}

pub use crate::shaders_properties::{ShadersProperties, shaders_properties};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PackFiles;
    use crate::plan::{BufferSpec, Clouds, PackProperties, TextureStage};

    fn plan_of(sources: &[(&str, &str)]) -> PackPlan {
        let mut plan = PackPlan::default();
        read_globals(
            &mut plan,
            sources.iter().map(|(raw, pre)| ProgramText {
                raw: ["", raw],
                preprocessed: ["", pre],
            }),
        );
        plan
    }

    #[test]
    fn const_statements() {
        let text = "uniform float x; const int a = 1;\nvoid f() { const highp float b=2.0f; }\n\
                    const\tbool c\n = true ;\nfloat constant = 1.0; const vec4 d = vec4(1.0, 0.5, 0.0, 1.0);";
        let want = [
            ("a", "1"),
            ("b", "2.0f"),
            ("c", "true"),
            ("d", "vec4(1.0, 0.5, 0.0, 1.0)"),
        ];
        assert_eq!(
            consts(text),
            want.map(|(k, v)| (k.to_string(), v.to_string()))
        );
        assert_eq!(
            block_comments("a /* one */ // /* not */\n/* two"),
            [" one ", " two"]
        );
    }

    #[test]
    fn buffer_directives() {
        let pre = "const int RGB16 = 0;\nconst int gnormalFormat = RGB16;\nconst int colortex9Format = R11F_G11F_B10F;\n\
                   const bool gaux4Clear = false;\nconst vec4 colortex2ClearColor = vec4(0.5);\n\
                   const vec4 colortex3ClearColor = vec4(1.0, 0.0, 0.0, 1.0);\nconst int colortex16Format = RGBA8;\n\
                   const int shadowcolor1Format = RGBA16F;\nconst bool shadowcolor0Clear = false;\n\
                   const vec4 shadowcolor0ClearColor = vec4(0.0, 0.0, 0.0, 1.0);\nuniform sampler2D gdepth;";
        let raw = "/*\nconst int colortex5Format = RGBA32F;\n*/\n/* GAUX4FORMAT:RGB16 */ // /* const int colortex6Format = RG16; */";
        let plan = plan_of(&[(raw, pre)]);
        let f = |i: usize| plan.buffers[i].format;
        assert_eq!(
            [f(0), f(1), f(2), f(5), f(6), f(7), f(9)],
            [
                BufferFormat::Rgba8,
                // `gdepth` is declared and colortex1 has no format: float.
                BufferFormat::Rgba32F,
                BufferFormat::Rgb16,
                BufferFormat::Rgba32F,
                BufferFormat::Rgba8,
                BufferFormat::Rgb16,
                BufferFormat::R11G11B10F,
            ]
        );
        assert!(!plan.buffers[7].clear && plan.buffers[6].clear);
        assert_eq!(plan.buffers[2].clear_color, Some([0.5; 4]));
        assert_eq!(plan.buffers[3].clear_color, Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(plan.buffers[4], BufferSpec::default());
        assert_eq!(plan.shadow.color_format[1], BufferFormat::Rgba16F);
        assert!(!plan.shadow.color_clear[0] && plan.shadow.color_clear[1]);
        assert_eq!(plan.shadow.color_clear_color[0], [0.0, 0.0, 0.0, 1.0]);
        // A format named for colortex1 wins over the gdepth rule.
        let named = plan_of(&[(
            "",
            "uniform sampler2D gdepth;\nconst int gdepthFormat = RGBA8;",
        )]);
        assert_eq!(named.buffers[1].format, BufferFormat::Rgba8);
        let generic = plan_of(&[(
            "",
            "uniform sampler2D gdepth;\nconst int gdepthFormat = RGBA;",
        )]);
        assert_eq!(generic.buffers[1].format, BufferFormat::Rgba32F);
        // A format identifier the pack `#define`s: the raw text's name counts.
        let defined = plan_of(&[(
            "#define RGBA16F 1\nconst int colortex8Format = RGBA16F;",
            "const int colortex8Format = 1;",
        )]);
        assert_eq!(defined.buffers[8].format, BufferFormat::Rgba16F);
    }

    #[test]
    fn shadow_and_world_directives() {
        let pre = "const int shadowMapResolution = 2048;\nconst float shadowDistance=96.0;\n\
                   const float shadowIntervalSize = 4.0f;\nconst float shadowDistanceRenderMul = 1.0f;\n\
                   const float sunPathRotation = -40.0f;\nconst bool shadowHardwareFiltering0 = true;\n\
                   const bool shadowtex1Mipmap = true;\nconst bool shadowtex1Nearest = false;\n\
                   const bool shadowcolor0Mipmap = true;\nconst bool shadow0MinMagNearest = true;\n\
                   const int noiseTextureResolution = 64;\nconst float ambientOcclusionLevel = 1.5;\n\
                   const float wetnessHalflife = 1.0;\nconst float drynessHalflife = 60.0;\n\
                   const float eyeBrightnessHalflife = 5.0;";
        let plan = plan_of(&[("", pre)]);
        let s = plan.shadow;
        assert_eq!(
            (
                s.resolution,
                s.distance,
                s.interval,
                s.distance_render_mul,
                s.sun_path_rotation
            ),
            (2048, 96.0, 4.0, 1.0, -40.0)
        );
        assert_eq!(s.hardware_filtering, [true, false]);
        assert_eq!(s.tex_mipmap, [false, true]);
        assert_eq!(s.tex_nearest, [true, false]);
        assert_eq!(s.color_mipmap, [true, false]);
        assert_eq!(plan.noise_resolution, 64);
        assert_eq!(plan.ambient_occlusion_level, 1.0);
        // The half-life quirk: the last of the two sets the rising one.
        assert_eq!(
            (plan.wetness_half_life, plan.dryness_half_life),
            (60.0, 200.0)
        );
        assert_eq!(plan.eye_brightness_half_life, 5.0);
        let both = plan_of(&[
            (
                "/* SHADOWRES:512 */",
                "const bool shadowHardwareFiltering = true;\nconst bool generateShadowMipmap = true;",
            ),
            ("", "const bool generateShadowColorMipmap = true;"),
        ]);
        assert_eq!(both.shadow.resolution, 512);
        assert_eq!(both.shadow.hardware_filtering, [true; 2]);
        assert_eq!(both.shadow.tex_mipmap, [true; 2]);
        assert_eq!(both.shadow.color_mipmap, [true; 2]);
        // Not declared: the defaults.
        assert_eq!(plan_of(&[("", "void main() {}")]), PackPlan::default());
    }

    #[test]
    fn pass_mipmaps() {
        let pre = "const bool gaux3MipmapEnabled = true;\nconst bool gaux1MipmapEnabled = false;\n\
                   const bool colortex9MipmapEnabled = true;\nconst bool colortex2MipmapEnabled = true;\n\
                   const bool colortex2MipmapEnabled = false;";
        assert_eq!(
            mipmaps("/* const bool colortex0MipmapEnabled = true; */", pre),
            BTreeSet::from([0, 6, 9])
        );
    }

    #[test]
    fn properties_file() {
        let text = "clouds = off\noldLighting = true\nunderwaterOverlay=on\nsun=false\nseparateAo=true\n\
                    oldHandLight=false\ntexture.gbuffers.gaux1=textures/a.png\n\
                    texture.composite.colortex3=/tex/b.dat TEXTURE_2D RGBA8 4 4 RGBA UNSIGNED_BYTE\n\
                    texture.prepare.colortex3=c.png\ntexture.noise=n.png\n\
                    #ifdef FANCY\ntexture.deferred.shadowcolor=f.png\n#endif\n\
                    program.world0/gbuffers_clouds.enabled=false\nprogram.composite3.enabled=!FANCY\n\
                    program.composite4.enabled=SLOW\nprogram.final.enabled=true\n";
        let files = PackFiles(BTreeMap::from([(
            "shaders/shaders.properties".to_string(),
            text.as_bytes().to_vec(),
        )]));
        let macros = BTreeMap::from([("FANCY".to_string(), String::new())]);
        let p = shaders_properties(&files, &macros).unwrap();
        assert_eq!(
            p.properties,
            PackProperties {
                clouds: Clouds::Off,
                old_lighting: true,
                underwater_overlay: true,
                sun: false,
                moon: true,
                separate_ao: true,
                old_hand_light: false,
                ..PackProperties::default()
            }
        );
        let t: Vec<_> = p
            .textures
            .iter()
            .map(|t| (t.stage, t.sampler.as_str(), t.path.as_str()))
            .collect();
        assert_eq!(t.len(), 5);
        assert_eq!(
            t[0],
            (
                TextureStage::Gbuffers,
                "colortex4",
                "shaders/textures/a.png"
            )
        );
        assert_eq!(
            t[1],
            (TextureStage::Composite, "colortex3", "shaders/tex/b.dat")
        );
        assert_eq!(t[2], (TextureStage::Prepare, "colortex3", "shaders/c.png"));
        assert_eq!(t[3], (TextureStage::All, "noisetex", "shaders/n.png"));
        assert_eq!(
            t[4],
            (TextureStage::Deferred, "shadowcolor0", "shaders/f.png")
        );
        assert_eq!(
            p.disabled,
            BTreeSet::from([
                "shaders/world0/gbuffers_clouds".to_string(),
                "shaders/composite3".to_string(),
                "shaders/composite4".to_string(),
            ])
        );
        assert_eq!(
            shaders_properties(&PackFiles::default(), &macros).unwrap(),
            ShadersProperties::default()
        );
    }

    #[test]
    fn names() {
        assert_eq!(
            [
                buffer_index("gaux4"),
                buffer_index("colortex15"),
                buffer_index("colortex16"),
                buffer_index("colortex")
            ],
            [Some(7), Some(15), None, None]
        );
        assert_eq!(canonical_sampler("gaux2"), "colortex5");
        assert_eq!(canonical_sampler("gdepthtex"), "depthtex0");
        assert_eq!(canonical_sampler("noisetex"), "noisetex");
    }
}

#[cfg(test)]
mod output_tests {
    use super::*;
    use crate::PackFiles;

    #[test]
    fn declared_outputs() {
        let text = "uniform float x; layout(location = 3) out vec4 a; out vec3 b, c;\n\
                    layout (location=1, index = 0) out highp float d;\nvoid f(out vec4 e) { }\n\
                    vec4 g() { out_value = 1.0; }\nflat out int h[2];";
        let outs: Vec<(String, String, Option<u32>, usize)> = fragment_outputs(text)
            .into_iter()
            .map(|o| (o.name.clone(), o.ty.clone(), o.location, o.slot()))
            .collect();
        let want = [
            ("a", "vec4", Some(3), 3),
            ("b", "vec3", None, 1),
            ("c", "vec3", None, 2),
            ("d", "float", Some(1), 1),
        ];
        assert_eq!(
            outs,
            want.map(|(n, t, l, s)| (n.to_string(), t.to_string(), l, s))
        );
        assert_eq!(
            highest_output("gl_FragData[2] = x; gl_FragData[11] = y;"),
            Some(11)
        );
        assert_eq!(highest_output(text), Some(3));
        assert_eq!(highest_output("gl_FragColor = x;"), None);
    }

    #[test]
    fn live_directives_only() {
        let files = PackFiles::default();
        let macros = BTreeMap::from([("HIGH".to_string(), "1".to_string())]);
        let text = "#if HIGH == 1 // a comment\n/* DRAWBUFFERS:12 */\n#else\n/* DRAWBUFFERS:3 */\n#endif\n\
                    /* note:\n#if 0\n*/\nvoid main() {}\n";
        assert_eq!(
            live_draw_buffers(text, "x.fsh", &files, &macros),
            Some(vec![1, 2])
        );
        assert_eq!(
            live_draw_buffers(text, "x.fsh", &files, &BTreeMap::new()),
            Some(vec![3])
        );
        // Unbalanced branches: the raw text's last directive.
        assert_eq!(
            live_draw_buffers(
                "#if 1\n/* RENDERTARGETS: 4,15 */\n",
                "x.fsh",
                &files,
                &macros
            ),
            Some(vec![4, 15])
        );
    }
}
