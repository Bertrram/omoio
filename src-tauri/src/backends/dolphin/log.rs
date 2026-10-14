//! Dolphin's log of a session, kept by Omoio after the game like every
//! emulator's.
//!
//! Dolphin writes a log file only when asked to, and then only the kinds of
//! message switched on one by one, at the level asked for: Logger.ini's
//! `[Options]` `WriteToFile` and `Verbosity`, and one line per kind under
//! `[Logs]`, all off as it ships (`LogManager`, Source/Core/Common/Logging/
//! LogManager.cpp, Dolphin 2609a). It adds to `Logs/dolphin.log` without
//! ever starting it afresh (`std::ios::app`), so Omoio empties it before each
//! game, and what Omoio keeps afterwards is that game's alone.

use super::ini;
use std::path::{Path, PathBuf};

/// Notices, errors and warnings (`LogLevel`, Common/Logging/Log.h: 1 to 3).
const VERBOSITY: &str = "3";

/// The kinds of message that say how a game started and what went wrong in
/// it: booting the disc, Dolphin's core and common code, its picture, the
/// emulated USB devices, which the Skylanders portal is one of, the master
/// log, where Dolphin puts the warnings it no longer shows as boxes
/// (settings.rs; `ShowMessageAlert`, Common/MsgHandler.cpp, writes each one
/// there as a warning), and its controller interface, which notes each pad
/// it finds (`added_devices`). Each is the short name Dolphin files it
/// under (`LogManager::LogManager`, the controller interface's "CI").
const KINDS: [&str; 7] = ["BOOT", "CORE", "COMMON", "Video", "IOS_USB", "MASTER", "CI"];

pub fn path(user: &Path) -> PathBuf {
    user.join("Logs").join("dolphin.log")
}

/// Every device Dolphin found while the log was kept, by the name a layout
/// binds it with, such as `SDL/0/DualSense Wireless Controller`. Dolphin
/// notes each one as it takes it in, "Added device: " and that name, as a
/// notice of its controller interface (`ControllerInterface::AddDevice`,
/// InputCommon/ControllerInterface/ControllerInterface.cpp line 273, the
/// name from `Device::GetQualifiedName`, CoreDevice.cpp line 85; Dolphin
/// 2609a, read 14 October 2026), and the log file gets each line as it is
/// logged (`FileLogListener::Log` flushes it). The pads plugged in as
/// Dolphin starts are taken in before its main window opens, and so before
/// any game (`UICommon::InitControllers`, called from `MainWindow`'s
/// constructor), and those plugged in later as they arrive.
pub fn added_devices(log: &str) -> Vec<&str> {
    log.lines()
        .filter_map(|line| line.split_once("Added device: "))
        .map(|(_, device)| device.trim_end())
        .filter(|device| !device.is_empty())
        .collect()
}

/// Has Dolphin write the log, and empties the last game's.
pub fn prepare(user: &Path) -> std::io::Result<()> {
    let mut values: Vec<(&str, &str, &str)> = vec![("Options", "WriteToFile", "True"), ("Options", "Verbosity", VERBOSITY)];
    values.extend(KINDS.iter().map(|kind| ("Logs", *kind, "True")));
    ini::update(&user.join("Config").join("Logger.ini"), &values)?;
    match std::fs::remove_file(path(user)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_is_switched_on_and_started_afresh() {
        let user = std::env::temp_dir().join(format!("omoio-dolphin-log-{}", std::process::id()));
        std::fs::create_dir_all(user.join("Logs")).unwrap();
        std::fs::write(path(&user), "the last game\n").unwrap();
        prepare(&user).unwrap();
        assert!(!path(&user).exists());
        let text = std::fs::read_to_string(user.join("Config").join("Logger.ini")).unwrap();
        assert_eq!(ini::get(&text, "Options", "WriteToFile").as_deref(), Some("True"));
        assert_eq!(ini::get(&text, "Logs", "BOOT").as_deref(), Some("True"));
        assert_eq!(ini::get(&text, "Logs", "MASTER").as_deref(), Some("True"), "the warnings Dolphin doesn't show");
        assert_eq!(ini::get(&text, "Logs", "CI").as_deref(), Some("True"), "the pads Dolphin finds");
        prepare(&user).unwrap();
        let _ = std::fs::remove_dir_all(&user);
    }

    #[test]
    fn the_devices_dolphin_found_are_read_from_its_log() {
        let log = "13:05:002 InputCommon\\ControllerInterface\\ControllerInterface.cpp:273 N[CI]: Added device: DInput/0/Keyboard Mouse\r\n\
                   13:05:004 InputCommon\\ControllerInterface\\ControllerInterface.cpp:273 N[CI]: Added device: XInput/0/Gamepad\r\n\
                   13:05:010 Core\\Boot\\Boot.cpp:500 N[BOOT]: Booting the disc\r\n\
                   13:05:020 InputCommon\\ControllerInterface\\ControllerInterface.cpp:273 N[CI]: Added device: SDL/0/DualSense Wireless Controller\r\n";
        assert_eq!(
            added_devices(log),
            ["DInput/0/Keyboard Mouse", "XInput/0/Gamepad", "SDL/0/DualSense Wireless Controller"]
        );
        assert!(added_devices("").is_empty());
    }
}
