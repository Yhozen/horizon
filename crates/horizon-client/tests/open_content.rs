//! Horizon Open Creative (docs/open-creative.md): the bundle `horizon-prepare
//! open` writes loads through every shared loader the open edition uses, with
//! no Minecraft files anywhere, and exposes only its own content.
use horizon_client::items::ItemStack;
use horizon_client::{
    assets::Assets, biome::Biomes, blocks::BlockFacts, collisions, crafting::Recipes,
    creative::Creative, environment::Environment, hud::HudAtlas, item_render::ItemRenderer,
    items::Items, light::LightTables, motion::MotionResources,
};
use horizon_core::{content_profile::ContentProfile, position::BlockPos};
use std::path::{Path, PathBuf};

/// The palette's blocks and items, as the generator defines them.
fn palette_blocks() -> Vec<horizon_open_content::blocks::Block> {
    horizon_open_content::blocks::defined()
        .into_iter()
        .filter(|b| b.has_item())
        .collect()
}

fn bundle(name: &str) -> PathBuf {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = std::env::temp_dir().join(format!("horizon-open-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let built = horizon_open_content::build(&horizon_open_content::Inputs::at(&repo)).unwrap();
    horizon_open_content::write(&built, &out).unwrap();
    out
}

#[test]
fn the_open_bundle_loads_through_the_shared_loaders() {
    let root = bundle("loaders");
    let assets = Assets::load(&root).unwrap();
    assert_eq!(assets.profile, ContentProfile::OpenCreative);
    let render = assets.reload(&root).unwrap();
    // Every palette block bakes faces, each on a sprite the bundle has:
    // six for cubes, at least two for plants (a lily pad's two sides), none
    // for the blocks the block-entity renderer draws.
    let palette = palette_blocks();
    assert_eq!(palette.len(), 1009);
    for block in &palette {
        let name = block.id;
        let state = assets
            .registry
            .default_state(&format!("minecraft:{name}"))
            .unwrap();
        let models = render
            .model_catalog
            .collect(state, BlockPos::new(0, 0, 0))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let faces: usize = models.iter().map(|(_, m)| m.faces.len()).sum();
        use horizon_open_content::{blocks::Form, connected::Family};
        let least = match block.form {
            // Chests and shulker boxes are drawn by the block-entity
            // renderer, as Minecraft's are: their block models bake nothing.
            _ if name.ends_with("chest") || name.ends_with("shulker_box") => 0,
            Form::Cross => 2,
            // Thin shaped blocks (rails, ladders, carpets...) may bake as
            // little as one flat plane's two sides.
            Form::Shaped => 2,
            // A door's lower half has no top face.
            Form::Connected(Family::Door) => 5,
            _ => 6,
        };
        assert!(faces >= least, "{name} bakes {faces} faces");
    }
    for sprite in render.sprite_layers.keys() {
        let path = sprite.trim_start_matches("minecraft:");
        assert!(
            root.join(format!("assets/minecraft/textures/{path}.png"))
                .exists(),
            "sprite {sprite} has no texture in the bundle"
        );
    }
    let (facts, behaviours) = BlockFacts::load_pair(&root, assets.registry).unwrap();
    let glowstone = assets
        .registry
        .default_state("minecraft:glowstone")
        .unwrap();
    assert_eq!(facts.get(glowstone).emission, 15);
    let _ = behaviours;
    let biomes = Biomes::load(&root).unwrap();
    assert_eq!(biomes.list.len(), 1);
    Environment::load(&root).unwrap();
    LightTables::load(&root, assets.registry, &facts).unwrap();
    collisions::load(&root, assets.registry).unwrap();
    MotionResources::load_with_facts(&root, &assets, &facts).unwrap();
    let items = Items::load(&root, assets.registry, &facts).unwrap();
    for block in &palette {
        assert!(items.get(&block.key()).is_some(), "no item {}", block.id);
    }
    for item in horizon_open_content::items::all() {
        assert!(items.get(&item.key()).is_some(), "no item {}", item.id);
    }
    // Progress from punching trees: logs to planks to sticks and a table,
    // then wooden and stone tools. Stone needs a pickaxe to drop anything.
    let recipes = Recipes::load(&root, &items).unwrap();
    let stack = |id: &str| Some(ItemStack::new(id, 1));
    let planks = recipes
        .craft(&[stack("minecraft:oak_log"), None, None, None], 2)
        .unwrap();
    assert_eq!((&*planks.item, planks.count), ("minecraft:oak_planks", 4));
    let sticks = recipes
        .craft(
            &[
                stack("minecraft:spruce_planks"),
                None,
                stack("minecraft:oak_planks"),
                None,
            ],
            2,
        )
        .unwrap();
    assert_eq!((&*sticks.item, sticks.count), ("minecraft:stick", 4));
    let p = || stack("minecraft:oak_planks");
    let table = recipes.craft(&[p(), p(), p(), p()], 2).unwrap();
    assert_eq!(&*table.item, "minecraft:crafting_table");
    let s = || stack("minecraft:stick");
    let c = || stack("minecraft:cobblestone");
    let pickaxe = recipes
        .craft(&[c(), c(), c(), None, s(), None, None, s(), None], 3)
        .unwrap();
    assert_eq!(&*pickaxe.item, "minecraft:stone_pickaxe");
    let wooden = ItemStack::new("minecraft:wooden_pickaxe", 1);
    assert!(!items.correct_for_drops(None, "minecraft:stone"));
    assert!(items.correct_for_drops(Some(&wooden), "minecraft:stone"));
    assert!(!items.correct_for_drops(Some(&wooden), "minecraft:iron_ore"));
    assert!(items.correct_for_drops(Some(&pickaxe), "minecraft:iron_ore"));
    assert!(items.mining_speed(Some(&wooden), "minecraft:stone") > 1.0);
    assert!(
        facts
            .get(assets.registry.default_state("minecraft:stone").unwrap())
            .requires_tool
    );
    let mut random = horizon_core::random::LegacyRandom::new(1);
    let stone = assets.registry.default_state("minecraft:stone").unwrap();
    let drops = items.block_drops(assets.registry, stone, Some(&wooden), &mut random);
    assert_eq!(drops.len(), 1);
    assert_eq!(&*drops[0].item, "minecraft:cobblestone");
    let apple = items.get("minecraft:apple").unwrap();
    assert_eq!(apple.food, Some((4, 2.4)));
    ItemRenderer::load(&root).unwrap();
    let atlas = HudAtlas::build_over(&root, &root).unwrap();
    for sprite in [
        "gui/sprites/hud/hotbar",
        "gui/sprites/hud/hotbar_selection",
        "gui/sprites/hud/crosshair",
        "gui/sprites/widget/button",
        "entity/player/wide/steve",
        "gui/sprites/hud/heart/container",
        "gui/sprites/hud/heart/full",
        "gui/sprites/hud/heart/half",
        "gui/sprites/hud/food_full",
        "gui/sprites/hud/air",
        "gui/sprites/hud/armor_full",
        "gui/sprites/hud/experience_bar_background",
        "gui/sprites/hud/experience_bar_progress",
        "gui/sprites/recipe_book/button",
        "gui/container/inventory",
        "gui/container/crafting_table",
        "gui/recipe_book",
        "item/stick",
        "item/wooden_pickaxe",
    ] {
        assert!(
            atlas.sprites.contains_key(sprite),
            "the interface atlas lacks {sprite}"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_palette_offers_exactly_the_generated_blocks_and_items() {
    let root = bundle("palette");
    let creative = Creative::load(&root).unwrap();
    let mut offered: Vec<String> = creative
        .tabs
        .iter()
        .filter(|t| !t.search && !t.inventory)
        .flat_map(|t| t.items.iter().map(|s| s.item.to_string()))
        .collect();
    offered.sort();
    let mut expected: Vec<String> = palette_blocks()
        .iter()
        .map(|b| b.key())
        .chain(horizon_open_content::items::all().iter().map(|i| i.key()))
        .collect();
    expected.sort();
    assert_eq!(offered, expected);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn damaged_open_bundles_fail_with_what_to_do() {
    let root = bundle("damaged");
    // A revision this build does not read.
    let manifest = root.join("horizon-resources.json");
    let original = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        original.replace("\"content_revision\": 1", "\"content_revision\": 999"),
    )
    .unwrap();
    let error = Assets::load(&root).err().unwrap().to_string();
    assert!(error.contains("horizon-prepare open"), "{error}");
    std::fs::write(&manifest, &original).unwrap();
    // A missing texture fails the bundle's verification by name.
    std::fs::remove_file(root.join("assets/minecraft/textures/block/stone.png")).unwrap();
    let error = horizon_open_content::verify(&root).unwrap_err();
    assert!(error.contains("stone.png"), "{error}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn every_open_mob_loads_with_its_models_and_skins() {
    use horizon_client::mobs::{Mob, MobTypes, render};
    let root = bundle("mobs");
    let types = MobTypes::load(&root).unwrap();
    // The boats, minecarts, arrows and armor stands have renderers of their own.
    let objects = horizon_open_content::creatures::object_names();
    let mut names = horizon_open_content::mobs::names();
    names.retain(|n| !objects.iter().any(|o| o == n));
    assert_eq!((names.len(), objects.len()), (83, 32));
    for name in names {
        let id = format!("minecraft:{name}");
        let kind = std::sync::Arc::new(
            types
                .types
                .get(&id)
                .unwrap_or_else(|| panic!("{id} does not load"))
                .clone(),
        );
        // Adult and baby, each climate variant, a dyed sheep: whatever the renderer
        // picks is in the bundle.
        for (age, variant, color) in [
            (0, 0, 0),
            (0, 1, 0),
            (0, 2, 3),
            (-24000, 0, 0),
            (-24000, 2, 0),
        ] {
            let mut m = Mob::new(1, kind.clone(), horizon_core::geometry::Vec3::ZERO, 0.0);
            (m.extra.age, m.extra.variant, m.extra.color) = (age, variant, color);
            let (model, texture) = render::body(&m);
            let layers = render::layers(&m);
            let drawn = std::iter::once((model, texture))
                .chain(layers.into_iter().map(|l| (l.model, l.texture)))
                // Layers the open edition doesn't draw (a drowned's outer layer) are
                // skipped by the renderer when the model is missing.
                .filter(|(model, _)| kind.layer(model).is_some());
            let mut any = false;
            for (model, texture) in drawn {
                any = true;
                let png = root.join(format!("assets/minecraft/textures/{texture}.png"));
                assert!(png.is_file(), "{id} ({model}): missing {}", png.display());
            }
            assert!(any, "{id} draws nothing");
        }
    }
}
