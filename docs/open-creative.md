# Horizon Open Creative

A sandbox that runs Horizon's own engine with **no Mojang files**: openly licensed block and
item artwork, and block data, recipes, interface art, world data and text written for it. It is a
separate edition with its own content profile; the Minecraft-backed game is unchanged and keeps
preparing its resources from the player's own install.

What it is: a deterministic landscape (rolling hills, trees, beaches and water), walking, jumping,
creative flight, collision and swimming, a creative palette of 86 blocks and 28 items in nine tabs,
and survival play: mining with tools that wear out, drops, hunger and health, crafting in the 2x2
inventory grid and at the crafting table (56 recipes, all in the recipe book), and death and
respawning. Browser worlds last for one visit: reloading the page starts a fresh world, and the
page says so before play. Local worlds have a weather cycle. Natural mob spawning, sound,
smelting and travel to other dimensions are unavailable in those worlds.

## Content

The bundle's blocks (`blocks.rs`) and items (`items.rs`), by palette tab:

| Tab | Content |
|---|---|
| Ground and Stone | stone, cobblestone and mossy cobblestone, stone bricks, smooth stone, dirt, grass, sand, gravel, the sandstone and desert sandstone families, desert sand and desert stone, clay, snow, ice, obsidian, bedrock |
| Construction | the logs and planks of five species (oak, pine, aspen, jungle, acacia; ids `oak_`, `spruce_`, `birch_`, `jungle_`, `acacia_`), bricks, glass, obsidian glass (`tinted_glass`), the light block (`glowstone`), bookshelf, crafting table |
| Colored Blocks | the fifteen wool colours Minetest Game has |
| Plants | the five leaves and saplings, eight flowers and mushrooms, fern, dry shrub, papyrus (`sugar_cane`), cactus |
| Ores and Minerals | coal, copper, iron, gold and diamond ores and their storage blocks |
| Tools / Combat | wooden, stone, iron and diamond pickaxes, axes and shovels / swords |
| Food | apple |
| Ingredients | stick, coal, copper, iron and gold lumps (`raw_*`) and ingots, diamond, clay lump, clay brick |

Names follow Minetest Game where its art does (Pine Planks, Desert Stone); identifiers are the
familiar `minecraft:` ones the shared code expects.

Survival rules, all data the shared survival code reads:

- **Mining:** each block names its fast tool (`data/minecraft/tags/block/mineable/*`). Stone-like
  blocks set `requires_tool` and drop nothing by hand; copper and iron need a stone pickaxe, gold
  and diamond an iron one, obsidian a diamond one (`needs_*_tool`, `incorrect_for_*_tool`). Tools
  carry `minecraft:tool` rules, durability and attack modifiers.
- **Drops** (`data/minecraft/loot_table/blocks/`, in the shared evaluator's format): stone gives
  cobblestone, grass gives dirt, clay four clay lumps, ores their mineral (Fortune applies);
  leaves give their sapling (5%), sticks (4%) and, for oak, apples (5%), the only food.
- **Crafting** (`data/minecraft/recipe/`): planks from logs, sticks, the crafting table, the
  sixteen tools, building blocks in 2x2 squares, storage blocks and back. There is no furnace:
  what smelting would make (ingots, glass, stone, smooth stone, clay bricks) is crafted from the
  material and a piece of coal. One advancement (`advancement/recipes/open_all.json`) teaches every
  recipe on the first tick, so the recipe book lists them all. Item tags (`tags/item/`) name the
  planks, logs, stone tool materials and coal recipes accept.
- Progress from nothing: punch a tree, make planks, sticks and a table, a wooden pickaxe, then
  stone tools from cobblestone; iron and copper with a stone pickaxe and coal; gold and diamond
  with an iron pickaxe.

### Mobs

The bundle has the common creatures of a survival world (`crates/horizon-open-content/src/mobs.rs`):
zombie, husk, drowned, skeleton, stray, creeper, spider, cave spider, enderman, cow, pig, sheep (its
wool dyed by the client) and chicken, with each climate variant and the young. Each adult is a
model in Minecraft's standard box layout: the same part names (`hat` included), boxes, pivots,
texture origins and mirrored boxes as the Java model, on a skin of the Java texture's size, so a
skin from a Minecraft-layout resource pack fits it. The skins are painted by the generator,
Horizon's own: a painter colours each face of each box. The young are the adults' boxes scaled about
the feet, the head less, on the adult skin layout (Minecraft's young have models and skins of their
own).

The other creatures players meet (`crates/horizon-open-content/src/creatures.rs`): villagers,
the wandering trader, zombie villagers, iron, snow and copper golems, the witch and every
illager (pillager, vindicator, evoker, illusioner, ravager, vex), horses, donkeys, mules, zombie
and skeleton horses, wolves, cats, ocelots, foxes, rabbits, bats, parrots, squid and glow squid,
cod, salmon, tropical fish, pufferfish, dolphins, turtles, llamas and trader llamas, goats, bees,
polar bears, pandas, mooshrooms, camels, sniffers, armadillos, allays, frogs, tadpoles, axolotls,
slimes, magma cubes, piglins, piglin brutes, zombified piglins, hoglins, zoglins, striders, the
ghast, blaze, wither skeleton, bogged, phantom, silverfish, endermite, both guardians, the giant,
shulker, breeze, creaking, warden and wither. The same file carries the other entities the
relay used to hide: every boat, chest boat and raft, the minecarts, arrows and armor stands (`thing`
lines: no attributes, drawn by the game's own renderers). Their facts are in `data/mobs/creatures.txt`, which
`tools/open_mob_layouts.py` writes from the prepared vanilla tables (numbers and names only): the
entity type's hit box, category and default attributes, and every model layer the renderer draws
(the young's own models too) as Minecraft's parts, poses and boxes, which the tool recovers from
the baked polygons and checks by rebuilding them. `creatures::tests` compares every rebuilt layer
with the prepared `entity-models.json`, corner for corner, when it is at hand. Each texture
the renderer may ask for is the vendored pack's at Minecraft's path when it has one of that layout
(a whole multiple of Minecraft's size), else painted by the generator on the model's boxes (overlays
such as professions mostly clear). The relay shows these as themselves; its stand-ins
(`rules.rs` `STAND_INS`) are left for the types still without content.

Open players see these on any server they reach through the relay; the shared world's own
server spawns no mobs (the open profile turns spawning off). The private preview lays its own mob
skins over these when it plays on the open bundle, but for the young (`web.rs`'s overlay skips
`textures/entity/**/*_baby.png`), whose layouts wouldn't fit.

## The content profile

`horizon-resources.json` names the content a resource folder holds
(`horizon_core::content_profile::ContentProfile`):

| Profile | Manifest | Version check |
|---|---|---|
| `vanilla` | no `content_profile` key (every folder `horizon-prepare` made before profiles), or `"vanilla"` | `data_version` must be the pinned Minecraft data version |
| `open-creative` | `"content_profile": "open-creative"` | `content_revision` must be this build's `OPEN_CONTENT_REVISION`; Minecraft's data version is never read |

The checks live at the boundaries, not through the game:

- **Startup** (`crates/horizon-client/src/game/open_profile.rs`, `apply`): the bundle's files are
  checked against its provenance record (a missing or changed file stops the game with the file's
  name and `horizon-prepare open` as the fix). Startup rejects `--benchmark`, `--dev-mod` and other
  dimensions. Single player runs in the frame (`--in-frame`), and every world,
  a dedicated server's too, uses the open terrain preset. Arguments from the browser's `?args=`
  go through the same code.
- **Servers** (`assets::server_brand`): an open server's `minecraft:brand` is `horizon-open/`
  and the start of its provenance record's SHA-256. Both ends number blocks, items, entities and
  the other network registries from the bundle's `registries.json` (Minecraft's numbers, see
  [Numbering](#numbering)), but content can still differ, so an open player leaves any
  server whose brand differs (a Minecraft server, or an open one with other content), and a
  Minecraft-backed player leaves an open server, before their numbers could disagree.
- **Worlds** (`check_world`): a world made with the open bundle carries `horizon-content.json`.
  The open bundle opens only such worlds (a new folder becomes one) and refuses vanilla worlds
  before reading or writing them; the vanilla game refuses open worlds.
- **In the game** (`open_allows`): changing dimension is refused with a chat line, and the
  commands are those that only use what the bundle has (`OPEN_COMMANDS`: `/gamemode`, `/give`,
  `/clear`, `/tp`, `/time`, `/setblock`, `/fill`, `/clone`, `/kill`, `/xp`, `/spawnpoint`,
  `/difficulty`, `/weather`, chat commands and a few more). Game rules keep mob spawning, random
  ticks and fire spread off. Water and lava flow as in Minecraft: the bundle's
  `washed_away_by_fluids` and `blocks_fluid_flow` tags say where they go and where they stop.
- **Weather**: use `--weather clear`, `--weather rain` or `--weather thunder` to set the starting
  weather, or `/weather` in worlds where commands are enabled. Open's plains get rain with original
  textures. On a Java server, the server's biomes determine where rain or snow appears. Rebuild an
  existing native resource bundle with `horizon-prepare open` to pick up the precipitation textures.
- **Menus**: the title has Singleplayer, Multiplayer, Options and Quit; the pause menu offers Back
  to the World, Options, Credits, Statistics and Save and Quit to Title (Disconnect on a server);
  the options page offers video, controls, chat, accessibility and credits only.
- **On a server** (a Java server through `horizon-open-relay`) these limits of open's own worlds
  step aside: the chat parses and completes with the server's command tree (the relay passes it
  when both sides number argument types alike; without one, `OPEN_COMMANDS`), the Nether and End
  load with the sky, fog and light of the server's dimension type (the bundle's biomes are the
  overworld's, so biome colours are left out there), and the pause menu offers Advancements once
  the server sends ones to display (the relay translates their icons; the bundle draws the screen
  with its own art, `art_advancements.rs`).

## Preparing the bundle

```sh
cargo run --profile fast -p horizon-prepare -- open --out /tmp/open-resources
cargo run --profile fast -p horizon-prepare -- open --out /tmp/open-resources --check   # verify only
cargo run --profile fast -p horizon-client -- --resources /tmp/open-resources --world Open
```

`horizon-prepare open` dispatches before any Minecraft, launcher or JDK discovery: it needs no
Java and no network. It is the `open` feature of horizon-prepare (on by default); the launcher links
the preparer's library with `default-features = false`, so it carries none of the generator. `--archive FILE` also writes the browser's data archive: the bundle's files as one stream of records, gzip-compressed (about 1.3 MB; the page inflates it with the browser's `DecompressionStream`, `inflateArchive` in `web/cache.js`, before the game mounts it). The per-state tables leave out the fields their readers default (`gameplay-facts.json` rows keep only what differs from a false flag, a zero, no item, a model shape, push-pull and no sound). Preparing twice from
the same inputs gives byte-identical files.

The generator is `crates/horizon-open-content`:

| Module | Writes |
|---|---|
| `blocks.rs` | the block definitions: the palette, terrain (air, short grass), the fluids (water, lava, bubble column) and inert support blocks, with each block's tool, tier and drop |
| `plants.rs` | the plants and small decorations, their per-state facts and models ([Plants](#plants-and-small-decorations)) |
| `items.rs` | the items that are not blocks (tools, food, materials), their components, the recipes, item tags and loot tables |
| `tables.rs` | the per-state tables (`facts.json`, `gameplay-facts.json`, `block-behaviours.json`, shapes, movement, tags), the biome, dimension and day cycle, player attributes, damage types, loot |
| `assets.rs` | textures, block states, models (its own display transforms, `item/generated` and `item/handheld`), items, the creative palette (`creative-inventory.json`) |
| `art.rs` | original art: bedrock, breaking cracks, the interface, sun, moon, clouds, the player's model and skin |
| `art_advancements.rs` | original art: the advancements window, its tabs on every side, the task, goal and challenge frames, the title and description boxes, and the five tab backgrounds vanilla's advancements name |
| `art_survival.rs` | original art: hearts (every kind the HUD draws), food, air, armour, experience and jump bars, the inventory, crafting table, recipe book and game-mode switcher screens, empty equipment slots, toasts, menu icons, world and server list icons, statistics headers, the vignette, the title's wordmark and panorama, the crafting table's faces |
| `lang.rs` | the English interface text: hand-written labels (menus, survival, death messages, joining and leaving, statistics, the allowed commands' replies and errors), then a line for every key Minecraft's English has: every block, item, entity, effect, attribute and statistic named by its id title-cased (from `data/registries/`, a short table where that reads wrong), and every other key of `data/lang/keys.txt` (written by `tools/open_lang_keys.py`: keys and their placeholders, no text) worded by rules on the key. Each line keeps Minecraft's placeholders in Minecraft's order |
| `sources.rs` | `content/open/sources.json`: inputs, checksums, credits and notices |
| `numbering.rs`, `network.rs` | Minecraft's numbering from `data/` ([Numbering](#numbering)) and `registries.json` |
| `coverage.rs` | `horizon-open-coverage.json` ([Coverage](#coverage)) |
| `shapes.rs` | the shape library: per-state collision, outline and occlusion shapes, the facts that follow from them, the cached shape tables |
| `slabs.rs` | slabs and stairs ([Slabs and stairs](#slabs-and-stairs)) |

Building fails on a missing or changed input, a licence whose text is not vendored, a vendored
file nobody attributed, or an unresolved model, texture, parent or item reference.

### Numbering

The bundle numbers everything as Minecraft 26.3 does, so a packet means the same to an open
client and to a Minecraft one. The numbers come from name lists committed in
`crates/horizon-open-content/data/` (names only, no other fact of Minecraft's), written by
`python3 tools/open_registry_names.py` from the data generator's reports of the player's own
server jar (`--check` fails if a list is out of date):

- `registries/<registry>.txt`: one name per line in protocol order (line n is id n), without
  the `minecraft:` namespace (others keep theirs: `brigadier:bool`). Each becomes a registry of
  `registries.json` with every name, content or not: attributes, blocks, block entity types,
  command argument types, custom statistics, data components, entity types, items, map
  decorations, menus, mob effects, particles, the recipe book's categories, recipe and slot
  displays, sound events, statistic types, villager professions and types.
- `blocks/states.txt`: one block per line in registry order: its name, then each property as
  `key=v1,v2,...`, keys sorted, values in Minecraft's order, the default value marked `*`
  (`oak_leaves distance=1,2,3,4,5,6,*7 persistent=true,*false waterlogged=true,*false`). A
  block's states are the product of its properties with the last key varying fastest, and its
  first state follows the previous block's last: this is Minecraft's state numbering, and
  `facts.json`'s rows follow it (35,723 states).

`attributes.json` lists all 40 attributes in registry order (the index is the network id).
`numbering.rs` reads the lists; a test checks every name the bundle defines and every state id
against them.

### Full-cube blocks

`data/blocks/cubes.txt` (written by `python3 tools/open_block_facts.py` from the prepared
vanilla tables; `--check` fails if it is out of date) gives four families their content: the
182 single-state blocks of class `Block`, the 60 `RotatedPillarBlock`s (logs, stems, basalt,
hay), the furnace, blast furnace and smoker, and 126 blocks of other classes whose every state
draws one plain cube model (ores, stained glass, leaves, copper, coral blocks, concrete powder,
sand, netherrack, magma, pumpkins). Their layout says how: unturned (`cube`; `clear` when light
shows through, as grates; `leaves` when its faces also carry a tint index; `glass` when its
sprites are drawn translucent), turned at random among the turns Minecraft picks (`spin_y` for
sand, `spin_xy` for netherrack), or by a horizontal `facing`; soul sand and mud are `sunken`
(a full outline over a collision box 14 sixteenths tall, from the shape library), and soul sand
keeps its speed factor of 0.4. Only their default state's numbers are written: the states of
such a block differ at most in light, by `lit` (redstone ore).

The `variants` layout covers 50 more blocks whose states draw full-cube models with any UVs and
turns, or a multipart of single boxes: glazed terracotta (its faces' UVs turn), mycelium and
podzol (the snowy model), observers, dispensers, droppers, barrels, crafters, copper bulbs,
redstone lamps, beehives, sculk, sculk catalysts, infested stone and deepslate (mirrored
models), suspicious sand and gravel, the creaking heart, command blocks and the three mushroom
blocks (six faces, each a part). `data/blocks/cube_variants.txt` lists each block's variants or
parts (state key or condition, model, x and y turns, uvlock) and each model's box and faces
(texture, UV, rotation, tint index, cull face); `cubes.rs` writes them in the bundle's format.

The `shaped` layout covers 42 blocks that are not cubes, drawn by models of any boxes:
redstone wire, the four rails, end rods and lightning rods, carpets and moss carpet, repeaters,
comparators, snow layers, farmland, dirt paths, ladders, levers, cakes and anvils. Their models
are listed as the `variants` blocks' are (any elements, turned ones too, and multipart
conditions with alternatives). `cubes.rs` gives each state Minecraft's boxes by class
(`cubes::shape`, through the shape library), and `cubes.txt`'s last columns give the facts the
tables cannot derive for them: piston reaction, replaceable, random ticks, lava ignition, block
entity and item. Redstone wire is placed by redstone, so it has no item of its own. Candles,
candle cakes, cauldrons (empty, water, lava and powder snow) and the pale moss carpet are
`shaped` too. A candle's lit light is written once per candle count (`3,6,9,12` in the lit light
column), and filled cauldrons and candle cakes, placed by other items, have no item of their own. One line per block, numbers and names only:
class chain, destroy time, resistance, light (and lit light), dampening, friction, map colour,
sound type, mining tool and level, and the texture names of its six faces (the lit front last).
`cubes.rs` reads it: a block the crate already authors keeps its name, tab and drop and takes
these numbers and faces; the others join the palette with a model in the bundle's own format, their
textures from the pack (a placeholder in the map colour when it has none), and an item; each
drops itself (a deepslate ore what its stone ore drops; new leaves nothing yet). Logs lying
down turn their top half a turn, as Minecraft's horizontal columns do; furnaces turn with
`facing` and show their lit front when `lit`.

### Fluids

Water and lava are Minecraft's `LiquidBlock`s: all 16 `level` states (0 the source, 1 to 7
flowing, 8 and up falling), each with Minecraft's fluid and amount in `block-behaviours.json`,
no collision, and the fluid tags (`minecraft:water`, `minecraft:lava` and the water tags mobs,
plants and boats look for), so the shared code swims, sinks, slows and fogs the player as in
Minecraft. Lava gives light 15 and is drawn from the pack's animated `lava_still` and `lava_flow`.
The bubble column is water to look at and swim in (its fluid is a water source). A waterlogged
state holds a water source, as Minecraft's do. On a Java server, through the relay, they all
show as Minecraft's; in the edition's own worlds too they flow, make sources (water), and
meet as cobblestone, stone and obsidian as Minecraft's do.

### Slabs and stairs

`data/blocks/slabs.txt` (written by `python3 tools/open_slab_facts.py`, `--check` as above) lists
the 101 slabs and 97 stairs (the copper ones too): class chain, destroy time, resistance,
friction, map colour, sound type, mining tool and level, note block instrument, whether lava
ignites it, the bottom, top and side textures of its models, and a slab's double-block faces.
`slabs.rs` adds them to the palette with all of Minecraft's states. Their shapes come from the
shape library (`shapes.rs`): boxes joined with the shared engine's `VoxelShape`, as
`Shapes.box` and `Shapes.or` do, turned and flipped per state. From each state's shapes the
generator writes its collision and light occlusion (`cached-collision.nbt`, `occlusion.nbt`),
outline and edges, sturdy faces, light dampening and full-block flags. A waterlogged state
holds a water source. The models are the bundle's own, with the boxes, UVs and cull faces
Minecraft's slab and stairs templates have; stairs have a straight, an inner and an outer
model, turned per state with `uvlock` as Minecraft's block states do. A double slab drops two.

### Walls, fences, gates, panes and bars

`data/blocks/connected.txt` (written by `tools/open_block_facts.py`) lists the 32 walls, 13 fences, 12 fence gates,
17 glass panes and 9 bars, with the same numbers and the names of their textures (a pane's
glass and edge). The bamboo fence and gate, drawn with their own templates, are not in it yet.
`connected.rs` gives each of their states Minecraft's collision, outline and light occlusion
boxes: fences, walls and closed gates collide 1.5 blocks high, an open gate not at all, and a
waterlogged state holds water and dims light by one. Their models are templates in the bundle's
own format (`block/open_fence_post`, `open_wall_side`, `open_glass_pane_noside`...), a child per
block naming its textures, and a multipart block state that draws a post and an arm for each
connection, as Minecraft's do. Fences and walls have an inventory model; panes and bars are flat
items. The `fences`, `wooden_fences`, `walls`, `fence_gates` and `bars` tags list them.

`data/blocks/openings.txt`, in the same format, lists the 22 doors, 22 trapdoors (11 whose
texture turns with them), 15 buttons and 17 pressure plates; `openings.rs` gives them their
shapes (a door swings to its hinge side, buttons and plates never collide, a pressed plate is
half as thick) and their templates and variants. Doors are flat items, trapdoors, buttons and
plates show their block model. The `doors`, `trapdoors`, `buttons` and `pressure_plates` tags
list them, and their `wooden_` tags those an axe mines.

### Plants and small decorations

`data/blocks/plants.txt` (written by `python3 tools/open_plant_facts.py` from the prepared
vanilla tables, block states and model names; `--check` fails if it is out of date) gives content
to the plants and small decorations: flowers and tall flowers, saplings, grass, ferns and bushes,
mushrooms, fungi and roots, crops by age, sugar cane, sweet berry bushes, seagrass, kelp, coral,
coral fans and wall fans (live and dead), sea pickles, the Nether and cave vines, vines, glow
lichen, sculk veins, hanging roots, cobwebs, amethyst buds, torches, wall torches, redstone
torches, lanterns, lily pads, chains and leaf litter; the chests, beds and shulker boxes; and the
standing and wall signs of every wood (their editor boards `gui/signs/<wood>` from the pack).
Numbers and names only: per state its light, dampening, map colour, flags, outline boxes, collision and occlusion shapes (the cached tables' cells, written
through the shape library's table), random offset, behaviour row and the models, turns and
weights its variant uses (or, for vines and lichen, the multipart cases); per model the layout
its geometry follows (`cross`, `crop`, `coral_fan`, `template_torch`...) and its texture names.
`plants.rs` reads it and draws each layout itself, with Minecraft's planes, UVs and tint
indices, so a Minecraft-layout pack's textures fit. A block the crate already authors keeps its
name and tab. Blocks with their own item join the palette (the item flat, with Minecraft's
texture, tints and default components); the others (wall torches, crops, kelp's stem, tall
seagrass) name Minecraft's item (`torch`, `wheat_seeds`) and are never offered. Seagrass, kelp
and waterlogged states hold water, so the water draws around the plant.

Chests (single and double, trapped, ender and copper) and shulker boxes have no block model of
their own: the client's block-entity renderer draws them from their block states, with the
`chest`, `double_chest_left`, `double_chest_right` and `shulker_box` layers of
`entity-models.json` (`chests.rs`: Minecraft's part names, boxes, pivots and texture origins)
and the skins under `textures/entity/chest/` and `entity/shulker/` from the pack. Their items
are Minecraft's special item models. Beds are block models (a mattress and two legs per half),
their textures placeholders in their wool's colour until a pack draws them. The block entity
types `chest`, `trapped_chest`, `ender_chest` and `shulker_box` count as content.

### Support blocks

Every Minecraft block the bundle does not define is an inert support block: all of Minecraft's
states (so the numbering stays exact), each with the same inert facts: no model, item,
collision or light, never generated and never offered. Cave air and void air are air. Blocks
the bundle defines take Minecraft's properties too (leaves have their distances, grass its
`snowy`); states the bundle's content doesn't use share the block's facts and model. The
manifest counts the support blocks (`support_blocks`).

### Items that are not blocks

Every Minecraft item that is not a block has content: its prototype in `items.json` carries
Minecraft's default components (stack size, food and consumable, tool, weapon, equippable,
durability, rarity...), from `data/items/defaults.json`. `python3 tools/open_item_defaults.py`
writes that file from the prepared resources of the player's own install (`--check` fails if it
is out of date): facts only (numbers, identifiers, booleans), never translated text, and which
parent each item model uses (`item/generated` or `item/handheld`) and its texture name. Each item
gets a model on that parent with its texture as `layer0`, taken from an open input (an enabled
pack) when one provides it, else a generated placeholder icon (a coloured tile). Its English name
comes from its identifier ("Rotten Flesh"). Tool rules that name a block tag the bundle lacks are
left out. The edition's own items (`items::all`: tools, apple, stick, ores' materials) keep their
own definitions.

Block items take Minecraft's default components from the same file (its `blocks` list: stack
size, furnace fuel, compost chance, container...) when the generator writes their block, so a
block family added later gets them with no further change.

### Coverage

`horizon-open-coverage.json` says which registered names have content, so the relay can keep a
stand-in for the others and players never see a hole; it is also the count of what is finished:

```json
{"schema": 1, "registries": {
  "minecraft:block": {"total": 1286, "content": ["minecraft:air", "minecraft:stone", ...]},
  "minecraft:item": {"total": 1658, "content": ["minecraft:air", "minecraft:stone", ...]},
  "minecraft:attribute": {"total": 40, "missing": []}
}}
```

Each registry of `registries.json` has an entry with `total` (its number of names) and exactly
one of `content` (the names with content) or `missing` (the names without), whichever is
shorter (`missing` on a tie), full names in protocol order; every other name of the registry is
in the other set. Content means: a block the bundle defines, or cave and void air (all of its
states); an item with a prototype in `items.json`; an entity type the bundle can show (player,
item, experience orb, falling block and the mobs); a menu whose screen background is in the bundle (all
25 since the container screens). Attributes, command argument types, custom statistics, data components, the recipe
book's categories, recipe and slot displays and statistic types are complete: the shared code
handles every name. Block entity types have content for the chests and shulker boxes. Map
decorations, mob effects, particles, sound events
and villager professions and types have no content yet. A name without content still loads:
an inert block, an item drawn as the missing model, an entity with no model.

### Provenance

`content/open/` holds only the open inputs: the selected Minetest Game textures at revision
`9e895af6e9feb98332fa1a778001442f5669bc20` (`media/`), the upstream credit and licence files
(`notices/`), and the licence texts (`licenses/`). `sources.json` records, for every input, its
repository, revision and path, author, licence, SHA-256, the bundle files it becomes and how it is
transformed (re-encoded RGBA; the grass side is composited over dirt and the ore minerals over
stone, as Minetest Game layers them).
The fonts come from `crates/horizon-theme/fonts` (Pixelify Sans, OFL).

Licences: each input keeps its own (CC BY-SA 3.0 and CC0 1.0 today), the generated bundle is
distributed under CC BY-SA 4.0 as a whole, and the generator stays MIT
([legal/LICENSING.md](../legal/LICENSING.md#horizon-open-content)).

Every bundle file is listed in `horizon-open-provenance.json` with its SHA-256 and either the
input it came from or the generator that wrote it. `horizon-prepare open --check`, the web
packaging and the game itself refuse a bundle that disagrees with it. The credits reach players
three ways: Options → Credits (rolling credits), the pause menu's Credits button, and the web
page's Credits dialog; `credits/CREDITS.md` and the licence texts ship in the bundle.

To change the artwork, vendor the file under `content/open/media/`, add its `sources.json` entry
(the `sha256` must match) and map it to a destination; textures the inputs lack come from
`art.rs`.

### Vendored texture packs

A Minecraft-layout resource pack under an open licence is vendored whole, by file name:

```sh
python3 tools/vendor_open_pack.py --source PACK.zip --name NAME --title TITLE --url URL \
  --version VERSION --sha256 SHA256 --author AUTHOR --license CC-BY-SA-4.0 \
  --evidence "where the licence is stated" --notice pack.txt \
  --vanilla .local/horizon-client/26.3/assets/minecraft/textures
```

It copies `assets/minecraft/textures/**.png` and their `.mcmeta`, unchanged, into
`content/open/packs/NAME/` and writes `pack.json` (source, version, author, licence, licence
evidence, the SHA-256 of every file). It leaves out, and lists under `excluded`, every file that
looks taken from Minecraft: the same bytes or pixels as any vanilla texture, a quarter or more of
its visible pixels equal to vanilla's file of that name, a recolour of it, or Mojang's names and
logos. The vanilla textures are only read for the comparison.

`sources.json`'s `packs` lists each manifest with its SHA-256 and `enabled`. `Sources::load` checks
every pack file and turns it into a `Media` whose destination is its own path. While a pack is
disabled it is checked but offered to nothing; once enabled, `Sources::image` and
`Sources::provides` take its file before a Minetest Game file with the same destination, and the
credits name the pack, ship its credit file and the CC BY-SA 4.0 text, and state the bundle's
licence as soon as the bundle uses one of its files.

Pixel Perfection Legacy is enabled. The generator takes each block and item texture from it when it
has the name, else from Minetest Game, else from `art.rs`; water keeps Minetest Game's coloured
art (`KEEP_MEDIA` in `assets.rs`), as the biome's water tint is white. A pack's textures are whole
(the grass side and the ores are not composited) and its own `.mcmeta` ship with them. Its grass,
fern, sugar cane and leaf textures are grey, as Minecraft's are: their models carry `tintindex` 0
on the faces Minecraft's models tint (the grass block's top and a second, overlay cube on its
sides with `grass_block_side_overlay`; every leaf face; both cross planes), their items carry
`tints`, and the biome takes grass and foliage colours from the colormaps, which are Horizon's
own gradients (`art::colormap`).

## The browser build

```sh
cd web
pnpm build:open      # bundle + WASM + static site in web/.build-open/static
pnpm package:open    # the same with the existing WASM build
pnpm serve:open      # http://127.0.0.1:8081 (single player only)
pnpm serve:open:multiplayer -- --server 127.0.0.1:25565   # the site and /connect to a local open server
pnpm deploy:open     # public: https://open.rccpr.tech (Cloudflare, the stored Alchemy profile)
```

The page opens on the landing menu both sites share (`web/menu/`, [WEB.md](../WEB.md#the-landing-menu)):
a player name, a choice of **Survival** (the Java server, through the relay below), **Horizon world**
(the shared world) or **Single player** (a fresh creative world in the tab), and "How to play" with
the controls, the credits and the private preview's opt-in **Share diagnostics** and **Record
sessions** (`web/telemetry.js`, bundled by packaging as app `horizon-open` when
`NEXT_PUBLIC_POSTHOG_*` is set in `web/.env.local`; nothing is sent unless the player ticks a box).
Nothing downloads until the player presses Play. `?args=`
passes game arguments, which the game still holds to the edition's scope.

The open build never fetches the private resource archive and needs no storage credentials. Its
site is separate from the private preview: `index.html` (from `web/open/`), `menu.js` and `menu.css`, the
shared `cache.js`, `release.js` and `telemetry.js`, `credits.json` and the licence texts, and
`release.json` (`"edition": "open-creative"`) pointing at immutable `assets/<sha256>/` files. Any
static file server can host single player.

### The shared world

The shared world is Horizon's own dedicated server (`horizon-client --dedicated`) on the same open
bundle: survival, normal difficulty, ten players, view distance 4 (`web/open-server/`:
`Dockerfile`, `server.properties`). Names are not verified (offline mode, as browsers have no
accounts); the world lives in the container and resets when it restarts or is replaced. Java and
Minecraft-backed clients cannot join it, and it is reachable only through the Worker.

To run one locally (fresh folders as for any run; `white-list=false` in its `server.properties`):

```sh
cargo build --profile fast -p horizon-client
mkdir srv && cd srv && echo eula=true > eula.txt && cp ../web/open-server/server.properties .
../target/fast/horizon-client --resources /tmp/open-resources --dedicated --nogui
```

Native open players join it with `--server HOST:PORT` (or Multiplayer on the title).

The private preview (https://horizon.rccpr.tech, WEB.md) sends its multiplayer to the same server
through the `SharedWorld` entrypoint of this Worker (a service binding, not reachable publicly).
Its players run the open bundle too, so every number agrees, wearing their own Minecraft art over
it (`web::overlay_art`; the bundle's integrity check skips the files listed in
`horizon-overlay.json`). Deploy this site before the private one when the bundle changes.

### Deploying

`pnpm deploy:open` (`web/scripts/deploy-open.mjs`) publishes, with the stored profile's API token:
the immutable assets to the site's own R2 bucket (`horizon-open-assets`, uploaded only when
missing); then, with wrangler (`pnpm dlx wrangler@4.162.0`, Docker for the image), the Worker
`horizon-open` (`web/src/open-worker.ts`, the page bundled in, no sign-in: everything it serves
is open), its Durable Object `OpenServer` with the server's container (one instance,
`standard-1`, stopped after ten idle minutes, no internet access) and its custom domain
(`--domain`). The image holds the server binary (`--server-binary`, default
`target/server/fast/horizon-client`: build it with `CARGO_TARGET_DIR=target/server cargo build
--profile fast -p horizon-client --bin horizon-client --no-default-features`) and
`web/.build-open/resources`, the bundle the page's archive was packed from, so the page and the
server always share content. It also deploys the relay below as a second container,
`SurvivalRelay` (one instance, `standard-1`, with internet access): `--relay-binary` (default
`target/fast/horizon-open-relay`), the same bundle, and `--survival HOST:PORT` (default
`survival.minecraft.rccpr.tech:25565`) baked into the image as `HORIZON_RELAY_UPSTREAM`;
`--survival ""` leaves it out. The first join after a deploy waits about a minute while the relay
fetches Mojang's server jar and derives the vanilla numbering on its own disk. `/connect?world=`
(`shared` or `survival`) accepts only WebSocket upgrades from the site's own origin and forwards
nothing of the browser's request; `/worlds.json` tells the menu which worlds exist. It touches no
Alchemy state and nothing of the private preview.

In any browser, Escape also ends the pointer lock: the game follows the page's real lock state
(`sync_web_pointer_lock`), so a lock the browser refused is asked for again on the next click and a
lock the browser ended pauses the game, as losing focus does. E closes the inventory without pausing.

## Joining a Java server through the relay

An open player cannot join a Minecraft server directly: the two number blocks, items and entities
differently. `crates/horizon-open-relay` sits between them. It accepts open players, logs each into
the upstream server as an offline-mode player of the same name (offline-mode servers only, as the
browser path already requires: `online-mode=false`, `enforce-secure-profile=false`), and
translates every packet:

- **Blocks**: chunk sections (single-value, paletted and global palettes, repacked for the open
  registry, counts recomputed), block updates, section updates and break effects. A vanilla block
  the bundle has keeps its properties (log axes); any other becomes the nearest open block by name,
  material and colour (`rules.rs`: dark oak → pine, concrete and terracotta → wool of the same
  colour, deepslate → stone, chests → planks). Thin or non-solid decoration
  (torches, rails, signs, buttons, carpets, beds, redstone, flower pots, vines) is air, or water
  when waterlogged. `horizon-open-relay mapping` lists what becomes air.
- **Items**: every stack (inventories, containers, the cursor, equipment, dropped items). A
  stand-in keeps the vanilla name as its `item_name` ("Golden Apple" on an apple); tools map by
  tier and kind, food to the apple, everything else to a stick. Components the bundle lacks are
  dropped. Clicks and creative slots go back as the vanilla item of the same name, with
  vanilla component ids.
- **Entities**: players, dropped items, experience orbs, falling blocks and the bundle's own mobs
  (below) are shown. A creature the bundle lacks shows as the bundle's mob nearest in shape
  (`rules::stand_in`: a pillager or witch as a zombie, a wither skeleton as a skeleton, a horse as a
  cow, a fox as a pig, a bat as a chicken...), with only the data every mob shares (flags, health;
  entries below 16). Anything else (boats, minecarts, projectiles, golems, ghasts, villagers...)
  is never spawned for the open player, and its later packets are dropped.
- **Attributes**: renumbered by name to the bundle's `attributes.json` order (the open wire ids);
  attributes the bundle lacks are dropped, so none is read as another.
- **Recipe book**: the server numbers its recipe displays in its own recipe order, the open client
  in the bundle's (`data/minecraft/recipe`, by name). Each display the server sends takes the id
  of the bundle's recipe that draws the same (shape, ingredients, result: `recipes.rs`); the others
  are left out, so the book never shows one recipe and places another. Picks from the book go back
  by the server's id. Crafting itself is the server's: the open client shows the result slot the
  server sends, so every Java recipe crafts, in the book or not.
- **Advancements**: `update_advancements` passes with each displayed advancement's icon translated
  as any stack is (a vanilla item the bundle lacks shows as its stand-in, named as vanilla's);
  titles, descriptions, backgrounds (`gui/advancements/backgrounds/*`, drawn by the bundle) and
  progress pass as sent, and so does `select_advancements_tab` (an id, no number).
- **Dropped**: sounds, particles, block entities (sign text, chest lids), mob effects, the server's
  recipe list (`update_recipes`: the open client reads none), tags, statistics, maps, trades, explosions (no knockback),
  waypoints, transfers and dialogs. Chat, commands, the tab list, scoreboards, titles, time, health,
  hunger and experience pass unchanged.
- **Brand**: the server's `minecraft:brand` becomes the bundle's open brand, so open clients stay.
  The relay derives it from the bundle it is given: it must be the bundle the players run.
- **Coverage**: the relay reads the bundle's `horizon-open-coverage.json` ([Coverage](#coverage)).
  A name the bundle registers without content counts as absent for every mapping above: rotten
  flesh is still the apple named "Rotten Flesh", a furnace still cobblestone, a horse still the
  cow. A bundle without the file has content for every name it registers.

`policy.rs` holds what happens to each packet, in both directions: passed, passed unless its entity
is hidden, translated, or dropped. Its test walks every packet id of the protocol: the dropped
packets must equal `src/dropped.txt`, and every built-in registry a forwarded packet still holds in
vanilla numbers must be listed in `src/gaps.txt` (beacon effects, villager data, the recipe
display kinds and book categories, which the bundle numbers as vanilla does). Change a packet's
handling and update the list in the same change.

Limits: the world looks approximate (stairs and slabs are full blocks, so the open client predicts
collisions that differ a little from the server's), creatures without a model or stand-in are
invisible and can still hurt, the server
sees every relayed player at the relay's address (IP bans apply to the relay), and the client's
mining speed comes from its stand-in tool, so the server may undo a break it thinks was too fast.

The relay reads the vanilla numbers from the server's own data generator at runtime (the bundle's
committed name lists, [Numbering](#numbering), give the same ones): `horizon-open-relay vanilla-data --out DIR` downloads Mojang's official server jar
(SHA-1 checked against Mojang's manifest; `--jar` uses one you have) and keeps
`reports/blocks.json` and `reports/registries.json`. `serve --vanilla-data DIR` does this on first
start.

```sh
cargo build --profile fast -p horizon-open-relay
target/fast/horizon-open-relay serve --upstream mc.example.com:25565 \
  --open-resources /tmp/open-resources --vanilla-data /tmp/relay-vanilla --listen 0.0.0.0:25566
target/fast/horizon-client --resources /tmp/open-resources --server 127.0.0.1:25566 --name Alex
```

Flags also read `HORIZON_RELAY_<FLAG>` from the environment (`HORIZON_RELAY_UPSTREAM`,
`HORIZON_RELAY_LISTEN`, `HORIZON_RELAY_OPEN_RESOURCES`, `HORIZON_RELAY_VANILLA_DATA`,
`HORIZON_RELAY_MAX_CONNECTIONS`, `HORIZON_RELAY_DEBUG`). `crates/horizon-open-relay/deploy/Dockerfile`
runs it in a container: the image holds the relay and the open bundle, and fetches the jar when it
first starts.

## Verification

- `cargo test -p horizon-open-content` (determinism, provenance, verification) and
  `cargo test -p horizon-client --test open_content` (every shared loader on the bundle, the
  palette, damaged bundles).
- `node --test web/tests/open.test.mjs` (the static site and its release manifest).
- Blender contact sheets: `cargo run --profile fast -p horizon-client --example
  open_content_mesh_probe -- RESOURCES OUT`, then `blender -b --factory-startup --python
  tools/open_content_blender.py -- --input OUT --out SHEETS`. It renders the real baked meshes
  (every palette block, the three log axes, grass, leaves, short grass, glass, water, the arm and
  held blocks; flat items are not in the sheets) with their textures and with a calibration image, and writes `report.json` with
  the winding, scale, UV, texture and alpha checks. Blender checks the assets; Horizon's own
  screenshots remain the reference for its lighting and shaders.
- Browser recordings: `tools/web/open_proof.sh OUT` (Xvfb, ffmpeg, agent-browser, node) serves
  nothing itself: start `pnpm serve:open` first. It records three videos of the static build driven
  with real keyboard and mouse input (`tools/web/cdp_input.mjs` sends held keys with their physical
  codes, typed text and pointer-locked mouse motion through the DevTools input domain) and logs the
  player's positions from the game's progress lines.
