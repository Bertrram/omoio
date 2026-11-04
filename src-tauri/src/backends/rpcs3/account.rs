//! The name games call the player, and the region they run as.
//!
//! Both belong to RPCS3 and were read off a real install rather than assumed:
//!
//! - The name is a plain text file, `dev_hdd0/home/<user>/localusername`.
//! - The region is two settings in `config/config.yml`: `License Area`, which
//!   is the console's territory, and `Language`, which is what most games
//!   actually read to pick their text. A Game language picked in Omoio's
//!   settings takes the place of the region's language as each game starts
//!   (`start_in_game_language`).
//!
//! The values come from RPCS3's own cellSysutil.cpp, so what Omoio writes is
//! what the emulator expects to read back.

use crate::core::language;
use crate::core::settings::Settings;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// RPCS3's default user. It supports more, but nothing in Omoio creates them,
/// and inventing a second one would only split someone's saves in half.
const USER: &str = "00000001";

const K_AREA: &str = "License Area";
const K_LANGUAGE: &str = "Language";

/// RPCS3's entry in `Settings::language_kept`.
const KEPT: &str = "rpcs3";

/// A place to play from, as somebody choosing one would think of it.
///
/// Each pairs the console territory with the language games start in. The
/// spellings are RPCS3's, from cellSysutil.cpp; the names are ours.
pub struct Region {
    pub id: &'static str,
    pub name: &'static str,
    pub area: &'static str,
    pub language: &'static str,
}

pub const REGIONS: &[Region] = &[
    Region { id: "eu-en", name: "Europe", area: "SCEE", language: "English (UK)" },
    Region { id: "us", name: "North America", area: "SCEA", language: "English (US)" },
    Region { id: "jp", name: "Japan", area: "SCEJ", language: "Japanese" },
    Region { id: "eu-fr", name: "France", area: "SCEE", language: "French" },
    Region { id: "eu-de", name: "Germany", area: "SCEE", language: "German" },
    Region { id: "eu-es", name: "Spain", area: "SCEE", language: "Spanish" },
    Region { id: "eu-it", name: "Italy", area: "SCEE", language: "Italian" },
    Region { id: "eu-nl", name: "Netherlands", area: "SCEE", language: "Dutch" },
    Region { id: "eu-pt", name: "Portugal", area: "SCEE", language: "Portuguese (Portugal)" },
    Region { id: "eu-da", name: "Denmark", area: "SCEE", language: "Danish" },
    Region { id: "eu-sv", name: "Sweden", area: "SCEE", language: "Swedish" },
    Region { id: "eu-no", name: "Norway", area: "SCEE", language: "Norwegian" },
    Region { id: "eu-fi", name: "Finland", area: "SCEE", language: "Finnish" },
    Region { id: "eu-pl", name: "Poland", area: "SCEE", language: "Polish" },
    Region { id: "eu-ru", name: "Russia", area: "SCEE", language: "Russian" },
    Region { id: "eu-tr", name: "Turkey", area: "SCEE", language: "Turkish" },
    Region { id: "br", name: "Brazil", area: "SCEA", language: "Portuguese (Brazil)" },
    Region { id: "kr", name: "Korea", area: "SCEK", language: "Korean" },
    Region { id: "hk", name: "Hong Kong and Taiwan", area: "SCEH", language: "Chinese (Traditional)" },
    Region { id: "cn", name: "China", area: "SCH", language: "Chinese (Simplified)" },
];

#[derive(Debug, Serialize, Deserialize)]
pub struct Account {
    pub username: String,
    /// The id of the closest matching region, or empty when the emulator is
    /// set to a combination none of ours describes.
    pub region: String,
}

#[derive(Serialize)]
pub struct RegionChoice {
    pub id: &'static str,
    pub name: &'static str,
    pub language: &'static str,
}

pub fn regions() -> Vec<RegionChoice> {
    REGIONS
        .iter()
        .map(|r| RegionChoice { id: r.id, name: r.name, language: r.language })
        .collect()
}

fn username_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(super::install_dir(app)?
        .join("dev_hdd0")
        .join("home")
        .join(USER)
        .join("localusername"))
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(super::install_dir(app)?.join("config").join("config.yml"))
}

fn settings_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().data_dir().map_err(|e| e.to_string())?.join("Omoio").join("settings.json"))
}

/// PS3 names are short and plain. Anything else is trimmed rather than
/// refused, so a name is never rejected for a reason nobody explained.
fn tidy(name: &str) -> String {
    let kept: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == ' ')
        .collect();
    kept.trim().chars().take(16).collect()
}

pub fn read(app: &AppHandle) -> Account {
    let username = username_path(app)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|name| name.trim().to_string())
        .unwrap_or_default();

    let config = config_path(app)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let remembered = settings_file(app).ok().and_then(|file| Settings::load(&file).region);

    Account { username, region: region_of(&config, remembered.as_deref()) }
}

/// The region config.yml is set to.
///
/// A Game language changes config.yml's Language as each game starts, so
/// the area and language together no longer name the region picked: Europe
/// with German games would read as Germany, and Europe with Japanese ones as
/// no region at all. So the region picked is remembered in Omoio's settings,
/// and it is the answer while config.yml's License Area is still that
/// region's. A region picked before Omoio remembered it, or an area changed
/// in RPCS3's own settings since, is found by the pair, as before.
fn region_of(config: &str, remembered: Option<&str>) -> String {
    let area = setting(config, K_AREA).unwrap_or_default();
    if let Some(region) = remembered.and_then(|id| REGIONS.iter().find(|r| r.id == id)) {
        if region.area == area {
            return region.id.to_string();
        }
    }
    let language = setting(config, K_LANGUAGE).unwrap_or_default();
    REGIONS
        .iter()
        .find(|r| r.area == area && r.language == language)
        .map(|r| r.id.to_string())
        .unwrap_or_default()
}

/// The Language a region sets, by the region's id. Kept in Omoio's settings
/// when a region is picked, so going back to the default Game language gives
/// the region's language, as picking the region always did.
pub fn language_of_region(region_id: &str) -> Option<&'static str> {
    REGIONS.iter().find(|r| r.id == region_id).map(|r| r.language)
}

pub fn set_username(app: &AppHandle, name: &str) -> Result<String, String> {
    let tidied = tidy(name);
    if tidied.is_empty() {
        return Err("Pick a name with at least one letter or number in it.".into());
    }
    let path = username_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, &tidied).map_err(|e| e.to_string())?;
    Ok(tidied)
}

/// Sets RPCS3's License Area and Language to the region's. Language too
/// while a Game language is picked: config.yml then holds the region's own
/// pair, the one the default goes back to, and the next game Omoio starts
/// is set to the Game language again (`start_in_game_language`).
pub fn set_region(app: &AppHandle, id: &str) -> Result<(), String> {
    let region = REGIONS
        .iter()
        .find(|r| r.id == id)
        .ok_or("That isn't a region Omoio knows.")?;

    let path = config_path(app)?;
    let existing = std::fs::read_to_string(&path).map_err(|_| {
        "RPCS3 hasn't written its settings yet. Start a game once first.".to_string()
    })?;

    let updated = replace_setting(&existing, K_AREA, region.area);
    let updated = replace_setting(&updated, K_LANGUAGE, region.language);
    std::fs::write(&path, updated).map_err(|e| e.to_string())
}

/// Sets config.yml's Language to the Game language picked in Omoio's
/// settings, as a game starts, and puts back the one it had once the
/// setting is on its default again.
///
/// RPCS3 reads config.yml again at every boot, and a game's own custom
/// config over it, so a Language picked for one game in its settings still
/// wins (Emu/System.cpp, `Emulator::Init`, which `Emulator::Load` calls,
/// "Reload global configuration", and `Emulator::Load`, "Load custom
/// configs"; RPCS3/rpcs3 b2e52d6, read 4 November 2026). License Area stays
/// the region's. Only the names core::language gives are written: RPCS3
/// refuses a value it can't read (Utilities/Config.cpp, `decode`) and logs
/// "Failed to apply global config" as fatal (`Emulator::Init`).
///
/// config.yml is RPCS3's own file and keeps what is written in it, so the
/// Language it had before Omoio first changed it is kept in Omoio's
/// settings, put back once on the default, and then forgotten. For a player
/// who never picks a Game language, no file of RPCS3's is read or written
/// here. Without a config.yml with a Language line in it nothing is written
/// either: RPCS3
/// writes its whole file of defaults only where there is none or it is
/// empty (`Emulator::Init`, "Save new global config"), and Omoio only ever
/// changes a line RPCS3 wrote (`replace_setting`).
pub(super) fn start_in_game_language(app: &AppHandle) {
    if let (Ok(config), Ok(settings)) = (config_path(app), settings_file(app)) {
        game_language_into(&config, &settings);
    }
}

fn game_language_into(config_path: &Path, settings_path: &Path) {
    let mut settings = Settings::load(settings_path);
    let picked = settings.game_language.as_deref().and_then(language::rpcs3);
    let kept = settings.language_kept.get(KEPT).cloned();
    if picked.is_none() && kept.is_none() {
        return;
    }
    let Ok(config) = std::fs::read_to_string(config_path) else {
        return;
    };
    let Some(current) = setting(&config, K_LANGUAGE) else {
        return;
    };
    let (write, kept_after) = language_at_start(&current, picked, kept.as_deref());

    // Kept before the file changes, and forgotten only once it has, so the
    // value Omoio replaced is never lost on the way.
    if let Some(keep) = kept_after.filter(|_| kept.is_none()) {
        settings.language_kept.insert(KEPT.to_string(), keep.to_string());
        if settings.save(settings_path).is_err() {
            return;
        }
    }
    if let Some(value) = write {
        if std::fs::write(config_path, replace_setting(&config, K_LANGUAGE, value)).is_err() {
            return;
        }
    }
    if kept_after.is_none() && kept.is_some() {
        settings.language_kept.remove(KEPT);
        let _ = settings.save(settings_path);
    }
}

/// What a game start does to config.yml's Language, from the one it holds,
/// the RPCS3 name of the Game language picked (none on the default) and the
/// one Omoio kept from before: the Language to write, when it changes, and
/// what is kept after. The first value kept stays kept however often the
/// pick changes, since that is the one from before Omoio.
fn language_at_start<'a>(
    current: &'a str,
    picked: Option<&'a str>,
    kept: Option<&'a str>,
) -> (Option<&'a str>, Option<&'a str>) {
    match (picked, kept) {
        (Some(picked), kept) if picked != current => (Some(picked), kept.or(Some(current))),
        (Some(_), kept) => (None, kept),
        (None, Some(kept)) => ((kept != current).then_some(kept), None),
        (None, None) => (None, None),
    }
}

/// Reads one indented `Key: value` out of RPCS3's config.
fn setting(config: &str, key: &str) -> Option<String> {
    config.lines().find_map(|line| {
        let trimmed = line.trim_start();
        let rest = trimmed.strip_prefix(key)?.strip_prefix(':')?;
        Some(rest.trim().trim_matches('"').to_string())
    })
}

/// Rewrites one line and leaves the rest of the file exactly as it was.
///
/// The global config holds lists and nested maps that our own reader is not
/// built to reproduce, so it is never parsed and re-emitted. Touching the one
/// line means nothing else can be lost on the way through.
pub(super) fn replace_setting(config: &str, key: &str, value: &str) -> String {
    let mut out = String::with_capacity(config.len() + 32);
    let mut done = false;

    for line in config.lines() {
        let trimmed = line.trim_start();
        if !done && trimmed.starts_with(key) && trimmed[key.len()..].starts_with(':') {
            let indent = &line[..line.len() - trimmed.len()];
            out.push_str(&format!("{indent}{key}: {value}"));
            done = true;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "\
Core:
  PPU Decoder: Recompiler (LLVM)
  Libraries Control:
    []
System:
  License Area: SCEA
  Language: English (US)
  Keyboard Type: English keyboard (US standard)
Video:
  Renderer: Vulkan
";

    #[test]
    fn reads_the_settings_a_region_is_made_of() {
        assert_eq!(setting(CONFIG, K_AREA).as_deref(), Some("SCEA"));
        assert_eq!(setting(CONFIG, K_LANGUAGE).as_deref(), Some("English (US)"));
        assert_eq!(setting(CONFIG, "Nothing Like This"), None);
    }

    #[test]
    fn changes_one_line_and_leaves_the_rest_alone() {
        let out = replace_setting(CONFIG, K_LANGUAGE, "Danish");

        assert!(out.contains("  Language: Danish"));
        assert!(!out.contains("English (US)"));
        // Everything the global config holds that our reader cannot reproduce
        // has to survive untouched.
        assert!(out.contains("  Libraries Control:\n    []"));
        assert!(out.contains("  Keyboard Type: English keyboard (US standard)"));
        assert!(out.contains("  Renderer: Vulkan"));
        assert_eq!(out.lines().count(), CONFIG.lines().count());
    }

    #[test]
    fn only_the_first_match_is_rewritten() {
        // "Renderer" appears under both Video and Audio in the real file, so a
        // blind replace-all would change a setting nobody asked about.
        let both = "Video:\n  Renderer: Vulkan\nAudio:\n  Renderer: Cubeb\n";
        let out = replace_setting(both, "Renderer", "OpenGL");
        assert!(out.contains("  Renderer: OpenGL"));
        assert!(out.contains("  Renderer: Cubeb"), "the audio one must not move");
    }

    /// RPCS3's names for its languages, as `fmt_class_string<CellSysutilLang>`
    /// gives them (rpcs3/Emu/Cell/Modules/cellSysutil.cpp, RPCS3/rpcs3
    /// b2e52d6, read 4 November 2026), which is what config.yml holds.
    const RPCS3_LANGUAGES: [&str; 20] = [
        "Japanese",
        "English (US)",
        "French",
        "Spanish",
        "German",
        "Italian",
        "Dutch",
        "Portuguese (Portugal)",
        "Russian",
        "Korean",
        "Chinese (Traditional)",
        "Chinese (Simplified)",
        "Finnish",
        "Swedish",
        "Danish",
        "Norwegian",
        "Polish",
        "English (UK)",
        "Portuguese (Brazil)",
        "Turkish",
    ];

    #[test]
    fn a_region_is_a_territory_and_a_language_together() {
        let denmark = REGIONS.iter().find(|r| r.id == "eu-da").unwrap();
        assert_eq!(denmark.area, "SCEE");
        assert_eq!(denmark.language, "Danish");

        // Every region must name values RPCS3 actually accepts.
        const AREAS: &[&str] = &["SCEJ", "SCEA", "SCEE", "SCEH", "SCEK", "SCH", "Other"];
        for region in REGIONS {
            assert!(AREAS.contains(&region.area), "{} has an unknown area", region.id);
            assert!(RPCS3_LANGUAGES.contains(&region.language), "{} has an unknown language", region.id);
        }
    }

    #[test]
    fn every_game_language_is_written_in_rpcs3s_own_words() {
        for picked in &language::LANGUAGES {
            let name = language::rpcs3(picked.id).unwrap_or_else(|| panic!("{} has no RPCS3 name", picked.id));
            assert!(RPCS3_LANGUAGES.contains(&name), "{} gives {name}, which RPCS3 can't read", picked.id);
            assert!(REGIONS.iter().any(|r| r.language == name), "{} is no region's language", picked.id);
        }
        // RPCS3 has all twenty, so none stands in for another.
        let mut names: Vec<&str> = language::LANGUAGES.iter().filter_map(|l| language::rpcs3(l.id)).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), RPCS3_LANGUAGES.len());

        let table = [
            ("en-gb", "English (UK)"),
            ("en-us", "English (US)"),
            ("de", "German"),
            ("fr", "French"),
            ("es", "Spanish"),
            ("it", "Italian"),
            ("nl", "Dutch"),
            ("pt-pt", "Portuguese (Portugal)"),
            ("pt-br", "Portuguese (Brazil)"),
            ("ru", "Russian"),
            ("pl", "Polish"),
            ("da", "Danish"),
            ("fi", "Finnish"),
            ("no", "Norwegian"),
            ("sv", "Swedish"),
            ("tr", "Turkish"),
            ("ja", "Japanese"),
            ("ko", "Korean"),
            ("zh-hant", "Chinese (Traditional)"),
            ("zh-hans", "Chinese (Simplified)"),
        ];
        for (id, name) in table {
            assert_eq!(language::rpcs3(id), Some(name), "{id}");
        }
    }

    #[test]
    fn a_region_says_which_language_it_sets() {
        assert_eq!(language_of_region("eu-en"), Some("English (UK)"));
        assert_eq!(language_of_region("eu-de"), Some("German"));
        assert_eq!(language_of_region("jp"), Some("Japanese"));
        assert_eq!(language_of_region("atlantis"), None);
    }

    fn config_set_to(area: &str, language: &str) -> String {
        replace_setting(&replace_setting(CONFIG, K_AREA, area), K_LANGUAGE, language)
    }

    #[test]
    fn the_region_picked_holds_while_a_game_language_is_set() {
        // Europe, with games started in German or Japanese.
        assert_eq!(region_of(&config_set_to("SCEE", "German"), Some("eu-en")), "eu-en");
        assert_eq!(region_of(&config_set_to("SCEE", "Japanese"), Some("eu-en")), "eu-en");
        // Germany itself is still Germany.
        assert_eq!(region_of(&config_set_to("SCEE", "German"), Some("eu-de")), "eu-de");

        // An install from before Omoio remembered a region goes by the pair.
        assert_eq!(region_of(&config_set_to("SCEE", "German"), None), "eu-de");
        assert_eq!(region_of(&config_set_to("SCEE", "Japanese"), None), "");

        // An area changed in RPCS3's own settings since is no longer the
        // region remembered.
        assert_eq!(region_of(&config_set_to("SCEA", "English (US)"), Some("eu-en")), "us");
        assert_eq!(region_of(&config_set_to("SCEA", "German"), Some("eu-en")), "");
        // Nor is one Omoio doesn't know, or a config RPCS3 hasn't written.
        assert_eq!(region_of(&config_set_to("SCEE", "Danish"), Some("atlantis")), "eu-da");
        assert_eq!(region_of("", Some("eu-en")), "");
    }

    #[test]
    fn the_language_from_before_is_kept_once_and_put_back_once() {
        // A pick that changes the Language keeps the one it replaces.
        assert_eq!(
            language_at_start("English (UK)", Some("German"), None),
            (Some("German"), Some("English (UK)"))
        );
        // Picked again, or another picked, the first one kept stays kept.
        assert_eq!(language_at_start("German", Some("German"), Some("English (UK)")), (None, Some("English (UK)")));
        assert_eq!(
            language_at_start("German", Some("Japanese"), Some("English (UK)")),
            (Some("Japanese"), Some("English (UK)"))
        );
        // A pick the file already holds changes nothing, so nothing is kept.
        assert_eq!(language_at_start("German", Some("German"), None), (None, None));

        // The default puts the kept one back, then forgets it.
        assert_eq!(language_at_start("Japanese", None, Some("English (UK)")), (Some("English (UK)"), None));
        assert_eq!(language_at_start("English (UK)", None, Some("English (UK)")), (None, None));
        // And with nothing kept leaves the file alone.
        assert_eq!(language_at_start("Danish", None, None), (None, None));
    }

    #[test]
    fn a_game_language_is_written_and_taken_back_again() {
        let dir = std::env::temp_dir().join(format!("omoio-game-language-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("config.yml");
        let settings = dir.join("settings.json");
        let europe = config_set_to("SCEE", "English (UK)");
        std::fs::write(&config, &europe).unwrap();
        let pick = |id: Option<&str>| {
            let mut chosen = Settings::load(&settings);
            chosen.game_language = id.map(str::to_string);
            chosen.save(&settings).unwrap();
            game_language_into(&config, &settings);
        };
        let kept = || Settings::load(&settings).language_kept.get(KEPT).cloned();

        pick(Some("de"));
        let german = std::fs::read_to_string(&config).unwrap();
        let changed: Vec<(&str, &str)> = europe.lines().zip(german.lines()).filter(|(a, b)| a != b).collect();
        assert_eq!(changed, [("  Language: English (UK)", "  Language: German")], "only the Language line");
        assert_eq!(german.lines().count(), europe.lines().count());
        assert_eq!(kept().as_deref(), Some("English (UK)"));

        pick(Some("ja"));
        assert!(std::fs::read_to_string(&config).unwrap().contains("  Language: Japanese\n"));
        assert_eq!(kept().as_deref(), Some("English (UK)"), "the one from before Omoio");

        pick(None);
        assert_eq!(std::fs::read_to_string(&config).unwrap(), europe, "every game as before");
        assert_eq!(kept(), None, "forgotten once it is back");
        assert!(!std::fs::read_to_string(&settings).unwrap().contains("language_kept"));

        // On the default with nothing kept, a Language set in RPCS3 itself
        // is left as it is.
        let own = config_set_to("SCEE", "Danish");
        std::fs::write(&config, &own).unwrap();
        pick(None);
        assert_eq!(std::fs::read_to_string(&config).unwrap(), own);

        // RPCS3 hasn't written its settings: nothing is made, nothing kept.
        std::fs::remove_file(&config).unwrap();
        pick(Some("de"));
        assert!(!config.exists());
        assert_eq!(kept(), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn region_ids_are_unique() {
        let mut ids: Vec<&str> = REGIONS.iter().map(|r| r.id).collect();
        let before = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(before, ids.len());
    }

    #[test]
    fn a_name_is_tidied_rather_than_refused() {
        assert_eq!(tidy("  Bertram  "), "Bertram");
        assert_eq!(tidy("Bertram!!!"), "Bertram");
        assert_eq!(tidy("a name that is far too long to fit"), "a name that is f");
        assert_eq!(tidy("!!!"), "", "nothing usable left is the one case we refuse");
    }
}
