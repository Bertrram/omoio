//! What Omoio sets in Dolphin's own settings files before a game, and the
//! picture sized for the machine. Every key here was read in Dolphin's
//! source at the release Omoio installs (release.rs): Dolphin.ini's in
//! Source/Core/Core/Config/MainSettings.cpp, GFX.ini's in
//! Core/Config/GraphicsSettings.cpp (Dolphin 2609a, read 8 October 2026).
//! Everything else is left as Dolphin keeps it, so a change the user makes
//! in Dolphin itself stays.

use super::ini;
use std::path::Path;

/// Settings for every game, set before each one.
///
/// - Dolphin asks on its first start whether it may send usage statistics
///   (`ShowAnalyticsPrompt`, DolphinQt/Main.cpp), which would stand in front
///   of the game. It is marked as asked; sending stays off, as it ships.
/// - Dolphin looks for its own updates as it starts unless its update track
///   is empty (`AutoUpdateChecker::CheckForUpdate`, UICommon/AutoUpdate.cpp).
///   Omoio keeps Dolphin at the release it was checked against.
/// - Stopping a game asks first by default (`ConfirmStop`). Omoio's Stop and
///   the game's window closing need no question.
/// - The game reads the pad only while its own window is in front
///   (`BackgroundInput` off, Dolphin's default, made sure of), so the game
///   doesn't hear the pad while Omoio's menus are over it.
/// - The game draws in a window of its own, which Omoio takes into its own
///   window, and not full screen: Omoio places the picture itself.
/// - Dolphin's notices over the picture are off, as Cemu's are.
/// - Dolphin speaks English, whatever Windows does. It otherwise takes the
///   language of Windows (`Translation.cpp`), and its Danish names the
///   Tools menu, which the portal window is opened from, "Værktøjer".
/// - Dolphin warns before an NKit image starts (`NKitWarningDialog`); such
///   an image plays, and the warning would stand in front of it.
/// - Dolphin's warnings about trouble it plays on through (`PanicAlertFmt`,
///   Common/MsgHandler.h) are boxes the game waits behind. A Wii game that
///   readies its online play at start, as Skylanders does, brings one up at
///   every start: the files that play needs come only from a real Wii
///   (`ReadCertFile`, Core/IOS/Network/SSL.cpp), Omoio never supplies them,
///   and Dolphin has no setting for that warning alone. With
///   `UsePanicHandlers` off Dolphin shows none of these, as its own "Ignore
///   for this session" does (DolphinQt/Main.cpp); each still goes into its
///   log (log.rs), and errors and questions still show
///   (`ShowMessageAlert`, Common/MsgHandler.cpp).
pub const EVERY_GAME: [(&str, &str, &str); 10] = [
    ("Analytics", "PermissionAsked", "True"),
    ("AutoUpdate", "UpdateTrack", ""),
    ("Interface", "ConfirmStop", "False"),
    ("Input", "BackgroundInput", "False"),
    ("Display", "RenderToMain", "False"),
    ("Display", "Fullscreen", "False"),
    ("Interface", "OnScreenDisplayMessages", "False"),
    ("Interface", "LanguageCode", "en"),
    ("Interface", "SkipNKitWarning", "True"),
    ("Interface", "UsePanicHandlers", "False"),
];

/// GFX.ini's settings for every game, set before each one.
///
/// - Full screen is borderless, never exclusive (`BorderlessFullscreen`,
///   Core/Config/GraphicsSettings.cpp lines 97 to 98). Omoio covers the
///   screen with the game's window itself, in its full screen and in Big
///   Picture, and Qt then takes the window for a full screen one by its
///   size alone (`QWindowsWindow::handleWmSize` and `isFullScreen_sys`,
///   qwindowswindow.cpp lines 2254 to 2263 and 2608 to 2618, Qt 6.8.3).
///   Dolphin answers that with exclusive full screen on D3D11, its first
///   choice on Windows (`Host::SetRenderFullscreen`, DolphinQt/Host.cpp
///   lines 173 to 181; `ExclusiveFullscreenEnabled`, VideoCommon/
///   VideoConfig.h lines 367 to 370), and from then on pauses the game to
///   go in or out of that mode each time its window gains or loses the
///   front (`Host::SetRenderFocus`, Host.cpp lines 137 to 147, pausing
///   through `RunWithGPUThreadInactive`, lines 79 to 117), which the portal
///   menu and Alt+Tab both make it do. Borderless, it neither pauses nor
///   switches, and the picture is the same. Read in the source on 27 October
///   2026 after players reported the game lagging after the menu and
///   Dolphin closing on Alt+Tab; not yet tried.
pub const EVERY_GAME_GRAPHICS: [(&str, &str, &str); 1] = [("Settings", "BorderlessFullscreen", "True")];

/// Dolphin.ini, with what a game needs set: `EVERY_GAME`, and the Skylanders
/// portal plugged in for a Skylanders game only, with Omoio's figures folder
/// as the place Dolphin's own portal window looks for figures first. And
/// GFX.ini with `EVERY_GAME_GRAPHICS`.
pub fn prepare(config: &Path, skylanders: bool, figures: &Path) -> std::io::Result<()> {
    let figures = figures.to_string_lossy();
    let portal = if skylanders { "True" } else { "False" };
    let mut values: Vec<(&str, &str, &str)> = EVERY_GAME.to_vec();
    values.push(("EmulatedUSBDevices", "EmulateSkylanderPortal", portal));
    if skylanders {
        values.push(("General", "SkylandersCollectionPath", &figures));
    }
    ini::update(&config.join("Dolphin.ini"), &values)?;
    // The game starts without it, so a GFX.ini that can't be written is left
    // as it was rather than keeping the game from starting.
    let _ = ini::update(&config.join("GFX.ini"), &EVERY_GAME_GRAPHICS);
    Ok(())
}

/// What Omoio sets in `[Interface]` while a player is on the keyboard and
/// mouse: the cursor never drawn over the game (`CursorVisibility` 0,
/// `ShowCursor::Never`, stored as its number, `ValueToString` in
/// Common/StringUtil.h lines 161 to 164; MAIN_SHOW_CURSOR,
/// Core/Config/MainSettings.cpp line 438 and MainSettings.h lines 255 to
/// 263, Dolphin 2609a, read 20 October 2026). Dolphin's own default brings
/// the arrow back each time the mouse moves (`OnMovement`,
/// DolphinQt/RenderWidget.cpp lines 397 to 404), and the mouse moves all
/// the time while it aims.
///
/// Dolphin's `LockCursor` is left alone. With it on, the game hears nothing,
/// pads included, until Dolphin has hold of the cursor (`UpdateInputGate`,
/// Core/Core.cpp lines 1028 to 1038, given `LockCursor` at every reading of
/// the pads, Core/HW/VideoInterface.cpp lines 955 to 956), and after its
/// window has been out of the front once, Dolphin takes hold again only on
/// a click in the game (RenderWidget.cpp lines 423 to 444), so every close
/// of the portal menu would leave the game deaf until a click. Omoio keeps
/// the cursor inside the game itself instead (cursor.rs).
const HELD_CURSOR: (&str, &str, &str) = ("Interface", "CursorVisibility", "0");

/// Dolphin.ini's text with the cursor hidden for a keyboard player
/// (`HELD_CURSOR`), or, with nobody on the keyboard and the setting as
/// Omoio sets it, without it, so Dolphin goes back to its own default.
/// Dolphin keeps in its file only what was set (`BaseConfigLayerLoader::Save`,
/// Core/ConfigLoaders/BaseConfigLoader.cpp lines 179 to 215), so the file is
/// then as it would be had Omoio never hidden the cursor. Omoio never set
/// this before there were keyboard players; any other value was set by the
/// user in Dolphin and is left alone.
fn with_cursor_held(text: &str, keyboard: bool) -> String {
    let (section, key, value) = HELD_CURSOR;
    if keyboard {
        ini::set(text, section, key, value)
    } else if ini::get(text, section, key).is_some_and(|found| found.trim() == value) {
        ini::remove(text, section, key)
    } else {
        text.to_string()
    }
}

/// Hides the cursor in Dolphin.ini for a keyboard player, or shows it again
/// (`with_cursor_held`). Written with the controller layout before every
/// game (controllers.rs, `write`), which knows who plays; the file is
/// written only when that changes it.
pub fn hold_the_cursor(dolphin_ini: &Path, keyboard: bool) -> std::io::Result<()> {
    let before = ini::read(dolphin_ini)?;
    let after = with_cursor_held(&before, keyboard);
    if after == before {
        return Ok(());
    }
    ini::write(dolphin_ini, &after)
}

/// A Hotkeys.ini with no keys at all, written once when Dolphin has none.
/// Dolphin's own keys (`HotkeyManager::LoadDefaults`, Core/HotkeyManager.cpp)
/// would do things in Omoio's place: Escape stops the game, Alt and Enter
/// takes the picture full screen behind Omoio's back, F11, Omoio's own key
/// for full screen, steps the debugger, and Tab and the F keys change the
/// game's speed and load saved states. A file with its section and nothing
/// in it binds nothing (`InputConfig::LoadConfig`, InputCommon/InputConfig.cpp).
/// A file the user made in Dolphin is left alone.
///
/// A keyboard player needs this too: of the keys they can be given
/// (core/keys.rs, `KEYS`), Dolphin's own hotkeys would take Tab, which turns
/// the speed limit off while held, and the numpad's plus, minus and 1 to 4 and
/// M, which change a connected Game Boy Advance's volume and speed
/// (`HotkeyManager::LoadDefaults`, Core/HotkeyManager.cpp lines 433 to 466,
/// Dolphin 2609a, read 20 October 2026). None of the others is bound there.
pub fn quiet_hotkeys(config: &Path) -> std::io::Result<()> {
    let path = config.join("Hotkeys.ini");
    if path.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(config)?;
    std::fs::write(path, "[Hotkeys]\r\n")
}

/// Dolphin draws at 640x528 times its Internal Resolution (`EFB_WIDTH`,
/// `EFB_HEIGHT`). Its own settings suggest, for each screen, the smallest
/// multiple that covers the screen both ways (the list in
/// DolphinQt/Config/Graphics/EnhancementsWidget.cpp: 2 for 720p, 3 for
/// 1080p, 4 for 1440p, 6 for 4K), and Omoio picks the same.
const EFB_WIDTH: u32 = 640;
const EFB_HEIGHT: u32 = 528;

/// The scale for a screen, capped by the graphics card's own memory: under
/// 3 GB at 2, under 6 GB at 3, under 10 GB at 4. The caps are Omoio's
/// judgement, the same steps it uses for RPCS3's picture.
pub fn scale_for(width: u32, height: u32, graphics_memory: u64) -> u32 {
    const GB: u64 = 1 << 30;
    let fits = width.div_ceil(EFB_WIDTH).max(height.div_ceil(EFB_HEIGHT)).max(1);
    let cap = match graphics_memory {
        m if m < 3 * GB => 2,
        m if m < 6 * GB => 3,
        m if m < 10 * GB => 4,
        _ => u32::MAX,
    };
    fits.min(cap)
}

/// Sets the Internal Resolution in GFX.ini for this screen, unless one is
/// there: Dolphin writes only what differs from its own default, so a value
/// there is one the user chose. Returns the scale set, or `None`.
pub fn size_picture(config: &Path, width: u32, height: u32, graphics_memory: u64) -> std::io::Result<Option<u32>> {
    let path = config.join("GFX.ini");
    let text = ini::read(&path)?;
    if ini::get(&text, "Settings", "InternalResolution").is_some() {
        return Ok(None);
    }
    let scale = scale_for(width, height, graphics_memory);
    ini::update(&path, &[("Settings", "InternalResolution", &scale.to_string())])?;
    Ok(Some(scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    #[test]
    fn the_scale_is_dolphins_own_suggestion_for_the_screen() {
        assert_eq!(scale_for(1280, 720, 16 * GB), 2);
        assert_eq!(scale_for(1920, 1080, 16 * GB), 3);
        assert_eq!(scale_for(2560, 1440, 16 * GB), 4);
        assert_eq!(scale_for(3840, 2160, 16 * GB), 6);
        assert_eq!(scale_for(2196, 1464, 16 * GB), 4, "a 3:2 screen, wider than 1080p");
        assert_eq!(scale_for(640, 480, 16 * GB), 1);
    }

    #[test]
    fn a_small_graphics_card_caps_the_scale() {
        assert_eq!(scale_for(3840, 2160, 2 * GB), 2);
        assert_eq!(scale_for(3840, 2160, 4 * GB), 3);
        assert_eq!(scale_for(3840, 2160, 8 * GB), 4);
        assert_eq!(scale_for(1280, 720, 2 * GB), 2, "a cap never raises the scale");
    }

    #[test]
    fn a_scale_the_user_chose_is_kept() {
        let dir = std::env::temp_dir().join(format!("omoio-dolphin-gfx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("GFX.ini"), "[Settings]\r\nInternalResolution = 2\r\n").unwrap();
        assert_eq!(size_picture(&dir, 3840, 2160, 16 * GB).unwrap(), None);
        std::fs::remove_file(dir.join("GFX.ini")).unwrap();
        assert_eq!(size_picture(&dir, 1920, 1080, 16 * GB).unwrap(), Some(3));
        let text = std::fs::read_to_string(dir.join("GFX.ini")).unwrap();
        assert_eq!(ini::get(&text, "Settings", "InternalResolution").as_deref(), Some("3"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_portal_is_plugged_in_for_skylanders_only() {
        let dir = std::env::temp_dir().join(format!("omoio-dolphin-ini-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        prepare(&dir, true, Path::new("C:\\Figures")).unwrap();
        let text = std::fs::read_to_string(dir.join("Dolphin.ini")).unwrap();
        assert_eq!(ini::get(&text, "EmulatedUSBDevices", "EmulateSkylanderPortal").as_deref(), Some("True"));
        assert_eq!(ini::get(&text, "General", "SkylandersCollectionPath").as_deref(), Some("C:\\Figures"));
        assert_eq!(ini::get(&text, "AutoUpdate", "UpdateTrack").as_deref(), Some(""));
        prepare(&dir, false, Path::new("C:\\Figures")).unwrap();
        let text = std::fs::read_to_string(dir.join("Dolphin.ini")).unwrap();
        assert_eq!(ini::get(&text, "EmulatedUSBDevices", "EmulateSkylanderPortal").as_deref(), Some("False"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_cursor_is_hidden_for_a_keyboard_player_and_shown_after() {
        let before = "[Interface]
ConfirmStop = False
[Core]
CPUThread = True
";
        let held = with_cursor_held(before, true);
        assert_eq!(held, "[Interface]
ConfirmStop = False
CursorVisibility = 0
[Core]
CPUThread = True
");
        assert_eq!(with_cursor_held(&held, true), held, "nothing changes the second time");
        assert_eq!(with_cursor_held(&held, false), before, "pads only: as it was");
        assert!(ini::get(&held, "Interface", "LockCursor").is_none(), "Dolphin's own hold stays off");
    }

    #[test]
    fn a_pad_players_cursor_settings_stay_as_they_were() {
        for text in [
            "",
            "[Interface]
ConfirmStop = False
",
            "[Interface]
LockCursor = True
",
            "[Interface]
LockCursor = True
CursorVisibility = 1
",
        ] {
            assert_eq!(with_cursor_held(text, false), text);
        }
    }

    #[test]
    fn dolphins_warnings_dont_hold_up_a_game() {
        let dir = std::env::temp_dir().join(format!("omoio-dolphin-warnings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Dolphin.ini"), "[Interface]\r\nUsePanicHandlers = True\r\n").unwrap();
        prepare(&dir, true, Path::new("C:\\Figures")).unwrap();
        let text = std::fs::read_to_string(dir.join("Dolphin.ini")).unwrap();
        assert_eq!(ini::get(&text, "Interface", "UsePanicHandlers").as_deref(), Some("False"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn full_screen_is_borderless_and_the_resolution_kept() {
        let dir = std::env::temp_dir().join(format!("omoio-dolphin-borderless-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("GFX.ini"), "[Settings]\r\nInternalResolution = 3\r\n").unwrap();
        prepare(&dir, true, Path::new("C:\\Figures")).unwrap();
        let text = std::fs::read_to_string(dir.join("GFX.ini")).unwrap();
        assert_eq!(ini::get(&text, "Settings", "BorderlessFullscreen").as_deref(), Some("True"));
        assert_eq!(ini::get(&text, "Settings", "InternalResolution").as_deref(), Some("3"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
