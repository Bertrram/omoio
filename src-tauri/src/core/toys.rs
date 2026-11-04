//! The toys a game is played with, which pick the menu over it: the
//! Skylanders portal, the Disney Infinity base and the LEGO Dimensions Toy
//! Pad. A Skylanders game is sent to figures.rs for every answer, as it was
//! before there were other toys, so nothing it does changes.

use crate::core::console::{Console, Features};
use crate::core::dimensions;
use crate::core::figures;
use crate::core::infinity::{self, InfinityGame};
use serde::{Deserialize, Serialize};

/// The toy a game is played with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Toy {
    Skylanders,
    Infinity,
    /// LEGO Dimensions' Toy Pad.
    ToyPad,
}

impl Toy {
    /// The page the menu's window is built with over a game of this toy.
    /// There is one window, the portal menu's, so whatever looks for it
    /// finds it for every toy; it lives for one game, so its page is picked
    /// once, as it is built.
    pub fn page(self) -> &'static str {
        match self {
            Self::Skylanders => "portal.html",
            Self::Infinity => "infinity.html",
            Self::ToyPad => "toypad.html",
        }
    }

    /// The menu window's title.
    pub fn title(self) -> &'static str {
        match self {
            Self::Skylanders => "Portal",
            Self::Infinity => "Infinity Base",
            Self::ToyPad => "Toy Pad",
        }
    }

    /// The folder inside Omoio's figures folder that the toy's files are
    /// kept in. Skylanders' are in the figures folder itself, as they always
    /// were, and the Skylanders menu reads only that folder's top level
    /// (portal_menu.rs, `list`), so it never lists another toy's.
    pub fn folder(self) -> Option<&'static str> {
        match self {
            Self::Skylanders => None,
            Self::Infinity => Some("Infinity"),
            Self::ToyPad => Some("Dimensions"),
        }
    }

    /// The size of every one of the toy's figure files, which tells a .bin
    /// the player brought in: an Infinity figure is 20 blocks of 16 bytes
    /// (Cemu's `INF_FIGURE_SIZE`, src/Cafe/OS/libs/nsyshid/Infinity.h lines
    /// 45 to 47, Bertrram/Cemu v2.6-2; RPCS3's rpcs3/Emu/Io/Infinity.h line
    /// 12, master b2e52d6; Dolphin's Core/IOS/USB/Emulated/Infinity.h lines
    /// 18 and 19, 2609a), and a Toy Pad tag 45 pages of 4 (Cemu's
    /// nsyshid/Dimensions.h line 50), read 4 November 2026. A Skylanders
    /// figure is 1024 bytes, and no emulator takes a file of either size for
    /// one, so sorting by them moves no file that worked. `None` for
    /// Skylanders, whose files are told by their extension as they always
    /// were.
    pub fn file_size(self) -> Option<u64> {
        match self {
            Self::Skylanders => None,
            Self::Infinity => Some(320),
            Self::ToyPad => Some(180),
        }
    }

    /// How many places the toy has: the portal menu's slots
    /// (backends/cemu/portal.rs, `SLOTS`), the base's nine, and the Toy
    /// Pad's seven, three on each side pad and one in the middle
    /// (dimensions.rs).
    pub fn places(self) -> usize {
        match self {
            Self::Skylanders => 16,
            Self::Infinity => infinity::PLACES,
            Self::ToyPad => dimensions::PLACES,
        }
    }
}

/// A figure an emulator's figure maker makes: its name and number as the
/// maker lists it, and for Disney Infinity the series it came out with, 1,
/// 2 or 3 for 1.0, 2.0 and 3.0, where the list gives one (Cemu's and
/// RPCS3's do, Dolphin's doesn't). The Toy Pad's figures have no series.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToyFigure {
    pub name: String,
    pub number: u32,
    pub series: Option<u8>,
}

/// The toy a game is played with, or `None` for a game with no toy.
/// Skylanders exactly where `figures::is_skylanders` says so, asked first,
/// so every Skylanders game takes the paths it always took. Then Disney
/// Infinity, by its id and then its name (infinity.rs), and then LEGO
/// Dimensions the same way (dimensions.rs).
pub fn toy_of(console: Console, title_id: &str, title: &str) -> Option<Toy> {
    if figures::is_skylanders(title) {
        return Some(Toy::Skylanders);
    }
    if InfinityGame::of(console, title_id, title).is_some() {
        return Some(Toy::Infinity);
    }
    dimensions::is_dimensions(console, title_id, title).then_some(Toy::ToyPad)
}

/// Where the base menu is offered: every Disney Infinity release, on each
/// console whose emulator fills the base. None has been played with the menu
/// in Omoio yet, and each is offered so players can say how it goes. What
/// is known of each game in each emulator was read on 4 November 2026.
const INFINITY_MENU: [(Console, InfinityGame); 3] = [
    // Untested in Omoio until Bertram has played it. RPCS3's list rates it
    // Ingame (BLUS30977 and BLES01842, 22 January 2023). The three games
    // show blocky pictures in RPCS3 (RPCS3 issue 14572, open); a player
    // found 1.0 playable with Vsync on Full and Strict Rendering Mode on (7
    // July 2026), which Omoio doesn't set.
    (Console::Ps3, InfinityGame::One),
    // Untested in Omoio until Bertram has played it. Ingame (BLUS31418, 22
    // June 2023; BLES02064, 17 March 2024), with RPCS3 issue 14572.
    (Console::Ps3, InfinityGame::Two),
    // Untested in Omoio until Bertram has played it. Ingame on disc (27
    // November 2023), Intro from the US PlayStation Store (NPUB31657, 24
    // October 2026), with RPCS3 issue 14572.
    (Console::Ps3, InfinityGame::Three),
    // The Wii U and Wii versions are left out of this release, which starts
    // with the PS3 versions (Bertram, 4 November 2026): the Wii U's base
    // needs an Omoio Cemu newer than v2.6-2, and neither has been played.
    // What is known of them, for when they are offered:
    // - Wii U, 1.0: Cemu's wiki rates it Loads. Tests up to Cemu 2.0-94 stop
    //   at an Err_Eula notice after + at the menu, which a Cemu developer
    //   calls the game saying its online service is gone, and the black
    //   screen after it a known problem of its own (Cemu issue 1620). One
    //   test, on a Cemu called 2.7, calls it playable.
    // - Wii U, 2.0: Loads, with Err_Eula and a black screen after it, as
    //   1.0, and a hang at the base prompt in Cemu 1.22.12c. One test on
    //   Cemu 2.7 calls it playable.
    // - Wii U, 3.0: Loads, with Err_Eula coming back after A or +. It crashes
    //   at or after the first loading screen, and Cemu's lead says its
    //   rendering problems leave a black screen past that (Cemu issue 1714,
    //   open since 28 October 2025).
    // - Wii, 1.0: Dolphin's wiki gives it four stars, and Dolphin 2609a
    //   switches on its own patch for the game's speed
    //   (Data/Sys/GameSettings/SQIE4Q.ini, SQIP4Q.ini and SQIY4Q.ini).
];

/// Where the Toy Pad menu is offered: LEGO Dimensions on each console whose
/// emulator fills the Toy Pad, every release there, known by its id or its
/// name (dimensions.rs). Of the consoles Omoio runs, the game came out on
/// the PS3 and the Wii U alone (Wikipedia and GameTDB), and Dolphin has no
/// Toy Pad. It hasn't been played with the menu in Omoio yet, and it is
/// offered so players can say how it goes. All of it, and what is known of
/// the game in each emulator, was read on 4 November 2026.
const TOYPAD_MENU: [Console; 1] = [
    // Untested in Omoio until Bertram has played it. RPCS3's list rates
    // both discs, BLES02105 and BLUS31473, Playable (4 August 2024, build
    // f6b5511). The US disc crashes in the Phantom Zone level as the Zod
    // battle changes phase (RPCS3 issue 19156, open since August 2026).
    // RPCS3 keeps none of the lights the game sets on the pads
    // (rpcs3/Emu/Io/Dimensions.cpp lines 647 to 659, master b2e52d6), so
    // the menu can't show them there.
    Console::Ps3,
    // The Wii U version is left out of this release, which starts with the
    // PS3 version (Bertram, 4 November 2026). What is known of it: Cemu's
    // wiki rates it Playable (revision 19675, 25 January 2026), with shadows
    // wrong away from 720p and voices quiet, and the menu fills Cemu's Toy
    // Pad through the commands of an Omoio Cemu newer than v2.6-2.
];

/// Whether the toy's menu works in this game on this console. Skylanders'
/// is `figures::has_portal_menu`, as it always was.
pub fn has_menu(toy: Toy, console: Console, title_id: &str, title: &str) -> bool {
    match toy {
        Toy::Skylanders => figures::has_portal_menu(console, title),
        Toy::Infinity => {
            InfinityGame::of(console, title_id, title).is_some_and(|game| INFINITY_MENU.contains(&(console, game)))
        }
        Toy::ToyPad => dimensions::is_dimensions(console, title_id, title) && TOYPAD_MENU.contains(&console),
    }
}

/// Whether a game in the library shows its toy's menu: its emulator fills
/// the toy, and the menu works in the game. The game's page and Big Picture
/// ask this, so neither guesses from the title. A Skylanders game is asked
/// of `figures::offers_portal_menu`, as it always was.
pub fn offers_menu(features: Features, console: Console, title_id: &str, title: &str) -> bool {
    match toy_of(console, title_id, title) {
        Some(Toy::Skylanders) => figures::offers_portal_menu(features, console, title),
        Some(toy @ Toy::Infinity) => features.infinity && has_menu(toy, console, title_id, title),
        Some(toy @ Toy::ToyPad) => features.toypad && has_menu(toy, console, title_id, title),
        None => false,
    }
}

/// What the menu's key or button shows over a game whose toy's menu isn't
/// offered, instead of the menu. A Skylanders game's is
/// `figures::menu_note`, as it always was. `None` where the menu is offered,
/// and for a game with no toy, where the key does nothing.
pub fn menu_note(features: Features, console: Console, title_id: &str, title: &str) -> Option<String> {
    let toy = toy_of(console, title_id, title)?;
    if toy == Toy::Skylanders {
        return figures::menu_note(features, console, title);
    }
    if offers_menu(features, console, title_id, title) {
        return None;
    }
    let menu = if toy == Toy::ToyPad { "The Toy Pad menu" } else { "The base menu" };
    let elsewhere = elsewhere(toy, console, title_id, title);
    Some(if elsewhere.is_empty() {
        format!("{menu} doesn't work in this game yet.")
    } else {
        format!("{menu} doesn't work in this version yet. It works in {}.", figures::versions(&elsewhere))
    })
}

/// The other consoles whose version of the game has the menu, for a game
/// whose own version doesn't. None where its version has it, and the
/// emulator is what can't fill the toy.
fn elsewhere(toy: Toy, console: Console, title_id: &str, title: &str) -> Vec<Console> {
    if has_menu(toy, console, title_id, title) {
        return Vec::new();
    }
    match toy {
        Toy::Infinity => {
            let Some(game) = InfinityGame::of(console, title_id, title) else {
                return Vec::new();
            };
            Console::ALL.into_iter().filter(|&other| INFINITY_MENU.contains(&(other, game))).collect()
        }
        Toy::ToyPad => Console::ALL.into_iter().filter(|other| TOYPAD_MENU.contains(other)).collect(),
        Toy::Skylanders => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every title figures.rs's own tests use, Skylanders or not, and the
    /// Disney Infinity and LEGO Dimensions names.
    const TITLES: [&str; 40] = [
        "Skylanders: Spyro's Adventure",
        "Skylanders Spyro's Adventure",
        "Skylanders Giants™",
        "Skylanders Giants",
        "Skylanders: Giants",
        "skylanders giants",
        "Skylanders SWAP Force",
        "Skylanders - Swap Force",
        "Skylanders: Swap Force",
        "SKYLANDERS SWAP FORCE",
        "Skylanders Trap Team",
        "Skylanders: Trap Team",
        "Skylanders - Trap Team",
        "Skylanders Trap Team™",
        "Skylanders_Trap_Team",
        "SKYLANDERS",
        "Skylanders SuperChargers",
        "Skylanders: SuperChargers",
        "Skylanders SuperChargers™",
        "Skylanders SuperChargers Racing",
        "Skylanders Imaginators",
        "Skylanders: Imaginators",
        "Skylanders Battlecast",
        "スカイランダーズ　スパイロの大冒険",
        "Spyro's Adventure (English)",
        "LittleBigPlanet 3",
        "Giants: Citizen Kabuto",
        "Mario Kart 8",
        "Disney INFINITY",
        "Disney Infinity",
        "Disney Infinity [2.0]",
        "Disney Infinity: Marvel Super Heroes",
        "DISNEY INFINITY 3.0 EDITION",
        "Disney Infinity 3.0",
        "Infinity Runner",
        "LEGO Marvel Super Heroes",
        "LEGO Dimensions",
        "LEGO® DIMENSIONS™",
        "LEGO® Dimensions™",
        "",
    ];

    /// Ids of every shape the library keeps: a PS3 serial, a Wii U title id
    /// and disc image id, a Wii game id, one of each Disney Infinity's and
    /// one of each LEGO Dimensions'.
    const IDS: [&str; 11] = [
        "BLES02055",
        "0005000010142D00",
        "WUD0123456789ABC",
        "SX7P52",
        "",
        "BLES01844",
        "00050000101DDC00",
        "SQIE4Q",
        "000500001016DC00",
        "BLUS31473",
        "0005000010195D00",
    ];

    fn every(features: Features) -> [Features; 4] {
        [
            features,
            Features { portal: true, ..features },
            Features { infinity: true, toypad: true, ..features },
            Features { portal: true, infinity: true, toypad: true, ..features },
        ]
    }

    #[test]
    fn a_game_is_skylanders_exactly_where_it_always_was() {
        for title in TITLES {
            for console in Console::ALL {
                for id in IDS {
                    let toy = toy_of(console, id, title);
                    assert_eq!(toy == Some(Toy::Skylanders), figures::is_skylanders(title), "{title} {id}");
                }
            }
        }
        // The title decides, whatever the id says.
        assert_eq!(toy_of(Console::Ps3, "BLES01844", "Skylanders Giants"), Some(Toy::Skylanders));
    }

    #[test]
    fn a_skylanders_game_gets_the_answers_it_always_got() {
        for title in TITLES {
            for console in Console::ALL {
                for id in IDS {
                    let toy = toy_of(console, id, title);
                    if toy == Some(Toy::Skylanders) {
                        assert_eq!(has_menu(Toy::Skylanders, console, id, title), figures::has_portal_menu(console, title));
                    }
                    if matches!(toy, Some(Toy::Skylanders) | None) {
                        for features in every(Features::default()) {
                            let note = menu_note(features, console, id, title);
                            assert_eq!(note, figures::menu_note(features, console, title), "{title}");
                            let offered = offers_menu(features, console, id, title);
                            assert_eq!(offered, figures::offers_portal_menu(features, console, title), "{title}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn disney_infinity_is_known_by_its_id_and_its_name() {
        assert_eq!(toy_of(Console::Ps3, "BLES01844", "Disney Infinity"), Some(Toy::Infinity));
        assert_eq!(toy_of(Console::Ps3, "BLES01844", ""), Some(Toy::Infinity));
        assert_eq!(toy_of(Console::WiiU, "00050000101DDC00", "ディズニーインフィニティ3.0"), Some(Toy::Infinity));
        assert_eq!(toy_of(Console::WiiU, "000500001016DC00", "Disney Infinity [2.0]"), Some(Toy::Infinity));
        assert_eq!(toy_of(Console::WiiU, "WUD0123456789ABC", "DISNEY INFINITY 3.0 EDITION"), Some(Toy::Infinity));
        assert_eq!(toy_of(Console::Wii, "SQIY4Q", "Disney Infinity"), Some(Toy::Infinity));
        assert_eq!(toy_of(Console::Ps3, "BLUS31522", "Disney Infinity 3.0"), Some(Toy::Infinity));
        for title in ["Infinity Runner", "Ace Combat Infinity", "LEGO Marvel Super Heroes", "Mario Kart 8"] {
            for console in Console::ALL {
                assert_eq!(toy_of(console, "BLES02055", title), None, "{title}");
            }
        }
        for console in Console::ALL {
            assert_ne!(toy_of(console, "BLES02055", "LEGO Dimensions"), Some(Toy::Infinity));
        }
    }

    #[test]
    fn a_game_is_disney_infinity_exactly_where_infinity_rs_says() {
        for title in TITLES {
            for console in Console::ALL {
                for id in IDS {
                    let infinity = InfinityGame::of(console, id, title).is_some() && !figures::is_skylanders(title);
                    assert_eq!(toy_of(console, id, title) == Some(Toy::Infinity), infinity, "{title} {id}");
                }
            }
        }
    }

    #[test]
    fn lego_dimensions_is_known_by_its_id_and_its_name() {
        use Console::*;
        let releases = [(WiiU, "0005000010194200"), (WiiU, "0005000010195D00"), (Ps3, "BLES02105"), (Ps3, "BLUS31473")];
        for (console, id) in releases {
            assert_eq!(toy_of(console, id, ""), Some(Toy::ToyPad), "{id}");
            assert_eq!(toy_of(console, id, "LEGO® DIMENSIONS™"), Some(Toy::ToyPad), "{id}");
        }
        for title in ["LEGO® DIMENSIONS™", "LEGO® Dimensions™", "LEGO Dimensions", "LEGO_Dimensions"] {
            for console in Console::ALL {
                assert_eq!(toy_of(console, "WUD0123456789ABC", title), Some(Toy::ToyPad), "{title}");
            }
        }
        for title in ["LEGO Marvel Super Heroes", "LEGO Batman 3: Beyond Gotham", "Hyperdimension Neptunia"] {
            for console in Console::ALL {
                assert_eq!(toy_of(console, "BLES02055", title), None, "{title}");
            }
        }
        // A game takes the Toy Pad only where dimensions.rs says it is LEGO
        // Dimensions.
        for title in TITLES {
            for console in Console::ALL {
                for id in IDS {
                    let toypad = toy_of(console, id, title) == Some(Toy::ToyPad);
                    assert_eq!(
                        toypad,
                        dimensions::is_dimensions(console, id, title)
                            && !figures::is_skylanders(title)
                            && InfinityGame::of(console, id, title).is_none(),
                        "{title} {id}"
                    );
                    assert!(!has_menu(Toy::ToyPad, console, id, title) || dimensions::is_dimensions(console, id, title));
                }
            }
        }
    }

    #[test]
    fn the_toy_pad_menu_is_offered_for_every_release_where_the_emulator_fills_the_toy_pad() {
        use Console::*;
        let releases = [
            (Ps3, "BLES02105", "LEGO® DIMENSIONS™"),
            (Ps3, "BLUS31473", "LEGO® DIMENSIONS™"),
            (Ps3, "BLES99999", "LEGO Dimensions"),
        ];
        let toypad = Features { toypad: true, ..Features::default() };
        for (console, id, title) in releases {
            assert!(has_menu(Toy::ToyPad, console, id, title), "{title}");
            assert!(offers_menu(toypad, console, id, title), "{title}");
            assert!(!offers_menu(Features::default(), console, id, title), "{title}");
            assert!(!offers_menu(Features { portal: true, infinity: true, ..Features::default() }, console, id, title));
            assert_eq!(menu_note(toypad, console, id, title), None, "{title}");
        }
        // This release offers the PS3 version only (Bertram, 4 November
        // 2026), and there is no LEGO Dimensions for the Wii or the GameCube.
        for (console, id, title) in [
            (WiiU, "0005000010194200", "LEGO® Dimensions™"),
            (WiiU, "0005000010195D00", "LEGO® DIMENSIONS™"),
            (WiiU, "WUD0123456789ABC", "LEGO Dimensions"),
        ] {
            assert!(!has_menu(Toy::ToyPad, console, id, title), "{title}");
            assert_eq!(
                menu_note(toypad, console, id, title).as_deref(),
                Some("The Toy Pad menu doesn't work in this version yet. It works in the PS3 version.")
            );
        }
        assert!(!has_menu(Toy::ToyPad, Wii, "", "LEGO Dimensions"));
        assert!(!has_menu(Toy::ToyPad, GameCube, "", "LEGO Dimensions"));
        assert!(!has_menu(Toy::ToyPad, Ps3, "BLES02055", "Skylanders Trap Team"));
        assert!(!has_menu(Toy::ToyPad, Ps3, "BLES01844", "Disney Infinity"));
        assert!(!offers_menu(toypad, Ps3, "", "LEGO Marvel Super Heroes"));
    }

    #[test]
    fn the_toy_pad_menus_key_says_why_where_the_menu_isnt_offered() {
        let toypad = Features { toypad: true, ..Features::default() };
        assert_eq!(
            menu_note(Features::default(), Console::Ps3, "BLES02105", "LEGO® DIMENSIONS™").as_deref(),
            Some("The Toy Pad menu doesn't work in this game yet.")
        );
        assert_eq!(
            menu_note(toypad, Console::Wii, "", "LEGO Dimensions").as_deref(),
            Some("The Toy Pad menu doesn't work in this version yet. It works in the PS3 version.")
        );
        assert_eq!(menu_note(toypad, Console::Ps3, "", "LEGO Marvel Super Heroes"), None);
    }

    #[test]
    fn the_base_menu_is_offered_for_every_release_where_the_emulator_fills_the_base() {
        use Console::*;
        let releases = [
            (Ps3, "BLUS30977", "Disney Infinity"),
            (Ps3, "BLES01844", "Disney Infinity"),
            (Ps3, "BLES02064", "Disney Infinity [2.0]"),
            (Ps3, "BLJS10323", "Disney Infinity 3.0"),
        ];
        let base = Features { infinity: true, ..Features::default() };
        for (console, id, title) in releases {
            assert!(has_menu(Toy::Infinity, console, id, title), "{title}");
            assert!(offers_menu(base, console, id, title), "{title}");
            assert!(!offers_menu(Features::default(), console, id, title), "{title}");
            assert!(!offers_menu(Features { portal: true, toypad: true, ..Features::default() }, console, id, title));
            assert_eq!(menu_note(base, console, id, title), None, "{title}");
        }
        // This release offers the PS3 versions only (Bertram, 4 November
        // 2026). There is no Disney Infinity for the GameCube, nor 2.0 or 3.0
        // for the Wii.
        for (console, id, title) in [
            (WiiU, "0005000010136F00", "Disney Infinity"),
            (WiiU, "0005000010188C00", "Disney Infinity [2.0]"),
            (WiiU, "00050000101B3D00", "DISNEY INFINITY 3.0: PLAY WITHOUT LIMITS"),
            (WiiU, "00050000101DDC00", "ディズニーインフィニティ3.0"),
            (WiiU, "WUD0123456789ABC", "Disney Infinity 2.0: Marvel Super Heroes"),
            (Wii, "SQIP4Q", "Disney Infinity"),
        ] {
            assert!(!has_menu(Toy::Infinity, console, id, title), "{title}");
            assert_eq!(
                menu_note(base, console, id, title).as_deref(),
                Some("The base menu doesn't work in this version yet. It works in the PS3 version.")
            );
        }
        assert!(!has_menu(Toy::Infinity, GameCube, "", "Disney Infinity"));
        assert!(!has_menu(Toy::Infinity, Wii, "", "Disney Infinity 3.0"));
        assert!(!has_menu(Toy::Infinity, Ps3, "BLES02055", "Skylanders Trap Team"));
        assert!(!offers_menu(base, Ps3, "", "LittleBigPlanet 3"));
    }

    #[test]
    fn the_base_menus_key_says_why_where_the_menu_isnt_offered() {
        let base = Features { infinity: true, ..Features::default() };
        assert_eq!(
            menu_note(Features::default(), Console::Ps3, "BLES01844", "Disney Infinity").as_deref(),
            Some("The base menu doesn't work in this game yet.")
        );
        assert_eq!(
            menu_note(base, Console::Wii, "", "Disney Infinity 3.0").as_deref(),
            Some("The base menu doesn't work in this version yet. It works in the PS3 version.")
        );
        assert_eq!(
            menu_note(base, Console::GameCube, "", "Disney Infinity").as_deref(),
            Some("The base menu doesn't work in this version yet. It works in the PS3 version.")
        );
        assert_eq!(menu_note(base, Console::WiiU, "", "Mario Kart 8"), None);
        for features in every(Features::default()) {
            for console in Console::ALL {
                for title in TITLES {
                    let Some(note) = menu_note(features, console, "", title) else { continue };
                    assert!(note.ends_with('.') && !note.contains('!') && !note.contains('\u{2014}'), "{note}");
                }
            }
        }
    }

    #[test]
    fn each_toy_has_its_page_folder_and_files() {
        use Toy::*;
        assert_eq!([Skylanders, Infinity, ToyPad].map(Toy::page), ["portal.html", "infinity.html", "toypad.html"]);
        assert_eq!([Skylanders, Infinity, ToyPad].map(Toy::title), ["Portal", "Infinity Base", "Toy Pad"]);
        assert_eq!([Skylanders, Infinity, ToyPad].map(Toy::folder), [None, Some("Infinity"), Some("Dimensions")]);
        assert_eq!([Skylanders, Infinity, ToyPad].map(Toy::file_size), [None, Some(320), Some(180)]);
        assert_eq!([Skylanders, Infinity, ToyPad].map(Toy::places), [16, 9, 7]);
        assert_eq!(
            [Skylanders, Infinity, ToyPad].map(|toy| serde_json::to_value(toy).unwrap()),
            ["skylanders", "infinity", "toypad"]
        );
        assert_eq!(serde_json::from_str::<Toy>("\"toypad\"").unwrap(), ToyPad);
        let made = ToyFigure { name: "Stitch".into(), number: 1_000_118, series: Some(2) };
        let written = serde_json::to_value(&made).unwrap();
        assert_eq!(written, serde_json::json!({ "name": "Stitch", "number": 1_000_118, "series": 2 }));
        assert_eq!(serde_json::from_value::<ToyFigure>(written).unwrap(), made);
    }
}
