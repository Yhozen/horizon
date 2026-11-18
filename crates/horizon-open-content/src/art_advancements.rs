//! Horizon's original artwork for the advancements screen, drawn in the
//! light panel style of the inventory: the window, the tabs around it, the
//! task, goal and challenge frames, the title and description boxes, and the
//! tab backgrounds a server's advancements name. The files sit at
//! Minecraft's paths and pixel sizes, laid out where the client's screen
//! (`game/advancements_screen.rs`) draws them: a 252x140 window with its
//! 234x113 inside at (9, 18), 28x32 tabs above and below it and 32x28 ones
//! beside it overlapping it by 4 pixels, 26x26 frames, 200x26 boxes.
//! Every pixel is drawn here (generator `art`).
use crate::{
    Bundle, Result,
    art_survival::{
        ACCENT, PANEL, PANEL_EDGE, PANEL_SOFT, PLATE, RULE, SHADE, WHITE, darken, lighten, put,
        texture,
    },
    image::{Rgba, noise},
};
use serde_json::json;

const GOLD: [u8; 4] = [0xf2, 0xc1, 0x4e, 255];
const GOLD_EDGE: [u8; 4] = [0x8a, 0x5a, 0x10, 255];
const GREY: [u8; 4] = [0xc4, 0xc7, 0xd4, 255];
const NIGHT: [u8; 4] = [0x18, 0x1a, 0x22, 255];

pub fn write(bundle: &mut Bundle) -> Result<()> {
    texture(bundle, "gui/advancements/window", &window())?;
    tabs(bundle)?;
    for (kind, shape) in [
        ("task", Shape::Task),
        ("goal", Shape::Goal),
        ("challenge", Shape::Challenge),
    ] {
        for obtained in [true, false] {
            let state = if obtained { "obtained" } else { "unobtained" };
            put(
                bundle,
                &format!("advancements/{kind}_frame_{state}"),
                &frame(shape, obtained),
            )?;
        }
    }
    // The title bars and the description box, stretched by the client.
    for (name, fill, edge) in [
        ("box_obtained", darken(ACCENT, 0x50), ACCENT),
        ("box_unobtained", PLATE, RULE),
        ("title_box", NIGHT, RULE),
    ] {
        let mut image = Rgba::filled(200, 26, edge);
        image.fill(1, 1, 198, 24, fill);
        image.fill(1, 1, 198, 1, lighten(fill, 0x18));
        put(bundle, &format!("advancements/{name}"), &image)?;
        bundle.json(
            format!("assets/minecraft/textures/gui/sprites/advancements/{name}.png.mcmeta"),
            &json!({"gui": {"scaling": {"type": "nine_slice", "width": 200, "height": 26, "border": 3}}}),
            "art",
        )?;
    }
    // The tiles behind each vanilla tab (`background` in its root's display).
    for (name, base, seed) in [
        ("stone", [0x7c, 0x80, 0x8a, 255], 11),
        ("adventure", [0xa8, 0x8a, 0x5c, 255], 23),
        ("husbandry", [0x9c, 0x8a, 0x3e, 255], 37),
        ("nether", [0x5a, 0x22, 0x24, 255], 41),
        ("end", [0xd8, 0xd8, 0xa4, 255], 53),
    ] {
        texture(
            bundle,
            &format!("gui/advancements/backgrounds/{name}"),
            &tile(base, seed),
        )?;
    }
    Ok(())
}

/// The 256x256 window texture: a 252x140 light panel with the inside clear,
/// sunk by a one-pixel rim, the title's row above it.
fn window() -> Rgba {
    let mut image = Rgba::new(256, 256);
    image.fill(0, 0, 252, 140, PANEL_EDGE);
    image.fill(1, 1, 250, 138, PANEL);
    image.fill(2, 137, 248, 1, PANEL_SOFT);
    // The rim around the 234x113 inside at (9, 18): shade above and left,
    // white below and right, as the slots are.
    image.fill(8, 17, 236, 1, SHADE);
    image.fill(8, 17, 1, 115, SHADE);
    image.fill(8, 131, 236, 1, WHITE);
    image.fill(243, 17, 1, 115, WHITE);
    image.fill(9, 18, 234, 113, [0, 0, 0, 0]);
    image
}

/// One tab pointing up (28x32): rounded top, its last four rows over the
/// window's edge. Selected, it opens into the window; otherwise it stops at
/// the window's edge, behind it and a shade darker.
fn tab_above(selected: bool) -> Rgba {
    let (w, h) = (28, 32);
    let mut image = Rgba::new(w, h);
    let bottom = if selected { h } else { 29 };
    let fill = if selected { PANEL } else { PANEL_SOFT };
    image.fill(1, 0, w - 2, bottom, PANEL_EDGE);
    image.fill(0, 1, w, bottom - 1, PANEL_EDGE);
    image.fill(1, 1, w - 2, bottom - 1, fill);
    if !selected {
        // The window's own edge closes the tab.
        image.fill(0, 28, w, 1, PANEL_EDGE);
    } else {
        image.fill(2, 1, w - 4, 1, WHITE);
    }
    image
}

/// The tab turned to face `side` (the image is `tab_above` rotated).
fn tab(side: &str, selected: bool) -> Rgba {
    let up = tab_above(selected);
    let (w, h) = (up.width, up.height);
    let (ow, oh) = match side {
        "above" | "below" => (w, h),
        _ => (h, w),
    };
    let mut image = Rgba::new(ow, oh);
    for y in 0..oh {
        for x in 0..ow {
            let (sx, sy) = match side {
                "above" => (x, y),
                "below" => (x, h - 1 - y),
                // Pointing left: the up tab's top is the left column.
                "left" => (y, x),
                _ => (y, h - 1 - x),
            };
            image.set(x, y, up.get(sx, sy));
        }
    }
    image
}

fn tabs(bundle: &mut Bundle) -> Result<()> {
    for (side, parts) in [
        ("above", ["left", "middle", "right"]),
        ("below", ["left", "middle", "right"]),
        ("left", ["top", "middle", "bottom"]),
        ("right", ["top", "middle", "bottom"]),
    ] {
        for part in parts {
            for selected in [false, true] {
                let suffix = if selected { "_selected" } else { "" };
                put(
                    bundle,
                    &format!("advancements/tab_{side}_{part}{suffix}"),
                    &tab(side, selected),
                )?;
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Shape {
    /// A square with soft corners.
    Task,
    /// An octagon.
    Goal,
    /// A square notched at the middle of each side, its corners pointed out.
    Challenge,
}
impl Shape {
    fn inside(self, x: i32, y: i32) -> bool {
        if !(0..26).contains(&x) || !(0..26).contains(&y) {
            return false;
        }
        // Distance from the nearest edge on each axis.
        let (dx, dy) = (x.min(25 - x), y.min(25 - y));
        match self {
            Self::Task => dx + dy >= 1,
            Self::Goal => dx + dy >= 6,
            Self::Challenge => {
                let notch = (10..16).contains(&x) && dy < 2 || (10..16).contains(&y) && dx < 2;
                !notch && (dx + dy >= 1 || dx == dy)
            }
        }
    }
}

/// A 26x26 frame for the 16x16 icon at (5, 5): gold when obtained, grey
/// otherwise, with a light inner rim.
fn frame(shape: Shape, obtained: bool) -> Rgba {
    let (fill, edge) = if obtained {
        (GOLD, GOLD_EDGE)
    } else {
        (GREY, SHADE)
    };
    let mut image = Rgba::new(26, 26);
    for y in 0..26 {
        for x in 0..26 {
            if !shape.inside(x, y) {
                continue;
            }
            let rim = [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .iter()
                .any(|(ox, oy)| !shape.inside(x + ox, y + oy));
            let light = [(-2, 0), (0, -2)]
                .iter()
                .any(|(ox, oy)| !shape.inside(x + ox, y + oy));
            let colour = if rim {
                edge
            } else if light {
                lighten(fill, 0x28)
            } else {
                fill
            };
            image.set(x as u32, y as u32, colour);
        }
    }
    image
}

/// A seamless 16x16 tile: `base` broken by noise into lighter and darker
/// specks.
fn tile(base: [u8; 4], seed: u32) -> Rgba {
    let mut image = Rgba::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            let n = noise(x, y, seed) % 16;
            let colour = match n {
                0..=2 => darken(base, 0x18),
                3..=4 => lighten(base, 0x12),
                _ => base,
            };
            image.set(x, y, colour);
        }
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_meet_the_window_where_the_screen_draws_them() {
        // Above: the tab's row 28 is the window's top edge (y = -32 + 4).
        let up = tab("above", false);
        assert_eq!((up.width, up.height), (28, 32));
        assert_eq!(up.get(14, 28), PANEL_EDGE);
        assert_eq!(up.get(14, 30)[3], 0, "an unselected tab stops at the edge");
        assert_eq!(
            tab("above", true).get(14, 30),
            PANEL,
            "a selected one opens in"
        );
        // Beside: 32x28, the open end toward the window.
        let left = tab("left", true);
        assert_eq!((left.width, left.height), (32, 28));
        assert_eq!(left.get(31, 14), PANEL);
        assert_eq!(tab("right", true).get(0, 14), PANEL);
        assert_eq!(tab("below", true).get(14, 0), PANEL);
    }

    #[test]
    fn the_window_is_clear_inside_and_frames_hold_their_icon() {
        let w = window();
        assert_eq!(w.get(9 + 117, 18 + 56)[3], 0);
        assert_eq!(w.get(4, 8), PANEL);
        for shape in [Shape::Task, Shape::Goal, Shape::Challenge] {
            let f = frame(shape, true);
            // The icon's 16x16 square at (5, 5) is all frame.
            for y in 5..21 {
                for x in 5..21 {
                    assert_ne!(f.get(x, y)[3], 0);
                }
            }
        }
    }
}
