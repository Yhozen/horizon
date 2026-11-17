//! Horizon's original artwork for survival play and the menus around it:
//! the status bars (hearts, food, air, armour, experience), the inventory,
//! crafting table and recipe book screens, toasts, the world and server
//! lists, statistics, the title's backdrop and wordmark, and the crafting
//! table's block faces. Every pixel is drawn here (generator `art`).
use crate::{
    Bundle, Origin, Result,
    image::{Rgba, noise},
};
use serde_json::json;

const GENERATOR: &str = "art";
const SPRITES: &str = "assets/minecraft/textures/gui/sprites";

// The light panel palette shared with the creative screen (art.rs).
const PANEL_EDGE: [u8; 4] = [0xa9, 0xac, 0xbb, 255];
const PANEL: [u8; 4] = [0xf3, 0xf3, 0xf8, 255];
const PANEL_SOFT: [u8; 4] = [0xd6, 0xd8, 0xe4, 255];
const SHADE: [u8; 4] = [0x8a, 0x8e, 0x9e, 255];
const WHITE: [u8; 4] = [0xff, 0xff, 0xff, 255];
const SLOT_FILL: [u8; 4] = [0xc4, 0xc7, 0xd4, 255];
const PLATE: [u8; 4] = [0x20, 0x22, 0x2c, 255];
const RULE: [u8; 4] = [0x3a, 0x3d, 0x4a, 255];
const ACCENT: [u8; 4] = [0x9a, 0x86, 0xff, 255];
const INK: [u8; 4] = [0xec, 0xed, 0xf3, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

fn lighten(c: [u8; 4], by: u8) -> [u8; 4] {
    [
        c[0].saturating_add(by),
        c[1].saturating_add(by),
        c[2].saturating_add(by),
        c[3],
    ]
}

fn darken(c: [u8; 4], by: u8) -> [u8; 4] {
    [
        c[0].saturating_sub(by),
        c[1].saturating_sub(by),
        c[2].saturating_sub(by),
        c[3],
    ]
}

/// An image from rows of characters, each mapped to a colour ('.' is clear).
fn pattern(rows: &[&str], palette: &[(char, [u8; 4])]) -> Rgba {
    let h = rows.len() as u32;
    let w = rows.iter().map(|r| r.len()).max().unwrap_or(0) as u32;
    let mut image = Rgba::new(w, h);
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if let Some((_, c)) = palette.iter().find(|(k, _)| *k == ch) {
                image.set(x as u32, y as u32, *c);
            }
        }
    }
    image
}

/// A recessed slot (dark top-left, light bottom-right), `size` square.
fn slot(image: &mut Rgba, x: u32, y: u32, size: u32) {
    image.fill(x, y, size, size, SHADE);
    image.fill(x + 1, y + 1, size - 1, size - 1, WHITE);
    image.fill(x + 1, y + 1, size - 2, size - 2, SLOT_FILL);
}

fn framed(w: u32, h: u32, fill: [u8; 4], border: [u8; 4]) -> Rgba {
    let mut image = Rgba::filled(w, h, border);
    image.fill(1, 1, w - 2, h - 2, fill);
    image
}

fn put(bundle: &mut Bundle, name: &str, image: &Rgba) -> Result<()> {
    bundle.png(
        format!("{SPRITES}/{name}.png"),
        image,
        Origin::Generator(GENERATOR),
    )
}

fn nine_slice(
    bundle: &mut Bundle,
    name: &str,
    w: u32,
    h: u32,
    border: serde_json::Value,
) -> Result<()> {
    bundle.json(
        format!("{SPRITES}/{name}.png.mcmeta"),
        &json!({"gui": {"scaling": {"type": "nine_slice", "width": w, "height": h, "border": border}}}),
        GENERATOR,
    )
}

fn texture(bundle: &mut Bundle, path: &str, image: &Rgba) -> Result<()> {
    bundle.png(
        format!("assets/minecraft/textures/{path}.png"),
        image,
        Origin::Generator(GENERATOR),
    )
}

pub fn write(bundle: &mut Bundle) -> Result<()> {
    status_bars(bundle)?;
    containers(bundle)?;
    recipe_book(bundle)?;
    menus(bundle)?;
    lists(bundle)?;
    screen_effects(bundle)?;
    title(bundle)?;
    Ok(())
}

// --- The status bars -------------------------------------------------------

const HEART: [&str; 9] = [
    ".ooo.ooo.",
    "ohhfofffo",
    "ohfffffso",
    "offffffso",
    ".offffso.",
    "..offso..",
    "...oso...",
    "....o....",
    ".........",
];

/// One heart: `fill` its colour, `half` keeps only the left half,
/// `container` draws the empty frame, `blink` the white flash of a change.
fn heart(fill: [u8; 4], half: bool, container: bool, blink: bool, hardcore: bool) -> Rgba {
    let outline = if container && blink {
        [0xf4, 0xf4, 0xf4, 255]
    } else {
        [0x24, 0x10, 0x14, 255]
    };
    let (f, h, s) = if container {
        let c = [0x3a, 0x22, 0x28, 230];
        (c, c, c)
    } else {
        let f = if blink { lighten(fill, 70) } else { fill };
        (f, lighten(f, 90), darken(f, 50))
    };
    let mut image = pattern(&HEART, &[('o', outline), ('f', f), ('h', h), ('s', s)]);
    if half {
        for y in 0..9 {
            for x in 4..9 {
                image.set(x, y, CLEAR);
            }
        }
    }
    if hardcore && !container {
        // Hardcore hearts wear a bright crest on each lobe.
        for x in [1u32, 2, 5, 6] {
            if !half || x < 4 {
                image.set(x, 0, [0xff, 0xf2, 0xc8, 255]);
            }
        }
    }
    image
}

const APPLE: [&str; 9] = [
    "....bgg..",
    "....bg...",
    ".ooobooo.",
    "ohhffffso",
    "ohfffffso",
    "offffffso",
    "offfffsso",
    ".offfsso.",
    "..oo.oo..",
];

/// The hunger icon: an apple (the edition's food). `fill` None is empty.
fn food(fill: Option<[u8; 4]>, half: bool) -> Rgba {
    let outline = [0x2a, 0x14, 0x10, 255];
    let stem = [0x5a, 0x3a, 0x1c, 255];
    let leaf = [0x4c, 0xa0, 0x3c, 255];
    let (f, h, s) = match fill {
        Some(c) => (c, lighten(c, 80), darken(c, 50)),
        None => {
            let c = [0x30, 0x22, 0x20, 220];
            (c, c, c)
        }
    };
    let mut image = pattern(
        &APPLE,
        &[
            ('o', outline),
            ('f', f),
            ('h', h),
            ('s', s),
            ('b', stem),
            ('g', leaf),
        ],
    );
    if half {
        // The right half is eaten: only the outline stays.
        for y in 2..9 {
            for x in 5..9 {
                let p = image.get(x, y);
                if p != outline && p[3] != 0 {
                    image.set(x, y, [0x30, 0x22, 0x20, 220]);
                }
            }
        }
    }
    image
}

const BUBBLE: [&str; 9] = [
    ".........",
    "...ooo...",
    "..ohhfo..",
    ".ohffffo.",
    ".offfffo.",
    ".offffso.",
    "..offso..",
    "...ooo...",
    ".........",
];

const SHIELD: [&str; 9] = [
    ".oo...oo.",
    "ohhoooffo",
    "ohfffffso",
    ".offfffo.",
    ".ofhffso.",
    ".offffso.",
    ".offfsso.",
    ".ooooooo.",
    ".........",
];

fn status_bars(bundle: &mut Bundle) -> Result<()> {
    // Hearts: every kind the shared HUD can draw.
    let kinds: [(&str, [u8; 4]); 5] = [
        ("", [0xd8, 0x2a, 0x3a, 255]),
        ("poisoned_", [0x8a, 0xa8, 0x24, 255]),
        ("withered_", [0x3a, 0x36, 0x3c, 255]),
        ("frozen_", [0x7c, 0xc8, 0xf0, 255]),
        ("absorbing_", [0xf0, 0xc0, 0x30, 255]),
    ];
    for (kind, color) in kinds {
        for hardcore in [false, true] {
            for half in [false, true] {
                for blink in [false, true] {
                    let name = format!(
                        "hud/heart/{kind}{}{}{}",
                        if hardcore { "hardcore_" } else { "" },
                        if half { "half" } else { "full" },
                        if blink { "_blinking" } else { "" }
                    );
                    put(bundle, &name, &heart(color, half, false, blink, hardcore))?;
                }
            }
        }
    }
    for (name, blink) in [
        ("container", false),
        ("container_blinking", true),
        ("container_hardcore", false),
        ("container_hardcore_blinking", true),
    ] {
        let mut image = heart([0; 4], false, true, blink, false);
        if name.contains("hardcore") {
            for x in [1u32, 2, 5, 6] {
                image.set(x, 0, [0xc8, 0xbc, 0x98, 255]);
            }
        }
        put(bundle, &format!("hud/heart/{name}"), &image)?;
    }
    // A mount's health: amber hearts.
    let amber = [0xe0, 0x84, 0x28, 255];
    put(
        bundle,
        "hud/heart/vehicle_container",
        &heart(amber, false, true, false, false),
    )?;
    put(
        bundle,
        "hud/heart/vehicle_full",
        &heart(amber, false, false, false, false),
    )?;
    put(
        bundle,
        "hud/heart/vehicle_half",
        &heart(amber, true, false, false, false),
    )?;

    // Food: apples; hunger turns them sickly green.
    let red = [0xc8, 0x30, 0x2c, 255];
    let sick = [0x84, 0x9c, 0x30, 255];
    for (suffix, color) in [("", red), ("_hunger", sick)] {
        put(
            bundle,
            &format!("hud/food_empty{suffix}"),
            &food(None, false),
        )?;
        put(
            bundle,
            &format!("hud/food_half{suffix}"),
            &food(Some(color), true),
        )?;
        put(
            bundle,
            &format!("hud/food_full{suffix}"),
            &food(Some(color), false),
        )?;
    }

    // Air: bubbles, one bursting, and the faint empty ring.
    let water = [0x5c, 0xa8, 0xf0, 255];
    let ring = [0x1c, 0x3c, 0x78, 255];
    let bubble = |o: [u8; 4], f: [u8; 4]| {
        pattern(
            &BUBBLE,
            &[('o', o), ('f', f), ('h', WHITE), ('s', darken(f, 40))],
        )
    };
    put(bundle, "hud/air", &bubble(ring, water))?;
    let mut bursting = bubble(ring, lighten(water, 50));
    for (x, y) in [(4, 1), (1, 4), (7, 4), (4, 7), (4, 4)] {
        bursting.set(x, y, CLEAR);
    }
    for (x, y) in [(0, 0), (8, 0), (0, 8), (8, 8)] {
        bursting.set(x, y, [0xc8, 0xe4, 0xff, 255]);
    }
    put(bundle, "hud/air_bursting", &bursting)?;
    let faint = [0x1c, 0x3c, 0x78, 120];
    put(bundle, "hud/air_empty", &bubble(faint, [0, 0, 0, 0]))?;

    // Armour: a small shield, silver when worn.
    let silver = [0xc8, 0xcc, 0xd8, 255];
    let edge = [0x2c, 0x2e, 0x38, 255];
    let shield = |f: [u8; 4], h: [u8; 4], s: [u8; 4]| {
        pattern(&SHIELD, &[('o', edge), ('f', f), ('h', h), ('s', s)])
    };
    let hollow = [0x3a, 0x3c, 0x48, 200];
    put(bundle, "hud/armor_empty", &shield(hollow, hollow, hollow))?;
    put(
        bundle,
        "hud/armor_full",
        &shield(silver, WHITE, darken(silver, 60)),
    )?;
    let mut half = shield(silver, WHITE, darken(silver, 60));
    for y in 0..9 {
        for x in 5..9 {
            if half.get(x, y) != edge && half.get(x, y)[3] != 0 {
                half.set(x, y, hollow);
            }
        }
    }
    put(bundle, "hud/armor_half", &half)?;

    // Experience: a dark track and a green fill, 182x5.
    let bar = |fill: [u8; 4], light: [u8; 4], border: [u8; 4]| {
        let mut image = Rgba::filled(182, 5, border);
        image.fill(1, 1, 180, 3, fill);
        image.fill(1, 1, 180, 1, light);
        image
    };
    put(
        bundle,
        "hud/experience_bar_background",
        &bar(
            [0x1c, 0x24, 0x1c, 230],
            [0x2c, 0x34, 0x2c, 230],
            [0x08, 0x0c, 0x08, 255],
        ),
    )?;
    put(
        bundle,
        "hud/experience_bar_progress",
        &bar(
            [0x6c, 0xe0, 0x3c, 255],
            [0xc4, 0xff, 0x9c, 255],
            [0x08, 0x0c, 0x08, 255],
        ),
    )?;
    // A mount's jump: the same track in blue.
    put(
        bundle,
        "hud/jump_bar_background",
        &bar(
            [0x1c, 0x20, 0x30, 230],
            [0x2c, 0x30, 0x40, 230],
            [0x08, 0x08, 0x10, 255],
        ),
    )?;
    put(
        bundle,
        "hud/jump_bar_cooldown",
        &bar(
            [0x40, 0x40, 0x48, 230],
            [0x58, 0x58, 0x60, 230],
            [0x08, 0x08, 0x10, 255],
        ),
    )?;
    put(
        bundle,
        "hud/jump_bar_progress",
        &bar(
            [0x48, 0x9c, 0xf0, 255],
            [0xb0, 0xd8, 0xff, 255],
            [0x08, 0x08, 0x10, 255],
        ),
    )?;
    // The locator bar (other players' directions).
    put(
        bundle,
        "hud/locator_bar_background",
        &bar(
            [0x18, 0x1a, 0x22, 200],
            [0x24, 0x26, 0x30, 200],
            [0x08, 0x08, 0x10, 220],
        ),
    )?;
    nine_slice(bundle, "hud/locator_bar_background", 182, 5, json!(2))?;
    for (i, radius) in [4.4f32, 3.6, 2.8, 2.0].into_iter().enumerate() {
        let mut dot = Rgba::new(9, 9);
        for y in 0..9 {
            for x in 0..9 {
                let (dx, dy) = (x as f32 - 4.0, y as f32 - 4.0);
                let d = (dx * dx + dy * dy).sqrt();
                if d <= radius {
                    dot.set(
                        x,
                        y,
                        if d > radius - 1.0 {
                            [0x40, 0x40, 0x40, 255]
                        } else {
                            WHITE
                        },
                    );
                }
            }
        }
        put(bundle, &format!("hud/locator_bar_dot/default_{i}"), &dot)?;
    }
    let arrow_up = pattern(
        &["...o...", "..ooo..", ".ooooo.", "ooooooo", "......."],
        &[('o', WHITE)],
    );
    let arrow_down = pattern(
        &["ooooooo", ".ooooo.", "..ooo..", "...o...", "......."],
        &[('o', WHITE)],
    );
    put(bundle, "hud/locator_bar_arrow_up", &arrow_up)?;
    put(bundle, "hud/locator_bar_arrow_down", &arrow_down)?;

    // The attack cooldown, under the crosshair and beside the hotbar.
    let mut background = Rgba::filled(16, 4, [0x10, 0x10, 0x14, 160]);
    background.fill(1, 1, 14, 2, [0x40, 0x42, 0x4c, 200]);
    put(
        bundle,
        "hud/crosshair_attack_indicator_background",
        &background,
    )?;
    let mut progress = Rgba::new(16, 4);
    progress.fill(1, 1, 14, 2, [0xf4, 0xf4, 0xf8, 255]);
    put(bundle, "hud/crosshair_attack_indicator_progress", &progress)?;
    let full = pattern(
        &[
            "................",
            "................",
            "................",
            "................",
            "................",
            "................",
            "................",
            "................",
            "................",
            "................",
            ".......oo.......",
            "......oooo......",
            ".....oooooo.....",
            "................",
            "................",
            "................",
        ],
        &[('o', [0xf4, 0xf4, 0xf8, 255])],
    );
    put(bundle, "hud/crosshair_attack_indicator_full", &full)?;
    let mut hotbar_back = framed(18, 18, [0x20, 0x22, 0x2c, 170], [0x0d, 0x0e, 0x13, 200]);
    hotbar_back.set(0, 0, CLEAR);
    hotbar_back.set(17, 0, CLEAR);
    hotbar_back.set(0, 17, CLEAR);
    hotbar_back.set(17, 17, CLEAR);
    put(
        bundle,
        "hud/hotbar_attack_indicator_background",
        &hotbar_back,
    )?;
    let mut hotbar_progress = Rgba::new(18, 18);
    hotbar_progress.fill(2, 2, 14, 14, [0x9a, 0x86, 0xff, 220]);
    put(
        bundle,
        "hud/hotbar_attack_indicator_progress",
        &hotbar_progress,
    )?;
    // Status effects (none come from this edition's content, but commands
    // and servers can give them).
    for (name, border) in [
        ("effect_background", ACCENT),
        ("effect_background_ambient", [0x4c, 0x7c, 0xa0, 255]),
    ] {
        put(
            bundle,
            &format!("hud/{name}"),
            &framed(24, 24, [0x18, 0x1a, 0x22, 220], border),
        )?;
        put(
            bundle,
            &format!("container/inventory/{name}"),
            &framed(120, 32, [0x18, 0x1a, 0x22, 230], border),
        )?;
        nine_slice(
            bundle,
            &format!("container/inventory/{name}"),
            120,
            32,
            json!(4),
        )?;
    }
    Ok(())
}

// --- Inventory and crafting -------------------------------------------------

/// A 256x256 container texture whose 176x166 panel is a light plate with
/// the player's inventory slots in their usual places.
fn container_panel() -> Rgba {
    let mut image = Rgba::new(256, 256);
    image.fill(0, 0, 176, 166, PANEL_EDGE);
    image.fill(1, 1, 174, 164, PANEL);
    image.fill(2, 163, 172, 1, PANEL_SOFT);
    for row in 0..3 {
        for column in 0..9 {
            slot(&mut image, 7 + column * 18, 83 + row * 18, 18);
        }
    }
    for column in 0..9 {
        slot(&mut image, 7 + column * 18, 141, 18);
    }
    image
}

/// A right-pointing arrow between a grid and its result.
fn arrow(image: &mut Rgba, x: u32, y: u32) {
    image.fill(x, y + 6, 16, 4, SHADE);
    for i in 0..8u32 {
        image.fill(x + 15 + i, y + i, 1, 16 - 2 * i, SHADE);
    }
}

fn containers(bundle: &mut Bundle) -> Result<()> {
    // The survival inventory: armour, the player, off hand, 2x2 grid, result.
    let mut inventory = container_panel();
    for i in 0..4 {
        slot(&mut inventory, 7, 7 + i * 18, 18);
    }
    inventory.fill(25, 7, 51, 72, SHADE);
    inventory.fill(26, 8, 49, 70, [0x22, 0x24, 0x2e, 255]);
    // A soft horizon behind the player.
    inventory.fill(26, 58, 49, 20, [0x2c, 0x3a, 0x34, 255]);
    slot(&mut inventory, 76, 61, 18);
    for r in 0..2 {
        for c in 0..2 {
            slot(&mut inventory, 97 + c * 18, 17 + r * 18, 18);
        }
    }
    arrow(&mut inventory, 135, 29);
    slot(&mut inventory, 153, 27, 18);
    texture(bundle, "gui/container/inventory", &inventory)?;

    // The crafting table: 3x3 grid and a large result slot.
    let mut table = container_panel();
    for r in 0..3 {
        for c in 0..3 {
            slot(&mut table, 29 + c * 18, 16 + r * 18, 18);
        }
    }
    arrow(&mut table, 90, 35);
    slot(&mut table, 119, 30, 26);
    texture(bundle, "gui/container/crafting_table", &table)?;

    // Empty equipment slots: faint outlines of what goes there.
    let ghost = [0x8a, 0x8e, 0x9e, 200];
    let icons: [(&str, [&str; 16]); 5] = [
        (
            "helmet",
            [
                "................",
                "................",
                "................",
                "....oooooooo....",
                "...o........o...",
                "..o..........o..",
                "..o..........o..",
                "..o...oooo...o..",
                "..o..o....o..o..",
                "..oooo....oooo..",
                "................",
                "................",
                "................",
                "................",
                "................",
                "................",
            ],
        ),
        (
            "chestplate",
            [
                "................",
                "...ooo....ooo...",
                "..o...oooo...o..",
                ".o............o.",
                ".o............o.",
                ".oooo......oooo.",
                "....o......o....",
                "....o......o....",
                "....o......o....",
                "....o......o....",
                "....o......o....",
                "....o......o....",
                "....oooooooo....",
                "................",
                "................",
                "................",
            ],
        ),
        (
            "leggings",
            [
                "................",
                "................",
                "....oooooooo....",
                "....o......o....",
                "....o......o....",
                "....o..oo..o....",
                "....o..oo..o....",
                "....o..oo..o....",
                "....o..oo..o....",
                "....o..oo..o....",
                "....o..oo..o....",
                "....o..oo..o....",
                "....oooooooo....",
                "................",
                "................",
                "................",
            ],
        ),
        (
            "boots",
            [
                "................",
                "................",
                "................",
                "................",
                "................",
                "...ooo....ooo...",
                "...o.o....o.o...",
                "...o.o....o.o...",
                "...o.o....o.o...",
                "...o.oo..oo.o...",
                "..o...o..o...o..",
                "..ooooo..ooooo..",
                "................",
                "................",
                "................",
                "................",
            ],
        ),
        (
            "shield",
            [
                "................",
                "................",
                "...oooooooooo...",
                "...o........o...",
                "...o........o...",
                "...o........o...",
                "...o........o...",
                "...o........o...",
                "....o......o....",
                "....o......o....",
                ".....o....o.....",
                "......o..o......",
                ".......oo.......",
                "................",
                "................",
                "................",
            ],
        ),
    ];
    for (name, rows) in icons {
        put(
            bundle,
            &format!("container/slot/{name}"),
            &pattern(&rows, &[('o', ghost)]),
        )?;
    }

    // The game-mode switcher (F3+F4): a dark plate and its cells.
    let mut switcher = Rgba::new(128, 128);
    switcher.fill(0, 0, 125, 75, RULE);
    switcher.fill(1, 1, 123, 73, [0x18, 0x1a, 0x22, 235]);
    texture(bundle, "gui/container/gamemode_switcher", &switcher)?;
    put(
        bundle,
        "gamemode_switcher/slot",
        &framed(26, 26, PLATE, RULE),
    )?;
    let mut selection = Rgba::new(26, 26);
    for (x, y, w, h) in [(0, 0, 26, 2), (0, 24, 26, 2), (0, 0, 2, 26), (24, 0, 2, 26)] {
        selection.fill(x, y, w, h, ACCENT);
    }
    put(bundle, "gamemode_switcher/selection", &selection)?;
    Ok(())
}

// --- The recipe book ----------------------------------------------------------

fn recipe_book(bundle: &mut Bundle) -> Result<()> {
    let mut panel = Rgba::new(256, 256);
    panel.fill(1, 1, 147, 166, PANEL_EDGE);
    panel.fill(2, 2, 145, 164, PANEL);
    // The grid of results.
    panel.fill(10, 30, 129, 105, PANEL_SOFT);
    texture(bundle, "gui/recipe_book", &panel)?;

    // The book button: a closed book with a bookmark.
    let book = |cover: [u8; 4]| {
        let mut b = framed(20, 18, PANEL, PANEL_EDGE);
        let art = pattern(
            &[
                "..........",
                ".cccccccp.",
                ".cwwwwwcp.",
                ".cccccccp.",
                ".cccccccp.",
                ".cccrcccp.",
                ".cccrcccp.",
                ".cccccccp.",
                ".cccccccp.",
                ".cccccccp.",
                "..........",
            ],
            &[
                ('c', cover),
                ('w', WHITE),
                ('p', [0xf0, 0xe8, 0xd0, 255]),
                ('r', [0xd8, 0x2a, 0x3a, 255]),
            ],
        );
        b.blit(&art, 5, 3);
        b
    };
    put(bundle, "recipe_book/button", &book([0x4c, 0x6c, 0xb0, 255]))?;
    put(
        bundle,
        "recipe_book/button_highlighted",
        &book([0x9a, 0x86, 0xff, 255]),
    )?;
    // Tabs on the book's left edge.
    let tab = |selected: bool| {
        let mut t = Rgba::new(35, 27);
        let fill = if selected { PANEL } else { PANEL_SOFT };
        let x0 = if selected { 0 } else { 4 };
        t.fill(x0, 0, 35 - x0, 27, PANEL_EDGE);
        t.fill(x0 + 1, 1, 34 - x0, 25, fill);
        t
    };
    put(bundle, "recipe_book/tab", &tab(false))?;
    put(bundle, "recipe_book/tab_selected", &tab(true))?;
    // The "craftable only" filter: a toggle showing a small grid.
    for prefix in ["", "furnace_"] {
        for (state, on) in [("enabled", true), ("disabled", false)] {
            for highlighted in [false, true] {
                let border = if highlighted { ACCENT } else { PANEL_EDGE };
                let mut f = framed(
                    26,
                    16,
                    if on {
                        [0xd4, 0xf0, 0xd0, 255]
                    } else {
                        PANEL_SOFT
                    },
                    border,
                );
                let mark = if on { [0x3c, 0x8c, 0x3c, 255] } else { SHADE };
                if prefix.is_empty() {
                    for r in 0..3 {
                        for c in 0..3 {
                            f.fill(8 + c * 3, 4 + r * 3, 2, 2, mark);
                        }
                    }
                } else {
                    // A flame for the furnace books.
                    f.blit(
                        &pattern(
                            &["..o..", ".ooo.", "ooooo", "ooooo", ".ooo."],
                            &[('o', mark)],
                        ),
                        10,
                        5,
                    );
                }
                put(
                    bundle,
                    &format!(
                        "recipe_book/{prefix}filter_{state}{}",
                        if highlighted { "_highlighted" } else { "" }
                    ),
                    &f,
                )?;
            }
        }
    }
    for (name, fill, border) in [
        (
            "slot_craftable",
            [0xe4, 0xec, 0xff, 255],
            [0x6c, 0x7c, 0xb0, 255],
        ),
        (
            "slot_uncraftable",
            [0xf4, 0xd8, 0xd8, 255],
            [0xb0, 0x5c, 0x5c, 255],
        ),
        (
            "slot_many_craftable",
            [0xe4, 0xec, 0xff, 255],
            [0x6c, 0x7c, 0xb0, 255],
        ),
        (
            "slot_many_uncraftable",
            [0xf4, 0xd8, 0xd8, 255],
            [0xb0, 0x5c, 0x5c, 255],
        ),
    ] {
        let mut s = framed(25, 25, fill, border);
        if name.contains("many") {
            // A second card behind the first.
            s.fill(21, 2, 2, 21, border);
            s.fill(2, 21, 21, 2, border);
        }
        put(bundle, &format!("recipe_book/{name}"), &s)?;
    }
    for (name, forward) in [("page_forward", true), ("page_backward", false)] {
        for highlighted in [false, true] {
            let color = if highlighted { ACCENT } else { SHADE };
            let mut a = Rgba::new(12, 17);
            for i in 0..8u32 {
                let x = if forward { 2 + i } else { 9 - i };
                a.fill(x, i, 1, 17 - 2 * i, color);
            }
            put(
                bundle,
                &format!(
                    "recipe_book/{name}{}",
                    if highlighted { "_highlighted" } else { "" }
                ),
                &a,
            )?;
        }
    }
    put(
        bundle,
        "recipe_book/overlay_recipe",
        &framed(32, 32, [0x18, 0x1a, 0x22, 240], RULE),
    )?;
    nine_slice(bundle, "recipe_book/overlay_recipe", 32, 32, json!(4))?;
    for prefix in ["crafting", "furnace"] {
        for disabled in [false, true] {
            for highlighted in [false, true] {
                let fill = if disabled {
                    [0x40, 0x30, 0x34, 255]
                } else {
                    PLATE
                };
                let border = if highlighted { ACCENT } else { RULE };
                put(
                    bundle,
                    &format!(
                        "recipe_book/{prefix}_overlay{}{}",
                        if disabled { "_disabled" } else { "" },
                        if highlighted { "_highlighted" } else { "" }
                    ),
                    &framed(24, 24, fill, border),
                )?;
            }
        }
    }
    // Toasts: a card with an accent edge, in the colours their text needs
    // (dark on the recipe toast, light on the advancement toast).
    for (name, fill, border) in [
        ("recipe", PANEL, PANEL_EDGE),
        ("advancement", [0x18, 0x1a, 0x22, 240], RULE),
    ] {
        let mut t = framed(160, 32, fill, border);
        t.fill(0, 0, 3, 32, ACCENT);
        put(bundle, &format!("toast/{name}"), &t)?;
    }
    let mut system = framed(160, 64, [0x18, 0x1a, 0x22, 240], RULE);
    system.fill(0, 0, 3, 64, ACCENT);
    put(bundle, "toast/system", &system)?;
    nine_slice(
        bundle,
        "toast/system",
        160,
        64,
        json!({"left": 18, "top": 4, "right": 4, "bottom": 4}),
    )?;
    Ok(())
}

// --- Menus --------------------------------------------------------------------

/// A 15x15 icon from a pattern, drawn in the interface's ink colour.
fn icon(rows: &[&str]) -> Rgba {
    pattern(rows, &[('o', INK), ('a', ACCENT)])
}

fn menus(bundle: &mut Bundle) -> Result<()> {
    let backdrop = Rgba::filled(16, 16, [0x0d, 0x0e, 0x13, 150]);
    texture(bundle, "gui/inworld_menu_background", &backdrop)?;
    texture(
        bundle,
        "gui/menu_list_background",
        &Rgba::filled(16, 16, [0x0d, 0x0e, 0x13, 120]),
    )?;
    texture(
        bundle,
        "gui/inworld_menu_list_background",
        &Rgba::filled(16, 16, [0x0d, 0x0e, 0x13, 90]),
    )?;
    for name in ["inworld_header_separator", "inworld_footer_separator"] {
        let mut rule = Rgba::new(32, 2);
        rule.fill(0, 0, 32, 1, RULE);
        texture(bundle, &format!("gui/{name}"), &rule)?;
    }
    let mut tab_header = Rgba::filled(16, 16, [0x0d, 0x0e, 0x13, 200]);
    tab_header.fill(0, 15, 16, 1, RULE);
    texture(bundle, "gui/tab_header_background", &tab_header)?;
    // Create World's tabs.
    for (name, fill, border) in [
        ("widget/tab", PLATE, RULE),
        ("widget/tab_highlighted", [0x2b, 0x2a, 0x40, 255], ACCENT),
        ("widget/tab_selected", [0x2b, 0x2a, 0x40, 255], RULE),
        (
            "widget/tab_selected_highlighted",
            [0x34, 0x30, 0x50, 255],
            ACCENT,
        ),
    ] {
        let mut t = Rgba::filled(130, 24, border);
        t.fill(1, 1, 128, 23, fill);
        if name.contains("selected") {
            t.fill(2, 21, 126, 2, ACCENT);
        }
        put(bundle, name, &t)?;
        nine_slice(
            bundle,
            name,
            130,
            24,
            json!({"left": 2, "top": 2, "right": 2, "bottom": 0}),
        )?;
    }
    // World Options' difficulty lock.
    let lock = |open: bool| {
        icon(&[
            "...............",
            "...............",
            "......ooo......",
            ".....o...o.....",
            ".....o...o.....",
            if open {
                ".........o....."
            } else {
                ".....o...o....."
            },
            "....ooooooo....",
            "....ooooooo....",
            "....ooo.ooo....",
            "....ooo.ooo....",
            "....ooooooo....",
            "...............",
            "...............",
            "...............",
            "...............",
        ])
    };
    for (name, open) in [("locked", false), ("unlocked", true)] {
        for (suffix, fill, border) in [
            ("", PLATE, RULE),
            ("_highlighted", [0x2b, 0x2a, 0x40, 255], ACCENT),
            ("_disabled", [0x18, 0x1a, 0x22, 255], PLATE),
        ] {
            let mut b = framed(20, 20, fill, border);
            b.blit(&lock(open), 3, 3);
            put(bundle, &format!("widget/{name}_button{suffix}"), &b)?;
        }
    }
    // Title and pause menu icons, 15x15.
    put(
        bundle,
        "icon/accessibility",
        &icon(&[
            "......ooo......",
            "......ooo......",
            "...............",
            ".ooooooooooooo.",
            "......ooo......",
            "......ooo......",
            "......ooo......",
            "......ooo......",
            ".....oo.oo.....",
            ".....oo.oo.....",
            "....oo...oo....",
            "....oo...oo....",
            "...............",
            "...............",
            "...............",
        ]),
    )?;
    put(
        bundle,
        "icon/language",
        &icon(&[
            "....ooooooo....",
            "..oo..o.o..oo..",
            ".o...o...o...o.",
            ".o...o...o...o.",
            "ooooooooooooooo",
            "o....o...o....o",
            "o....o...o....o",
            "o....o...o....o",
            "ooooooooooooooo",
            ".o...o...o...o.",
            ".o...o...o...o.",
            "..oo..o.o..oo..",
            "....ooooooo....",
            "...............",
            "...............",
        ]),
    )?;
    put(
        bundle,
        "friends/friends",
        &icon(&[
            "................",
            "....oo....aa....",
            "...oooo..aaaa...",
            "...oooo..aaaa...",
            "....oo....aa....",
            "................",
            "..oooooo.aaaaa..",
            ".oooooooo.aaaaa.",
            ".oooooooo.aaaaa.",
            ".oooooooo.aaaaa.",
            "................",
            "................",
            "................",
            "................",
            "................",
            "................",
        ]),
    )?;
    put(
        bundle,
        "pause_menu/bug",
        &icon(&[
            "..o.......o..",
            "...o.....o...",
            "....ooooo....",
            "o..ooooooo..o",
            ".o.ooo.ooo.o.",
            "...ooo.ooo...",
            "oo.ooo.ooo.oo",
            "...ooo.ooo...",
            ".o.ooo.ooo.o.",
            "o...ooooo...o",
            ".....ooo.....",
            ".............",
            ".............",
        ]),
    )?;
    put(
        bundle,
        "pause_menu/social_interactions",
        &icon(&[
            "....................",
            "....................",
            "...oooooooooo.......",
            "...o........o.......",
            "...o........o.......",
            "...o........o.......",
            "...oooo.ooooo.......",
            "......o.o.aaaaaaaaa.",
            "......oo..a.......a.",
            "..........a.......a.",
            "..........a.......a.",
            "..........aaaaa.aaa.",
            "..............a.a...",
            "...............aa...",
            "....................",
            "....................",
            "....................",
            "....................",
            "....................",
            "....................",
        ]),
    )?;
    put(
        bundle,
        "pause_menu/player_reporting",
        &icon(&[
            "...............",
            "..o............",
            "..ooooooooooo..",
            "..o.........o..",
            "..o.........o..",
            "..o.........o..",
            "..ooooooooooo..",
            "..o............",
            "..o............",
            "..o............",
            "..o............",
            "..o............",
            "...............",
            "...............",
        ]),
    )?;
    Ok(())
}

// --- World list, server list, player list, statistics ----------------------

/// Signal bars, 10x8: `bars` lit out of five.
fn ping(bars: u32, lit: [u8; 4]) -> Rgba {
    let mut image = Rgba::new(10, 8);
    for i in 0..5u32 {
        let h = 2 + i + i / 2;
        let color = if i < bars {
            lit
        } else {
            [0x40, 0x42, 0x4c, 255]
        };
        image.fill(i * 2, 8 - h.min(8), 1, h.min(8), color);
    }
    image
}

fn lists(bundle: &mut Bundle) -> Result<()> {
    let green = [0x5c, 0xd0, 0x5c, 255];
    for bars in 1..=5u32 {
        let color = match bars {
            1 => [0xe0, 0x50, 0x40, 255],
            2 => [0xe0, 0xa0, 0x40, 255],
            _ => green,
        };
        put(
            bundle,
            &format!("server_list/ping_{bars}"),
            &ping(bars, color),
        )?;
        put(bundle, &format!("icon/ping_{bars}"), &ping(bars, color))?;
        // Pinging: one bar sweeps across.
        let mut sweep = ping(0, green);
        let i = bars - 1;
        let h = 2 + i + i / 2;
        sweep.fill(i * 2, 8 - h.min(8), 1, h.min(8), [0xb0, 0xb4, 0xc4, 255]);
        put(bundle, &format!("server_list/pinging_{bars}"), &sweep)?;
    }
    let mut unknown = ping(0, green);
    unknown.blit(
        &pattern(
            &[".oo.", "o..o", "..o.", ".o..", "....", ".o.."],
            &[('o', INK)],
        ),
        5,
        1,
    );
    put(bundle, "icon/ping_unknown", &unknown)?;
    let mut unreachable = ping(0, green);
    for i in 0..6u32 {
        unreachable.set(2 + i, 1 + i, [0xe0, 0x50, 0x40, 255]);
        unreachable.set(7 - i, 1 + i, [0xe0, 0x50, 0x40, 255]);
    }
    put(bundle, "server_list/unreachable", &unreachable)?;
    // Hover buttons over a list entry's icon, 32x32.
    let overlay = |glyph: &[&str], highlighted: bool| {
        let mut b = Rgba::filled(
            32,
            32,
            [0x0d, 0x0e, 0x13, if highlighted { 200 } else { 140 }],
        );
        let color = if highlighted { ACCENT } else { INK };
        b.blit(&pattern(glyph, &[('o', color)]), 8, 8);
        b
    };
    let play = [
        "o...............",
        "ooo.............",
        "ooooo...........",
        "ooooooo.........",
        "ooooooooo.......",
        "ooooooooooo.....",
        "ooooooooooooo...",
        "ooooooooooooooo.",
        "ooooooooooooooo.",
        "ooooooooooooo...",
        "ooooooooooo.....",
        "ooooooooo.......",
        "ooooooo.........",
        "ooooo...........",
        "ooo.............",
        "o...............",
    ];
    let up = [
        ".......oo.......",
        "......oooo......",
        ".....oooooo.....",
        "....oooooooo....",
        "...oooooooooo...",
        "..oooooooooooo..",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
    ];
    let mut down = up;
    down.reverse();
    for highlighted in [false, true] {
        let suffix = if highlighted { "_highlighted" } else { "" };
        put(
            bundle,
            &format!("world_list/join{suffix}"),
            &overlay(&play, highlighted),
        )?;
        put(
            bundle,
            &format!("server_list/join{suffix}"),
            &overlay(&play, highlighted),
        )?;
        put(
            bundle,
            &format!("server_list/move_up{suffix}"),
            &overlay(&up, highlighted),
        )?;
        put(
            bundle,
            &format!("server_list/move_down{suffix}"),
            &overlay(&down, highlighted),
        )?;
    }
    // A world or server with no picture of its own: a little landscape.
    let mut world = Rgba::new(128, 128);
    for y in 0..128u32 {
        for x in 0..128u32 {
            let hill =
                70 + ((x as f32 / 18.0).sin() * 8.0) as i32 + (noise(x / 8, 0, 5) % 6) as i32;
            let c = if (y as i32) < hill {
                let t = y as f32 / 128.0;
                [(110.0 + 60.0 * t) as u8, (160.0 + 50.0 * t) as u8, 255, 255]
            } else if (y as i32) < hill + 6 {
                [0x5c, 0xa0, 0x3c, 255]
            } else {
                let n = (noise(x / 4, y / 4, 9) % 20) as u8;
                [0x7c - n, 0x5c - n / 2, 0x3c, 255]
            };
            world.set(x, y, c);
        }
    }
    texture(bundle, "misc/unknown_server", &world)?;
    // Statistics column headers, 18x18.
    let headers: [(&str, [&str; 12]); 6] = [
        (
            "block_mined",
            [
                "....oooo....",
                "...o....o...",
                "..o......o..",
                "..oooooooo..",
                ".....oo.....",
                ".....oo.....",
                ".....oo.....",
                ".....oo.....",
                ".....oo.....",
                ".....oo.....",
                "............",
                "............",
            ],
        ),
        (
            "item_broken",
            [
                "..........oo",
                ".........oo.",
                "........oo..",
                ".......oo...",
                "............",
                ".....oo.....",
                "....oo......",
                "...oo.......",
                "..oo........",
                ".oo.........",
                "............",
                "............",
            ],
        ),
        (
            "item_crafted",
            [
                "oooo.oooo...",
                "o..o.o..o...",
                "oooo.oooo...",
                "............",
                "oooo.oooo...",
                "o..o.o..o.o.",
                "oooo.oooo.oo",
                "..........o.",
                "............",
                "............",
                "............",
                "............",
            ],
        ),
        (
            "item_used",
            [
                ".....oo.....",
                "....o..o....",
                "....o..o....",
                ".....oo.....",
                "....oooo....",
                "...oooooo...",
                "...oooooo...",
                "....oooo....",
                "............",
                "............",
                "............",
                "............",
            ],
        ),
        (
            "item_picked_up",
            [
                ".....oo.....",
                "....oooo....",
                "...oooooo...",
                ".....oo.....",
                ".....oo.....",
                ".....oo.....",
                "............",
                "..oooooooo..",
                "..oooooooo..",
                "............",
                "............",
                "............",
            ],
        ),
        (
            "item_dropped",
            [
                ".....oo.....",
                ".....oo.....",
                ".....oo.....",
                "...oooooo...",
                "....oooo....",
                ".....oo.....",
                "............",
                "..oooooooo..",
                "..oooooooo..",
                "............",
                "............",
                "............",
            ],
        ),
    ];
    for (name, rows) in headers {
        let mut h = Rgba::new(18, 18);
        h.blit(&pattern(&rows, &[('o', [0x30, 0x32, 0x3c, 255])]), 3, 3);
        put(bundle, &format!("statistics/{name}"), &h)?;
    }
    let mut header = Rgba::new(18, 18);
    slot(&mut header, 0, 0, 18);
    header.fill(1, 1, 16, 16, [0xe4, 0xe0, 0xff, 255]);
    put(bundle, "statistics/header", &header)?;
    let sort = |down: bool| {
        let mut s = Rgba::new(18, 18);
        for i in 0..4u32 {
            let y = if down { 12 + i } else { 15 - i };
            s.fill(9 - (3 - i), y, 2 * (3 - i) + 1, 1, ACCENT);
        }
        s
    };
    put(bundle, "statistics/sort_down", &sort(true))?;
    put(bundle, "statistics/sort_up", &sort(false))?;
    Ok(())
}

// --- Screen effects -------------------------------------------------------------

fn screen_effects(bundle: &mut Bundle) -> Result<()> {
    // The vignette darkens by its colour: black (nothing) at the centre to
    // a soft grey at the corners.
    let mut vignette = Rgba::new(256, 256);
    for y in 0..256u32 {
        for x in 0..256u32 {
            let (dx, dy) = ((x as f32 - 127.5) / 128.0, (y as f32 - 127.5) / 128.0);
            let d = ((dx * dx + dy * dy).sqrt() - 0.45).max(0.0) / 0.97;
            let v = (d.min(1.0) * d.min(1.0) * 150.0) as u8;
            vignette.set(x, y, [v, v, v, 255]);
        }
    }
    texture(bundle, "misc/vignette", &vignette)?;
    // Under water: a faint blue ripple.
    let mut underwater = Rgba::new(64, 64);
    for y in 0..64u32 {
        for x in 0..64u32 {
            let n = (noise(x / 4, y / 4, 41) % 40) as u8;
            underwater.set(x, y, [20 + n / 2, 60 + n, 140 + n, 255]);
        }
    }
    texture(bundle, "misc/underwater", &underwater)?;
    // Entity shadows: a soft dark disc.
    let mut shadow = Rgba::new(32, 32);
    for y in 0..32u32 {
        for x in 0..32u32 {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            let d = (dx * dx + dy * dy).sqrt() / 16.0;
            if d < 1.0 {
                shadow.set(x, y, [0, 0, 0, ((1.0 - d) * 180.0) as u8]);
            }
        }
    }
    texture(bundle, "misc/shadow", &shadow)?;
    Ok(())
}

// --- The title ------------------------------------------------------------------

/// 5x7 capitals for the wordmark (only the letters it spells).
fn glyph(c: char) -> [&'static str; 7] {
    match c {
        'A' => [
            ".ooo.", "o...o", "o...o", "ooooo", "o...o", "o...o", "o...o",
        ],
        'C' => [
            ".oooo", "o....", "o....", "o....", "o....", "o....", ".oooo",
        ],
        'E' => [
            "ooooo", "o....", "o....", "oooo.", "o....", "o....", "ooooo",
        ],
        'H' => [
            "o...o", "o...o", "o...o", "ooooo", "o...o", "o...o", "o...o",
        ],
        'I' => [
            "ooooo", "..o..", "..o..", "..o..", "..o..", "..o..", "ooooo",
        ],
        'N' => [
            "o...o", "oo..o", "o.o.o", "o.o.o", "o..oo", "o...o", "o...o",
        ],
        'O' => [
            ".ooo.", "o...o", "o...o", "o...o", "o...o", "o...o", ".ooo.",
        ],
        'P' => [
            "oooo.", "o...o", "o...o", "oooo.", "o....", "o....", "o....",
        ],
        'R' => [
            "oooo.", "o...o", "o...o", "oooo.", "o.o..", "o..o.", "o...o",
        ],
        'T' => [
            "ooooo", "..o..", "..o..", "..o..", "..o..", "..o..", "..o..",
        ],
        'V' => [
            "o...o", "o...o", "o...o", "o...o", ".o.o.", ".o.o.", "..o..",
        ],
        'Z' => [
            "ooooo", "....o", "...o.", "..o..", ".o...", "o....", "ooooo",
        ],
        _ => ["....."; 7],
    }
}

/// `text` in the 5x7 capitals at `scale`, centred in `image` at row `top`,
/// with a darker shadow one cell down and right.
fn wordmark(
    image: &mut Rgba,
    text: &str,
    scale: u32,
    top: u32,
    face: [u8; 4],
    low: [u8; 4],
    shadow: [u8; 4],
) {
    let advance = 6 * scale;
    let width = text.chars().count() as u32 * advance - scale;
    let left = (image.width - width) / 2;
    for (pass, offset) in [(0, scale.max(2) / 2), (1, 0)] {
        for (i, c) in text.chars().enumerate() {
            for (row, line) in glyph(c).iter().enumerate() {
                for (column, ch) in line.chars().enumerate() {
                    if ch != 'o' {
                        continue;
                    }
                    let x = left + i as u32 * advance + column as u32 * scale + offset;
                    let y = top + row as u32 * scale + offset;
                    let color = if pass == 0 {
                        shadow
                    } else if row >= 4 {
                        low
                    } else {
                        face
                    };
                    image.fill(x, y, scale, scale, color);
                }
            }
        }
    }
}

fn title(bundle: &mut Bundle) -> Result<()> {
    // The wordmark: 256x64, of which the top 44 rows are drawn.
    let mut logo = Rgba::new(256, 64);
    wordmark(
        &mut logo,
        "HORIZON",
        4,
        6,
        [0xf4, 0xf2, 0xff, 255],
        [0xc8, 0xbc, 0xff, 255],
        [0x24, 0x1c, 0x48, 230],
    );
    texture(bundle, "gui/title/minecraft", &logo)?;
    let mut edition = Rgba::new(128, 16);
    wordmark(
        &mut edition,
        "OPEN CREATIVE",
        1,
        3,
        [0xff, 0xe0, 0x8c, 255],
        [0xff, 0xc8, 0x5c, 255],
        [0x30, 0x20, 0x08, 220],
    );
    texture(bundle, "gui/title/edition", &edition)?;
    // The backdrop: six faces of a cube around a hilltop at dawn. Faces 0..3
    // circle the horizon, 4 is the sky above and 5 the ground below.
    const SIZE: u32 = 256;
    let sky = |t: f32| -> [u8; 4] {
        // t: 0 at the zenith, 1 at the horizon.
        let top = [70.0, 120.0, 210.0];
        let low = [250.0, 200.0, 170.0];
        let mix = |i: usize| (top[i] + (low[i] - top[i]) * t.powf(2.2)) as u8;
        [mix(0), mix(1), mix(2), 255]
    };
    for face in 0..6u32 {
        let mut image = Rgba::new(SIZE, SIZE);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let c = match face {
                    4 => sky(0.0),
                    5 => {
                        let n = (noise(x / 8, y / 8, 77) % 30) as u8;
                        [0x3c + n / 2, 0x7c + n, 0x34, 255]
                    }
                    _ => {
                        // Distant hills, then nearer ones, over the sky.
                        let u =
                            (face * SIZE + x) as f32 / (4.0 * SIZE as f32) * std::f32::consts::TAU;
                        let far = 150.0 + (u * 3.0).sin() * 10.0 + (u * 7.0 + 1.0).sin() * 5.0;
                        let near = 175.0 + (u * 5.0 + 2.0).sin() * 9.0 + (u * 11.0).sin() * 4.0;
                        let yf = y as f32;
                        if yf >= near {
                            let n = (noise(x / 3, y / 3, 13 + face) % 24) as u8;
                            [0x3c + n / 2, 0x84 + n, 0x3c, 255]
                        } else if yf >= far {
                            [0x6c, 0x94, 0xa4, 255]
                        } else {
                            let mut c = sky(yf / far);
                            // Two soft clouds per face, long flat ellipses.
                            for k in 0..2u32 {
                                let cx = (noise(face, k, 71) % 200 + 28) as f32;
                                let cy = (noise(face, k, 73) % 50 + 50) as f32;
                                let (dx, dy) = ((x as f32 - cx) / 34.0, (yf - cy) / 7.0);
                                let d = dx * dx + dy * dy;
                                if d < 1.0 {
                                    let a = ((1.0 - d) * 120.0) as u32;
                                    let mix =
                                        |v: u8| ((u32::from(v) * (255 - a) + 255 * a) / 255) as u8;
                                    c = [mix(c[0]), mix(c[1]), mix(c[2]), 255];
                                }
                            }
                            c
                        }
                    }
                };
                image.set(x, y, c);
            }
        }
        texture(
            bundle,
            &format!("gui/title/background/panorama_{face}"),
            &image,
        )?;
    }
    Ok(())
}

// --- Block faces ----------------------------------------------------------------

/// A wooden board texture: warm planks with grain.
fn boards(seed: u32) -> Rgba {
    let mut image = Rgba::new(16, 16);
    for y in 0..16u32 {
        for x in 0..16u32 {
            let grain = (noise(x / 3, y, seed) % 18) as u8;
            let seam = y % 4 == 3;
            let c = if seam {
                [0x6a, 0x48, 0x28, 255]
            } else {
                [0xa8 - grain, 0x80 - grain, 0x4c - grain / 2, 255]
            };
            image.set(x, y, c);
        }
    }
    image
}

/// The crafting table's faces (Minetest Game has no workbench block).
pub fn crafting_table(face: &str) -> Option<Rgba> {
    let rim = [0x5a, 0x3c, 0x20, 255];
    match face {
        "crafting_table_top" => {
            let mut image = boards(3);
            image.fill(0, 0, 16, 1, rim);
            image.fill(0, 15, 16, 1, rim);
            image.fill(0, 0, 1, 16, rim);
            image.fill(15, 0, 1, 16, rim);
            // A carved 3x3 grid.
            for i in [5u32, 10] {
                image.fill(i, 2, 1, 12, [0x4c, 0x32, 0x1a, 255]);
                image.fill(2, i, 12, 1, [0x4c, 0x32, 0x1a, 255]);
            }
            Some(image)
        }
        "crafting_table_side" | "crafting_table_front" => {
            let mut image = boards(if face == "crafting_table_side" { 5 } else { 7 });
            image.fill(0, 0, 16, 3, [0x7a, 0x52, 0x2c, 255]);
            image.fill(0, 3, 16, 1, rim);
            let iron = [0xa8, 0xac, 0xb8, 255];
            let handle = [0x6a, 0x48, 0x28, 255];
            if face == "crafting_table_side" {
                // A saw hanging on the side.
                image.fill(3, 6, 8, 3, iron);
                for x in (3..11).step_by(2) {
                    image.set(x, 9, iron);
                }
                image.fill(11, 6, 3, 3, handle);
            } else {
                // A hammer and a drawer.
                image.fill(3, 6, 4, 2, iron);
                image.fill(4, 8, 2, 5, handle);
                image.fill(9, 10, 5, 4, [0x5a, 0x3c, 0x20, 255]);
                image.fill(10, 11, 3, 2, [0x8a, 0x62, 0x38, 255]);
                image.set(11, 11, iron);
            }
            Some(image)
        }
        _ => None,
    }
}
