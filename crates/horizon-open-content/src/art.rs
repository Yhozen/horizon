//! Horizon's original artwork for what the open inputs do not cover, drawn
//! procedurally here so every pixel's origin is this generator: bedrock,
//! breaking overlays, the interface, the sky and the player's arm.
use crate::{
    Bundle, Origin, Result,
    image::{Rgba, noise},
};

const GENERATOR: &str = "art";

/// Block textures the open inputs lack.
pub fn block_texture(name: &str) -> Option<Rgba> {
    match name {
        "bedrock" => Some(bedrock()),
        other => crate::art_survival::crafting_table(other),
    }
}

/// The colours a biome's temperature and downfall give grey grass and
/// foliage textures: Horizon's own gradient, from a blue-green cold edge to a
/// dry olive and a wet green at the hot edge. The layout is the one the
/// client reads: x = (1 - temperature) * 255, y = (1 - downfall * temperature)
/// * 255. Each kind is `(cold, hot and dry, hot and wet)`.
pub fn colormap(kind: &str) -> Option<Rgba> {
    let [cold, dry, wet]: [[f32; 3]; 3] = match kind {
        "grass" => [
            [128.0, 180.0, 151.0],
            [191.0, 183.0, 85.0],
            [71.0, 205.0, 51.0],
        ],
        "foliage" => [
            [96.0, 161.0, 123.0],
            [174.0, 164.0, 42.0],
            [26.0, 191.0, 0.0],
        ],
        _ => return None,
    };
    let mut image = Rgba::new(256, 256);
    for y in 0..256 {
        for x in 0..256 {
            let temperature = 1.0 - x as f32 / 255.0;
            let humidity = 1.0 - y as f32 / 255.0;
            let downfall = if temperature > 0.0 {
                (humidity / temperature).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let channel = |i: usize| {
                let hot = dry[i] + (wet[i] - dry[i]) * downfall;
                (cold[i] + (hot - cold[i]) * temperature).round() as u8
            };
            image.set(x, y, [channel(0), channel(1), channel(2), 255]);
        }
    }
    Some(image)
}

/// Dark, coarse, high-contrast stone: unmistakably the world's floor.
fn bedrock() -> Rgba {
    let mut image = Rgba::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            // Two octaves of cell noise: 4x4 blobs with 2x2 detail.
            let coarse = noise(x / 4, y / 4, 11) % 100;
            let fine = noise(x / 2, y / 2, 23) % 100;
            let grain = noise(x, y, 37) % 18;
            let v = match (coarse + fine) / 2 {
                0..=24 => 34,
                25..=49 => 58,
                50..=74 => 86,
                _ => 118,
            } + grain;
            let v = v.min(255) as u8;
            image.set(x, y, [v, v, v.saturating_add(4), 255]);
        }
    }
    image
}

/// The ten cracking overlays: dark cracks that grow from the centre.
pub fn destroy_stage(stage: u32) -> Rgba {
    let mut image = Rgba::new(16, 16);
    // Two pixels at the first stage, reaching the edge at the last.
    let reach = 2 + stage as i32 * 7 / 9 + stage as i32 / 2;
    // Eight crack arms from the centre, each a stepped random walk.
    for arm in 0..8u32 {
        let (mut x, mut y) = (8i32, 8i32);
        let (dx, dy) = [
            (1, 0),
            (1, 1),
            (0, 1),
            (-1, 1),
            (-1, 0),
            (-1, -1),
            (0, -1),
            (1, -1),
        ][arm as usize];
        for step in 0..reach {
            if !(0..16).contains(&x) || !(0..16).contains(&y) {
                break;
            }
            let alpha = 150u8.saturating_add((stage * 10) as u8);
            image.set(x as u32, y as u32, [20, 20, 20, alpha]);
            let jitter = noise(arm, step as u32, 91) % 3;
            match jitter {
                0 => x += dx,
                1 => y += dy,
                _ => {
                    x += dx;
                    y += dy;
                }
            }
        }
    }
    image
}

/// Interface strings this edition adds to the translations.
pub fn ui_strings() -> Vec<(&'static str, &'static str)> {
    vec![
        ("open.title", "Horizon Open Creative"),
        (
            "open.session_notice",
            "This world lasts for this visit only: reloading the page starts a fresh world.",
        ),
        ("open.credits", "Credits"),
    ]
}

/// The interface, sky and player artwork.
pub fn write(bundle: &mut Bundle) -> Result<()> {
    let art = Origin::Generator(GENERATOR);
    bundle.png(
        "assets/minecraft/textures/entity/player/wide/steve.png",
        &skin(),
        art.clone(),
    )?;
    // The player's model, and every mob's (crate::mobs).
    let (mobs, types) = crate::mobs::write(bundle)?;
    let mut models = entity_models();
    models.as_object_mut().expect("an object").extend(mobs);
    bundle.json("entity-models.json", &models, GENERATOR)?;
    bundle.json(
        "entity-types.json",
        &serde_json::Value::Object(types),
        GENERATOR,
    )?;
    gui(bundle)?;
    sky(bundle)?;
    crate::art_survival::write(bundle)?;
    crate::screens::write(bundle)?;
    crate::art_advancements::write(bundle)?;
    Ok(())
}

// Horizon's design tokens (crates/horizon-theme), dark and light.
const PAGE: [u8; 4] = [0x0d, 0x0e, 0x13, 255];
const PLATE: [u8; 4] = [0x20, 0x22, 0x2c, 255];
const PLATE_SOFT: [u8; 4] = [0x18, 0x1a, 0x22, 255];
const RULE: [u8; 4] = [0x3a, 0x3d, 0x4a, 255];
const INK: [u8; 4] = [0xec, 0xed, 0xf3, 255];
const MUTED: [u8; 4] = [0x8b, 0x8f, 0x9c, 255];
const ACCENT: [u8; 4] = [0x9a, 0x86, 0xff, 255];
const LIGHT_PAGE: [u8; 4] = [0xf3, 0xf3, 0xf8, 255];
const LIGHT_PLATE: [u8; 4] = [0xd6, 0xd8, 0xe4, 255];
const LIGHT_RULE: [u8; 4] = [0xa9, 0xac, 0xbb, 255];
const LIGHT_SHADE: [u8; 4] = [0x8a, 0x8e, 0x9e, 255];

fn with_alpha(c: [u8; 4], a: u8) -> [u8; 4] {
    [c[0], c[1], c[2], a]
}

/// A filled box with a one-pixel border.
fn framed(w: u32, h: u32, fill: [u8; 4], border: [u8; 4]) -> Rgba {
    let mut image = Rgba::filled(w, h, border);
    image.fill(1, 1, w - 2, h - 2, fill);
    image
}

/// A recessed slot: dark top-left edge, light bottom-right edge.
fn slot(image: &mut Rgba, x: u32, y: u32, size: u32) {
    image.fill(x, y, size, size, LIGHT_SHADE);
    image.fill(x + 1, y + 1, size - 1, size - 1, [0xff, 0xff, 0xff, 255]);
    image.fill(x + 1, y + 1, size - 2, size - 2, [0xc4, 0xc7, 0xd4, 255]);
}

fn nine_slice(width: u32, height: u32, border: u32) -> serde_json::Value {
    serde_json::json!({"gui": {"scaling": {"type": "nine_slice", "width": width, "height": height, "border": border}}})
}

/// The HUD, widgets, slots, tooltips and the creative inventory.
fn gui(bundle: &mut Bundle) -> Result<()> {
    let art = || Origin::Generator(GENERATOR);
    let sprites = "assets/minecraft/textures/gui/sprites";
    let put = |bundle: &mut Bundle, name: &str, image: &Rgba| -> Result<()> {
        bundle.png(format!("{sprites}/{name}.png"), image, art())
    };

    // The hotbar: nine cells on a translucent plate.
    let mut hotbar = Rgba::filled(182, 22, with_alpha(PAGE, 170));
    for i in 0..9 {
        let x = 1 + i * 20;
        hotbar.fill(x, 1, 20, 20, with_alpha(PLATE_SOFT, 200));
        hotbar.fill(x + 1, 2, 18, 18, with_alpha(PLATE, 200));
        hotbar.fill(x + 1, 20, 18, 1, with_alpha(RULE, 220));
    }
    put(bundle, "hud/hotbar", &hotbar)?;
    let mut selection = Rgba::new(24, 23);
    for (x, y, w, h) in [(0, 0, 24, 2), (0, 21, 24, 2), (0, 0, 2, 23), (22, 0, 2, 23)] {
        selection.fill(x, y, w, h, ACCENT);
    }
    for (x, y) in [(0, 0), (23, 0), (0, 22), (23, 22)] {
        selection.set(x, y, [0, 0, 0, 0]);
    }
    put(bundle, "hud/hotbar_selection", &selection)?;
    for side in ["left", "right"] {
        let mut offhand = Rgba::new(29, 24);
        offhand.blit(
            &framed(22, 22, with_alpha(PLATE, 200), with_alpha(PAGE, 170)),
            if side == "left" { 0 } else { 7 },
            1,
        );
        put(bundle, &format!("hud/hotbar_offhand_{side}"), &offhand)?;
    }
    // The crosshair: a small plus with a dark outline.
    let mut crosshair = Rgba::new(15, 15);
    for i in 3..12 {
        for (x, y) in [(7, i), (i, 7)] {
            crosshair.set(x, y, [255, 255, 255, 230]);
        }
    }
    crosshair.set(7, 7, [255, 255, 255, 120]);
    put(bundle, "hud/crosshair", &crosshair)?;

    // Buttons, fields and sliders: 200x20, nine-sliced.
    let button = |fill: [u8; 4], border: [u8; 4]| {
        let mut b = framed(200, 20, fill, border);
        b.fill(1, 18, 198, 1, with_alpha(PAGE, 255));
        b
    };
    let widgets = [
        ("widget/button", button(PLATE, RULE), 200, 20, 3),
        (
            "widget/button_highlighted",
            button([0x2b, 0x2a, 0x40, 255], ACCENT),
            200,
            20,
            3,
        ),
        (
            "widget/button_disabled",
            button(PLATE_SOFT, PLATE),
            200,
            20,
            3,
        ),
        (
            "widget/text_field",
            framed(200, 20, PAGE, MUTED),
            200,
            20,
            1,
        ),
        (
            "widget/text_field_highlighted",
            framed(200, 20, PAGE, INK),
            200,
            20,
            1,
        ),
        ("widget/slider", framed(200, 20, PAGE, RULE), 200, 20, 2),
        (
            "widget/slider_highlighted",
            framed(200, 20, PAGE, MUTED),
            200,
            20,
            2,
        ),
        ("widget/slider_handle", framed(8, 20, MUTED, RULE), 8, 20, 2),
        (
            "widget/slider_handle_highlighted",
            framed(8, 20, INK, ACCENT),
            8,
            20,
            2,
        ),
        ("widget/scroller", framed(6, 32, MUTED, RULE), 6, 32, 1),
        (
            "widget/scroller_background",
            framed(6, 32, PAGE, PLATE),
            6,
            32,
            1,
        ),
        (
            "tooltip/background",
            Rgba::filled(32, 32, with_alpha(PAGE, 235)),
            32,
            32,
            4,
        ),
        (
            "tooltip/frame",
            {
                let mut f = Rgba::new(32, 32);
                for (x, y, w, h) in [(3, 3, 26, 1), (3, 28, 26, 1), (3, 3, 1, 26), (28, 3, 1, 26)] {
                    f.fill(x, y, w, h, with_alpha(ACCENT, 200));
                }
                f
            },
            32,
            32,
            4,
        ),
    ];
    for (name, image, w, h, border) in widgets {
        put(bundle, name, &image)?;
        bundle.json(
            format!("{sprites}/{name}.png.mcmeta"),
            &nine_slice(w, h, border),
            GENERATOR,
        )?;
    }
    let mut slot18 = Rgba::new(18, 18);
    slot(&mut slot18, 0, 0, 18);
    put(bundle, "container/slot", &slot18)?;

    // The creative inventory's panels: 195x136 on 256x256 textures.
    let panel = |kind: &str| {
        let mut image = Rgba::new(256, 256);
        image.fill(0, 0, 195, 136, LIGHT_RULE);
        image.fill(1, 1, 193, 134, LIGHT_PAGE);
        image.fill(2, 133, 191, 1, LIGHT_PLATE);
        match kind {
            "items" | "search" => {
                for row in 0..5 {
                    for column in 0..9 {
                        slot(&mut image, 8 + column * 18, 17 + row * 18, 18);
                    }
                }
                // The scrollbar track.
                image.fill(174, 17, 14, 112, LIGHT_SHADE);
                image.fill(175, 18, 13, 111, [0xff, 0xff, 0xff, 255]);
                image.fill(175, 18, 12, 110, LIGHT_PLATE);
            }
            _ => {
                // The inventory tab: armour, off hand, the player box and the trash.
                for i in 0..4 {
                    slot(&mut image, 53 + (i / 2) * 54, 5 + (i % 2) * 27, 18);
                }
                slot(&mut image, 34, 19, 18);
                image.fill(73, 5, 32, 43, LIGHT_SHADE);
                image.fill(74, 6, 30, 41, [0x22, 0x24, 0x2e, 255]);
                for row in 0..3 {
                    for column in 0..9 {
                        slot(&mut image, 8 + column * 18, 53 + row * 18, 18);
                    }
                }
                slot(&mut image, 172, 111, 18);
                // A small cross on the trash slot.
                for i in 0..8 {
                    image.set(177 + i, 116 + i, [0xb4, 0x37, 0x2a, 255]);
                    image.set(184 - i, 116 + i, [0xb4, 0x37, 0x2a, 255]);
                }
            }
        }
        if kind == "search" {
            image.fill(80, 4, 90, 12, LIGHT_SHADE);
            image.fill(81, 5, 88, 10, [0xff, 0xff, 0xff, 255]);
        }
        for column in 0..9 {
            slot(&mut image, 8 + column * 18, 111, 18);
        }
        image
    };
    for (file, kind) in [
        ("tab_items", "items"),
        ("tab_item_search", "search"),
        ("tab_inventory", "inventory"),
    ] {
        bundle.png(
            format!("assets/minecraft/textures/gui/container/creative_inventory/{file}.png"),
            &panel(kind),
            art(),
        )?;
    }
    for row in ["top", "bottom"] {
        for selected in [true, false] {
            for column in 1..=7 {
                let mut tab = Rgba::new(26, 32);
                let fill = if selected { LIGHT_PAGE } else { LIGHT_PLATE };
                let (y0, y1) = if row == "top" {
                    (0, if selected { 32 } else { 28 })
                } else {
                    (if selected { 0 } else { 4 }, 32)
                };
                tab.fill(0, y0, 26, y1 - y0, LIGHT_RULE);
                let inner_top = if row == "top" { y0 + 1 } else { y0 };
                let inner_bottom = if row == "top" { y1 } else { y1 - 1 };
                tab.fill(1, inner_top, 24, inner_bottom - inner_top, fill);
                if !selected {
                    // Unselected tabs end at the panel's edge.
                    if row == "top" {
                        tab.fill(0, y1 - 1, 26, 1, LIGHT_RULE);
                    } else {
                        tab.fill(0, y0, 26, 1, LIGHT_RULE);
                    }
                }
                put(
                    bundle,
                    &format!(
                        "container/creative_inventory/tab_{row}_{}_{column}",
                        if selected { "selected" } else { "unselected" }
                    ),
                    &tab,
                )?;
            }
        }
    }
    put(
        bundle,
        "container/creative_inventory/scroller",
        &framed(12, 15, ACCENT, [0x4a, 0x3a, 0x9a, 255]),
    )?;
    put(
        bundle,
        "container/creative_inventory/scroller_disabled",
        &framed(12, 15, LIGHT_SHADE, LIGHT_RULE),
    )?;

    // Menus: a dark backdrop and its rules.
    bundle.png(
        "assets/minecraft/textures/gui/menu_background.png",
        &Rgba::filled(16, 16, with_alpha(PAGE, 200)),
        art(),
    )?;
    for name in ["header_separator", "footer_separator"] {
        let mut rule = Rgba::new(32, 2);
        rule.fill(0, 0, 32, 1, RULE);
        bundle.png(
            format!("assets/minecraft/textures/gui/{name}.png"),
            &rule,
            art(),
        )?;
    }
    // The text: Pixelify Sans (OFL), with a plain space.
    bundle.json(
        "assets/minecraft/font/default.json",
        &serde_json::json!({"providers": [
            {"type": "space", "advances": {" ": 4.0, "\u{200c}": 0.0}},
            {"type": "ttf", "file": "minecraft:open/pixelify_sans.ttf", "size": 10.0, "oversample": 4.0, "shift": [0.0, 1.0]},
        ]}),
        GENERATOR,
    )?;
    Ok(())
}

/// The sun, the moon and the clouds.
fn sky(bundle: &mut Bundle) -> Result<()> {
    let art = || Origin::Generator(GENERATOR);
    // A soft-edged warm disc.
    let mut sun = Rgba::new(32, 32);
    for y in 0..32 {
        for x in 0..32 {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            let d = (dx * dx + dy * dy).sqrt();
            if d < 9.0 {
                sun.set(x, y, [255, 246, 196, 255]);
            } else if d < 12.0 {
                sun.set(x, y, [255, 214, 120, (255.0 * (12.0 - d) / 3.0) as u8]);
            }
        }
    }
    bundle.png(
        "assets/minecraft/textures/environment/celestial/sun.png",
        &sun,
        art(),
    )?;
    // Eight moon phases: a disc with a sliding shadow.
    for (phase, name) in [
        "full_moon",
        "waning_gibbous",
        "third_quarter",
        "waning_crescent",
        "new_moon",
        "waxing_crescent",
        "first_quarter",
        "waxing_gibbous",
    ]
    .iter()
    .enumerate()
    {
        let mut moon = Rgba::new(32, 32);
        let shadow = phase as f32 / 4.0 - 1.0;
        for y in 0..32 {
            for x in 0..32 {
                let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
                if dx * dx + dy * dy < 81.0 {
                    let lit = phase == 0
                        || (phase != 4
                            && if phase < 4 {
                                dx / 9.0 < -shadow
                            } else {
                                dx / 9.0 > shadow - 1.0
                            });
                    let n = (noise(x, y, 61) % 20) as u8;
                    let c = if lit {
                        [220 - n, 224 - n, 236 - n, 255]
                    } else {
                        [40, 44, 60, 160]
                    };
                    moon.set(x, y, c);
                }
            }
        }
        bundle.png(
            format!("assets/minecraft/textures/environment/celestial/moon/{name}.png"),
            &moon,
            art(),
        )?;
    }
    // Clouds: a 256x256 cell map of soft blobs.
    let mut clouds = Rgba::new(256, 256);
    for y in 0..256 {
        for x in 0..256 {
            let coarse = noise(x / 16, y / 16, 71) % 100;
            let fine = noise(x / 4, y / 4, 73) % 100;
            if coarse * 2 + fine > 210 {
                clouds.set(x, y, [255, 255, 255, 255]);
            }
        }
    }
    bundle.png(
        "assets/minecraft/textures/environment/clouds.png",
        &clouds,
        art(),
    )?;
    Ok(())
}

/// A cuboid in model units (1/16 block) with box UV from `uv` on a 64x64
/// texture: six outward-facing quads `[x, y, z, u, v]` and their normals.
fn cuboid(
    min: [f32; 3],
    size: [f32; 3],
    uv: [f32; 2],
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = [min[0] + size[0], min[1] + size[1], min[2] + size[2]];
    let [w, h, d] = size;
    let [u, v] = uv;
    let t = |a: f32| a / 64.0;
    // Each face: corners counter-clockwise seen from outside, and its UV rect
    // (u0, v0, u1, v1) in box layout: top/bottom row, then the side strip.
    let faces: [([[f32; 3]; 4], [f32; 4], [f32; 3]); 6] = [
        // -Y (bottom of the part in model space, which is up on screen: model Y grows downward)
        (
            [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
            [u + d, v, u + d + w, v + d],
            [0.0, -1.0, 0.0],
        ),
        // +Y
        (
            [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
            [u + d + w, v, u + d + w + w, v + d],
            [0.0, 1.0, 0.0],
        ),
        // -Z (front)
        (
            [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
            [u + d, v + d, u + d + w, v + d + h],
            [0.0, 0.0, -1.0],
        ),
        // +Z (back)
        (
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            [u + d + w + d, v + d, u + d + w + d + w, v + d + h],
            [0.0, 0.0, 1.0],
        ),
        // -X
        (
            [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
            [u + d + w, v + d, u + d + w + d, v + d + h],
            [-1.0, 0.0, 0.0],
        ),
        // +X
        (
            [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
            [u, v + d, u + d, v + d + h],
            [1.0, 0.0, 0.0],
        ),
    ];
    let mut polygons = Vec::new();
    let mut normals = Vec::new();
    for (corners, [u0, v0, u1, v1], normal) in faces {
        // Corners 0..3 take the rect's (u1,v0) (u0,v0) (u0,v1) (u1,v1).
        let uvs = [[u1, v0], [u0, v0], [u0, v1], [u1, v1]];
        let quad: Vec<serde_json::Value> = corners
            .iter()
            .zip(uvs)
            .map(|(c, [uu, vv])| serde_json::json!([c[0], c[1], c[2], t(uu), t(vv)]))
            .collect();
        // Keep the winding consistent with the outward normal.
        let a = corners[0];
        let (b, c) = (corners[1], corners[2]);
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cross = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let dot = cross[0] * normal[0] + cross[1] * normal[1] + cross[2] * normal[2];
        let quad = if dot < 0.0 {
            quad.into_iter().rev().collect()
        } else {
            quad
        };
        polygons.push(serde_json::Value::Array(quad));
        normals.push(serde_json::json!(normal));
    }
    (polygons, normals)
}

fn part(pivot: [f32; 3], min: [f32; 3], size: [f32; 3], uv: [f32; 2]) -> serde_json::Value {
    let (polygons, normals) = cuboid(min, size, uv);
    serde_json::json!({
        "pose": [pivot[0], pivot[1], pivot[2], 0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        "polygons": polygons,
        "normals": normals,
        "children": {},
    })
}

/// The player's model: a plain humanoid of boxes (head, body, arms, legs).
fn entity_models() -> serde_json::Value {
    serde_json::json!({
        "minecraft:player#main": {
            "pose": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            "polygons": [],
            "children": {
                "head": part([0.0, 0.0, 0.0], [-4.0, -8.0, -4.0], [8.0, 8.0, 8.0], [0.0, 0.0]),
                "body": part([0.0, 0.0, 0.0], [-4.0, 0.0, -2.0], [8.0, 12.0, 4.0], [16.0, 16.0]),
                "right_arm": part([-5.0, 2.0, 0.0], [-3.0, -2.0, -2.0], [4.0, 12.0, 4.0], [40.0, 16.0]),
                "left_arm": part([5.0, 2.0, 0.0], [-1.0, -2.0, -2.0], [4.0, 12.0, 4.0], [32.0, 48.0]),
                "right_leg": part([-1.9, 12.0, 0.0], [-2.0, 0.0, -2.0], [4.0, 12.0, 4.0], [0.0, 16.0]),
                "left_leg": part([1.9, 12.0, 0.0], [-2.0, 0.0, -2.0], [4.0, 12.0, 4.0], [16.0, 48.0]),
            },
        },
    })
}

/// The player's skin: Horizon's own explorer, a teal jacket over warm skin,
/// laid out for the box UVs in [`entity_models`].
fn skin() -> Rgba {
    let mut image = Rgba::new(64, 64);
    let shade = |c: [u8; 3], x: u32, y: u32, seed: u32| {
        let n = (noise(x, y, seed) % 14) as i32 - 7;
        let f = |v: u8| (v as i32 + n).clamp(0, 255) as u8;
        [f(c[0]), f(c[1]), f(c[2]), 255]
    };
    let skin = [214, 168, 128];
    let jacket = [32, 128, 136];
    let cuff = [24, 92, 100];
    let trousers = [58, 64, 88];
    let hair = [70, 46, 30];
    let paint = |image: &mut Rgba,
                 x0: u32,
                 y0: u32,
                 w: u32,
                 h: u32,
                 f: &dyn Fn(u32, u32) -> [u8; 3],
                 seed: u32| {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                image.set(x, y, shade(f(x - x0, y - y0), x, y, seed));
            }
        }
    };
    // Head (8x8x8 at 0,0): hair on top and the upper face, skin below.
    paint(
        &mut image,
        0,
        0,
        32,
        16,
        &|x, y| {
            if y < 10 && (y < 8 || x < 32) {
                hair
            } else {
                skin
            }
        },
        3,
    );
    // Eyes on the front face (u 8..16, v 8..16).
    for (x, y) in [(10, 12), (13, 12)] {
        image.set(x, y, [30, 40, 60, 255]);
    }
    // Body (16,16): the jacket with a lighter zip line.
    paint(
        &mut image,
        16,
        16,
        24,
        16,
        &|x, _| {
            if x == 7 || x == 8 {
                [60, 160, 168]
            } else {
                jacket
            }
        },
        5,
    );
    // Arms (40,16 and 32,48): sleeves, cuffs, bare hands at the end.
    for (u, v) in [(40u32, 16u32), (32, 48)] {
        paint(
            &mut image,
            u,
            v,
            16,
            16,
            &|_, y| match y {
                0..=3 => jacket,
                4..=11 => jacket,
                12..=12 => cuff,
                _ => skin,
            },
            7,
        );
    }
    // Legs (0,16 and 16,48).
    for (u, v) in [(0u32, 16u32), (16, 48)] {
        paint(
            &mut image,
            u,
            v,
            16,
            16,
            &|_, y| if y >= 14 { [40, 34, 30] } else { trousers },
            9,
        );
    }
    image
}
