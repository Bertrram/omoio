//! Disney Infinity: which release a game is, what kind of piece each figure
//! number is, which of the base's nine places each kind goes in, and which
//! pieces each game takes. None of it comes from a figure file. A figure's
//! number sits encrypted in its file (Cemu's `InfinityUSB::LoadFigure`,
//! src/Cafe/OS/libs/nsyshid/Infinity.cpp lines 856 to 878, Bertrram/Cemu
//! v2.6-2, read 4 November 2026), and Omoio never decrypts one: it knows a
//! piece it had an emulator make from its own record of it, and one the
//! player brought only by the name the emulator shows once it is on.

use crate::core::console::Console;
use crate::core::toys::ToyFigure;
use serde::{Deserialize, Serialize};

/// The Disney Infinity games, in the order they came out. 2.0 is Marvel
/// Super Heroes. The Wii had 1.0 only (GameTDB, read 4 November 2026).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InfinityGame {
    One,
    Two,
    Three,
}

impl InfinityGame {
    /// The release a game is: by its id first, then by its name, which is
    /// all a Wii U disc image has, since its library id is made from its
    /// file's name (backends/cemu/mod.rs, `disc_id`). `None` for any other
    /// game.
    pub fn of(console: Console, title_id: &str, title: &str) -> Option<InfinityGame> {
        let ids: &[(&str, InfinityGame)] = match console {
            Console::Ps3 => &PS3,
            Console::WiiU => &WII_U,
            Console::Wii => &WII,
            Console::GameCube => &[],
        };
        ids.iter()
            .find(|(id, _)| id.eq_ignore_ascii_case(title_id))
            .map(|&(_, game)| game)
            .or_else(|| from_title(title))
    }

    /// The series the emulators' figure lists give the game's own pieces:
    /// 1, 2 and 3 for 1.0, 2.0 and 3.0 (Cemu's `s_listFigures`,
    /// nsyshid/Infinity.cpp lines 21 to 343, v2.6-2, and RPCS3's
    /// `list_figures`, rpcs3qt/infinity_dialog.cpp lines 31 to 353, master
    /// b2e52d6, which hold the same 322 entries; read 4 November 2026).
    pub fn series(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
        }
    }

    /// The game's name, for a disc image whose file's name doesn't say it.
    /// 2.0 goes by the name of its page on Cemu's wiki, "Disney Infinity
    /// 2.0 Marvel Super Heroes" (page 1069, read 4 November 2026), so the
    /// import warning finds its rating there.
    pub fn name(self) -> &'static str {
        match self {
            Self::One => "Disney Infinity",
            Self::Two => "Disney Infinity 2.0: Marvel Super Heroes",
            Self::Three => "Disney Infinity 3.0",
        }
    }
}

/// Every Wii U release by its title id, from WiiUBrew's title database
/// (revision 4868, 24 October 2026), whose product codes GameTDB's pages
/// agree with (read 4 November 2026). The Japanese 3.0 is the one only its
/// id tells: WiiUBrew notes that its meta.xml gives its English name in
/// Japanese. 000500021015A301 is the Japanese 1.0's demo.
const WII_U: [(&str, InfinityGame); 12] = [
    ("0005000010132900", InfinityGame::One),
    ("0005000010136F00", InfinityGame::One),
    ("0005000010137000", InfinityGame::One),
    ("000500001015A300", InfinityGame::One),
    ("000500021015A301", InfinityGame::One),
    ("000500001016DC00", InfinityGame::Two),
    ("0005000010188C00", InfinityGame::Two),
    ("0005000010188D00", InfinityGame::Two),
    ("00050000101AC900", InfinityGame::Three),
    ("00050000101B3D00", InfinityGame::Three),
    ("00050000101B3E00", InfinityGame::Three),
    ("00050000101DDC00", InfinityGame::Three),
];

/// Every Wii U disc by its product code, the four letters after "WUP-P-" at
/// the start of its image, from WiiUBrew's title database (revision 4868)
/// and GameTDB, read 4 November 2026. Neither has a 2.0 for Japan.
const WII_U_DISCS: [(&str, InfinityGame); 11] = [
    ("ADSE", InfinityGame::One),
    ("ADSP", InfinityGame::One),
    ("ADSZ", InfinityGame::One),
    ("ADSJ", InfinityGame::One),
    ("ADRE", InfinityGame::Two),
    ("ADRP", InfinityGame::Two),
    ("ADRZ", InfinityGame::Two),
    ("BD3E", InfinityGame::Three),
    ("BD3P", InfinityGame::Three),
    ("BD3Z", InfinityGame::Three),
    ("BD3J", InfinityGame::Three),
];

/// Every PS3 release by its serial, each found on Sony's update server
/// (the manifests Omoio's update check reads, backends/rpcs3/updates.rs),
/// whose TITLE for each says Disney Infinity, and most in GameTDB too, read
/// 4 November 2026. So a release missing here is still known by its name.
const PS3: [(&str, InfinityGame); 16] = [
    ("BLUS30977", InfinityGame::One),
    ("BLES01842", InfinityGame::One),
    ("BLES01843", InfinityGame::One),
    // Bertram's own disc, read 4 November 2026: TITLE "Disney Infinity",
    // APP_VER 01.00, VERSION 01.01, the release for Scandinavia (En, Sv,
    // No, Da, Fi). Sony's update server knows it; GameTDB doesn't.
    ("BLES01844", InfinityGame::One),
    ("BLUS31418", InfinityGame::Two),
    ("BLES02064", InfinityGame::Two),
    ("BLES02065", InfinityGame::Two),
    ("BLES02066", InfinityGame::Two),
    ("BLES02100", InfinityGame::Two),
    ("BLUS31522", InfinityGame::Three),
    ("BLES02147", InfinityGame::Three),
    ("BLES02148", InfinityGame::Three),
    ("BLJS10323", InfinityGame::Three),
    // The PlayStation Store's, in the US and in Europe.
    ("NPUB31657", InfinityGame::Three),
    ("NPEB02230", InfinityGame::Three),
    ("NPEB02231", InfinityGame::Three),
];

/// Every Wii release by its game id, from GameTDB and Dolphin's own titles
/// database (Data/Sys/wiitdb-en.txt lines 3137 to 3139, 2609a), read 4
/// November 2026.
const WII: [(&str, InfinityGame); 3] = [
    ("SQIE4Q", InfinityGame::One),
    ("SQIP4Q", InfinityGame::One),
    ("SQIY4Q", InfinityGame::One),
];

/// What every release's name has in it, whatever else it says: "Disney
/// INFINITY", "Disney Infinity [2.0]" and "DISNEY INFINITY 3.0 EDITION" in
/// WiiUBrew, and the Japanese 3.0's own, which WiiUBrew notes its meta.xml
/// gives. Never "infinity" alone, which Infinity Runner, Ace Combat
/// Infinity and Marvel Super Hero Squad: The Infinity Gauntlet have too
/// (GameTDB and RPCS3's compatibility list, read 4 November 2026).
const NAMES: [&str; 2] = ["disney infinity", "ディズニーインフィニティ"];

/// The release a name says. Words are read whatever stands between them, as
/// in a file named Disney_Infinity_3.0. The number follows the name, "3.0"
/// or "[2.0]" in the games' own names, and 2.0 also goes by Marvel Super
/// Heroes, as RPCS3's list names BLES02064. That is asked only once the name
/// says Disney Infinity, since LEGO Marvel Super Heroes has those words too.
fn from_title(title: &str) -> Option<InfinityGame> {
    let lower = title.to_lowercase();
    let text = lower
        .split(|c: char| !c.is_alphanumeric() && c != '.')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let end = NAMES.iter().find_map(|name| text.find(name).map(|at| at + name.len()))?;
    let next = text[end..].split(' ').find(|word| !word.is_empty()).unwrap_or("");
    Some(if text.contains("3.0") || next == "3" {
        InfinityGame::Three
    } else if text.contains("2.0") || next == "2" || text.contains("marvel super heroes") {
        InfinityGame::Two
    } else {
        InfinityGame::One
    })
}

/// The game's English name, for a Wii U disc image whose name, taken from
/// its file's, doesn't say which game it is: one named "Disney Infinity
/// [2.0] (Europe)" loses its "[2.0]" with the rest in brackets
/// (backends/cemu/mod.rs, `title_from_file`), and would be taken for 1.0.
/// `None` for any other disc, and for one whose name already says it.
pub fn disc_english_name(product_code: &str, title: &str) -> Option<&'static str> {
    let (_, game) = WII_U_DISCS.iter().find(|(code, _)| code.eq_ignore_ascii_case(product_code))?;
    (from_title(title) != Some(*game)).then(|| game.name())
}

/// What a figure is, which decides the places it goes in. The emulators'
/// lists number each kind from its own million: characters from 1000001,
/// play sets from 2000001, round power discs from 3000001 and hexagonal
/// ones from 4000001 (Cemu's `s_listFigures`, nsyshid/Infinity.cpp lines 22
/// to 143, 144 to 162, 163 to 220 and 221 to 343, v2.6-2, read 4 November
/// 2026).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Piece {
    Character,
    /// A play set piece, and the Toy Box games numbered among them, 2.0's
    /// Toy Box Games and 3.0's Toy Box Expansion Games, which the emulators
    /// put on the hexagon's first place only, as play sets.
    PlaySet,
    /// A round power disc, which goes under a character.
    RoundDisc,
    /// A hexagonal power disc, which goes on the hexagon.
    HexDisc,
}

impl Piece {
    /// The kind of piece a number is, by the bounds the emulators' figure
    /// makers sort their lists by (see `fits`). `None` for a number outside
    /// them, which no list has.
    pub fn of_number(number: u32) -> Option<Piece> {
        match number {
            1..=1_999_998 => Some(Self::Character),
            2_000_001..=2_999_998 => Some(Self::PlaySet),
            3_000_001..=3_999_998 => Some(Self::RoundDisc),
            4_000_001..=4_999_998 => Some(Self::HexDisc),
            _ => None,
        }
    }

    /// The kind as the menu's page reads it.
    pub fn kind(self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::PlaySet => "play_set",
            Self::RoundDisc => "round_disc",
            Self::HexDisc => "hex_disc",
        }
    }
}

/// How many places the base has: three on the hexagon and three on each
/// player's pad, a character and two round discs under it, as all three
/// emulators keep it (Cemu's nsyshid/Infinity.h line 84, v2.6-2; RPCS3's
/// rpcs3/Emu/Io/Infinity.h line 35, master b2e52d6; Dolphin's
/// Core/IOS/USB/Emulated/Infinity.h line 111, 2609a; read 4 November 2026).
/// RPCS3 gave the hexagon three in its pull request 15583 (21 May 2024),
/// since a real one holds three discs.
pub const PLACES: usize = 9;

/// A place's name, as all three emulators' base windows name it (Cemu's
/// gui/EmulatedUSBDevices/EmulatedUSBDeviceFrame.cpp lines 110 to 118,
/// v2.6-2; RPCS3's rpcs3qt/infinity_dialog.cpp lines 736 to 752, master
/// b2e52d6; Dolphin's DolphinQt/InfinityBase/InfinityBaseWindow.cpp lines
/// 83 to 99, 2609a; read 4 November 2026). Empty for a place the base
/// doesn't have.
pub fn place_name(place: usize) -> &'static str {
    match place {
        0 => "Play Set/Power Disc",
        1 => "Power Disc Two",
        2 => "Power Disc Three",
        3 => "Player One",
        4 => "Player One Ability One",
        5 => "Player One Ability Two",
        6 => "Player Two",
        7 => "Player Two Ability One",
        8 => "Player Two Ability Two",
        _ => "",
    }
}

/// Whether a kind of piece goes in a place, as all three emulators' figure
/// makers offer them: the hexagon's first place a play set or a hexagonal
/// disc, its other two hexagonal discs, each player's first place a
/// character and the two under it round discs (Cemu's
/// EmulatedUSBDeviceFrame.cpp lines 526 to 531, v2.6-2; RPCS3's
/// infinity_dialog.cpp lines 456 to 461, master b2e52d6; Dolphin's
/// InfinityBaseWindow.cpp lines 211 to 219, 2609a; read 4 November 2026).
/// None of them checks a file put on a place, so the menu has to.
pub fn fits(place: usize, piece: Piece) -> bool {
    matches!(
        (place, piece),
        (0, Piece::PlaySet | Piece::HexDisc)
            | (1 | 2, Piece::HexDisc)
            | (3 | 6, Piece::Character)
            | (4 | 5 | 7 | 8, Piece::RoundDisc)
    )
}

/// Whether a game takes a piece of a series, from the Disney Infinity
/// wiki's "Compatibility" page (revision 127460, 12 January 2020, read 4
/// November 2026): characters and power discs work in the game they came
/// out with and in every later one, an older character in the Toy Box only,
/// while a play set piece, a Toy Box game's among them, opens its own
/// game's content and works there alone. No game takes a later one's.
pub fn takes(game: InfinityGame, piece: Piece, series: u8) -> bool {
    match piece {
        Piece::PlaySet => series == game.series(),
        Piece::Character | Piece::RoundDisc | Piece::HexDisc => (1..=game.series()).contains(&series),
    }
}

/// The places a figure can go in for a game: where its kind fits, if the
/// game takes it. A figure whose list gives no series is placed by its kind
/// alone: Dolphin's list has none, and every figure on it is 1.0's
/// (Core/IOS/USB/Emulated/Infinity.cpp line 24, 2609a, read 4 November
/// 2026), which every game takes but for its play sets.
pub fn places_for(game: InfinityGame, figure: &ToyFigure) -> Vec<usize> {
    let Some(piece) = Piece::of_number(figure.number) else {
        return Vec::new();
    };
    if figure.series.is_some_and(|series| !takes(game, piece, series)) {
        return Vec::new();
    }
    (0..PLACES).filter(|&place| fits(place, piece)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use InfinityGame::{One, Three, Two};

    fn figure(name: &str, number: u32, series: u8) -> ToyFigure {
        ToyFigure { name: name.to_string(), number, series: Some(series) }
    }

    #[test]
    fn every_release_is_known_by_its_id_whatever_its_name() {
        for (console, ids) in [(Console::WiiU, &WII_U[..]), (Console::Ps3, &PS3[..]), (Console::Wii, &WII[..])] {
            for &(id, game) in ids {
                assert_eq!(InfinityGame::of(console, id, ""), Some(game), "{id}");
                assert_eq!(InfinityGame::of(console, &id.to_lowercase(), "Game"), Some(game), "{id}");
            }
        }
        assert_eq!(WII_U.len() + PS3.len() + WII.len(), 31);
        // Each table has each id once.
        for ids in [&WII_U[..], &PS3[..], &WII[..], &WII_U_DISCS[..]] {
            for (at, (id, _)) in ids.iter().enumerate() {
                assert!(ids[at + 1..].iter().all(|(other, _)| other != id), "{id}");
            }
        }
    }

    #[test]
    fn bertrams_scandinavian_ps3_disc_is_1_0() {
        assert_eq!(InfinityGame::of(Console::Ps3, "BLES01844", "Disney Infinity"), Some(One));
        assert_eq!(InfinityGame::of(Console::Ps3, "BLES01844", ""), Some(One));
    }

    #[test]
    fn the_japanese_wii_u_3_0_is_known_by_its_id() {
        assert_eq!(InfinityGame::of(Console::WiiU, "00050000101DDC00", "ディズニーインフィニティ3.0"), Some(Three));
        assert_eq!(InfinityGame::of(Console::WiiU, "00050000101ddc00", "何か"), Some(Three));
    }

    #[test]
    fn an_id_is_read_on_its_own_console_only() {
        assert_eq!(InfinityGame::of(Console::Ps3, "SQIE4Q", ""), None);
        assert_eq!(InfinityGame::of(Console::Wii, "BLES01844", ""), None);
        assert_eq!(InfinityGame::of(Console::GameCube, "SQIE4Q", ""), None);
        assert_eq!(InfinityGame::of(Console::Ps3, "0005000010132900", ""), None);
    }

    #[test]
    fn every_release_is_known_by_the_name_it_gives() {
        let named = |title| InfinityGame::of(Console::WiiU, "WUD0123456789ABC", title);
        // Wii U meta.xml names in WiiUBrew.
        assert_eq!(named("Disney INFINITY"), Some(One));
        assert_eq!(named("体験版Disney INFINITY"), Some(One));
        assert_eq!(named("Disney Infinity [2.0]"), Some(Two));
        assert_eq!(named("DISNEY INFINITY 3.0 EDITION"), Some(Three));
        assert_eq!(named("DISNEY INFINITY 3.0: PLAY WITHOUT LIMITS"), Some(Three));
        assert_eq!(named("ディズニーインフィニティ3.0"), Some(Three));
        // PS3 PARAM.SFO titles, and the Wii's name in GameTDB.
        let ps3 = |title| InfinityGame::of(Console::Ps3, "BLUS99999", title);
        assert_eq!(ps3("Disney Infinity"), Some(One));
        assert_eq!(ps3("Disney Infinity [2.0]"), Some(Two));
        assert_eq!(ps3("Disney Infinity 3.0"), Some(Three));
        assert_eq!(InfinityGame::of(Console::Wii, "SQIX4Q", "Disney Infinity"), Some(One));
        // Names in GameTDB and RPCS3's list, and the way files get named.
        assert_eq!(named("Disney Infinity: Marvel Super Heroes"), Some(Two));
        assert_eq!(named("Disney Infinity 2.0 Edition"), Some(Two));
        assert_eq!(named("Disney Infinity 2.0: Play Without Limits"), Some(Two));
        assert_eq!(named("Disney Infinity 2.0: Marvel Super Heroes"), Some(Two));
        assert_eq!(named("Disney_Infinity_3.0"), Some(Three));
        assert_eq!(named("Disney Infinity 3"), Some(Three));
        assert_eq!(named("disney infinity 2"), Some(Two));
        assert_eq!(named("Disney Infinity 1.0"), Some(One));
    }

    #[test]
    fn other_games_with_infinity_in_their_name_are_not_disney_infinity() {
        for title in [
            "Infinity Runner",
            "Ace Combat Infinity",
            "Marvel Super Hero Squad: The Infinity Gauntlet",
            "LEGO Marvel Super Heroes",
            "Big Sky: Infinity",
            "Space Invaders Infinity Gene",
            "Code Lyoko: Quest for Infinity",
            "Skylanders Trap Team",
            "",
        ] {
            for console in Console::ALL {
                assert_eq!(InfinityGame::of(console, "BLES01234", title), None, "{title}");
            }
        }
    }

    #[test]
    fn a_disc_image_is_named_by_its_product_code_when_its_file_doesnt_say_the_game() {
        // A 2.0 file loses its "[2.0]" with the rest in brackets.
        assert_eq!(disc_english_name("ADRP", "Disney Infinity"), Some("Disney Infinity 2.0: Marvel Super Heroes"));
        assert_eq!(disc_english_name("bd3j", "game"), Some("Disney Infinity 3.0"));
        assert_eq!(disc_english_name("ADSE", "Wii U disc"), Some("Disney Infinity"));
        // One that says it is kept, and so is every other disc.
        assert_eq!(disc_english_name("ADSE", "Disney Infinity"), None);
        assert_eq!(disc_english_name("BD3J", "ディズニーインフィニティ3.0"), None);
        assert_eq!(disc_english_name("BD3E", "Disney Infinity 3.0 Edition"), None);
        assert_eq!(disc_english_name("BK7P", "Skylanders_Trap_Team"), None);
        assert_eq!(disc_english_name("ARKE", "game"), None);
        for (code, game) in WII_U_DISCS {
            let name = disc_english_name(code, "game").unwrap();
            assert_eq!(InfinityGame::of(Console::WiiU, "WUD0123456789ABC", name), Some(game), "{code}");
            assert!(code.len() == 4 && code.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()), "{code}");
        }
    }

    #[test]
    fn games_are_written_the_way_the_menu_reads_them() {
        assert_eq!([One, Two, Three].map(|game| serde_json::to_value(game).unwrap()), ["one", "two", "three"]);
        assert_eq!([One, Two, Three].map(InfinityGame::series), [1, 2, 3]);
    }

    #[test]
    fn a_number_tells_the_kind_of_piece() {
        assert_eq!(Piece::of_number(1_000_001), Some(Piece::Character)); // Mr. Incredible
        assert_eq!(Piece::of_number(1_000_150), Some(Piece::Character)); // Sam Flynn
        assert_eq!(Piece::of_number(2_000_001), Some(Piece::PlaySet)); // Starter Play Set
        assert_eq!(Piece::of_number(2_000_103), Some(Piece::PlaySet)); // Assault on Asgard
        assert_eq!(Piece::of_number(2_000_300), Some(Piece::PlaySet)); // Finding Dory Play Set
        assert_eq!(Piece::of_number(3_000_003), Some(Piece::RoundDisc)); // Bolt's Super Strength
        assert_eq!(Piece::of_number(3_000_231), Some(Piece::RoundDisc));
        assert_eq!(Piece::of_number(4_000_018), Some(Piece::HexDisc)); // Mickey's Car
        assert_eq!(Piece::of_number(4_000_229), Some(Piece::HexDisc)); // Quad Jumper
        for nothing in [0, 1_999_999, 2_000_000, 2_999_999, 3_000_000, 3_999_999, 4_000_000, 4_999_999, 5_000_000] {
            assert_eq!(Piece::of_number(nothing), None, "{nothing}");
        }
        for piece in [Piece::Character, Piece::PlaySet, Piece::RoundDisc, Piece::HexDisc] {
            assert_eq!(serde_json::to_value(piece).unwrap(), piece.kind());
        }
    }

    #[test]
    fn each_place_takes_the_kinds_the_emulators_offer_it() {
        use Piece::*;
        let kinds = [Character, PlaySet, RoundDisc, HexDisc];
        let offered: [&[Piece]; PLACES] = [
            &[PlaySet, HexDisc],
            &[HexDisc],
            &[HexDisc],
            &[Character],
            &[RoundDisc],
            &[RoundDisc],
            &[Character],
            &[RoundDisc],
            &[RoundDisc],
        ];
        for (place, fitting) in offered.iter().enumerate() {
            for kind in kinds {
                assert_eq!(fits(place, kind), fitting.contains(&kind), "{place} {kind:?}");
            }
            assert!(!place_name(place).is_empty());
        }
        assert!(kinds.iter().all(|&kind| !fits(PLACES, kind)));
        assert_eq!(place_name(0), "Play Set/Power Disc");
        assert_eq!(place_name(3), "Player One");
        assert_eq!(place_name(8), "Player Two Ability Two");
        assert_eq!(place_name(PLACES), "");
    }

    #[test]
    fn a_game_takes_its_own_pieces_and_older_characters_and_discs() {
        use Piece::*;
        assert!(takes(Three, Character, 1), "a 1.0 character, in the Toy Box");
        assert!(takes(Three, Character, 2));
        assert!(takes(Two, RoundDisc, 1));
        assert!(takes(Three, HexDisc, 1));
        assert!(takes(Three, PlaySet, 3));
        assert!(takes(One, PlaySet, 1));
        assert!(!takes(Three, PlaySet, 2), "a 2.0 play set, or 2.0's Toy Box Games");
        assert!(!takes(Two, PlaySet, 1), "a 1.0 play set");
        assert!(!takes(One, Character, 3), "a later game's character");
        assert!(!takes(One, RoundDisc, 2));
        assert!(!takes(Two, HexDisc, 3));
        for game in [One, Two, Three] {
            for kind in [Character, PlaySet, RoundDisc, HexDisc] {
                assert!(!takes(game, kind, 0) && !takes(game, kind, 4), "{game:?} {kind:?}");
            }
        }
    }

    #[test]
    fn a_figure_is_offered_where_it_fits_in_a_game_that_takes_it() {
        assert_eq!(places_for(Three, &figure("Mr. Incredible", 1_000_001, 1)), [3, 6]);
        assert_eq!(places_for(One, &figure("Sam Flynn", 1_000_150, 3)), [] as [usize; 0]);
        assert_eq!(places_for(Two, &figure("Marvel's The Avengers Play Set", 2_000_100, 2)), [0]);
        assert_eq!(places_for(Three, &figure("Marvel's The Avengers Play Set", 2_000_100, 2)), [] as [usize; 0]);
        assert_eq!(places_for(Three, &figure("Assault on Asgard", 2_000_103, 2)), [] as [usize; 0]);
        assert_eq!(places_for(Three, &figure("Inside Out Play Set", 2_000_200, 3)), [0]);
        assert_eq!(places_for(Two, &figure("Bolt's Super Strength", 3_000_003, 1)), [4, 5, 7, 8]);
        assert_eq!(places_for(One, &figure("Sorcerer Supreme", 3_000_173, 2)), [] as [usize; 0]);
        assert_eq!(places_for(Three, &figure("Mickey's Car", 4_000_018, 1)), [0, 1, 2]);
        assert_eq!(places_for(Two, &figure("View from the Suit", 4_000_117, 2)), [0, 1, 2]);
        // Dolphin's list gives no series.
        let unlisted = ToyFigure { name: "The Incredibles - Mr. Incredible".into(), number: 1_000_001, series: None };
        assert_eq!(places_for(One, &unlisted), [3, 6]);
        assert_eq!(places_for(One, &figure("Nothing", 0, 1)), [] as [usize; 0]);
    }
}
