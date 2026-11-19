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
