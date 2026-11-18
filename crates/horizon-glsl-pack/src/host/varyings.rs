//! The varyings both stages share, and their interface declarations.

use super::*;

/// A varying both stages declare, at `location` (arrays take one per element).
pub(super) struct Shared {
    pub(super) name: String,
    pub(super) ty: String,
    pub(super) flat: bool,
    pub(super) len: Option<u32>,
    pub(super) location: u32,
}

impl Shared {
    /// `(location, suffix of the interface variable, index of the global)`.
    pub(super) fn elements(&self) -> Vec<(u32, String, String)> {
        match self.len {
            None => vec![(self.location, String::new(), String::new())],
            Some(n) => (0..n)
                .map(|k| (self.location + k, format!("_{k}"), format!("[{k}]")))
                .collect(),
        }
    }
}

/// Whether a varying type is integer (`Some(true)`), float (`Some(false)`) or
/// not allowed between stages in WGSL (matrices, booleans).
pub(super) fn integer_type(ty: &str) -> Option<bool> {
    match ty {
        "float" | "vec2" | "vec3" | "vec4" => Some(false),
        "int" | "uint" | "ivec2" | "ivec3" | "ivec4" | "uvec2" | "uvec3" | "uvec4" => Some(true),
        _ => None,
    }
}

pub(super) fn shared_varyings(
    vertex: &Rewritten,
    fragment: &Rewritten,
    geometry: Geometry,
) -> Result<Vec<Shared>, String> {
    let mut by_name = BTreeMap::new();
    for f in &fragment.varyings {
        let Some(v) = vertex.varyings.iter().find(|v| v.name == f.name) else {
            continue;
        };
        if v.ty != f.ty || v.array_len != f.array_len {
            return Err(format!(
                "varying {} has different types in the two stages",
                v.name
            ));
        }
        let integer =
            integer_type(&v.ty).ok_or_else(|| format!("varying {} of type {}", v.name, v.ty))?;
        // WGSL wants both ends to agree, and integers can only be flat.
        by_name.insert(
            v.name.clone(),
            (v.ty.clone(), v.flat || f.flat || integer, v.array_len),
        );
    }
    for (name, ty) in BUILTIN_VARYINGS {
        if fragment.builtins.contains(name) {
            by_name.insert(name.to_string(), (ty.to_string(), false, None));
        }
    }
    let mut location = 0;
    let mut shared = Vec::new();
    for (name, (ty, flat, len)) in by_name {
        shared.push(Shared {
            name,
            ty,
            flat,
            len,
            location,
        });
        location += len.unwrap_or(1);
    }
    // Terrain adds `hz_sprite` after the pack's varyings.
    let total = location + u32::from(geometry == Geometry::Terrain);
    if total > MAX_LOCATIONS {
        return Err(format!(
            "{total} varying locations, WebGPU allows {MAX_LOCATIONS}"
        ));
    }
    Ok(shared)
}

pub(super) fn declare(out: &mut String, var: &Var) {
    match var.array_len {
        Some(n) => writeln!(out, "{} {}[{n}];", var.ty, var.name),
        None => writeln!(out, "{} {};", var.ty, var.name),
    }
    .unwrap();
}

pub(super) fn interface(out: &mut String, shared: &[Shared], dir: &str) {
    for s in shared {
        let flat = if s.flat { "flat " } else { "" };
        for (location, suffix, _) in s.elements() {
            writeln!(
                out,
                "layout(location = {location}) {flat}{dir} {} hz_v_{}{suffix};",
                s.ty, s.name
            )
            .unwrap();
        }
    }
}
