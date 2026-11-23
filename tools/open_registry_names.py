#!/usr/bin/env python3
"""Write the name lists Horizon Open numbers its content from (docs/open-creative.md).

Reads the data generator's reports of the pinned Minecraft version (`registries.json` and
`blocks.json`, from the player's own server jar: `python3 tools/lab.py datagen`) and writes, under
`crates/horizon-open-content/data/`:

- `registries/<registry>.txt`: one name per line in protocol order (line n is id n). Names in the
  `minecraft:` namespace are written without it; others (`brigadier:bool`) keep theirs.
- `blocks/states.txt`: one block per line, in block registry order: the block's name, then each
  property as `key=v1,v2,...` with its keys sorted and its values in Minecraft's order, the
  default state's value marked with a leading `*`. A block's states are the product of its
  properties, the last key varying fastest; its first state's id follows the previous block's last.

Only names are written: no other fact of the reports.

    python3 tools/open_registry_names.py [--reports DIR] [--check]
"""

import argparse
import itertools
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "crates" / "horizon-open-content" / "data"
REPORTS = ROOT / ".local" / "datagen" / "out" / "reports"

# The registries the open bundle numbers: those registries.json carries and the client or the
# open server read numbers from.
REGISTRIES = [
    "attribute",
    "block",
    "block_entity_type",
    "command_argument_type",
    "custom_stat",
    "data_component_type",
    "entity_type",
    "item",
    "map_decoration_type",
    "menu",
    "mob_effect",
    "particle_type",
    "recipe_book_category",
    "recipe_display",
    "slot_display",
    "sound_event",
    "stat_type",
    "villager_profession",
    "villager_type",
]


def short(name):
    return name.removeprefix("minecraft:")


def registry_lines(report, registry):
    entries = report[f"minecraft:{registry}"]["entries"]
    names = sorted(entries, key=lambda n: entries[n]["protocol_id"])
    ids = [entries[n]["protocol_id"] for n in names]
    if ids != list(range(len(names))):
        raise SystemExit(f"{registry}: protocol ids are not 0..{len(names) - 1}")
    return [short(n) for n in names]


def state_lines(blocks, block_names):
    # blocks.json lists blocks by name; their states are numbered in registry order.
    order = sorted(blocks, key=lambda n: blocks[n]["states"][0]["id"])
    if [short(n) for n in order] != block_names:
        raise SystemExit("blocks.json's state ids do not follow the block registry")
    lines, next_id = [], 0
    for name in order:
        block = blocks[name]
        properties = block.get("properties", {})
        keys = sorted(properties)
        states = block["states"]
        # The rule the bundle numbers by must give exactly Minecraft's ids.
        product = list(itertools.product(*(properties[k] for k in keys)))
        listed = [tuple(s.get("properties", {}).get(k) for k in keys) for s in states]
        if product != listed or [s["id"] for s in states] != list(range(next_id, next_id + len(states))):
            raise SystemExit(f"{name}: states are not numbered by the sorted-key rule")
        next_id += len(states)
        default = next(s for s in states if s.get("default"))
        parts = [short(name)]
        for key in keys:
            chosen = default["properties"][key]
            values = [("*" if v == chosen else "") + v for v in properties[key]]
            parts.append(f"{key}={','.join(values)}")
        lines.append(" ".join(parts))
    return lines


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--reports", type=Path, default=REPORTS)
    parser.add_argument("--check", action="store_true", help="fail if a written file differs")
    args = parser.parse_args()
    report = json.loads((args.reports / "registries.json").read_text(encoding="utf-8"))
    blocks = json.loads((args.reports / "blocks.json").read_text(encoding="utf-8"))
    files = {}
    for registry in REGISTRIES:
        files[f"registries/{registry}.txt"] = registry_lines(report, registry)
    files["blocks/states.txt"] = state_lines(blocks, files["registries/block.txt"])
    stale = []
    for path, lines in files.items():
        target = DATA / path
        text = "\n".join(lines) + "\n"
        if args.check:
            if not target.is_file() or target.read_text(encoding="utf-8") != text:
                stale.append(path)
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="utf-8")
    if stale:
        print("out of date: " + ", ".join(stale), file=sys.stderr)
        return 1
    states = sum(len(b["states"]) for b in blocks.values())
    print(f"{len(files)} files, {len(blocks)} blocks, {states} states")
    return 0


if __name__ == "__main__":
    sys.exit(main())
