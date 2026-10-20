//! Dolphin's side of the controller layout: the GameCube's pads in
//! GCPadNew.ini and the Wii's remotes in WiimoteNew.ini, written from the
//! layout Omoio keeps (core/pad_layout.rs).
//!
//! Dolphin binds nothing but the keyboard by itself (`LoadDefaults` in
//! Core/HW/GCPadEmu.cpp and WiimoteEmu.cpp), so a game started with a pad
//! plugged in hears nothing until a pad is set up. Every name here was read
//! in Dolphin 2609a's source on 8 October 2026:
//!
//! - A pad is named `<source>/<id>/<name>` and found again by exactly that
//!   (`ciface::Core::DeviceQualifier`, InputCommon/ControllerInterface/
//!   CoreDevice.cpp). A pad not plugged in is waited for: its buttons answer
//!   from the moment it arrives.
//! - An Xbox-style pad is read through XInput, `XInput/<slot>/Gamepad`, the
//!   slot from 0 (XInput/XInput.cpp), the same slots Omoio's own players
//!   are kept on.
//! - Any other pad goes through SDL 3, named as SDL names it, numbered from
//!   0 among pads of the same name. SDL's own drivers name the PlayStation
//!   and Switch Pro pads (`SDL_hidapi_ps5.c`, `SDL_hidapi_ps4.c`,
//!   `SDL_hidapi_switch.c` at the SDL Dolphin builds with). A pad whose name
//!   SDL would give some other way is left without one: a guessed name
//!   would bind nothing.
//! - Buttons, sticks and triggers have the same names through XInput and
//!   SDL (`Button A`, `Shoulder L`, `Left Y+`, `Pad N`), with SDL's face
//!   buttons named by where they sit, as Omoio's places are (`SDLGamepad.h`,
//!   whose `Button A` is the bottom one).
//!
//! A game's own layout goes in a profile of Dolphin's, which the game's own
//! settings file names (`[Controls]` in User/GameSettings, read by
//! `InputConfig::LoadConfig`).
//!
//! A player on the keyboard and mouse is bound to Dolphin's one device for
//! both, `DInput/0/Keyboard Mouse` (`KeyboardMouse::GetName` and
//! `DINPUT_SOURCE_NAME`, InputCommon/ControllerInterface/DInput/
//! DInputKeyboardMouse.cpp lines 272 to 280 and DInput.h, Dolphin 2609a,
//! read 20 October 2026), always there, so never waited for nor stood in
//! for (`stand_in`).

use super::ini;
use crate::core::console::Console;
use crate::core::library::Library;
use crate::core::pad_layout::{self, Pad, Player, PLAYERS};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

/// What each place on a pad is on the GameCube's pad, after Dolphin's own
/// profile for a gamepad (`Sys/Profiles/GCPad/SDL Gamepad.ini` in the
/// release): A, B, X and Y by their letters, Z on the right shoulder, the
/// triggers on the triggers. Places it leaves out stay unused.
pub const GAMECUBE: [(&str, &str); 20] = [
    ("South", "A"),
    ("East", "B"),
    ("West", "X"),
    ("North", "Y"),
    ("RB", "Z"),
    ("LT", "L"),
    ("RT", "R"),
    ("Start", "Start"),
    ("Up", "D-pad up"),
    ("Down", "D-pad down"),
    ("Left", "D-pad left"),
    ("Right", "D-pad right"),
    ("LS Y+", "Control stick up"),
    ("LS Y-", "Control stick down"),
    ("LS X-", "Control stick left"),
    ("LS X+", "Control stick right"),
    ("RS Y+", "C-stick up"),
    ("RS Y-", "C-stick down"),
    ("RS X-", "C-stick left"),
    ("RS X+", "C-stick right"),
];

/// What each place is on a Wii Remote with a Nunchuk, the way most Wii
/// games are played and the way Skylanders on the Wii must be (GameTDB
/// lists the Nunchuk as required for Spyro's Adventure, Giants, SWAP Force
/// and Trap Team). Dolphin has no layout of its own for a gamepad here, so
/// this is Omoio's: the Nunchuk's stick on the left stick, the remote's A
/// and B on the bottom and right buttons as the Wii's menus use them, the
/// Nunchuk's Z on the left trigger, where the finger holds it on the real
/// thing, the Nunchuk's C and Z on the other two face buttons, and the
/// pointer on the right stick. The two moves of the remote that games ask
/// for most sit on the right shoulder and trigger: a shake on RB, and a push
/// of the remote towards the screen on RT. Skylanders on the Wii asks for
/// one ("Move!") to open a chest or put a key in a lock: "shaking to open
/// chests and thrusting forward to insert a key into a door" (Nintendo
/// World Report's review of Spyro's Adventure on the Wii, October 2011),
/// and a shake opens a lock too. The push is Dolphin's swing forward: a
/// quick 50 cm jab, then a slow way back that no game takes for a second
/// move (`EmulateSwing`, Core/HW/WiimoteEmu/Dynamics.cpp). The home button
/// stays Omoio's, for the portal menu, and the Wii's Home is left off: on a
/// stick's press it came up whenever a player leant on the pointer, stopping
/// the game.
pub const WII: [(&str, &str); 23] = [
    ("South", "A"),
    ("East", "B"),
    ("West", "Nunchuk Z"),
    ("LT", "Nunchuk Z"),
    ("North", "Nunchuk C"),
    ("RB", "Shake"),
    ("RT", "Push forward"),
    ("LB", "1"),
    ("LS", "2"),
    ("Back", "Minus"),
    ("Start", "Plus"),
    ("Up", "D-pad up"),
    ("Down", "D-pad down"),
    ("Left", "D-pad left"),
    ("Right", "D-pad right"),
    ("LS Y+", "Nunchuk stick up"),
    ("LS Y-", "Nunchuk stick down"),
    ("LS X-", "Nunchuk stick left"),
    ("LS X+", "Nunchuk stick right"),
    ("RS Y+", "Pointer up"),
    ("RS Y-", "Pointer down"),
    ("RS X-", "Pointer left"),
    ("RS X+", "Pointer right"),
];

pub fn button_names(console: Console) -> &'static [(&'static str, &'static str)] {
    match console {
        Console::GameCube => &GAMECUBE,
        _ => &WII,
    }
}

/// Each of Dolphin's GameCube pad settings, and the places on a player's pad
/// it answers to, as `GAMECUBE` lays them out.
const GAMECUBE_KEYS: [(&str, &[&str]); 22] = [
    ("Buttons/A", &["South"]),
    ("Buttons/B", &["East"]),
    ("Buttons/X", &["West"]),
    ("Buttons/Y", &["North"]),
    ("Buttons/Z", &["RB"]),
    ("Buttons/Start", &["Start"]),
    ("Main Stick/Up", &["LS Y+"]),
    ("Main Stick/Down", &["LS Y-"]),
    ("Main Stick/Left", &["LS X-"]),
    ("Main Stick/Right", &["LS X+"]),
    ("C-Stick/Up", &["RS Y+"]),
    ("C-Stick/Down", &["RS Y-"]),
    ("C-Stick/Left", &["RS X-"]),
    ("C-Stick/Right", &["RS X+"]),
    ("Triggers/L", &["LT"]),
    ("Triggers/R", &["RT"]),
    ("Triggers/L-Analog", &["LT"]),
    ("Triggers/R-Analog", &["RT"]),
    ("D-Pad/Up", &["Up"]),
    ("D-Pad/Down", &["Down"]),
    ("D-Pad/Left", &["Left"]),
    ("D-Pad/Right", &["Right"]),
];

/// The same for the Wii Remote and its Nunchuk, as `WII` lays them out.
const WII_KEYS: [(&str, &[&str]); 24] = [
    ("Buttons/A", &["South"]),
    ("Buttons/B", &["East"]),
    ("Buttons/1", &["LB"]),
    ("Buttons/2", &["LS"]),
    ("Buttons/-", &["Back"]),
    ("Buttons/+", &["Start"]),
    ("D-Pad/Up", &["Up"]),
    ("D-Pad/Down", &["Down"]),
    ("D-Pad/Left", &["Left"]),
    ("D-Pad/Right", &["Right"]),
    ("IR/Up", &["RS Y+"]),
    ("IR/Down", &["RS Y-"]),
    ("IR/Left", &["RS X-"]),
    ("IR/Right", &["RS X+"]),
    ("Shake/X", &["RB"]),
    ("Shake/Y", &["RB"]),
    ("Shake/Z", &["RB"]),
    ("Swing/Forward", &["RT"]),
    ("Nunchuk/Buttons/C", &["North"]),
    ("Nunchuk/Buttons/Z", &["West", "LT"]),
    ("Nunchuk/Stick/Up", &["LS Y+"]),
    ("Nunchuk/Stick/Down", &["LS Y-"]),
    ("Nunchuk/Stick/Left", &["LS X-"]),
    ("Nunchuk/Stick/Right", &["LS X+"]),
];

/// Dolphin's name for an input on an XInput or SDL pad, by the place Omoio
/// calls it (`XInput.cpp` and `SDLGamepad.h`). The left and right sticks'
/// up is `+` in both.
fn control(place: &str) -> Option<&'static str> {
    Some(match place {
        "South" => "Button A",
        "East" => "Button B",
        "West" => "Button X",
        "North" => "Button Y",
        "LB" => "Shoulder L",
        "RB" => "Shoulder R",
        "LT" => "Trigger L",
        "RT" => "Trigger R",
        "LS" => "Thumb L",
        "RS" => "Thumb R",
        "Up" => "Pad N",
        "Down" => "Pad S",
        "Left" => "Pad W",
        "Right" => "Pad E",
        "Back" => "Back",
        "Start" => "Start",
        "Guide" => "Guide",
        "LS Y+" => "Left Y+",
        "LS Y-" => "Left Y-",
        "LS X-" => "Left X-",
        "LS X+" => "Left X+",
        "RS Y+" => "Right Y+",
        "RS Y-" => "Right Y-",
        "RS X-" => "Right X-",
        "RS X+" => "Right X+",
        _ => return None,
    })
}

/// What a Dolphin setting is bound to for this player: each place's input
/// on their pad, or each key and mouse input on the keyboard, joined with
/// `|` so any of them presses it (`TOK_OR`, the larger of the two, in
/// InputCommon/ControlReference/ExpressionParser.cpp lines 401 to 404,
/// Dolphin 2609a). Every name is in backticks, as Dolphin's own defaults
/// write them (`GCPad::LoadDefaults`, Core/HW/GCPadEmu.cpp lines 201 to
/// 260): a bare `1` would be read as a number.
fn expression(player: &Player, places: &[&str]) -> String {
    if player.pad.is_keyboard() {
        let mut bound: Vec<String> = Vec::new();
        for input in places.iter().flat_map(|place| player.keys(place)) {
            if let Some(name) = keyboard_input(input, player.mouse) {
                if !bound.contains(&name) {
                    bound.push(name);
                }
            }
        }
        return bound.join(" | ");
    }
    places
        .iter()
        .filter_map(|place| control(player.input(place)))
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Dolphin's device for the keyboard and mouse together.
const KEYBOARD_MOUSE: &str = "DInput/0/Keyboard Mouse";

/// Dolphin's names for the keys a player can be given, by DirectInput key
/// code (`named_keys`, InputCommon/ControllerInterface/DInput/NamedKeys.h,
/// Dolphin 2609a, read 20 October 2026; the codes are the `DIK_` values in
/// the Windows SDK's dinput.h, 10.0.26100.0). Dolphin reads the keyboard
/// through DirectInput's own key table (`c_dfDIKeyboard`,
/// DInputKeyboardMouse.cpp line 81), whose codes are the keys' scan codes,
/// so a key is named by where it sits whatever the keyboard layout, as
/// Omoio's keys are, and no layout is looked up.
const NAMED_KEYS: [(u8, &str); 80] = [
    (0x1E, "A"),
    (0x30, "B"),
    (0x2E, "C"),
    (0x20, "D"),
    (0x12, "E"),
    (0x21, "F"),
    (0x22, "G"),
    (0x23, "H"),
    (0x17, "I"),
    (0x24, "J"),
    (0x25, "K"),
    (0x26, "L"),
    (0x32, "M"),
    (0x31, "N"),
    (0x18, "O"),
    (0x19, "P"),
    (0x10, "Q"),
    (0x13, "R"),
    (0x1F, "S"),
    (0x14, "T"),
    (0x16, "U"),
    (0x2F, "V"),
    (0x11, "W"),
    (0x2D, "X"),
    (0x15, "Y"),
    (0x2C, "Z"),
    (0x02, "1"),
    (0x03, "2"),
    (0x04, "3"),
    (0x05, "4"),
    (0x06, "5"),
    (0x07, "6"),
    (0x08, "7"),
    (0x09, "8"),
    (0x0A, "9"),
    (0x0B, "0"),
    (0x0C, "MINUS"),
    (0x0D, "EQUALS"),
    (0x0E, "BACK"),
    (0x0F, "TAB"),
    (0x1A, "LBRACKET"),
    (0x1B, "RBRACKET"),
    (0x1C, "RETURN"),
    (0x27, "SEMICOLON"),
    (0x28, "APOSTROPHE"),
    (0x29, "GRAVE"),
    (0x2A, "LSHIFT"),
    (0x2B, "BACKSLASH"),
    (0x33, "COMMA"),
    (0x34, "PERIOD"),
    (0x35, "SLASH"),
    (0x36, "RSHIFT"),
    (0x39, "SPACE"),
    (0x3A, "CAPITAL"),
    (0x56, "OEM_102"),
    (0x37, "MULTIPLY"),
    (0x47, "NUMPAD7"),
    (0x48, "NUMPAD8"),
    (0x49, "NUMPAD9"),
    (0x4A, "SUBTRACT"),
    (0x4B, "NUMPAD4"),
    (0x4C, "NUMPAD5"),
    (0x4D, "NUMPAD6"),
    (0x4E, "ADD"),
    (0x4F, "NUMPAD1"),
    (0x50, "NUMPAD2"),
    (0x51, "NUMPAD3"),
    (0x52, "NUMPAD0"),
    (0x53, "DECIMAL"),
    (0x9C, "NUMPADENTER"),
    (0xB5, "DIVIDE"),
    (0xC7, "HOME"),
    (0xC8, "UP"),
    (0xC9, "PRIOR"),
    (0xCB, "LEFT"),
    (0xCD, "RIGHT"),
    (0xCF, "END"),
    (0xD0, "DOWN"),
    (0xD1, "NEXT"),
    (0xD3, "DELETE"),
];

/// Dolphin's name for the key with this scan code (`keys::KEYS`). An
/// extended key, 0xE0 in the high byte, is its low byte with the top bit
/// set in DirectInput's codes (`DIK_NUMPADENTER` 0x9C for 0xE0 0x1C, in
/// dinput.h).
fn key_name(scan: u16) -> Option<&'static str> {
    let code = match scan >> 8 {
        0 if scan < 0x80 => scan as u8,
        0xE0 if scan & 0xFF < 0x80 => (scan & 0xFF) as u8 | 0x80,
        _ => return None,
    };
    NAMED_KEYS.iter().find(|(named, _)| *named == code).map(|(_, name)| *name)
}

/// How many counts the mouse moves for a stick to go all the way over at
/// Omoio's own speed, 100 percent: Dolphin's own, the eight it divides the
/// mouse's movement by for its `Axis` inputs (`MOUSE_AXIS_SENSITIVITY`,
/// DInputKeyboardMouse.cpp lines 16 to 19, Dolphin 2609a).
const COUNTS_FOR_FULL_STICK: f64 = 8.0;

/// A Dolphin expression for one of the keyboard player's inputs, `None` for
/// one Omoio doesn't know. `mouse` is the player's mouse speed in percent.
///
/// - The mouse's buttons are `Click 0` to `Click 4`, by their place in
///   DirectInput's mouse state (`KeyboardMouse::Button::GetName`,
///   DInputKeyboardMouse.cpp lines 299 to 302): the left as the Wii Remote's
///   A and the right as its B in Dolphin's own layout, the middle as its
///   shake (`Wiimote::LoadDefaults`, Core/HW/WiimoteEmu/WiimoteEmu.cpp lines
///   722 to 747). The side buttons follow as 3 and 4.
/// - The mouse moving is `RelativeMouse`, the counts it moved since the
///   game last asked (`RelativeMouseAxis` and `RelativeInputState`,
///   DInputKeyboardMouse.cpp lines 27 to 49 and ControllerInterface.h lines
///   147 to 176). Dolphin's `Axis` inputs read the same mouse, but share
///   each reading with its hotkey thread, which reads every device twice
///   every 5 ms (`HotkeyScheduler::Run`, DolphinQt/HotkeyScheduler.cpp lines
///   143 to 156), so the game would be left the scraps. Each count is an
///   eighth of a stick at 100 percent (`COUNTS_FOR_FULL_STICK`), no more
///   than the whole stick (`min`), and `smooth` lets it fall back over 50 ms
///   rather than at once, so a reading the mouse had nothing new for doesn't
///   drop the stick for a moment (`SmoothExpression`, InputCommon/
///   ControlReference/FunctionExpression.cpp lines 386 to 428). It rises
///   within 10 ms; not 0, which would divide by nothing when two readings
///   come at the same instant. Up is the mouse moving away, `Y-`, as screen
///   rows count down (Dolphin's own pointer up is `Cursor Y-`).
/// - The wheel is `RelativeMouse Z`, a press of a tenth of a second for
///   each notch (`PulseExpression`, FunctionExpression.cpp lines 585 to
///   635): a notch alone is gone by the next reading, which a game reading
///   its buttons once a frame could miss.
fn keyboard_input(input: &str, mouse: u32) -> Option<String> {
    if let Some(name) = crate::core::keys::scan_code(input).and_then(key_name) {
        return Some(format!("`{name}`"));
    }
    let moved = |axis: &str| {
        let scale = f64::from(mouse) / (100.0 * COUNTS_FOR_FULL_STICK);
        format!("smooth(min(`RelativeMouse {axis}` * {scale}, 1), 0.01, 0.05)")
    };
    Some(match input {
        "MouseLeft" => "`Click 0`".to_string(),
        "MouseRight" => "`Click 1`".to_string(),
        "MouseMiddle" => "`Click 2`".to_string(),
        "MouseBack" => "`Click 3`".to_string(),
        "MouseForward" => "`Click 4`".to_string(),
        "WheelUp" => "pulse(`RelativeMouse Z+`, 0.1)".to_string(),
        "WheelDown" => "pulse(`RelativeMouse Z-`, 0.1)".to_string(),
        "MouseMoveLeft" => moved("X-"),
        "MouseMoveRight" => moved("X+"),
        "MouseMoveUp" => moved("Y-"),
        "MouseMoveDown" => moved("Y+"),
        _ => return None,
    })
}

/// The Wii Remote's pointer for a keyboard player: the mouse cursor over
/// the game, as in Dolphin's own layout (`Wiimote::LoadDefaults`,
/// WiimoteEmu.cpp lines 750 to 753), whatever the player's right stick
/// says, since pointing the remote is what the mouse is for on the Wii.
/// The right stick's places then have nothing on the Wii: the pad layout
/// puts only the pointer there (`WII`), and a key on the pointer would
/// throw it to the edge of the screen for as long as it was held.
fn pointer(key: &str) -> Option<&'static str> {
    Some(match key {
        "IR/Up" => "`Cursor Y-`",
        "IR/Down" => "`Cursor Y+`",
        "IR/Left" => "`Cursor X-`",
        "IR/Right" => "`Cursor X+`",
        _ => return None,
    })
}

/// SDL's name for a pad by its USB vendor and product ids, for the pads its
/// own drivers name (SDL 3, as Dolphin 2609a builds it).
fn sdl_name(vendor: u16, product: u16) -> Option<&'static str> {
    Some(match (vendor, product) {
        (0x054C, 0x0CE6) => "DualSense Wireless Controller",
        (0x054C, 0x0DF2) => "DualSense Edge Wireless Controller",
        (0x054C, 0x05C4 | 0x09CC) => "PS4 Controller",
        (0x057E, 0x2009) => "Nintendo Switch Pro Controller",
        _ => return None,
    })
}

/// Whether SDL gives pads of these two USB vendor and product ids the same
/// name. Dolphin numbers a pad among the pads of its source and name that
/// came before it (`ControllerInterface::AddDevice`, InputCommon/
/// ControllerInterface/ControllerInterface.cpp), so the first DualShock 4
/// and the second model of it are `SDL/0` and `SDL/1` of "PS4 Controller".
fn same_sdl_name(a: (u16, u16), b: (u16, u16)) -> bool {
    sdl_name(a.0, a.1).is_some_and(|name| sdl_name(b.0, b.1) == Some(name))
}

/// How Dolphin numbers the pads: a pad's USB vendor and product ids and how
/// many pads SDL names the same come before it.
fn usb_ids(device: &str) -> Option<(u16, u16, usize)> {
    crate::pads::usb_ids_counting(device, same_sdl_name)
}

/// A pad's USB vendor and product ids and how many of the same name come
/// before it, as `usb_ids` gives them.
type UsbIds<'a> = &'a dyn Fn(&str) -> Option<(u16, u16, usize)>;

/// The XInput slot a pad sits in, counted from 0.
fn xinput_slot(pad: &Pad) -> Option<usize> {
    pad.device
        .strip_prefix("XInput Pad #")?
        .parse::<usize>()
        .ok()?
        .checked_sub(1)
        .filter(|slot| *slot < 4)
}

/// How Dolphin knows this pad, or `None` for one it can't be told about.
fn device(pad: &Pad, ids: UsbIds) -> Option<String> {
    if pad.is_keyboard() {
        return Some(KEYBOARD_MOUSE.to_string());
    }
    if let Some(slot) = xinput_slot(pad) {
        return Some(format!("XInput/{slot}/Gamepad"));
    }
    let (vendor, product, before) = ids(&pad.device)?;
    Some(format!("SDL/{before}/{}", sdl_name(vendor, product)?))
}

/// One player's settings for Dolphin, the lines of their section.
fn lines(console: Console, player: &Player, device: Option<&str>, emulated: bool) -> Vec<String> {
    let keyboard = player.pad.is_keyboard();
    let mut lines = vec![format!("Device = {}", device.unwrap_or(""))];
    let keys: &[(&str, &[&str])] = match console {
        Console::GameCube => &GAMECUBE_KEYS,
        _ => &WII_KEYS,
    };
    for (key, places) in keys {
        let bound = match pointer(key).filter(|_| keyboard) {
            Some(cursor) => cursor.to_string(),
            None => expression(player, places),
        };
        lines.push(format!("{key} = {bound}"));
    }
    match console {
        Console::GameCube => {
            // As Dolphin's own gamepad profile has them, so a stick reaches
            // its whole circle. The mouse turns a stick by degrees as a pad
            // does, so the keyboard player's are the same.
            lines.push("Main Stick/Calibration = 100.00".to_string());
            lines.push("C-Stick/Calibration = 100.00".to_string());
        }
        _ => {
            lines.push(format!("Source = {}", if emulated { 1 } else { 0 }));
            lines.push("Extension = Nunchuk".to_string());
            // The pointer moves with the stick and stays where it is left,
            // and goes out of sight when the stick is left alone. The mouse
            // cursor is a place on the screen rather than a push, so on the
            // keyboard the pointer is where the cursor is, as Dolphin has
            // it by itself (`Cursor::UpdateState`, InputCommon/
            // ControllerEmu/ControlGroup/Cursor.cpp lines 115 to 135), and
            // goes out of sight when the mouse is left alone, as with a pad.
            let relative = if keyboard { "False" } else { "True" };
            lines.push(format!("IR/Relative Input = {relative}"));
            lines.push("IR/Auto-Hide = True".to_string());
        }
    }
    // The keyboard and mouse have no motors to bind.
    if !keyboard {
        lines.push("Rumble/Motor = `Motor L` | `Motor R`".to_string());
    }
    lines
}

/// GCPadNew.ini or WiimoteNew.ini, `before`, with these players in it: each
/// player's section written afresh, but for those `left` to the user
/// (`left_alone`), and every other section as it was, such as the Balance
/// Board's.
fn file(
    console: Console,
    before: &str,
    players: &[Player],
    devices: &[Option<String>],
    emulated: &[bool],
    left: &[bool],
) -> String {
    let section = if console == Console::GameCube { "GCPad" } else { "Wiimote" };
    let sections: Vec<(String, Vec<String>)> = players
        .iter()
        .enumerate()
        .take(PLAYERS)
        .filter(|(at, _)| !left.get(*at).copied().unwrap_or(false))
        .map(|(at, player)| {
            let device = devices.get(at).and_then(Option::as_deref);
            let on = emulated.get(at).copied().unwrap_or(false);
            (format!("{section}{}", at + 1), lines(console, player, device, on))
        })
        .collect();
    ini::replace_sections(before, &sections)
}

/// Which players' places Omoio leaves as the user set them up in Dolphin,
/// from WiimoteNew.ini for the Wii or Dolphin.ini for the GameCube: a real
/// Wii Remote (`Source = 2`, `WiimoteSource::Real` in Core/HW/Wiimote.h and
/// `WIIMOTE_1_SOURCE` and on in Core/Config/WiimoteSettings.cpp), and a
/// GameCube port given a device other than a standard pad, such as a
/// steering wheel or a dance mat, whose buttons are set up for it
/// (`port_device`). Both are the user's own choice: Omoio only ever
/// switches a remote between emulated and none, and fills an empty port
/// with a standard pad (`standard_controllers`).
fn left_alone(console: Console, settings: &str) -> Vec<bool> {
    (0..PLAYERS)
        .map(|at| match console {
            Console::GameCube => ![0, STANDARD_PAD].contains(&port_device(settings, at)),
            _ => {
                let source = ini::get(settings, &format!("Wiimote{}", at + 1), "Source");
                source.and_then(|value| value.parse::<u32>().ok()) == Some(REAL_REMOTE)
            }
        })
        .collect()
}

/// `left_alone` from the file it is read from.
fn left_alone_in(console: Console, config: &Path) -> std::io::Result<Vec<bool>> {
    let name = if console == Console::GameCube { "Dolphin.ini" } else { "WiimoteNew.ini" };
    Ok(left_alone(console, &ini::read(&config.join(name))?))
}

/// Dolphin's number for a real Wii Remote as a player's source
/// (`WiimoteSource`, Core/HW/Wiimote.h).
const REAL_REMOTE: u32 = 2;

/// The same player in a profile of Dolphin's, for one game.
fn profile(console: Console, player: &Player, device: Option<&str>) -> String {
    let mut text = "[Profile]\r\n".to_string();
    for line in lines(console, player, device, true).into_iter().filter(|line| !line.starts_with("Source = ")) {
        text.push_str(&line);
        text.push_str("\r\n");
    }
    text
}

fn config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(super::install::user_dir(app)?.join("Config"))
}

/// The game's id as Dolphin files its settings, the six characters a disc
/// names itself by, from the library's id, which adds a second disc's number
/// as `D2` (mod.rs, `library_id`). `None` for an id of any other shape, such
/// as a PS3 or Wii U game's: a game's own layout is handed to every
/// emulator.
pub(super) fn game_id(title_id: &str) -> Option<&str> {
    let (id, disc) = title_id.split_at_checked(6)?;
    let is_disc = |number: &str| !number.is_empty() && number.bytes().all(|digit| digit.is_ascii_digit());
    let disc_ok = disc.is_empty() || disc.strip_prefix('D').is_some_and(is_disc);
    (disc_ok && id.bytes().all(|byte| byte.is_ascii_alphanumeric())).then_some(id)
}

/// Whether the library has `title_id` as one of `console`'s games. A Wii
/// game's id and a GameCube game's look alike, so the shape alone can't
/// keep one console's layout out of the other's files.
fn is_own_game(app: &AppHandle, console: Console, title_id: &str) -> bool {
    crate::commands::library_path(app).is_ok_and(|file| {
        Library::load(&file).games().iter().any(|game| game.title_id == title_id && game.console == console)
    })
}

fn profile_name(game: &str, player: usize) -> String {
    format!("Omoio {game} {}", player + 1)
}

/// Where Dolphin keeps profiles and the setting naming one, per console:
/// `Profiles/GCPad` and `PadProfile1` to 4, or `Profiles/Wiimote` and
/// `WiimoteProfile1` to 4 (`InputConfig::GetUserProfileDirectoryPath` and
/// `GetProfileKey`, the names given in Core/HW/GCPad.cpp and Wiimote.cpp).
fn profile_kind(console: Console) -> (&'static str, &'static str) {
    if console == Console::GameCube {
        ("GCPad", "Pad")
    } else {
        ("Wiimote", "Wiimote")
    }
}

/// Player 1 on a pad Dolphin can't be told about would leave every game
/// without anyone to answer it, so they play on an XInput pad that is
/// plugged in instead, keeping their own buttons: one no other player has,
/// else another player's, who takes player 1's pad in return. The same rule
/// Omoio follows for Cemu (backends/cemu/controllers.rs, `stand_in`).
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
    let free = xinput.clone().find(|pad| !players.iter().any(|p| p.pad.device == pad.device));
    if let Some(pad) = free.or_else(|| xinput.next()) {
        let (buttons, mouse) = (first.buttons.clone(), first.mouse);
        pad_layout::give(&mut players, 0, Player { pad: pad.clone(), buttons, mouse });
    }
    players
}

/// The file Dolphin keeps the layout for every game in, for `console`
/// (`InputConfig`'s file name, given in Core/HW/GCPad.cpp and
/// `WIIMOTE_INI_NAME` in Core/HW/Wiimote.h).
fn every_game_file(console: Console) -> &'static str {
    if console == Console::GameCube {
        "GCPadNew.ini"
    } else {
        "WiimoteNew.ini"
    }
}

/// Writes the players' layout into Dolphin's files for `console`. An empty
/// `title_id` is the layout for every game: GCPadNew.ini or WiimoteNew.ini.
/// A game's own goes in profiles its settings file names, for this
/// console's games only, since every emulator is handed it. A Wii Remote is
/// switched on for player 1, for every player whose pad is plugged in and
/// for the keyboard player; one switched on with nothing to answer it would
/// look to a game like a player who never presses anything. With a player
/// on the keyboard, Dolphin keeps the mouse cursor out of sight over the
/// game (settings.rs, `hold_the_cursor`). A real Wii Remote, the Balance Board
/// and a GameCube port given another device stay as the user set them up
/// in Dolphin (`file`, `left_alone`).
///
/// A game's own layout goes into the file for every game too, under its
/// profiles. Dolphin reads a game's profiles only on top of that file
/// (`InputConfig::LoadConfig`, InputCommon/InputConfig.cpp lines 70 to 118,
/// Dolphin 2609a, read 14 October 2026): a player without a profile Dolphin
/// can load takes their section of it, and when the file has no section at
/// all, as before Dolphin first started, no profile is read and player 1
/// gets Dolphin's own layout, the keyboard and mouse, with the left click
/// as the Wii Remote's A (`Wiimote::LoadDefaults`, Core/HW/WiimoteEmu/
/// WiimoteEmu.cpp lines 722 to 726). Dolphin saves that layout into the file
/// as its main window opens, before any game (`MainWindow::InitControllers`,
/// DolphinQt/MainWindow.cpp lines 389 to 393), so without Omoio's the file
/// holds the keyboard from Dolphin's first start on. With the players' pads
/// in it, a game whose profile isn't read answers the pad all the same. The
/// file is the layout for every game only between games: a game without a
/// layout of its own has it written afresh before it starts
/// (controllers.rs, `before_launch`).
pub fn write(app: &AppHandle, console: Console, title_id: &str, players: &[Player]) -> Result<(), String> {
    let config = config_dir(app)?;
    if !super::install::exe_path(app)?.is_file() {
        return Ok(());
    }
    let plugged = crate::pads::connected();
    let ids = |device: &str| usb_ids(device);
    let usable = |player: &Player| device(&player.pad, &ids).is_some();
    let players = stand_in(players, &plugged, &usable);
    let devices: Vec<Option<String>> = players.iter().map(|player| device(&player.pad, &ids)).collect();
    let emulated: Vec<bool> = players
        .iter()
        .enumerate()
        .map(|(at, player)| {
            at == 0 || player.pad.is_keyboard() || plugged.iter().any(|pad| pad.device == player.pad.device)
        })
        .collect();
    let failed = |_| "Couldn't save the controller settings for Dolphin.".to_string();
    // A file that can't be read is left as it is rather than written over
    // (`ini::read`).
    let every_game = |left: &[bool]| -> Result<(), String> {
        let path = config.join(every_game_file(console));
        let before = ini::read(&path).map_err(failed)?;
        let after = file(console, &before, &players, &devices, &emulated, left);
        if after != before {
            ini::write(&path, &after).map_err(failed)?;
        }
        if console == Console::GameCube {
            standard_controllers(&config.join("Dolphin.ini")).map_err(failed)?;
        }
        let keyboard = players
            .iter()
            .enumerate()
            .take(PLAYERS)
            .any(|(at, player)| player.pad.is_keyboard() && !left.get(at).copied().unwrap_or(false));
        super::settings::hold_the_cursor(&config.join("Dolphin.ini"), keyboard).map_err(failed)
    };
    if title_id.is_empty() {
        let left = left_alone_in(console, &config).map_err(failed)?;
        return every_game(&left);
    }
    let Some(game) = game_id(title_id).filter(|_| is_own_game(app, console, title_id)) else {
        return Ok(());
    };
    let left = left_alone_in(console, &config).map_err(failed)?;
    let (folder, key) = profile_kind(console);
    let profiles = config.join("Profiles").join(folder);
    let mut settings: Vec<(String, String)> = Vec::new();
    for (at, player) in players.iter().enumerate().take(PLAYERS) {
        // Not even for one game: the game's settings would win over the
        // user's real remote or their device in that port.
        if left[at] {
            continue;
        }
        let name = profile_name(game, at);
        let text = profile(console, player, devices[at].as_deref());
        ini::write(&profiles.join(format!("{name}.ini")), &text).map_err(failed)?;
        settings.push((format!("{key}Profile{}", at + 1), name));
        if console != Console::GameCube {
            // Counted from 0 here (`GameConfigLoader.cpp`).
            settings.push((format!("WiimoteSource{at}"), if emulated[at] { "1" } else { "0" }.to_string()));
        }
    }
    let values: Vec<(&str, &str, &str)> = settings.iter().map(|(k, v)| ("Controls", k.as_str(), v.as_str())).collect();
    let game_settings = super::install::user_dir(app)?.join("GameSettings").join(format!("{game}.ini"));
    ini::update(&game_settings, &values).map_err(failed)?;
    // After the game's own, so a file for every game that can't be read
    // keeps nothing of the game's from being written.
    every_game(&left)
}

/// A standard GameCube pad in every port Dolphin has left empty, so a second
/// player's pad answers when it is plugged in. A port with a pad set to
/// nothing is reported to the game as unplugged (`GCPadEmu.cpp`), so a port
/// with no one on it is no different to a game. Dolphin numbers a standard
/// pad 6 and leaves ports 2 to 4 empty (`SIDevices`, Core/HW/SI/SI_Device.h;
/// `SIDevice0` to 3 in Core/Config/MainSettings.cpp). A port the user set
/// to something else is left as it is.
fn standard_controllers(dolphin_ini: &Path) -> std::io::Result<()> {
    let text = ini::read(dolphin_ini)?;
    let keys = ["SIDevice0", "SIDevice1", "SIDevice2", "SIDevice3"];
    let empty: Vec<(&str, &str, &str)> = keys
        .iter()
        .filter(|key| ini::get(&text, "Core", key).is_none_or(|value| value == "0"))
        .map(|key| ("Core", *key, "6"))
        .collect();
    ini::update(dolphin_ini, &empty)
}

/// Dolphin's number for a standard GameCube pad in a port (`SIDevices`,
/// Core/HW/SI/SI_Device.h).
const STANDARD_PAD: u32 = 6;

/// What is in a GameCube port as Dolphin reads Dolphin.ini: the number set
/// there, else its own default, a standard pad in the first port and
/// nothing in the others (`SIDevice0` to 3, Core/Config/MainSettings.cpp).
/// A value it can't read counts as the default too (`Config::GetUncached`,
/// Common/Config/Config.h).
fn port_device(dolphin_ini: &str, port: usize) -> u32 {
    let default = if port == 0 { STANDARD_PAD } else { 0 };
    ini::get(dolphin_ini, "Core", &format!("SIDevice{port}")).and_then(|value| value.parse().ok()).unwrap_or(default)
}

/// Dolphin's arguments that empty, for one Wii game's session, every
/// GameCube port with a standard pad in it. Omoio binds those to the
/// players' pads for GameCube games, so a Wii game that also reads GameCube
/// pads, as Mario Kart Wii and Super Smash Bros. Brawl do, would hear each
/// press twice: from Wii Remote 1 and from GameCube pad 1.
///
/// `--config` sets a value for that run alone (`CommandLineConfigLayerLoader`,
/// UICommon/CommandLineParse.cpp, whose `Save` writes nothing), in a layer
/// above Dolphin.ini and below a game's own settings (`SEARCH_ORDER`,
/// Common/Config/Enums.h). So nothing the user set up changes: a port they
/// gave another device, such as a GameCube adapter, keeps it, a game they
/// gave its own ports in Dolphin (`[Controls] PadType0`, read by
/// Core/ConfigLoaders/GameConfigLoader.cpp) keeps those, and their GameCube
/// games find their pads where they were.
pub fn gamecube_ports_off(dolphin_ini: &str) -> Vec<String> {
    (0..4)
        .filter(|&port| port_device(dolphin_ini, port) == STANDARD_PAD)
        .flat_map(|port| ["--config".to_string(), format!("Dolphin.Core.SIDevice{port}=0")])
        .collect()
}

/// Takes a game's own layout out of Dolphin: its profiles and the settings
/// naming them, so it goes back to the layout for every game. Only settings
/// that name Omoio's own profiles are taken out, so a profile the user chose
/// for the game in Dolphin stays, and so does everything of the other
/// console's, which is asked to forget the same id.
pub fn forget(app: &AppHandle, console: Console, title_id: &str) -> Result<(), String> {
    let Some(game) = game_id(title_id) else {
        return Ok(());
    };
    let (folder, _) = profile_kind(console);
    let user = super::install::user_dir(app)?;
    let profiles = user.join("Config").join("Profiles").join(folder);
    let game_settings = user.join("GameSettings").join(format!("{game}.ini"));
    let failed = |_| "Couldn't save the controller settings for Dolphin.".to_string();
    let before = ini::read(&game_settings).map_err(failed)?;
    let text = without_own_profiles(&before, console, game);
    for at in 0..PLAYERS {
        let _ = std::fs::remove_file(profiles.join(format!("{}.ini", profile_name(game, at))));
    }
    if text != before {
        ini::write(&game_settings, &text).map_err(failed)?;
    }
    Ok(())
}

/// A game's settings without the ones naming Omoio's profiles for it, nor
/// the Wii Remote's source Omoio set beside each.
fn without_own_profiles(text: &str, console: Console, game: &str) -> String {
    let (_, key) = profile_kind(console);
    let mut text = text.to_string();
    for at in 0..PLAYERS {
        let profile_key = format!("{key}Profile{}", at + 1);
        if ini::get(&text, "Controls", &profile_key).as_deref() != Some(profile_name(game, at).as_str()) {
            continue;
        }
        text = ini::remove(&text, "Controls", &profile_key);
        if console != Console::GameCube {
            text = ini::remove(&text, "Controls", &format!("WiimoteSource{at}"));
        }
    }
    text
}

/// Why player 1 would have no pad in Dolphin with these `players`, after
/// `stand_in` has had its go with the pads `connected`, worded for the
/// person about to press Play. `None` when they have one.
pub fn missing_first_player(players: &[Player], connected: &[Pad]) -> Option<String> {
    let ids = |device: &str| usb_ids(device);
    missing(players, connected, &|player: &Player| device(&player.pad, &ids).is_some())
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
        "Dolphin can't use player 1's controller, {}, so the game won't answer it. \
         Plug in an Xbox, PlayStation or Switch Pro controller, or choose another for player 1 on the Controller screen.",
        first.pad.name
    ))
}

/// The device each player was bound to as the game started, as Dolphin
/// picks it (`InputConfig::LoadConfig`, InputCommon/InputConfig.cpp lines 37
/// to 99, Dolphin 2609a, read 14 October 2026): the profile the game's
/// settings name for the player, the first of a list, else the player's
/// section of the file for every game. A profile that isn't there is
/// skipped with a warning, and the player takes their section. `None`
/// where Omoio can't tell:
///
/// - The file for every game has no section, so Dolphin took its own
///   layout for every player (`has_sections`).
/// - The profile named is one Omoio can't read, such as a folder of them,
///   whose first Dolphin takes (`GetProfilesFromSetting`, InputCommon/
///   InputProfile.cpp).
/// - A player without a profile after one with a profile: Dolphin reads
///   them from that profile's file, which has no section of theirs, and
///   their device stays as it was (line 87, and `EmulatedController::
///   LoadConfig`, InputCommon/ControllerEmu/ControllerEmu.cpp line 118).
///
/// `profile` gives a profile by its name: `None` when there is none, and
/// its text when Omoio can read it. Only the game's own
/// settings file in User/GameSettings is read, the one Omoio names profiles
/// in. Dolphin also reads the ones it ships and the user's for the game's
/// region and revision (`GetGameIniFilenames`, Core/ConfigLoaders/
/// GameConfigLoader.cpp). Omoio doesn't, and where only those name a
/// profile, takes the player's device from the file for every game.
fn bound_devices(
    console: Console,
    every_game: &str,
    game_settings: &str,
    profile: &dyn Fn(&str) -> Option<Option<String>>,
) -> Vec<Option<String>> {
    if !has_sections(every_game) {
        return vec![None; PLAYERS];
    }
    let (_, key) = profile_kind(console);
    let section = if console == Console::GameCube { "GCPad" } else { "Wiimote" };
    let mut after_profile = false;
    (0..PLAYERS)
        .map(|at| {
            let named = ini::get(game_settings, "Controls", &format!("{key}Profile{}", at + 1));
            let found = named.and_then(|setting| profile(setting.split(',').next().unwrap_or_default().trim()));
            let device = match found {
                Some(text) => {
                    after_profile = true;
                    text.and_then(|text| ini::get(&text, "Profile", "Device"))
                }
                None if after_profile => None,
                None => ini::get(every_game, &format!("{section}{}", at + 1), "Device"),
            };
            device.filter(|device| !device.is_empty())
        })
        .collect()
}

/// Whether Dolphin finds a section in this text: a line that opens with `[`
/// and has a `]` after it, the first line read past a UTF-8 byte order mark
/// (`IniFile::Load`, Common/IniFile.cpp lines 239 to 317, Dolphin 2609a).
/// A file without one is taken as missing (`InputConfig::LoadConfig`, line
/// 71).
fn has_sections(text: &str) -> bool {
    text.trim_start_matches('\u{feff}')
        .lines()
        .any(|line| line.starts_with('[') && line.contains(']'))
}

/// The first player, counted from 0, bound to a pad that is plugged in but
/// that Dolphin never `added`, and that pad's name. `plugged` is each pad
/// plugged in, by its name and the name Dolphin would know it by (`device`).
/// A player `left` as the user set them up, such as a real Wii Remote, is
/// not Omoio's to ask about.
fn unfound_player(
    bound: &[Option<String>],
    left: &[bool],
    added: &[&str],
    plugged: &[(String, Option<String>)],
) -> Option<(usize, String)> {
    bound.iter().enumerate().find_map(|(at, device)| {
        let device = device.as_deref()?;
        if left.get(at).copied().unwrap_or(false) || added.contains(&device) {
            return None;
        }
        plugged
            .iter()
            .find(|(_, known_as)| known_as.as_deref() == Some(device))
            .map(|(name, _)| (at, name.clone()))
    })
}

/// What Omoio says about a player whose pad Dolphin didn't find. `player`
/// counts from zero.
fn notice(player: usize, pad_name: &str) -> String {
    format!(
        "Dolphin couldn't find player {}'s controller, {pad_name}, so the game won't answer it. Start the game again. If that doesn't help, the log on the Logs page shows what Dolphin saw.",
        player + 1
    )
}

/// Why a player's pad, plugged in, isn't reaching the game started for
/// `title_id`: the device the player is bound to is one Omoio knows as a
/// pad plugged in now, and Dolphin's log of this game has no "Added device"
/// for it (log.rs, `added_devices`). A pad Dolphin names some other way than
/// Omoio told it, as a pad another program has taken over may be, is found
/// so. `None` when Dolphin found every pad plugged in, or the log has no
/// device at all, as when Dolphin hasn't logged its pads yet: Dolphin always
/// takes in the keyboard and mouse.
pub fn unfound(app: &AppHandle, console: Console, title_id: &str) -> Option<String> {
    let user = super::install::user_dir(app).ok()?;
    let log = std::fs::read(super::log::path(&user)).ok()?;
    let log = String::from_utf8_lossy(&log);
    let added = super::log::added_devices(&log);
    if added.is_empty() {
        return None;
    }
    let config = user.join("Config");
    let every_game = ini::read(&config.join(every_game_file(console))).ok()?;
    let game_settings = game_id(title_id)
        .and_then(|game| ini::read(&user.join("GameSettings").join(format!("{game}.ini"))).ok())
        .unwrap_or_default();
    let (folder, _) = profile_kind(console);
    let profiles = config.join("Profiles").join(folder);
    // Found as Dolphin finds it: a folder of that name, else the file with
    // ".ini" after it (`GetProfilesFromSetting`, InputCommon/
    // InputProfile.cpp lines 30 to 54). A folder isn't followed.
    let profile = |name: &str| {
        let path = profiles.join(format!("{name}.ini"));
        if !name.is_empty() && profiles.join(name).is_dir() {
            Some(None)
        } else if !name.is_empty() && path.is_file() {
            Some(ini::read(&path).ok())
        } else {
            None
        }
    };
    let bound = bound_devices(console, &every_game, &game_settings, &profile);
    let left = left_alone_in(console, &config).ok()?;
    let ids = |device: &str| usb_ids(device);
    let plugged: Vec<(String, Option<String>)> = crate::pads::connected()
        .iter()
        .map(|pad| (pad.name.clone(), device(pad, &ids)))
        .collect();
    let (at, name) = unfound_player(&bound, &left, &added, &plugged)?;
    Some(notice(at, &name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn pad(device: &str, handler: &str) -> Pad {
        Pad {
            device: device.to_string(),
            name: device.to_string(),
            handler: handler.to_string(),
            family: "generic".to_string(),
        }
    }

    fn no_ids(_: &str) -> Option<(u16, u16, usize)> {
        None
    }

    #[test]
    fn an_xbox_pad_on_the_wii_is_a_remote_with_a_nunchuk() {
        let player = Player::on(crate::pads::xinput_slot(0));
        let devices = [device(&player.pad, &no_ids)];
        let text = file(Console::Wii, "", &[player], &devices, &[true], &[]);
        assert_eq!(
            text,
            "[Wiimote1]\r\n\
             Device = XInput/0/Gamepad\r\n\
             Buttons/A = `Button A`\r\n\
             Buttons/B = `Button B`\r\n\
             Buttons/1 = `Shoulder L`\r\n\
             Buttons/2 = `Thumb L`\r\n\
             Buttons/- = `Back`\r\n\
             Buttons/+ = `Start`\r\n\
             D-Pad/Up = `Pad N`\r\n\
             D-Pad/Down = `Pad S`\r\n\
             D-Pad/Left = `Pad W`\r\n\
             D-Pad/Right = `Pad E`\r\n\
             IR/Up = `Right Y+`\r\n\
             IR/Down = `Right Y-`\r\n\
             IR/Left = `Right X-`\r\n\
             IR/Right = `Right X+`\r\n\
             Shake/X = `Shoulder R`\r\n\
             Shake/Y = `Shoulder R`\r\n\
             Shake/Z = `Shoulder R`\r\n\
             Swing/Forward = `Trigger R`\r\n\
             Nunchuk/Buttons/C = `Button Y`\r\n\
             Nunchuk/Buttons/Z = `Button X` | `Trigger L`\r\n\
             Nunchuk/Stick/Up = `Left Y+`\r\n\
             Nunchuk/Stick/Down = `Left Y-`\r\n\
             Nunchuk/Stick/Left = `Left X-`\r\n\
             Nunchuk/Stick/Right = `Left X+`\r\n\
             Source = 1\r\n\
             Extension = Nunchuk\r\n\
             IR/Relative Input = True\r\n\
             IR/Auto-Hide = True\r\n\
             Rumble/Motor = `Motor L` | `Motor R`\r\n"
        );
    }

    #[test]
    fn an_xbox_pad_on_the_gamecube_follows_dolphins_own_gamepad_profile() {
        let player = Player::on(crate::pads::xinput_slot(1));
        let devices = [device(&player.pad, &no_ids)];
        let text = file(Console::GameCube, "", &[player], &devices, &[true], &[]);
        assert_eq!(
            text,
            "[GCPad1]\r\n\
             Device = XInput/1/Gamepad\r\n\
             Buttons/A = `Button A`\r\n\
             Buttons/B = `Button B`\r\n\
             Buttons/X = `Button X`\r\n\
             Buttons/Y = `Button Y`\r\n\
             Buttons/Z = `Shoulder R`\r\n\
             Buttons/Start = `Start`\r\n\
             Main Stick/Up = `Left Y+`\r\n\
             Main Stick/Down = `Left Y-`\r\n\
             Main Stick/Left = `Left X-`\r\n\
             Main Stick/Right = `Left X+`\r\n\
             C-Stick/Up = `Right Y+`\r\n\
             C-Stick/Down = `Right Y-`\r\n\
             C-Stick/Left = `Right X-`\r\n\
             C-Stick/Right = `Right X+`\r\n\
             Triggers/L = `Trigger L`\r\n\
             Triggers/R = `Trigger R`\r\n\
             Triggers/L-Analog = `Trigger L`\r\n\
             Triggers/R-Analog = `Trigger R`\r\n\
             D-Pad/Up = `Pad N`\r\n\
             D-Pad/Down = `Pad S`\r\n\
             D-Pad/Left = `Pad W`\r\n\
             D-Pad/Right = `Pad E`\r\n\
             Main Stick/Calibration = 100.00\r\n\
             C-Stick/Calibration = 100.00\r\n\
             Rumble/Motor = `Motor L` | `Motor R`\r\n"
        );
    }

    #[test]
    fn a_real_remote_and_the_balance_board_are_left_as_the_user_set_them() {
        let before = "[Wiimote1]\r\nSource = 1\r\nDevice = Keyboard\r\n\
                      [Wiimote2]\r\nSource = 2\r\nButtons/A = `Click 0`\r\n\
                      [BalanceBoard]\r\nSource = 2\r\n";
        let left = left_alone(Console::Wii, before);
        assert_eq!(left, [false, true, false, false]);
        let players = vec![Player::on(crate::pads::xinput_slot(0)), Player::on(crate::pads::xinput_slot(1))];
        let devices: Vec<Option<String>> = players.iter().map(|player| device(&player.pad, &no_ids)).collect();
        let after = file(Console::Wii, before, &players, &devices, &[true, true], &left);
        assert!(after.starts_with("[Wiimote1]\r\nDevice = XInput/0/Gamepad\r\n"), "{after}");
        assert!(
            after.ends_with("[Wiimote2]\r\nSource = 2\r\nButtons/A = `Click 0`\r\n[BalanceBoard]\r\nSource = 2\r\n"),
            "{after}"
        );
        // A remote Omoio switched off is Omoio's to switch on again.
        assert_eq!(left_alone(Console::Wii, "[Wiimote2]\r\nSource = 0\r\n"), [false; 4]);
    }

    #[test]
    fn a_gamecube_port_given_another_device_keeps_its_buttons() {
        // A dance mat in port 2 and a GameCube adapter in port 4.
        let dolphin_ini = "[Core]\r\nSIDevice0 = 6\r\nSIDevice1 = 9\r\nSIDevice2 = 0\r\nSIDevice3 = 12\r\n";
        assert_eq!(left_alone(Console::GameCube, dolphin_ini), [false, true, false, true]);
        assert_eq!(left_alone(Console::GameCube, ""), [false; 4]);
    }

    /// A DualSense as player 1, as Omoio tells Dolphin of it.
    fn on_a_dualsense() -> (Vec<Player>, Vec<Option<String>>) {
        let ids = |device: &str| (device == "PS5 Controller 0").then_some((0x054C, 0x0CE6, 0));
        let players = vec![Player::on(pad("PS5 Controller 0", "SDL"))];
        let devices = players.iter().map(|player| device(&player.pad, &ids)).collect();
        (players, devices)
    }

    #[test]
    fn a_games_own_layout_fills_the_file_for_every_game_when_there_is_none() {
        let (players, devices) = on_a_dualsense();
        assert!(!has_sections(""), "before Dolphin first started");
        let left = left_alone(Console::Wii, "");
        let text = file(Console::Wii, "", &players, &devices, &[true], &left);
        assert!(has_sections(&text));
        assert!(text.starts_with("[Wiimote1]\r\nDevice = SDL/0/DualSense Wireless Controller\r\nButtons/A = `Button A`\r\n"), "{text}");
        assert!(!text.contains("Click"), "{text}");
    }

    #[test]
    fn dolphins_own_keyboard_layout_gives_way_and_the_balance_board_stays() {
        // As Dolphin saves the file on its first start, with the keyboard
        // and mouse for player 1 (`MainWindow::InitControllers`).
        let before = "[Wiimote1]\r\nDevice = DInput/0/Keyboard Mouse\r\nButtons/A = `Click 0`\r\nButtons/B = `Click 1`\r\n\
                      [Wiimote2]\r\nDevice = \r\n[BalanceBoard]\r\nDevice = DInput/0/Keyboard Mouse\r\nSource = 2\r\n";
        let (players, devices) = on_a_dualsense();
        let after = file(Console::Wii, before, &players, &devices, &[true], &left_alone(Console::Wii, before));
        assert!(after.starts_with("[Wiimote1]\r\nDevice = SDL/0/DualSense Wireless Controller\r\n"), "{after}");
        assert!(!after.contains("Click 0"), "{after}");
        assert!(after.ends_with("[BalanceBoard]\r\nDevice = DInput/0/Keyboard Mouse\r\nSource = 2\r\n"), "{after}");
    }

    #[test]
    fn a_section_is_what_dolphin_takes_as_one() {
        assert!(has_sections("[Wiimote1]\r\nSource = 1\r\n"));
        assert!(has_sections("\u{feff}[Wiimote1]\r\n"), "after a byte order mark");
        assert!(has_sections("# a note\r\n[GCPad1]\r\n"));
        assert!(!has_sections("# a note\r\nDevice = XInput/0/Gamepad\r\n"), "values before any section are dropped");
        assert!(!has_sections(" [Wiimote1]\r\n"), "Dolphin wants the bracket first");
        assert!(!has_sections("[Wiimote1\r\n"));
    }

    #[test]
    fn each_player_is_bound_as_dolphin_picks_it() {
        let every_game = "[Wiimote1]\r\nDevice = XInput/0/Gamepad\r\n[Wiimote2]\r\nDevice = XInput/1/Gamepad\r\n\
                          [Wiimote3]\r\nDevice = \r\n";
        let profiles = |name: &str| match name {
            "Omoio SSPE52 1" => Some(Some("[Profile]\r\nDevice = SDL/0/DualSense Wireless Controller\r\n".to_string())),
            "A folder of them" => Some(None),
            _ => None,
        };
        // No game settings: the file for every game.
        assert_eq!(
            bound_devices(Console::Wii, every_game, "", &profiles),
            [Some("XInput/0/Gamepad".to_string()), Some("XInput/1/Gamepad".to_string()), None, None]
        );
        // Player 1's profile, and player 2 read from that profile's file.
        let game = "[Controls]\r\nWiimoteProfile1 = Omoio SSPE52 1, Another\r\nWiimoteSource0 = 1\r\n";
        assert_eq!(
            bound_devices(Console::Wii, every_game, game, &profiles),
            [Some("SDL/0/DualSense Wireless Controller".to_string()), None, None, None]
        );
        // A profile that isn't there is skipped, and the player takes their
        // section.
        let missing = "[Controls]\r\nWiimoteProfile1 = Gone\r\n";
        assert_eq!(
            bound_devices(Console::Wii, every_game, missing, &profiles)[..2],
            [Some("XInput/0/Gamepad".to_string()), Some("XInput/1/Gamepad".to_string())]
        );
        // One Omoio can't read says nothing, nor of the players after it.
        let folder = "[Controls]\r\nWiimoteProfile1 = A folder of them\r\n";
        assert_eq!(bound_devices(Console::Wii, every_game, folder, &profiles), [None, None, None, None]);
        // No section: Dolphin's own layout, which Omoio can't say anything of.
        assert_eq!(bound_devices(Console::Wii, "", game, &profiles), [None, None, None, None]);
        // The GameCube's profiles and sections.
        let pads = "[GCPad1]\r\nDevice = XInput/0/Gamepad\r\n";
        assert_eq!(bound_devices(Console::GameCube, pads, "", &profiles)[0].as_deref(), Some("XInput/0/Gamepad"));
    }

    #[test]
    fn a_plugged_pad_dolphin_never_added_is_reported() {
        let bound = [Some("SDL/0/DualSense Wireless Controller".to_string()), Some("XInput/0/Gamepad".to_string())];
        let plugged = [
            ("DualSense".to_string(), Some("SDL/0/DualSense Wireless Controller".to_string())),
            ("Xbox Controller".to_string(), Some("XInput/0/Gamepad".to_string())),
        ];
        let added = ["DInput/0/Keyboard Mouse", "XInput/0/Gamepad"];
        assert_eq!(unfound_player(&bound, &[], &added, &plugged), Some((0, "DualSense".to_string())));
        let all = ["DInput/0/Keyboard Mouse", "SDL/0/DualSense Wireless Controller", "XInput/0/Gamepad"];
        assert_eq!(unfound_player(&bound, &[], &all, &plugged), None);
        // Not plugged in: nothing for Dolphin to find.
        assert_eq!(unfound_player(&bound, &[], &added, &plugged[1..]), None);
        // A real Wii Remote's place is the user's.
        assert_eq!(unfound_player(&bound, &[true, false], &added, &plugged), None);
        let said = notice(0, "DualSense");
        assert!(said.starts_with("Dolphin couldn't find player 1's controller, DualSense,"), "{said}");
    }

    #[test]
    fn every_key_a_player_can_be_given_has_a_name_in_dolphin() {
        for (key, scan) in crate::core::keys::KEYS {
            assert!(key_name(scan).is_some(), "{key}");
            assert!(keyboard_input(key, 100).is_some(), "{key}");
        }
        let names: Vec<&str> = NAMED_KEYS.iter().map(|(_, name)| *name).collect();
        for (at, name) in names.iter().enumerate() {
            assert!(!names[at + 1..].contains(name), "{name} twice");
        }
    }

    #[test]
    fn keys_are_named_by_where_they_sit() {
        let name = |key: &str| key_name(crate::core::keys::scan_code(key).unwrap());
        assert_eq!(name("KeyW"), Some("W"));
        assert_eq!(name("Digit1"), Some("1"));
        assert_eq!(name("Enter"), Some("RETURN"));
        assert_eq!(name("NumpadEnter"), Some("NUMPADENTER"));
        assert_eq!(name("ShiftLeft"), Some("LSHIFT"));
        assert_eq!(name("Backspace"), Some("BACK"));
        assert_eq!(name("Backquote"), Some("GRAVE"));
        assert_eq!(name("IntlBackslash"), Some("OEM_102"));
        assert_eq!(name("ArrowUp"), Some("UP"));
        assert_eq!(name("PageDown"), Some("NEXT"));
        assert_eq!(name("NumpadDivide"), Some("DIVIDE"));
        assert_eq!(key_name(0xE052), None, "Insert, Omoio's own");
        assert_eq!(key_name(0xE11D), None);
    }

    #[test]
    fn the_keyboard_on_the_wii_is_a_remote_with_a_nunchuk_and_the_mouse_points() {
        let player = Player::on(Pad::keyboard());
        let devices = [device(&player.pad, &no_ids)];
        let text = file(Console::Wii, "", &[player], &devices, &[true], &[]);
        let mouse = |axis: &str| format!("smooth(min(`RelativeMouse {axis}` * 0.125, 1), 0.01, 0.05)");
        assert_eq!(
            text,
            "[Wiimote1]\r\n\
             Device = DInput/0/Keyboard Mouse\r\n\
             Buttons/A = `SPACE`\r\n\
             Buttons/B = `Click 2` | `L`\r\n\
             Buttons/1 = `Q`\r\n\
             Buttons/2 = `LSHIFT`\r\n\
             Buttons/- = `BACK`\r\n\
             Buttons/+ = `RETURN`\r\n\
             D-Pad/Up = `1`\r\n\
             D-Pad/Down = `3`\r\n\
             D-Pad/Left = `4`\r\n\
             D-Pad/Right = `2`\r\n\
             IR/Up = `Cursor Y-`\r\n\
             IR/Down = `Cursor Y+`\r\n\
             IR/Left = `Cursor X-`\r\n\
             IR/Right = `Cursor X+`\r\n\
             Shake/X = `E`\r\n\
             Shake/Y = `E`\r\n\
             Shake/Z = `E`\r\n\
             Swing/Forward = `F`\r\n\
             Nunchuk/Buttons/C = `Click 1` | `K`\r\n\
             Nunchuk/Buttons/Z = `Click 0` | `J` | `R`\r\n\
             Nunchuk/Stick/Up = `W`\r\n\
             Nunchuk/Stick/Down = `S`\r\n\
             Nunchuk/Stick/Left = `A`\r\n\
             Nunchuk/Stick/Right = `D`\r\n\
             Source = 1\r\n\
             Extension = Nunchuk\r\n\
             IR/Relative Input = False\r\n\
             IR/Auto-Hide = True\r\n"
        );
        assert!(!text.contains(&mouse("X+")), "the right stick's places are the pointer's");
    }

    #[test]
    fn the_mouse_turns_the_gamecube_c_stick_at_the_players_speed() {
        let player = Player::on(Pad::keyboard());
        let lines = lines(Console::GameCube, &player, Some(KEYBOARD_MOUSE), true);
        let has = |line: &str| lines.contains(&line.to_string());
        assert!(has("Device = DInput/0/Keyboard Mouse"), "{lines:?}");
        assert!(has("C-Stick/Up = smooth(min(`RelativeMouse Y-` * 0.125, 1), 0.01, 0.05)"), "{lines:?}");
        assert!(has("C-Stick/Right = smooth(min(`RelativeMouse X+` * 0.125, 1), 0.01, 0.05)"), "{lines:?}");
        assert!(has("Buttons/X = `Click 0` | `J`"), "{lines:?}");
        assert!(has("Main Stick/Left = `A`"), "{lines:?}");
        assert!(has("Triggers/R-Analog = `F`"), "{lines:?}");
        assert!(!lines.iter().any(|line| line.starts_with("Rumble")), "{lines:?}");
        let slower = Player::on(Pad::keyboard()).with_mouse(50);
        let lines = super::lines(Console::GameCube, &slower, Some(KEYBOARD_MOUSE), true);
        assert!(
            lines.contains(&"C-Stick/Down = smooth(min(`RelativeMouse Y+` * 0.0625, 1), 0.01, 0.05)".to_string()),
            "{lines:?}"
        );
    }

    #[test]
    fn a_keyboard_players_own_inputs_are_followed() {
        let buttons = BTreeMap::from([
            ("South".to_string(), "Space MouseForward".to_string()),
            ("RB".to_string(), "WheelUp".to_string()),
            ("LS Y+".to_string(), "KeyW ArrowUp".to_string()),
        ]);
        let player = Player::with_buttons(Pad::keyboard(), buttons);
        let lines = lines(Console::GameCube, &player, Some(KEYBOARD_MOUSE), true);
        assert!(lines.contains(&"Buttons/A = `SPACE` | `Click 4`".to_string()), "{lines:?}");
        assert!(lines.contains(&"Buttons/Z = pulse(`RelativeMouse Z+`, 0.1)".to_string()), "{lines:?}");
        assert!(lines.contains(&"Main Stick/Up = `W` | `UP`".to_string()), "{lines:?}");
    }

    #[test]
    fn a_pad_player_next_to_a_keyboard_player_is_written_as_before() {
        let pad_alone = vec![Player::on(crate::pads::xinput_slot(0))];
        let alone_devices: Vec<Option<String>> = pad_alone.iter().map(|p| device(&p.pad, &no_ids)).collect();
        let alone = file(Console::Wii, "", &pad_alone, &alone_devices, &[true], &[]);
        let players = vec![Player::on(crate::pads::xinput_slot(0)), Player::on(Pad::keyboard())];
        let devices: Vec<Option<String>> = players.iter().map(|p| device(&p.pad, &no_ids)).collect();
        let both = file(Console::Wii, "", &players, &devices, &[true, true], &[]);
        assert!(both.starts_with(&alone), "{both}");
        assert!(both[alone.len()..].starts_with("[Wiimote2]\r\nDevice = DInput/0/Keyboard Mouse\r\n"), "{both}");
    }

    #[test]
    fn a_keyboard_player_is_usable_and_never_stood_in_for() {
        let players = vec![Player::on(Pad::keyboard()), Player::on(crate::pads::xinput_slot(1))];
        let plugged = [crate::pads::xinput_slot(0)];
        let usable = |player: &Player| device(&player.pad, &no_ids).is_some();
        assert_eq!(stand_in(&players, &plugged, &usable), players);
        assert!(missing(&players, &plugged, &usable).is_none());
        assert!(missing(&players, &[], &usable).is_none(), "with no pad plugged in at all");
    }

    #[test]
    fn a_players_own_buttons_are_followed() {
        let buttons = BTreeMap::from([("South".to_string(), "East".to_string())]);
        let player = Player::with_buttons(crate::pads::xinput_slot(0), buttons);
        let lines = lines(Console::Wii, &player, Some("XInput/0/Gamepad"), true);
        assert!(lines.contains(&"Buttons/A = `Button B`".to_string()));
    }

    #[test]
    fn playstation_and_switch_pads_are_named_as_sdl_names_them() {
        let ids = |device: &str| match device {
            "PS5 Controller 0" => Some((0x054C, 0x0CE6, 0)),
            "PS5 Controller 1" => Some((0x054C, 0x0CE6, 1)),
            "PS4 Controller 0" => Some((0x054C, 0x09CC, 0)),
            "Nintendo Switch Pro Controller 0" => Some((0x057E, 0x2009, 0)),
            "8BitDo Something 0" => Some((0x2DC8, 0x6001, 0)),
            _ => None,
        };
        let name = |device: &str| super::device(&pad(device, "SDL"), &ids);
        assert_eq!(name("PS5 Controller 0").as_deref(), Some("SDL/0/DualSense Wireless Controller"));
        assert_eq!(name("PS5 Controller 1").as_deref(), Some("SDL/1/DualSense Wireless Controller"));
        assert_eq!(name("PS4 Controller 0").as_deref(), Some("SDL/0/PS4 Controller"));
        assert_eq!(name("Nintendo Switch Pro Controller 0").as_deref(), Some("SDL/0/Nintendo Switch Pro Controller"));
        assert_eq!(name("8BitDo Something 0"), None, "a name SDL would give some other way");
        assert_eq!(super::device(&pad("XInput Pad #4", "XInput"), &ids).as_deref(), Some("XInput/3/Gamepad"));
    }

    #[test]
    fn pads_are_numbered_among_those_sdl_names_the_same() {
        let (first, second) = ((0x054C, 0x05C4), (0x054C, 0x09CC));
        assert!(same_sdl_name(first, second), "both models of the DualShock 4");
        assert!(same_sdl_name(second, second));
        assert!(!same_sdl_name(first, (0x054C, 0x0CE6)), "a DualSense");
        assert!(!same_sdl_name((0x2DC8, 0x6001), (0x2DC8, 0x6001)), "a pad SDL names some other way");
    }

    #[test]
    fn player_one_with_no_controller_keeps_none() {
        let players = vec![Player::on(Pad::none()), Player::on(crate::pads::xinput_slot(1))];
        let plugged = [crate::pads::xinput_slot(0)];
        let usable = |player: &Player| xinput_slot(&player.pad).is_some();
        assert_eq!(stand_in(&players, &plugged, &usable), players);
        assert!(missing(&players, &plugged, &usable).unwrap().contains("Player 1 has no controller"));
    }

    #[test]
    fn player_one_on_an_unknown_pad_plays_on_an_xbox_pad_instead() {
        let players = vec![Player::on(pad("Mystery Pad 0", "SDL")), Player::on(crate::pads::xinput_slot(1))];
        let plugged = [crate::pads::xinput_slot(0)];
        let usable = |player: &Player| xinput_slot(&player.pad).is_some();
        let seated = stand_in(&players, &plugged, &usable);
        assert_eq!(seated[0].pad.device, "XInput Pad #1");
        assert!(missing(&players, &plugged, &usable).is_none());
        let warned = missing(&players, &[], &usable).unwrap();
        assert!(warned.contains("Mystery Pad 0"), "{warned}");
    }

    #[test]
    fn a_games_own_layout_is_filed_under_its_disc_id() {
        assert_eq!(game_id("SSPP52"), Some("SSPP52"));
        assert_eq!(game_id("GALE01D2"), Some("GALE01"), "a second disc");
        assert_eq!(game_id("BLES"), None);
        assert_eq!(profile_name("SSPP52", 0), "Omoio SSPP52 1");
    }

    #[test]
    fn another_consoles_id_is_no_dolphin_game() {
        assert_eq!(game_id("BLES01272"), None, "a PS3 game");
        assert_eq!(game_id("NPUB30910"), None, "a PS3 game from the store");
        assert_eq!(game_id("0005000010140400"), None, "a Wii U game");
        assert_eq!(game_id("WUD87E51FD0F7F95"), None, "a Wii U disc image");
        assert_eq!(game_id("GALE01D"), None);
        assert_eq!(game_id("GALE01X2"), None);
        assert_eq!(game_id("SS-P52"), None);
        assert_eq!(game_id("GALE01D12"), Some("GALE01"));
    }

    #[test]
    fn forgetting_takes_out_only_omoios_own_profiles() {
        let text = "[Controls]\r\nWiimoteProfile1 = Omoio SSPP52 1\r\nWiimoteSource0 = 1\r\n\
                    WiimoteProfile2 = My remote\r\nWiimoteSource1 = 2\r\n[Core]\r\nCPUThread = False\r\n";
        assert_eq!(
            without_own_profiles(text, Console::Wii, "SSPP52"),
            "[Controls]\r\nWiimoteProfile2 = My remote\r\nWiimoteSource1 = 2\r\n[Core]\r\nCPUThread = False\r\n"
        );
        // The GameCube's are asked about the same id, and have none there.
        assert_eq!(without_own_profiles(text, Console::GameCube, "SSPP52"), text);
    }

    #[test]
    fn a_wii_game_starts_with_the_standard_gamecube_pads_unplugged() {
        // Dolphin's own defaults: a pad in the first port only.
        assert_eq!(gamecube_ports_off(""), ["--config", "Dolphin.Core.SIDevice0=0"]);
        // As Omoio leaves them for GameCube games, but for a port the user
        // gave a GameCube adapter (12) and one with nothing in it.
        let ini = "[Core]\r\nSIDevice0 = 6\r\nSIDevice1 = 12\r\nSIDevice2 = 6\r\nSIDevice3 = 0\r\n";
        assert_eq!(
            gamecube_ports_off(ini),
            ["--config", "Dolphin.Core.SIDevice0=0", "--config", "Dolphin.Core.SIDevice2=0"]
        );
        assert!(gamecube_ports_off("[Core]\r\nSIDevice0 = 0\r\n").is_empty());
        assert_eq!(gamecube_ports_off("[Core]\r\nSIDevice0 = pad\r\n").len(), 2, "unreadable is Dolphin's default");
    }

    #[test]
    fn empty_gamecube_ports_get_a_standard_pad_and_chosen_ones_stay() {
        let dir = std::env::temp_dir().join(format!("omoio-dolphin-pads-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ini_path = dir.join("Dolphin.ini");
        std::fs::write(&ini_path, "[Core]\r\nSIDevice1 = 5\r\n").unwrap();
        standard_controllers(&ini_path).unwrap();
        let text = std::fs::read_to_string(&ini_path).unwrap();
        assert_eq!(ini::get(&text, "Core", "SIDevice0").as_deref(), Some("6"));
        assert_eq!(ini::get(&text, "Core", "SIDevice1").as_deref(), Some("5"), "a Game Boy Advance the user chose");
        assert_eq!(ini::get(&text, "Core", "SIDevice3").as_deref(), Some("6"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
