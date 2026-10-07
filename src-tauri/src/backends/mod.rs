//! Everything specific to one emulator lives behind `EmulatorBackend`.
//!
//! Each game records its console, and the console picks the backend. Import,
//! launch and cover art go through here, so adding an emulator means adding an
//! implementation and a line in `all`, not touching the library or a screen.

pub mod cemu;
pub mod rpcs3;

use crate::core::community::{PackChange, Packs};
use crate::core::console::{Console, Features};
use crate::core::game_settings::{Chosen, GameSettings};
use crate::core::import_warning::Imported;
use crate::core::library::Game;
use crate::core::figures::Character;
use crate::core::pad_layout::{Pad, Player};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use tauri::AppHandle;

pub trait EmulatorBackend: Sync {
    fn console(&self) -> Console;

    /// The emulator's own name, for the Emulators screen and for messages
    /// about updating it.
    fn name(&self) -> &'static str;

    /// The newest official release, in the form `detect_version` reports, so
    /// the two can be compared. Boxed for the same reason as
    /// `refresh_catalogue`; `'static` because it borrows nothing.
    fn newest_version(&self) -> futures_util::future::BoxFuture<'static, Result<String, String>>;

    fn features(&self) -> Features;

    /// Whether this looks like one of this console's dumps. It must not read
    /// or measure the dump: every backend is asked about every import.
    fn recognises(&self, path: &Path) -> bool;

    fn identify(&self, path: &Path) -> Result<Game, String>;

    /// Whether a file, by its name alone, is a whole game of this console's
    /// that a scan of a drive should take, such as a Wii U .wua. A scan meets
    /// thousands of files, so this must not open them.
    fn is_game_file(&self, _name: &str) -> bool {
        false
    }

    /// Which of this console's games an archive holds, from the one small
    /// file in it that says, so it is known before anything is unpacked.
    /// `read` gives a file by its name in `names`, or `None` when getting it
    /// out would mean unpacking much of the archive.
    fn identify_packed(&self, _names: &[String], _read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Option<Imported> {
        None
    }

    /// Why a dump with these file names cannot be taken, when it is one of
    /// this console's in a form that needs something the user has not given.
    /// Asked with only the names, so an archive is answered before anything
    /// is unpacked.
    fn refuses(&self, _app: &AppHandle, _names: &[String]) -> Option<String> {
        None
    }

    /// The game's own picture, as the bytes of a PNG file: the one its dump
    /// ships, or, when the dump can't be read, one the emulator keeps for it.
    /// Bytes rather than a path, because the picture may be in a form the
    /// webview can't show and is turned into a PNG first, as a Wii U game's
    /// is.
    fn icon(&self, app: &AppHandle, game: &Game) -> Option<Vec<u8>>;

    /// Gets anything the emulator needs ready before `game` starts, apart
    /// from the controller layout, which `write_layout` hands over.
    fn prepare(&self, _app: &AppHandle, _game: &Game) {}

    /// The figures on the running game's toy portal, by slot, empty where
    /// there is none. `pid` is the emulator's process.
    fn portal_figures(&self, _pid: u32) -> Result<Vec<String>, String> {
        Err(NO_PORTAL.to_string())
    }

    /// Puts the figure in `figure` on the portal in `slot`, counted from 0,
    /// and returns what the portal holds afterwards.
    fn portal_load(&self, _pid: u32, _slot: usize, _figure: &Path) -> Result<Vec<String>, String> {
        Err(NO_PORTAL.to_string())
    }

    /// Takes the figure in `slot` off the portal, and returns what is left.
    fn portal_clear(&self, _pid: u32, _slot: usize) -> Result<Vec<String>, String> {
        Err(NO_PORTAL.to_string())
    }

    /// Every character the emulator's own figure maker can make.
    fn portal_characters(&self, _pid: u32) -> Result<Vec<Character>, String> {
        Err(NO_PORTAL.to_string())
    }

    /// Has the emulator's figure maker make a figure of `character` into
    /// `file` and put it on the portal in `slot`. Returns what the portal
    /// holds afterwards.
    fn portal_create(&self, _pid: u32, _slot: usize, _character: &Character, _file: &Path) -> Result<Vec<String>, String> {
        Err(NO_PORTAL.to_string())
    }

    /// Gets the emulator's own portal window open as the portal menu opens,
    /// before the game is hushed. An emulator that needs nothing leaves this.
    fn ready_portal(&self, _pid: u32) {}

    /// A copy of the game whose files Omoio can read, for the figures'
    /// pictures: decrypted, as an archive or an unpacked folder. `title_of`
    /// says which game such an archive holds, so another game's is never
    /// taken. `None` when there is no such copy.
    fn readable_copy(&self, _app: &AppHandle, _game: &Game, _title_of: &dyn Fn(&Path) -> Option<String>) -> Option<PathBuf> {
        None
    }

    /// The room a readable copy of the game would take, when there may be
    /// none and the emulator can make one: about the size of the game.
    /// `None` when the game's own files are read as they are, or the
    /// emulator can't make a copy.
    fn copy_size(&self, _game: &Game) -> Option<u64> {
        None
    }

    /// Has the emulator make a readable copy of the game at `into`, for the
    /// figures' pictures. `progress` hears how far it is, out of 100, and
    /// `cancel` stops it.
    fn make_copy(
        &self,
        _app: &AppHandle,
        _game: &Game,
        _into: &Path,
        _progress: &dyn Fn(u32),
        _cancel: &AtomicBool,
    ) -> Result<(), String> {
        Err("This emulator can't make a copy of the game.".to_string())
    }

    /// What this console calls each place on a pad, for the Controller
    /// screen. A place the emulator cannot use is left out.
    fn button_names(&self) -> &'static [(&'static str, &'static str)];

    /// Writes the players' layout into the emulator's own files. An empty
    /// `title_id` is the layout for every game.
    fn write_layout(&self, app: &AppHandle, title_id: &str, players: &[Player]) -> Result<(), String>;

    /// Why player 1 would have no pad in the emulator if a game started with
    /// these `players`, with `connected` the pads plugged in, worded for the
    /// person about to play. `None` when they have one.
    fn missing_first_player(&self, _app: &AppHandle, _players: &[Player], _connected: &[Pad]) -> Option<String> {
        None
    }

    /// Takes a game's own layout out of the emulator's files.
    fn forget_layout(&self, _app: &AppHandle, _title_id: &str) -> Result<(), String> {
        Ok(())
    }

    /// Layouts the emulator had before Omoio kept its own, by title id, empty
    /// for every game.
    fn existing_layouts(&self, _app: &AppHandle) -> Vec<(String, Vec<Player>)> {
        Vec::new()
    }

    /// Sizes the picture for this machine if it has not been. Returns the
    /// scale that applies, or `None` when the user already chose their own.
    fn tune_picture(
        &self,
        app: &AppHandle,
        display_height: u32,
        graphics_memory: u64,
    ) -> Result<Option<u32>, String>;

    /// Starts the game and returns the emulator's process id.
    fn launch(&self, app: &AppHandle, game: &Game) -> Result<u32, String>;

    /// The installed version, or `None` when it is not installed.
    fn detect_version(&self, app: &AppHandle) -> Option<String>;

    /// Where the emulator writes the log of the session that just ran.
    fn log_file(&self, app: &AppHandle) -> Option<PathBuf>;

    /// The game's version as the emulator's log of a session gives it, for a
    /// game whose own files Omoio can't read.
    fn version_from_log(&self, _log: &str) -> Option<String> {
        None
    }

    /// Whether a window of the running emulator that has no owner, by its
    /// title, is the game's, for Omoio to take into its own window. Most
    /// emulators show no other window like it.
    fn is_game_window(&self, _title: &str) -> bool {
        true
    }

    /// Called again and again while a game runs, with the game's window, for
    /// anything the emulator puts on the screen that doesn't belong over the
    /// game.
    fn tidy_window(&self, _pid: u32, _game: isize) {}

    /// Stops the running game hearing the pad while one of Omoio's menus is
    /// over it, or lets it hear again. An emulator that needs nothing done,
    /// or can't be told, leaves this alone.
    fn hush(&self, _pid: u32, _hushed: bool) -> Result<(), String> {
        Ok(())
    }

    /// The emulator's own settings for one game, for the settings sheet.
    fn game_settings(&self, _app: &AppHandle, _game: &Game) -> Result<GameSettings, String> {
        Err("This emulator has no settings of its own for a game.".to_string())
    }

    /// Saves only what was chosen. Choosing nothing puts the game back on
    /// the emulator's own settings.
    fn set_game_settings(&self, _app: &AppHandle, _game: &Game, _chosen: &Chosen) -> Result<(), String> {
        Err("This emulator has no settings of its own for a game.".to_string())
    }

    /// Every title this console's compatibility list knows, or `None` until
    /// the list has been downloaded.
    fn catalogue(&self, app: &AppHandle) -> Option<Vec<crate::core::catalogue::Entry>>;

    /// Downloads the list again and returns how many titles it holds. Boxed
    /// because a trait used through `dyn` cannot declare an `async fn`; the
    /// box is where the download's state lives while it runs.
    fn refresh_catalogue<'a>(
        &'a self,
        app: &'a AppHandle,
        cancel: &'a std::sync::atomic::AtomicBool,
    ) -> futures_util::future::BoxFuture<'a, Result<usize, String>>;

    /// Who publishes the list and where, for the credit under the catalogue.
    fn catalogue_source(&self) -> (&'static str, &'static str);

    /// Switches on the known fixes for this game that have not been applied
    /// before, and returns their ids so none is ever applied twice. Most
    /// emulators have none, so this does nothing unless one overrides it.
    fn apply_fixes(&self, _app: &AppHandle, _game: &Game, _applied: &[String]) -> Vec<&'static str> {
        Vec::new()
    }

    /// What the emulator's community publishes for a game, such as patches
    /// or graphic packs, and which are on. `game` is the library's copy when
    /// the game is in the library; one from the catalogue has only its id.
    fn community_packs(&self, _app: &AppHandle, _title_id: &str, _game: Option<&Game>) -> Packs {
        Packs::default()
    }

    /// Switches one pack on or off, with the choices made in it.
    fn set_community_pack(&self, _app: &AppHandle, _game: &Game, _change: &PackChange) -> Result<(), String> {
        Err(NO_PACKS.to_string())
    }

    /// Downloads the newest packs and returns how many there are. Boxed for
    /// the same reason as `refresh_catalogue`.
    fn refresh_community<'a>(
        &'a self,
        _app: &'a AppHandle,
        _cancel: &'a AtomicBool,
    ) -> futures_util::future::BoxFuture<'a, Result<usize, String>> {
        Box::pin(async { Err(NO_PACKS.to_string()) })
    }
}

const NO_PORTAL: &str = "Omoio can't reach this emulator's portal yet.";
const NO_PACKS: &str = "There are no community packs for this emulator.";

/// Every emulator Omoio can run, one per console.
pub fn all() -> &'static [&'static dyn EmulatorBackend] {
    &[&rpcs3::Rpcs3, &cemu::Cemu]
}

pub fn for_console(console: Console) -> Option<&'static dyn EmulatorBackend> {
    all().iter().copied().find(|backend| backend.console() == console)
}

/// Works out which console a dump is for and reads it with that emulator.
pub fn identify(path: &Path) -> Result<Game, String> {
    match all().iter().find(|backend| backend.recognises(path)) {
        Some(backend) => backend.identify(path),
        None => Err(unknown_dump(all().iter().map(|backend| backend.console().short()))),
    }
}

/// Whether any emulator takes this one file as a whole game.
pub fn is_game_file(name: &str) -> bool {
    all().iter().any(|backend| backend.is_game_file(name))
}

/// Which game an archive holds, by whichever emulator can tell, before it is
/// unpacked.
pub fn identify_packed(names: &[String], read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Option<Imported> {
    all().iter().find_map(|backend| backend.identify_packed(names, read))
}

/// Why no emulator can take a dump with these file names, if one knows why.
pub fn refuses(app: &AppHandle, names: &[String]) -> Option<String> {
    all().iter().find_map(|backend| backend.refuses(app, names))
}

/// The names of what was picked: the file itself, or what a folder holds
/// and one level below, which is where a dump sits inside the folder an
/// archive unpacked into.
pub fn names_in(path: &Path) -> Vec<String> {
    let list = |dir: &Path| -> Vec<std::path::PathBuf> {
        std::fs::read_dir(dir)
            .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
            .unwrap_or_default()
    };
    let mut found = vec![path.to_path_buf()];
    if path.is_dir() {
        for inside in list(path) {
            if inside.is_dir() {
                found.extend(list(&inside));
            }
            found.push(inside);
        }
    }
    found
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// Says which consoles Omoio takes, so an unknown folder gets an answer that
/// helps rather than one that only says no.
fn unknown_dump<'a>(consoles: impl Iterator<Item = &'a str>) -> String {
    let names: Vec<&str> = consoles.collect();
    let list = match names.as_slice() {
        [] => String::new(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    format!("This doesn't look like a game Omoio can play. It takes {list} games.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_dump_is_told_which_consoles_are_taken() {
        assert_eq!(
            unknown_dump(["PS3"].into_iter()),
            "This doesn't look like a game Omoio can play. It takes PS3 games."
        );
        assert_eq!(
            unknown_dump(["PS3", "Wii U"].into_iter()),
            "This doesn't look like a game Omoio can play. It takes PS3 and Wii U games."
        );
        assert_eq!(
            unknown_dump(["PS3", "Wii U", "PS2"].into_iter()),
            "This doesn't look like a game Omoio can play. It takes PS3, Wii U and PS2 games."
        );
    }

    #[test]
    fn every_console_has_at_most_one_emulator() {
        let mut seen = std::collections::HashSet::new();
        for backend in all() {
            assert!(seen.insert(backend.console()), "{:?} twice", backend.console());
        }
        assert!(for_console(Console::Ps3).is_some());
        assert!(for_console(Console::WiiU).is_some());
    }
}
