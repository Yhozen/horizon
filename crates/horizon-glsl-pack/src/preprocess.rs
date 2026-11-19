//! The pack's C-style preprocessor: `#include`, `#define` (object-like and
//! simple function-like), `#if/#ifdef/#ifndef/#elif/#else/#endif`, option
//! overrides. The output has no directives, no comments and no blank lines.

use std::collections::BTreeMap;

use crate::{PackError, PackFiles};

mod expr;

use expr::{K, lex};

/// The macros OptiFine defines before a pack's sources (`MC_VERSION`, …).
pub fn predefined(mc_version: u32) -> BTreeMap<String, String> {
    [
        ("MC_VERSION", mc_version.to_string()),
        ("MC_GL_VERSION", "320".into()),
        ("MC_GLSL_VERSION", "150".into()),
        ("MC_OS_LINUX", "1".into()),
        ("MC_RENDER_QUALITY", "1.0".into()),
        ("MC_SHADOW_QUALITY", "1.0".into()),
        ("MC_HAND_DEPTH", "0.125".into()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

/// The file at `path` with includes expanded, comments removed, conditionals
/// resolved, `#define`s applied (and their lines removed), `#version`,
/// `#extension` and other directives dropped: plain GLSL text.
pub fn preprocess(
    files: &PackFiles,
    path: &str,
    predefined: &BTreeMap<String, String>,
    overrides: &[(String, Option<String>)],
) -> Result<String, PackError> {
    let text = files
        .text(path)
        .ok_or_else(|| PackError::new(path, "no such file"))?;
    preprocess_text(&text, path, files, predefined, overrides)
}

/// Like [`preprocess`] for text that is not a pack file (`.properties`).
/// `file` names it in errors and anchors its relative includes. Unknown
/// directives are dropped, which also drops `.properties` `#` comments.
pub fn preprocess_text(
    text: &str,
    file: &str,
    files: &PackFiles,
    predefined: &BTreeMap<String, String>,
    overrides: &[(String, Option<String>)],
) -> Result<String, PackError> {
    let macros = predefined
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                Macro {
                    params: None,
                    body: v.clone(),
                },
            )
        })
        .collect();
    let mut pp = Pp {
        files,
        overrides,
        macros,
        conds: Vec::new(),
        including: vec![file.to_string()],
        out: String::new(),
    };
    pp.run(text, file)?;
    if !pp.conds.is_empty() {
        return Err(PackError::new(file, "`#if` without `#endif`"));
    }
    Ok(pp.out)
}

/// The file at `path` with only `#include`s expanded (comments kept, every
/// include followed whatever `#if` surrounds it).
pub fn expand_includes(files: &PackFiles, path: &str) -> Result<String, PackError> {
    fn walk(
        files: &PackFiles,
        path: &str,
        stack: &mut Vec<String>,
        out: &mut String,
    ) -> Result<(), PackError> {
        let text = files
            .text(path)
            .ok_or_else(|| PackError::new(path, "no such file"))?;
        for (n, line) in text.lines().enumerate() {
            let Some(target) = include_target(line) else {
                out.push_str(line);
                out.push('\n');
                continue;
            };
            let target = resolve(path, target);
            if stack.contains(&target) {
                return Err(PackError::new(
                    path,
                    format!("line {}: include cycle through {target}", n + 1),
                ));
            }
            stack.push(target.clone());
            walk(files, &target, stack, out)?;
            stack.pop();
        }
        Ok(())
    }
    let mut out = String::new();
    walk(files, path, &mut vec![path.to_string()], &mut out)?;
    Ok(out)
}

struct Macro {
    params: Option<Vec<String>>,
    body: String,
}

/// One `#if` group: whether its parent is live, whether a branch was taken,
/// whether the current branch is live.
struct Cond {
    parent: bool,
    taken: bool,
    live: bool,
}

struct Pp<'a> {
    files: &'a PackFiles,
    overrides: &'a [(String, Option<String>)],
    macros: BTreeMap<String, Macro>,
    conds: Vec<Cond>,
    /// The include stack, for cycle detection.
    including: Vec<String>,
    out: String,
}

impl Pp<'_> {
    fn live(&self) -> bool {
        self.conds.last().is_none_or(|c| c.live)
    }

    fn run(&mut self, text: &str, file: &str) -> Result<(), PackError> {
        for (n, line) in strip_comments(text).lines().enumerate() {
            let at = |m: String| PackError::new(file, format!("line {}: {m}", n + 1));
            let trimmed = line.trim();
            if let Some(target) = include_target(trimmed) {
                if !self.live() {
                    continue;
                }
                let path = resolve(file, target);
                if self.including.contains(&path) {
                    return Err(at(format!("include cycle through {path}")));
                }
                let text = self
                    .files
                    .text(&path)
                    .ok_or_else(|| at(format!("missing include {path}")))?;
                self.including.push(path.clone());
                self.run(&text, &path)?;
                self.including.pop();
            } else if let Some(d) = trimmed.strip_prefix('#') {
                let d = d.trim_start();
                let split = d
                    .find(|c: char| !c.is_ascii_alphanumeric())
                    .unwrap_or(d.len());
                self.directive(&d[..split], d[split..].trim()).map_err(at)?;
            } else if self.live() && !trimmed.is_empty() {
                let line = self.expand(line, &mut Vec::new());
                self.out.push_str(line.trim_end());
                self.out.push('\n');
            }
        }
        Ok(())
    }

    fn directive(&mut self, name: &str, args: &str) -> Result<(), String> {
        let live = self.live();
        match name {
            "if" | "ifdef" | "ifndef" => {
                let v = live
                    && match name {
                        "if" => self.condition(args)?,
                        "ifdef" => self.macros.contains_key(ident(args)),
                        _ => !self.macros.contains_key(ident(args)),
                    };
                self.conds.push(Cond {
                    parent: live,
                    taken: v,
                    live: v,
                });
            }
            "elif" => {
                let c = self.conds.last().ok_or("`#elif` without `#if`")?;
                let v = c.parent && !c.taken && self.condition(args)?;
                let c = self.conds.last_mut().expect("checked above");
                c.live = v;
                c.taken |= v;
            }
            "else" => {
                let c = self.conds.last_mut().ok_or("`#else` without `#if`")?;
                c.live = c.parent && !c.taken;
                c.taken = true;
            }
            "endif" => {
                self.conds.pop().ok_or("`#endif` without `#if`")?;
            }
            _ if !live => {}
            "define" => self.define(args)?,
            "undef" => {
                self.macros.remove(ident(args));
            }
            "error" => return Err(format!("#error {args}")),
            // `#version`, `#extension`, `#pragma`, `#line`: the host writes its own.
            _ => {}
        }
        Ok(())
    }

    fn define(&mut self, args: &str) -> Result<(), String> {
        let name = ident(args);
        if name.is_empty() {
            return Err("`#define` without a name".into());
        }
        let rest = &args[name.len()..];
        // Function-like only when `(` touches the name, as in C.
        let (params, body) = match rest.strip_prefix('(') {
            Some(r) => {
                let close = r.find(')').ok_or("unclosed macro parameter list")?;
                let params = r[..close]
                    .split(',')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty());
                (Some(params.collect()), r[close + 1..].trim())
            }
            None => (None, rest.trim()),
        };
        let body = match self.overrides.iter().rev().find(|o| o.0 == name) {
            Some((_, None)) => return Ok(()),
            Some((_, Some(v))) => v.clone(),
            None => body.to_string(),
        };
        self.macros.insert(name.to_string(), Macro { params, body });
        Ok(())
    }

    /// `defined` is replaced before expansion so `defined(X)` sees `X` itself.
    fn condition(&self, expr: &str) -> Result<bool, String> {
        let toks = lex(expr);
        let skip = |mut j: usize| {
            while toks.get(j).is_some_and(|t| t.0 == K::Space) {
                j += 1;
            }
            j
        };
        let (mut s, mut i) = (String::new(), 0);
        while i < toks.len() {
            if toks[i] != (K::Ident, "defined") {
                s.push_str(toks[i].1);
                i += 1;
                continue;
            }
            let mut j = skip(i + 1);
            let paren = toks.get(j).is_some_and(|t| t.1 == "(");
            if paren {
                j = skip(j + 1);
            }
            let Some(&(K::Ident, name)) = toks.get(j) else {
                return Err("`defined` without a name".into());
            };
            j += 1;
            if paren {
                j = skip(j);
                if toks.get(j).map(|t| t.1) != Some(")") {
                    return Err("missing `)` after `defined(`".into());
                }
                j += 1;
            }
            s.push_str(if self.macros.contains_key(name) {
                " 1 "
            } else {
                " 0 "
            });
            i = j;
        }
        Ok(expr::eval(&self.expand(&s, &mut Vec::new()))? != 0.0)
    }

    /// Expands macros in `text`, whole identifiers only; `hide` holds the
    /// macros being expanded so none expands inside itself.
    fn expand(&self, text: &str, hide: &mut Vec<String>) -> String {
        let toks = lex(text);
        let (mut out, mut i) = (String::new(), 0);
        while i < toks.len() {
            let (kind, s) = toks[i];
            i += 1;
            let m = match self.macros.get(s) {
                Some(m) if kind == K::Ident && !hide.iter().any(|h| h == s) => m,
                _ => {
                    out.push_str(s);
                    continue;
                }
            };
            let body = match &m.params {
                None => m.body.clone(),
                Some(params) => match call_args(&toks, i) {
                    Some((args, next))
                        if args.len() == params.len() || (params.is_empty() && args == [""]) =>
                    {
                        i = next;
                        let args: Vec<_> =
                            args.iter().map(|a| self.expand(a.trim(), hide)).collect();
                        let subst = |(k, t): (K, &str)| match params.iter().position(|p| p == t) {
                            Some(p) if k == K::Ident => args[p].clone(),
                            _ => t.to_string(),
                        };
                        lex(&m.body).into_iter().map(subst).collect()
                    }
                    // Not a call (or a wrong arity): the name stays, as in C.
                    _ => {
                        out.push_str(s);
                        continue;
                    }
                },
            };
            hide.push(s.to_string());
            let e = self.expand(&body, hide);
            hide.pop();
            // Spaces where an operator could fuse with its neighbour (`x-A`, A = -1).
            let op = |c: Option<char>| c.is_some_and(|c| "+-*/%<>=!&|^".contains(c));
            if op(e.chars().next()) {
                out.push(' ');
            }
            out.push_str(&e);
            if op(e.chars().last()) {
                out.push(' ');
            }
        }
        out
    }
}

/// A call's arguments on the same line, split at top-level commas, and the
/// token index after its `)`.
fn call_args(toks: &[(K, &str)], mut i: usize) -> Option<(Vec<String>, usize)> {
    while toks.get(i)?.0 == K::Space {
        i += 1;
    }
    if toks[i].1 != "(" {
        return None;
    }
    let (mut depth, mut args, mut cur) = (0, Vec::new(), String::new());
    for (j, &(_, s)) in toks.iter().enumerate().skip(i + 1) {
        match s {
            ")" if depth == 0 => {
                args.push(cur);
                return Some((args, j + 1));
            }
            "," if depth == 0 => {
                args.push(std::mem::take(&mut cur));
                continue;
            }
            "(" => depth += 1,
            ")" => depth -= 1,
            _ => {}
        }
        cur.push_str(s);
    }
    None
}

/// The leading identifier of a directive's arguments.
fn ident(args: &str) -> &str {
    &args[..args
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(args.len())]
}

fn include_target(line: &str) -> Option<&str> {
    let rest = line
        .trim()
        .strip_prefix('#')?
        .trim_start()
        .strip_prefix("include")?;
    Some(rest.trim().trim_matches(['"', '<', '>']))
}

/// `/x` is relative to `shaders/`, anything else to the including file's folder.
fn resolve(from: &str, target: &str) -> String {
    let joined = match (target.strip_prefix('/'), from.rsplit_once('/')) {
        (Some(abs), _) => format!("shaders/{abs}"),
        (None, Some((dir, _))) => format!("{dir}/{target}"),
        (None, None) => target.to_string(),
    };
    let mut parts = Vec::new();
    for p in joined.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

/// Removes comments and joins `\` continuations; the lines they swallow come
/// back as blank lines so error line numbers stay right.
fn strip_comments(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    let (mut out, mut held) = (String::with_capacity(text.len()), 0);
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        match (c, it.peek()) {
            ('\\', Some('\n')) => {
                it.next();
                held += 1;
            }
            ('/', Some('/')) => while it.next_if(|&c| c != '\n').is_some() {},
            ('/', Some('*')) => {
                it.next();
                out.push(' ');
                let mut prev = ' ';
                for c in it.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                    } else if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
            }
            ('\n', _) => {
                out.extend(std::iter::repeat_n('\n', held + 1));
                held = 0;
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(files: &[(&str, &str)]) -> PackFiles {
        PackFiles(
            files
                .iter()
                .map(|(p, t)| (p.to_string(), t.as_bytes().to_vec()))
                .collect(),
        )
    }

    fn pp(text: &str) -> String {
        pp_with(text, &[])
    }

    fn pp_with(text: &str, overrides: &[(&str, Option<&str>)]) -> String {
        let o: Vec<_> = overrides
            .iter()
            .map(|(k, v)| (k.to_string(), v.map(str::to_string)))
            .collect();
        preprocess_text(
            text,
            "shaders/t.fsh",
            &PackFiles::default(),
            &predefined(12111),
            &o,
        )
        .unwrap()
    }

    #[test]
    fn includes_absolute_relative_and_dead() {
        let files = pack(&[
            (
                "shaders/world0/a.fsh",
                "#version 120\n#include \"/lib/b.glsl\"\n#if 0\n#include \"missing.glsl\"\n#endif\nA",
            ),
            ("shaders/lib/b.glsl", "#include \"../lib/c.glsl\"\nB"),
            ("shaders/lib/c.glsl", "C // comment"),
        ]);
        assert_eq!(
            preprocess(&files, "shaders/world0/a.fsh", &BTreeMap::new(), &[]).unwrap(),
            "C\nB\nA\n"
        );
        let raw = expand_includes(&files, "shaders/lib/b.glsl").unwrap();
        assert_eq!(raw, "C // comment\nB\n");
    }

    #[test]
    fn include_cycles_are_errors() {
        let files = pack(&[
            ("shaders/a.glsl", "#include \"b.glsl\""),
            ("shaders/b.glsl", "#include \"/a.glsl\""),
        ]);
        let e = preprocess(&files, "shaders/a.glsl", &BTreeMap::new(), &[]).unwrap_err();
        assert!(e.message.contains("cycle"), "{e}");
        assert!(expand_includes(&files, "shaders/a.glsl").is_err());
    }

    #[test]
    fn macros_expand_whole_identifiers() {
        let src = "#define N 4\n#define M (N * 2)\n#define S 1.0e5\n#define NEG -1\nint x = M + NN + N_2 + aN;\nfloat f = S * 1e5 + 2.5e-3;\nint y = x-NEG;\n";
        assert_eq!(
            pp(src),
            "int x = (4 * 2) + NN + N_2 + aN;\nfloat f = 1.0e5 * 1e5 + 2.5e-3;\nint y = x- -1;\n"
        );
        assert_eq!(
            pp("#define A A + B\n#define B A\nA;\n#undef A\nA;"),
            "A + A;\nA;\n"
        );
        assert_eq!(
            pp("#define F(a, b) ((a) * (b))\n#define G F\nF(1, F(2, 3)) G F (x,(y, z)) F\n"),
            "((1) * (((2) * (3)))) F ((x) * ((y, z))) F\n"
        );
    }

    #[test]
    fn conditions() {
        let t = |e: &str| {
            pp(&format!(
                "#define TWO 2\n#define EMPTY\n#if {e}\nyes\n#else\nno\n#endif"
            )) == "yes\n"
        };
        for e in [
            "1",
            "TWO == 2",
            "defined TWO",
            "defined(EMPTY) && !defined NOPE",
            "UNDEFINED == 0",
            "!0 && ~0 == -1",
        ] {
            assert!(t(e), "{e}");
        }
        for e in [
            "(1 + 2) * 3 == 9",
            "7 / 2 == 3 && 7 % 4 == 3",
            "1 << 3 == 8 && 16 >> 2 == 4",
            "(5 & 3) == 1 && (5 ^ 3) == 6",
            "(4 | 1) == 5",
        ] {
            assert!(t(e), "{e}");
        }
        for e in [
            "1.5 > 1 && 0.5 < 1 && 2 >= 2 && 2 <= 2 && 1 != 2",
            "0 || TWO > 1",
            "MC_VERSION >= 12100",
            "-TWO + 3 == 1",
            "1 ? 2 : 0",
        ] {
            assert!(t(e), "{e}");
        }
        for e in [
            "0",
            "TWO < 2",
            "defined NOPE || 0",
            "MC_VERSION < 12111",
            "1 && 0",
            "!TWO",
            "0x10 == 15",
        ] {
            assert!(!t(e), "{e}");
        }
        let chain = |v: &str| {
            pp(&format!(
                "#define V {v}\n#if V == 1\na\n#elif V == 2\nb\n#elif V >= 2\nc\n#else\nd\n#endif\n"
            ))
        };
        assert_eq!(
            [chain("1"), chain("2"), chain("3"), chain("0")],
            ["a\n", "b\n", "c\n", "d\n"]
        );
        assert_eq!(
            pp("#ifdef X\n#if 1/0\n#endif\n#elif 1\nk\n#endif\n#ifndef X\nn\n#endif"),
            "k\nn\n"
        );
        assert!(pp_err("#if 1\n#error stop\n#endif").contains("stop"));
        assert!(pp_err("#if 1\nx").contains("#endif"));
        assert!(pp_err("#endif").contains("without"));
        assert_eq!(pp("#if 0\n#error dead\n#endif\nok"), "ok\n");
    }

    fn pp_err(text: &str) -> String {
        preprocess_text(text, "t", &PackFiles::default(), &BTreeMap::new(), &[])
            .unwrap_err()
            .message
    }

    #[test]
    fn overrides_and_dropped_directives() {
        let src = "#version 120\n#extension GL_EXT_gpu_shader4 : enable\n#pragma optimize(on)\n#define Q 2 // [1 2 3]\n#define ON\n#ifdef ON\non\n#endif\nint q = Q;";
        assert_eq!(pp(src), "on\nint q = 2;\n");
        assert_eq!(
            pp_with(src, &[("Q", Some("3")), ("ON", None)]),
            "int q = 3;\n"
        );
        // Overrides do not touch predefined macros.
        assert_eq!(
            pp_with("int v = MC_VERSION;", &[("MC_VERSION", None)]),
            "int v = 12111;\n"
        );
    }

    #[test]
    fn comments_go_and_lines_hold() {
        let s = strip_comments("a /* x\ny */ b // c\n#define L 1 + \\\n 2\nd");
        assert_eq!(s, "a  \n b \n#define L 1 +  2\n\nd");
        assert_eq!(
            pp("#define L 1 + \\\n 2\nint i = L; /* multi\nline */ int j;"),
            "int i = 1 +  2;\n int j;\n"
        );
    }

    /// Set `HORIZON_TEST_SHADERPACK` to an unpacked pack (the folder holding
    /// `shaders/`) to run the preprocessor over its real programs.
    #[test]
    fn real_pack() {
        let Ok(dir) = std::env::var("HORIZON_TEST_SHADERPACK") else {
            return;
        };
        let files = load_pack(std::path::Path::new(&dir));
        let pre = predefined(12111);
        for overrides in [vec![], vec![("ENABLE_SHADOWS".to_string(), None)]] {
            for name in ["gbuffers_terrain", "gbuffers_water", "final", "shadow"] {
                for ext in ["vsh", "fsh"] {
                    let path = format!("shaders/world0/{name}.{ext}");
                    let out = preprocess(&files, &path, &pre, &overrides).unwrap();
                    assert!(
                        !out.lines().any(|l| l.trim_start().starts_with('#')),
                        "{path}: directive left"
                    );
                    assert!(
                        !out.contains("BLOCK_REFLECTIONS")
                            && !out.contains("/*")
                            && !out.contains("//"),
                        "{path}"
                    );
                    if name == "gbuffers_terrain" && ext == "fsh" {
                        assert!(
                            out.contains("blockReflectivity"),
                            "BLOCK_REFLECTIONS > 0 branch taken"
                        );
                    }
                    if name == "gbuffers_terrain" && ext == "vsh" {
                        assert_eq!(out.contains("shadowLightPosition"), overrides.is_empty());
                        if overrides.is_empty() {
                            println!("{out}");
                        }
                    }
                }
            }
        }
        let blocks = crate::properties::block_ids(&files, &pre).unwrap();
        // The two fluid surfaces, then 23 ids.
        assert_eq!(blocks.entries.len(), 25);
        assert_eq!(
            blocks.entity(blocks.material("minecraft:water", true)),
            (10008, true)
        );
        let raw = expand_includes(&files, "shaders/world0/gbuffers_terrain.fsh").unwrap();
        assert_eq!(crate::properties::draw_buffers(&raw), Some(vec![0, 6, 7]));
        assert_eq!(
            crate::properties::dimension_folder(&files, "minecraft:overworld"),
            "world0"
        );
        assert_eq!(
            crate::properties::dimension_folder(&files, "minecraft:the_nether"),
            "world-1"
        );
    }

    fn load_pack(dir: &std::path::Path) -> PackFiles {
        let mut files = PackFiles::default();
        let mut todo = vec![dir.to_path_buf()];
        while let Some(d) = todo.pop() {
            for entry in std::fs::read_dir(&d).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    todo.push(path);
                } else {
                    let rel = path
                        .strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    files.0.insert(rel, std::fs::read(&path).unwrap());
                }
            }
        }
        files
    }
}
