//! The language games start in: one choice for every console, and the value
//! each emulator takes for it.
//!
//! Omoio names each language by a short id of its own (`de`, `pt-br`), so
//! settings.json never holds one emulator's spelling, and each backend asks
//! here for its value. The values were read in each emulator's source at the
//! version Omoio runs (4 November 2026):
//!
//! - RPCS3, master b2e52d6: config.yml's `System: Language:` takes the names
//!   `fmt_class_string<CellSysutilLang>` writes (Emu/Cell/Modules/
//!   cellSysutil.cpp:363 to :393), one for each of the twenty
//!   `CellSysutilLang` (cellSysutil.h:93 to :115). A name it can't read makes
//!   RPCS3 fail to apply its config (Utilities/Config.cpp, `decode`;
//!   Emu/System.cpp:603 to :612), so only these are ever handed out.
//! - Cemu, Omoio's build at v2.6-2: settings.xml's `<console_language>` is
//!   the number of a `CafeConsoleLanguage` (src/config/CemuConfig.h:160 to
//!   :174). Twelve: one Portuguese, ZH for Simplified Chinese and TW for
//!   Traditional, as meta.xml's `zhs` and `zht` are read
//!   (src/Cafe/TitleList/ParsedMetaXml.h:124 to :151).
//! - The Wii, Dolphin 2609a: the SYSCONF's `IPL.LNG`, numbered as
//!   `DiscIO::Language`, which is Nintendo's own numbering
//!   (Source/Core/DiscIO/Enums.h:55 to :71).
//! - The GameCube, Dolphin 2609a: `[Core] SelectedLanguage`
//!   (Core/Config/MainSettings.cpp:70), the PAL GameCube's six, English to
//!   Dutch as 0 to 5 (`ToGameCubeLanguage`, DiscIO/Enums.cpp:155 to :161).
//!
//! Dolphin keeps every disc in a language its region had, unless
//! `OverrideRegionSettings` is on, which Omoio never sets: a Japanese disc in
//! Japanese, a Korean Wii disc in Korean, a US Wii disc in English unless the
//! pick is French or Spanish, a US GameCube disc in English, and a European
//! disc in English unless the pick is one of English to Dutch
//! (`GetLanguageAdjustedForRegion`, Core/ConfigManager.cpp:517 to :549). So
//! German reaches a European disc and never a US one, which is what the
//! setting's hint says about every console: a game starts in the language
//! picked when its release has it.

use super::console::Console;
use super::settings::Settings;
use serde::Serialize;

pub struct Language {
    pub id: &'static str,
    pub name: &'static str,
}

/// RPCS3's twenty, in the order someone looks for theirs: English first,
/// then Europe's languages, then Asia's.
pub const LANGUAGES: [Language; 20] = [
    Language { id: "en-gb", name: "English (UK)" },
    Language { id: "en-us", name: "English (US)" },
    Language { id: "de", name: "German" },
    Language { id: "fr", name: "French" },
    Language { id: "es", name: "Spanish" },
    Language { id: "it", name: "Italian" },
    Language { id: "nl", name: "Dutch" },
    Language { id: "pt-pt", name: "Portuguese (Portugal)" },
    Language { id: "pt-br", name: "Portuguese (Brazil)" },
    Language { id: "ru", name: "Russian" },
    Language { id: "pl", name: "Polish" },
    Language { id: "da", name: "Danish" },
    Language { id: "fi", name: "Finnish" },
    Language { id: "no", name: "Norwegian" },
    Language { id: "sv", name: "Swedish" },
    Language { id: "tr", name: "Turkish" },
    Language { id: "ja", name: "Japanese" },
    Language { id: "ko", name: "Korean" },
    Language { id: "zh-hant", name: "Chinese (Traditional)" },
    Language { id: "zh-hans", name: "Chinese (Simplified)" },
];

/// English on each Nintendo console, which a console is set to when it has
/// no such language as the one picked, so the note beside the setting is
/// true: `CafeConsoleLanguage::EN`, `DiscIO::Language::English`, and the
/// GameCube's first.
const CEMU_ENGLISH: u8 = 1;
const WII_ENGLISH: u8 = 1;
const GAMECUBE_ENGLISH: u8 = 0;

/// One language on each console, `None` where the console has no such
/// language.
struct Values {
    rpcs3: &'static str,
    cemu: Option<u8>,
    wii: Option<u8>,
    gamecube: Option<u8>,
}

/// Three are less plain than the rest:
///
/// - Japanese on the GameCube: SelectedLanguage has no Japanese, but its 0
///   is Japanese on a Japanese GameCube ("English and Japanese both use the
///   value 0 in GC SRAM", ConfigManager.cpp:525), and Dolphin starts every
///   Japanese disc in Japanese. So the GameCube has Japanese wherever a game
///   does.
/// - Korean: the Wii has it, and Dolphin starts a Korean Wii disc in it. The
///   GameCube has none: Dolphin runs a Korean GameCube disc as a Japanese one
///   (ConfigManager.cpp:521 to :522).
/// - Chinese: the Wii numbers both, but neither is "selectable on any
///   unmodded retail Wii" (Enums.h:67 to :68), and Dolphin gives neither to
///   any game: a Japanese or Korean disc gets its own language and every
///   other disc English. So the Wii is taken to have none.
fn values(id: &str) -> Option<Values> {
    let (rpcs3, cemu, wii, gamecube) = match id {
        "en-gb" => ("English (UK)", Some(1), Some(1), Some(0)),
        "en-us" => ("English (US)", Some(1), Some(1), Some(0)),
        "de" => ("German", Some(3), Some(2), Some(1)),
        "fr" => ("French", Some(2), Some(3), Some(2)),
        "es" => ("Spanish", Some(5), Some(4), Some(3)),
        "it" => ("Italian", Some(4), Some(5), Some(4)),
        "nl" => ("Dutch", Some(8), Some(6), Some(5)),
        "pt-pt" => ("Portuguese (Portugal)", Some(9), None, None),
        "pt-br" => ("Portuguese (Brazil)", Some(9), None, None),
        "ru" => ("Russian", Some(10), None, None),
        "pl" => ("Polish", None, None, None),
        "da" => ("Danish", None, None, None),
        "fi" => ("Finnish", None, None, None),
        "no" => ("Norwegian", None, None, None),
        "sv" => ("Swedish", None, None, None),
        "tr" => ("Turkish", None, None, None),
        "ja" => ("Japanese", Some(0), Some(0), Some(0)),
        "ko" => ("Korean", Some(7), Some(9), None),
        "zh-hant" => ("Chinese (Traditional)", Some(11), None, None),
        "zh-hans" => ("Chinese (Simplified)", Some(6), None, None),
        _ => return None,
    };
    Some(Values { rpcs3, cemu, wii, gamecube })
}

/// The language with this id, or `None` for one Omoio doesn't know.
pub fn find(id: &str) -> Option<&'static Language> {
    let all: &'static [Language] = &LANGUAGES;
    all.iter().find(|language| language.id == id)
}

/// RPCS3's name for the language, as config.yml's Language takes it. The
/// PS3 has all twenty.
pub fn rpcs3(id: &str) -> Option<&'static str> {
    values(id).map(|values| values.rpcs3)
}

/// Cemu's `console_language`, English where the Wii U has no such language.
pub fn cemu(id: &str) -> Option<u8> {
    values(id).map(|values| values.cemu.unwrap_or(CEMU_ENGLISH))
}

/// The Wii's `IPL.LNG`, English where the Wii has no such language.
pub fn wii(id: &str) -> Option<u8> {
    values(id).map(|values| values.wii.unwrap_or(WII_ENGLISH))
}

/// The GameCube's `SelectedLanguage`, English where the GameCube has no such
/// language.
pub fn gamecube(id: &str) -> Option<u8> {
    values(id).map(|values| values.gamecube.unwrap_or(GAMECUBE_ENGLISH))
}

/// Whether the console has no such language, so its games start in English.
/// An id Omoio doesn't know lacks nothing, since nothing is written for it.
pub fn lacks(console: Console, id: &str) -> bool {
    let Some(values) = values(id) else {
        return false;
    };
    match console {
        Console::Ps3 => false,
        Console::WiiU => values.cemu.is_none(),
        Console::Wii => values.wii.is_none(),
        Console::GameCube => values.gamecube.is_none(),
    }
}

/// The line beside the setting for this language, naming the consoles that
/// don't have it: "The Wii U, the Wii and the GameCube have no Danish, so
/// their games start in English." `None` when every console has it.
pub fn note(id: &str) -> Option<String> {
    let language = find(id)?;
    let lacking: Vec<Console> = Console::ALL.into_iter().filter(|&console| lacks(console, id)).collect();
    let names: Vec<String> = lacking
        .iter()
        .enumerate()
        .map(|(at, console)| format!("{} {}", if at == 0 { "The" } else { "the" }, console.short()))
        .collect();
    let consoles = match names.split_last()? {
        (last, []) => last.clone(),
        (last, before) => format!("{} and {last}", before.join(", ")),
    };
    let (have, their) = if lacking.len() == 1 { ("has", "its") } else { ("have", "their") };
    // A console without Brazil's Portuguese has no Portuguese at all, so the
    // sentence names the language without the bracket.
    let spoken = language.name.split(" (").next().unwrap_or(language.name);
    Some(format!("{consoles} {have} no {spoken}, so {their} games start in English."))
}

/// One language as the setting lists it.
#[derive(Debug, Serialize)]
pub struct Choice {
    pub id: &'static str,
    pub name: &'static str,
    pub note: Option<String>,
}

/// Every language, in the order the setting lists them.
pub fn choices() -> Vec<Choice> {
    LANGUAGES.iter().map(|language| Choice { id: language.id, name: language.name, note: note(language.id) }).collect()
}

/// A region was picked, whose Language is `language` in RPCS3's words.
///
/// What RPCS3's Language held before a game language replaced it is kept,
/// and put back once the setting is on its default again. A region picked
/// since then says what the default should give, as the region always has,
/// so it takes the place of what was kept: while a game language is picked,
/// and after a return to the default that no PS3 game has put back yet.
/// With neither, RPCS3's file holds the region's language itself, as it
/// always has, and nothing is kept.
pub fn follow_region(settings: &mut Settings, language: &str) {
    if settings.game_language.is_some() || settings.language_kept.contains_key("rpcs3") {
        settings.language_kept.insert("rpcs3".to_string(), language.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_id() -> impl Iterator<Item = &'static str> {
        LANGUAGES.iter().map(|language| language.id)
    }

    #[test]
    fn ids_are_unique_and_english_comes_first() {
        let mut ids: Vec<&str> = every_id().collect();
        assert_eq!(&ids[..2], ["en-gb", "en-us"]);
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), LANGUAGES.len());
    }

    #[test]
    fn every_language_has_a_value_on_every_console() {
        for id in every_id() {
            assert!(find(id).is_some(), "{id}");
            assert!(rpcs3(id).is_some(), "{id} on the PS3");
            assert!(cemu(id).is_some(), "{id} on the Wii U");
            assert!(wii(id).is_some(), "{id} on the Wii");
            assert!(gamecube(id).is_some(), "{id} on the GameCube");
        }
    }

    #[test]
    fn an_unknown_id_gets_nothing() {
        for id in ["", "german", "DE", "en", "pt", "zh"] {
            assert!(find(id).is_none(), "{id}");
            assert_eq!(rpcs3(id), None);
            assert_eq!(cemu(id), None);
            assert_eq!(wii(id), None);
            assert_eq!(gamecube(id), None);
            assert!(Console::ALL.into_iter().all(|console| !lacks(console, id)));
            assert_eq!(note(id), None);
        }
    }

    #[test]
    fn rpcs3_takes_the_names_cellsysutil_writes() {
        // CellSysutilLang 0 to 19 and fmt_class_string's name for each.
        let expected = [
            ("ja", "Japanese"),
            ("en-us", "English (US)"),
            ("fr", "French"),
            ("es", "Spanish"),
            ("de", "German"),
            ("it", "Italian"),
            ("nl", "Dutch"),
            ("pt-pt", "Portuguese (Portugal)"),
            ("ru", "Russian"),
            ("ko", "Korean"),
            ("zh-hant", "Chinese (Traditional)"),
            ("zh-hans", "Chinese (Simplified)"),
            ("fi", "Finnish"),
            ("sv", "Swedish"),
            ("da", "Danish"),
            ("no", "Norwegian"),
            ("pl", "Polish"),
            ("pt-br", "Portuguese (Brazil)"),
            ("en-gb", "English (UK)"),
            ("tr", "Turkish"),
        ];
        assert_eq!(expected.len(), LANGUAGES.len());
        for (id, name) in expected {
            assert_eq!(rpcs3(id), Some(name), "{id}");
        }
        assert!(every_id().all(|id| !lacks(Console::Ps3, id)), "the PS3 has all twenty");
    }

    #[test]
    fn cemu_takes_cafe_console_language() {
        let expected = [
            ("ja", 0),
            ("en-gb", 1),
            ("en-us", 1),
            ("fr", 2),
            ("de", 3),
            ("it", 4),
            ("es", 5),
            ("zh-hans", 6),
            ("ko", 7),
            ("nl", 8),
            ("pt-pt", 9),
            ("pt-br", 9),
            ("ru", 10),
            ("zh-hant", 11),
        ];
        for (id, value) in expected {
            assert_eq!(cemu(id), Some(value), "{id}");
            assert!(!lacks(Console::WiiU, id), "{id}");
        }
        for id in ["pl", "da", "fi", "no", "sv", "tr"] {
            assert_eq!(cemu(id), Some(CEMU_ENGLISH), "{id} starts in English");
            assert!(lacks(Console::WiiU, id), "{id}");
        }
    }

    #[test]
    fn the_wii_takes_its_own_numbering() {
        let expected = [("ja", 0), ("en-gb", 1), ("en-us", 1), ("de", 2), ("fr", 3), ("es", 4), ("it", 5), ("nl", 6), ("ko", 9)];
        for (id, value) in expected {
            assert_eq!(wii(id), Some(value), "{id}");
            assert!(!lacks(Console::Wii, id), "{id}");
        }
        for id in ["pt-pt", "pt-br", "ru", "pl", "da", "fi", "no", "sv", "tr", "zh-hant", "zh-hans"] {
            assert_eq!(wii(id), Some(WII_ENGLISH), "{id} starts in English");
            assert!(lacks(Console::Wii, id), "{id}");
        }
    }

    #[test]
    fn the_gamecube_takes_the_pal_six() {
        let expected = [("en-gb", 0), ("en-us", 0), ("de", 1), ("fr", 2), ("es", 3), ("it", 4), ("nl", 5), ("ja", 0)];
        for (id, value) in expected {
            assert_eq!(gamecube(id), Some(value), "{id}");
            assert!(!lacks(Console::GameCube, id), "{id}");
        }
        for id in ["pt-pt", "pt-br", "ru", "pl", "da", "fi", "no", "sv", "tr", "ko", "zh-hant", "zh-hans"] {
            assert_eq!(gamecube(id), Some(GAMECUBE_ENGLISH), "{id} starts in English");
            assert!(lacks(Console::GameCube, id), "{id}");
        }
    }

    #[test]
    fn the_note_names_the_consoles_without_the_language() {
        assert_eq!(
            note("da").as_deref(),
            Some("The Wii U, the Wii and the GameCube have no Danish, so their games start in English.")
        );
        assert_eq!(note("ru").as_deref(), Some("The Wii and the GameCube have no Russian, so their games start in English."));
        assert_eq!(
            note("pt-br").as_deref(),
            Some("The Wii and the GameCube have no Portuguese, so their games start in English.")
        );
        assert_eq!(note("zh-hant").as_deref(), Some("The Wii and the GameCube have no Chinese, so their games start in English."));
        assert_eq!(note("ko").as_deref(), Some("The GameCube has no Korean, so its games start in English."));
        for id in ["en-gb", "en-us", "de", "fr", "es", "it", "nl", "ja"] {
            assert_eq!(note(id), None, "every console has {id}");
        }
    }

    #[test]
    fn the_list_carries_each_note() {
        let listed = choices();
        assert_eq!(listed.len(), LANGUAGES.len());
        assert_eq!(listed[0].name, "English (UK)");
        let danish = listed.iter().find(|choice| choice.id == "da").unwrap();
        assert_eq!(danish.note, note("da"));
        let json = serde_json::to_string(&listed[2]).unwrap();
        assert_eq!(json, r#"{"id":"de","name":"German","note":null}"#);
    }

    #[test]
    fn a_region_changes_nothing_kept_for_a_player_who_never_picked_a_language() {
        let mut settings = Settings::default();
        follow_region(&mut settings, "Japanese");
        assert!(settings.language_kept.is_empty());
    }

    #[test]
    fn a_region_is_what_the_default_goes_back_to() {
        let mut settings = Settings { game_language: Some("de".to_string()), ..Settings::default() };
        follow_region(&mut settings, "Japanese");
        assert_eq!(settings.language_kept["rpcs3"], "Japanese", "before any PS3 game kept one");

        settings.language_kept.insert("rpcs3".to_string(), "English (UK)".to_string());
        settings.language_kept.insert("cemu".to_string(), "1".to_string());
        follow_region(&mut settings, "French");
        assert_eq!(settings.language_kept["rpcs3"], "French");
        assert_eq!(settings.language_kept["cemu"], "1", "Cemu's is no region's");
    }

    #[test]
    fn a_region_after_the_default_but_before_a_game_replaces_what_was_kept() {
        let mut settings = Settings::default();
        settings.language_kept.insert("rpcs3".to_string(), "English (UK)".to_string());
        follow_region(&mut settings, "Korean");
        assert_eq!(settings.language_kept["rpcs3"], "Korean");
    }
}
