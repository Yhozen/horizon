//! What the relay does with each packet after login, in one table: pass it
//! as it came, pass it unless its entity is hidden, translate it (a typed arm
//! in `translate.rs`), or drop it. `tests.rs` walks every packet id of
//! protocol 777 through [`policy`]: the dropped set must equal `dropped.txt`,
//! and every built-in registry a forwarded packet still carries must be one
//! the relay renumbers ([`TRANSLATED`]) or a known gap (`gaps.txt`).
use crate::translate::Phase;
use horizon_protocol::{
    Direction,
    ids::{
        configuration::{clientbound as cfg_cb, serverbound as cfg_sb},
        play::{clientbound as cb, serverbound as sb},
    },
};

/// How a packet's entity id is written, when it is the first field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityId {
    VarInt,
    Int,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    /// Forwarded as it came.
    Pass,
    /// Forwarded unless the entity it is about is hidden from the open player.
    PassUnlessHidden(EntityId),
    /// Read, and rewritten where it holds vanilla numbers, by a typed arm.
    Translate,
    /// Never forwarded.
    Drop,
}

/// One packet's rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rule {
    pub policy: Policy,
    /// The built-in registries whose numbers the forwarded packet holds.
    /// Dynamic registries (biomes, dimension types, damage types, chat
    /// types...) are fine: the server's own reach the client.
    pub carries: &'static [&'static str],
}

/// The registries `Tables` renumbers. A translated packet that carries one
/// of these has it rewritten.
pub const TRANSLATED: &[&str] = &[
    "minecraft:attribute",
    "minecraft:block",
    "minecraft:data_component_type",
    "minecraft:entity_type",
    "minecraft:item",
    "minecraft:menu",
];

const fn rule(policy: Policy, carries: &'static [&'static str]) -> Rule {
    Rule { policy, carries }
}
const PASS: Rule = rule(Policy::Pass, &[]);
const DROP: Rule = rule(Policy::Drop, &[]);
const TRANSLATE: Rule = rule(Policy::Translate, &[]);
const BY_VARINT: Rule = rule(Policy::PassUnlessHidden(EntityId::VarInt), &[]);
const BY_INT: Rule = rule(Policy::PassUnlessHidden(EntityId::Int), &[]);

const ITEMS: &[&str] = &["minecraft:item", "minecraft:data_component_type"];
const BLOCKS: &[&str] = &["minecraft:block"];

/// The rule for a packet after login.
pub fn policy(phase: Phase, direction: Direction, id: i32) -> Rule {
    match (phase, direction) {
        (Phase::Configuration, Direction::Clientbound) => configuration_clientbound(id),
        (Phase::Configuration, Direction::Serverbound) => configuration_serverbound(id),
        (Phase::Play, Direction::Clientbound) => play_clientbound(id),
        (Phase::Play, Direction::Serverbound) => play_serverbound(id),
    }
}

fn configuration_clientbound(id: i32) -> Rule {
    match id {
        cfg_cb::COOKIE_REQUEST
        | cfg_cb::DISCONNECT
        | cfg_cb::KEEP_ALIVE
        | cfg_cb::PING
        | cfg_cb::RESET_CHAT
        | cfg_cb::RESOURCE_PACK_POP
        | cfg_cb::RESOURCE_PACK_PUSH
        | cfg_cb::UPDATE_ENABLED_FEATURES
        | cfg_cb::SELECT_KNOWN_PACKS
        | cfg_cb::CLEAR_DIALOG
        | cfg_cb::CODE_OF_CONDUCT => PASS,
        // The brand is rewritten; registry data and the end of configuration
        // are read for the dimensions and biomes, then pass.
        cfg_cb::CUSTOM_PAYLOAD | cfg_cb::REGISTRY_DATA | cfg_cb::FINISH_CONFIGURATION => TRANSLATE,
        // Tags (vanilla ids), transfers to other servers, cookies, report
        // details, server links, dialogs and post effects.
        _ => DROP,
    }
}

fn configuration_serverbound(id: i32) -> Rule {
    match id {
        // Read to follow the phase, then passed.
        cfg_sb::FINISH_CONFIGURATION => TRANSLATE,
        _ => PASS,
    }
}

fn play_clientbound(id: i32) -> Rule {
    match id {
        // Beacon menus send their effects as mob effect ids.
        cb::CONTAINER_SET_DATA => rule(Policy::Pass, &["minecraft:mob_effect"]),
        cb::BUNDLE_DELIMITER
        | cb::BLOCK_CHANGED_ACK
        | cb::BLOCK_DESTRUCTION
        | cb::CHANGE_DIFFICULTY
        | cb::CHUNK_BATCH_FINISHED
        | cb::CHUNK_BATCH_START
        | cb::CHUNKS_BIOMES
        | cb::CLEAR_TITLES
        | cb::COMMAND_SUGGESTIONS
        | cb::CONTAINER_CLOSE
        | cb::DISCONNECT
        | cb::DISGUISED_CHAT
        | cb::FORGET_LEVEL_CHUNK
        | cb::GAME_EVENT
        | cb::GAME_RULE_VALUES
        | cb::INITIALIZE_BORDER
        | cb::KEEP_ALIVE
        | cb::LIGHT_UPDATE
        | cb::OPEN_BOOK
        | cb::OPEN_SIGN_EDITOR
        | cb::PING
        | cb::PONG_RESPONSE
        | cb::PLAYER_ABILITIES
        | cb::PLAYER_CHAT
        | cb::PLAYER_COMBAT_KILL
        | cb::PLAYER_INFO_REMOVE
        | cb::PLAYER_INFO_UPDATE
        | cb::PLAYER_POSITION
        | cb::PLAYER_ROTATION
        | cb::RESET_SCORE
        | cb::RESOURCE_PACK_PUSH
        | cb::SERVER_DATA
        | cb::SET_ACTION_BAR_TEXT
        | cb::SET_BORDER_CENTER
        | cb::SET_BORDER_LERP_SIZE
        | cb::SET_BORDER_SIZE
        | cb::SET_BORDER_WARNING_DELAY
        | cb::SET_BORDER_WARNING_DISTANCE
        | cb::SET_CHUNK_CACHE_CENTER
        | cb::SET_CHUNK_CACHE_RADIUS
        | cb::SET_DEFAULT_SPAWN_POSITION
        | cb::SET_DISPLAY_OBJECTIVE
        | cb::SET_EXPERIENCE
        | cb::SET_HEALTH
        | cb::SET_HELD_SLOT
        | cb::SET_OBJECTIVE
        | cb::MOVE_VEHICLE
        | cb::SET_PLAYER_TEAM
        | cb::SET_SCORE
        | cb::SET_SIMULATION_DISTANCE
        | cb::SET_SUBTITLE_TEXT
        | cb::SET_TIME
        | cb::SET_TITLE_TEXT
        | cb::SET_TITLES_ANIMATION
        | cb::SYSTEM_CHAT
        | cb::TAB_LIST
        | cb::TICKING_STATE
        | cb::TICKING_STEP => PASS,
        cb::ANIMATE
        | cb::ENTITY_POSITION_SYNC
        | cb::HURT_ANIMATION
        | cb::DAMAGE_EVENT
        | cb::MOVE_ENTITY_POS
        | cb::MOVE_ENTITY_POS_ROT
        | cb::MOVE_ENTITY_ROT
        | cb::ROTATE_HEAD
        | cb::SET_ENTITY_MOTION
        | cb::TAKE_ITEM_ENTITY
        | cb::TELEPORT_ENTITY
        | cb::PROJECTILE_POWER
        | cb::SWING_ANIMATION
        | cb::SET_PASSENGERS => BY_VARINT,
        cb::ENTITY_EVENT | cb::SET_ENTITY_LINK => BY_INT,
        // Read for the dimension, the hidden entities or the phase, then pass.
        cb::LOGIN | cb::RESPAWN | cb::START_CONFIGURATION | cb::REMOVE_ENTITIES => TRANSLATE,
        // The brand is rewritten, other channels dropped.
        cb::CUSTOM_PAYLOAD => TRANSLATE,
        // Passed only when both sides number argument types alike
        // (`Tables::commands_compatible`), else dropped.
        cb::COMMANDS => TRANSLATE,
        cb::LEVEL_CHUNK_WITH_LIGHT
        | cb::BLOCK_UPDATE
        | cb::SECTION_BLOCKS_UPDATE
        | cb::LEVEL_EVENT => rule(Policy::Translate, BLOCKS),
        cb::ADD_ENTITY => rule(
            Policy::Translate,
            &["minecraft:entity_type", "minecraft:block"],
        ),
        // Villager data holds villager type and profession ids.
        cb::SET_ENTITY_DATA => rule(
            Policy::Translate,
            &[
                "minecraft:block",
                "minecraft:item",
                "minecraft:data_component_type",
                "minecraft:villager_type",
                "minecraft:villager_profession",
            ],
        ),
        cb::SET_EQUIPMENT
        | cb::CONTAINER_SET_CONTENT
        | cb::CONTAINER_SET_SLOT
        | cb::SET_CURSOR_ITEM
        | cb::SET_PLAYER_INVENTORY => rule(Policy::Translate, ITEMS),
        cb::UPDATE_ATTRIBUTES => rule(Policy::Translate, &["minecraft:attribute"]),
        // Display ids renamed to the bundle's recipes (`recipes.rs`); recipe
        // book categories and display kinds are the same registries on both
        // sides, as the coverage file says.
        cb::RECIPE_BOOK_ADD => rule(
            Policy::Translate,
            &[
                "minecraft:item",
                "minecraft:data_component_type",
                "minecraft:recipe_book_category",
                "minecraft:recipe_display",
                "minecraft:slot_display",
            ],
        ),
        cb::PLACE_GHOST_RECIPE => rule(
            Policy::Translate,
            &[
                "minecraft:item",
                "minecraft:data_component_type",
                "minecraft:recipe_display",
                "minecraft:slot_display",
            ],
        ),
        cb::RECIPE_BOOK_REMOVE => TRANSLATE,
        // Each book's open and filter flags.
        cb::RECIPE_BOOK_SETTINGS => PASS,
        cb::OPEN_SCREEN => rule(Policy::Translate, &["minecraft:menu"]),
        // Everything else names something by a vanilla number the open
        // client would misread (sounds, particles, block entities, mob
        // effects, advancements, tags, statistics, maps, trades,
        // explosions, waypoints, debug data) or is a packet it does not
        // decode at all (`update_recipes`: the open client reads none, it
        // takes furnace fuels and stonecutter recipes from its bundle).
        _ => DROP,
    }
}

fn play_serverbound(id: i32) -> Rule {
    match id {
        // Read to follow the phase, then passed.
        sb::CONFIGURATION_ACKNOWLEDGED => TRANSLATE,
        sb::CONTAINER_CLICK | sb::SET_CREATIVE_MODE_SLOT => rule(Policy::Translate, ITEMS),
        // Display ids back to the server's (`recipes.rs`).
        sb::PLACE_RECIPE | sb::RECIPE_BOOK_SEEN_RECIPE => TRANSLATE,
        // The beacon's chosen effects.
        sb::SET_BEACON => rule(Policy::Pass, &["minecraft:mob_effect"]),
        // The debug renderers a client subscribes to (the open client sends none).
        sb::DEBUG_SUBSCRIPTION_REQUEST => rule(Policy::Pass, &["minecraft:debug_subscription"]),
        _ => PASS,
    }
}
