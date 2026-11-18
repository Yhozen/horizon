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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Ident,
    Number,
    Punct,
    Space,
}

#[derive(Clone, Copy, Debug)]
struct Tok<'a> {
    kind: Kind,
    text: &'a str,
}

fn tokenize(src: &str) -> Vec<Tok<'_>> {
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
            i += src[i..].chars().next().map_or(1, char::len_utf8);
            Kind::Punct
        };
        toks.push(Tok {
            kind,
            text: &src[start..i],
        });
    }
    toks
}

fn is_type(name: &str, structs: &BTreeSet<String>) -> bool {
    let base = name.trim_start_matches(['i', 'u', 'b', 'd']);
    matches!(name, "void" | "float" | "int" | "uint" | "bool" | "double")
        || ["vec", "mat", "sampler"]
            .iter()
            .any(|p| base.starts_with(p))
        || structs.contains(name)
}

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
}

/// Rewrites one preprocessed stage. See the module doc and the spec, section 3.
pub fn rewrite(source: &str, stage: Stage) -> Result<Rewritten, String> {
    let toks = drop_unreachable(tokenize(source));
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
    cx.out.body = body;
    Ok(cx.out)
}

/// Removes the statements after a `return` that ends its block's
/// reachable code (`#ifdef X return a; #endif` patterns leave these):
/// naga's GLSL front end emits their expressions nowhere and then fails
/// validation ("Expression not visited by the appropriate statement").
fn drop_unreachable(toks: Vec<Tok<'_>>) -> Vec<Tok<'_>> {
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
fn top_level_items(toks: &[Tok]) -> Result<Vec<(usize, usize)>, String> {
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

impl Ctx<'_> {
    fn text(&self, i: usize) -> &str {
        self.toks[i].text
    }

    /// Records and drops an interface declaration (`attribute`, `varying`,
    /// `uniform`, `in`, `out`). Returns false for anything else.
    fn declaration(&mut self, sig: &[usize]) -> Result<bool, String> {
        let mut words: Vec<&str> = sig.iter().map(|&i| self.toks[i].text).collect();
        if words.last() == Some(&";") {
            words.pop();
        }
        // `layout(...)` is skipped; locations come from declaration order.
        let mut k = 0;
        if words.first() == Some(&"layout") {
            k = words
                .iter()
                .position(|w| *w == ")")
                .map_or(words.len(), |p| p + 1);
        }
        let mut quals = Vec::new();
        while k + 2 < words.len() && listed(QUALIFIERS, words[k]) {
            quals.push(words[k]);
            k += 1;
        }
        let storage = quals
            .iter()
            .copied()
            .find(|q| matches!(*q, "attribute" | "varying" | "uniform" | "in" | "out"));
        let Some(storage) = storage else {
            return Ok(false);
        };
        let ty = words[k].to_string();
        let flat = quals.contains(&"flat");
        let mut rest = &words[k + 1..];
        if rest.first() == Some(&"[") {
            return Err(format!(
                "array type syntax `{ty}[]` in a `{storage}` declaration"
            ));
        }
        while let Some(&name) = rest.first() {
            if !name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
                return Err(format!("unexpected `{name}` in a `{storage}` declaration"));
            }
            let mut array_len = None;
            let mut next = 1;
            if rest.get(1) == Some(&"[") {
                let len = rest
                    .get(2)
                    .and_then(|n| n.trim_end_matches(['u', 'U']).parse().ok());
                array_len =
                    Some(len.ok_or_else(|| format!("array `{name}` needs a literal length"))?);
                next = 4;
            }
            if rest.get(next) == Some(&"=") {
                return Err(format!("initializer on `{storage} {name}`"));
            }
            self.record(storage, &ty, name, flat, array_len)?;
            match rest.get(next) {
                None => break,
                Some(&",") => rest = &rest[next + 1..],
                Some(other) => {
                    return Err(format!("unexpected `{other}` after `{storage} {name}`"));
                }
            }
        }
        Ok(true)
    }

    fn record(
        &mut self,
        storage: &str,
        ty: &str,
        name: &str,
        flat: bool,
        len: Option<u32>,
    ) -> Result<(), String> {
        let var = Var {
            name: name.into(),
            ty: ty.into(),
            flat,
            array_len: len,
        };
        let fragment = self.stage == Stage::Fragment;
        match storage {
            "uniform" if ty.trim_start_matches(['i', 'u']).starts_with("sampler") => {
                if len.is_some() {
                    return Err(format!("sampler array `{name}`"));
                }
                self.sampler_shadow
                    .insert(name.into(), ty.ends_with("Shadow"));
                self.out.samplers.insert(name.into());
            }
            "uniform" if len.is_some() => return Err(format!("uniform array `{name}`")),
            "uniform" => self.out.uniforms.push(Uniform {
                name: name.into(),
                ty: ty.into(),
            }),
            "attribute" => self.out.attributes.push(var),
            "in" if !fragment => self.out.attributes.push(var),
            "out" if fragment => {
                let i = self.frag_outs.len();
                self.frag_outs.insert(name.into(), i);
            }
            _ => self.out.varyings.push(var),
        }
        Ok(())
    }

    fn next_sig(&self, mut i: usize, end: usize) -> Option<usize> {
        while i < end && self.toks[i].kind == Kind::Space {
            i += 1;
        }
        (i < end).then_some(i)
    }

    /// The argument ranges of the call whose `(` is at `open`, and the index after its `)`.
    fn call_args(&self, open: usize, end: usize) -> Result<(Vec<(usize, usize)>, usize), String> {
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
                        return Ok((args, i + 1));
                    }
                }
                _ => {}
            }
        }
        Err("unclosed call".into())
    }

    /// Copies tokens `start..end` to `out`, applying renames and the sampling rewrites.
    fn emit(&mut self, start: usize, end: usize, out: &mut String) -> Result<(), String> {
        let mut i = start;
        let mut after_dot = false;
        while i < end {
            let t = self.toks[i];
            i += 1;
            if t.kind != Kind::Ident {
                out.push_str(t.text);
                if t.kind != Kind::Space {
                    after_dot = t.text == ".";
                }
                continue;
            }
            let member = std::mem::take(&mut after_dot);
            let name = t.text;
            if member {
                // Swizzles and struct fields: only the reserved words need renaming.
                if listed(KEYWORDS, name) {
                    out.push_str("hz_u_");
                }
                out.push_str(name);
                continue;
            }
            if let Some(next) = self.sampling(name, i, end, out)? {
                i = next;
            } else if listed(PRECISION, name) {
                // Dropped: GLSL 450 desktop has no precision qualifiers worth keeping.
            } else if name.trim_start_matches(['i', 'u']).starts_with("sampler") {
                return Err(format!(
                    "sampler type `{name}` outside a uniform declaration"
                ));
            } else if self.sampler_shadow.contains_key(name) {
                return Err(format!(
                    "sampler `{name}` used other than as the first argument of a sampling call"
                ));
            } else if name == "gl_FragColor" {
                self.out.builtins.insert("hz_gl_FragData".into());
                out.push_str("hz_gl_FragData[0]");
            } else if listed(LEGACY, name) {
                self.out.builtins.insert(format!("hz_{name}"));
                out.push_str("hz_");
                out.push_str(name);
            } else if name == "gl_VertexID" || name == "gl_InstanceID" {
                out.push_str(if name == "gl_VertexID" {
                    "gl_VertexIndex"
                } else {
                    "gl_InstanceIndex"
                });
            } else if name.starts_with("gl_") && !listed(KEPT, name) {
                return Err(format!("unsupported built-in `{name}`"));
            } else if name == "ftransform" {
                self.out.builtins.insert("hz_ftransform".into());
                out.push_str("hz_ftransform");
            } else if name == "main" {
                out.push_str("hz_pack_main");
            } else if let Some(&n) = self.frag_outs.get(name) {
                self.out.builtins.insert("hz_gl_FragData".into());
                out.push_str(&format!("hz_gl_FragData[{n}]"));
            } else if listed(KEYWORDS, name) || self.user_builtins.contains(name) {
                out.push_str("hz_u_");
                out.push_str(name);
            } else {
                out.push_str(name);
            }
        }
        Ok(())
    }

    /// Rewrites `name(sampler, …)` (name at `i - 1`) into a host sampling
    /// call; returns the index after the call, or `None` if it is not one.
    fn sampling(
        &mut self,
        name: &str,
        i: usize,
        end: usize,
        out: &mut String,
    ) -> Result<Option<usize>, String> {
        let Some(open) = self.next_sig(i, end).filter(|&o| self.text(o) == "(") else {
            return Ok(None);
        };
        let (args, after) = self.call_args(open, end)?;
        let sig: Vec<usize> = (args[0].0..args[0].1)
            .filter(|&k| self.toks[k].kind != Kind::Space)
            .collect();
        let sampler = match sig[..] {
            [k] => self.text(k).to_string(),
            _ => return Ok(None),
        };
        let Some(&shadow) = self.sampler_shadow.get(&sampler) else {
            return Ok(None);
        };
        let plain = (2..=3).contains(&args.len());
        let (prefix, keep) = match name {
            "texture" if shadow && plain => ("hz_shadow_", 1..2),
            "texture2D" | "texture" if plain => ("hz_sample_", 1..2),
            "texture2DLod" | "textureLod" if args.len() == 3 => ("hz_sampleLod_", 1..3),
            "shadow2D" if args.len() == 2 => ("hz_shadow_", 1..2),
            "texelFetch" | "texelFetch2D" if args.len() == 3 => ("hz_fetch_", 1..3),
            "textureSize" | "textureSize2D" if args.len() == 2 => ("hz_size_", 1..2),
            // A sampler handed to a pack function (or an unknown sampling call) has no host form.
            _ => return Err(format!("sampler `{sampler}` passed to `{name}`")),
        };
        let func = format!("{prefix}{sampler}");
        out.push_str(&func);
        out.push('(');
        for (n, &(a, b)) in args[keep].iter().enumerate() {
            if n > 0 {
                out.push(',');
            }
            self.emit(a, b, out)?;
        }
        out.push(')');
        self.out.sample_calls.insert(func);
        Ok(Some(after))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naga_out::glsl_to_wgsl;

    /// A stand-in for the real host (S3): every recorded name becomes a plain
    /// global or a stub function, then naga must accept the whole stage.
    fn naga(rw: &Rewritten, stage: Stage) -> String {
        let mut s = String::from("#version 450\n");
        if !rw.uniforms.is_empty() {
            s += "layout(std140, set = 1, binding = 0) uniform HzUniforms {\n";
            for u in &rw.uniforms {
                s += &format!("    {} {};\n", u.ty, u.name);
            }
            s += "};\n";
        }
        s += "struct HzFog { vec4 color; float density; float start; float end; float scale; };\n";
        for b in &rw.builtins {
            let decl = match b.as_str() {
                "hz_gl_TextureMatrix" => "mat4 hz_gl_TextureMatrix[8];".into(),
                "hz_gl_FragData" => "vec4 hz_gl_FragData[8];".into(),
                "hz_gl_Normal" => "vec3 hz_gl_Normal;".into(),
                "hz_gl_NormalMatrix" => "mat3 hz_gl_NormalMatrix;".into(),
                "hz_gl_FogFragCoord" => "float hz_gl_FogFragCoord;".into(),
                "hz_gl_Fog" => "HzFog hz_gl_Fog;".into(),
                "hz_ftransform" => "vec4 hz_ftransform() { return vec4(0.0); }".into(),
                m if m.ends_with("Matrix") || m.ends_with("Inverse") => format!("mat4 {m};"),
                v => format!("vec4 {v};"),
            };
            s += &decl;
            s += "\n";
        }
        for v in rw.attributes.iter().chain(&rw.varyings) {
            let len = v.array_len.map(|n| format!("[{n}]")).unwrap_or_default();
            s += &format!("{} {}{len};\n", v.ty, v.name);
        }
        for f in &rw.sample_calls {
            let (ty, sig, ret) = match f.split('_').nth(1) {
                Some("sample") => ("vec4", "vec2 a", "vec4(0.0)"),
                Some("sampleLod") => ("vec4", "vec2 a, float l", "vec4(0.0)"),
                Some("shadow") => ("vec4", "vec3 a", "vec4(0.0)"),
                Some("fetch") => ("vec4", "ivec2 a, int l", "vec4(0.0)"),
                _ => ("ivec2", "int l", "ivec2(1)"),
            };
            s += &format!("{ty} {f}({sig}) {{ return {ret}; }}\n");
        }
        let write_out = stage == Stage::Fragment && rw.builtins.contains("hz_gl_FragData");
        if write_out {
            s += "layout(location = 0) out vec4 hz_out0;\n";
        }
        s += &rw.body;
        let out = if write_out {
            " hz_out0 = hz_gl_FragData[0];"
        } else {
            ""
        };
        s += &format!("\nvoid main() {{ hz_pack_main();{out} }}\n");
        glsl_to_wgsl(&s, stage).unwrap_or_else(|e| panic!("{e}\n--- source ---\n{s}"))
    }

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    const VS: &str = "precision highp float;
attribute vec4 mc_Entity;
uniform float rainStrength, frameTimeCounter;
uniform int worldTime;
uniform sampler2D lightmap;
flat varying float level;
varying vec2 texUV, lightUV;
varying vec3 tints[2];
varying vec4 ambient;
struct Light { vec3 color; float sample; };
const vec3 TABLE[2] = vec3[](vec3(0.0), vec3(0.25, 0.5, 0.75));
float round(float x) { return floor(x + 0.5); }
vec3 round(vec3 x) { return floor(x + 0.5); }
float early(float x) { if (x > 0.0) return x; return -x; float unreachable = x * 2.0; return unreachable; }
void main() {
    gl_Position = ftransform();
    texUV = (gl_TextureMatrix[0] * gl_MultiTexCoord0).st;
    lightUV = (gl_TextureMatrix[1] * gl_MultiTexCoord1).st;
    ambient = texture2D(lightmap, vec2(0.03, lightUV.t)) + texture2DLod(lightmap, lightUV, 0.0);
    level = float(mc_Entity.x == 3.0 || mc_Entity.x == 4);
    Light l = Light(gl_Color.rgb, 1.0e-3 + .5);
    mediump float sample = round(l.sample) + float(gl_VertexID % 2);
    tints[0] = round(TABLE[int(max(0.0, mc_Entity.x - 100.0))]) * sample;
    tints[1] = gl_NormalMatrix * gl_Normal + (gl_ModelViewMatrix * gl_Vertex).xyz;
    if (worldTime > 12000) { level += rainStrength * frameTimeCounter; }
}
";

    #[test]
    fn vertex_stage() {
        let rw = rewrite(VS, Stage::Vertex).unwrap();
        let var = |name: &str, ty: &str, flat, array_len| Var {
            name: name.into(),
            ty: ty.into(),
            flat,
            array_len,
        };
        assert_eq!(rw.attributes, [var("mc_Entity", "vec4", false, None)]);
        assert_eq!(
            rw.varyings,
            [
                var("level", "float", true, None),
                var("texUV", "vec2", false, None),
                var("lightUV", "vec2", false, None),
                var("tints", "vec3", false, Some(2)),
                var("ambient", "vec4", false, None),
            ]
        );
        let names: Vec<_> = rw.uniforms.iter().map(|u| u.name.as_str()).collect();
        assert_eq!(names, ["rainStrength", "frameTimeCounter", "worldTime"]);
        assert_eq!(rw.samplers, set(&["lightmap"]));
        assert_eq!(
            rw.sample_calls,
            set(&["hz_sample_lightmap", "hz_sampleLod_lightmap"])
        );
        let builtins = [
            "hz_ftransform",
            "hz_gl_Color",
            "hz_gl_ModelViewMatrix",
            "hz_gl_MultiTexCoord0",
        ];
        assert!(
            builtins.iter().all(|b| rw.builtins.contains(*b)),
            "{:?}",
            rw.builtins
        );
        for needle in [
            "void hz_pack_main()",
            "float hz_u_round(float x)",
            "l.hz_u_sample",
            "gl_VertexIndex",
        ] {
            assert!(
                rw.body.contains(needle),
                "{needle} missing from\n{}",
                rw.body
            );
        }
        assert!(rw.body.contains("return x; return -x;}") && !rw.body.contains("unreachable"));
        assert!(
            !rw.body.contains("highp")
                && !rw.body.contains("mediump")
                && !rw.body.contains("varying")
        );
        naga(&rw, Stage::Vertex);
    }

    const FS: &str = "uniform sampler2D gtexture;
uniform sampler2D colortex0, colortex1;
uniform sampler2DShadow shadowtex0;
uniform vec4 entityColor;
varying vec2 texUV;
varying vec4 ambient;
varying vec4 color;
vec4 sum(vec4 a, vec4 b) { return a + b; }
void main() {
    vec4 ambient = ambient;
    vec4 color = color * entityColor;
    vec4 albedo = texture2D(gtexture, texUV, -1.0)
        * texture(colortex0, texture2D(colortex1, sum(vec4(texUV, 0.0, 0.0), vec4(0.5)).xy).xy);
    ivec2 size = textureSize(colortex0, 0);
    albedo += texelFetch(colortex1, ivec2(gl_FragCoord.xy) % size, 0);
    albedo *= shadow2D(shadowtex0, vec3(texUV, 0.5)).x * texture(shadowtex0, vec3(texUV, 0.25)).x;
    gl_FragColor = albedo * ambient * color;
}
";

    #[test]
    fn fragment_stage() {
        let rw = rewrite(FS, Stage::Fragment).unwrap();
        assert_eq!(
            rw.samplers,
            set(&["gtexture", "colortex0", "colortex1", "shadowtex0"])
        );
        let calls = [
            "hz_sample_gtexture",
            "hz_sample_colortex0",
            "hz_sample_colortex1",
            "hz_size_colortex0",
            "hz_fetch_colortex1",
            "hz_shadow_shadowtex0",
        ];
        assert_eq!(rw.sample_calls, set(&calls));
        assert_eq!(rw.builtins, set(&["hz_gl_FragData"]));
        assert!(
            rw.body.contains("hz_sample_gtexture( texUV)"),
            "{}",
            rw.body
        );
        assert!(rw.body.contains("hz_gl_FragData[0] = albedo"));
        // naga scopes the local after its initializer (as GLSL does), so this reads the varying.
        assert!(rw.body.contains("vec4 ambient = ambient;"));
        naga(&rw, Stage::Fragment);
    }

    #[test]
    fn glsl_130_interface() {
        let vs = "in vec3 position;\nout vec2 uv;\nflat out int id;\n\
                  void main() { uv = position.xy; id = 1; gl_Position = vec4(position, 1.0); }\n";
        let rw = rewrite(vs, Stage::Vertex).unwrap();
        assert_eq!(rw.attributes.len(), 1);
        assert_eq!(rw.varyings.len(), 2);
        assert!(rw.varyings[1].flat);
        naga(&rw, Stage::Vertex);
        let fs = "in vec2 uv;\nlayout(location = 0) out vec4 outColor;\nout vec4 outNormal;\n\
                  void main() { outColor = vec4(uv, 0.0, 1.0); outNormal = outColor; }\n";
        let rw = rewrite(fs, Stage::Fragment).unwrap();
        assert!(
            rw.body.contains("hz_gl_FragData[1] = hz_gl_FragData[0]"),
            "{}",
            rw.body
        );
        naga(&rw, Stage::Fragment);
    }

    #[test]
    fn unsupported_input_is_an_error() {
        let cases = [
            (
                "uniform sampler2D s;\nvec4 f(sampler2D t) { return vec4(0.0); }",
                "sampler type",
            ),
            (
                "uniform sampler2D s;\nvoid main() { g(s); }",
                "passed to `g`",
            ),
            (
                "uniform sampler2D s;\nvoid main() { float x = s; }",
                "used other",
            ),
            (
                "uniform sampler2D s;\nvoid main() { texture2DGrad(s, vec2(0.0), vec2(0.0), vec2(0.0)); }",
                "passed",
            ),
            ("void main() { gl_TexCoord[0] = vec4(0.0); }", "gl_TexCoord"),
            ("uniform sampler2D s[2];", "sampler array"),
            ("void main() { {", "unbalanced"),
        ];
        for (src, msg) in cases {
            let err = rewrite(src, Stage::Fragment).unwrap_err();
            assert!(err.contains(msg), "{src}: {err}");
        }
    }

    #[test]
    fn numbers_are_single_tokens() {
        let toks: Vec<_> = tokenize("1.0e-3+2u*.5-0x1F")
            .into_iter()
            .map(|t| t.text)
            .collect();
        assert_eq!(toks, ["1.0e-3", "+", "2u", "*", ".5", "-", "0x1F"]);
    }

    /// `HORIZON_TEST_SHADERPACK_PREPROCESSED=<dir>`: every `.vsh`/`.fsh` in it
    /// (already preprocessed, made outside the repo) must rewrite and pass naga.
    #[test]
    fn preprocessed_pack_files() {
        let Ok(dir) = std::env::var("HORIZON_TEST_SHADERPACK_PREPROCESSED") else {
            return;
        };
        let mut paths: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        paths.sort();
        for path in paths {
            let stage = match path.extension().and_then(|e| e.to_str()) {
                Some("vsh") => Stage::Vertex,
                Some("fsh") => Stage::Fragment,
                _ => continue,
            };
            let src = std::fs::read_to_string(&path).unwrap();
            let rw = rewrite(&src, stage).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            naga(&rw, stage);
            println!("ok {}", path.display());
        }
    }
}
