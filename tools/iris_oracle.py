#!/usr/bin/env python3
"""Iris as a black-box reference renderer: run Minecraft 26.3 + Sodium + Iris on a shader pack and grab
a screenshot from the Xvfb display.

The script only downloads and runs the mods; it never opens their jars. Everything it fetches lives in
`.local/iris-oracle/` (never committed). The vanilla client jar and its libraries come from an existing
Minecraft folder (`--mcdir`, default `.context/mcdir`).

    python3 -I tools/iris_oracle.py prepare
    python3 -I tools/iris_oracle.py world --out .local/iris-oracle/worlds/scene
    python3 -I tools/iris_oracle.py run --pack PACK.zip [--options PACK.txt] --world SAVE --out OUT.png

`run` writes OUT.png and OUT.log (the game log) and prints the Iris lines of the log.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request
import uuid
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# Worktrees share the main checkout's `.local/` and `.context/`.
MAIN = Path(os.environ.get("HORIZON_MAIN", "/home/vercel-sandbox/horizon"))
HOME = MAIN / ".local" / "iris-oracle"
MCDIR = MAIN / ".context" / "mcdir"
UA = "horizon-iris-oracle/0.1"
MC = "26.3"
LOADER = "0.20.1"
MODS = {
    # slug: version_number on Modrinth
    "sodium": "mc26.3-0.9.3-beta.5-fabric",
    "iris": "1.11.7+26.3-fabric",
}
ASSET_HOST = "https://resources.download.minecraft.net/"


def fetch(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    for attempt in range(4):
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                return r.read()
        except Exception:
            if attempt == 3:
                raise
            time.sleep(1 + attempt * 2)
    raise AssertionError


def fetch_json(url: str):
    return json.loads(fetch(url))


def download(url: str, dest: Path, sha1: str | None = None, sha512: str | None = None) -> Path:
    """Downloads `url` to `dest` unless a file with the right checksum is already there."""

    def ok(data: bytes) -> bool:
        if sha1 and hashlib.sha1(data).hexdigest() != sha1:
            return False
        if sha512 and hashlib.sha512(data).hexdigest() != sha512:
            return False
        return True

    if dest.exists() and (sha1 or sha512) and ok(dest.read_bytes()):
        return dest
    if dest.exists() and not (sha1 or sha512):
        return dest
    data = fetch(url)
    if not ok(data):
        raise SystemExit(f"checksum mismatch for {url}")
    dest.parent.mkdir(parents=True, exist_ok=True)
    tmp = dest.with_suffix(dest.suffix + ".part")
    tmp.write_bytes(data)
    tmp.replace(dest)
    return dest


def maven_path(name: str) -> str:
    """`group:artifact:version[:classifier]` -> the Maven repository path."""
    parts = name.split(":")
    group, artifact, version = parts[0], parts[1], parts[2]
    classifier = f"-{parts[3]}" if len(parts) > 3 else ""
    return f"{group.replace('.', '/')}/{artifact}/{version}/{artifact}-{version}{classifier}.jar"


def rules_allow(rules) -> bool:
    """The launcher's rule check for Linux x86_64 with no optional features."""
    if not rules:
        return True
    allowed = False
    for rule in rules:
        os_rule = rule.get("os", {})
        if rule.get("features"):
            match = False
        else:
            match = (not os_rule.get("name") or os_rule["name"] == "linux") and (
                not os_rule.get("arch") or os_rule["arch"] in ("x86_64", "amd64")
            )
        if match:
            allowed = rule["action"] == "allow"
    return allowed


# ---------------------------------------------------------------- prepare


def prepare(args) -> None:
    HOME.mkdir(parents=True, exist_ok=True)
    mcdir = Path(args.mcdir)
    version = json.loads((mcdir / "versions" / MC / f"{MC}.json").read_text())

    # Fabric loader profile and its libraries.
    profile = fetch_json(f"https://meta.fabricmc.net/v2/versions/loader/{MC}/{LOADER}/profile/json")
    (HOME / "fabric-profile.json").write_text(json.dumps(profile, indent=1))
    for lib in profile["libraries"]:
        path = maven_path(lib["name"])
        download(lib["url"].rstrip("/") + "/" + path, HOME / "libraries" / path, sha1=lib.get("sha1"))
        print("fabric", lib["name"])

    # Sodium and Iris from Modrinth, plus any required dependency they declare.
    mods = HOME / "mods"
    mods.mkdir(exist_ok=True)
    wanted = dict(MODS)
    done: set[str] = set()
    while wanted:
        slug, number = wanted.popitem()
        if slug in done:
            continue
        q = urllib.parse.urlencode({"game_versions": json.dumps([MC]), "loaders": json.dumps(["fabric"])})
        versions = fetch_json(f"https://api.modrinth.com/v2/project/{slug}/version?{q}")
        v = next((v for v in versions if number is None or v["version_number"] == number), None)
        if v is None:
            raise SystemExit(f"{slug} {number} not on Modrinth for {MC}")
        f = next(f for f in v["files"] if f["primary"])
        download(f["url"], mods / f["filename"], sha512=f["hashes"]["sha512"])
        print("mod", f["filename"])
        done.add(slug)
        done.add(v["project_id"])
        for dep in v["dependencies"]:
            if dep["dependency_type"] == "required" and dep["project_id"] not in done:
                proj = fetch_json(f"https://api.modrinth.com/v2/project/{dep['project_id']}")
                if proj["slug"] not in done and proj["slug"] not in wanted:
                    wanted[proj["slug"]] = None
                    print("dependency", proj["slug"])

    # Mojang's assets, without sounds and music.
    idx = version["assetIndex"]
    index_path = HOME / "assets" / "indexes" / f"{idx['id']}.json"
    download(idx["url"], index_path, sha1=idx["sha1"])
    objects = json.loads(index_path.read_text())["objects"]
    todo = [
        o["hash"]
        for name, o in objects.items()
        if not (name.startswith("minecraft/sounds/") or name.endswith(".ogg") or name.startswith("minecraft/music"))
    ]
    print(f"assets: {len(todo)} of {len(objects)} objects")

    def one(h: str) -> None:
        download(ASSET_HOST + f"{h[:2]}/{h}", HOME / "assets" / "objects" / h[:2] / h, sha1=h)

    with ThreadPoolExecutor(8) as pool:
        for i, _ in enumerate(pool.map(one, todo)):
            if i % 500 == 0:
                print(f"  {i}/{len(todo)}")
    print("prepared", HOME)


# ---------------------------------------------------------------- run


def classpath(mcdir: Path, version: dict, profile: dict) -> list[str]:
    cp: list[str] = []
    seen: set[str] = set()
    # Fabric's libraries first; skip vanilla's copy of a library Fabric also brings (ASM).
    for lib in profile["libraries"]:
        group_artifact = ":".join(lib["name"].split(":")[:2])
        seen.add(group_artifact)
        cp.append(str(HOME / "libraries" / maven_path(lib["name"])))
    for lib in version["libraries"]:
        if not rules_allow(lib.get("rules")):
            continue
        parts = lib["name"].split(":")
        if len(parts) == 3 and ":".join(parts[:2]) in seen:
            continue
        art = lib.get("downloads", {}).get("artifact")
        if art:
            p = mcdir / "libraries" / art["path"]
            if not p.exists():
                raise SystemExit(f"missing vanilla library {p}")
            cp.append(str(p))
    cp.append(str(mcdir / "versions" / MC / f"{MC}.jar"))
    return cp


def flatten_args(items, subst: dict) -> list[str]:
    out: list[str] = []
    for item in items:
        if isinstance(item, str):
            vals = [item]
        else:
            if not rules_allow(item.get("rules")):
                continue
            vals = item["value"] if isinstance(item["value"], list) else [item["value"]]
        for v in vals:
            for k, s in subst.items():
                v = v.replace("${" + k + "}", s)
            out.append(v)
    return out


def write_options(game_dir: Path, extra: list[str]) -> None:
    lines = {
        "onboardAccessibility": "false",
        "tutorialStep": "none",
        "renderDistance": "6",
        "simulationDistance": "5",
        "fov": "0.0",
        "pauseOnLostFocus": "false",
        "guiScale": "2",
        "joinedFirstServer": "true",
        "skipMultiplayerWarning": "true",
        "narrator": "0",
        "soundCategory_master": "0.0",
        "maxFps": "30",
        "enableVsync": "false",
    }
    for kv in extra:
        k, _, v = kv.partition(":")
        lines[k] = v
    (game_dir / "options.txt").write_text("".join(f"{k}:{v}\n" for k, v in lines.items()))


def free_display() -> int:
    for n in range(91, 140):
        if not Path(f"/tmp/.X11-unix/X{n}").exists() and not Path(f"/tmp/.X{n}-lock").exists():
            return n
    raise SystemExit("no free X display")


def run(args) -> int:
    mcdir = Path(args.mcdir)
    version = json.loads((mcdir / "versions" / MC / f"{MC}.json").read_text())
    profile = json.loads((HOME / "fabric-profile.json").read_text())
    pack = Path(args.pack).resolve()
    world = Path(args.world).resolve()
    out = Path(args.out).resolve()
    out.parent.mkdir(parents=True, exist_ok=True)
    w, h = (int(x) for x in args.size.split("x"))
    name = args.name or f"{pack.stem}-{int(time.time())}"
    game_dir = HOME / "runs" / name
    if game_dir.exists():
        shutil.rmtree(game_dir)
    (game_dir / "mods").mkdir(parents=True)
    (game_dir / "shaderpacks").mkdir()
    (game_dir / "config").mkdir()
    (game_dir / "saves").mkdir()
    for jar in (HOME / "mods").glob("*.jar"):
        os.symlink(jar, game_dir / "mods" / jar.name)
    pack_name = pack.name
    if pack.is_dir():
        shutil.copytree(pack, game_dir / "shaderpacks" / pack_name)
    else:
        shutil.copy(pack, game_dir / "shaderpacks" / pack_name)
    if args.options:
        shutil.copy(args.options, game_dir / "shaderpacks" / f"{pack_name}.txt")
    iris_props = {"shaderPack": pack_name, "enableShaders": "true", "disableUpdateMessage": "true"}
    (game_dir / "config" / "iris.properties").write_text("".join(f"{k}={v}\n" for k, v in iris_props.items()))
    write_options(game_dir, args.option or [])
    shutil.copytree(world, game_dir / "saves" / world.name)

    natives = Path(tempfile.mkdtemp(prefix="iris-natives-"))
    subst = {
        "auth_player_name": "Oracle",
        "version_name": MC,
        "game_directory": str(game_dir),
        "assets_root": str(HOME / "assets"),
        "assets_index_name": version["assetIndex"]["id"],
        "auth_uuid": str(uuid.uuid3(uuid.NAMESPACE_OID, "OfflinePlayer:Oracle")).replace("-", ""),
        "auth_access_token": "0",
        "clientid": "0",
        "auth_xuid": "0",
        "version_type": "release",
        "natives_directory": str(natives),
        "launcher_name": "horizon-iris-oracle",
        "launcher_version": "0.1",
        "classpath": os.pathsep.join(classpath(mcdir, version, profile)),
        "resolution_width": str(w),
        "resolution_height": str(h),
        "quickPlaySingleplayer": world.name,
    }
    jvm = [f"-Xmx{args.xmx}", f"-Xms{args.xmx}" if False else "-Xms256M"]
    jvm += flatten_args(version["arguments"]["jvm"], subst)
    jvm += profile["arguments"]["jvm"]
    game = flatten_args(version["arguments"]["game"], subst)
    game += ["--width", str(w), "--height", str(h), "--quickPlaySingleplayer", world.name]
    game += args.game_arg or []
    cmd = ["java", *jvm, profile["mainClass"], *game]

    env = dict(os.environ)
    env.update(
        LIBGL_ALWAYS_SOFTWARE="1",
        GALLIUM_DRIVER="llvmpipe",
        VK_ICD_FILENAMES="/usr/share/vulkan/icd.d/lvp_icd.x86_64.json",
        SDL_AUDIODRIVER="dummy",
        ALSOFT_DRIVERS="null",
    )
    if args.gl_override:
        env.update(MESA_GL_VERSION_OVERRIDE="4.6", MESA_GLSL_VERSION_OVERRIDE="460")
    for kv in args.env or []:
        k, _, v = kv.partition("=")
        env[k] = v

    display = free_display()
    xvfb = subprocess.Popen(
        ["Xvfb", f":{display}", "-screen", "0", f"{w}x{h}x24", "-nolisten", "tcp"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    time.sleep(1.5)
    env["DISPLAY"] = f":{display}"
    log_path = out.with_suffix(".log")
    log = open(log_path, "wb")
    (game_dir / "command.txt").write_text(" ".join(cmd))
    game_proc = subprocess.Popen(cmd, cwd=game_dir, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    print(f"game pid {game_proc.pid} on :{display}, log {log_path}")

    status = "timeout"
    try:
        deadline = time.time() + args.timeout
        loaded_at = None
        while time.time() < deadline:
            if game_proc.poll() is not None:
                status = f"exited {game_proc.returncode}"
                break
            text = log_path.read_text(errors="replace")
            if loaded_at is None and ("joined the game" in text or "Loaded 0 advancements" in text and "Preparing spawn area" in text):
                loaded_at = time.time()
                print("world loaded")
            if loaded_at and time.time() - loaded_at > args.wait:
                status = "ok"
                break
            time.sleep(1)
        if status in ("ok", "timeout"):
            subprocess.run(
                ["ffmpeg", "-loglevel", "error", "-y", "-f", "x11grab", "-video_size", f"{w}x{h}", "-i", f":{display}", "-frames:v", "1", str(out)],
                check=False,
                env=env,
            )
    finally:
        try:
            os.killpg(game_proc.pid, signal.SIGTERM)
            game_proc.wait(15)
        except Exception:
            try:
                os.killpg(game_proc.pid, signal.SIGKILL)
            except Exception:
                pass
        xvfb.terminate()
        xvfb.wait(10)
        log.close()
        shutil.rmtree(natives, ignore_errors=True)
    for line in log_path.read_text(errors="replace").splitlines():
        if "iris" in line.lower() or "shader" in line.lower() or "ERROR" in line or "Exception" in line:
            print(line[:300])
    print(status, out if out.exists() else "(no screenshot)")
    if not args.keep:
        shutil.rmtree(game_dir / "saves", ignore_errors=True)
    return 0 if status == "ok" and out.exists() else 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--mcdir", default=str(MCDIR))
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("prepare")
    r = sub.add_parser("run")
    r.add_argument("--pack", required=True, help="shader pack zip or folder")
    r.add_argument("--options", help="the pack's option file (`<pack>.txt`, `NAME=value` lines)")
    r.add_argument("--world", required=True, help="save folder (copied)")
    r.add_argument("--out", required=True, help="screenshot path; the log goes beside it as .log")
    r.add_argument("--size", default="854x480")
    r.add_argument("--wait", type=float, default=60, help="seconds after the world loaded")
    r.add_argument("--timeout", type=float, default=600)
    r.add_argument("--name", help="run folder name under .local/iris-oracle/runs/")
    r.add_argument("--xmx", default="2G")
    r.add_argument("--gl-override", action="store_true", help="MESA_GL_VERSION_OVERRIDE=4.6")
    r.add_argument("--option", action="append", help="extra options.txt line `key:value`")
    r.add_argument("--env", action="append", help="extra environment `K=V`")
    r.add_argument("--game-arg", action="append", help="extra game argument")
    r.add_argument("--keep", action="store_true", help="keep the copied save")
    args = ap.parse_args()
    if args.cmd == "prepare":
        prepare(args)
        return 0
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
