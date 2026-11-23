#!/usr/bin/env python3
"""Vendor an openly licensed Minecraft-layout resource pack into content/open/packs/<name>/.

Copies only `assets/minecraft/textures/**.png` (and their `.mcmeta`) and writes `pack.json`:
the pinned source (URL, version, SHA-256 of the download), the author, the licence and the
SHA-256 of every vendored file. The pack is read as data: nothing in it is executed.

Files that look taken from Minecraft are left out and listed in `pack.json` under `excluded`,
with the reason. The comparison needs your own vanilla textures (`--vanilla`, for example
`.local/horizon-client/26.3/assets/minecraft/textures`); they are only read, never copied:

- `identical-bytes`: the same bytes as some vanilla texture (any name);
- `identical-pixels`: the same decoded pixels as some vanilla texture (any name);
- `shares-pixels`: at least SHARED (25%) of the visible pixels equal vanilla's file of that name;
- `recolour`: the visible pixels map colour for colour onto vanilla's file of that name
  (at least RECOLOUR, 90%, with eight or more vanilla colours);
- `trademark`: Mojang's names and logos (`gui/title/`, `*mojang*`), whoever drew them;
- `undecodable`: a PNG this script cannot decode, so it cannot be checked;
- `listed`: excluded by `--exclude`.

Only names vanilla has are kept (the generator takes textures by Minecraft's names); `--all-names`
keeps the pack's other files too (optional variants, other mods' layouts).

Example:

    python3 tools/vendor_open_pack.py --source /tmp/packs/pack.zip \\
        --name pixel_perfection_legacy --title "Pixel Perfection Legacy" \\
        --url https://cdn.modrinth.com/... --version 26.3-97.2 --sha256 <download sha256> \\
        --author "XSSheep; updated by Nova_Wostra" --license CC-BY-SA-4.0 \\
        --vanilla .local/horizon-client/26.3/assets/minecraft/textures
"""

import argparse
import fnmatch
import hashlib
import json
import shutil
import sys
import zipfile
import zlib
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PREFIX = "assets/minecraft/textures/"
SHARED = 0.25
RECOLOUR = 0.9
TRADEMARK = ["gui/title/*", "*mojang*"]


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def decode_png(data):
    """(width, height, RGBA bytes) of a non-interlaced PNG, or None."""
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        return None
    pos, idat, head, palette, trns = 8, [], None, None, None
    while pos + 8 <= len(data):
        size = int.from_bytes(data[pos:pos + 4], "big")
        kind, body = data[pos + 4:pos + 8], data[pos + 8:pos + 8 + size]
        if kind == b"IHDR":
            head = body
        elif kind == b"PLTE":
            palette = body
        elif kind == b"tRNS":
            trns = body
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
        pos += 12 + size
    if head is None or len(head) < 13:
        return None
    width, height = int.from_bytes(head[:4], "big"), int.from_bytes(head[4:8], "big")
    depth, colour, interlace = head[8], head[9], head[12]
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}.get(colour)
    if channels is None or interlace != 0 or depth not in (1, 2, 4, 8, 16):
        return None
    try:
        raw = zlib.decompress(b"".join(idat))
    except zlib.error:
        return None
    bits = channels * depth
    stride, bpp = (width * bits + 7) // 8, max(1, bits // 8)
    if len(raw) < height * (stride + 1):
        return None
    out, prev = bytearray(), bytearray(stride)
    for y in range(height):
        kind = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if kind == 1:
                line[i] = (line[i] + a) & 255
            elif kind == 2:
                line[i] = (line[i] + b) & 255
            elif kind == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif kind == 4:
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        prev = line
        if depth == 16:
            samples = [line[i] for i in range(0, len(line), 2)]
        elif depth == 8:
            samples = list(line)
        else:
            per = 8 // depth
            mask = (1 << depth) - 1
            samples = [(line[i // per] >> (8 - depth * (i % per + 1))) & mask for i in range(width * channels)]
            if colour != 3:
                samples = [s * 255 // mask for s in samples]
        for x in range(width):
            px = samples[x * channels:(x + 1) * channels]
            if colour == 0:
                out += bytes((px[0], px[0], px[0], 255))
            elif colour == 2:
                out += bytes((px[0], px[1], px[2], 255))
            elif colour == 3:
                if palette is None or px[0] * 3 + 3 > len(palette):
                    return None
                alpha = trns[px[0]] if trns is not None and px[0] < len(trns) else 255
                out += palette[px[0] * 3:px[0] * 3 + 3] + bytes((alpha,))
            elif colour == 4:
                out += bytes((px[0], px[0], px[0], px[1]))
            else:
                out += bytes(px)
    return width, height, bytes(out)


def pixels(image):
    return [image[2][i:i + 4] for i in range(0, len(image[2]), 4)]


def shared(a, b):
    """The share of visible pixels (in either image) that are equal."""
    visible = equal = 0
    for x, y in zip(pixels(a), pixels(b)):
        if x[3] or y[3]:
            visible += 1
            equal += x == y
    return equal / visible if visible else 0.0


def recolour(a, b):
    """How consistently each vanilla colour maps to one pack colour, and how many colours."""
    mapping, visible = defaultdict(Counter), 0
    for x, y in zip(pixels(a), pixels(b)):
        if x[3] or y[3]:
            visible += 1
            mapping[y][x] += 1
    if not visible:
        return 0.0, 0
    return sum(c.most_common(1)[0][1] for c in mapping.values()) / visible, len(mapping)


def read_source(source):
    """Pack-relative path to bytes, for every file under assets/minecraft/textures/."""
    files = {}
    if source.is_dir():
        base = source / PREFIX
        for path in sorted(base.rglob("*")):
            if path.is_file():
                files[PREFIX + path.relative_to(base).as_posix()] = path.read_bytes()
    else:
        with zipfile.ZipFile(source) as archive:
            for info in archive.infolist():
                name = info.filename
                if name.startswith(PREFIX) and not info.is_dir() and ".." not in name.split("/"):
                    files[name] = archive.read(info)
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--source", type=Path, required=True, help="the downloaded pack (zip) or its unpacked folder")
    parser.add_argument("--name", required=True, help="folder name under content/open/packs")
    parser.add_argument("--title", required=True)
    parser.add_argument("--url", required=True, help="the download URL")
    parser.add_argument("--page", default="", help="the project page")
    parser.add_argument("--version", required=True)
    parser.add_argument("--sha256", required=True, help="SHA-256 of the download (checked when --source is a zip)")
    parser.add_argument("--author", required=True)
    parser.add_argument("--license", required=True, help="SPDX id; its text must be in content/open/licenses")
    parser.add_argument("--evidence", action="append", default=[], help="where the licence is stated (repeatable)")
    parser.add_argument("--notice", type=Path, action="append", default=[],
                        help="a credit file from the pack to keep beside it (repeatable)")
    parser.add_argument("--vanilla", type=Path, required=True, help="your vanilla assets/minecraft/textures folder")
    parser.add_argument("--exclude", action="append", default=[], help="a glob under textures/ to leave out")
    parser.add_argument("--all-names", action="store_true", help="also keep names vanilla does not have")
    parser.add_argument("--out", type=Path, default=ROOT / "content/open/packs")
    args = parser.parse_args()

    if not args.source.is_dir() and sha256(args.source.read_bytes()) != args.sha256:
        sys.exit(f"{args.source}: sha256 differs from --sha256")
    if not (ROOT / "content/open/licenses" / f"{args.license}.txt").exists():
        sys.exit(f"content/open/licenses/{args.license}.txt is missing: vendor the licence text first")
    vanilla = {p.relative_to(args.vanilla).as_posix(): p.read_bytes() for p in args.vanilla.rglob("*.png")}
    if not vanilla:
        sys.exit(f"{args.vanilla}: no vanilla textures to compare with")
    vanilla_bytes = {sha256(b) for b in vanilla.values()}
    vanilla_images = {k: decode_png(b) for k, b in vanilla.items()}
    vanilla_pixels = {sha256(repr(i[:2]).encode() + i[2]) for i in vanilla_images.values() if i}

    files = read_source(args.source)
    kept, excluded = {}, defaultdict(list)
    for path in sorted(p for p in files if p.endswith(".png")):
        name = path[len(PREFIX):]
        if not args.all_names and name not in vanilla:
            continue
        data = files[path]
        image = decode_png(data)
        reason = None
        if any(fnmatch.fnmatch(name, g) for g in args.exclude):
            reason = "listed"
        elif any(fnmatch.fnmatch(name, g) for g in TRADEMARK):
            reason = "trademark"
        elif sha256(data) in vanilla_bytes:
            reason = "identical-bytes"
        elif image is None:
            reason = "undecodable"
        elif sha256(repr(image[:2]).encode() + image[2]) in vanilla_pixels:
            reason = "identical-pixels"
        else:
            theirs = vanilla_images.get(name)
            if theirs and theirs[:2] == image[:2]:
                score, colours = recolour(image, theirs)
                if shared(image, theirs) >= SHARED:
                    reason = "shares-pixels"
                elif score >= RECOLOUR and colours >= 8:
                    reason = "recolour"
        if reason:
            excluded[reason].append(name)
            continue
        kept[path] = data
        meta = path + ".mcmeta"
        if meta in files:
            kept[meta] = files[meta]

    target = args.out / args.name
    if target.exists():
        shutil.rmtree(target)
    for path, data in kept.items():
        out = target / path
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_bytes(data)
    notices = {}
    for notice in args.notice:
        data = notice.read_bytes()
        (target / notice.name).write_bytes(data)
        notices[notice.name] = sha256(data)
    manifest = {
        "schema": 1,
        "name": args.name,
        "title": args.title,
        "source": {"url": args.url, "page": args.page, "version": args.version, "sha256": args.sha256},
        "author": args.author,
        "license": args.license,
        "license_evidence": args.evidence,
        "transformation": "unchanged: copied byte for byte from the release",
        "notices": notices,
        "files": {path: sha256(data) for path, data in sorted(kept.items())},
        "excluded": {reason: sorted(names) for reason, names in sorted(excluded.items())},
    }
    (target / "pack.json").write_text(json.dumps(manifest, indent=1, ensure_ascii=False) + "\n")
    pngs = sum(1 for p in kept if p.endswith(".png"))
    print(f"{target}: {pngs} textures, {len(kept) - pngs} .mcmeta; excluded "
          + ", ".join(f"{len(v)} {k}" for k, v in sorted(excluded.items())))
    print(f"pack.json sha256 {sha256((target / 'pack.json').read_bytes())}")


if __name__ == "__main__":
    main()
