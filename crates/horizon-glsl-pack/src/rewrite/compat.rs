//! Source-level renames ahead of the rewriter's passes:
//!
//! - Core-profile inputs (`in vec3 vaPosition;`, `uniform mat3 normalMatrix;`
//!   …) become the compatibility built-ins they mirror, their declarations
//!   dropped: `vaPosition` reads `gl_Vertex.xyz` (Horizon's `chunkOffset`
//!   is zero), `vaUV2` the lightmap coordinates, `normalMatrix`
//!   `gl_NormalMatrix`. Only names the stage declares with that storage, or
//!   (vertex inputs) uses without declaring, are touched, so a pack's own
//!   `normalMatrix` local stays.
//! - `gl_TexCoord[i]` (literal `i`) becomes a varying `hz_TexCoord<i>` per
//!   index the stage uses.
//! - Initializer lists (`float k[2][2] = {{a, b}, {c, d}};`, GLSL 4.2)
//!   become constructors (`float[2][2](float[2](a, b), float[2](c, d))`),
//!   which naga parses.

use std::collections::BTreeSet;

use super::lex::{Kind, tokenize};
use crate::Stage;

/// (name, storage, type, replacement, vertex stage only)
const CORE: [(&str, &str, &str, bool); 9] = [
    ("vaPosition", "vec3", "(gl_Vertex.xyz)", true),
    ("vaColor", "vec4", "gl_Color", true),
    ("vaUV0", "vec2", "(gl_MultiTexCoord0.xy)", true),
    ("vaUV1", "ivec2", "ivec2(0, 10)", true),
    ("vaUV2", "ivec2", "ivec2(gl_MultiTexCoord1.xy)", true),
    ("vaNormal", "vec3", "gl_Normal", true),
    ("normalMatrix", "mat3", "gl_NormalMatrix", false),
    ("textureMatrix", "mat4", "gl_TextureMatrix[0]", false),
    (
        "modelViewMatrixInverse",
        "mat4",
        "gl_ModelViewMatrixInverse",
        false,
    ),
];

/// The stage with the renames of the module doc; None when there are none.
pub(super) fn apply(source: &str, stage: Stage) -> Option<String> {
    let listed = init_lists(source);
    let renamed = renames(listed.as_deref().unwrap_or(source), stage);
    renamed.or(listed)
}

/// Initializer lists as constructors (see the module doc).
fn init_lists(source: &str) -> Option<String> {
    let toks = tokenize(source);
    let sig: Vec<usize> = (0..toks.len())
        .filter(|&i| toks[i].kind != Kind::Space)
        .collect();
    let t = |j: usize| sig.get(j).map_or("", |&i| toks[i].text);
    // (token index of the `{`, of its matching `}`, the constructor text)
    let mut lists: Vec<(usize, usize, String)> = Vec::new();
    let mut j = 0;
    while j + 1 < sig.len() {
        if !(t(j) == "=" && t(j + 1) == "{") {
            j += 1;
            continue;
        }
        // The declaration: `… TYPE NAME [dims] =` back to the statement start.
        let start = (0..j)
            .rev()
            .find(|&h| matches!(t(h), ";" | "{" | "}"))
            .map_or(0, |h| h + 1);
        let decl: Vec<&str> = (start..j).map(t).collect();
        let Some(name_at) = decl
            .iter()
            .position(|w| *w == "[")
            .map(|p| p.saturating_sub(1))
            .or(decl.len().checked_sub(1))
        else {
            j += 1;
            continue;
        };
        if name_at == 0 {
            j += 1;
            continue;
        }
        let ty = decl[name_at - 1];
        let mut dims: Vec<String> = Vec::new();
        let mut k = name_at + 1;
        while k < decl.len() && decl[k] == "[" {
            let close = (k..decl.len())
                .find(|&c| decl[c] == "]")
                .unwrap_or(decl.len());
            dims.push(decl[k + 1..close].join(" "));
            k = close + 1;
        }
        // The list's tokens up to its matching `}`.
        let mut depth = 0;
        let mut end = j + 1;
        for m in j + 1..sig.len() {
            match t(m) {
                "{" => depth += 1,
                "}" => {
                    depth -= 1;
                    if depth == 0 {
                        end = m;
                        break;
                    }
                }
                _ => {}
            }
        }
        let words: Vec<&str> = (j + 1..=end).map(t).collect();
        let (text, _) = constructor(&words, 0, ty, &dims);
        lists.push((sig[j + 1], sig[end], text));
        j = end + 1;
    }
    if lists.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut i = 0;
    while i < toks.len() {
        if let Some((_, end, text)) = lists.iter().find(|(open, _, _)| *open == i) {
            out.push_str(text);
            i = end + 1;
            continue;
        }
        out.push_str(toks[i].text);
        i += 1;
    }
    Some(out)
}

/// The constructor for the brace list starting at `words[at]` (a `{`) at
/// array level `level`; returns it and the index after its `}`.
fn constructor(words: &[&str], at: usize, ty: &str, dims: &[String]) -> (String, usize) {
    let mut items: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut i = at + 1;
    let mut parens = 0;
    while i < words.len() {
        match words[i] {
            "{" if parens == 0 => {
                let (inner, next) = constructor(words, i, ty, dims.get(1..).unwrap_or(&[]));
                current.push_str(&inner);
                i = next;
                continue;
            }
            "}" if parens == 0 => {
                i += 1;
                break;
            }
            "," if parens == 0 => items.push(std::mem::take(&mut current)),
            w => {
                match w {
                    "(" | "[" => parens += 1,
                    ")" | "]" => parens -= 1,
                    _ => {}
                }
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(w);
            }
        }
        i += 1;
    }
    if !current.trim().is_empty() {
        items.push(current);
    }
    let shape: String = dims
        .iter()
        .enumerate()
        .map(|(k, d)| {
            if k == 0 && d.is_empty() {
                format!("[{}]", items.len())
            } else {
                format!("[{d}]")
            }
        })
        .collect();
    (format!("{ty}{shape}({})", items.join(", ")), i)
}

fn renames(source: &str, stage: Stage) -> Option<String> {
    let toks = tokenize(source);
    let sig: Vec<usize> = (0..toks.len())
        .filter(|&i| toks[i].kind != Kind::Space)
        .collect();
    let t = |j: usize| sig.get(j).map_or("", |&i| toks[i].text);
    // Declarations to drop (token index ranges) and the names they declare.
    let mut drop: Vec<(usize, usize)> = Vec::new();
    let mut renamed: BTreeSet<&str> = BTreeSet::new();
    for j in 0..sig.len() {
        let storage = t(j);
        if !matches!(storage, "in" | "attribute" | "uniform") {
            continue;
        }
        let Some(&(name, ty, _, vertex)) = CORE.iter().find(|c| c.0 == t(j + 2)) else {
            continue;
        };
        let wanted_storage = if matches!(
            name,
            "normalMatrix" | "textureMatrix" | "modelViewMatrixInverse"
        ) {
            storage == "uniform"
        } else {
            storage != "uniform"
        };
        if !wanted_storage
            || t(j + 1) != ty
            || t(j + 3) != ";"
            || (vertex && stage != Stage::Vertex)
        {
            continue;
        }
        drop.push((sig[j], sig[j + 3]));
        renamed.insert(name);
    }
    // Core-profile inputs used without any declaration (Iris declares them for
    // the pack): the same renames.
    for &(name, _, _, vertex) in &CORE {
        if renamed.contains(name) || (vertex && stage != Stage::Vertex) {
            continue;
        }
        let used = (0..sig.len()).any(|j| t(j) == name && t(j.wrapping_sub(1)) != ".");
        let declared = (1..sig.len()).any(|j| t(j) == name && toks[sig[j - 1]].kind == Kind::Ident);
        let uniform_like = matches!(
            name,
            "normalMatrix" | "textureMatrix" | "modelViewMatrixInverse"
        );
        if used && !declared && !uniform_like {
            renamed.insert(name);
        }
    }
    // gl_TexCoord[<literal>].
    let mut tex_coords: BTreeSet<String> = BTreeSet::new();
    for j in 0..sig.len() {
        if t(j) == "gl_TexCoord"
            && t(j + 1) == "["
            && t(j + 3) == "]"
            && t(j + 2).parse::<u8>().is_ok()
        {
            tex_coords.insert(t(j + 2).to_string());
        }
    }
    if renamed.is_empty() && tex_coords.is_empty() {
        return None;
    }
    let mut out = String::new();
    for i in &tex_coords {
        out.push_str(&format!("varying vec4 hz_TexCoord{i};\n"));
    }
    let mut j = 0;
    let mut i = 0;
    while i < toks.len() {
        if let Some(&(_, end)) = drop.iter().find(|(start, _)| *start == i) {
            i = end + 1;
            continue;
        }
        while j < sig.len() && sig[j] < i {
            j += 1;
        }
        let text = toks[i].text;
        if toks[i].kind == Kind::Ident && renamed.contains(text) && t(j.wrapping_sub(1)) != "." {
            let (_, _, value, _) = CORE
                .iter()
                .find(|c| c.0 == text)
                .expect("renamed is from CORE");
            out.push_str(value);
        } else if text == "gl_TexCoord"
            && sig.get(j) == Some(&i)
            && tex_coords.contains(t(j + 2))
            && t(j + 1) == "["
        {
            out.push_str(&format!("hz_TexCoord{}", t(j + 2)));
            i = sig[j + 3] + 1;
            continue;
        } else {
            out.push_str(text);
        }
        i += 1;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::apply;
    use crate::Stage;

    #[test]
    fn initializer_lists_become_constructors() {
        let src = "const float k[2][2] = {\n {1.0 / 4.0, 1.0 / 8.0},\n {1.0, f(2.0, 3.0)}\n};\n\
                   vec3 v = {1.0, 2.0, 3.0}; int w[] = {1, 2, 3};\nfloat g() { return k[0][1]; }\n";
        let out = apply(src, Stage::Fragment).unwrap();
        assert!(
            out.contains("const float k[2][2] = float[2][2](float[2](1.0 / 4.0, 1.0 / 8.0), float[2](1.0, f ( 2.0 , 3.0 )));"),
            "{out}"
        );
        assert!(out.contains("vec3 v = vec3(1.0, 2.0, 3.0);"), "{out}");
        assert!(out.contains("int w[] = int[3](1, 2, 3);"), "{out}");
        assert!(out.contains("float g()"), "{out}");
    }

    #[test]
    fn core_profile_inputs_become_built_ins() {
        let src = "in vec3 vaPosition;\nin vec4 vaColor;\nin ivec2 vaUV2;\nuniform mat3 normalMatrix;\n\
                   uniform mat4 modelViewMatrix;\n\
                   void main() { vec3 p = vaPosition + vec3(0.0); gl_Position = modelViewMatrix * vec4(p, 1.0); \
                   vec2 l = vec2(vaUV2) * normalMatrix[0].xy; gl_TexCoord[1] = vaColor * l.x; }\n";
        let out = apply(src, Stage::Vertex).unwrap();
        assert!(!out.contains("in vec3 vaPosition"), "{out}");
        assert!(
            out.contains("vec3 p = (gl_Vertex.xyz) + vec3(0.0)"),
            "{out}"
        );
        assert!(
            out.contains("vec2(ivec2(gl_MultiTexCoord1.xy)) * gl_NormalMatrix[0].xy"),
            "{out}"
        );
        assert!(out.contains("uniform mat4 modelViewMatrix;"), "{out}");
        assert!(out.starts_with("varying vec4 hz_TexCoord1;\n"), "{out}");
        assert!(out.contains("hz_TexCoord1 = gl_Color * l.x;"), "{out}");
        let rw = crate::rewrite::rewrite(src, Stage::Vertex).unwrap();
        assert!(rw.attributes.is_empty(), "{:?}", rw.attributes);
        assert_eq!(rw.varyings[0].name, "hz_TexCoord1");
        // Used without a declaration (Iris declares them).
        let out = apply(
            "void main() { gl_Position = vec4(vaPosition + vaNormal, 1.0); }",
            Stage::Vertex,
        )
        .unwrap();
        assert!(
            out.contains("vec4((gl_Vertex.xyz) + gl_Normal, 1.0)"),
            "{out}"
        );
        // A local called normalMatrix is the pack's own.
        assert_eq!(
            apply(
                "void main() { mat3 normalMatrix = mat3(1.0); }",
                Stage::Fragment
            ),
            None
        );
    }
}
