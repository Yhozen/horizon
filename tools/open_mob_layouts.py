#!/usr/bin/env python3
"""Write the layout facts of Horizon Open's creatures (docs/open-creative.md, "Mobs").

Reads the prepared vanilla tables (`.local/horizon-client/26.3`: `entity-models.json`,
`entity-types.json`) and the names and pixel sizes of the client jar's entity textures
(`.local/minecraft/26.3/jars/client.jar`), and writes
`crates/horizon-open-content/data/mobs/creatures.txt`, which `src/creatures.rs` builds the
creatures from. Only numbers and identifiers are written: no texture, model or other
Mojang file. Each line is tab-separated:

- `mob NAME WIDTH HEIGHT EYE_HEIGHT CATEGORY FIRE_IMMUNE`: an entity type and its hit box
  (`thing` for one that is not a mob: a boat, minecart, arrow or armor stand);
- `attr NAME VALUE`: one of its default attributes;
- `model KEY TEXTURE_WIDTH TEXTURE_HEIGHT`: a model layer it draws (`entity-models.json` key);
- `part PATH X Y Z XROT YROT ZROT XSCALE YSCALE ZSCALE`: a part of the last model (its path
  from the root, `/`-separated, `.` for the root) and its pose;
- `box X Y Z W H D U V GROWX GROWY GROWZ MIRROR FACES`: a box of the last part, in Minecraft's
  box layout: its corner, size, texture origin, growth, whether it is mirrored and which of its
  six faces it has (`tbrfls`: top, bottom, right, front, left, back; `-` for a missing one);
- `texture PATH WIDTH HEIGHT MODEL`: a texture the renderer may ask for, under `textures/`, the
  size of Minecraft's, and the model whose layout it wears.

The boxes are recovered from the baked polygons; `--check` rebuilds every polygon from them
and fails unless each equals vanilla's (positions and texture coordinates), so the open
model is Minecraft's layout exactly.

    python3 tools/open_mob_layouts.py [--resources DIR] [--check]
"""

import argparse
import fnmatch
import json
import struct
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "crates" / "horizon-open-content" / "data" / "mobs" / "creatures.txt"
RESOURCES = ROOT / ".local" / "horizon-client" / "26.3"
CLIENT_JAR = ROOT / ".local" / "minecraft" / "26.3" / "jars" / "client.jar"
EPS = 1e-4

# Each creature: its model layers and the texture patterns (under textures/entity/) it may
# draw, each with the model whose layout it wears (None: the layer the name implies, the
# `_baby` model for `*_baby` textures and those under `baby/`).
CREATURES = {
    "villager": (["villager#main", "villager_baby#main", "villager_no_hat#main", "villager_baby_no_hat#main"],
                 [("villager/*", None)]),
    "wandering_trader": (["wandering_trader#main"], [("wandering_trader/*", None)]),
    "zombie_villager": (["zombie_villager#main", "zombie_villager_baby#main", "zombie_villager_no_hat#main",
                         "zombie_villager_baby_no_hat#main"], [("zombie_villager/*", None)]),
    "iron_golem": (["iron_golem#main"], [("iron_golem/*", None)]),
    "snow_golem": (["snow_golem#main"], [("snow_golem/*", None)]),
    "witch": (["witch#main"], [("witch/*", None)]),
    "pillager": (["pillager#main"], [("illager/pillager", None)]),
    "vindicator": (["vindicator#main"], [("illager/vindicator", None)]),
    "evoker": (["evoker#main"], [("illager/evoker", None)]),
    "horse": (["horse#main", "horse_baby#main"], [("horse/horse_[bcdgmw]*", None)]),
    "donkey": (["donkey#main", "donkey_baby#main"], [("horse/donkey*", None)]),
    "mule": (["mule#main", "mule_baby#main"], [("horse/mule*", None)]),
    "wolf": (["wolf#main", "wolf_baby#main"], [("wolf/wolf*", None)]),
    "cat": (["cat#main", "cat_baby#main", "cat#collar", "cat_baby#collar"],
            [("cat/cat_collar*", "cat#collar"), ("cat/cat_*", None)]),
    "ocelot": (["ocelot#main", "ocelot_baby#main"], [("cat/ocelot*", None)]),
    "fox": (["fox#main", "fox_baby#main"], [("fox/*", None)]),
    "rabbit": (["rabbit#main", "rabbit_baby#main"], [("rabbit/*", None)]),
    "bat": (["bat#main"], [("bat/*", None)]),
    "parrot": (["parrot#main"], [("parrot/*", None)]),
    "squid": (["squid#main", "squid_baby#main"], [("squid/squid*", None)]),
    "glow_squid": (["glow_squid#main", "glow_squid_baby#main"], [("squid/glow_squid*", None)]),
    "cod": (["cod#main"], [("fish/cod", None)]),
    "salmon": (["salmon#main", "salmon_small#main", "salmon_large#main"], [("fish/salmon", "salmon#main")]),
    "tropical_fish": (["tropical_fish_small#main", "tropical_fish_small#pattern", "tropical_fish_large#main",
                       "tropical_fish_large#pattern"],
                      [("fish/tropical_a_pattern_*", "tropical_fish_small#pattern"),
                       ("fish/tropical_b_pattern_*", "tropical_fish_large#pattern"),
                       ("fish/tropical_a", "tropical_fish_small#main"),
                       ("fish/tropical_b", "tropical_fish_large#main")]),
    "pufferfish": (["pufferfish_small#main", "pufferfish_medium#main", "pufferfish_big#main"],
                   [("fish/pufferfish", "pufferfish_big#main")]),
    "dolphin": (["dolphin#main", "dolphin_baby#main"], [("dolphin/*", None)]),
    "turtle": (["turtle#main", "turtle_baby#main"], [("turtle/*", None)]),
    "llama": (["llama#main", "llama_baby#main"], [("llama/llama_[bcgw]*", None)]),
    "goat": (["goat#main", "goat_baby#main"], [("goat/*", None)]),
    "bee": (["bee#main", "bee_baby#main"], [("bee/bee", None), ("bee/bee_baby", None), ("bee/bee_angry*", None), ("bee/bee_nectar*", None)]),
    "polar_bear": (["polar_bear#main", "polar_bear_baby#main"], [("bear/*", None)]),
    "panda": (["panda#main", "panda_baby#main"], [("panda/*", None)]),
    "frog": (["frog#main"], [("frog/*", None)]),
    "axolotl": (["axolotl#main", "axolotl_baby#main"], [("axolotl/*", None)]),
    "slime": (["slime#main", "slime#outer"], [("slime/slime", "slime#outer")]),
    "magma_cube": (["magma_cube#main"], [("slime/magmacube", None)]),
    "piglin": (["piglin#main", "piglin_baby#main"], [("piglin/piglin", None), ("piglin/piglin_baby", None)]),
    "piglin_brute": (["piglin_brute#main"], [("piglin/piglin_brute", None)]),
    "zombified_piglin": (["zombified_piglin#main", "zombified_piglin_baby#main"], [("piglin/zombified_piglin*", None)]),
    "hoglin": (["hoglin#main", "hoglin_baby#main"], [("hoglin/hoglin*", None)]),
    "zoglin": (["zoglin#main", "zoglin_baby#main"], [("hoglin/zoglin*", None)]),
    "strider": (["strider#main", "strider_baby#main"], [("strider/*", None)]),
    "ghast": (["ghast#main"], [("ghast/ghast", None), ("ghast/ghast_shooting", None)]),
    "blaze": (["blaze#main"], [("blaze/*", None)]),
    "wither_skeleton": (["wither_skeleton#main"], [("skeleton/wither_skeleton", None)]),
    "bogged": (["bogged#main", "bogged#outer"], [("skeleton/bogged", None), ("skeleton/bogged_overlay", "bogged#outer")]),
    "phantom": (["phantom#main"], [("phantom/*", None)]),
    "silverfish": (["silverfish#main"], [("silverfish/*", None)]),
    "endermite": (["endermite#main"], [("endermite/*", None)]),
    "guardian": (["guardian#main"], [("guardian/guardian", None)]),
    "elder_guardian": (["elder_guardian#main"], [("guardian/guardian_elder", None)]),
    "mooshroom": (["mooshroom#main", "mooshroom_baby#main"], [("cow/mooshroom_*", None)]),
    "trader_llama": (["trader_llama#main", "trader_llama_baby#main"], [("llama/llama_[bcgw]*", None)]),
    "zombie_horse": (["zombie_horse#main", "zombie_horse_baby#main"], [("horse/horse_zombie*", None)]),
    "skeleton_horse": (["skeleton_horse#main", "skeleton_horse_baby#main"], [("horse/horse_skeleton*", None)]),
    "camel": (["camel#main", "camel_baby#main"], [("camel/camel", None), ("camel/camel_baby", None)]),
    "sniffer": (["sniffer#main", "sniffer_baby#main"],
                [("sniffer/sniffer", None), ("sniffer/snifflet", "sniffer_baby#main")]),
    "armadillo": (["armadillo#main", "armadillo_baby#main"], [("armadillo/*", None)]),
    "allay": (["allay#main"], [("allay/*", None)]),
    "vex": (["vex#main"], [("illager/vex*", None)]),
    "ravager": (["ravager#main"], [("illager/ravager", None)]),
    "illusioner": (["illusioner#main"], [("illager/illusioner", None)]),
    "giant": (["giant#main"], [("zombie/zombie", None)]),
    "tadpole": (["tadpole#main"], [("tadpole/*", None)]),
    "shulker": (["shulker#main"], [("shulker/shulker*", None)]),
    "breeze": (["breeze#main", "breeze#eyes", "breeze#wind"],
               [("breeze/breeze", None), ("breeze/breeze_eyes", "breeze#eyes"), ("breeze/breeze_wind", "breeze#wind")]),
    "creaking": (["creaking#main", "creaking#eyes"],
                 [("creaking/creaking", None), ("creaking/creaking_eyes", "creaking#eyes")]),
    "warden": (["warden#main", "warden#bioluminescent", "warden#heart", "warden#pulsating_spots", "warden#tendrils"],
               [("warden/warden", None), ("warden/warden_bioluminescent_layer", "warden#bioluminescent"),
                ("warden/warden_heart", "warden#heart"), ("warden/warden_pulsating_spots_*", "warden#pulsating_spots")]),
    "wither": (["wither#main", "wither#armor"],
               [("wither/wither", None), ("wither/wither_invulnerable", None), ("wither/wither_armor", "wither#armor")]),
    "copper_golem": (["copper_golem#main", "copper_golem#eyes", "copper_golem_running#main", "copper_golem_sitting#main",
                      "copper_golem_star#main"],
                     [("copper_golem/copper_golem_eyes*", "copper_golem#eyes"), ("copper_golem/copper_golem*", None)]),
}

# The non-living entities, as `thing` lines (the same facts): boats, minecarts, arrows and
# armor stands.
WOODS = ["acacia", "birch", "cherry", "dark_oak", "jungle", "mangrove", "oak", "pale_oak", "poplar", "spruce"]
OBJECTS = {
    **{f"{wood}_boat": ([f"boat/{wood}#main", "boat#water_patch"], [(f"boat/{wood}", f"boat/{wood}#main")])
       for wood in WOODS},
    **{f"{wood}_chest_boat": ([f"chest_boat/{wood}#main", "boat#water_patch"],
                              [(f"chest_boat/{wood}", f"chest_boat/{wood}#main")]) for wood in WOODS},
    "bamboo_raft": (["boat/bamboo#main"], [("boat/bamboo", None)]),
    "bamboo_chest_raft": (["chest_boat/bamboo#main"], [("chest_boat/bamboo", None)]),
    **{cart: ([f"{cart}#main"], [("minecart/minecart", None)])
       for cart in ["minecart", "chest_minecart", "furnace_minecart", "hopper_minecart", "tnt_minecart",
                    "spawner_minecart", "command_block_minecart"]},
    "arrow": (["arrow#main"], [("projectiles/arrow", None), ("projectiles/arrow_tipped", None)]),
    "spectral_arrow": (["arrow#main"], [("projectiles/arrow_spectral", None)]),
    "armor_stand": (["armor_stand#main", "armor_stand_small#main"], [("armorstand/*", None)]),
}

FACES = "tbrfls"


def png_size(data):
    return struct.unpack(">II", data[16:24])


def textures():
    with zipfile.ZipFile(CLIENT_JAR) as jar:
        out = {}
        prefix = "assets/minecraft/textures/entity/"
        for name in jar.namelist():
            if name.startswith(prefix) and name.endswith(".png"):
                with jar.open(name) as f:
                    out[name[len(prefix):-4]] = png_size(f.read(24))
        return out


def classify(poly, normal, last=-1):
    """The face slot (0..5, `FACES`) of a polygon and whether its box is mirrored (None if
    the polygon doesn't say)."""
    xs = [v[0] for v in poly]
    us = [v[3] for v in poly]
    zs = [v[2] for v in poly]
    nx, ny, nz = (round(c) for c in normal)

    def slope(pos):
        # du/dpos between the two vertices furthest apart along pos.
        lo = min(range(4), key=lambda i: pos[i])
        hi = max(range(4), key=lambda i: pos[i])
        if abs(pos[hi] - pos[lo]) < EPS or abs(us[hi] - us[lo]) < 1e-9:
            return 0
        return 1 if (us[hi] - us[lo]) * (pos[hi] - pos[lo]) > 0 else -1

    if ny:
        s = slope(xs)
        return (0 if ny < 0 else 1), (None if s == 0 else s < 0)
    if nz:
        s = slope(xs)
        if nz < 0:
            return 3, (None if s == 0 else s < 0)
        return 5, (None if s == 0 else s > 0)
    if nx:
        s = slope(zs)
        if s == 0:
            # A side of no depth: its slot is the next in the box's order.
            return (2 if last < 2 else 4), None
        slot = 2 if s < 0 else 4
        return slot, (slot == 2) == (nx > 0)
    return None, None


def recover(polys, normals, size):
    """The boxes of one part, from its baked polygons (each box's faces in slot order)."""
    tw, th = size
    groups, last = [], 99
    for poly, normal in zip(polys, normals):
        slot, mirror = classify(poly, normal, last)
        if slot is None:
            # A degenerate face (zero area, no normal): vanilla's next slot in this box.
            slot = last + 1 if last < 5 else 0
        if slot <= last:
            groups.append([])
        if not groups:
            groups.append([])
        groups[-1].append((slot, mirror, poly))
        last = slot
    boxes = []
    for group in groups:
        rect = {}
        mirror = False
        for slot, m, poly in group:
            us = [v[3] * tw for v in poly]
            vs = [v[4] * th for v in poly]
            rect[slot] = (min(us), min(vs), max(us), max(vs))
            if m:
                mirror = True
        pts = [v for _, _, poly in group for v in poly]
        lo = [min(p[i] for p in pts) for i in range(3)]
        hi = [max(p[i] for p in pts) for i in range(3)]
        # Size and texture origin from whichever faces the box has.
        w = d = h = None
        u = v = None
        if 0 in rect:
            u0, v0, u1, v1 = rect[0]
            w, d = u1 - u0, v1 - v0
            u, v = u0 - d, v0
        if 1 in rect and w is None:
            u0, v0, u1, v1 = rect[1]
            w, d = u1 - u0, v1 - v0
            u, v = u0 - d - w, v0
        for slot in (3, 2, 4, 5):
            if slot not in rect:
                continue
            u0, v0, u1, v1 = rect[slot]
            if slot in (3, 5):
                w = u1 - u0 if w is None else w
            else:
                d = u1 - u0 if d is None else d
            h = v1 - v0
            if d is None:
                d = hi[2] - lo[2]
            if w is None:
                w = hi[0] - lo[0]
            if u is None:
                u = {3: u0 - d, 2: u0, 4: u0 - d - w, 5: u0 - 2 * d - w}[slot]
                v = v0 - d
            break
        if h is None:
            h = hi[1] - lo[1]
        dims = [w, h, d]
        grow = [((hi[i] - lo[i]) - dims[i]) / 2 for i in range(3)]
        faces = "".join(FACES[i] if i in rect else "-" for i in range(6))
        snap = lambda x: round(x, 4) + 0.0
        boxes.append({"min": [snap(lo[i] + grow[i]) for i in range(3)], "size": list(map(snap, dims)),
                      "uv": [snap(u), snap(v)], "grow": list(map(snap, grow)), "mirror": mirror, "faces": faces})
    return boxes


def texture_size(model):
    """The model's texture size: the one that makes most boxes' pixel sizes equal their
    model sizes."""
    best, score = None, -1
    for tw in (16, 32, 48, 64, 96, 128, 256):
        for th in (16, 32, 48, 64, 96, 128, 256):
            n = 0
            for part in walk(model):
                for b in recover(part[1]["polygons"], part[1]["normals"], (tw, th)):
                    n += all(abs(g) < EPS for g in b["grow"])
            if n > score:
                best, score = (tw, th), n
    return best


def walk(part, path="."):
    yield path, part
    for name, child in part["children"].items():
        yield from walk(child, name if path == "." else f"{path}/{name}")


# Rebuilding the polygons, as src/mobs.rs `faces` does.
def rebuild(b, size):
    tw, th = size
    x0, y0, z0 = (b["min"][i] - b["grow"][i] for i in range(3))
    x1, y1, z1 = (b["min"][i] + b["size"][i] + b["grow"][i] for i in range(3))
    if b["mirror"]:
        x0, x1 = x1, x0
    w, h, d = b["size"]
    u, v = b["uv"]
    faces = [
        ([[x1, y0, z1], [x0, y0, z1], [x0, y0, z0], [x1, y0, z0]], [u + d, v, u + d + w, v + d]),
        ([[x1, y1, z1], [x0, y1, z1], [x0, y1, z0], [x1, y1, z0]], [u + d + w, v, u + d + 2 * w, v + d]),
        ([[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]], [u, v + d, u + d, v + d + h]),
        ([[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]], [u + d, v + d, u + d + w, v + d + h]),
        ([[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]], [u + d + w, v + d, u + 2 * d + w, v + d + h]),
        ([[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]], [u + 2 * d + w, v + d, u + 2 * d + 2 * w, v + d + h]),
    ]
    out = []
    for i, (corners, (u0, v0, u1, v1)) in enumerate(faces):
        if b["faces"][i] == "-":
            continue
        uvs = [[u1, v0], [u0, v0], [u0, v1], [u1, v1]]
        out.append([[*c, uu / tw, vv / th] for c, (uu, vv) in zip(corners, uvs)])
    return out


def same_polygons(want, got):
    """Equal polygons, in any order and with their corners in any order, to 1e-4."""
    def key(poly):
        return sorted(poly, key=lambda v: tuple(round(c, 2) for c in v))

    def order(polys):
        return sorted(map(key, polys), key=lambda p: [round(c, 2) for v in p for c in v])
    if len(want) != len(got):
        return False
    return all(abs(a - b) <= 1e-4 for p, q in zip(order(want), order(got))
               for v, w in zip(p, q) for a, b in zip(v, w))


def degenerate(poly):
    return len({tuple(round(c, 4) for c in v[:3]) for v in poly}) < 4 or \
        len({tuple(round(c, 6) for c in v[3:]) for v in poly}) < 3


def number(x):
    r = round(x, 6)
    if r == int(r):
        return str(int(r))
    return repr(float(r)) if abs(r - x) < 1e-12 else repr(float(x))


def float32(x):
    return number(struct.unpack("f", struct.pack("f", x))[0])


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--resources", type=Path, default=RESOURCES)
    parser.add_argument("--check", action="store_true", help="fail if creatures.txt is not what this writes")
    args = parser.parse_args()
    models = json.loads((args.resources / "entity-models.json").read_text())
    types = json.loads((args.resources / "entity-types.json").read_text())
    tex = textures()
    lines, problems = [], []
    for mob, (keys, patterns) in [*CREATURES.items(), *OBJECTS.items()]:
        t = types[f"minecraft:{mob}"]
        kind = "mob" if mob in CREATURES else "thing"
        lines.append("\t".join([kind, mob, number(t["width"]), number(t["height"]), number(t["eye_height"]),
                                t["category"], "1" if t["fire_immune"] else "0"]))
        for name, value in sorted(t.get("attributes", {}).items()):
            lines.append(f"attr\t{name.removeprefix('minecraft:')}\t{number(value)}")
        stem = keys[0].split("#")[0]
        chosen = []
        for pattern, model in patterns:
            for name in sorted(tex):
                if fnmatch.fnmatchcase(name, pattern) and name not in [c for c, _ in chosen]:
                    baby = name.endswith("_baby") or "/baby/" in name
                    target = model or keys[0]
                    kind, layer = target.split("#")
                    if baby and f"{kind}_baby#{layer}" in keys:
                        target = f"{kind}_baby#{layer}"
                    chosen.append((name, target))
        sizes = {}
        for key in keys:
            model = models[f"minecraft:{key}"]
            # The size of Minecraft's texture for the layer, else the one its boxes imply.
            size = next((tex[n] for n, t in chosen if t == key), None) or texture_size(model)
            sizes[key] = size
            lines.append(f"model\t{key}\t{size[0]}\t{size[1]}")
            for path, part in walk(model):
                lines.append("\t".join(["part", path, *map(float32, part["pose"])]))
                boxes = recover(part["polygons"], part["normals"], size)
                rebuilt = [p for b in boxes for p in rebuild(b, size)]
                want = [p for p in part["polygons"] if not degenerate(p)]
                got = [p for p in rebuilt if not degenerate(p)]
                if not same_polygons(want, got):
                    problems.append(f"{key} {path}: the recovered boxes don't rebuild vanilla's polygons")
                for b in boxes:
                    lines.append("\t".join(["box", *map(number, b["min"]), *map(number, b["size"]),
                                            *map(number, b["uv"]), *map(number, b["grow"]),
                                            "1" if b["mirror"] else "0", b["faces"]]))
        for name, target in chosen:
            w, h = tex[name]
            tw, th = sizes[target]
            if w * th != h * tw:
                problems.append(f"{name}: {w}x{h} is not the shape of {target}'s {tw}x{th} layout")
            lines.append(f"texture\tentity/{name}\t{w}\t{h}\t{target}")
        if not chosen:
            problems.append(f"{mob}: no texture matches {patterns}")
    text = "".join(f"{line}\n" for line in lines)
    for p in problems:
        print(p, file=sys.stderr)
    if args.check:
        if not OUT.exists() or OUT.read_text() != text:
            sys.exit(f"{OUT.relative_to(ROOT)} is stale: run tools/open_mob_layouts.py")
    else:
        OUT.parent.mkdir(parents=True, exist_ok=True)
        OUT.write_text(text)
        print(f"wrote {OUT.relative_to(ROOT)}: {len(CREATURES)} creatures, {len(OBJECTS)} other entities, {len(lines)} lines")
    if problems:
        sys.exit(1)


if __name__ == "__main__":
    main()
