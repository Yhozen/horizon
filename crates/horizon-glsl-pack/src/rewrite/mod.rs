//! Preprocessed GLSL 120 (compatibility profile) → GLSL 450 text without
//! interface declarations. OWNER: S2. Contract: .context/glsl-pack-spec.md.
//!
//! The rewriter works on tokens and copies everything it does not touch
//! verbatim, so the body keeps the pack's own layout (and naga's error
//! messages stay readable). It only needs enough structure to split the
//! top level into declarations and function definitions and to find the
//! balanced arguments of sampling calls.

use std::collections::{BTreeMap, BTreeSet};

use crate::Stage;

/// A variable the host declares: an attribute or a varying.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Var {
    pub name: String,
    /// GLSL type (`vec4`, `float`, `ivec2`…).
    pub ty: String,
    pub flat: bool,
    pub array_len: Option<u32>,
}

/// A loose `uniform` declaration the rewriter removed (non-sampler).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Uniform {
    pub name: String,
    pub ty: String,
}

/// One stage of a pack program, ready for the host to wrap.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rewritten {
    /// Everything but the removed declarations, with the renames applied.
    pub body: String,
    /// `varying` (and `in`/`out` between stages) declarations, in source order.
    pub varyings: Vec<Var>,
    /// `attribute` (vertex `in`) declarations, e.g. `mc_Entity`.
    pub attributes: Vec<Var>,
    pub uniforms: Vec<Uniform>,
    /// Samplers declared and/or sampled (`gtexture`, `lightmap`, `colortex0`…).
    pub samplers: BTreeSet<String>,
    /// Legacy built-ins the body uses, by their new names (`hz_gl_Vertex`…).
    pub builtins: BTreeSet<String>,
    /// Calls the body makes to host sampling functions, e.g. `hz_sample_gtexture`.
    pub sample_calls: BTreeSet<String>,
}

/// Compatibility-profile built-ins the host provides as plain globals named `hz_` + name.
const LEGACY: &str = "\
    gl_Vertex gl_Color gl_SecondaryColor gl_Normal gl_MultiTexCoord0 gl_MultiTexCoord1 \
    gl_MultiTexCoord2 gl_MultiTexCoord3 gl_MultiTexCoord4 gl_MultiTexCoord5 \
    gl_MultiTexCoord6 gl_MultiTexCoord7 gl_TextureMatrix gl_ModelViewMatrix \
    gl_ProjectionMatrix gl_ModelViewProjectionMatrix gl_NormalMatrix \
    gl_ModelViewMatrixInverse gl_ProjectionMatrixInverse gl_FogFragCoord gl_FragData gl_Fog";
/// Built-ins GLSL 450 has under the same name.
const KEPT: &str =
    "gl_Position gl_FragCoord gl_FrontFacing gl_PointCoord gl_FragDepth gl_PointSize";
/// Words GLSL 450 reserves that GLSL 120 left free: any use is the pack's own name.
const KEYWORDS: &str = "\
    sample smooth flat noperspective centroid patch subroutine invariant precise common \
    partition active filter input output resource buffer shared coherent volatile restrict \
    readonly writeonly atomic_uint layout superp";
/// GLSL 450 built-in functions that GLSL 120 did not have: a pack may define its own.
const NEW_BUILTINS: &str = "\
    round roundEven trunc texture textureLod textureSize texelFetch textureGrad textureProj \
    textureOffset isnan isinf fma saturate sinh cosh tanh asinh acosh atanh modf determinant \
    inverse transpose outerProduct bitCount findLSB findMSB ldexp frexp floatBitsToInt \
    intBitsToFloat";
const QUALIFIERS: &str = "\
    attribute varying uniform in out flat smooth noperspective centroid invariant const \
    highp mediump lowp";
const PRECISION: &str = "highp mediump lowp";

/// Whether `word` is one of the space-separated words of `list`.
fn listed(list: &str, word: &str) -> bool {
    list.split_whitespace().any(|w| w == word)
}

/// Built-ins with `out` parameters that naga's GLSL front end lacks, as
/// `hz_` functions: (name, its overloads).
const POLYFILLS: [(&str, &str); 2] = [
    (
        "modf",
        "float hz_modf(float x, out float i) { i = trunc(x); return x - i; }
vec2 hz_modf(vec2 x, out vec2 i) { i = trunc(x); return x - i; }
vec3 hz_modf(vec3 x, out vec3 i) { i = trunc(x); return x - i; }
vec4 hz_modf(vec4 x, out vec4 i) { i = trunc(x); return x - i; }",
    ),
    (
        "frexp",
        "float hz_frexp(float x, out int e) { e = x == 0.0 ? 0 : int(floor(log2(abs(x)))) + 1; return x * exp2(float(-e)); }
vec2 hz_frexp(vec2 x, out ivec2 e) { float a = hz_frexp(x.x, e.x); return vec2(a, hz_frexp(x.y, e.y)); }
vec3 hz_frexp(vec3 x, out ivec3 e) { float a = hz_frexp(x.x, e.x); float b = hz_frexp(x.y, e.y); return vec3(a, b, hz_frexp(x.z, e.z)); }
vec4 hz_frexp(vec4 x, out ivec4 e) { float a = hz_frexp(x.x, e.x); float b = hz_frexp(x.y, e.y); float c = hz_frexp(x.z, e.z); return vec4(a, b, c, hz_frexp(x.w, e.w)); }",
    ),
];

/// Calls of [`POLYFILLS`] renamed to their `hz_` versions, with the
/// definitions to put ahead of the body; None when the stage calls none (or
/// defines its own).
fn polyfill(text: &str) -> Option<(String, BTreeSet<String>)> {
    let toks = tokenize(text);
    let sig: Vec<&Tok> = toks.iter().filter(|t| t.kind != Kind::Space).collect();
    let called = |name: &str| {
        let calls = sig
            .windows(2)
            .any(|w| w[0].text == name && w[1].text == "(");
        // `float modf(`… is the pack's own function.
        let defined = sig.windows(3).any(|w| {
            w[1].text == name
                && w[2].text == "("
                && w[0].kind == Kind::Ident
                && is_type(w[0].text, &BTreeSet::new())
        });
        calls && !defined
    };
    let used: Vec<&(&str, &str)> = POLYFILLS.iter().filter(|(n, _)| called(n)).collect();
    if used.is_empty() {
        return None;
    }
    let out = toks
        .iter()
        .map(|t| match used.iter().find(|(n, _)| *n == t.text) {
            Some((n, _)) if t.kind == Kind::Ident => format!("hz_{n}"),
            _ => t.text.to_string(),
        })
        .collect();
    Some((out, used.iter().map(|(_, defs)| defs.to_string()).collect()))
}

mod dead;
mod decl;
mod emit;
mod fixes;
mod lex;
mod mono;
mod scope;
#[cfg(test)]
mod tests;

use lex::{Kind, Tok, drop_unreachable, is_type, tokenize, top_level_items};

struct Ctx<'a> {
    toks: Vec<Tok<'a>>,
    stage: Stage,
    out: Rewritten,
    /// Recorded samplers and whether each is a `sampler*Shadow`.
    sampler_shadow: BTreeMap<String, bool>,
    /// Fragment `out vec4 name;` → its `hz_gl_FragData` index.
    frag_outs: BTreeMap<String, usize>,
    /// Pack-defined names that collide with GLSL 450 built-in functions.
    user_builtins: BTreeSet<String>,
    /// Declarations the body needs ahead of it (dummy globals, helpers), deduplicated.
    prelude: BTreeSet<String>,
}

/// Rewrites one preprocessed stage. See the module doc and the spec, section 3.
pub fn rewrite(source: &str, stage: Stage) -> Result<Rewritten, String> {
    let mono = mono::monomorphise(source)?;
    let source = mono.as_deref().unwrap_or(source);
    let toks = dead::drop_dead_functions(drop_unreachable(tokenize(source)))?;
    // Token passes that need new text: each yields the stage again, tokenized anew.
    let scoped = scope::rename_shadowing(&toks)?;
    let sized = fixes::size_arrays(&tokenize(&scoped));
    let fixed = fixes::apply(&tokenize(sized.as_deref().unwrap_or(&scoped)))?;
    let (polyfilled, polyfills) =
        polyfill(&fixed).map_or((None, BTreeSet::new()), |(t, p)| (Some(t), p));
    let toks = tokenize(polyfilled.as_deref().unwrap_or(&fixed));
    let structs: BTreeSet<String> = toks
        .windows(3)
        .filter(|w| w[0].text == "struct" && w[2].kind == Kind::Ident)
        .map(|w| w[2].text.to_string())
        .collect();
    // A built-in name preceded by a type is a pack declaration (function, variable, parameter).
    let mut user_builtins = BTreeSet::new();
    let mut prev: Option<&str> = None;
    for t in toks.iter().filter(|t| t.kind != Kind::Space) {
        if t.kind == Kind::Ident
            && listed(NEW_BUILTINS, t.text)
            && prev.is_some_and(|p| is_type(p, &structs))
        {
            user_builtins.insert(t.text.to_string());
        }
        prev = Some(t.text);
    }
    let mut cx = Ctx {
        toks,
        stage,
        out: Rewritten::default(),
        sampler_shadow: BTreeMap::new(),
        frag_outs: BTreeMap::new(),
        user_builtins,
        prelude: polyfills,
    };
    let mut body = String::new();
    for (start, end) in top_level_items(&cx.toks)? {
        let sig: Vec<usize> = (start..end)
            .filter(|&i| cx.toks[i].kind != Kind::Space)
            .collect();
        let first = sig.first().map(|&i| cx.toks[i].text);
        let is_block = sig.iter().any(|&i| cx.toks[i].text == "{");
        if first == Some("precision") {
            continue;
        }
        if is_block
            && first.is_some_and(|w| matches!(w, "in" | "out" | "uniform" | "buffer" | "layout"))
        {
            return Err(format!("interface block `{}`", cx.text(sig[1])));
        }
        if !is_block && cx.declaration(&sig)? {
            continue;
        }
        cx.emit(start, end, &mut body)?;
    }
    let mut prelude: String = cx.prelude.iter().map(|l| format!("{l}\n")).collect();
    prelude.push_str(&body);
    cx.out.body = prelude;
    Ok(cx.out)
}
