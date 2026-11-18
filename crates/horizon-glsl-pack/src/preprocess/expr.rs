//! A lossless lexer (macro expansion rebuilds text from its slices) and the
//! `#if` expression evaluator.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum K {
    Ident,
    Num,
    Space,
    Punct,
}

const TWO_CHAR: [&str; 8] = ["<<", ">>", "<=", ">=", "==", "!=", "&&", "||"];

/// Splits `s` into tokens whose slices concatenate back to `s`. Numbers keep
/// their exponent sign (`1.0e-5`) and suffixes so expansion never splits one.
pub(super) fn lex(s: &str) -> Vec<(K, &str)> {
    let b = s.as_bytes();
    let (mut i, mut out) = (0, Vec::new());
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    while i < b.len() {
        let (start, c) = (i, b[i]);
        let kind = if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && word(b[i]) {
                i += 1;
            }
            K::Ident
        } else if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            let hex = s[i..].starts_with("0x") || s[i..].starts_with("0X");
            i += 1;
            while i < b.len()
                && (word(b[i])
                    || b[i] == b'.'
                    || (!hex && matches!(b[i], b'+' | b'-') && matches!(b[i - 1], b'e' | b'E')))
            {
                i += 1;
            }
            K::Num
        } else if c.is_ascii_whitespace() {
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            K::Space
        } else {
            i += if TWO_CHAR.iter().any(|op| s[i..].starts_with(op)) {
                2
            } else {
                s[i..].chars().next().map_or(1, char::len_utf8)
            };
            K::Punct
        };
        out.push((kind, &s[start..i]));
    }
    out
}

/// Evaluates a `#if` expression whose `defined` and macros are already
/// replaced: remaining identifiers are 0. Integer `/` and `%` truncate as in C.
pub(super) fn eval(expr: &str) -> Result<f64, String> {
    let toks: Vec<_> = lex(expr).into_iter().filter(|t| t.0 != K::Space).collect();
    let mut p = Parser { toks, i: 0 };
    let v = p.ternary()?;
    match p.toks.get(p.i) {
        None => Ok(v),
        Some(t) => Err(format!("unexpected `{}` in `#if {expr}`", t.1)),
    }
}

struct Parser<'a> {
    toks: Vec<(K, &'a str)>,
    i: usize,
}

fn prec(op: &str) -> u8 {
    match op {
        "||" => 1,
        "&&" => 2,
        "|" => 3,
        "^" => 4,
        "&" => 5,
        "==" | "!=" => 6,
        "<" | ">" | "<=" | ">=" => 7,
        "<<" | ">>" => 8,
        "+" | "-" => 9,
        "*" | "/" | "%" => 10,
        _ => 0,
    }
}

impl Parser<'_> {
    fn eat(&mut self, s: &str) -> bool {
        let hit = self.toks.get(self.i).is_some_and(|t| t.1 == s);
        self.i += hit as usize;
        hit
    }

    fn ternary(&mut self) -> Result<f64, String> {
        let c = self.binary(1)?;
        if !self.eat("?") {
            return Ok(c);
        }
        let a = self.ternary()?;
        if !self.eat(":") {
            return Err("`?` without `:` in `#if`".into());
        }
        let b = self.ternary()?;
        Ok(if c != 0.0 { a } else { b })
    }

    fn binary(&mut self, min: u8) -> Result<f64, String> {
        let mut lhs = self.unary()?;
        while let Some(&(K::Punct, op)) = self.toks.get(self.i) {
            let p = prec(op);
            if p == 0 || p < min {
                break;
            }
            self.i += 1;
            let rhs = self.binary(p + 1)?;
            let (a, b) = (lhs as i64, rhs as i64);
            let ints = lhs.fract() == 0.0 && rhs.fract() == 0.0;
            if matches!(op, "/" | "%") && rhs == 0.0 {
                return Err("division by zero in `#if`".into());
            }
            lhs = match op {
                "||" => truth(lhs != 0.0 || rhs != 0.0),
                "&&" => truth(lhs != 0.0 && rhs != 0.0),
                "|" => (a | b) as f64,
                "^" => (a ^ b) as f64,
                "&" => (a & b) as f64,
                "==" => truth(lhs == rhs),
                "!=" => truth(lhs != rhs),
                "<" => truth(lhs < rhs),
                ">" => truth(lhs > rhs),
                "<=" => truth(lhs <= rhs),
                ">=" => truth(lhs >= rhs),
                "<<" => a.wrapping_shl(b as u32) as f64,
                ">>" => a.wrapping_shr(b as u32) as f64,
                "+" => lhs + rhs,
                "-" => lhs - rhs,
                "*" => lhs * rhs,
                "/" if ints => (a / b) as f64,
                "/" => lhs / rhs,
                "%" if ints => (a % b) as f64,
                _ => lhs % rhs,
            };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<f64, String> {
        let t = self.toks.get(self.i).copied();
        self.i += 1;
        match t {
            Some((K::Punct, "!")) => Ok(truth(self.unary()? == 0.0)),
            Some((K::Punct, "~")) => Ok(!(self.unary()? as i64) as f64),
            Some((K::Punct, "-")) => Ok(-self.unary()?),
            Some((K::Punct, "+")) => self.unary(),
            Some((K::Punct, "(")) => {
                let v = self.ternary()?;
                if self.eat(")") {
                    Ok(v)
                } else {
                    Err("missing `)` in `#if`".into())
                }
            }
            Some((K::Num, n)) => number(n),
            Some((K::Ident, _)) => Ok(0.0),
            t => Err(format!("unexpected {:?} in `#if`", t.map(|t| t.1))),
        }
    }
}

fn truth(b: bool) -> f64 {
    b as u8 as f64
}

fn number(n: &str) -> Result<f64, String> {
    let bad = || format!("bad number `{n}` in `#if`");
    if let Some(hex) = n.strip_prefix("0x").or_else(|| n.strip_prefix("0X")) {
        let hex = hex.trim_end_matches(['u', 'U', 'l', 'L']);
        return i64::from_str_radix(hex, 16)
            .map(|v| v as f64)
            .map_err(|_| bad());
    }
    n.trim_end_matches(['f', 'F', 'u', 'U', 'l', 'L'])
        .parse()
        .map_err(|_| bad())
}
