#!/usr/bin/env python3
"""Write the facts of Horizon Open's full-cube blocks (docs/open-creative.md).

Reads the prepared vanilla tables (`.local/horizon-client/26.3`: `facts.json`,
`gameplay-facts.json`, `block-behaviours.json`, `block-tags.json`) and the names its block
models use, and writes `crates/horizon-open-content/data/blocks/cubes.txt`: one line per block
of these families, in block registry order:

- every block of class `Block` with a single state (all full cubes),
- every `RotatedPillarBlock` (logs, stems, basalt, hay: one `axis` property),
- the furnace, blast furnace and smoker (`facing` and `lit`).

Each line is tab-separated: the name, its class chain, destroy time, explosion resistance, light
emission (and when lit), light dampening, friction, map colour, sound type, the tool tag that
mines it fast, the needed tool level (0 none, 1 stone, 2 iron, 3 diamond), whether it needs the
tool to drop, the model layout (`cube`, `axis` or `facing`), and the texture names of its six
faces (down, up, north, south, west, east; the lit front last for `facing`). Only numbers and
identifiers are written: no texture, model or other Mojang file.

    python3 tools/open_block_facts.py [--resources DIR] [--check]
"""

import argparse
import json
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "crates" / "horizon-open-content" / "data" / "blocks" / "cubes.txt"
RESOURCES = ROOT / ".local" / "horizon-client" / "26.3"
FURNACES = ("furnace", "blast_furnace", "smoker")
FACES = ("down", "up", "north", "south", "west", "east")
TOOLS = ("pickaxe", "axe", "shovel", "hoe")
LEVELS = (("stone", 1), ("iron", 2), ("diamond", 3))
HEADER = ("name", "classes", "hardness", "resistance", "emission", "lit_emission", "opacity",
          "friction", "map_color", "sound", "tool", "needs", "requires_tool", "layout") + FACES + ("lit_north",)


def strip(name):
    return name.split(":", 1)[1] if ":" in name else name


def number(value):
    """A float as its shortest repr, an integral one without the point."""
    value = float(value)
    return str(int(value)) if value.is_integer() else repr(value)


def resolve_tag(tags, name, seen=()):
    out = []
    tag = tags[name]
    # The prepared file writes each tag as its JSON text.
    tag = json.loads(tag) if isinstance(tag, str) else tag
    for value in tag["values"]:
        value = value if isinstance(value, str) else value["id"]
        if value.startswith("#"):
            if value[1:] not in seen:
                out += resolve_tag(tags, value[1:], seen + (name,))
        else:
            out.append(value)
    return out


class Models:
    def __init__(self, root):
        self.root = root / "assets" / "minecraft"

    def load(self, name):
        return json.loads((self.root / "models" / f"{strip(name)}.json").read_text())

    def textures(self, name):
        """The model's resolved texture variables, walking its parents."""
        chain = []
        while name:
            model = self.load(name)
            chain.append(model)
            name = model.get("parent")
        variables = {}
        for model in reversed(chain):
            variables.update(model.get("textures", {}))

        def value(var, depth=0):
            v = variables.get(var)
            if v is None or depth > 8:
                return None
            return value(v[1:], depth + 1) if v.startswith("#") else strip(v).removeprefix("block/")

        elements = next((m["elements"] for m in chain if "elements" in m), None)
        faces = {}
        for face in FACES:
            ref = elements[0]["faces"][face]["texture"] if elements else f"#{face}"
            faces[face] = value(ref[1:])
        return faces

    def variant_model(self, block, key):
        state = json.loads((self.root / "blockstates" / f"{block}.json").read_text())
        variant = state["variants"][key]
        return (variant[0] if isinstance(variant, list) else variant)["model"]


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
        if classes[0] == "Block" and counts[name] == 1:
            layout, key = "cube", ""
        elif classes[0] == "RotatedPillarBlock":
            layout, key = "axis", "axis=y"
        elif short in FURNACES:
            layout, key = "facing", "facing=north,lit=false"
        else:
            continue
        states = gameplay[first[name]:first[name] + counts[name]]
        facts = states[0]
        if layout == "facing":
            # lit=true comes first among each facing's two states.
            lit, unlit = states[0], states[1]
            facts = unlit
            lit_emission = lit["emission"]
        else:
            lit_emission = facts["emission"]
        for s in states:
            if not (s["solid_render"] and s["full_collision"]):
                sys.exit(f"{name}: not a full opaque cube")
        sound = facts["sound"]
        kind = strip(sound["break"]["id"]).removeprefix("block.").removesuffix(".break")
        for event in ("break", "step", "place", "hit", "fall"):
            # Some types leave an event silent (resin's hit).
            if strip(sound[event]["id"]) not in (f"block.{kind}.{event}", "intentionally_empty"):
                sys.exit(f"{name}: sound {sound} does not follow block.<type>.<event>")
        faces = models.textures(models.variant_model(short, key))
        if None in faces.values():
            sys.exit(f"{name}: unresolved texture in {faces}")
        lit_north = "-"
        if layout == "facing":
            lit_north = models.textures(models.variant_model(short, "facing=north,lit=true"))["north"]
        out.append((
            short, ",".join(classes), number(facts["hardness"]), number(facts["explosion_resistance"]),
            str(facts["emission"]), str(lit_emission), str(facts["dampening"]), number(facts["friction"]),
            f"{facts['map_color']:06x}", kind, tagged.get(name, "-"), str(needs.get(name, 0)),
            "1" if facts["requires_tool"] else "0", layout, *(faces[f] for f in FACES), lit_north,
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
            sys.exit(f"{OUT.relative_to(ROOT)} is stale: run tools/open_block_facts.py")
        print(f"{OUT.relative_to(ROOT)} is up to date")
        return
    OUT.write_text(text)
    print(f"wrote {OUT.relative_to(ROOT)}: {text.count(chr(10)) - 1} blocks")


if __name__ == "__main__":
    main()
