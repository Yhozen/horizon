//! `horizon-prepare`: prepares Horizon's resources from your own Minecraft Java Edition.
//! Its flags are the ones tools/prepare_native_client.py took.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// This release's version: release.yml builds it with `HORIZON_VERSION` from the tag
/// (docs/versioning.md); a build from source says Cargo.toml's.
const VERSION: &str = match option_env!("HORIZON_VERSION") {
    Some(version) if !version.is_empty() => version,
    _ => env!("CARGO_PKG_VERSION"),
};

const HELP: &str = "\
horizon-prepare: prepare Horizon's resources from your own Minecraft Java Edition

Usage: horizon-prepare [--prepare] [--server-jar PATH] [--minecraft-dir PATH] [--root PATH]
       horizon-prepare sounds [--minecraft-dir PATH] [--root PATH]
       horizon-prepare panorama [--minecraft-dir PATH] [--root PATH]
       horizon-prepare fonts [--minecraft-dir PATH] [--root PATH]
       horizon-prepare packs [--minecraft-dir PATH] [--root PATH]
       horizon-prepare open --out PATH [--root PATH] [--check] [--archive FILE]

  --prepare             Import the pinned game files from your own copy first
  --server-jar PATH     The pinned server jar you downloaded yourself
                        (default: HORIZON_SERVER_JAR, or the copy imported before)
  --minecraft-dir PATH  The official launcher's game directory with your installed copy
                        (default: HORIZON_MINECRAFT_DIR, or the launcher's own)
  --root PATH           The Horizon folder to prepare (default: the folder this program is
                        in, else the nearest folder above the current one with research-lock.json)

Needs Minecraft played once from the official launcher, and Java (a JDK) of the version
research-lock.json pins: JAVA_HOME, else javac and java on the PATH.
The resources go to <root>/.local/horizon-client/<version>; pass that to horizon-client --resources.

horizon-prepare panorama repairs the classic menu background in existing resources without Java.
horizon-prepare fonts adds the GNU Unifont glyphs (characters the bitmap fonts lack) the same way.
horizon-prepare packs adds the built-in Programmer Art and High Contrast resource packs the same way.
Full preparation also imports all three after extracting the client JAR's placeholders.

horizon-prepare open builds Horizon Open Creative's resources (docs/open-creative.md) from the
repository's content/open folder alone: no Minecraft install, Java or download. --check only
verifies an existing open bundle at --out against its provenance record; --archive also packs the
verified bundle into the browser's data archive (WEB.md).

horizon-prepare sounds copies the pinned version's sounds (about 380 MB) from the official
launcher's asset store into the same folder, checking every file. It is optional: without it the
game is silent. Nothing is downloaded; it needs the version played once from the official launcher.
";

/// The folder with research-lock.json: beside the executable in a release folder, else the
/// current folder or one above it (a checkout, with the binary in target/).
fn default_root() -> Option<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    if let Some(dir) = beside.filter(|d| d.join("research-lock.json").is_file()) {
        return Some(dir);
    }
    let cwd = std::env::current_dir().ok()?;
    cwd.ancestors()
        .find(|d| d.join("research-lock.json").is_file())
        .map(Path::to_path_buf)
}

/// `horizon-prepare open`: the independent bundle. Reads only the repository's
/// open inputs; never looks for Minecraft, its launcher or a JDK.
#[cfg(not(feature = "open"))]
fn open(_: impl Iterator<Item = std::ffi::OsString>) -> ExitCode {
    eprintln!("horizon-prepare open: error: this build has no open bundle (the `open` feature)");
    ExitCode::from(2)
}
#[cfg(feature = "open")]
fn open(mut args: impl Iterator<Item = std::ffi::OsString>) -> ExitCode {
    let (mut out, mut root, mut check, mut archive) = (None, None, false, None);
    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy().into_owned();
        let (flag, inline) = match text.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => {
                (flag.to_string(), Some(value.to_string()))
            }
            _ => (text.clone(), None),
        };
        let mut value = || {
            inline
                .clone()
                .map(PathBuf::from)
                .or_else(|| args.next().map(PathBuf::from))
        };
        match flag.as_str() {
            "--out" => out = value(),
            "--root" => root = value(),
            "--check" => check = true,
            "--archive" => archive = value(),
            "-h" | "--help" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("horizon-prepare open: error: unrecognized argument: {text}\n\n{HELP}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(out) = out else {
        eprintln!("horizon-prepare open: error: --out PATH is required\n\n{HELP}");
        return ExitCode::from(2);
    };
    let pack = |files: usize| -> Result<usize, String> {
        if let Some(path) = &archive {
            let bytes = horizon_open_content::archive(&out)?;
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            std::fs::write(path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            println!("{}: {} bytes", path.display(), bytes.len());
        }
        Ok(files)
    };
    if check {
        return match horizon_open_content::verify(&out).and_then(pack) {
            Ok(files) => {
                println!(
                    "{}: open bundle verified ({files} files, every one attributed)",
                    out.display()
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        };
    }
    let Some(root) = root.or_else(default_root) else {
        eprintln!("horizon-prepare open: error: no research-lock.json beside this program or above the current folder; pass --root");
        return ExitCode::from(2);
    };
    let result = horizon_open_content::build(&horizon_open_content::Inputs::at(&root))
        .and_then(|bundle| horizon_open_content::write(&bundle, &out))
        .and_then(|_| horizon_open_content::verify(&out))
        .and_then(pack);
    match result {
        Ok(files) => {
            println!(
                "Horizon Open Creative resources: {files} files in {}",
                out.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    // The open bundle needs none of what follows (Minecraft, its launcher, Java).
    if std::env::args_os().nth(1).is_some_and(|a| a == "open") {
        return open(std::env::args_os().skip(2));
    }
    let mut options = horizon_prepare::Options {
        root: PathBuf::new(),
        import: false,
        minecraft_dir: None,
        server_jar: None,
        java_home: None,
    };
    let mut root = None;
    let mut args = std::env::args_os().skip(1).peekable();
    let assets =
        args.next_if(|a| matches!(a.to_str(), Some("sounds" | "panorama" | "fonts" | "packs")));
    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy().into_owned();
        let (flag, inline) = match text.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => {
                (flag.to_string(), Some(value.to_string()))
            }
            _ => (text.clone(), None),
        };
        let mut value = |name: &str| -> Result<PathBuf, String> {
            match &inline {
                Some(v) => Ok(PathBuf::from(v)),
                None => args
                    .next()
                    .map(PathBuf::from)
                    .ok_or_else(|| format!("{name} needs a path")),
            }
        };
        let parsed = match flag.as_str() {
            "--prepare" | "--server-jar" if assets.is_some() => {
                Err(format!("{flag} is for preparing, not for asset copying"))
            }
            "--prepare" if inline.is_none() => {
                options.import = true;
                Ok(())
            }
            "--server-jar" => value("--server-jar").map(|v| options.server_jar = Some(v)),
            "--minecraft-dir" => value("--minecraft-dir").map(|v| options.minecraft_dir = Some(v)),
            "--root" => value("--root").map(|v| root = Some(v)),
            "-h" | "--help" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("horizon-prepare {VERSION}");
                return ExitCode::SUCCESS;
            }
            _ => Err(format!("unrecognized argument: {text}")),
        };
        if let Err(e) = parsed {
            eprintln!("horizon-prepare: error: {e}\n\n{HELP}");
            return ExitCode::from(2);
        }
    }
    let Some(root) = root.or_else(default_root) else {
        eprintln!(
            "horizon-prepare: error: no research-lock.json beside this program or above the current folder; pass --root"
        );
        return ExitCode::from(2);
    };
    options.root = root;
    let log = |line: &str| {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    };
    let result = match assets.as_deref().and_then(|a| a.to_str()) {
        Some("sounds") => {
            horizon_prepare::sounds(&options.root, options.minecraft_dir.as_deref(), &log)
        }
        Some("panorama") => {
            horizon_prepare::panorama(&options.root, options.minecraft_dir.as_deref(), &log)
        }
        Some("fonts") => {
            horizon_prepare::fonts(&options.root, options.minecraft_dir.as_deref(), &log)
        }
        Some("packs") => {
            horizon_prepare::packs(&options.root, options.minecraft_dir.as_deref(), &log)
        }
        _ => horizon_prepare::prepare(&options, &log),
    };
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
