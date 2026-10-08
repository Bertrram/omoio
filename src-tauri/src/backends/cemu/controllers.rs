//! Cemu's side of the controller layout: one profile file per player.
//!
//! A fresh Cemu has no controller for anyone, so a game starts and nothing
//! answers. Cemu reads one file per player, `controllerProfiles/controller<N>.xml`
//! in its config folder (`InputManager::load` in Cemu v2.6), and Omoio writes
//! them from the layout it keeps (core/pad_layout.rs):
//!
//! - Player 1 is the Wii U GamePad, which most games expect to be there, but a
//!   Pro Controller in the few games that need one (`PRO_FIRST`).
//! - Players 2 to 4 are Wii U Pro Controllers, which multiplayer games take.
//!
//! An Xbox-style pad is an XInput one, named by its slot. Cemu's XInput pad is
//! known by the slot number alone, so a file can name a slot before a pad is
//! in it. Cemu reads XInput without the Guide button.
//!
//! A PlayStation pad or a Switch Pro Controller is one of Cemu's SDL pads,
//! named by the GUID SDL gives it (sdl.rs), which is worked out from the pad
//! itself, so it is named only while it is plugged in, and only in the Cemu
//! release sdl.rs was read against (release.rs). Omoio writes the files again
//! before every game, with the pads plugged in then. Any other pad gets none
//! in Cemu: its GUID would be a guess.
//!
//! Cemu reads these files once, as it starts (`CemuCommonInit` calls
//! `InputManager::load`, v2.6), so what Omoio writes during a game counts from
//! the next one. A pad that goes and comes back during a game is found again
//! by Cemu itself: SDL reports it added, and each player's SDL pad looks again
//! for a joystick of its GUID and number (`SDLControllerProvider::event_thread`,
//! `InputManager::on_device_changed`, `SDLController::connect`). That holds
//! for a pad back the way it went. Back over the other of cable and
//! Bluetooth, it has the same GUID only if Windows gives it the same version
//! there (sdl.rs), and otherwise plays again from the next game.
//!
//! Player 1 is the one every game answers, so they are never left without a
//! pad while an XInput one is plugged in (`stand_in`), and Play says so before
//! a game starts with nobody to answer it (`missing_first_player`).
//!
//! Cemu keeps one layout for every game. A game's own layout is RPCS3's alone.

use super::{release, sdl};
use crate::core::pad_layout::{self, Pad, Player, PLAYERS};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

/// Games whose first player has to be a Wii U Pro Controller rather than the
/// GamePad, by their title ids. Skylanders Trap Team asks for a language as
/// it starts, every time and not only the first (6 October 2026), and that
/// screen answers the GamePad's touch screen and a Pro Controller's buttons
/// but never the GamePad's buttons, so a pad standing in for the GamePad was
/// stuck on the flags. As a Pro Controller the same pad picked a flag and went
/// on through the logos, the save slots and into the opening film (European
/// disc, 5 October 2026; 000500001017c600 is the American one). Cemu turns
/// the stick into the presses menus move by for the GamePad but not for a
/// Pro Controller (`VPADController.cpp` against `WPADController::KPADRead`,
/// v2.6), so this Pro Controller takes its d-pad from the stick
/// (`dpad_from_stick`). Omoio shows only the TV picture, so nothing of the
/// GamePad's screen is lost.
pub const PRO_FIRST: [&str; 2] = ["0005000010181f00", "000500001017c600"];

/// Player 1's file as each kind of controller, kept beside the others so the
/// one a game needs can become `controller0.xml` as the game starts.
const FIRST_AS_GAMEPAD: &str = "Player 1 GamePad.xml";
const FIRST_AS_PRO: &str = "Player 1 Pro Controller.xml";

/// The Wii U's buttons with the place each sits, in the order of the
/// GamePad's `ButtonId`, which counts from 1. They sit where Nintendo's pads
/// have them, as Cemu's own XInput layout puts them: A on the right, B at the
/// bottom, X at the top, Y on the left. Wii U games are made for those places,
/// so jumping and attacking land on the bottom and left buttons of any pad.
/// Going by the letters instead put Swap Force's jump on Xbox B and its
/// attack on Xbox Y (13 September 2026). The price is that menus confirm with
/// the right-hand button, as they do on a Wii U.
pub const WII_U: [(&str, &str); 24] = [
    ("East", "A"),
    ("South", "B"),
    ("North", "X"),
    ("West", "Y"),
    ("LB", "L"),
    ("RB", "R"),
    ("LT", "ZL"),
    ("RT", "ZR"),
    ("Start", "Plus"),
    ("Back", "Minus"),
    ("Up", "D-pad up"),
    ("Down", "D-pad down"),
    ("Left", "D-pad left"),
    ("Right", "D-pad right"),
    ("LS", "Left stick press"),
    ("RS", "Right stick press"),
    ("LS Y+", "Left stick up"),
    ("LS Y-", "Left stick down"),
    ("LS X-", "Left stick left"),
    ("LS X+", "Left stick right"),
    ("RS Y+", "Right stick up"),
    ("RS Y-", "Right stick down"),
    ("RS X-", "Right stick left"),
    ("RS X+", "Right stick right"),
];

/// Cemu's number for an input on an SDL pad: the `SDL_GameControllerButton`
/// SDL reports it as (`SDLController::raw_state`), and for the sticks and
/// triggers the same `Buttons2` entries as XInput, where SDL counts down as
/// positive, so a stick pushed up is the negative entry (Cemu's own SDL layout
/// in `VPADController::set_default_mapping`). SDL 2.30 reports the face
/// buttons of a Switch pad by their letters (`RemapButton` in its Switch
/// driver, which Cemu leaves as it is), Nintendo's A sitting where an Xbox
/// pad has B, so `by_label` puts those by letter.
fn sdl_number(input: &str, by_label: bool) -> Option<u64> {
    Some(match (input, by_label) {
        ("South", false) | ("East", true) => 0,
        ("East", false) | ("South", true) => 1,
        ("West", false) | ("North", true) => 2,
        ("North", false) | ("West", true) => 3,
        ("Back", _) => 4,
        ("Guide", _) => 5,
        ("Start", _) => 6,
        ("LS", _) => 7,
        ("RS", _) => 8,
        ("LB", _) => 9,
        ("RB", _) => 10,
        ("Up", _) => 11,
        ("Down", _) => 12,
        ("Left", _) => 13,
        ("Right", _) => 14,
        ("LS X+", _) => 38,
        ("LS Y-", _) => 39,
        ("RS X+", _) => 40,
        ("RS Y-", _) => 41,
        ("LT", _) => 42,
        ("RT", _) => 43,
        ("LS X-", _) => 44,
        ("LS Y+", _) => 45,
        ("RS X-", _) => 46,
        ("RS Y+", _) => 47,
        _ => return None,
    })
}

/// Cemu's number for an input on an XInput pad, its `Buttons2` in
/// `Controller.h`. Buttons are the bits of `XINPUT_GAMEPAD.wButtons`; the
/// sticks and triggers come after Cemu's 32 buttons and its own trigger and
/// d-pad entries, positive directions first. Guide has none, since Cemu never
/// reads it.
fn xinput_number(input: &str) -> Option<u64> {
    Some(match input {
        "Up" => 0,
        "Down" => 1,
        "Left" => 2,
        "Right" => 3,
        "Start" => 4,
        "Back" => 5,
        "LS" => 6,
        "RS" => 7,
        "LB" => 8,
        "RB" => 9,
        "South" => 12,
        "East" => 13,
        "West" => 14,
        "North" => 15,
        "LS X+" => 38,
        "LS Y+" => 39,
        "RS X+" => 40,
        "RS Y+" => 41,
        "LT" => 42,
        "RT" => 43,
        "LS X-" => 44,
        "LS Y-" => 45,
        "RS X-" => 46,
        "RS Y-" => 47,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    GamePad,
    Pro,
}

impl Kind {
    fn for_player(player: usize) -> Kind {
        if player == 0 {
            Kind::GamePad
        } else {
            Kind::Pro
        }
    }

    /// As `EmulatedController::type_from_string` reads it.
    fn cemu_name(self) -> &'static str {
        match self {
            Kind::GamePad => "Wii U GamePad",
            Kind::Pro => "Wii U Pro Controller",
        }
    }

    /// The button's number in this controller's `ButtonId`. The two lists
    /// match except that the Pro Controller has Home straight after Minus, so
    /// from Up onwards its numbers are one higher.
    fn button_id(self, at: usize) -> u64 {
        let id = at as u64 + 1;
        match self {
            Kind::Pro if id > 10 => id + 1,
            _ => id,
        }
    }
}

/// How Cemu reads a player's pad.
#[derive(Debug, Clone, PartialEq)]
enum Reader {
    /// An XInput slot, counted from zero.
    XInput(u32),
    /// One of Cemu's SDL pads.
    Sdl(sdl::Found),
}

impl Reader {
    fn number(&self, input: &str) -> Option<u64> {
        match self {
            Reader::XInput(_) => xinput_number(input),
            Reader::Sdl(found) => sdl_number(input, found.by_label),
        }
    }

    /// The `<api>`, `<uuid>` and `<display_name>` a profile gives the pad.
    fn names(&self, player: &Player) -> (&'static str, String, String) {
        match self {
            Reader::XInput(slot) => ("XInput", slot.to_string(), format!("Controller {}", slot + 1)),
            Reader::Sdl(found) => ("SDLController", found.uuid.clone(), escape(&player.pad.name)),
        }
    }
}

/// A pad's USB vendor and product ids, with how many of that model come
/// before it, by its device name, as `pads::usb_ids` gives them.
type UsbIds<'a> = &'a dyn Fn(&str) -> Option<(u16, u16, usize)>;

/// How Cemu reads this player's pad, with `hid` the HID devices plugged in
/// and `ids` what gilrs knows of the pads it reads. `None` for a pad Cemu
/// cannot be told about: neither XInput nor one sdl.rs finds.
///
/// A pad gilrs has no ids for may still be plugged in. gilrs keeps what it
/// first learnt of each pad for as long as Omoio runs (`handle_event` in
/// gilrs-core 0.6.8's `windows_wgi/gamepad.rs`), and a pad that went away and
/// came back, by cable or over Bluetooth, has needed Omoio started again
/// before Cemu answered it (reported 8 October 2026). The HID devices are
/// read afresh each time, so such a pad is looked for there, by the name and
/// number it has in the layout.
fn reader_for(player: &Player, hid: &[sdl::HidPad], ids: UsbIds) -> Option<Reader> {
    if let Some(slot) = xinput_slot(&player.pad) {
        return Some(Reader::XInput(slot));
    }
    if player.pad.handler != "SDL" {
        return None;
    }
    let found = match ids(&player.pad.device) {
        Some((vendor, product, ordinal)) => sdl::find(vendor, product, ordinal, hid),
        None => sdl::find_named(&player.pad.name, number(&player.pad)?, hid),
    };
    found.map(Reader::Sdl)
}

/// The number pads.rs puts after a pad's name, the 0 of "PS5 Controller 0".
fn number(pad: &Pad) -> Option<usize> {
    pad.device.strip_prefix(pad.name.as_str())?.strip_prefix(' ')?.parse().ok()
}

/// The HID devices plugged in, as `find` lists them, for naming SDL pads in
/// the Cemu installed, `cemu` by its version. None when that Cemu is not the
/// release sdl.rs was read against (release.rs): its SDL may name the pads
/// otherwise, so they are left to stand in for or warn about instead. Looked
/// for only when someone plays on a pad gilrs reads, since SDL's way of
/// finding them opens every HID device.
fn hid_for(players: &[Player], cemu: Option<&str>, find: fn() -> Vec<sdl::HidPad>) -> Vec<sdl::HidPad> {
    if release::names_sdl_pads_as_checked(cemu) && players.iter().any(|p| p.pad.handler == "SDL") {
        find()
    } else {
        Vec::new()
    }
}

/// A pad's name as text inside an XML element.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The XInput slot a pad is in, counted from zero, or `None` for a pad that
/// is not read through XInput.
fn xinput_slot(pad: &Pad) -> Option<u32> {
    if pad.handler != "XInput" {
        return None;
    }
    pad.device
        .strip_prefix("XInput Pad #")?
        .parse::<u32>()
        .ok()
        .filter(|number| (1..=4).contains(number))
        .map(|number| number - 1)
}

/// The left stick's direction to take a d-pad direction from, for a Pro
/// Controller standing in for the GamePad. Two of Cemu's buttons may share
/// one of the pad's, so the stick still steers as a stick; the pad's own
/// d-pad then does nothing, which costs nothing in Trap Team, where it has no
/// job of its own (darkspyro.net's controls for the Wii U).
fn dpad_from_stick(place: &str) -> &str {
    match place {
        "Up" => "LS Y+",
        "Down" => "LS Y-",
        "Left" => "LS X-",
        "Right" => "LS X+",
        other => other,
    }
}

/// A player's file, as the `kind` of controller, with their pad read by
/// `reader` and the d-pad taken from the stick when `stick_dpad`. A player
/// Cemu cannot read the pad of still gets the file, with no pad in it, so one
/// left from before does not keep driving them.
fn profile(kind: Kind, player: &Player, reader: Option<&Reader>, stick_dpad: bool) -> String {
    let controller = reader
        .map(|reader| {
            let entries: String = WII_U
                .iter()
                .enumerate()
                .filter_map(|(at, (place, _))| {
                    let place = if stick_dpad { dpad_from_stick(place) } else { place };
                    let number = reader.number(player.input(place))?;
                    Some(format!(
                        "\t\t\t<entry>\n\t\t\t\t<mapping>{}</mapping>\n\t\t\t\t<button>{number}</button>\n\t\t\t</entry>\n",
                        kind.button_id(at)
                    ))
                })
                .collect();
            let (api, uuid, display_name) = reader.names(player);
            format!(
                "\t<controller>\n\
                 \t\t<api>{api}</api>\n\
                 \t\t<uuid>{uuid}</uuid>\n\
                 \t\t<display_name>{display_name}</display_name>\n\
                 \t\t<mappings>\n\
                 {entries}\
                 \t\t</mappings>\n\
                 \t</controller>\n"
            )
        })
        .unwrap_or_default();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <emulated_controller>\n\
         \t<type>{}</type>\n\
         {controller}\
         </emulated_controller>\n",
        kind.cemu_name()
    )
}

/// The players as Cemu gets them, with `plugged` the pads plugged in now and
/// `usable` whether Cemu can read a player's pad: an XInput one, or one of the
/// SDL pads sdl.rs finds. Player 1 on a pad Cemu cannot read would leave every game without anyone
/// to answer it, so they play on an XInput pad that is plugged in instead,
/// keeping their own buttons: one no other player has, else another player's,
/// who takes player 1's pad in return, so no pad drives two players. A PS5
/// pad run through DS4Windows is seen twice, by gilrs and as an XInput pad,
/// and a layout made while DS4Windows was off has the PS5 pad as player 1 and
/// its XInput twin, plugged in later, as player 2; this plays it. Only Cemu's
/// files change; the layout Omoio keeps stays as it was chosen, since RPCS3
/// reads the other pads itself.
fn stand_in(players: &[Player], plugged: &[Pad], usable: &dyn Fn(&Player) -> bool) -> Vec<Player> {
    let mut players = players.to_vec();
    let Some(first) = players.first() else {
        return players;
    };
    if usable(first) {
        return players;
    }
    let mut xinput = plugged.iter().filter(|pad| xinput_slot(pad).is_some());
    let free = xinput
        .clone()
        .find(|pad| !players.iter().any(|p| p.pad.device == pad.device));
    if let Some(pad) = free.or_else(|| xinput.next()) {
        let buttons = first.buttons.clone();
        pad_layout::give(&mut players, 0, Player { pad: pad.clone(), buttons });
    }
    players
}

/// Why player 1 would have no pad in Cemu with these `players`, after
/// `stand_in` has had its go with the pads `connected`, worded for the person
/// about to press Play. `None` when they have one.
pub fn missing_first_player(app: &AppHandle, players: &[Player], connected: &[Pad]) -> Option<String> {
    let hid = hid_for(players, super::detect_version(app).as_deref(), sdl::hid_pads);
    let ids = |device: &str| crate::pads::usb_ids(device);
    missing(players, connected, &|player: &Player| reader_for(player, &hid, &ids).is_some())
}

fn missing(players: &[Player], connected: &[Pad], usable: &dyn Fn(&Player) -> bool) -> Option<String> {
    let first = players.first()?;
    if stand_in(players, connected, usable).first().is_some_and(usable) {
        return None;
    }
    Some(format!(
        "Cemu can't use player 1's controller, {}, so the game won't answer it. \
         Plug in an Xbox controller, or one that works as one, or choose another for player 1 on the Controller screen.",
        first.pad.name
    ))
}

/// One file per player, counted from 0 as Cemu names them, and player 1's
/// again as both kinds of controller. `readers` goes with `players`.
fn write_all(dir: &Path, players: &[Player], readers: &[Option<Reader>]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let reader = |index: usize| readers.get(index).and_then(Option::as_ref);
    for (index, player) in players.iter().enumerate().take(PLAYERS) {
        let text = profile(Kind::for_player(index), player, reader(index), false);
        std::fs::write(dir.join(format!("controller{index}.xml")), text)?;
    }
    if let Some(first) = players.first() {
        std::fs::write(dir.join(FIRST_AS_GAMEPAD), profile(Kind::GamePad, first, reader(0), false))?;
        std::fs::write(dir.join(FIRST_AS_PRO), profile(Kind::Pro, first, reader(0), true))?;
    }
    Ok(())
}

/// Makes player 1 the kind of controller the game about to start needs: a
/// Pro Controller for one in `PRO_FIRST`, the GamePad for any other. Cemu
/// reads `controller0.xml` as the game starts, and each start sets it again.
pub fn first_player(app: &AppHandle, pro: bool) -> Result<(), String> {
    let dir = profile_dir(app)?;
    let from = dir.join(if pro { FIRST_AS_PRO } else { FIRST_AS_GAMEPAD });
    if !from.is_file() {
        return Ok(());
    }
    std::fs::copy(from, dir.join("controller0.xml"))
        .map(|_| ())
        .map_err(|_| "Couldn't save the controller settings for Cemu.".to_string())
}

fn profile_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(super::install_dir(app)?.join("portable").join("controllerProfiles"))
}

/// Writes the layout for every game where Cemu reads it. Nothing is written
/// before Cemu is installed, and a game's own layout is left to RPCS3.
pub fn write(app: &AppHandle, title_id: &str, players: &[Player]) -> Result<(), String> {
    if !title_id.is_empty() || !super::install_dir(app)?.join("Cemu.exe").is_file() {
        return Ok(());
    }
    let hid = hid_for(players, super::detect_version(app).as_deref(), sdl::hid_pads);
    let ids = |device: &str| crate::pads::usb_ids(device);
    let usable = |player: &Player| reader_for(player, &hid, &ids).is_some();
    let players = stand_in(players, &crate::pads::connected(), &usable);
    let readers: Vec<Option<Reader>> = players.iter().map(|player| reader_for(player, &hid, &ids)).collect();
    write_all(&profile_dir(app)?, &players, &readers)
        .map_err(|_| "Couldn't save the controller settings for Cemu.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::pad_layout::Pad;
    use std::collections::BTreeMap;

    fn xinput(slot: u32) -> Pad {
        Pad {
            device: format!("XInput Pad #{slot}"),
            name: format!("Controller {slot}"),
            handler: "XInput".to_string(),
            family: "xbox".to_string(),
        }
    }

    fn dualsense() -> Pad {
        Pad {
            device: "PS5 Controller 0".to_string(),
            name: "PS5 Controller".to_string(),
            handler: "SDL".to_string(),
            family: "playstation".to_string(),
        }
    }

    fn switch_pro() -> Pad {
        Pad {
            device: "Nintendo Switch Pro Controller 0".to_string(),
            name: "Nintendo Switch Pro Controller".to_string(),
            handler: "SDL".to_string(),
            family: "nintendo".to_string(),
        }
    }

    /// Cemu's SDL pad as sdl.rs finds a DualSense on USB.
    fn sdl_dualsense() -> Reader {
        Reader::Sdl(sdl::Found {
            uuid: "0_030057564c050000e60c000000016800".to_string(),
            by_label: false,
        })
    }

    fn sdl_switch_pro() -> Reader {
        Reader::Sdl(sdl::Found {
            uuid: "0_0300bb977e0500000920000010026803".to_string(),
            by_label: true,
        })
    }

    /// A player's file with their pad read through XInput, as `write` does.
    fn on_xinput(kind: Kind, player: &Player, stick_dpad: bool) -> String {
        profile(kind, player, xinput_slot(&player.pad).map(Reader::XInput).as_ref(), stick_dpad)
    }

    fn entry(mapping: u64, button: u64) -> String {
        format!("<mapping>{mapping}</mapping>\n\t\t\t\t<button>{button}</button>")
    }

    fn id_of(kind: Kind, name: &str) -> u64 {
        let at = WII_U.iter().position(|(_, n)| *n == name).unwrap();
        kind.button_id(at)
    }

    #[test]
    fn button_numbers_follow_each_controllers_own_list() {
        assert_eq!(id_of(Kind::GamePad, "A"), 1);
        assert_eq!(id_of(Kind::GamePad, "Minus"), 10);
        assert_eq!(id_of(Kind::GamePad, "D-pad up"), 11);
        assert_eq!(id_of(Kind::GamePad, "Right stick right"), 24);
        assert_eq!(id_of(Kind::Pro, "Minus"), 10);
        assert_eq!(id_of(Kind::Pro, "D-pad up"), 12, "Home sits before it");
        assert_eq!(id_of(Kind::Pro, "Right stick right"), 25);
    }

    #[test]
    fn player_one_is_the_gamepad_and_the_rest_are_pro_controllers() {
        let one = on_xinput(Kind::for_player(0), &Player::on(xinput(1)), false);
        assert!(one.contains("<type>Wii U GamePad</type>"));
        assert!(one.contains("<api>XInput</api>"));
        assert!(one.contains("<uuid>0</uuid>"));
        assert!(one.contains("<mapping>1</mapping>\n\t\t\t\t<button>13</button>"), "A is the right button");
        assert!(one.contains("<mapping>2</mapping>\n\t\t\t\t<button>12</button>"), "B, which games jump with, is the bottom one");

        let two = on_xinput(Kind::for_player(1), &Player::on(xinput(2)), false);
        assert!(two.contains("<type>Wii U Pro Controller</type>"));
        assert!(two.contains("<uuid>1</uuid>"));
        assert!(two.contains("<display_name>Controller 2</display_name>"));
        assert_eq!(two.matches("<entry>").count(), 24);
    }

    #[test]
    fn a_changed_layout_moves_the_button() {
        let buttons = BTreeMap::from([
            ("East".to_string(), "South".to_string()),
            ("South".to_string(), "East".to_string()),
        ]);
        let text = on_xinput(Kind::for_player(0), &Player::with_buttons(xinput(1), buttons), false);
        assert!(text.contains("<mapping>1</mapping>\n\t\t\t\t<button>12</button>"), "A now on the bottom button");
    }

    #[test]
    fn guide_is_never_written_for_xinput_since_cemu_cannot_read_it() {
        let buttons = BTreeMap::from([("Start".to_string(), "Guide".to_string())]);
        let text = on_xinput(Kind::for_player(1), &Player::with_buttons(xinput(2), buttons), false);
        assert_eq!(text.matches("<entry>").count(), 23);
    }

    #[test]
    fn a_pad_cemu_cannot_find_leaves_the_player_without_one() {
        // A pad that is not plugged in, or not one SDL's HID drivers take.
        let text = profile(Kind::for_player(1), &Player::on(dualsense()), None, false);
        assert!(text.contains("<type>Wii U Pro Controller</type>"));
        assert!(!text.contains("<controller>"));
    }

    #[test]
    fn every_player_gets_a_file_and_player_one_a_pro_controller_too() {
        let dir = std::env::temp_dir().join(format!("omoio-cemu-pads-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let players: Vec<Player> = (1..=4).map(|slot| Player::on(xinput(slot))).collect();
        let readers: Vec<Option<Reader>> = players.iter().map(|p| xinput_slot(&p.pad).map(Reader::XInput)).collect();
        write_all(&dir, &players, &readers).unwrap();
        for index in 0..PLAYERS {
            assert!(dir.join(format!("controller{index}.xml")).is_file());
        }
        let pro = std::fs::read_to_string(dir.join(FIRST_AS_PRO)).unwrap();
        assert!(pro.contains("<type>Wii U Pro Controller</type>"));
        assert!(pro.contains("<uuid>0</uuid>"), "player 1's own pad");
        // D-pad up, numbered after Home, is taken from the stick, which still
        // steers as a stick too.
        assert!(pro.contains("<mapping>12</mapping>\n\t\t\t\t<button>39</button>"), "d-pad up from the stick: {pro}");
        assert!(pro.contains("<mapping>18</mapping>\n\t\t\t\t<button>39</button>"), "stick up: {pro}");
        assert!(!pro.contains("<button>0</button>"), "the pad's own d-pad has no button: {pro}");
        let gamepad = std::fs::read_to_string(dir.join(FIRST_AS_GAMEPAD)).unwrap();
        assert_eq!(gamepad, std::fs::read_to_string(dir.join("controller0.xml")).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_playstation_pad_is_one_of_cemus_sdl_pads_with_buttons_by_place() {
        let text = profile(Kind::for_player(0), &Player::on(dualsense()), Some(&sdl_dualsense()), false);
        assert!(text.contains("<type>Wii U GamePad</type>"));
        assert!(text.contains("<api>SDLController</api>"));
        assert!(text.contains("<uuid>0_030057564c050000e60c000000016800</uuid>"));
        assert!(text.contains("<display_name>PS5 Controller</display_name>"));
        assert!(text.contains(&entry(1, 1)), "A on the right button, Circle: {text}");
        assert!(text.contains(&entry(2, 0)), "B on the bottom one, Cross: {text}");
        assert!(text.contains(&entry(3, 3)), "X on the top one, Triangle: {text}");
        assert!(text.contains(&entry(4, 2)), "Y on the left one, Square: {text}");
        assert!(text.contains(&entry(9, 6)), "Plus on Options: {text}");
        assert!(text.contains(&entry(11, 11)), "d-pad up: {text}");
        assert!(text.contains(&entry(17, 45)), "left stick up, which SDL counts as negative: {text}");
        assert!(text.contains(&entry(18, 39)), "left stick down: {text}");
        assert!(text.contains(&entry(21, 47)), "right stick up: {text}");
        assert!(text.contains(&entry(7, 42)), "ZL on L2: {text}");
        assert_eq!(text.matches("<entry>").count(), 24);
    }

    #[test]
    fn a_switch_pads_buttons_go_by_place_too() {
        // SDL calls the right button A on a Switch pad, so the Wii U's A, on
        // the right, is SDL's A there, and B, at the bottom, SDL's B.
        let text = profile(Kind::for_player(1), &Player::on(switch_pro()), Some(&sdl_switch_pro()), false);
        assert!(text.contains("<type>Wii U Pro Controller</type>"));
        assert!(text.contains("<uuid>0_0300bb977e0500000920000010026803</uuid>"));
        assert!(text.contains(&entry(1, 0)), "A: {text}");
        assert!(text.contains(&entry(2, 1)), "B: {text}");
        assert!(text.contains(&entry(3, 2)), "X, on top: {text}");
        assert!(text.contains(&entry(4, 3)), "Y, on the left: {text}");
    }

    #[test]
    fn a_changed_layout_moves_the_button_on_an_sdl_pad() {
        let buttons = BTreeMap::from([
            ("East".to_string(), "South".to_string()),
            ("South".to_string(), "East".to_string()),
        ]);
        let player = Player::with_buttons(dualsense(), buttons);
        let text = profile(Kind::for_player(0), &player, Some(&sdl_dualsense()), false);
        assert!(text.contains(&entry(1, 0)), "A now on Cross: {text}");
        assert!(text.contains(&entry(2, 1)), "B now on Circle: {text}");
    }

    #[test]
    fn guide_is_written_for_an_sdl_pad_since_cemu_reads_it() {
        let buttons = BTreeMap::from([("Start".to_string(), "Guide".to_string())]);
        let player = Player::with_buttons(dualsense(), buttons);
        let text = profile(Kind::for_player(1), &player, Some(&sdl_dualsense()), false);
        assert_eq!(text.matches("<entry>").count(), 24);
        assert!(text.contains(&entry(9, 5)), "Plus on the PS button: {text}");
    }

    #[test]
    fn a_pads_name_is_kept_as_text() {
        let mut pad = dualsense();
        pad.name = "Pads & <Co>".to_string();
        let text = profile(Kind::for_player(1), &Player::on(pad), Some(&sdl_dualsense()), false);
        assert!(text.contains("<display_name>Pads &amp; &lt;Co&gt;</display_name>"));
    }

    #[test]
    fn trap_team_on_a_playstation_pad_takes_the_d_pad_from_the_stick() {
        let dir = std::env::temp_dir().join(format!("omoio-cemu-sdl-pads-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let players = vec![Player::on(dualsense()), Player::on(xinput(1))];
        let readers = vec![Some(sdl_dualsense()), Some(Reader::XInput(0))];
        write_all(&dir, &players, &readers).unwrap();
        let pro = std::fs::read_to_string(dir.join(FIRST_AS_PRO)).unwrap();
        assert!(pro.contains("<type>Wii U Pro Controller</type>"));
        assert!(pro.contains("<api>SDLController</api>"));
        // D-pad up, numbered after Home, is the stick pushed up, which SDL
        // counts as negative, and the stick still steers as a stick too.
        assert!(pro.contains(&entry(12, 45)), "d-pad up from the stick: {pro}");
        assert!(pro.contains(&entry(18, 45)), "stick up: {pro}");
        assert!(pro.contains(&entry(13, 39)), "d-pad down from the stick: {pro}");
        assert!(!pro.contains("<button>11</button>"), "the pad's own d-pad has no button: {pro}");
        let gamepad = std::fs::read_to_string(dir.join(FIRST_AS_GAMEPAD)).unwrap();
        assert!(gamepad.contains(&entry(11, 11)), "as the GamePad the d-pad is the d-pad: {gamepad}");
        let two = std::fs::read_to_string(dir.join("controller1.xml")).unwrap();
        assert!(two.contains("<api>XInput</api>"), "an Xbox pad beside it stays XInput: {two}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("omoio-cemu-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn devices(players: &[Player]) -> Vec<&str> {
        players.iter().map(|p| p.pad.device.as_str()).collect()
    }

    /// A DualSense on USB, as Windows reports it, for sdl.rs to find.
    fn dualsense_hid() -> sdl::HidPad {
        sdl::HidPad {
            vendor: 0x054C,
            product: 0x0CE6,
            version: 0x0100,
            manufacturer: Some("Sony Interactive Entertainment".to_string()),
            product_name: Some("Wireless Controller".to_string()),
            bluetooth: false,
        }
    }

    /// What gilrs knows of `dualsense()` while it is plugged in.
    fn dualsense_ids(device: &str) -> Option<(u16, u16, usize)> {
        (device == "PS5 Controller 0").then_some((0x054C, 0x0CE6, 0))
    }

    /// Whether Cemu can use a player's pad, with `hid` the HID devices
    /// plugged in, as `write` asks it.
    fn usable_with(hid: &[sdl::HidPad]) -> impl Fn(&Player) -> bool + '_ {
        move |player| reader_for(player, hid, &dualsense_ids).is_some()
    }

    #[test]
    fn a_ps5_pad_through_ds4windows_plays_player_one_as_its_xinput_twin() {
        // Set up while DS4Windows was off, then played with it on: the PS5
        // pad is player 1 and the XInput pad DS4Windows makes of it player 2.
        // DS4Windows can hide the PS5 pad itself, so SDL finds none here.
        let buttons = BTreeMap::from([
            ("East".to_string(), "South".to_string()),
            ("South".to_string(), "East".to_string()),
        ]);
        let mut players = vec![Player::with_buttons(dualsense(), buttons.clone())];
        players.extend((1..=3).map(|slot| Player::on(xinput(slot))));
        let plugged = [xinput(1), dualsense()];
        let usable = usable_with(&[]);
        let cemu = stand_in(&players, &plugged, &usable);
        assert_eq!(devices(&cemu), ["XInput Pad #1", "PS5 Controller 0", "XInput Pad #2", "XInput Pad #3"]);
        assert_eq!(cemu[0].buttons, buttons, "player 1 keeps their own buttons");
        assert!(cemu[1].buttons.is_empty(), "and player 2 theirs");
        assert_eq!(devices(&players)[0], "PS5 Controller 0", "the layout Omoio keeps is untouched");

        let dir = scratch("ds4windows");
        let readers: Vec<Option<Reader>> = cemu.iter().map(|p| reader_for(p, &[], &dualsense_ids)).collect();
        write_all(&dir, &cemu, &readers).unwrap();
        let one = std::fs::read_to_string(dir.join("controller0.xml")).unwrap();
        assert!(one.contains("<api>XInput</api>") && one.contains("<uuid>0</uuid>"), "{one}");
        assert!(one.contains(&entry(1, 12)), "A as player 1 set it: {one}");
        let two = std::fs::read_to_string(dir.join("controller1.xml")).unwrap();
        assert!(!two.contains("<controller>"), "the same pad does not drive player 2 too: {two}");
        let pro = std::fs::read_to_string(dir.join(FIRST_AS_PRO)).unwrap();
        assert!(pro.contains("<uuid>0</uuid>"), "Trap Team's player 1 gets it as well: {pro}");
        assert!(pro.contains(&entry(12, 39)), "d-pad from the stick: {pro}");
        assert_eq!(missing(&players, &plugged, &usable), None, "so Play says nothing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_ps5_pad_cemu_can_find_keeps_player_one() {
        // With SDL pads in Cemu, the PS5 pad is player 1's in Cemu too, and
        // its XInput twin from DS4Windows is not swapped in for it.
        let mut players = vec![Player::on(dualsense())];
        players.extend((1..=3).map(|slot| Player::on(xinput(slot))));
        let plugged = [xinput(1), dualsense()];
        let hid = [dualsense_hid()];
        let usable = usable_with(&hid);
        assert_eq!(stand_in(&players, &plugged, &usable), players);
        assert_eq!(missing(&players, &plugged, &usable), None, "so Play says nothing");
        let reader = reader_for(&players[0], &hid, &dualsense_ids);
        assert_eq!(reader, Some(sdl_dualsense()), "and its file names it as an SDL pad");
    }

    #[test]
    fn an_xinput_pad_no_one_has_is_taken_first() {
        let mut players = vec![Player::on(dualsense())];
        players.extend((1..=3).map(|slot| Player::on(xinput(slot))));
        let cemu = stand_in(&players, &[xinput(1), dualsense(), xinput(4)], &usable_with(&[]));
        assert_eq!(devices(&cemu), ["XInput Pad #4", "XInput Pad #1", "XInput Pad #2", "XInput Pad #3"]);
    }

    #[test]
    fn player_one_on_xinput_is_left_alone() {
        let players = vec![Player::on(xinput(2)), Player::on(dualsense())];
        let cemu = stand_in(&players, &[xinput(1), dualsense()], &usable_with(&[]));
        assert_eq!(cemu, players, "even with their own pad not plugged in");
    }

    #[test]
    fn with_no_pad_cemu_can_use_play_says_why_nothing_will_answer() {
        let mut players = vec![Player::on(dualsense())];
        players.extend((1..=3).map(|slot| Player::on(xinput(slot))));
        let usable = usable_with(&[]);
        assert_eq!(stand_in(&players, &[dualsense()], &usable), players);
        let why = missing(&players, &[dualsense()], &usable).expect("player 1 has no pad in Cemu");
        assert!(why.starts_with("Cemu can't use player 1's controller, PS5 Controller,"), "{why}");
        assert!(why.contains("Controller screen"), "{why}");
    }

    #[test]
    fn player_one_on_xinput_is_never_warned_about() {
        // An XInput slot is named in Cemu's file even with nothing in it, so
        // a pad switched on late still plays.
        let players = vec![Player::on(xinput(1)), Player::on(dualsense())];
        assert_eq!(missing(&players, &[], &usable_with(&[])), None);
    }

    fn plugged_dualsense() -> Vec<sdl::HidPad> {
        vec![dualsense_hid()]
    }

    fn not_looked_for() -> Vec<sdl::HidPad> {
        panic!("the HID devices were looked at")
    }

    #[test]
    fn only_the_checked_cemu_has_its_sdl_pads_named() {
        let mut players = vec![Player::on(dualsense())];
        players.extend((1..=3).map(|slot| Player::on(xinput(slot))));
        let plugged = [xinput(1), dualsense()];

        // Cemu v2.6, which sdl.rs was read against, plays the PS5 pad itself.
        let hid = hid_for(&players, Some("2.6"), plugged_dualsense);
        assert_eq!(hid, [dualsense_hid()]);
        assert_eq!(stand_in(&players, &plugged, &usable_with(&hid)), players);

        // A later Cemu, built with another SDL, is not told about it: its
        // XInput twin from DS4Windows stands in, ...
        let hid = hid_for(&players, Some("2.7"), plugged_dualsense);
        assert!(hid.is_empty());
        assert_eq!(devices(&stand_in(&players, &plugged, &usable_with(&hid)))[0], "XInput Pad #1");
        // ... and with no XInput pad plugged in, Play says so.
        let why = missing(&players, &[dualsense()], &usable_with(&hid));
        assert!(why.is_some_and(|why| why.contains("PS5 Controller")));

        assert!(hid_for(&players, None, plugged_dualsense).is_empty(), "nor a Cemu of no known version");
    }

    /// gilrs has lost sight of every pad.
    fn no_ids(_device: &str) -> Option<(u16, u16, usize)> {
        None
    }

    #[test]
    fn a_pad_gilrs_has_no_ids_for_is_found_among_the_hid_devices() {
        let hid = [dualsense_hid()];
        let player = Player::on(dualsense());
        assert_eq!(reader_for(&player, &hid, &no_ids), Some(sdl_dualsense()), "the same name as through gilrs");
        let mut second = dualsense();
        second.device = "PS5 Controller 1".to_string();
        assert_eq!(reader_for(&Player::on(second), &hid, &no_ids), None, "only one is plugged in");
        assert_eq!(reader_for(&player, &[], &no_ids), None, "nor when Windows has none either");

        let mut players = vec![Player::on(dualsense())];
        players.extend((1..=3).map(|slot| Player::on(xinput(slot))));
        let usable = |player: &Player| reader_for(player, &hid, &no_ids).is_some();
        assert_eq!(stand_in(&players, &[xinput(1)], &usable), players, "player 1 keeps the pad");
        assert_eq!(missing(&players, &[], &usable), None, "and Play says nothing");
    }

    #[test]
    fn a_switch_pad_gilrs_has_no_ids_for_keeps_its_buttons_by_letter() {
        let pro = sdl::HidPad {
            vendor: 0x057E,
            product: 0x2009,
            version: 0x0210,
            manufacturer: Some("Nintendo Co., Ltd.".to_string()),
            product_name: Some("Pro Controller".to_string()),
            bluetooth: true,
        };
        assert_eq!(reader_for(&Player::on(switch_pro()), &[pro], &no_ids), Some(sdl_switch_pro()));
    }

    #[test]
    fn a_pad_named_otherwise_is_not_looked_for_by_its_name() {
        let mut pad = dualsense();
        pad.name = "Wireless Controller".to_string();
        pad.device = "Wireless Controller 0".to_string();
        assert_eq!(reader_for(&Player::on(pad), &[dualsense_hid()], &no_ids), None);
    }

    #[test]
    fn hid_devices_are_only_looked_at_for_a_pad_gilrs_reads() {
        let players = vec![Player::on(xinput(1)), Player::on(xinput(2))];
        assert!(hid_for(&players, Some("2.6"), not_looked_for).is_empty());
        let later = vec![Player::on(dualsense())];
        assert!(hid_for(&later, Some("2.7"), not_looked_for).is_empty(), "nor for a Cemu it would not name");
    }
}
