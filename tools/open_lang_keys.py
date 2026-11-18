#!/usr/bin/env python3
"""Write the translation keys Horizon Open's English covers (docs/open-creative.md).

Reads the pinned version's English language file from the player's own resources
(`.local/horizon-client/26.3/assets/minecraft/lang/en_us.json`) and writes
`crates/horizon-open-content/data/lang/keys.txt`: one key per line, sorted, then a tab and its
placeholders in order (`%s %s`, `%2$s %1$s`) when it has any. Keys the generator already makes
from the registry name lists (block, item, entity, effect, attribute, profession and statistic
names: `lang.rs` `registry_keys`) are left out.

Only keys and placeholder order are written: none of the English text.

    python3 tools/open_lang_keys.py [--lang FILE] [--check]
"""

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "crates" / "horizon-open-content" / "data"
LANG = ROOT / ".local" / "horizon-client" / "26.3" / "assets" / "minecraft" / "lang" / "en_us.json"

# Java's format specifiers as the client reads them: `%%` is a literal percent sign.
SPEC = re.compile(r"%%|%(\d+\$)?([a-zA-Z])")


def placeholders(text):
    return [m.group(0) for m in SPEC.finditer(text) if m.group(0) != "%%"]


def names(registry):
    path = DATA / "registries" / f"{registry}.txt"
    return [line.strip() for line in path.read_text().splitlines() if line.strip()]


def registry_keys():
    """The keys `lang.rs` `registry_keys` writes from the name lists."""
    blocks = names("block")
    block_set = set(blocks)
    keys = {f"block.minecraft.{b}" for b in blocks}
    keys |= {f"item.minecraft.{i}" for i in names("item") if i not in block_set}
    keys |= {f"entity.minecraft.{e}" for e in names("entity_type")}
    keys |= {f"effect.minecraft.{e}" for e in names("mob_effect")}
    keys |= {f"attribute.name.{a}" for a in names("attribute")}
    keys |= {f"entity.minecraft.villager.{p}" for p in names("villager_profession")}
    keys |= {f"stat.minecraft.{s}" for s in names("custom_stat")}
    keys |= {f"stat_type.minecraft.{s}" for s in names("stat_type")}
    return keys


def lines(lang):
    made = registry_keys()
    out = []
    for key in sorted(lang):
        signature = placeholders(lang[key])
        if key in made and not signature:
            continue
        out.append(key + ("\t" + " ".join(signature) if signature else ""))
    return "".join(line + "\n" for line in out)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--lang", type=Path, default=LANG)
    parser.add_argument("--check", action="store_true", help="fail if the committed list differs")
    args = parser.parse_args()
    text = lines(json.loads(args.lang.read_text(encoding="utf-8")))
    target = DATA / "lang" / "keys.txt"
    if args.check:
        if target.read_text() != text:
            sys.exit(f"{target} is out of date: run tools/open_lang_keys.py")
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text)
    print(f"{target.relative_to(ROOT)}: {text.count(chr(10))} keys")


if __name__ == "__main__":
    main()
