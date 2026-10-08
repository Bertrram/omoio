//! What the user has told us, kept between runs. Only the games folder for
//! now: the place archives are unpacked into.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub games_folder: Option<PathBuf>,
    /// Start games filling the screen rather than sitting in the content area.
    #[serde(default)]
    pub start_fullscreen: bool,
    /// Open Omoio in Big Picture, for a machine that lives under a TV.
    #[serde(default)]
    pub start_in_big_picture: bool,
    /// How many played sessions to keep logs for.
    #[serde(default = "default_keep_sessions")]
    pub keep_sessions: usize,
    /// Whether the first-run questions have been answered. Asked once, then
    /// never again; both answers stay changeable in Settings.
    #[serde(default)]
    pub set_up: bool,
    /// Set once Omoio has sized RPCS3's picture for this machine. It happens
    /// once, so a scale the user picks afterwards is never put back. Kept
    /// from before each emulator was sized on its own, so RPCS3 is not sized
    /// a second time.
    #[serde(default)]
    pub tuned: bool,
    /// RPCS3's scale that was, for the System screen.
    #[serde(default)]
    pub tuned_scale: Option<u32>,
    /// The other emulators Omoio has sized the picture for, by name, each
    /// once, for the same reason as `tuned`.
    #[serde(default)]
    pub tuned_for: Vec<String>,
    /// Real covers from RAWG in place of the generated tiles. Off until the
    /// user turns it on.
    #[serde(default)]
    pub covers: bool,
    /// The user's own RAWG key. Kept in this file on their machine, never
    /// shipped with the app.
    #[serde(default)]
    pub rawg_key: Option<String>,
    /// The known fixes already applied, by title id. Each is applied once, so
    /// one the user switches off afterwards stays off.
    #[serde(default)]
    pub applied_fixes: std::collections::BTreeMap<String, Vec<String>>,
    /// Figure files put on a portal lately, newest first, by file name, so
    /// the Skylanders menu lists them first.
    #[serde(default)]
    pub recent_figures: Vec<String>,
    /// The pad button that opens the portal menu over a Skylanders game, as
    /// a place on the pad. The home button unless the user picks another.
    #[serde(default = "default_portal_button")]
    pub portal_button: String,
}

fn default_keep_sessions() -> usize {
    20
}

fn default_portal_button() -> String {
    "Guide".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            games_folder: None,
            start_fullscreen: false,
            start_in_big_picture: false,
            keep_sessions: default_keep_sessions(),
            set_up: false,
            tuned: false,
            tuned_scale: None,
            tuned_for: Vec::new(),
            covers: false,
            rawg_key: None,
            applied_fixes: Default::default(),
            recent_figures: Vec::new(),
            portal_button: default_portal_button(),
        }
    }
}

impl Settings {
    /// Unreadable settings fall back to defaults rather than stopping the app;
    /// the worst case is being asked for the games folder again.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let temp = path.with_extension("json.writing");
        std::fs::write(&temp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("omoio-settings-{}-{name}.json", std::process::id()))
    }

    #[test]
    fn round_trips_what_the_user_chose() {
        let path = temp_path("roundtrip");
        let settings = Settings {
            games_folder: Some(PathBuf::from("D:\\PS3")),
            start_fullscreen: true,
            start_in_big_picture: true,
            keep_sessions: 5,
            set_up: true,
            tuned: true,
            tuned_scale: Some(200),
            tuned_for: vec!["Dolphin".to_string()],
            covers: true,
            rawg_key: Some("k".to_string()),
            applied_fixes: Default::default(),
            recent_figures: vec!["Whirlwind.sky".to_string()],
            portal_button: "Back".to_string(),
        };
        settings.save(&path).unwrap();

        let loaded = Settings::load(&path);
        assert_eq!(loaded.games_folder, Some(PathBuf::from("D:\\PS3")));
        assert!(loaded.start_fullscreen);
        assert!(loaded.start_in_big_picture);
        assert_eq!(loaded.keep_sessions, 5);
        assert!(loaded.tuned);
        assert_eq!(loaded.tuned_scale, Some(200));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_settings_file_from_an_older_build_still_loads() {
        // Written before start_fullscreen and keep_sessions existed.
        let path = temp_path("older");
        std::fs::write(&path, r#"{"games_folder":"D:\\PS3"}"#).unwrap();

        let loaded = Settings::load(&path);
        assert_eq!(loaded.games_folder, Some(PathBuf::from("D:\\PS3")));
        assert!(!loaded.start_fullscreen);
        assert!(!loaded.start_in_big_picture);
        assert_eq!(loaded.keep_sessions, 20, "missing values fall back to the default");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_or_unreadable_settings_fall_back_to_defaults() {
        assert!(Settings::load(Path::new("no-such-settings.json")).games_folder.is_none());

        let path = temp_path("corrupt");
        std::fs::write(&path, "not json at all").unwrap();
        assert!(Settings::load(&path).games_folder.is_none());
        let _ = std::fs::remove_file(&path);
    }
}
