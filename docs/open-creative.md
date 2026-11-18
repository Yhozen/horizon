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
page says so before play. There are no mobs, sound, weather, furnaces or other dimensions.

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
own). Open players see these on any server they reach through the relay; the shared world's own
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
  name and `horizon-prepare open` as the fix); `--benchmark`, `--dev-mod`, `--weather` and other
  dimensions are errors; single player runs in the frame (`--in-frame`), and every world,
  a dedicated server's too, uses the open terrain preset. Arguments from the browser's `?args=`
  go through the same code.
- **Servers** (`assets::server_brand`): an open server's `minecraft:brand` is `horizon-open/`
  and the start of its provenance record's SHA-256. Both ends number blocks, items, entities and
  the other network registries from the bundle's `registries.json`, so an open player leaves any
  server whose brand differs (a Minecraft server, or an open one with other content), and a
  Minecraft-backed player leaves an open server, before their numbers could disagree.
- **Worlds** (`check_world`): a world made with the open bundle carries `horizon-content.json`.
  The open bundle opens only such worlds (a new folder becomes one) and refuses vanilla worlds
  before reading or writing them; the vanilla game refuses open worlds.
- **In the game** (`open_allows`): changing dimension is refused with a chat line, and the
  commands are those that only use what the bundle has (`OPEN_COMMANDS`: `/gamemode`, `/give`,
  `/clear`, `/tp`, `/time`, `/setblock`, `/fill`, `/clone`, `/kill`, `/xp`, `/spawnpoint`,
  `/difficulty`, chat commands and a few more). Game rules keep weather, mob spawning, random
  ticks, fire spread and water source creation off, and scheduled fluid ticks are dropped, so
  water is fixed scenery.
- **Menus**: the title has Singleplayer, Multiplayer, Options and Quit; the pause menu offers Back
  to the World, Options, Credits, Statistics and Save and Quit to Title (Disconnect on a server);
  the options page offers video, controls, chat, accessibility and credits only.

## Preparing the bundle

```sh
cargo run --profile fast -p horizon-prepare -- open --out /tmp/open-resources
cargo run --profile fast -p horizon-prepare -- open --out /tmp/open-resources --check   # verify only
cargo run --profile fast -p horizon-client -- --resources /tmp/open-resources --world Open
```

`horizon-prepare open` dispatches before any Minecraft, launcher or JDK discovery: it needs no
Java and no network. It is the `open` feature of horizon-prepare (on by default); the launcher links
the preparer's library with `default-features = false`, so it carries none of the generator. `--archive FILE` also writes the browser's data archive. Preparing twice from
the same inputs gives byte-identical files.

The generator is `crates/horizon-open-content`:

| Module | Writes |
|---|---|
| `blocks.rs` | the block definitions: the palette, terrain (air, water, short grass) and inert support blocks, with each block's tool, tier and drop |
| `items.rs` | the items that are not blocks (tools, food, materials), their components, the recipes, item tags and loot tables |
| `tables.rs` | the per-state tables (`facts.json`, `gameplay-facts.json`, `block-behaviours.json`, shapes, movement, tags), the biome, dimension and day cycle, player attributes, damage types, loot |
| `assets.rs` | textures, block states, models (its own display transforms, `item/generated` and `item/handheld`), items, the creative palette (`creative-inventory.json`) |
| `art.rs` | original art: bedrock, breaking cracks, the interface, sun, moon, clouds, the player's model and skin |
| `art_survival.rs` | original art: hearts (every kind the HUD draws), food, air, armour, experience and jump bars, the inventory, crafting table, recipe book and game-mode switcher screens, empty equipment slots, toasts, menu icons, world and server list icons, statistics headers, the vignette, the title's wordmark and panorama, the crafting table's faces |
| `lang.rs` | the English interface text: menus, survival, death messages, joining and leaving, statistics, the allowed commands' replies and errors |
| `sources.rs` | `content/open/sources.json`: inputs, checksums, credits and notices |

Building fails on a missing or changed input, a licence whose text is not vendored, a vendored
file nobody attributed, or an unresolved model, texture, parent or item reference.

### Support blocks

The shared block-behaviour loader resolves about 280 vanilla blocks by name
(`horizon_world::level::data::NAMED_BLOCKS`). The bundle gives each name it does not define an
inert support block: one state (a few keep the properties shared code validates by name, such as
lava's levels), no model, item, collision or light, never generated and never offered. The
manifest counts them (`support_blocks`).

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
  colour, deepslate → stone, lava → orange wool, chests → planks). Thin or non-solid decoration
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
- **Dropped**: sounds, particles, block entities (sign text, chest lids), mob effects, recipes and
  the recipe book, advancements, tags, statistics, maps, trades, explosions (no knockback),
  waypoints, transfers and dialogs. Chat, commands, the tab list, scoreboards, titles, time, health,
  hunger and experience pass unchanged.
- **Brand**: the server's `minecraft:brand` becomes the bundle's open brand, so open clients stay.
  The relay derives it from the bundle it is given: it must be the bundle the players run.

`policy.rs` holds what happens to each packet, in both directions: passed, passed unless its entity
is hidden, translated, or dropped. Its test walks every packet id of the protocol: the dropped
packets must equal `src/dropped.txt`, and every built-in registry a forwarded packet still holds in
vanilla numbers must be listed in `src/gaps.txt` (beacon effects, villager data). Change a packet's
handling and update the list in the same change.

Limits: the world looks approximate (stairs and slabs are full blocks, so the open client predicts
collisions that differ a little from the server's), creatures without a model or stand-in are
invisible and can still hurt, the server
sees every relayed player at the relay's address (IP bans apply to the relay), and the client's
mining speed comes from its stand-in tool, so the server may undo a break it thinks was too fast.

The vanilla numbers come from the server's own data generator at runtime, never from the
repository: `horizon-open-relay vanilla-data --out DIR` downloads Mojang's official server jar
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
