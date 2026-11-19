//! Tokens and token-level passes over a whole stage.

use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Ident,
    Number,
    Punct,
    Space,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Tok<'a> {
    pub(super) kind: Kind,
    pub(super) text: &'a str,
}

/// GLSL's operators of more than one character, longest first.
const OPERATORS: [&str; 22] = [
    "<<=", ">>=", "||", "&&", "^^", "==", "!=", "<=", ">=", "<<", ">>", "++", "--", "+=", "-=",
    "*=", "/=", "%=", "&=", "|=", "^=", "->",
];

pub(super) fn tokenize(src: &str) -> Vec<Tok<'_>> {
    let b = src.as_bytes();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let start = i;
        let kind = if c.is_ascii_whitespace() {
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            Kind::Space
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            Kind::Ident
        } else if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            // Digits, `.`, suffixes and hex letters; a sign only right after a decimal exponent.
            let hex = src[i..].starts_with("0x") || src[i..].starts_with("0X");
            while i < b.len() {
                let d = b[i];
                let sign = (d == b'+' || d == b'-') && !hex && matches!(b[i - 1], b'e' | b'E');
                if !(d.is_ascii_alphanumeric() || d == b'.' || d == b'_' || sign) {
                    break;
                }
                i += 1;
            }
            Kind::Number
        } else {
            // Multi-character operators stay one token, so passes that rebuild text from
            // tokens with spaces between them keep `||`, `==`, `<<=`… intact.
            let op = OPERATORS.iter().find(|op| src[i..].starts_with(**op));
            i += op.map_or_else(
                || src[i..].chars().next().map_or(1, char::len_utf8),
                |op| op.len(),
            );
            Kind::Punct
        };
        toks.push(Tok {
            kind,
            text: &src[start..i],
        });
    }
    toks
}

pub(super) fn is_type(name: &str, structs: &BTreeSet<String>) -> bool {
    let base = name.trim_start_matches(['i', 'u', 'b', 'd']);
    matches!(name, "void" | "float" | "int" | "uint" | "bool" | "double")
        || ["vec", "mat", "sampler"]
            .iter()
            .any(|p| base.starts_with(p))
        || structs.contains(name)
}

/// Removes the statements after a `return` that ends its block's
/// reachable code (`#ifdef X return a; #endif` patterns leave these):
/// naga's GLSL front end emits their expressions nowhere and then fails
/// validation ("Expression not visited by the appropriate statement").
pub(super) fn drop_unreachable(toks: Vec<Tok<'_>>) -> Vec<Tok<'_>> {
    let mut dead = vec![false; toks.len()];
    let mut prev = "";
    for i in 0..toks.len() {
        if toks[i].kind == Kind::Space || dead[i] {
            continue;
        }
        // Only a return directly in a `{}` block: `if (c) return x;` guards the code after it.
        if toks[i].text == "return" && matches!(prev, "{" | ";" | "}") {
            let semi = (i..toks.len()).find(|&j| toks[j].text == ";");
            let mut depth = 0;
            let close = semi.and_then(|s| {
                (s + 1..toks.len()).find(|&k| match toks[k].text {
                    "{" => {
                        depth += 1;
                        false
                    }
                    "}" if depth == 0 => true,
                    "}" => {
                        depth -= 1;
                        false
                    }
                    _ => false,
                })
            });
            if let (Some(s), Some(c)) = (semi, close)
                && !toks[s..c]
                    .iter()
                    .any(|t| matches!(t.text, "case" | "default"))
            {
                dead[s + 1..c].iter_mut().for_each(|d| *d = true);
            }
        }
        prev = toks[i].text;
    }
    toks.into_iter()
        .zip(dead)
        .filter(|(_, d)| !d)
        .map(|(t, _)| t)
        .collect()
}

/// Token ranges of the top-level items: declarations ending in `;` and
/// function definitions ending with their closing brace.
pub(super) fn top_level_items(toks: &[Tok]) -> Result<Vec<(usize, usize)>, String> {
    let (mut items, mut start, mut depth, mut is_struct) = (Vec::new(), 0, 0i32, false);
    for (i, t) in toks.iter().enumerate() {
        match t.text {
            "struct" if depth == 0 => is_struct = true,
            "{" => depth += 1,
            "}" => {
                depth -= 1;
                if depth < 0 {
                    return Err("unbalanced `}`".into());
                }
                if depth == 0 && !is_struct {
                    items.push((start, i + 1));
                    start = i + 1;
                }
            }
            ";" if depth == 0 => {
                items.push((start, i + 1));
                start = i + 1;
                is_struct = false;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err("unbalanced `{`".into());
    }
    items.push((start, toks.len()));
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::tokenize;

    #[test]
    fn operators_are_one_token() {
        let texts: Vec<&str> = tokenize("a<<=b||c&&-d==e>>f<=g++")
            .into_iter()
            .map(|t| t.text)
            .collect();
        assert_eq!(
            texts,
            [
                "a", "<<=", "b", "||", "c", "&&", "-", "d", "==", "e", ">>", "f", "<=", "g", "++"
            ]
        );
    }
}
