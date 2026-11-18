//! Legacy GLSL that drivers accept and naga does not:
//!
//! - `const` on parameters (`const in float x` is a parse error) and on
//!   locals (initialised from parameters or other run-time values): stripped
//!   inside every function.
//! - A `return e;` whose type differs from the function's (`return 1;` in a
//!   `float` function): wrapped as `return T(e);` for value-returning
//!   functions of built-in type.
//! - Globals initialised from run-time values (uniforms, varyings, pack
//!   functions, other non-constant globals): naga rejects them when `const`
//!   and silently drops the initializer otherwise. Both lose the initializer
//!   (and `const`) and get assigned, in source order, at the start of `main`.

use std::collections::BTreeSet;

use super::dead::{Item, classify};
use super::lex::{Kind, Tok, is_type, top_level_items};
use super::scope::BUILTIN_FUNCTIONS;
use super::{NEW_BUILTINS, QUALIFIERS, listed};

/// Return types worth coercing to: the built-in scalars, vectors and matrices.
fn is_value_type(ty: &str) -> bool {
    let base = ty.trim_start_matches(['i', 'u', 'b', 'd']);
    matches!(ty, "float" | "int" | "uint" | "bool" | "double")
        || (["vec", "mat"].iter().any(|p| base.starts_with(p)))
}

fn is_ident(w: &str) -> bool {
    w.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
}

/// Applies the fixes in the module doc; returns the stage's text.
pub(super) fn apply(toks: &[Tok]) -> Result<String, String> {
    let items = top_level_items(toks)?;
    let kinds: Vec<Item> = items.iter().map(|&r| classify(toks, r)).collect();
    let structs: BTreeSet<&str> = toks
        .iter()
        .filter(|t| t.kind != Kind::Space)
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|w| w[0].text == "struct" && w[1].kind == Kind::Ident)
        .map(|w| w[1].text)
        .collect();
    let structs_owned: BTreeSet<String> = structs.iter().map(|s| s.to_string()).collect();
    // Names whose value is known when the shader is compiled: global consts kept as such.
    let mut constants: BTreeSet<&str> = BTreeSet::new();
    let mut prologue = String::new();
    let mut texts: Vec<String> = Vec::with_capacity(items.len());
    for (&(a, b), kind) in items.iter().zip(&kinds) {
        let item = &toks[a..b];
        let text = match kind {
            Item::Function(_) => function(item),
            Item::Other => global(item, &structs_owned, &mut constants, &mut prologue)?,
            _ => item.iter().map(|t| t.text).collect(),
        };
        texts.push(text);
    }
    if !prologue.is_empty() {
        let main = kinds
            .iter()
            .position(|k| matches!(k, Item::Function(n) if n == "main"))
            .ok_or("globals initialised at run time but no `main`")?;
        let text = &mut texts[main];
        let brace = text.find('{').ok_or("`main` without a body")?;
        text.insert_str(brace + 1, &prologue);
    }
    Ok(texts.concat())
}

/// One function definition: `const` stripped, returns coerced.
fn function(toks: &[Tok]) -> String {
    let sig: Vec<usize> = (0..toks.len())
        .filter(|&i| toks[i].kind != Kind::Space)
        .collect();
    let open = sig.iter().position(|&i| toks[i].text == "(");
    let ret = open
        .filter(|&o| o >= 2)
        .map(|o| toks[sig[o - 2]].text)
        .filter(|ty| is_value_type(ty));
    let mut out = String::new();
    // Paren depth of the open `return T(`s still waiting for their `;`.
    let mut returns: Vec<i32> = Vec::new();
    let mut parens = 0i32;
    let mut skip_space = false;
    for t in toks {
        if skip_space && t.kind == Kind::Space {
            skip_space = false;
            continue;
        }
        skip_space = false;
        match t.text {
            "const" => {
                skip_space = true;
                continue;
            }
            "(" => parens += 1,
            ")" => parens -= 1,
            ";" if returns.last() == Some(&parens) => {
                returns.pop();
                out.push(')');
            }
            _ => {}
        }
        out.push_str(t.text);
        if t.text == "return"
            && let Some(ty) = ret
        {
            out.push(' ');
            out.push_str(ty);
            out.push('(');
            returns.push(parens);
        }
    }
    // `return;` in a value function (invalid anyway) would have become `return T();`.
    out.replace(&format!("return {}();", ret.unwrap_or("")), "return;")
}

/// One top-level item other than a function: global variables with run-time
/// initializers lose them to `prologue`.
fn global<'a>(
    toks: &[Tok<'a>],
    structs: &BTreeSet<String>,
    constants: &mut BTreeSet<&'a str>,
    prologue: &mut String,
) -> Result<String, String> {
    let verbatim = || toks.iter().map(|t| t.text).collect::<String>();
    let sig: Vec<&'a str> = toks
        .iter()
        .filter(|t| t.kind != Kind::Space)
        .map(|t| t.text)
        .collect();
    let quals: Vec<&str> = sig
        .iter()
        .copied()
        .take_while(|w| listed(QUALIFIERS, w))
        .collect();
    let interface = quals
        .iter()
        .any(|q| matches!(*q, "attribute" | "varying" | "uniform" | "in" | "out"));
    let Some(&ty) = sig.get(quals.len()) else {
        return Ok(verbatim());
    };
    if interface || sig.first() == Some(&"struct") || !is_type(ty, structs) {
        return Ok(verbatim());
    }
    let is_const = quals.contains(&"const");
    // Declarators: `name [N] = init`, split at depth-0 commas (before the final `;`).
    let body = &sig[quals.len() + 1..sig.len().saturating_sub(1)];
    let mut decls: Vec<&[&str]> = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    for (k, w) in body.iter().enumerate() {
        match *w {
            "(" | "[" => depth += 1,
            ")" | "]" => depth -= 1,
            "," if depth == 0 => {
                decls.push(&body[start..k]);
                start = k + 1;
            }
            _ => {}
        }
    }
    decls.push(&body[start..]);
    let runtime = |init: &[&str], constants: &BTreeSet<&str>| {
        init.iter().enumerate().any(|(k, w)| {
            is_ident(w)
                && !(k > 0 && init[k - 1] == ".")
                && !is_type(w, structs)
                && !matches!(*w, "true" | "false")
                && !listed(BUILTIN_FUNCTIONS, w)
                && !listed(NEW_BUILTINS, w)
                && !constants.contains(w)
        })
    };
    let mut moved = false;
    let mut kept = Vec::new();
    for d in &decls {
        let Some(&name) = d.first() else {
            return Ok(verbatim());
        };
        let eq = d.iter().position(|w| *w == "=");
        let init = eq.map_or(&[][..], |e| &d[e + 1..]);
        if eq.is_some() && runtime(init, constants) {
            moved = true;
            prologue.push_str(&format!(" {name} = {};", init.join(" ")));
            kept.push(d[..eq.unwrap_or(d.len())].join(" "));
        } else {
            if is_const {
                constants.insert(name);
            }
            kept.push(d.join(" "));
        }
    }
    if !moved {
        return Ok(verbatim());
    }
    if moved && is_const {
        // A const keeps all its declarators constant or none: drop the ones we kept.
        for d in &decls {
            if let Some(name) = d.first() {
                constants.remove(name);
            }
        }
    }
    // Leading whitespace (and comments-free layout) of the item is kept; the rest is rebuilt.
    let lead: String = toks
        .iter()
        .take_while(|t| t.kind == Kind::Space)
        .map(|t| t.text)
        .collect();
    let quals: Vec<&str> = quals.into_iter().filter(|q| *q != "const").collect();
    let mut text = lead;
    for q in quals {
        text.push_str(q);
        text.push(' ');
    }
    text.push_str(ty);
    text.push(' ');
    text.push_str(&kept.join(", "));
    text.push(';');
    Ok(text)
}
