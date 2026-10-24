#!/usr/bin/env python3
"""Packs prepared resources (horizon-prepare's .local/horizon-client/<version>) into the
browser build's data archive (docs/web.md): repeated records of a little-endian u32 path
length, the UTF-8 path (relative, '/'-separated), a u32 data length and the data. The game
mounts it with horizon_core::fs::mount_archive.

The archive holds Mojang's game files: it stays in a gitignored folder (web/dist by default)
and is never committed or published.

  tools/web/pack.py [--resources DIR] [--out FILE] [--only LIST]

--only LIST keeps the files LIST names (one path per line, absolute or relative to the
resources), for example the files a desktop run opened (see docs/web.md); every top-level
file is kept as well. Without it, everything but the export logs and class dumps is packed.
"""
import argparse
import os
import struct
import sys

SKIP_DIRS = {"classes", "client-classes", "logs", "sounds"}


def wanted_files(root, only):
    if only is not None:
        keep = set()
        for line in open(only, encoding="utf-8"):
            path = line.strip()
            if not path:
                continue
            rel = os.path.relpath(path, root) if os.path.isabs(path) else path
            if not rel.startswith("..") and os.path.isfile(os.path.join(root, rel)):
                keep.add(rel.replace(os.sep, "/"))
        for name in os.listdir(root):
            if os.path.isfile(os.path.join(root, name)) and not name.endswith(".log"):
                keep.add(name)
        return sorted(keep)
    files = []
    for dirpath, dirnames, filenames in os.walk(root):
        rel_dir = os.path.relpath(dirpath, root)
        if rel_dir == ".":
            dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for name in filenames:
            if name.endswith(".log"):
                continue
            rel = os.path.normpath(os.path.join(rel_dir, name))
            files.append(rel.replace(os.sep, "/"))
    # Sounds are not loaded in the browser build (no audio yet).
    return sorted(f for f in files if not f.startswith("assets/minecraft/sounds/"))


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    repo = os.path.dirname(os.path.dirname(here))
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--resources", default=os.path.join(repo, ".local", "horizon-client", "26.3"))
    parser.add_argument("--out", default=os.path.join(repo, "web", "dist", "horizon-data.bin"))
    parser.add_argument("--only")
    args = parser.parse_args()
    root = os.path.abspath(args.resources)
    if not os.path.isfile(os.path.join(root, "horizon-resources.json")):
        sys.exit(f"{root}: no prepared resources (run horizon-prepare --prepare first)")
    files = wanted_files(root, args.only)
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    total = 0
    with open(args.out + ".tmp", "wb") as out:
        for rel in files:
            data = open(os.path.join(root, rel), "rb").read()
            name = rel.encode("utf-8")
            out.write(struct.pack("<I", len(name)))
            out.write(name)
            out.write(struct.pack("<I", len(data)))
            out.write(data)
            total += len(data)
    os.replace(args.out + ".tmp", args.out)
    print(f"{args.out}: {len(files)} files, {total / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
