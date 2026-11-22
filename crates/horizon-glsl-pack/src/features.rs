//! What a pack uses of the format beyond plain vertex and fragment programs,
//! for the report: compute, geometry and tessellation stages, custom images,
//! storage buffers, Iris feature flags, programs Horizon does not run, raw
//! and 3D textures, custom uniforms and per-program state. A feature the
//! pack requires and Horizon lacks refuses the pack.
//!
//! The supported-ness of each is a constant here, so whoever lands a feature
//! flips one line.

use std::collections::BTreeMap;

use crate::PackFiles;
use crate::program::ProgramKind;
use crate::report::Feature;
use crate::shaders_properties::ShadersProperties;

/// Iris feature flags (`iris.features.*`) Horizon has: listed ones define
/// `IRIS_FEATURE_<flag>` for the pack's sources and properties.
/// `BLOCK_EMISSION_ATTRIBUTE` is `at_midBlock.w` (the terrain vertex stream).
/// `SEPARATE_HARDWARE_SAMPLERS` is `shadowtex0HW` / `shadowtex1HW` (host/sampling.rs).
/// `COMPUTE_SHADERS` and `SSBO` are `.csh` programs and `bufferObject.*`
/// storage buffers (compute.rs).
pub const SUPPORTED_FLAGS: &[&str] = &[
    "BLOCK_EMISSION_ATTRIBUTE",
    "COMPUTE_SHADERS",
    "CUSTOM_IMAGES",
    "SEPARATE_HARDWARE_SAMPLERS",
    "SSBO",
];
/// `image.*` declarations and `uniform image*` (custom images, `crate::images`).
pub const CUSTOM_IMAGES: bool = true;

/// `uniform.*` / `variable.*` in shaders.properties (custom uniforms).
pub const CUSTOM_UNIFORMS: bool = true;
/// `customTexture.*` (Iris's extra samplers).
pub const CUSTOM_TEXTURES: bool = true;
/// Raw (`.dat`) textures in `texture.*` / `customTexture.*`.
pub const RAW_TEXTURES: bool = true;
/// 3D textures (`TEXTURE_3D` raw textures).
pub const TEXTURES_3D: bool = true;
/// Compute programs (`.csh`) of setup, begin, shadowcomp, prepare, deferred and composite.
pub const COMPUTE_SHADERS: bool = true;
/// `bufferObject.*` storage buffers and `buffer` blocks.
pub const STORAGE_BUFFERS: bool = true;
/// `prepare*` full-screen passes (vertex and fragment stages).
pub const PREPARE_PASSES: bool = true;

/// A flag's player-facing name.
fn flag_name(flag: &str) -> String {
    match flag {
        "SSBO" => "storage buffers (SSBO)".into(),
        "CUSTOM_IMAGES" => "custom images".into(),
        "COMPUTE_SHADERS" => "compute shaders".into(),
        "TESSELATION_SHADERS" | "TESSELLATION_SHADERS" => "tessellation shaders".into(),
        "SEPARATE_HARDWARE_SAMPLERS" => "separate hardware shadow samplers".into(),
        "HIGHER_SHADOWCOLOR" => "more than two shadowcolor buffers".into(),
        "PER_BUFFER_BLENDING" => "per-buffer blending".into(),
        "ENTITY_TRANSLUCENT" => "the gbuffers_entities_translucent program".into(),
        "BLOCK_EMISSION_ATTRIBUTE" => "the block emission attribute".into(),
        "REVERSED_CULLING" => "reversed shadow culling".into(),
        "CAN_DISABLE_WEATHER" => "the weather directive".into(),
        "FADE_VARIABLE" => "the chunk fade attribute".into(),
        "TEXTURE_FILTERING" => "texture filtering modes".into(),
        other => format!("the Iris feature {other}"),
    }
}

/// The `iris.features.required` and `iris.features.optional` flags.
pub fn flags(props: &ShadersProperties) -> (Vec<String>, Vec<String>) {
    let mut required = Vec::new();
    let mut optional = Vec::new();
    for (key, value) in &props.entries {
        let to = match key.as_str() {
            "iris.features.required" => &mut required,
            "iris.features.optional" => &mut optional,
            _ => continue,
        };
        for flag in value.split([' ', '\t', ',']).filter(|f| !f.is_empty()) {
            let flag = flag.to_ascii_uppercase();
            if !to.contains(&flag) {
                to.push(flag);
            }
        }
    }
    (required, optional)
}

/// `IRIS_FEATURE_<flag>` for every flag the pack lists that Horizon has.
pub fn feature_macros(props: &ShadersProperties) -> BTreeMap<String, String> {
    let (required, optional) = flags(props);
    required
        .into_iter()
        .chain(optional)
        .filter(|f| SUPPORTED_FLAGS.contains(&f.as_str()))
        .map(|f| (format!("IRIS_FEATURE_{f}"), String::new()))
        .collect()
}

/// One feature, found at `detail` (a few places, then a count).
fn feature(name: &str, supported: bool, required: bool, places: &[String]) -> Feature {
    let mut detail = places
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if places.len() > 3 {
        detail.push_str(&format!(" and {} more", places.len() - 3));
    }
    Feature {
        name: name.to_string(),
        supported,
        required,
        detail,
    }
}

/// The program name of a pack path (`shaders/world0/composite_a.csh` →
/// `composite_a`), if the file is in `shaders/` or the world folder.
fn program_file<'a>(path: &'a str, folder: &str) -> Option<(&'a str, &'a str)> {
    let rest = path.strip_prefix("shaders/")?;
    let rest = if folder.is_empty() {
        rest
    } else {
        rest.strip_prefix(folder)
            .and_then(|r| r.strip_prefix('/'))
            .unwrap_or(rest)
    };
    if rest.contains('/') {
        return None;
    }
    rest.rsplit_once('.')
}

/// Every feature the pack uses for the dimension whose programs are in
/// `folder` (and in `shaders/` itself), with whether Horizon has it and
/// whether the pack says it needs it.
pub fn detect(files: &PackFiles, folder: &str, props: &ShadersProperties) -> Vec<Feature> {
    let (required, optional) = flags(props);
    let needs = |flag: &str| required.iter().any(|f| f == flag);
    let mut out = Vec::new();

    // Stages and programs by file name.
    let mut compute = Vec::new();
    let mut geometry = Vec::new();
    let mut tessellation = Vec::new();
    let mut not_run: Vec<String> = Vec::new();
    let mut prepare = Vec::new();
    for path in files.0.keys() {
        let Some((name, ext)) = program_file(path, folder) else {
            continue;
        };
        match ext {
            "csh" => compute.push(path.clone()),
            "gsh" => geometry.push(path.clone()),
            "tcs" | "tes" => tessellation.push(path.clone()),
            "vsh" | "fsh" => {
                let base = name.trim_end_matches(|c: char| c.is_ascii_digit());
                if base == "prepare" && ProgramKind::from_name(name).is_some() {
                    if ext == "fsh" {
                        prepare.push(path.clone());
                    }
                } else if (["shadowcomp", "begin", "setup"].contains(&base)
                    || name.starts_with("dh_"))
                    && !not_run.contains(path)
                {
                    not_run.push(path.clone());
                }
            }
            _ => {}
        }
    }
    if !compute.is_empty() || needs("COMPUTE_SHADERS") {
        out.push(feature(
            "compute shaders",
            COMPUTE_SHADERS,
            needs("COMPUTE_SHADERS"),
            &compute,
        ));
    }
    if !geometry.is_empty() {
        out.push(feature("geometry shaders", false, false, &geometry));
    }
    if !tessellation.is_empty() || needs("TESSELATION_SHADERS") || needs("TESSELLATION_SHADERS") {
        out.push(feature(
            "tessellation shaders",
            false,
            needs("TESSELATION_SHADERS") || needs("TESSELLATION_SHADERS"),
            &tessellation,
        ));
    }
    if !prepare.is_empty() {
        out.push(feature("prepare passes", PREPARE_PASSES, false, &prepare));
    }
    if !not_run.is_empty() {
        out.push(feature(
            "programs Horizon does not run (shadowcomp, begin, setup, Distant Horizons)",
            false,
            false,
            &not_run,
        ));
    }

    // Keys of shaders.properties.
    let keys = |prefixes: &[&str]| -> Vec<String> {
        let mut found: Vec<String> = props
            .entries
            .iter()
            .filter(|(k, _)| prefixes.iter().any(|p| k.starts_with(p)))
            .map(|(k, _)| k.clone())
            .collect();
        found.dedup();
        found
    };
    let images = keys(&["image."]);
    if !images.is_empty() || needs("CUSTOM_IMAGES") {
        out.push(feature(
            "custom images",
            CUSTOM_IMAGES,
            needs("CUSTOM_IMAGES"),
            &images,
        ));
    }
    let buffers = keys(&["bufferObject."]);
    if !buffers.is_empty() || needs("SSBO") {
        out.push(feature(
            "storage buffers (SSBO)",
            STORAGE_BUFFERS,
            needs("SSBO"),
            &buffers,
        ));
    }
    let uniforms = keys(&["uniform.", "variable."]);
    if !uniforms.is_empty() {
        out.push(feature(
            "custom uniforms",
            CUSTOM_UNIFORMS,
            false,
            &uniforms,
        ));
    }
    let custom = keys(&["customTexture."]);
    if !custom.is_empty() {
        out.push(feature("custom textures", CUSTOM_TEXTURES, false, &custom));
    }
    let raw: Vec<(String, String)> = props
        .entries
        .iter()
        .filter(|(k, v)| {
            (k.starts_with("texture.") || k.starts_with("customTexture."))
                && v.split_whitespace().nth(1).is_some()
        })
        .cloned()
        .collect();
    let raw_keys: Vec<String> = raw.iter().map(|(k, _)| k.clone()).collect();
    if !raw.is_empty() {
        out.push(feature("raw textures", RAW_TEXTURES, false, &raw_keys));
    }
    let three_d: Vec<String> = raw
        .iter()
        .filter(|(_, v)| v.split_whitespace().nth(1) == Some("TEXTURE_3D"))
        .map(|(k, _)| k.clone())
        .collect();
    if !three_d.is_empty() {
        out.push(feature("3D textures", TEXTURES_3D, false, &three_d));
    }
    for (name, prefixes) in [
        ("per-program blending", &["blend."][..]),
        ("per-program alpha test", &["alphaTest."]),
        ("buffer sizes", &["size.buffer."]),
        ("pass viewports", &["scale."]),
        ("flip control", &["flip."]),
    ] {
        let found = keys(prefixes);
        if !found.is_empty() {
            out.push(feature(name, true, false, &found));
        }
    }

    // Flags not covered above.
    let covered = [
        "COMPUTE_SHADERS",
        "TESSELATION_SHADERS",
        "TESSELLATION_SHADERS",
        "CUSTOM_IMAGES",
        "SSBO",
    ];
    for (flag, is_required) in required
        .iter()
        .map(|f| (f, true))
        .chain(optional.iter().map(|f| (f, false)))
    {
        let supported = SUPPORTED_FLAGS.contains(&flag.as_str());
        // An optional flag Horizon lacks is simply not offered: the pack
        // takes its own fallback, nothing is missing.
        if covered.contains(&flag.as_str()) || !(supported || is_required) {
            continue;
        }
        out.push(feature(
            &flag_name(flag),
            supported,
            is_required,
            &[format!(
                "iris.features.{}",
                if is_required { "required" } else { "optional" }
            )],
        ));
    }
    out
}

/// The player-facing sentence refusing a pack that requires a feature
/// Horizon lacks, if it does.
pub fn refusal(features: &[Feature]) -> Option<String> {
    let f = features.iter().find(|f| f.required && !f.supported)?;
    Some(format!(
        "This pack needs {}, which Horizon does not support yet.",
        f.name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(files: &[(&str, &str)]) -> PackFiles {
        PackFiles(
            files
                .iter()
                .map(|(p, t)| (p.to_string(), t.as_bytes().to_vec()))
                .collect(),
        )
    }

    fn props(entries: &[(&str, &str)]) -> ShadersProperties {
        ShadersProperties {
            entries: entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..ShadersProperties::default()
        }
    }

    fn names(features: &[Feature]) -> Vec<(&str, bool, bool)> {
        features
            .iter()
            .map(|f| (f.name.as_str(), f.supported, f.required))
            .collect()
    }

    #[test]
    fn stages_programs_and_keys() {
        let files = pack(&[
            ("shaders/world0/composite_a.csh", ""),
            ("shaders/composite.csh", ""),
            ("shaders/world-1/deferred.csh", ""),
            ("shaders/gbuffers_terrain.gsh", ""),
            ("shaders/gbuffers_water.tcs", ""),
            ("shaders/world0/shadowcomp1.fsh", ""),
            ("shaders/begin.vsh", ""),
            ("shaders/dh_terrain.fsh", ""),
            ("shaders/prepare2.fsh", ""),
            ("shaders/lib/compute.csh", ""),
        ]);
        let p = props(&[
            (
                "image.cimage1",
                "colorimg1 RGBA RGBA16F FLOAT false true 1.0 1.0",
            ),
            ("bufferObject.0", "1024"),
            ("uniform.float.x", "1.0"),
            ("variable.float.y", "2.0"),
            (
                "texture.composite.colortex5",
                "tex/lut.dat TEXTURE_3D RGBA8 32 32 32 RGBA UNSIGNED_BYTE",
            ),
            ("texture.gbuffers.gaux1", "tex/a.png"),
            ("blend.gbuffers_water", "off"),
            ("size.buffer.colortex4", "0.5 0.5"),
            (
                "iris.features.optional",
                "ENTITY_TRANSLUCENT BLOCK_EMISSION_ATTRIBUTE",
            ),
        ]);
        let f = detect(&files, "world0", &p);
        assert_eq!(
            names(&f),
            [
                ("compute shaders", true, false),
                ("geometry shaders", false, false),
                ("tessellation shaders", false, false),
                ("prepare passes", true, false),
                (
                    "programs Horizon does not run (shadowcomp, begin, setup, Distant Horizons)",
                    false,
                    false
                ),
                ("custom images", true, false),
                ("storage buffers (SSBO)", true, false),
                ("custom uniforms", true, false),
                ("raw textures", true, false),
                ("3D textures", true, false),
                ("per-program blending", true, false),
                ("buffer sizes", true, false),
                ("the block emission attribute", true, false),
            ]
        );
        // Only this dimension's folder and shaders/ itself.
        assert_eq!(
            f[0].detail,
            "shaders/composite.csh, shaders/world0/composite_a.csh"
        );
        assert_eq!(
            f[4].detail,
            "shaders/begin.vsh, shaders/dh_terrain.fsh, shaders/world0/shadowcomp1.fsh"
        );
        assert_eq!(refusal(&f), None);
        assert_eq!(
            feature_macros(&p).into_keys().collect::<Vec<_>>(),
            ["IRIS_FEATURE_BLOCK_EMISSION_ATTRIBUTE"]
        );
    }

    #[test]
    fn required_features_refuse() {
        let p = props(&[("iris.features.required", "SSBO  CUSTOM_IMAGES")]);
        let f = detect(&PackFiles::default(), "", &p);
        assert_eq!(
            names(&f),
            [
                ("custom images", true, true),
                ("storage buffers (SSBO)", true, true)
            ]
        );
        assert_eq!(refusal(&f), None);
        let p = props(&[("iris.features.required", "BLOCK_EMISSION_ATTRIBUTE")]);
        assert_eq!(refusal(&detect(&PackFiles::default(), "", &p)), None);
        let p = props(&[("iris.features.required", "HIGHER_SHADOWCOLOR")]);
        assert_eq!(
            refusal(&detect(&PackFiles::default(), "", &p)).as_deref(),
            Some(
                "This pack needs more than two shadowcolor buffers, which Horizon does not support yet."
            )
        );
    }
}
