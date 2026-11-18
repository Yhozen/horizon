//! Copying the body with renames and the sampling rewrites.

use super::*;

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
            } else if name == "dFdy" && self.stage == Stage::Fragment {
                // The y flip lives in the projection, so window y runs against view-space
                // y: negated, view-space derivatives keep GL's handedness (packs rebuild
                // normals with `cross(dFdx(p), dFdy(p))`).
                for ty in ["float", "vec2", "vec3", "vec4"] {
                    self.prelude
                        .insert(format!("{ty} hz_dFdy({ty} v) {{ return -dFdy(v); }}"));
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
        let Some(&shadow) = self.sampler_shadow.get(&sampler) else {
            return Ok(None);
        };
        let n = args.len();
        let (prefix, keep) = match name {
            "texture" | "texture2D" if shadow && (2..=3).contains(&n) => ("hz_shadow_", 1..2),
            "texture2D" | "texture" if n == 2 => ("hz_sample_", 1..2),
            "texture2D" | "texture" if n == 3 => ("hz_sampleBias_", 1..3),
            "texture2DProj" | "textureProj" if !shadow && (2..=3).contains(&n) => {
                ("hz_sample_", 1..2)
            }
            "texture2DLod" | "textureLod" if n == 3 => ("hz_sampleLod_", 1..3),
            "texture2DGrad" | "texture2DGradARB" | "textureGrad" if n == 4 => {
                ("hz_sampleGrad_", 1..4)
            }
            "shadow2D" if (2..=3).contains(&n) => ("hz_shadow_", 1..2),
            "shadow2DLod" if n == 3 => ("hz_shadowLod_", 1..3),
            "texelFetch" | "texelFetch2D" if n == 3 => ("hz_fetch_", 1..3),
            "textureSize" | "textureSize2D" if n == 2 => ("hz_size_", 1..2),
            // A sampler handed to a pack function (or an unknown sampling call) has no host form.
            _ => return Err(format!("sampler `{sampler}` passed to `{name}`")),
        };
        let func = format!("{prefix}{}", sampler_alias(&sampler));
        out.push_str(&func);
        // The projective forms divide by the last component, a vec3's z or a vec4's w.
        let proj = name.ends_with("Proj");
        if proj {
            self.prelude.insert(
                "vec2 hz_proj(vec3 p) { return p.xy / p.z; }\n\
                 vec2 hz_proj(vec4 p) { return p.xy / p.w; }"
                    .into(),
            );
            out.push_str("(hz_proj");
        }
        out.push('(');
        for (n, &(a, b)) in args[keep].iter().enumerate() {
            if n > 0 {
                out.push(',');
            }
            self.emit(a, b, out)?;
        }
        out.push(')');
        if proj {
            out.push(')');
        }
        self.out.sample_calls.insert(func);
        Ok(Some(after))
    }
}

/// The name a sampler goes by after the rewrite: `texture`, GLSL 120's albedo
/// sampler, is a GLSL 450 built-in function, so it becomes `gtexture`.
pub(super) fn sampler_alias(name: &str) -> &str {
    if name == "texture" { "gtexture" } else { name }
}
