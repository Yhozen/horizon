//! Horizon's original artwork for the container screens the client draws:
//! chests, shulker boxes, dispensers, hoppers, the crafter, the three
//! furnaces, the anvil, beacon, brewing stand, enchanting table, grindstone,
//! smithing table, stonecutter, cartography table, loom, the trading screen,
//! a mount's inventory and the book (lectern), with their sprites. The
//! files sit at Minecraft's paths and keep Minecraft's layout (every slot,
//! bar and arrow where the screens' layout constants expect it), so a
//! Minecraft-layout pack can replace any of them. Every pixel is drawn here
//! (generator `art`), in the light panel style of the inventory.
use crate::{
    Bundle, Result,
    art_survival::{
        ACCENT, INK, PANEL, PANEL_EDGE, PANEL_SOFT, PLATE, RULE, SHADE, SLOT_FILL, WHITE, darken,
        framed, lighten, pattern, put, slot, texture,
    },
    image::Rgba,
};

const GREEN: [u8; 4] = [0x5c, 0xd0, 0x5c, 255];
const RED: [u8; 4] = [0xe0, 0x50, 0x40, 255];
const GHOST: [u8; 4] = [0x8a, 0x8e, 0x9e, 200];
const DIM: [u8; 4] = [0x2b, 0x2a, 0x40, 255];

/// A `tw`x`th` texture whose top-left `w`x`h` is a light panel.
fn panel(tw: u32, th: u32, w: u32, h: u32) -> Rgba {
    let mut image = Rgba::new(tw, th);
    image.fill(0, 0, w, h, PANEL_EDGE);
    image.fill(1, 1, w - 2, h - 2, PANEL);
    image.fill(2, h - 3, w - 4, 1, PANEL_SOFT);
    image
}

/// `columns` x `rows` slot frames from (x, y).
fn grid(image: &mut Rgba, x: u32, y: u32, columns: u32, rows: u32) {
    for r in 0..rows {
        for c in 0..columns {
            slot(image, x + c * 18, y + r * 18, 18);
        }
    }
}

/// The player's 27 slots at (x, y) and the hotbar 58 pixels below.
fn inventory(image: &mut Rgba, x: u32, y: u32) {
    grid(image, x, y, 9, 3);
    grid(image, x, y + 58, 9, 1);
}

/// A recessed area like a slot, of any size, filled with `fill`.
fn well(image: &mut Rgba, x: u32, y: u32, w: u32, h: u32, fill: [u8; 4]) {
    image.fill(x, y, w, h, SHADE);
    image.fill(x + 1, y + 1, w - 1, h - 1, WHITE);
    image.fill(x + 1, y + 1, w - 2, h - 2, fill);
}

/// The 23x16 right arrow (16 shaft, 8 head) at (x, y).
fn arrow(image: &mut Rgba, x: u32, y: u32, color: [u8; 4]) {
    image.fill(x, y + 6, 16, 4, color);
    for i in 0..8u32 {
        image.fill(x + 15 + i, y + i, 1, 16 - 2 * i, color);
    }
}

/// A plus sign, 9x9, thickness 3.
fn plus(image: &mut Rgba, x: u32, y: u32, color: [u8; 4]) {
    image.fill(x + 3, y, 3, 9, color);
    image.fill(x, y + 3, 9, 3, color);
}

/// A diagonal cross of two-pixel strokes inside (x, y, size, size).
fn cross(image: &mut Rgba, x: u32, y: u32, size: u32, color: [u8; 4]) {
    for i in 0..size - 1 {
        image.fill(x + i, y + i, 2, 2, color);
        image.fill(x + size - 2 - i, y + i, 2, 2, color);
    }
}

/// A flame (a drop pointing up) filling `w`x`h`, with a brighter core.
fn flame(w: u32, h: u32, outer: [u8; 4], core: [u8; 4]) -> Rgba {
    let mut image = Rgba::new(w, h);
    let cx = (w as f32 - 1.0) / 2.0;
    for y in 0..h {
        let t = y as f32 / (h as f32 - 1.0);
        let base = w as f32 / 2.0;
        let half = if t < 0.65 {
            0.5 + (base - 0.5) * (t / 0.65).powf(0.8)
        } else {
            base * (1.0 - ((t - 0.65) / 0.4).powi(2)).max(0.0).sqrt()
        };
        for x in 0..w {
            let d = (x as f32 - cx).abs();
            if d <= half {
                let inner = d <= half - 2.0 && t > 0.35;
                image.set(x, y, if inner { core } else { outer });
            }
        }
    }
    image
}

/// An image from text rows ('.' is clear), each mapped by the palette.
fn art(rows: &str, palette: &[(char, [u8; 4])]) -> Rgba {
    let lines: Vec<&str> = rows
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    pattern(&lines, palette)
}

/// `art`'s opaque pixels drawn over `image` at (x, y).
fn stamp(image: &mut Rgba, art: &Rgba, x: u32, y: u32) {
    for yy in 0..art.height {
        for xx in 0..art.width {
            let c = art.get(xx, yy);
            if c[3] != 0 {
                image.set(x + xx, y + yy, c);
            }
        }
    }
}

/// `rows` drawn faintly, centred on a 16x16 empty-slot icon.
fn ghost(rows: &str) -> Rgba {
    let glyph = art(rows, &[('o', GHOST)]);
    let mut image = Rgba::new(16, 16);
    image.blit(&glyph, (16 - glyph.width) / 2, (16 - glyph.height) / 2);
    image
}

pub fn write(bundle: &mut Bundle) -> Result<()> {
    storage(bundle)?;
    furnaces(bundle)?;
    crafter(bundle)?;
    brewing(bundle)?;
    enchanting(bundle)?;
    workstations(bundle)?;
    cutters(bundle)?;
    beacon(bundle)?;
    villager(bundle)?;
    horse(bundle)?;
    book(bundle)?;
    slot_icons(bundle)?;
    Ok(())
}

// --- Storage --------------------------------------------------------------------

fn storage(bundle: &mut Bundle) -> Result<()> {
    // Chests: six rows on top, the player's half from v = 126; the screen
    // draws the top 17 + 18 x rows and that half, so the side edges run
    // unbroken through both.
    let mut chest = panel(256, 256, 176, 222);
    grid(&mut chest, 7, 17, 9, 6);
    inventory(&mut chest, 7, 139);
    texture(bundle, "gui/container/generic_54", &chest)?;
    let mut shulker = panel(256, 256, 176, 167);
    grid(&mut shulker, 7, 17, 9, 3);
    inventory(&mut shulker, 7, 83);
    texture(bundle, "gui/container/shulker_box", &shulker)?;
    let mut dispenser = panel(256, 256, 176, 166);
    well(&mut dispenser, 59, 14, 58, 58, PANEL_SOFT);
    grid(&mut dispenser, 61, 16, 3, 3);
    inventory(&mut dispenser, 7, 83);
    texture(bundle, "gui/container/dispenser", &dispenser)?;
    let mut hopper = panel(256, 256, 176, 133);
    grid(&mut hopper, 43, 19, 5, 1);
    inventory(&mut hopper, 7, 50);
    texture(bundle, "gui/container/hopper", &hopper)?;
    Ok(())
}

// --- Furnaces -------------------------------------------------------------------

fn furnaces(bundle: &mut Bundle) -> Result<()> {
    let kinds: [(&str, [u8; 4], [u8; 4]); 3] = [
        ("furnace", [0xf0, 0x8c, 0x30, 255], [0xff, 0xe0, 0x70, 255]),
        (
            "blast_furnace",
            [0x58, 0x8c, 0xf0, 255],
            [0xd8, 0xf0, 0xff, 255],
        ),
        ("smoker", [0xd0, 0x5c, 0x30, 255], [0xf8, 0xb8, 0x60, 255]),
    ];
    for (name, outer, core) in kinds {
        let mut image = panel(256, 256, 176, 166);
        slot(&mut image, 55, 16, 18);
        slot(&mut image, 55, 52, 18);
        slot(&mut image, 111, 30, 26);
        // The empty flame and arrow the progress sprites fill.
        stamp(&mut image, &flame(14, 14, PANEL_SOFT, PANEL_SOFT), 56, 36);
        arrow(&mut image, 79, 34, PANEL_SOFT);
        inventory(&mut image, 7, 83);
        texture(bundle, &format!("gui/container/{name}"), &image)?;
        put(
            bundle,
            &format!("container/{name}/lit_progress"),
            &flame(14, 14, outer, core),
        )?;
        let mut burn = Rgba::new(24, 16);
        arrow(&mut burn, 0, 0, ACCENT);
        put(bundle, &format!("container/{name}/burn_progress"), &burn)?;
    }
    Ok(())
}

// --- The crafter ----------------------------------------------------------------

fn crafter(bundle: &mut Bundle) -> Result<()> {
    let mut image = panel(256, 256, 176, 166);
    grid(&mut image, 25, 16, 3, 3);
    slot(&mut image, 129, 30, 26);
    image.fill(115, 41, 9, 4, PANEL_SOFT);
    for i in 0..4u32 {
        image.fill(123 + i, 39 + i, 1, 8 - 2 * i, PANEL_SOFT);
    }
    inventory(&mut image, 7, 83);
    texture(bundle, "gui/container/crafter", &image)?;
    // A disabled slot: the frame, hatched.
    let mut disabled = Rgba::new(18, 18);
    slot(&mut disabled, 0, 0, 18);
    disabled.fill(1, 1, 16, 16, PANEL_SOFT);
    cross(&mut disabled, 3, 3, 12, SHADE);
    put(bundle, "container/crafter/disabled_slot", &disabled)?;
    let dust = |lit: bool| {
        let (c, h) = if lit {
            ([0xe0, 0x20, 0x20, 255], [0xff, 0x8c, 0x7c, 255])
        } else {
            ([0x5c, 0x1c, 0x1c, 255], [0x7c, 0x30, 0x2c, 255])
        };
        art(
            "
            ......oo........
            .....ohho.......
            ....ohcco.......
            ...occcco.......
            ..occco.........
            .occco...oo.....
            .occo...ohco....
            .oco...occcco...
            ..o...occccco...
            .....occcco.....
            ....occcco......
            ....occco.......
            .....ooo........
            ................
            ................
            ................",
            &[('o', darken(c, 30)), ('c', c), ('h', h)],
        )
    };
    put(bundle, "container/crafter/powered_redstone", &dust(true))?;
    put(bundle, "container/crafter/unpowered_redstone", &dust(false))
}

// --- The brewing stand ----------------------------------------------------------

fn brewing(bundle: &mut Bundle) -> Result<()> {
    let mut image = panel(256, 256, 176, 166);
    // The stand's rod and arms, behind the bottles.
    image.fill(86, 34, 2, 24, SHADE);
    image.fill(73, 58, 5, 2, SHADE);
    image.fill(96, 58, 5, 2, SHADE);
    slot(&mut image, 16, 16, 18);
    slot(&mut image, 78, 16, 18);
    for (x, y) in [(55, 50), (78, 57), (101, 50)] {
        slot(&mut image, x, y, 18);
    }
    // The fuel bar's track, the brewing arrow's track, the bubbles' column.
    well(&mut image, 59, 43, 20, 6, PANEL_SOFT);
    image.fill(98, 17, 7, 22, PANEL_SOFT);
    for i in 0..5u32 {
        image.fill(97 + i, 39 + i, 9 - 2 * i, 1, PANEL_SOFT);
    }
    image.fill(64, 15, 10, 27, lighten(PANEL_SOFT, 12));
    inventory(&mut image, 7, 83);
    texture(bundle, "gui/container/brewing_stand", &image)?;
    let mut fuel = Rgba::filled(18, 4, [0xf0, 0xa0, 0x30, 255]);
    fuel.fill(0, 0, 18, 1, [0xff, 0xd8, 0x80, 255]);
    put(bundle, "container/brewing_stand/fuel_length", &fuel)?;
    // A downward arrow, drawn from the top as the brew advances.
    let mut brew = Rgba::new(9, 28);
    brew.fill(1, 0, 7, 22, ACCENT);
    for i in 0..5u32 {
        brew.fill(i, 22 + i, 9 - 2 * i, 1, ACCENT);
    }
    put(bundle, "container/brewing_stand/brew_progress", &brew)?;
    // Bubbles rising, drawn from the bottom.
    let mut bubbles = Rgba::new(12, 29);
    let water = [0x7c, 0xc0, 0xf8, 255];
    for (x, y, r) in [
        (6.0, 25.0, 3.0),
        (3.0, 18.0, 2.2),
        (8.0, 12.0, 2.0),
        (4.0, 6.0, 1.6),
        (8.0, 2.0, 1.2),
    ] {
        for yy in 0..29u32 {
            for xx in 0..12u32 {
                let (dx, dy) = (xx as f32 - x, yy as f32 - y);
                let d = (dx * dx + dy * dy).sqrt();
                if d <= r {
                    bubbles.set(xx, yy, if d > r - 1.0 { water } else { WHITE });
                }
            }
        }
    }
    put(bundle, "container/brewing_stand/bubbles", &bubbles)
}

// --- The enchanting table -------------------------------------------------------

fn enchanting(bundle: &mut Bundle) -> Result<()> {
    let mut image = panel(256, 256, 176, 166);
    // A soft plate where the book floats, and the well of the three offers.
    image.fill(12, 16, 40, 26, PANEL_SOFT);
    well(&mut image, 59, 13, 110, 59, SLOT_FILL);
    slot(&mut image, 14, 46, 18);
    slot(&mut image, 34, 46, 18);
    inventory(&mut image, 7, 83);
    texture(bundle, "gui/container/enchanting_table", &image)?;
    for (name, fill, border) in [
        ("enchantment_slot", PANEL, PANEL_EDGE),
        ("enchantment_slot_highlighted", DIM, ACCENT),
        ("enchantment_slot_disabled", PANEL_SOFT, SHADE),
    ] {
        let mut s = framed(108, 19, fill, border);
        // The cost badge's square on the left.
        s.fill(1, 1, 17, 17, darken(fill, 18));
        put(bundle, &format!("container/enchanting_table/{name}"), &s)?;
    }
    for level in 1..=3u32 {
        for disabled in [false, true] {
            let (c, h) = if disabled {
                (SHADE, PANEL_SOFT)
            } else {
                (GREEN, [0xc4, 0xff, 0x9c, 255])
            };
            let mut icon = Rgba::new(16, 16);
            // One orb per level, in a row from the bottom left.
            for i in 0..level {
                let (x, y) = (1 + i * 5, 10 - i * 4);
                icon.fill(x, y + 1, 4, 2, c);
                icon.fill(x + 1, y, 2, 4, c);
                icon.set(x + 1, y + 1, h);
            }
            let suffix = if disabled { "_disabled" } else { "" };
            put(
                bundle,
                &format!("container/enchanting_table/level_{level}{suffix}"),
                &icon,
            )?;
        }
    }
    Ok(())
}

// --- Anvil, grindstone, smithing table ------------------------------------------

/// The 28x21 "this does not work" mark drawn over an arrow.
fn error_mark() -> Rgba {
    let mut image = Rgba::new(28, 21);
    cross(&mut image, 6, 2, 17, RED);
    image
}

fn workstations(bundle: &mut Bundle) -> Result<()> {
    let mut anvil = panel(256, 256, 176, 166);
    // An anvil glyph beside the title.
    anvil.fill(16, 14, 30, 6, SHADE);
    anvil.fill(24, 20, 14, 6, SHADE);
    anvil.fill(19, 26, 24, 4, SHADE);
    slot(&mut anvil, 26, 46, 18);
    plus(&mut anvil, 55, 50, SHADE);
    slot(&mut anvil, 75, 46, 18);
    arrow(&mut anvil, 102, 48, SHADE);
    slot(&mut anvil, 133, 46, 18);
    inventory(&mut anvil, 7, 83);
    texture(bundle, "gui/container/anvil", &anvil)?;
    put(
        bundle,
        "container/anvil/text_field",
        &framed(110, 16, PLATE, RULE),
    )?;
    put(
        bundle,
        "container/anvil/text_field_disabled",
        &framed(110, 16, [0x18, 0x1a, 0x22, 255], PLATE),
    )?;
    put(bundle, "container/anvil/error", &error_mark())?;

    let mut grindstone = panel(256, 256, 176, 166);
    // The stone between its two inputs.
    grindstone.fill(44, 14, 26, 46, PANEL_SOFT);
    grindstone.fill(46, 36, 22, 4, SHADE);
    slot(&mut grindstone, 48, 18, 18);
    slot(&mut grindstone, 48, 39, 18);
    arrow(&mut grindstone, 95, 34, SHADE);
    slot(&mut grindstone, 128, 33, 18);
    inventory(&mut grindstone, 7, 83);
    texture(bundle, "gui/container/grindstone", &grindstone)?;
    put(bundle, "container/grindstone/error", &error_mark())?;

    let mut smithing = panel(256, 256, 176, 166);
    // A hammer beside the title, and a stand on the right for the preview.
    smithing.fill(12, 10, 18, 7, SHADE);
    smithing.fill(19, 17, 4, 18, SHADE);
    smithing.fill(122, 10, 46, 68, PANEL_SOFT);
    for (x, y) in [(7, 47), (25, 47), (43, 47)] {
        slot(&mut smithing, x, y, 18);
    }
    arrow(&mut smithing, 68, 49, SHADE);
    slot(&mut smithing, 97, 47, 18);
    inventory(&mut smithing, 7, 83);
    texture(bundle, "gui/container/smithing", &smithing)?;
    put(bundle, "container/smithing/error", &error_mark())
}

// --- Stonecutter, cartography table, loom ---------------------------------------

/// A 12x15 scroller handle, lit or greyed.
fn scroller(active: bool) -> Rgba {
    if active {
        framed(12, 15, ACCENT, [0x4a, 0x3a, 0x9a, 255])
    } else {
        framed(12, 15, SHADE, PANEL_EDGE)
    }
}

/// A list button: normal, under the mouse, or chosen.
fn tile(w: u32, h: u32, state: usize) -> Rgba {
    match state {
        0 => framed(w, h, PANEL, PANEL_EDGE),
        1 => framed(w, h, lighten(ACCENT, 70), ACCENT),
        _ => framed(w, h, ACCENT, [0x4a, 0x3a, 0x9a, 255]),
    }
}

/// A sheet of map paper, `w`x`h`, with a few contour lines.
fn paper(w: u32, h: u32, inset: u32) -> Rgba {
    let mut image = framed(w, h, [0xf0, 0xe8, 0xd4, 255], [0xb8, 0xa8, 0x88, 255]);
    let ink = [0xc8, 0xb8, 0x98, 255];
    for i in 0..3u32 {
        let y = h / 4 + i * h / 5;
        image.fill(inset, y, w - 2 * inset, 1, ink);
    }
    image
}

fn cutters(bundle: &mut Bundle) -> Result<()> {
    let names = ["", "_highlighted", "_selected"];

    let mut stonecutter = panel(256, 256, 176, 166);
    slot(&mut stonecutter, 19, 32, 18);
    well(&mut stonecutter, 51, 14, 66, 56, SLOT_FILL);
    well(&mut stonecutter, 118, 14, 14, 56, PANEL_SOFT);
    slot(&mut stonecutter, 138, 28, 26);
    inventory(&mut stonecutter, 7, 83);
    texture(bundle, "gui/container/stonecutter", &stonecutter)?;
    for (i, suffix) in names.iter().enumerate() {
        put(
            bundle,
            &format!("container/stonecutter/recipe{suffix}"),
            &tile(16, 18, i),
        )?;
    }
    put(bundle, "container/stonecutter/scroller", &scroller(true))?;
    put(
        bundle,
        "container/stonecutter/scroller_disabled",
        &scroller(false),
    )?;

    let mut cartography = panel(256, 256, 176, 166);
    slot(&mut cartography, 14, 14, 18);
    plus(&mut cartography, 19, 37, SHADE);
    slot(&mut cartography, 14, 51, 18);
    well(&mut cartography, 65, 11, 70, 70, PANEL_SOFT);
    slot(&mut cartography, 140, 34, 26);
    inventory(&mut cartography, 7, 83);
    texture(bundle, "gui/container/cartography_table", &cartography)?;
    put(bundle, "container/cartography_table/map", &paper(66, 66, 6))?;
    let mut scaled = paper(66, 66, 6);
    scaled.blit(&paper(30, 30, 4), 18, 18);
    put(bundle, "container/cartography_table/scaled_map", &scaled)?;
    put(
        bundle,
        "container/cartography_table/duplicated_map",
        &paper(50, 66, 5),
    )?;
    let lock = art(
        "
        ..oooooo..
        .o......o.
        .o......o.
        .o......o.
        oooooooooo
        offffffffo
        offffffffo
        offffoffff
        offffoffff
        offffffffo
        offffffffo
        offffffffo
        offffffffo
        oooooooooo",
        &[('o', RULE), ('f', [0xd8, 0xb0, 0x48, 255])],
    );
    put(bundle, "container/cartography_table/locked", &lock)?;
    put(bundle, "container/cartography_table/error", &error_mark())?;

    let mut loom = panel(256, 256, 176, 166);
    slot(&mut loom, 12, 25, 18);
    slot(&mut loom, 32, 25, 18);
    slot(&mut loom, 22, 44, 18);
    well(&mut loom, 59, 12, 58, 58, SLOT_FILL);
    well(&mut loom, 118, 12, 14, 58, PANEL_SOFT);
    well(&mut loom, 139, 6, 24, 44, PANEL_SOFT);
    slot(&mut loom, 138, 52, 26);
    inventory(&mut loom, 7, 83);
    texture(bundle, "gui/container/loom", &loom)?;
    for (i, suffix) in names.iter().enumerate() {
        put(
            bundle,
            &format!("container/loom/pattern{suffix}"),
            &tile(14, 14, i),
        )?;
    }
    put(bundle, "container/loom/scroller", &scroller(true))?;
    put(bundle, "container/loom/scroller_disabled", &scroller(false))?;
    let mut error = Rgba::new(26, 26);
    cross(&mut error, 4, 4, 18, RED);
    put(bundle, "container/loom/error", &error)
}

// --- The beacon -------------------------------------------------------------------

fn beacon(bundle: &mut Bundle) -> Result<()> {
    // The powers sit on a dark plate: their labels are light text.
    let mut image = panel(256, 256, 230, 219);
    image.fill(4, 4, 222, 128, RULE);
    image.fill(5, 5, 220, 126, PLATE);
    image.fill(114, 8, 1, 92, RULE);
    // Tier marks beside the primary rows: one to three bars.
    for tier in 0..3u32 {
        for bar in 0..=tier {
            image.fill(10 + bar * 4, 26 + tier * 25, 2, 14, ACCENT);
        }
    }
    image.fill(10, 104, 120, 24, [0x18, 0x1a, 0x22, 255]);
    slot(&mut image, 135, 109, 18);
    inventory(&mut image, 35, 136);
    texture(bundle, "gui/container/beacon", &image)?;
    for (name, fill, border) in [
        ("button", PLATE, RULE),
        ("button_highlighted", DIM, ACCENT),
        ("button_selected", [0x34, 0x30, 0x50, 255], ACCENT),
        ("button_disabled", [0x18, 0x1a, 0x22, 255], PLATE),
    ] {
        let mut b = framed(22, 22, fill, border);
        if name == "button_selected" {
            b.fill(1, 19, 20, 2, ACCENT);
        }
        put(bundle, &format!("container/beacon/{name}"), &b)?;
    }
    let check = art(
        "
        ...............
        ..............o
        .............oo
        ............oo.
        ...........oo..
        o.........oo...
        oo.......oo....
        .oo.....oo.....
        ..oo...oo......
        ...oo.oo.......
        ....ooo........
        .....o.........",
        &[('o', GREEN)],
    );
    let mut confirm = Rgba::new(18, 18);
    confirm.blit(&check, 1, 3);
    put(bundle, "container/beacon/confirm", &confirm)?;
    let mut cancel = Rgba::new(18, 18);
    cross(&mut cancel, 3, 3, 12, RED);
    put(bundle, "container/beacon/cancel", &cancel)
}

// --- Trading ----------------------------------------------------------------------

fn villager(bundle: &mut Bundle) -> Result<()> {
    let mut image = panel(512, 256, 276, 166);
    // The offers list (88-wide buttons) with its scroller's track.
    well(&mut image, 4, 17, 97, 142, PANEL_SOFT);
    image.fill(94, 18, 6, 140, SLOT_FILL);
    slot(&mut image, 135, 36, 18);
    // A small plus in the narrow gap between the two costs.
    image.fill(155, 44, 5, 1, SHADE);
    image.fill(157, 42, 1, 5, SHADE);
    slot(&mut image, 161, 36, 18);
    arrow(&mut image, 185, 38, SHADE);
    slot(&mut image, 215, 32, 26);
    inventory(&mut image, 107, 83);
    texture(bundle, "gui/container/villager", &image)?;
    put(
        bundle,
        "container/villager/discount_strikethrough",
        &Rgba::filled(9, 2, RED),
    )?;
    let bar = |fill: [u8; 4], light: [u8; 4]| {
        let mut b = Rgba::filled(102, 5, [0x08, 0x0c, 0x08, 255]);
        b.fill(1, 1, 100, 3, fill);
        b.fill(1, 1, 100, 1, light);
        b
    };
    put(
        bundle,
        "container/villager/experience_bar_background",
        &bar([0x1c, 0x24, 0x1c, 230], [0x2c, 0x34, 0x2c, 230]),
    )?;
    put(
        bundle,
        "container/villager/experience_bar_current",
        &bar([0x6c, 0xe0, 0x3c, 255], [0xc4, 0xff, 0x9c, 255]),
    )?;
    put(
        bundle,
        "container/villager/experience_bar_result",
        &bar([0x3c, 0x8c, 0xd8, 255], [0xa8, 0xd8, 0xff, 255]),
    )?;
    put(bundle, "container/villager/out_of_stock", &error_mark())?;
    put(
        bundle,
        "container/villager/scroller",
        &framed(6, 27, ACCENT, [0x4a, 0x3a, 0x9a, 255]),
    )?;
    put(
        bundle,
        "container/villager/scroller_disabled",
        &framed(6, 27, SHADE, PANEL_EDGE),
    )?;
    // The small arrow on each offer button (dark buttons: light ink).
    for (name, color) in [("trade_arrow", INK), ("trade_arrow_out_of_stock", RED)] {
        let mut a = Rgba::new(10, 9);
        a.fill(0, 3, 6, 3, color);
        for i in 0..5u32 {
            a.fill(5 + i, i, 1, 9 - 2 * i, color);
        }
        put(bundle, &format!("container/villager/{name}"), &a)?;
    }
    Ok(())
}

// --- Mounts -------------------------------------------------------------------------

fn horse(bundle: &mut Bundle) -> Result<()> {
    let mut image = panel(256, 256, 176, 166);
    // The mount stands in a dark box.
    well(&mut image, 25, 17, 54, 54, PLATE);
    inventory(&mut image, 7, 83);
    texture(bundle, "gui/container/horse", &image)?;
    let mut chest = Rgba::new(90, 54);
    grid(&mut chest, 0, 0, 5, 3);
    put(bundle, "container/horse/chest_slots", &chest)
}

// --- The book (lectern) ---------------------------------------------------------------

fn book(bundle: &mut Bundle) -> Result<()> {
    // One page, 146x180 from (20, 1), the binding on its left.
    let mut image = Rgba::new(256, 256);
    image.fill(20, 1, 146, 180, PANEL_EDGE);
    image.fill(21, 2, 144, 178, PANEL);
    image.fill(21, 2, 6, 178, PANEL_SOFT);
    image.fill(27, 2, 1, 178, PANEL_EDGE);
    image.fill(28, 178, 137, 1, PANEL_SOFT);
    texture(bundle, "gui/book", &image)?;
    for (name, forward) in [("page_forward", true), ("page_backward", false)] {
        for highlighted in [false, true] {
            let color = if highlighted { ACCENT } else { SHADE };
            let mut a = Rgba::new(23, 13);
            a.fill(if forward { 3 } else { 10 }, 5, 10, 3, color);
            for i in 0..6u32 {
                let x = if forward { 13 + i } else { 9 - i };
                a.fill(x, 2 + i, 1, 11 - 2 * i - 1, color);
            }
            let suffix = if highlighted { "_highlighted" } else { "" };
            put(bundle, &format!("widget/{name}{suffix}"), &a)?;
        }
    }
    Ok(())
}

// --- Empty-slot icons ----------------------------------------------------------------

/// Faint outlines of what goes in an empty slot (the armour ones are in
/// art_survival.rs).
const SLOT_ICONS: &[(&str, &str)] = &[
    (
        "potion",
        "
        ....oooo....
        ....o..o....
        ....o..o....
        ...oo..oo...
        ..o......o..
        .o........o.
        .o........o.
        .o........o.
        .o........o.
        ..o......o..
        ...oooooo...",
    ),
    (
        "brewing_fuel",
        "
        .....o.....
        ....ooo....
        .o..ooo..o.
        ..o.ooo.o..
        ...ooooo...
        oooooooooo.
        ...ooooo...
        ..o.ooo.o..
        .o..ooo..o.
        ....ooo....
        .....o.....",
    ),
    (
        "lapis_lazuli",
        "
        ....oooo....
        ...o....o...
        ..o......o..
        .o........o.
        o..........o
        .o........o.
        ..o......o..
        ...o....o...
        ....oooo....",
    ),
    (
        "saddle",
        "
        .oo......oo.
        o..oooooo..o
        o..........o
        .o........o.
        ..oo....oo..
        ...o....o...
        ...o....o...
        ..ooo..ooo..",
    ),
    (
        "horse_armor",
        "
        ........oo..
        .......o..o.
        ......o...o.
        .ooooo....o.
        o.........o.
        o........o..
        .oooo...o...
        ....o..o....
        ....oooo....",
    ),
    (
        "llama_armor",
        "
        oooooooooooo
        o..........o
        o.oo.oo.oo.o
        o..........o
        o.oo.oo.oo.o
        o..........o
        oooooooooooo
        o.o.o.o.o.o.",
    ),
    (
        "banner",
        "
        oooooooooo
        o........o
        o........o
        o..oooo..o
        o..o..o..o
        o..oooo..o
        o........o
        o........o
        o........o
        o...oo...o
        o..o..o..o
        ooo....ooo",
    ),
    (
        "dye",
        "
        ....oo....
        ...o..o...
        ..o....o..
        .o......o.
        o........o
        o........o
        o........o
        .o......o.
        ..oooooo..",
    ),
    (
        "banner_pattern",
        "
        oooooooooo
        o........o
        o.oooooo.o
        o.o....o.o
        o.o.oo.o.o
        o.o.oo.o.o
        o.o....o.o
        o.oooooo.o
        o........o
        oooooooooo",
    ),
    (
        "smithing_template_netherite_upgrade",
        "
        oooooooooo
        o........o
        o...oo...o
        o..o..o..o
        o.o....o.o
        o..o..o..o
        o...oo...o
        o........o
        oooooooooo",
    ),
    (
        "smithing_template_armor_trim",
        "
        oooooooooo
        o........o
        o.oooooo.o
        o.o....o.o
        o..o..o..o
        o..o..o..o
        o...oo...o
        o........o
        oooooooooo",
    ),
    (
        "ingot",
        "
        ....oooooooo
        ...o.......o
        ..o.......o.
        .o.......o..
        oooooooooo..",
    ),
    (
        "diamond",
        "
        ..oooooo..
        .o..o...o.
        o..o.....o
        .o......o.
        ..o....o..
        ...o..o...
        ....oo....",
    ),
    (
        "emerald",
        "
        ...oooo...
        ..o....o..
        .o......o.
        .o......o.
        .o......o.
        .o......o.
        ..o....o..
        ...oooo...",
    ),
    (
        "quartz",
        "
        ...oo.....
        ..o..o....
        ..o...oo..
        .o......o.
        .o.....o..
        o.....o...
        .ooooo....",
    ),
    (
        "amethyst_shard",
        "
        ......o...
        .....o.o..
        ....o...o.
        ...o...o..
        ..o...o...
        .o...o....
        o...o.....
        .ooo......",
    ),
    (
        "redstone_dust",
        "
        ..o.....o.
        .ooo...o..
        ..o..o....
        ....ooo...
        .o...o..o.
        ooo....ooo
        .o..o...o.
        ...ooo....
        ....o.....",
    ),
    (
        "sword",
        "
        .........oo
        ........o.o
        .......o.o.
        ......o.o..
        .....o.o...
        .o..o.o....
        ..oo.o.....
        ..oo.......
        .o..o......
        o..........",
    ),
    (
        "pickaxe",
        "
        ..ooooo....
        .o....oo...
        .......oo..
        ......o.oo.
        .....o...o.
        ....o....o.
        ...o.......
        ..o........
        .o.........
        o..........",
    ),
    (
        "axe",
        "
        .....ooo...
        ....o...o..
        ...o....o..
        ....o..o...
        ...o.oo....
        ..o........
        .o.........
        o..........",
    ),
    (
        "shovel",
        "
        .......ooo.
        ......o...o
        ......o...o
        .......ooo.
        ......o....
        .....o.....
        ....o......
        ...o.......
        ..o........
        .o.........",
    ),
    (
        "hoe",
        "
        ....oooo...
        ...o...o...
        .......o...
        ......o....
        .....o.....
        ....o......
        ...o.......
        ..o........
        .o.........",
    ),
    (
        "spear",
        "
        .........oo
        ........ooo
        .......oo..
        ......o....
        .....o.....
        ....o......
        ...o.......
        ..o........
        .o.........",
    ),
    (
        "nautilus_armor",
        "
        ...oooooo...
        ..o......o..
        .o..oooo..o.
        o..o....o..o
        o.o..oo..o.o
        o..o...o.o.o
        .o..ooo..o..
        ..o.....o...
        ...ooooo....",
    ),
    (
        "nautilus_armor_inventory",
        "
        ...oooooo...
        ..o......o..
        .o..oooo..o.
        o..o....o..o
        o.o..oo..o.o
        o..o...o.o.o
        .o..ooo..o..
        ..o.....o...
        ...ooooo....",
    ),
];

fn slot_icons(bundle: &mut Bundle) -> Result<()> {
    for (name, rows) in SLOT_ICONS {
        put(bundle, &format!("container/slot/{name}"), &ghost(rows))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file at Minecraft's pixel size (measured on the 26.3 files),
    /// so the screens' layout constants land on the drawn slots.
    #[test]
    fn every_screen_file_has_vanilla_pixel_size() {
        let mut bundle = Bundle::default();
        write(&mut bundle).unwrap();
        let mut sizes: Vec<(String, u32, u32)> = [
            "anvil",
            "beacon",
            "blast_furnace",
            "brewing_stand",
            "cartography_table",
            "crafter",
            "dispenser",
            "enchanting_table",
            "furnace",
            "generic_54",
            "grindstone",
            "hopper",
            "horse",
            "loom",
            "shulker_box",
            "smithing",
            "smoker",
            "stonecutter",
        ]
        .iter()
        .map(|n| (format!("textures/gui/container/{n}"), 256, 256))
        .collect();
        sizes.push(("textures/gui/container/villager".into(), 512, 256));
        sizes.push(("textures/gui/book".into(), 256, 256));
        let sprites: &[(&str, u32, u32)] = &[
            ("anvil/error", 28, 21),
            ("anvil/text_field", 110, 16),
            ("anvil/text_field_disabled", 110, 16),
            ("beacon/button", 22, 22),
            ("beacon/button_disabled", 22, 22),
            ("beacon/button_highlighted", 22, 22),
            ("beacon/button_selected", 22, 22),
            ("beacon/cancel", 18, 18),
            ("beacon/confirm", 18, 18),
            ("blast_furnace/burn_progress", 24, 16),
            ("blast_furnace/lit_progress", 14, 14),
            ("brewing_stand/brew_progress", 9, 28),
            ("brewing_stand/bubbles", 12, 29),
            ("brewing_stand/fuel_length", 18, 4),
            ("cartography_table/duplicated_map", 50, 66),
            ("cartography_table/error", 28, 21),
            ("cartography_table/locked", 10, 14),
            ("cartography_table/map", 66, 66),
            ("cartography_table/scaled_map", 66, 66),
            ("crafter/disabled_slot", 18, 18),
            ("crafter/powered_redstone", 16, 16),
            ("crafter/unpowered_redstone", 16, 16),
            ("enchanting_table/enchantment_slot", 108, 19),
            ("enchanting_table/enchantment_slot_disabled", 108, 19),
            ("enchanting_table/enchantment_slot_highlighted", 108, 19),
            ("enchanting_table/level_1", 16, 16),
            ("enchanting_table/level_1_disabled", 16, 16),
            ("enchanting_table/level_2", 16, 16),
            ("enchanting_table/level_2_disabled", 16, 16),
            ("enchanting_table/level_3", 16, 16),
            ("enchanting_table/level_3_disabled", 16, 16),
            ("furnace/burn_progress", 24, 16),
            ("furnace/lit_progress", 14, 14),
            ("grindstone/error", 28, 21),
            ("horse/chest_slots", 90, 54),
            ("loom/error", 26, 26),
            ("loom/pattern", 14, 14),
            ("loom/pattern_highlighted", 14, 14),
            ("loom/pattern_selected", 14, 14),
            ("loom/scroller", 12, 15),
            ("loom/scroller_disabled", 12, 15),
            ("smithing/error", 28, 21),
            ("smoker/burn_progress", 24, 16),
            ("smoker/lit_progress", 14, 14),
            ("stonecutter/recipe", 16, 18),
            ("stonecutter/recipe_highlighted", 16, 18),
            ("stonecutter/recipe_selected", 16, 18),
            ("stonecutter/scroller", 12, 15),
            ("stonecutter/scroller_disabled", 12, 15),
            ("villager/discount_strikethrough", 9, 2),
            ("villager/experience_bar_background", 102, 5),
            ("villager/experience_bar_current", 102, 5),
            ("villager/experience_bar_result", 102, 5),
            ("villager/out_of_stock", 28, 21),
            ("villager/scroller", 6, 27),
            ("villager/scroller_disabled", 6, 27),
            ("villager/trade_arrow", 10, 9),
            ("villager/trade_arrow_out_of_stock", 10, 9),
        ];
        for (name, w, h) in sprites {
            sizes.push((format!("textures/gui/sprites/container/{name}"), *w, *h));
        }
        for (name, _) in SLOT_ICONS {
            sizes.push((
                format!("textures/gui/sprites/container/slot/{name}"),
                16,
                16,
            ));
        }
        for name in [
            "page_forward",
            "page_backward",
            "page_forward_highlighted",
            "page_backward_highlighted",
        ] {
            sizes.push((format!("textures/gui/sprites/widget/{name}"), 23, 13));
        }
        for (path, w, h) in &sizes {
            let full = format!("assets/minecraft/{path}.png");
            let bytes = bundle
                .get(&full)
                .unwrap_or_else(|| panic!("{full} missing"));
            let image = Rgba::decode(bytes).unwrap();
            assert_eq!((image.width, image.height), (*w, *h), "{full}");
        }
        assert_eq!(sizes.len(), bundle.len(), "a file without a listed size");
        assert_eq!(SLOT_ICONS.len(), 25);
    }

    /// The slot frames sit where the menus put their items: an item at
    /// (x, y) has its frame's dark corner at (x - 1, y - 1).
    #[test]
    fn slot_frames_sit_under_the_menus_items() {
        let mut bundle = Bundle::default();
        write(&mut bundle).unwrap();
        let image = |name: &str| {
            Rgba::decode(
                bundle
                    .get(&format!("assets/minecraft/textures/gui/{name}.png"))
                    .unwrap(),
            )
            .unwrap()
        };
        // (texture, item positions of the menu's own slots, of the player's first slot).
        let cases: &[(&str, &[(u32, u32)], (u32, u32))] = &[
            ("container/furnace", &[(56, 17), (56, 53)], (8, 84)),
            ("container/blast_furnace", &[(56, 17), (56, 53)], (8, 84)),
            ("container/smoker", &[(56, 17), (56, 53)], (8, 84)),
            ("container/dispenser", &[(62, 17), (98, 53)], (8, 84)),
            ("container/hopper", &[(44, 20), (124, 20)], (8, 51)),
            ("container/generic_54", &[(8, 18), (152, 108)], (8, 140)),
            ("container/shulker_box", &[(8, 18), (152, 54)], (8, 84)),
            ("container/crafter", &[(26, 17), (62, 53)], (8, 84)),
            (
                "container/brewing_stand",
                &[(17, 17), (79, 17), (56, 51), (79, 58), (102, 51)],
                (8, 84),
            ),
            ("container/enchanting_table", &[(15, 47), (35, 47)], (8, 84)),
            ("container/anvil", &[(27, 47), (76, 47), (134, 47)], (8, 84)),
            (
                "container/grindstone",
                &[(49, 19), (49, 40), (129, 34)],
                (8, 84),
            ),
            (
                "container/smithing",
                &[(8, 48), (26, 48), (44, 48), (98, 48)],
                (8, 84),
            ),
            ("container/stonecutter", &[(20, 33)], (8, 84)),
            (
                "container/cartography_table",
                &[(15, 15), (15, 52)],
                (8, 84),
            ),
            ("container/loom", &[(13, 26), (33, 26), (23, 45)], (8, 84)),
            ("container/beacon", &[(136, 110)], (36, 137)),
            ("container/villager", &[(136, 37), (162, 37)], (108, 84)),
            ("container/horse", &[], (8, 84)),
        ];
        // Every panel is opaque over the area the screens draw.
        for (name, w, h) in [
            ("container/generic_54", 176, 222),
            ("container/shulker_box", 176, 167),
            ("container/hopper", 176, 133),
            ("container/beacon", 230, 219),
            ("container/villager", 276, 166),
            ("container/furnace", 176, 166),
            ("container/blast_furnace", 176, 166),
            ("container/smoker", 176, 166),
            ("container/brewing_stand", 176, 166),
            ("container/enchanting_table", 176, 166),
        ] {
            let image = image(name);
            for y in 0..h {
                for x in 0..w {
                    assert_eq!(image.get(x, y)[3], 255, "{name} at {x},{y}");
                }
            }
        }
        for (name, items, (px, py)) in cases {
            let image = image(name);
            // The player's 27 slots, then the hotbar 58 pixels below the first row.
            let player = (0..36).map(|i| (px + i % 9 * 18, py + i / 9 * 18 + i / 27 * 4));
            for (x, y) in items.iter().copied().chain(player) {
                assert_eq!(image.get(x - 1, y - 1), SHADE, "{name} at {x},{y}");
                assert_eq!(image.get(x, y), SLOT_FILL, "{name} at {x},{y}");
            }
        }
    }
}
