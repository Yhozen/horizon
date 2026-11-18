//! The pack's C-style preprocessor: `#include`, object-like `#define`,
//! `#if/#ifdef/#ifndef/#elif/#else/#endif`, option overrides. OWNER: S1.

use std::collections::BTreeMap;

use crate::{PackError, PackFiles};

/// The macros OptiFine defines before a pack's sources (`MC_VERSION`, …).
pub fn predefined(mc_version: u32) -> BTreeMap<String, String> {
    let _ = mc_version;
    todo!("S1")
}

/// The file at `path` with includes expanded, comments removed, conditionals
/// resolved, `#define`s applied (and their lines removed), `#version`,
/// `#extension` and other directives dropped: plain GLSL text.
pub fn preprocess(
    files: &PackFiles,
    path: &str,
    predefined: &BTreeMap<String, String>,
    overrides: &[(String, Option<String>)],
) -> Result<String, PackError> {
    let _ = (files, path, predefined, overrides);
    todo!("S1")
}

/// Like [`preprocess`] for text that is not a pack file (`.properties`).
pub fn preprocess_text(
    text: &str,
    file: &str,
    files: &PackFiles,
    predefined: &BTreeMap<String, String>,
    overrides: &[(String, Option<String>)],
) -> Result<String, PackError> {
    let _ = (text, file, files, predefined, overrides);
    todo!("S1")
}

/// The file at `path` with only `#include`s expanded (comments kept).
pub fn expand_includes(files: &PackFiles, path: &str) -> Result<String, PackError> {
    let _ = (files, path);
    todo!("S1")
}
