//! Generated GLSL 450 around a rewritten program: inputs from Horizon's
//! vertex data, bindings, sampling functions, outputs.
//! Contract: .context/glsl-pack-spec.md, section 4.
//!
//! Each stage is: the uniform block, the textures it samples, its
//! inter-stage interface, the plain globals the pack's body reads and writes
//! (varyings, attributes, `hz_gl_*` built-ins), the sampling functions, the
//! body, and a host `main` that fills the globals, calls `hz_pack_main` and
//! hands the results on.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::rewrite::{Rewritten, Var};
use crate::uniforms::{UNIFORM_BLOCK, uniform_type};
use crate::{BlockIds, HostParams, PackPlan, ProgramKind, Stage, TextureBinding};

mod builtins;
mod fragment;
mod sampling;
#[cfg(test)]
mod tests;
mod varyings;
mod vertex;

use builtins::*;
use fragment::fragment_source;
pub use fragment::variant;
use sampling::*;
pub use sampling::{
    COMPARE_BINDING, CUSTOM_BINDING, MIPS_BINDING, SAMPLING_OPS, SamplingCall, parse_sampling_call,
    sampler_binding, sampler_type, sampling_call, split_sampling_call,
};
use varyings::*;
use vertex::vertex_source;

/// What a program draws, which decides its vertex inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Geometry {
    /// Horizon's `TerrainVertex` plus the per-section origin instance.
    Terrain,
    /// Horizon's entity `Vertex` (`mesher.rs`): entities, block entities,
    /// particles, the hand, the sky and the sun and moon.
    Entity,
    /// A full-screen quad from `gl_VertexIndex` (no buffers).
    Fullscreen,
}

/// Everything [`sources`] needs besides the two rewritten stages.
pub struct ProgramSpec<'a> {
    pub kind: ProgramKind,
    pub geometry: Geometry,
    /// Colortex indices, one per fragment output location.
    pub draw_buffers: &'a [u8],
    pub host: &'a HostParams,
    pub blocks: &'a BlockIds,
    /// Buffer formats, mipmaps, shadow settings and custom textures.
    pub plan: &'a PackPlan,
}

/// A program's two generated stages and what they bind.
#[derive(Clone, Debug, Default)]
pub struct HostOutput {
    pub vertex: String,
    pub fragment: String,
    /// Group 1 textures of both stages, by binding.
    pub textures: Vec<TextureBinding>,
    /// Uniforms the pack declares that Horizon's block lacks: zero-valued globals.
    pub unknown_uniforms: BTreeSet<String>,
    /// Report lines about what may not look as in Iris (samplers with no texture…).
    pub notes: Vec<String>,
}

/// The uniform blocks. For entity geometry reading `entityColor`, its member
/// is renamed so a per-vertex `entityColor` global can take the name. A
/// uniform the pack declares with another type than the block's (`uniform
/// float worldTime`, `uniform bool is_sneaking`) reads the renamed member
/// through a converting `#define`. Custom uniforms the program declares come
/// from the second block (group 1, binding 1), by `#define` as well.
fn uniform_block(extras: &Extras) -> String {
    let mut out = if extras.entity_color {
        UNIFORM_BLOCK.replace(" entityColor;", " hz_block_entityColor;")
    } else {
        UNIFORM_BLOCK.to_string()
    };
    if extras.entity {
        // Entity geometry samples the shared atlas: its size is the atlas's.
        out = out
            .replace(" atlasSize;", " hz_terrain_atlasSize;")
            .replace(" gtextureSize;", " hz_terrain_gtextureSize;");
        out += "#define atlasSize hz_entityAtlasSize\n#define gtextureSize hz_entityAtlasSize\n";
    }
    for (name, ty) in &extras.converted {
        if extras.entity && matches!(name.as_str(), "atlasSize" | "gtextureSize") {
            writeln!(
                out,
                "#undef {name}\n#define {name} {ty}(hz_entityAtlasSize)"
            )
            .unwrap();
            continue;
        }
        out = out.replace(&format!(" {name};\n"), &format!(" hz_b_{name};\n"));
        writeln!(out, "#define {name} {ty}(hz_b_{name})").unwrap();
    }
    if let Some(block) = &extras.custom_block {
        out += block;
        for (name, from, to) in &extras.customs {
            if from == to {
                writeln!(out, "#define {name} hz_c_{name}").unwrap();
            } else {
                writeln!(out, "#define {name} {to}(hz_c_{name})").unwrap();
            }
        }
    }
    out
}

/// Zero-valued globals for the uniforms nothing provides (private globals
/// start zeroed in WGSL). `program_sources` leaves only those in the stage's
/// list.
fn declare_unknown_uniforms(out: &mut String, stage: &Rewritten) {
    for u in &stage.uniforms {
        writeln!(out, "{} {};", u.ty, u.name).unwrap();
    }
}

/// The shape of a GLSL type for conversions: 1 for scalars, N for vectors,
/// `None` for anything else (matrices, arrays).
fn shape(ty: &str) -> Option<u8> {
    match ty {
        "float" | "int" | "uint" | "bool" => Some(1),
        _ => {
            let n = ty.strip_prefix(['i', 'u', 'b']).unwrap_or(ty);
            let n = n.strip_prefix("vec")?.parse().ok()?;
            (2..=4).contains(&n).then_some(n)
        }
    }
}

/// Whether a value of GLSL type `from` converts to `to` by a constructor.
fn converts(from: &str, to: &str) -> bool {
    shape(from).is_some() && shape(from) == shape(to)
}

/// How each uniform the program declares is provided.
#[derive(Default)]
struct UniformPlan {
    /// The block member under another type: name → the pack's type.
    converted: BTreeMap<String, String>,
    /// Custom uniforms: (name, block type, the pack's type).
    customs: Vec<(String, String, String)>,
    /// Nothing provides them: zero-valued globals.
    unknown: BTreeSet<String>,
}

impl UniformPlan {
    fn new(vertex: &Rewritten, fragment: &Rewritten, spec: &ProgramSpec) -> Result<Self, String> {
        let mut plan = Self::default();
        let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
        for u in vertex.uniforms.iter().chain(&fragment.uniforms) {
            match seen.insert(&u.name, &u.ty) {
                Some(ty) if ty != u.ty => {
                    return Err(format!(
                        "uniform {} is {ty} in one stage and {} in the other",
                        u.name, u.ty
                    ));
                }
                Some(_) => continue,
                None => {}
            }
            let custom = spec.plan.custom_uniforms.uniform(&u.name);
            match (uniform_type(&u.name), custom) {
                (Some(ty), _) if ty == u.ty => {}
                (Some(ty), _) if converts(ty, &u.ty) => {
                    plan.converted.insert(u.name.clone(), u.ty.clone());
                }
                (Some(ty), _) => {
                    return Err(format!(
                        "uniform {} is {} in the pack but {ty} in Horizon's",
                        u.name, u.ty
                    ));
                }
                (None, Some(c)) if converts(c.ty.block_glsl(), &u.ty) => {
                    plan.customs
                        .push((u.name.clone(), c.ty.block_glsl().into(), u.ty.clone()));
                }
                (None, _) => {
                    plan.unknown.insert(u.name.clone());
                }
            }
        }
        Ok(plan)
    }

    /// `stage` with only the uniforms nothing provides left in its list.
    fn unknown_only(&self, stage: &Rewritten) -> Rewritten {
        let mut stage = stage.clone();
        stage.uniforms.retain(|u| self.unknown.contains(&u.name));
        stage
    }
}

/// Per-program host additions that depend on what both stages use.
pub(super) struct Extras {
    /// Entity geometry reading `entityColor`: a flat per-vertex value
    /// replaces the block member.
    pub(super) entity_color: bool,
    /// Entity geometry whose atlas UVs Horizon wraps per fragment (beacon
    /// beams, breaking cracks; entity.wgsl's `wrap`): a flat `vec4`.
    pub(super) uv_wrap: bool,
    /// The program declares `watershadow` (then `shadow` is shadowtex1).
    pub(super) watershadow: bool,
    /// Entity geometry (its atlas is the shared one, not the sprite arrays).
    entity: bool,
    /// Block members the pack declares with another type: name → its type.
    converted: BTreeMap<String, String>,
    /// The custom uniform block, when the program declares one of them.
    custom_block: Option<String>,
    /// The custom uniforms it declares: (name, block type, the pack's type).
    customs: Vec<(String, String, String)>,
}

impl Extras {
    fn new(vertex: &Rewritten, fragment: &Rewritten, spec: &ProgramSpec) -> Self {
        let entity = spec.geometry == Geometry::Entity;
        let uses = |name: &str| {
            vertex
                .uniforms
                .iter()
                .chain(&fragment.uniforms)
                .any(|u| u.name == name)
        };
        let atlas = fragment.sample_calls.iter().any(|c| {
            split_sampling_call(c).is_some_and(|(_, name)| {
                // `gcolor` is the atlas in gbuffers programs (sampling::resolve).
                name == "gcolor" || canonical(name, false).is_some_and(|n| n == "gtexture")
            })
        });
        Self {
            entity_color: entity && uses("entityColor"),
            uv_wrap: entity && atlas && variant(spec.kind).1,
            watershadow: vertex
                .samplers
                .iter()
                .chain(&fragment.samplers)
                .any(|s| s == "watershadow"),
            entity,
            converted: BTreeMap::new(),
            custom_block: None,
            customs: Vec::new(),
        }
    }

    /// Inter-stage locations the extras take after the pack's.
    fn locations(&self) -> u32 {
        u32::from(self.entity_color) + u32::from(self.uv_wrap)
    }
}

/// Built-ins GLSL 120 itself carries from the vertex to the fragment stage.
const BUILTIN_VARYINGS: [(&str, &str); 2] =
    [("hz_gl_Color", "vec4"), ("hz_gl_FogFragCoord", "float")];

/// The vertex and fragment GLSL 450 sources of a program.
pub fn sources(
    vertex: &Rewritten,
    fragment: &Rewritten,
    spec: &ProgramSpec,
) -> Result<(String, String), String> {
    let out = program_sources(vertex, fragment, spec)?;
    Ok((out.vertex, out.fragment))
}

/// The vertex and fragment GLSL 450 sources of a program, with its textures.
pub fn program_sources(
    vertex: &Rewritten,
    fragment: &Rewritten,
    spec: &ProgramSpec,
) -> Result<HostOutput, String> {
    let uniforms = UniformPlan::new(vertex, fragment, spec)?;
    if spec.draw_buffers.len() > 8 {
        return Err(format!(
            "{} draw buffers, at most 8",
            spec.draw_buffers.len()
        ));
    }
    if !(1..=8).contains(&spec.host.sprite_arrays)
        || spec.host.sprite_grid == 0
        || spec.host.sprite_split == 0
    {
        return Err(format!("unsupported host parameters {:?}", spec.host));
    }
    let mut extras = Extras::new(vertex, fragment, spec);
    extras.converted = uniforms.converted.clone();
    if !uniforms.customs.is_empty() {
        extras.custom_block = Some(spec.plan.custom_uniforms.block());
        extras.customs = uniforms.customs.clone();
    }
    let fragment = &uniforms.unknown_only(fragment);
    let mut vertex = uniforms.unknown_only(vertex);
    let adopted = varyings::adopt_undeclared(&mut vertex, fragment);
    let vertex = &vertex;
    // Entity extras ride after the pack's varyings: keep room for them.
    let max = spec.host.max_varyings.saturating_sub(extras.locations());
    let shared = Interface::new(vertex, fragment, spec.geometry, max)?;
    let (vs, vertex_textures) = vertex_source(vertex, fragment, &shared, spec, &extras)?;
    let (fs, fragment_textures) = fragment_source(fragment, &shared, spec, &extras)?;
    let mut textures: BTreeMap<u32, TextureBinding> = BTreeMap::new();
    for t in vertex_textures.into_iter().chain(fragment_textures) {
        match textures.get_mut(&t.binding) {
            Some(old) if old.source != t.source || old.sampler != t.sampler => {
                return Err(format!(
                    "binding {} is {:?} in one stage and {:?} in the other",
                    t.binding, old.source, t.source
                ));
            }
            Some(old) => {
                old.vertex |= t.vertex;
                old.fragment |= t.fragment;
            }
            None => {
                textures.insert(t.binding, t);
            }
        }
    }
    let mut notes = sampling_notes(&vertex.sample_calls, spec, extras.watershadow);
    notes.extend(adopted);
    notes.extend(sampling_notes(
        &fragment.sample_calls,
        spec,
        extras.watershadow,
    ));
    Ok(HostOutput {
        vertex: vs,
        fragment: fs,
        textures: textures.into_values().collect(),
        unknown_uniforms: uniforms.unknown,
        notes: notes.into_iter().collect(),
    })
}
