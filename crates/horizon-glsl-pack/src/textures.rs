//! Custom textures in `shaders.properties`: `texture.<stage>.<sampler>=<path>`
//! (every stage, `prepare`, `shadowcomp`, `begin` and `setup` included),
//! `customTexture.<name>=<path>` and `texture.noise=<path>`, each a PNG, a raw
//! texel file (`<path> TEXTURE_3D RGBA8 64 64 64 RGBA UNSIGNED_BYTE`) or a
//! resource-pack texture (`minecraft:textures/…`). Only described here: the
//! client reads and uploads the files.

use crate::directives::canonical_sampler;
use crate::plan::{CustomTexture, TextureStage};
use crate::{PackFiles, SampleKind, SamplerKind, TexDim};

/// What a custom texture's file holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CustomTextureKind {
    /// A PNG, loaded as RGBA8 with no sRGB conversion, top row at v = 0.
    #[default]
    Png,
    /// Raw texels as OpenGL's `glTexImage` would take them.
    Raw(RawTexture),
    /// A resource-pack texture (`minecraft:textures/block/stone.png`): the
    /// client maps it to its own textures.
    Resource(String),
}

/// A raw texture's description: `TEXTURE_2D RG16F 256 256 RG HALF_FLOAT`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawTexture {
    /// `TEXTURE_1D` and `TEXTURE_RECTANGLE` are 2D here (1D with a height of 1).
    pub dim: TexDim,
    /// The GL internal format as written (`RGBA8`, `R32F`, `RGBA16UI`…).
    pub internal_format: String,
    /// Width, height, depth (1 where the type has fewer).
    pub size: [u32; 3],
    /// The GL pixel format of the file (`RGBA`, `RED_INTEGER`…).
    pub pixel_format: String,
    /// The GL pixel type of the file (`UNSIGNED_BYTE`, `FLOAT`…).
    pub pixel_type: String,
}

impl RawTexture {
    /// `<type> <internalFormat> <w> [h] [d] <pixelFormat> <pixelType>`.
    fn parse(words: &[&str]) -> Option<Self> {
        let (dim, sizes) = match *words.first()? {
            "TEXTURE_1D" => (TexDim::D2, 1),
            "TEXTURE_2D" | "TEXTURE_RECTANGLE" => (TexDim::D2, 2),
            "TEXTURE_3D" => (TexDim::D3, 3),
            _ => return None,
        };
        if words.len() != 4 + sizes {
            return None;
        }
        let mut size = [1; 3];
        for (k, s) in size.iter_mut().enumerate().take(sizes) {
            *s = words[2 + k].parse().ok()?;
        }
        Some(Self {
            dim,
            internal_format: words[1].to_string(),
            size,
            pixel_format: words[2 + sizes].to_string(),
            pixel_type: words[3 + sizes].to_string(),
        })
    }

    /// Integer internal formats (`…UI` unsigned, `…I` signed) read as integers.
    pub fn sample(&self) -> SampleKind {
        let f = self.internal_format.to_ascii_uppercase();
        if f.ends_with("UI") {
            SampleKind::Uint
        } else if f.ends_with('I') {
            SampleKind::Sint
        } else {
            SampleKind::Float
        }
    }

    /// Whether WebGPU filters it: not 32-bit float or integer formats.
    pub fn filterable(&self) -> bool {
        self.sample() == SampleKind::Float && !self.internal_format.contains("32F")
    }
}

impl CustomTexture {
    /// The texture's dimension (2D unless a raw `TEXTURE_3D`).
    pub fn dim(&self) -> TexDim {
        match &self.kind {
            CustomTextureKind::Raw(r) => r.dim,
            _ => TexDim::D2,
        }
    }

    /// The texel type the client creates it with.
    pub fn sample(&self) -> SampleKind {
        match &self.kind {
            CustomTextureKind::Raw(r) => r.sample(),
            _ => SampleKind::Float,
        }
    }

    /// Whether WebGPU can filter it.
    pub fn filterable(&self) -> bool {
        match &self.kind {
            CustomTextureKind::Raw(r) => r.filterable(),
            _ => true,
        }
    }

    /// The sampler from `blur` and `clamp`; unfilterable formats sample nearest
    /// (the host filters 2D float ones itself).
    pub fn sampler(&self) -> SamplerKind {
        match (self.blur && self.filterable(), self.clamp) {
            (false, false) => SamplerKind::PointRepeat,
            (false, true) => SamplerKind::Point,
            (true, false) => SamplerKind::LinearRepeat,
            (true, true) => SamplerKind::Linear,
        }
    }
}

/// The stage of a `texture.<stage>.` key.
pub fn texture_stage(name: &str) -> Option<TextureStage> {
    Some(match name {
        "gbuffers" => TextureStage::Gbuffers,
        "shadow" => TextureStage::Shadow,
        "deferred" => TextureStage::Deferred,
        "composite" | "final" => TextureStage::Composite,
        "prepare" => TextureStage::Prepare,
        "shadowcomp" => TextureStage::Shadowcomp,
        "begin" => TextureStage::Begin,
        "setup" => TextureStage::Setup,
        _ => return None,
    })
}

/// `{"texture": {"blur": true, "clamp": true}}` beside a PNG: `(blur, clamp)`.
/// A loose scan, as packs write it by hand.
pub fn mcmeta_flags(text: &str) -> (bool, bool) {
    let flag = |key: &str| {
        let quoted = format!("\"{key}\"");
        text.match_indices(&quoted).any(|(i, _)| {
            let rest = text[i + quoted.len()..].trim_start();
            rest.strip_prefix(':')
                .is_some_and(|r| r.trim_start().starts_with("true"))
        })
    };
    (flag("blur"), flag("clamp"))
}

/// One custom texture from its key's stage and sampler and its value.
pub fn custom_texture(
    stage: TextureStage,
    sampler: &str,
    value: &str,
    files: &PackFiles,
) -> Option<CustomTexture> {
    let words: Vec<&str> = value.split_whitespace().collect();
    let first = *words.first()?;
    let sampler = canonical_sampler(sampler);
    if words.len() == 1 && first.contains(':') {
        return Some(CustomTexture {
            stage,
            sampler,
            path: first.to_string(),
            kind: CustomTextureKind::Resource(first.to_string()),
            blur: false,
            clamp: false,
        });
    }
    let path = format!("shaders/{}", first.trim_start_matches('/'));
    if words.len() > 1 {
        let raw = RawTexture::parse(&words[1..])?;
        return Some(CustomTexture {
            stage,
            sampler,
            path,
            kind: CustomTextureKind::Raw(raw),
            blur: true,
            clamp: true,
        });
    }
    let (blur, clamp) = files
        .text(&format!("{path}.mcmeta"))
        .map_or((false, false), |t| mcmeta_flags(&t));
    Some(CustomTexture {
        stage,
        sampler,
        path,
        kind: CustomTextureKind::Png,
        blur,
        clamp,
    })
}

/// Every custom texture of `shaders.properties`'s live entries, in file order.
/// Malformed values (a raw texture with missing sizes…) are skipped.
pub fn custom_textures(entries: &[(String, String)], files: &PackFiles) -> Vec<CustomTexture> {
    let mut out = Vec::new();
    for (key, value) in entries {
        let found = if key == "texture.noise" {
            custom_texture(TextureStage::All, "noisetex", value, files)
        } else if let Some(name) = key.strip_prefix("customTexture.") {
            custom_texture(TextureStage::All, name, value, files)
        } else if let Some((stage, sampler)) = key
            .strip_prefix("texture.")
            .and_then(|rest| rest.split_once('.'))
        {
            texture_stage(stage).and_then(|s| custom_texture(s, sampler, value, files))
        } else {
            None
        };
        out.extend(found);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[(&str, &str)]) -> PackFiles {
        PackFiles(
            list.iter()
                .map(|(p, t)| (p.to_string(), t.as_bytes().to_vec()))
                .collect(),
        )
    }

    fn entries(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn every_stage_custom_names_and_noise() {
        let f = files(&[(
            "shaders/tex/a.png.mcmeta",
            "{ \"texture\": { \"blur\" : true, \"clamp\":false } }",
        )]);
        let t = custom_textures(
            &entries(&[
                ("texture.prepare.colortex3", "tex/a.png"),
                ("texture.shadowcomp.shadowcolor1", "/tex/b.png"),
                ("texture.begin.gaux1", "tex/c.png"),
                ("texture.setup.depthtex0", "tex/c.png"),
                ("texture.composite.myLut", "tex/a.png"),
                ("texture.final.colortex0", "tex/a.png"),
                ("texture.nowhere.colortex0", "tex/a.png"),
                ("customTexture.clouds", "tex/c.png"),
                ("texture.noise", "tex/n.png"),
                ("shadowMapResolution", "1024"),
            ]),
            &f,
        );
        let got: Vec<(TextureStage, &str, &str)> = t
            .iter()
            .map(|t| (t.stage, t.sampler.as_str(), t.path.as_str()))
            .collect();
        use TextureStage::*;
        assert_eq!(
            got,
            [
                (Prepare, "colortex3", "shaders/tex/a.png"),
                (Shadowcomp, "shadowcolor1", "shaders/tex/b.png"),
                (Begin, "colortex4", "shaders/tex/c.png"),
                (Setup, "depthtex0", "shaders/tex/c.png"),
                (Composite, "myLut", "shaders/tex/a.png"),
                (Composite, "colortex0", "shaders/tex/a.png"),
                (All, "clouds", "shaders/tex/c.png"),
                (All, "noisetex", "shaders/tex/n.png"),
            ]
        );
        // The .mcmeta beside a.png: blur, repeat.
        assert!(t[0].blur && !t[0].clamp);
        assert_eq!(t[0].sampler(), SamplerKind::LinearRepeat);
        assert_eq!(t[2].sampler(), SamplerKind::PointRepeat);
        assert_eq!(t[0].kind, CustomTextureKind::Png);
    }

    #[test]
    fn raw_and_resource_textures() {
        let t = custom_textures(
            &entries(&[
                (
                    "customTexture.cloudNoise",
                    "tex/noise.dat TEXTURE_3D RGBA8 64 32 16 RGBA UNSIGNED_BYTE",
                ),
                (
                    "customTexture.lut",
                    "tex/lut.bin TEXTURE_2D R32F 256 128 RED FLOAT",
                ),
                (
                    "customTexture.ids",
                    "tex/ids.bin TEXTURE_1D R16UI 64 RED_INTEGER UNSIGNED_SHORT",
                ),
                ("customTexture.bad", "tex/x.bin TEXTURE_2D RGBA8 64 RGBA"),
                ("texture.gbuffers.stone", "minecraft:textures/block/stone.png"),
            ]),
            &PackFiles::default(),
        );
        assert_eq!(t.len(), 4);
        let CustomTextureKind::Raw(r) = &t[0].kind else {
            panic!("{:?}", t[0]);
        };
        assert_eq!(r.dim, TexDim::D3);
        assert_eq!(r.size, [64, 32, 16]);
        assert_eq!(
            (r.internal_format.as_str(), r.pixel_format.as_str(), r.pixel_type.as_str()),
            ("RGBA8", "RGBA", "UNSIGNED_BYTE")
        );
        assert_eq!(t[0].dim(), TexDim::D3);
        assert_eq!(t[0].sampler(), SamplerKind::Linear);
        // A 32-bit float raw texture is not filterable: nearest.
        assert_eq!(t[1].sampler(), SamplerKind::Point);
        assert_eq!(t[2].sample(), SampleKind::Uint);
        let CustomTextureKind::Raw(r) = &t[2].kind else {
            panic!()
        };
        assert_eq!((r.dim, r.size), (TexDim::D2, [64, 1, 1]));
        assert_eq!(
            t[3].kind,
            CustomTextureKind::Resource("minecraft:textures/block/stone.png".into())
        );
        assert_eq!(t[3].sampler, "stone");
    }
}
