//! The open edition's English interface text, written for it: the labels of
//! the screens it keeps (pause, options, controls, chat, credits), the key
//! names, and the replies of the commands it allows. Keys are the shared
//! interface's identifiers; the words are Horizon's.

/// Hand-written labels, by key.
pub const LABELS: &[(&str, &str)] = &[
    // Pause menu and common buttons.
    ("menu.game", "Paused"),
    ("menu.returnToGame", "Back to the World"),
    ("menu.options", "Options..."),
    ("menu.paused", "Paused"),
    ("menu.savingLevel", "Saving..."),
    ("menu.loadingLevel", "Preparing the world"),
    ("menu.generatingTerrain", "Shaping the terrain"),
    ("menu.preparingSpawn", "Finding a safe place to start: %s%%"),
    ("menu.working", "Working..."),
    ("gui.done", "Done"),
    ("gui.back", "Back"),
    ("gui.cancel", "Cancel"),
    ("gui.yes", "Yes"),
    ("gui.no", "No"),
    ("gui.ok", "OK"),
    ("gui.proceed", "Continue"),
    ("gui.none", "None"),
    ("gui.toMenu", "Back"),
    ("gui.language.search", "Search languages..."),
    ("gui.recipebook.search_hint", "Search..."),
    ("open.credits", "Credits"),
    // Options: the main page.
    ("options.title", "Options"),
    ("options.generic_value", "%s: %s"),
    ("options.percent_value", "%s: %s%%"),
    ("options.pixel_value", "%s: %spx"),
    ("options.multiplier", "%sx"),
    ("options.on", "On"),
    ("options.off", "Off"),
    ("options.fov", "Field of View"),
    ("options.fov.min", "Normal"),
    ("options.fov.max", "Wide"),
    ("options.video", "Video Settings..."),
    ("options.videoTitle", "Video Settings"),
    ("options.controls", "Controls..."),
    ("options.chat", "Chat Settings..."),
    ("options.chat.title", "Chat Settings"),
    ("options.accessibility", "Accessibility..."),
    ("options.accessibility.title", "Accessibility"),
    ("options.credits_and_attribution", "Credits..."),
    ("credits_and_attribution.screen.title", "Credits"),
    ("credits_and_attribution.button.credits", "Show the Credits"),
    ("credits_and_attribution.button.attribution", "Attribution"),
    ("credits_and_attribution.button.licenses", "Licences"),
    // Video.
    ("options.video.display.header", "Display"),
    ("options.video.quality.header", "Quality"),
    ("options.video.preferences.header", "Preferences"),
    ("options.graphics.preset", "Graphics Preset"),
    ("options.graphics", "Graphics"),
    ("options.graphics.fast", "Fast"),
    ("options.graphics.fancy", "Fancy"),
    ("options.graphics.fabulous", "Fabulous"),
    ("options.graphics.custom", "Custom"),
    ("options.renderDistance", "View Distance"),
    ("options.simulationDistance", "Simulation Distance"),
    ("options.chunks", "%s chunks"),
    ("options.framerateLimit", "Frame Rate Limit"),
    ("options.framerateLimit.max", "Unlimited"),
    ("options.framerate", "%s fps"),
    ("options.inactivityFpsLimit", "Reduce Frame Rate When"),
    ("options.inactivityFpsLimit.afk", "Idle"),
    (
        "options.inactivityFpsLimit.afk.tooltip",
        "Lowers the frame rate after a minute without input.",
    ),
    ("options.inactivityFpsLimit.minimized", "Minimized"),
    (
        "options.inactivityFpsLimit.minimized.tooltip",
        "Lowers the frame rate only while the window is hidden.",
    ),
    ("options.vsync", "VSync"),
    ("options.fullscreen", "Fullscreen"),
    ("options.fullscreen.resolution", "Fullscreen Resolution"),
    ("options.fullscreen.current", "Current"),
    ("options.guiScale", "Interface Size"),
    ("options.guiScale.auto", "Auto"),
    ("options.gamma", "Brightness"),
    ("options.gamma.min", "Dim"),
    ("options.gamma.default", "Default"),
    ("options.gamma.max", "Bright"),
    ("options.ao", "Smooth Lighting"),
    ("options.biomeBlendRadius", "Colour Blending"),
    ("options.biomeBlendRadius.1", "Off"),
    ("options.biomeBlendRadius.3", "3x3"),
    ("options.biomeBlendRadius.5", "5x5"),
    ("options.biomeBlendRadius.7", "7x7"),
    ("options.biomeBlendRadius.9", "9x9"),
    ("options.biomeBlendRadius.11", "11x11"),
    ("options.biomeBlendRadius.13", "13x13"),
    ("options.biomeBlendRadius.15", "15x15"),
    ("options.blocks", "%s blocks"),
    ("options.renderClouds", "Clouds"),
    ("options.renderCloudsDistance", "Cloud Distance"),
    ("options.clouds.fancy", "Fancy"),
    ("options.clouds.fast", "Fast"),
    ("options.particles", "Particles"),
    ("options.particles.all", "All"),
    ("options.particles.decreased", "Fewer"),
    ("options.particles.minimal", "Minimal"),
    ("options.mipmapLevels", "Texture Smoothing Levels"),
    ("options.entityShadows", "Shadows"),
    ("options.entityDistanceScaling", "Entity Distance"),
    ("options.viewBobbing", "View Bobbing"),
    ("options.cutoutLeaves", "See-through Leaves"),
    ("options.improvedTransparency", "Better Transparency"),
    ("options.chunkFade", "Fade-in of New Terrain"),
    ("options.chunkFade.none", "Off"),
    ("options.chunkFade.seconds", "%s s"),
    ("options.prioritizeChunkUpdates", "Terrain Updates"),
    ("options.prioritizeChunkUpdates.none", "In the Background"),
    (
        "options.prioritizeChunkUpdates.byPlayer",
        "Near Your Changes",
    ),
    ("options.prioritizeChunkUpdates.nearby", "Nearby First"),
    ("options.weatherRadius", "Weather Radius"),
    ("options.vignette", "Vignette"),
    ("options.attackIndicator", "Attack Indicator"),
    ("options.attack.crosshair", "Crosshair"),
    ("options.attack.hotbar", "Hotbar"),
    ("options.autosaveIndicator", "Saving Indicator"),
    ("options.glintSpeed", "Shimmer Speed"),
    ("options.glintStrength", "Shimmer Strength"),
    ("options.screenEffectScale", "Distortion Effects"),
    ("options.fovEffectScale", "Field of View Effects"),
    ("options.darknessEffectScale", "Darkness Pulsing"),
    ("options.damageTiltStrength", "Damage Tilt"),
    ("options.hideLightningFlashes", "Hide Lightning Flashes"),
    ("options.hideSplashTexts", "Hide Splash Texts"),
    ("options.rotateWithMinecart", "Turn with Vehicles"),
    ("options.reducedDebugInfo", "Reduced Debug Info"),
    // Controls and key bindings.
    ("controls.title", "Controls"),
    ("controls.keybinds", "Key Binds..."),
    ("controls.keybinds.title", "Key Binds"),
    (
        "controls.keybinds.duplicateKeybinds",
        "This key is also used for:\n%s",
    ),
    ("controls.reset", "Reset"),
    ("controls.resetAll", "Reset Keys"),
    ("options.mouse_settings", "Mouse Settings..."),
    ("options.mouse_settings.title", "Mouse Settings"),
    ("options.sensitivity", "Sensitivity"),
    ("options.sensitivity.min", "Slow"),
    ("options.sensitivity.max", "Fast"),
    ("options.invertMouseY", "Invert Vertical Look"),
    ("options.invertMouseX", "Invert Horizontal Look"),
    ("options.mouseWheelSensitivity", "Scroll Sensitivity"),
    ("options.discrete_mouse_scroll", "Step Scrolling"),
    ("options.touchscreen", "Touch Mode"),
    ("options.autoJump", "Auto-Jump"),
    ("options.key.toggle", "Toggle"),
    ("options.key.hold", "Hold"),
    ("options.sprintWindow", "Double-tap Sprint Window"),
    ("options.operatorItemsTab", "Operator Items"),
    ("key.categories.movement", "Movement"),
    ("key.categories.gameplay", "Gameplay"),
    ("key.categories.inventory", "Inventory"),
    ("key.categories.misc", "Other"),
    ("key.categories.multiplayer", "Chat"),
    ("key.categories.ui", "Interface"),
    ("key.categories.creative", "Creative"),
    ("key.categories.debug", "Debug"),
    ("key.categories.spectator", "Spectating"),
    ("key.forward", "Move Forward"),
    ("key.back", "Move Back"),
    ("key.left", "Strafe Left"),
    ("key.right", "Strafe Right"),
    ("key.jump", "Jump / Fly Up"),
    ("key.sneak", "Crouch / Fly Down"),
    ("key.sprint", "Run"),
    ("key.inventory", "Open the Inventory"),
    ("key.swapOffhand", "Swap to the Other Hand"),
    ("key.drop", "Drop the Held Item"),
    ("key.use", "Use an Item / Place a Block"),
    ("key.attack", "Attack / Break a Block"),
    ("key.pickItem", "Pick the Targeted Block"),
    ("key.chat", "Open Chat"),
    ("key.playerlist", "List Players"),
    ("key.command", "Type a Command"),
    ("key.socialInteractions", "Social Screen"),
    ("key.screenshot", "Take a Screenshot"),
    ("key.togglePerspective", "Change the View"),
    ("key.smoothCamera", "Cinematic Camera"),
    ("key.fullscreen", "Toggle Fullscreen"),
    ("key.spectatorOutlines", "Highlight Players"),
    ("key.spectatorHotbar", "Spectator Menu"),
    ("key.advancements", "Progress"),
    ("key.quickActions", "Quick Actions"),
    ("key.toggleGui", "Hide the Interface"),
    ("key.toggleSpectatorShaderEffects", "Spectator Effects"),
    ("key.debug.overlay", "Debug Overlay"),
    ("key.debug.modifier", "Debug Modifier"),
    ("key.debug.crash", "Debug Crash"),
    ("key.debug.reloadChunk", "Reload Chunks"),
    ("key.debug.showHitboxes", "Show Hitboxes"),
    ("key.debug.clearChat", "Clear Chat"),
    ("key.debug.showChunkBorders", "Show Chunk Borders"),
    ("key.debug.showAdvancedTooltips", "Advanced Tooltips"),
    ("key.debug.copyRecreateCommand", "Copy Recreate Command"),
    ("key.debug.spectate", "Spectate"),
    ("key.debug.switchGameMode", "Switch Game Mode"),
    ("key.debug.debugOptions", "Debug Options"),
    ("key.debug.focusPause", "Pause When Unfocused"),
    ("key.debug.dumpDynamicTextures", "Dump Textures"),
    ("key.debug.dumpVersion", "Copy Version"),
    ("key.debug.profiling", "Profiling"),
    ("key.debug.copyLocation", "Copy Location"),
    ("key.debug.reloadResourcePacks", "Reload Resources"),
    ("key.debug.toggleCamera", "Free Camera"),
    ("key.debug.lightmapTexture", "Light Map"),
    ("key.debug.showFps", "Show Frame Rate"),
    ("key.saveToolbarActivator", "Save Toolbar"),
    ("key.loadToolbarActivator", "Load Toolbar"),
    ("key.hotbar.1", "Hotbar Slot 1"),
    ("key.hotbar.2", "Hotbar Slot 2"),
    ("key.hotbar.3", "Hotbar Slot 3"),
    ("key.hotbar.4", "Hotbar Slot 4"),
    ("key.hotbar.5", "Hotbar Slot 5"),
    ("key.hotbar.6", "Hotbar Slot 6"),
    ("key.hotbar.7", "Hotbar Slot 7"),
    ("key.hotbar.8", "Hotbar Slot 8"),
    ("key.hotbar.9", "Hotbar Slot 9"),
    ("key.mouse.left", "Left Button"),
    ("key.mouse.right", "Right Button"),
    ("key.mouse.middle", "Middle Button"),
    ("key.mouse", "Mouse Button %1$s"),
    ("key.keyboard.unknown", "Not Bound"),
    // Chat.
    ("options.chat.visibility", "Chat"),
    ("options.chat.visibility.full", "Shown"),
    ("options.chat.visibility.system", "Commands Only"),
    ("options.chat.visibility.hidden", "Hidden"),
    ("options.chat.color", "Colours"),
    ("options.chat.links", "Web Links"),
    ("options.chat.links.prompt", "Ask Before Opening Links"),
    ("options.chat.opacity", "Text Opacity"),
    ("options.chat.line_spacing", "Line Spacing"),
    ("options.chat.scale", "Text Size"),
    ("options.chat.width", "Width"),
    ("options.chat.height.focused", "Height While Typing"),
    ("options.chat.height.unfocused", "Height"),
    ("options.chat.delay_none", "Delay: None"),
    ("options.chat.delay", "Delay: %s seconds"),
    ("options.autoSuggestCommands", "Command Suggestions"),
    ("options.notifications.display_time", "Notification Time"),
    ("options.narrator", "Narrator"),
    ("options.narrator.off", "Off"),
    ("options.showSubtitles", "Subtitles"),
    ("chat.editBox", "chat"),
    ("chat_screen.title", "Chat"),
    ("chat_screen.usage", "Type a message and press Enter"),
    ("chat.disabled.options", "Chat is off in the chat settings."),
    // Accessibility.
    ("options.accessibility.high_contrast", "High Contrast"),
    (
        "options.accessibility.high_contrast_block_outline",
        "High Contrast Outlines",
    ),
    (
        "options.accessibility.menu_background_blurriness",
        "Menu Background Blur",
    ),
    (
        "options.accessibility.menu_background_blurriness.tooltip",
        "How much the world blurs behind menus.",
    ),
    ("options.accessibility.narrator_hotkey", "Narrator Shortcut"),
    (
        "options.accessibility.panorama_speed",
        "Backdrop Scroll Speed",
    ),
    ("options.accessibility.text_background", "Text Background"),
    ("options.accessibility.text_background.chat", "Chat"),
    (
        "options.accessibility.text_background.everywhere",
        "Everywhere",
    ),
    (
        "options.accessibility.text_background_opacity",
        "Text Background Opacity",
    ),
    (
        "options.accessibility.tooltip",
        "Settings that make the game easier to see and use",
    ),
    (
        "options.darkMojangStudiosBackgroundColor",
        "Monochrome Loading Screen",
    ),
    ("options.japaneseGlyphVariants", "Japanese Glyph Variants"),
    ("options.forceUnicodeFont", "Force Unicode Font"),
    ("options.font", "Font Settings..."),
    ("options.font.title", "Font Settings"),
    // The creative palette screen.
    ("itemGroup.search", "Search"),
    ("itemGroup.inventory", "Inventory"),
    ("inventory.binSlot", "Destroy Item"),
    ("container.inventory", "Inventory"),
    ("container.creative", "Item Palette"),
    // The commands this edition keeps. Time replies name the clock, then the time.
    ("commands.time.set.absolute", "The time is now %2$s"),
    (
        "commands.time.set.already_at_time",
        "The time is already %2$s",
    ),
    ("commands.time.set.time_marker", "The time is now %2$s"),
    (
        "commands.time.set.already_at_time_marker",
        "The time is already that",
    ),
    ("commands.time.query.gametime", "The time is %s"),
    ("commands.time.query.daytime", "The time of day is %s"),
    ("commands.time.no_default_clock", "This world has no clock"),
    ("commands.time.no_time_marker_found", "No such time of day"),
    (
        "commands.time.pause.already_paused",
        "Time is already paused",
    ),
    (
        "commands.time.pause.already_running",
        "Time is already running",
    ),
    (
        "commands.teleport.success.location.single",
        "Moved %s to %s, %s, %s",
    ),
    ("commands.teleport.success.entity.single", "Moved %s to %s"),
    (
        "commands.teleport.invalidPosition",
        "That position is outside the world",
    ),
    ("command.unknown.command", "Unknown command"),
    (
        "command.unknown.argument",
        "That argument is not valid here",
    ),
    ("command.context.here", "<--[HERE]"),
    (
        "command.expected.separator",
        "Expected a space before the next argument",
    ),
    ("argument.pos.incomplete", "Three coordinates are needed"),
    ("argument.pos3d.incomplete", "Three coordinates are needed"),
    ("argument.time.invalid_unit", "That time unit is not valid"),
    ("parsing.int.expected", "A whole number is needed"),
    ("parsing.float.expected", "A number is needed"),
    // Loading and connection.
    ("multiplayer.downloadingTerrain", "Building the world..."),
    ("connect.joining", "Entering the world..."),
    ("screenshot.success", "Saved the screenshot as %s"),
    (
        "screenshot.failure",
        "The screenshot could not be saved: %s",
    ),
    ("chat.square_brackets", "[%s]"),
    ("chat.type.announcement", "[%s] %s"),
    ("debug.prefix", "[Debug]:"),
    ("narrator.loading", "Loading: %s"),
    ("selectWorld.loading_list", "Loading worlds"),
];

/// Survival play, menus, joining, the statistics and the commands the open
/// edition allows: words written for Horizon (`%s` takes the arguments in
/// order, `%2$s` names one).
pub const SURVIVAL: &[(&str, &str)] = &[
    ("container.crafting", "Crafting"),
    ("deathScreen.title", "You Died"),
    ("deathScreen.title.hardcore", "Game Over"),
    ("deathScreen.respawn", "Try Again"),
    ("deathScreen.spectate", "Watch the World"),
    ("deathScreen.titleScreen", "Main Menu"),
    ("deathScreen.score", "Score"),
    ("deathScreen.score.value", "Score: %s"),
    ("deathScreen.quit.confirm", "Leave for the main menu?"),
    ("gui.recipebook.toggleRecipes.all", "Showing Everything"),
    (
        "gui.recipebook.toggleRecipes.craftable",
        "Showing What You Can Craft",
    ),
    (
        "gui.recipebook.toggleRecipes.smeltable",
        "Showing What You Can Fire",
    ),
    (
        "gui.recipebook.toggleRecipes.blastable",
        "Showing What You Can Fire",
    ),
    (
        "gui.recipebook.toggleRecipes.smokable",
        "Showing What You Can Cook",
    ),
    ("gui.recipebook.page", "%s/%s"),
    ("gui.recipebook.next_page", "Next Page"),
    ("gui.recipebook.previous_page", "Previous Page"),
    ("gui.recipebook.moreRecipes", "Right-click for more"),
    ("recipe.toast.title", "Recipes Learned"),
    ("recipe.toast.description", "Open the recipe book"),
    ("advancements.toast.task", "Milestone"),
    ("advancements.toast.goal", "Goal Reached"),
    ("advancements.toast.challenge", "Challenge Done"),
    ("item.modifiers.any", "When equipped:"),
    ("item.modifiers.mainhand", "In the main hand:"),
    ("item.modifiers.offhand", "In the off hand:"),
    ("item.modifiers.hand", "In a hand:"),
    ("item.modifiers.head", "On the head:"),
    ("item.modifiers.chest", "On the body:"),
    ("item.modifiers.legs", "On the legs:"),
    ("item.modifiers.feet", "On the feet:"),
    ("item.modifiers.armor", "When worn:"),
    ("item.modifiers.body", "When worn:"),
    ("item.modifiers.saddle", "When saddled:"),
    ("attribute.modifier.plus.0", "+%s %s"),
    ("attribute.modifier.plus.1", "+%s%% %s"),
    ("attribute.modifier.plus.2", "+%s%% %s"),
    ("attribute.modifier.take.0", "-%s %s"),
    ("attribute.modifier.take.1", "-%s%% %s"),
    ("attribute.modifier.take.2", "-%s%% %s"),
    ("attribute.modifier.equals.0", "%s %s"),
    ("attribute.modifier.equals.1", "%s%% %s"),
    ("attribute.modifier.equals.2", "%s%% %s"),
    ("attribute.name.attack_damage", "Attack Damage"),
    ("attribute.name.attack_speed", "Attack Speed"),
    ("attribute.name.armor", "Armor"),
    ("attribute.name.armor_toughness", "Armor Toughness"),
    ("attribute.name.max_health", "Max Health"),
    ("attribute.name.movement_speed", "Speed"),
    ("attribute.name.block_break_speed", "Mining Speed"),
    ("attribute.name.block_interaction_range", "Block Reach"),
    ("attribute.name.entity_interaction_range", "Attack Reach"),
    (
        "attribute.name.knockback_resistance",
        "Knockback Resistance",
    ),
    ("attribute.name.luck", "Luck"),
    ("item.durability", "Durability: %s / %s"),
    ("item.unbreakable", "Unbreakable"),
    ("item.color", "Colour: %s"),
    ("item.dyed", "Dyed"),
    ("item.nbt_tags", "Data: %s tag(s)"),
    ("item.components", "%s component(s)"),
    ("entity.minecraft.player", "Player"),
    ("entity.minecraft.item", "Item"),
    ("entity.minecraft.experience_orb", "Experience Orb"),
    ("entity.minecraft.falling_block", "Falling Block"),
    ("entity.minecraft.zombie", "Zombie"),
    ("entity.minecraft.husk", "Husk"),
    ("entity.minecraft.drowned", "Drowned"),
    ("entity.minecraft.skeleton", "Skeleton"),
    ("entity.minecraft.stray", "Stray"),
    ("entity.minecraft.spider", "Spider"),
    ("entity.minecraft.cave_spider", "Cave Spider"),
    ("entity.minecraft.creeper", "Creeper"),
    ("entity.minecraft.enderman", "Enderman"),
    ("entity.minecraft.cow", "Cow"),
    ("entity.minecraft.pig", "Pig"),
    ("entity.minecraft.sheep", "Sheep"),
    ("entity.minecraft.chicken", "Chicken"),
    ("gameMode.survival", "Survival"),
    ("gameMode.creative", "Creative"),
    ("gameMode.adventure", "Adventure"),
    ("gameMode.spectator", "Spectator"),
    ("gameMode.hardcore", "Hardcore"),
    ("gameMode.changed", "You are now in %s mode"),
    ("selectWorld.gameMode", "Game Mode"),
    ("selectWorld.gameMode.survival", "Survival"),
    (
        "selectWorld.gameMode.survival.info",
        "Gather, craft and keep yourself fed.",
    ),
    ("selectWorld.gameMode.creative", "Creative"),
    (
        "selectWorld.gameMode.creative.info",
        "Every block at hand; fly and build freely.",
    ),
    ("selectWorld.gameMode.hardcore", "Hardcore"),
    (
        "selectWorld.gameMode.hardcore.info",
        "Survival on the hardest setting, with one life.",
    ),
    ("selectWorld.gameMode.adventure", "Adventure"),
    ("selectWorld.gameMode.spectator", "Spectator"),
    ("options.difficulty", "Difficulty"),
    ("options.difficulty.peaceful", "Peaceful"),
    ("options.difficulty.easy", "Easy"),
    ("options.difficulty.normal", "Normal"),
    ("options.difficulty.hard", "Hard"),
    ("options.difficulty.hardcore", "Hardcore"),
    (
        "options.difficulty.peaceful.info",
        "Health and hunger refill on their own.",
    ),
    (
        "options.difficulty.easy.info",
        "Hunger never takes your last hearts.",
    ),
    (
        "options.difficulty.normal.info",
        "Hunger can leave you on half a heart.",
    ),
    ("options.difficulty.hard.info", "Hunger can be fatal."),
    ("difficulty.lock.title", "Lock the Difficulty"),
    (
        "difficulty.lock.question",
        "Lock this world at %s? It cannot be changed again.",
    ),
    ("narrator.button.difficulty_lock", "Difficulty lock"),
    ("debug.gamemodes.select_next", "%s Next"),
    (
        "debug.gamemodes.error",
        "You may not switch game modes here",
    ),
    ("debug.gamemodes.press_f4", "[ F4 ]"),
    ("menu.returnToMenu", "Save and Quit"),
    ("menu.disconnect", "Disconnect"),
    ("menu.shareToLan", "Open to the Network"),
    ("menu.multiplayer", "Play Together"),
    ("menu.singleplayer", "Play Alone"),
    ("menu.quit", "Quit"),
    ("gui.stats", "Statistics"),
    ("gui.advancements", "Progress"),
    ("options.worldOptions.button", "World Options..."),
    ("options.worldOptions.title", "World Options"),
    ("options.worldOptions.general.title", "General"),
    ("options.worldOptions.multiplayer.title", "Playing Together"),
    ("options.worldOptions.game_mode", "Game Mode"),
    ("options.worldOptions.personal_game_mode", "Your Game Mode"),
    (
        "options.worldOptions.guest.command_access",
        "Guests Can Use Commands",
    ),
    (
        "options.worldOptions.guest.force_game_mode",
        "Guests' Game Mode",
    ),
    ("selectWorld.allowCommands", "Commands"),
    (
        "selectWorld.allowCommands.info",
        "Lets you type commands such as /gamemode.",
    ),
    ("editGamerule.inGame.button", "Game Rules..."),
    ("restrictions_screen.button", "Restrictions..."),
    ("restrictions_screen.title", "Restrictions"),
    ("menu.multiplayerOptions.lan", "Local Network"),
    ("menu.multiplayerOptions.applyChanges", "Apply"),
    ("lanServer.port", "Port"),
    (
        "menu.multiplayerOptions.publish.started.lan",
        "The world is open on port %s",
    ),
    (
        "menu.multiplayerOptions.publish.stopped",
        "The world is closed to the network",
    ),
    (
        "commands.publish.failed",
        "The world could not be opened to the network",
    ),
    ("commands.gamerule.set", "Game rule %s is now %s"),
    (
        "chat_restriction.chat_and_commands_disabled_by_options",
        "Chat and commands are off in the chat settings",
    ),
    (
        "chat_restriction.chat_disabled_by_options",
        "Chat is off in the chat settings",
    ),
    (
        "chat_restriction.chat_disabled_by_options.action",
        "Open the chat settings",
    ),
    ("stat.generalButton", "General"),
    ("stat.itemsButton", "Items"),
    ("stat.mobsButton", "Creatures"),
    ("stats.none", "-"),
    ("stat_type.minecraft.mined", "Mined"),
    ("stat_type.minecraft.crafted", "Crafted"),
    ("stat_type.minecraft.used", "Used"),
    ("stat_type.minecraft.broken", "Worn Out"),
    ("stat_type.minecraft.picked_up", "Picked Up"),
    ("stat_type.minecraft.dropped", "Dropped"),
    ("stat_type.minecraft.killed", "You defeated %s %s"),
    (
        "stat_type.minecraft.killed.none",
        "You have never defeated %s",
    ),
    (
        "stat_type.minecraft.killed_by",
        "%s defeated you %s time(s)",
    ),
    (
        "stat_type.minecraft.killed_by.none",
        "%s has never defeated you",
    ),
    ("connect.authorizing", "Signing in..."),
    ("connect.encrypting", "Securing the connection..."),
    ("connect.connecting", "Connecting..."),
    ("connect.failed", "Could not connect"),
    ("disconnect.lost", "Connection lost"),
    ("disconnect.timeout", "The server stopped answering"),
    ("disconnect.disconnected", "Disconnected"),
    ("disconnect.closed", "The connection was closed"),
    ("disconnect.kicked", "You were removed from the server"),
    ("disconnect.genericReason", "%s"),
    ("disconnect.endOfStream", "The connection ended"),
    ("disconnect.loginFailedInfo", "Could not sign in: %s"),
    (
        "disconnect.loginFailedInfo.serversUnavailable",
        "the sign-in service is not reachable",
    ),
    (
        "disconnect.loginFailedInfo.invalidSession",
        "your session is not valid; restart the game",
    ),
    (
        "disconnect.loginFailedInfo.insufficientPrivileges",
        "your account may not play online",
    ),
    (
        "disconnect.loginFailedInfo.userBanned",
        "your account is banned from online play",
    ),
    ("multiplayer.player.joined", "%s joined"),
    ("multiplayer.player.left", "%s left"),
    ("multiplayer.player.joined.renamed", "%s (once %s) joined"),
    ("multiplayer.player.list.hp", "%s hp"),
    ("multiplayer.title", "Play Together"),
    ("multiplayer.status.player_count", "%s/%s"),
    ("multiplayer.status.unknown", "???"),
    ("multiplayer.status.ping", "%s ms"),
    ("multiplayer.status.no_connection", "(no connection)"),
    ("multiplayer.status.pinging", "Asking..."),
    (
        "multiplayer.status.cannot_connect",
        "Cannot reach this server",
    ),
    (
        "multiplayer.requiredTexturePrompt.disconnect",
        "This server requires its resource pack",
    ),
    (
        "multiplayer.disconnect.duplicate_login",
        "You signed in somewhere else",
    ),
    ("multiplayer.disconnect.server_full", "The server is full"),
    (
        "multiplayer.disconnect.slow_login",
        "Signing in took too long",
    ),
    (
        "multiplayer.disconnect.invalid_player_movement",
        "The server rejected your movement",
    ),
    (
        "multiplayer.disconnect.invalid_vehicle_movement",
        "The server rejected your vehicle's movement",
    ),
    (
        "multiplayer.disconnect.server_shutdown",
        "The server closed",
    ),
    (
        "multiplayer.disconnect.authservers_down",
        "The sign-in service is down; try again later",
    ),
    (
        "multiplayer.disconnect.invalid_name",
        "That player name is not valid",
    ),
    (
        "multiplayer.disconnect.outdated_client",
        "This game is older than the server (%s)",
    ),
    (
        "multiplayer.disconnect.incompatible",
        "This game does not match the server (%s)",
    ),
    (
        "multiplayer.disconnect.transfers_disabled",
        "This server does not accept transfers",
    ),
    (
        "multiplayer.disconnect.unverified_username",
        "Your name could not be verified",
    ),
    (
        "multiplayer.disconnect.chat_validation_failed",
        "A chat message could not be verified",
    ),
    (
        "multiplayer.disconnect.expired_public_key",
        "Your chat key has expired",
    ),
    (
        "multiplayer.disconnect.illegal_characters",
        "That message has characters chat does not allow",
    ),
    (
        "multiplayer.disconnect.too_many_pending_chats",
        "Too many chat messages at once",
    ),
    (
        "multiplayer.disconnect.not_whitelisted",
        "This server only admits listed players",
    ),
    ("multiplayer.disconnect.kicked", "An operator removed you"),
    (
        "multiplayer.disconnect.banned",
        "You are banned from this server",
    ),
    (
        "chat.disabled.invalid_command_signature",
        "A command could not be verified",
    ),
    ("chat.type.text", "<%s> %s"),
    ("chat.type.emote", "* %s %s"),
    ("chat.type.admin", "[%s: %s]"),
    ("chat.type.team.text", "%s <%s> %s"),
    ("chat.type.team.sent", "-> %s <%s> %s"),
    ("chat.copy.click", "Click to copy"),
    ("chat.copy", "Copy"),
    (
        "commands.message.display.incoming",
        "%s whispers to you: %s",
    ),
    ("commands.message.display.outgoing", "You whisper to %s: %s"),
    ("selectServer.select", "Join"),
    ("selectServer.direct", "Direct Connection"),
    ("selectServer.add", "Add Server"),
    ("selectServer.edit", "Edit"),
    ("selectServer.delete", "Delete"),
    ("selectServer.refresh", "Refresh"),
    ("selectServer.deleteButton", "Delete"),
    (
        "selectServer.deleteQuestion",
        "Remove this server from the list?",
    ),
    (
        "selectServer.deleteWarning",
        "'%s' will be gone from the list.",
    ),
    ("addServer.enterIp", "Server Address"),
    ("addServer.enterName", "Server Name"),
    ("addServer.title", "Server Details"),
    ("lanServer.scanning", "Looking for games on your network"),
    ("lanServer.title", "Network Game"),
    ("gui.selectWorld.search", "Search worlds..."),
    ("selectWorld.title", "Worlds"),
    ("selectWorld.commands", "Commands"),
    ("selectWorld.version", "Version:"),
    ("selectWorld.edit", "Edit"),
    ("selectWorld.delete", "Delete"),
    ("selectWorld.recreate", "Re-create"),
    ("selectWorld.edit.title", "Edit the World"),
    ("selectWorld.enterName", "World Name"),
    ("selectWorld.edit.resetIcon", "Reset the Picture"),
    ("selectWorld.edit.openFolder", "Open the World's Folder"),
    ("selectWorld.edit.backup", "Make a Backup"),
    ("selectWorld.edit.backupFolder", "Open the Backups Folder"),
    ("selectWorld.edit.optimize", "Optimize the World"),
    ("selectWorld.edit.save", "Save"),
    ("selectWorld.edit.backupCreated", "Backed up: %s"),
    ("selectWorld.edit.backupSize", "Size: %s MB"),
    ("selectWorld.edit.backupFailed", "The backup failed"),
    ("selectWorld.waitingForBackup.title", "Making a backup"),
    (
        "selectWorld.waitingForBackup.message",
        "This may take a moment.",
    ),
    ("selectWorld.deleteQuestion", "Delete this world?"),
    ("selectWorld.deleteWarning", "'%s' will be lost for good."),
    ("selectWorld.deleteButton", "Delete"),
    (
        "selectWorld.access_failure",
        "The world could not be opened",
    ),
    ("selectWorld.create", "Create the World"),
    ("selectWorld.mapType", "World Type"),
    ("selectWorld.customizeType", "Customize"),
    ("selectWorld.mapFeatures", "Structures"),
    (
        "selectWorld.mapFeatures.info",
        "Villages and other structures (none in this edition)",
    ),
    ("selectWorld.bonusItems", "Starter Chest"),
    ("selectWorld.gameRules", "Game Rules"),
    ("selectWorld.experiments", "Experiments"),
    ("selectWorld.dataPacks", "Data Packs"),
    ("selectWorld.enterSeed", "Seed"),
    ("selectWorld.seedInfo", "Leave empty for a random seed"),
    ("selectWorld.targetFolder", "Saved in: %s"),
    ("createWorld.tab.game.title", "Game"),
    ("createWorld.tab.world.title", "World"),
    ("createWorld.tab.more.title", "More"),
    ("generator.minecraft.normal", "Default"),
    ("generator.minecraft.flat", "Flat"),
    ("death.attack.generic", "%1$s died"),
    ("death.attack.generic.player", "%1$s died because of %2$s"),
    (
        "death.attack.genericKill",
        "%1$s was removed from the world",
    ),
    (
        "death.attack.genericKill.player",
        "%1$s was removed from the world by %2$s",
    ),
    ("death.attack.outOfWorld", "%1$s fell out of the world"),
    (
        "death.attack.outOfWorld.player",
        "%1$s fell out of the world fleeing %2$s",
    ),
    (
        "death.attack.outsideBorder",
        "%1$s left the edge of the world",
    ),
    (
        "death.attack.outsideBorder.player",
        "%1$s left the edge of the world fleeing %2$s",
    ),
    ("death.attack.cactus", "%1$s was pricked to death"),
    (
        "death.attack.cactus.player",
        "%1$s ran into a cactus fleeing %2$s",
    ),
    ("death.attack.cramming", "%1$s was squeezed too tight"),
    ("death.attack.cramming.player", "%1$s was squeezed by %2$s"),
    ("death.attack.drown", "%1$s drowned"),
    ("death.attack.drown.player", "%1$s drowned fleeing %2$s"),
    ("death.attack.dryout", "%1$s dried out"),
    ("death.attack.dryout.player", "%1$s dried out fleeing %2$s"),
    ("death.attack.explosion", "%1$s blew up"),
    ("death.attack.explosion.player", "%1$s was blown up by %2$s"),
    ("death.attack.fall", "%1$s hit the ground too hard"),
    (
        "death.attack.fall.player",
        "%1$s hit the ground too hard fleeing %2$s",
    ),
    (
        "death.attack.fallingBlock",
        "%1$s was buried by a falling block",
    ),
    (
        "death.attack.fallingBlock.player",
        "%1$s was buried by a falling block fleeing %2$s",
    ),
    ("death.attack.flyIntoWall", "%1$s flew into a wall"),
    (
        "death.attack.flyIntoWall.player",
        "%1$s flew into a wall fleeing %2$s",
    ),
    ("death.attack.freeze", "%1$s froze"),
    ("death.attack.freeze.player", "%1$s froze fleeing %2$s"),
    ("death.attack.hotFloor", "%1$s stood on something too hot"),
    (
        "death.attack.hotFloor.player",
        "%1$s stood on something too hot fleeing %2$s",
    ),
    ("death.attack.inFire", "%1$s burned"),
    (
        "death.attack.inFire.player",
        "%1$s walked into fire fleeing %2$s",
    ),
    ("death.attack.inWall", "%1$s suffocated in a wall"),
    (
        "death.attack.inWall.player",
        "%1$s suffocated in a wall fleeing %2$s",
    ),
    ("death.attack.lava", "%1$s fell into lava"),
    (
        "death.attack.lava.player",
        "%1$s fell into lava fleeing %2$s",
    ),
    ("death.attack.magic", "%1$s was undone by magic"),
    (
        "death.attack.magic.player",
        "%1$s was undone by the magic of %2$s",
    ),
    ("death.attack.mob", "%1$s was defeated by %2$s"),
    ("death.attack.mob.player", "%1$s was defeated by %2$s"),
    ("death.attack.onFire", "%1$s burned away"),
    (
        "death.attack.onFire.player",
        "%1$s burned away fighting %2$s",
    ),
    ("death.attack.player", "%1$s was defeated by %2$s"),
    ("death.attack.player.player", "%1$s was defeated by %2$s"),
    ("death.attack.stalagmite", "%1$s landed on a spike"),
    (
        "death.attack.stalagmite.player",
        "%1$s landed on a spike fighting %2$s",
    ),
    ("death.attack.starve", "%1$s starved"),
    ("death.attack.starve.player", "%1$s starved fighting %2$s"),
    (
        "death.attack.sweetBerryBush",
        "%1$s was scratched to death by a bush",
    ),
    (
        "death.attack.sweetBerryBush.player",
        "%1$s was scratched to death by a bush fleeing %2$s",
    ),
    ("death.attack.anvil", "%1$s was flattened"),
    (
        "death.attack.anvil.player",
        "%1$s was flattened fighting %2$s",
    ),
    ("death.attack.lightningBolt", "%1$s was struck by lightning"),
    (
        "death.attack.lightningBolt.player",
        "%1$s was struck by lightning fighting %2$s",
    ),
    ("death.attack.wither", "%1$s withered away"),
    (
        "death.attack.wither.player",
        "%1$s withered away fighting %2$s",
    ),
    ("death.attack.arrow", "%1$s was shot by %2$s"),
    ("death.attack.arrow.player", "%1$s was shot by %2$s"),
    (
        "death.attack.thrown",
        "%1$s was hit by something %2$s threw",
    ),
    (
        "death.attack.thrown.player",
        "%1$s was hit by something %2$s threw",
    ),
    ("death.fell.accident.generic", "%1$s fell from a high place"),
    ("death.fell.accident.ladder", "%1$s fell off a ladder"),
    ("death.fell.accident.vines", "%1$s fell off some vines"),
    (
        "death.fell.accident.weeping_vines",
        "%1$s fell off some vines",
    ),
    (
        "death.fell.accident.twisting_vines",
        "%1$s fell off some vines",
    ),
    (
        "death.fell.accident.scaffolding",
        "%1$s fell off scaffolding",
    ),
    (
        "death.fell.accident.other_climbable",
        "%1$s lost their grip",
    ),
    ("death.fell.accident.water", "%1$s fell out of the water"),
    ("death.fell.assist", "%1$s was knocked off a ledge by %2$s"),
    (
        "death.fell.finish",
        "%1$s fell too far and was finished off by %2$s",
    ),
    ("death.fell.killer", "%1$s was made to fall"),
    ("commands.clear.success.single", "Took %s item(s) from %s"),
    (
        "commands.clear.success.multiple",
        "Took %s item(s) from %s players",
    ),
    (
        "commands.clear.test.single",
        "%2$s holds %1$s matching item(s)",
    ),
    (
        "commands.clear.test.multiple",
        "%2$s players hold %1$s matching item(s)",
    ),
    ("clear.failed.single", "%s has no such items"),
    (
        "clear.failed.multiple",
        "None of the %s players have such items",
    ),
    (
        "commands.clone.overlap",
        "The source and destination overlap",
    ),
    (
        "commands.clone.toobig",
        "That is too many blocks (at most %s, you named %s)",
    ),
    ("commands.clone.failed", "No blocks were copied"),
    ("commands.clone.success", "Copied %s block(s)"),
    (
        "commands.defaultgamemode.success",
        "New players now start in %s mode",
    ),
    ("commands.difficulty.query", "The difficulty is %s"),
    ("commands.difficulty.success", "The difficulty is now %s"),
    (
        "commands.difficulty.failure",
        "The difficulty is already %s",
    ),
    (
        "commands.experience.query.points",
        "%s has %s experience points",
    ),
    (
        "commands.experience.query.levels",
        "%s has %s experience levels",
    ),
    (
        "commands.experience.add.points.success.single",
        "Gave %s experience points to %s",
    ),
    (
        "commands.experience.add.points.success.multiple",
        "Gave %s experience points to %s players",
    ),
    (
        "commands.experience.add.levels.success.single",
        "Gave %s experience levels to %s",
    ),
    (
        "commands.experience.add.levels.success.multiple",
        "Gave %s experience levels to %s players",
    ),
    (
        "commands.experience.set.points.success.single",
        "Set %2$s's experience points to %1$s",
    ),
    (
        "commands.experience.set.points.success.multiple",
        "Set the experience points of %2$s players to %1$s",
    ),
    (
        "commands.experience.set.levels.success.single",
        "Set %2$s's experience level to %1$s",
    ),
    (
        "commands.experience.set.levels.success.multiple",
        "Set the experience level of %2$s players to %1$s",
    ),
    (
        "commands.experience.set.points.invalid",
        "That is more points than the current level holds",
    ),
    (
        "commands.fill.toobig",
        "That is too many blocks (at most %s, you named %s)",
    ),
    ("commands.fill.failed", "No blocks were changed"),
    ("commands.fill.success", "Changed %s block(s)"),
    ("commands.gamemode.success.self", "You are now in %s mode"),
    ("commands.gamemode.success.other", "%s is now in %s mode"),
    (
        "commands.give.failed.toomanyitems",
        "At most %s of %s can be given at once",
    ),
    ("commands.give.success.single", "Gave %s %s to %s"),
    ("commands.give.success.multiple", "Gave %s %s to %s players"),
    (
        "commands.help.failed",
        "No such command, or you may not use it",
    ),
    ("commands.kill.success.single", "Removed %s"),
    ("commands.kill.success.multiple", "Removed %s entities"),
    (
        "commands.list.players",
        "%s of at most %s players are here: %s",
    ),
    ("commands.list.nameAndId", "%s (%s)"),
    ("commands.seed.success", "Seed: %s"),
    ("commands.setblock.failed", "The block could not be placed"),
    (
        "commands.setblock.success",
        "Placed the block at %s, %s, %s",
    ),
    (
        "commands.setworldspawn.success",
        "The world's spawn point is now %1$s, %2$s, %3$s [%4$s, %5$s] in %6$s",
    ),
    (
        "commands.spawnpoint.success.single",
        "%7$s's spawn point is now %1$s, %2$s, %3$s [%4$s, %5$s] in %6$s",
    ),
    (
        "commands.spawnpoint.success.multiple",
        "The spawn point of %7$s players is now %1$s, %2$s, %3$s [%4$s, %5$s] in %6$s",
    ),
    (
        "commands.teleport.success.entity.multiple",
        "Moved %s entities to %s",
    ),
    (
        "commands.teleport.success.location.multiple",
        "Moved %s entities to %s, %s, %s",
    ),
    ("commands.time.query.absolute", "The %s clock reads %s"),
    ("commands.time.query.timeline", "%s is at %s"),
    (
        "commands.time.query.timeline.repetitions",
        "%s has run %s time(s)",
    ),
    (
        "commands.time.wrong_timeline_for_clock",
        "%s does not follow the %s clock",
    ),
    ("commands.time.pause", "The %s clock is paused"),
    ("commands.time.resume", "The %s clock runs again"),
    ("commands.time.rate", "The %s clock now runs at %s"),
    (
        "commands.time.rate.already_same",
        "The %s clock already runs at %s",
    ),
    ("commands.publish.started", "The world is open on port %s"),
    ("command.context.parse_error", "%s at position %s: %s"),
    ("command.failed", "The command could not run"),
    ("command.exception", "The command failed: %s"),
    ("permissions.requires.player", "Only a player can run this"),
    ("permissions.requires.entity", "Only an entity can run this"),
    ("parsing.expected", "Expected '%s'"),
    ("parsing.bool.expected", "Expected true or false"),
    ("parsing.bool.invalid", "'%s' is not true or false"),
    ("parsing.int.invalid", "'%s' is not a whole number"),
    ("parsing.float.invalid", "'%s' is not a number"),
    ("parsing.double.expected", "A number is needed"),
    ("parsing.double.invalid", "'%s' is not a number"),
    ("parsing.quote.escape", "'\\%s' cannot be escaped in quotes"),
    ("parsing.quote.expected.end", "The quoted text never ends"),
    ("argument.literal.incorrect", "Expected %s"),
    ("argument.entity.notfound.player", "No player was found"),
    ("argument.entity.notfound.entity", "No entity was found"),
    ("argument.player.entities", "Only players can be named here"),
    (
        "argument.player.toomany",
        "Only one player can be named here",
    ),
    (
        "argument.entity.toomany",
        "Only one entity can be named here",
    ),
    ("argument.entity.invalid", "That is not a name or an ID"),
    (
        "argument.entity.selector.missing",
        "The selector needs a type (@p, @a, @r, @s, @e, @n)",
    ),
    (
        "argument.entity.selector.not_allowed",
        "Selectors are not allowed here",
    ),
    ("argument.entity.selector.unknown", "No such selector: %s"),
    (
        "argument.entity.selector.nearestPlayer",
        "The nearest player",
    ),
    ("argument.entity.selector.allPlayers", "Every player"),
    ("argument.entity.selector.randomPlayer", "A random player"),
    ("argument.entity.selector.self", "You"),
    ("argument.entity.selector.allEntities", "Every entity"),
    (
        "argument.entity.selector.nearestEntity",
        "The nearest entity",
    ),
    ("argument.entity.options.unknown", "No such option: %s"),
    (
        "argument.entity.options.inapplicable",
        "Option %s does not apply here",
    ),
    (
        "argument.entity.options.valueless",
        "Option %s needs a value",
    ),
    (
        "argument.entity.options.mode.invalid",
        "No such game mode: %s",
    ),
    (
        "argument.entity.options.sort.irreversible",
        "Sort order %s cannot be negated",
    ),
    (
        "argument.entity.options.type.invalid",
        "No such entity type: %s",
    ),
    (
        "argument.entity.options.unterminated",
        "The options never end",
    ),
    (
        "argument.entity.options.distance.negative",
        "Distance cannot be negative",
    ),
    (
        "argument.entity.options.level.negative",
        "Level cannot be negative",
    ),
    (
        "argument.entity.options.limit.toosmall",
        "The limit must be at least 1",
    ),
    ("argument.pos.missing.int", "A block position is needed"),
    ("argument.pos.missing.double", "A position is needed"),
    (
        "argument.pos.mixed",
        "Local (^) and world coordinates cannot be mixed",
    ),
    ("argument.pos.unloaded", "That position is not loaded"),
    (
        "argument.pos.outofworld",
        "That position is outside the world",
    ),
    ("argument.pos2d.incomplete", "Two coordinates are needed"),
    ("argument.rotation.incomplete", "Two angles are needed"),
    ("argument.angle.incomplete", "An angle is needed"),
    ("argument.angle.invalid", "That angle is not valid"),
    ("argument.gamemode.invalid", "No such game mode: %s"),
    (
        "argument.integer.low",
        "The number must be at least %s; it is %s",
    ),
    (
        "argument.integer.big",
        "The number must be at most %s; it is %s",
    ),
    (
        "argument.float.low",
        "The number must be at least %s; it is %s",
    ),
    (
        "argument.float.big",
        "The number must be at most %s; it is %s",
    ),
    (
        "argument.double.low",
        "The number must be at least %s; it is %s",
    ),
    (
        "argument.double.big",
        "The number must be at most %s; it is %s",
    ),
    (
        "argument.message.too_long",
        "The message is too long (%s of at most %s characters)",
    ),
    ("argument.id.invalid", "That ID is not valid"),
    ("argument.uuid.invalid", "That UUID is not valid"),
    ("argument.range.empty", "A value or a range is needed"),
    (
        "argument.range.swapped",
        "The smallest value cannot be larger than the largest",
    ),
    ("argument.enum.invalid", "'%s' is not one of the choices"),
    (
        "argument.time.tick_count_too_low",
        "The time must be at least %s ticks; it is %s",
    ),
    ("argument.anchor.invalid", "No such anchor: %s"),
    ("argument.color.invalid", "No such colour: %s"),
    (
        "argument.hexcolor.invalid",
        "'%s' is not a hexadecimal colour",
    ),
    ("argument.resource.not_found", "No %2$s named '%1$s'"),
    ("argument.resource.invalid_type", "'%s' is a %s, not a %s"),
    (
        "argument.resource_tag.not_found",
        "No %2$s tag named '%1$s'",
    ),
    (
        "argument.resource_selector.not_found",
        "Nothing of type %2$s matches '%1$s'",
    ),
    (
        "argument.resource_or_id.no_such_element",
        "No %2$s named '%1$s'",
    ),
    (
        "argument.resource_or_id.failed_to_parse",
        "Could not read: %s",
    ),
    (
        "argument.component.invalid",
        "That text component is not valid: %s",
    ),
    ("argument.style.invalid", "That style is not valid: %s"),
    ("argument.nbt.invalid", "That data is not valid: %s"),
    ("argument.nbt.trailing", "Unexpected text after the data"),
    ("argument.nbt.expected.compound", "A compound tag is needed"),
    ("argument.block.id.invalid", "No such block: %s"),
    (
        "argument.block.property.duplicate",
        "Property %s of %s is given twice",
    ),
    (
        "argument.block.property.invalid",
        "%s does not accept '%s' for %s",
    ),
    (
        "argument.block.property.novalue",
        "Property %s of %s needs a value",
    ),
    ("argument.block.property.unknown", "%s has no property %s"),
    (
        "argument.block.property.unclosed",
        "Expected ] to close the block's properties",
    ),
    (
        "argument.block.tag.disallowed",
        "Tags are not allowed here, only blocks",
    ),
    ("arguments.block.tag.unknown", "No such block tag: %s"),
    ("argument.item.id.invalid", "No such item: %s"),
    ("arguments.item.component.expected", "A component is needed"),
    (
        "arguments.item.component.malformed",
        "Component %s is not valid: %s",
    ),
    (
        "arguments.item.component.repeated",
        "Component %s is given twice",
    ),
    ("arguments.item.component.unknown", "No such component: %s"),
    ("arguments.item.tag.unknown", "No such item tag: %s"),
    ("arguments.item.predicate.unknown", "No such item test: %s"),
    (
        "arguments.item.predicate.malformed",
        "Item test %s is not valid: %s",
    ),
    (
        "arguments.nbtpath.node.invalid",
        "That data path is not valid",
    ),
    ("arguments.nbtpath.nothing_found", "Nothing matches %s"),
    (
        "arguments.nbtpath.too_deep",
        "The data is nested too deeply",
    ),
    ("arguments.operation.invalid", "No such operation"),
    (
        "arguments.swizzle.invalid",
        "Use a combination of x, y and z",
    ),
    ("slot.unknown", "No such slot: %s"),
    (
        "slot.only_single_allowed",
        "Only one slot can be named here, not %s",
    ),
    ("particle.notFound", "No such particle: %s"),
    (
        "particle.invalidOptions",
        "Particle options are not valid: %s",
    ),
    ("argument.entity.options.name.description", "Entity name"),
    ("argument.entity.options.distance.description", "Distance"),
    (
        "argument.entity.options.level.description",
        "Experience level",
    ),
    ("argument.entity.options.x.description", "X position"),
    ("argument.entity.options.y.description", "Y position"),
    ("argument.entity.options.z.description", "Z position"),
    ("argument.entity.options.dx.description", "Width (X)"),
    ("argument.entity.options.dy.description", "Height (Y)"),
    ("argument.entity.options.dz.description", "Depth (Z)"),
    ("argument.entity.options.x_rotation.description", "Pitch"),
    ("argument.entity.options.y_rotation.description", "Yaw"),
    (
        "argument.entity.options.limit.description",
        "How many at most",
    ),
    ("argument.entity.options.sort.description", "Order"),
    ("argument.entity.options.gamemode.description", "Game mode"),
    ("argument.entity.options.team.description", "Team"),
    ("argument.entity.options.type.description", "Entity type"),
    ("argument.entity.options.tag.description", "Tag"),
    ("argument.entity.options.nbt.description", "Data"),
    ("argument.entity.options.scores.description", "Scores"),
    (
        "argument.entity.options.advancements.description",
        "Progress",
    ),
    ("argument.entity.options.predicate.description", "Predicate"),
    ("stat.minecraft.leave_game", "Times Left"),
    ("stat.minecraft.play_time", "Time Played"),
    (
        "stat.minecraft.total_world_time",
        "Time with the World Open",
    ),
    ("stat.minecraft.time_since_death", "Since the Last Death"),
    ("stat.minecraft.time_since_rest", "Since the Last Rest"),
    ("stat.minecraft.sneak_time", "Time Crouching"),
    ("stat.minecraft.walk_one_cm", "Distance Walked"),
    ("stat.minecraft.crouch_one_cm", "Distance Crouched"),
    ("stat.minecraft.sprint_one_cm", "Distance Run"),
    (
        "stat.minecraft.walk_on_water_one_cm",
        "Distance Walked on Water",
    ),
    ("stat.minecraft.fall_one_cm", "Distance Fallen"),
    ("stat.minecraft.climb_one_cm", "Distance Climbed"),
    ("stat.minecraft.fly_one_cm", "Distance Flown"),
    (
        "stat.minecraft.walk_under_water_one_cm",
        "Distance Walked under Water",
    ),
    ("stat.minecraft.minecart_one_cm", "Distance by Cart"),
    ("stat.minecraft.boat_one_cm", "Distance by Boat"),
    ("stat.minecraft.pig_one_cm", "Distance by Pig"),
    ("stat.minecraft.happy_ghast_one_cm", "Distance by Ghast"),
    ("stat.minecraft.horse_one_cm", "Distance on Horseback"),
    ("stat.minecraft.aviate_one_cm", "Distance Glided"),
    ("stat.minecraft.swim_one_cm", "Distance Swum"),
    ("stat.minecraft.strider_one_cm", "Distance by Strider"),
    ("stat.minecraft.nautilus_one_cm", "Distance by Nautilus"),
    ("stat.minecraft.jump", "Jumps"),
    ("stat.minecraft.drop", "Items Dropped"),
    ("stat.minecraft.damage_dealt", "Damage Dealt"),
    (
        "stat.minecraft.damage_dealt_absorbed",
        "Damage Dealt (Absorbed)",
    ),
    (
        "stat.minecraft.damage_dealt_resisted",
        "Damage Dealt (Resisted)",
    ),
    ("stat.minecraft.damage_taken", "Damage Taken"),
    ("stat.minecraft.damage_blocked_by_shield", "Damage Blocked"),
    ("stat.minecraft.damage_absorbed", "Damage Absorbed"),
    ("stat.minecraft.damage_resisted", "Damage Resisted"),
    ("stat.minecraft.deaths", "Deaths"),
    ("stat.minecraft.mob_kills", "Creatures Defeated"),
    ("stat.minecraft.animals_bred", "Animals Bred"),
    ("stat.minecraft.player_kills", "Players Defeated"),
    ("stat.minecraft.fish_caught", "Fish Caught"),
    (
        "stat.minecraft.talked_to_villager",
        "Villager Conversations",
    ),
    ("stat.minecraft.traded_with_villager", "Trades"),
    ("stat.minecraft.eat_cake_slice", "Cake Slices Eaten"),
    ("stat.minecraft.fill_cauldron", "Cauldrons Filled"),
    ("stat.minecraft.use_cauldron", "Water Taken from Cauldrons"),
    ("stat.minecraft.clean_armor", "Armor Pieces Washed"),
    ("stat.minecraft.clean_banner", "Banners Washed"),
    ("stat.minecraft.clean_shulker_box", "Boxes Washed"),
    (
        "stat.minecraft.interact_with_brewingstand",
        "Brewing Stand Uses",
    ),
    ("stat.minecraft.interact_with_beacon", "Beacon Uses"),
    ("stat.minecraft.inspect_dropper", "Droppers Searched"),
    ("stat.minecraft.inspect_hopper", "Hoppers Searched"),
    ("stat.minecraft.inspect_dispenser", "Dispensers Searched"),
    ("stat.minecraft.play_noteblock", "Note Blocks Played"),
    ("stat.minecraft.tune_noteblock", "Note Blocks Tuned"),
    ("stat.minecraft.pot_flower", "Plants Potted"),
    (
        "stat.minecraft.trigger_trapped_chest",
        "Trapped Chests Sprung",
    ),
    ("stat.minecraft.open_enderchest", "Ender Chests Opened"),
    ("stat.minecraft.enchant_item", "Items Enchanted"),
    ("stat.minecraft.play_record", "Records Played"),
    ("stat.minecraft.interact_with_furnace", "Furnace Uses"),
    (
        "stat.minecraft.interact_with_crafting_table",
        "Crafting Table Uses",
    ),
    ("stat.minecraft.open_chest", "Chests Opened"),
    ("stat.minecraft.sleep_in_bed", "Nights Slept in a Bed"),
    ("stat.minecraft.sleep_in_straw_bed", "Nights Slept on Straw"),
    ("stat.minecraft.open_shulker_box", "Boxes Opened"),
    ("stat.minecraft.open_barrel", "Barrels Opened"),
    (
        "stat.minecraft.interact_with_blast_furnace",
        "Blast Furnace Uses",
    ),
    ("stat.minecraft.interact_with_smoker", "Smoker Uses"),
    ("stat.minecraft.interact_with_lectern", "Lectern Uses"),
    ("stat.minecraft.interact_with_campfire", "Campfire Uses"),
    (
        "stat.minecraft.interact_with_cartography_table",
        "Cartography Table Uses",
    ),
    ("stat.minecraft.interact_with_loom", "Loom Uses"),
    (
        "stat.minecraft.interact_with_stonecutter",
        "Stonecutter Uses",
    ),
    ("stat.minecraft.bell_ring", "Bells Rung"),
    ("stat.minecraft.raid_trigger", "Raids Started"),
    ("stat.minecraft.raid_win", "Raids Won"),
    ("stat.minecraft.interact_with_anvil", "Anvil Uses"),
    ("stat.minecraft.interact_with_grindstone", "Grindstone Uses"),
    ("stat.minecraft.target_hit", "Targets Hit"),
    (
        "stat.minecraft.interact_with_smithing_table",
        "Smithing Table Uses",
    ),
];

/// Keyboard key names: the named keys, then letters, digits and function keys.
pub fn key_names() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = [
        ("space", "Space"),
        ("tab", "Tab"),
        ("enter", "Enter"),
        ("escape", "Escape"),
        ("backspace", "Backspace"),
        ("delete", "Delete"),
        ("insert", "Insert"),
        ("home", "Home"),
        ("end", "End"),
        ("page.up", "Page Up"),
        ("page.down", "Page Down"),
        ("up", "Up Arrow"),
        ("down", "Down Arrow"),
        ("left", "Left Arrow"),
        ("right", "Right Arrow"),
        ("left.shift", "Left Shift"),
        ("right.shift", "Right Shift"),
        ("left.control", "Left Ctrl"),
        ("right.control", "Right Ctrl"),
        ("left.alt", "Left Alt"),
        ("right.alt", "Right Alt"),
        ("left.win", "Left Super"),
        ("right.win", "Right Super"),
        ("caps.lock", "Caps Lock"),
        ("num.lock", "Num Lock"),
        ("scroll.lock", "Scroll Lock"),
        ("print.screen", "Print Screen"),
        ("pause", "Pause"),
        ("menu", "Menu"),
        ("grave.accent", "`"),
        ("minus", "-"),
        ("equal", "="),
        ("left.bracket", "["),
        ("right.bracket", "]"),
        ("backslash", "\\"),
        ("semicolon", ";"),
        ("apostrophe", "'"),
        ("comma", ","),
        ("period", "."),
        ("slash", "/"),
    ]
    .into_iter()
    .map(|(k, v)| (format!("key.keyboard.{k}"), v.to_owned()))
    .collect();
    for c in 'a'..='z' {
        out.push((
            format!("key.keyboard.{c}"),
            c.to_ascii_uppercase().to_string(),
        ));
    }
    for d in 0..=9 {
        out.push((format!("key.keyboard.{d}"), d.to_string()));
        out.push((format!("key.keyboard.keypad.{d}"), format!("Keypad {d}")));
    }
    for f in 1..=25 {
        out.push((format!("key.keyboard.f{f}"), format!("F{f}")));
    }
    out
}

// EVERY KEY
//
// Beyond the hand-written labels above, the bundle has an English line for every key
// Minecraft's interface can show: the names of everything in the registries (from the name
// lists in `data/registries/`, title-cased from the id) and every other key of
// `data/lang/keys.txt` (written by `tools/open_lang_keys.py`, with each key's placeholders in
// order), worded here by rules on the key or by the short table below. The placeholders are
// Minecraft's, in Minecraft's order, as the client formats them so.

/// Every key the generator writes besides the registry names, with its placeholders.
const KEYS: &str = include_str!("../data/lang/keys.txt");

/// Short English for keys whose rule-made wording would read poorly, by key.
const PROSE: &[(&str, &str)] = &[
    ("container.barrel", "Barrel"),
    ("container.beacon", "Beacon"),
    ("container.beehive.bees", "Bees: %s of %s"),
    ("container.beehive.honey", "Honey: %s of %s"),
    ("container.blast_furnace", "Blast Furnace"),
    ("container.brewing", "Brewing Stand"),
    ("container.cartography_table", "Cartography Table"),
    ("container.chest", "Chest"),
    ("container.chestDouble", "Large Chest"),
    ("container.crafter", "Crafter"),
    ("container.crafting", "Crafting"),
    ("container.creative", "Pick an Item"),
    ("container.dispenser", "Dispenser"),
    ("container.dropper", "Dropper"),
    ("container.enchant", "Enchant"),
    ("container.enchant.clue", "%s . . . ?"),
    ("container.enchant.lapis.many", "%s Lapis Lazuli"),
    ("container.enchant.lapis.one", "1 Lapis Lazuli"),
    ("container.enchant.level.many", "%s Enchantment Levels"),
    ("container.enchant.level.one", "1 Enchantment Level"),
    ("container.enchant.level.requirement", "Needs level %s"),
    ("container.enderchest", "Ender Chest"),
    ("container.furnace", "Furnace"),
    ("container.grindstone_title", "Repair and Disenchant"),
    ("container.hopper", "Hopper"),
    ("container.inventory", "Inventory"),
    ("container.isLocked", "%s is locked."),
    ("container.lectern", "Lectern"),
    ("container.loom", "Loom"),
    ("container.repair", "Repair and Rename"),
    ("container.repair.cost", "Cost: %1$s levels"),
    ("container.repair.expensive", "Too costly"),
    ("container.shulkerBox", "Shulker Box"),
    ("container.shulkerBox.itemCount", "%s x%s"),
    ("container.shulkerBox.more", "and %s more..."),
    ("container.shulkerBox.unknownContents", "???????"),
    ("container.smoker", "Smoker"),
    (
        "container.spectatorCantOpen",
        "Can't open: nothing has been generated here yet.",
    ),
    ("container.stonecutter", "Stonecutter"),
    ("container.upgrade", "Upgrade Gear"),
    (
        "container.upgrade.error_tooltip",
        "This item can't be upgraded this way",
    ),
    (
        "container.upgrade.missing_template_tooltip",
        "Add a smithing template",
    ),
    ("merchant.deprecated", "Villagers restock twice a day."),
    ("merchant.level.1", "Novice"),
    ("merchant.level.2", "Apprentice"),
    ("merchant.level.3", "Journeyman"),
    ("merchant.level.4", "Expert"),
    ("merchant.level.5", "Master"),
    ("merchant.title", "%s - %s"),
    ("merchant.trades", "Trades"),
    ("menu.disconnect", "Leave"),
    ("menu.multiplayer", "Multiplayer"),
    ("menu.quit", "Quit"),
    ("menu.returnToMenu", "Save and Quit to Title"),
    ("menu.savingChunks", "Saving chunks"),
    ("menu.shareToLan", "Open to LAN"),
    ("menu.singleplayer", "Singleplayer"),
    (
        "block.minecraft.spawn.not_valid",
        "You have no home bed or charged respawn anchor",
    ),
    ("chat.type.text", "<%s> %s"),
    ("chat.type.announcement", "[%s] %s"),
    ("chat.type.emote", "* %s %s"),
    ("chat.type.admin", "[%s: %s]"),
    ("multiplayer.player.joined", "%s joined the game"),
    ("multiplayer.player.left", "%s left the game"),
    ("item.durability", "Durability: %s / %s"),
    ("item.dyed", "Dyed"),
    ("item.unbreakable", "Unbreakable"),
    ("item.nbt_tags", "NBT: %s tag(s)"),
    ("enchantment.level.1", "I"),
    ("enchantment.level.2", "II"),
    ("enchantment.level.3", "III"),
    ("enchantment.level.4", "IV"),
    ("enchantment.level.5", "V"),
    ("enchantment.level.6", "VI"),
    ("enchantment.level.7", "VII"),
    ("enchantment.level.8", "VIII"),
    ("enchantment.level.9", "IX"),
    ("enchantment.level.10", "X"),
    ("potion.withAmplifier", "%s %s"),
    ("potion.withDuration", "%s (%s)"),
];

/// Words written other than by plain title case.
const WORDS: &[(&str, &str)] = &[
    ("tnt", "TNT"),
    ("of", "of"),
    ("the", "the"),
    ("on", "on"),
    ("a", "a"),
    ("and", "and"),
    ("with", "with"),
    ("in", "in"),
    ("o", "o'"),
    ("lan", "LAN"),
    ("ui", "UI"),
    ("gui", "GUI"),
    ("fov", "FOV"),
    ("hud", "HUD"),
    ("nbt", "NBT"),
    ("snbt", "SNBT"),
    ("url", "URL"),
    ("id", "ID"),
    ("uuid", "UUID"),
    ("xp", "XP"),
    ("or", "or"),
    ("to", "to"),
    ("for", "for"),
    ("at", "at"),
    ("by", "by"),
];

/// Names that title case gets wrong, by registry key.
const NAMES: &[(&str, &str)] = &[
    ("block.minecraft.jack_o_lantern", "Jack o'Lantern"),
    ("item.minecraft.tnt_minecart", "Minecart with TNT"),
    ("item.minecraft.chest_minecart", "Minecart with Chest"),
    ("item.minecraft.furnace_minecart", "Minecart with Furnace"),
    ("item.minecraft.hopper_minecart", "Minecart with Hopper"),
    (
        "item.minecraft.command_block_minecart",
        "Minecart with Command Block",
    ),
    ("entity.minecraft.tnt_minecart", "Minecart with TNT"),
    ("entity.minecraft.chest_minecart", "Minecart with Chest"),
    ("entity.minecraft.furnace_minecart", "Minecart with Furnace"),
    ("entity.minecraft.hopper_minecart", "Minecart with Hopper"),
    (
        "entity.minecraft.command_block_minecart",
        "Minecart with Command Block",
    ),
    ("entity.minecraft.spawner_minecart", "Minecart with Spawner"),
    ("entity.minecraft.tnt", "Primed TNT"),
    ("entity.minecraft.item", "Item"),
    ("entity.minecraft.player", "Player"),
];

/// Verbs for the subtitles' last word: "Zombie groans".
const SOUNDS: &[(&str, &str)] = &[
    ("ambient", "is heard"),
    ("hurt", "hurts"),
    ("death", "dies"),
    ("step", "steps"),
    ("attack", "attacks"),
    ("break", "breaks"),
    ("place", "is placed"),
    ("hit", "is hit"),
    ("fall", "falls"),
    ("eat", "eats"),
    ("drink", "drinks"),
    ("open", "opens"),
    ("close", "closes"),
    ("shoot", "shoots"),
    ("throw", "is thrown"),
    ("land", "lands"),
    ("jump", "jumps"),
    ("swim", "swims"),
    ("splash", "splashes"),
    ("growl", "growls"),
    ("hiss", "hisses"),
    ("roar", "roars"),
    ("idle", "idles"),
    ("use", "is used"),
    ("equip", "is equipped"),
    ("fill", "fills"),
    ("empty", "empties"),
    ("burn", "burns"),
    ("extinguish", "goes out"),
    ("flap", "flaps"),
    ("fly", "flies"),
    ("spawn", "appears"),
    ("explode", "explodes"),
    ("primed", "fizzes"),
    ("ignite", "ignites"),
    ("teleport", "teleports"),
    ("hurt_drown", "drowns"),
    ("stare", "stares"),
    ("scream", "screams"),
];

/// Last segments that say little alone: the segment before them names the thing.
const VAGUE: &[&str] = &[
    "title",
    "name",
    "tooltip",
    "description",
    "desc",
    "button",
    "label",
    "info",
    "hint",
    "success",
    "failed",
    "failure",
    "single",
    "multiple",
    "text",
    "message",
    "query",
    "set",
    "get",
    "value",
    "narration",
    "header",
    "footer",
    "invalid",
    "unknown",
    "error",
    "empty",
    "none",
    "all",
    "on",
    "off",
    "short",
    "long",
    "line1",
    "line2",
    "line3",
    "detail",
    "details",
    "prompt",
    "confirm",
    "toggle",
    "status",
    "entry",
    "entries",
    "list",
    "count",
    "one",
    "many",
];

/// A key segment as words: `selectWorld` and `select_world` are both "select world".
fn words(segment: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in segment.split(['_', '-']) {
        let mut word = String::new();
        for c in part.chars() {
            if c.is_ascii_uppercase() && !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
            word.push(c.to_ascii_lowercase());
        }
        if !word.is_empty() {
            out.push(word);
        }
    }
    out
}

/// One word in a title: capitalised, but for the small words and acronyms of `WORDS`.
fn title_word(word: &str, first: bool) -> String {
    if let Some((_, w)) = WORDS.iter().find(|(k, _)| *k == word) {
        if first {
            let mut c = w.chars();
            return c
                .next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default();
        }
        return (*w).to_owned();
    }
    let mut c = word.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// `rotten_flesh` → "Rotten Flesh".
pub fn title_case(id: &str) -> String {
    words(id)
        .iter()
        .enumerate()
        .map(|(i, w)| title_word(w, i == 0))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Words in sentence case: the first capitalised.
fn sentence(words: &[String]) -> String {
    let text = words.join(" ");
    let mut c = text.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// A key's placeholders in order (`%s`, `%2$s`), as Java's formatter reads them.
pub fn placeholders(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if bytes.get(i + 1) == Some(&b'%') {
                i += 2;
                continue;
            }
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 && bytes.get(j) == Some(&b'$') {
                j += 1;
            } else {
                j = i + 1;
            }
            if j < bytes.len() && bytes[j].is_ascii_alphabetic() {
                out.push(text[i..=j].to_owned());
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The English for `key`, whose placeholders are `slots` in order.
fn english(key: &str, slots: &[&str]) -> String {
    if let Some((_, text)) = PROSE.iter().find(|(k, _)| *k == key) {
        return (*text).to_owned();
    }
    let segments: Vec<&str> = key.split('.').collect();
    let last = segments[segments.len() - 1];
    // A sound's subtitle: the thing, then what it does.
    if segments[0] == "subtitles" && segments.len() >= 3 {
        let thing = title_case(segments[segments.len() - 2]);
        let verb = SOUNDS
            .iter()
            .find(|(k, _)| *k == last)
            .map(|(_, v)| (*v).to_owned())
            .unwrap_or_else(|| words(last).join(" "));
        return with_slots(format!("{thing} {verb}"), slots);
    }
    // A death message: the one who died, how, then by whom and with what.
    if segments[0] == "death" && segments.len() >= 3 && !slots.is_empty() {
        let how = words(segments[2]).join(" ");
        let mut text = format!("{} died: {how}", slots[0]);
        for (i, slot) in slots.iter().enumerate().skip(1) {
            text.push_str(match i {
                1 => " by ",
                2 => " using ",
                _ => ", ",
            });
            text.push_str(slot);
        }
        return text;
    }
    // Potions and tipped arrows: "Splash Potion of Swiftness".
    if let ["item", "minecraft", kind, "effect", effect] = segments[..] {
        let kind = if kind == "tipped_arrow" {
            "arrow"
        } else {
            kind
        };
        return format!("{} of {}", title_case(kind), title_case(effect));
    }
    // Banner patterns: "Orange Creeper".
    if let ["block", "minecraft", "banner", pattern, color] = segments[..] {
        return format!("{} {}", title_case(color), title_case(pattern));
    }
    // Names of things under a `minecraft` segment: a title.
    if let Some(at) = segments.iter().position(|s| *s == "minecraft") {
        let tail = &segments[at + 1..];
        if !tail.is_empty() && slots.is_empty() {
            let name = tail
                .iter()
                .filter(|s| !VAGUE.contains(s))
                .map(|s| title_case(s))
                .collect::<Vec<_>>()
                .join(" ");
            if !name.is_empty() {
                return name;
            }
        }
    }
    // Anything else: its last meaningful segments.
    let mut from = segments.len() - 1;
    while from > 1 && (VAGUE.contains(&segments[from]) || segments[from].len() <= 1) {
        from -= 1;
    }
    let mut ws: Vec<String> = segments[from..]
        .iter()
        .filter(|s| !VAGUE.contains(s) || from == segments.len() - 1)
        .flat_map(|s| words(s))
        .collect();
    if ws.is_empty() {
        ws = words(last);
    }
    let text = if slots.is_empty() && ws.len() <= 4 {
        ws.iter()
            .enumerate()
            .map(|(i, w)| title_word(w, i == 0))
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        sentence(&ws)
    };
    with_slots(text, slots)
}

/// `text` followed by its placeholders, so each is shown where the key expects one.
fn with_slots(text: String, slots: &[&str]) -> String {
    if slots.is_empty() {
        text
    } else {
        format!("{text}: {}", slots.join(", "))
    }
}

/// The registry names' keys and their English, title-cased from the ids.
pub fn registry_keys() -> Vec<(String, String)> {
    use crate::numbering::names;
    let blocks = names("block");
    let mut out: Vec<(String, String)> = Vec::new();
    let mut add = |prefix: &str, id: &str| {
        let key = format!("{prefix}{id}");
        let name = NAMES
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| (*v).to_owned())
            .unwrap_or_else(|| title_case(id));
        out.push((key, name));
    };
    for b in blocks {
        add("block.minecraft.", b);
    }
    for i in names("item").iter().filter(|i| !blocks.contains(i)) {
        add("item.minecraft.", i);
    }
    for (registry, prefix) in [
        ("entity_type", "entity.minecraft."),
        ("mob_effect", "effect.minecraft."),
        ("attribute", "attribute.name."),
        ("villager_profession", "entity.minecraft.villager."),
        ("custom_stat", "stat.minecraft."),
        ("stat_type", "stat_type.minecraft."),
    ] {
        for id in names(registry) {
            add(prefix, id);
        }
    }
    out
}

/// Every other key and its English.
pub fn other_keys() -> impl Iterator<Item = (String, String)> {
    KEYS.lines().filter(|l| !l.is_empty()).map(|line| {
        let (key, slots) = line.split_once('\t').unwrap_or((line, ""));
        let slots: Vec<&str> = slots.split(' ').filter(|s| !s.is_empty()).collect();
        (key.to_owned(), english(key, &slots))
    })
}

/// Adds every key `lang` lacks, and names every block and item by its id: the bundle's
/// older names (Minetest Game's: "Pine Needles", "Papyrus") give way to Minecraft's ids
/// title-cased, as the art now draws them. The hand-written labels are kept.
pub fn fill(lang: &mut std::collections::BTreeMap<String, String>) {
    for (key, text) in other_keys() {
        lang.entry(key).or_insert(text);
    }
    for (key, text) in registry_keys() {
        if key.starts_with("block.") || key.starts_with("item.") {
            lang.insert(key, text);
        } else {
            lang.entry(key).or_insert(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every written line keeps its key's placeholders, in order.
    #[test]
    fn every_line_keeps_its_placeholders() {
        let signatures: std::collections::HashMap<&str, Vec<&str>> = KEYS
            .lines()
            .map(|l| {
                let (k, s) = l.split_once('\t').unwrap_or((l, ""));
                (k, s.split(' ').filter(|s| !s.is_empty()).collect())
            })
            .collect();
        for (key, text) in other_keys() {
            assert_eq!(
                placeholders(&text),
                signatures[key.as_str()],
                "{key}: {text}"
            );
        }
        for (key, _) in PROSE {
            assert!(signatures.contains_key(key), "{key} is not a key");
        }
        for (key, text) in registry_keys() {
            assert!(placeholders(&text).is_empty(), "{key}: {text}");
        }
    }

    #[test]
    fn names_are_title_cased_from_ids() {
        assert_eq!(title_case("rotten_flesh"), "Rotten Flesh");
        assert_eq!(title_case("bucket_of_axolotl"), "Bucket of Axolotl");
        assert_eq!(title_case("tnt"), "TNT");
        assert_eq!(
            placeholders("%1$s hit %2$s for 100%% with %s"),
            ["%1$s", "%2$s", "%s"]
        );
    }

    /// Hand-written labels (above) that place their arguments by index, in the order English
    /// reads best, or name the arguments Horizon's own server sends: their placeholders
    /// differ from Minecraft's line on purpose.
    const BY_INDEX: &[&str] = &[
        "argument.resource.not_found",
        "argument.resource_or_id.no_such_element",
        "argument.resource_selector.not_found",
        "argument.resource_tag.not_found",
        "commands.clear.test.multiple",
        "commands.clear.test.single",
        "commands.experience.set.levels.success.multiple",
        "commands.experience.set.levels.success.single",
        "commands.experience.set.points.success.multiple",
        "commands.experience.set.points.success.single",
        "commands.setworldspawn.success",
        "commands.spawnpoint.success.multiple",
        "commands.spawnpoint.success.single",
        "commands.time.no_default_clock",
        "commands.time.no_time_marker_found",
        "commands.time.pause.already_paused",
        "commands.time.pause.already_running",
        "commands.time.set.absolute",
        "commands.time.set.already_at_time",
        "commands.time.set.already_at_time_marker",
        "commands.time.set.time_marker",
        "difficulty.lock.question",
    ];

    /// Against the player's own Minecraft English (when `.local` has it): every key covered,
    /// and every line with the same placeholders in the same order.
    #[test]
    fn every_vanilla_key_is_covered_with_its_placeholders() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/horizon-client/26.3/assets/minecraft/lang/en_us.json");
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("skipped: no {}", path.display());
            return;
        };
        let vanilla: std::collections::BTreeMap<String, String> =
            serde_json::from_slice(&bytes).unwrap();
        // As the bundle writes it: the hand-written labels, then every other key.
        let mut ours: std::collections::BTreeMap<String, String> = LABELS
            .iter()
            .chain(SURVIVAL)
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .chain(key_names())
            .collect();
        fill(&mut ours);
        let (mut missing, mut differing) = (Vec::new(), Vec::new());
        for (key, text) in &vanilla {
            match ours.get(key) {
                Some(line)
                    if placeholders(line) != placeholders(text)
                        && !BY_INDEX.contains(&key.as_str()) =>
                {
                    differing.push(format!(
                        "{key}: {line:?} has {:?}, Minecraft {:?}",
                        placeholders(line),
                        placeholders(text)
                    ))
                }
                Some(_) => {}
                None => missing.push(key.as_str()),
            }
        }
        assert!(
            differing.is_empty(),
            "placeholders differ:\n{}",
            differing.join("\n")
        );
        assert!(
            missing.is_empty(),
            "{} keys missing: {:?}",
            missing.len(),
            &missing[..missing.len().min(20)]
        );
    }

    #[test]
    fn every_label_is_written_once() {
        let mut keys: Vec<&str> = super::LABELS
            .iter()
            .chain(super::SURVIVAL)
            .map(|(k, _)| *k)
            .collect();
        keys.sort_unstable();
        let len = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), len, "a label key appears twice");
    }
}
