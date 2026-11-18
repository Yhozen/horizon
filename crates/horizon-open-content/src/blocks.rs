//! The open content's blocks, authored here. Identifiers reuse the familiar
//! `minecraft:` names the shared gameplay code looks blocks up by, and the
//! bundle numbers blocks and their states exactly as Minecraft 26.3 does
//! ([`crate::numbering`]): every Minecraft block is registered, those this
//! crate does not define as inert support blocks. The facts about each block
//! are this crate's own.
use crate::numbering::{self, BlockStates};
use std::collections::{HashMap, HashSet};

/// What a block is for in this edition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// In the creative palette: placeable, breakable, with an item.
    Palette,
    /// Terrain and fluids players cannot pick (air, water, lava, grass).
    Terrain,
    /// A Minecraft block without content yet, registered so the numbering is
    /// Minecraft's: never generated or offered, every state inert, with no
    /// model, item, collision or light.
    Support,
}

/// How a block is drawn and collides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// Nothing at all (air).
    Air,
    /// A full opaque cube.
    Cube,
    /// A full cube drawn with holes or transparency (glass, leaves).
    SeeThroughCube,
    /// Two crossed planes, no collision (short grass, flowers, saplings).
    Cross,
    /// A fluid surface drawn by the fluid renderer.
    Fluid,
    /// Solid but not a full cube in some states: its shapes come from
    /// [`crate::shapes::of`] per state (slabs, stairs).
    Shaped,
    /// Present only so loaders resolve its name.
    Inert,
    /// A wall, fence, gate, pane or bars: its shapes and models follow its
    /// connections ([`crate::connected`]).
    Connected(crate::connected::Family),
}

/// The render layer (`ChunkSectionLayer`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Solid,
    Cutout,
    Translucent,
}

/// Which textures a block's faces use (paths under `textures/block/`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Faces {
    None,
    All(&'static str),
    /// Sides, top and bottom (logs: rotated by `axis`).
    Column {
        side: &'static str,
        end: &'static str,
    },
    /// Grass: a distinct top, sides and bottom.
    Grass {
        top: &'static str,
        side: &'static str,
        bottom: &'static str,
    },
    /// A workbench: top, bottom, a front on north and south, plain sides.
    Sided {
        top: &'static str,
        bottom: &'static str,
        front: &'static str,
        side: &'static str,
    },
    /// A cube with one texture per face: down, up, north, south, west,
    /// east (rotated by `axis` when it has one).
    Cube([&'static str; 6]),
    /// A log: a cube turned by `axis` whose top, lying down, is turned half
    /// a turn (Minecraft's horizontal column).
    Pillar([&'static str; 6]),
    /// A cube facing its `facing` (north unturned), with another front
    /// when `lit` (furnaces).
    Facing {
        faces: [&'static str; 6],
        lit_north: &'static str,
    },
    Cross(&'static str),
    /// A slab: its halves' bottom, top and side, and the six faces of the
    /// double slab (down, up, north, south, west, east).
    Slab {
        bottom: &'static str,
        top: &'static str,
        side: &'static str,
        double: [&'static str; 6],
    },
    /// Stairs: the bottom, top and side of its models.
    Stairs {
        bottom: &'static str,
        top: &'static str,
        side: &'static str,
    },
    /// Models drawn by [`crate::plants`] from its facts file.
    Plant,
    Fluid {
        still: &'static str,
        flow: &'static str,
    },
    /// Walls, fences and gates: one texture; panes and bars: the pane and
    /// its edge.
    Connected {
        texture: &'static str,
        edge: &'static str,
    },
}

/// The block's sound family (`SoundType`); identifiers only, the open edition
/// ships no audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Stone,
    Gravel,
    Sand,
    Grass,
    Wood,
    Glass,
    Wool,
    /// Another of Minecraft's sound types, by name (`block.<name>.break`).
    Named(&'static str),
    None,
}
impl Sound {
    pub fn name(self) -> &'static str {
        match self {
            Self::Stone => "stone",
            Self::Gravel => "gravel",
            Self::Sand => "sand",
            Self::Grass => "grass",
            Self::Wood => "wood",
            Self::Glass => "glass",
            Self::Wool => "wool",
            Self::Named(name) => name,
            Self::None => "stone",
        }
    }
}

/// The tool that mines a block quickly (`minecraft:mineable/<tool>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tool {
    None,
    Pickaxe,
    Axe,
    Shovel,
    Hoe,
}
impl Tool {
    pub const ALL: [Tool; 4] = [Self::Pickaxe, Self::Axe, Self::Shovel, Self::Hoe];
    pub fn tag(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Pickaxe => "pickaxe",
            Self::Axe => "axe",
            Self::Shovel => "shovel",
            Self::Hoe => "hoe",
        }
    }
}

/// How a block's mining is gated: which tool is fast, and the material a
/// tool must at least be made of for the block to drop anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mining {
    pub tool: Tool,
    /// The block drops nothing unless mined with `tool` of at least `needs`.
    pub requires_tool: bool,
    /// 0 any tool, 1 stone, 2 iron, 3 diamond (`minecraft:needs_*_tool`).
    pub needs: u8,
}
const HAND: Mining = Mining {
    tool: Tool::None,
    requires_tool: false,
    needs: 0,
};
const fn fast(tool: Tool) -> Mining {
    Mining {
        tool,
        requires_tool: false,
        needs: 0,
    }
}
const fn pick(needs: u8) -> Mining {
    Mining {
        tool: Tool::Pickaxe,
        requires_tool: true,
        needs,
    }
}

/// What breaking the block in survival gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drop {
    Itself,
    /// Another item (`minecraft:<id>`), between `min` and `max` of it.
    Item {
        id: &'static str,
        min: u8,
        max: u8,
        /// Fortune multiplies it as ores do.
        ore: bool,
    },
    /// Leaves: a chance of their sapling, of sticks and (oak) of an apple.
    Leaves {
        sapling: &'static str,
        apple: bool,
    },
    Nothing,
}

#[derive(Clone, Debug)]
pub struct Block {
    /// The path of its `minecraft:` identifier.
    pub id: &'static str,
    /// Its English name (the open bundle's `en_us` translation).
    pub name: &'static str,
    pub tier: Tier,
    pub form: Form,
    pub layer: Layer,
    pub faces: Faces,
    /// Properties in key order with their values in state order; states are
    /// their cartesian product, the last property varying fastest. [`defined`]
    /// and [`all`] set them to Minecraft's.
    pub properties: &'static [(&'static str, &'static [&'static str])],
    /// The default state's values, by key (Minecraft's, as `properties`).
    pub defaults: &'static [(&'static str, &'static str)],
    /// Destroy time; -1 is unbreakable outside creative.
    pub hardness: f32,
    pub resistance: f32,
    pub light: u8,
    /// Light it gives in its `lit=true` states (furnaces).
    pub lit_light: u8,
    /// Light it absorbs (0..=15).
    pub opacity: u8,
    pub sound: Sound,
    /// Java class chain the shared behaviour code dispatches on.
    pub classes: &'static [&'static str],
    /// `MapColor` colour (0 = none).
    pub map_color: u32,
    /// Creative palette category (palette blocks only).
    pub category: Option<Category>,
    pub mining: Mining,
    pub drop: Drop,
    /// Surface friction (0.6 for most blocks, ice is slippery).
    pub friction: f32,
}

/// The creative palette's tabs, blocks and items alike, in tab order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Ground,
    Construction,
    Colored,
    Plants,
    Minerals,
    Tools,
    Combat,
    Food,
    Ingredients,
}
impl Category {
    pub const ALL: [Category; 9] = [
        Self::Ground,
        Self::Construction,
        Self::Colored,
        Self::Plants,
        Self::Minerals,
        Self::Tools,
        Self::Combat,
        Self::Food,
        Self::Ingredients,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::Ground => "ground_and_stone",
            Self::Construction => "construction",
            Self::Colored => "colored_blocks",
            Self::Plants => "plants",
            Self::Minerals => "ores_and_minerals",
            Self::Tools => "tools",
            Self::Combat => "combat",
            Self::Food => "food",
            Self::Ingredients => "ingredients",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Ground => "Ground and Stone",
            Self::Construction => "Construction",
            Self::Colored => "Colored Blocks",
            Self::Plants => "Plants",
            Self::Minerals => "Ores and Minerals",
            Self::Tools => "Tools",
            Self::Combat => "Combat",
            Self::Food => "Food",
            Self::Ingredients => "Ingredients",
        }
    }
    /// The item that stands for the tab.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Ground => "grass_block",
            Self::Construction => "bricks",
            Self::Colored => "red_wool",
            Self::Plants => "poppy",
            Self::Minerals => "iron_ore",
            Self::Tools => "stone_pickaxe",
            Self::Combat => "iron_sword",
            Self::Food => "apple",
            Self::Ingredients => "stick",
        }
    }
    /// Its tab's place: top row columns 0..=4, bottom row columns 0..=3
    /// (search and the inventory take column 6 of each row).
    pub fn place(self) -> (&'static str, u32) {
        match self {
            Self::Ground => ("TOP", 0),
            Self::Construction => ("TOP", 1),
            Self::Colored => ("TOP", 2),
            Self::Plants => ("TOP", 3),
            Self::Minerals => ("TOP", 4),
            Self::Tools => ("BOTTOM", 0),
            Self::Combat => ("BOTTOM", 1),
            Self::Food => ("BOTTOM", 2),
            Self::Ingredients => ("BOTTOM", 3),
        }
    }
}

const NO_PROPERTIES: &[(&str, &[&str])] = &[];
const BLOCK: &[&str] = &["Block"];
const AXIS: &[(&str, &[&str])] = &[("axis", &["x", "y", "z"])];
const LEVELS: &[(&str, &[&str])] = &[(
    "level",
    &[
        "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15",
    ],
)];

#[allow(clippy::too_many_arguments)]
const fn cube(
    id: &'static str,
    name: &'static str,
    texture: &'static str,
    hardness: f32,
    sound: Sound,
    map_color: u32,
    category: Category,
    mining: Mining,
) -> Block {
    Block {
        id,
        name,
        tier: Tier::Palette,
        form: Form::Cube,
        layer: Layer::Solid,
        faces: Faces::All(texture),
        properties: NO_PROPERTIES,
        defaults: &[],
        hardness,
        resistance: hardness * 3.0,
        light: 0,
        lit_light: 0,
        opacity: 15,
        sound,
        classes: BLOCK,
        map_color,
        category: Some(category),
        mining,
        drop: Drop::Itself,
        friction: 0.6,
    }
}

/// A stone-like block a pickaxe must mine for it to drop.
const fn rock(
    id: &'static str,
    name: &'static str,
    hardness: f32,
    map_color: u32,
    category: Category,
) -> Block {
    cube(
        id,
        name,
        id,
        hardness,
        Sound::Stone,
        map_color,
        category,
        pick(0),
    )
}

const fn wool(id: &'static str, name: &'static str, map_color: u32) -> Block {
    cube(
        id,
        name,
        id,
        0.8,
        Sound::Wool,
        map_color,
        Category::Colored,
        HAND,
    )
}

/// An ore: stone with a mineral, dropping its mineral.
const fn ore(
    id: &'static str,
    name: &'static str,
    needs: u8,
    drop: &'static str,
    max: u8,
) -> Block {
    Block {
        drop: Drop::Item {
            id: drop,
            min: 1,
            max,
            ore: true,
        },
        ..cube(
            id,
            name,
            id,
            3.0,
            Sound::Stone,
            0x707070,
            Category::Minerals,
            pick(needs),
        )
    }
}

/// A storage block of a mineral.
const fn mineral(
    id: &'static str,
    name: &'static str,
    hardness: f32,
    needs: u8,
    map_color: u32,
) -> Block {
    Block {
        resistance: 6.0,
        ..cube(
            id,
            name,
            id,
            hardness,
            Sound::Stone,
            map_color,
            Category::Minerals,
            pick(needs),
        )
    }
}

/// One tree species: its log, planks and leaves.
fn tree(
    species: &'static str,
    names: [&'static str; 3],
    textures: [&'static str; 4],
    apple: bool,
) -> [Block; 3] {
    let [log, planks, leaves] = names;
    let [side, end, wood, leaf] = textures;
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    [
        Block {
            faces: Faces::Column { side, end },
            properties: AXIS,
            defaults: &[("axis", "y")],
            classes: &["RotatedPillarBlock", "Block"],
            ..cube(
                leak(format!("{species}_log")),
                log,
                "",
                2.0,
                Sound::Wood,
                0x8F7748,
                Category::Construction,
                fast(Tool::Axe),
            )
        },
        cube(
            leak(format!("{species}_planks")),
            planks,
            wood,
            2.0,
            Sound::Wood,
            0x8F7748,
            Category::Construction,
            fast(Tool::Axe),
        ),
        Block {
            form: Form::SeeThroughCube,
            layer: Layer::Cutout,
            opacity: 1,
            drop: Drop::Leaves {
                sapling: leak(format!("{species}_sapling")),
                apple,
            },
            ..cube(
                leak(format!("{species}_leaves")),
                leaves,
                leaf,
                0.2,
                Sound::Grass,
                0x007C00,
                Category::Plants,
                fast(Tool::Hoe),
            )
        },
    ]
}

/// A cross-shaped plant: no collision, broken at once, drawn flat as an item.
const fn plant(id: &'static str, name: &'static str, map_color: u32) -> Block {
    Block {
        id,
        name,
        tier: Tier::Palette,
        form: Form::Cross,
        layer: Layer::Cutout,
        faces: Faces::Cross(id),
        properties: NO_PROPERTIES,
        defaults: &[],
        hardness: 0.0,
        resistance: 0.0,
        light: 0,
        lit_light: 0,
        opacity: 0,
        sound: Sound::Grass,
        classes: BLOCK,
        map_color,
        category: Some(Category::Plants),
        mining: HAND,
        drop: Drop::Itself,
        friction: 0.6,
    }
}

/// Every block the open content defines itself, with Minecraft's properties
/// and default state, in the order this file lists them.
pub fn defined() -> Vec<Block> {
    let vanilla: HashMap<&str, &BlockStates> =
        numbering::blocks().iter().map(|b| (b.name, b)).collect();
    let mut blocks = authored();
    crate::cubes::add(&mut blocks);
    crate::slabs::add(&mut blocks);
    crate::connected::add(&mut blocks);
    crate::plants::add(&mut blocks);
    blocks
        .into_iter()
        .map(|mut block| {
            let states = vanilla
                .get(block.id)
                .unwrap_or_else(|| panic!("{} is not a Minecraft 26.3 block", block.id));
            block.properties = states.properties;
            block.defaults = states.defaults;
            block
        })
        .collect()
}

fn authored() -> Vec<Block> {
    use Category::*;
    let mut blocks = vec![
        Block {
            id: "air",
            name: "Air",
            tier: Tier::Terrain,
            form: Form::Air,
            layer: Layer::Solid,
            faces: Faces::None,
            properties: NO_PROPERTIES,
            defaults: &[],
            hardness: 0.0,
            resistance: 0.0,
            light: 0,
            lit_light: 0,
            opacity: 0,
            sound: Sound::None,
            classes: &["AirBlock", "Block"],
            map_color: 0,
            category: None,
            mining: HAND,
            drop: Drop::Nothing,
            friction: 0.6,
        },
        Block {
            drop: Drop::Item {
                id: "cobblestone",
                min: 1,
                max: 1,
                ore: false,
            },
            ..rock("stone", "Stone", 1.5, 0x707070, Ground)
        },
        rock("cobblestone", "Cobblestone", 2.0, 0x707070, Ground),
        cube(
            "dirt",
            "Dirt",
            "dirt",
            0.5,
            Sound::Gravel,
            0x976D4D,
            Ground,
            fast(Tool::Shovel),
        ),
        Block {
            faces: Faces::Grass {
                top: "grass_block_top",
                side: "grass_block_side",
                bottom: "dirt",
            },
            drop: Drop::Item {
                id: "dirt",
                min: 1,
                max: 1,
                ore: false,
            },
            ..cube(
                "grass_block",
                "Grass Block",
                "",
                0.6,
                Sound::Grass,
                0x7FB238,
                Ground,
                fast(Tool::Shovel),
            )
        },
        cube(
            "sand",
            "Sand",
            "sand",
            0.5,
            Sound::Sand,
            0xF7E9A3,
            Ground,
            fast(Tool::Shovel),
        ),
        cube(
            "gravel",
            "Gravel",
            "gravel",
            0.6,
            Sound::Gravel,
            0x707070,
            Ground,
            fast(Tool::Shovel),
        ),
        rock("sandstone", "Sandstone", 0.8, 0xF7E9A3, Ground),
        rock("smooth_stone", "Smooth Stone", 2.0, 0x707070, Ground),
        Block {
            resistance: 1200.0,
            ..cube(
                "obsidian",
                "Obsidian",
                "obsidian",
                50.0,
                Sound::Stone,
                0x191919,
                Ground,
                pick(3),
            )
        },
        Block {
            hardness: -1.0,
            resistance: 3_600_000.0,
            drop: Drop::Nothing,
            ..cube(
                "bedrock",
                "Bedrock",
                "bedrock",
                -1.0,
                Sound::Stone,
                0x707070,
                Ground,
                HAND,
            )
        },
    ];
    blocks.extend(tree(
        "oak",
        ["Oak Log", "Oak Planks", "Oak Leaves"],
        ["oak_log", "oak_log_top", "oak_planks", "oak_leaves"],
        true,
    ));
    blocks.extend([
        rock("bricks", "Bricks", 2.0, 0x993333, Construction),
        Block {
            form: Form::SeeThroughCube,
            layer: Layer::Cutout,
            opacity: 0,
            classes: &["TransparentBlock", "HalfTransparentBlock", "Block"],
            ..cube(
                "glass",
                "Glass",
                "glass",
                0.3,
                Sound::Glass,
                0,
                Construction,
                HAND,
            )
        },
        Block {
            light: 15,
            ..cube(
                "glowstone",
                "Light Block",
                "glowstone",
                0.3,
                Sound::Glass,
                0xF7E9A3,
                Construction,
                HAND,
            )
        },
        wool("white_wool", "White Wool", 0xFFFFFF),
        wool("black_wool", "Black Wool", 0x191919),
        wool("red_wool", "Red Wool", 0x993333),
        wool("orange_wool", "Orange Wool", 0xD87F33),
        wool("yellow_wool", "Yellow Wool", 0xE5E533),
        wool("green_wool", "Green Wool", 0x667F33),
        wool("blue_wool", "Blue Wool", 0x334CB2),
        wool("purple_wool", "Purple Wool", 0x7F3FB2),
    ]);
    blocks.extend([
        fluid("water", "Water", 0x4040FF, Layer::Translucent),
        Block {
            light: 15,
            ..fluid("lava", "Lava", 0xFF0000, Layer::Solid)
        },
        // Minecraft's bubble column is water to look at and to swim in (its
        // fluid is a water source); the server pushes players up or down.
        Block {
            id: "bubble_column",
            name: "Bubble Column",
            hardness: 0.0,
            resistance: 0.0,
            classes: &["BubbleColumnBlock", "Block"],
            properties: NO_PROPERTIES,
            defaults: &[],
            ..fluid("water", "Water", 0x4040FF, Layer::Translucent)
        },
        Block {
            tier: Tier::Terrain,
            category: None,
            drop: Drop::Nothing,
            ..plant("short_grass", "Short Grass", 0x7FB238)
        },
    ]);
    // Everything after the first palette, in palette order within each tab.
    blocks.extend([
        rock(
            "mossy_cobblestone",
            "Mossy Cobblestone",
            2.0,
            0x707070,
            Ground,
        ),
        rock("stone_bricks", "Stone Bricks", 1.5, 0x707070, Ground),
        rock("granite", "Desert Stone", 1.5, 0x976D4D, Ground),
        rock(
            "polished_granite",
            "Desert Stone Block",
            1.5,
            0x976D4D,
            Ground,
        ),
        rock(
            "chiseled_sandstone",
            "Sandstone Bricks",
            0.8,
            0xF7E9A3,
            Ground,
        ),
        rock("cut_sandstone", "Sandstone Block", 0.8, 0xF7E9A3, Ground),
        cube(
            "red_sand",
            "Desert Sand",
            "red_sand",
            0.5,
            Sound::Sand,
            0xD87F33,
            Ground,
            fast(Tool::Shovel),
        ),
        rock("red_sandstone", "Desert Sandstone", 0.8, 0xD87F33, Ground),
        rock(
            "chiseled_red_sandstone",
            "Desert Sandstone Bricks",
            0.8,
            0xD87F33,
            Ground,
        ),
        rock(
            "cut_red_sandstone",
            "Desert Sandstone Block",
            0.8,
            0xD87F33,
            Ground,
        ),
        Block {
            drop: Drop::Item {
                id: "clay_ball",
                min: 4,
                max: 4,
                ore: false,
            },
            ..cube(
                "clay",
                "Clay",
                "clay",
                0.6,
                Sound::Gravel,
                0xA4A8B8,
                Ground,
                fast(Tool::Shovel),
            )
        },
        cube(
            "snow_block",
            "Snow Block",
            "snow_block",
            0.2,
            Sound::Wool,
            0xFFFFFF,
            Ground,
            fast(Tool::Shovel),
        ),
        Block {
            friction: 0.98,
            ..cube(
                "ice",
                "Ice",
                "ice",
                0.5,
                Sound::Glass,
                0xA0A0FF,
                Ground,
                fast(Tool::Pickaxe),
            )
        },
    ]);
    for (species, names, textures) in [
        (
            "spruce",
            ["Pine Log", "Pine Planks", "Pine Needles"],
            [
                "spruce_log",
                "spruce_log_top",
                "spruce_planks",
                "spruce_leaves",
            ],
        ),
        (
            "birch",
            ["Aspen Log", "Aspen Planks", "Aspen Leaves"],
            ["birch_log", "birch_log_top", "birch_planks", "birch_leaves"],
        ),
        (
            "jungle",
            ["Jungle Log", "Jungle Planks", "Jungle Leaves"],
            [
                "jungle_log",
                "jungle_log_top",
                "jungle_planks",
                "jungle_leaves",
            ],
        ),
        (
            "acacia",
            ["Acacia Log", "Acacia Planks", "Acacia Leaves"],
            [
                "acacia_log",
                "acacia_log_top",
                "acacia_planks",
                "acacia_leaves",
            ],
        ),
    ] {
        blocks.extend(tree(species, names, textures, false));
    }
    blocks.extend([
        Block {
            faces: Faces::Sided {
                top: "crafting_table_top",
                bottom: "oak_planks",
                front: "crafting_table_front",
                side: "crafting_table_side",
            },
            classes: &["CraftingTableBlock", "Block"],
            ..cube(
                "crafting_table",
                "Crafting Table",
                "",
                2.5,
                Sound::Wood,
                0x8F7748,
                Construction,
                fast(Tool::Axe),
            )
        },
        Block {
            faces: Faces::Column {
                side: "bookshelf",
                end: "oak_planks",
            },
            ..cube(
                "bookshelf",
                "Bookshelf",
                "",
                1.5,
                Sound::Wood,
                0x8F7748,
                Construction,
                fast(Tool::Axe),
            )
        },
        Block {
            form: Form::SeeThroughCube,
            layer: Layer::Cutout,
            opacity: 15,
            classes: &["TransparentBlock", "HalfTransparentBlock", "Block"],
            ..cube(
                "tinted_glass",
                "Obsidian Glass",
                "tinted_glass",
                0.3,
                Sound::Glass,
                0x191919,
                Construction,
                HAND,
            )
        },
        wool("light_gray_wool", "Light Gray Wool", 0x999999),
        wool("gray_wool", "Gray Wool", 0x4C4C4C),
        wool("brown_wool", "Brown Wool", 0x664C33),
        wool("pink_wool", "Pink Wool", 0xF27FA5),
        wool("magenta_wool", "Magenta Wool", 0xB24CD8),
        wool("cyan_wool", "Cyan Wool", 0x4C7F99),
        wool("lime_wool", "Lime Wool", 0x7FCC19),
        plant("oak_sapling", "Oak Sapling", 0x007C00),
        plant("spruce_sapling", "Pine Sapling", 0x007C00),
        plant("birch_sapling", "Aspen Sapling", 0x007C00),
        plant("jungle_sapling", "Jungle Sapling", 0x007C00),
        plant("acacia_sapling", "Acacia Sapling", 0x007C00),
        plant("dandelion", "Dandelion", 0xE5E533),
        plant("oxeye_daisy", "White Dandelion", 0xFFFFFF),
        plant("poppy", "Rose", 0x993333),
        plant("orange_tulip", "Tulip", 0xD87F33),
        plant("cornflower", "Geranium", 0x334CB2),
        plant("allium", "Viola", 0x7F3FB2),
        plant("brown_mushroom", "Brown Mushroom", 0x664C33),
        plant("red_mushroom", "Red Mushroom", 0x993333),
        plant("fern", "Fern", 0x007C00),
        plant("dead_bush", "Dry Shrub", 0x8F7748),
        plant("sugar_cane", "Papyrus", 0x007C00),
        Block {
            faces: Faces::Column {
                side: "cactus_side",
                end: "cactus_top",
            },
            ..cube(
                "cactus",
                "Cactus",
                "",
                0.4,
                Sound::Wool,
                0x007C00,
                Plants,
                HAND,
            )
        },
        ore("coal_ore", "Coal Ore", 0, "coal", 1),
        ore("copper_ore", "Copper Ore", 1, "raw_copper", 1),
        ore("iron_ore", "Iron Ore", 1, "raw_iron", 1),
        ore("gold_ore", "Gold Ore", 2, "raw_gold", 1),
        ore("diamond_ore", "Diamond Ore", 2, "diamond", 1),
        mineral("coal_block", "Block of Coal", 5.0, 0, 0x191919),
        mineral("copper_block", "Block of Copper", 3.0, 1, 0xD87F33),
        mineral("iron_block", "Block of Iron", 5.0, 1, 0xA7A7A7),
        mineral("gold_block", "Block of Gold", 3.0, 2, 0xFAEE4D),
        mineral("diamond_block", "Block of Diamond", 5.0, 2, 0x5CDBD5),
    ]);
    blocks
}

/// A fluid block (`LiquidBlock`) named as its fluid, `water` or `lava`: drawn
/// by the fluid renderer from `<id>_still` and `<id>_flow`, with no collision
/// or item; level 0 is the source, 1 to 7 flowing, 8 and up falling.
fn fluid(id: &'static str, name: &'static str, map_color: u32, layer: Layer) -> Block {
    let (still, flow) = match id {
        "lava" => ("lava_still", "lava_flow"),
        _ => ("water_still", "water_flow"),
    };
    Block {
        id,
        name,
        tier: Tier::Terrain,
        form: Form::Fluid,
        layer,
        faces: Faces::Fluid { still, flow },
        properties: LEVELS,
        defaults: &[("level", "0")],
        hardness: 100.0,
        resistance: 100.0,
        light: 0,
        lit_light: 0,
        opacity: 1,
        sound: Sound::None,
        classes: &["LiquidBlock", "Block"],
        map_color,
        category: None,
        mining: HAND,
        drop: Drop::Nothing,
        friction: 0.6,
    }
}

/// The inert stand-in for a Minecraft block without content ([`all`] gives
/// it Minecraft's properties).
pub fn support(id: &'static str) -> Block {
    let air = matches!(id, "cave_air" | "void_air");
    Block {
        id,
        name: "",
        tier: Tier::Support,
        form: if air { Form::Air } else { Form::Inert },
        layer: Layer::Solid,
        faces: Faces::None,
        properties: NO_PROPERTIES,
        defaults: &[],
        hardness: 0.0,
        resistance: 0.0,
        light: 0,
        lit_light: 0,
        opacity: 0,
        sound: Sound::None,
        classes: if air { &["AirBlock", "Block"] } else { BLOCK },
        map_color: 0,
        category: None,
        mining: HAND,
        drop: Drop::Nothing,
        friction: 0.6,
    }
}

impl Block {
    pub fn key(&self) -> String {
        format!("minecraft:{}", self.id)
    }
    /// Its states' property maps, in state order.
    pub fn states(&self) -> Vec<Vec<(&'static str, &'static str)>> {
        let mut states = vec![Vec::new()];
        for (key, values) in self.properties {
            states = states
                .into_iter()
                .flat_map(|state| {
                    values.iter().map(move |value| {
                        let mut next = state.clone();
                        next.push((*key, *value));
                        next
                    })
                })
                .collect();
        }
        states
    }
    /// The default state's index among [`Self::states`]: its values' places,
    /// read as digits with the last property least significant.
    pub fn default_index(&self) -> usize {
        self.properties.iter().fold(0, |index, (key, values)| {
            let value = self
                .defaults
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v)
                .unwrap_or_else(|| panic!("{}: no default {key}", self.id));
            let place = values
                .iter()
                .position(|v| v == value)
                .unwrap_or_else(|| panic!("{}: default {key}={value} is not a value", self.id));
            index * values.len() + place
        })
    }
    pub fn state_count(&self) -> usize {
        numbering::state_count(self.properties)
    }
    /// Whether it has a property of that name.
    pub fn has_property(&self, key: &str) -> bool {
        self.properties.iter().any(|(k, _)| *k == key)
    }
    pub fn is_air(&self) -> bool {
        self.form == Form::Air
    }
    pub fn has_item(&self) -> bool {
        self.tier == Tier::Palette
    }
    pub fn collides(&self) -> bool {
        matches!(self.form, Form::Cube | Form::SeeThroughCube | Form::Shaped)
    }
    pub fn opaque(&self) -> bool {
        self.form == Form::Cube
    }
    /// Its outline when targeted: a full cube, the grass's smaller box, or none.
    pub fn outline(&self) -> Vec<[f64; 6]> {
        match self.form {
            Form::Cube | Form::SeeThroughCube | Form::Shaped => {
                vec![[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]]
            }
            Form::Cross if self.id == "short_grass" => {
                vec![[0.125, 0.0, 0.125, 0.875, 0.8125, 0.875]]
            }
            Form::Cross => vec![[0.1875, 0.0, 0.1875, 0.8125, 0.8125, 0.8125]],
            _ => Vec::new(),
        }
    }
    /// The light it gives in a state.
    pub fn emission(&self, properties: &[(&str, &str)]) -> u8 {
        if properties.contains(&("lit", "true")) {
            self.lit_light
        } else {
            self.light
        }
    }
    /// The fluid it is made of (`water` or `lava`), if it is a fluid block.
    pub fn fluid(&self) -> Option<&'static str> {
        match self.faces {
            Faces::Fluid {
                still: "lava_still",
                ..
            } => Some("lava"),
            Faces::Fluid { .. } => Some("water"),
            _ => None,
        }
    }
    /// Every texture its model and fluid renderer read.
    pub fn textures(&self) -> Vec<&'static str> {
        match self.faces {
            Faces::None => Vec::new(),
            Faces::All(t) | Faces::Cross(t) => vec![t],
            Faces::Column { side, end } => vec![side, end],
            Faces::Grass { top, side, bottom } => vec![top, side, bottom],
            Faces::Sided {
                top,
                bottom,
                front,
                side,
            } => vec![top, bottom, front, side],
            Faces::Cube(faces) | Faces::Pillar(faces) => unique(&faces),
            Faces::Facing { faces, lit_north } => {
                let mut all = unique(&faces);
                all.push(lit_north);
                all
            }
            Faces::Slab {
                bottom,
                top,
                side,
                double,
            } => {
                let mut all = vec![bottom, top, side];
                all.extend(double);
                unique(&all)
            }
            Faces::Stairs { bottom, top, side } => unique(&[bottom, top, side]),
            Faces::Fluid { still, flow } => vec![still, flow],
            Faces::Connected { texture, edge } => unique(&[texture, edge]),
            Faces::Plant => crate::plants::textures(self.id),
        }
    }
}

/// Names in first-seen order, each once.
fn unique(names: &[&'static str]) -> Vec<&'static str> {
    let mut out = Vec::new();
    for name in names {
        if !out.contains(name) {
            out.push(*name);
        }
    }
    out
}

/// The open bundle's blocks in Minecraft's registry order: its own, and an
/// inert support block for every other Minecraft block, each with
/// Minecraft's properties, so block and state ids are Minecraft's. A name the
/// shared loaders require that Minecraft lacks would follow, sorted.
pub fn all() -> Vec<Block> {
    let mut own: HashMap<&str, Block> = defined().into_iter().map(|b| (b.id, b)).collect();
    let mut blocks: Vec<Block> = numbering::blocks()
        .iter()
        .map(|states| {
            let mut block = own
                .remove(states.name)
                .unwrap_or_else(|| support(states.name));
            block.properties = states.properties;
            block.defaults = states.defaults;
            block
        })
        .collect();
    let known: HashSet<&str> = blocks.iter().map(|b| b.id).collect();
    let mut extra: Vec<&'static str> = horizon_world::level::data::NAMED_BLOCKS
        .iter()
        .map(|name| name.strip_prefix("minecraft:").expect("a minecraft: name"))
        .filter(|id| !known.contains(id))
        .collect();
    extra.sort_unstable();
    extra.dedup();
    blocks.extend(extra.into_iter().map(support));
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_fills_its_tabs() {
        let palette: Vec<_> = defined()
            .into_iter()
            .filter(|b| b.tier == Tier::Palette)
            .collect();
        assert_eq!(palette.len(), PALETTE_BLOCKS);
        for category in [
            Category::Ground,
            Category::Construction,
            Category::Colored,
            Category::Plants,
            Category::Minerals,
        ] {
            assert!(palette.iter().any(|b| b.category == Some(category)));
        }
        // Stone-like blocks need a pickaxe; wood and plants do not.
        let find = |id: &str| palette.iter().find(|b| b.id == id).unwrap();
        assert!(find("stone").mining.requires_tool);
        assert!(find("iron_ore").mining.needs == 1);
        assert!(!find("oak_log").mining.requires_tool);
        assert_eq!(
            find("oak_leaves").drop,
            Drop::Leaves {
                sapling: "oak_sapling",
                apple: true
            }
        );
    }

    /// The number of palette blocks (checked again by the client's test).
    pub const PALETTE_BLOCKS: usize = 936;

    #[test]
    fn states_vary_the_last_property_fastest_and_have_a_default() {
        let log = defined().into_iter().find(|b| b.id == "oak_log").unwrap();
        assert_eq!(
            log.states(),
            vec![
                vec![("axis", "x")],
                vec![("axis", "y")],
                vec![("axis", "z")]
            ]
        );
        assert_eq!(log.default_index(), 1);
        for block in all() {
            block.default_index();
        }
    }

    #[test]
    fn air_comes_first_and_names_are_unique() {
        let blocks = all();
        assert_eq!(blocks[0].id, "air");
        let mut ids: Vec<_> = blocks.iter().map(|b| b.id).collect();
        ids.sort_unstable();
        let len = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), len);
    }
}
