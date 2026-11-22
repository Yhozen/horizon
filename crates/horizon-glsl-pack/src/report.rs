//! What happened when a pack was read and compiled, for the player and for
//! whoever fixes the next gap: the pack's status, what it uses that Horizon
//! lacks, and every program that did not make it, with the reason.

use std::fmt::{self, Write as _};

/// How well a pack runs, from the player's side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum PackStatus {
    /// Every program the pack enables compiled.
    #[default]
    Works,
    /// It runs, with programs skipped or features missing (see the report).
    Partial,
    /// It needs something Horizon does not have; [`PackReport::refusal`] says what.
    Refused,
    /// It could not be compiled (an essential program failed).
    Failed,
}

impl PackStatus {
    /// A short word for lists.
    pub fn label(self) -> &'static str {
        match self {
            Self::Works => "Works",
            Self::Partial => "Partial",
            Self::Refused => "Not supported",
            Self::Failed => "Failed",
        }
    }
}

/// A feature of the format a pack uses, and whether Horizon has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feature {
    /// What it is, in a few words (`compute shaders`, `custom images`, …).
    pub name: String,
    /// Whether Horizon supports it.
    pub supported: bool,
    /// Whether the pack cannot run without it (`iris.features.required`, or
    /// the only path it has).
    pub required: bool,
    /// Where it was seen (`shaders/world0/composite.csh`, a property key, …).
    pub detail: String,
}

/// One program's outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramOutcome {
    /// The slot (`gbuffers_terrain`, `composite3`; see `ProgramKind::name`).
    pub slot: String,
    /// The pack file that ran it, without extension, if any did.
    pub file: Option<String>,
    /// Candidates tried before it that failed, with the reason (fallbacks).
    pub failed: Vec<(String, String)>,
    /// The error when no candidate compiled.
    pub error: Option<String>,
}

/// The whole story of one compile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackReport {
    pub status: PackStatus,
    /// Why the pack is refused, in one sentence for the player.
    pub refusal: Option<String>,
    pub features: Vec<Feature>,
    pub programs: Vec<ProgramOutcome>,
    /// Uniforms some program declares that Horizon does not provide (read as zero).
    pub unknown_uniforms: Vec<String>,
    /// Anything else worth knowing (device limits hit, properties ignored, …).
    pub notes: Vec<String>,
    /// Messages from the GPU's shader compiler (the browser's), by program.
    pub gpu_errors: Vec<(String, String)>,
}

impl PackReport {
    /// Recomputes [`Self::status`] from the rest: refused, failed (an
    /// essential program has an error is the caller's call: it sets
    /// `Failed` itself), partial when anything was skipped or fell back
    /// after a failure, a used feature is missing or the GPU's compiler
    /// complained, else works.
    pub fn settle(&mut self) {
        if self.refusal.is_some() {
            self.status = PackStatus::Refused;
        } else if self.status != PackStatus::Failed {
            let partial = self
                .programs
                .iter()
                .any(|p| p.error.is_some() || !p.failed.is_empty())
                || self.features.iter().any(|f| !f.supported)
                || !self.gpu_errors.is_empty();
            self.status = if partial {
                PackStatus::Partial
            } else {
                PackStatus::Works
            };
        }
    }

    /// The report as plain text, one fact a line (the Report page, the log,
    /// "Copy Report").
    pub fn text(&self, pack: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "Shader pack: {pack}");
        let _ = writeln!(out, "Status: {}", self.status.label());
        if let Some(r) = &self.refusal {
            let _ = writeln!(out, "Why: {r}");
        }
        let missing: Vec<&Feature> = self.features.iter().filter(|f| !f.supported).collect();
        if !missing.is_empty() {
            let _ = writeln!(out, "Not supported:");
            for f in missing {
                let need = if f.required { "required" } else { "optional" };
                let _ = writeln!(out, "  {} ({need}; {})", f.name, f.detail);
            }
        }
        let bad: Vec<&ProgramOutcome> = self
            .programs
            .iter()
            .filter(|p| p.error.is_some() || !p.failed.is_empty())
            .collect();
        if !bad.is_empty() {
            let _ = writeln!(out, "Programs:");
            for p in bad {
                match (&p.error, &p.file) {
                    (Some(e), _) => {
                        let _ = writeln!(out, "  {}: skipped: {e}", p.slot);
                    }
                    (None, Some(f)) => {
                        let _ = writeln!(out, "  {}: runs {f}", p.slot);
                    }
                    (None, None) => {}
                }
                for (file, e) in &p.failed {
                    // The slot's error is its first candidate's: said once.
                    if p.error.as_deref() != Some(&format!("{file}: {e}")) {
                        let _ = writeln!(out, "    {file} failed: {e}");
                    }
                }
            }
        }
        for (program, e) in &self.gpu_errors {
            let _ = writeln!(out, "GPU compiler, {program}: {e}");
        }
        if !self.unknown_uniforms.is_empty() {
            let _ = writeln!(
                out,
                "Uniforms reading zero: {}",
                self.unknown_uniforms.join(", ")
            );
        }
        for n in &self.notes {
            let _ = writeln!(out, "Note: {n}");
        }
        out
    }
}

impl fmt::Display for PackReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text(""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_follows_what_was_found() {
        let mut r = PackReport::default();
        r.settle();
        assert_eq!(r.status, PackStatus::Works);
        r.programs.push(ProgramOutcome {
            slot: "gbuffers_clouds".into(),
            file: None,
            failed: vec![],
            error: Some("x".into()),
        });
        r.settle();
        assert_eq!(r.status, PackStatus::Partial);
        r.refusal = Some("It needs compute shaders.".into());
        r.settle();
        assert_eq!(r.status, PackStatus::Refused);
        let text = r.text("Test.zip");
        assert!(text.contains("Status: Not supported"));
        assert!(text.contains("gbuffers_clouds: skipped: x"));
    }
}
