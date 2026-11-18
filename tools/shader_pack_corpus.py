#!/usr/bin/env python3
"""Shader pack corpus: download packs from Modrinth and measure the translator on them.

    python3 -I tools/shader_pack_corpus.py fetch --slugs a,b,c        # or --ranks 13-24
    python3 -I tools/shader_pack_corpus.py check [PACK...] [--dimension all] [--chrome]

`fetch` saves `<slug>.zip` files (checksums from Modrinth's API) and `index.json` in
`.local/shaderpacks-corpus/packs`; it never opens a pack. `check` runs the crate's `pack_check`
example on every pack (every program slot compiled on its own, the WGSL validated, naga's SPIR-V
writer run), optionally Chrome's WGSL compiler (`web/scripts/check-wgsl.mjs`), and writes
`summary.md` and `summary.json`: a status per pack and the errors bucketed by message.

Packs are for testing only: they stay in `.local/` and are never committed.
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# Override to share one corpus between git worktrees.
CORPUS = Path(os.environ.get("SHADER_PACK_CORPUS", ROOT / ".local" / "shaderpacks-corpus"))
API = "https://api.modrinth.com/v2"
USER_AGENT = "horizon-shader-corpus/0.1"
TARGET_DIR = Path(os.environ.get("CARGO_TARGET_DIR", "/home/vercel-sandbox/horizon/.context/target-pack"))



# ---------------------------------------------------------------- Modrinth


def api_get(path: str) -> object:
    request = urllib.request.Request(API + path, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.load(response)


def search_page(offset: int, limit: int) -> list[dict]:
    facets = urllib.parse.quote(json.dumps([["project_type:shader"]]))
    path = f"/search?facets={facets}&index=downloads&offset={offset}&limit={limit}"
    return api_get(path)["hits"]  # type: ignore[index]


def pick_version(versions: list[dict]) -> dict | None:
    """The newest version whose loaders include Iris, else the newest."""
    ordered = sorted(versions, key=lambda v: v.get("date_published", ""), reverse=True)
    for version in ordered:
        if "iris" in version.get("loaders", []):
            return version
    return ordered[0] if ordered else None


def primary_file(version: dict) -> dict | None:
    files = version.get("files", [])
    return next((f for f in files if f.get("primary")), files[0] if files else None)


def download(url: str, dest: Path, hashes: dict) -> None:
    """Downloads `url` to `dest`, checking the API's sha512 (else sha1)."""
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    algorithm = "sha512" if "sha512" in hashes else "sha1"
    digest = hashlib.new(algorithm)
    with tempfile.NamedTemporaryFile(dir=dest.parent, delete=False) as tmp:
        with urllib.request.urlopen(request, timeout=300) as response:
            while chunk := response.read(1 << 16):
                digest.update(chunk)
                tmp.write(chunk)
    if hashes.get(algorithm) and digest.hexdigest() != hashes[algorithm]:
        os.unlink(tmp.name)
        raise ValueError(f"{dest.name}: {algorithm} mismatch")
    os.replace(tmp.name, dest)


def fetch(args: argparse.Namespace) -> int:
    dest = Path(args.dest)
    dest.mkdir(parents=True, exist_ok=True)
    index_path = dest / "index.json"
    index = json.loads(index_path.read_text()) if index_path.exists() else {}
    wanted: dict[str, int | None] = {}
    if args.ranks:
        first, _, last = args.ranks.partition("-")
        first, last = int(first), int(last or first)
        offset = first - 1
        while offset < last:
            limit = min(100, last - offset)
            for i, hit in enumerate(search_page(offset, limit)):
                wanted[hit["slug"]] = offset + i + 1
            offset += limit
    for slug in filter(None, (args.slugs or "").split(",")):
        wanted.setdefault(slug.strip(), None)
    if any(rank is None for rank in wanted.values()):
        # Ranks of named slugs, from the first 200 results (only the named ones are kept).
        for offset in range(0, 200, 100):
            for i, hit in enumerate(search_page(offset, 100)):
                if hit["slug"] in wanted and wanted[hit["slug"]] is None:
                    wanted[hit["slug"]] = offset + i + 1
    failures = 0
    for slug, rank in wanted.items():
        try:
            project = api_get(f"/project/{slug}")
            version = pick_version(api_get(f"/project/{slug}/version"))  # type: ignore[arg-type]
            file = primary_file(version) if version else None
            if not file:
                print(f"{slug}: no file", file=sys.stderr)
                failures += 1
                continue
            if not file["filename"].lower().endswith(".zip"):
                print(f"{slug}: skipped, primary file is not a zip", file=sys.stderr)
                continue
            target = dest / f"{slug}.zip"
            if not (target.exists() and index.get(slug, {}).get("filename") == file["filename"]):
                download(file["url"], target, file.get("hashes", {}))
            license = project.get("license") or {}  # type: ignore[union-attr]
            index[slug] = {
                "slug": slug,
                "rank": rank,
                "title": project.get("title"),  # type: ignore[union-attr]
                "version": version.get("version_number"),  # type: ignore[union-attr]
                "loaders": version.get("loaders"),  # type: ignore[union-attr]
                "filename": file["filename"],
                "license": license.get("id"),
                "downloads": project.get("downloads"),  # type: ignore[union-attr]
            }
            print(f"{slug}: {file['filename']} (rank {rank})")
        except Exception as e:  # noqa: BLE001 - one bad pack must not stop the rest
            print(f"{slug}: {e}", file=sys.stderr)
            failures += 1
        index_path.write_text(json.dumps(index, indent=2, sort_keys=True) + "\n")
    return 1 if failures else 0


# ---------------------------------------------------------------- pure parts (unit tested)

_ANSI = re.compile(r"\x1b\[[0-9;]*m")
_QUOTED = re.compile(r"'([^']*)'|\"([^\"]*)\"|`([^`]*)`")
_PATH = re.compile(r"\b[\w./-]+\.(?:vsh|fsh|gsh|csh|glsl|wgsl|properties|png|dat|bin)\b")
_LOCATION = re.compile(r"(?:\bline\s*)?:?\d+:\d+\b|\bline \d+\b", re.IGNORECASE)
_HEX = re.compile(r"\b0x[0-9a-fA-F]+\b")
_NUMBER = re.compile(r"\b\d+(?:\.\d+)?\b")
_HANDLE = re.compile(r"\[\d+\]")
# An identifier after a word that names it: `Unknown variable: foo`, `uniform foo is float`.
_NAMED = re.compile(
    r"\b(variable:|function:|uniform|varying|sampler|type|macro|of type|name)(\s+)([A-Za-z_]\w*)"
)
# Names that say which feature is missing, so they stay in the bucket key: GLSL built-ins and
# Horizon's generated sampling helpers (`hz_sampleLod_<sampler>` keeps `hz_sampleLod_`).
_KEPT = re.compile(r"^(?:#\w+|texture\w*|texelFetch\w*|shadow2D\w*|modf|frexp|ldexp|image\w+|atomic\w+|"
                   r"uniform|varying|in|out|const)$")
_HZ_HELPER = re.compile(r"^(hz_[a-zA-Z]+_)\w+$")
# Macro families Iris defines: `MC_RENDER_STAGE_SUN` buckets as `MC_RENDER_STAGE_*`.
_MACRO_FAMILY = re.compile(r"^((?:MC|IRIS|IS)_(?:[A-Z]+_)?[A-Z]+?_)[A-Z0-9_]+$")


def headline(message: str) -> str:
    """The first meaningful line of an error, with naga's innermost note (`= ...`) for
    validation errors whose first line only names the function; no source excerpts."""
    lines = [line.strip() for line in _ANSI.sub("", message).splitlines() if line.strip()]
    if not lines:
        return ""
    first = re.sub(r"^error:\s*", "", lines[0])
    notes = [line[1:].strip() for line in lines[1:] if line.startswith("= ")]
    if notes and first.endswith("is invalid"):
        first = f"{first}: {notes[-1]}"
    return first


def _name(name: str) -> str:
    if _KEPT.match(name):
        return name
    if m := _HZ_HELPER.match(name):
        return m.group(1) + "…"
    if m := _MACRO_FAMILY.match(name):
        return m.group(1) + "*"
    return "…"


def normalise(message: str) -> str:
    """The bucket key of a message: names, paths, positions and numbers stripped (names that
    say which feature is missing are kept)."""
    text = headline(message)
    text = _QUOTED.sub(lambda m: "'" + " ".join(map(_name, next(g for g in m.groups() if g is not None)
                                                      .split())) + "'", text)
    # All-lowercase words after them are English ("varying locations"), not names.
    text = _NAMED.sub(lambda m: m.group(0) if re.fullmatch(r"[a-z]+", m.group(3)) and not m.group(1).endswith(":")
                      else m.group(1) + m.group(2) + _name(m.group(3)), text)
    text = _PATH.sub("<file>", text)
    text = _HANDLE.sub("[N]", text)
    text = _LOCATION.sub("", text)
    text = _HEX.sub("N", text)
    text = _NUMBER.sub("N", text)
    return re.sub(r"\s+", " ", text).strip(" :")


def is_naga_error(message: str) -> bool:
    """Whether a compile error came from naga (codespan output) rather than the rewriter or host."""
    text = _ANSI.sub("", message).lstrip()
    return "┌─" in text or text.startswith("error:")


def failures(result: dict, dimension: str) -> list[dict]:
    """Every failure of one pack in one dimension: (stage, slot, message, essential)."""
    out = []
    dim = result.get("dimensions", {}).get(dimension, {})
    for p in dim.get("programs", []):
        base = {"slot": p["slot"], "essential": p.get("essential", False), "source": p.get("source")}
        if p["status"] == "error":
            stage = "naga" if is_naga_error(p.get("error", "")) else "translate"
            out.append({**base, "stage": stage, "message": p.get("error", "")})
            continue
        for shader, err in (p.get("spirv_errors") or {}).items():
            if err:
                out.append({**base, "stage": "spirv", "message": err, "shader": shader})
        for shader, errs in (p.get("chrome_errors") or {}).items():
            for err in errs[:1]:
                out.append({**base, "stage": "chrome", "message": err, "shader": shader})
    return out


def pack_row(result: dict, dimension: str, chrome: bool) -> dict:
    """The summary row of one pack in one dimension, with its status."""
    dim = result.get("dimensions", {}).get(dimension, {})
    programs = [p for p in dim.get("programs", []) if p["status"] != "disabled"]
    errors = [p for p in programs if p["status"] == "error"]
    fails = failures(result, dimension)
    translate_fail = {f["slot"] for f in fails if f["stage"] == "translate"}
    spirv_fail = {f["slot"] for f in fails if f["stage"] == "spirv"}
    chrome_fail = {f["slot"] for f in fails if f["stage"] == "chrome"}
    ok = [p for p in programs if p["status"] == "ok"]
    features = result.get("features", {})
    row = {
        "pack": result.get("pack"),
        "dimension": dimension,
        "found": len(programs),
        "translated": len(programs) - len(translate_fail),
        "naga_ok": len(ok),
        "spirv_ok": len([p for p in ok if p["slot"] not in spirv_fail]),
        "chrome_ok": len([p for p in ok if p["slot"] not in chrome_fail]) if chrome else None,
        "essential_failed": sorted({f["slot"] for f in fails if f["essential"]}),
        "features": {k: len(v) for k, v in sorted(features.items())},
        "plan_error": dim.get("plan_error"),
        "read_error": result.get("read_error"),
    }
    row["status"] = status(row, features, bool(errors) or bool(fails))
    return row


def status(row: dict, features: dict, any_failure: bool) -> str:
    """Refused (requires Iris-only features), Fails (an essential program or the plan fails),
    Partial (some optional program fails) or Works-candidate."""
    if features.get("iris_features_required"):
        return "Refused"
    if row.get("read_error") or row.get("plan_error") or not row["found"] or row["essential_failed"]:
        return "Fails"
    return "Partial" if any_failure else "Works-candidate"


def buckets(results: list[dict], dimensions: list[str]) -> list[dict]:
    """Failures grouped by (stage, normalised message), most packs blocked first."""
    groups: dict[tuple[str, str], dict] = {}
    for result in results:
        for dimension in dimensions:
            for f in failures(result, dimension):
                key = (f["stage"], normalise(f["message"]))
                g = groups.setdefault(key, {"stage": key[0], "message": key[1], "count": 0,
                                            "packs": set(), "essential_packs": set(), "examples": []})
                g["count"] += 1
                g["packs"].add(result["pack"])
                if f["essential"]:
                    g["essential_packs"].add(result["pack"])
                example = (result["pack"], f["slot"])
                if len(g["examples"]) < 3 and all(e[0] != example[0] for e in g["examples"]):
                    g["examples"].append(example)
    out = []
    for g in groups.values():
        out.append({**g, "packs": sorted(g["packs"]), "essential_packs": sorted(g["essential_packs"]),
                    "examples": [list(e) for e in g["examples"]]})
    out.sort(key=lambda g: (-len(g["essential_packs"]), -len(g["packs"]), -g["count"], g["message"]))
    return out


def unknown_uniforms(results: list[dict], dimensions: list[str]) -> list[tuple[str, list[str]]]:
    """Uniforms compiled programs read that Horizon does not know (they read 0), with the packs
    that use them, most packs first."""
    packs: dict[str, set[str]] = {}
    for result in results:
        for dimension in dimensions:
            for p in result.get("dimensions", {}).get(dimension, {}).get("programs", []):
                for name in p.get("unknown_uniforms", []):
                    packs.setdefault(name, set()).add(result["pack"])
    return sorted(((n, sorted(p)) for n, p in packs.items()), key=lambda e: (-len(e[1]), e[0]))


FLAG_SHORT = {
    "compute_shaders": "csh", "geometry_shaders": "gsh", "tessellation_shaders": "tess",
    "raw_textures": "raw-tex", "prepare_programs": "prepare", "shadowcomp_programs": "shadowcomp",
    "begin_programs": "begin", "setup_programs": "setup", "iris_gbuffers": "iris-gbuf",
    "unhandled_programs": "unhandled", "iris_features_required": "REQUIRED",
    "iris_features_optional": "optional", "images": "image", "buffer_objects": "ssbo",
    "custom_uniforms": "uniform", "custom_variables": "variable", "custom_textures": "customTex",
}


def table(rows: list[dict], chrome: bool) -> str:
    head = ["pack", "dim", "found", "translated", "naga-ok", "spirv-ok"]
    head += ["chrome-ok"] if chrome else []
    head += ["essential failed", "feature flags", "status"]
    lines = ["| " + " | ".join(head) + " |", "|" + "---|" * len(head)]
    for r in rows:
        flags = " ".join(f"{FLAG_SHORT.get(k, k)}:{n}" for k, n in r["features"].items())
        cells = [r["pack"], r["dimension"].removeprefix("minecraft:"), r["found"], r["translated"],
                 r["naga_ok"], r["spirv_ok"]]
        cells += [r["chrome_ok"]] if chrome else []
        essential = ", ".join(r["essential_failed"]) or ("plan" if r["plan_error"] else "")
        cells += [essential, flags, r["status"]]
        lines.append("| " + " | ".join(str(c) for c in cells) + " |")
    return "\n".join(lines)


def bucket_table(groups: list[dict]) -> str:
    lines = ["| # | stage | message | programs | packs | essential in | examples |", "|---|---|---|---|---|---|---|"]
    for i, g in enumerate(groups, 1):
        examples = ", ".join(f"{p} {s}" for p, s in g["examples"])
        message = g["message"].replace("|", "\\|")
        lines.append(f"| {i} | {g['stage']} | {message} | {g['count']} | {len(g['packs'])} | "
                     f"{len(g['essential_packs'])} | {examples} |")
    return "\n".join(lines)


_CHROME_LINE = re.compile(r"^(.*\.wgsl):(\d+):(\d+): (error|warning|info): (.*)$")


def parse_chrome(output: str) -> dict[str, list[str]]:
    """check-wgsl.mjs's output as error messages by file (warnings dropped)."""
    errors: dict[str, list[str]] = {}
    for line in output.splitlines():
        m = _CHROME_LINE.match(line)
        if m and m.group(4) == "error":
            errors.setdefault(m.group(1), []).append(m.group(5))
    return errors


# ---------------------------------------------------------------- check


def pack_paths(given: list[str]) -> list[Path]:
    if given:
        return [Path(p).resolve() for p in given]
    # Zips, and pack folders (or links to them) beside them.
    return sorted(p for p in (CORPUS / "packs").iterdir() if p.suffix.lower() == ".zip" or p.is_dir())


def build_pack_check() -> Path:
    env = {**os.environ, "CARGO_TARGET_DIR": str(TARGET_DIR)}
    subprocess.run(["cargo", "build", "--release", "--example", "pack_check", "-p", "horizon-glsl-pack"],
                   cwd=ROOT, env=env, check=True)
    return TARGET_DIR / "release" / "examples" / "pack_check"


def run_chrome(result: dict, pack_dir: Path, web: Path) -> None:
    """Compiles the pack's WGSL with Chrome and adds `chrome_errors` to its programs."""
    for dimension, dim in result.get("dimensions", {}).items():
        wgsl_dir = pack_dir / dimension.replace(":", "_")
        if not wgsl_dir.is_dir():
            continue
        proc = subprocess.run(["node", str(web / "scripts" / "check-wgsl.mjs"), str(wgsl_dir)],
                              cwd=web, capture_output=True, text=True, timeout=3600)
        errors = parse_chrome(proc.stdout)
        if proc.returncode not in (0, 1) or "compiled" not in proc.stdout:
            dim["chrome_failure"] = (proc.stderr or proc.stdout)[-2000:]
            continue
        for p in dim["programs"]:
            if p["status"] != "ok":
                continue
            p["chrome_errors"] = {
                stage: errors.get(str(wgsl_dir / f"{p['slot']}.{stage}.wgsl"), []) for stage in ("vs", "fs")
            }


def check(args: argparse.Namespace) -> int:
    out = Path(args.out or CORPUS / "report" / datetime.datetime.now().strftime("%Y%m%d-%H%M%S")).resolve()
    out.mkdir(parents=True, exist_ok=True)
    packs = pack_paths(args.packs)
    if not args.reuse:
        binary = args.binary and Path(args.binary) or build_pack_check()
        command = [str(binary), *map(str, packs), "--out", str(out), "--dimension", args.dimension,
                   "--jobs", str(args.jobs)]
        for option in args.option:
            command += ["--option", option]
        subprocess.run(command, check=True)
    dimensions = (["minecraft:overworld", "minecraft:the_nether", "minecraft:the_end"]
                  if args.dimension == "all" else [args.dimension])
    results = []
    for pack in packs:
        name = pack.stem if pack.suffix.lower() == ".zip" else pack.name
        path = out / name / "result.json"
        result = json.loads(path.read_text())
        if args.chrome:
            print(f"chrome: {name}", flush=True)
            run_chrome(result, out / name, Path(args.web).resolve())
            path.write_text(json.dumps(result, indent=2))
        results.append(result)
    rows = [pack_row(r, d, args.chrome) for r in results for d in dimensions]
    groups = buckets(results, dimensions)
    unknown = unknown_uniforms(results, dimensions)
    summary = {"dimensions": dimensions, "chrome": args.chrome, "rows": rows, "buckets": groups,
               "unknown_uniforms": dict(unknown)}
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    counts = {s: sum(r["status"] == s for r in rows) for s in ("Works-candidate", "Partial", "Refused", "Fails")}
    md = [
        f"# Shader pack corpus check ({datetime.date.today()})", "",
        f"Dimensions: {', '.join(dimensions)}. Chrome: {'yes' if args.chrome else 'no'}. "
        + ", ".join(f"{k}: {v}" for k, v in counts.items()) + ".", "",
        table(rows, args.chrome), "", "## Errors by message", "",
        "Sorted by the packs whose essential programs they block, then packs, then programs.", "",
        bucket_table(groups), "",
        "## Unknown uniforms (read as 0) in programs that compiled", "",
        ", ".join(f"`{n}` ({len(p)})" for n, p in unknown) or "None.", "",
    ]
    (out / "summary.md").write_text("\n".join(md))
    print(out / "summary.md")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    f = sub.add_parser("fetch", help="download packs from Modrinth")
    f.add_argument("--slugs", help="comma-separated Modrinth slugs")
    f.add_argument("--ranks", help="download ranks, e.g. 13-24")
    f.add_argument("--dest", default=str(CORPUS / "packs"))
    c = sub.add_parser("check", help="run pack_check (and Chrome) on packs")
    c.add_argument("packs", nargs="*", help="pack zips or folders (default: every fetched pack)")
    c.add_argument("--out")
    c.add_argument("--dimension", default="minecraft:overworld")
    c.add_argument("--option", action="append", default=[], help="NAME=VALUE, passed to every pack")
    c.add_argument("--chrome", action="store_true", help="also compile the WGSL in headless Chrome")
    c.add_argument("--web", default=str(ROOT / "web"), help="the web folder with node_modules")
    c.add_argument("--jobs", type=int, default=3, help="packs checked in parallel")
    c.add_argument("--reuse", action="store_true",
                   help="skip pack_check: reuse the result.json files in --out (with --chrome, rerun Chrome only)")
    c.add_argument("--binary", help="a pack_check binary already built")
    args = parser.parse_args(argv)
    return fetch(args) if args.command == "fetch" else check(args)


if __name__ == "__main__":
    sys.exit(main())
