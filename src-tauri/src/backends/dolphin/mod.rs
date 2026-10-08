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

pub mod compat;
pub mod controllers;
pub mod copy;
pub mod disc;
pub mod ini;
pub mod install;
pub mod log;
pub mod mods;
pub mod packs;
pub mod portal;
pub mod release;
pub mod saves;
pub mod settings;

pub use install::install;

use crate::core::community::{PackChange, Packs};
use crate::core::console::{Console, Features};
use crate::core::figures::{self, Character};
use crate::core::library::{Game, Library};
use crate::core::pad_layout::{Pad, Player};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use tauri::{AppHandle, Manager};

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

/// Where a Wii game's save keeps its banner in Dolphin's copy of the Wii's
/// storage, from the library's id, whose first four characters are the
/// title's (`disc::nand_title_folder`).
fn save_banner(user: &Path, title_id: &str) -> Option<PathBuf> {
    let title = disc::nand_title_folder(title_id)?;
    Some(user.join("Wii").join("title").join(title).join("data").join("banner.bin"))
}

/// The disc's revision, which the library keeps as the game's version.
fn revision(game: &Game) -> Option<u16> {
    game.version.as_deref()?.parse().ok()
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

/// The six characters a disc shares with the game's other discs, and its
/// number, 1 for the first, from its library id (`library_id`).
fn disc_of(title_id: &str) -> Option<(&str, u32)> {
    let game = title_id.get(..6)?;
    match &title_id[6..] {
        "" => Some((game, 1)),
        rest => Some((game, rest.strip_prefix('D')?.parse().ok()?)),
    }
}

/// Every disc Dolphin is given for `game`: the one started, then the
/// game's other discs in the library, each the one after the disc before
/// and round to the first. Given more than one, Dolphin puts the next in
/// by itself when the game asks for another disc
/// (`DVDInterface::AutoChangeDisc`, Core/HW/DVD/DVDInterface.cpp, Dolphin
/// 2609a); its main window, which could change one by hand, isn't shown.
/// Another disc is one of the same console's with the same six characters,
/// a disc image that is there. A game unpacked into a folder is given alone.
fn discs<'a>(game: &'a Game, library: &'a [Game]) -> Vec<&'a Path> {
    let mut all = vec![game.path.as_path()];
    let Some((id, number)) = disc_of(&game.title_id).filter(|_| game.path.is_file()) else {
        return all;
    };
    let mut others: Vec<(u32, &Path)> = library
        .iter()
        .filter(|other| other.console == game.console && other.path.is_file())
        .filter_map(|other| {
            let (other_id, other_number) = disc_of(&other.title_id)?;
            (other_id == id && other_number != number).then_some((other_number, other.path.as_path()))
        })
        .collect();
    others.sort_by_key(|&(other_number, _)| (other_number < number, other_number));
    all.extend(others.into_iter().map(|(_, path)| path));
    all
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
            packs: true,
            portal: self.console == Console::Wii,
            saves: true,
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
    /// Dolphin's copy of the Wii's storage, which Dolphin keeps plain. The
    /// library asks on every load until there is one, so it is found by the
    /// game's id, without opening the dump (`save_banner`).
    fn icon(&self, app: &AppHandle, game: &Game) -> Option<Vec<u8>> {
        match self.console {
            Console::GameCube => disc::gamecube_banner_png(&game.path),
            _ => disc::save_banner_png(&save_banner(&install::user_dir(app).ok()?, &game.title_id)?),
        }
    }

    /// A Dolphin still running is ended first (`install::end_running`): on
    /// its way out it would write its own settings over these
    /// (`MainWindow::~MainWindow`, DolphinQt/MainWindow.cpp), and it holds
    /// the log this empties. A Dolphin.ini that can't be changed stops the
    /// start, since the game would come up behind Dolphin's own questions
    /// and warnings, and a Skylanders game without its portal. A Hotkeys.ini
    /// or a log left as it was harms nothing, and is tried again next time.
    fn prepare(&self, app: &AppHandle, game: &Game) -> Result<(), String> {
        let Ok(exe) = install::exe_path(app) else {
            return Ok(());
        };
        if !exe.is_file() {
            return Ok(());
        }
        install::end_running(&exe);
        let (Ok(user), Ok(figures)) = (install::user_dir(app), crate::portal_menu::folder(app)) else {
            return Ok(());
        };
        let config = user.join("Config");
        settings::prepare(&config, self.wants_portal(game), &figures)
            .map_err(|_| "Couldn't change Dolphin's settings, so the game can't start. Try again in a moment.".to_string())?;
        let _ = settings::quiet_hotkeys(&config);
        let _ = log::prepare(&user);
        Ok(())
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

    /// The game's own partition, or for a game whose pictures are in a few
    /// files, just those (copy.rs).
    fn copy_size(&self, game: &Game) -> Option<u64> {
        if game.path.is_dir() || self.console != Console::Wii {
            return None;
        }
        let found = disc::read(&game.path).ok()?;
        Some(copy::room_for(figures::game_from_title(&game.title)).unwrap_or(found.data_size))
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
        let files = copy::files_for(figures::game_from_title(&game.title));
        copy::extract(&tool, &game.path, &into.join("game"), files, progress, cancel)
    }

    fn save_folders(&self, app: &AppHandle, game: &Game) -> Vec<super::SaveFolder> {
        let Ok(user) = install::user_dir(app) else {
            return Vec::new();
        };
        match self.console {
            Console::GameCube => saves::gamecube_folders(&user, &game.title_id),
            _ => saves::wii_folders(&user, &game.title_id),
        }
    }

    fn save_folder(&self, app: &AppHandle, game: &Game, kept_as: &str) -> Option<PathBuf> {
        let user = install::user_dir(app).ok()?;
        match self.console {
            Console::GameCube => saves::gamecube_folder(&user, kept_as),
            _ => saves::wii_folder(&user, &game.title_id, kept_as),
        }
    }

    /// A GameCube save is known by what it holds, and a file of the same
    /// save under another name would be loaded in place of the one put back.
    fn is_same_save(&self, kept: &Path, saved: &Path) -> bool {
        self.console == Console::GameCube && saves::same_gamecube_save(kept, saved)
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
    /// DolphinQt). `--exec` names the game, once for each of its discs
    /// (`discs`), and `--user` the folder Dolphin keeps everything in
    /// (UICommon/CommandLineParse.cpp); named outright, no setting of
    /// another Dolphin's in the registry can send it elsewhere
    /// (`UICommon::SetUserDirectory`). A Wii game starts with the GameCube
    /// ports Omoio drives empty (controllers.rs, `gamecube_ports_off`).
    fn launch(&self, app: &AppHandle, game: &Game) -> Result<u32, String> {
        let exe = install::exe_path(app)?;
        if !exe.is_file() {
            return Err("Install Dolphin from the Emulators screen first, then you can play.".to_string());
        }
        if !game.path.exists() {
            return Err("This game isn't where it was. Reconnect the drive it's on.".to_string());
        }
        let portal = self.wants_portal(game);
        let user = install::user_dir(app)?;
        let mut command = install::command(&exe);
        command.arg("--user").arg(&user);
        if self.console == Console::Wii {
            // Read only to see which ports hold a standard pad, so a file
            // that can't be read counts as Dolphin's defaults.
            let dolphin_ini = std::fs::read_to_string(user.join("Config").join("Dolphin.ini")).unwrap_or_default();
            command.args(controllers::gamecube_ports_off(&dolphin_ini));
        }
        if portal {
            command.env("QT_QPA_PLATFORM", portal::QT_PLATFORM);
        } else {
            command.arg("--batch");
        }
        let library = app
            .path()
            .data_dir()
            .map(|dir| Library::load(&dir.join("Omoio").join("library.json")))
            .unwrap_or_default();
        let discs = discs(game, library.games());
        if discs.len() > 1 {
            // Dolphin changes discs by itself only when this is on, and it
            // is off as it ships (`MAIN_AUTO_DISC_CHANGE`,
            // Core/Config/MainSettings.cpp). Set here, it holds for this
            // game alone and is never saved (`CommandLineConfigLayerLoader`).
            command.arg("--config").arg("Dolphin.Core.AutoDiscChange=True");
        }
        for disc in discs {
            command.arg("--exec").arg(disc);
        }
        let child = command.spawn().map_err(|e| e.to_string())?;
        let pid = child.id();
        if portal {
            portal::prepare(pid);
        }
        Ok(pid)
    }

    /// Dolphin writes a figure on the portal through a buffer it never
    /// empties by itself: each of the game's writes reaches the file only
    /// with the next one, or as Dolphin quits (`SkylanderPortal::WriteBlock`
    /// in Core/IOS/USB/Emulated/Skylanders/Skylander.cpp, `SkylanderFigure::Save`,
    /// `IOFile::WriteArray` in Common/IOFile.h). A GameCube memory card is
    /// written a second after the game's last change, or as the game stops
    /// (`GCMemcardDirectory`, Core/HW/GCMemcard/GCMemcardDirectory.cpp).
    /// Ended at once, Dolphin would lose both.
    fn closes_when_asked(&self) -> bool {
        true
    }

    fn ask_to_close(&self, pid: u32) -> bool {
        portal::ask_to_close(pid)
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

    fn catalogue(&self, app: &AppHandle) -> Option<Vec<crate::core::catalogue::Entry>> {
        compat::entries(app, self.console)
    }

    fn refresh_catalogue<'a>(
        &'a self,
        app: &'a AppHandle,
        cancel: &'a AtomicBool,
    ) -> futures_util::future::BoxFuture<'a, Result<usize, String>> {
        Box::pin(compat::refresh(app, self.console, cancel))
    }

    /// The Dolphin wiki's ratings, which Dolphin's compatibility list is made
    /// from, as a list for each console.
    fn catalogue_source(&self) -> Option<(&'static str, &'static str)> {
        Some(compat::source(self.console))
    }

    /// The patches and codes Dolphin comes with for the game, for the disc's
    /// revision as Dolphin matches them (packs.rs), and its graphics mods
    /// that change the game (mods.rs).
    fn community_packs(&self, app: &AppHandle, title_id: &str, game: Option<&Game>) -> Packs {
        packs::view(app, title_id, game.and_then(revision))
    }

    fn set_community_pack(&self, app: &AppHandle, game: &Game, change: &PackChange) -> Result<(), String> {
        packs::set(app, &game.title_id, revision(game), change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disc_is_known_by_its_game_and_number() {
        assert_eq!(disc_of("GLMP01"), Some(("GLMP01", 1)));
        assert_eq!(disc_of("GLMP01D2"), Some(("GLMP01", 2)));
        assert_eq!(disc_of("GLMP0"), None);
        assert_eq!(disc_of("GLMP01X2"), None);
    }

    #[test]
    fn every_disc_of_a_game_goes_to_dolphin_the_one_started_first() {
        let dir = std::env::temp_dir().join(format!("omoio-dolphin-discs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = |console, title_id: &str, file: &str| {
            let path = if file.is_empty() { PathBuf::new() } else { dir.join(file) };
            if !file.is_empty() {
                std::fs::write(&path, b"").unwrap();
            }
            Game {
                console,
                title_id: title_id.to_string(),
                title: String::new(),
                version: None,
                update_version: None,
                path,
                size_bytes: 0,
            }
        };
        let library = vec![
            entry(Console::GameCube, "GLMP01", "first.iso"),
            entry(Console::GameCube, "GLMP01D2", "second.iso"),
            entry(Console::GameCube, "GLMP01D3", "third.iso"),
            entry(Console::GameCube, "GALE01", "another.iso"),
            entry(Console::Wii, "GLMP01D4", "wii.iso"),
            entry(Console::GameCube, "GLMP01D5", ""),
        ];
        let names = |started: usize| -> Vec<String> {
            discs(&library[started], &library)
                .iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(names(0), ["first.iso", "second.iso", "third.iso"]);
        assert_eq!(names(1), ["second.iso", "third.iso", "first.iso"]);
        assert_eq!(names(2), ["third.iso", "first.iso", "second.iso"]);
        assert_eq!(names(3), ["another.iso"], "a game of one disc");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wii_games_save_banner_is_found_by_its_id_alone() {
        let user = Path::new("User");
        let banner = user.join("Wii").join("title").join("00010000/53535050").join("data").join("banner.bin");
        assert_eq!(save_banner(user, "SSPP52"), Some(banner));
        assert_eq!(save_banner(user, "SSPP52D2"), save_banner(user, "SSPP52"), "a second disc's save is the game's");
        assert_eq!(save_banner(user, "SS-P52"), None);
    }
}
