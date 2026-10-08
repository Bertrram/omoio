//! Dolphin as the Wii's and the GameCube's emulator.
//!
//! One Dolphin runs both consoles, so it is listed twice in `backends::all`,
//! once for each, and everything here is shared between the two. Every name
//! here was read out of Dolphin's own source at the release Omoio installs
//! (release.rs) or measured on that release. What was found, and how, is in
//! docs/what-we-verified.md.
//!
//! A Wii disc is encrypted, and Dolphin decrypts it as it plays. Omoio reads
//! only the plain header at the front of a disc image, which names the game,
//! and never decrypts anything itself.

pub mod controllers;
pub mod copy;
pub mod disc;
pub mod ini;
pub mod install;
pub mod log;
pub mod portal;
pub mod release;
pub mod settings;

pub use install::install;

use crate::core::console::{Console, Features};
use crate::core::figures::{self, Character};
use crate::core::library::Game;
use crate::core::pad_layout::{Pad, Player};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use tauri::AppHandle;

pub struct Dolphin {
    console: Console,
}

pub static WII: Dolphin = Dolphin { console: Console::Wii };
pub static GAMECUBE: Dolphin = Dolphin { console: Console::GameCube };

fn console_of(platform: &disc::Platform) -> Console {
    match platform {
        disc::Platform::Wii => Console::Wii,
        disc::Platform::GameCube => Console::GameCube,
    }
}

/// A file's own name, lower case, wherever it sits.
fn file_name(name: &str) -> String {
    name.rsplit(['/', '\\']).next().unwrap_or(name).to_ascii_lowercase()
}

/// The one disc image picked, or the one inside the folder picked, at the
/// top or one level down, which is where an archive unpacks it. Two or more
/// is not clear, so none.
fn disc_image(picked: &Path) -> Option<PathBuf> {
    let is_image = |path: &Path| path.is_file() && disc::is_disc_name(&path.to_string_lossy());
    if is_image(picked) {
        return Some(picked.to_path_buf());
    }
    if !picked.is_dir() {
        return None;
    }
    let mut found = Vec::new();
    for entry in std::fs::read_dir(picked).ok()?.flatten() {
        let path = entry.path();
        if is_image(&path) {
            found.push(path);
        } else if path.is_dir() {
            if let Ok(inner) = std::fs::read_dir(&path) {
                found.extend(inner.flatten().map(|e| e.path()).filter(|p| is_image(p)));
            }
        }
    }
    (found.len() == 1).then(|| found.remove(0))
}

/// What a dump picked is, for this console: a disc image, or a disc's files
/// unpacked into a folder, as Dolphin itself reads both.
fn find(picked: &Path) -> Option<(PathBuf, disc::Disc)> {
    if picked.is_dir() {
        if let Some(found) = disc::read_folder(picked) {
            return Some((picked.to_path_buf(), found));
        }
    }
    let image = disc_image(picked)?;
    let found = disc::read(&image).ok()?;
    Some((image, found))
}

/// The library's id for a game: the six characters the disc names itself
/// by, such as SSPP52, and its disc number after them for a second disc, so
/// both discs of a two-disc game can be in the library.
fn library_id(found: &disc::Disc) -> String {
    if found.disc_number == 0 {
        found.game_id.clone()
    } else {
        format!("{}D{}", found.game_id, found.disc_number + 1)
    }
}

impl Dolphin {
    /// A Skylanders game on the Wii has the portal plugged in, and its
    /// window ready to be filled. The GameCube has no Skylanders game.
    fn wants_portal(&self, game: &Game) -> bool {
        self.console == Console::Wii && figures::is_skylanders(&game.title)
    }

    fn identify_here(&self, picked: &Path) -> Result<Game, String> {
        let (path, found) = match find(picked) {
            Some(found) => found,
            None => match disc_image(picked) {
                Some(image) => (image.clone(), disc::read(&image)?),
                None => return Err(NOT_A_DISC.to_string()),
            },
        };
        let console = console_of(&found.platform);
        let size_bytes = if path.is_dir() {
            crate::import::directory_size(&path)
        } else {
            std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
        };
        Ok(Game {
            console,
            title_id: library_id(&found),
            title: found.title.clone(),
            // The disc's revision, as Dolphin's own game list shows it.
            version: Some(found.revision.to_string()),
            update_version: None,
            size_bytes,
            path,
        })
    }
}

const NOT_A_DISC: &str = "This doesn't look like a Wii or GameCube disc image.";

impl super::EmulatorBackend for Dolphin {
    fn console(&self) -> Console {
        self.console
    }

    fn name(&self) -> &'static str {
        "Dolphin"
    }

    fn newest_version(&self) -> futures_util::future::BoxFuture<'static, Result<String, String>> {
        Box::pin(async { Ok(release::VERSION.to_string()) })
    }

    fn features(&self) -> Features {
        Features {
            portal: self.console == Console::Wii,
            quiet_behind: true,
            ..Features::default()
        }
    }

    /// By name and the header's few bytes, which say which console a disc is
    /// for; nothing is measured. Every emulator is asked about every import,
    /// and only the one that says yes reads the dump.
    fn recognises(&self, path: &Path) -> bool {
        find(path).is_some_and(|(_, found)| console_of(&found.platform) == self.console)
    }

    fn identify(&self, path: &Path) -> Result<Game, String> {
        self.identify_here(path)
    }

    fn is_game_file(&self, name: &str) -> bool {
        disc::is_disc_name(name)
    }

    /// A Wii channel (.wad) is installed into the console's own storage rather
    /// than played from a disc, and Omoio doesn't do that.
    fn refuses(&self, _app: &AppHandle, names: &[String]) -> Option<String> {
        if names.iter().any(|name| disc::is_disc_name(name)) {
            return None;
        }
        names.iter().any(|name| disc::is_channel_name(&file_name(name))).then(|| {
            "This is a Wii channel (.wad), which Omoio doesn't install. Omoio takes Wii and GameCube disc images.".to_string()
        })
    }

    /// A GameCube disc's own banner, which is plain on the disc. A Wii disc's
    /// is inside its encrypted part, so a Wii game gets the banner its save
    /// carries once it has been played and saved: the game writes it into
    /// Dolphin's copy of the Wii's storage, which Dolphin keeps plain.
    fn icon(&self, app: &AppHandle, game: &Game) -> Option<Vec<u8>> {
        match self.console {
            Console::GameCube => disc::gamecube_banner_png(&game.path),
            _ => {
                let found = find(&game.path)?.1;
                let title = disc::nand_title_folder(&found.game_id)?;
                let banner = install::user_dir(app).ok()?.join("Wii").join("title").join(title).join("data").join("banner.bin");
                disc::save_banner_png(&banner)
            }
        }
    }

    fn prepare(&self, app: &AppHandle, game: &Game) {
        let (Ok(user), Ok(figures)) = (install::user_dir(app), crate::portal_menu::folder(app)) else {
            return;
        };
        let config = user.join("Config");
        let _ = settings::prepare(&config, self.wants_portal(game), &figures);
        let _ = settings::quiet_hotkeys(&config);
        let _ = log::prepare(&user);
    }

    fn portal_figures(&self, pid: u32) -> Result<Vec<String>, String> {
        portal::figures(pid)
    }

    fn portal_load(&self, pid: u32, slot: usize, figure: &Path) -> Result<Vec<String>, String> {
        portal::load(pid, slot, figure)
    }

    fn portal_clear(&self, pid: u32, slot: usize) -> Result<Vec<String>, String> {
        portal::clear(pid, slot)
    }

    fn portal_create(&self, pid: u32, slot: usize, character: &Character, file: &Path) -> Result<Vec<String>, String> {
        portal::create(pid, slot, character, file)
    }

    /// Dolphin's figure maker makes a figure from any ID and Variant typed
    /// into it, and its list of characters shows their names alone, so
    /// Omoio offers the characters it has read from the other emulators'
    /// makers (`portal_menu::characters`).
    fn lists_characters(&self) -> bool {
        false
    }

    fn ready_portal(&self, pid: u32) {
        portal::ready(pid)
    }

    /// A game unpacked into a folder is read as it is. A disc image needs a
    /// copy first (`make_copy`): a Wii disc's files are encrypted.
    fn readable_copy(&self, _app: &AppHandle, game: &Game, _title_of: &dyn Fn(&Path) -> Option<String>) -> Option<PathBuf> {
        game.path.is_dir().then(|| game.path.clone())
    }

    fn copy_size(&self, game: &Game) -> Option<u64> {
        if game.path.is_dir() || self.console != Console::Wii {
            return None;
        }
        disc::read(&game.path).ok().map(|found| found.data_size)
    }

    fn make_copy(
        &self,
        app: &AppHandle,
        game: &Game,
        into: &Path,
        progress: &dyn Fn(u32),
        cancel: &AtomicBool,
    ) -> Result<PathBuf, String> {
        let tool = install::tool_path(app)?;
        if !tool.is_file() {
            return Err("Install Dolphin from the Emulators screen first.".to_string());
        }
        copy::extract(&tool, &game.path, &into.join("game"), progress, cancel)
    }

    fn button_names(&self) -> &'static [(&'static str, &'static str)] {
        controllers::button_names(self.console)
    }

    fn write_layout(&self, app: &AppHandle, title_id: &str, players: &[Player]) -> Result<(), String> {
        controllers::write(app, self.console, title_id, players)
    }

    fn missing_first_player(&self, _app: &AppHandle, players: &[Player], connected: &[Pad]) -> Option<String> {
        controllers::missing_first_player(players, connected)
    }

    fn forget_layout(&self, app: &AppHandle, title_id: &str) -> Result<(), String> {
        controllers::forget(app, self.console, title_id)
    }

    fn tune_picture(
        &self,
        app: &AppHandle,
        display_width: u32,
        display_height: u32,
        graphics_memory: u64,
    ) -> Result<Option<u32>, String> {
        let config = install::user_dir(app)?.join("Config");
        settings::size_picture(&config, display_width, display_height, graphics_memory).map_err(|e| e.to_string())
    }

    /// A Skylanders game starts with Dolphin's main window, which the portal
    /// window is opened from, hidden the moment it shows. Every other game
    /// starts in batch mode, where Dolphin never shows its main window and
    /// closes once the game has (`--batch`, `MainWindow::OnStopComplete`,
    /// DolphinQt). `--exec` names the game (UICommon/CommandLineParse.cpp).
    fn launch(&self, app: &AppHandle, game: &Game) -> Result<u32, String> {
        let exe = install::exe_path(app)?;
        if !exe.is_file() {
            return Err("Install Dolphin from the Emulators screen first, then you can play.".to_string());
        }
        if !game.path.exists() {
            return Err("This game isn't where it was. Reconnect the drive it's on.".to_string());
        }
        install::end_running(&exe);
        let portal = self.wants_portal(game);
        let mut command = install::command(&exe);
        if portal {
            command.env("QT_QPA_PLATFORM", portal::QT_PLATFORM);
        } else {
            command.arg("--batch");
        }
        let child = command.arg("--exec").arg(&game.path).spawn().map_err(|e| e.to_string())?;
        let pid = child.id();
        if portal {
            portal::prepare(pid);
        }
        Ok(pid)
    }

    fn detect_version(&self, app: &AppHandle) -> Option<String> {
        install::detect_version(app)
    }

    fn log_file(&self, app: &AppHandle) -> Option<PathBuf> {
        install::user_dir(app).ok().map(|user| log::path(&user))
    }

    fn is_game_window(&self, title: &str) -> bool {
        portal::is_game_title(title)
    }

    fn tidy_window(&self, pid: u32, game: isize) {
        portal::tidy(pid, game);
    }

    /// Omoio's menus keep the game deaf by taking the keyboard from its
    /// window, since Dolphin reads the pad only while its game's window is in
    /// front (`BackgroundInput` off, settings.rs). When it may hear again,
    /// the game gets the keyboard back if one of Dolphin's own windows has
    /// it, as the portal's file windows take it.
    fn hush(&self, pid: u32, hushed: bool) -> Result<(), String> {
        if !hushed {
            portal::give_back_to_game(pid);
        }
        Ok(())
    }
}
