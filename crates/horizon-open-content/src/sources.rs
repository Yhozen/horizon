//! `content/open/sources.json`: every open input with its origin, author,
//! licence and checksum. Building fails on a missing file, a changed file, an
//! unknown licence or an input nobody attributed.
//!
//! Its `packs` entries name vendored resource packs (`packs/<name>/pack.json`,
//! written by `tools/vendor_open_pack.py`). Each pack file becomes a [`Media`]
//! whose destination is its own path (`assets/minecraft/textures/...`). Once a
//! pack is `enabled`, its files win over the other media for the same
//! destination in [`Sources::image`].
use crate::{Bundle, Inputs, Origin, Result, image::Rgba, sha256_hex};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};

pub struct Media {
    /// Its path under `content/open/`.
    pub file: String,
    pub author: String,
    pub license: String,
    pub origin: Value,
    pub destinations: Vec<String>,
    pub transformation: String,
    pub bytes: Vec<u8>,
    /// The vendored pack it comes from, if any.
    pub pack: Option<String>,
    /// Whether [`Sources::image`] and [`Sources::provides`] offer it: always
    /// for plain media, only once its pack is enabled for pack files.
    pub active: bool,
}

/// A vendored resource pack (`packs/<name>/pack.json`).
pub struct Pack {
    pub name: String,
    pub title: String,
    pub author: String,
    pub license: String,
    /// `url`, `page`, `version` and `sha256` of the pinned download.
    pub source: Value,
    pub enabled: bool,
    /// The manifest's path under `content/open/` and its bytes.
    pub manifest: (String, Vec<u8>),
    /// The pack's own credit files: path under `content/open/` and bytes.
    pub notices: Vec<(String, Vec<u8>)>,
}

pub struct Font {
    /// Its path under the repository root.
    pub file: String,
    pub author: String,
    pub license: String,
    pub notice: String,
    pub destinations: Vec<String>,
    pub bytes: Vec<u8>,
    pub notice_bytes: Vec<u8>,
}

pub struct Sources {
    pub media: Vec<Media>,
    pub packs: Vec<Pack>,
    pub fonts: Vec<Font>,
    /// Licence id to (file under content/open, its text).
    pub licenses: BTreeMap<String, (String, Vec<u8>)>,
    /// Upstream notices: path under content/open to bytes, with their origin.
    pub notices: Vec<(String, Vec<u8>, Value)>,
}

fn text<'a>(value: &'a Value, key: &str, what: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("sources.json: {what} has no {key}"))
}

impl Sources {
    pub fn load(inputs: &Inputs) -> Result<Self> {
        let path = inputs.content.join("sources.json");
        let doc: Value = serde_json::from_slice(
            &fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?,
        )
        .map_err(|e| format!("{}: {e}", path.display()))?;
        let read = |base: &std::path::Path, file: &str, sha: &str| -> Result<Vec<u8>> {
            let at = base.join(file);
            let bytes =
                fs::read(&at).map_err(|e| format!("missing open input {}: {e}", at.display()))?;
            let actual = sha256_hex(&bytes);
            if actual != sha {
                return Err(format!(
                    "open input {file} changed: sha256 {actual}, sources.json records {sha}"
                ));
            }
            Ok(bytes)
        };
        let mut licenses = BTreeMap::new();
        for license in doc["licenses"]
            .as_array()
            .ok_or("sources.json lists no licences")?
        {
            let id = text(license, "id", "a licence")?;
            let file = text(license, "file", id)?;
            let bytes = read(&inputs.content, file, text(license, "sha256", id)?)?;
            licenses.insert(id.to_owned(), (file.to_owned(), bytes));
        }
        let known = |id: &str, what: &str| -> Result<()> {
            if id == "OFL-1.1" || licenses.contains_key(id) {
                Ok(())
            } else {
                Err(format!(
                    "sources.json: {what} uses {id}, whose text is not vendored"
                ))
            }
        };
        let mut media = Vec::new();
        for entry in doc["media"]
            .as_array()
            .ok_or("sources.json lists no media")?
        {
            let file = text(entry, "file", "a media entry")?;
            let license = text(entry, "license", file)?;
            known(license, file)?;
            for key in ["repository", "revision", "path"] {
                text(&entry["origin"], key, file)?;
            }
            let destinations: Vec<String> = entry["destinations"]
                .as_array()
                .filter(|d| !d.is_empty())
                .ok_or_else(|| format!("sources.json: {file} has no destinations"))?
                .iter()
                .map(|d| {
                    d.as_str()
                        .map(str::to_owned)
                        .ok_or("a destination is not text")
                })
                .collect::<std::result::Result<_, _>>()?;
            media.push(Media {
                file: file.to_owned(),
                author: text(entry, "author", file)?.to_owned(),
                license: license.to_owned(),
                origin: entry["origin"].clone(),
                destinations,
                transformation: text(entry, "transformation", file)?.to_owned(),
                bytes: read(&inputs.content, file, text(entry, "sha256", file)?)?,
                pack: None,
                active: true,
            });
        }
        let mut packs = Vec::new();
        for entry in doc["packs"].as_array().into_iter().flatten() {
            let file = text(entry, "manifest", "a pack")?;
            let bytes = read(&inputs.content, file, text(entry, "sha256", file)?)?;
            let manifest: Value =
                serde_json::from_slice(&bytes).map_err(|e| format!("{file}: {e}"))?;
            let name = text(&manifest, "name", file)?;
            let dir = file
                .strip_suffix("pack.json")
                .filter(|d| *d == format!("packs/{name}/"))
                .ok_or_else(|| format!("{file}: a pack's manifest is packs/<name>/pack.json"))?;
            let license = text(&manifest, "license", file)?;
            known(license, file)?;
            let author = text(&manifest, "author", file)?;
            for key in ["url", "version", "sha256"] {
                text(&manifest["source"], key, file)?;
            }
            let source = &manifest["source"];
            let enabled = entry["enabled"].as_bool().unwrap_or(false);
            let files = manifest["files"]
                .as_object()
                .filter(|f| !f.is_empty())
                .ok_or_else(|| format!("{file} lists no files"))?;
            for (path, sha) in files {
                if !path.starts_with("assets/minecraft/textures/")
                    || path.split('/').any(|p| p == ".." || p.is_empty())
                {
                    return Err(format!("{file}: {path} is not a texture path"));
                }
                let sha = sha
                    .as_str()
                    .ok_or_else(|| format!("{file}: {path} has no sha256"))?;
                let at = format!("{dir}{path}");
                media.push(Media {
                    bytes: read(&inputs.content, &at, sha)?,
                    file: at,
                    author: author.to_owned(),
                    license: license.to_owned(),
                    origin: json!({
                        "repository": source["page"].as_str().filter(|p| !p.is_empty()).unwrap_or(text(source, "url", file)?),
                        "revision": source["version"],
                        "path": path,
                        "download": source["url"],
                        "download_sha256": source["sha256"],
                    }),
                    destinations: vec![path.clone()],
                    transformation: text(&manifest, "transformation", file)?.to_owned(),
                    pack: Some(name.to_owned()),
                    active: enabled,
                });
            }
            let mut notices = Vec::new();
            for (notice, sha) in manifest["notices"].as_object().into_iter().flatten() {
                let at = format!("{dir}{notice}");
                let sha = sha
                    .as_str()
                    .ok_or_else(|| format!("{file}: {notice} has no sha256"))?;
                notices.push((at.clone(), read(&inputs.content, &at, sha)?));
            }
            packs.push(Pack {
                name: name.to_owned(),
                title: text(&manifest, "title", file)?.to_owned(),
                author: author.to_owned(),
                license: license.to_owned(),
                source: source.clone(),
                enabled,
                manifest: (file.to_owned(), bytes),
                notices,
            });
        }
        let mut fonts = Vec::new();
        for entry in doc["fonts"]
            .as_array()
            .ok_or("sources.json lists no fonts")?
        {
            let file = text(entry, "file", "a font")?;
            let license = text(entry, "license", file)?;
            known(license, file)?;
            let notice = text(entry, "notice", file)?;
            fonts.push(Font {
                file: file.to_owned(),
                author: text(entry, "author", file)?.to_owned(),
                license: license.to_owned(),
                notice: notice.to_owned(),
                destinations: entry["destinations"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                bytes: read(&inputs.repo, file, text(entry, "sha256", file)?)?,
                notice_bytes: read(&inputs.repo, notice, text(entry, "notice_sha256", file)?)?,
            });
        }
        let mut notices = Vec::new();
        for entry in doc["notices"]
            .as_array()
            .ok_or("sources.json lists no notices")?
        {
            let file = text(entry, "file", "a notice")?;
            notices.push((
                file.to_owned(),
                read(&inputs.content, file, text(entry, "sha256", file)?)?,
                entry["origin"].clone(),
            ));
        }
        // Every vendored media file must be attributed.
        let media_dir = inputs.content.join("media");
        let mut on_disk = Vec::new();
        list(&media_dir, "media", &mut on_disk)?;
        for file in on_disk {
            if !media.iter().any(|m| m.file == file) {
                return Err(format!(
                    "content/open/{file} is not attributed in sources.json"
                ));
            }
        }
        // Every file under packs/ belongs to a listed pack.
        let packs_dir = inputs.content.join("packs");
        if packs_dir.is_dir() {
            let mut on_disk = Vec::new();
            list(&packs_dir, "packs", &mut on_disk)?;
            for file in on_disk {
                let listed = media.iter().any(|m| m.file == file)
                    || packs
                        .iter()
                        .any(|p| p.manifest.0 == file || p.notices.iter().any(|(n, _)| *n == file));
                if !listed {
                    return Err(format!(
                        "content/open/{file} is not listed by a pack in sources.json"
                    ));
                }
            }
        }
        Ok(Self {
            media,
            packs,
            fonts,
            licenses,
            notices,
        })
    }

    /// The input that provides `destination` (a bundle path): an enabled
    /// pack's file first, else the other media.
    pub fn input(&self, destination: &str) -> Option<&Media> {
        let maps = |m: &&Media| m.active && m.destinations.iter().any(|d| d == destination);
        self.media
            .iter()
            .filter(|m| m.pack.is_some())
            .find(maps)
            .or_else(|| self.media.iter().filter(|m| m.pack.is_none()).find(maps))
    }

    /// The input mapped to `destination` (a bundle path), decoded; an enabled
    /// pack wins over the other media.
    pub fn image(&self, destination: &str) -> Result<(Rgba, Origin)> {
        let media = self
            .input(destination)
            .ok_or_else(|| format!("no open input provides {destination}"))?;
        let image = Rgba::decode(&media.bytes).map_err(|e| format!("{}: {e}", media.file))?;
        Ok((image, Origin::Input(media.file.clone())))
    }

    /// The input mapped to `destination` as stored, for files that are not
    /// images (a pack's `.png.mcmeta`).
    pub fn raw(&self, destination: &str) -> Option<(&[u8], Origin)> {
        self.input(destination)
            .map(|m| (m.bytes.as_slice(), Origin::Input(m.file.clone())))
    }

    /// Whether some input provides `destination`.
    pub fn provides(&self, destination: &str) -> bool {
        self.input(destination).is_some()
            || self
                .fonts
                .iter()
                .any(|f| f.destinations.iter().any(|d| d == destination))
    }

    /// The attribution of an input, for the provenance record.
    pub fn describe(&self, input: &str) -> Option<Value> {
        if let Some(m) = self.media.iter().find(|m| m.file == input) {
            let mut source = json!({
                "author": m.author, "license": m.license, "origin": m.origin,
                "sha256": sha256_hex(&m.bytes), "transformation": m.transformation,
            });
            if let Some(pack) = &m.pack {
                source["pack"] = json!(pack);
            }
            return Some(source);
        }
        for pack in &self.packs {
            if pack.manifest.0 == input {
                return Some(
                    json!({"pack_manifest": pack.name, "sha256": sha256_hex(&pack.manifest.1)}),
                );
            }
            if let Some((file, bytes)) = pack.notices.iter().find(|(f, _)| f == input) {
                return Some(
                    json!({"notice": file, "pack": pack.name, "sha256": sha256_hex(bytes)}),
                );
            }
        }
        if let Some(f) = self
            .fonts
            .iter()
            .find(|f| f.file == input || f.notice == input)
        {
            return Some(json!({
                "author": f.author, "license": f.license, "file": f.file,
                "sha256": sha256_hex(if f.file == input { &f.bytes } else { &f.notice_bytes }),
            }));
        }
        if let Some((_, (file, bytes))) = self.licenses.iter().find(|(_, (f, _))| f == input) {
            return Some(json!({"license_text": file, "sha256": sha256_hex(bytes)}));
        }
        if let Some((file, bytes, origin)) = self.notices.iter().find(|(f, _, _)| f == input) {
            return Some(json!({"notice": file, "origin": origin, "sha256": sha256_hex(bytes)}));
        }
        None
    }

    /// The licence texts, the upstream notices, the fonts and the credits
    /// (`credits.json` for the Credits screen, `CREDITS.md` for people).
    pub fn write_notices(&self, bundle: &mut Bundle) -> Result<()> {
        // A pack is credited, and its licence shipped, once the bundle uses
        // one of its files.
        let used: std::collections::BTreeSet<&str> = bundle
            .files
            .values()
            .filter_map(|(_, origin)| match origin {
                Origin::Input(input) => Some(input.as_str()),
                Origin::Generator(_) => None,
            })
            .collect();
        let used_packs: Vec<(&Pack, usize)> = self
            .packs
            .iter()
            .filter_map(|p| {
                let n = self
                    .media
                    .iter()
                    .filter(|m| m.pack.as_ref() == Some(&p.name) && used.contains(m.file.as_str()))
                    .count();
                (n > 0).then_some((p, n))
            })
            .collect();
        let in_use = |id: &str| {
            self.media
                .iter()
                .any(|m| m.pack.is_none() && m.license == id)
                || used_packs.iter().any(|(p, _)| p.license == id)
                || (!used_packs.is_empty() && id == "CC-BY-SA-4.0")
        };
        if !used_packs.is_empty() && !self.licenses.contains_key("CC-BY-SA-4.0") {
            return Err("sources.json: the bundle's licence, CC-BY-SA-4.0, is not vendored".into());
        }
        for (id, (file, bytes)) in &self.licenses {
            if !in_use(id) {
                continue;
            }
            bundle.add(
                format!("credits/{file}"),
                bytes.clone(),
                Origin::Input(file.clone()),
            )?;
        }
        for (file, bytes, _) in &self.notices {
            bundle.add(
                format!("credits/{file}"),
                bytes.clone(),
                Origin::Input(file.clone()),
            )?;
        }
        for (pack, _) in &used_packs {
            for (file, bytes) in &pack.notices {
                bundle.add(
                    format!("credits/{file}"),
                    bytes.clone(),
                    Origin::Input(file.clone()),
                )?;
            }
        }
        for font in &self.fonts {
            for destination in &font.destinations {
                bundle.add(
                    destination.clone(),
                    font.bytes.clone(),
                    Origin::Input(font.file.clone()),
                )?;
            }
            let name = font.notice.rsplit('/').next().unwrap_or(&font.notice);
            bundle.add(
                format!("credits/licenses/{name}"),
                font.notice_bytes.clone(),
                Origin::Input(font.notice.clone()),
            )?;
        }
        // Group the artwork by author and licence for the credits.
        let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        for m in self.media.iter().filter(|m| m.pack.is_none()) {
            let name = m.origin["path"]
                .as_str()
                .unwrap_or(&m.file)
                .rsplit('/')
                .next()
                .unwrap()
                .to_owned();
            groups
                .entry((m.author.clone(), m.license.clone()))
                .or_default()
                .push(name);
        }
        let repository = self
            .media
            .first()
            .map(|m| m.origin["repository"].clone())
            .unwrap_or(Value::Null);
        let revision = self
            .media
            .first()
            .map(|m| m.origin["revision"].clone())
            .unwrap_or(Value::Null);
        let artwork: Vec<Value> = groups
            .iter()
            .map(|((author, license), files)| json!({"author": author, "license": license, "files": files}))
            .collect();
        let fonts: Vec<Value> = self
            .fonts
            .iter()
            .map(|f| json!({"name": f.file.rsplit('/').next().unwrap(), "author": f.author, "license": f.license}))
            .collect();
        let credits = json!({
            "title": "Horizon Open Creative",
            "artwork_source": {"name": "Minetest Game", "repository": repository, "revision": revision},
            "artwork": artwork,
            "fonts": fonts,
            "original": "All other artwork (bedrock, the crafting table, the interface and status bars, the sky and title backdrop, the player's arm) and all block, item, recipe and world data are original to Horizon, generated by horizon-open-content (MIT).",
            "licenses": {
                "CC-BY-SA-3.0": "Creative Commons Attribution-ShareAlike 3.0 Unported (credits/licenses/CC-BY-SA-3.0.txt)",
                "CC0-1.0": "Creative Commons CC0 1.0 Universal (credits/licenses/CC0-1.0.txt)",
                "OFL-1.1": "SIL Open Font License 1.1 (credits/licenses/OFL-PixelifySans.txt)",
            },
        });
        let mut credits = credits;
        if !used_packs.is_empty() {
            credits["packs"] = json!(
                used_packs
                    .iter()
                    .map(|(p, n)| json!({
                        "title": p.title, "author": p.author, "license": p.license,
                        "source": p.source, "files_used": n,
                    }))
                    .collect::<Vec<_>>()
            );
            credits["licenses"]["CC-BY-SA-4.0"] = json!(
                "Creative Commons Attribution-ShareAlike 4.0 International (credits/licenses/CC-BY-SA-4.0.txt)"
            );
            credits["bundle_license"] = json!(BUNDLE_LICENSE);
        }
        bundle.json("credits/credits.json", &credits, "notices")?;
        // The same, as the rolling Credits screen reads them.
        let mut disciplines = vec![json!({
            "discipline": format!("Block artwork: Minetest Game ({})", revision.as_str().unwrap_or("").get(..12).unwrap_or("")),
            "titles": groups.iter().map(|((author, license), files)| json!({
                "title": format!("{author} ({license})"),
                "names": files,
            })).collect::<Vec<_>>(),
        })];
        for (pack, _) in &used_packs {
            disciplines.push(json!({
                "discipline": format!("Textures: {} ({})", pack.title, pack.source["version"].as_str().unwrap_or("")),
                "titles": [{
                    "title": format!("{} ({})", pack.author, pack.license),
                    "names": [pack.source["page"].as_str().filter(|p| !p.is_empty()).or(pack.source["url"].as_str()).unwrap_or("")],
                }],
            }));
        }
        disciplines.push(json!({
            "discipline": "Fonts",
            "titles": self.fonts.iter().map(|f| json!({
                "title": format!("{} ({})", f.author, f.license),
                "names": [f.file.rsplit('/').next().unwrap()],
            })).collect::<Vec<_>>(),
        }));
        disciplines.push(json!({
            "discipline": "Horizon",
            "titles": [{"title": "Original artwork, block data and the game", "names": ["The Horizon contributors (MIT)"]}],
        }));
        disciplines.push(json!({
            "discipline": "Licences",
            "titles": [
                {"title": "CC BY-SA 3.0", "names": ["creativecommons.org/licenses/by-sa/3.0"]},
                {"title": "CC0 1.0", "names": ["creativecommons.org/publicdomain/zero/1.0"]},
                {"title": "SIL Open Font License 1.1", "names": ["openfontlicense.org"]},
            ],
        }));
        bundle.json(
            "assets/minecraft/texts/credits.json",
            &json!([{"section": "Horizon Open Creative", "disciplines": disciplines}]),
            "notices",
        )?;
        let mut md =
            String::from("# Horizon Open Creative: credits\n\nBlock artwork from Minetest Game (");
        md.push_str(repository.as_str().unwrap_or(""));
        md.push_str(" at revision ");
        md.push_str(revision.as_str().unwrap_or(""));
        md.push_str(
            "), used under its licences; the upstream notices are in `credits/notices/`.\n\n",
        );
        for ((author, license), files) in &groups {
            md.push_str(&format!("- {author} ({license}): {}\n", files.join(", ")));
        }
        md.push_str("\nFonts:\n\n");
        for f in &self.fonts {
            md.push_str(&format!(
                "- {} by {} ({})\n",
                f.file.rsplit('/').next().unwrap(),
                f.author,
                f.license
            ));
        }
        md.push('\n');
        for (pack, n) in &used_packs {
            md.push_str(&format!(
                "Textures from {} {} by {} ({}), {n} files used, re-encoded as RGBA PNG; source {}, SHA-256 {}. Its own credits are in `credits/packs/{}/`.\n\n",
                pack.title,
                pack.source["version"].as_str().unwrap_or(""),
                pack.author,
                pack.license,
                pack.source["url"].as_str().unwrap_or(""),
                pack.source["sha256"].as_str().unwrap_or(""),
                pack.name,
            ));
        }
        if !used_packs.is_empty() {
            md.push_str(&format!("{BUNDLE_LICENSE}\n\n"));
        }
        md.push_str(credits["original"].as_str().unwrap());
        md.push_str("\n\nChanges: the textures are re-encoded as RGBA PNG with their pixels unchanged, except the grass side and the ore minerals, which are composited over the dirt and stone textures as Minetest Game layers them; Horizon renames them to its block and item identifiers and adds animation metadata for water.\n");
        bundle.text("credits/CREDITS.md", &md, "notices")
    }
}

/// The bundle's own terms, stated once it carries a CC BY-SA 4.0 pack.
const BUNDLE_LICENSE: &str = "This bundle as a whole is distributed under CC BY-SA 4.0 (credits/licenses/CC-BY-SA-4.0.txt). Files taken unchanged from an input also stay under that input's licence, as horizon-open-provenance.json records; the fonts stay under the SIL Open Font License 1.1.";

fn list(dir: &std::path::Path, prefix: &str, out: &mut Vec<String>) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{prefix}/{name}");
        if entry.path().is_dir() {
            list(&entry.path(), &path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const STONE: &str = "assets/minecraft/textures/block/stone.png";
    const DIRT: &str = "assets/minecraft/textures/block/dirt.png";

    fn put(root: &Path, file: &str, bytes: &[u8]) -> String {
        let at = root.join(file);
        fs::create_dir_all(at.parent().unwrap()).unwrap();
        fs::write(at, bytes).unwrap();
        sha256_hex(bytes)
    }

    fn png(shade: u8) -> Vec<u8> {
        Rgba::filled(16, 16, [shade, shade, shade, 255]).encode()
    }

    /// A content folder with one media file for stone and a pack with stone
    /// and dirt.
    fn content(name: &str, enabled: bool) -> (PathBuf, Inputs) {
        let root = std::env::temp_dir().join(format!(
            "horizon-open-sources-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let cc0 = put(&root, "licenses/CC0-1.0.txt", b"CC0");
        let by_sa = put(&root, "licenses/CC-BY-SA-4.0.txt", b"BY-SA");
        let media = put(&root, "media/stone.png", &png(1));
        let mut files = serde_json::Map::new();
        for (path, shade) in [(STONE, 2), (DIRT, 3)] {
            let sha = put(&root, &format!("packs/test/{path}"), &png(shade));
            files.insert(path.to_owned(), json!(sha));
        }
        let notice = put(&root, "packs/test/pack.txt", b"credits");
        let manifest = serde_json::to_vec(&json!({
            "schema": 1, "name": "test", "title": "Test Pack", "author": "Someone",
            "license": "CC-BY-SA-4.0", "transformation": "unchanged",
            "source": {"url": "https://example.com/pack.zip", "page": "", "version": "1", "sha256": "00"},
            "notices": {"pack.txt": notice},
            "files": files,
        }))
        .unwrap();
        let manifest_sha = put(&root, "packs/test/pack.json", &manifest);
        let doc = json!({
            "schema": 1,
            "licenses": [
                {"id": "CC0-1.0", "file": "licenses/CC0-1.0.txt", "sha256": cc0},
                {"id": "CC-BY-SA-4.0", "file": "licenses/CC-BY-SA-4.0.txt", "sha256": by_sa},
            ],
            "media": [{
                "file": "media/stone.png", "author": "Media Author", "license": "CC0-1.0",
                "origin": {"repository": "r", "revision": "v", "path": "stone.png"},
                "sha256": media, "destinations": [STONE], "transformation": "none",
            }],
            "packs": [{"manifest": "packs/test/pack.json", "sha256": manifest_sha, "enabled": enabled}],
            "fonts": [],
            "notices": [],
        });
        put(&root, "sources.json", &serde_json::to_vec(&doc).unwrap());
        let inputs = Inputs {
            repo: root.clone(),
            content: root.clone(),
        };
        (root, inputs)
    }

    fn shade(sources: &Sources, destination: &str) -> u8 {
        sources.image(destination).unwrap().0.get(0, 0)[0]
    }

    #[test]
    fn an_enabled_pack_wins_over_the_media_for_the_same_destination() {
        let (root, inputs) = content("enabled", true);
        let sources = Sources::load(&inputs).unwrap();
        assert_eq!(shade(&sources, STONE), 2);
        assert_eq!(shade(&sources, DIRT), 3);
        let (image, origin) = sources.image(STONE).unwrap();
        let Origin::Input(input) = &origin else {
            panic!("an input")
        };
        assert_eq!(*input, format!("packs/test/{STONE}"));
        assert_eq!(sources.describe(input).unwrap()["pack"], "test");
        // Both stay resolvable: the media file is still attributed.
        assert_eq!(
            sources.describe("media/stone.png").unwrap()["author"],
            "Media Author"
        );
        // The credits name the pack, and ship its notice and the CC BY-SA
        // 4.0 text, once the bundle uses one of its files.
        let mut bundle = Bundle::default();
        bundle.png(STONE, &image, origin).unwrap();
        sources.write_notices(&mut bundle).unwrap();
        let md = String::from_utf8(bundle.get("credits/CREDITS.md").unwrap().to_vec()).unwrap();
        assert!(md.contains("Textures from Test Pack 1 by Someone (CC-BY-SA-4.0), 1 files"));
        assert!(md.contains(BUNDLE_LICENSE));
        assert!(bundle.contains("credits/packs/test/pack.txt"));
        assert!(bundle.contains("credits/licenses/CC-BY-SA-4.0.txt"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_disabled_pack_is_checked_but_not_offered() {
        let (root, inputs) = content("disabled", false);
        let sources = Sources::load(&inputs).unwrap();
        assert_eq!(shade(&sources, STONE), 1);
        assert!(!sources.provides(DIRT));
        let mut bundle = Bundle::default();
        sources.write_notices(&mut bundle).unwrap();
        assert!(!bundle.contains("credits/packs/test/pack.txt"));
        assert!(!bundle.contains("credits/licenses/CC-BY-SA-4.0.txt"));
        // A changed pack file, or a file no pack lists, fails loading.
        fs::write(root.join("packs/test").join(DIRT), b"changed").unwrap();
        assert!(Sources::load(&inputs).err().unwrap().contains("changed"));
        let _ = fs::remove_dir_all(&root);
        let (root, inputs) = content("stray", false);
        fs::write(root.join("packs/test/stray.png"), b"x").unwrap();
        assert!(Sources::load(&inputs).err().unwrap().contains("stray.png"));
        let _ = fs::remove_dir_all(root);
    }
}
