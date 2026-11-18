//! Pack options (the user-facing settings a pack exposes) found in its
//! sources, and overrides applied by rewriting the source text before
//! preprocessing, as the format does:
//!
//! - boolean: a `#define X` line, or `//#define X` when off, is an option when
//!   some `#ifdef X` / `#ifndef X` in the same program (includes expanded)
//!   tests X (`#if` references do not count). Toggling adds or removes the `//`.
//! - value: `#define X v // [a b c]` (the trailing bracket list is required);
//!   the value is replaced.
//! - `const`: `const int shadowMapResolution = 2048; // [1024 2048]` for a
//!   fixed list of names the renderer reads ([`CONST_OPTIONS`]); the value
//!   after `=` is replaced.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use crate::plan::PackOption;
use crate::{PackFiles, preprocess};

/// The `const` names a pack can expose as options (and an override can set).
pub const CONST_OPTIONS: &[&str] = &[
    "shadowMapResolution",
    "shadowDistance",
    "shadowDistanceRenderMul",
    "shadowIntervalSize",
    "shadowMapFov",
    "sunPathRotation",
    "ambientOcclusionLevel",
    "superSamplingLevel",
    "wetnessHalflife",
    "drynessHalflife",
    "eyeBrightnessHalflife",
    "centerDepthHalflife",
    "noiseTextureResolution",
    "shadowHardwareFiltering",
    "shadowHardwareFiltering0",
    "shadowHardwareFiltering1",
    "generateShadowMipmap",
    "generateShadowColorMipmap",
    "shadowtexMipmap",
    "shadowtex0Mipmap",
    "shadowtex1Mipmap",
    "shadowcolor0Mipmap",
    "shadowcolor1Mipmap",
    "shadowtexNearest",
    "shadowtex0Nearest",
    "shadowtex1Nearest",
    "shadowcolor0Nearest",
    "shadowcolor1Nearest",
    "shadow0MinMagNearest",
    "shadow1MinMagNearest",
];

/// One line that could carry an option, with byte ranges into the line.
#[derive(Debug, PartialEq, Eq)]
enum OptionLine {
    /// `#define NAME [value]`, `commented` when it reads `//#define`.
    Define {
        name: String,
        /// The `//` (and spaces up to `#`) that turn the line off.
        comment: Option<Range<usize>>,
        /// The `#` of the directive.
        hash: usize,
        /// Where the name ends (a value is inserted there when there is none).
        name_end: usize,
        value: Option<Range<usize>>,
        allowed: Vec<String>,
    },
    /// `const TYPE NAME = value;`
    Const {
        name: String,
        value: Range<usize>,
        allowed: Vec<String>,
    },
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The values of a trailing `// … [a b c]` comment.
fn allowed_values(comment: &str) -> Vec<String> {
    let Some(open) = comment.rfind('[') else {
        return Vec::new();
    };
    let Some(len) = comment[open..].find(']') else {
        return Vec::new();
    };
    comment[open + 1..open + len]
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn skip_ws(line: &str, mut at: usize) -> usize {
    while line[at..].starts_with([' ', '\t']) {
        at += 1;
    }
    at
}

fn ident_end(line: &str, at: usize) -> usize {
    at + line[at..]
        .find(|c: char| !is_ident(c))
        .unwrap_or(line.len() - at)
}

fn parse_line(line: &str) -> Option<OptionLine> {
    let start = skip_ws(line, 0);
    let (comment, hash) = if line[start..].starts_with("//") {
        let hash = skip_ws(line, start + 2);
        (Some(start..hash), hash)
    } else {
        (None, start)
    };
    if line[hash..].starts_with('#') {
        let kw = skip_ws(line, hash + 1);
        let rest = line[kw..].strip_prefix("define")?;
        if !rest.starts_with([' ', '\t']) {
            return None;
        }
        let name_at = skip_ws(line, kw + "define".len());
        let name_end = ident_end(line, name_at);
        if name_end == name_at || line[name_end..].starts_with('(') {
            return None;
        }
        let tail = &line[name_end..];
        let (body, note) = match tail.find("//") {
            Some(i) => (&tail[..i], &tail[i..]),
            None => (tail, ""),
        };
        let value = (!body.trim().is_empty()).then(|| {
            let lead = body.len() - body.trim_start().len();
            name_end + lead..name_end + body.trim_end().len()
        });
        return Some(OptionLine::Define {
            name: line[name_at..name_end].to_string(),
            comment,
            hash,
            name_end,
            value,
            allowed: allowed_values(note),
        });
    }
    if comment.is_some() {
        return None;
    }
    // `const TYPE NAME = value;` on one line.
    let rest = line[start..].strip_prefix("const")?;
    if !rest.starts_with([' ', '\t']) {
        return None;
    }
    let ty = skip_ws(line, start + 5);
    let ty_end = ident_end(line, ty);
    let name_at = skip_ws(line, ty_end);
    let name_end = ident_end(line, name_at);
    if ty_end == ty || name_end == name_at {
        return None;
    }
    let eq = skip_ws(line, name_end);
    if !line[eq..].starts_with('=') {
        return None;
    }
    let value_at = skip_ws(line, eq + 1);
    let semi = value_at + line[value_at..].find(';')?;
    let value_end = value_at + line[value_at..semi].trim_end().len();
    Some(OptionLine::Const {
        name: line[name_at..name_end].to_string(),
        value: value_at..value_end,
        allowed: allowed_values(&line[semi..]),
    })
}

/// The names tested by `#ifdef` / `#ifndef` lines of `text`.
fn ifdef_names(text: &str) -> BTreeSet<&str> {
    text.lines()
        .filter_map(|line| {
            let d = line.trim_start().strip_prefix('#')?.trim_start();
            let rest = d
                .strip_prefix("ifdef")
                .or_else(|| d.strip_prefix("ifndef"))?;
            let name = rest.trim_start();
            let end = name.find(|c: char| !is_ident(c)).unwrap_or(name.len());
            (rest.starts_with([' ', '\t']) && end > 0).then(|| &name[..end])
        })
        .collect()
}

/// Program sources (the files a pass is compiled from).
fn is_program(path: &str) -> bool {
    path.starts_with("shaders/")
        && [".vsh", ".fsh", ".gsh", ".csh"]
            .iter()
            .any(|e| path.ends_with(e))
}

/// Files whose text overrides rewrite: sources and includes, not images,
/// properties or language files.
fn is_source(path: &str) -> bool {
    let ext = path.rsplit_once('.').map_or("", |(_, e)| e);
    path.starts_with("shaders/")
        && !matches!(
            ext,
            "png" | "properties" | "lang" | "mcmeta" | "txt" | "md" | "json" | "bin" | "dat"
        )
}

/// The options of a pack, each once (first definition wins), sorted by name.
pub fn discover(files: &PackFiles) -> Vec<PackOption> {
    let mut found: BTreeMap<String, PackOption> = BTreeMap::new();
    for path in files.0.keys().filter(|p| is_program(p)) {
        let Ok(text) = preprocess::expand_includes(files, path) else {
            continue;
        };
        let tested = ifdef_names(&text);
        for line in text.lines() {
            let option = match parse_line(line) {
                Some(OptionLine::Define {
                    name,
                    comment,
                    value: None,
                    ..
                }) if tested.contains(name.as_str()) => PackOption {
                    name,
                    value: None,
                    enabled: comment.is_none(),
                    allowed: Vec::new(),
                },
                Some(OptionLine::Define {
                    name,
                    comment: None,
                    value: Some(v),
                    allowed,
                    ..
                }) if !allowed.is_empty() => PackOption {
                    name,
                    value: Some(line[v].to_string()),
                    enabled: true,
                    allowed,
                },
                Some(OptionLine::Const {
                    name,
                    value,
                    allowed,
                }) if !allowed.is_empty() && CONST_OPTIONS.contains(&name.as_str()) => PackOption {
                    name,
                    value: Some(line[value].to_string()),
                    enabled: true,
                    allowed,
                },
                _ => continue,
            };
            found.entry(option.name.clone()).or_insert(option);
        }
    }
    found.into_values().collect()
}

/// The macros options define, for preprocessing `shaders.properties`: an
/// enabled boolean is defined empty, a value option to its value.
pub fn macros(options: &[PackOption]) -> BTreeMap<String, String> {
    options
        .iter()
        .filter(|o| o.enabled && !CONST_OPTIONS.contains(&o.name.as_str()))
        .map(|o| (o.name.clone(), o.value.clone().unwrap_or_default()))
        .collect()
}

/// `line` with the override `value` (see [`crate::PackOptions::overrides`]).
fn override_line(line: &str, value: Option<&str>) -> Option<String> {
    let set = |r: Range<usize>, with: &str| format!("{}{with}{}", &line[..r.start], &line[r.end..]);
    match parse_line(line)? {
        OptionLine::Const { name, value: r, .. } if CONST_OPTIONS.contains(&name.as_str()) => {
            Some(set(r, value?))
        }
        OptionLine::Const { .. } => None,
        OptionLine::Define {
            comment,
            hash,
            name_end,
            value: current,
            ..
        } => {
            let on = match value {
                None => false,
                Some("false") => current.is_some(),
                Some(_) => true,
            };
            if !on {
                return comment.is_none().then(|| set(hash..hash, "//"));
            }
            // Set the value first (it lies after the comment marker).
            let line = match (value, current) {
                (Some("true"), _) | (None, _) => line.to_string(),
                (Some(v), Some(r)) => set(r, v),
                (Some(v), None) => set(name_end..name_end, &format!(" {v}")),
            };
            Some(match comment {
                Some(c) => format!("{}{}", &line[..c.start], &line[c.end..]),
                None => line,
            })
        }
    }
}

fn option_name(line: &str) -> Option<String> {
    match parse_line(line)? {
        OptionLine::Define { name, .. } | OptionLine::Const { name, .. } => Some(name),
    }
}

/// The pack with `overrides` written into its sources: every option line of
/// a named option, in every source and include file, is rewritten. For
/// `(name, None)` (off) `#define name` lines get a leading `//`; for
/// `Some("true")` the `//` goes; `Some("false")` turns a boolean off; any
/// other value replaces a define's or a `const` option's value. The last
/// override of a name wins.
pub fn apply(files: &PackFiles, overrides: &[(String, Option<String>)]) -> PackFiles {
    if overrides.is_empty() {
        return files.clone();
    }
    let mut last: BTreeMap<&str, Option<&str>> = BTreeMap::new();
    for (name, value) in overrides {
        last.insert(name, value.as_deref());
    }
    let mut out = files.clone();
    for (path, bytes) in &mut out.0 {
        if !is_source(path) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let mut changed = false;
        let mut new = String::with_capacity(text.len());
        for line in text.split_inclusive('\n') {
            let (body, end) = match line.strip_suffix("\r\n") {
                Some(b) => (b, "\r\n"),
                None => match line.strip_suffix('\n') {
                    Some(b) => (b, "\n"),
                    None => (line, ""),
                },
            };
            let rewritten = option_name(body)
                .and_then(|name| last.get(name.as_str()).copied())
                .and_then(|value| override_line(body, value));
            match rewritten {
                Some(r) if r != body => {
                    changed = true;
                    new.push_str(&r);
                }
                _ => new.push_str(body),
            }
            new.push_str(end);
        }
        if changed {
            *bytes = new.into_bytes();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(files: &[(&str, &str)]) -> PackFiles {
        PackFiles(
            files
                .iter()
                .map(|(p, t)| (p.to_string(), t.as_bytes().to_vec()))
                .collect(),
        )
    }

    fn o(name: &str, v: Option<&str>) -> (String, Option<String>) {
        (name.to_string(), v.map(str::to_string))
    }

    #[test]
    fn discovers_each_form() {
        let header = "#define FOG // fog on\n//#define BLOOM\n  // #define HIDDEN\n#define USED_BY_IF\n\
                      #define LEVEL 2 // how much [1 2 4]\n#define CONSTANT 3\n";
        let main = "#include \"/lib/h.glsl\"\nconst int shadowMapResolution = 2048; // size [1024 2048]\n\
                    const float shadowDistance = 90.0;\nconst float other = 1.0; // [1.0 2.0]\n\
                    #ifdef FOG\n#endif\n#ifndef BLOOM\n#endif\n# ifdef  HIDDEN\n#endif\n#if defined USED_BY_IF\n#endif\n";
        let files = pack(&[
            ("shaders/lib/h.glsl", header),
            ("shaders/composite.fsh", main),
            // Not a program: its #ifdef does not count.
            ("shaders/lib/other.glsl", "#ifdef USED_BY_IF\n#endif\n"),
        ]);
        let found = discover(&files);
        let names: Vec<_> = found.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(
            names,
            ["BLOOM", "FOG", "HIDDEN", "LEVEL", "shadowMapResolution"]
        );
        let get = |n: &str| found.iter().find(|o| o.name == n).unwrap();
        assert!(get("FOG").enabled && !get("BLOOM").enabled && !get("HIDDEN").enabled);
        assert_eq!(get("FOG").value, None);
        assert_eq!(get("LEVEL").value.as_deref(), Some("2"));
        assert_eq!(get("LEVEL").allowed, ["1", "2", "4"]);
        assert_eq!(get("shadowMapResolution").value.as_deref(), Some("2048"));
        let m = macros(&found);
        assert_eq!(m.get("FOG").map(String::as_str), Some(""));
        assert_eq!(m.get("LEVEL").map(String::as_str), Some("2"));
        assert!(!m.contains_key("BLOOM") && !m.contains_key("shadowMapResolution"));
    }

    #[test]
    fn overrides_rewrite_lines() {
        let text = "#define FOG // fog on\r\n\t//#define BLOOM\n// #define SPACED 1 // [1 2]\n\
                    #define LEVEL 2 // [1 2 4]\nconst int shadowMapResolution = 2048; // [1024 2048]\n\
                    const float keep = 1.0;\n#define FOGGY\nvoid main() {}";
        let files = pack(&[
            ("shaders/a.fsh", text),
            ("shaders/shaders.properties", "#define FOG\n"),
        ]);
        let out = apply(
            &files,
            &[
                o("FOG", None),
                o("BLOOM", Some("true")),
                o("SPACED", Some("2")),
                o("LEVEL", Some("1")),
                o("LEVEL", Some("4")),
                o("shadowMapResolution", Some("4096")),
                o("keep", Some("3.0")),
            ],
        );
        assert_eq!(
            out.text("shaders/a.fsh").unwrap(),
            "//#define FOG // fog on\r\n\t#define BLOOM\n#define SPACED 2 // [1 2]\n\
             #define LEVEL 4 // [1 2 4]\nconst int shadowMapResolution = 4096; // [1024 2048]\n\
             const float keep = 1.0;\n#define FOGGY\nvoid main() {}"
        );
        // Properties are not sources.
        assert_eq!(
            out.text("shaders/shaders.properties").unwrap(),
            "#define FOG\n"
        );
        let toggled = apply(
            &pack(&[("shaders/a.fsh", "#define A\n//#define B\n#define C 1\n")]),
            &[
                o("A", Some("false")),
                o("B", Some("7")),
                o("C", Some("false")),
            ],
        );
        assert_eq!(
            toggled.text("shaders/a.fsh").unwrap(),
            "//#define A\n#define B 7\n#define C false\n"
        );
        let off = apply(&files, &[o("LEVEL", None)]);
        assert!(
            off.text("shaders/a.fsh")
                .unwrap()
                .contains("//#define LEVEL 2")
        );
    }

    #[test]
    fn line_forms() {
        assert_eq!(parse_line("#define F(x) x"), None);
        assert_eq!(parse_line("#defineX"), None);
        assert_eq!(parse_line("// plain comment"), None);
        assert_eq!(parse_line("const vec3 c = vec3(1.0)"), None);
        assert!(matches!(
            parse_line("const float sunPathRotation=-40.0f;"),
            Some(OptionLine::Const { name, .. }) if name == "sunPathRotation"
        ));
    }
}
