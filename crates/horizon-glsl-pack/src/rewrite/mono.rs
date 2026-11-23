//! Monomorphisation of functions that take samplers.
//!
//! GLSL 120 lets a function take `sampler2D tex` and sample it; the GLSL 450
//! the host writes has no sampler values (textures and samplers are separate
//! bindings, and the rewriter turns every sampling call into a host function
//! named after a global sampler). So each such function is cloned once per
//! distinct tuple of global samplers its call sites pass, named
//! `<f>_hz_<sampler>`, with the sampler parameters removed and their uses
//! replaced by the global name. Clones of a function that forwards its sampler
//! parameter to another such function are found transitively (a worklist runs
//! to a fixed point). Only call sites that exist produce clones: the generic
//! original is always dropped, so an unused one costs nothing.

use std::collections::{BTreeMap, BTreeSet};

use super::lex::{Kind, Tok, tokenize, top_level_items};

fn is_sampler_type(word: &str) -> bool {
    word.trim_start_matches(['i', 'u']).starts_with("sampler")
}

/// One parameter of a function with sampler parameters.
struct Param {
    /// Token range, without the separating commas.
    range: (usize, usize),
    name: String,
    /// The sampler type (`sampler2DShadow`), for a sampler parameter.
    sampler: Option<String>,
}

/// One definition (or prototype) of a function that has a sampler parameter.
struct Generic {
    /// Token range of the whole top-level item.
    item: (usize, usize),
    name_tok: usize,
    params: Vec<Param>,
    /// Token range from after the `)` to the item's end; `None` for a prototype.
    body: Option<(usize, usize)>,
}

impl Generic {
    fn sampler_positions(&self) -> Vec<usize> {
        (0..self.params.len())
            .filter(|&k| self.params[k].sampler.is_some())
            .collect()
    }
}

/// A clone: function name, argument count and the global sampler per sampler parameter.
type Key = (String, usize, Vec<String>);

fn clone_name(key: &Key) -> String {
    let mut name = key.0.clone();
    for s in &key.2 {
        name.push_str("_hz_");
        name.push_str(s);
    }
    name
}

struct Mono<'a> {
    toks: Vec<Tok<'a>>,
    /// Samplers declared by `uniform sampler… name;` at the top level, with their type.
    globals: BTreeMap<String, String>,
    generics: BTreeMap<String, Vec<Generic>>,
    /// Functions defined with no sampler parameter: a call to an overloaded
    /// name whose sampler position holds no sampler is one of these.
    plain: BTreeSet<String>,
}

impl Mono<'_> {
    fn text(&self, i: usize) -> &str {
        self.toks[i].text
    }

    fn sig(&self, a: usize, b: usize) -> Vec<usize> {
        (a..b)
            .filter(|&i| self.toks[i].kind != Kind::Space)
            .collect()
    }

    /// Argument ranges of the parenthesised list opening at `open`, and the index after its `)`.
    fn args(&self, open: usize, end: usize) -> Result<(Vec<(usize, usize)>, usize), String> {
        let (mut args, mut depth, mut start) = (Vec::new(), 0, open + 1);
        for i in open..end {
            match self.text(i) {
                "(" | "[" | "{" => depth += 1,
                "," if depth == 1 => {
                    args.push((start, i));
                    start = i + 1;
                }
                ")" | "]" | "}" => {
                    depth -= 1;
                    if depth == 0 {
                        args.push((start, i));
                        // `f()` has no arguments rather than one empty one.
                        if args.len() == 1 && self.sig(start, i).is_empty() {
                            args.clear();
                        }
                        return Ok((args, i + 1));
                    }
                }
                _ => {}
            }
        }
        Err("unclosed parenthesis".into())
    }

    /// Parses a top-level item as a function definition or prototype with a
    /// sampler parameter.
    fn generic(&self, (start, end): (usize, usize)) -> Result<Option<Generic>, String> {
        let sig = self.sig(start, end);
        let Some(p) = sig.iter().position(|&i| self.text(i) == "(") else {
            return Ok(None);
        };
        if p < 2
            || sig[..p]
                .iter()
                .any(|&i| matches!(self.text(i), "=" | "uniform" | "struct"))
            || self.toks[sig[p - 1]].kind != Kind::Ident
            || self.toks[sig[p - 2]].kind != Kind::Ident
        {
            return Ok(None);
        }
        let (ranges, after) = self.args(sig[p], end)?;
        let next = self.sig(after, end);
        let body = match next.first().map(|&i| self.text(i)) {
            Some("{") => Some((after, end)),
            Some(";") => None,
            _ => return Ok(None),
        };
        let mut params = Vec::new();
        for (a, b) in ranges {
            let words = self.sig(a, b);
            if words.len() == 1 && self.text(words[0]) == "void" {
                continue;
            }
            let sampler = words
                .iter()
                .map(|&i| self.text(i))
                .find(|w| is_sampler_type(w))
                .map(str::to_string);
            // The last identifier before any array suffix: `in sampler2D tex`, `float w[2]`.
            let before = words
                .iter()
                .position(|&i| self.text(i) == "[")
                .unwrap_or(words.len());
            let name = words[..before]
                .iter()
                .rev()
                .map(|&i| self.toks[i])
                .find(|t| t.kind == Kind::Ident)
                .map(|t| t.text.to_string())
                .unwrap_or_default();
            if sampler.is_some() && name.is_empty() {
                return Err(format!(
                    "unnamed sampler parameter of `{}`",
                    self.text(sig[p - 1])
                ));
            }
            params.push(Param {
                range: (a, b),
                name,
                sampler,
            });
        }
        if !params.iter().any(|q| q.sampler.is_some()) {
            return Ok(None);
        }
        Ok(Some(Generic {
            item: (start, end),
            name_tok: sig[p - 1],
            params,
            body,
        }))
    }

    /// Copies tokens `a..b` to `out`, replacing identifiers by `subst` and
    /// calls to generic functions by calls to their clones (recorded in `found`).
    fn walk(
        &self,
        (a, b): (usize, usize),
        subst: &BTreeMap<&str, &str>,
        out: &mut String,
        found: &mut Vec<Key>,
    ) -> Result<(), String> {
        let mut i = a;
        let mut after_dot = false;
        while i < b {
            let t = self.toks[i];
            i += 1;
            if t.kind != Kind::Ident {
                out.push_str(t.text);
                if t.kind != Kind::Space {
                    after_dot = t.text == ".";
                }
                continue;
            }
            if std::mem::take(&mut after_dot) {
                out.push_str(t.text);
                continue;
            }
            let open = self
                .sig(i, b)
                .first()
                .copied()
                .filter(|&o| self.text(o) == "(");
            if let (Some(overloads), Some(open)) = (self.generics.get(t.text), open) {
                let (args, after) = self.args(open, b)?;
                let n = args.len();
                let mut candidates = overloads.iter().filter(|g| g.params.len() == n);
                // A same-named function without sampler parameters is an ordinary call.
                if let Some(first) = candidates.next() {
                    let positions = first.sampler_positions();
                    if candidates.any(|g| g.sampler_positions() != positions) {
                        return Err(format!(
                            "overloads of `{}` take samplers at different positions",
                            t.text
                        ));
                    }
                    // `f(x, false)` beside `f(vec3, sampler2D)`: the plain overload.
                    let plain_call = self.plain.contains(t.text)
                        && positions.iter().any(|&k| {
                            let words = self.sig(args[k].0, args[k].1);
                            !matches!(words[..], [w] if {
                                let arg = self.text(w);
                                self.globals.contains_key(subst.get(arg).copied().unwrap_or(arg))
                            })
                        });
                    let mut samplers = Vec::new();
                    for &k in &positions {
                        if plain_call {
                            break;
                        }
                        let words = self.sig(args[k].0, args[k].1);
                        let arg = match words[..] {
                            [w] if self.toks[w].kind == Kind::Ident => self.text(w),
                            _ => {
                                return Err(format!(
                                    "sampler argument {} of `{}` is not a sampler name",
                                    k + 1,
                                    t.text
                                ));
                            }
                        };
                        let global = subst.get(arg).copied().unwrap_or(arg);
                        if !self.globals.contains_key(global) {
                            return Err(format!(
                                "`{arg}` passed to `{}` is not a global sampler",
                                t.text
                            ));
                        }
                        samplers.push(global.to_string());
                    }
                    if plain_call {
                        out.push_str(t.text);
                        continue;
                    }
                    let key = (t.text.to_string(), n, samplers);
                    out.push_str(&clone_name(&key));
                    out.push('(');
                    let kept = (0..n).filter(|k| !positions.contains(k));
                    for (m, k) in kept.enumerate() {
                        if m > 0 {
                            out.push(',');
                        }
                        self.walk(args[k], subst, out, found)?;
                    }
                    out.push(')');
                    found.push(key);
                    i = after;
                    continue;
                }
            }
            out.push_str(subst.get(t.text).copied().unwrap_or(t.text));
        }
        Ok(())
    }

    /// The overloads a clone instantiates: those of its argument count whose
    /// sampler parameters have the types of the globals passed
    /// (`f(sampler2DShadow, vec3)` beside `f(sampler2D, vec3)`), else all of them.
    fn overloads(&self, key: &Key) -> Vec<&Generic> {
        let all: Vec<&Generic> = self.generics[&key.0]
            .iter()
            .filter(|g| g.params.len() == key.1)
            .collect();
        let exact: Vec<&Generic> = all
            .iter()
            .copied()
            .filter(|g| {
                let types = g.params.iter().filter_map(|p| p.sampler.as_deref());
                types
                    .zip(&key.2)
                    .all(|(ty, global)| self.globals.get(global).map(String::as_str) == Some(ty))
            })
            .collect();
        if exact.is_empty() { all } else { exact }
    }

    /// Writes the clone of `g` for `key`.
    fn instantiate(
        &self,
        g: &Generic,
        key: &Key,
        out: &mut String,
        found: &mut Vec<Key>,
    ) -> Result<(), String> {
        let none = BTreeMap::new();
        self.walk((g.item.0, g.name_tok), &none, out, found)?;
        out.push_str(&clone_name(key));
        out.push('(');
        let mut subst = BTreeMap::new();
        let mut samplers = key.2.iter();
        let mut first = true;
        for p in &g.params {
            if p.sampler.is_some() {
                if let Some(s) = samplers.next() {
                    subst.insert(p.name.as_str(), s.as_str());
                }
                continue;
            }
            if !first {
                out.push(',');
            }
            first = false;
            self.walk(p.range, &none, out, found)?;
        }
        out.push(')');
        match g.body {
            Some(body) => self.walk(body, &subst, out, found)?,
            None => out.push(';'),
        }
        out.push('\n');
        Ok(())
    }
}

/// Rewrites `src` so that no function takes a sampler. Returns `None` when
/// none does (the source is then used as is).
pub(super) fn monomorphise(src: &str) -> Result<Option<String>, String> {
    let toks = tokenize(src);
    if !toks.iter().any(|t| is_sampler_type(t.text)) {
        return Ok(None);
    }
    let items = top_level_items(&toks)?;
    let mut mono = Mono {
        toks,
        globals: BTreeMap::new(),
        generics: BTreeMap::new(),
        plain: BTreeSet::new(),
    };
    let mut generic_items = BTreeSet::new();
    for &item in &items {
        let words: Vec<&str> = mono
            .sig(item.0, item.1)
            .iter()
            .map(|&i| mono.toks[i].text)
            .collect();
        if let Some(t) = words.iter().position(|w| is_sampler_type(w))
            && words[..t].contains(&"uniform")
        {
            // `uniform sampler2D a, b;`: every identifier directly before `,`, `;` or `[`.
            for w in words[t + 1..].windows(2) {
                if matches!(w[1], "," | ";" | "[") {
                    mono.globals.insert(w[0].to_string(), words[t].to_string());
                }
            }
        } else if let Some(g) = mono.generic(item)? {
            generic_items.insert(item.0);
            let name = mono.text(g.name_tok).to_string();
            mono.generics.entry(name).or_default().push(g);
        } else if let Some(p) = words.iter().position(|w| *w == "(")
            && p >= 2
            && words.contains(&"{")
            && !words[..p].iter().any(|w| matches!(*w, "=" | "struct"))
        {
            mono.plain.insert(words[p - 1].to_string());
        }
    }
    if mono.generics.is_empty() {
        return Ok(None);
    }
    // Clones needed by the ordinary code, then by the clones themselves.
    let none = BTreeMap::new();
    let mut found = Vec::new();
    let mut sink = String::new();
    for &item in items.iter().filter(|i| !generic_items.contains(&i.0)) {
        mono.walk(item, &none, &mut sink, &mut found)?;
    }
    let mut keys = BTreeSet::new();
    while let Some(key) = found.pop() {
        if keys.insert(key.clone()) {
            for g in mono.overloads(&key) {
                mono.instantiate(g, &key, &mut sink, &mut found)?;
            }
        }
    }
    let mut out = String::new();
    for &item in &items {
        if !generic_items.contains(&item.0) {
            mono.walk(item, &none, &mut out, &mut found)?;
            continue;
        }
        let name = mono.text(mono.sig(item.0, item.1)[0]).to_string();
        let g = mono
            .generics
            .values()
            .flatten()
            .find(|g| g.item == item)
            .ok_or_else(|| format!("lost function at `{name}`"))?;
        let fname = mono.text(g.name_tok);
        for key in keys
            .iter()
            .filter(|k| k.0 == fname && mono.overloads(k).iter().any(|o| std::ptr::eq(*o, g)))
        {
            mono.instantiate(g, key, &mut out, &mut found)?;
        }
    }
    Ok(Some(out))
}
