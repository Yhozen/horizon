#!/usr/bin/env python3
"""Write the facts of Horizon Open's slabs and stairs (docs/open-creative.md).

Reads the prepared vanilla tables (`.local/horizon-client/26.3`) as `open_block_facts.py` does and
writes `crates/horizon-open-content/data/blocks/slabs.txt`: one line per `SlabBlock` and
`StairBlock` (the weathering copper ones too), in block registry order.

Each line is tab-separated: the name, `slab` or `stairs`, its class chain, destroy time, explosion
resistance, friction, map colour, sound type, the tool tag that mines it fast, the needed tool
level, whether it needs the tool to drop, its note block instrument, whether lava ignites it, the
texture names its model gives the bottom, top and side, and for a slab the six faces of its double
block (down, up, north, south, west, east; `-` for stairs). Shapes, states and models are the
generator's own (`crates/horizon-open-content/src/shapes.rs`, `slabs.rs`): only numbers and
identifiers are written here.

    python3 tools/open_slab_facts.py [--resources DIR] [--check]
"""

import argparse
import json
import sys
from collections import Counter
from pathlib import Path

from open_block_facts import FACES, LEVELS, RESOURCES, ROOT, TOOLS, Models, number, resolve_tag, strip

OUT = ROOT / "crates" / "horizon-open-content" / "data" / "blocks" / "slabs.txt"
HEADER = ("name", "kind", "classes", "hardness", "resistance", "friction", "map_color", "sound", "tool",
          "needs", "requires_tool", "instrument", "lava", "bottom", "top", "side") + FACES
# Facts that must be the same in every state of a block.
SHARED = ("hardness", "explosion_resistance", "friction", "map_color", "requires_tool", "ignited_by_lava", "sound")


def rows(resources):
    behaviours = json.loads((resources / "block-behaviours.json").read_text())
    names = [row[0] for row in json.loads((resources / "facts.json").read_text())["states"]]
    gameplay = json.loads((resources / "gameplay-facts.json").read_text())["states"]
    tags = json.loads((resources / "block-tags.json").read_text())
    counts = Counter(names)
    first = {}
    for i, name in enumerate(names):
        first.setdefault(name, i)
    tagged = {}
    for tool in TOOLS:
        for name in resolve_tag(tags, f"minecraft:mineable/{tool}"):
            tagged.setdefault(name, tool)
    needs = {}
    for level, n in LEVELS:
        for name in resolve_tag(tags, f"minecraft:needs_{level}_tool"):
            needs[name] = max(needs.get(name, 0), n)
    models = Models(resources)
    out = []
    for block in behaviours["blocks"]:
        name, classes = block["name"], block["class"]
        short = strip(name)
        if "SlabBlock" in classes:
            kind, key = "slab", "type=bottom"
        elif "StairBlock" in classes:
            kind, key = "stairs", "facing=east,half=bottom,shape=straight"
        else:
            continue
        start = first[name]
        states = gameplay[start:start + counts[name]]
        facts = states[0]
        for s in states:
            for field in SHARED:
                if s.get(field) != facts.get(field):
                    sys.exit(f"{name}: {field} differs between its states")
        instruments = {row[8] for row in behaviours["states"][start:start + counts[name]]}
        if len(instruments) != 1:
            sys.exit(f"{name}: more than one instrument")
        sound = facts["sound"]
        kind_name = strip(sound["break"]["id"]).removeprefix("block.").removesuffix(".break")
        for event in ("break", "step", "place", "hit", "fall"):
            if strip(sound[event]["id"]) not in (f"block.{kind_name}.{event}", "intentionally_empty"):
                sys.exit(f"{name}: sound {sound} does not follow block.<type>.<event>")
        model = models.variant_model(short, key)
        parents = models.parents(model)
        if parents[1] != ("slab" if kind == "slab" else "stairs"):
            sys.exit(f"{name}: model {model} is not on the {kind} template")
        variables = models.load(model)["textures"]
        bottom, top, side = (strip(variables[v]).removeprefix("block/") for v in ("bottom", "top", "side"))
        double = ("-",) * 6
        if kind == "slab":
            faces = models.textures(models.variant_model(short, "type=double"))
            if None in faces.values():
                sys.exit(f"{name}: unresolved double texture in {faces}")
            double = tuple(faces[f] for f in FACES)
        out.append((
            short, kind, ",".join(classes), number(facts["hardness"]), number(facts["explosion_resistance"]),
            number(facts["friction"]), f"{facts['map_color']:06x}", kind_name, tagged.get(name, "-"),
            str(needs.get(name, 0)), "1" if facts["requires_tool"] else "0", instruments.pop(),
            "1" if facts["ignited_by_lava"] else "0", bottom, top, side, *double,
        ))
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--resources", type=Path, default=RESOURCES)
    parser.add_argument("--check", action="store_true", help="fail if the committed file differs")
    args = parser.parse_args()
    text = "".join("\t".join(row) + "\n" for row in [HEADER, *rows(args.resources)])
    if args.check:
        if OUT.read_text() != text:
            sys.exit(f"{OUT.relative_to(ROOT)} is stale: run tools/open_slab_facts.py")
        print(f"{OUT.relative_to(ROOT)} is up to date")
        return
    OUT.write_text(text)
    print(f"wrote {OUT.relative_to(ROOT)}: {text.count(chr(10)) - 1} blocks")


if __name__ == "__main__":
    main()
