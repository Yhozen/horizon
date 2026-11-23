#!/usr/bin/env python3
"""Twin clients: an open player on a Java server against a Horizon player in the same place.

One vanilla 26.3 server (flat world, offline mode, normal difficulty, `spawn_mobs` off, ticks frozen
from the start: players still tick) serves one player name, Twin, an operator. The Horizon client (the
vanilla resources) joins it directly; the open client (a bundle from `horizon-prepare open`) joins
it through `horizon-open-relay`, never both at once. The console sets each scenario up while the
first Horizon login is in the world, then each client logs in again and is read with `horizon-ctl`
(`state`, `inventory`, ...). Their replies are compared by name, item by item.

Scenarios: `attributes` (every attribute the player has, given a non-default base, then a relog:
eye height, dimensions, health and the values); `blocks` (every block of blocks.json in its default
state, set with `setblock ... strict` from a datapack function in a layer above the spawn: the
state each client shows there); `items` (every item, 36 at a time in the inventory: item, count
and components each client shows); `entities` (every summonable entity type, without AI or
gravity, on a grid around the spawn: the types each client lists there).

Each scenario lists what it compares as `scenario:item` keys. `tools/open_parity/twin_gaps.txt`
lists the keys that still differ (one per line, sorted): a difference outside it fails, and so
does a listed key that now matches (remove it), so the list only shrinks.

    python3 tools/test_open_twin_parity.py
    python3 tools/test_open_twin_parity.py --bin-dir target/fast   # prebuilt binaries
    HORIZON_TWIN_BIN_DIR=target/fast python3 tools/parity.py open_twin

Without --bin-dir (or HORIZON_TWIN_BIN_DIR) it builds horizon-client, horizon-ctl,
horizon-open-relay and horizon-prepare with the fast profile.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

try:
    from . import lab, test_rust_parity as base
    from .test_dedicated_server_parity import Server
except ImportError:
    import lab
    import test_rust_parity as base
    from test_dedicated_server_parity import Server
sys.path.insert(0, str(Path(__file__).resolve().parent / 'open_parity'))
import twin_scripted  # noqa: E402

NAME = 'open-twin-parity'
PLAYER = 'Twin'
GAPS = lab.ROOT / 'tools/open_parity/twin_gaps.txt'
REPORTS = lab.LOCAL / 'datagen/out/reports'
PROPERTIES = {
    'online-mode': 'false',
    'enforce-secure-profile': 'false',
    'white-list': 'false',
    'level-seed': '12345',
    'level-type': 'minecraft\\:flat',
    'generate-structures': 'false',
    'difficulty': 'normal',  # peaceful refuses to summon monsters
    'gamemode': 'survival',
    'view-distance': '4',
    'simulation-distance': '4',
    'spawn-protection': '0',
    'max-players': '5',
    'pause-when-empty-seconds': '-1',
}
LAVAPIPE = Path('/run/opengl-driver/share/vulkan/icd.d/lvp_icd.x86_64.json')
# Half the side of the square around the spawn the scenarios use (whole chunks).
AREA = 32
# Floats closer than this are the same (the open client reads the same packets).
EPSILON = 1e-6


# ---------------------------------------------------------------- the server and the relay

class Console:
    """The server's console: a command and the reply it waits for."""

    def __init__(self, server):
        self.server = server

    def run(self, command, reply=None, timeout=60):
        start = len(self.server.lines)
        self.server.console(command)
        if reply is None:
            return None
        regex = re.compile(reply)
        deadline = time.time() + timeout
        while time.time() < deadline:
            for line in self.server.lines[start:]:
                if regex.search(line):
                    return line
            if self.server.process.poll() is not None:
                raise AssertionError(f'server exited during {command!r}')
            time.sleep(0.05)
        raise AssertionError(f'no reply matching {reply!r} to {command!r}:\n' + '\n'.join(self.server.lines[start:][-10:]))

    def wait_offline(self, timeout=60):
        """Waits until no player is online (the last one has been saved)."""
        deadline = time.time() + timeout
        while 'There are 0 of' not in self.run('list', r'There are \d+ of'):
            if time.time() > deadline:
                raise AssertionError(f'{PLAYER} is still online after {timeout} s')
            time.sleep(0.2)

    def sync(self):
        """Waits until the console has run everything sent before."""
        mark = f'sync {time.monotonic_ns()}'
        self.run(f'say {mark}', re.escape(f'[Server] {mark}'))


def phase(started, what):
    print(f'{time.time() - started:6.1f} s  {what}', flush=True)


def write_datapack(directory, functions):
    """The scenarios' functions as the world's datapack `twin` (twin:NAME)."""
    pack = directory / 'world/datapacks/twin'
    data = lab.version_info()['pack_version']['data_major']
    (pack / 'data/twin/function').mkdir(parents=True)
    (pack / 'pack.mcmeta').write_text(json.dumps({'pack': {'description': 'open_twin scenarios',
                                                           'min_format': data, 'max_format': data}}))
    for name, lines in functions.items():
        (pack / f'data/twin/function/{name}.mcfunction').write_text(''.join(f'{l}\n' for l in lines))


def launch_server(build, jars, libraries, functions):
    directory = build / 'server'
    shutil.rmtree(directory, ignore_errors=True)
    directory.mkdir(parents=True)
    write_datapack(directory, functions)
    port = lab.free_port()
    props = dict(PROPERTIES, **{'server-port': str(port)})
    (directory / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in props.items()), encoding='iso-8859-1')
    (directory / 'eula.txt').write_text('eula=true\n')
    (directory / 'ops.json').write_text(json.dumps([{'uuid': str(lab.offline_uuid(PLAYER)), 'name': PLAYER, 'level': 4,
                                                     'bypassesPlayerLimit': False}]))
    classpath = os.pathsep.join(map(str, [jars['server'], *libraries]))
    server = Server(['java', '-Xmx1g', '-Duser.timezone=UTC', '-cp', classpath, 'net.minecraft.server.Main', 'nogui'],
                    directory, 'server')
    server.port = port
    return server


def configure_server(server):
    server.wait_for(r'Done \(')
    console = Console(server)
    # Frozen from the start: placed sand stays up, summoned arrows stay put; players still tick.
    for command, reply in [('tick freeze', 'freeze|frozen'),
                           ('gamerule spawn_mobs false', 'spawn_mobs'),
                           # The console's commands are not echoed to the operator's chat.
                           ('gamerule log_admin_commands false', 'log_admin_commands'),
                           ('gamerule respawn_radius 0', 'respawn_radius'),
                           ('gamerule advance_time false', 'advance_time'),
                           ('gamerule advance_weather false', 'advance_weather'),
                           ('setworldspawn 0 -60 0', 'Set the world spawn'),
                           ('forceload add -48 -48 48 48', 'force loaded')]:
        console.run(command, reply)
    return console, server.port


def start_relay(build, relay, bundle, upstream):
    port = lab.free_port()
    log = (build / 'relay.log').open('w')
    process = subprocess.Popen([str(relay), 'serve', '--listen', f'127.0.0.1:{port}', '--upstream', f'127.0.0.1:{upstream}',
                                '--open-resources', str(bundle), '--vanilla-reports', str(REPORTS)],
                               stdout=log, stderr=subprocess.STDOUT)
    return process, port


def wait_relay(build, process):
    deadline = time.time() + 120
    while 'listening on' not in (build / 'relay.log').read_text(errors='replace'):
        if process.poll() is not None or time.time() > deadline:
            raise AssertionError('the relay did not start:\n' + (build / 'relay.log').read_text(errors='replace')[-3000:])
        time.sleep(0.1)


# ---------------------------------------------------------------- the clients

def display_env():
    """A display for the games: vscreen where there is one; `--offscreen` renders without showing it."""
    env = dict(os.environ, HORIZON_NO_AUDIO='1', HORIZON_TELEMETRY_URL='off', HORIZON_LAUNCHER_NO_SELF_UPDATE='1')
    vscreen = shutil.which('vscreen') or ('/tmp/vscreen-shim/vscreen' if os.path.exists('/tmp/vscreen-shim/vscreen') else None)
    if sys.platform != 'darwin' and vscreen:
        subprocess.run([vscreen, 'up'], env=env, check=False, capture_output=True)
        text = subprocess.run([vscreen, 'env'], env=env, capture_output=True, text=True).stdout
        for match in re.finditer(r'(\w+)=(\S+)', text.split('unset')[0]):
            env[match.group(1)] = match.group(2)
        if 'XDG_RUNTIME_DIR' in env:
            os.makedirs(env['XDG_RUNTIME_DIR'], mode=0o700, exist_ok=True)
    if LAVAPIPE.exists():
        env['VK_ICD_FILENAMES'] = str(LAVAPIPE)
    return env


class Client:
    """One game, launched and driven with horizon-ctl, in its own fresh folders."""

    def __init__(self, label, bins, resources, build, env):
        self.label, self.bins, self.resources = label, bins, resources
        self.home = Path(tempfile.mkdtemp(prefix=f'twin-{label}-'))
        self.env = dict(env)
        for key, sub in [('HORIZON_HOME', 'game'), ('HORIZON_LAUNCHER_HOME', 'launcher'), ('XDG_DATA_HOME', 'data'),
                         ('XDG_CONFIG_HOME', 'config')]:
            (self.home / sub).mkdir()
            self.env[key] = str(self.home / sub)
        self.socket = self.home / 'ctl.sock'
        self.build = build
        self.logins = 0
        self.pid = None

    def ctl(self, *words, timeout=300, check=True):
        for attempt in range(50):
            result = subprocess.run([str(self.bins / 'horizon-ctl'), '--socket', str(self.socket), *map(str, words)],
                                    cwd=self.home, env=self.env, capture_output=True, text=True, timeout=timeout)
            out = result.stdout.strip().splitlines()
            try:
                reply = json.loads(out[-1]) if out else {'error': result.stderr.strip()}
            except json.JSONDecodeError:
                reply = {'error': result.stdout[-2000:] + result.stderr[-2000:]}
            # A busy game can refuse a connection for a moment (a full accept queue).
            if reply.get('kind') != 'connection' or words[0] == 'launch' or not self.pid:
                break
            time.sleep(0.2)
        if check and (result.returncode or 'error' in reply):
            raise AssertionError(f'{self.label}: horizon-ctl {" ".join(map(str, words))}: {reply}\n{self.log_tail()}')
        return reply

    def ctl_async(self, *words):
        """A horizon-ctl command left running (read the game while it moves)."""
        return subprocess.Popen([str(self.bins / 'horizon-ctl'), '--socket', str(self.socket), *map(str, words)],
                                cwd=self.home, env=self.env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    def log_tail(self, lines=25):
        log = self.socket.with_suffix('.log')
        return '\n'.join(log.read_text(errors='replace').splitlines()[-lines:]) if log.exists() else ''

    def start(self):
        """Starts the game on its title screen (it loads its resources while the server starts)."""
        reply = self.ctl('launch', '--game', self.bins / 'horizon-client', '--offscreen', '--resources', self.resources,
                         '--render-distance', '2')
        self.pid = reply.get('pid')

    def login(self, address):
        self.logins += 1
        deadline = time.time() + 300
        while self.ctl('state').get('screen') != 'title':
            if time.time() > deadline:
                raise AssertionError(f'{self.label}: no title screen in 300 s\n{self.log_tail()}')
            time.sleep(0.2)
        self.ctl('connect', address, PLAYER)
        self.ctl('wait-ready', 120, timeout=180)
        # Every chunk the scenarios use has arrived: the flat world's bedrock is under all of them.
        low, high = -AREA, AREA - 1
        while self.ctl('blocks', low, -64, low, high, -64, high)['counts'].get('minecraft:bedrock', 0) < (2 * AREA) ** 2:
            if time.time() > deadline:
                raise AssertionError(f'{self.label}: the chunks around the spawn did not arrive')
            time.sleep(0.2)

    def wait_chat(self, text, timeout=30):
        """Waits until a chat line with `text` has arrived (and so everything the server sent before it)."""
        deadline = time.time() + timeout
        while text not in json.dumps(self.ctl('messages', 5)):
            if time.time() > deadline:
                raise AssertionError(f'{self.label}: no chat line {text!r} in {timeout} s')
            time.sleep(0.05)

    def catch_up(self, console):
        """Returns once the server has what this client sent so far (its chat line comes after its
        clicks), and this client has what the server sent so far: a chat line sent a few ticks after
        the console's commands ran comes after the slots and blocks they changed."""
        said = f'twin said {time.monotonic_ns()}'
        start = len(console.server.lines)
        self.ctl('chat', said)
        deadline = time.time() + 30
        while not any(said in line for line in console.server.lines[start:]):
            if time.time() > deadline:
                raise AssertionError(f'{self.label}: the server did not get the chat line {said!r}')
            time.sleep(0.05)
        console.sync()
        time.sleep(0.15)
        marker = f'twin {self.label} {time.monotonic_ns()}'
        console.run(f'say {marker}', re.escape(f'[Server] {marker}'))
        self.wait_chat(marker)

    def logout(self):
        self.ctl('disconnect')

    def close(self):
        if self.pid:
            self.ctl('quit', check=False, timeout=60)
            deadline = time.time() + 60
            while time.time() < deadline:
                try:
                    os.kill(self.pid, 0)
                except ProcessLookupError:
                    break
                time.sleep(0.1)
            else:
                os.kill(self.pid, 9)
            self.pid = None
        log = self.socket.with_suffix('.log')
        if log.exists():
            shutil.copy(log, self.build / f'{self.label}.log')
        shutil.rmtree(self.home, ignore_errors=True)


# ---------------------------------------------------------------- scenarios

def attribute_values():
    """A non-default base value within range for every vanilla attribute (vanilla's own table)."""
    import struct
    table = json.loads((lab.RESOURCES / 'attributes.json').read_text())['attributes']

    def number(bits):
        return struct.unpack('>d', bytes.fromhex(bits))[0]
    values = {}
    for a in table:
        low, default, high = number(a['min']), number(a['default']), number(a['max'])
        value = default + 0.5 if default + 0.5 <= high else (default + low) / 2
        values[a['id']] = round(value, 4)
    return values


class Attributes:
    """Every attribute the player has, given a non-default base, then a relog: eye height and values."""
    name = 'attributes'

    def setup(self, console):
        for attribute, value in attribute_values().items():
            # A player lacks the mobs' attributes (follow range, tempt range...): those are refused.
            console.run(f'attribute {PLAYER} {attribute} base set {value}',
                        r'(?i)base value for attribute|has no attribute|cannot|can.t')

    def reset(self, console):
        for attribute in attribute_values():
            console.server.console(f'attribute {PLAYER} {attribute} base reset')
        console.sync()

    def read(self, client, console):
        state = client.ctl('state')
        player = state['player']
        items = {
            'eye_height': round(player['eye'][1] - player['position'][1], 4),
            'position': [round(v, 4) for v in player['position']],
            'health': player['health'],
            'air': player['air'],
        }
        for key in ('pose', 'swimming', 'in_water', 'eye_height', 'dimensions'):
            if key in player:
                items[key] = player[key]
        for name, value in (player.get('attributes') or {}).items():
            items[name if ':' in name else f'minecraft:{name}'] = value
        return items


def registry(name):
    """A vanilla registry's names, in its order (the server's own report)."""
    return list(json.loads((REPORTS / 'registries.json').read_text())[name]['entries'])


def run_function(console, name):
    reply = console.run(f'function twin:{name}', r'Running function|Executed \d+ command|Unknown function')
    if 'Unknown' in reply:
        raise AssertionError(f'the datapack was not loaded: {reply}')


def grid(count, y, spacing=1, skip=()):
    """`count` positions in a square around the spawn, x fastest."""
    side = 1
    while (side * side - len(skip)) < count:
        side += 1
    low = -(side // 2) * spacing
    spots = [(low + i * spacing, y, low + j * spacing) for j in range(side) for i in range(side)]
    return [p for p in spots if (p[0], p[2]) not in skip][:count]


class Blocks:
    """Every block in its default state, set (`strict`: no updates) in a layer above the spawn."""
    name = 'blocks'
    Y = -50

    def __init__(self):
        report = json.loads((REPORTS / 'blocks.json').read_text())
        self.blocks = {}
        for block, info in report.items():
            if block in ('minecraft:air', 'minecraft:cave_air', 'minecraft:void_air'):
                continue
            default = next(s for s in info['states'] if s.get('default'))
            props = ','.join(f'{k}={v}' for k, v in sorted(default.get('properties', {}).items()))
            self.blocks[block] = f'{block}[{props}]' if props else block
        self.at = dict(zip(self.blocks, grid(len(self.blocks), self.Y)))

    def function(self):
        return [f'setblock {x} {y} {z} {state} strict' for block, state in self.blocks.items()
                for x, y, z in [self.at[block]]]

    def setup(self, console):
        run_function(console, self.name)

    def read(self, client, console):
        xs = [p[0] for p in self.at.values()]
        zs = [p[2] for p in self.at.values()]
        reply = client.ctl('blocks', min(xs), self.Y, min(zs), max(xs), self.Y, max(zs))
        found = {(x, y, z): state for x, y, z, state in reply['blocks']}
        return {block: found.get(pos, '<air>') for block, pos in self.at.items()}


class Items:
    """Every item, 36 at a time in the inventory (one of each), read live."""
    name = 'items'

    def __init__(self):
        self.items = [i for i in registry('minecraft:item') if i != 'minecraft:air']

    def setup(self, console):
        pass

    def read(self, client, console):
        out = {}
        for batch_index, start in enumerate(range(0, len(self.items), 36)):
            batch = self.items[start:start + 36]
            for slot in range(36):
                item = batch[slot] if slot < len(batch) else 'minecraft:air'
                console.server.console(f'item replace entity {PLAYER} container.{slot} with {item}')
            # The off hand last, as a counter: once it shows this batch, the slots set before it have arrived.
            marker = batch_index % 63 + 1
            console.server.console(f'item replace entity {PLAYER} weapon.offhand with minecraft:stone {marker}')
            deadline = time.time() + 30
            while (inventory := client.ctl('inventory'))['offhand'].get('count') != marker:
                if time.time() > deadline:
                    raise AssertionError(f'{client.label}: items batch {batch_index} did not arrive in 30 s')
                time.sleep(0.05)
            slots = {s['slot']: s for s in inventory['hotbar'] + inventory['main']}
            for slot, item in enumerate(batch):
                got = slots.get(slot) or {}
                out[item] = {k: got[k] for k in ('item', 'count', 'components') if got.get(k) is not None}
        console.run(f'clear {PLAYER}', r'Removed|No items')
        return out


class Entities:
    """Every entity type a command can summon, without AI, on the ground around the spawn."""
    name = 'entities'
    # Not summonable, or gone at once: the player, a bobber needs its angler, an item its stack.
    SKIP = {'minecraft:player', 'minecraft:fishing_bobber', 'minecraft:item', 'minecraft:lightning_bolt'}
    SPACING = 3

    def __init__(self):
        self.types = [t for t in registry('minecraft:entity_type') if t not in self.SKIP]
        # The player stands at 0 0.
        self.at = dict(zip(self.types, grid(len(self.types), -60, self.SPACING, skip={(0, 0)})))

    def function(self):
        return [f'summon {kind} {x + 0.5} {y} {z + 0.5} {{NoAI:1b,NoGravity:1b,PersistenceRequired:1b}}'
                for kind, (x, y, z) in self.at.items()]

    def setup(self, console):
        run_function(console, self.name)
        # What the server has: a type it does not keep (a marker never leaves the server) is not compared.
        self.on_server = set()
        for kind, (x, y, z) in self.at.items():
            reply = console.run(f'execute if entity @e[type={kind},x={x + 0.5},y={y},z={z + 0.5},distance=..1]',
                                r'Test (passed|failed)')
            if 'passed' in reply:
                self.on_server.add(kind)
        if missing := sorted(set(self.at) - self.on_server):
            print(f'entities: the server did not keep {" ".join(missing)}')

    def read(self, client, console):
        reach = max(max(abs(x), abs(z)) for x, _, z in self.at.values()) * 1.5 + 4
        listed = client.ctl('entities', reach)['entities']
        lab.write_json(client.build / f'{client.label}-entities.json', listed)
        out = {}
        for kind, (x, y, z) in self.at.items():
            near = sorted(e['type'] for e in listed
                          if abs(e['pos'][0] - x - 0.5) < 1 and abs(e['pos'][2] - z - 0.5) < 1)
            out[kind] = near
        # A type the server did not keep is not compared.
        return {kind: types for kind, types in out.items() if kind in self.on_server}


SCENARIOS = [Attributes(), Blocks(), Items(), Entities()]
# The second batch: each client runs a script after the first batch was read by both.
SCRIPTED = twin_scripted.SCRIPTED


# ---------------------------------------------------------------- comparison

def same(a, b):
    if isinstance(a, float) or isinstance(b, float):
        return isinstance(a, (int, float)) and isinstance(b, (int, float)) and abs(a - b) <= EPSILON
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(same(x, y) for x, y in zip(a, b))
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(same(a[k], b[k]) for k in a)
    return a == b


def compare(horizon, open_, gaps):
    """(failures, differing keys) for the scenarios' items, against the gap list."""
    failures, differing = [], set()
    for key in sorted(set(horizon) | set(open_)):
        a, b = horizon.get(key, '<missing>'), open_.get(key, '<missing>')
        if not same(a, b):
            differing.add(key)
            if key not in gaps:
                failures.append(f'{key}: horizon {json.dumps(a)[:200]}, open {json.dumps(b)[:200]}')
    # A listed key neither client reports (a horizon-ctl field this build lacks) is not measured.
    measured = set(horizon) | set(open_)
    for key in sorted(gaps & measured - differing):
        failures.append(f'{key}: matches now; remove it from {GAPS.relative_to(lab.ROOT)}')
    return failures, differing


def read_gaps():
    if not GAPS.exists():
        return set()
    lines = [l.strip() for l in GAPS.read_text().splitlines() if l.strip() and not l.startswith('#')]
    if lines != sorted(lines):
        raise AssertionError(f'{GAPS.relative_to(lab.ROOT)} is not sorted')
    return set(lines)


# ---------------------------------------------------------------- main

def binaries(given):
    if given:
        return Path(given).resolve()
    subprocess.run(['cargo', 'build', '--locked', '--profile', 'fast', '-p', 'horizon-client', '--bins',
                    '-p', 'horizon-open-relay', '-p', 'horizon-prepare'], cwd=lab.ROOT, check=True)
    return lab.cargo_target() / 'fast'


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--bin-dir', default=os.environ.get('HORIZON_TWIN_BIN_DIR'),
                        help='prebuilt horizon-client, horizon-ctl, horizon-open-relay and horizon-prepare')
    parser.add_argument('--write-gaps', action='store_true', help='write the differing keys as the new gap list')
    parser.add_argument('--only', help='comma-separated scenarios to run (default: all)')
    args = parser.parse_args()
    if args.only:
        wanted = set(args.only.split(','))
        SCENARIOS[:] = [s for s in SCENARIOS if s.name in wanted]
        SCRIPTED[:] = [s for s in SCRIPTED if s.name in wanted]
    started = time.time()
    _, jars, libraries = base.verify_artifacts()
    if not (REPORTS / 'blocks.json').exists():
        raise AssertionError(f'no vanilla reports in {REPORTS}: run tools/lab.py prepare')
    bins = binaries(args.bin_dir)
    # One folder per checkout: worktrees share .local.
    build = lab.LOCAL / 'open-twin' / lab.ROOT.name
    shutil.rmtree(build, ignore_errors=True)
    build.mkdir(parents=True)
    bundle = build / 'open'
    subprocess.run([str(bins / 'horizon-prepare'), 'open', '--root', str(lab.ROOT), '--out', str(bundle)],
                   check=True, capture_output=True)
    gaps = read_gaps()
    env = display_env()
    horizon = Client('horizon', bins, lab.RESOURCES, build, env)
    open_ = Client('open', bins, bundle, build, env)
    server = relay = None
    try:
        # Everything starts at once: the server, the relay (it derives its tables), both games.
        server = launch_server(build, jars, libraries,
                               {s.name: s.function() for s in SCENARIOS if hasattr(s, 'function')})
        relay, relay_port = start_relay(build, bins / 'horizon-open-relay', bundle, server.port)
        for client in (horizon, open_):
            client.start()
        wait_relay(build, relay)
        console, port = configure_server(server)
        address = {'horizon': f'127.0.0.1:{port}', 'open': f'127.0.0.1:{relay_port}'}
        phase(started, 'server, relay and both games up')
        # The first login makes the player; the console sets every scenario up around it.
        horizon.login(address['horizon'])
        for scenario in SCENARIOS:
            scenario.setup(console)
        console.sync()
        horizon.logout()
        console.wait_offline()
        phase(started, 'scenarios set up')
        seen = {}
        for client in (open_, horizon) if SCENARIOS else ():
            client.login(address[client.label])
            seen[client.label] = {f'{s.name}:{k}': v for s in SCENARIOS for k, v in s.read(client, console).items()}
            client.logout()
            console.wait_offline()
            phase(started, f'{client.label} read')
        # The second batch starts from a bare flat world around the spawn.
        console.run('kill @e[type=!minecraft:player]', r'Killed|No entity')
        console.run('fill -18 -50 -18 17 -50 17 minecraft:air strict', r'filled|No blocks')
        for client in (open_, horizon) if SCRIPTED else ():
            client.login(address[client.label])
            # The scripts play at vanilla's size and speed.
            Attributes().reset(console)

            def relog(client=client):
                client.logout()
                console.wait_offline()
                client.login(address[client.label])
            for scenario in SCRIPTED:
                for key, value in scenario.run(client, console, relog).items():
                    seen.setdefault(client.label, {})[f'{scenario.name}:{key}'] = value
            client.logout()
            console.wait_offline()
            phase(started, f'{client.label} scripted')
        lab.write_json(build / 'seen.json', seen)
    finally:
        for client in (horizon, open_):
            client.close()
        if relay:
            relay.terminate()
            relay.wait(timeout=10)
        if server:
            server.stop()
    failures, differing = compare(seen['horizon'], seen['open'], gaps)
    if args.write_gaps:
        GAPS.parent.mkdir(parents=True, exist_ok=True)
        GAPS.write_text(''.join(f'{k}\n' for k in sorted(differing)))
    counts = {s.name: sum(k.startswith(s.name + ':') for k in differing) for s in SCENARIOS + SCRIPTED}
    print(f'open twin: {len(seen["horizon"])} items compared, gaps per scenario {counts}, '
          f'{time.time() - started:.0f} s (details: {build}/seen.json)')
    if failures and not args.write_gaps:
        raise AssertionError(f'{len(failures)} differences outside the gap list:\n' + '\n'.join(failures[:80]))


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
