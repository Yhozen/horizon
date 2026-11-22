//! The frame's render-target schedule, worked out once from a [`PackPlan`]:
//! which buffers are cleared, which side of each `colortexN` every pass reads
//! and writes, where mipmaps are regenerated, and what is copied back at the
//! end of the frame. Pure data; the client turns the steps into wgpu passes.
//!
//! Every `colortexN` has two sides, [`Side::Main`] and [`Side::Alt`]. The
//! rules (format notes §5):
//!
//! - Opaque gbuffers always write the main side.
//! - A full-screen pass (deferred, composite) reads every buffer from the
//!   side it is on before the pass, writes the *other* side of the buffers in
//!   its draw buffers, and those buffers then flip. So a pass that reads a
//!   buffer it also writes sees its previous contents.
//! - `final` reads the latest side and writes the screen.
//! - At the end of the frame, a buffer that ended on its alternate side and
//!   is not cleared every frame is copied back to its main side, so the next
//!   frame's gbuffers (and passes) find the last result there.
//! - Buffers with `Clear` on are cleared (both sides) every frame; the others
//!   only when the targets are created (first frame, resize):
//!   [`FrameSchedule::initial_clears`].
//! - `flip.<program>.<buffer>=false` keeps a buffer on its side after the
//!   pass wrote the other one (the next pass reads the old contents again);
//!   `=true` flips a buffer the pass does not write.
//! - `prepare*` passes run after the shadow map and before the opaque
//!   gbuffers, which then draw into the side each buffer is on.

use std::collections::BTreeSet;

use crate::ProgramKind;
use crate::plan::{BufferSize, PackPlan, Scale};

/// Number of `colortexN` buffers.
pub const BUFFERS: usize = 16;

/// One of the two textures behind a `colortexN`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Side {
    #[default]
    Main,
    Alt,
}

impl Side {
    /// The side a pass writes when the buffer is currently on `self`.
    pub fn other(self) -> Self {
        match self {
            Self::Main => Self::Alt,
            Self::Alt => Self::Main,
        }
    }
}

/// The colour a buffer is cleared to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClearColor {
    /// The frame's fog colour with alpha 1 (colortex0's default); the client
    /// fills it in each frame.
    Fog,
    Rgba([f32; 4]),
}

/// Clear both sides of `buffer` to `color`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BufferClear {
    pub buffer: u8,
    pub color: ClearColor,
}

/// Which gbuffers programs a [`Step::Gbuffers`] draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GbufferPhase {
    /// Sky, opaque and cutout terrain, entities, block entities, opaque
    /// particles, the solid hand (everything before `deferred`). It draws
    /// into the sides the `prepare` passes left (main without them).
    Opaque,
    /// Translucent terrain and particles, weather, the translucent hand
    /// (everything after `deferred`, before `composite`).
    Translucent,
}

/// A full-screen pass (`deferred*`, `composite*`, `final`).
#[derive(Clone, Debug, PartialEq)]
pub struct Pass {
    pub kind: ProgramKind,
    /// The side of each `colortexN` the pass samples (index = N).
    pub reads: [Side; BUFFERS],
    /// `(buffer, side)` for each of the pass's draw buffers, in
    /// `gl_FragData` order (`DRAWBUFFERS:46` → `[(4, _), (6, _)]`). Always
    /// the side opposite to `reads[buffer]`. Empty for `final` (it draws to
    /// the screen).
    pub writes: Vec<(u8, Side)>,
    /// Regenerate the mip chain of these `(buffer, side)` before the pass
    /// runs (the side is the one the pass reads). From then on the buffer is
    /// sampled trilinearly for the rest of the frame.
    pub regenerate_mips: Vec<(u8, Side)>,
    /// The size of the targets it draws into (its first draw buffer's
    /// `size.buffer`; the screen's for `final`).
    pub size: BufferSize,
    /// `scale.<program>`: the viewport inside those targets.
    pub scale: Option<Scale>,
}

/// One step of a frame, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Clear both sides of these buffers (the ones with `Clear` on that some
    /// program writes).
    Clear(Vec<BufferClear>),
    /// The shadow map (present when the pack has a `shadow` program).
    Shadow,
    /// Geometry programs. `sides[N]` is the side of `colortexN` they draw
    /// into (and the one a gbuffers program samples for N >= 4). The opaque
    /// phase is all [`Side::Main`]; the translucent phase draws into the
    /// side each buffer is on after the deferred passes, so translucent
    /// geometry lands on top of the deferred result.
    Gbuffers {
        phase: GbufferPhase,
        sides: [Side; BUFFERS],
    },
    Pass(Pass),
    /// A `shadowcomp` pass, right after [`Step::Shadow`]: it draws into the
    /// `shadowcolorN` in `writes` (one texture each, no sides). The client
    /// copies each before the pass and binds the copy for its reads, so the
    /// pass reads what the shadow map held, as Iris's flipped buffers give.
    ShadowComp {
        kind: ProgramKind,
        writes: Vec<u8>,
    },
    /// Copy these buffers' alternate side into their main side (end of frame).
    CopyBack(Vec<u8>),
}

/// The whole frame for one plan. Rebuild it when the plan changes (it does
/// not depend on the window size).
#[derive(Clone, Debug, PartialEq)]
pub struct FrameSchedule {
    /// Run in order every frame.
    pub steps: Vec<Step>,
    /// Clear both sides of every buffer to these when the targets are
    /// (re)created: on the first frame and after a resize, before `steps`.
    pub initial_clears: Vec<BufferClear>,
    /// The side of each buffer once the last pass ran (before copy-back).
    /// Without a `final` program, the client presents `colortex0` from
    /// `latest[0]`.
    pub latest: [Side; BUFFERS],
    /// Buffers some program writes (gbuffers or full-screen; shadow programs
    /// write shadowcolor, not colortex). Others are only ever cleared.
    pub written: BTreeSet<u8>,
}

/// The colour `buffer` is cleared to under `plan`.
pub fn clear_color(plan: &PackPlan, buffer: u8) -> ClearColor {
    match plan.buffers[usize::from(buffer)].clear_color {
        Some(c) => ClearColor::Rgba(c),
        None => match buffer {
            0 => ClearColor::Fog,
            1 => ClearColor::Rgba([1.0; 4]),
            _ => ClearColor::Rgba([0.0; 4]),
        },
    }
}

/// The colortex buffers `kind` draws into, deduplicated, in `gl_FragData`
/// order. Programs without a draw-buffer entry write colortex0. Indices
/// outside 0..16 are dropped.
fn draw_buffers(plan: &PackPlan, kind: ProgramKind) -> Vec<u8> {
    let mut out = Vec::new();
    match plan.draw_buffers.get(&kind) {
        Some(list) => {
            for &b in list {
                if usize::from(b) < BUFFERS && !out.contains(&b) {
                    out.push(b);
                }
            }
        }
        None => out.push(0),
    }
    out
}

impl FrameSchedule {
    /// The schedule of `plan`'s programs (`plan.programs`, `draw_buffers`,
    /// `buffers`, `mipmaps`).
    pub fn new(plan: &PackPlan) -> Self {
        let mut written = BTreeSet::new();
        for &kind in plan.programs.keys() {
            if kind.writes_shadowcolor() || kind == ProgramKind::Final {
                continue;
            }
            written.extend(draw_buffers(plan, kind));
        }
        let clear = |b: u8| BufferClear {
            buffer: b,
            color: clear_color(plan, b),
        };
        let per_frame: Vec<BufferClear> = written
            .iter()
            .copied()
            .filter(|&b| plan.buffers[usize::from(b)].clear)
            .map(clear)
            .collect();
        let initial_clears = (0..BUFFERS as u8).map(clear).collect();

        let mut steps = Vec::new();
        if !per_frame.is_empty() {
            steps.push(Step::Clear(per_frame));
        }
        if plan.programs.contains_key(&ProgramKind::Shadow)
            || plan.programs.contains_key(&ProgramKind::ShadowEntities)
        {
            steps.push(Step::Shadow);
            for &kind in plan.programs.keys() {
                if let ProgramKind::ShadowComp(_) = kind {
                    let mut writes = plan.draw_buffers.get(&kind).cloned().unwrap_or(vec![0]);
                    writes.retain(|&b| b < 2);
                    writes.dedup();
                    steps.push(Step::ShadowComp { kind, writes });
                }
            }
        }
        let mut sides = [Side::Main; BUFFERS];
        let mut passes = plan.fullscreen_passes();
        passes.retain(|k| !k.writes_shadowcolor());
        let (prepare, passes): (Vec<_>, Vec<_>) = passes
            .iter()
            .partition(|k| matches!(k, ProgramKind::Prepare(_)));
        let (deferred, rest): (Vec<_>, Vec<_>) = passes
            .into_iter()
            .partition(|k| matches!(k, ProgramKind::Deferred(_)));
        for kind in prepare {
            steps.push(Step::Pass(pass(plan, kind, &mut sides)));
        }
        steps.push(Step::Gbuffers {
            phase: GbufferPhase::Opaque,
            sides,
        });
        for kind in deferred {
            steps.push(Step::Pass(pass(plan, kind, &mut sides)));
        }
        steps.push(Step::Gbuffers {
            phase: GbufferPhase::Translucent,
            sides,
        });
        for kind in rest {
            steps.push(Step::Pass(pass(plan, kind, &mut sides)));
        }
        let copy_back: Vec<u8> = (0..BUFFERS as u8)
            .filter(|&b| sides[usize::from(b)] == Side::Alt && !plan.buffers[usize::from(b)].clear)
            .collect();
        if !copy_back.is_empty() {
            steps.push(Step::CopyBack(copy_back));
        }
        Self {
            steps,
            initial_clears,
            latest: sides,
            written,
        }
    }

    /// The full-screen passes, in order.
    pub fn passes(&self) -> impl Iterator<Item = &Pass> {
        self.steps.iter().filter_map(|s| match s {
            Step::Pass(p) => Some(p),
            _ => None,
        })
    }

    /// The buffers copied back at the end of the frame.
    pub fn copy_back(&self) -> &[u8] {
        self.steps
            .iter()
            .find_map(|s| match s {
                Step::CopyBack(b) => Some(b.as_slice()),
                _ => None,
            })
            .unwrap_or(&[])
    }
}

/// For each `colortexN`, whether the full-screen pass `kind` samples it with
/// mipmaps: only when the side it reads had its mip chain rebuilt earlier in
/// the frame (or before `kind` itself). A pass that writes a mipmapped buffer
/// flips it, and the side written has no fresh mips: the next pass reads it
/// at level 0. (Fantasy's composite6 mipmaps colortex3 and writes it; its
/// FXAA in composite7 then read stale levels where implicit derivatives
/// were large, which left black dots.)
pub fn mipmapped_reads(plan: &PackPlan, kind: ProgramKind) -> [bool; BUFFERS] {
    let mut kinds: Vec<ProgramKind> = plan
        .programs
        .keys()
        .chain(plan.draw_buffers.keys())
        .chain(plan.mipmaps.keys())
        .copied()
        .filter(|k| k.is_fullscreen() && !k.writes_shadowcolor() && *k < kind)
        .collect();
    kinds.sort();
    kinds.dedup();
    let mut sides = [Side::Main; BUFFERS];
    let mut fresh = BTreeSet::new();
    for k in kinds {
        fresh.extend(pass(plan, k, &mut sides).regenerate_mips);
    }
    for &b in plan.mipmaps.get(&kind).into_iter().flatten() {
        if usize::from(b) < BUFFERS {
            fresh.insert((b, sides[usize::from(b)]));
        }
    }
    std::array::from_fn(|b| fresh.contains(&(b as u8, sides[b])))
}

/// `kind` run on `sides`, which it then flips.
fn pass(plan: &PackPlan, kind: ProgramKind, sides: &mut [Side; BUFFERS]) -> Pass {
    let reads = *sides;
    let regenerate_mips = plan
        .mipmaps
        .get(&kind)
        .into_iter()
        .flatten()
        .filter(|&&b| usize::from(b) < BUFFERS)
        .map(|&b| (b, reads[usize::from(b)]))
        .collect();
    let writes: Vec<(u8, Side)> = if kind == ProgramKind::Final {
        Vec::new()
    } else {
        draw_buffers(plan, kind)
            .into_iter()
            .map(|b| (b, reads[usize::from(b)].other()))
            .collect()
    };
    let state = plan.state(kind);
    for &(b, side) in &writes {
        if state.flip.get(&b) != Some(&false) {
            sides[usize::from(b)] = side;
        }
    }
    for (&b, &flip) in &state.flip {
        let written = writes.iter().any(|w| w.0 == b);
        if flip && !written && usize::from(b) < BUFFERS {
            let s = &mut sides[usize::from(b)];
            *s = s.other();
        }
    }
    let size = writes.first().map_or(BufferSize::Screen, |&(b, _)| {
        plan.buffers[usize::from(b)].size
    });
    Pass {
        kind,
        reads,
        writes,
        regenerate_mips,
        size,
        scale: state.scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use Side::{Alt, Main};

    fn plan(programs: &[(ProgramKind, &[u8])]) -> PackPlan {
        let mut plan = PackPlan::default();
        for &(kind, buffers) in programs {
            plan.programs
                .insert(kind, format!("shaders/{}", kind.candidates()[0]));
            plan.draw_buffers.insert(kind, buffers.to_vec());
        }
        plan
    }

    fn sides(alt: &[u8]) -> [Side; BUFFERS] {
        let mut s = [Main; BUFFERS];
        for &b in alt {
            s[usize::from(b)] = Alt;
        }
        s
    }

    #[test]
    fn three_passes_with_a_buffer_written_twice() {
        let mut p = plan(&[
            (ProgramKind::Terrain, &[0, 1, 2]),
            (ProgramKind::Composite(0), &[3, 4]),
            (ProgramKind::Composite(1), &[4]),
            (ProgramKind::Final, &[]),
        ]);
        p.buffers[4].clear = false;
        p.mipmaps = BTreeMap::from([(ProgramKind::Composite(1), BTreeSet::from([4, 0]))]);
        let s = FrameSchedule::new(&p);

        let Step::Clear(clears) = &s.steps[0] else {
            panic!("{:?}", s.steps[0])
        };
        let cleared: Vec<u8> = clears.iter().map(|c| c.buffer).collect();
        assert_eq!(cleared, [0, 1, 2, 3]);
        assert_eq!(clears[0].color, ClearColor::Fog);
        assert_eq!(clears[1].color, ClearColor::Rgba([1.0; 4]));
        assert_eq!(clears[2].color, ClearColor::Rgba([0.0; 4]));

        let passes: Vec<&Pass> = s.passes().collect();
        assert_eq!(passes.len(), 3);
        assert_eq!(passes[0].reads, sides(&[]));
        assert_eq!(passes[0].writes, [(3, Alt), (4, Alt)]);
        // The second pass reads colortex4 as the first left it and writes
        // the other side: back to main.
        assert_eq!(passes[1].reads, sides(&[3, 4]));
        assert_eq!(passes[1].writes, [(4, Main)]);
        assert_eq!(passes[1].regenerate_mips, [(0, Main), (4, Alt)]);
        assert_eq!(passes[2].kind, ProgramKind::Final);
        assert_eq!(passes[2].reads, sides(&[3]));
        assert!(passes[2].writes.is_empty());

        // colortex3 ends on alt but is cleared every frame: no copy; 4 ends on main.
        assert!(s.copy_back().is_empty());
        assert_eq!(s.latest, sides(&[3]));
        assert_eq!(s.initial_clears.len(), BUFFERS);
        assert_eq!(s.written, BTreeSet::from([0, 1, 2, 3, 4]));
    }

    #[test]
    fn uncleared_buffer_left_on_alt_is_copied_back() {
        let mut p = plan(&[
            (ProgramKind::Terrain, &[0]),
            (ProgramKind::Composite(0), &[7, 0]),
        ]);
        p.buffers[7].clear = false;
        let s = FrameSchedule::new(&p);
        assert_eq!(s.copy_back(), [7]);
        assert!(matches!(s.steps.last(), Some(Step::CopyBack(_))));
        // colortex7 is never cleared per frame, only when targets are made.
        let Step::Clear(clears) = &s.steps[0] else {
            panic!()
        };
        assert!(clears.iter().all(|c| c.buffer != 7));
        assert!(s.initial_clears.iter().any(|c| c.buffer == 7));
    }

    #[test]
    fn deferred_runs_between_the_gbuffer_phases() {
        let p = plan(&[
            (ProgramKind::Shadow, &[0, 1]),
            // shadowcomp writes shadowcolors (5 is not one), right after the shadow map.
            (ProgramKind::ShadowComp(1), &[1, 5]),
            (ProgramKind::ShadowComp(0), &[0]),
            (ProgramKind::Terrain, &[0, 2]),
            (ProgramKind::Deferred(0), &[0]),
            (ProgramKind::Composite(0), &[0]),
        ]);
        let s = FrameSchedule::new(&p);
        let names: Vec<String> = s
            .steps
            .iter()
            .map(|st| match st {
                Step::Clear(_) => "clear".into(),
                Step::Shadow => "shadow".into(),
                Step::Gbuffers { phase, .. } => format!("{phase:?}"),
                Step::Pass(p) => format!("{:?}", p.kind),
                Step::ShadowComp { kind, writes } => format!("{kind:?} {writes:?}"),
                Step::CopyBack(_) => "copy".into(),
            })
            .collect();
        assert_eq!(
            names,
            [
                "clear",
                "shadow",
                "ShadowComp(0) [0]",
                "ShadowComp(1) [1]",
                "Opaque",
                "Deferred(0)",
                "Translucent",
                "Composite(0)"
            ]
        );
        let Step::Gbuffers { sides: after, .. } = &s.steps[6] else {
            panic!()
        };
        assert_eq!(*after, sides(&[0]));
        // Shadow and shadowcomp draw buffers are shadowcolors, not colortex.
        assert_eq!(s.written, BTreeSet::from([0, 2]));
        // Two flips of colortex0: back on main.
        assert_eq!(s.latest, sides(&[]));
    }

    #[test]
    fn prepare_flips_and_sizes() {
        use crate::plan::{ProgramState, Scale};
        let mut p = plan(&[
            (ProgramKind::Prepare(0), &[5]),
            (ProgramKind::Terrain, &[0]),
            (ProgramKind::Composite(0), &[3, 4]),
            (ProgramKind::Composite(1), &[3]),
            (ProgramKind::Final, &[]),
        ]);
        p.buffers[3].size = BufferSize::Relative(0.5, 0.5);
        p.program_states.insert(
            "composite".into(),
            ProgramState {
                flip: [(4, false), (6, true)].into(),
                scale: Some(Scale {
                    scale: 0.5,
                    offset: [0.0; 2],
                }),
                ..ProgramState::default()
            },
        );
        let s = FrameSchedule::new(&p);
        let kinds: Vec<String> = s
            .steps
            .iter()
            .map(|st| match st {
                Step::Gbuffers { phase, sides } => format!("{phase:?}{:?}", sides[5]),
                Step::Pass(p) => format!("{:?}", p.kind),
                _ => String::new(),
            })
            .filter(|n| !n.is_empty())
            .collect();
        // The prepare pass runs first and the opaque gbuffers draw on its side.
        assert_eq!(
            kinds,
            [
                "Prepare(0)",
                "OpaqueAlt",
                "TranslucentAlt",
                "Composite(0)",
                "Composite(1)",
                "Final"
            ]
        );
        let passes: Vec<&Pass> = s.passes().collect();
        assert_eq!(passes[1].writes, [(3, Alt), (4, Alt)]);
        assert_eq!(passes[1].size, BufferSize::Relative(0.5, 0.5));
        assert_eq!(passes[1].scale.map(|s| s.scale), Some(0.5));
        // colortex4 did not flip, colortex6 did without being written.
        assert_eq!(passes[2].reads, sides(&[3, 5, 6]));
        assert_eq!(passes[3].reads, sides(&[5, 6]));
        assert_eq!(passes[3].size, BufferSize::Screen);
    }

    #[test]
    fn defaults_and_custom_clear_colours() {
        let mut p = plan(&[(ProgramKind::Terrain, &[0, 0, 1, 20])]);
        p.draw_buffers.remove(&ProgramKind::Terrain);
        p.programs.insert(ProgramKind::Composite(2), "c".into());
        p.draw_buffers
            .insert(ProgramKind::Composite(2), vec![5, 5, 20]);
        p.buffers[0].clear_color = Some([0.5, 0.5, 0.5, 1.0]);
        let s = FrameSchedule::new(&p);
        // Terrain without directive: colortex0; duplicates and bad indices dropped.
        assert_eq!(s.written, BTreeSet::from([0, 5]));
        assert_eq!(s.passes().next().unwrap().writes, [(5, Alt)]);
        assert_eq!(
            s.initial_clears[0].color,
            ClearColor::Rgba([0.5, 0.5, 0.5, 1.0])
        );
    }
}
