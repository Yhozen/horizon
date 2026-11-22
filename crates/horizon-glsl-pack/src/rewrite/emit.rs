//! Copying the body with renames and the sampling rewrites.

use super::*;
use crate::host;

impl Ctx<'_> {
    pub(super) fn next_sig(&self, mut i: usize, end: usize) -> Option<usize> {
        while i < end && self.toks[i].kind == Kind::Space {
            i += 1;
        }
        (i < end).then_some(i)
    }

    /// The argument ranges of the call whose `(` is at `open`, and the index after its `)`.
    pub(super) fn call_args(
        &self,
        open: usize,
        end: usize,
    ) -> Result<(Vec<(usize, usize)>, usize), String> {
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
    pub(super) fn emit(
        &mut self,
        start: usize,
        end: usize,
        out: &mut String,
    ) -> Result<(), String> {
        let mut i = start;
        let mut after_dot = false;
        // A struct body's names are its members' (`struct R { float depth; }` beside an
        // `out float depth`): renamed like fields, not like the globals they shadow.
        let (mut struct_head, mut struct_depth) = (false, 0u32);
        while i < end {
            let t = self.toks[i];
            i += 1;
            if t.kind != Kind::Ident {
                out.push_str(t.text);
                if t.kind != Kind::Space {
                    after_dot = t.text == ".";
                }
                match t.text {
                    "{" if struct_head => (struct_head, struct_depth) = (false, 1),
                    "{" if struct_depth > 0 => struct_depth += 1,
                    "}" if struct_depth > 0 => struct_depth -= 1,
                    ";" => struct_head = false,
                    _ => {}
                }
                continue;
            }
            let member = std::mem::take(&mut after_dot) || struct_depth > 0;
            let name = t.text;
            if name == "struct" {
                struct_head = true;
            }
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
            } else if name == "dFdy" && self.stage == Stage::Fragment {
                // dFdy along GL's window y: the host declares `hz_dFdy_sign` (rows run
                // up in pack targets, down on the screen `final` draws), so view-space
                // derivatives keep GL's handedness (`cross(dFdx(p), dFdy(p))` normals).
                for ty in ["float", "vec2", "vec3", "vec4"] {
                    self.prelude.insert(format!(
                        "{ty} hz_dFdy({ty} v) {{ return hz_dFdy_sign * dFdy(v); }}"
                    ));
                }
                out.push_str("hz_dFdy");
            } else if name == "gl_FragCoord" {
                // A host global: GL's window depth from Horizon's reversed Z, and writable
                // (some packs nudge `.z` in place).
                self.out.builtins.insert("hz_FragCoord".into());
                out.push_str("hz_FragCoord");
            } else if matches!(
                name,
                "gl_FrontColor"
                    | "gl_BackColor"
                    | "gl_FrontSecondaryColor"
                    | "gl_BackSecondaryColor"
            ) {
                // Written by old vertex shaders and never read back: a dummy global.
                self.prelude.insert(format!("vec4 hz_{name};"));
                out.push_str("hz_");
                out.push_str(name);
            } else if name == "gl_FragColor" {
                self.out.builtins.insert("hz_gl_FragData".into());
                out.push_str("hz_gl_FragData[0]");
            } else if listed(LEGACY, name) {
                self.out.builtins.insert(format!("hz_{name}"));
                out.push_str("hz_");
                out.push_str(name);
            } else if name == "gl_VertexID" || name == "gl_InstanceID" {
                // GLSL's are `int`; naga's Vulkan-style built-ins are `uint`.
                out.push_str(if name == "gl_VertexID" {
                    "int(gl_VertexIndex)"
                } else {
                    "int(gl_InstanceIndex)"
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
    pub(super) fn sampling(
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
        let Some(&declared_shadow) = self.sampler_shadow.get(&sampler) else {
            return Ok(None);
        };
        let alias = sampler_alias(&sampler).to_string();
        let ty = self
            .out
            .sampler_types
            .get(&alias)
            .cloned()
            .unwrap_or_else(|| "sampler2D".into());
        let (dim, sample) = host::sampler_type(&ty)
            .ok_or_else(|| format!("`{ty} {sampler}` is not supported (sampled with `{name}`)"))?;
        // A sampler handed to a pack function (or an unknown sampling call) has no host form.
        let passed = || format!("sampler `{sampler}` passed to `{name}`");
        let form = CallForm::parse(name).ok_or_else(passed)?;
        let named_dim = if name.contains("3D") {
            Some(crate::TexDim::D3)
        } else if name.contains("2D") {
            Some(crate::TexDim::D2)
        } else {
            None
        };
        if named_dim.is_some_and(|d| d != dim) {
            return Err(format!("{} (a `{ty}`)", passed()));
        }
        let shadow = declared_shadow || form.legacy_shadow;
        let n = args.len();
        // Arguments after the coordinate, before the offset.
        let core = match form.op {
            Op::Implicit | Op::QueryLod => 0,
            Op::Lod | Op::Fetch => 1,
            Op::Grad => 2,
            Op::Gather => usize::from(shadow),
            Op::Levels => {
                if n != 1 {
                    return Err(passed());
                }
                let call = host::sampling_call("hz_levels_", dim, sample, &alias);
                out.push_str(&call);
                out.push_str("()");
                self.out.sample_calls.insert(call);
                return Ok(Some(after));
            }
            Op::Size => {
                if n > 2 {
                    return Err(passed());
                }
                let call = host::sampling_call("hz_size_", dim, sample, &alias);
                out.push_str(&call);
                out.push('(');
                match args.get(1) {
                    Some(&(a, b)) => self.emit(a, b, out)?,
                    None => out.push('0'),
                }
                out.push(')');
                self.out.sample_calls.insert(call);
                return Ok(Some(after));
            }
        };
        let fixed = 2 + core + usize::from(form.offset);
        // texture(s, P[, bias]), textureGather(s, P[, comp]).
        let optional = matches!(form.op, Op::Implicit) || (form.op == Op::Gather && !shadow);
        if n != fixed && !(optional && n == fixed + 1) {
            return Err(format!("`{name}` on `{sampler}` with {} arguments", n - 1));
        }
        let mut text = Vec::new();
        for &(a, b) in &args[1..] {
            let mut s = String::new();
            self.emit(a, b, &mut s)?;
            text.push(s);
        }
        let mut coord = text[0].clone();
        let d3 = dim == crate::TexDim::D3;
        if form.proj {
            // The projective forms divide by the last component.
            self.prelude.insert(
                "vec2 hz_proj(vec3 p) { return p.xy / p.z; }\n\
                 vec2 hz_proj(vec4 p) { return p.xy / p.w; }\n\
                 vec3 hz_proj3(vec4 p) { return p.xyz / p.w; }"
                    .into(),
            );
            let f = if shadow || d3 { "hz_proj3" } else { "hz_proj" };
            coord = format!("{f}({coord})");
        }
        if form.offset {
            // WGSL wants constant offsets: the offset moves the coordinate by whole texels
            // of level 0 instead (exact for fetches, and at level 0).
            let off = &text[1 + core];
            if form.op == Op::Fetch {
                coord = format!("(({coord}) + ({off}))");
            } else {
                let size = host::sampling_call("hz_size_", dim, sample, &alias);
                self.out.sample_calls.insert(size.clone());
                self.prelude.insert(
                    "vec2 hz_off(vec2 p, ivec2 o, ivec2 s) { return p + vec2(o) / vec2(s); }\n\
                     vec3 hz_off(vec3 p, ivec3 o, ivec3 s) { return p + vec3(o) / vec3(s); }\n\
                     vec3 hz_off_shadow(vec3 p, ivec2 o, ivec2 s) \
                     { return vec3(p.xy + vec2(o) / vec2(s), p.z); }\n\
                     vec4 hz_off_shadow(vec4 p, ivec2 o, ivec2 s) \
                     { return vec4(p.xy + vec2(o) / vec2(s) * p.w, p.zw); }"
                        .into(),
                );
                let f = if shadow && form.op != Op::Gather {
                    "hz_off_shadow"
                } else {
                    "hz_off"
                };
                coord = format!("{f}({coord}, {off}, {size}(0))");
            }
        }
        let extra: Vec<&str> = text[1..1 + core].iter().map(String::as_str).collect();
        let trailing = text.get(fixed - 1).map(String::as_str);
        // Modern names on shadow samplers return a float, the GLSL 120 ones a vec4.
        // A swizzle after it (`texture(shadowtex0, p).x`, accepted by many drivers) reads the
        // vec4 instead, whose channels are all the result.
        let swizzled = self
            .next_sig(after, end)
            .is_some_and(|k| self.text(k) == ".");
        let scalar = shadow && !form.legacy_shadow && !name.starts_with("texture2D") && !swizzled;
        let (op, args, wrap): (&str, Vec<String>, Option<String>) = match (form.op, shadow) {
            (Op::Implicit | Op::Grad, true) => ("hz_shadow_", vec![coord], None),
            (Op::Lod, true) => ("hz_shadowLod_", vec![coord, extra[0].into()], None),
            (Op::Gather, true) => (
                "hz_gather_",
                vec![coord, "0".into()],
                // GL's LEQUAL per texel: 1 where the reference is not behind.
                Some(format!("step(vec4({}), ", extra[0])),
            ),
            (Op::Implicit, false) => match trailing {
                Some(bias) => ("hz_sampleBias_", vec![coord, bias.into()], None),
                None => ("hz_sample_", vec![coord], None),
            },
            (Op::Lod, false) => ("hz_sampleLod_", vec![coord, extra[0].into()], None),
            (Op::Grad, false) => (
                "hz_sampleGrad_",
                vec![coord, extra[0].into(), extra[1].into()],
                None,
            ),
            (Op::Fetch, _) => ("hz_fetch_", vec![coord, extra[0].into()], None),
            (Op::Gather, false) => (
                "hz_gather_",
                vec![coord, trailing.unwrap_or("0").into()],
                None,
            ),
            (Op::QueryLod, _) => ("hz_queryLod_", vec![coord], None),
            (Op::Size | Op::Levels, _) => unreachable!(),
        };
        if d3 && matches!(op, "hz_gather_" | "hz_shadow_" | "hz_shadowLod_") {
            return Err(format!("`{name}` on the 3D sampler `{sampler}`"));
        }
        let func = host::sampling_call(op, dim, sample, &alias);
        if let Some(w) = &wrap {
            out.push_str(w);
        }
        out.push_str(&func);
        out.push('(');
        out.push_str(&args.join(","));
        out.push(')');
        if wrap.is_some() {
            out.push(')');
        }
        if scalar && op.starts_with("hz_shadow") {
            out.push_str(".x");
        }
        self.out.sample_calls.insert(func);
        Ok(Some(after))
    }
}

/// What a GLSL sampling function does, from its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    /// `texture`, `texture2D`…: the level from derivatives (and an optional bias).
    Implicit,
    Lod,
    Grad,
    Fetch,
    Size,
    Gather,
    QueryLod,
    /// `textureQueryLevels`.
    Levels,
}

#[derive(Clone, Copy, Debug)]
struct CallForm {
    op: Op,
    /// `…Proj…`: the coordinate is divided by its last component.
    proj: bool,
    /// `…Offset`: an integer texel offset follows the core arguments.
    offset: bool,
    /// `shadow2D…`: a comparison returning a `vec4`.
    legacy_shadow: bool,
}

impl CallForm {
    /// `texture2DProjLod` → Lod + proj; `textureGatherOffset` → Gather + offset…
    fn parse(name: &str) -> Option<Self> {
        let mut f = CallForm {
            op: Op::Implicit,
            proj: false,
            offset: false,
            legacy_shadow: false,
        };
        let strip = |rest: &mut &str, word: &str| {
            let found = rest.strip_prefix(word);
            if let Some(r) = found {
                *rest = r;
            }
            found.is_some()
        };
        let mut rest;
        if let Some(r) = name.strip_prefix("shadow2D") {
            rest = r;
            f.legacy_shadow = true;
            f.proj = strip(&mut rest, "Proj");
            if strip(&mut rest, "Lod") {
                f.op = Op::Lod;
            }
        } else if let Some(r) = name.strip_prefix("texelFetch") {
            rest = r;
            f.op = Op::Fetch;
            let _ = strip(&mut rest, "2D") || strip(&mut rest, "3D");
            f.offset = strip(&mut rest, "Offset");
        } else if let Some(r) = name.strip_prefix("textureSize") {
            rest = r;
            f.op = Op::Size;
            let _ = strip(&mut rest, "2D") || strip(&mut rest, "3D");
        } else if name == "textureQueryLevels" {
            rest = "";
            f.op = Op::Levels;
        } else if let Some(r) = name.strip_prefix("textureQueryLod") {
            rest = r;
            f.op = Op::QueryLod;
        } else if let Some(r) = name.strip_prefix("textureGather") {
            rest = r;
            f.op = Op::Gather;
            f.offset = strip(&mut rest, "Offset");
        } else {
            rest = name.strip_prefix("texture")?;
            let _ = strip(&mut rest, "2D") || strip(&mut rest, "3D");
            f.proj = strip(&mut rest, "Proj");
            if strip(&mut rest, "Lod") {
                f.op = Op::Lod;
            } else if strip(&mut rest, "Grad") {
                f.op = Op::Grad;
            }
            f.offset = strip(&mut rest, "Offset");
        }
        let _ = strip(&mut rest, "ARB") || strip(&mut rest, "EXT");
        rest.is_empty().then_some(f)
    }
}

/// The name a sampler goes by after the rewrite: `texture`, GLSL 120's albedo
/// sampler, is a GLSL 450 built-in function, so it becomes `gtexture`.
pub(super) fn sampler_alias(name: &str) -> &str {
    if name == "texture" { "gtexture" } else { name }
}
