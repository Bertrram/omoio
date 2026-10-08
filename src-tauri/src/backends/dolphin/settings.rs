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
pub const EVERY_GAME: [(&str, &str, &str); 9] = [
    ("Analytics", "PermissionAsked", "True"),
    ("AutoUpdate", "UpdateTrack", ""),
    ("Interface", "ConfirmStop", "False"),
    ("Input", "BackgroundInput", "False"),
    ("Display", "RenderToMain", "False"),
    ("Display", "Fullscreen", "False"),
    ("Interface", "OnScreenDisplayMessages", "False"),
    ("Interface", "LanguageCode", "en"),
    ("Interface", "SkipNKitWarning", "True"),
];

/// Dolphin.ini, with what a game needs set: `EVERY_GAME`, and the Skylanders
/// portal plugged in for a Skylanders game only, with Omoio's figures folder
/// as the place Dolphin's own portal window looks for figures first.
pub fn prepare(config: &Path, skylanders: bool, figures: &Path) -> std::io::Result<()> {
    let figures = figures.to_string_lossy();
    let portal = if skylanders { "True" } else { "False" };
    let mut values: Vec<(&str, &str, &str)> = EVERY_GAME.to_vec();
    values.push(("EmulatedUSBDevices", "EmulateSkylanderPortal", portal));
    if skylanders {
        values.push(("General", "SkylandersCollectionPath", &figures));
    }
    ini::update(&config.join("Dolphin.ini"), &values)
}

/// A Hotkeys.ini with no keys at all, written once when Dolphin has none.
/// Dolphin's own keys (`HotkeyManager::LoadDefaults`, Core/HotkeyManager.cpp)
/// would do things in Omoio's place: Escape stops the game, Alt and Enter
/// takes the picture full screen behind Omoio's back, F11, Omoio's own key
/// for full screen, steps the debugger, and Tab and the F keys change the
/// game's speed and load saved states. A file with its section and nothing
/// in it binds nothing (`InputConfig::LoadConfig`, InputCommon/InputConfig.cpp).
/// A file the user made in Dolphin is left alone.
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
    let text = std::fs::read_to_string(&path).unwrap_or_default();
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
}
