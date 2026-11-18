#!/usr/bin/env python3
"""Write the facts of Horizon Open's full-cube blocks (docs/open-creative.md).

Reads the prepared vanilla tables (`.local/horizon-client/26.3`: `facts.json`,
`gameplay-facts.json`, `block-behaviours.json`, `block-tags.json`) and the names its block
models use, and writes `crates/horizon-open-content/data/blocks/cubes.txt`: one line per block
of these families, in block registry order:

- every block of class `Block` with a single state (all full cubes),
- every `RotatedPillarBlock` (logs, stems, basalt, hay: one `axis` property),
- the furnace, blast furnace and smoker (`facing` and `lit`),
- every other block whose states all draw one plain cube model: unturned (ores, glass, leaves,
  copper, coral blocks: layout `cube`; `clear` when light shows through it, `leaves` when its
  faces also carry a tint index, `glass` when its sprites are drawn translucent), turned at
  random as Minecraft turns sand (`spin_y`) and netherrack (`spin_xy`), or turned by a
  horizontal `facing` alone (pumpkins, the loom); soul sand and mud too (`sunken`: their
  collision box is 14 sixteenths tall),
- every other block whose states all draw full-cube models, whatever their faces' UVs and
  turns (glazed terracotta, observers, dispensers, barrels, copper bulbs, mycelium), or a
  multipart of single boxes (mushroom blocks): layout `variants`. `cube_variants.txt` then
  lists its block-state variants or parts (state or condition, model, x and y turns, uvlock)
  and each model's elements (box, rotation, shade, and faces: texture, UV, rotation, tint index,
  cull face),
- the blocks of the classes in `SHAPED` (redstone wire, rails, rods, carpets, repeaters and
  comparators, snow, farmland, dirt paths, ladders, levers, cakes, anvils, candles, candle
  cakes, cauldrons, the pale moss carpet): layout `shaped` (a candle's lit light is one number
  per candle count, `3,6,9,12`),
  their models listed as `variants`' are, and the default state's piston reaction,
  replaceable, random ticks, lava ignition, block entity and item in the last columns, which
  the tables cannot derive for them; `cubes.rs` writes their shapes by class.

Each line is tab-separated: the name, its class chain, destroy time, explosion resistance, light
emission (and when lit), light dampening, friction, map colour, sound type, the tool tag that
mines it fast, the needed tool level (0 none, 1 stone, 2 iron, 3 diamond), whether it needs the
tool to drop, the model layout (`cube`, `clear`, `leaves`, `glass`, `spin_y`, `spin_xy`, `axis`, `pillar`:
an axis whose lying models turn the top half a turn, or `facing`), and the texture names of its six
faces (down, up, north, south, west, east; the lit front last for `facing`), and the speed
factor (soul sand's 0.4).

It also writes `data/blocks/connected.txt`: the walls, fences, fence gates, glass panes and bars
(classes `WallBlock`, `FenceBlock`, `FenceGateBlock`, `IronBarsBlock`) whose models use
Minecraft's common templates (the bamboo fence and gate, drawn with their own templates, are
left out). Each line: the name, its class chain, destroy time, resistance, map colour, sound
type, mining tool, needed level, whether it needs the tool, the family (`wall`, `fence`,
`gate`, `pane`, `bars`) and its texture names (the main one, then the edge for panes and bars).

And `data/blocks/openings.txt`, in the same format: the doors, trapdoors, buttons and pressure
plates (classes `DoorBlock`, `TrapDoorBlock`, `ButtonBlock`, `PressurePlateBlock`,
`WeightedPressurePlateBlock`), families `door` (textures: top, bottom), `trapdoor`,
`orientable_trapdoor` (its texture turns with `facing`), `button`, `plate` and `weighted_plate`.

Only numbers and identifiers are written: no texture, model or other Mojang file.

    python3 tools/open_block_facts.py [--resources DIR] [--check]
"""

import argparse
import json
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "crates" / "horizon-open-content" / "data" / "blocks" / "cubes.txt"
CONNECTED = OUT.with_name("connected.txt")
OPENINGS = OUT.with_name("openings.txt")
VARIANTS_OUT = OUT.with_name("cube_variants.txt")
RESOURCES = ROOT / ".local" / "horizon-client" / "26.3"
FURNACES = ("furnace", "blast_furnace", "smoker")
# Cubes whose collision box is 14 sixteenths tall (`SoulSandBlock`, `MudBlock`): layout `sunken`.
SUNKEN = ("SoulSandBlock", "MudBlock")
# Blocks drawn by models of any boxes, with shapes `cubes.rs` writes by class: layout `shaped`.
SHAPED = ("RedstoneWireBlock", "LightningRodBlock", "EndRodBlock", "CarpetBlock", "RepeaterBlock",
          "ComparatorBlock", "SnowLayerBlock", "FarmlandBlock", "PathBlock", "LadderBlock",
          "BaseRailBlock", "LeverBlock", "CakeBlock", "AnvilBlock", "AbstractCandleBlock",
          "AbstractCauldronBlock", "MossyCarpetBlock")
# Blocks with a plain cube model that are not plain blocks to place.
# The jigsaw has a block entity without `BaseEntityBlock`, which the open tables read it from.
SKIP = ("minecraft:test_instance_block", "minecraft:jigsaw")
SPIN_Y = [(0, y) for y in (0, 90, 180, 270)]
SPIN_XY = [(x, y) for y in (0, 90, 180, 270) for x in (0, 90, 180, 270)]
HORIZONTAL = {"north": 0, "east": 90, "south": 180, "west": 270}
FACES = ("down", "up", "north", "south", "west", "east")
TOOLS = ("pickaxe", "axe", "shovel", "hoe")
LEVELS = (("stone", 1), ("iron", 2), ("diamond", 3))
CONNECTED_HEADER = ("name", "classes", "hardness", "resistance", "map_color", "sound", "tool", "needs",
                    "requires_tool", "family", "textures")
# The family of each class, and the template its first model part uses.
FAMILIES = (("WallBlock", "wall", "template_wall_post", ("wall",)),
            ("FenceGateBlock", "gate", "template_fence_gate", ("texture",)),
            ("FenceBlock", "fence", "fence_post", ("texture",)),
            ("IronBarsBlock", None, None, None))
OPENING_FAMILIES = (("DoorBlock", "door", "door_bottom_left", ("top", "bottom")),
                    ("TrapDoorBlock", "trapdoor", "template_trapdoor_bottom", ("texture",)),
                    ("ButtonBlock", "button", "button", ("texture",)),
                    ("WeightedPressurePlateBlock", "weighted_plate", "pressure_plate_up", ("texture",)),
                    ("PressurePlateBlock", "plate", "pressure_plate_up", ("texture",)))
HEADER = ("name", "classes", "hardness", "resistance", "emission", "lit_emission", "opacity",
          "friction", "map_color", "sound", "tool", "needs", "requires_tool", "layout") + FACES + ("lit_north", "speed_factor", "push", "replaceable",
                                       "random_ticks", "ignited_by_lava", "block_entity", "item")


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
        self.translucent = False

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
            if isinstance(v, dict):
                # A sprite drawn translucent (glass): its name, the flag in `self.translucent`.
                self.translucent |= bool(v.get("force_translucent"))
                v = v["sprite"]
            if v is None or depth > 8:
                return None
            return value(v[1:], depth + 1) if v.startswith("#") else strip(v).removeprefix("block/")

        elements = next((m["elements"] for m in chain if "elements" in m), None)
        faces = {}
        for face in FACES:
            ref = elements[0]["faces"][face]["texture"] if elements else f"#{face}"
            faces[face] = value(ref[1:])
        return faces

    def parents(self, name):
        """The model and its parents' names, without namespaces."""
        names = []
        while name:
            names.append(strip(name).removeprefix("block/"))
            name = self.load(name).get("parent")
        return names

    def tinted(self, name):
        """Whether the model's faces carry a tint index (Minecraft's leaves model)."""
        model = self.load(name)
        while "elements" not in model and model.get("parent"):
            model = self.load(model["parent"])
        return any("tintindex" in f for f in model.get("elements", [{}])[0].get("faces", {}).values())

    def element(self, name, whole=True):
        """The model's one unturned element, when it has one: a full cube with six faces when
        `whole`, else any box with any faces (a mushroom block's single face)."""
        model = self.load(name)
        while "elements" not in model and model.get("parent"):
            model = self.load(model["parent"])
        elements = model.get("elements", [])
        if len(elements) != 1 or "rotation" in elements[0]:
            return None
        e = elements[0]
        if whole and (e["from"] != [0, 0, 0] or e["to"] != [16, 16, 16] or set(e["faces"]) != set(FACES)):
            return None
        return e

    def full_cube(self, name):
        return self.element(name)

    def elements(self, name):
        """The model's elements, each as `from;to;rotation;shade;down;up;north;south;west;east`:
        corners `x,y,z`, the rotation `origin x,y,z,axis,angle,rescale` (or `-`), `1` or `0`
        for shade, and each face as `texture/u1,v1,u2,v2/rotation/tintindex/cullface` (`-` for
        a face it lacks, and for an unset UV or cull face); then whether it uses ambient
        occlusion first."""
        chain = self.chain(name)
        variables = {}
        for model in reversed(chain):
            variables.update(model.get("textures", {}))
        elements = next((m["elements"] for m in chain if "elements" in m), [])
        ao = next((m["ambientocclusion"] for m in chain if "ambientocclusion" in m), True)
        out = ["1" if ao else "0"]
        for e in elements:
            r = e.get("rotation")
            if r is None:
                rotation = "-"
            else:
                if "angle" not in r:
                    sys.exit(f"{name}: a rotation on more than one axis")
                rotation = ",".join([*(number(v) for v in r["origin"]), r["axis"], number(r["angle"]),
                                     "1" if r.get("rescale") else "0"])
            parts = [",".join(number(v) for v in e["from"]), ",".join(number(v) for v in e["to"]), rotation,
                     "1" if e.get("shade", True) else "0"]
            if e.get("light_emission"):
                sys.exit(f"{name}: an element giving light")
            for d in FACES:
                f = e["faces"].get(d)
                if f is None:
                    parts.append("-")
                    continue
                texture = self.resolve(variables, f["texture"][1:])
                uv = ",".join(number(v) for v in f["uv"]) if "uv" in f else "-"
                parts.append(f"{texture}/{uv}/{f.get('rotation', 0)}/{f.get('tintindex', -1)}/{f.get('cullface', '-')}")
            out.append(";".join(parts))
        return out

    def chain(self, name):
        models = []
        while name:
            model = self.load(name)
            models.append(model)
            name = model.get("parent")
        return models

    def resolve(self, variables, var, depth=0):
        v = variables.get(var)
        if isinstance(v, dict):
            v = v["sprite"]
        if v is None or depth > 8:
            sys.exit(f"unresolved texture #{var}")
        return self.resolve(variables, v[1:], depth + 1) if v.startswith("#") else strip(v).removeprefix("block/")

    def variant_cubes(self, block, shaped=False):
        """The block-state variants as (key, [(model, x, y, uvlock)]) when every model is one
        full cube, else None. When `shaped`, models may have any elements, and multipart
        conditions any values (`a|b`) and alternatives (written `or` between them)."""
        path = self.root / "blockstates" / f"{block}.json"
        if not path.exists():
            return None
        definition = json.loads(path.read_text())
        variants = definition.get("variants")
        out = []

        def apply(v):
            return (strip(v["model"]).removeprefix("block/"), v.get("x", 0), v.get("y", 0), bool(v.get("uvlock")))
        if not variants:
            # A multipart of single boxes whose conditions are plain values (mushroom blocks).
            for part in definition.get("multipart", []):
                when, v = part.get("when", {}), part["apply"]
                if isinstance(v, list) or not (shaped or self.element(v["model"], whole=False)):
                    return None
                alternatives = when.get("OR", [when])
                if not shaped and ("OR" in when or any("|" in value for value in when.values())):
                    return None
                if "AND" in when or any(not isinstance(value, str) for a in alternatives for value in a.values()):
                    return None
                key = " or ".join(",".join(f"{k}={value}" for k, value in a.items()) for a in alternatives)
                out.append(("part", key, [apply(v)]))
            return out or None
        for key, variant in variants.items():
            variant = variant if isinstance(variant, list) else [variant]
            if any("weight" in v for v in variant):
                return None
            for v in variant:
                if not (shaped or self.full_cube(v["model"])):
                    return None
            out.append(("variant", key, [apply(v) for v in variant]))
        return out

    def plain_cube(self, name):
        """Whether the model is one unturned full cube with all six faces."""
        model = self.load(name)
        while "elements" not in model and model.get("parent"):
            model = self.load(model["parent"])
        elements = model.get("elements", [])
        if len(elements) != 1:
            return False
        e = elements[0]
        if e["from"] != [0, 0, 0] or e["to"] != [16, 16, 16] or "rotation" in e:
            return False
        return set(e["faces"]) == set(FACES) and all(
            f.get("uv", [0, 0, 16, 16]) == [0, 0, 16, 16] and "rotation" not in f and f.get("cullface") == d
            for d, f in e["faces"].items()
        )

    def plain_layout(self, block):
        """`cube`, `spin_y`, `spin_xy` or `facing` when every state of the block draws one plain
        cube model as that layout does, else None."""
        path = self.root / "blockstates" / f"{block}.json"
        if not path.exists():
            return None
        variants = json.loads(path.read_text()).get("variants")
        if not variants:
            return None
        models = set()
        turns = set()
        for key, variant in variants.items():
            variant = variant if isinstance(variant, list) else [variant]
            models |= {v["model"] for v in variant}
            turns.add((key, tuple((v.get("x", 0), v.get("y", 0)) for v in variant)))
            if any(v.get("uvlock") or "weight" in v for v in variant):
                return None
        if len(models) != 1 or not self.plain_cube(next(iter(models))):
            return None
        keys = {key for key, _ in turns}
        if len(turns) == 1:
            ((_, t),) = turns
            return {((0, 0),): "cube", tuple(SPIN_Y): "spin_y", tuple(SPIN_XY): "spin_xy"}.get(t)
        if keys == {f"facing={f}" for f in HORIZONTAL} and all(
            t == ((0, HORIZONTAL[key.split("=")[1]]),) for key, t in turns
        ):
            return "facing"
        return None

    def variant_model(self, block, key):
        state = json.loads((self.root / "blockstates" / f"{block}.json").read_text())
        variant = state["variants"][key]
        return (variant[0] if isinstance(variant, list) else variant)["model"]


def sound_kind(name, sound):
    kind = strip(sound["break"]["id"]).removeprefix("block.").removesuffix(".break")
    for event in ("break", "step", "place", "hit", "fall"):
        # Some types leave an event silent (resin's hit).
        if strip(sound[event]["id"]) not in (f"block.{kind}.{event}", "intentionally_empty"):
            sys.exit(f"{name}: sound {sound} does not follow block.<type>.<event>")
    return kind


def all_models(models, block):
    """Every model a block state definition names, in its order."""
    state = json.loads((models.root / "blockstates" / f"{block}.json").read_text())
    parts = [p["apply"] for p in state["multipart"]] if "multipart" in state else state["variants"].values()
    out = []
    for part in parts:
        model = (part[0] if isinstance(part, list) else part)["model"]
        if model not in out:
            out.append(model)
    return out


def sprite(value):
    """A texture variable's value: a name, or a sprite object (glass panes)."""
    value = value["sprite"] if isinstance(value, dict) else value
    return strip(value).removeprefix("block/")


def connected_rows(resources, families=FAMILIES):
    behaviours = json.loads((resources / "block-behaviours.json").read_text())
    names = [row[0] for row in json.loads((resources / "facts.json").read_text())["states"]]
    gameplay = json.loads((resources / "gameplay-facts.json").read_text())["states"]
    tags = json.loads((resources / "block-tags.json").read_text())
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
        family = next((f for f in families if f[0] in classes), None)
        if family is None:
            continue
        names = all_models(models, short)
        chain = [parent for model in names for parent in models.parents(model)]
        if family[1] is None:
            kind = "bars" if "template_bars_post_ends" in chain else "pane"
            if kind == "pane" and "template_glass_pane_post" not in chain:
                continue
            variables = ("bars", "edge") if kind == "bars" else ("pane", "edge")
        else:
            kind, template, variables = family[1:]
            if kind == "trapdoor" and "template_orientable_trapdoor_bottom" in chain:
                kind, template = "orientable_trapdoor", "template_orientable_trapdoor_bottom"
            if template not in chain:
                continue  # its own templates (bamboo)
        textures = {}
        for model in names:
            for parent in reversed(models.parents(model)):
                textures.update(models.load(f"block/{parent}").get("textures", {}))
        facts = gameplay[first[name]]
        out.append((
            short, ",".join(classes), number(facts["hardness"]), number(facts["explosion_resistance"]),
            f"{facts['map_color']:06x}", sound_kind(name, facts["sound"]), tagged.get(name, "-"),
            str(needs.get(name, 0)), "1" if facts["requires_tool"] else "0", kind,
            ",".join(sprite(textures[v]) for v in variables),
        ))
    return out


def rows(resources):
    behaviours = json.loads((resources / "block-behaviours.json").read_text())
    vanilla_facts = json.loads((resources / "facts.json").read_text())
    names = [row[0] for row in vanilla_facts["states"]]
    props = [row[4].get("properties", {}) for row in vanilla_facts["states"]]
    defaults = vanilla_facts["defaults"]
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
    variant_rows, model_rows = [], {}
    for block in behaviours["blocks"]:
        name, classes = block["name"], block["class"]
        short = strip(name)
        plain = False
        if classes[0] == "Block" and counts[name] == 1:
            layout, key = "cube", ""
        elif classes[0] == "RotatedPillarBlock":
            layout, key = "axis", "axis=y"
        elif short in FURNACES:
            layout, key = "facing", "facing=north,lit=false"
        elif name not in SKIP and (layout := models.plain_layout(short)):
            plain, key = True, "facing=north" if layout == "facing" else ""
        elif any(c in SHAPED for c in classes) and (found := models.variant_cubes(short, shaped=True)):
            layout, plain = "shaped", True
            state = props[defaults[name]]
            key = next((k for kind, k, _ in found if kind == "variant" and all(
                state.get(p) == v for p, v in (c.split("=") for c in k.split(",") if c))), None)
        elif name not in SKIP and (found := models.variant_cubes(short)):
            layout, plain = "variants", True
            state = props[defaults[name]]
            key = next((k for kind, k, _ in found if kind == "variant" and all(
                state.get(p) == v for p, v in (c.split("=") for c in k.split(",") if c))), None)
        else:
            continue
        states = gameplay[first[name]:first[name] + counts[name]]
        if layout == "cube" and classes[0] in SUNKEN:
            # The outline stays a full cube; the collision box is checked by open_tables.
            if not all(s["outline"] == [[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]] for s in states):
                sys.exit(f"{name}: not a sunken cube")
            layout = "sunken"
        elif layout == "shaped":
            pass
        elif not all(s["full_collision"] for s in states):
            if plain:
                continue
            sys.exit(f"{name}: not a full cube")
        if layout == "variants" and not all(s["solid_render"] for s in states):
            continue
        if layout != "shaped" and not all(s["solid_render"] for s in states):
            if not plain or layout != "cube":
                sys.exit(f"{name}: not a full opaque cube")
            layout = "clear"
        facts = states[0]
        lit_emission = facts["emission"]
        if plain:
            # Several states (leaves' distance, a lit ore): the default's facts, the same in every
            # state but for light, which only `lit` may change.
            facts = gameplay[defaults[name]]
            keep = ("hardness", "explosion_resistance", "dampening", "friction", "map_color",
                    "requires_tool", "sound", "solid_render", "emission")
            if layout == "shaped":
                # Their shapes give light dampening and solid rendering, state by state.
                keep = ("hardness", "explosion_resistance", "friction", "map_color", "requires_tool",
                        "sound", "emission", "push", "replaceable", "random_ticks", "ignited_by_lava",
                        "block_entity", "item")
            lit_emission = facts["emission"]
            by_count = {}
            for i, s in enumerate(states, first[name]):
                if props[i].get("lit") == "true":
                    lit_emission = s["emission"]
                    # Candles light by their count.
                    by_count[props[i].get("candles")] = s["emission"]
                    s = {**s, "emission": facts["emission"]}
                if any(s[k] != facts[k] for k in keep):
                    break
            else:
                keep = None
            if keep:
                if layout in ("variants", "shaped"):
                    # Light by charge or level (respawn anchor): not a family of this file.
                    continue
                sys.exit(f"{name}: its states' facts differ")
        elif layout == "facing":
            # lit=true comes first among each facing's two states.
            lit, unlit = states[0], states[1]
            facts = unlit
            lit_emission = lit["emission"]
        kind = sound_kind(name, facts["sound"])
        models.translucent = False
        if layout == "shaped":
            # Its particle texture names the block's sprite on every face.
            model = models.variant_model(short, key) if key is not None else found[0][2][0][0]
            model = model if "/" in model else f"block/{model}"
            variables = {}
            for m in reversed(models.chain(model)):
                variables.update(m.get("textures", {}))
            faces = dict.fromkeys(FACES, models.resolve(variables, "particle"))
        else:
            model = models.variant_model(short, key) if key is not None else f"block/{short}_inventory"
            faces = models.textures(model)
        if models.translucent:
            if layout != "clear":
                sys.exit(f"{name}: a translucent sprite on an opaque block")
            layout = "glass"
        elif layout == "clear" and models.tinted(models.variant_model(short, key)):
            layout = "leaves"
        if layout == "axis" and "cube_column_horizontal" in models.parents(models.variant_model(short, "axis=x")):
            # Lying down, its top turns half a turn (Minecraft's horizontal column).
            layout = "pillar"
        if None in faces.values():
            sys.exit(f"{name}: unresolved texture in {faces}")
        if layout in ("variants", "shaped"):
            for kind, k, variant in found:
                for model, x, y, uvlock in variant:
                    variant_rows.append((kind, short, k or "-", model, str(x), str(y), "1" if uvlock else "0"))
                    model_rows.setdefault(model, ("model", model, *models.elements(f"block/{model}")))
        lit_north = "-"
        if layout == "facing" and short not in FURNACES:
            lit_north = faces["north"]
        elif layout == "facing":
            lit_north = models.textures(models.variant_model(short, "facing=north,lit=true"))["north"]
        out.append((
            short, ",".join(classes), number(facts["hardness"]), number(facts["explosion_resistance"]),
            str(facts["emission"]),
            ",".join(str(by_count[c]) for c in sorted(by_count)) if plain and len(set(by_count.values())) > 1
            else str(lit_emission),
            str(facts["dampening"]), number(facts["friction"]),
            f"{facts['map_color']:06x}", kind, tagged.get(name, "-"), str(needs.get(name, 0)),
            "1" if facts["requires_tool"] else "0", layout, *(faces[f] for f in FACES), lit_north,
            number(facts["speed_factor"]), facts["push"], *("1" if facts[k] else "0" for k in (
                "replaceable", "random_ticks", "ignited_by_lava", "block_entity")), strip(facts["item"]),
        ))
    return out, [*sorted(model_rows.values()), *variant_rows]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--resources", type=Path, default=RESOURCES)
    parser.add_argument("--check", action="store_true", help="fail if the committed file differs")
    args = parser.parse_args()
    cubes, variants = rows(args.resources)
    files = (
        (OUT, [HEADER, *cubes]),
        (VARIANTS_OUT, variants),
        (CONNECTED, [CONNECTED_HEADER, *connected_rows(args.resources)]),
        (OPENINGS, [CONNECTED_HEADER, *connected_rows(args.resources, OPENING_FAMILIES)]),
    )
    for out, lines in files:
        text = "".join("\t".join(row) + "\n" for row in lines)
        if args.check:
            if not out.exists() or out.read_text() != text:
                sys.exit(f"{out.relative_to(ROOT)} is stale: run tools/open_block_facts.py")
            print(f"{out.relative_to(ROOT)} is up to date")
            continue
        out.write_text(text)
        print(f"wrote {out.relative_to(ROOT)}: {text.count(chr(10))} lines")


if __name__ == "__main__":
    main()
