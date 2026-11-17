//! `content/open/sources.json`: every open input with its origin, author,
//! licence and checksum. Building fails on a missing file, a changed file, an
//! unknown licence or an input nobody attributed.
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
        Ok(Self {
            media,
            fonts,
            licenses,
            notices,
        })
    }

    /// The input mapped to `destination` (a bundle path), decoded.
    pub fn image(&self, destination: &str) -> Result<(Rgba, Origin)> {
        let media = self
            .media
            .iter()
            .find(|m| m.destinations.iter().any(|d| d == destination))
            .ok_or_else(|| format!("no open input provides {destination}"))?;
        let image = Rgba::decode(&media.bytes).map_err(|e| format!("{}: {e}", media.file))?;
        Ok((image, Origin::Input(media.file.clone())))
    }

    /// Whether some input provides `destination`.
    pub fn provides(&self, destination: &str) -> bool {
        self.media
            .iter()
            .any(|m| m.destinations.iter().any(|d| d == destination))
            || self
                .fonts
                .iter()
                .any(|f| f.destinations.iter().any(|d| d == destination))
    }

    /// The attribution of an input, for the provenance record.
    pub fn describe(&self, input: &str) -> Option<Value> {
        if let Some(m) = self.media.iter().find(|m| m.file == input) {
            return Some(json!({
                "author": m.author, "license": m.license, "origin": m.origin,
                "sha256": sha256_hex(&m.bytes), "transformation": m.transformation,
            }));
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
        for (file, bytes) in self.licenses.values() {
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
        for m in &self.media {
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
        bundle.json("credits/credits.json", &credits, "notices")?;
        // The same, as the rolling Credits screen reads them.
        let mut disciplines = vec![json!({
            "discipline": format!("Block artwork: Minetest Game ({})", revision.as_str().unwrap_or("").get(..12).unwrap_or("")),
            "titles": groups.iter().map(|((author, license), files)| json!({
                "title": format!("{author} ({license})"),
                "names": files,
            })).collect::<Vec<_>>(),
        })];
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
        md.push_str(credits["original"].as_str().unwrap());
        md.push_str("\n\nChanges: the textures are re-encoded as RGBA PNG with their pixels unchanged, except the grass side and the ore minerals, which are composited over the dirt and stone textures as Minetest Game layers them; Horizon renames them to its block and item identifiers and adds animation metadata for water.\n");
        bundle.text("credits/CREDITS.md", &md, "notices")
    }
}

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
