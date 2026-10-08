//! RPCS3's side of the controller layout: its per-player profile file.
//!
//! RPCS3 starts out with no pad for anyone. Its own dialog says a real
//! controller is recommended, but nothing happens until someone opens that
//! dialog and picks a handler, and every button in a fresh profile is empty.
//! So plugging in a pad and pressing Play does nothing at all.
//!
//! Omoio keeps the layout itself (core/pad_layout.rs) and writes it here in
//! RPCS3's form. The device name has to be the one RPCS3 will see: a name it
//! cannot find becomes a placeholder rather than binding to whatever is
//! plugged in.
//!
//! - An Xbox pad, or anything else speaking XInput, goes through RPCS3's
//!   XInput handler. It names pads by slot, "XInput Pad #1" to "#4", so the
//!   name is known the moment Windows says a slot is in use. No guessing.
//! - A DualSense, a DualSense Edge or a DualShock 4 goes through RPCS3's own
//!   handler for it, which also names pads by slot: "DualSense Pad #1", "DS4
//!   Pad #1". See `SONY_PADS`.
//! - Anything else, a Switch Pro or an 8BitDo, goes through SDL, under the
//!   name SDL gives it and a number counted from 1. See `sdl_name`.
//!
//! The saved layout names a pad as gilrs does, "PS5 Controller 0", and is
//! shared with Cemu, so it is turned into RPCS3's names here, as the file is
//! written, from the pads plugged in at that moment. A pad that is not
//! plugged in keeps the name the layout has.
//!
//! An XInput slot can be named before anything is in it (RPCS3's
//! `xinput_pad_handler::get_device` takes any of the four), and RPCS3 notices
//! when a pad arrives, so players can wait on the slots a second, third and
//! fourth pad will take. The Sony handlers' slots are the same.
//!
//! What RPCS3 does with each name was read in its source at commit 222754bf,
//! the build (v0.0.43-20247) its releases page gave as the latest on 8 October
//! 2026, and in the SDL it builds with, SDL 3.4 at commit 829a65d.

use crate::core::pad_layout::{self, Pad, Player, PLAYERS};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tauri::AppHandle;

const SONY: u16 = 0x054C;

/// Sony's pads by USB product id, with the RPCS3 handler that reads each and
/// the name that handler gives a pad before its slot number.
///
/// RPCS3 makes seven slots per handler before looking for a pad
/// (`hid_pad_handler::Init`) and finds a pad's slot by that name alone
/// (`get_hid_device`), so the name is known in advance, as XInput's is. The
/// ids are the handlers' own (`dualsense_pad_handler.cpp`, `SONY_DUALSENSE_ID_0`
/// and `_1`; `ds4_pad_handler.cpp`, `SONY_DS4_ID_0` to `_2`). Windows lists a
/// pad on Bluetooth by the same ids (hidapi's `hid_enumerate`, which RPCS3
/// uses, compares `HidD_GetAttributes`), and both handlers read it there:
/// `dualsense_pad_handler::get_data` takes the Bluetooth report 0x31 and
/// `ds4_pad_handler::check_add_device` reads the bus type.
///
/// A pad found goes in the first empty slot (`check_add_device`), and pads
/// found together go in the order of their Windows device paths
/// (`hid_pad_handler::update_devices` walks a sorted set). So one pad of a
/// kind is always #1; with two, which is #1 cannot be told from here, and the
/// order gilrs lists them in stands in.
const SONY_PADS: [(u16, &str, &str); 5] = [
    (0x0CE6, "DualSense", "DualSense Pad #"),
    (0x0DF2, "DualSense", "DualSense Pad #"),
    (0x05C4, "DualShock 4", "DS4 Pad #"),
    (0x09CC, "DualShock 4", "DS4 Pad #"),
    (0x0BA0, "DualShock 4", "DS4 Pad #"),
];

fn is_sony(handler: &str) -> bool {
    SONY_PADS.iter().any(|(_, sony, _)| *sony == handler)
}

/// Pads one of SDL's own drivers reads, by USB vendor and product id, with
/// the name SDL gives them: `UpdateDeviceIdentity` in SDL_hidapi_switch.c,
/// which an 8BitDo pad in its Switch mode also meets, as it reports itself
/// as a Pro Controller, and `HIDAPI_Driver8BitDo_InitDevice` in
/// SDL_hidapi_8bitdo.c for 8BitDo pads in their D-input mode. Those drivers
/// are on by default on Windows (`SDL_HIDAPI_DEFAULT`), and RPCS3 turns
/// neither off (`sdl_instance::initialize_impl`).
const SDL_NAMES: [(u16, u16, &str); 8] = [
    (0x057E, 0x2009, "Nintendo Switch Pro Controller"),
    (0x2DC8, 0x6000, "8BitDo SF30 Pro"),
    (0x2DC8, 0x6100, "8BitDo SF30 Pro"),
    (0x2DC8, 0x6001, "8BitDo SN30 Pro"),
    (0x2DC8, 0x6101, "8BitDo SN30 Pro"),
    (0x2DC8, 0x6003, "8BitDo Pro 2"),
    (0x2DC8, 0x6006, "8BitDo Pro 2"),
    (0x2DC8, 0x6009, "8BitDo Pro 3"),
];

/// The name RPCS3's SDL handler gives a pad, before its number.
///
/// RPCS3 names an SDL pad `SDL_GetGamepadName` and a number counting the pads
/// of that name so far, from 1 (`sdl_pad_handler::enumerate_devices`). A name
/// it has no pad for gets an empty stand-in (`get_device`, "Adding empty
/// device"), which only a pad plugged in later takes, so a pad there from the
/// start must be named exactly. For a pad one of SDL's own drivers reads,
/// `SDL_GetGamepadName` gives the driver's name: the controller database
/// RPCS3 ships has no mapping for those drivers' pads (none of its Windows
/// entries carries their mark, an `h` in the GUID, in the copy installed on 8
/// October 2026), so SDL makes one named "*"
/// (`SDL_CreateMappingForHIDAPIGamepad`), which means the pad's own name.
/// Any other pad goes by its mapping's name in that database, which gilrs
/// reads too, so gilrs's name stands in for it. One RPCS3 then can't find
/// is told about after the game starts (`unfound`).
fn sdl_name(pad: &Pad, ids: Option<(u16, u16)>) -> String {
    ids.and_then(|ids| SDL_NAMES.iter().find(|(vendor, product, _)| (*vendor, *product) == ids))
        .map_or_else(|| pad.name.clone(), |(.., name)| name.to_string())
}

/// A pad gilrs reads that is plugged in, with its USB vendor and product ids
/// when gilrs knows them.
struct Plugged {
    pad: Pad,
    ids: Option<(u16, u16)>,
}

fn plugged() -> Vec<Plugged> {
    crate::pads::connected()
        .into_iter()
        .filter(|pad| pad.handler == "SDL")
        .map(|pad| {
            let ids = crate::pads::usb_ids(&pad.device).map(|(vendor, product, _)| (vendor, product));
            Plugged { pad, ids }
        })
        .collect()
}

/// RPCS3's handler and device for each pad plugged in, in the same order.
fn rpcs3_names(plugged: &[Plugged]) -> Vec<(&'static str, String)> {
    let kinds: Vec<(&'static str, String)> = plugged
        .iter()
        .map(|p| {
            let sony = p
                .ids
                .filter(|(vendor, _)| *vendor == SONY)
                .and_then(|(_, product)| SONY_PADS.iter().find(|(id, ..)| *id == product));
            match sony {
                Some((_, handler, prefix)) => (*handler, prefix.to_string()),
                None => ("SDL", format!("{} ", sdl_name(&p.pad, p.ids))),
            }
        })
        .collect();
    kinds
        .iter()
        .enumerate()
        .map(|(at, (handler, prefix))| {
            let number = kinds[..at].iter().filter(|(_, other)| other == prefix).count() + 1;
            (*handler, format!("{prefix}{number}"))
        })
        .collect()
}

/// What gilrs 0.11.2 calls Sony's pads on Windows, before their number, with
/// the RPCS3 handler and slot name for each: its controller list names the
/// DualSense and the DualSense Edge "PS5 Controller" and every DualShock 4
/// "PS4 Controller" (`SDL_GameControllerDB/gamecontrollerdb.txt`, read 8
/// October 2026).
const SONY_BY_NAME: [(&str, &str, &str); 2] = [
    ("PS5 Controller", "DualSense", "DualSense Pad #"),
    ("PS4 Controller", "DualShock 4", "DS4 Pad #"),
];

/// The handler and device RPCS3 is told a player's pad is. An XInput slot,
/// or a Sony handler's slot read back from RPCS3's file, is RPCS3's own name
/// already.
///
/// A Sony pad not plugged in as the game starts, such as one switched on a
/// moment later, is named from the name the layout keeps, "PS5 Controller
/// 0": its slot exists whether or not a pad is in it, and RPCS3 fills it
/// when one arrives. Any other pad not plugged in keeps the name it has. A
/// layout kept by an earlier Omoio may hold the second listing of an
/// Xbox-style pad from another maker as a player of its own (pads.rs,
/// `connected`), and named so that RPCS3 could find it, that pad would drive
/// two players.
fn rpcs3_pad(pad: &Pad, plugged: &[Plugged]) -> (String, String) {
    if let Some(at) = plugged
        .iter()
        .position(|p| pad.handler == "SDL" && p.pad.device == pad.device)
    {
        let (handler, device) = rpcs3_names(plugged).swap_remove(at);
        return (handler.to_string(), device);
    }
    let sony = pad
        .device
        .rsplit_once(' ')
        .filter(|_| pad.handler == "SDL")
        .and_then(|(name, number)| {
            let (_, handler, prefix) = SONY_BY_NAME.iter().find(|(gilrs, ..)| *gilrs == name)?;
            Some((handler.to_string(), format!("{prefix}{}", number.parse::<u32>().ok()? + 1)))
        });
    sony.unwrap_or_else(|| (pad.handler.clone(), pad.device.clone()))
}

/// The pad a player in RPCS3's file is on, as Omoio names it: the pad plugged
/// in that would be written as this device, else the device itself, waiting
/// for its pad as an XInput slot does.
fn omoio_pad(handler: &str, device: &str, plugged: &[Plugged]) -> Pad {
    if let Some(at) = rpcs3_names(plugged)
        .iter()
        .position(|(h, d)| *h == handler && d == device)
    {
        return plugged[at].pad.clone();
    }
    let family = match handler {
        "XInput" => "xbox",
        sony if is_sony(sony) => "playstation",
        _ => "generic",
    };
    Pad {
        name: display_name(handler, device),
        family: family.to_string(),
        device: device.to_string(),
        handler: handler.to_string(),
    }
}

/// Each PS3 input by RPCS3's name for it, which is also its key in the file,
/// and the place it sits on a pad. In the order RPCS3's own dialog lists them.
const PS3: [(&str, &str); 25] = [
    ("Cross", "South"),
    ("Circle", "East"),
    ("Square", "West"),
    ("Triangle", "North"),
    ("Up", "Up"),
    ("Down", "Down"),
    ("Left", "Left"),
    ("Right", "Right"),
    ("L1", "LB"),
    ("L2", "LT"),
    ("L3", "LS"),
    ("R1", "RB"),
    ("R2", "RT"),
    ("R3", "RS"),
    ("Start", "Start"),
    ("Select", "Back"),
    ("PS Button", "Guide"),
    ("Left Stick Left", "LS X-"),
    ("Left Stick Right", "LS X+"),
    ("Left Stick Up", "LS Y+"),
    ("Left Stick Down", "LS Y-"),
    ("Right Stick Left", "RS X-"),
    ("Right Stick Right", "RS X+"),
    ("Right Stick Up", "RS Y+"),
    ("Right Stick Down", "RS Y-"),
];

/// What the Controller screen calls each place for the PS3.
pub const BUTTON_NAMES: [(&str, &str); 25] = [
    ("South", "Cross"),
    ("East", "Circle"),
    ("West", "Square"),
    ("North", "Triangle"),
    ("LB", "L1"),
    ("RB", "R1"),
    ("LT", "L2"),
    ("RT", "R2"),
    ("LS", "L3"),
    ("RS", "R3"),
    ("Up", "D-pad up"),
    ("Down", "D-pad down"),
    ("Left", "D-pad left"),
    ("Right", "D-pad right"),
    ("Back", "Select"),
    ("Start", "Start"),
    ("Guide", "PS button"),
    ("LS Y+", "Left stick up"),
    ("LS Y-", "Left stick down"),
    ("LS X-", "Left stick left"),
    ("LS X+", "Left stick right"),
    ("RS Y+", "Right stick up"),
    ("RS Y-", "Right stick down"),
    ("RS X-", "Right stick left"),
    ("RS X+", "Right stick right"),
];

/// SDL names the face buttons by where they sit; XInput by what is printed on
/// them. Everything else is spelled the same by both.
const FACE: [(&str, &str); 4] = [("South", "A"), ("East", "B"), ("West", "X"), ("North", "Y")];

/// The DualSense and DualShock 4 handlers name buttons as a PlayStation pad
/// is marked (`button_list` in dualsense_pad_handler.cpp and
/// ds4_pad_handler.cpp). The d-pad and the sticks' directions are spelled as
/// SDL spells them. The DualSense handler's own "LB" and "RB" are the Edge's
/// back buttons, not L1 and R1, so no other place passes through by its SDL
/// name.
const PLAYSTATION: [(&str, &str); 13] = [
    ("South", "Cross"),
    ("East", "Circle"),
    ("West", "Square"),
    ("North", "Triangle"),
    ("LB", "L1"),
    ("RB", "R1"),
    ("LT", "L2"),
    ("RT", "R2"),
    ("LS", "L3"),
    ("RS", "R3"),
    ("Back", "Share"),
    ("Start", "Options"),
    ("Guide", "PS Button"),
];

/// The places a handler names otherwise than SDL does, with its names.
fn renamed(handler: &str) -> &'static [(&'static str, &'static str)] {
    match handler {
        "XInput" => &FACE,
        sony if is_sony(sony) => &PLAYSTATION,
        _ => &[],
    }
}

fn to_handler(input: &str, handler: &str) -> String {
    renamed(handler)
        .iter()
        .find(|(place, _)| *place == input)
        .map_or(input, |(_, name)| name)
        .to_string()
}

/// A handler's own name that is spelled like a place it names otherwise,
/// such as the DualSense handler's "LB", is not that place, and comes back
/// empty.
fn from_handler(input: &str, handler: &str) -> String {
    let renamed = renamed(handler);
    if let Some((place, _)) = renamed.iter().find(|(_, name)| *name == input) {
        return place.to_string();
    }
    if renamed.iter().any(|(place, _)| *place == input) {
        return String::new();
    }
    input.to_string()
}

/// What the interface calls a pad. RPCS3's own names are "XInput Pad #2",
/// "DualSense Pad #1", or an SDL name with an index on the end.
fn display_name(handler: &str, device: &str) -> String {
    match handler {
        "XInput" => {
            let slot = device.rsplit('#').next().unwrap_or("1");
            format!("Controller {slot}")
        }
        sony if is_sony(sony) => sony.to_string(),
        _ => device
            .trim_end_matches(|c: char| c.is_ascii_digit())
            .trim_end()
            .to_string(),
    }
}

fn profile_dir(app: &AppHandle, title_id: &str) -> Result<PathBuf, String> {
    let root = super::install_dir(app)?.join("config").join("input_configs");
    Ok(if title_id.is_empty() {
        root.join("global")
    } else {
        root.join(title_id)
    })
}

/// Where RPCS3 reads the profile from. An empty `title_id` is the profile that
/// applies to every game; a title id is that game's own.
fn profile_path(app: &AppHandle, title_id: &str) -> Result<PathBuf, String> {
    Ok(profile_dir(app, title_id)?.join("Default.yml"))
}

/// An empty file is what RPCS3 itself writes for an untouched profile, so it
/// counts as no profile.
fn read_profile(app: &AppHandle, title_id: &str) -> Option<String> {
    std::fs::read_to_string(profile_path(app, title_id).ok()?)
        .ok()
        .filter(|text| !text.trim().is_empty())
}

/// Reads one `Key: value` out of the profile.
///
/// The file is ours: we write it, RPCS3 rewrites it in the same shape, and the
/// values are short strings. A YAML parser would be a dependency for a few
/// dozen lines of flat key-value.
fn read_value(text: &str, key: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| line.starts_with(key) && line[key.len()..].starts_with(':'))
        .map(|line| line[key.len() + 1..].trim().trim_matches('"').to_string())
}

/// Each player's part of a profile, by number, empty for a player the file
/// leaves out.
fn sections(text: &str) -> Vec<String> {
    let mut sections = vec![String::new(); PLAYERS];
    let mut current: Option<usize> = None;
    for line in text.lines() {
        if let Some(number) = line
            .strip_prefix("Player ")
            .and_then(|rest| rest.trim_end().strip_suffix(" Input:"))
        {
            current = number
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=PLAYERS).contains(n))
                .map(|n| n - 1);
            continue;
        }
        // Any other line at the left edge starts something that is not a
        // player.
        if !line.starts_with(' ') {
            current = None;
        }
        if let Some(at) = current {
            sections[at].push_str(line);
            sections[at].push('\n');
        }
    }
    sections
}

/// A player's handler and device in a profile, when the file names both.
fn handler_and_device(section: &str) -> Option<(String, String)> {
    let handler = read_value(section, "Handler")?;
    let device = read_value(section, "Device").filter(|d| !d.is_empty())?;
    Some((handler, device))
}

/// The players in a profile, by number, on the pads Omoio knows them by, with
/// `plugged` the pads gilrs reads that are plugged in. A player the file
/// leaves out, or gives a handler Omoio does not write, comes back as `None`.
fn parse_players(text: &str, plugged: &[Plugged]) -> Vec<Option<Player>> {
    sections(text)
        .iter()
        .map(|section| {
            let (handler, device) = handler_and_device(section)?;
            if handler != "XInput" && handler != "SDL" && !is_sony(&handler) {
                return None;
            }
            let buttons: BTreeMap<String, String> = PS3
                .iter()
                .filter_map(|(key, place)| {
                    read_value(section, key).map(|found| (place.to_string(), from_handler(&found, &handler)))
                })
                .collect();
            Some(Player::with_buttons(omoio_pad(&handler, &device, plugged), buttons))
        })
        .collect()
}

/// The file RPCS3 reads, with `plugged` the pads gilrs reads that are plugged
/// in, for naming them as RPCS3 does. Every button is written, not just the
/// ones that differ, because a fresh profile has them all empty. Leaving one
/// out means leaving it unbound.
fn profile_text(players: &[Player], plugged: &[Plugged]) -> String {
    let mut out = String::new();
    for (at, player) in players.iter().enumerate().take(PLAYERS) {
        let (handler, device) = rpcs3_pad(&player.pad, plugged);
        out.push_str(&format!("Player {} Input:\n", at + 1));
        out.push_str(&format!("  Handler: {handler}\n"));
        out.push_str(&format!("  Device: {}\n", quoted(&device)));
        out.push_str("  Buddy Device: \"\"\n");
        out.push_str("  Config:\n");
        for (key, place) in PS3 {
            out.push_str(&format!(
                "    {key}: {}\n",
                quoted(&to_handler(player.input(place), &handler))
            ));
        }
        // A profile's deadzone defaults to zero rather than to the handler's,
        // and at zero a worn stick drifts. These are the handlers' own
        // numbers: SDL's from its init_config, XInput's the constants in
        // Microsoft's XInput.h, and the DualSense and DualShock 4 handlers'
        // from theirs, 40 on a stick that counts to 255 (`init_config` in
        // dualsense_pad_handler.cpp and ds4_pad_handler.cpp at 222754bf).
        let (left, right) = match handler.as_str() {
            "XInput" => (7849, 8689),
            sony if is_sony(sony) => (40, 40),
            _ => (8000, 8000),
        };
        out.push_str(&format!("    Left Stick Deadzone: {left}\n"));
        out.push_str(&format!("    Right Stick Deadzone: {right}\n"));
    }
    out
}

/// A name can hold anything the hardware reports, including a colon, which
/// would end the key early.
fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Writes the players' layout where RPCS3 reads it, for every game or for one.
pub fn write(app: &AppHandle, title_id: &str, players: &[Player]) -> Result<(), String> {
    let usable = |p: &Player| {
        let handler = p.pad.handler.as_str();
        !p.pad.device.is_empty() && (handler == "XInput" || handler == "SDL" || is_sony(handler))
    };
    if !players.iter().all(usable) {
        return Err("Couldn't save the controller settings.".to_string());
    }
    std::fs::create_dir_all(profile_dir(app, title_id)?).map_err(|e| e.to_string())?;
    std::fs::write(profile_path(app, title_id)?, profile_text(players, &plugged()))
        .map_err(|_| "Couldn't save the controller settings.".to_string())
}

/// Takes a game's own profile away, so it goes back to the one for every game.
pub fn forget(app: &AppHandle, title_id: &str) -> Result<(), String> {
    if title_id.is_empty() || title_id.contains(['/', '\\', ':', '.']) {
        return Err("Couldn't remove that.".to_string());
    }
    let dir = profile_dir(app, title_id)?;
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir).map_err(|_| "Couldn't remove that.".to_string())?;
    }
    Ok(())
}

/// The profiles RPCS3 already has, the one for every game and each game's
/// own, read in the first time Omoio keeps a layout of its own.
pub fn existing(app: &AppHandle) -> Vec<(String, Vec<Player>)> {
    let mut scopes = vec![String::new()];
    if let Ok(entries) = super::install_dir(app)
        .map(|dir| dir.join("config").join("input_configs"))
        .and_then(|root| std::fs::read_dir(root).map_err(|e| e.to_string()))
    {
        scopes.extend(
            entries
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name != "global"),
        );
    }
    let spare = crate::pads::xinput_slots();
    let plugged = plugged();
    scopes
        .into_iter()
        .filter_map(|scope| {
            let text = read_profile(app, &scope)?;
            Some((scope, pad_layout::fill(parse_players(&text, &plugged), &spare)))
        })
        .collect()
}

/// Why RPCS3 could not use a player's pad, as its log says it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Unbound {
    /// No pad by the profile's name was there to take.
    Missing,
    /// A pad of the kind was there, but RPCS3 could not open it, as when
    /// another program holds it.
    Held,
}

/// The players, counted from zero, whose pad in `profile` RPCS3's `log`
/// says it could not use, and why. RPCS3 says so in three ways (its source
/// at 222754bf, read 8 October 2026):
///
/// - "Failed to bind device '<device>' to handler <handler>." when the
///   handler has no pad by that name at all (`pad_thread::Init`; RPCS3's pad
///   thread for its own menus says it the same way, `gui_pad_thread::init`).
/// - "Adding empty device: <device>" when SDL found no pad by that name and
///   holds an empty place for it (`sdl_pad_handler::get_device`).
/// - "One or more <handler> pads were detected but couldn't be interacted
///   with directly" when a Sony pad is there but could not be opened
///   (`hid_pad_handler::update_devices`). That names no slot, so it counts
///   for every player on that handler.
///
/// A slot handler's slot always exists, so a slot with no pad in it is never
/// in the log: a pad switched off is not found this way.
fn unbound_players(log: &str, profile: &str) -> Vec<(usize, Unbound)> {
    let mut missing: Vec<&str> = Vec::new();
    let mut held: Vec<&str> = Vec::new();
    for line in log.lines().map(str::trim_end) {
        if let Some((_, rest)) = line.split_once("Failed to bind device '") {
            if let Some((device, _)) = rest.split_once("' to handler ") {
                missing.push(device);
            }
        } else if let Some((_, device)) = line.split_once("Adding empty device: ") {
            missing.push(device);
        } else if let Some((_, rest)) = line.split_once("One or more ") {
            if let Some((handler, _)) =
                rest.split_once(" pads were detected but couldn't be interacted with directly")
            {
                held.push(handler);
            }
        }
    }
    sections(profile)
        .iter()
        .enumerate()
        .filter_map(|(at, section)| {
            let (handler, device) = handler_and_device(section)?;
            if missing.contains(&device.as_str()) {
                Some((at, Unbound::Missing))
            } else if held.contains(&handler.as_str()) {
                Some((at, Unbound::Held))
            } else {
                None
            }
        })
        .collect()
}

/// What Omoio says about a player whose pad RPCS3 could not use. `player`
/// counts from zero.
fn notice(player: usize, pad_name: &str, why: Unbound) -> String {
    let number = player + 1;
    match why {
        Unbound::Missing => format!(
            "RPCS3 couldn't find player {number}'s controller, {pad_name}, so the game won't answer it. Start the game again. If that doesn't help, the log on the Logs page shows what RPCS3 saw."
        ),
        Unbound::Held => format!(
            "RPCS3 couldn't open player {number}'s controller, {pad_name}. Close any other program using it, such as DS4Windows, then start the game again."
        ),
    }
}

/// Why a player's pad, plugged in, isn't reaching the game started for
/// `title_id`, from what RPCS3 logged as it set its pads up. RPCS3 reads a
/// game's own profile when it has one, else the one for every game
/// (`cfg_input::load`). `None` when RPCS3 found every pad plugged in, or
/// there is no log to read.
pub fn unfound(app: &AppHandle, title_id: &str) -> Option<String> {
    let log = std::fs::read(super::install_dir(app).ok()?.join("log").join("RPCS3.log")).ok()?;
    let log = String::from_utf8_lossy(&log);
    let profile = read_profile(app, title_id).or_else(|| read_profile(app, ""))?;
    let plugged = plugged();
    let players = parse_players(&profile, &plugged);
    unbound_players(&log, &profile).into_iter().find_map(|(at, why)| {
        let pad = &players.get(at)?.as_ref()?.pad;
        plugged
            .iter()
            .any(|p| p.pad.device == pad.device)
            .then(|| notice(at, &pad.name, why))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(handler: &str, device: &str) -> Pad {
        Pad {
            device: device.to_string(),
            name: display_name(handler, device),
            handler: handler.to_string(),
            family: if handler == "XInput" { "xbox" } else { "generic" }.to_string(),
        }
    }

    #[test]
    fn every_ps3_input_has_a_place_of_its_own() {
        let mut places: Vec<&str> = PS3.iter().map(|(_, place)| *place).collect();
        assert!(places.iter().all(|place| pad_layout::INPUTS.contains(place)));
        places.sort();
        places.dedup();
        assert_eq!(places.len(), 25);
        assert_eq!(BUTTON_NAMES.len(), 25);
    }

    #[test]
    fn xinput_profiles_use_its_names_for_the_face_buttons() {
        let text = profile_text(&[Player::on(pad("XInput", "XInput Pad #1"))], &[]);
        assert!(text.starts_with("Player 1 Input:\n"));
        assert!(text.contains("  Handler: XInput\n"));
        assert!(text.contains("  Device: \"XInput Pad #1\"\n"));
        assert!(text.contains("    Cross: \"A\"\n"));
        assert!(text.contains("    Triangle: \"Y\"\n"));
        assert!(text.contains("    L1: \"LB\"\n"));
        assert!(text.contains("    PS Button: \"Guide\"\n"));
        assert!(text.contains("    Left Stick Deadzone: 7849\n"));
    }

    /// An Xbox pad's profile, whole, as Omoio wrote it before Sony pads got
    /// RPCS3's own handlers. Nothing about it may change.
    #[test]
    fn an_xinput_player_is_written_exactly_as_before() {
        let text = profile_text(&[Player::on(pad("XInput", "XInput Pad #1"))], &[]);
        assert_eq!(
            text,
            "Player 1 Input:\n  Handler: XInput\n  Device: \"XInput Pad #1\"\n  Buddy Device: \"\"\n  Config:\n    \
             Cross: \"A\"\n    Circle: \"B\"\n    Square: \"X\"\n    Triangle: \"Y\"\n    \
             Up: \"Up\"\n    Down: \"Down\"\n    Left: \"Left\"\n    Right: \"Right\"\n    \
             L1: \"LB\"\n    L2: \"LT\"\n    L3: \"LS\"\n    R1: \"RB\"\n    R2: \"RT\"\n    R3: \"RS\"\n    \
             Start: \"Start\"\n    Select: \"Back\"\n    PS Button: \"Guide\"\n    \
             Left Stick Left: \"LS X-\"\n    Left Stick Right: \"LS X+\"\n    \
             Left Stick Up: \"LS Y+\"\n    Left Stick Down: \"LS Y-\"\n    \
             Right Stick Left: \"RS X-\"\n    Right Stick Right: \"RS X+\"\n    \
             Right Stick Up: \"RS Y+\"\n    Right Stick Down: \"RS Y-\"\n    \
             Left Stick Deadzone: 7849\n    Right Stick Deadzone: 8689\n"
        );
    }

    #[test]
    fn sdl_profiles_keep_sdl_names() {
        let text = profile_text(&[Player::on(pad("SDL", "DualSense Wireless Controller 0"))], &[]);
        assert!(text.contains("  Handler: SDL\n"));
        assert!(text.contains("    Cross: \"South\"\n"));
        assert!(text.contains("    Left Stick Deadzone: 8000\n"));
    }

    #[test]
    fn a_changed_button_lands_on_the_ps3_input_in_that_place() {
        let buttons = BTreeMap::from([("South".to_string(), "East".to_string())]);
        let text = profile_text(&[Player::with_buttons(pad("XInput", "XInput Pad #1"), buttons)], &[]);
        assert!(text.contains("    Cross: \"B\"\n"));
        assert!(text.contains("    Circle: \"B\"\n"), "the other place keeps its own");
    }

    #[test]
    fn four_players_written_read_back_the_same() {
        let buttons = BTreeMap::from([("North".to_string(), "West".to_string())]);
        let mut players = pad_layout::default_players(
            &[pad("SDL", "DualSense Wireless Controller 0")],
            &crate::pads::xinput_slots(),
        );
        players[1] = Player::with_buttons(players[1].pad.clone(), buttons);
        let text = profile_text(&players, &[]);
        for number in 1..=4 {
            assert!(text.contains(&format!("Player {number} Input:\n")));
        }
        let back: Vec<Player> = parse_players(&text, &[]).into_iter().map(Option::unwrap).collect();
        assert_eq!(back, players);
    }

    #[test]
    fn a_buddy_device_is_not_read_as_the_device() {
        let text = "Player 2 Input:\n  Handler: XInput\n  Buddy Device: \"\"\n  Device: \"XInput Pad #2\"\n  Config:\n    Cross: \"B\"\n";
        let players = parse_players(text, &[]);
        assert!(players[0].is_none());
        let second = players[1].as_ref().unwrap();
        assert_eq!(second.pad.device, "XInput Pad #2");
        assert_eq!(second.input("South"), "East", "B is read back as its place");
    }

    #[test]
    fn pads_have_names_worth_reading() {
        assert_eq!(display_name("XInput", "XInput Pad #3"), "Controller 3");
        assert_eq!(display_name("SDL", "DualSense Wireless Controller 0"), "DualSense Wireless Controller");
        assert_eq!(display_name("SDL", "8BitDo Pro 2 1"), "8BitDo Pro 2");
        assert_eq!(display_name("DualSense", "DualSense Pad #2"), "DualSense");
        assert_eq!(display_name("DualShock 4", "DS4 Pad #1"), "DualShock 4");
    }

    /// A pad gilrs reads, plugged in, as pads.rs names it.
    fn on_usb(device: &str, ids: Option<(u16, u16)>) -> Plugged {
        let family = match ids.map(|(vendor, _)| vendor) {
            Some(SONY) => "playstation",
            Some(0x057E) => "nintendo",
            _ => "generic",
        };
        Plugged {
            pad: Pad {
                device: device.to_string(),
                name: display_name("SDL", device),
                handler: "SDL".to_string(),
                family: family.to_string(),
            },
            ids,
        }
    }

    fn dualsense() -> Plugged {
        on_usb("PS5 Controller 0", Some((SONY, 0x0CE6)))
    }

    #[test]
    fn a_dualsense_goes_through_rpcs3s_own_handler_by_slot() {
        let buttons = BTreeMap::from([("South".to_string(), "East".to_string())]);
        let player = Player::with_buttons(dualsense().pad, buttons);
        let text = profile_text(&[player], &[dualsense()]);
        for line in [
            "  Handler: DualSense\n",
            "  Device: \"DualSense Pad #1\"\n",
            "    Cross: \"Circle\"\n",
            "    Circle: \"Circle\"\n",
            "    Square: \"Square\"\n",
            "    Triangle: \"Triangle\"\n",
            "    L1: \"L1\"\n",
            "    R2: \"R2\"\n",
            "    L3: \"L3\"\n",
            "    Select: \"Share\"\n",
            "    Start: \"Options\"\n",
            "    PS Button: \"PS Button\"\n",
            "    Up: \"Up\"\n",
            "    Left Stick Up: \"LS Y+\"\n",
            "    Left Stick Deadzone: 40\n",
            "    Right Stick Deadzone: 40\n",
        ] {
            assert!(text.contains(line), "{line:?} missing from\n{text}");
        }
        // The DualSense handler's "LB" is the Edge's back button.
        assert!(!text.contains("\"LB\""));
    }

    #[test]
    fn a_dualsense_player_reads_back_as_the_pad_plugged_in() {
        let buttons = BTreeMap::from([("South".to_string(), "East".to_string())]);
        let player = Player::with_buttons(dualsense().pad, buttons);
        let text = profile_text(&[player.clone()], &[dualsense()]);
        assert_eq!(parse_players(&text, &[dualsense()])[0], Some(player));
    }

    /// Unplugged, the player waits on the slot RPCS3 knows, as one on an
    /// XInput slot does, and keeps their buttons.
    #[test]
    fn a_dualsense_player_waits_on_its_slot_while_unplugged() {
        let buttons = BTreeMap::from([("South".to_string(), "East".to_string())]);
        let text = profile_text(&[Player::with_buttons(dualsense().pad, buttons)], &[dualsense()]);
        let back = parse_players(&text, &[])[0].clone().unwrap();
        assert_eq!(back.pad.device, "DualSense Pad #1");
        assert_eq!(back.pad.handler, "DualSense");
        assert_eq!(back.pad.family, "playstation");
        assert_eq!(back.pad.name, "DualSense");
        assert_eq!(back.input("South"), "East");
        assert_eq!(profile_text(&[back], &[]), text, "written again as it was");
    }

    #[test]
    fn sony_pads_are_numbered_by_handler() {
        let plugged = [
            dualsense(),
            on_usb("PS4 Controller 0", Some((SONY, 0x09CC))),
            on_usb("DualSense Edge 0", Some((SONY, 0x0DF2))),
            on_usb("PS4 Controller 1", Some((SONY, 0x0BA0))),
        ];
        assert_eq!(
            rpcs3_names(&plugged),
            [
                ("DualSense", "DualSense Pad #1".to_string()),
                ("DualShock 4", "DS4 Pad #1".to_string()),
                ("DualSense", "DualSense Pad #2".to_string()),
                ("DualShock 4", "DS4 Pad #2".to_string()),
            ]
        );
    }

    /// RPCS3 counts SDL pads of one name from 1, and a pad SDL's own drivers
    /// read goes by the driver's name whatever gilrs calls it.
    #[test]
    fn sdl_pads_are_named_as_sdl_names_them_and_counted_from_one() {
        let plugged = [
            on_usb("Pro Controller 0", Some((0x057E, 0x2009))),
            on_usb("8BitDo Pro 2 0", Some((0x2DC8, 0x6003))),
            on_usb("HORIPAD S 0", Some((0x0F0D, 0x00C1))),
            on_usb("HORIPAD S 1", Some((0x0F0D, 0x00C1))),
            on_usb("Unknown Pad 0", None),
        ];
        assert_eq!(
            rpcs3_names(&plugged),
            [
                ("SDL", "Nintendo Switch Pro Controller 1".to_string()),
                ("SDL", "8BitDo Pro 2 1".to_string()),
                ("SDL", "HORIPAD S 1".to_string()),
                ("SDL", "HORIPAD S 2".to_string()),
                ("SDL", "Unknown Pad 1".to_string()),
            ]
        );
        let text = profile_text(&[Player::on(plugged[0].pad.clone())], &plugged);
        assert!(text.contains("  Handler: SDL\n  Device: \"Nintendo Switch Pro Controller 1\"\n"));
        assert!(text.contains("    Cross: \"South\"\n"));
        assert_eq!(parse_players(&text, &plugged)[0].as_ref().unwrap().pad, plugged[0].pad);
    }

    /// Switched on after Play, a Sony pad still lands in the slot waiting for
    /// it. Any other pad keeps the name it has.
    #[test]
    fn a_sony_pad_not_plugged_in_is_named_where_rpcs3_will_find_it() {
        let away = on_usb("PS5 Controller 1", Some((SONY, 0x0CE6))).pad;
        let text = profile_text(&[Player::on(away)], &[dualsense()]);
        assert!(text.contains("  Handler: DualSense\n  Device: \"DualSense Pad #2\"\n"), "{text}");
        assert!(text.contains("    Cross: \"Cross\"\n"));
        let ds4 = on_usb("PS4 Controller 0", Some((SONY, 0x09CC))).pad;
        assert!(profile_text(&[Player::on(ds4)], &[]).contains("  Handler: DualShock 4\n  Device: \"DS4 Pad #1\"\n"));
        let pro = on_usb("Nintendo Switch Pro Controller 0", Some((0x057E, 0x2009))).pad;
        assert!(profile_text(&[Player::on(pro)], &[])
            .contains("  Handler: SDL\n  Device: \"Nintendo Switch Pro Controller 0\"\n"));
    }

    #[test]
    fn sony_pads_plugged_in_change_nothing_for_an_xbox_pad() {
        let player = Player::on(pad("XInput", "XInput Pad #1"));
        assert_eq!(
            profile_text(&[player.clone()], &[dualsense()]),
            profile_text(&[player], &[])
        );
    }

    #[test]
    fn a_handlers_own_name_spelled_like_a_place_is_not_read_as_it() {
        let text = "Player 1 Input:\n  Handler: DualSense\n  Device: \"DualSense Pad #1\"\n  Config:\n    R1: \"LB\"\n    Cross: \"Circle\"\n";
        let player = parse_players(text, &[])[0].clone().unwrap();
        assert_eq!(player.input("RB"), "RB", "the Edge's back button is no place of Omoio's");
        assert_eq!(player.input("South"), "East");
    }

    /// How RPCS3's log reads as it sets the pads up, in its own words, with
    /// lines around them that must not count.
    const SETUP_LOG: &str = "\u{b7}! 0:00:01.684565 {Pad Thread} Input: Using input configuration: '' (override='')\n\
        \u{b7}W 0:00:01.689000 {Pad Thread} SDL: Adding empty device: Nintendo Switch Pro Controller 1\n\
        \u{b7}! 0:00:01.689388 {Pad Thread} Input: Pad 0: device='Nintendo Switch Pro Controller 1', handler=SDL, VID=0x0, PID=0x0, class_type=0x0, class_profile=0x0\n\
        \u{b7}! 0:00:01.689699 {Pad Thread} Input: Pad 0: config=\n\
        Handler: SDL\n\
        Device: \"Nintendo Switch Pro Controller 1\"\n\
        \u{b7}E 0:00:01.690100 {Pad Thread} HID: DualShock 4 hid_open_path failed! error='Access is denied.', path='\\\\?\\hid#vid_054c&pid_09cc&mi_03'\n\
        \u{b7}E 0:00:01.690200 {Pad Thread} HID: One or more DualShock 4 pads were detected but couldn't be interacted with directly\r\n\
        \u{b7}E 0:00:01.690300 {Pad Thread} HID: Check https://wiki.rpcs3.net/index.php?title=Help:Controller_Configuration for instructions on how to solve this issue\n\
        \u{b7}E 0:00:01.690400 {Pad Thread} Input: PadHandlerBase::bindPadToDevice: no PadDevice found for device 'DualSense Pad #8'\n\
        \u{b7}E 0:00:01.690500 {Pad Thread} Input: Failed to bind device 'DualSense Pad #8' to handler DualSense. Falling back to NullPadHandler.\n\
        \u{b7}S 0:00:02.435919 {Pad Thread} Input: XInput device 0 connected\n";

    #[test]
    fn the_players_rpcs3_could_not_use_are_read_from_its_log() {
        let profile = "Player 1 Input:\n  Handler: SDL\n  Device: \"Nintendo Switch Pro Controller 1\"\n\
             Player 2 Input:\n  Handler: DualShock 4\n  Device: \"DS4 Pad #1\"\n\
             Player 3 Input:\n  Handler: XInput\n  Device: \"XInput Pad #1\"\n\
             Player 4 Input:\n  Handler: DualSense\n  Device: \"DualSense Pad #8\"\n";
        assert_eq!(
            unbound_players(SETUP_LOG, profile),
            [(0, Unbound::Missing), (1, Unbound::Held), (3, Unbound::Missing)]
        );
    }

    #[test]
    fn a_log_with_every_pad_found_names_nobody() {
        let profile = "Player 1 Input:\n  Handler: XInput\n  Device: \"XInput Pad #1\"\n\
             Player 2 Input:\n  Handler: DualSense\n  Device: \"DualSense Pad #1\"\n";
        assert!(unbound_players(SETUP_LOG, profile).is_empty());
    }

    #[test]
    fn the_notice_says_who_and_what_to_do() {
        assert_eq!(
            notice(0, "PS5 Controller", Unbound::Missing),
            "RPCS3 couldn't find player 1's controller, PS5 Controller, so the game won't answer it. Start the game again. If that doesn't help, the log on the Logs page shows what RPCS3 saw."
        );
        assert_eq!(
            notice(1, "PS4 Controller", Unbound::Held),
            "RPCS3 couldn't open player 2's controller, PS4 Controller. Close any other program using it, such as DS4Windows, then start the game again."
        );
    }

    #[test]
    fn a_name_with_punctuation_survives_being_written() {
        assert_eq!(quoted("Pad: v2"), "\"Pad: v2\"");
        assert_eq!(quoted("say \"hi\""), "\"say \\\"hi\\\"\"");
    }

    /// "Left Stick Left" starts with "Left", so a looser match would answer
    /// one with the other's value.
    #[test]
    fn a_key_is_not_matched_by_a_longer_one_starting_the_same_way() {
        let text = "    Left Stick Left: \"LS X-\"\n    Left: \"Left\"\n";
        assert_eq!(read_value(text, "Left").as_deref(), Some("Left"));
        assert_eq!(read_value(text, "Left Stick Left").as_deref(), Some("LS X-"));
    }
}
