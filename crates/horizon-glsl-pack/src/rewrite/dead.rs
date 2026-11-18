//! Dead-function elimination: packs carry libraries of helpers that a given
//! program never calls, and some of those would not compile here (sampler
//! parameters, names naga rejects). Only what `main` reaches is kept, before
//! any other check runs. Stray top-level `;` go too.

use std::collections::{BTreeMap, BTreeSet};

use super::lex::{Kind, Tok, top_level_items};

/// What a top-level item is, for reachability.
pub(super) enum Item {
    /// A function definition, by name (overloads share it).
    Function(String),
    /// A prototype (`float f(float);`).
    Prototype(String),
    /// A lone `;`, or only whitespace.
    Empty,
    /// Anything else (declarations, structs): always kept, and a root.
    Other,
}

pub(super) fn classify(toks: &[Tok], range: (usize, usize)) -> Item {
    let sig: Vec<&str> = toks[range.0..range.1]
        .iter()
        .filter(|t| t.kind != Kind::Space)
        .map(|t| t.text)
        .collect();
    if sig.is_empty() || sig == [";"] {
        return Item::Empty;
    }
    let Some(open) = sig.iter().position(|&w| w == "(") else {
        return Item::Other;
    };
    // `type name (`: at least a return type and a name, no `=`, no interface qualifier.
    let head = &sig[..open];
    let is_ident = |w: &str| w.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_');
    let qualified = head.iter().any(|w| {
        matches!(
            *w,
            "=" | "layout" | "uniform" | "in" | "out" | "buffer" | "struct"
        )
    });
    if head.len() < 2 || qualified || !head.iter().all(|w| is_ident(w)) {
        return Item::Other;
    }
    // After the parameter list's `)`: a body or the prototype's `;`.
    let mut depth = 0;
    let close = (open..sig.len()).find(|&k| {
        match sig[k] {
            "(" => depth += 1,
            ")" => depth -= 1,
            _ => {}
        }
        depth == 0
    });
    let name = head[head.len() - 1].to_string();
    match close.and_then(|c| sig.get(c + 1)) {
        Some(&"{") => Item::Function(name),
        Some(&";") => Item::Prototype(name),
        _ => Item::Other,
    }
}

/// Drops the functions `main` does not reach, directly or through other
/// functions or global initializers, prototypes, and stray top-level `;`.
pub(super) fn drop_dead_functions(toks: Vec<Tok<'_>>) -> Result<Vec<Tok<'_>>, String> {
    let items = top_level_items(&toks)?;
    let kinds: Vec<Item> = items.iter().map(|&r| classify(&toks, r)).collect();
    let idents = |(a, b): (usize, usize)| {
        toks[a..b]
            .iter()
            .filter(|t| t.kind == Kind::Ident)
            .map(|t| t.text)
    };
    // Name → the identifiers its definitions (all overloads) mention.
    let mut uses: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut work: Vec<&str> = vec!["main"];
    for (&range, kind) in items.iter().zip(&kinds) {
        match kind {
            Item::Function(name) => {
                let set = uses.entry(name.as_str()).or_default();
                set.extend(idents(range).filter(|&w| w != name));
            }
            Item::Other => work.extend(idents(range)),
            Item::Empty | Item::Prototype(_) => {}
        }
    }
    let mut live: BTreeSet<&str> = BTreeSet::new();
    while let Some(name) = work.pop() {
        if let Some(called) = uses.get(name)
            && live.insert(name)
        {
            work.extend(called.iter().copied());
        }
    }
    let mut keep = vec![true; toks.len()];
    for (&(a, b), kind) in items.iter().zip(&kinds) {
        let dead = match kind {
            Item::Function(name) => !live.contains(name.as_str()),
            // naga's front end mishandles a prototype whose body calls a function
            // defined after it ("depends on … which has not been processed yet"):
            // drop it when the definition exists. A call before the definition stays
            // an error, which no pack seen so far needs.
            Item::Prototype(name) => uses.contains_key(name.as_str()),
            // Keep whitespace-only tails; drop the `;` itself.
            Item::Empty => toks[a..b].iter().any(|t| t.kind != Kind::Space),
            Item::Other => false,
        };
        if dead {
            keep[a..b].iter_mut().for_each(|k| *k = false);
        }
    }
    Ok(toks
        .into_iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(t, _)| t)
        .collect())
}
