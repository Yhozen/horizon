//! Guests' saved state: `players/data/<uuid>.dat` in the vanilla player
//! compound, through the same code that saves the host.
use super::guest::{Guest, SavedBody};
use crate::{
    items::ItemStack,
    save::{Level, read_player_dat},
};
#[allow(unused_imports)]
use horizon_core::fs::PathExt as _;
use horizon_protocol::crypto::uuid_string;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub fn path(world: &Path, uuid: u128) -> PathBuf {
    world
        .join("players")
        .join("data")
        .join(format!("{}.dat", uuid_string(uuid)))
}

/// `UUIDUtil.uuidToIntArray`.
pub fn uuid_ints(uuid: u128) -> [i32; 4] {
    [
        (uuid >> 96) as i32,
        (uuid >> 64) as i32,
        (uuid >> 32) as i32,
        uuid as i32,
    ]
}

/// Whether a saved file exists for this player.
pub fn exists(world: &Path, uuid: u128) -> bool {
    path(world, uuid).is_file_vfs()
}

/// Load a guest's saved state, if they have been here before.
pub fn load(guest: &mut Guest, world: &Path) -> bool {
    let Some(tag) = read_player_dat(&path(world, guest.uuid)) else {
        return false;
    };
    let mut level = Level {
        health: 20.0,
        food: 20,
        saturation: 5.0,
        ..Level::default()
    };
    level.read_player(&tag);
    level.player_extra = Some(tag);
    apply(guest, &level);
    true
}

/// The integrated owner loads the named file before their own file; other
/// players are never candidates. Legacy Data.Player remains the last fallback.
pub fn load_owner(guest: &mut Guest, world: &Path, saved: Option<&Level>) -> bool {
    if let Some(level) = saved.filter(|l| l.singleplayer_file) {
        apply(guest, level);
        return level.position.is_some();
    }
    if load(guest, world) {
        return true;
    }
    saved.is_some_and(|level| {
        apply(guest, level);
        level.position.is_some()
    })
}

/// Leaving must update the same named file that the next owner join loads.
pub fn save_owner(guest: &Guest, level: &mut Level, world: &Path) {
    level.player_uuid.get_or_insert(uuid_ints(guest.uuid));
    store(guest, level);
    if let Err(e) = level.write_player(world) {
        log::warn!("Horizon: the owner's playerdata is not saved: {e}");
    }
    if level.player_uuid != Some(uuid_ints(guest.uuid)) {
        save(guest, world);
    }
}

/// Give a guest the player state read into `level`.
pub fn apply(guest: &mut Guest, level: &Level) {
    if let Some(p) = level.position {
        guest.position = p;
        guest.previous_position = p;
    }
    guest.yaw = level.rotation[0];
    guest.pitch = level.rotation[1];
    guest.head_yaw = guest.yaw;
    guest.on_ground = level.on_ground;
    guest.health = level.health;
    guest.saved_absorption = Some(level.absorption);
    guest.food = level.food;
    guest.saturation = level.saturation;
    guest.xp_level = level.xp_level;
    guest.xp_progress = level.xp_progress;
    guest.xp_total = level.xp_total;
    guest.xp_seed = level.xp_seed;
    guest.effects = crate::effects::PlayerEffects::from_saved(&level.active_effects);
    guest.game_mode = level.mode();
    guest.flying = level.flying;
    guest.selected = (level.selected_slot.max(0) as usize).min(8);
    // A point that is the world spawn is none of the player's own: it
    // respawns wherever the world spawn is then (`setworldspawn`).
    guest.spawn = level
        .spawn
        .filter(|s| Some(s.map(|v| v.floor() as i32)) != level.world_spawn)
        .map(|s| [s[0] + 0.5, s[1], s[2] + 0.5]);
    guest.respawn_elsewhere = level.respawn_elsewhere.clone();
    guest.saved = level.player_extra.clone();
    let extra = level.player_extra.as_ref();
    let short = |key: &str, default: i64| {
        extra
            .and_then(|t| crate::save::number(t.get(key)))
            .unwrap_or(default) as i32
    };
    guest.loaded_body = Some(SavedBody {
        fire_ticks: short("Fire", -20),
        air: short("Air", 300),
        fall_distance: level.fall_distance,
    });
    guest.recipe_book = level.recipe_book.clone();
    guest.dimension = match level.dimension.as_str() {
        d @ ("minecraft:the_nether" | "minecraft:the_end") => d.to_owned(),
        _ => "minecraft:overworld".to_owned(),
    };
    if level.ender_items.len() == 27 {
        guest.ender_items = level.ender_items.clone();
    }
    for (slot, item, count, damage) in &level.inventory {
        let mut stack = ItemStack {
            item: Arc::from(item.as_str()),
            count: *count,
            damage: *damage,
            extra: level.inventory_components.get(slot).cloned(),
        };
        if stack.count == 0 {
            stack.count = 1;
        }
        match *slot {
            0..=35 => guest.inventory[usize::from(*slot)] = Some(stack),
            100 => guest.armor[3] = Some(stack),
            101 => guest.armor[2] = Some(stack),
            102 => guest.armor[1] = Some(stack),
            103 => guest.armor[0] = Some(stack),
            150 => guest.offhand = Some(stack),
            _ => {}
        }
    }
}

/// Save a guest (PlayerDataStorage.save).
pub fn save(guest: &Guest, world: &Path) {
    let mut level = Level {
        player_uuid: Some(uuid_ints(guest.uuid)),
        ..Level::default()
    };
    store(guest, &mut level);
    // Use the same recovery/preservation path as the named owner: a corrupt
    // primary must never rotate over the readable backup just loaded.
    if let Err(e) = level.write_player(world) {
        log::warn!(
            "Horizon: {} is not saved: {e}",
            path(world, guest.uuid).display()
        );
    }
}

/// Put a guest's state in `level`'s player fields.
pub fn store(guest: &Guest, level: &mut Level) {
    level.position = Some(guest.position);
    level.dimension = guest.dimension.clone();
    level.rotation = [guest.yaw, guest.pitch];
    level.health = guest.health;
    level.absorption = guest
        .motion
        .as_ref()
        .map_or(guest.saved_absorption.unwrap_or(0.0), |m| {
            m.health.absorption()
        });
    level.food = guest.food;
    level.saturation = guest.saturation;
    level.xp_level = guest.xp_level;
    level.xp_progress = guest.xp_progress;
    level.xp_total = guest.xp_total;
    level.xp_seed = guest.xp_seed;
    level.active_effects = guest.effects.saved();
    level.game_mode = guest.game_mode;
    level.creative = guest.game_mode == 1;
    level.flying = guest.flying;
    level.selected_slot = guest.selected as i32;
    level.spawn = guest.spawn.map(|s| [s[0] - 0.5, s[1], s[2] - 0.5]);
    level.respawn_elsewhere = guest.respawn_elsewhere.clone();
    level.on_ground = guest.on_ground;
    level.player_extra = guest.saved.clone();
    // The live body's fire, air and fall distance (Entity.saveWithoutId),
    // or what the file had while the player has no body yet.
    let body = guest
        .motion
        .as_deref()
        .map(SavedBody::of)
        .or(guest.loaded_body);
    if let Some(body) = body {
        level.fall_distance = body.fall_distance;
        let extra = level
            .player_extra
            .get_or_insert_with(|| horizon_world::nbt::Tag::Compound(Default::default()));
        if let horizon_world::nbt::Tag::Compound(map) = extra {
            let short = |v: i32| horizon_world::nbt::Tag::Short(v.clamp(-32768, 32767) as i16);
            map.insert("Fire".into(), short(body.fire_ticks));
            map.insert("Air".into(), short(body.air));
        }
    }
    level.recipe_book = guest.recipe_book.clone();
    level.ender_items = guest.ender_items.clone();
    level.inventory.clear();
    level.inventory_components.clear();
    let mut put = |slot: u8, stack: &Option<ItemStack>| {
        if let Some(s) = stack {
            level
                .inventory
                .push((slot, s.item.to_string(), s.count, s.damage));
            if let Some(extra) = &s.extra {
                level.inventory_components.insert(slot, extra.clone());
            }
        }
    };
    for (i, s) in guest.inventory.iter().enumerate() {
        put(i as u8, s);
    }
    put(103, &guest.armor[0]);
    put(102, &guest.armor[1]);
    put(101, &guest.armor[2]);
    put(100, &guest.armor[3]);
    put(150, &guest.offhand);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lan::{Joined, PlayerSender};
    use crate::save::write_dat;
    use horizon_protocol::packets::configuration::ClientInformation;
    use std::sync::{Arc, atomic::AtomicI32, mpsc};

    fn guest() -> Guest {
        let (tx, _rx) = mpsc::channel();
        Guest::new(
            Joined {
                conn: 2,
                uuid: 2,
                name: "Alex".into(),
                properties: Vec::new(),
                info: ClientInformation::new(8),
                brand: "test".into(),
                sender: PlayerSender::new(tx, Arc::default()),
                latency: Arc::new(AtomicI32::new(0)),
                address: "127.0.0.1:25565".parse().unwrap(),
            },
            2,
            [0.0, 64.0, 0.0],
            8,
        )
    }

    fn holding(uuid: u128, item: &str, count: u32) -> Guest {
        let mut g = guest();
        g.uuid = uuid;
        g.position = [10.5, 70.0, -3.5];
        g.inventory[0] = Some(ItemStack {
            item: Arc::from(item),
            count,
            damage: 0,
            extra: None,
        });
        g
    }
    fn world(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("horizon-owner-{name}-{}", std::process::id()));
        let _ = horizon_core::fs::remove_dir_all(&dir);
        dir
    }
    fn first_item(g: &Guest) -> Option<(&str, u32)> {
        g.inventory[0].as_ref().map(|s| (&*s.item, s.count))
    }
    fn imported_world(dir: &Path) {
        Level::default().write(dir).unwrap();
        let mut tag = crate::save::read_dat(&dir.join("level.dat")).unwrap();
        if let Some(horizon_world::nbt::Tag::Compound(data)) = tag.get_mut("Data") {
            data.remove(&"singleplayer_uuid".into());
        }
        write_dat(&dir.join("level.dat"), tag).unwrap();
    }

    #[test]
    fn imported_owner_never_adopts_or_overwrites_a_friend_in_either_path() {
        let owner = horizon_protocol::crypto::offline_uuid("Owner");
        let friend = horizon_protocol::crypto::offline_uuid("Friend");
        for in_frame in [false, true] {
            let dir = world(if in_frame {
                "import-frame"
            } else {
                "import-server"
            });
            imported_world(&dir);
            save(&holding(friend, "minecraft:netherite_ingot", 9), &dir);
            let before = horizon_core::fs::read(path(&dir, friend)).unwrap();
            let mut level = Level::read(&dir).unwrap();
            assert!(level.inventory.is_empty());
            assert!(level.position.is_none());
            if in_frame {
                level.load_owner_player(&dir, owner);
                assert_eq!(level.player_uuid, Some(uuid_ints(owner)));
                level.write(&dir).unwrap();
            } else {
                let mut g = guest();
                g.uuid = owner;
                assert!(!load_owner(&mut g, &dir, Some(&level)));
                assert_eq!(first_item(&g), None);
                save_owner(&g, &mut level, &dir);
                assert_eq!(level.player_uuid, Some(uuid_ints(owner)));
                level.write(&dir).unwrap();
            }
            assert_eq!(horizon_core::fs::read(path(&dir, friend)).unwrap(), before);
            let reopened = Level::read(&dir).unwrap();
            assert_eq!(reopened.player_uuid, Some(uuid_ints(owner)));
            assert!(reopened.inventory.is_empty());
            horizon_core::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn named_owner_beats_stale_own_file_and_leave_updates_both() {
        let owner = horizon_protocol::crypto::offline_uuid("Owner");
        let dir = world("named-leave");
        save(&holding(owner, "minecraft:diamond", 5), &dir);
        let mut level = Level::default();
        store(&holding(owner, "minecraft:gold_ingot", 7), &mut level);
        level.write(&dir).unwrap();
        let named = level.player_uuid.unwrap();
        assert_ne!(named, uuid_ints(owner));
        let mut read = Level::read(&dir).unwrap();
        assert!(read.singleplayer_file);
        read.load_owner_player(&dir, owner);
        assert_eq!(read.inventory[0].1, "minecraft:gold_ingot");
        let mut g = guest();
        g.uuid = owner;
        assert!(load_owner(&mut g, &dir, Some(&read)));
        assert_eq!(first_item(&g), Some(("minecraft:gold_ingot", 7)));
        g.inventory[0] = holding(owner, "minecraft:emerald", 3).inventory[0].clone();
        save_owner(&g, &mut read, &dir);
        let reopened = Level::read(&dir).unwrap();
        let mut g = guest();
        g.uuid = owner;
        assert!(load_owner(&mut g, &dir, Some(&reopened)));
        assert_eq!(first_item(&g), Some(("minecraft:emerald", 3)));
        assert!(load(&mut g, &dir));
        assert_eq!(first_item(&g), Some(("minecraft:emerald", 3)));
        horizon_core::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn owner_save_rotates_shared_profile_path_only_once() {
        let owner = horizon_protocol::crypto::offline_uuid("Owner");
        let dir = world("owner-single-write");
        save(&holding(owner, "minecraft:diamond", 5), &dir);
        let file = path(&dir, owner);
        let original = horizon_core::fs::read(&file).unwrap();
        horizon_core::fs::rename(&file, file.with_extension("dat_old")).unwrap();
        let corrupt = b"truncated owner NBT";
        horizon_core::fs::write(&file, corrupt).unwrap();
        let mut g = guest();
        g.uuid = owner;
        assert!(load(&mut g, &dir));
        g.inventory[0] = holding(owner, "minecraft:emerald", 3).inventory[0].clone();
        let mut level = Level {
            player_uuid: Some(uuid_ints(owner)),
            ..Level::default()
        };
        save_owner(&g, &mut level, &dir);
        assert_eq!(
            horizon_core::fs::read(file.with_extension("dat_old")).unwrap(),
            original
        );
        assert!(
            horizon_core::fs::read_dir(file.parent().unwrap())
                .unwrap()
                .flatten()
                .any(
                    |entry| entry.file_name().to_string_lossy().contains("_corrupted_")
                        && horizon_core::fs::read(entry.path()).unwrap() == corrupt
                )
        );
        assert!(load(&mut g, &dir));
        assert_eq!(first_item(&g), Some(("minecraft:emerald", 3)));
        horizon_core::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn own_player_replaces_optional_legacy_respawn_rotation_dimension_and_mode() {
        use horizon_world::nbt::Tag;
        let owner = horizon_protocol::crypto::offline_uuid("Owner");
        for dimension in ["minecraft:overworld", "minecraft:the_nether"] {
            let dir = world(if dimension.ends_with("overworld") {
                "legacy-bed"
            } else {
                "legacy-anchor"
            });
            imported_world(&dir);
            let mut root = crate::save::read_dat(&dir.join("level.dat")).unwrap();
            if let Some(Tag::Compound(data)) = root.get_mut("Data") {
                data.insert(
                    "spawn".into(),
                    Tag::compound([("pos", Tag::IntArray(vec![1, 64, 2]))]),
                );
                data.insert(
                    "Player".into(),
                    Tag::compound([
                        (
                            "respawn",
                            Tag::compound([
                                ("pos", Tag::IntArray(vec![8, 80, 9])),
                                ("dimension", Tag::String(dimension.into())),
                            ]),
                        ),
                        ("Dimension", Tag::String("minecraft:the_end".into())),
                        (
                            "Rotation",
                            Tag::List(vec![
                                Tag::Float(90f32.to_bits()),
                                Tag::Float(45f32.to_bits()),
                            ]),
                        ),
                        ("playerGameType", Tag::Int(1)),
                    ]),
                );
            }
            write_dat(&dir.join("level.dat"), root).unwrap();
            let own = Tag::compound([(
                "Pos",
                Tag::List(vec![
                    Tag::Double(10f64.to_bits()),
                    Tag::Double(70f64.to_bits()),
                    Tag::Double(3f64.to_bits()),
                ]),
            )]);
            write_dat(&path(&dir, owner), own).unwrap();
            let saved = Level::read(&dir).unwrap();
            assert!(saved.respawn_elsewhere.is_some());
            let mut frame = saved.clone();
            frame.load_owner_player(&dir, owner);
            assert_eq!(frame.spawn, Some([1.0, 64.0, 2.0]));
            assert_eq!(frame.respawn_elsewhere, None);
            assert_eq!(frame.rotation, [0.0; 2]);
            assert_eq!(frame.dimension, "minecraft:overworld");
            assert_eq!(frame.game_mode, 0);
            assert!(!frame.creative);
            let mut g = guest();
            g.uuid = owner;
            assert!(load_owner(&mut g, &dir, Some(&saved)));
            assert_eq!(g.respawn_elsewhere, None);
            assert_eq!(g.spawn, None);
            assert_eq!(g.yaw, 0.0);
            assert_eq!(g.dimension, "minecraft:overworld");
            assert_eq!(g.game_mode, 0);
            horizon_core::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn guest_save_preserves_corrupt_primary_and_readable_backup() {
        let owner = horizon_protocol::crypto::offline_uuid("Guest");
        let dir = world("guest-save-backup");
        save(&holding(owner, "minecraft:diamond", 5), &dir);
        let file = path(&dir, owner);
        let original = horizon_core::fs::read(&file).unwrap();
        horizon_core::fs::rename(&file, file.with_extension("dat_old")).unwrap();
        let corrupt = b"truncated guest NBT";
        horizon_core::fs::write(&file, corrupt).unwrap();
        let mut g = guest();
        g.uuid = owner;
        assert!(load(&mut g, &dir));
        assert_eq!(first_item(&g), Some(("minecraft:diamond", 5)));
        g.inventory[0] = holding(owner, "minecraft:emerald", 3).inventory[0].clone();
        save(&g, &dir);
        assert_eq!(
            horizon_core::fs::read(file.with_extension("dat_old")).unwrap(),
            original
        );
        let preserved = horizon_core::fs::read_dir(file.parent().unwrap())
            .unwrap()
            .flatten()
            .any(|entry| {
                entry.file_name().to_string_lossy().contains("_corrupted_")
                    && horizon_core::fs::read(entry.path()).unwrap() == corrupt
            });
        assert!(preserved, "the broken primary was preserved before saving");
        assert!(load(&mut g, &dir));
        assert_eq!(first_item(&g), Some(("minecraft:emerald", 3)));
        horizon_core::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_named_file_recovers_only_owners_validated_backup_in_both_paths() {
        let owner = horizon_protocol::crypto::offline_uuid("Owner");
        let dir = world("own-backup");
        imported_world(&dir);
        save(&holding(owner, "minecraft:diamond", 5), &dir);
        let file = path(&dir, owner);
        horizon_core::fs::rename(&file, file.with_extension("dat_old")).unwrap();
        // A parseable non-compound primary must not hide the validated backup.
        write_dat(&file, horizon_world::nbt::Tag::Int(123)).unwrap();
        let mut read = Level::read(&dir).unwrap();
        assert!(!read.singleplayer_file);
        let mut g = guest();
        g.uuid = owner;
        assert!(load_owner(&mut g, &dir, Some(&read)));
        assert_eq!(first_item(&g), Some(("minecraft:diamond", 5)));
        assert!(load(&mut g, &dir)); // non-owner guest recovery uses same rule
        read.load_owner_player(&dir, owner);
        assert_eq!(read.inventory[0].1, "minecraft:diamond");
        assert_eq!(read.player_uuid, Some(uuid_ints(owner)));
        horizon_core::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_saved_burn_and_fall_come_back() {
        // A player saved burning in mid-fall (#1323, #1327): Entity.saveWithoutId's
        // Fire, Air and fall_distance, read back for the next body.
        let mut g = guest();
        g.loaded_body = Some(SavedBody {
            fire_ticks: 96,
            air: 250,
            fall_distance: 9.5,
        });
        let mut level = Level::default();
        store(&g, &mut level);
        let tag = level.player_tag();
        let mut read = Level::default();
        read.read_player(&tag);
        read.player_extra = Some(tag);
        let mut back = guest();
        apply(&mut back, &read);
        assert_eq!(back.loaded_body, g.loaded_body);
    }
}
