#!/usr/bin/env python3
"""Write the facts of Horizon Open's plants and small decorations (docs/open-creative.md).

Reads the prepared vanilla tables (`.local/horizon-client/26.3`: `facts.json`,
`gameplay-facts.json`, `block-behaviours.json`, `block-tags.json`, `cached-collision.nbt`,
`occlusion.nbt`, `items.json`) and the names its block states, block models and item models
use, and writes `crates/horizon-open-content/data/blocks/plants.txt` for the blocks of the
classes in FAMILIES whose models all use a layout `plants.rs` draws (LAYOUTS).

The file has one record per line, tab-separated, its kind first:

- `shape ID JSON`: a collision or occlusion shape as the shape table stores it (cell
  boundaries per axis, the occupied cells, the cube flags and the occupied bounds).
- `row ID JSON`: the facts of a state: light, light dampening, map colour, flags, push reaction, random
  offset, its outline boxes, its collision and occlusion shapes (shape ids) and its behaviour
  row (sturdy faces, flags, fluid).
- `model NAME LAYOUT VAR=TEXTURE...`: a block model by name: the layout its geometry follows
  and its texture variables (texture names, without `block/`).
- `block NAME CLASSES ITEM HARDNESS RESISTANCE FRICTION SOUND TOOL NEEDS
  REQUIRES_TOOL ITEM_MODEL ITEM_COMPONENTS STATES`: a block, in registry order. ITEM is the
  item its gameplay row names (`-` for none), ITEM_MODEL the item's texture and tints
  (`TEXTURE` or `TEXTURE|TINTS-JSON`, `-` when the block has no item of its own), and
  ITEM_COMPONENTS the item's default components besides the base set (its name
  included), as JSON. STATES is one entry per state, in state order, separated by spaces:
  `ROW:MODEL:X:Y:UVLOCK:WEIGHT`, with `|MODEL:X:Y:UVLOCK:WEIGHT` for each further model the
  state picks among at random (nothing after `ROW:` for a multipart block).
- `multipart NAME JSON`: a block drawn in parts (vines, lichen): each case's condition on the
  state and the models it adds, encoded as above.

Only numbers and identifiers are written: no texture, model geometry or other Mojang file.

    python3 tools/open_plant_facts.py [--resources DIR] [--check]
"""

import argparse
import json
import struct
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "crates" / "horizon-open-content" / "data" / "blocks" / "plants.txt"
DEFAULTS = ROOT / "crates" / "horizon-open-content" / "data" / "items" / "defaults.json"
RESOURCES = ROOT / ".local" / "horizon-client" / "26.3"
TOOLS = ("pickaxe", "axe", "shovel", "hoe")
LEVELS = (("stone", 1), ("iron", 2), ("diamond", 3))

# The classes of the families (first class of the chain).
FAMILIES = {
    "SaplingBlock", "TallGrassBlock", "DryVegetationBlock", "BushBlock", "ShortDryGrassBlock",
    "TallDryGrassBlock", "SeagrassBlock", "TallSeagrassBlock", "FlowerBlock", "WitherRoseBlock",
    "MushroomBlock", "CropBlock", "CarrotBlock", "PotatoBlock", "BeetrootBlock", "SugarCaneBlock",
    "CactusFlowerBlock", "TallFlowerBlock", "DoublePlantBlock", "KelpBlock", "KelpPlantBlock",
    "BaseCoralPlantBlock", "CoralPlantBlock", "BaseCoralFanBlock", "CoralFanBlock",
    "BaseCoralWallFanBlock", "CoralWallFanBlock", "SweetBerryBushBlock", "NetherFungusBlock",
    "NetherRootsBlock", "NetherSproutsBlock", "HangingRootsBlock", "WebBlock", "EyeblossomBlock",
    "FireflyBushBlock", "TorchflowerCropBlock", "NetherWartBlock", "WeepingVinesBlock",
    "WeepingVinesPlantBlock", "TwistingVinesBlock", "TwistingVinesPlantBlock", "CaveVinesBlock",
    "CaveVinesPlantBlock", "HangingMossBlock", "AmethystClusterBlock", "TorchBlock", "WallTorchBlock", "RedstoneTorchBlock",
    "RedstoneWallTorchBlock", "LanternBlock", "WeatheringLanternBlock", "LilyPadBlock",
    "SeaPickleBlock", "VineBlock", "GlowLichenBlock", "SculkVeinBlock",
}
# The geometries plants.rs draws, by the name of the model that holds the elements.
LAYOUTS = {
    "cross", "tinted_cross", "cross_emissive", "crop", "coral_fan", "coral_wall_fan",
    "template_seagrass", "sunflower_top", "template_torch", "template_torch_unlit",
    "template_redstone_torch", "template_torch_wall", "template_torch_wall_unlit",
    "template_redstone_torch_wall", "template_lantern", "template_hanging_lantern", "lily_pad",
    "vine", "glow_lichen", "sculk_vein", "sea_pickle", "two_sea_pickles", "three_sea_pickles",
    "four_sea_pickles", "dead_sea_pickle", "two_dead_sea_pickles", "three_dead_sea_pickles",
    "four_dead_sea_pickles",
}
# Gameplay facts kept per state; the others are the block's.
ROW_FIELDS = (
    "emission", "dampening", "skylight_down", "shape_occlusion", "can_occlude", "solid_render",
    "map_color", "replaceable", "random_ticks", "block_entity", "render_shape", "liquid",
    "ignited_by_lava", "push", "offset", "full_collision", "outline_offset", "speed_factor",
    "jump_factor",
)
BLOCK_FIELDS = ("hardness", "explosion_resistance", "friction", "item", "requires_tool")
HEADER = "# open_plant_facts.py: shapes, state rows, models and blocks (see the script)\n"


def strip(name):
    return name.split(":", 1)[1] if ":" in name else name


def number(value):
    value = float(value)
    return str(int(value)) if value.is_integer() else repr(value)


def read_nbt(path):
    data = path.read_bytes()
    at = [0]

    def take(fmt, size):
        value = struct.unpack_from(">" + fmt, data, at[0])[0]
        at[0] += size
        return value

    def string():
        n = take("H", 2)
        value = data[at[0]:at[0] + n].decode("utf-8")
        at[0] += n
        return value

    def payload(tag):
        if tag == 1:
            return take("b", 1)
        if tag == 2:
            return take("h", 2)
        if tag == 3:
            return take("i", 4)
        if tag == 4:
            return take("q", 8)
        if tag == 5:
            return take("f", 4)
        if tag == 6:
            return take("d", 8)
        if tag == 7:
            n = take("i", 4)
            value = list(data[at[0]:at[0] + n])
            at[0] += n
            return value
        if tag == 8:
            return string()
        if tag == 9:
            inner, n = take("b", 1), take("i", 4)
            return [payload(inner) for _ in range(n)]
        if tag == 10:
            out = {}
            while (inner := take("b", 1)) != 0:
                key = string()
                out[key] = payload(inner)
            return out
        if tag in (11, 12):
            n = take("i", 4)
            fmt, size = ("i", 4) if tag == 11 else ("q", 8)
            value = list(struct.unpack_from(f">{n}{fmt}", data, at[0]))
            at[0] += n * size
            return value
        raise ValueError(f"tag {tag}")

    tag = take("b", 1)
    string()
    return payload(tag)


def shape_json(entry):
    """A shape table entry with its coordinates as numbers."""
    def axis(name):
        return [struct.unpack(">d", struct.pack(">q", v))[0] for v in entry[name]]

    return {
        "x": axis("x"), "y": axis("y"), "z": axis("z"),
        "occupied": "".join(str(b) for b in entry["occupied"]),
        "cube": entry["cube"], "cube_axes": entry["cube_axes"],
        "min": entry["discrete_min"], "max": entry["discrete_max"],
    }


def resolve_tag(tags, name, seen=()):
    out = []
    tag = tags[name]
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

    def layout(self, name):
        """The name of the model in the chain that holds the elements, and the resolved
        texture variables."""
        chain = []
        while name:
            model = self.load(name)
            chain.append((strip(name).removeprefix("block/"), model))
            name = model.get("parent")
        variables = {}
        for _, model in reversed(chain):
            variables.update(model.get("textures", {}))

        def value(v, depth=0):
            if v is None or depth > 8:
                return None
            return value(variables.get(v[1:]), depth + 1) if v.startswith("#") else strip(v)

        layout = next((n for n, m in chain if "elements" in m), None)
        textures = {k: value(v) for k, v in variables.items()}
        return layout, {k: v for k, v in textures.items() if v is not None}

    def states(self, block):
        return json.loads((self.root / "blockstates" / f"{block}.json").read_text())

    def item(self, item):
        path = self.root / "items" / f"{item}.json"
        if not path.exists():
            return None
        model = json.loads(path.read_text())["model"]
        if model.get("type") != "minecraft:model":
            return None
        chain, name = [], model["model"]
        while name:
            m = self.load(name)
            chain.append(m)
            name = m.get("parent")
            if name and strip(name) in ("item/generated", "builtin/generated"):
                layer = next((c["textures"]["layer0"] for c in chain if "layer0" in c.get("textures", {})), None)
                if layer is None:
                    return None
                return strip(layer), model.get("tints")
        return None


def matches(key, properties):
    if key == "":
        return True
    for part in key.split(","):
        k, v = part.split("=")
        if properties.get(k) != v:
            return False
    return True


def rows(resources):
    behaviours = json.loads((resources / "block-behaviours.json").read_text())
    facts = json.loads((resources / "facts.json").read_text())["states"]
    gameplay = json.loads((resources / "gameplay-facts.json").read_text())["states"]
    behaviour_states = behaviours["states"]
    tags = json.loads((resources / "block-tags.json").read_text())
    items = json.loads((resources / "items.json").read_text())
    base = json.loads(DEFAULTS.read_text())["base"]
    collision = read_nbt(resources / "cached-collision.nbt")
    occlusion = read_nbt(resources / "occlusion.nbt")
    names = [row[0] for row in facts]
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
    shapes, shape_ids = [], {}
    state_rows, row_ids = [], {}
    used_models = {}
    blocks = []

    def shape(table, index):
        cached = table["cached_shapes"][index]
        if cached < 0:
            sys.exit(f"{names[index]}: shape computed in code")
        text = json.dumps(shape_json(table["shapes"][cached]), separators=(",", ":"))
        if text not in shape_ids:
            shape_ids[text] = len(shapes)
            shapes.append(text)
        return shape_ids[text]

    def encode(choices):
        """The models a variant or a multipart case picks among, or None for a layout plants.rs
        does not draw."""
        out = []
        for choice in choices if isinstance(choices, list) else [choices]:
            layout, textures = models.layout(choice["model"])
            if layout not in LAYOUTS:
                return None
            model = strip(choice["model"]).removeprefix("block/")
            used_models[model] = (layout, textures)
            out.append(":".join([model, str(choice.get("x", 0)), str(choice.get("y", 0)),
                                 "1" if choice.get("uvlock") else "0", str(choice.get("weight", 1))]))
        return "|".join(out)

    multiparts = []
    for block in behaviours["blocks"]:
        name, classes = block["name"], block["class"]
        short = strip(name)
        if classes[0] not in FAMILIES:
            continue
        definition = models.states(short)
        lo, n = first[name], counts[name]
        entries, ok = [], True
        parts = None
        if "multipart" in definition:
            parts = [[case.get("when", {}), encode(case["apply"])] for case in definition["multipart"]]
            if any(p[1] is None for p in parts):
                continue
        for index in range(lo, lo + n):
            state = facts[index][4]
            props = state.get("properties", {}) if isinstance(state, dict) else {}
            alternatives = ""
            if parts is None:
                variant = next((v for k, v in definition["variants"].items() if matches(k, props)), None)
                alternatives = None if variant is None else encode(variant)
                if alternatives is None:
                    ok = False
                    break
            g = gameplay[index]
            row = {f: g[f] for f in ROW_FIELDS}
            row["outline"] = g["outline"]
            row["collision"] = shape(collision, index)
            row["occlusion"] = shape(occlusion, index)
            row["behaviour"] = behaviour_states[index]
            text = json.dumps(row, separators=(",", ":"))
            if text not in row_ids:
                row_ids[text] = len(state_rows)
                state_rows.append(text)
            entries.append(f"{row_ids[text]}:{alternatives}")
        if not ok:
            continue
        g = gameplay[lo]
        for f in BLOCK_FIELDS:
            if any(gameplay[i][f] != g[f] for i in range(lo, lo + n)):
                sys.exit(f"{name}: {f} differs between states")
        sound = g["sound"]
        # The sound type by its break event (the bundle ships no audio).
        kind = strip(sound["break"]["id"]).removeprefix("block.").removesuffix(".break")
        item = strip(g["item"]) if g["item"] != "minecraft:air" else "-"
        item_model, components = "-", "{}"
        if item == short:
            found = models.item(short)
            if found is None:
                sys.exit(f"{name}: its item model is not a flat item")
            texture, tints = found
            item_model = texture if not tints else f"{texture}|{json.dumps(tints, separators=(',', ':'))}"
            own = {k: v for k, v in items[g["item"]].items()
                   if k != "minecraft:item_model" and base.get(k) != v}
            components = json.dumps(own, separators=(",", ":"), sort_keys=True)
        if parts is not None:
            multiparts.append(f"multipart\t{short}\t{json.dumps(parts, separators=(',', ':'))}")
        blocks.append("\t".join([
            "block", short, ",".join(classes), item, number(g["hardness"]),
            number(g["explosion_resistance"]), number(g["friction"]), kind,
            tagged.get(name, "-"), str(needs.get(name, 0)), "1" if g["requires_tool"] else "0",
            item_model, components, " ".join(entries),
        ]))
    lines = [f"shape\t{i}\t{s}" for i, s in enumerate(shapes)]
    lines += [f"row\t{i}\t{r}" for i, r in enumerate(state_rows)]
    for model, (layout, textures) in sorted(used_models.items()):
        variables = " ".join(f"{k}={v.removeprefix('block/')}" for k, v in sorted(textures.items()))
        lines.append(f"model\t{model}\t{layout}\t{variables}")
    lines += blocks
    lines += multiparts
    return lines


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--resources", type=Path, default=RESOURCES)
    parser.add_argument("--check", action="store_true", help="fail if the committed file differs")
    args = parser.parse_args()
    lines = rows(args.resources)
    text = HEADER + "".join(line + "\n" for line in lines)
    if args.check:
        if OUT.read_text() != text:
            sys.exit(f"{OUT.relative_to(ROOT)} is stale: run tools/open_plant_facts.py")
        print(f"{OUT.relative_to(ROOT)} is up to date")
        return
    OUT.write_text(text)
    count = sum(1 for line in lines if line.startswith("block\t"))
    print(f"wrote {OUT.relative_to(ROOT)}: {count} blocks")


if __name__ == "__main__":
    main()
