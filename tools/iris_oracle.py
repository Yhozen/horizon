#!/usr/bin/env python3
"""Iris as a black-box reference renderer: run Minecraft 26.3 + Sodium + Iris on a shader pack and grab
a screenshot from the Xvfb display.

The script only downloads and runs the mods; it never opens their jars. Everything it fetches lives in
`.local/iris-oracle/` (never committed). The vanilla client jar and its libraries come from an existing
Minecraft folder (`--mcdir`, default `.context/mcdir`).

    python3 -I tools/iris_oracle.py prepare      # Fabric, Sodium 0.9.2, Iris 1.11.7, assets without sounds
    python3 -I tools/iris_oracle.py world        # Horizon builds the scene save (.local/iris-oracle/worlds/scene)
    python3 -I tools/iris_oracle.py run --pack PACK.zip [--options PACK.txt] --world SAVE --out OUT.png
    python3 -I tools/iris_oracle.py decode --png OUT.png [--fsh tools/shader_probe_packs/macros/shaders/final.fsh]

`run` writes OUT.png (hotbar and crosshair masked) and OUT.log (the game log). On Xvfb the game needs EGL
(`SDL_VIDEO_FORCE_EGL=1`) and the OpenGL back end (`preferredGraphicsBackend:"opengl"`); both are set.
A run takes about 3 minutes on llvmpipe (start ~1.5 min, spawn ~40 s, terrain ~30 s).
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
    "sodium": "mc26.3-0.9.2-fabric",  # beta.5 declares it breaks Iris <= 1.12.0
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
        "guiScale": str(GUI_SCALE),
        "joinedFirstServer": "true",
        "skipMultiplayerWarning": "true",
        "narrator": "0",
        "soundCategory_master": "0.0",
        "maxFps": "30",
        "enableVsync": "false",
        # Iris needs OpenGL; 26.3 falls back to Vulkan when the GL window fails.
        "preferredGraphicsBackend": '"opengl"',
    }
    for kv in extra:
        k, _, v = kv.partition(":")
        lines[k] = v
    (game_dir / "options.txt").write_text("".join(f"{k}:{v}\n" for k, v in lines.items()))


def offline_uuid(name: str) -> str:
    """Java's `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)`: an MD5 version-3 UUID with no namespace."""
    h = bytearray(hashlib.md5(f"OfflinePlayer:{name}".encode()).digest())
    h[6] = (h[6] & 0x0F) | 0x30
    h[8] = (h[8] & 0x3F) | 0x80
    return str(uuid.UUID(bytes=bytes(h)))


def has_loading_bar(png: Path) -> bool:
    """Whether the picture shows the "Loading terrain..." screen: a long run of pure green pixels on one row
    in the upper half (the progress bar)."""
    w, h, raw = read_png_rgb(png)
    for y in range(h // 6, h // 2):
        run = 0
        for x in range(w // 6, w - w // 6):
            i = (y * w + x) * 3
            if raw[i] < 60 and raw[i + 1] > 200 and raw[i + 2] < 60:
                run += 1
                if run > w // 4:
                    return True
            else:
                run = 0
    return False


GUI_SCALE = 2


def mask_hud(png: Path, w: int, h: int) -> None:
    """Paints the hotbar and the crosshair black (no way to hide the HUD without input injection).

    Vanilla's hotbar is 182x22 GUI pixels (24 high with the selection frame) centred at the bottom, the
    crosshair 15x15 at the centre; `options.txt` pins the GUI scale to `GUI_SCALE`."""
    s = GUI_SCALE
    hw, hh = 184 * s, 24 * s
    cs = 16 * s
    boxes = [((w - hw) // 2, h - hh, hw, hh), ((w - cs) // 2, (h - cs) // 2, cs, cs)]
    filt = ",".join(f"drawbox=x={x}:y={y}:w={bw}:h={bh}:color=black:t=fill" for x, y, bw, bh in boxes)
    tmp = png.with_suffix(".mask.png")
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", str(png), "-vf", filt, str(tmp)], check=True)
    tmp.replace(png)


def free_display() -> int:
    for n in range(91, 140):
        if not Path(f"/tmp/.X11-unix/X{n}").exists() and not Path(f"/tmp/.X{n}-lock").exists():
            return n
    raise SystemExit("no free X display")


class XInput:
    """Keyboard input on an X display through XTest (libXtst): opens the chat, types commands, presses F1.

    The game sees ordinary key presses, so no mod or driver is needed."""

    def __init__(self, display: str):
        import ctypes

        self.ct = ctypes
        self.x = ctypes.CDLL("libX11.so.6")
        self.xt = ctypes.CDLL("libXtst.so.6")
        self.x.XOpenDisplay.restype = ctypes.c_void_p
        self.x.XOpenDisplay.argtypes = [ctypes.c_char_p]
        self.x.XStringToKeysym.restype = ctypes.c_ulong
        self.x.XStringToKeysym.argtypes = [ctypes.c_char_p]
        self.x.XKeysymToKeycode.restype = ctypes.c_ubyte
        self.x.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        self.x.XKeycodeToKeysym.restype = ctypes.c_ulong
        self.x.XKeycodeToKeysym.argtypes = [ctypes.c_void_p, ctypes.c_ubyte, ctypes.c_int]
        self.x.XFlush.argtypes = [ctypes.c_void_p]
        self.xt.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        self.xt.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
        self.d = self.x.XOpenDisplay(display.encode())
        if not self.d:
            raise RuntimeError(f"cannot open X display {display}")

    NAMES = {" ": "space", "/": "slash", "_": "underscore", ":": "colon", ".": "period", "-": "minus",
             "@": "at", "~": "asciitilde", "=": "equal", "[": "bracketleft", "]": "bracketright"}

    def _tap(self, keysym_name: str) -> None:
        sym = self.x.XStringToKeysym(keysym_name.encode())
        code = self.x.XKeysymToKeycode(self.d, sym)
        if not code:
            raise RuntimeError(f"no key for {keysym_name!r}")
        shift = self.x.XKeycodeToKeysym(self.d, code, 0) != sym
        shift_code = self.x.XKeysymToKeycode(self.d, self.x.XStringToKeysym(b"Shift_L"))
        if shift:
            self.xt.XTestFakeKeyEvent(self.d, shift_code, 1, 0)
        self.xt.XTestFakeKeyEvent(self.d, code, 1, 0)
        self.xt.XTestFakeKeyEvent(self.d, code, 0, 0)
        if shift:
            self.xt.XTestFakeKeyEvent(self.d, shift_code, 0, 0)
        self.x.XFlush(self.d)
        time.sleep(0.04)

    def key(self, name: str) -> None:
        self._tap(name)

    def center_pointer(self, w: int, h: int) -> None:
        self.xt.XTestFakeMotionEvent(self.d, -1, w // 2, h // 2, 0)
        self.x.XFlush(self.d)

    def command(self, text: str) -> None:
        """Types `/text` in the chat and sends it ('/' opens the chat with the slash typed)."""
        self._tap("slash")
        time.sleep(1.5)
        for ch in text.lstrip("/"):
            self._tap(self.NAMES.get(ch, ch))
        time.sleep(0.5)
        self._tap("Return")
        time.sleep(1.0)


def scene_commands(scene: str, pose: dict) -> list[str]:
    """The commands that set up one named scene; the same lines replay it in Horizon (see `pose_notes`)."""
    x, y, z = pose["x"], pose["y"], pose["z"]
    def tp(px, py, pz, pitch=None):
        return f"/tp Player {px} {py} {pz} {pose['yaw']} {pose['pitch'] if pitch is None else pitch}"

    if scene == "noon":
        return ["/time set 6000", tp(x, y, z)]
    if scene == "dusk":
        return ["/time set 13000", tp(x, y, z)]
    if scene == "underwater":
        # Deepen the pool by three blocks (air only) and stand on its sand floor, eyes ~2.4 below the
        # surface, looking level at the wall.
        return ["/time set 6000",
                f"/fill {x - 6} {y} {z - 14} {x + 6} {y + 2} {z - 3} minecraft:water replace minecraft:air",
                tp(x, y - 1, z - 5, pitch=0)]
    raise SystemExit(f"unknown scene {scene!r} (noon, dusk, underwater)")


SCENE_SETTLE = {"noon": 8, "dusk": 10, "underwater": 25}


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
    pack_name = Path(args.pack).absolute().name  # a symlinked folder keeps its link's name
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
        # Horizon's single player is "Player"; same name, same offline UUID, so the save's player data loads.
        "auth_player_name": "Player",
        "version_name": MC,
        "game_directory": str(game_dir),
        "assets_root": str(HOME / "assets"),
        "assets_index_name": version["assetIndex"]["id"],
        "auth_uuid": offline_uuid("Player").replace("-", ""),
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
    jvm = [f"-Xmx{args.xmx}", "-Xms256M"]
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
        # Xvfb's GLX has no sRGB-capable visual, so SDL's GLX path fails ("Couldn't find matching GLX
        # visual") and the game falls back to Vulkan, which Iris cannot use. EGL on X11 works.
        SDL_VIDEO_FORCE_EGL="1",
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

    def grab(path: Path) -> bool:
        r = subprocess.run(
            ["ffmpeg", "-loglevel", "error", "-y", "-f", "x11grab", "-video_size", f"{w}x{h}", "-i", f":{display}",
             "-frames:v", "1", str(path)],
            check=False, env=env,
        )
        return r.returncode == 0 and path.exists()

    status = "timeout"
    shots: list[Path] = []
    try:
        deadline = time.time() + args.timeout
        loaded_at = None
        ready_at = None
        probe = out.with_suffix(".probe.png")
        while time.time() < deadline:
            if game_proc.poll() is not None:
                status = f"exited {game_proc.returncode}"
                break
            if loaded_at is None and "joined the game" in log_path.read_text(errors="replace"):
                loaded_at = time.time()
                print("player joined")
            now = time.time()
            # After the join the game shows "Loading terrain..." until the chunks around the player are
            # drawn (slow on llvmpipe); the screen has a bright green progress bar until then.
            if loaded_at and ready_at is None and now - loaded_at > 10:
                if grab(probe) and not has_loading_bar(probe):
                    ready_at = now
                    print(f"terrain shown {now - loaded_at:.0f}s after the join")
                else:
                    time.sleep(4)
            if ready_at and now - ready_at > args.wait:
                status = "ok"
                break
            time.sleep(1)
        probe.unlink(missing_ok=True)
        if status == "ok" and args.scenes:
            pose = json.loads((world / "oracle-pose.json").read_text())
            xin = XInput(f":{display}")
            xin.center_pointer(w, h)
            if args.hide_gui:
                xin.key("F1")
            for scene in args.scenes.split(","):
                for cmd in scene_commands(scene, pose):
                    xin.command(cmd)
                time.sleep(SCENE_SETTLE.get(scene, 10))
                path = out.with_name(f"{out.stem}-{scene}.png")
                if grab(path):
                    if args.mask_hud and not args.hide_gui:
                        mask_hud(path, w, h)
                    shots.append(path)
                if game_proc.poll() is not None:
                    status = f"exited {game_proc.returncode}"
                    break
        elif status in ("ok", "timeout") and grab(out):
            if args.mask_hud:
                mask_hud(out, w, h)
            shots.append(out)
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
        if "OpenGL debug message" in line or "refmap" in line:
            continue
        if "Using shaderpack" in line or "ERROR" in line or "Exception" in line or "compil" in line.lower():
            print(line[:300])
    print(status, *(shots or ["(no screenshot)"]))
    if not args.keep:
        shutil.rmtree(game_dir / "saves", ignore_errors=True)
    return 0 if status == "ok" and shots else 1


# ---------------------------------------------------------------- world


def build_world(args) -> int:
    """Builds the screenshot scene of `.context/run/shot.sh` with Horizon and copies the save out.

    The player stands at the viewpoint looking at it (yaw 180, pitch 25); daylight cycle off, time fixed.
    """
    ctl = MAIN / "target" / "fast" / "horizon-ctl"
    client = MAIN / "target" / "fast" / "horizon-client"
    res = Path(args.resources)
    out = Path(args.out).resolve()
    home = Path(tempfile.mkdtemp(prefix="hz-home-"))
    env = dict(os.environ)
    env.update(
        VK_ICD_FILENAMES="/usr/share/vulkan/icd.d/lvp_icd.x86_64.json",
        HORIZON_TELEMETRY_URL="off",
        HORIZON_HOME=str(home),
        HORIZON_LAUNCHER_HOME=tempfile.mkdtemp(),
        XDG_DATA_HOME=tempfile.mkdtemp(),
        XDG_CONFIG_HOME=tempfile.mkdtemp(),
        HORIZON_LAUNCHER_NO_SELF_UPDATE="1",
        HORIZON_NO_AUDIO="1",
    )
    sock = str(home / "ctl.sock")
    display = free_display()
    xvfb = subprocess.Popen(["Xvfb", f":{display}", "-screen", "0", "854x480x24", "-nolisten", "tcp"],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(1.5)
    env["DISPLAY"] = f":{display}"

    def c(*a: str, check: bool = True) -> str:
        r = subprocess.run([str(ctl), "--socket", sock, *a], env=env, cwd=home, capture_output=True, text=True, timeout=200)
        if check and r.returncode != 0:
            raise SystemExit(f"horizon-ctl {a[0]} failed: {r.stdout}{r.stderr}")
        return r.stdout

    c("launch", "--world", "scene", "--seed", str(args.seed), "--creative", "--offscreen", "--wait", "--game", str(client),
      "--resources", str(res), "--width", "854", "--height", "480", "--time", str(args.time), "--spawn-radius", "0",
      "--render-distance", "6")
    try:
        c("wait-ready", "180")
        # 26.3's names (advance_time, advance_weather, spawn_mobs); the old camelCase names are gone.
        for cmd in ("/gamerule advance_time false", "/gamerule advance_weather false", "/gamerule spawn_mobs false",
                    "/gamerule send_command_feedback false", f"/time set {args.time}", "/weather clear"):
            print(cmd, c("command", cmd, check=False).strip()[:200])
        state = json.loads(c("state"))
        p = state.get("player", state).get("position") or state["player"]["pos"]
        x, y, z = (int(v) for v in p[:3])
        print("player at", x, y, z)
        fills = [
            f"/fill {x-6} {y-1} {z-14} {x+6} {y-1} {z-3} minecraft:water",
            f"/fill {x-6} {y-2} {z-14} {x+6} {y-2} {z-3} minecraft:sand",
            f"/fill {x-6} {y} {z-16} {x+6} {y+4} {z-15} minecraft:stone_bricks",
            f"/fill {x+3} {y} {z-10} {x+5} {y+2} {z-8} minecraft:oak_leaves",
            f"/fill {x-4} {y} {z-12} {x-4} {y+3} {z-12} minecraft:glass",
            f"/setblock {x-2} {y} {z-2} minecraft:short_grass",
            f"/setblock {x+2} {y} {z-2} minecraft:poppy",
            f"/setblock {x-1} {y} {z-17} minecraft:torch",
        ]
        for f in fills:
            c("command", f)
        c("tp", str(x), str(y), str(z))
        c("look", "180", "25")
        time.sleep(4)
        if args.shot:
            c("screenshot", str(Path(args.shot).resolve()))
        c("command", "/save-all flush")
        time.sleep(3)
    finally:
        c("quit", check=False)
        time.sleep(5)
        xvfb.terminate()
    save = home / "saves" / "scene"
    if not (save / "level.dat").exists():
        raise SystemExit(f"no save at {save}: {list(home.rglob('level.dat'))}")
    if out.exists():
        shutil.rmtree(out)
    shutil.copytree(save, out)
    (out / "session.lock").unlink(missing_ok=True)
    # The pose `run --scenes` re-applies (and Horizon replays): feet position, yaw, pitch.
    pose = {"x": x, "y": y, "z": z, "yaw": 180, "pitch": 25, "time": args.time}
    (out / "oracle-pose.json").write_text(json.dumps(pose) + "\n")
    print("world", out, pose)
    return 0


# ---------------------------------------------------------------- decode


def read_png_rgb(png: Path) -> tuple[int, int, bytes]:
    """The picture as raw RGB bytes (through ffmpeg, so no image library is needed)."""
    probe = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0", str(png)],
        capture_output=True, text=True, check=True,
    )
    w, h = (int(x) for x in probe.stdout.strip().split(","))
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", str(png), "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
                         capture_output=True, check=True).stdout
    return w, h, raw


def decode_rows(png: Path, fsh: Path, cell: int = 8, bits: int = 24) -> list[tuple[str, int, bool, int | None]]:
    """Reads the rows painted by a probe pack's `final.fsh`: (name, scale, defined, value) per `// row` line."""
    import re

    rows = [(int(m[1]), m[2], int(m[3])) for m in re.finditer(r"// row (\d+): (.+?) x(\d+)$", fsh.read_text(), re.M)]
    w, _, raw = read_png_rgb(png)

    def px(x: int, y: int) -> tuple[int, int, int]:
        i = (y * w + x) * 3
        return raw[i], raw[i + 1], raw[i + 2]

    out = []
    for r, name, scale in rows:
        y = r * cell + cell // 2
        mr, mg, _ = px(cell // 2, y)
        defined = mg > 128 and mr < 128
        value = 0
        for b in range(bits):
            value = value * 2 + (1 if px((b + 1) * cell + cell // 2, y)[0] > 128 else 0)
        out.append((name, scale, defined, value if defined else None))
    return out


def decode(args) -> int:
    for name, scale, defined, value in decode_rows(Path(args.png), Path(args.fsh)):
        shown = "undefined" if not defined else (str(value) if scale == 1 else f"{value / scale:g}")
        print(f"{name:60} {shown}")
    return 0


# ---------------------------------------------------------------- index

# Log errors every run has (offline account, no Realms, no narrator), left out of summaries.
LOG_NOISE = ("Failed to fetch user properties", "Realms", "narrator", "Failed to fetch", "XDG_RUNTIME_DIR",
             "libflite", "sessionserver", "Couldn't find matching GLX", "Failed to create backend")


def summarize_log(log: Path) -> str:
    """One line: Iris's own errors and shader warnings in a run's log."""
    import re

    text = log.read_text(errors="replace")
    errors: list[str] = []
    warned: set[str] = set()
    for line in text.splitlines():
        m = re.search(r"Shader compilation log for (\S+?):", line)
        if m:
            warned.add(m[1])
            continue
        if re.search(r"/(ERROR|FATAL)\]", line) and not any(n in line for n in LOG_NOISE):
            msg = re.sub(r"^\[[^]]*\] \[[^]]*\]: ", "", line).strip()
            if msg not in errors:
                errors.append(msg[:160])
    parts = []
    if "Using shaderpack" not in text:
        parts.append("pack not loaded")
    if "FATAL ERROR" in text or "Game crashed" in text or "crash report" in text.lower():
        parts.append("game crashed")
    if errors:
        parts.append(f"{len(errors)} error(s): " + " | ".join(errors[:3]))
    if warned:
        parts.append(f"compile warnings in {len(warned)} program(s) ({', '.join(sorted(warned)[:4])}{', ...' if len(warned) > 4 else ''})")
    return "; ".join(parts) or "clean"


def index(args) -> int:
    d = Path(args.dir)
    rows = ["# Iris reference screenshots", "",
            "Iris 1.11.7 + Sodium 0.9.2, Minecraft 26.3, Mesa llvmpipe, 854x480, default pack options, HUD and "
            "hand hidden (F1). Made by `tools/iris_oracle.py run --scenes noon,dusk,underwater`.", "",
            "| Pack | Scene | File | Iris log (errors / warnings) |", "|---|---|---|---|"]
    for log in sorted(d.glob("*.log")):
        pack = log.stem
        summary = summarize_log(log).replace("|", "\\|")
        shots = sorted(d.glob(f"{pack}-*.png"), key=lambda p: ["noon", "dusk", "underwater"].index(p.stem.rsplit("-", 1)[1])
                       if p.stem.rsplit("-", 1)[1] in ("noon", "dusk", "underwater") else 9)
        if not shots:
            rows.append(f"| {pack} | - | (none) | {summary} |")
        for shot in shots:
            rows.append(f"| {pack} | {shot.stem.rsplit('-', 1)[1]} | `{shot.name}` | {summary} |")
    (d / "index.md").write_text("\n".join(rows) + "\n" + (Path(args.notes).read_text() if args.notes else ""))
    print(d / "index.md")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--mcdir", default=str(MCDIR))
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("prepare")
    ix = sub.add_parser("index", help="write index.md for a folder of run outputs")
    ix.add_argument("--dir", required=True)
    ix.add_argument("--notes", help="markdown appended to the index")
    de = sub.add_parser("decode", help="read the value rows a probe pack painted")
    de.add_argument("--png", required=True)
    de.add_argument("--fsh", default=str(ROOT / "tools" / "shader_probe_packs" / "macros" / "shaders" / "final.fsh"), help="the probe pack's final.fsh (its `// row` comments)")
    wo = sub.add_parser("world", help="build the scene save with Horizon")
    wo.add_argument("--out", default=str(HOME / "worlds" / "scene"))
    wo.add_argument("--resources", default=str(MAIN / ".context" / "wt" / "prep" / ".local" / "horizon-client" / MC))
    wo.add_argument("--seed", type=int, default=1)
    wo.add_argument("--time", type=int, default=6000)
    wo.add_argument("--shot", help="also save Horizon's own screenshot of the scene here")
    r = sub.add_parser("run")
    r.add_argument("--pack", required=True, help="shader pack zip or folder")
    r.add_argument("--options", help="the pack's option file (`<pack>.txt`, `NAME=value` lines)")
    r.add_argument("--world", required=True, help="save folder (copied)")
    r.add_argument("--out", required=True, help="screenshot path; the log goes beside it as .log")
    r.add_argument("--size", default="854x480")
    r.add_argument("--wait", type=float, default=15, help="seconds after the terrain is shown")
    r.add_argument("--timeout", type=float, default=600)
    r.add_argument("--name", help="run folder name under .local/iris-oracle/runs/")
    r.add_argument("--xmx", default="2G")
    r.add_argument("--gl-override", action="store_true", help="MESA_GL_VERSION_OVERRIDE=4.6")
    r.add_argument("--option", action="append", help="extra options.txt line `key:value`")
    r.add_argument("--env", action="append", help="extra environment `K=V`")
    r.add_argument("--game-arg", action="append", help="extra game argument")
    r.add_argument("--keep", action="store_true", help="keep the copied save")
    r.add_argument("--scenes", help="comma list of noon, dusk, underwater: typed as commands in one game, one "
                   "picture each (OUT-<scene>.png); needs <world>/oracle-pose.json")
    r.add_argument("--no-hide-gui", dest="hide_gui", action="store_false", help="with --scenes: do not press F1")
    r.add_argument("--no-mask-hud", dest="mask_hud", action="store_false", help="leave the hotbar and crosshair visible")
    args = ap.parse_args()
    if args.cmd == "prepare":
        prepare(args)
        return 0
    if args.cmd == "index":
        return index(args)
    if args.cmd == "decode":
        return decode(args)
    if args.cmd == "world":
        return build_world(args)
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
