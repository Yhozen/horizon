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
//!   So do initializers with `?:` or built-ins naga cannot evaluate as
//!   constants (`mix`, `smoothstep`…).
//! - `discard;` with statements after it in the same block: wrapped in an
//!   `if (true)` so naga keeps those statements.

use std::collections::BTreeSet;

use super::dead::{Item, classify};
use super::lex::{Kind, Tok, is_type, top_level_items};
use super::scope::BUILTIN_FUNCTIONS;
use super::{NEW_BUILTINS, QUALIFIERS, listed};

/// Built-in functions naga's constant evaluator does not implement (naga 30).
const NOT_CONSTANT: &str = "mix smoothstep exp2 log2 determinant transpose inverse outerProduct \
    reflect refract faceforward ldexp frexp modf";

/// Return types worth coercing to: the built-in scalars, vectors and matrices.
fn is_value_type(ty: &str) -> bool {
    let base = ty.strip_prefix(['i', 'u', 'b', 'd']).unwrap_or(ty);
    let dims = |d: &str| {
        let b = d.as_bytes();
        let ok = |c: u8| (b'2'..=b'4').contains(&c);
        matches!(b, [c] if ok(*c)) || matches!(b, [c, b'x', r] if ok(*c) && ok(*r))
    };
    matches!(ty, "float" | "int" | "uint" | "bool" | "double")
        || base
            .strip_prefix("vec")
            .is_some_and(|d| d.len() == 1 && dims(d))
        || ty
            .strip_prefix("mat")
            .or_else(|| ty.strip_prefix("dmat"))
            .is_some_and(dims)
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
    if texts.iter().any(|t| t.contains(ZERO)) {
        texts.insert(0, format!("float {ZERO};\n"));
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

/// Whether the declaration in `toks` (up to its `;`) is initialised from
/// numbers and operators only (`const int n = 6;`, `const float k = 1.0 / 3.0;`).
fn literal_initializer(toks: &[Tok]) -> bool {
    let decl: Vec<&Tok> = toks
        .iter()
        .take_while(|t| t.text != ";")
        .filter(|t| t.kind != Kind::Space)
        .collect();
    let Some(eq) = decl.iter().position(|t| t.text == "=") else {
        return false;
    };
    !decl[eq + 1..].is_empty()
        && decl[..eq].iter().all(|t| t.text != "," && t.text != "(")
        && decl[eq + 1..]
            .iter()
            .all(|t| matches!(t.kind, Kind::Number | Kind::Punct) && t.text != "," && t.text != "?")
}

/// A float zero that naga cannot fold (a private global, zero-initialised).
const ZERO: &str = "hz_zero";

fn is_float_zero(literal: &str) -> bool {
    let digits = literal.trim_end_matches(['f', 'F']);
    digits.contains('.') && digits.chars().all(|c| c == '0' || c == '.')
}

/// What `discard;` becomes: naga's GLSL front end drops the statements after
/// a bare `discard` but keeps their expressions, which then fail validation
/// ("Expression not visited"); behind an `if` the rest of the block stays.
const DISCARD: &str = "{ if (true) discard; }";

/// One function definition: `const` stripped, returns coerced, `discard` guarded.
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
    let mut after_discard = false;
    let mut last = "";
    for (at, t) in toks.iter().enumerate() {
        if t.kind != Kind::Space {
            let prev = std::mem::replace(&mut last, t.text);
            // `x / 0.0`: naga folds it to an infinity and refuses it; GL gives the infinity at run time.
            if prev == "/" && t.kind == Kind::Number && is_float_zero(t.text) {
                out.push_str(ZERO);
                continue;
            }
        }
        if skip_space && t.kind == Kind::Space {
            skip_space = false;
            continue;
        }
        skip_space = false;
        if after_discard && t.kind != Kind::Space {
            after_discard = false;
            if t.text == ";" {
                continue;
            }
        }
        match t.text {
            "discard" => {
                out.push_str(DISCARD);
                after_discard = true;
                continue;
            }
            // Kept on a local whose value is a literal expression: it may size an array.
            "const" if literal_initializer(&toks[at + 1..]) => {}
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
    // naga's constant evaluator has no `?:` and lacks some built-ins: such
    // initializers run in `main` too.
    let runtime = |init: &[&str], constants: &BTreeSet<&str>| {
        init.contains(&"?")
            || init.iter().any(|w| listed(NOT_CONSTANT, w))
            || init.iter().enumerate().any(|(k, w)| {
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

#[cfg(test)]
mod tests {
    use super::super::lex::tokenize;
    use super::apply;

    #[test]
    fn moved_initializers_keep_their_operators() {
        let src = "uniform float u; uniform int k;\n\
                   float g = (u < 0.5 || u >= 2.0) && u != 1.0 ? 1.0 : 0.0;\n\
                   int h = (k >> 2) << 1; bool e = k == 3 && k <= 4;\n\
                   void main() { }\n";
        let out = apply(&tokenize(src)).unwrap();
        for op in ["||", ">=", "&&", "!=", ">>", "<<", "==", "<="] {
            assert!(out.contains(&format!(" {op} ")), "{op} in {out}");
        }
        assert!(out.contains("void main() { g = "), "{out}");
    }

    #[test]
    fn struct_returns_are_not_coerced() {
        let src = "struct materialLAB { float r; };\nmaterialLAB f() { materialLAB m; return m; }\n\
                   mat2x3 g() { return mat2x3(1.0); }\nvec3 h() { return 1.0; }\nvoid main() { }\n";
        let out = apply(&tokenize(src)).unwrap();
        assert!(out.contains("return m;"), "{out}");
        assert!(out.contains("return mat2x3( mat2x3(1.0));"), "{out}");
        assert!(out.contains("return vec3( 1.0);"), "{out}");
    }

    #[test]
    fn literal_local_constants_stay_constant() {
        let src = "float f(float x) { const int n = 6; const float k = 1.0 / 3.0; const float y = x * 2.0; \
                   float a[n]; return k + y + a[0]; }\nvoid main() { }\n";
        let out = apply(&tokenize(src)).unwrap();
        assert!(
            out.contains("const int n = 6; const float k = 1.0 / 3.0; float y = x * 2.0;"),
            "{out}"
        );
    }

    #[test]
    fn division_by_literal_zero_is_left_to_run_time() {
        let src = "void main() { gl_Position = vec4(-1.0 / 0.0, 1.0 / 0., 2.0 / 0.5, 1.0); }\n";
        let out = apply(&tokenize(src)).unwrap();
        assert!(out.starts_with("float hz_zero;\n"), "{out}");
        assert!(
            out.contains("-1.0 / hz_zero, 1.0 / hz_zero, 2.0 / 0.5"),
            "{out}"
        );
    }

    #[test]
    fn code_after_discard_is_kept() {
        let src = "uniform sampler2D tex; varying vec2 uv; uniform float a;\n\
                   void main() { if (a > 0.5) discard; else a; discard; gl_FragColor = texture2D(tex, uv); }\n";
        let out = apply(&tokenize(src)).unwrap();
        assert!(
            out.contains("if (a > 0.5) { if (true) discard; } else"),
            "{out}"
        );
        let rw = crate::rewrite::rewrite(src, crate::Stage::Fragment).unwrap();
        assert!(
            rw.body.contains("{ if (true) discard; } hz_gl_FragData"),
            "{}",
            rw.body
        );
    }

    #[test]
    fn ternary_constants_run_in_main() {
        let src = "const float a = 1.0 > 0.0 ? 1.2 : 0.0;\nconst float b = a * 2.0;\n\
                   const float c = 3.0;\nvoid main() { }\n";
        let out = apply(&tokenize(src)).unwrap();
        assert!(
            out.contains("void main() { a = 1.0 > 0.0 ? 1.2 : 0.0; b = a * 2.0;"),
            "{out}"
        );
        assert!(out.contains("const float c = 3.0;"), "{out}");
        assert!(!out.contains("const float a"), "{out}");
        let src = "const vec3 m = mix(vec3(0.0), vec3(1.0), 0.5);\nvoid main() { }\n";
        assert!(apply(&tokenize(src)).unwrap().contains("main() { m = mix"));
    }
}

/// Gives unsized arrays initialised by a constructor their length
/// (`vec3[] a = vec3[](x, y)` → `vec3[2] a = vec3[2](x, y)`, also
/// `T a[] = T[N](…)`): naga types them as run-time-sized and rejects them.
/// None when the stage has none.
pub(super) fn size_arrays(toks: &[Tok]) -> Option<String> {
    let s: Vec<usize> = (0..toks.len())
        .filter(|&i| toks[i].kind != Kind::Space)
        .collect();
    let t = |j: usize| s.get(j).map_or("", |&i| toks[i].text);
    // Token index of an empty `[` → the length to write in it.
    let mut fills: Vec<(usize, String)> = Vec::new();
    for k in 0..s.len() {
        let ctor = k + 1;
        if t(k) != "=" || !is_ident(t(ctor)) || t(ctor + 1) != "[" {
            continue;
        }
        // The constructor's length: none, a literal, or a constant's name.
        let (close, given) = if t(ctor + 2) == "]" {
            (ctor + 2, None)
        } else if t(ctor + 3) == "]" {
            (ctor + 3, Some(t(ctor + 2).to_string()))
        } else {
            continue;
        };
        if t(close + 1) != "(" {
            continue;
        }
        // The declaration: back to the previous statement's end.
        let start = (0..k)
            .rev()
            .find(|&h| matches!(t(h), ";" | "{" | "}"))
            .map_or(0, |h| h + 1);
        let holes: Vec<usize> = (start..k)
            .filter(|&h| t(h) == "[" && t(h + 1) == "]")
            .collect();
        if holes.is_empty() && given.is_some() {
            continue;
        }
        // Arguments: depth-0 commas inside the constructor's parentheses.
        let (mut depth, mut commas, mut empty) = (0i32, 0usize, true);
        for m in close + 1..s.len() {
            match t(m) {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                "," if depth == 1 => commas += 1,
                _ => {}
            }
            if m > close + 1 {
                empty = false;
            }
        }
        let len = given
            .clone()
            .unwrap_or_else(|| (if empty { 0 } else { commas + 1 }).to_string());
        fills.extend(holes.into_iter().map(|h| (s[h], len.clone())));
        if given.is_none() {
            fills.push((s[ctor + 1], len));
        }
    }
    if fills.is_empty() {
        return None;
    }
    let mut out = String::new();
    for (i, tok) in toks.iter().enumerate() {
        out.push_str(tok.text);
        if let Some((_, len)) = fills.iter().find(|(at, _)| *at == i) {
            out.push_str(len);
        }
    }
    Some(out)
}

#[cfg(test)]
mod array_tests {
    use super::super::lex::tokenize;
    use super::size_arrays;

    #[test]
    fn unsized_arrays_get_their_length() {
        let src = "const vec3[] a = vec3[](vec3(1.0), vec3(1.0, 0.3, 0.1));\n\
                   const vec2 b[] = vec2[3](vec2(0.0), vec2(1.0), vec2(2.0));\n\
                   float c[2] = float[](f(1.0, 2.0), 3.0);\n\
                   void main() { int d[] = int[](1, 2, 3, 4); }\n";
        let out = size_arrays(&tokenize(src)).unwrap();
        assert!(out.contains("const vec3[2] a = vec3[2]("), "{out}");
        assert!(out.contains("const vec2 b[3] = vec2[3]("), "{out}");
        assert!(
            out.contains("float c[2] = float[2](f(1.0, 2.0), 3.0)"),
            "{out}"
        );
        assert!(out.contains("int d[4] = int[4](1, 2, 3, 4)"), "{out}");
        assert_eq!(size_arrays(&tokenize("float x[4]; void main() {}")), None);
        let named = "const int n = 2; void main() { vec3[] c = vec3[n](vec3(0.0), vec3(1.0)); vec3[n] d = vec3[n](c[0], c[1]); }";
        let out = size_arrays(&tokenize(named)).unwrap();
        assert!(out.contains("vec3[n] c = vec3[n]("), "{out}");
        assert!(out.contains("vec3[n] d = vec3[n](c[0]"), "{out}");
    }
}

/// `m[i].x = v` → `m[i][0] = v`: naga's GLSL front end cannot assign to a
/// one-component swizzle of an indexed matrix column ("Can't lookup field on
/// this type"); the index form means the same for vectors too. None when
/// the stage has none.
pub(super) fn index_swizzle_stores(toks: &[Tok]) -> Option<String> {
    let s: Vec<usize> = (0..toks.len())
        .filter(|&i| toks[i].kind != Kind::Space)
        .collect();
    let t = |j: usize| s.get(j).map_or("", |&i| toks[i].text);
    let component = |w: &str| {
        ["xyzw", "rgba", "stpq"]
            .iter()
            .find_map(|set| set.find(w).filter(|_| w.len() == 1))
    };
    let mut swaps: Vec<(usize, usize, usize)> = Vec::new();
    for j in 1..s.len() {
        if t(j) == "."
            && t(j - 1) == "]"
            && matches!(t(j + 2), "=" | "+=" | "-=" | "*=" | "/=")
            && let Some(k) = component(t(j + 1))
        {
            swaps.push((s[j], s[j + 1], k));
        }
    }
    if swaps.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut skip = None;
    for (i, tok) in toks.iter().enumerate() {
        if skip == Some(i) {
            continue;
        }
        if let Some(&(_, field, k)) = swaps.iter().find(|(dot, _, _)| *dot == i) {
            out.push_str(&format!("[{k}]"));
            skip = Some(field);
            continue;
        }
        if skip.is_some_and(|f| i < f) {
            // Spaces between the dot and the field.
            continue;
        }
        out.push_str(tok.text);
    }
    Some(out)
}

#[cfg(test)]
mod swizzle_tests {
    use super::super::lex::tokenize;
    use super::index_swizzle_stores;

    #[test]
    fn one_component_stores_into_indexed_columns() {
        let src = "void main() { mat2 m; m[1].x = 1.0; m[0].g += 2.0; float a = m[1].x; m[1].xy = vec2(0.0); }";
        let out = index_swizzle_stores(&tokenize(src)).unwrap();
        assert!(
            out.contains("m[1][0] = 1.0; m[0][1] += 2.0; float a = m[1].x; m[1].xy ="),
            "{out}"
        );
        let rw = crate::rewrite::rewrite(
            "varying vec2 c;\nvoid main() { mat2 m = mat2(1.0); m[1].y = c.x; gl_FragData[0] = vec4(m[1], 0.0, 1.0); }",
            crate::Stage::Fragment,
        )
        .unwrap();
        assert!(rw.body.contains("m[1][1] = c.x"), "{}", rw.body);
    }
}
