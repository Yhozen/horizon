#!/usr/bin/env python3
"""Horizon Open's tables against vanilla's, name by name (docs/open-creative.md).

The probe `crates/horizon-client/examples/open_parity_probe.rs` loads a resource folder
through the game's own loaders and writes every table keyed by name: registries (entry
to network id), blocks (properties, values in order, default state), block states (global
id and facts), shapes (collision, outline, occlusion boxes per state), items (default
components), entity types (width, height, eye height), entity model layers (part names),
attributes (id, default, range) and their entity defaults, the container screen textures,
every texture with its pixel size, and baked block models (a digest of each state's quads:
positions, UVs, facing, cull face, tint index, texture name; never pixels). It runs on the prepared vanilla resources (.local/horizon-client/26.3) and
on the open bundle, which it builds from this checkout's generator.

A name matches when its facts are equal, numbers included: a block state matches only if
open gives it vanilla's global id. What open still lacks is listed, sorted, one name per
line, in tools/open_parity/gaps/<category>.txt. The harness fails on a name outside its
list that is missing or differs, and on a listed name that now matches (remove it from the
list), so the lists only shrink.

  test_open_tables_parity.py [--write-gaps] [--examples N] [--bin-dir DIR]

--write-gaps rewrites the lists from this run (for the first version, or after a
deliberate change of what is compared). --bin-dir runs a prebuilt probe from
DIR/examples instead of building it: its open bundle then comes from the generator
it was built with.
"""
import argparse
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import lab

ROOT = Path(__file__).resolve().parent.parent
RESOURCES = lab.RESOURCES
GAPS = ROOT / 'tools' / 'open_parity' / 'gaps'
PROBE = 'open_parity_probe'
# The parity runner's probe settings (tools/parity.py PROFILE) are those of the fast profile.
BUILD = ['cargo', 'build', '--quiet', '--locked', '--profile', 'fast', '-p', 'horizon-client', '--example', PROBE]


def read_open_content():
    """Read the generator's content files, so the parity runner counts them as inputs (the probe
    reads them from Rust, which its tracer does not see)."""
    for path in sorted((ROOT / 'content' / 'open').rglob('*')):
        if path.is_file():
            path.read_bytes()


def run_probes(binary, out, open_root=None):
    """Both folders at once: vanilla's prepared resources and a freshly built open bundle (or the
    bundle already prepared in `open_root`)."""
    source = [str(open_root)] if open_root else ['--open']
    jobs = {
        'vanilla': subprocess.Popen([str(binary), str(RESOURCES), str(out / 'vanilla.json')]),
        'open': subprocess.Popen([str(binary), *source, str(out / 'open.json')]),
    }
    for name, job in jobs.items():
        if job.wait():
            sys.exit(f'{PROBE} failed on the {name} resources')
    return {name: json.loads((out / f'{name}.json').read_text()) for name in jobs}


def read_gaps(category):
    path = GAPS / f'{category}.txt'
    if not path.exists():
        return [], path
    return [line for line in path.read_text(encoding='utf-8').splitlines() if line], path


def differing_fields(want, got):
    if isinstance(want, dict) and isinstance(got, dict):
        return sorted(k for k in want.keys() | got.keys() if want.get(k) != got.get(k))
    return []


def compare(vanilla, open_, write_gaps, examples):
    failures = []
    rows = []
    for category, want in vanilla.items():
        if category.startswith('_'):
            continue
        got = open_.get(category, {})
        matching, different, missing = [], [], []
        for name, facts in want.items():
            if name not in got:
                missing.append(name)
            elif got[name] == facts:
                matching.append(name)
            else:
                different.append(name)
        extra = sorted(set(got) - set(want))
        rows.append((category, len(want), len(matching), len(different), len(missing), len(extra)))
        gaps = sorted(different + missing)
        listed, path = read_gaps(category)
        if write_gaps:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(''.join(f'{name}\n' for name in gaps), encoding='utf-8')
            continue
        if listed != sorted(set(listed)):
            failures.append(f'{path.relative_to(ROOT)}: not sorted, or a name is listed twice')
        listed = set(listed)
        new = [name for name in gaps if name not in listed]
        fixed = sorted(listed & set(matching))
        stale = sorted(listed - set(want))
        for name in new[:examples]:
            if name in got:
                fields = differing_fields(want[name], got[name])
                detail = ', '.join(f'{k}: vanilla {json.dumps(want[name].get(k))}, open {json.dumps(got[name].get(k))}'
                                   for k in fields) or f'vanilla {json.dumps(want[name])}, open {json.dumps(got[name])}'
                failures.append(f'{category}: {name} differs ({detail})')
            else:
                failures.append(f'{category}: {name} is missing from open')
        if len(new) > examples:
            failures.append(f'{category}: {len(new) - examples} more names missing or different, not in {path.name}')
        for name in fixed[:examples]:
            failures.append(f'{category}: {name} now matches: remove it from {path.relative_to(ROOT)}')
        if len(fixed) > examples:
            failures.append(f'{category}: {len(fixed) - examples} more listed names now match')
        for name in stale[:examples]:
            failures.append(f'{category}: {name} is not a vanilla name: remove it from {path.relative_to(ROOT)}')
    return rows, failures


def print_scoreboard(rows):
    header = ('category', 'vanilla', 'matching', 'different', 'missing', 'open extra')
    width = max(len(header[0]), *(len(r[0]) for r in rows))
    print(f'{header[0]:<{width}}' + ''.join(f'{h:>12}' for h in header[1:]))
    for row in rows:
        print(f'{row[0]:<{width}}' + ''.join(f'{v:>12}' for v in row[1:]))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--write-gaps', action='store_true', help='rewrite the gap lists from this run')
    parser.add_argument('--examples', type=int, default=10, help='failures shown per kind and category')
    parser.add_argument('--bin-dir', help='a cargo profile directory with a built probe (default: build it)')
    parser.add_argument('--open-root', help='compare this prepared open bundle (horizon-prepare open --out DIR) '
                        'instead of building one in the probe, so a probe built once serves generator changes')
    args = parser.parse_args()
    if not (RESOURCES / 'facts.json').exists():
        sys.exit(f'{RESOURCES} is missing: prepare the vanilla resources first (docs/parity-workflow.md)')
    started = time.monotonic()
    read_open_content()
    if args.bin_dir:
        binary = Path(args.bin_dir).resolve() / 'examples' / PROBE
    else:
        subprocess.run(BUILD, cwd=ROOT, check=True)
        binary = lab.cargo_target() / 'fast' / 'examples' / PROBE
    built = time.monotonic()
    with tempfile.TemporaryDirectory(prefix='open-tables-') as out:
        docs = run_probes(binary, Path(out), args.open_root and Path(args.open_root).resolve())
    ran = time.monotonic()
    rows, failures = compare(docs['vanilla'], docs['open'], args.write_gaps, args.examples)
    print_scoreboard(rows)
    seconds = {name: doc.get('_seconds', {}) for name, doc in docs.items()}
    print(f'build {built - started:.1f} s, probes {ran - built:.1f} s '
          f'(vanilla {seconds["vanilla"].get("total", 0):.1f} s, open {seconds["open"].get("total", 0):.1f} s '
          f'of which generating the bundle {seconds["open"].get("build", 0):.1f} s)')
    if args.write_gaps:
        print(f'wrote the gap lists in {GAPS.relative_to(ROOT)}')
        return
    for failure in failures:
        print('FAIL', failure)
    if failures:
        sys.exit(1)
    print('open tables: every difference is in a gap list')


if __name__ == '__main__':
    main()
