//! Interface declarations (`attribute`, `varying`, `uniform`, `in`, `out`): recorded and dropped.

use super::*;

/// Whether a GLSL type is an image (`image2D`, `uimage3D`…).
pub(super) fn is_image(ty: &str) -> bool {
    ["image", "iimage", "uimage"]
        .iter()
        .any(|p| ty.starts_with(p))
}

impl Ctx<'_> {
    pub(super) fn text(&self, i: usize) -> &str {
        self.toks[i].text
    }

    /// Records and drops an interface declaration (`attribute`, `varying`,
    /// `uniform`, `in`, `out`). Returns false for anything else.
    pub(super) fn declaration(&mut self, sig: &[usize]) -> Result<bool, String> {
        let mut words: Vec<&str> = sig.iter().map(|&i| self.toks[i].text).collect();
        if words.last() == Some(&";") {
            words.pop();
        }
        // `layout(...)` is skipped (locations come from declaration order) but
        // for an image's format: a word not followed by `=`.
        let mut k = 0;
        let mut format = None;
        if words.first() == Some(&"layout") {
            k = words
                .iter()
                .position(|w| *w == ")")
                .map_or(words.len(), |p| p + 1);
            format = words[..k]
                .windows(2)
                .find(|w| {
                    w[0].starts_with(|c: char| c.is_ascii_alphabetic())
                        && matches!(w[1], "," | ")")
                        && !matches!(w[0], "std140" | "std430" | "shared" | "packed")
                })
                .map(|w| w[0].to_ascii_lowercase());
        }
        let mut quals = Vec::new();
        while k + 2 < words.len() && (listed(QUALIFIERS, words[k]) || listed(MEMORY, words[k])) {
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
                // GLSL 330 allows `uniform T x = v;`: Horizon's value wins for a uniform it
                // provides; any other keeps `v` as a plain global instead of the zero fallback.
                if storage != "uniform" || array_len.is_some() {
                    return Err(format!("initializer on `{storage} {name}`"));
                }
                let mut depth = 0i32;
                let len = rest[next + 1..]
                    .iter()
                    .position(|w| {
                        match *w {
                            "(" | "[" => depth += 1,
                            ")" | "]" => depth -= 1,
                            _ => {}
                        }
                        depth == 0 && *w == ","
                    })
                    .unwrap_or(rest.len() - next - 1);
                let init = rest[next + 1..next + 1 + len].join(" ");
                if crate::uniforms::uniform_type(name).is_none() {
                    self.prelude.insert(format!("{ty} {name} = {init};"));
                } else {
                    self.record(storage, &ty, name, flat, array_len)?;
                }
                match rest.get(next + 1 + len) {
                    None => break,
                    _ => rest = &rest[next + 2 + len..],
                }
                continue;
            }
            if storage == "uniform" && is_image(&ty) {
                if array_len.is_some() {
                    return Err(format!("image array `{name}`"));
                }
                let access = match (quals.contains(&"readonly"), quals.contains(&"writeonly")) {
                    (true, false) => ImageAccess::ReadOnly,
                    (false, true) => ImageAccess::WriteOnly,
                    _ => ImageAccess::ReadWrite,
                };
                self.out.images.push(ImageUniform {
                    name: name.into(),
                    ty: ty.clone(),
                    format: format.clone(),
                    access,
                    calls: BTreeSet::new(),
                });
            } else {
                self.record(storage, &ty, name, flat, array_len)?;
            }
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

    /// Records the samplers of a top-level `uniform sampler… a, b;` ahead of
    /// the pass that emits the body (errors are left to that pass).
    pub(super) fn sampler_declaration(&mut self, sig: &[usize]) {
        let words: Vec<&str> = sig.iter().map(|&i| self.toks[i].text).collect();
        let Some(u) = words.iter().position(|w| *w == "uniform") else {
            return;
        };
        if words.contains(&"{") || words.contains(&"(") && words.first() != Some(&"layout") {
            return;
        }
        let Some(t) = words[u..]
            .iter()
            .position(|w| w.trim_start_matches(['i', 'u']).starts_with("sampler"))
            .map(|t| t + u)
        else {
            return;
        };
        let ty = words[t];
        for w in words[t + 1..].windows(2) {
            if matches!(w[1], "," | ";")
                && w[0].starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            {
                self.sampler_shadow
                    .insert(w[0].into(), ty.ends_with("Shadow"));
                self.out
                    .sampler_types
                    .insert(emit::sampler_alias(w[0]).into(), ty.into());
            }
        }
    }

    pub(super) fn record(
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
                self.out.samplers.insert(emit::sampler_alias(name).into());
                self.out
                    .sampler_types
                    .insert(emit::sampler_alias(name).into(), ty.into());
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
}
