//! LEGO Dimensions: how a game is known as it, the Toy Pad's seven places
//! and the pad each is on, what kind of tag each id is, and the lights the
//! game sets on the three pads as Omoio's Cemu reports them.
//!
//! None of it comes from a tag file. A minifigure's id sits encrypted in its
//! tag, with a key made from the tag's own UID (Cemu's `GenerateFigureKey`
//! and `GetFigureId`, src/Cafe/OS/libs/nsyshid/Dimensions.cpp lines 937 to
//! 1017, Bertrram/Cemu v2.6-2, read 4 November 2026), and Omoio reads no id
//! out of a tag, a vehicle's included. It knows a tag it had the emulator
//! make from its own record of the id it asked for, and one the player
//! brought only by what the emulator reports once it is on the Toy Pad.

use crate::core::catalogue::fold;
use crate::core::console::Console;
use serde::{Deserialize, Serialize};

/// Both Wii U releases by their title ids, the USA's and Europe's, from
/// WiiUBrew's title database (revision 4868, 24 October 2026) and the
/// infobox of Cemu's wiki page for the game (revision 19675), read 4
/// November 2026. Neither lists a release anywhere else.
const WII_U: [&str; 2] = ["0005000010194200", "0005000010195D00"];

/// Both Wii U discs by their product codes, the four letters after "WUP-P-"
/// at the start of their images, from WiiUBrew's title database and
/// GameTDB's ids APZEWR and APZPWR, read 4 November 2026.
const WII_U_DISCS: [&str; 2] = ["APZE", "APZP"];

/// Both PS3 discs by their serials, Europe's and the USA's, from GameTDB
/// and RPCS3's compatibility list, both read 4 November 2026. Neither
/// lists one from the PlayStation Store.
const PS3: [&str; 2] = ["BLES02105", "BLUS31473"];

/// What every release's name says once it is folded (`catalogue::fold`):
/// "LEGO® Dimensions™" and "LEGO® DIMENSIONS™" in WiiUBrew, and "LEGO®
/// DIMENSIONS™" in the PS3 updates' names on Sony's update server, whose
/// marks fold away (read 4 November 2026).
const NAME: &str = "lego dimensions";

/// The name a disc image goes by when its file's name doesn't say the game.
const ENGLISH_NAME: &str = "LEGO Dimensions";

/// Whether a game is LEGO Dimensions: by its id first, on its own console,
/// then by its name, which is all a Wii U disc image has, since its
/// library id is made from its file's name (backends/cemu/mod.rs,
/// `disc_id`). The name is read word by word, so "LEGO_Dimensions" is it
/// and a longer word with those letters in it isn't.
pub fn is_dimensions(console: Console, title_id: &str, title: &str) -> bool {
    let ids: &[&str] = match console {
        Console::Ps3 => &PS3,
        Console::WiiU => &WII_U,
        Console::Wii | Console::GameCube => &[],
    };
    ids.iter().any(|id| id.eq_ignore_ascii_case(title_id)) || says_dimensions(title)
}

fn says_dimensions(title: &str) -> bool {
    format!(" {} ", fold(title)).contains(&format!(" {NAME} "))
}

/// The game's English name, for a Wii U disc image whose product code says
/// it is LEGO Dimensions and whose name, taken from its file's, doesn't.
/// `None` for any other disc, and for one whose name already says it.
pub fn disc_english_name(product_code: &str, title: &str) -> Option<&'static str> {
    let ours = WII_U_DISCS.iter().any(|code| code.eq_ignore_ascii_case(product_code));
    (ours && !says_dimensions(title)).then_some(ENGLISH_NAME)
}

/// How many places the Toy Pad has: three on each side pad and one on the
/// centre pad, as both emulators keep it (Cemu's nsyshid/Dimensions.h line
/// 84, v2.6-2; RPCS3's rpcs3/Emu/Io/Dimensions.h line 9, master b2e52d6;
/// read 4 November 2026).
pub const PLACES: usize = 7;

/// The pad a place is on, as the game numbers the pads: 1 the centre, 2
/// the left and 3 the right. Places 0, 3 and 4 are on the left pad, 1 is on
/// the centre one, and 2, 5 and 6 are on the right, as both emulators'
/// windows lay them out and tell the game (Cemu's
/// gui/EmulatedUSBDevices/EmulatedUSBDeviceFrame.cpp lines 151 to 161,
/// v2.6-2; RPCS3's rpcs3qt/dimensions_dialog.cpp lines 551 to 553 and 585
/// to 595, master b2e52d6; read 4 November 2026). 0 for a place the Toy Pad
/// doesn't have.
pub fn pad_of(place: usize) -> u8 {
    match place {
        1 => 1,
        0 | 3 | 4 => 2,
        2 | 5 | 6 => 3,
        _ => 0,
    }
}

/// A place's name, in the order the places sit on their pad, first the one
/// the emulators' windows put in the top row. Empty for a place the Toy Pad
/// doesn't have.
pub fn place_name(place: usize) -> &'static str {
    match place {
        0 => "Left pad, first place",
        1 => "Centre pad",
        2 => "Right pad, first place",
        3 => "Left pad, second place",
        4 => "Left pad, third place",
        5 => "Right pad, second place",
        6 => "Right pad, third place",
        _ => "",
    }
}

/// What a tag is, by its id. Both emulators number their minifigures from
/// 0, the Blank Tag, to 770, and their vehicles and gadgets from 1000 to
/// 1265 (Cemu's `s_listMinis` and `s_listTokens`, nsyshid/Dimensions.cpp
/// lines 25 to 112 and 114 to 380, v2.6-2; RPCS3's `list_minifigs` and
/// `list_tokens`, rpcs3qt/dimensions_dialog.cpp lines 23 to 110 and 112 to
/// 378, master b2e52d6, which hold the same entries; read 4 November 2026).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Piece {
    Minifigure,
    /// A vehicle or a gadget. Each has three builds, numbered one after the
    /// other (1006 Batmobile, 1007 Batblaster, 1008 Sonic Batray in both
    /// lists).
    Vehicle,
    /// A tag with nothing on it yet, which the game writes a vehicle or a
    /// gadget to when one is built with it on the centre pad
    /// (Berny23/LD-ToyPad-Emulator's index.js line 279, commit 32ccf72, and
    /// players' reports). Both emulators' figure makers make it as id 0
    /// (Cemu's `CreateFigure`, Dimensions.cpp lines 707 to 749; RPCS3's
    /// `create_blank_character`, rpcs3/Emu/Io/Dimensions.cpp lines 496 to
    /// 529), and Cemu's pull request that brought the Toy Pad (1371, 11
    /// November 2024) calls it the one to store gadgets from the game; all
    /// read 4 November 2026.
    BlankTag,
}

impl Piece {
    /// The kind of tag an id is, as the emulators' lists are numbered.
    pub fn of_id(id: u32) -> Piece {
        match id {
            0 => Self::BlankTag,
            1..=999 => Self::Minifigure,
            _ => Self::Vehicle,
        }
    }

    /// The kind as the menu's page reads it.
    pub fn kind(self) -> &'static str {
        match self {
            Self::Minifigure => "minifigure",
            Self::Vehicle => "vehicle",
            Self::BlankTag => "blank_tag",
        }
    }
}

/// The light the game last asked of one of the three pads, as Omoio's Cemu
/// keeps it and reports it in `toypad status` (the portal server's Toy Pad
/// commands, branch infinity-toypad), without changing what it answers the
/// game. RPCS3 keeps none (rpcs3/Emu/Io/Dimensions.cpp lines 647 to 659,
/// master b2e52d6). The page draws it, `mode` saying how, from the light
/// commands as Ellerbach/LegoDimensions' LegoDimensionsProtocol.md lines
/// 125 to 271 (commit 58d1f12) and AlinaNova21/node-ld's src/lib/ToyPad.js
/// lines 120 to 165 (commit f54b177) describe them, read 4 November 2026:
///
/// - "none": the game hasn't lit the pad.
/// - "solid": `color`, at once.
/// - "fade": from `from` to `color` and back, `count` steps of `time`
///   ticks each, ending on `color` after an odd count and on `from` after
///   an even one. A count of 0 never ends.
/// - "flash": `color` for `time` ticks, then `off` ticks without it,
///   `count` times; 255 never ends. The sources don't settle what the pad
///   shows between flashes, or after the last.
/// - "random": a fade through colours the pad picks itself, which the game
///   never says, `count` steps of `time` ticks, 0 never ending.
///
/// Neither source says how long a tick is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PadLight {
    /// "none", "solid", "fade", "flash" or "random".
    pub mode: String,
    pub color: [u8; 3],
    /// What the pad showed when the game asked for this light.
    pub from: [u8; 3],
    pub time: u8,
    pub off: u8,
    pub count: u8,
    /// Goes up by one each time the game changes the pad's light, so an
    /// animation starts again only for a light the game asked anew.
    pub changes: u32,
}

const MODES: [&str; 5] = ["none", "solid", "fade", "flash", "random"];

/// A whole number written in digits alone, as the server writes them.
fn number(word: &str) -> Option<u32> {
    if word.is_empty() || !word.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    word.parse().ok()
}

/// One of `toypad status`'s light lines, "light<P> <mode> <r> <g> <b>
/// <fromR> <fromG> <fromB> <time> <off> <count> <changes>", as the pad it
/// is for, 1 the centre, 2 the left and 3 the right, and its light. `None`
/// for any other line.
pub fn read_light(line: &str) -> Option<(u8, PadLight)> {
    let mut words = line.split(' ');
    let pad = match words.next()?.strip_prefix("light")? {
        "1" => 1,
        "2" => 2,
        "3" => 3,
        _ => return None,
    };
    let mode = words.next().filter(|mode| MODES.contains(mode))?;
    let numbers = words.map(number).collect::<Option<Vec<u32>>>()?;
    let [r, g, b, from_r, from_g, from_b, time, off, count, changes] = numbers[..] else {
        return None;
    };
    let byte = |n: u32| u8::try_from(n).ok();
    let light = PadLight {
        mode: mode.to_string(),
        color: [byte(r)?, byte(g)?, byte(b)?],
        from: [byte(from_r)?, byte(from_g)?, byte(from_b)?],
        time: byte(time)?,
        off: byte(off)?,
        count: byte(count)?,
        changes,
    };
    Some((pad, light))
}

/// The three pads' lights in a `toypad status` answer, the centre's, the
/// left's and the right's, from its lines that start with "light". `None`
/// unless it has each pad's light once and every one reads.
pub fn read_lights<S: AsRef<str>>(lines: &[S]) -> Option<[PadLight; 3]> {
    let mut lights: [Option<PadLight>; 3] = Default::default();
    for line in lines.iter().map(AsRef::as_ref).filter(|line| line.starts_with("light")) {
        let (pad, light) = read_light(line)?;
        let kept = &mut lights[usize::from(pad - 1)];
        if kept.is_some() {
            return None;
        }
        *kept = Some(light);
    }
    let [centre, left, right] = lights;
    Some([centre?, left?, right?])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light(mode: &str, color: [u8; 3], from: [u8; 3], time: u8, off: u8, count: u8, changes: u32) -> PadLight {
        PadLight { mode: mode.to_string(), color, from, time, off, count, changes }
    }

    #[test]
    fn every_release_is_known_by_its_id_whatever_its_name() {
        for (console, ids) in [(Console::WiiU, &WII_U[..]), (Console::Ps3, &PS3[..])] {
            for id in ids {
                assert!(is_dimensions(console, id, ""), "{id}");
                assert!(is_dimensions(console, &id.to_lowercase(), "Game"), "{id}");
            }
        }
        assert!(is_dimensions(Console::WiiU, "0005000010195d00", "何か"));
    }

    #[test]
    fn an_id_is_read_on_its_own_console_only() {
        assert!(!is_dimensions(Console::WiiU, "BLES02105", ""));
        assert!(!is_dimensions(Console::Ps3, "0005000010194200", ""));
        for console in [Console::Wii, Console::GameCube] {
            for id in WII_U.iter().chain(&PS3) {
                assert!(!is_dimensions(console, id, ""), "{id}");
            }
        }
        // A disc's product code is never a library id.
        assert!(!is_dimensions(Console::WiiU, "APZE", ""));
    }

    #[test]
    fn every_release_is_known_by_the_name_it_gives() {
        for title in [
            "LEGO® DIMENSIONS™",
            "LEGO® Dimensions™",
            "LEGO Dimensions",
            "LEGO DIMENSIONS",
            "Lego Dimensions",
            "LEGO_Dimensions",
            "LEGO®DIMENSIONS™",
            "LEGO Dimensions (Europe) (En,Fr,De,Nl,Da)",
        ] {
            for console in Console::ALL {
                assert!(is_dimensions(console, "WUD0123456789ABC", title), "{title}");
            }
        }
    }

    #[test]
    fn other_lego_games_and_dimensions_are_not_lego_dimensions() {
        for title in [
            "LEGO® Marvel Super Heroes",
            "LEGO Batman 3: Beyond Gotham",
            "LEGO Star Wars: The Force Awakens",
            "LEGO Worlds",
            "Hyperdimension Neptunia",
            "Dimensions",
            "LEGO",
            "Megalego Dimensions",
            "LEGO Dimensionsx",
            "Disney Infinity",
            "Skylanders Imaginators",
            "",
        ] {
            for console in Console::ALL {
                assert!(!is_dimensions(console, "BLES01234", title), "{title}");
            }
        }
    }

    #[test]
    fn a_disc_image_is_named_by_its_product_code_when_its_file_doesnt_say_the_game() {
        assert_eq!(disc_english_name("APZE", "Wii U disc"), Some("LEGO Dimensions"));
        assert_eq!(disc_english_name("apzp", "game"), Some("LEGO Dimensions"));
        assert_eq!(disc_english_name("APZP", "LEGO® DIMENSIONS™"), None);
        assert_eq!(disc_english_name("APZE", "LEGO_Dimensions"), None);
        assert_eq!(disc_english_name("BK7P", "game"), None);
        assert_eq!(disc_english_name("ADSE", "game"), None);
        for code in WII_U_DISCS {
            let name = disc_english_name(code, "game").unwrap();
            assert!(is_dimensions(Console::WiiU, "WUD0123456789ABC", name), "{code}");
        }
    }

    #[test]
    fn each_place_is_on_its_pad() {
        let pads: [u8; PLACES] = [2, 1, 3, 2, 2, 3, 3];
        for (place, pad) in pads.into_iter().enumerate() {
            assert_eq!(pad_of(place), pad, "{place}");
        }
        assert_eq!(pad_of(PLACES), 0);
        assert_eq!(pad_of(usize::MAX), 0);
        // Three places on each side pad and one on the centre.
        for (pad, places) in [(1, 1), (2, 3), (3, 3)] {
            assert_eq!((0..PLACES).filter(|&place| pad_of(place) == pad).count(), places, "{pad}");
        }
    }

    #[test]
    fn each_place_has_a_name_of_its_own() {
        let names: Vec<&str> = (0..PLACES).map(place_name).collect();
        for (place, name) in names.iter().enumerate() {
            assert!(!name.is_empty(), "{place}");
            assert!(names[place + 1..].iter().all(|other| other != name), "{name}");
            let pad = ["", "Centre pad", "Left pad", "Right pad"][usize::from(pad_of(place))];
            assert!(name.starts_with(pad), "{name}");
            assert!(!name.contains('!') && !name.contains('\u{2014}'), "{name}");
        }
        assert_eq!(place_name(0), "Left pad, first place");
        assert_eq!(place_name(1), "Centre pad");
        assert_eq!(place_name(6), "Right pad, third place");
        assert_eq!(place_name(PLACES), "");
    }

    #[test]
    fn an_id_tells_the_kind_of_tag() {
        assert_eq!(Piece::of_id(0), Piece::BlankTag);
        for minifigure in [1, 2, 3, 76, 83, 768, 769, 770, 999] {
            assert_eq!(Piece::of_id(minifigure), Piece::Minifigure, "{minifigure}");
        }
        for vehicle in [1000, 1006, 1007, 1008, 1265, 0xFFFF, u32::MAX] {
            assert_eq!(Piece::of_id(vehicle), Piece::Vehicle, "{vehicle}");
        }
        assert_eq!(Piece::Minifigure.kind(), "minifigure");
        assert_eq!(Piece::Vehicle.kind(), "vehicle");
        assert_eq!(Piece::BlankTag.kind(), "blank_tag");
        for piece in [Piece::Minifigure, Piece::Vehicle, Piece::BlankTag] {
            assert_eq!(serde_json::to_value(piece).unwrap(), piece.kind());
        }
    }

    #[test]
    fn a_light_line_reads_as_the_server_writes_it() {
        assert_eq!(read_light("light1 none 0 0 0 0 0 0 0 0 0 0"), Some((1, light("none", [0; 3], [0; 3], 0, 0, 0, 0))));
        assert_eq!(
            read_light("light3 solid 255 0 0 0 0 0 0 0 0 1"),
            Some((3, light("solid", [255, 0, 0], [0; 3], 0, 0, 0, 1)))
        );
        assert_eq!(
            read_light("light2 fade 0 255 0 255 0 0 30 0 0 7"),
            Some((2, light("fade", [0, 255, 0], [255, 0, 0], 30, 0, 0, 7)))
        );
        assert_eq!(
            read_light("light1 flash 255 255 255 0 0 255 5 10 255 12"),
            Some((1, light("flash", [255; 3], [0, 0, 255], 5, 10, 255, 12)))
        );
        assert_eq!(
            read_light("light2 random 0 0 0 16 32 48 20 0 3 4294967295"),
            Some((2, light("random", [0; 3], [16, 32, 48], 20, 0, 3, u32::MAX)))
        );
    }

    #[test]
    fn a_line_that_isnt_a_light_line_reads_as_none() {
        for line in [
            "",
            "ok",
            "place0 empty",
            "place1 loaded 3",
            "light0 solid 255 0 0 0 0 0 0 0 0 1",
            "light4 solid 255 0 0 0 0 0 0 0 0 1",
            "light solid 255 0 0 0 0 0 0 0 0 1",
            "light12 solid 255 0 0 0 0 0 0 0 0 1",
            "light1 glow 255 0 0 0 0 0 0 0 0 1",
            "light1 Solid 255 0 0 0 0 0 0 0 0 1",
            "light1 solid 255 0 0 0 0 0 0 0 0",
            "light1 solid 255 0 0 0 0 0 0 0 0 1 1",
            "light1 solid 256 0 0 0 0 0 0 0 0 1",
            "light1 solid -1 0 0 0 0 0 0 0 0 1",
            "light1 solid +5 0 0 0 0 0 0 0 0 1",
            "light1 solid 0x10 0 0 0 0 0 0 0 0 1",
            "light1 solid 255 0 0 0 0 0 0 0 0 4294967296",
            "light1  solid 255 0 0 0 0 0 0 0 0 1",
            "light1 solid 255 0 0 0 0 0 0 0 0 1 ",
            "light1",
        ] {
            assert_eq!(read_light(line), None, "{line:?}");
        }
    }

    #[test]
    fn a_status_answer_gives_the_centre_left_and_right_lights() {
        let answer = [
            "place0 loaded 1",
            "place1 empty",
            "place2 empty",
            "place3 loaded 0",
            "place4 empty",
            "place5 loaded 1006",
            "place6 empty",
            "light1 solid 255 255 255 0 0 0 0 0 0 2",
            "light2 fade 0 255 0 255 255 255 30 0 0 5",
            "light3 none 0 0 0 0 0 0 0 0 0 0",
            "ok",
        ];
        let [centre, left, right] = read_lights(&answer).unwrap();
        assert_eq!(centre, light("solid", [255; 3], [0; 3], 0, 0, 0, 2));
        assert_eq!(left, light("fade", [0, 255, 0], [255; 3], 30, 0, 0, 5));
        assert_eq!(right.mode, "none");

        // Each pad's line is known by its number, wherever it stands.
        let turned = [
            "light3 solid 0 0 255 0 0 0 0 0 0 1",
            "light1 none 0 0 0 0 0 0 0 0 0 0",
            "light2 solid 255 0 0 0 0 0 0 0 0 1",
            "ok",
        ]
        .map(String::from);
        let [centre, left, right] = read_lights(&turned).unwrap();
        assert_eq!((centre.mode.as_str(), left.color, right.color), ("none", [255, 0, 0], [0, 0, 255]));
    }

    #[test]
    fn a_status_answer_without_each_pads_light_once_gives_none() {
        let one = "light1 none 0 0 0 0 0 0 0 0 0 0";
        let two = "light2 none 0 0 0 0 0 0 0 0 0 0";
        let three = "light3 none 0 0 0 0 0 0 0 0 0 0";
        assert!(read_lights(&[one, two, three, "ok"]).is_some());
        assert_eq!(read_lights(&[one, two, "ok"]), None, "a pad missing");
        assert_eq!(read_lights(&[one, two, three, two, "ok"]), None, "a pad twice");
        assert_eq!(read_lights(&[one, two, "light3 glow 0 0 0 0 0 0 0 0 0 0", "ok"]), None);
        // What an Omoio's Cemu that keeps no lights would answer.
        assert_eq!(read_lights(&["place0 empty", "place1 empty", "ok"]), None);
        assert_eq!(read_lights::<&str>(&[]), None);
    }

    #[test]
    fn a_light_is_written_the_way_the_page_reads_it() {
        let solid = light("solid", [255, 0, 0], [0, 0, 0], 0, 0, 0, 1);
        let written = serde_json::to_value(&solid).unwrap();
        assert_eq!(
            written,
            serde_json::json!({
                "mode": "solid", "color": [255, 0, 0], "from": [0, 0, 0],
                "time": 0, "off": 0, "count": 0, "changes": 1
            })
        );
        assert_eq!(serde_json::from_value::<PadLight>(written).unwrap(), solid);
    }
}
