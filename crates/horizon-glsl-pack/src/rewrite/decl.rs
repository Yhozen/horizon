//! Interface declarations (`attribute`, `varying`, `uniform`, `in`, `out`): recorded and dropped.

use super::*;

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
