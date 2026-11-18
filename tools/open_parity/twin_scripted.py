"""Scripted scenarios of the open_twin harness: the cases players reported.

Each scenario's `run(client, console, relog)` resets what it uses with server console commands,
drives the client with horizon-ctl exactly as it drives the other one, and returns what it read,
by name. Both clients run the same script against the same world; only the replies are compared.
`client.catch_up(console)` returns once the client has everything the server sent so far.
"""
import re
import time

PLAYER = 'Twin'


def step(console, ticks):
    """Runs `ticks` game ticks of the frozen server and waits until they have run."""
    def game_time():
        return int(re.search(r'time is (\d+)', console.run('time query gametime', r'time is \d+')).group(1))
    until = game_time() + ticks
    console.run(f'tick step {ticks}', r'Stepping')
    deadline = time.time() + ticks / 20 * 4 + 30
    while game_time() < until:
        if time.time() > deadline:
            raise AssertionError(f'tick step {ticks} did not finish')
        time.sleep(0.25)


def wait_screen(client, name, timeout=5):
    """The `screen` reply once it shows `name`, or as it is after `timeout` (a gap, not an error)."""
    deadline = time.time() + timeout
    while (screen := client.ctl('screen'))['screen'] != name and time.time() < deadline:
        time.sleep(0.1)
    return screen


def filled(screen, slots=None):
    """The screen's non-empty slots: slot -> item and count (components left out: names differ)."""
    return {str(s['slot']): f"{s['item']} x{s.get('count', 1)}" for s in screen.get('slots', [])
            if s.get('item') and (slots is None or s['slot'] in slots)}


def player(client, keys=('pose', 'swimming', 'in_water', 'eye_height', 'dimensions')):
    p = client.ctl('state')['player']
    return {k: p.get(k) for k in keys if k in p}


def carried(client):
    """The inventory's non-empty slots: slot -> item and count."""
    inventory = client.ctl('inventory')
    return {str(s['slot']): f"{s['item']} x{s.get('count', 1)}" for s in inventory['hotbar'] + inventory['main']
            if s.get('item')}


def on_server(console, pos, block):
    """Whether the server has `block` at `pos` (what the client's clicks really did)."""
    return 'passed' in console.run('execute if block {} {} {} {}'.format(*pos, block), r'Test (passed|failed)')


def reset(console, commands):
    for command in commands:
        console.server.console(command)
    console.sync()


class Furnace:
    """Craft a furnace on a crafting table, place it, open it and smelt raw iron with coal."""
    name = 'furnace'
    TABLE, FURNACE = (2, -60, 0), (-2, -60, 0)
    # A ring of cobblestone in the 3x3 grid (slots 100-108, row-major): the furnace recipe.
    RING = [100, 101, 102, 103, 105, 106, 107, 108]

    def run(self, client, console, relog):
        out = {}
        # Every recipe known, so the recipe book shows the same on both.
        reset(console, [f'clear {PLAYER}', f'recipe give {PLAYER} *',
                        'setblock {} {} {} minecraft:crafting_table'.format(*self.TABLE),
                        'setblock {} {} {} minecraft:air'.format(*self.FURNACE),
                        f'tp {PLAYER} 0.5 -60 0.5 90 0', f'give {PLAYER} minecraft:cobblestone 8'])
        client.catch_up(console)
        client.ctl('interact', *self.TABLE, 'west')
        screen = wait_screen(client, 'inventory')
        out['table screen'] = [screen['screen'], screen.get('craft_size')]
        out['recipe book'] = (screen.get('recipe_book') or {}).get('known')
        client.ctl('click-slot', 0)
        for slot in self.RING:
            client.ctl('click-slot', slot, 'right')
        client.catch_up(console)
        out['grid'] = filled(client.ctl('screen'), set(range(100, 109)) | {200})
        client.ctl('click-slot', 200, 'shift')
        client.ctl('close')
        client.catch_up(console)
        out['crafted'] = carried(client)
        # Shift-clicked out of the result, the furnace goes to the hotbar's last slot.
        client.ctl('select', 9)
        out['held'] = client.ctl('state').get('held')
        client.ctl('place', *self.FURNACE, check=False)
        client.catch_up(console)
        out['placed'] = client.ctl('block', *self.FURNACE).get('state')
        out['placed on server'] = on_server(console, self.FURNACE, 'minecraft:furnace')
        # The furnace from here on, whatever the crafting did.
        reset(console, [f'clear {PLAYER}', 'setblock {} {} {} minecraft:furnace[facing=east]'.format(*self.FURNACE),
                        f'give {PLAYER} minecraft:raw_iron 1', f'give {PLAYER} minecraft:coal 1'])
        client.catch_up(console)
        client.ctl('interact', *self.FURNACE, 'east', check=False)
        screen = wait_screen(client, 'container')
        out['furnace screen'] = [screen['screen'], screen.get('container')]
        client.ctl('click-slot', 0, 'shift', check=False)
        client.ctl('click-slot', 1, 'shift', check=False)
        client.catch_up(console)
        out['loaded'] = filled(client.ctl('screen'), {300, 301, 302})
        step(console, 220)
        client.catch_up(console)
        out['smelted'] = filled(client.ctl('screen'), {300, 301, 302})
        out['lit'] = client.ctl('block', *self.FURNACE).get('state')
        client.ctl('close', check=False)
        return out


class RottenFlesh:
    """Get hungry, eat rotten flesh: what the hand holds, and the food after."""
    name = 'rotten_flesh'

    def run(self, client, console, relog):
        out = {}
        # Saturation 256 fills food and saturation to 20; then 80 ticks of Hunger 256 (102.4
        # exhaustion) take the saturation and about 5 food. Looking up, the click eats.
        reset(console, [f'clear {PLAYER}', f'effect clear {PLAYER}', f'tp {PLAYER} 0.5 -60 0.5 0 -90',
                        f'effect give {PLAYER} minecraft:instant_health 1 3 true',
                        f'effect give {PLAYER} minecraft:saturation 1 255 true'])
        time.sleep(0.5)
        reset(console, [f'effect clear {PLAYER}', f'effect give {PLAYER} minecraft:hunger 4 255 true'])
        time.sleep(5)
        reset(console, [f'give {PLAYER} minecraft:rotten_flesh 1'])
        client.catch_up(console)
        client.ctl('select', 1)
        out['held'] = client.ctl('state').get('held')
        # Leftover exhaustion from earlier moves can take a point more: the gain is compared.
        before = player(client, ('food',)).get('food')
        out['hungry'] = before is not None and before < 20
        client.ctl('use-item', '--hold', 2.5, check=False)
        client.catch_up(console)
        after = player(client, ('food',)).get('food')
        out['food gained'] = after - before if None not in (before, after) else None
        out['held after'] = client.ctl('state').get('held')
        reset(console, [f'effect clear {PLAYER}'])
        return out


class SwimRelog:
    """Swim under water, leave and join again: pose and eye height while swimming and after."""
    name = 'swim_relog'

    def run(self, client, console, relog):
        out = {}
        reset(console, ['fill 3 -62 3 9 -60 9 minecraft:water', f'clear {PLAYER}', f'effect clear {PLAYER}',
                        f'tp {PLAYER} 6.5 -62 4.5 0 20'])
        client.catch_up(console)
        out['in water'] = player(client)
        # Read while the sprint is held: swimming ends with it.
        moving = client.ctl_async('move', 'forward', 1.5, '--sprint')
        time.sleep(1.0)
        out['swimming'] = player(client)
        moving.wait(timeout=60)
        relog()
        out['after relog'] = player(client)
        reset(console, [f'fill 3 {low} 3 9 {high} 9 minecraft:{block}'
                        for low, high, block in [(-62, -62, 'dirt'), (-61, -61, 'grass_block'), (-60, -60, 'air')]])
        return out


SCRIPTED = [Furnace(), RottenFlesh(), SwimRelog()]
