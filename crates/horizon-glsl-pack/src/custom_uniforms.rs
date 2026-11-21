//! Custom uniforms: `uniform.<type>.<name>=<expr>` and
//! `variable.<type>.<name>=<expr>` lines of `shaders.properties`, computed on
//! the CPU once per frame. Uniforms are uploaded in a second std140 block
//! (group 1, binding 1, [`CUSTOM_BINDING`]); variables only feed other
//! expressions.
//!
//! Written from the format's public documentation: types `float int bool
//! vec2 vec3 vec4`; numbers, `true`/`false`, `pi`, the biome, category and
//! precipitation constants; other custom values and standard uniforms by
//! name, one element at a time (`cameraPosition.y`, `skyColor.r`, matrix
//! elements `gbufferModelView.3.0` = GLSL's `m[3][0]`); `+ - * / %`, `! && ||`,
//! comparisons; the documented functions (see [`call`]). A line that does not
//! parse, or whose value does not fit its type, is noted and reads 0: a pack
//! never fails for a custom uniform.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::uniforms::uniform_type;

/// The binding of the custom uniform block in group 1.
pub const CUSTOM_BINDING: u32 = 1;

/// The type of a custom value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    Float,
    Int,
    Bool,
    /// `vec2`…`vec4`.
    Vec(u8),
}

impl Ty {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "float" => Ty::Float,
            "int" => Ty::Int,
            "bool" => Ty::Bool,
            "vec2" => Ty::Vec(2),
            "vec3" => Ty::Vec(3),
            "vec4" => Ty::Vec(4),
            _ => return None,
        })
    }

    /// The pack-facing GLSL type.
    pub fn glsl(self) -> &'static str {
        match self {
            Ty::Float => "float",
            Ty::Int => "int",
            Ty::Bool => "bool",
            Ty::Vec(2) => "vec2",
            Ty::Vec(3) => "vec3",
            _ => "vec4",
        }
    }

    /// The type in the uniform block (WGSL has no `bool` uniforms).
    pub fn block_glsl(self) -> &'static str {
        match self {
            Ty::Bool => "int",
            t => t.glsl(),
        }
    }

    /// std140 `(size, alignment)`.
    fn std140(self) -> (usize, usize) {
        match self {
            Ty::Vec(2) => (8, 8),
            Ty::Vec(3) => (12, 16),
            Ty::Vec(_) => (16, 16),
            _ => (4, 4),
        }
    }
}

/// A value while evaluating.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Val {
    F(f32),
    I(i32),
    B(bool),
    V(u8, [f32; 4]),
}

impl Val {
    fn f(self) -> Result<f32, String> {
        match self {
            Val::F(f) => Ok(f),
            Val::I(i) => Ok(i as f32),
            Val::B(b) => Ok(f32::from(u8::from(b))),
            Val::V(n, _) => Err(format!("a vec{n} where a number is needed")),
        }
    }

    fn b(self) -> Result<bool, String> {
        match self {
            Val::B(b) => Ok(b),
            Val::F(f) => Ok(f != 0.0),
            Val::I(i) => Ok(i != 0),
            Val::V(n, _) => Err(format!("a vec{n} where a boolean is needed")),
        }
    }

    /// Components (a scalar is one).
    fn comps(self) -> (u8, [f32; 4], bool) {
        match self {
            Val::V(n, v) => (n, v, false),
            Val::I(i) => (1, [i as f32, 0.0, 0.0, 0.0], true),
            s => (1, [s.f().unwrap_or(0.0), 0.0, 0.0, 0.0], false),
        }
    }

    fn zero(ty: Ty) -> Val {
        match ty {
            Ty::Float => Val::F(0.0),
            Ty::Int => Val::I(0),
            Ty::Bool => Val::B(false),
            Ty::Vec(n) => Val::V(n, [0.0; 4]),
        }
    }

    /// The value as `ty` (numbers convert; vectors only to their own size).
    fn to(self, ty: Ty) -> Result<Val, String> {
        Ok(match ty {
            Ty::Float => Val::F(self.f()?),
            Ty::Int => match self {
                Val::I(i) => Val::I(i),
                v => Val::I(v.f()? as i32),
            },
            Ty::Bool => Val::B(self.b()?),
            Ty::Vec(n) => match self {
                Val::V(m, v) if m == n => Val::V(n, v),
                Val::V(m, _) => return Err(format!("a vec{m} for a vec{n}")),
                _ => return Err(format!("a number for a vec{n}")),
            },
        })
    }
}

/// The biomes of 26.3 in the order their `BIOME_*` constants count (the
/// registry's, alphabetical): `BIOME_BADLANDS` = 0 … The client's `biome`
/// uniform uses [`biome_id`] with the same list.
pub const BIOMES: &[&str] = &[
    "badlands",
    "bamboo_jungle",
    "basalt_deltas",
    "beach",
    "birch_forest",
    "cherry_grove",
    "cold_ocean",
    "crimson_forest",
    "dappled_forest",
    "dark_forest",
    "deep_cold_ocean",
    "deep_dark",
    "deep_frozen_ocean",
    "deep_lukewarm_ocean",
    "deep_ocean",
    "desert",
    "dripstone_caves",
    "end_barrens",
    "end_highlands",
    "end_midlands",
    "eroded_badlands",
    "flower_forest",
    "forest",
    "frozen_ocean",
    "frozen_peaks",
    "frozen_river",
    "grove",
    "ice_spikes",
    "jagged_peaks",
    "jungle",
    "lukewarm_ocean",
    "lush_caves",
    "mangrove_swamp",
    "meadow",
    "mushroom_fields",
    "nether_wastes",
    "ocean",
    "old_growth_birch_forest",
    "old_growth_pine_taiga",
    "old_growth_spruce_taiga",
    "pale_garden",
    "plains",
    "river",
    "savanna",
    "savanna_plateau",
    "small_end_islands",
    "snowy_beach",
    "snowy_plains",
    "snowy_slopes",
    "snowy_taiga",
    "soul_sand_valley",
    "sparse_jungle",
    "stony_peaks",
    "stony_shore",
    "sulfur_caves",
    "sunflower_plains",
    "swamp",
    "taiga",
    "the_end",
    "the_void",
    "warm_ocean",
    "warped_forest",
    "windswept_forest",
    "windswept_gravelly_hills",
    "windswept_hills",
    "windswept_savanna",
    "wooded_badlands",
];

/// The biome categories in the documented order: `CAT_NONE` = 0 …
pub const CATEGORIES: [&str; 17] = [
    "NONE",
    "TAIGA",
    "EXTREME_HILLS",
    "JUNGLE",
    "MESA",
    "PLAINS",
    "SAVANNA",
    "ICY",
    "THE_END",
    "BEACH",
    "FOREST",
    "OCEAN",
    "DESERT",
    "RIVER",
    "SWAMP",
    "MUSHROOM",
    "NETHER",
];

/// `BIOME_<NAME>` of a biome (`plains` or `minecraft:plains`); -1 for one
/// that is not vanilla's.
pub fn biome_id(biome: &str) -> i32 {
    let name = biome.strip_prefix("minecraft:").unwrap_or(biome);
    BIOMES
        .iter()
        .position(|b| *b == name)
        .map_or(-1, |i| i as i32)
}

/// `CAT_<NAME>` of a biome: vanilla's old categories, which current
/// versions no longer store, from the name (an approximation).
pub fn biome_category(biome: &str) -> i32 {
    let n = biome.strip_prefix("minecraft:").unwrap_or(biome);
    let has = |s: &str| n.contains(s);
    let cat = if matches!(
        n,
        "nether_wastes" | "soul_sand_valley" | "crimson_forest" | "warped_forest" | "basalt_deltas"
    ) {
        "NETHER"
    } else if n.starts_with("end_") || n == "the_end" || n == "small_end_islands" {
        "THE_END"
    } else if has("ocean") {
        "OCEAN"
    } else if has("river") {
        "RIVER"
    } else if has("beach") || has("shore") {
        "BEACH"
    } else if has("taiga") {
        "TAIGA"
    } else if has("jungle") {
        "JUNGLE"
    } else if has("badlands") {
        "MESA"
    } else if has("savanna") {
        "SAVANNA"
    } else if has("desert") {
        "DESERT"
    } else if has("swamp") {
        "SWAMP"
    } else if has("mushroom") {
        "MUSHROOM"
    } else if has("snowy") || has("ice") || has("frozen") || n == "grove" {
        "ICY"
    } else if has("windswept") || has("peaks") || has("slopes") {
        "EXTREME_HILLS"
    } else if has("forest") || n == "pale_garden" || n == "cherry_grove" {
        "FOREST"
    } else if has("plains") || n == "meadow" {
        "PLAINS"
    } else {
        "NONE"
    };
    CATEGORIES.iter().position(|c| *c == cat).unwrap_or(0) as i32
}

/// The value of a `BIOME_*` / `CAT_*` constant this version does not have.
pub const NO_ID: i32 = -1000;

/// Constants by name: `pi`, `true`, `false`, `BIOME_*`, `CAT_*`, `PPT_*`.
fn constant(name: &str) -> Option<Val> {
    match name {
        "pi" => return Some(Val::F(std::f32::consts::PI)),
        "true" => return Some(Val::B(true)),
        "false" => return Some(Val::B(false)),
        "PPT_NONE" => return Some(Val::I(0)),
        "PPT_RAIN" => return Some(Val::I(1)),
        "PPT_SNOW" => return Some(Val::I(2)),
        _ => {}
    }
    // A biome or category this version lacks (packs list future ones)
    // matches nothing, not biome 0.
    if let Some(b) = name.strip_prefix("BIOME_") {
        let id = biome_id(&b.to_ascii_lowercase());
        return Some(Val::I(if id >= 0 { id } else { NO_ID }));
    }
    let c = name.strip_prefix("CAT_")?;
    Some(Val::I(
        CATEGORIES
            .iter()
            .position(|n| *n == c)
            .map_or(NO_ID, |i| i as i32),
    ))
}

/// Standard values the format documents for custom uniforms that are not
/// members of Horizon's block; the client returns them by name (0 until it
/// does).
pub const PARAMETERS: &[(&str, &str)] = &[
    ("is_alive", "bool"),
    ("is_burning", "bool"),
    ("is_child", "bool"),
    ("is_glowing", "bool"),
    ("is_hurt", "bool"),
    ("is_in_lava", "bool"),
    ("is_in_water", "bool"),
    ("is_invisible", "bool"),
    ("is_on_ground", "bool"),
    ("is_ridden", "bool"),
    ("is_riding", "bool"),
    ("is_sneaking", "bool"),
    ("is_sprinting", "bool"),
    ("is_wet", "bool"),
];

/// The type of a standard value by name (`cameraPosition`, `is_sneaking`…).
fn standard_type(name: &str) -> Option<Ty> {
    let ty = uniform_type(name)
        .or_else(|| PARAMETERS.iter().find(|(n, _)| *n == name).map(|&(_, t)| t))?;
    Some(match ty {
        "float" => Ty::Float,
        "int" => Ty::Int,
        "bool" => Ty::Bool,
        "vec2" | "ivec2" => Ty::Vec(2),
        "vec3" | "ivec3" => Ty::Vec(3),
        "vec4" | "ivec4" => Ty::Vec(4),
        "mat4" => Ty::Vec(4),
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

/// A parsed expression.
#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Const(Val),
    /// A custom value by slot, optionally one element of it.
    Custom(usize, Option<u8>),
    /// A standard value: the client's name (`cameraPosition`,
    /// `gbufferModelView.3` for a matrix column), its type, an element.
    Standard(String, Ty, Option<u8>),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    /// `smooth(..)`: its state slot and the value, fade-up and fade-down
    /// half-lives in seconds.
    Smooth(usize, Box<Expr>, Option<Box<Expr>>, Option<Box<Expr>>),
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f32, bool),
    Name(String),
    Op(&'static str),
    Open,
    Close,
    Comma,
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let start = i;
            let mut int = true;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                int &= b[i] != b'.';
                i += 1;
            }
            if i < b.len() && matches!(b[i], b'e' | b'E') {
                let mut j = i + 1;
                if j < b.len() && matches!(b[j], b'+' | b'-') {
                    j += 1;
                }
                if j < b.len() && b[j].is_ascii_digit() {
                    int = false;
                    i = j;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let text = &s[start..i];
            if i < b.len() && matches!(b[i], b'f' | b'F' | b'd' | b'D') {
                int = false;
                i += 1;
            } else if i < b.len() && matches!(b[i], b'l' | b'L') {
                i += 1;
            }
            let v: f32 = text.parse().map_err(|_| format!("bad number {text}"))?;
            out.push(Tok::Num(v, int));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < b.len()
                && (b[i].is_ascii_alphanumeric()
                    || b[i] == b'_'
                    || (b[i] == b'.' && b.get(i + 1).is_some_and(|n| n.is_ascii_alphanumeric())))
            {
                i += 1;
            }
            out.push(Tok::Name(s[start..i].to_string()));
        } else {
            let two = s.get(i..i + 2).unwrap_or("");
            let op: &'static str = match two {
                "&&" => "&&",
                "||" => "||",
                "==" => "==",
                "!=" => "!=",
                "<=" => "<=",
                ">=" => ">=",
                _ => "",
            };
            if !op.is_empty() {
                out.push(Tok::Op(op));
                i += 2;
                continue;
            }
            out.push(match c {
                '(' => Tok::Open,
                ')' => Tok::Close,
                ',' => Tok::Comma,
                '+' => Tok::Op("+"),
                '-' => Tok::Op("-"),
                '*' => Tok::Op("*"),
                '/' => Tok::Op("/"),
                '%' => Tok::Op("%"),
                '<' => Tok::Op("<"),
                '>' => Tok::Op(">"),
                '!' => Tok::Op("!"),
                _ => return Err(format!("unexpected `{c}`")),
            });
            i += 1;
        }
    }
    Ok(out)
}

/// What names resolve to while parsing.
struct Scope<'a> {
    /// Custom name → (slot, type).
    customs: &'a BTreeMap<String, (usize, Ty)>,
    smooth_slots: &'a mut usize,
    /// Names that read 0 because nothing provides them.
    unknown: &'a mut Vec<String>,
}

struct Parser<'a, 'b> {
    toks: Vec<Tok>,
    at: usize,
    scope: &'a mut Scope<'b>,
}

fn element(s: &str) -> Option<u8> {
    Some(match s {
        "x" | "r" | "s" | "0" => 0,
        "y" | "g" | "t" | "1" => 1,
        "z" | "b" | "p" | "2" => 2,
        "w" | "a" | "q" | "3" => 3,
        _ => return None,
    })
}

impl Parser<'_, '_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn eat_op(&mut self, ops: &[&str]) -> Option<&'static str> {
        match self.peek() {
            Some(Tok::Op(o)) if ops.contains(o) => {
                let o = *o;
                self.at += 1;
                Some(o)
            }
            _ => None,
        }
    }

    fn binary(
        &mut self,
        ops: &[(&str, Op)],
        next: fn(&mut Self) -> Result<Expr, String>,
    ) -> Result<Expr, String> {
        let mut left = next(self)?;
        let names: Vec<&str> = ops.iter().map(|(n, _)| *n).collect();
        while let Some(o) = self.eat_op(&names) {
            let op = ops.iter().find(|(n, _)| *n == o).unwrap().1;
            let right = next(self)?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn or(&mut self) -> Result<Expr, String> {
        self.binary(&[("||", Op::Or)], Self::and)
    }
    fn and(&mut self) -> Result<Expr, String> {
        self.binary(&[("&&", Op::And)], Self::equality)
    }
    fn equality(&mut self) -> Result<Expr, String> {
        self.binary(&[("==", Op::Eq), ("!=", Op::Ne)], Self::relation)
    }
    fn relation(&mut self) -> Result<Expr, String> {
        self.binary(
            &[("<", Op::Lt), ("<=", Op::Le), (">", Op::Gt), (">=", Op::Ge)],
            Self::sum,
        )
    }
    fn sum(&mut self) -> Result<Expr, String> {
        self.binary(&[("+", Op::Add), ("-", Op::Sub)], Self::product)
    }
    fn product(&mut self) -> Result<Expr, String> {
        self.binary(
            &[("*", Op::Mul), ("/", Op::Div), ("%", Op::Rem)],
            Self::unary,
        )
    }

    fn unary(&mut self) -> Result<Expr, String> {
        match self.eat_op(&["-", "!", "+"]) {
            Some("-") => Ok(match self.unary()? {
                Expr::Const(Val::F(f)) => Expr::Const(Val::F(-f)),
                Expr::Const(Val::I(i)) => Expr::Const(Val::I(i.wrapping_neg())),
                e => Expr::Neg(Box::new(e)),
            }),
            Some("!") => Ok(Expr::Not(Box::new(self.unary()?))),
            Some(_) => self.unary(),
            None => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<Expr, String> {
        let tok = self.peek().cloned().ok_or("unexpected end")?;
        self.at += 1;
        match tok {
            Tok::Num(v, int) => Ok(Expr::Const(if int && v.fract() == 0.0 && v.abs() < 2.1e9 {
                Val::I(v as i32)
            } else {
                Val::F(v)
            })),
            Tok::Open => {
                let e = self.or()?;
                match self.peek() {
                    Some(Tok::Close) => {
                        self.at += 1;
                        Ok(e)
                    }
                    _ => Err("missing `)`".into()),
                }
            }
            Tok::Name(name) if self.peek() == Some(&Tok::Open) => {
                self.at += 1;
                let mut args = Vec::new();
                if self.peek() == Some(&Tok::Close) {
                    self.at += 1;
                } else {
                    loop {
                        args.push(self.or()?);
                        match self.peek() {
                            Some(Tok::Comma) => self.at += 1,
                            Some(Tok::Close) => {
                                self.at += 1;
                                break;
                            }
                            _ => return Err(format!("missing `)` after {name}(")),
                        }
                    }
                }
                self.function(name, args)
            }
            Tok::Name(name) => Ok(self.name(&name)),
            t => Err(format!("unexpected {t:?}")),
        }
    }

    fn function(&mut self, name: String, mut args: Vec<Expr>) -> Result<Expr, String> {
        if name == "smooth" {
            // smooth([id], value, [fadeUp, [fadeDown]]): four arguments carry the
            // id; fewer start with the value (an id names the state, which is
            // per call here anyway).
            if args.is_empty() || args.len() > 4 {
                return Err("smooth takes 1 to 4 arguments".into());
            }
            if args.len() == 4 {
                args.remove(0);
            }
            let mut it = args.into_iter();
            let value = Box::new(it.next().unwrap());
            let up = it.next().map(Box::new);
            let down = it.next().map(Box::new);
            let slot = *self.scope.smooth_slots;
            *self.scope.smooth_slots += 1;
            return Ok(Expr::Smooth(slot, value, up, down));
        }
        let arity = |ok: bool| {
            if ok {
                Ok(())
            } else {
                Err(format!("{name} with {} arguments", args.len()))
            }
        };
        let n = args.len();
        match name.as_str() {
            "sin" | "cos" | "tan" | "asin" | "acos" | "exp" | "exp2" | "exp10" | "log2"
            | "log10" | "sqrt" | "abs" | "signum" | "sign" | "floor" | "ceil" | "frac"
            | "round" | "torad" | "radians" | "todeg" | "degrees" => arity(n == 1)?,
            "atan" | "log" => arity(n == 1 || n == 2)?,
            "atan2" | "pow" | "fmod" | "edge" | "min" | "max" => arity(n >= 2)?,
            "clamp" | "mix" | "between" => arity(n == 3)?,
            "equals" => arity(n == 2 || n == 3)?,
            "if" | "ifb" => arity(n >= 3 && n % 2 == 1)?,
            "in" => arity(n >= 2)?,
            "random" | "randomInt" => arity(n == 0 || n == 2)?,
            // One argument fills every component, as in GLSL.
            "vec2" => arity(n == 1 || n == 2)?,
            "vec3" => arity(n == 1 || n == 3)?,
            "vec4" => arity(n == 1 || n == 4)?,
            "print" => arity(n >= 1)?,
            _ => return Err(format!("unknown function {name}")),
        }
        Ok(Expr::Call(name, args))
    }

    fn name(&mut self, name: &str) -> Expr {
        if let Some(&(slot, _)) = self.scope.customs.get(name) {
            return Expr::Custom(slot, None);
        }
        if let Some(v) = constant(name) {
            return Expr::Const(v);
        }
        if let Some(ty) = standard_type(name) {
            return Expr::Standard(name.to_string(), ty, None);
        }
        let parts: Vec<&str> = name.split('.').collect();
        let base = parts[0];
        match parts.len() {
            2 => {
                if let (Some(&(slot, _)), Some(e)) =
                    (self.scope.customs.get(base), element(parts[1]))
                {
                    return Expr::Custom(slot, Some(e));
                }
                if let (Some(ty), Some(e)) = (standard_type(base), element(parts[1])) {
                    return Expr::Standard(base.to_string(), ty, Some(e));
                }
            }
            3 if uniform_type(base) == Some("mat4") => {
                if let (Some(c), Some(r)) = (element(parts[1]), element(parts[2])) {
                    return Expr::Standard(format!("{base}.{c}"), Ty::Vec(4), Some(r));
                }
            }
            _ => {}
        }
        self.scope.unknown.push(name.to_string());
        Expr::Const(Val::F(0.0))
    }
}

/// One custom value.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomDef {
    pub name: String,
    pub ty: Ty,
    /// `uniform.*` (uploaded) rather than `variable.*`.
    pub uniform: bool,
    /// Byte offset in the block (uniforms only).
    pub offset: usize,
    expr: Expr,
    /// The first error while evaluating it was noted.
    failed: bool,
}

/// A pack's custom uniforms and variables, with what they remember between
/// frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CustomUniforms {
    /// In evaluation order (file order; a name defined again keeps its last
    /// definition at the last position).
    pub defs: Vec<CustomDef>,
    /// Lines that did not parse or fit their type, names that read 0.
    pub notes: Vec<String>,
    /// Block size in bytes (a multiple of 16; 0 without uniforms).
    size: usize,
    values: Vec<Val>,
    smooth: Vec<Option<[f32; 4]>>,
    seed: u64,
}

/// A standard value by name for [`CustomUniforms::evaluate`]: `[x, y, z, w]`
/// (scalars in `x`; `name.N` is a matrix's column N).
pub type Standard<'a> = &'a dyn Fn(&str) -> Option<[f32; 4]>;

impl CustomUniforms {
    /// Reads every `uniform.*` / `variable.*` entry of `shaders.properties`
    /// (live keys in file order).
    pub fn parse(entries: &[(String, String)]) -> Self {
        let mut out = Self {
            seed: 0x9E37_79B9_7F4A_7C15,
            ..Self::default()
        };
        // The last definition of a name wins, at its position.
        let mut lines: Vec<(bool, Ty, String, &str)> = Vec::new();
        for (key, value) in entries {
            let (uniform, rest) =
                match (key.strip_prefix("uniform."), key.strip_prefix("variable.")) {
                    (Some(r), _) => (true, r),
                    (_, Some(r)) => (false, r),
                    _ => continue,
                };
            let Some((ty, name)) = rest.split_once('.') else {
                out.notes.push(format!("{key}: no type"));
                continue;
            };
            let Some(ty) = Ty::parse(ty) else {
                out.notes.push(format!("{key}: unknown type {ty}"));
                continue;
            };
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                out.notes.push(format!("{key}: bad name"));
                continue;
            }
            if uniform && uniform_type(name).is_some() {
                out.notes.push(format!(
                    "{key}: {name} is a standard uniform; the standard one is kept"
                ));
                continue;
            }
            // A stray `;` at the end is tolerated.
            let value = value.trim_end().trim_end_matches(';');
            lines.retain(|l| l.2 != name);
            lines.push((uniform, ty, name.to_string(), value));
        }
        // Every name is known up front: a value may read one defined later
        // (it then sees last frame's value).
        let customs: BTreeMap<String, (usize, Ty)> = lines
            .iter()
            .enumerate()
            .map(|(slot, (_, ty, name, _))| (name.clone(), (slot, *ty)))
            .collect();
        let mut smooth_slots = 0;
        for (uniform, ty, name, text) in &lines {
            let mut unknown = Vec::new();
            let parsed = lex(text).and_then(|toks| {
                let mut scope = Scope {
                    customs: &customs,
                    smooth_slots: &mut smooth_slots,
                    unknown: &mut unknown,
                };
                let mut p = Parser {
                    toks,
                    at: 0,
                    scope: &mut scope,
                };
                let e = p.or()?;
                match p.peek() {
                    None => Ok(e),
                    Some(t) => Err(format!("unexpected {t:?} after the expression")),
                }
            });
            let kind = if *uniform { "uniform" } else { "variable" };
            let key = format!("{kind}.{}.{name}", ty.glsl());
            for u in unknown {
                out.notes.push(format!("{key}: {u} is unknown and reads 0"));
            }
            let expr = match parsed {
                Ok(e) => e,
                Err(e) => {
                    out.notes.push(format!("{key}: {e}; it reads 0"));
                    Expr::Const(Val::zero(*ty))
                }
            };
            out.defs.push(CustomDef {
                name: name.clone(),
                ty: *ty,
                uniform: *uniform,
                offset: 0,
                expr,
                failed: false,
            });
        }
        out.values = out.defs.iter().map(|d| Val::zero(d.ty)).collect();
        out.smooth = vec![None; smooth_slots];
        // Type check with every standard value at 0: a mismatch is noted now.
        let mut check = out.clone();
        check.evaluate_values(&|_| Some([0.0; 4]), 0.0);
        for (d, c) in out.defs.iter_mut().zip(&check.defs) {
            if c.failed {
                d.expr = Expr::Const(Val::zero(d.ty));
            }
        }
        out.notes
            .extend(check.notes.into_iter().skip(out.notes.len()));
        // std140 offsets of the uploaded ones, in order.
        let mut at: usize = 0;
        for d in out.defs.iter_mut().filter(|d| d.uniform) {
            let (size, align) = d.ty.std140();
            at = at.div_ceil(align) * align;
            d.offset = at;
            at += size;
        }
        out.size = at.div_ceil(16) * 16;
        out
    }

    /// Whether the pack uploads any custom uniform.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// The block's size in bytes (a multiple of 16).
    pub fn size(&self) -> usize {
        self.size
    }

    /// The uploaded uniform called `name`.
    pub fn uniform(&self, name: &str) -> Option<&CustomDef> {
        self.defs.iter().find(|d| d.uniform && d.name == name)
    }

    /// The GLSL block (set 1, binding [`CUSTOM_BINDING`]); members are
    /// `hz_c_<name>`, so the pack's names come from `#define`s the host
    /// writes for the uniforms a program declares.
    pub fn block(&self) -> String {
        let mut out = format!(
            "layout(std140, set = 1, binding = {CUSTOM_BINDING}) uniform HzCustomUniforms {{\n"
        );
        for d in self.defs.iter().filter(|d| d.uniform) {
            writeln!(out, "    {} hz_c_{};", d.ty.block_glsl(), d.name).unwrap();
        }
        out + "};\n"
    }

    /// The current value of a custom uniform or variable.
    pub fn value(&self, name: &str) -> Option<Val> {
        let i = self.defs.iter().position(|d| d.name == name)?;
        self.values.get(i).copied()
    }

    /// Computes every value for this frame (`frame_time` in seconds) and
    /// writes the block's bytes into `out` (resized to [`Self::size`]).
    pub fn evaluate(&mut self, standard: Standard, frame_time: f32, out: &mut Vec<u8>) {
        self.evaluate_values(standard, frame_time);
        out.clear();
        out.resize(self.size, 0);
        for (d, v) in self.defs.iter().zip(&self.values) {
            if !d.uniform {
                continue;
            }
            let mut put = |i: usize, bytes: [u8; 4]| {
                out[d.offset + 4 * i..d.offset + 4 * i + 4].copy_from_slice(&bytes);
            };
            match *v {
                Val::F(f) => put(0, f.to_le_bytes()),
                Val::I(i) => put(0, i.to_le_bytes()),
                Val::B(b) => put(0, i32::from(b).to_le_bytes()),
                Val::V(n, c) => {
                    for (i, x) in c.iter().take(usize::from(n)).enumerate() {
                        put(i, x.to_le_bytes());
                    }
                }
            }
        }
    }

    fn evaluate_values(&mut self, standard: Standard, frame_time: f32) {
        for i in 0..self.defs.len() {
            let mut ctx = Ctx {
                standard,
                values: &self.values,
                smooth: &mut self.smooth,
                seed: &mut self.seed,
                dt: frame_time,
            };
            let def = &self.defs[i];
            let v = eval(&def.expr, &mut ctx).and_then(|v| v.to(def.ty));
            self.values[i] = match v {
                Ok(v) => v,
                Err(e) => {
                    let def = &mut self.defs[i];
                    if !def.failed {
                        def.failed = true;
                        let kind = if def.uniform { "uniform" } else { "variable" };
                        self.notes.push(format!(
                            "{kind}.{}.{}: {e}; it reads 0",
                            def.ty.glsl(),
                            def.name
                        ));
                    }
                    Val::zero(def.ty)
                }
            };
        }
    }
}

struct Ctx<'a> {
    standard: Standard<'a>,
    values: &'a [Val],
    smooth: &'a mut [Option<[f32; 4]>],
    seed: &'a mut u64,
    dt: f32,
}

impl Ctx<'_> {
    fn random(&mut self) -> f32 {
        // xorshift64*: a new number every call, so every frame.
        let mut x = *self.seed;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        *self.seed = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / (1u64 << 24) as f32
    }
}

fn pick(v: Val, e: Option<u8>) -> Result<Val, String> {
    match (v, e) {
        (v, None) => Ok(v),
        (Val::V(n, c), Some(i)) if i < n => Ok(Val::F(c[usize::from(i)])),
        (Val::V(n, _), Some(i)) => Err(format!("element {i} of a vec{n}")),
        (_, Some(_)) => Err("an element of a number".into()),
    }
}

/// Applies `f` per component (scalars stay scalars; ints stay ints when `f`
/// keeps whole numbers and every input is an int).
fn map(args: &[Val], keep_int: bool, f: impl Fn(&[f32]) -> f32) -> Result<Val, String> {
    let mut n = 1;
    for a in args {
        let (m, _, _) = a.comps();
        if m > 1 {
            if n > 1 && m != n {
                return Err(format!("vec{n} with vec{m}"));
            }
            n = m;
        }
    }
    let all_int = keep_int && args.iter().all(|a| matches!(a, Val::I(_)));
    let mut out = [0.0; 4];
    for (i, o) in out.iter_mut().enumerate().take(usize::from(n)) {
        let xs: Vec<f32> = args
            .iter()
            .map(|a| {
                let (m, c, _) = a.comps();
                if m == 1 { c[0] } else { c[i] }
            })
            .collect();
        *o = f(&xs);
    }
    Ok(if n > 1 {
        Val::V(n, out)
    } else if all_int {
        Val::I(out[0] as i32)
    } else {
        Val::F(out[0])
    })
}

fn eval(e: &Expr, ctx: &mut Ctx) -> Result<Val, String> {
    Ok(match e {
        Expr::Const(v) => *v,
        Expr::Custom(slot, el) => pick(ctx.values[*slot], *el)?,
        Expr::Standard(name, ty, el) => {
            let raw = (ctx.standard)(name).unwrap_or([0.0; 4]);
            let v = match ty {
                Ty::Float => Val::F(raw[0]),
                Ty::Int => Val::I(raw[0] as i32),
                Ty::Bool => Val::B(raw[0] != 0.0),
                Ty::Vec(n) => Val::V(*n, raw),
            };
            pick(v, *el)?
        }
        Expr::Neg(x) => match eval(x, ctx)? {
            Val::I(i) => Val::I(i.wrapping_neg()),
            v => map(&[v], false, |x| -x[0])?,
        },
        Expr::Not(x) => Val::B(!eval(x, ctx)?.b()?),
        Expr::Bin(op, a, b) => {
            let (a, b) = (eval(a, ctx)?, eval(b, ctx)?);
            binary(*op, a, b)?
        }
        Expr::Smooth(slot, value, up, down) => {
            let v = eval(value, ctx)?;
            let up = match up {
                Some(u) => eval(u, ctx)?.f()?,
                None => 1.0,
            };
            let down = match down {
                Some(d) => eval(d, ctx)?.f()?,
                None => up,
            };
            let (n, target, _) = v.comps();
            let state = ctx.smooth[*slot].get_or_insert(target);
            for i in 0..usize::from(n) {
                let half = if target[i] > state[i] { up } else { down };
                state[i] = if half <= 0.0 {
                    target[i]
                } else {
                    let k = 1.0 - 0.5f32.powf(ctx.dt / half);
                    state[i] + (target[i] - state[i]) * k
                };
            }
            let s = *state;
            if n > 1 { Val::V(n, s) } else { Val::F(s[0]) }
        }
        Expr::Call(name, args) => {
            let mut vals = Vec::with_capacity(args.len());
            for a in args {
                vals.push(eval(a, ctx)?);
            }
            call(name, &vals, ctx)?
        }
    })
}

fn binary(op: Op, a: Val, b: Val) -> Result<Val, String> {
    use Op::*;
    // `/` is always float division: OptiFine's shaders.txt ("Custom uniforms") has only floating point
    // constants and lists `worldTime` and the other scalar uniforms as float parameters, and Iris draws
    // BSL Classic's `timeAngle = worldTime / 24000` as noon at day time 6000 (integer division gives 0,
    // sunrise). The other operators give the same value either way, so they stay integral.
    let ints = op != Div && matches!((a, b), (Val::I(_), Val::I(_)));
    Ok(match op {
        And => Val::B(a.b()? && b.b()?),
        Or => Val::B(a.b()? || b.b()?),
        Eq | Ne => {
            let (_, x, _) = a.comps();
            let (_, y, _) = b.comps();
            let same = if let (Val::V(n, _), Val::V(m, _)) = (a, b) {
                n == m && x == y
            } else {
                a.f()? == b.f()?
            };
            Val::B(same == (op == Eq))
        }
        Lt => Val::B(a.f()? < b.f()?),
        Le => Val::B(a.f()? <= b.f()?),
        Gt => Val::B(a.f()? > b.f()?),
        Ge => Val::B(a.f()? >= b.f()?),
        _ if ints => {
            let (Val::I(x), Val::I(y)) = (a, b) else {
                unreachable!()
            };
            Val::I(match op {
                Add => x.wrapping_add(y),
                Sub => x.wrapping_sub(y),
                Mul => x.wrapping_mul(y),
                _ => x.checked_rem(y).unwrap_or(0),
            })
        }
        _ => map(&[a, b], false, |v| match op {
            Add => v[0] + v[1],
            Sub => v[0] - v[1],
            Mul => v[0] * v[1],
            Div => v[0] / v[1],
            _ => v[0] % v[1],
        })?,
    })
}

/// The documented functions. Angles are radians; `fmod` is the floored
/// modulus and `%` Java's remainder; `round` rounds half up; `random()` is in
/// [0, 1) and new each frame; `smooth` is [`Expr::Smooth`].
fn call(name: &str, a: &[Val], ctx: &mut Ctx) -> Result<Val, String> {
    let f1 = |f: fn(f32) -> f32| map(a, false, |x| f(x[0]));
    Ok(match name {
        "sin" => f1(f32::sin)?,
        "cos" => f1(f32::cos)?,
        "tan" => f1(f32::tan)?,
        "asin" => f1(f32::asin)?,
        "acos" => f1(f32::acos)?,
        "atan" if a.len() == 1 => f1(f32::atan)?,
        "atan" | "atan2" => map(a, false, |x| x[0].atan2(x[1]))?,
        "exp" => f1(f32::exp)?,
        "exp2" => f1(f32::exp2)?,
        "exp10" => f1(|x| 10f32.powf(x))?,
        "log" if a.len() == 1 => f1(f32::ln)?,
        "log" => map(a, false, |x| x[1].ln() / x[0].ln())?,
        "log2" => f1(f32::log2)?,
        "log10" => f1(f32::log10)?,
        "sqrt" => f1(f32::sqrt)?,
        "pow" => map(a, false, |x| x[0].powf(x[1]))?,
        "torad" | "radians" => f1(f32::to_radians)?,
        "todeg" | "degrees" => f1(f32::to_degrees)?,
        "abs" => map(a, true, |x| x[0].abs())?,
        "signum" | "sign" => map(a, true, |x| {
            if x[0] > 0.0 {
                1.0
            } else if x[0] < 0.0 {
                -1.0
            } else {
                0.0
            }
        })?,
        "floor" => map(a, true, |x| x[0].floor())?,
        "ceil" => map(a, true, |x| x[0].ceil())?,
        "round" => map(a, true, |x| (x[0] + 0.5).floor())?,
        "frac" => f1(|x| x - x.floor())?,
        "fmod" => map(a, false, |x| x[0] - x[1] * (x[0] / x[1]).floor())?,
        "min" => map(a, true, |x| x.iter().copied().fold(f32::INFINITY, f32::min))?,
        "max" => map(a, true, |x| {
            x.iter().copied().fold(f32::NEG_INFINITY, f32::max)
        })?,
        "clamp" => map(a, true, |x| x[0].max(x[1]).min(x[2]))?,
        "mix" => map(a, false, |x| x[0] + (x[1] - x[0]) * x[2])?,
        "edge" => map(a, false, |x| if x[1] < x[0] { 0.0 } else { 1.0 })?,
        "between" => {
            let x = a[0].f()?;
            Val::B(x >= a[1].f()? && x <= a[2].f()?)
        }
        "equals" => {
            let eps = match a.get(2) {
                Some(e) => e.f()?,
                None => 0.0,
            };
            Val::B((a[0].f()? - a[1].f()?).abs() <= eps)
        }
        "in" => {
            let x = a[0].f()?;
            let mut hit = false;
            for v in &a[1..] {
                hit |= v.f()? == x;
            }
            Val::B(hit)
        }
        "if" | "ifb" => {
            let mut i = 0;
            while i + 1 < a.len() {
                if a[i].b()? {
                    return Ok(a[i + 1]);
                }
                i += 2;
            }
            a[a.len() - 1]
        }
        "random" => {
            let r = ctx.random();
            match a {
                [lo, hi] => Val::F(lo.f()? + (hi.f()? - lo.f()?) * r),
                _ => Val::F(r),
            }
        }
        "randomInt" => {
            let r = ctx.random();
            match a {
                [lo, hi] => {
                    let (lo, hi) = (lo.f()? as i64, hi.f()? as i64);
                    Val::I((lo + ((hi - lo).max(0) as f32 * r) as i64) as i32)
                }
                _ => Val::I((r * u32::MAX as f32) as u32 as i32),
            }
        }
        "vec2" | "vec3" | "vec4" => {
            let n = name.as_bytes()[3] - b'0';
            let mut c = [0.0; 4];
            for (i, o) in c.iter_mut().enumerate().take(usize::from(n)) {
                *o = a[if a.len() == 1 { 0 } else { i }].f()?;
            }
            Val::V(n, c)
        }
        "print" => a[a.len() - 1],
        _ => return Err(format!("unknown function {name}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(lines: &[(&str, &str)]) -> Vec<(String, String)> {
        lines
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// Evaluates `expr` as a float variable with the standard values below.
    fn value_of(expr: &str) -> Val {
        let mut c = CustomUniforms::parse(&entries(&[
            ("variable.float.a", "2.5"),
            ("variable.vec3.v", "vec3(1, 2, 3)"),
            ("uniform.float.x", expr),
        ]));
        assert!(c.notes.is_empty(), "{expr}: {:?}", c.notes);
        c.evaluate(&standard, 0.05, &mut Vec::new());
        c.value("x").unwrap()
    }

    fn standard(name: &str) -> Option<[f32; 4]> {
        Some(match name {
            "sunAngle" => [0.25, 0.0, 0.0, 0.0],
            "worldTime" => [13500.0, 0.0, 0.0, 0.0],
            "cameraPosition" => [10.0, 64.5, -3.0, 0.0],
            "eyeBrightnessSmooth" => [120.0, 240.0, 0.0, 0.0],
            "gbufferModelViewInverse.3" => [7.0, 8.0, 9.0, 1.0],
            "biome" => [biome_id("desert") as f32, 0.0, 0.0, 0.0],
            "is_sneaking" => [1.0, 0.0, 0.0, 0.0],
            _ => return None,
        })
    }

    fn close(v: Val, want: f32) {
        let Val::F(f) = v else { panic!("{v:?}") };
        assert!((f - want).abs() < 1e-4, "{f} != {want}");
    }

    #[test]
    fn precedence_and_operators() {
        close(value_of("1 + 2 * 3 - 4 / 8.0"), 6.5);
        close(value_of("(1 + 2) * 3"), 9.0);
        close(value_of("-a * 2"), -5.0);
        close(value_of("7 % 3 + 7.5 % 2"), 2.5);
        // `/` divides as floats even between ints (OptiFine's docs: constants and parameters are
        // floats; Iris draws BSL Classic's `worldTime / 24000` as noon).
        close(value_of("worldTime / 1000"), 13.5);
        close(value_of("6000 / 24000"), 0.25);
        close(value_of("worldTime / 1000.0"), 13.5);
        close(value_of("if(1 < 2 && !(3 >= 4) || false, 1.5, 2.5)"), 1.5);
        close(value_of("if(1 == 2, 1, 2 != 2, 2, 3)"), 3.0);
        close(value_of("cameraPosition.y + v.z + v.b"), 70.5);
        close(value_of("gbufferModelViewInverse.3.1"), 8.0);
        close(value_of("eyeBrightnessSmooth.y / 240.0"), 1.0);
        close(value_of("if(is_sneaking, 1, 0)"), 1.0);
        close(value_of("1.5e1 + .5f"), 15.5);
    }

    #[test]
    fn functions() {
        close(value_of("sin(2.0 * pi * sunAngle)"), 1.0);
        close(
            value_of("frac(-1.25) + fmod(-1, 3) + abs(-2)"),
            0.75 + 2.0 + 2.0,
        );
        close(value_of("clamp(5, 0, 1) + min(3, 1, 2) + max(-1, -5)"), 1.0);
        close(
            value_of("pow(2, 10) + sqrt(16) + exp2(3) + log2(8)"),
            1024.0 + 4.0 + 8.0 + 3.0,
        );
        close(
            value_of("signum(-3) + floor(1.7) + ceil(1.2) + round(2.5)"),
            -1.0 + 1.0 + 2.0 + 3.0,
        );
        close(value_of("atan(1, 1) * 4"), std::f32::consts::PI);
        close(value_of("log(10, 1000)"), 3.0);
        close(value_of("mix(0, 10, 0.25) + edge(0.5, 0.4)"), 2.5);
        close(
            value_of("if(in(biome, BIOME_PLAINS, BIOME_DESERT), 1, 0)"),
            1.0,
        );
        close(value_of("if(in(biome, BIOME_PLAINS, 3), 1, 0)"), 0.0);
        close(
            value_of("if(between(a, 2.5, 3), 1, 0) + if(between(a, 2.6, 3), 1, 0)"),
            1.0,
        );
        close(value_of("if(equals(a, 2.4, 0.11), 1, 0)"), 1.0);
        close(value_of("if(CAT_NONE == 0 && PPT_SNOW == 2, 1, 0)"), 1.0);
        // A biome this version lacks matches nothing (not biome 0).
        close(
            value_of("if(in(BIOME_BADLANDS, BIOME_ICE_CAVES), 1, 0)"),
            0.0,
        );
        close(value_of("1.0;"), 1.0);
    }

    #[test]
    fn vectors_layout_and_bytes() {
        let mut c = CustomUniforms::parse(&entries(&[
            ("uniform.float.f", "1.5"),
            ("uniform.vec3.v", "vec3(1, 2, 3) * 2"),
            ("variable.vec2.hidden", "vec2(4, 5)"),
            ("uniform.int.i", "hidden.y"),
            ("uniform.bool.b", "1 < 2"),
            ("uniform.vec2.w", "abs(vec2(-1, 2))"),
            ("uniform.vec4.q", "vec4(1, 2, 3, 4)"),
        ]));
        assert!(c.notes.is_empty(), "{:?}", c.notes);
        let offsets: Vec<_> = c
            .defs
            .iter()
            .filter(|d| d.uniform)
            .map(|d| (d.name.as_str(), d.offset))
            .collect();
        assert_eq!(
            offsets,
            [
                ("f", 0),
                ("v", 16),
                ("i", 28),
                ("b", 32),
                ("w", 40),
                ("q", 48)
            ]
        );
        assert_eq!(c.size(), 64);
        let mut bytes = Vec::new();
        c.evaluate(&|_| None, 0.0, &mut bytes);
        let f = |o: usize| f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let i = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        assert_eq!([f(0), f(16), f(20), f(24)], [1.5, 2.0, 4.0, 6.0]);
        assert_eq!([i(28), i(32)], [5, 1]);
        assert_eq!([f(40), f(44), f(48), f(60)], [1.0, 2.0, 1.0, 4.0]);

        // naga's std140 layout of the generated block agrees.
        let source = format!("#version 450\n{}void main() {{}}\n", c.block());
        let module = naga::front::glsl::Frontend::default()
            .parse(
                &naga::front::glsl::Options::from(naga::ShaderStage::Vertex),
                &source,
            )
            .expect("block parses");
        let mut layouter = naga::proc::Layouter::default();
        layouter.update(module.to_ctx()).unwrap();
        let block = module.global_variables.iter().next().unwrap().1;
        let naga::TypeInner::Struct { members, .. } = &module.types[block.ty].inner else {
            panic!()
        };
        let naga_offsets: Vec<_> = members.iter().map(|m| m.offset as usize).collect();
        assert_eq!(naga_offsets, [0, 16, 28, 32, 40, 48]);
        assert_eq!(layouter[block.ty].size as usize, c.size());
    }

    #[test]
    fn smooth_over_frames() {
        let mut c = CustomUniforms::parse(&entries(&[
            ("variable.float.target", "if(worldTime > 0, 1.0, 0.0)"),
            ("uniform.float.s", "smooth(1, target, 2, 0.5)"),
            ("uniform.float.t", "smooth(target)"),
        ]));
        let time = std::cell::Cell::new(0.0f32);
        let std = |n: &str| (n == "worldTime").then(|| [time.get(), 0.0, 0.0, 0.0]);
        let mut out = Vec::new();
        let s = |c: &CustomUniforms| match c.value("s").unwrap() {
            Val::F(f) => f,
            v => panic!("{v:?}"),
        };
        // The first frame starts at the value itself.
        c.evaluate(&std, 0.1, &mut out);
        assert_eq!(s(&c), 0.0);
        time.set(1.0);
        // Up with a 2 s half-life: half way after 2 s, three quarters after 4 s.
        c.evaluate(&std, 2.0, &mut out);
        assert!((s(&c) - 0.5).abs() < 1e-5);
        c.evaluate(&std, 2.0, &mut out);
        assert!((s(&c) - 0.75).abs() < 1e-5);
        // `t` (default 1 s) is at 1 - 0.5^4 after the same 4 s.
        assert!(matches!(c.value("t"), Some(Val::F(f)) if (f - 0.9375).abs() < 1e-5));
        // Down with 0.5 s: halves every half second.
        time.set(0.0);
        c.evaluate(&std, 0.5, &mut out);
        assert!((s(&c) - 0.375).abs() < 1e-5);
    }

    #[test]
    fn redefinitions_and_errors_read_zero() {
        let mut c = CustomUniforms::parse(&entries(&[
            ("variable.float.k", "1"),
            ("uniform.float.a", "k * 3"),
            ("variable.float.k", "2"),
            ("uniform.vec3.bad_type", "1.0"),
            ("uniform.float.bad_vec", "vec2(1, 2)"),
            ("uniform.float.syntax", "1 +"),
            ("uniform.float.func", "nope(1)"),
            ("uniform.float.unknown", "mystery + 1"),
            ("uniform.mat4.m", "1"),
            ("uniform.float.sunAngle", "1"),
        ]));
        // `a` was parsed before `k` moved: it reads the new `k` (last
        // definition wins) once evaluated in order… which is after `a`, so the
        // first frame sees 0, the next 2 * 3.
        let mut out = Vec::new();
        c.evaluate(&|_| None, 0.0, &mut out);
        c.evaluate(&|_| None, 0.0, &mut out);
        assert_eq!(c.value("a"), Some(Val::F(6.0)));
        assert_eq!(c.value("unknown"), Some(Val::F(1.0)));
        for name in ["bad_type", "bad_vec", "syntax", "func"] {
            assert!(
                c.notes.iter().any(|n| n.contains(&format!(".{name}:"))),
                "{name}: {:?}",
                c.notes
            );
        }
        assert_eq!(c.value("bad_type"), Some(Val::V(3, [0.0; 4])));
        assert!(c.notes.iter().any(|n| n.contains("mystery is unknown")));
        assert!(c.notes.iter().any(|n| n.contains("unknown type mat4")));
        assert!(c.notes.iter().any(|n| n.contains("standard uniform")));
        assert!(c.uniform("sunAngle").is_none());
        // Notes are not repeated frame after frame.
        let count = c.notes.len();
        c.evaluate(&|_| None, 0.0, &mut out);
        assert_eq!(c.notes.len(), count);
    }

    #[test]
    fn random_and_biomes() {
        let mut c = CustomUniforms::parse(&entries(&[
            ("uniform.float.r", "random()"),
            ("uniform.float.r2", "random(5, 6)"),
        ]));
        let mut out = Vec::new();
        c.evaluate(&|_| None, 0.0, &mut out);
        let first = c.value("r");
        c.evaluate(&|_| None, 0.0, &mut out);
        assert_ne!(first, c.value("r"));
        assert!(matches!(c.value("r2"), Some(Val::F(f)) if (5.0..6.0).contains(&f)));
        assert_eq!(biome_id("minecraft:badlands"), 0);
        assert_eq!(biome_id("wooded_badlands"), BIOMES.len() as i32 - 1);
        assert_eq!(biome_id("mod:x"), -1);
        let cat = |b| CATEGORIES[biome_category(b) as usize];
        assert_eq!(
            [
                cat("frozen_ocean"),
                cat("snowy_taiga"),
                cat("ice_spikes"),
                cat("crimson_forest"),
                cat("end_highlands"),
                cat("meadow"),
                cat("dark_forest")
            ],
            [
                "OCEAN", "TAIGA", "ICY", "NETHER", "THE_END", "PLAINS", "FOREST"
            ]
        );
    }
}
