#!/usr/bin/env python3
"""Shader pack corpus, render half: the native picture of every pack, checked automatically.

    python3 -I tools/shader_pack_render.py [PACK...] [--run NAME] [--md PATH] [--no-vanilla]

For each pack (default: every pack fetched by `tools/shader_pack_corpus.py`) the game draws one scene
(`SHOT` script, default `.context/run/pack_shot.sh`: fresh data dirs, a pool, a wall, leaves, glass),
one game at a time. The screenshot is checked with the standard library only (a small PNG decoder):
black, uniform, sky equal to ground, magenta or NaN-looking regions. When an Iris reference exists
(`IRIS_DIR/<pack>-<scene>.png`) a coarse similarity is computed on a 16×9 grid, the HUD masked, and a
picture of Horizon without a pack tells whether the pack changed anything at all.

Writes `.local/shaderpacks-corpus/render/<run>/`: `<pack>.png`, `<pack>.log` (the run's output and the
game log's shader-pack lines), `summary.md`, `summary.json` and `index.html` (Horizon | Iris | vanilla).
Packs are for testing only: their pictures stay in `.local/`.
"""

from __future__ import annotations

import argparse
import datetime
import html
import json
import os
import struct
import subprocess
import sys
import tempfile
import time
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MAIN = Path("/home/vercel-sandbox/horizon")
CORPUS = Path(os.environ.get("SHADER_PACK_CORPUS", MAIN / ".local" / "shaderpacks-corpus"))
SHOT = Path(os.environ.get("SHOT", MAIN / ".context" / "run" / "pack_shot.sh"))
IRIS_DIR = Path(os.environ.get("IRIS_DIR", MAIN / ".context" / "packs" / "iris"))
IRIS_WORLD = Path(os.environ.get("IRIS_WORLD", MAIN / ".local" / "iris-oracle" / "worlds" / "scene"))
# `iris`: the Iris oracle's world and poses (noon, dusk, underwater; .context/packs/iris/replay.md),
# else `pool`: the SHOT script's scene.
SCENE = os.environ.get("SCENE", "iris" if IRIS_WORLD.is_dir() else "pool")
GAME = Path(os.environ.get("GAME", MAIN / "target" / "fast" / "horizon-client"))
CTL = Path(os.environ.get("CTL", MAIN / "target" / "fast" / "horizon-ctl"))
RES = os.environ.get("RES", str(MAIN / ".context/wt/prep/.local/horizon-client/26.3"))
SIZE = (int(os.environ.get("W", "480")), int(os.environ.get("H", "270")))
GRID = (16, 9)
# The oracle's poses: (name, commands before, feet position, yaw, pitch, settle seconds).
IRIS_POSES = [
    ("noon", ["/time set 6000"], (160.5, 78, 160.5), 180, 25, 12),
    ("dusk", ["/time set 13000"], (160.5, 78, 160.5), 180, 25, 12),
    ("underwater", ["/time set 6000", "/fill 154 78 146 166 80 157 minecraft:water replace minecraft:air"],
     (160.5, 77, 155.5), 180, 0, 25),
]


# ---------------------------------------------------------------- PNG


def decode_png(data: bytes) -> tuple[int, int, list[tuple[int, int, int]]]:
    """8-bit, non-interlaced PNGs (grey, grey+alpha, RGB, RGBA, palette) → (w, h, RGB pixels)."""
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    pos, idat, palette = 8, [], b""
    width = height = depth = ctype = interlace = 0
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos:pos + 4])
        kind = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            width, height, depth, ctype, _, _, interlace = struct.unpack(">IIBBBBB", body)
        elif kind == b"PLTE":
            palette = body
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    if depth != 8 or interlace:
        raise ValueError(f"unsupported PNG (depth {depth}, interlace {interlace})")
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[ctype]
    raw = zlib.decompress(b"".join(idat))
    stride = width * channels
    rows: list[bytearray] = []
    prev = bytearray(stride)
    at = 0
    for _ in range(height):
        f = raw[at]
        line = bytearray(raw[at + 1:at + 1 + stride])
        at += 1 + stride
        for i in range(stride):
            a = line[i - channels] if i >= channels else 0
            b = prev[i]
            c = prev[i - channels] if i >= channels else 0
            if f == 1:
                line[i] = (line[i] + a) & 0xFF
            elif f == 2:
                line[i] = (line[i] + b) & 0xFF
            elif f == 3:
                line[i] = (line[i] + (a + b) // 2) & 0xFF
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[i] = (line[i] + pred) & 0xFF
        rows.append(line)
        prev = line
    pixels = []
    for line in rows:
        for x in range(width):
            p = line[x * channels:(x + 1) * channels]
            if ctype == 3:
                k = p[0] * 3
                pixels.append((palette[k], palette[k + 1], palette[k + 2]))
            elif channels <= 2:
                pixels.append((p[0], p[0], p[0]))
            else:
                pixels.append((p[0], p[1], p[2]))
    return width, height, pixels


def encode_png(width: int, height: int, pixels: list[tuple[int, int, int]]) -> bytes:
    """RGB pixels → PNG (for tests and stand-ins)."""
    raw = bytearray()
    for y in range(height):
        raw.append(0)
        for r, g, b in pixels[y * width:(y + 1) * width]:
            raw += bytes((r, g, b))

    def chunk(kind: bytes, body: bytes) -> bytes:
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(bytes(raw))) + chunk(b"IEND", b"")


# ---------------------------------------------------------------- checks


def luminance(p: tuple[int, int, int]) -> float:
    return 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]


def region_mean(width, height, pixels, y0: float, y1: float, x0: float = 0.0, x1: float = 1.0):
    """Mean RGB of the region given in fractions of the picture."""
    xs = range(int(x0 * width), max(int(x1 * width), int(x0 * width) + 1))
    ys = range(int(y0 * height), max(int(y1 * height), int(y0 * height) + 1))
    total = [0.0, 0.0, 0.0]
    n = 0
    for y in ys:
        row = y * width
        for x in xs:
            p = pixels[row + x]
            total[0] += p[0]
            total[1] += p[1]
            total[2] += p[2]
            n += 1
    return tuple(t / max(n, 1) for t in total)


def grid(width, height, pixels, cells=GRID):
    """Mean RGB per cell, rows top first."""
    cw, ch = cells
    return [[region_mean(width, height, pixels, j / ch, (j + 1) / ch, i / cw, (i + 1) / cw)
             for i in range(cw)] for j in range(ch)]


# The hotbar and hand: bottom rows, and the centre crosshair cell; not compared.
def hud_masked(j: int, i: int, cells=GRID) -> bool:
    cw, ch = cells
    return j >= ch - 2 or (j == ch // 2 and i in (cw // 2 - 1, cw // 2))


def similarity(a, b, cells=GRID) -> float:
    """Mean per-cell colour difference in 0…255 between two grids (lower is closer), HUD masked."""
    diffs = []
    for j in range(cells[1]):
        for i in range(cells[0]):
            if hud_masked(j, i, cells):
                continue
            pa, pb = a[j][i], b[j][i]
            diffs.append(sum(abs(x - y) for x, y in zip(pa, pb)) / 3)
    return sum(diffs) / max(len(diffs), 1)


def health(width, height, pixels) -> dict:
    """Automatic checks on one picture; `problems` lists what looks broken."""
    lum = [luminance(p) for p in pixels]
    n = len(lum)
    mean = sum(lum) / n
    std = (sum((v - mean) ** 2 for v in lum) / n) ** 0.5
    sky = region_mean(width, height, pixels, 0.0, 0.25)
    ground = region_mean(width, height, pixels, 0.6, 0.85)
    sky_ground = sum(abs(a - b) for a, b in zip(sky, ground)) / 3
    magenta = sum(1 for r, g, b in pixels if r > 180 and b > 180 and g < 80) / n
    # NaN in a pass usually ends as pure black or pure white blocks in a lit scene.
    black = sum(1 for v in lum if v < 2) / n
    white = sum(1 for r, g, b in pixels if r > 252 and g > 252 and b > 252) / n
    problems = []
    if mean < 4:
        problems.append("black")
    elif std < 3:
        problems.append("uniform")
    if sky_ground < 6:
        problems.append("sky = ground")
    if magenta > 0.05:
        problems.append(f"magenta {magenta:.0%}")
    if black > 0.4 and mean >= 4:
        problems.append(f"black areas {black:.0%}")
    if white > 0.4:
        problems.append(f"white areas {white:.0%}")
    return {
        "mean": round(mean, 1),
        "std": round(std, 1),
        "sky": [round(v) for v in sky],
        "ground": [round(v) for v in ground],
        "sky_ground": round(sky_ground, 1),
        "magenta": round(magenta, 3),
        "black": round(black, 3),
        "white": round(white, 3),
        "problems": problems,
    }


# ---------------------------------------------------------------- runs


def interesting(line: str) -> bool:
    low = line.lower()
    return any(k in low for k in ("shader pack", "glsl", "panic", "error", "wgpu", "validation"))


def run_one(pack: Path | None, out: Path, timeout: int, extra: list[str]) -> dict:
    """Runs the scene with `pack` (None: Horizon's own rendering); returns what happened."""
    start = time.time()
    before = {p for p in Path(tempfile.gettempdir()).iterdir() if p.name.startswith("tmp.")}
    args = [str(SHOT), str(pack) if pack else "", str(out), *extra]
    if pack is None:
        # pack_shot.sh always passes --shader-pack: call shot.sh for the plain picture.
        env_res = os.environ.get("RES", str(MAIN / ".context/wt/prep/.local/horizon-client/26.3"))
        args = [str(SHOT.parent / "shot.sh"), str(out), *extra]
        env = dict(os.environ, RES=env_res, W=os.environ.get("W", "480"), H=os.environ.get("H", "270"),
                   WAIT=os.environ.get("WAIT", "25"))
    else:
        env = dict(os.environ)
    try:
        proc = subprocess.run(args, capture_output=True, text=True, timeout=timeout, env=env)
        output, code = proc.stdout + proc.stderr, proc.returncode
    except subprocess.TimeoutExpired as e:
        output = (e.stdout or b"").decode(errors="replace") if isinstance(e.stdout, bytes) else (e.stdout or "")
        output += f"\nTIMEOUT after {timeout}s"
        code = -1
    # The game's own log: the newest fresh data dir of this run that has one.
    log_lines: list[str] = []
    after = [p for p in Path(tempfile.gettempdir()).iterdir() if p.name.startswith("tmp.") and p not in before]
    logs = [d / "logs" / "latest.log" for d in after if (d / "logs" / "latest.log").is_file()]
    # Other games may run at the same time: prefer the log that names this pack.
    logs.sort(key=lambda p: (pack is not None and pack.name in p.read_text(errors="replace"),
                             p.stat().st_mtime), reverse=True)
    if logs:
        log_lines = logs[0].read_text(errors="replace").splitlines()
    return {
        "seconds": round(time.time() - start),
        "code": code,
        "output": output,
        "log_tail": log_lines[-40:],
        "log_pack": [l for l in log_lines if interesting(l)][:60],
        "shot": out.is_file(),
    }


def run_iris(pack: Path | None, out: Path, timeout: int, extra: list[str]) -> dict:
    """The oracle's world and poses: `out` is the noon picture, `<stem>-dusk.png` and
    `<stem>-underwater.png` beside it."""
    import shutil

    start = time.time()
    home = Path(tempfile.mkdtemp(prefix="hzrender."))
    env = dict(os.environ, HORIZON_TELEMETRY_URL="off", HORIZON_LAUNCHER_NO_SELF_UPDATE="1", HORIZON_NO_AUDIO="1",
               VK_ICD_FILENAMES=os.environ.get("VK_ICD_FILENAMES", "/usr/share/vulkan/icd.d/lvp_icd.x86_64.json"))
    for var in ("HORIZON_HOME", "HORIZON_LAUNCHER_HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME"):
        d = home / var.lower()
        d.mkdir()
        env[var] = str(d)
    shutil.copytree(IRIS_WORLD, home / "horizon_home" / "saves" / "scene")
    sock = str(home / "ctl.sock")
    deadline = start + timeout
    output: list[str] = []

    def ctl(*args: str, limit: float = 120) -> str:
        left = max(5.0, min(limit, deadline - time.time()))
        try:
            p = subprocess.run([str(CTL), "--socket", sock, *args], capture_output=True, text=True,
                               timeout=left, env=env, cwd=env["HORIZON_HOME"])
            text = (p.stdout + p.stderr).strip()
        except subprocess.TimeoutExpired:
            text = f"TIMEOUT: {' '.join(args)}"
        output.append(f"$ {' '.join(args)[:120]}\n{text[:2000]}")
        return text

    launch = ["launch", "--world", "scene", "--offscreen", "--wait", "--game", str(GAME), "--resources", RES,
              "--width", str(SIZE[0]), "--height", str(SIZE[1]), "--render-distance", "6"]
    if pack:
        launch += ["--shader-pack", str(pack)]
    code = 0
    reply = ctl(*launch, *extra, limit=400)
    if '"ok":true' not in reply.replace(" ", "") and "error" in reply.lower():
        code = 1
    else:
        ctl("wait-ready", "300", limit=320)
        ctl("key", "f1", "tap")
        poses = os.environ.get("POSES", "noon,dusk,underwater").split(",")
        for name, commands, (x, y, z), yaw, pitch, settle in IRIS_POSES:
            if name not in poses:
                continue
            for c in commands:
                ctl("command", c)
            ctl("tp", str(x), str(y), str(z))
            ctl("look", str(yaw), str(pitch))
            time.sleep(settle)
            target = out if name == "noon" else out.with_name(f"{out.stem}-{name}.png")
            ctl("screenshot", str(target), limit=180)
            if time.time() > deadline:
                code = -1
                break
    logs = ctl("logs", "--lines", "2000")
    ctl("quit", limit=60)
    # A game too slow to answer is still running: stop it.
    try:
        pid = int(json.loads(reply).get("pid") or json.loads(reply).get("launch", {}).get("pid"))
    except (ValueError, TypeError, AttributeError):
        pid = None
    if pid:
        try:
            os.kill(pid, 0)
            os.kill(pid, 15)
            output.append(f"game {pid} did not quit: stopped")
        except OSError:
            pass
    log_lines = logs.splitlines()
    shutil.rmtree(home, ignore_errors=True)
    return {
        "seconds": round(time.time() - start),
        "code": code,
        "output": "\n".join(output),
        "log_tail": log_lines[-40:],
        "log_pack": [l for l in log_lines if interesting(l)][:60],
        "shot": out.is_file(),
    }


def find_packs(names: list[str]) -> list[Path]:
    if names:
        return [Path(n) if Path(n).exists() else CORPUS / "packs" / n for n in names]
    packs = CORPUS / "packs"
    return sorted(p for p in packs.iterdir() if p.suffix == ".zip" or (p.is_dir() and (p / "shaders").is_dir()))


def pack_name(p: Path) -> str:
    return p.stem if p.suffix == ".zip" else p.name


def iris_reference(name: str, scene: str = "noon") -> Path | None:
    for candidate in (IRIS_DIR / f"{name}-{scene}.png", IRIS_DIR / f"{name}-{SCENE}.png", IRIS_DIR / f"{name}.png"):
        if candidate.is_file():
            return candidate
    return None


def load_grid(path: Path):
    w, h, px = decode_png(path.read_bytes())
    return w, h, px, grid(w, h, px)


def verdict(r: dict) -> str:
    if not r["shot"]:
        return "Crash" if r["code"] not in (0, -1) else ("Timeout" if r["code"] == -1 else "No picture")
    problems = r["health"]["problems"]
    if "black" in problems:
        return "Black"
    if r.get("vanilla_diff") is not None and r["vanilla_diff"] < 4:
        return "Looks vanilla"
    if problems:
        return "Suspect"
    return "Draws"


def report_lines(r: dict) -> list[str]:
    """The shader-pack lines worth reading: the game log's, else the script's output."""
    lines = [l for l in r["log_pack"] if "shader pack" in l.lower() or "panic" in l.lower()]
    if not lines:
        lines = [l for l in r["output"].splitlines() if interesting(l)]
    return lines


def cell(text: str, limit: int = 220) -> str:
    text = " ".join(text.split()).replace("|", "\\|")
    return text if len(text) <= limit else text[:limit - 1] + "…"


def scenes_cell(r: dict) -> str:
    parts = []
    for scene, e in (r.get("scenes") or {}).items():
        bits = ([f"Iris {e['iris_diff']:.0f}"] if e.get("iris_diff") is not None else []) + e["problems"]
        parts.append(f"{scene}: {', '.join(bits) or 'ok'}")
    return "; ".join(parts)


def write_outputs(run_dir: Path, results: list[dict], md: Path | None) -> None:
    now = datetime.date.today().isoformat()
    lines = [
        f"# Shader pack render check ({now})",
        "",
        f"Scene `{SCENE}` ({'the Iris oracle world, noon pose; dusk and underwater in their column' if SCENE == 'iris' else SHOT.name}, {SIZE[0]}×{SIZE[1]}); one game at a time. Checks: black (mean luminance < 4), uniform "
        "(stddev < 3), sky = ground (top quarter vs 60–85% band), magenta, black/white areas; "
        "`vs Iris` / `vs vanilla`: mean per-cell colour difference on a 16×9 grid, HUD masked (0 = same, "
        "lower is closer).",
        "",
        "| pack | verdict | s | mean | std | sky–ground | vs Iris | vs vanilla | problems | dusk / underwater | report / log |",
        "|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for r in results:
        h = r.get("health") or {}
        fmt = lambda v: "" if v is None else f"{v:.0f}"
        note = report_lines(r)
        if r["verdict"] in ("Crash", "Timeout", "No picture", "Black"):
            note = note or r["log_tail"][-6:]
        lines.append(
            f"| {r['pack']} | **{r['verdict']}** | {r['seconds']} | {h.get('mean', '')} | {h.get('std', '')} | "
            f"{h.get('sky_ground', '')} | {fmt(r.get('iris_diff'))} | {fmt(r.get('vanilla_diff'))} | "
            f"{', '.join(h.get('problems', []))} | {scenes_cell(r)} | {cell(' ⏎ '.join(note[:4]))} |"
        )
    text = "\n".join(lines) + "\n"
    details = ["", "## Details", ""]
    for r in results:
        if r["verdict"] == "Draws" and not report_lines(r):
            continue
        details += [f"### {r['pack']}: {r['verdict']}", "", "```"]
        details += (report_lines(r) or [])[:20]
        if r["verdict"] in ("Crash", "Timeout", "No picture", "Black"):
            details += ["--- log tail ---", *r["log_tail"][-25:], "--- script output (tail) ---",
                        *r["output"].splitlines()[-15:]]
        details += ["```", ""]
    full = text + "\n".join(details)
    (run_dir / "summary.md").write_text(full)
    if md:
        md.write_text(full)
    slim = [{k: v for k, v in r.items() if k not in ("output",)} for r in results]
    (run_dir / "summary.json").write_text(json.dumps(slim, indent=1))
    def img(src: str, alt: str) -> str:
        return f'<img src="{html.escape(src)}" alt="{alt}">' if src else "<i>none</i>"

    def scene_cells(name: str) -> str:
        cells = ""
        for sc in ("dusk", "underwater"):
            mine = f"{name}-{sc}.png"
            ref = iris_reference(name, sc)
            iris = os.path.relpath(ref, run_dir) if ref and ref.name.endswith(f"-{sc}.png") else ""
            cells += f"<td>{img(mine if (run_dir / mine).is_file() else '', sc)}</td><td>{img(iris, 'Iris')}</td>"
        return cells

    rows = []
    for r in results:
        vanilla = "_vanilla.png" if (run_dir / "_vanilla.png").is_file() else ""
        rows.append(
            f"<tr><td><b>{html.escape(r['pack'])}</b><br>{html.escape(r['verdict'])}<br>"
            f"{html.escape(', '.join((r.get('health') or {}).get('problems', [])))}</td>"
            f"<td>{img(r['pack'] + '.png' if r['shot'] else '', 'Horizon')}</td>"
            f"<td>{img(r.get('iris_src', ''), 'Iris')}</td><td>{img(vanilla, 'vanilla')}</td>"
            f"{scene_cells(r['pack'])}"
            f"<td><pre>{html.escape(chr(10).join(report_lines(r)[:8]))}</pre></td></tr>"
        )
    (run_dir / "index.html").write_text(
        "<!doctype html><meta charset=utf-8><title>Shader pack renders</title>"
        "<style>body{font:13px sans-serif}img{width:480px}td{vertical-align:top;padding:4px}"
        "pre{white-space:pre-wrap;max-width:420px;font-size:11px}</style>"
        f"<h1>Shader pack renders ({now})</h1><table><tr><th>pack</th><th>Horizon</th><th>Iris</th>"
        "<th>Horizon, no pack</th><th>dusk</th><th>Iris dusk</th><th>underwater</th><th>Iris underwater</th>"
        "<th>report</th></tr>" + "".join(rows) + "</table>"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("packs", nargs="*", help="pack zips or folders, or names in the corpus")
    parser.add_argument("--run", default=datetime.datetime.now().strftime("%Y%m%d-%H%M"))
    parser.add_argument("--md", help="also write the summary here")
    parser.add_argument("--timeout", type=int, default=900, help="seconds per game")
    parser.add_argument("--no-vanilla", action="store_true", help="skip the picture without a pack")
    parser.add_argument("--reuse", action="store_true", help="keep pictures already in the run folder")
    parser.add_argument("--extra", action="append", default=[], help="extra game argument (repeatable)")
    args = parser.parse_args()
    run_dir = CORPUS / "render" / args.run
    run_dir.mkdir(parents=True, exist_ok=True)
    xvfb = None
    if not os.environ.get("DISPLAY") and not os.environ.get("WAYLAND_DISPLAY"):
        # The game opens a (hidden) window even offscreen: a private X server.
        display = os.environ.get("RENDER_DISPLAY", ":93")
        xvfb = subprocess.Popen(["Xvfb", display, "-screen", "0", "1280x720x24", "-nolisten", "tcp"],
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        os.environ["DISPLAY"] = display
        time.sleep(1)
    try:
        return render_all(args, run_dir)
    finally:
        if xvfb:
            xvfb.terminate()


def run(pack: Path | None, out: Path, timeout: int, extra: list[str]) -> dict:
    return (run_iris if SCENE == "iris" else run_one)(pack, out, timeout, extra)


def render_all(args: argparse.Namespace, run_dir: Path) -> int:
    vanilla_grid = None
    vanilla = run_dir / "_vanilla.png"
    if not args.no_vanilla:
        if not (args.reuse and vanilla.is_file()):
            print("vanilla …", flush=True)
            run(None, vanilla, args.timeout, args.extra)
        if vanilla.is_file():
            vanilla_grid = load_grid(vanilla)[3]
    results = []
    for pack in find_packs(args.packs):
        name = pack_name(pack)
        out = run_dir / f"{name}.png"
        saved = run_dir / f"{name}.json"
        if args.reuse and saved.is_file():
            r = json.loads(saved.read_text())
        else:
            if out.exists():
                out.unlink()
            print(f"{name} …", flush=True)
            r = run(pack, out, args.timeout, args.extra)
            r["pack"] = name
            (run_dir / f"{name}.log").write_text(
                r["output"] + "\n--- game log (shader pack, errors) ---\n" + "\n".join(r["log_pack"])
                + "\n--- game log tail ---\n" + "\n".join(r["log_tail"]) + "\n")
        r["pack"] = name
        r["health"] = None
        if r["shot"]:
            try:
                w, h, px, g = load_grid(out)
                r["health"] = health(w, h, px)
                if vanilla_grid:
                    r["vanilla_diff"] = round(similarity(g, vanilla_grid), 1)
                ref = iris_reference(name)
                if ref:
                    r["iris_diff"] = round(similarity(g, load_grid(ref)[3]), 1)
                    r["iris_src"] = os.path.relpath(ref, run_dir)
                # The other poses of the iris scene, each against its own reference.
                r["scenes"] = {}
                for scene in ("dusk", "underwater"):
                    shot = run_dir / f"{name}-{scene}.png"
                    if not shot.is_file():
                        continue
                    sw, sh, spx, sg = load_grid(shot)
                    entry = {"problems": health(sw, sh, spx)["problems"]}
                    ref = iris_reference(name, scene)
                    if ref and ref.name.endswith(f"-{scene}.png"):
                        entry["iris_diff"] = round(similarity(sg, load_grid(ref)[3]), 1)
                    r["scenes"][scene] = entry
            except (ValueError, KeyError, zlib.error) as e:
                r["shot"] = False
                r["output"] += f"\nunreadable screenshot: {e}"
        r["verdict"] = verdict(r)
        saved.write_text(json.dumps(r))
        print(f"  {r['verdict']} in {r['seconds']} s {', '.join((r['health'] or {}).get('problems', []))}",
              flush=True)
        results.append(r)
        write_outputs(run_dir, results, Path(args.md) if args.md else None)
    print(run_dir / "index.html")
    return 0


if __name__ == "__main__":
    sys.exit(main())
