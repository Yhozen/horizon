//! The varyings both stages share, and their interface declarations.
//!
//! Each shared varying splits into *pieces*: one per array element and
//! matrix column, each a scalar or vector (WGSL passes no matrices or arrays
//! between stages). A piece normally has its own location (`hz_v_<name>`,
//! `_<k>` per element, `_c<j>` per column). When the pieces, plus Horizon's
//! own `hz_sprite` on terrain, need more locations than
//! [`HostParams::max_varyings`], they are packed into `vec4` slots instead
//! (`hz_vp<n>`): interpolated floats together in `vec4`s, integers and flat
//! floats (as their bits) together in flat `ivec4`s.

use super::*;

/// One interface variable.
pub(super) struct Slot {
    pub(super) name: String,
    pub(super) ty: String,
    pub(super) flat: bool,
}

/// A scalar or vector of a varying, carried in (part of) a slot.
struct Piece {
    /// The global it comes from and goes to (`m[2]`, `w[1][0]`).
    global: String,
    vertex_ty: String,
    fragment_ty: String,
    /// The type it travels as: the vertex type, or (packed) the slot's base
    /// type at its width.
    carrier: String,
    slot: usize,
    /// The slot's components it takes (`.xyz`), or `None` when it is the whole slot.
    swizzle: Option<String>,
    /// A flat float carried as its bits in an integer slot.
    bits: bool,
}

/// What the two stages pass each other.
pub(super) struct Interface {
    pub(super) slots: Vec<Slot>,
    pieces: Vec<Piece>,
    /// Fragment varyings the vertex stage does not declare: they read zero.
    zeroed: Vec<Var>,
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
        let own = u32::from(geometry == Geometry::Terrain);
        let mut interface = Self {
            slots: Vec::new(),
            pieces: Vec::new(),
            zeroed,
        };
        if parts.len() as u32 + own <= max {
            for (suffix, global, vertex_ty, fragment_ty, flat) in parts {
                interface.pieces.push(Piece {
                    global,
                    carrier: vertex_ty.clone(),
                    vertex_ty: vertex_ty.clone(),
                    fragment_ty,
                    slot: interface.slots.len(),
                    swizzle: None,
                    bits: false,
                });
                interface.slots.push(Slot {
                    name: format!("hz_v_{}", suffix.replace(['.', '[', ']'], "_")),
                    ty: vertex_ty,
                    flat,
                });
            }
        } else {
            interface.pack(parts);
        }
        let total = interface.slots.len() as u32 + own;
        if total > max {
            return Err(format!(
                "{total} varying locations after packing, the device allows {max}"
            ));
        }
        Ok(interface)
    }

    /// Packs pieces into 4-wide slots, widest first (first fit), one group
    /// of slots per kind: interpolated floats, then everything flat (integers,
    /// and flat floats as their bits) in `ivec4`s.
    fn pack(&mut self, parts: Vec<(String, String, String, String, bool)>) {
        for (group, slot_ty, flat) in [(0, "vec4", false), (2, "ivec4", true)] {
            let mut members: Vec<_> = parts
                .iter()
                .filter(|p| {
                    let integer =
                        integer_type(&p.2) == Some(true) || integer_type(&p.3) == Some(true);
                    match group {
                        0 => !p.4 && !integer,
                        _ => p.4 || integer,
                    }
                })
                .collect();
            // Stable: equal widths keep their name order.
            members.sort_by_key(|p| std::cmp::Reverse(width(&p.2)));
            let first = self.slots.len();
            let mut used: Vec<u32> = Vec::new();
            for (_, global, vertex_ty, fragment_ty, piece_flat) in members {
                let bits = *piece_flat
                    && integer_type(vertex_ty) == Some(false)
                    && integer_type(fragment_ty) == Some(false);
                let w = width(vertex_ty);
                let at = match used.iter().position(|u| u + w <= 4) {
                    Some(i) => i,
                    None => {
                        used.push(0);
                        self.slots.push(Slot {
                            name: format!("hz_vp{}", self.slots.len()),
                            ty: slot_ty.into(),
                            flat,
                        });
                        used.len() - 1
                    }
                };
                let offset = used[at];
                used[at] += w;
                self.pieces.push(Piece {
                    global: global.clone(),
                    vertex_ty: vertex_ty.clone(),
                    fragment_ty: fragment_ty.clone(),
                    carrier: vector(slot_ty, w),
                    slot: first + at,
                    swizzle: Some("xyzw"[offset as usize..(offset + w) as usize].into()),
                    bits,
                });
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
            if self
                .pieces
                .iter()
                .any(|p| p.slot == k && p.swizzle.is_some())
            {
                // Unused components are written too: WGSL outputs have no undefined parts.
                writeln!(out, "    {} = {}(0);", s.name, s.ty).unwrap();
            }
        }
        for p in &self.pieces {
            let slot = &self.slots[p.slot].name;
            let swizzle = p
                .swizzle
                .as_ref()
                .map_or(String::new(), |s| format!(".{s}"));
            let value = if p.bits {
                format!("floatBitsToInt({})", p.global)
            } else {
                convert(&p.carrier, &p.vertex_ty, &p.global)
            };
            writeln!(out, "    {slot}{swizzle} = {value};").unwrap();
        }
    }

    /// The fragment `main`'s statements filling the globals.
    pub(super) fn read(&self, out: &mut String) {
        for p in &self.pieces {
            let slot = &self.slots[p.slot].name;
            let swizzle = p
                .swizzle
                .as_ref()
                .map_or(String::new(), |s| format!(".{s}"));
            let value = if p.bits {
                format!("intBitsToFloat({slot}{swizzle})")
            } else {
                convert(&p.fragment_ty, &p.carrier, &format!("{slot}{swizzle}"))
            };
            writeln!(out, "    {} = {value};", p.global).unwrap();
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
                format!("float({g}.x)")
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
        // Terrain needs one more for hz_sprite: still fits.
        translate(&p, Geometry::Terrain, 16).unwrap();
        // Unpacked when the device allows them all.
        let (vs, _) = translate(&p, Geometry::Fullscreen, 28).unwrap();
        assert!(
            vs.contains("@location(16)") && !vs.contains("@location(17)"),
            "{vs}"
        );
        // Too many even packed.
        let err = translate(&p, Geometry::Terrain, 7).unwrap_err();
        assert!(err.contains("8 varying locations"), "{err}");
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
