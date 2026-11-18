#!/usr/bin/env python3
"""Write the default components of Minecraft's items that are not blocks (docs/open-creative.md).

Reads the prepared resources of the pinned Minecraft version (`.local/horizon-client/26.3`, made
by `horizon-prepare` from the player's own install): `items.json` (each item's default
components) and `assets/minecraft/models/item/<name>.json` (which parent and texture its model
uses). Writes `crates/horizon-open-content/data/items/defaults.json`:

    {"base": {components every item shares},
     "items": {
       "rotten_flesh": {"components": {the ones that differ from base}},
       "iron_sword": {"handheld": true, "components": {...}},
       "compass": {"texture": "item/compass_16", "components": {...}},
       ...}}

Items are every name of `data/registries/item.txt` that is not a block name, in registry order,
one per line. `item_model` and `item_name` are left out: the generator writes its own. Only facts
are written (numbers, identifiers, booleans), never translated text.

    python3 tools/open_item_defaults.py [--resources DIR] [--check]
"""

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "crates" / "horizon-open-content" / "data"
OUT = DATA / "items" / "defaults.json"
RESOURCES = ROOT / ".local" / "horizon-client" / "26.3"

# Left out: the generator names and models every item itself.
OWN = {"minecraft:item_model", "minecraft:item_name"}
HANDHELD = {"item/handheld", "item/handheld_rod", "item/handheld_mace"}


def dumps(value):
    return json.dumps(value, separators=(",", ":"), sort_keys=True, ensure_ascii=True)


def has_text(value):
    if isinstance(value, dict):
        return any(k in ("text", "translate") or has_text(v) for k, v in value.items())
    if isinstance(value, list):
        return any(has_text(v) for v in value)
    return False


def render(resources):
    prototypes = json.loads((resources / "items.json").read_text())
    items = (DATA / "registries" / "item.txt").read_text().split()
    blocks = set((DATA / "registries" / "block.txt").read_text().split())
    names = [n for n in items if n not in blocks]
    components = {}
    for name in names:
        c = {k: v for k, v in prototypes[f"minecraft:{name}"].items() if k not in OWN}
        for k, v in c.items():
            if has_text(v):
                raise SystemExit(f"{name}: {k} holds text")
        components[name] = c
    # The base: for each component every item has, its most common value
    # (items with another value list theirs).
    base = {}
    for key in components[names[0]]:
        if all(key in components[n] for n in names):
            values = [dumps(components[n][key]) for n in names]
            base[key] = json.loads(max(sorted(set(values)), key=values.count))
    lines = []
    for name in names:
        entry = {}
        model_file = resources / "assets" / "minecraft" / "models" / "item" / f"{name}.json"
        if model_file.exists():
            model = json.loads(model_file.read_text())
            parent = model.get("parent", "").removeprefix("minecraft:")
            if parent in HANDHELD:
                entry["handheld"] = True
            layer0 = model.get("textures", {}).get("layer0", "").removeprefix("minecraft:")
            if layer0 and layer0 != f"item/{name}":
                entry["texture"] = layer0
        c = components[name]
        diff = {k: v for k, v in c.items() if k not in base or dumps(base[k]) != dumps(v)}
        # A base component this item lacks: written as null (removed).
        diff.update({k: None for k in base if k not in c})
        entry["components"] = diff
        lines.append(f"{dumps(name)}:{dumps(entry)}")
    return '{"base":' + dumps(base) + ',\n"items":{\n' + ",\n".join(lines) + "\n}}\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--resources", type=Path, default=RESOURCES)
    parser.add_argument("--check", action="store_true", help="fail if the file is out of date")
    args = parser.parse_args()
    text = render(args.resources)
    if args.check:
        if not OUT.exists() or OUT.read_text() != text:
            print(f"{OUT.relative_to(ROOT)} is out of date: python3 tools/open_item_defaults.py")
            return 1
        print(f"{OUT.relative_to(ROOT)} is up to date")
        return 0
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(text)
    print(f"wrote {OUT.relative_to(ROOT)} ({text.count(chr(10)) - 3} items)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
