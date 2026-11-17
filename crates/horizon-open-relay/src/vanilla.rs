//! The vanilla side's numbers, derived where the relay runs and never stored
//! in the repository: Mojang's official server jar for the pinned version
//! (fetched from Mojang's version manifest and checked against its SHA-1, as
//! any server host does, or a jar the operator already has), and its data
//! generator's reports (`--reports`: `blocks.json`, `registries.json`).
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

type Result<T> = std::result::Result<T, String>;

fn get(url: &str) -> Result<Vec<u8>> {
    let response = ureq::get(url)
        .set("User-Agent", "horizon-open-relay")
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(512 << 20)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{url}: {e}"))?;
    Ok(bytes)
}

fn sha1_hex(bytes: &[u8]) -> String {
    use sha1::Digest;
    sha1::Sha1::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn json(bytes: &[u8], what: &str) -> Result<serde_json::Value> {
    serde_json::from_slice(bytes).map_err(|e| format!("{what}: {e}"))
}

/// Download the official server jar of `version` into `dir/server.jar`
/// (kept when it is already there with the right checksum).
pub fn fetch_server_jar(version: &str, dir: &Path) -> Result<PathBuf> {
    let manifest = json(&get(MANIFEST)?, "version manifest")?;
    let url = manifest["versions"]
        .as_array()
        .and_then(|v| v.iter().find(|e| e["id"] == version))
        .and_then(|e| e["url"].as_str())
        .ok_or_else(|| format!("version {version} is not in Mojang's manifest"))?;
    let meta = json(&get(url)?, "version metadata")?;
    let server = &meta["downloads"]["server"];
    let (jar_url, sha1) = server["url"]
        .as_str()
        .zip(server["sha1"].as_str())
        .ok_or_else(|| format!("version {version} has no server download"))?;
    let jar = dir.join("server.jar");
    if fs::read(&jar).is_ok_and(|b| sha1_hex(&b) == sha1) {
        return Ok(jar);
    }
    let bytes = get(jar_url)?;
    if sha1_hex(&bytes) != sha1 {
        return Err(format!(
            "{jar_url}: the SHA-1 does not match Mojang's manifest"
        ));
    }
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    fs::write(&jar, bytes).map_err(|e| format!("{}: {e}", jar.display()))?;
    Ok(jar)
}

/// Run the jar's data generator (`--reports`) in `dir` and keep the two
/// reports the relay reads, in `dir/reports/`.
pub fn generate_reports(jar: &Path, dir: &Path, java: &str) -> Result<PathBuf> {
    let jar = jar
        .canonicalize()
        .map_err(|e| format!("{}: {e}", jar.display()))?;
    let work = dir.join("generator");
    fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let status = Command::new(java)
        .current_dir(&work)
        .arg("-DbundlerMainClass=net.minecraft.data.Main")
        .arg("-jar")
        .arg(&jar)
        .args(["--reports", "--output", "generated"])
        .status()
        .map_err(|e| format!("{java}: {e}"))?;
    if !status.success() {
        return Err(format!("the data generator failed ({status})"));
    }
    let reports = dir.join("reports");
    fs::create_dir_all(&reports).map_err(|e| format!("{}: {e}", reports.display()))?;
    for name in ["blocks.json", "registries.json"] {
        let from = work.join("generated/reports").join(name);
        fs::copy(&from, reports.join(name)).map_err(|e| format!("{}: {e}", from.display()))?;
    }
    // The extracted libraries and the generator's other output are not needed again.
    let _ = fs::remove_dir_all(&work);
    Ok(reports)
}
