//! Locals and parameters that shadow a global name the rewriter (or the
//! host) treats specially: a sampler (`vec4 tex = texture2D(tex, uv)`), a
//! uniform, a varying or attribute, a pack function or a GLSL built-in
//! function. Each is renamed `hz_l_<name>` within its scope, so the later
//! passes, which rename by name alone, only ever see the global.
//!
//! The walk follows GLSL scoping: parameters cover the function body, `{}`
//! opens a block, a `for (…)` header opens a scope covering its body, and a
//! declared name becomes visible after its own initializer.

use std::collections::{BTreeMap, BTreeSet};

use super::dead::{Item, classify};
use super::lex::{Kind, Tok, is_type, top_level_items};
use super::{NEW_BUILTINS, listed};

/// GLSL built-in functions a pack's local may hide (GLSL 120 and the later
/// ones in `NEW_BUILTINS`).
pub(super) const BUILTIN_FUNCTIONS: &str = "\
    radians degrees sin cos tan asin acos atan pow exp log exp2 log2 sqrt inversesqrt abs \
    sign floor ceil fract mod min max clamp mix step smoothstep length distance dot cross \
    normalize faceforward reflect refract matrixCompMult lessThan lessThanEqual greaterThan \
    greaterThanEqual equal notEqual any all not dFdx dFdy fwidth texture1D texture2D texture3D \
    textureCube shadow1D shadow2D texture2DLod texture2DProj texture3DLod shadow2DLod \
    texture2DGrad texture2DGradARB texelFetch2D textureSize2D noise1 noise2 noise3 noise4";

/// The prefix of a renamed local.
const PREFIX: &str = "hz_l_";

/// Whether `name` would be confused with a global the later passes rewrite.
struct Protected {
    names: BTreeSet<String>,
}

impl Protected {
    fn new(toks: &[Tok], items: &[(usize, usize)], kinds: &[Item]) -> Self {
        let mut names = BTreeSet::new();
        for (&(a, b), kind) in items.iter().zip(kinds) {
            match kind {
                Item::Function(name) | Item::Prototype(name) => {
                    names.insert(name.clone());
                }
                Item::Other => {
                    let words = || {
                        toks[a..b]
                            .iter()
                            .filter(|t| t.kind == Kind::Ident)
                            .map(|t| t.text)
                    };
                    // Interface declarations: every identifier in them (types and
                    // qualifiers too, which no local can be named anyway).
                    let interface = words()
                        .any(|w| matches!(w, "uniform" | "varying" | "attribute" | "in" | "out"));
                    if interface {
                        names.extend(words().map(str::to_string));
                    }
                }
                Item::Empty => {}
            }
        }
        Protected { names }
    }

    fn contains(&self, name: &str) -> bool {
        !name.starts_with("gl_")
            && (self.names.contains(name)
                || listed(BUILTIN_FUNCTIONS, name)
                || listed(NEW_BUILTINS, name))
    }
}

/// A `for (…)` statement's scope, popped when its body ends.
struct ForScope {
    /// Brace depth of the `for` itself.
    braces: usize,
    /// Paren depth inside its header.
    parens: usize,
    /// `None` while in the header; then whether the body is a `{}` block.
    block: Option<bool>,
}

/// A declaration being read (`T a = …, b;`).
struct Decl<'a> {
    parens: usize,
    /// The declarator whose initializer is being read: visible after it.
    pending: Option<(&'a str, String)>,
    /// The next identifier is a declared name.
    expect_name: bool,
}

/// Renames locals and parameters that shadow protected globals; returns the
/// stage's text with the renames applied (everything else verbatim).
pub(super) fn rename_shadowing(toks: &[Tok]) -> Result<String, String> {
    let items = top_level_items(toks)?;
    let kinds: Vec<Item> = items.iter().map(|&r| classify(toks, r)).collect();
    let protected = Protected::new(toks, &items, &kinds);
    let structs: BTreeSet<String> = toks
        .iter()
        .filter(|t| t.kind != Kind::Space)
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|w| w[0].text == "struct" && w[1].kind == Kind::Ident)
        .map(|w| w[1].text.to_string())
        .collect();
    let mut out = String::new();
    for (&(a, b), kind) in items.iter().zip(&kinds) {
        if matches!(kind, Item::Function(_)) {
            function(&toks[a..b], &protected, &structs, &mut out);
        } else {
            toks[a..b].iter().for_each(|t| out.push_str(t.text));
        }
    }
    Ok(out)
}

/// Makes the declarator just read visible in the innermost scope.
fn commit<'a>(decl: &mut Decl<'a>, scopes: &mut [BTreeMap<&'a str, String>]) {
    if let Some((name, new)) = decl.pending.take()
        && let Some(scope) = scopes.last_mut()
    {
        scope.insert(name, new);
    }
}

fn is_sampler(ty: &str) -> bool {
    ty.trim_start_matches(['i', 'u']).starts_with("sampler")
}

/// Copies one function definition with its shadowing locals renamed.
fn function<'a>(
    toks: &[Tok<'a>],
    protected: &Protected,
    structs: &BTreeSet<String>,
    out: &mut String,
) {
    let sig: Vec<usize> = (0..toks.len())
        .filter(|&i| toks[i].kind != Kind::Space)
        .collect();
    let next_sig = |i: usize| sig.iter().copied().find(|&k| k > i);
    let mut scopes: Vec<BTreeMap<&'a str, String>> = vec![BTreeMap::new()];
    // Parameters: `(` … `)` of the header, split at top-level commas.
    let open = sig.iter().copied().find(|&i| toks[i].text == "(");
    let mut body_start = 0;
    if let Some(open) = open {
        let (mut depth, mut param) = (0, Vec::new());
        for &i in sig.iter().filter(|&&i| i >= open) {
            let text = toks[i].text;
            match text {
                "(" => depth += 1,
                ")" => depth -= 1,
                _ => {}
            }
            if (depth == 1 && text == ",") || depth == 0 {
                let words: Vec<&str> = param.iter().map(|&k: &usize| toks[k].text).collect();
                let before_array = words.iter().position(|w| *w == "[").unwrap_or(words.len());
                let name = words[..before_array]
                    .iter()
                    .rev()
                    .find(|w| w.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_'));
                if let Some(&name) = name
                    && words.len() >= 2
                    && !words.iter().any(|w| is_sampler(w))
                    && protected.contains(name)
                {
                    scopes[0].insert(name, format!("{PREFIX}{name}"));
                }
                param.clear();
                if depth == 0 {
                    body_start = i + 1;
                    break;
                }
            } else if !(depth == 1 && text == "(") {
                param.push(i);
            }
        }
    }
    // The function's own name may be one of them (`vec3 f(vec3 f)`): rename only parameters.
    let params_from = open.unwrap_or(body_start);
    toks[..body_start].iter().enumerate().for_each(|(i, t)| {
        let renamed = (t.kind == Kind::Ident && i > params_from)
            .then(|| scopes[0].get(t.text))
            .flatten();
        out.push_str(renamed.map_or(t.text, String::as_str));
    });

    let (mut braces, mut parens) = (0usize, 0usize);
    let mut fors: Vec<ForScope> = Vec::new();
    let mut decl: Option<Decl> = None;
    let mut prev = "";
    for i in body_start..toks.len() {
        let t = toks[i];
        if t.kind == Kind::Space {
            out.push_str(t.text);
            continue;
        }
        let text = t.text;
        let next = next_sig(i).map(|k| toks[k].text);
        let mut emitted = None;
        match text {
            "{" => {
                braces += 1;
                scopes.push(BTreeMap::new());
            }
            "}" => {
                braces = braces.saturating_sub(1);
                if scopes.len() > 1 {
                    scopes.pop();
                }
                while fors
                    .last()
                    .is_some_and(|f| f.block == Some(true) && f.braces == braces)
                {
                    fors.pop();
                    scopes.pop();
                }
            }
            "(" => {
                parens += 1;
                if prev == "for" {
                    scopes.push(BTreeMap::new());
                    fors.push(ForScope {
                        braces,
                        parens,
                        block: None,
                    });
                }
            }
            ")" => {
                if let Some(d) = decl.as_mut()
                    && parens <= d.parens
                {
                    commit(d, &mut scopes);
                    decl = None;
                }
                if let Some(f) = fors.last_mut()
                    && f.block.is_none()
                    && f.parens == parens
                {
                    f.block = Some(next == Some("{"));
                }
                parens = parens.saturating_sub(1);
            }
            "," | ";" => {
                if let Some(d) = decl.as_mut()
                    && d.parens == parens
                {
                    commit(d, &mut scopes);
                    if text == "," {
                        d.expect_name = true;
                    } else {
                        decl = None;
                    }
                }
                if text == ";" && parens == 0 {
                    while fors
                        .last()
                        .is_some_and(|f| f.block == Some(false) && f.braces == braces)
                    {
                        fors.pop();
                        scopes.pop();
                    }
                }
            }
            _ if t.kind == Kind::Ident && prev != "." => {
                let declared = decl.as_ref().is_some_and(|d| d.expect_name);
                if declared {
                    let d = decl.as_mut().expect("checked above");
                    d.expect_name = false;
                    if protected.contains(text) {
                        let new = format!("{PREFIX}{text}");
                        emitted = Some(new.clone());
                        d.pending = Some((text, new));
                    }
                } else if is_type(text, structs)
                    && !is_sampler(text)
                    && next.is_some_and(|n| {
                        n.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                    })
                {
                    decl = Some(Decl {
                        parens,
                        pending: None,
                        expect_name: true,
                    });
                } else if next != Some("(") {
                    emitted = scopes.iter().rev().find_map(|s| s.get(text)).cloned();
                }
            }
            _ => {}
        }
        out.push_str(emitted.as_deref().unwrap_or(text));
        prev = text;
    }
}
