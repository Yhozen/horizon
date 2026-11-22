//! The varyings both stages share, and their interface declarations.
//!
//! Each shared varying splits into *pieces*: one per array element and
//! matrix column, each a scalar or vector (WGSL passes no matrices or arrays
//! between stages). A piece normally has its own location (`hz_v_<name>`,
//! `_<k>` per element, `_c<j>` per column). When the pieces, plus Horizon's
//! own `hz_sprite` on terrain, need more locations than
//! [`HostParams::max_varyings`], they are packed into `vec4` slots instead
//! (`hz_vp<n>`): interpolated floats together in `vec4`s, integers and flat
//! floats (as their bits) together in flat `ivec4`s. Packed, a vector the
//! fragment stage only reads through swizzles carries just those
//! components, a piece may straddle two slots when that saves one, and
//! terrain's `hz_sprite` travels as one component of the slots (see
//! [`SpriteCarrier`]) instead of a location of its own.

use super::*;

/// One interface variable.
pub(super) struct Slot {
    pub(super) name: String,
    pub(super) ty: String,
    pub(super) flat: bool,
}

/// Where part of a piece travels.
struct Segment {
    slot: usize,
    /// The slot's components (`.yzw`), or `None` when it is the whole slot.
    slot_swizzle: Option<String>,
    /// The piece's components it carries (`xy`); empty for the whole piece.
    source: String,
}

/// A scalar or vector of a varying, carried in (parts of) slots.
struct Piece {
    /// The global it comes from and goes to (`m[2]`, `w[1][0]`).
    global: String,
    vertex_ty: String,
    fragment_ty: String,
    segments: Vec<Segment>,
    /// Some components are not carried (the fragment stage never reads them): they read 0.
    partial: bool,
    /// A flat float carried as its bits in an integer slot.
    bits: bool,
}

/// How terrain's `hz_sprite` (all of it follows from the vertex's
/// `hz_in_layer`, of which the fragment stage reads the low 16 bits) gets
/// to the fragment stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SpriteCarrier {
    /// Its own flat `uvec4` location after the slots.
    Own,
    /// A component of a flat `ivec4` slot.
    Bits(usize, u8),
    /// A component of an interpolated `vec4` slot, rounded on arrival: the
    /// value is the same at every vertex of a face and below 2^16, so the
    /// interpolation's rounding error (well under 0.5) cannot change it.
    Float(usize, u8),
}

/// What the two stages pass each other.
pub(super) struct Interface {
    pub(super) slots: Vec<Slot>,
    pieces: Vec<Piece>,
    /// Fragment varyings the vertex stage does not declare: they read zero.
    zeroed: Vec<Var>,
    pub(super) sprite: SpriteCarrier,
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

/// Components of a scalar or vector type (`vec3` → 3).
fn width(ty: &str) -> u32 {
    ty.bytes()
        .last()
        .filter(u8::is_ascii_digit)
        .map_or(1, |d| u32::from(d - b'0'))
}

/// `matCxR` (`mat3` = `mat3x3`) → (columns, column type).
fn matrix(ty: &str) -> Option<(u32, String)> {
    let dims = ty.strip_prefix("mat")?;
    let (c, r) = match dims.as_bytes() {
        [c] => (*c, *c),
        [c, b'x', r] => (*c, *r),
        _ => return None,
    };
    let ok = |d: u8| (b'2'..=b'4').contains(&d);
    (ok(c) && ok(r)).then(|| (u32::from(c - b'0'), format!("vec{}", r - b'0')))
}

/// The scalar or vector type of `width` components, with the base of `like`.
fn vector(like: &str, width: u32) -> String {
    let (scalar, prefix) = match like.as_bytes()[0] {
        b'i' => ("int", "ivec"),
        b'u' => ("uint", "uvec"),
        _ => ("float", "vec"),
    };
    if width == 1 {
        scalar.into()
    } else {
        format!("{prefix}{width}")
    }
}

/// `value` converted to `to` when its type `from` differs.
fn convert(to: &str, from: &str, value: &str) -> String {
    if to == from {
        value.into()
    } else {
        format!("{to}({value})")
    }
}

impl Interface {
    /// Matches the stages' varyings (and the built-ins GLSL carries between
    /// them), splits them into pieces and gives them locations: one each
    /// when they fit `max` (with `hz_sprite` on terrain), else packed.
    pub(super) fn new(
        vertex: &Rewritten,
        fragment: &Rewritten,
        geometry: Geometry,
        max: u32,
    ) -> Result<Self, String> {
        // name → (vertex type, fragment type, flat, array length)
        let mut by_name = BTreeMap::new();
        let mut zeroed = Vec::new();
        // Struct varyings travel member by member.
        let bodies = [vertex.body.as_str(), fragment.body.as_str()];
        let vertex_vars = expand_structs(&vertex.varyings, &bodies);
        let fragment_vars = expand_structs(&fragment.varyings, &bodies);
        for f in &fragment_vars {
            // A varying the fragment stage never reads costs no location.
            let base = f.name.split(['.', '[']).next().unwrap_or(&f.name);
            if !uses(&fragment.body, base) {
                continue;
            }
            let Some(v) = vertex_vars.iter().find(|v| v.name == f.name) else {
                zeroed.push(f.clone());
                continue;
            };
            let mismatch = || {
                format!(
                    "varying {} is {} in the vertex stage but {} in the fragment stage",
                    v.name, v.ty, f.ty
                )
            };
            if v.array_len != f.array_len {
                return Err(mismatch());
            }
            let flat = v.flat || f.flat;
            if v.ty == f.ty && matrix(&v.ty).is_some() {
                by_name.insert(
                    v.name.clone(),
                    (v.ty.clone(), f.ty.clone(), flat, v.array_len),
                );
                continue;
            }
            let (Some(vi), Some(fi)) = (integer_type(&v.ty), integer_type(&f.ty)) else {
                return Err(format!("varying {} of type {}", v.name, v.ty));
            };
            // Same number of components: GLSL's constructor converts.
            if width(&v.ty) != width(&f.ty) {
                return Err(mismatch());
            }
            // WGSL wants both ends to agree, and integers can only be flat.
            by_name.insert(
                v.name.clone(),
                (v.ty.clone(), f.ty.clone(), flat || vi || fi, v.array_len),
            );
        }
        for (name, ty) in BUILTIN_VARYINGS {
            if fragment.builtins.contains(name) {
                by_name.insert(
                    name.to_string(),
                    (ty.to_string(), ty.to_string(), false, None),
                );
            }
        }
        // (suffix, global, vertex type, fragment type, flat)
        let mut parts = Vec::new();
        for (name, (vty, fty, flat, len)) in by_name {
            let elements: Vec<(String, String)> = match len {
                None => vec![(String::new(), String::new())],
                Some(n) => (0..n)
                    .map(|k| (format!("_{k}"), format!("[{k}]")))
                    .collect(),
            };
            for (suffix, index) in elements {
                match matrix(&vty) {
                    Some((columns, column)) => {
                        for j in 0..columns {
                            parts.push((
                                format!("{name}{suffix}_c{j}"),
                                format!("{name}{index}[{j}]"),
                                column.clone(),
                                column.clone(),
                                flat,
                            ));
                        }
                    }
                    None => parts.push((
                        format!("{name}{suffix}"),
                        format!("{name}{index}"),
                        vty.clone(),
                        fty.clone(),
                        flat,
                    )),
                }
            }
        }
        let terrain = geometry == Geometry::Terrain;
        let mut interface = Self {
            slots: Vec::new(),
            pieces: Vec::new(),
            zeroed,
            sprite: SpriteCarrier::Own,
        };
        if parts.len() as u32 + u32::from(terrain) <= max {
            for (suffix, global, vertex_ty, fragment_ty, flat) in parts {
                interface.pieces.push(Piece {
                    global,
                    vertex_ty: vertex_ty.clone(),
                    fragment_ty,
                    segments: vec![Segment {
                        slot: interface.slots.len(),
                        slot_swizzle: None,
                        source: String::new(),
                    }],
                    partial: false,
                    bits: false,
                });
                interface.slots.push(Slot {
                    name: format!("hz_v_{}", suffix.replace(['.', '[', ']'], "_")),
                    ty: vertex_ty,
                    flat,
                });
            }
        } else {
            interface.pack(parts, terrain, &fragment.body);
        }
        let own = u32::from(interface.sprite == SpriteCarrier::Own && terrain);
        let total = interface.slots.len() as u32 + own;
        if total > max {
            return Err(format!(
                "{total} varying locations after packing, the device allows {max}"
            ));
        }
        Ok(interface)
    }

    /// Packs pieces into 4-wide slots, widest first, one group of slots per
    /// kind: interpolated floats, then everything flat (integers, and flat
    /// floats as their bits) in `ivec4`s. A vector the fragment stage reads
    /// only through swizzles carries just the components it reads; pieces
    /// straddle slots when first fit would take more than the components need;
    /// on terrain, `hz_sprite` takes a free component (see [`SpriteCarrier`]).
    fn pack(
        &mut self,
        parts: Vec<(String, String, String, String, bool)>,
        terrain: bool,
        fragment_body: &str,
    ) {
        // (global, vertex type, fragment type, carried components, flat group, bits)
        let mut groups: [Vec<(String, String, String, String, bool)>; 2] = Default::default();
        for (_, global, vertex_ty, fragment_ty, piece_flat) in parts {
            let integer =
                integer_type(&vertex_ty) == Some(true) || integer_type(&fragment_ty) == Some(true);
            let w = width(&vertex_ty) as usize;
            let all = &"xyzw"[..w];
            let plain = global
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_');
            let comps = if w > 1 && plain {
                read_components(fragment_body, &global).unwrap_or_else(|| all.to_string())
            } else {
                all.to_string()
            };
            let comps = if comps.is_empty() {
                all.to_string()
            } else {
                comps
            };
            let bits = piece_flat && !integer;
            let g = usize::from(piece_flat || integer);
            groups[g].push((global, vertex_ty, fragment_ty, comps, bits));
        }
        // hz_sprite: a free component, preferably flat (exact bits).
        let total = |g: usize| groups[g].iter().map(|m| m.3.len()).sum::<usize>();
        let (smooth, flat) = (total(0), total(1));
        let sprite_group = if !terrain {
            None
        } else if flat % 4 != 0 {
            Some(1)
        } else if smooth % 4 != 0 {
            Some(0)
        } else if flat > 0 {
            Some(1)
        } else {
            None
        };
        for (g, (slot_ty, flat)) in [("vec4", false), ("ivec4", true)].into_iter().enumerate() {
            let mut members = std::mem::take(&mut groups[g]);
            // Stable: equal widths keep their name order.
            members.sort_by_key(|m| std::cmp::Reverse(m.3.len()));
            let mut widths: Vec<u32> = members.iter().map(|m| m.3.len() as u32).collect();
            if sprite_group == Some(g) {
                widths.push(1);
            }
            if widths.is_empty() {
                continue;
            }
            let (count, places) = layout(&widths);
            let first = self.slots.len();
            for k in 0..count {
                self.slots.push(Slot {
                    name: format!("hz_vp{}", first + k),
                    ty: slot_ty.into(),
                    flat,
                });
            }
            let lanes = |offset: u32, len: u32| {
                "xyzw"[offset as usize..(offset + len) as usize].to_string()
            };
            for (m, place) in members.into_iter().zip(&places) {
                let (global, vertex_ty, fragment_ty, comps, bits) = m;
                let w = width(&vertex_ty) as usize;
                // A piece carried whole in one segment needs no swizzle of its own.
                let scalar = w == 1 || (comps.len() == w && place.len() == 1);
                let segments = place
                    .iter()
                    .map(|&(slot, offset, len, from)| Segment {
                        slot: first + slot,
                        slot_swizzle: Some(lanes(offset, len)),
                        source: if scalar {
                            String::new()
                        } else {
                            comps[from as usize..(from + len) as usize].to_string()
                        },
                    })
                    .collect();
                self.pieces.push(Piece {
                    partial: comps.len() < w,
                    global,
                    vertex_ty,
                    fragment_ty,
                    segments,
                    bits,
                });
            }
            if sprite_group == Some(g) {
                let &(slot, offset, _, _) = &places[places.len() - 1][0];
                self.sprite = if flat {
                    SpriteCarrier::Bits(first + slot, offset as u8)
                } else {
                    SpriteCarrier::Float(first + slot, offset as u8)
                };
            }
        }
    }

    /// The location after the pack's slots (terrain's `hz_sprite`).
    pub(super) fn next_location(&self) -> u32 {
        self.slots.len() as u32
    }

    /// The interface variables, `dir` = `out` (vertex) or `in` (fragment).
    pub(super) fn declare(&self, out: &mut String, dir: &str) {
        for (location, s) in self.slots.iter().enumerate() {
            let flat = if s.flat { "flat " } else { "" };
            writeln!(
                out,
                "layout(location = {location}) {flat}{dir} {} {};",
                s.ty, s.name
            )
            .unwrap();
        }
    }

    /// The vertex `main`'s statements handing the globals on.
    pub(super) fn write(&self, out: &mut String) {
        for (k, s) in self.slots.iter().enumerate() {
            let packed = self
                .pieces
                .iter()
                .flat_map(|p| &p.segments)
                .any(|g| g.slot == k && g.slot_swizzle.is_some());
            if packed
                || matches!(self.sprite, SpriteCarrier::Bits(s, _) | SpriteCarrier::Float(s, _) if s == k)
            {
                // Unused components are written too: WGSL outputs have no undefined parts.
                writeln!(out, "    {} = {}(0);", s.name, s.ty).unwrap();
            }
        }
        for p in &self.pieces {
            for g in &p.segments {
                let slot = &self.slots[g.slot];
                let (target, len) = match &g.slot_swizzle {
                    Some(sw) => (format!("{}.{sw}", slot.name), sw.len() as u32),
                    None => (slot.name.clone(), width(&slot.ty)),
                };
                let source = component(&p.global, &g.source);
                let from = vector(
                    &p.vertex_ty,
                    if g.source.is_empty() {
                        width(&p.vertex_ty)
                    } else {
                        len
                    },
                );
                let value = if p.bits {
                    format!("floatBitsToInt({source})")
                } else {
                    convert(&vector(&slot.ty, len), &from, &source)
                };
                writeln!(out, "    {target} = {value};").unwrap();
            }
        }
    }

    /// The vertex statement handing `hz_sprite` on, once it is computed.
    pub(super) fn write_sprite(&self, out: &mut String) {
        match self.sprite {
            SpriteCarrier::Own => {}
            SpriteCarrier::Bits(k, c) => {
                let lane = char::from(b"xyzw"[usize::from(c)]);
                writeln!(out, "    {}.{lane} = int(hz_sprite.x);", self.slots[k].name).unwrap();
            }
            SpriteCarrier::Float(k, c) => {
                let lane = char::from(b"xyzw"[usize::from(c)]);
                writeln!(
                    out,
                    "    {}.{lane} = float(hz_sprite.x & 0xffffu);",
                    self.slots[k].name
                )
                .unwrap();
            }
        }
    }

    /// The fragment `main`'s statements rebuilding `hz_sprite` (as the vertex
    /// stage computes it from `hz_in_layer`, `grid` = the sprite grid).
    pub(super) fn read_sprite(&self, out: &mut String, grid: u32) {
        let (k, c, value) = match self.sprite {
            SpriteCarrier::Own => return,
            SpriteCarrier::Bits(k, c) => (k, c, "uint({})"),
            SpriteCarrier::Float(k, c) => (k, c, "uint(round({}))"),
        };
        let lane = char::from(b"xyzw"[usize::from(c)]);
        let read = value.replace("{}", &format!("{}.{lane}", self.slots[k].name));
        writeln!(
            out,
            "    uint hz_l = {read};\n    uint hz_n = hz_l & 0x7fffu;\n    \
             hz_sprite = uvec4(hz_l, hz_n % {per}u % {grid}u, hz_n % {per}u / {grid}u, hz_n / {per}u);",
            per = grid * grid
        )
        .unwrap();
    }

    /// The fragment `main`'s statements filling the globals.
    pub(super) fn read(&self, out: &mut String) {
        for p in &self.pieces {
            if p.partial {
                writeln!(out, "    {} = {}(0);", p.global, p.fragment_ty).unwrap();
            }
            if !p.partial && p.segments.len() > 1 {
                // One constructor of the parts: naga cannot store into a
                // swizzle of a matrix column (`tbn[2].x = …`).
                let parts: Vec<String> = p
                    .segments
                    .iter()
                    .map(|g| {
                        let slot = &self.slots[g.slot];
                        let sw = g.slot_swizzle.as_deref().unwrap_or("xyzw");
                        let source = format!("{}.{sw}", slot.name);
                        let len = sw.len() as u32;
                        if p.bits {
                            format!("intBitsToFloat({source})")
                        } else {
                            convert(
                                &vector(&p.fragment_ty, len),
                                &vector(&slot.ty, len),
                                &source,
                            )
                        }
                    })
                    .collect();
                writeln!(
                    out,
                    "    {} = {}({});",
                    p.global,
                    p.fragment_ty,
                    parts.join(", ")
                )
                .unwrap();
                continue;
            }
            for g in &p.segments {
                let slot = &self.slots[g.slot];
                let (source, len) = match &g.slot_swizzle {
                    Some(sw) => (format!("{}.{sw}", slot.name), sw.len() as u32),
                    None => (slot.name.clone(), width(&slot.ty)),
                };
                let target = component(&p.global, &g.source);
                let to = vector(
                    &p.fragment_ty,
                    if g.source.is_empty() {
                        width(&p.fragment_ty)
                    } else {
                        len
                    },
                );
                let value = if p.bits {
                    format!("intBitsToFloat({source})")
                } else {
                    convert(&to, &vector(&slot.ty, len), &source)
                };
                writeln!(out, "    {target} = {value};").unwrap();
            }
        }
        for z in &self.zeroed {
            match z.array_len {
                Some(n) => writeln!(
                    out,
                    "    for (int i = 0; i < {n}; i++) {{ {}[i] = {}(0); }}",
                    z.name, z.ty
                ),
                None => writeln!(out, "    {} = {}(0);", z.name, z.ty),
            }
            .unwrap();
        }
    }
}

/// `global.xy`, or `global` for its whole value.
fn component(global: &str, source: &str) -> String {
    if source.is_empty() {
        global.to_string()
    } else {
        format!("{global}.{source}")
    }
}

/// Slots for pieces of `widths` (widest first): `(slot count, per piece its
/// segments (slot, offset in the slot, length, offset in the piece))`. First
/// fit; when that takes more slots than the components need, the pieces are
/// laid end to end instead, straddling slots.
#[allow(clippy::type_complexity)]
fn layout(widths: &[u32]) -> (usize, Vec<Vec<(usize, u32, u32, u32)>>) {
    let need = widths.iter().sum::<u32>().div_ceil(4) as usize;
    let mut used: Vec<u32> = Vec::new();
    let mut places = Vec::new();
    for &w in widths {
        let at = match used.iter().position(|u| u + w <= 4) {
            Some(i) => i,
            None => {
                used.push(0);
                used.len() - 1
            }
        };
        places.push(vec![(at, used[at], w, 0)]);
        used[at] += w;
    }
    if used.len() <= need {
        return (used.len(), places);
    }
    let mut places = Vec::new();
    let mut at = 0u32;
    for &w in widths {
        let mut segments = Vec::new();
        let mut done = 0;
        while done < w {
            let (slot, offset) = ((at / 4) as usize, at % 4);
            let len = (4 - offset).min(w - done);
            segments.push((slot, offset, len, done));
            done += len;
            at += len;
        }
        places.push(segments);
    }
    (need, places)
}

/// The components of vector `name` the fragment stage reads (`"xz"`), when
/// every use of it in `body` is a swizzle; `None` when any is not.
fn read_components(body: &str, name: &str) -> Option<String> {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut seen = [false; 4];
    for (i, _) in body.match_indices(name) {
        if body[..i].ends_with(ident) || body[i + name.len()..].starts_with(ident) {
            continue;
        }
        // A member of something else (`s.name`).
        if body[..i].trim_end().ends_with('.') {
            continue;
        }
        let rest = body[i + name.len()..].trim_start().strip_prefix('.')?;
        let swizzle: String = rest
            .trim_start()
            .chars()
            .take_while(|&c| ident(c))
            .collect();
        let set = ["xyzw", "rgba", "stpq"]
            .into_iter()
            .find(|set| !swizzle.is_empty() && swizzle.chars().all(|c| set.contains(c)))?;
        for c in swizzle.chars() {
            seen[set.find(c)?] = true;
        }
    }
    Some(
        "xyzw"
            .chars()
            .zip(seen)
            .filter_map(|(c, s)| s.then_some(c))
            .collect(),
    )
}

/// `vars` with each struct-typed varying (`S s;`, `S s[2];`) replaced by
/// one varying per member (`s.a`, `s[1].b`), nested structs included; the
/// struct definitions come from `bodies`.
fn expand_structs(vars: &[Var], bodies: &[&str]) -> Vec<Var> {
    let mut out = Vec::new();
    for var in vars {
        let Some(members) = bodies.iter().find_map(|b| struct_members(b, &var.ty)) else {
            out.push(var.clone());
            continue;
        };
        let elements: Vec<String> = match var.array_len {
            None => vec![String::new()],
            Some(n) => (0..n).map(|k| format!("[{k}]")).collect(),
        };
        let mut expanded = Vec::new();
        for index in &elements {
            for (ty, name, len) in &members {
                expanded.push(Var {
                    name: format!("{}{index}.{name}", var.name),
                    ty: ty.clone(),
                    flat: var.flat,
                    array_len: *len,
                });
            }
        }
        out.extend(expand_structs(&expanded, bodies));
    }
    out
}

/// The members `(type, name, array length)` of `struct ty { … }` in `body`.
fn struct_members(body: &str, ty: &str) -> Option<Vec<(String, String, Option<u32>)>> {
    let words: Vec<&str> = body
        .split(|c: char| c.is_whitespace())
        .flat_map(split_punct)
        .filter(|w| !w.is_empty())
        .collect();
    let at = words
        .windows(3)
        .position(|w| w[0] == "struct" && w[1] == ty && w[2] == "{")?;
    let end = at + 3 + words[at + 3..].iter().position(|w| *w == "}")?;
    let mut members = Vec::new();
    for decl in words[at + 3..end].split(|w| *w == ";") {
        // Qualifiers (precision) before the type are skipped.
        let decl: Vec<&str> = decl
            .iter()
            .copied()
            .skip_while(|w| matches!(*w, "highp" | "mediump" | "lowp" | "flat"))
            .collect();
        let Some((&mty, rest)) = decl.split_first() else {
            continue;
        };
        for d in rest.split(|w| *w == ",") {
            match d {
                [name] => members.push((mty.to_string(), name.to_string(), None)),
                [name, "[", n, "]"] => {
                    members.push((mty.to_string(), name.to_string(), n.parse().ok()))
                }
                _ => return None,
            }
        }
    }
    Some(members)
}

/// `word` split around the punctuation struct definitions use.
fn split_punct(word: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, c) in word.char_indices() {
        if matches!(c, '{' | '}' | ';' | ',' | '[' | ']') {
            out.push(&word[start..i]);
            out.push(&word[i..i + 1]);
            start = i + 1;
        }
    }
    out.push(&word[start..]);
    out
}

/// What Iris does with a fragment `in` the vertex stage has no `out` for
/// (its compatibility patch): the vertex stage gets the declaration, zeroed
/// at the start (WGSL's private globals start zeroed), so a vertex stage that
/// writes the variable without declaring it (bloop-shaders' `shadow.vsh`)
/// compiles and hands the value on. Only for variables the vertex stage
/// mentions and does not declare at file scope itself; the others read zero
/// in the fragment stage and cost no location. Returns a note per variable.
pub(super) fn adopt_undeclared(vertex: &mut Rewritten, fragment: &Rewritten) -> Vec<String> {
    let mut notes = Vec::new();
    for f in &fragment.varyings {
        let plain = integer_type(&f.ty).is_some() || matrix(&f.ty).is_some();
        if !plain
            || vertex.varyings.iter().any(|v| v.name == f.name)
            || !uses(&vertex.body, &f.name)
            || declared_at_file_scope(&vertex.body, &f.name)
        {
            continue;
        }
        notes.push(format!(
            "varying {}: the vertex stage writes it without declaring it; declared there, as Iris does",
            f.name
        ));
        vertex.varyings.push(f.clone());
    }
    notes
}

/// Whether `body` declares `name` outside any function (`vec2 name;`,
/// `uniform float a, name;`): a type or a comma before it outside braces
/// and parentheses.
fn declared_at_file_scope(body: &str, name: &str) -> bool {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    body.match_indices(name).any(|(i, _)| {
        if body[..i].ends_with(ident) || body[i + name.len()..].starts_with(ident) {
            return false;
        }
        let depth = |open: char, close: char| {
            body[..i].matches(open).count() as i64 - body[..i].matches(close).count() as i64
        };
        // Inside a function, or a parameter list.
        if depth('{', '}') != 0 || depth('(', ')') != 0 {
            return false;
        }
        let before = body[..i].trim_end();
        before.ends_with(',') || before.ends_with(ident)
    })
}

/// Whether `body` mentions the identifier `name`.
fn uses(body: &str, name: &str) -> bool {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    body.match_indices(name)
        .any(|(i, _)| !body[..i].ends_with(ident) && !body[i + name.len()..].starts_with(ident))
}

/// Declares a varying's global; struct-typed ones are declared by the
/// rewritten body, after their struct.
pub(super) fn declare(out: &mut String, var: &Var) {
    if integer_type(&var.ty).is_none() && matrix(&var.ty).is_none() && var.ty != "bool" {
        return;
    }
    match var.array_len {
        Some(n) => writeln!(out, "{} {}[{n}];", var.ty, var.name),
        None => writeln!(out, "{} {};", var.ty, var.name),
    }
    .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naga_out::glsl_to_wgsl;

    fn var(name: &str, ty: &str, flat: bool, array_len: Option<u32>) -> Var {
        Var {
            name: name.into(),
            ty: ty.into(),
            flat,
            array_len,
        }
    }

    /// A full-screen pass passing `vertex` (all written from the texcoord) to
    /// `fragment` (all summed into the output).
    fn program(vertex: Vec<Var>, fragment: Vec<Var>) -> (Rewritten, Rewritten) {
        let assign = |vars: &[Var], each: &dyn Fn(&str, &str) -> String| -> String {
            let mut body = String::new();
            for v in vars {
                match v.array_len {
                    Some(n) => {
                        (0..n).for_each(|k| body += &each(&format!("{}[{k}]", v.name), &v.ty))
                    }
                    None => body += &each(&v.name, &v.ty),
                }
            }
            body
        };
        let write = assign(&vertex, &|g, ty| {
            format!("    {g} = {ty}(hz_gl_MultiTexCoord0.x);\n")
        });
        let read = assign(&fragment, &|g, ty| {
            let value = if matrix(ty).is_some() {
                format!("{g}[0][0]")
            } else if width(ty) == 1 {
                format!("float({g})")
            } else {
                // An index, not a swizzle: every component counts as read.
                format!("float({g}[0])")
            };
            format!("    s += {value};\n")
        });
        let vs = Rewritten {
            body: format!(
                "void hz_pack_main() {{\n    gl_Position = hz_ftransform();\n{write}}}\n"
            ),
            varyings: vertex,
            builtins: ["hz_ftransform", "hz_gl_MultiTexCoord0"]
                .map(String::from)
                .into(),
            ..Default::default()
        };
        let fs = Rewritten {
            body: format!(
                "void hz_pack_main() {{\n    float s = 0.0;\n{read}    hz_gl_FragData[0] = vec4(s);\n}}\n"
            ),
            varyings: fragment,
            builtins: ["hz_gl_FragData"].map(String::from).into(),
            ..Default::default()
        };
        (vs, fs)
    }

    /// Both stages' WGSL, validated by naga.
    fn translate(
        program: &(Rewritten, Rewritten),
        geometry: Geometry,
        max: u32,
    ) -> Result<(String, String), String> {
        let host = HostParams {
            max_varyings: max,
            ..Default::default()
        };
        let blocks = BlockIds::default();
        let spec = ProgramSpec {
            kind: if geometry == Geometry::Terrain {
                ProgramKind::Water
            } else {
                ProgramKind::Final
            },
            geometry,
            draw_buffers: &[0],
            host: &host,
            blocks: &blocks,
            plan: &Default::default(),
        };
        let (vs, fs) = sources(&program.0, &program.1, &spec)?;
        let vs = glsl_to_wgsl(&vs, Stage::Vertex).map_err(|e| format!("{e}\n{vs}"))?;
        let fs = glsl_to_wgsl(&fs, Stage::Fragment).map_err(|e| format!("{e}\n{fs}"))?;
        Ok((vs, fs))
    }

    #[test]
    fn matrices_split_into_columns() {
        let vars = vec![
            var("tbn", "mat3", false, None),
            var("pair", "mat2", true, Some(2)),
            var("m", "mat4x2", false, None),
        ];
        let p = program(vars.clone(), vars);
        let i = Interface::new(&p.0, &p.1, Geometry::Fullscreen, 16).unwrap();
        let names: Vec<_> = i
            .slots
            .iter()
            .map(|s| (s.name.as_str(), s.ty.as_str(), s.flat))
            .collect();
        assert_eq!(
            names,
            [
                ("hz_v_m_c0", "vec2", false),
                ("hz_v_m_c1", "vec2", false),
                ("hz_v_m_c2", "vec2", false),
                ("hz_v_m_c3", "vec2", false),
                ("hz_v_pair_0_c0", "vec2", true),
                ("hz_v_pair_0_c1", "vec2", true),
                ("hz_v_pair_1_c0", "vec2", true),
                ("hz_v_pair_1_c1", "vec2", true),
                ("hz_v_tbn_c0", "vec3", false),
                ("hz_v_tbn_c1", "vec3", false),
                ("hz_v_tbn_c2", "vec3", false),
            ]
        );
        let mut read = String::new();
        i.read(&mut read);
        assert!(
            read.contains("    pair[1][0] = hz_v_pair_1_c0;\n"),
            "{read}"
        );
        translate(&p, Geometry::Fullscreen, 16).unwrap();
    }

    #[test]
    fn too_many_varyings_pack_into_vec4s() {
        // 17 locations unpacked (SEUS-sized), on a 16-location device.
        let mut vars: Vec<Var> = (0..10)
            .map(|i| var(&format!("f{i}"), "float", false, None))
            .collect();
        vars.push(var("n", "vec3", false, None));
        vars.push(var("uv", "vec2", false, Some(2)));
        vars.push(var("id", "float", true, None));
        vars.push(var("level", "int", false, None));
        vars.push(var("mask", "uvec2", true, None));
        vars.push(var("c", "vec4", false, None));
        let p = program(vars.clone(), vars);
        let i = Interface::new(&p.0, &p.1, Geometry::Fullscreen, 16).unwrap();
        // c | n f0 | uv0 uv1 | f1..f4 | f5..f8 | f9, then mask id level (id as its bits).
        let slots: Vec<_> = i.slots.iter().map(|s| (s.ty.as_str(), s.flat)).collect();
        let mut want = vec![("vec4", false); 6];
        want.push(("ivec4", true));
        assert_eq!(slots, want);
        let mut write = String::new();
        i.write(&mut write);
        for line in [
            "hz_vp0 = vec4(0);",
            "hz_vp0.xyzw = c;",
            "hz_vp1.xyz = n;",
            "hz_vp1.w = f0;",
            "hz_vp2.zw = uv[1];",
            "hz_vp5.x = f9;",
            "hz_vp6.xy = ivec2(mask);",
            "hz_vp6.z = floatBitsToInt(id);",
            "hz_vp6.w = level;",
        ] {
            assert!(write.contains(line), "{line}\n{write}");
        }
        let mut read = String::new();
        i.read(&mut read);
        assert!(read.contains("mask = uvec2(hz_vp6.xy);"), "{read}");
        assert!(read.contains("id = intBitsToFloat(hz_vp6.z);"), "{read}");
        let (_, fs) = translate(&p, Geometry::Fullscreen, 16).unwrap();
        assert!(
            fs.contains("@location(6)") && !fs.contains("@location(7)"),
            "{fs}"
        );
        // Terrain carries hz_sprite too: still fits.
        translate(&p, Geometry::Terrain, 16).unwrap();
        // Unpacked when the device allows them all.
        let (vs, _) = translate(&p, Geometry::Fullscreen, 28).unwrap();
        assert!(
            vs.contains("@location(16)") && !vs.contains("@location(17)"),
            "{vs}"
        );
        // Terrain fits in 7: hz_sprite takes the free component of the sixth slot.
        translate(&p, Geometry::Terrain, 7).unwrap();
        // Too many even packed.
        let err = translate(&p, Geometry::Terrain, 6).unwrap_err();
        assert!(err.contains("7 varying locations"), "{err}");
    }

    /// `program` with the fragment stage reading `reads` (GLSL expressions) instead.
    fn program_reading(vars: Vec<Var>, reads: &str) -> (Rewritten, Rewritten) {
        let (vs, mut fs) = program(vars.clone(), vars);
        fs.body = format!(
            "void hz_pack_main() {{\n    float s = 0.0;\n    s += {reads};\n    hz_gl_FragData[0] = vec4(s);\n}}\n"
        );
        (vs, fs)
    }

    #[test]
    fn unread_components_are_not_carried() {
        // 15 vec4s (60 components) + 2 more vec4s read as `.y` and `.xz`: 63 → 16 slots.
        let mut vars: Vec<Var> = (0..15)
            .map(|i| var(&format!("c{i}"), "vec4", false, None))
            .collect();
        vars.push(var("lm", "vec4", false, None));
        vars.push(var("wp", "vec4", false, None));
        let reads = (0..15)
            .map(|i| format!("c{i}[0]"))
            .chain(["lm.y".into(), "wp . x".into(), "wp.zx.y".into()])
            .collect::<Vec<_>>()
            .join(" + ");
        let p = program_reading(vars, &reads);
        let i = Interface::new(&p.0, &p.1, Geometry::Fullscreen, 16).unwrap();
        assert_eq!(i.slots.len(), 16);
        let mut write = String::new();
        i.write(&mut write);
        assert!(
            write.contains("= wp.xz;") && write.contains("= lm.y;"),
            "{write}"
        );
        let mut read = String::new();
        i.read(&mut read);
        assert!(
            read.contains("    wp = vec4(0);\n") && read.contains("    wp.xz = "),
            "{read}"
        );
        translate(&p, Geometry::Fullscreen, 16).unwrap();
        // A whole read carries everything: 68 components do not fit.
        let whole = (0..15)
            .map(|i| format!("c{i}[0]"))
            .chain(["lm[1]".into(), "wp[2]".into()]);
        let mut vars: Vec<Var> = (0..15)
            .map(|i| var(&format!("c{i}"), "vec4", false, None))
            .collect();
        vars.push(var("lm", "vec4", false, None));
        vars.push(var("wp", "vec4", false, None));
        let p = program_reading(vars, &whole.collect::<Vec<_>>().join(" + "));
        assert!(Interface::new(&p.0, &p.1, Geometry::Fullscreen, 16).is_err());
        assert_eq!(
            read_components("a.x + a . zw + b.a", "a").as_deref(),
            Some("xzw")
        );
        assert_eq!(read_components("a.x + f(a)", "a"), None);
        assert_eq!(read_components("s.a + a.y", "a").as_deref(), Some("y"));
    }

    #[test]
    fn pieces_straddle_slots_when_that_saves_one() {
        assert_eq!(layout(&[3, 3, 1]).0, 2);
        // Four vec3s and two vec2s: first fit takes 5 slots, 16 components need 4.
        let (count, places) = layout(&[3, 3, 3, 3, 2, 2]);
        assert_eq!(count, 4);
        assert_eq!(places[1], vec![(0, 3, 1, 0), (1, 0, 2, 1)]);
        let mut vars: Vec<Var> = (0..4)
            .map(|i| var(&format!("n{i}"), "vec3", false, None))
            .collect();
        vars.push(var("a", "vec2", false, None));
        vars.push(var("b", "vec2", false, None));
        let p = program(vars.clone(), vars);
        let i = Interface::new(&p.0, &p.1, Geometry::Fullscreen, 4).unwrap();
        assert_eq!(i.slots.len(), 4);
        let mut write = String::new();
        i.write(&mut write);
        assert!(
            write.contains("hz_vp0.w = n1.x;") && write.contains("hz_vp1.xy = n1.yz;"),
            "{write}"
        );
        let mut read = String::new();
        i.read(&mut read);
        assert!(read.contains("n1 = vec3(hz_vp0.w, hz_vp1.xy);"), "{read}");
        translate(&p, Geometry::Fullscreen, 4).unwrap();
        // Matrix columns straddle too (naga cannot store into `tbn[1].x`).
        let vars = vec![
            var("tbn", "mat3", false, None),
            var("n", "vec3", false, None),
            var("a", "vec2", false, None),
            var("b", "vec2", false, None),
        ];
        let p = program(vars.clone(), vars);
        let i = Interface::new(&p.0, &p.1, Geometry::Fullscreen, 4).unwrap();
        let mut read = String::new();
        i.read(&mut read);
        assert!(
            read.contains("tbn[0] = vec3(hz_vp0.w, hz_vp1.xy);"),
            "{read}"
        );
        translate(&p, Geometry::Fullscreen, 4).unwrap();
    }

    #[test]
    fn terrain_sprite_shares_the_slots() {
        // 15 floats + a vec4 = 19 components: 5 slots, 1 free; hz_sprite takes it.
        let mut vars: Vec<Var> = (0..15)
            .map(|i| var(&format!("f{i}"), "float", false, None))
            .collect();
        vars.push(var("c", "vec4", false, None));
        let p = program(vars.clone(), vars);
        let i = Interface::new(&p.0, &p.1, Geometry::Terrain, 5).unwrap();
        assert_eq!(i.slots.len(), 5);
        assert!(
            matches!(i.sprite, SpriteCarrier::Float(4, 3)),
            "{:?}",
            i.sprite
        );
        let (vs, fs) = translate(&p, Geometry::Terrain, 5).unwrap();
        assert!(
            !fs.contains("@location(5)") && fs.contains("round("),
            "{fs}"
        );
        assert!(vs.contains("& 65535u"), "{vs}");
        // With a flat piece, it goes as bits in the flat slot.
        let mut vars: Vec<Var> = (0..16)
            .map(|i| var(&format!("f{i}"), "float", false, None))
            .collect();
        vars.push(var("id", "int", true, None));
        let p = program(vars.clone(), vars);
        let i = Interface::new(&p.0, &p.1, Geometry::Terrain, 5).unwrap();
        assert!(
            matches!(i.sprite, SpriteCarrier::Bits(4, 1)),
            "{:?}",
            i.sprite
        );
        let mut write = String::new();
        i.write_sprite(&mut write);
        assert_eq!(write, "    hz_vp4.y = int(hz_sprite.x);\n");
        let mut read = String::new();
        i.read_sprite(&mut read, 4);
        assert!(
            read.contains("uint hz_l = uint(hz_vp4.y);") && read.contains("hz_n % 16u % 4u"),
            "{read}"
        );
        translate(&p, Geometry::Terrain, 5).unwrap();
        // Unpacked, hz_sprite keeps its own location.
        let i = Interface::new(&p.0, &p.1, Geometry::Terrain, 28).unwrap();
        assert_eq!(i.sprite, SpriteCarrier::Own);
    }

    #[test]
    fn one_sided_and_mismatched_varyings() {
        let vertex = vec![
            var("only_vertex", "vec3", false, None),
            var("count", "float", false, None),
            var("dir", "ivec3", true, None),
        ];
        let fragment = vec![
            var("only_fragment", "vec4", false, None),
            var("arr", "float", false, Some(3)),
            var("count", "int", false, None),
            var("dir", "vec3", false, None),
        ];
        let p = program(vertex, fragment);
        let i = Interface::new(&p.0, &p.1, Geometry::Fullscreen, 16).unwrap();
        let slots: Vec<_> = i
            .slots
            .iter()
            .map(|s| (s.name.as_str(), s.ty.as_str(), s.flat))
            .collect();
        // Mismatches travel as the vertex type, flat when either end is an integer.
        assert_eq!(
            slots,
            [("hz_v_count", "float", true), ("hz_v_dir", "ivec3", true)]
        );
        let mut read = String::new();
        i.read(&mut read);
        for line in [
            "count = int(hz_v_count);",
            "dir = vec3(hz_v_dir);",
            "only_fragment = vec4(0);",
            "for (int i = 0; i < 3; i++) { arr[i] = float(0); }",
        ] {
            assert!(read.contains(line), "{line}\n{read}");
        }
        translate(&p, Geometry::Fullscreen, 16).unwrap();
        // Different sizes stay an error.
        let p = program(
            vec![var("x", "vec3", false, None)],
            vec![var("x", "vec2", false, None)],
        );
        let err = translate(&p, Geometry::Fullscreen, 16).unwrap_err();
        assert!(err.contains("varying x is vec3"), "{err}");
    }

    #[test]
    fn undeclared_vertex_outputs_are_declared_as_iris_does() {
        // The vertex stage writes `lm` without declaring it; the fragment stage reads it.
        let mut p = program(vec![], vec![var("lm", "vec2", false, None)]);
        let write = "    lm.xy = hz_gl_MultiTexCoord0.xy;\n";
        p.0.body = p.0.body.replace("}\n", &format!("{write}}}\n"));
        let (vs, fs) = translate(&p, Geometry::Fullscreen, 16).unwrap();
        assert!(
            vs.contains("@location(0)") && fs.contains("@location(0)"),
            "{vs}\n{fs}"
        );
        let mut notes = p.0.clone();
        let adopted = adopt_undeclared(&mut notes, &p.1);
        assert_eq!(adopted.len(), 1);
        assert!(adopted[0].starts_with("varying lm:"), "{adopted:?}");
        // Not mentioned by the vertex stage: zero, no location.
        let p = program(vec![], vec![var("lm", "vec2", false, None)]);
        let (vs, _) = translate(&p, Geometry::Fullscreen, 16).unwrap();
        assert!(!vs.contains("@location(0)"), "{vs}");
        // Declared at file scope as a plain global: left alone (not a varying).
        let mut p = program(vec![], vec![var("lm", "vec2", false, None)]);
        p.0.body = format!(
            "vec2 lm;\n{}",
            p.0.body.replace("}\n", &format!("{write}}}\n"))
        );
        let mut v = p.0.clone();
        assert!(adopt_undeclared(&mut v, &p.1).is_empty());
        let (vs, _) = translate(&p, Geometry::Fullscreen, 16).unwrap();
        assert!(!vs.contains("@location(0)"), "{vs}");
        // A parameter of the same name inside a function does not count as declared.
        assert!(!declared_at_file_scope(
            "void f(vec2 lm) { lm.x; }\nvoid main() { lm = vec2(1); }",
            "lm"
        ));
        assert!(declared_at_file_scope("uniform float a, lm;", "lm"));
    }

    #[test]
    fn mc_entity_reads_the_pack_stream() {
        for (geometry, ty, want) in [
            (
                Geometry::Terrain,
                "vec4",
                "mc_Entity = vec4(vec4(hz_in_block.x == 65535u ? -1.0 : float(hz_in_block.x), (hz_in_block.y & 4096u) != 0u ? 1.0 : 0.0, 0.0, 1.0));",
            ),
            (
                Geometry::Terrain,
                "float",
                "mc_Entity = float(vec4(hz_in_block.x == 65535u ? -1.0 : float(hz_in_block.x), (hz_in_block.y & 4096u) != 0u ? 1.0 : 0.0, 0.0, 1.0).x);",
            ),
            (
                Geometry::Terrain,
                "ivec3",
                "mc_Entity = ivec3(vec4(hz_in_block.x == 65535u ? -1.0 : float(hz_in_block.x), (hz_in_block.y & 4096u) != 0u ? 1.0 : 0.0, 0.0, 1.0).xyz);",
            ),
            (
                Geometry::Fullscreen,
                "vec4",
                "mc_Entity = vec4(vec4(0.0, 0.0, 0.0, 1.0));",
            ),
        ] {
            let mut p = program(
                vec![var("x", "vec2", false, None)],
                vec![var("x", "vec2", false, None)],
            );
            p.0.attributes.push(var("mc_Entity", ty, false, None));
            p.0.body = p.0.body.replace(
                "gl_Position = hz_ftransform();",
                &format!(
                    "gl_Position = hz_ftransform() + float(mc_Entity{});",
                    if ty == "float" { "" } else { ".x" }
                ),
            );
            let host = HostParams::default();
            let blocks = BlockIds::legacy();
            let spec = ProgramSpec {
                kind: ProgramKind::Final,
                geometry,
                draw_buffers: &[0],
                host: &host,
                blocks: &blocks,
                plan: &Default::default(),
            };
            let (vs, _) = sources(&p.0, &p.1, &spec).unwrap();
            assert!(vs.contains(want), "{want}\n{vs}");
            glsl_to_wgsl(&vs, Stage::Vertex).unwrap_or_else(|e| panic!("{e}\n{vs}"));
        }
    }
}

#[cfg(test)]
mod unread_tests {
    use super::*;

    #[test]
    fn struct_varyings_travel_by_member() {
        let defs = "struct Inner { vec2 uv; float w[2]; };\nstruct Fog { vec3 color; float density, start; Inner inner; };\n";
        let vertex = Rewritten {
            body: format!("{defs}void hz_pack_main() {{ fog.color = vec3(1.0); }}"),
            varyings: vec![Var {
                name: "fog".into(),
                ty: "Fog".into(),
                flat: true,
                array_len: None,
            }],
            ..Default::default()
        };
        let fragment = Rewritten {
            body: format!(
                "{defs}void hz_pack_main() {{ hz_gl_FragData[0] = vec4(fog.color, fog.inner.w[1]); }}"
            ),
            ..vertex.clone()
        };
        let i = Interface::new(&vertex, &fragment, Geometry::Fullscreen, 16).unwrap();
        let names: Vec<&str> = i.slots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "hz_v_fog_color",
                "hz_v_fog_density",
                "hz_v_fog_inner_uv",
                "hz_v_fog_inner_w_0",
                "hz_v_fog_inner_w_1",
                "hz_v_fog_start"
            ]
        );
        let mut read = String::new();
        i.read(&mut read);
        assert!(
            read.contains("fog.inner.w[1] = hz_v_fog_inner_w_1;"),
            "{read}"
        );
    }

    #[test]
    fn unread_varyings_get_no_location() {
        let v = |name: &str| Var {
            name: name.into(),
            ty: "vec3".into(),
            flat: false,
            array_len: None,
        };
        let vertex = Rewritten {
            varyings: vec![v("used"), v("unused"), v("used_2")],
            ..Default::default()
        };
        let fragment = Rewritten {
            body: "void hz_pack_main() { hz_gl_FragData[0] = vec4(used + used_2, 1.0); }".into(),
            varyings: vec![v("used"), v("unused"), v("used_2")],
            ..Default::default()
        };
        let i = Interface::new(&vertex, &fragment, Geometry::Fullscreen, 16).unwrap();
        let names: Vec<&str> = i.slots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["hz_v_used", "hz_v_used_2"]);
    }
}

/// Runs the vertex stage's hand-off statements and the fragment stage's
/// rebuilding ones on concrete values, as GLSL would, and checks every
/// carried component arrives with the same bits.
#[cfg(test)]
mod round_trip_tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Clone, Copy, Debug)]
    enum Val {
        F(f32),
        I(i32),
        U(u32),
    }

    impl Val {
        fn bits(self) -> (u8, u32) {
            match self {
                Val::F(f) => (b'f', f.to_bits()),
                Val::I(i) => (b'i', i as u32),
                Val::U(u) => (b'u', u),
            }
        }
        /// GLSL's constructor conversion to the base type `to` (`f`, `i`, `u`).
        fn to(self, to: u8) -> Val {
            match (to, self) {
                (b'f', Val::F(f)) => Val::F(f),
                (b'f', Val::I(i)) => Val::F(i as f32),
                (b'f', Val::U(u)) => Val::F(u as f32),
                (b'i', Val::F(f)) => Val::I(f as i32),
                (b'i', Val::I(i)) => Val::I(i),
                (b'i', Val::U(u)) => Val::I(u as i32),
                (b'u', Val::F(f)) => Val::U(f as u32),
                (b'u', Val::I(i)) => Val::U(i as u32),
                (b'u', Val::U(u)) => Val::U(u),
                _ => unreachable!(),
            }
        }
    }

    fn base(ty: &str) -> u8 {
        match ty.as_bytes()[0] {
            b'i' => b'i',
            b'u' => b'u',
            _ => b'f',
        }
    }

    /// `name.xz` → (`name`, lanes), when the part after the last dot is a swizzle.
    fn split_swizzle(s: &str) -> (&str, Option<Vec<usize>>) {
        if let Some((name, sw)) = s.rsplit_once('.')
            && !sw.is_empty()
            && sw.bytes().all(|b| b"xyzw".contains(&b))
        {
            let lanes = sw
                .bytes()
                .map(|b| (b"xyzw".iter().position(|&c| c == b)).unwrap());
            return (name, Some(lanes.collect()));
        }
        (s, None)
    }

    struct Machine {
        vars: HashMap<String, Vec<Val>>,
    }

    impl Machine {
        fn eval(&self, e: &str) -> Vec<Val> {
            let e = e.trim();
            if e == "0" {
                return vec![Val::I(0)];
            }
            if let Some(open) = e.find('(')
                && e.ends_with(')')
                && e[..open].bytes().all(|b| b.is_ascii_alphanumeric())
            {
                let func = &e[..open];
                let inner = &e[open + 1..e.len() - 1];
                let mut args = Vec::new();
                let (mut depth, mut start) = (0, 0);
                for (i, c) in inner.char_indices() {
                    match c {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        ',' if depth == 0 => {
                            args.push(&inner[start..i]);
                            start = i + 1;
                        }
                        _ => {}
                    }
                }
                args.push(&inner[start..]);
                let vals: Vec<Val> = args.iter().flat_map(|a| self.eval(a)).collect();
                return match func {
                    "floatBitsToInt" => vals
                        .iter()
                        .map(|v| match v {
                            Val::F(f) => Val::I(f.to_bits() as i32),
                            _ => panic!("floatBitsToInt of {v:?}"),
                        })
                        .collect(),
                    "intBitsToFloat" => vals
                        .iter()
                        .map(|v| match v {
                            Val::I(i) => Val::F(f32::from_bits(*i as u32)),
                            _ => panic!("intBitsToFloat of {v:?}"),
                        })
                        .collect(),
                    ty => {
                        let n = width(ty) as usize;
                        let vals = if vals.len() == 1 {
                            vec![vals[0]; n]
                        } else {
                            vals
                        };
                        assert_eq!(vals.len(), n, "{e}");
                        vals.into_iter().map(|v| v.to(base(ty))).collect()
                    }
                };
            }
            let (name, lanes) = split_swizzle(e);
            let v = self
                .vars
                .get(name)
                .unwrap_or_else(|| panic!("unset {name} in {e}"));
            match lanes {
                Some(l) => l.iter().map(|&k| v[k]).collect(),
                None => v.clone(),
            }
        }

        /// Runs `target = value;` lines; returns the (variable, lane)s written.
        fn run(&mut self, code: &str, written: &mut Vec<(String, usize)>) {
            for line in code.lines() {
                let line = line.trim().strip_suffix(';').unwrap();
                let (target, value) = line.split_once(" = ").unwrap();
                let value = self.eval(value);
                let (name, lanes) = split_swizzle(target);
                let var = self
                    .vars
                    .get_mut(name)
                    .unwrap_or_else(|| panic!("no {name}"));
                let lanes = lanes.unwrap_or_else(|| (0..var.len()).collect());
                assert_eq!(lanes.len(), value.len(), "{line}");
                for (&k, v) in lanes.iter().zip(value) {
                    // Stores keep the variable's base type (as GLSL's types would).
                    let ty = var[k].bits().0;
                    assert_eq!(v.bits().0, ty, "{line}: stores a different type");
                    var[k] = v;
                    written.push((name.to_string(), k));
                }
            }
        }
    }

    fn zero(ty: &str) -> Vec<Val> {
        vec![Val::I(0).to(base(ty)); width(ty) as usize]
    }

    /// Distinct values that survive only an exact trip: negative zero,
    /// subnormals, NaN-free extremes and large integers.
    fn value(ty: &str, seed: u32, k: u32) -> Val {
        let n = seed * 4 + k;
        match base(ty) {
            b'f' => Val::F(match n % 5 {
                0 => -0.0,
                1 => f32::from_bits(n + 1), // subnormal
                2 => f32::MAX / (n as f32 + 1.0),
                3 => -(n as f32) * 0.1 - 0.0625,
                _ => 1.0e-30 * n as f32,
            }),
            b'i' => Val::I(if n % 2 == 0 {
                i32::MIN + n as i32
            } else {
                -(n as i32) - 1
            }),
            _ => Val::U(u32::MAX - n),
        }
    }

    /// Packs `vars` (vertex and fragment alike, the fragment reading `reads`
    /// of each: `None` = every component) at `max` locations, runs the
    /// statements and checks each component: read ones exact, unread ones 0.
    fn check(
        vars: &[(&str, &str, bool, Option<u32>, Option<&str>)],
        geometry: Geometry,
        max: u32,
    ) -> Interface {
        let decl: Vec<Var> = vars
            .iter()
            .map(|&(n, t, f, a, _)| Var {
                name: n.into(),
                ty: t.into(),
                flat: f,
                array_len: a,
            })
            .collect();
        let reads: Vec<String> = vars
            .iter()
            .map(|&(n, t, _, a, r)| {
                let g = if a.is_some() {
                    format!("{n}[0]")
                } else {
                    n.to_string()
                };
                match r {
                    Some(sw) => format!("float({g}.{sw})"),
                    None if matrix(t).is_some() => format!("{g}[0][0]"),
                    None if width(t) == 1 => format!("float({g})"),
                    None => format!("float({g}[0])"),
                }
            })
            .collect();
        let vs = Rewritten {
            varyings: decl.clone(),
            ..Default::default()
        };
        let fs = Rewritten {
            body: format!("void hz_pack_main() {{ float s = {}; }}", reads.join(" + ")),
            varyings: decl,
            ..Default::default()
        };
        let i = Interface::new(&vs, &fs, geometry, max).unwrap_or_else(|e| panic!("{e}"));
        let mut vertex = Machine {
            vars: HashMap::new(),
        };
        let mut fragment = Machine {
            vars: HashMap::new(),
        };
        for s in &i.slots {
            vertex.vars.insert(s.name.clone(), zero(&s.ty));
        }
        for (seed, p) in i.pieces.iter().enumerate() {
            let w = width(&p.vertex_ty);
            let vals = (0..w)
                .map(|k| value(&p.vertex_ty, seed as u32, k))
                .collect();
            vertex.vars.insert(p.global.clone(), vals);
            fragment.vars.insert(p.global.clone(), zero(&p.fragment_ty));
        }
        let mut code = String::new();
        i.write(&mut code);
        let mut written = Vec::new();
        vertex.run(&code, &mut written);
        // Every lane at most once, besides the zeroing of whole slots.
        // (`write` zeroes whole packed slots first.)
        let zeroed: usize = i
            .slots
            .iter()
            .filter(|s| code.contains(&format!("    {} = {}(0);", s.name, s.ty)))
            .map(|s| width(&s.ty) as usize)
            .sum();
        let mut lanes = written.split_off(zeroed);
        let before = lanes.len();
        lanes.sort();
        lanes.dedup();
        assert_eq!(before, lanes.len(), "a lane written twice:\n{code}");
        let mut sprite = String::new();
        i.write_sprite(&mut sprite);
        if let Some((target, _)) = sprite.trim().split_once(" = ") {
            let (name, l) = split_swizzle(target);
            let lane = (name.to_string(), l.unwrap()[0]);
            assert!(!lanes.contains(&lane), "hz_sprite shares {lane:?}:\n{code}");
        }
        for s in &i.slots {
            fragment
                .vars
                .insert(s.name.clone(), vertex.vars[&s.name].clone());
        }
        let mut code = String::new();
        i.read(&mut code);
        fragment.run(&code, &mut Vec::new());
        for p in &i.pieces {
            let base_name = p.global.split('[').next().unwrap();
            let read = vars.iter().find(|v| v.0 == base_name).and_then(|v| v.4);
            let sent = &vertex.vars[&p.global];
            let got = &fragment.vars[&p.global];
            for (k, (s, g)) in sent.iter().zip(got).enumerate() {
                let lane = b"xyzw"[k];
                // Unpacked pieces travel whole; packed ones only what is read.
                let want = match read {
                    Some(sw) if p.partial && !sw.as_bytes().contains(&lane) => {
                        Val::I(0).to(base(&p.fragment_ty))
                    }
                    _ => *s,
                };
                assert_eq!(g.bits(), want.bits(), "{}[{k}]:\n{code}", p.global);
            }
        }
        i
    }

    #[test]
    fn values_round_trip_exactly() {
        let vars = [
            ("a", "float", false, None, None),
            ("b", "vec2", false, None, None),
            ("c", "vec3", false, None, None),
            ("d", "vec4", false, None, None),
            ("e", "vec3", false, Some(2), None),
            ("tbn", "mat3", false, None, None),
            ("id", "int", true, None, None),
            ("mat", "float", true, None, None),
            ("dir", "ivec3", true, None, None),
            ("mask", "uvec2", true, None, None),
            ("normal", "vec3", true, None, None),
            ("level", "int", false, None, None),
        ];
        // From every piece its own location down to the tightest packing.
        for max in (10..=24).rev() {
            for geometry in [Geometry::Fullscreen, Geometry::Terrain] {
                let i = check(&vars, geometry, max);
                assert!(i.slots.len() as u32 <= max);
            }
        }
        // 13 + 8 + 4 = 25 smooth components (7 slots) and 1 + 1 + 3 + 2 + 3 + 1 = 11 flat (3).
        let i = check(&vars, Geometry::Fullscreen, 10);
        let kinds: Vec<_> = i.slots.iter().map(|s| (s.ty.as_str(), s.flat)).collect();
        let mut want = vec![("vec4", false); 7];
        want.extend([("ivec4", true); 3]);
        assert_eq!(kinds, want);
        // Straddling happened (first fit would take 11).
        assert!(i.pieces.iter().any(|p| p.segments.len() > 1));
        assert!(
            Interface::new(
                &Rewritten::default(),
                &Rewritten::default(),
                Geometry::Fullscreen,
                9
            )
            .is_ok()
        );
    }

    #[test]
    fn partly_read_vectors_round_trip() {
        let vars = [
            ("lm", "vec4", false, None, Some("y")),
            ("wp", "vec4", false, None, Some("zx")),
            ("n", "vec3", false, None, Some("xz")),
            ("uv", "vec2", false, None, Some("y")),
            ("c", "vec4", false, None, None),
            ("f", "float", false, None, None),
            ("ids", "ivec4", true, None, Some("w")),
            ("pos", "vec3", true, None, Some("zy")),
        ];
        for max in (4..=10).rev() {
            for geometry in [Geometry::Fullscreen, Geometry::Terrain] {
                check(&vars, geometry, max);
            }
        }
        // 1 + 2 + 2 + 1 + 4 + 1 = 11 smooth (3 slots), 1 + 2 flat (1).
        let i = check(&vars, Geometry::Fullscreen, 4);
        assert_eq!(i.slots.len(), 4);
        assert!(i.pieces.iter().any(|p| p.partial));
        // hz_sprite takes the flat slot's free lane.
        let i = check(&vars, Geometry::Terrain, 4);
        assert!(
            matches!(i.sprite, SpriteCarrier::Bits(3, _)),
            "{:?}",
            i.sprite
        );
    }
}
