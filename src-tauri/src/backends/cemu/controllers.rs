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
//! The keyboard and mouse are a pad Omoio serves to Cemu while their game
//! runs (dsu.rs, keyboard_pad.rs), which Cemu reads as one of its DSU pads,
//! so the mouse's buttons press and its moving turns a stick
//! (`Reader::Dsu`). When Omoio can't serve it, the keyboard is Cemu's own
//! keyboard controller, one key for each button (`Keymap`): Cemu reads no
//! mouse for a controller, so then the mouse's buttons are left out and the
//! mouse moving is the arrow keys (`arrow_for`).
//!
//! Player 1 is the one every game answers, so they are never left without a
//! pad while an XInput one is plugged in (`stand_in`), and Play says so before
//! a game starts with nobody to answer it (`missing_first_player`).
//!
//! Cemu keeps one set of files for every game, so a game's own layout goes
//! into the same files: before every game Omoio writes the layout that game
//! plays with, its own when it has one (controllers.rs, `before_launch`),
//! and the next game gets its own again. Up to 0.3.2 only the layout for
//! every game was written, so a Wii U game's own layout did nothing (Discord,
//! v1ad, ticket #5, 10 October 2026: Cross and Circle swapped on a PS5 pad
//! for one game changed nothing in it).

use super::{release, sdl};
use crate::core::keys;
use crate::core::pad_layout::{self, Pad, Player, PLAYERS};
use crate::keyboard_pad;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, MapVirtualKeyExW, MAPVK_VSC_TO_VK_EX, VIRTUAL_KEY, VK_DECIMAL, VK_DOWN, VK_LEFT, VK_LSHIFT,
    VK_NUMPAD0, VK_NUMPAD1, VK_NUMPAD2, VK_NUMPAD3, VK_NUMPAD4, VK_NUMPAD5, VK_NUMPAD6, VK_NUMPAD7, VK_NUMPAD8,
    VK_NUMPAD9, VK_RIGHT, VK_RSHIFT, VK_UP,
};

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

/// Cemu's number for each key a keyboard player can be given, by its name
/// in core/keys.rs.
///
/// Cemu hears a key in whichever of its windows has the keyboard and keeps it
/// by the virtual-key code Windows sends with it (`CemuApp::FilterEvent`,
/// gui/CemuApp.cpp lines 389 to 409, Cemu v2.6), which wxWidgets passes on as
/// it comes in `WM_KEYDOWN` (`MSWInitAnyKeyEvent`, src/msw/window.cpp line
/// 6342, wxWidgets 3.2.5, the one Cemu 2.6 is built with; both read 20
/// October 2026). A letter's code follows the keyboard layout: the key a US
/// keyboard has Y on is Z on a German one, and Cemu keeps it as Z. Omoio
/// keeps keys by where they sit, so the codes are worked out through the
/// layout Windows has on this PC, each time the files are written before a
/// game.
#[derive(Debug, Clone, PartialEq)]
struct Keymap(Vec<(&'static str, u64)>);

impl Keymap {
    /// With `layout` giving a scan code's virtual-key code, as `layout_vk`.
    fn new(layout: &dyn Fn(u16) -> u32) -> Self {
        let known = keys::KEYS.iter().filter_map(|&(name, scan)| Some((name, cemu_key(name, scan, layout)?)));
        Keymap(known.collect())
    }

    fn number(&self, input: &str) -> Option<u64> {
        if let Some(arrow) = arrow_for(input) {
            return Some(u64::from(arrow.0));
        }
        self.0.iter().find(|(name, _)| *name == input).map(|&(_, number)| number)
    }
}

/// Cemu's number for the key `name`, whose scan code is `scan`, with `layout`
/// giving the virtual-key code a scan code has in the keyboard layout, 0 for
/// none. Some keys are the same in every layout and are not looked up:
///
/// - Shift, which Cemu tells left from right by scan code, 0x2A as 160 and
///   0x36 as 161 (`fix_raw_keycode`, gui/helpers/wxHelpers.cpp lines 38 to
///   66, v2.6, read 20 October 2026).
/// - The numeric keypad's digits and its point. Windows sends them as
///   `VK_NUMPAD0` to `VK_NUMPAD9` and `VK_DECIMAL` while Num Lock is on,
///   which no lookup by scan code gives, and they are written so. With Num
///   Lock off the same keys come as Home, the arrows, Page Up and the like,
///   so Cemu takes them for those keys: keypad 8 then presses what the up
///   arrow does, and a button on keypad 8 doesn't answer.
///
/// Enter on the keypad comes as Enter itself, `VK_RETURN`, which the lookup
/// gives too, so in Cemu it presses what Enter does.
fn cemu_key(name: &str, scan: u16, layout: &dyn Fn(u16) -> u32) -> Option<u64> {
    let fixed = match name {
        "ShiftLeft" => Some(VK_LSHIFT),
        "ShiftRight" => Some(VK_RSHIFT),
        "Numpad0" => Some(VK_NUMPAD0),
        "Numpad1" => Some(VK_NUMPAD1),
        "Numpad2" => Some(VK_NUMPAD2),
        "Numpad3" => Some(VK_NUMPAD3),
        "Numpad4" => Some(VK_NUMPAD4),
        "Numpad5" => Some(VK_NUMPAD5),
        "Numpad6" => Some(VK_NUMPAD6),
        "Numpad7" => Some(VK_NUMPAD7),
        "Numpad8" => Some(VK_NUMPAD8),
        "Numpad9" => Some(VK_NUMPAD9),
        "NumpadDecimal" => Some(VK_DECIMAL),
        _ => None,
    };
    let code = fixed.map_or_else(|| layout(scan), |key| u32::from(key.0));
    (code != 0).then_some(u64::from(code))
}

/// The virtual-key code of a scan code in the keyboard layout Windows has
/// now, 0 for none. An extended key's scan code carries 0xE0 in its high
/// byte, as core/keys.rs keeps it, which `MAPVK_VSC_TO_VK_EX` reads
/// ("MapVirtualKeyExW function", Microsoft Learn, read 20 October 2026).
fn layout_vk(scan: u16) -> u32 {
    unsafe { MapVirtualKeyExW(u32::from(scan), MAPVK_VSC_TO_VK_EX, Some(GetKeyboardLayout(0))) }
}

/// The arrow key that stands for the mouse moving one way. Cemu 2.6 reads
/// controllers through its keyboard, SDL, XInput, DirectInput, DSU,
/// GameCube adapter, Wii Remote and Windows.Gaming.Input apis, and none of
/// them reads a mouse (`InputAPI::Type`, input/api/InputAPI.h lines 7 to 21,
/// v2.6, read 20 October 2026). So when Omoio can't serve the mouse as a
/// pad (`Reader::Dsu`), the mouse moving is the arrow keys in Cemu, and the
/// right stick, which the mouse moves by default, is on the arrows. The
/// mouse's buttons and wheel have no stand-in, and are left out; the default
/// layout has keys beside them on the face buttons (J, K and E).
fn arrow_for(input: &str) -> Option<VIRTUAL_KEY> {
    Some(match input {
        "MouseMoveLeft" => VK_LEFT,
        "MouseMoveRight" => VK_RIGHT,
        "MouseMoveUp" => VK_UP,
        "MouseMoveDown" => VK_DOWN,
        _ => return None,
    })
}

/// What Cemu writes for its keyboard controller between its names and its
/// mappings: the stick, rotation and trigger settings every controller has,
/// at Cemu's own defaults (`AxisSetting`, input/api/Controller.h line 135),
/// as `InputManager::save` writes them (input/InputManager.cpp lines 487 to
/// 530, v2.6; both read 20 October 2026). Loading them changes nothing, but
/// Cemu saves every player's file again each time its input settings window
/// closes (`InputSettings2::~InputSettings2`, gui/input/InputSettings2.cpp
/// lines 121 to 128, the window `hush` opens and closes for the portal
/// menu), and then the file reads as Cemu would write it. Pad
/// players' files have never had them, and stay as they were.
const KEYBOARD_SETTINGS: &str = "\t\t<axis>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</axis>\n\
     \t\t<rotation>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</rotation>\n\
     \t\t<trigger>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</trigger>\n";

/// What Cemu writes for a DSU pad between its names and its mappings, in
/// the order `InputManager::save` writes it, with the server's address after
/// (`DSUController::save`, input/api/DSU/DSUController.cpp lines 21 to 29,
/// v2.6, read 20 October 2026), `{port}` for the port:
///
/// - Motion on. Cemu then reads the GamePad's tilt from the pad
///   (`EmulatedController::has_motion`), which stays still unless the mouse
///   tilts it, and no longer turns it while the right mouse button drags in
///   its window (`VPADController::update_motion`, VPADController.cpp lines
///   237 to 310), which by default is a face button here.
/// - No dead zone for either stick. Keys push a stick all the way, and the
///   mouse is given its own least push (`keyboard_pad::LEAST`), which a dead
///   zone would only take away from (`apply_axis_setting`, Controller.cpp
///   lines 74 to 120). The triggers travel as buttons and keep the default.
const DSU_SETTINGS: &str = "\t\t<motion>true</motion>\n\
     \t\t<axis>\n\t\t\t<deadzone>0</deadzone>\n\t\t\t<range>1</range>\n\t\t</axis>\n\
     \t\t<rotation>\n\t\t\t<deadzone>0</deadzone>\n\t\t\t<range>1</range>\n\t\t</rotation>\n\
     \t\t<trigger>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</trigger>\n\
     \t\t<ip>127.0.0.1</ip>\n\t\t<port>{port}</port>\n";

/// How Cemu reads a player's pad.
#[derive(Debug, Clone, PartialEq)]
enum Reader {
    /// An XInput slot, counted from zero.
    XInput(u32),
    /// One of Cemu's SDL pads.
    Sdl(sdl::Found),
    /// Cemu's keyboard controller (`KeyboardController`, v2.6).
    Keyboard(Keymap),
    /// The keyboard and mouse as the pad Omoio serves on this port.
    Dsu(u16),
}

impl Reader {
    /// Cemu's number for what `player` presses for `place`. Cemu keeps one
    /// button for each of its own (`EmulatedController::set_mapping`, an
    /// `unordered_map`, input/emulated/EmulatedController.cpp lines 325 to
    /// 329, v2.6), so of a keyboard player's inputs for a place, the first
    /// Cemu can read is the one.
    fn number(&self, player: &Player, place: &str) -> Option<u64> {
        match self {
            Reader::XInput(_) => xinput_number(player.input(place)),
            Reader::Sdl(found) => sdl_number(player.input(place), found.by_label),
            Reader::Keyboard(keymap) => player.keys(place).find_map(|input| keymap.number(input)),
            // Each place is one button or stick direction of the pad, and
            // the pad presses it for every input the player has there.
            Reader::Dsu(_) => player
                .keys(place)
                .next()
                .and_then(|_| keyboard_pad::output(place))
                .map(keyboard_pad::Output::cemu_number),
        }
    }

    /// The `<api>`, `<uuid>` and `<display_name>` a profile gives the pad,
    /// the keyboard's as Cemu names its own (`KeyboardController`'s
    /// constructor, input/api/Keyboard/KeyboardController.cpp line 7, v2.6).
    fn names(&self, player: &Player) -> (&'static str, String, String) {
        match self {
            Reader::XInput(slot) => ("XInput", slot.to_string(), format!("Controller {}", slot + 1)),
            Reader::Sdl(found) => ("SDLController", found.uuid.clone(), escape(&player.pad.name)),
            Reader::Keyboard(_) => ("Keyboard", "keyboard".to_string(), "Keyboard".to_string()),
            // The uuid is the pad's slot, and the name Cemu gives a DSU pad
            // (`ControllerFactory::CreateController`, input/ControllerFactory.cpp
            // lines 79 to 84, and DSUController.cpp lines 5 to 6, v2.6).
            Reader::Dsu(_) => (
                "DSUController",
                crate::dsu::SLOT.to_string(),
                format!("Controller {}", crate::dsu::SLOT + 1),
            ),
        }
    }

    /// What goes between the names and the mappings.
    fn settings(&self) -> String {
        match self {
            Reader::Keyboard(_) => KEYBOARD_SETTINGS.to_string(),
            Reader::Dsu(port) => DSU_SETTINGS.replace("{port}", &port.to_string()),
            Reader::XInput(_) | Reader::Sdl(_) => String::new(),
        }
    }
}

/// A pad's USB vendor and product ids, with how many of that model come
/// before it, by its device name, as `pads::usb_ids` gives them.
type UsbIds<'a> = &'a dyn Fn(&str) -> Option<(u16, u16, usize)>;

/// How Cemu reads this player's pad, with `hid` the HID devices plugged in
/// and `ids` what gilrs knows of the pads it reads. `None` for a pad Cemu
/// cannot be told about: neither the keyboard, nor XInput, nor one sdl.rs
/// finds. The keyboard is always there, so Cemu can always read it.
///
/// A pad gilrs has no ids for may still be plugged in. gilrs keeps what it
/// first learnt of each pad for as long as Omoio runs (`handle_event` in
/// gilrs-core 0.6.8's `windows_wgi/gamepad.rs`), and a pad that went away and
/// came back, by cable or over Bluetooth, has needed Omoio started again
/// before Cemu answered it (reported 8 October 2026). The HID devices are
/// read afresh each time, so such a pad is looked for there, by the name and
/// number it has in the layout.
///
/// The keyboard is the pad Omoio serves while it serves one, and Cemu's own
/// keyboard controller otherwise.
fn reader_for(player: &Player, hid: &[sdl::HidPad], ids: UsbIds) -> Option<Reader> {
    if player.pad.is_keyboard() {
        return Some(match crate::dsu::port() {
            Some(port) => Reader::Dsu(port),
            None => Reader::Keyboard(Keymap::new(&layout_vk)),
        });
    }
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
                    let number = reader.number(player, place)?;
                    Some(format!(
                        "\t\t\t<entry>\n\t\t\t\t<mapping>{}</mapping>\n\t\t\t\t<button>{number}</button>\n\t\t\t</entry>\n",
                        kind.button_id(at)
                    ))
                })
                .collect();
            let (api, uuid, display_name) = reader.names(player);
            let settings = reader.settings();
            format!(
                "\t<controller>\n\
                 \t\t<api>{api}</api>\n\
                 \t\t<uuid>{uuid}</uuid>\n\
                 \t\t<display_name>{display_name}</display_name>\n\
                 {settings}\
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
/// `usable` whether Cemu can read a player's pad: the keyboard, an XInput
/// one, or one of the SDL pads sdl.rs finds. Player 1 on a pad Cemu cannot read would leave every game without anyone
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
    // No controller is what someone chose for player 1, so it stays.
    if usable(first) || first.pad.is_none() {
        return players;
    }
    let mut xinput = plugged.iter().filter(|pad| xinput_slot(pad).is_some());
    let free = xinput
        .clone()
        .find(|pad| !players.iter().any(|p| p.pad.device == pad.device));
    if let Some(pad) = free.or_else(|| xinput.next()) {
        let (buttons, mouse) = (first.buttons.clone(), first.mouse);
        pad_layout::give(&mut players, 0, Player { pad: pad.clone(), buttons, mouse });
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
    if first.pad.is_none() {
        return Some(
            "Player 1 has no controller, so the game won't answer anyone as player 1. Choose one for player 1 on the Controller screen."
                .to_string(),
        );
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

/// Writes a layout where Cemu reads it: the one for every game, or a game's
/// own, which take the same files (the header). Nothing is written before
/// Cemu is installed. Saving any game's own layout on the Controller screen
/// writes it here too, which lasts only until the next game starts and
/// writes its own.
pub fn write(app: &AppHandle, _title_id: &str, players: &[Player]) -> Result<(), String> {
    if !super::install_dir(app)?.join("Cemu.exe").is_file() {
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

/// Serves the keyboard and mouse as a pad for `game`, which is about to
/// start, when a player has them in the layout it plays with, its own or
/// the one for every game, as `controllers::before_launch` picks it. That
/// runs after this (commands.rs, `launch_game`) and writes the player's file,
/// which names the pad while it is served (`reader_for`). Any pad served
/// for the game before stops. When this one can't be served, the player
/// plays on Cemu's keyboard controller.
pub fn serve_keyboard(app: &AppHandle, game: &crate::core::library::Game) {
    crate::dsu::stop();
    let connected = crate::pads::connected();
    let mut players = crate::controllers::current(app, &game.title_id, &connected).players;
    pad_layout::seat_from(&mut players, &connected, &crate::pads::seatable(&connected));
    let Some(player) = players.iter().find(|player| player.pad.is_keyboard()) else {
        return;
    };
    let aim = keyboard_pad::Aim::for_game(super::mouse_aim(app, game).as_deref(), &game.title);
    let _ = crate::dsu::start(player, aim);
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
    fn player_one_with_no_controller_keeps_none() {
        let players = vec![Player::on(Pad::none()), Player::on(xinput(2))];
        let usable = usable_with(&[]);
        assert_eq!(stand_in(&players, &[xinput(1)], &usable), players);
        assert!(missing(&players, &[xinput(1)], &usable).unwrap().contains("Player 1 has no controller"));
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
    fn an_xinput_players_files_are_what_cemu_has_always_been_given() {
        // Every byte, so nothing done for other pads changes an Xbox pad's.
        fn text(kind: &str, controller: u32, entries: &[(u64, u64)]) -> String {
            let entries: String = entries
                .iter()
                .map(|(mapping, button)| {
                    format!("\t\t\t<entry>\n\t\t\t\t<mapping>{mapping}</mapping>\n\t\t\t\t<button>{button}</button>\n\t\t\t</entry>\n")
                })
                .collect();
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<emulated_controller>\n\t<type>{kind}</type>\n\t<controller>\n\
                 \t\t<api>XInput</api>\n\t\t<uuid>{controller}</uuid>\n\t\t<display_name>Controller {}</display_name>\n\
                 \t\t<mappings>\n{entries}\t\t</mappings>\n\t</controller>\n</emulated_controller>\n",
                controller + 1
            )
        }
        let gamepad = text(
            "Wii U GamePad",
            0,
            &[
                (1, 13), (2, 12), (3, 15), (4, 14), (5, 8), (6, 9), (7, 42), (8, 43), (9, 4), (10, 5), (11, 0), (12, 1),
                (13, 2), (14, 3), (15, 6), (16, 7), (17, 39), (18, 45), (19, 44), (20, 38), (21, 41), (22, 47), (23, 46),
                (24, 40),
            ],
        );
        let pro = text(
            "Wii U Pro Controller",
            1,
            &[
                (1, 13), (2, 12), (3, 15), (4, 14), (5, 8), (6, 9), (7, 42), (8, 43), (9, 4), (10, 5), (12, 0), (13, 1),
                (14, 2), (15, 3), (16, 6), (17, 7), (18, 39), (19, 45), (20, 44), (21, 38), (22, 41), (23, 47), (24, 46),
                (25, 40),
            ],
        );
        let dir = scratch("xinput-bytes");
        let players: Vec<Player> = (1..=4).map(|slot| Player::on(xinput(slot))).collect();
        let readers: Vec<Option<Reader>> = players.iter().map(|p| reader_for(p, &[dualsense_hid()], &no_ids)).collect();
        write_all(&dir, &players, &readers).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("controller0.xml")).unwrap(), gamepad);
        assert_eq!(std::fs::read_to_string(dir.join(FIRST_AS_GAMEPAD)).unwrap(), gamepad);
        assert_eq!(std::fs::read_to_string(dir.join("controller1.xml")).unwrap(), pro);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A US keyboard's codes by scan code, as `MapVirtualKeyExW` gives them,
    /// for the keys these tests use, with Shift as one key and keypad 8 as
    /// the up arrow, as Windows' own lookup has them.
    fn us(scan: u16) -> u32 {
        let Some(&(name, _)) = keys::KEYS.iter().find(|&&(_, s)| s == scan) else {
            return 0;
        };
        if let Some(letter) = name.strip_prefix("Key").or_else(|| name.strip_prefix("Digit")) {
            return u32::from(letter.as_bytes()[0]);
        }
        match name {
            "Space" => 0x20,
            "Enter" | "NumpadEnter" => 0x0D,
            "Backspace" => 0x08,
            "ShiftLeft" | "ShiftRight" => 0x10,
            "Numpad8" | "ArrowUp" => 0x26,
            "ArrowLeft" => 0x25,
            "ArrowRight" => 0x27,
            "ArrowDown" => 0x28,
            _ => 0,
        }
    }

    /// A German keyboard's, with Y and Z the other way round.
    fn german(scan: u16) -> u32 {
        match scan {
            0x15 => u32::from(b'Z'),
            0x2C => u32::from(b'Y'),
            other => us(other),
        }
    }

    fn keyboard_on(layout: fn(u16) -> u32) -> Reader {
        Reader::Keyboard(Keymap::new(&layout))
    }

    #[test]
    fn a_key_is_kept_as_cemu_keeps_a_press_of_it() {
        let us_keys = Keymap::new(&us);
        assert_eq!(us_keys.number("KeyY"), Some(u64::from(b'Y')));
        assert_eq!(us_keys.number("KeyW"), Some(u64::from(b'W')));
        assert_eq!(us_keys.number("Digit1"), Some(u64::from(b'1')));
        let german_keys = Keymap::new(&german);
        assert_eq!(german_keys.number("KeyY"), Some(u64::from(b'Z')), "the key in the US Y's place is Z there");
        assert_eq!(german_keys.number("KeyZ"), Some(u64::from(b'Y')));
        assert_eq!(german_keys.number("KeyW"), Some(u64::from(b'W')));

        assert_eq!(cemu_key("ShiftLeft", 0x2A, &us), Some(160), "left Shift apart from right");
        assert_eq!(cemu_key("ShiftRight", 0x36, &us), Some(161));
        assert_eq!(cemu_key("Numpad8", 0x48, &us), Some(0x68), "as with Num Lock on, not the up arrow");
        assert_eq!(cemu_key("NumpadDecimal", 0x53, &us), Some(0x6E));
        assert_eq!(cemu_key("NumpadEnter", 0xE01C, &us), Some(0x0D), "the keypad's Enter is Enter");
        assert_eq!(cemu_key("CapsLock", 0x3A, &us), None, "a key the layout has no code for");
        assert_eq!(us_keys.number("CapsLock"), None);

        assert_eq!(us_keys.number("MouseMoveUp"), Some(0x26), "the mouse moving is the arrows");
        assert_eq!(us_keys.number("MouseMoveLeft"), Some(0x25));
        assert_eq!(us_keys.number("MouseLeft"), None, "Cemu reads no mouse button");
        assert_eq!(us_keys.number("WheelUp"), None);
    }

    #[test]
    fn windows_gives_the_codes_cemu_keeps() {
        // The same on every keyboard layout.
        assert_eq!(layout_vk(0x39), 0x20, "Space");
        assert_eq!(layout_vk(0x2A), 160, "left Shift");
        assert_eq!(layout_vk(0xE048), u32::from(VK_UP.0), "the up arrow, an extended key");
        assert_eq!(layout_vk(0xE01C), 0x0D, "the keypad's Enter");
    }

    #[test]
    fn a_keyboard_player_one_is_cemus_keyboard_as_the_gamepad() {
        let entries: String = [
            (1, b'E'), (2, b' '), (3, b'K'), (4, b'J'), (5, b'Q'), (6, b'R'), (7, b'F'), (8, b'G'), (9, 0x0D),
            (10, 0x08), (11, b'1'), (12, b'3'), (13, b'4'), (14, b'2'), (15, 160), (16, b'C'), (17, b'W'), (18, b'S'),
            (19, b'A'), (20, b'D'), (21, 0x26), (22, 0x28), (23, 0x25), (24, 0x27),
        ]
        .iter()
        .map(|(mapping, button)| {
            format!("\t\t\t<entry>\n\t\t\t\t<mapping>{mapping}</mapping>\n\t\t\t\t<button>{button}</button>\n\t\t\t</entry>\n")
        })
        .collect();
        let expected = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<emulated_controller>\n\t<type>Wii U GamePad</type>\n\t<controller>\n\
             \t\t<api>Keyboard</api>\n\t\t<uuid>keyboard</uuid>\n\t\t<display_name>Keyboard</display_name>\n\
             \t\t<axis>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</axis>\n\
             \t\t<rotation>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</rotation>\n\
             \t\t<trigger>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</trigger>\n\
             \t\t<mappings>\n{entries}\t\t</mappings>\n\t</controller>\n</emulated_controller>\n"
        );
        // The face buttons take J, K and E, beside the mouse buttons Cemu
        // can't read; the right stick is on the arrows; Guide has no key.
        let text = profile(Kind::for_player(0), &Player::on(Pad::keyboard()), Some(&keyboard_on(us)), false);
        assert_eq!(text, expected);
    }

    #[test]
    fn a_keyboard_player_gets_one_key_per_button_the_first_cemu_can_read() {
        let buttons = BTreeMap::from([
            ("West".to_string(), "MouseRight KeyK KeyJ".to_string()),
            ("East".to_string(), "MouseLeft".to_string()),
            ("RS Y+".to_string(), "KeyI MouseMoveUp".to_string()),
        ]);
        let player = Player::with_buttons(Pad::keyboard(), buttons);
        let text = profile(Kind::for_player(1), &player, Some(&keyboard_on(german)), false);
        assert!(text.contains("<type>Wii U Pro Controller</type>"), "{text}");
        assert!(text.contains(&entry(4, u64::from(b'K'))), "Y on K, its first key: {text}");
        assert_eq!(text.matches("<mapping>4</mapping>").count(), 1, "{text}");
        assert!(!text.contains("<mapping>1</mapping>"), "A, on a mouse button alone, is left out: {text}");
        assert!(text.contains(&entry(22, u64::from(b'I'))), "right stick up on I: {text}");
        assert_eq!(text.matches("<entry>").count(), 23, "{text}");
    }

    #[test]
    fn trap_team_on_the_keyboard_takes_the_d_pad_from_w_a_s_d_and_a_pad_beside_it_stays_as_it_was() {
        let dir = scratch("keyboard");
        let players = vec![Player::on(Pad::keyboard()), Player::on(xinput(1))];
        let readers = vec![Some(keyboard_on(us)), Some(Reader::XInput(0))];
        write_all(&dir, &players, &readers).unwrap();
        let pro = std::fs::read_to_string(dir.join(FIRST_AS_PRO)).unwrap();
        assert!(pro.contains("<type>Wii U Pro Controller</type>") && pro.contains("<api>Keyboard</api>"), "{pro}");
        // D-pad up, numbered after Home, is W, the left stick pushed up, and
        // W still pushes the stick too.
        assert!(pro.contains(&entry(12, u64::from(b'W'))), "d-pad up: {pro}");
        assert!(pro.contains(&entry(13, u64::from(b'S'))), "d-pad down: {pro}");
        assert!(pro.contains(&entry(14, u64::from(b'A'))), "d-pad left: {pro}");
        assert!(pro.contains(&entry(15, u64::from(b'D'))), "d-pad right: {pro}");
        assert!(pro.contains(&entry(18, u64::from(b'W'))), "stick up: {pro}");
        assert!(pro.contains(&entry(22, 0x26)), "right stick up on the up arrow: {pro}");
        assert!(!pro.contains(&format!("<button>{}</button>", b'1')), "1 to 4 have no button here: {pro}");
        let gamepad = std::fs::read_to_string(dir.join(FIRST_AS_GAMEPAD)).unwrap();
        assert!(gamepad.contains(&entry(11, u64::from(b'1'))), "as the GamePad the d-pad is 1 to 4: {gamepad}");
        assert_eq!(gamepad, std::fs::read_to_string(dir.join("controller0.xml")).unwrap());

        let two = std::fs::read_to_string(dir.join("controller1.xml")).unwrap();
        let alone = profile(Kind::Pro, &Player::on(xinput(1)), Some(&Reader::XInput(0)), false);
        assert_eq!(two, alone, "player 2's Xbox pad is written as it always was");
        assert!(!two.contains("<axis>"), "{two}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_keyboard_player_one_is_never_stood_in_for_nor_warned_about() {
        let players = vec![Player::on(Pad::keyboard()), Player::on(xinput(2))];
        let usable = usable_with(&[]);
        assert_eq!(stand_in(&players, &[xinput(1), xinput(2)], &usable), players);
        assert_eq!(missing(&players, &[], &usable), None, "with no pad plugged in at all");
        let reader = reader_for(&players[0], &[], &no_ids);
        assert!(matches!(reader, Some(Reader::Keyboard(_))), "{reader:?}");
    }

    #[test]
    fn a_keyboard_player_is_cemus_keyboard_while_omoio_serves_no_pad() {
        assert_eq!(crate::dsu::port(), None);
        let reader = reader_for(&Player::on(Pad::keyboard()), &[], &no_ids);
        assert!(matches!(reader, Some(Reader::Keyboard(_))), "{reader:?}");
    }

    #[test]
    fn a_keyboard_player_one_is_omoios_pad_as_the_gamepad_while_it_is_served() {
        // Every place on the pad's button or stick direction for it: Cross
        // for B at the bottom, Circle for A on the right, Square for Y and
        // Triangle for X, the shoulders and triggers, Options and Share, the
        // d-pad, and the sticks with up positive.
        let entries: String = [
            (1, 13), (2, 14), (3, 12), (4, 15), (5, 10), (6, 11), (7, 8), (8, 9), (9, 3), (10, 0), (11, 4), (12, 6),
            (13, 7), (14, 5), (15, 1), (16, 2), (17, 39), (18, 45), (19, 44), (20, 38), (21, 41), (22, 47), (23, 46),
            (24, 40),
        ]
        .iter()
        .map(|(mapping, button)| {
            format!("\t\t\t<entry>\n\t\t\t\t<mapping>{mapping}</mapping>\n\t\t\t\t<button>{button}</button>\n\t\t\t</entry>\n")
        })
        .collect();
        let expected = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<emulated_controller>\n\t<type>Wii U GamePad</type>\n\t<controller>\n\
             \t\t<api>DSUController</api>\n\t\t<uuid>0</uuid>\n\t\t<display_name>Controller 1</display_name>\n\
             \t\t<motion>true</motion>\n\
             \t\t<axis>\n\t\t\t<deadzone>0</deadzone>\n\t\t\t<range>1</range>\n\t\t</axis>\n\
             \t\t<rotation>\n\t\t\t<deadzone>0</deadzone>\n\t\t\t<range>1</range>\n\t\t</rotation>\n\
             \t\t<trigger>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</trigger>\n\
             \t\t<ip>127.0.0.1</ip>\n\t\t<port>26762</port>\n\
             \t\t<mappings>\n{entries}\t\t</mappings>\n\t</controller>\n</emulated_controller>\n"
        );
        let text = profile(Kind::for_player(0), &Player::on(Pad::keyboard()), Some(&Reader::Dsu(26762)), false);
        assert_eq!(text, expected);
    }

    #[test]
    fn trap_team_on_omoios_pad_takes_the_d_pad_from_the_left_stick_and_a_pad_beside_it_stays() {
        let dir = scratch("dsu");
        let buttons = BTreeMap::from([("Guide".to_string(), "KeyG".to_string()), ("LB".to_string(), String::new())]);
        let players = vec![Player::with_buttons(Pad::keyboard(), buttons), Player::on(xinput(1))];
        let readers = vec![Some(Reader::Dsu(26762)), Some(Reader::XInput(0))];
        write_all(&dir, &players, &readers).unwrap();
        let pro = std::fs::read_to_string(dir.join(FIRST_AS_PRO)).unwrap();
        assert!(pro.contains("<type>Wii U Pro Controller</type>") && pro.contains("<api>DSUController</api>"), "{pro}");
        assert!(pro.contains(&entry(12, 39)), "d-pad up, after Home, is the left stick up: {pro}");
        assert!(pro.contains(&entry(18, 39)), "and the stick still steers: {pro}");
        assert!(!pro.contains("<mapping>5</mapping>"), "L, with no key on it, has no entry: {pro}");
        assert_eq!(pro.matches("<entry>").count(), 23, "{pro}");
        let two = std::fs::read_to_string(dir.join("controller1.xml")).unwrap();
        assert_eq!(two, profile(Kind::Pro, &Player::on(xinput(1)), Some(&Reader::XInput(0)), false));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hid_devices_are_only_looked_at_for_a_pad_gilrs_reads() {
        let players = vec![Player::on(xinput(1)), Player::on(xinput(2))];
        assert!(hid_for(&players, Some("2.6"), not_looked_for).is_empty());
        let later = vec![Player::on(dualsense())];
        assert!(hid_for(&later, Some("2.7"), not_looked_for).is_empty(), "nor for a Cemu it would not name");
    }
}
