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

use super::ini;
use crate::core::console::Console;
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
/// and B on the bottom and right buttons as the Wii's menus use them, B
/// again and the Nunchuk's Z on the triggers, where the fingers hold them on
/// the real thing, the Nunchuk's C and Z on the other two face buttons, a
/// shake of the remote on the right shoulder for games that ask for one,
/// and the pointer on the right stick. The home button stays Omoio's, for
/// the portal menu.
pub const WII: [(&str, &str); 24] = [
    ("South", "A"),
    ("East", "B"),
    ("RT", "B"),
    ("West", "Nunchuk Z"),
    ("LT", "Nunchuk Z"),
    ("North", "Nunchuk C"),
    ("RB", "Shake"),
    ("LB", "1"),
    ("LS", "2"),
    ("Back", "Minus"),
    ("Start", "Plus"),
    ("RS", "Home"),
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
    ("Buttons/B", &["East", "RT"]),
    ("Buttons/1", &["LB"]),
    ("Buttons/2", &["LS"]),
    ("Buttons/-", &["Back"]),
    ("Buttons/+", &["Start"]),
    ("Buttons/Home", &["RS"]),
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
/// on their pad, joined with `|` so any of them presses it.
fn expression(player: &Player, places: &[&str]) -> String {
    places
        .iter()
        .filter_map(|place| control(player.input(place)))
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(" | ")
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

/// A pad's USB vendor and product ids and how many of that model come
/// before it, as `pads::usb_ids` gives them.
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
    if let Some(slot) = xinput_slot(pad) {
        return Some(format!("XInput/{slot}/Gamepad"));
    }
    let (vendor, product, before) = ids(&pad.device)?;
    Some(format!("SDL/{before}/{}", sdl_name(vendor, product)?))
}

/// One player's settings for Dolphin, the lines of their section.
fn lines(console: Console, player: &Player, device: Option<&str>, emulated: bool) -> Vec<String> {
    let mut lines = vec![format!("Device = {}", device.unwrap_or(""))];
    let keys: &[(&str, &[&str])] = match console {
        Console::GameCube => &GAMECUBE_KEYS,
        _ => &WII_KEYS,
    };
    for (key, places) in keys {
        lines.push(format!("{key} = {}", expression(player, places)));
    }
    match console {
        Console::GameCube => {
            // As Dolphin's own gamepad profile has them, so a stick reaches
            // its whole circle.
            lines.push("Main Stick/Calibration = 100.00".to_string());
            lines.push("C-Stick/Calibration = 100.00".to_string());
        }
        _ => {
            lines.push(format!("Source = {}", if emulated { 1 } else { 0 }));
            lines.push("Extension = Nunchuk".to_string());
            // The pointer moves with the stick and stays where it is left,
            // and goes out of sight when the stick is left alone.
            lines.push("IR/Relative Input = True".to_string());
            lines.push("IR/Auto-Hide = True".to_string());
        }
    }
    lines.push("Rumble/Motor = `Motor L` | `Motor R`".to_string());
    lines
}

/// The whole of GCPadNew.ini or WiimoteNew.ini for these players.
fn file(console: Console, players: &[Player], devices: &[Option<String>], emulated: &[bool]) -> String {
    let section = if console == Console::GameCube { "GCPad" } else { "Wiimote" };
    let mut text = String::new();
    for (at, player) in players.iter().enumerate().take(PLAYERS) {
        text.push_str(&format!("[{section}{}]\r\n", at + 1));
        let device = devices.get(at).and_then(Option::as_deref);
        let on = emulated.get(at).copied().unwrap_or(false);
        for line in lines(console, player, device, on) {
            text.push_str(&line);
            text.push_str("\r\n");
        }
    }
    text
}

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
/// names itself by, from the library's id, which adds a second disc's number.
fn game_id(title_id: &str) -> Option<&str> {
    title_id.get(..6).filter(|id| id.chars().all(|c| c.is_ascii_alphanumeric()))
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
    if usable(first) {
        return players;
    }
    let mut xinput = plugged.iter().filter(|pad| xinput_slot(pad).is_some());
    let free = xinput.clone().find(|pad| !players.iter().any(|p| p.pad.device == pad.device));
    if let Some(pad) = free.or_else(|| xinput.next()) {
        let buttons = first.buttons.clone();
        pad_layout::give(&mut players, 0, Player { pad: pad.clone(), buttons });
    }
    players
}

/// Writes the players' layout into Dolphin's files for `console`. An empty
/// `title_id` is the layout for every game: GCPadNew.ini or WiimoteNew.ini.
/// A game's own goes in profiles its settings file names. A Wii Remote is
/// switched on for player 1 and for every player whose pad is plugged in;
/// one switched on with nothing to answer it would look to a game like a
/// player who never presses anything.
pub fn write(app: &AppHandle, console: Console, title_id: &str, players: &[Player]) -> Result<(), String> {
    let config = config_dir(app)?;
    if !super::install::exe_path(app)?.is_file() {
        return Ok(());
    }
    let plugged = crate::pads::connected();
    let ids = |device: &str| crate::pads::usb_ids(device);
    let usable = |player: &Player| device(&player.pad, &ids).is_some();
    let players = stand_in(players, &plugged, &usable);
    let devices: Vec<Option<String>> = players.iter().map(|player| device(&player.pad, &ids)).collect();
    let emulated: Vec<bool> = players
        .iter()
        .enumerate()
        .map(|(at, player)| at == 0 || plugged.iter().any(|pad| pad.device == player.pad.device))
        .collect();
    let failed = |_| "Couldn't save the controller settings for Dolphin.".to_string();
    if title_id.is_empty() {
        let name = if console == Console::GameCube { "GCPadNew.ini" } else { "WiimoteNew.ini" };
        std::fs::create_dir_all(&config).map_err(failed)?;
        std::fs::write(config.join(name), file(console, &players, &devices, &emulated)).map_err(failed)?;
        if console == Console::GameCube {
            standard_controllers(&config.join("Dolphin.ini")).map_err(failed)?;
        }
        return Ok(());
    }
    let Some(game) = game_id(title_id) else {
        return Ok(());
    };
    let (folder, key) = profile_kind(console);
    let profiles = config.join("Profiles").join(folder);
    std::fs::create_dir_all(&profiles).map_err(failed)?;
    let mut settings: Vec<(String, String)> = Vec::new();
    for (at, player) in players.iter().enumerate().take(PLAYERS) {
        let name = profile_name(game, at);
        let text = profile(console, player, devices[at].as_deref());
        std::fs::write(profiles.join(format!("{name}.ini")), text).map_err(failed)?;
        settings.push((format!("{key}Profile{}", at + 1), name));
        if console != Console::GameCube {
            // Counted from 0 here (`GameConfigLoader.cpp`).
            settings.push((format!("WiimoteSource{at}"), if emulated[at] { "1" } else { "0" }.to_string()));
        }
    }
    let values: Vec<(&str, &str, &str)> = settings.iter().map(|(k, v)| ("Controls", k.as_str(), v.as_str())).collect();
    let game_settings = super::install::user_dir(app)?.join("GameSettings").join(format!("{game}.ini"));
    ini::update(&game_settings, &values).map_err(failed)
}

/// A standard GameCube pad in every port Dolphin has left empty, so a second
/// player's pad answers when it is plugged in. A port with a pad set to
/// nothing is reported to the game as unplugged (`GCPadEmu.cpp`), so a port
/// with no one on it is no different to a game. Dolphin numbers a standard
/// pad 6 and leaves ports 2 to 4 empty (`SIDevices`, Core/HW/SI/SI_Device.h;
/// `SIDevice0` to 3 in Core/Config/MainSettings.cpp). A port the user set
/// to something else is left as it is.
fn standard_controllers(dolphin_ini: &Path) -> std::io::Result<()> {
    let text = std::fs::read_to_string(dolphin_ini).unwrap_or_default();
    let keys = ["SIDevice0", "SIDevice1", "SIDevice2", "SIDevice3"];
    let empty: Vec<(&str, &str, &str)> = keys
        .iter()
        .filter(|key| ini::get(&text, "Core", key).is_none_or(|value| value == "0"))
        .map(|key| ("Core", *key, "6"))
        .collect();
    ini::update(dolphin_ini, &empty)
}

/// Takes a game's own layout out of Dolphin: its profiles and the settings
/// naming them, so it goes back to the layout for every game.
pub fn forget(app: &AppHandle, console: Console, title_id: &str) -> Result<(), String> {
    let Some(game) = game_id(title_id) else {
        return Ok(());
    };
    let (folder, key) = profile_kind(console);
    let user = super::install::user_dir(app)?;
    let profiles = user.join("Config").join("Profiles").join(folder);
    let game_settings = user.join("GameSettings").join(format!("{game}.ini"));
    let mut text = std::fs::read_to_string(&game_settings).unwrap_or_default();
    for at in 0..PLAYERS {
        let _ = std::fs::remove_file(profiles.join(format!("{}.ini", profile_name(game, at))));
        text = ini::remove(&text, "Controls", &format!("{key}Profile{}", at + 1));
        if console != Console::GameCube {
            text = ini::remove(&text, "Controls", &format!("WiimoteSource{at}"));
        }
    }
    if game_settings.is_file() {
        std::fs::write(&game_settings, text).map_err(|_| "Couldn't save the controller settings for Dolphin.".to_string())?;
    }
    Ok(())
}

/// Why player 1 would have no pad in Dolphin with these `players`, after
/// `stand_in` has had its go with the pads `connected`, worded for the
/// person about to press Play. `None` when they have one.
pub fn missing_first_player(players: &[Player], connected: &[Pad]) -> Option<String> {
    let ids = |device: &str| crate::pads::usb_ids(device);
    missing(players, connected, &|player: &Player| device(&player.pad, &ids).is_some())
}

fn missing(players: &[Player], connected: &[Pad], usable: &dyn Fn(&Player) -> bool) -> Option<String> {
    let first = players.first()?;
    if stand_in(players, connected, usable).first().is_some_and(usable) {
        return None;
    }
    Some(format!(
        "Dolphin can't use player 1's controller, {}, so the game won't answer it. \
         Plug in an Xbox, PlayStation or Switch Pro controller, or choose another for player 1 on the Controller screen.",
        first.pad.name
    ))
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
        let text = file(Console::Wii, &[player], &devices, &[true]);
        assert_eq!(
            text,
            "[Wiimote1]\r\n\
             Device = XInput/0/Gamepad\r\n\
             Buttons/A = `Button A`\r\n\
             Buttons/B = `Button B` | `Trigger R`\r\n\
             Buttons/1 = `Shoulder L`\r\n\
             Buttons/2 = `Thumb L`\r\n\
             Buttons/- = `Back`\r\n\
             Buttons/+ = `Start`\r\n\
             Buttons/Home = `Thumb R`\r\n\
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
        let text = file(Console::GameCube, &[player], &devices, &[true]);
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
