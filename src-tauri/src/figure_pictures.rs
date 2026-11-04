//! Pictures of the Skylanders figures for the portal menu, read out of the
//! user's own copy of the game. The reading is done by a separate program,
//! omoio-portraits, kept in its own repository so that the game-format
//! readers it needs don't live in Omoio. Omoio downloads it only when the
//! user asks for the pictures, and checks it against the fingerprint below
//! every time before it runs, so nothing else can stand in for it.
//!
//! A Wii U or Wii disc image is encrypted, so for one Omoio has the emulator
//! make a copy it can read (Cemu a .wua, Dolphin's own DolphinTool the
//! game's files), reads the pictures from that, and deletes the copy at
//! once. The emulator does the decrypting, never Omoio. The copy can be about
//! the size of the game and take minutes to make (Dolphin copies only the
//! files the reader needs where they are known), so it is made only when it
//! fits with room to spare, and only for a game the reader knows. Nothing of
//! it is kept but the pictures, and a copy left by a run that never finished
//! goes the next time Omoio starts.
//!
//! The Disney Infinity base menu and the LEGO Dimensions Toy Pad menu show
//! their pieces' pictures the same way, read by the same program from the
//! player's own copy and never downloaded or shipped (Bertram, 4 November
//! 2026).

use crate::backends::EmulatorBackend;
use crate::core::console::{Console, Features};
use crate::core::figures;
use crate::core::infinity::InfinityGame;
use crate::core::library::Game;
use crate::core::toys::{self, Toy};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

const READER_URL: &str = "https://github.com/Bertrram/omoio-portraits/releases/download/v0.3.0/omoio-portraits.exe";
const READER_SHA256: &str = "3a0f3937d165eb63844004343d9039e777baed8996e535b7784f156bdb32d538";
const READER: &str = "omoio-portraits.exe";

/// The games, on the consoles whose disc images need a copy, whose pictures
/// the reader at `READER_URL` knows (its README). A copy of a disc image is
/// made only for one of these, or one of `TOYS_READ`: for any other the
/// minutes and gigabytes would come to nothing.
const COPIES_FOR: [(Console, figures::Game); 5] = [
    (Console::WiiU, figures::Game::SwapForce),
    (Console::WiiU, figures::Game::TrapTeam),
    (Console::WiiU, figures::Game::SuperChargers),
    (Console::WiiU, figures::Game::Imaginators),
    (Console::Wii, figures::Game::Spyro),
];

/// The games played with a toy other than Skylanders whose pictures the
/// reader knows, by console, toy and, for Disney Infinity, release:
/// Disney Infinity 1.0 and LEGO Dimensions, from the PS3 game's folder
/// (omoio-portraits' README, commits a581d09 and 782f0ed, read 4 November
/// 2026). The reader tells them by the PS3 title ids core/infinity.rs and
/// core/dimensions.rs know them by.
const TOYS_READ: [(Console, Toy, Option<InfinityGame>); 2] = [
    (Console::Ps3, Toy::Infinity, Some(InfinityGame::One)),
    (Console::Ps3, Toy::ToyPad, None),
];

/// Room left free on the drive on top of the copy, so it is never filled to
/// the last byte.
const SPARE: u64 = 2 << 30;

/// The folder a copy is made in: in Omoio's own data, or when that drive has
/// no room, beside the game.
const COPY_FOLDER: &str = "picture-copy";
const COPY_BESIDE: &str = ".omoio-picture-copy";
/// Where a copy beside a game is noted while it is made, for tidying away
/// one that a run left behind.
const COPY_NOTE: &str = "picture-copy.txt";

const NO_COPY: &str = "Omoio can't read the pictures from this copy of the game.";
const NOT_YET: &str = "Omoio can't read this game's pictures yet.";

/// The reader while it runs, so it can be stopped.
static RUNNING: Mutex<Option<Child>> = Mutex::new(None);
/// Asks a copy being made to stop.
static CANCEL: AtomicBool = AtomicBool::new(false);
/// The emulator making a copy, by name, so no game starts in it meanwhile.
static COPYING: Mutex<Option<&'static str>> = Mutex::new(None);

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().data_dir().map_err(|e| e.to_string())?.join("Omoio"))
}

/// A title id becomes a folder name here, so only letters and digits pass.
fn is_plain_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 16 && id.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Where a game's pictures are kept: one folder per game, so each shows its
/// figures as that game draws them.
fn folder(app: &AppHandle, title_id: &str) -> Result<PathBuf, String> {
    if !is_plain_id(title_id) {
        return Err("That game isn't in the library any more.".to_string());
    }
    Ok(data_dir(app)?.join("figure-pictures").join(title_id))
}

#[derive(serde::Serialize)]
pub struct Pictures {
    folder: String,
    /// Each picture's name without `.png`: `<id>-<variant>`, the variant as
    /// four hex digits.
    names: Vec<String>,
    /// Badges this game lacks that another of the user's games had, by name,
    /// with the file: Giants keeps the Giant badge, and a Giant goes on
    /// Trap Team's portal too.
    elsewhere: BTreeMap<String, String>,
}

/// A badge for a kind of figure, which every game shows the same way.
const BADGE: &str = "class-";

fn png_names(folder: &Path) -> Vec<String> {
    std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| entry.file_name().to_str()?.strip_suffix(".png").map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The pictures a game has so far.
pub fn pictures(app: &AppHandle, title_id: &str) -> Result<Pictures, String> {
    let folder = folder(app, title_id)?;
    let names = png_names(&folder);
    let mut elsewhere = BTreeMap::new();
    let others = folder.parent().and_then(|all| std::fs::read_dir(all).ok());
    for other in others.into_iter().flatten().flatten().map(|entry| entry.path()) {
        for name in png_names(&other).into_iter().filter(|name| name.starts_with(BADGE) && !names.contains(name)) {
            let file = other.join(format!("{name}.png"));
            elsewhere.entry(name).or_insert_with(|| file.to_string_lossy().into_owned());
        }
    }
    Ok(Pictures {
        folder: folder.to_string_lossy().into_owned(),
        names,
        elsewhere,
    })
}

fn fingerprint(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// The reader, downloaded the first time and checked every time.
async fn reader(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app
        .path()
        .local_data_dir()
        .map_err(|e| e.to_string())?
        .join("Omoio")
        .join("tools")
        .join(READER);
    if std::fs::read(&path).is_ok_and(|bytes| fingerprint(&bytes) == READER_SHA256) {
        return Ok(path);
    }
    let offline = |_| "Couldn't download the picture reader. Check the internet connection and try again.".to_string();
    let bytes = reqwest::Client::new()
        .get(READER_URL)
        .header("User-Agent", "Omoio")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(offline)?
        .bytes()
        .await
        .map_err(offline)?;
    if fingerprint(&bytes) != READER_SHA256 {
        return Err("The picture reader that came down isn't the one Omoio expects, so it wasn't run.".to_string());
    }
    let unwritable = |_| "Couldn't keep the picture reader in Omoio's folder.".to_string();
    std::fs::create_dir_all(path.parent().ok_or("Couldn't keep the picture reader.")?).map_err(unwritable)?;
    std::fs::write(&path, &bytes).map_err(unwritable)?;
    Ok(path)
}

fn run(reader: &Path) -> Command {
    let mut command = Command::new(reader);
    // The reader writes to a console, and Windows would open a window for it.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Which game an archive holds, as the reader finds it in the archive's own
/// meta.xml.
fn title_of(reader: &Path, copy: &Path) -> Option<String> {
    let out = run(reader).arg("title").arg(copy).output().ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("title ").map(str::to_string))
}

#[derive(Clone, serde::Serialize)]
struct Progress {
    title_id: String,
    /// "copy" while the emulator makes a copy to read from, "read" while
    /// the pictures are read.
    step: &'static str,
    done: usize,
    of: usize,
}

/// What asking for a game's pictures came to: how many were read, or, for
/// a game whose own files can't be read, what a temporary copy of it would
/// take in bytes, for the user to agree to first.
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Got {
    Pictures(usize),
    Copy { need: u64, free: u64 },
}

/// Whether the reader knows the pictures of a game played with a toy other
/// than Skylanders: its toy as `toys::toy_of` says, and for Disney Infinity
/// its release as `InfinityGame::of` says.
fn reads_toy(console: Console, title_id: &str, title: &str) -> bool {
    let Some(toy) = toys::toy_of(console, title_id, title).filter(|&toy| toy != Toy::Skylanders) else {
        return false;
    };
    let release = if toy == Toy::Infinity { InfinityGame::of(console, title_id, title) } else { None };
    TOYS_READ.contains(&(console, toy, release))
}

/// Whether a game's page offers to get its figures' pictures: a Skylanders
/// game wherever its portal menu works, as it always has, and a Disney
/// Infinity or LEGO Dimensions game where its menu works and the reader
/// knows it. The pictures are only for the menu, and a reader that doesn't
/// know the game would be fetched for nothing.
pub fn offered(features: Features, console: Console, title_id: &str, title: &str) -> bool {
    match toys::toy_of(console, title_id, title) {
        Some(Toy::Skylanders) => figures::offers_portal_menu(features, console, title),
        Some(_) => toys::offers_menu(features, console, title_id, title) && reads_toy(console, title_id, title),
        None => false,
    }
}

fn copies_for(game: &Game) -> bool {
    figures::game_from_title(&game.title).is_some_and(|which| COPIES_FOR.contains(&(game.console, which)))
        || reads_toy(game.console, &game.title_id, &game.title)
}

fn local_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().local_data_dir().map_err(|e| e.to_string())?.join("Omoio"))
}

/// Where a copy of the game may go, in the order tried.
fn copy_places(app: &AppHandle, game: &Game) -> Vec<PathBuf> {
    let own = local_dir(app).ok().map(|dir| dir.join(COPY_FOLDER));
    let beside = game.path.parent().map(|dir| dir.join(COPY_BESIDE));
    own.into_iter().chain(beside).collect()
}

/// The room free on the drive a folder is on, whether or not the folder is
/// there yet.
fn free_space(folder: &Path) -> Option<u64> {
    let there = folder.ancestors().find(|dir| dir.is_dir())?;
    let path = windows::core::HSTRING::from(there);
    let mut free = 0u64;
    unsafe { windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(&path, Some(&mut free), None, None) }.ok()?;
    Some(free)
}

fn gigabytes(bytes: u64) -> String {
    format!("{:.0} GB", (bytes as f64 / f64::from(1u32 << 30)).ceil())
}

/// The most room free where a copy of the game could go.
fn most_free(app: &AppHandle, game: &Game) -> u64 {
    copy_places(app, game).iter().filter_map(|place| free_space(place)).max().unwrap_or(0)
}

/// Has the emulator make a copy of the game Omoio can read, `need` bytes
/// with room to spare, in the first place with room for it, and returns the
/// folder it is in and the copy to read inside it. The folder is Omoio's
/// alone and goes again after reading.
fn make_copy(
    app: &AppHandle,
    backend: &'static dyn EmulatorBackend,
    game: &Game,
    need: u64,
) -> Result<(PathBuf, PathBuf), String> {
    let playing = app.state::<crate::session::Session>().playing();
    let running = playing.and_then(|playing| crate::backends::for_console(playing.console));
    if running.is_some_and(|running| running.name() == backend.name()) {
        return Err(format!("Close the game first. {} makes a copy of this one to read the pictures from.", backend.name()));
    }
    let places = copy_places(app, game);
    let Some(place) = places.iter().find(|place| free_space(place).is_some_and(|free| free >= need)) else {
        return Err(format!(
            "Reading the pictures needs {} free for a few minutes, and {} is free. Free some room and try again.",
            gigabytes(need),
            gigabytes(most_free(app, game))
        ));
    };
    let copying = Copying::begin(backend.name(), || {
        app.state::<crate::commands::InstallState>().is_installing(backend)
    })?;
    let _ = std::fs::remove_dir_all(place);
    std::fs::create_dir_all(place).map_err(|_| "Couldn't make room for a copy of the game.".to_string())?;
    if let Ok(dir) = data_dir(app) {
        let _ = std::fs::write(dir.join(COPY_NOTE), place.to_string_lossy().as_bytes());
    }
    CANCEL.store(false, Ordering::Relaxed);
    let progress = |percent: u32| {
        let progress = Progress { title_id: game.title_id.clone(), step: "copy", done: percent as usize, of: 100 };
        let _ = app.emit("figure-pictures", progress);
    };
    let made = backend.make_copy(app, game, place, &progress, &CANCEL);
    drop(copying);
    match made {
        Ok(copy) => Ok((place.clone(), copy)),
        Err(said) => {
            remove_copy(app, place);
            Err(said)
        }
    }
}

fn remove_copy(app: &AppHandle, place: &Path) {
    let _ = std::fs::remove_dir_all(place);
    if let Ok(dir) = data_dir(app) {
        let _ = std::fs::remove_file(dir.join(COPY_NOTE));
    }
}

/// Deletes a copy a run left behind, when Omoio was closed or stopped while
/// one was being made or read.
pub fn tidy(app: &AppHandle) {
    let noted = data_dir(app).ok().and_then(|dir| std::fs::read_to_string(dir.join(COPY_NOTE)).ok());
    if let Some(place) = noted.map(PathBuf::from).filter(|place| place.ends_with(COPY_BESIDE) || place.ends_with(COPY_FOLDER)) {
        remove_copy(app, &place);
    }
    if let Ok(dir) = local_dir(app) {
        let _ = std::fs::remove_dir_all(dir.join(COPY_FOLDER));
    }
}

/// The emulator busy making a copy of a game, by name.
pub fn copying() -> Option<&'static str> {
    *COPYING.lock().unwrap()
}

/// An emulator marked as making a copy until this is dropped.
struct Copying;

impl Copying {
    /// Marks `emulator`, unless `installing` says it is being installed: its
    /// copier would run from files being replaced. Marked before asking, as
    /// an install marks itself before it asks about a copy
    /// (`commands::install_dolphin`), so the two never both go ahead.
    fn begin(emulator: &'static str, installing: impl FnOnce() -> bool) -> Result<Self, String> {
        *COPYING.lock().unwrap() = Some(emulator);
        if installing() {
            *COPYING.lock().unwrap() = None;
            return Err(format!("{emulator} is being updated. Try again when that is done."));
        }
        Ok(Self)
    }
}

impl Drop for Copying {
    fn drop(&mut self) {
        *COPYING.lock().unwrap() = None;
    }
}

/// Reads the figures' pictures out of the game and keeps them, in place of
/// any from before, and says how many there are. Progress goes out as
/// `figure-pictures` events. A game whose own files can't be read gets a
/// temporary copy first, once `copy` says the user agreed to it, and the
/// copy is deleted again whatever happens.
pub async fn get(app: AppHandle, backend: &'static dyn EmulatorBackend, game: Game, copy: bool) -> Result<Got, String> {
    // The reader takes a PS3 game's folder only, and is fetched for nothing
    // when the game is a disc image (backends/rpcs3, `readable_copy`).
    if game.console == crate::core::console::Console::Ps3 && crate::import::is_image(&game.path) {
        return Err("Pictures can't be read from a PS3 disc image yet. A game imported as its folder gives them."
            .to_string());
    }
    // Nor is it fetched for a Disney Infinity or LEGO Dimensions release it
    // doesn't know.
    let toy = toys::toy_of(game.console, &game.title_id, &game.title);
    if toy.is_some_and(|toy| toy != Toy::Skylanders) && !reads_toy(game.console, &game.title_id, &game.title) {
        return Err(NOT_YET.to_string());
    }
    let reader = reader(&app).await?;
    let folder = folder(&app, &game.title_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(found) = backend.readable_copy(&app, &game, &|path| title_of(&reader, path)) {
            return read(&app, &reader, &game, &found, &folder).map(Got::Pictures);
        }
        let need = backend.copy_size(&game).ok_or(NO_COPY)? + SPARE;
        if !copies_for(&game) {
            return Err(NOT_YET.to_string());
        }
        if !copy {
            return Ok(Got::Copy { need, free: most_free(&app, &game) });
        }
        let (place, copy) = make_copy(&app, backend, &game, need)?;
        let read = read(&app, &reader, &game, &copy, &folder);
        remove_copy(&app, &place);
        read.map(Got::Pictures)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn read(app: &AppHandle, reader: &Path, game: &Game, copy: &Path, folder: &Path) -> Result<usize, String> {
    // Written beside and swapped in at the end, so a run that fails or is
    // stopped leaves the pictures from before as they were.
    let fresh = folder.with_extension("new");
    let _ = std::fs::remove_dir_all(&fresh);
    let mut child = run(reader)
        .arg("pictures")
        .arg(copy)
        .arg(&fresh)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "Couldn't start the picture reader.".to_string())?;
    let (out, err) = (child.stdout.take(), child.stderr.take());
    *RUNNING.lock().unwrap() = Some(child);

    let mut written = 0;
    for line in out.map(BufReader::new).into_iter().flat_map(BufRead::lines).map_while(Result::ok) {
        if let Some((done, of)) = line.strip_prefix("progress ").and_then(|rest| rest.split_once(' ')) {
            let progress = Progress {
                title_id: game.title_id.clone(),
                step: "read",
                done: done.parse().unwrap_or(0),
                of: of.parse().unwrap_or(0),
            };
            let _ = app.emit("figure-pictures", progress);
        } else if let Some(count) = line.strip_prefix("done ") {
            written = count.parse().unwrap_or(0);
        }
    }
    let Some(mut child) = RUNNING.lock().unwrap().take() else {
        let _ = std::fs::remove_dir_all(&fresh);
        return Err("Stopped. The pictures from before are kept.".to_string());
    };
    let finished = child.wait().is_ok_and(|status| status.success());
    if !finished {
        let mut said = String::new();
        let _ = err.map(|mut err| err.read_to_string(&mut said));
        let _ = std::fs::remove_dir_all(&fresh);
        let said = said.trim();
        return Err(if said.is_empty() { "Couldn't read the pictures out of the game." } else { said }.to_string());
    }
    let _ = std::fs::remove_dir_all(folder);
    std::fs::rename(&fresh, folder).map_err(|_| "Couldn't keep the pictures in Omoio's folder.".to_string())?;
    Ok(written)
}

/// Stops the reader if it is running, or the copy it would read from.
pub fn stop() {
    CANCEL.store(true, Ordering::Relaxed);
    if let Some(mut child) = RUNNING.lock().unwrap().take() {
        let _ = child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_ids_become_folders() {
        assert!(is_plain_id("WUD87E51FD0F7F95"));
        assert!(is_plain_id("0005000010140400"));
        assert!(!is_plain_id("..\\..\\Windows"));
        assert!(!is_plain_id(""));
    }

    #[test]
    fn the_fingerprint_is_sha256_in_hex() {
        assert_eq!(fingerprint(b"hello"), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[test]
    fn no_copy_is_made_while_its_emulator_is_installed() {
        assert!(Copying::begin("Dolphin", || true).is_err());
        assert_eq!(copying(), None, "a refused copy leaves no mark");
        let copy = Copying::begin("Dolphin", || {
            assert_eq!(copying(), Some("Dolphin"), "marked before the install is asked about");
            false
        })
        .unwrap();
        assert_eq!(copying(), Some("Dolphin"));
        drop(copy);
        assert_eq!(copying(), None);
    }

    use Console::*;

    fn game(console: Console, title_id: &str, title: &str) -> Game {
        Game {
            console,
            title_id: title_id.to_string(),
            title: title.to_string(),
            version: None,
            update_version: None,
            path: PathBuf::new(),
            size_bytes: 0,
        }
    }

    #[test]
    fn the_reader_knows_disney_infinity_1_0_and_lego_dimensions_on_the_ps3() {
        // The title ids the reader tells the two games by (omoio-portraits'
        // src/infinity.rs and src/dimensions.rs, `TITLE_IDS`, 4 November
        // 2026), whatever the name says.
        for id in ["BLUS30977", "BLES01842", "BLES01843", "BLES01844", "BLES02105", "BLUS31473"] {
            assert!(reads_toy(Ps3, id, ""), "{id}");
        }
        assert!(reads_toy(Ps3, "BLES01844", "Disney Infinity"));
        assert!(reads_toy(Ps3, "BLUS31473", "LEGO® DIMENSIONS™"));
        for (console, id, title) in [
            (Ps3, "BLUS31418", "Disney Infinity: Marvel Super Heroes"),
            (Ps3, "BLES02064", "Disney Infinity [2.0]"),
            (Ps3, "BLUS31522", "Disney Infinity 3.0"),
            (Ps3, "NPUB31657", "Disney Infinity 3.0"),
            (WiiU, "0005000010136F00", "Disney Infinity"),
            (WiiU, "0005000010195D00", "LEGO® DIMENSIONS™"),
            (WiiU, "WUD0123456789ABC", "LEGO Dimensions"),
            (Wii, "SQIE4Q", "Disney Infinity"),
        ] {
            assert!(!reads_toy(console, id, title), "{title} {id}");
        }
        // A Skylanders game goes by its title, as it always has, and a game
        // with no toy isn't one of these.
        assert!(!reads_toy(Ps3, "BLES01844", "Skylanders Giants"));
        assert!(!reads_toy(Ps3, "BLES02055", "LittleBigPlanet 3"));
    }

    #[test]
    fn a_toy_games_page_offers_its_pictures_where_its_menu_works_and_the_reader_knows_it() {
        let base = Features { infinity: true, ..Features::default() };
        let pad = Features { toypad: true, ..Features::default() };
        assert!(offered(base, Ps3, "BLES01844", "Disney Infinity"));
        assert!(offered(base, Ps3, "BLUS30977", ""));
        assert!(offered(pad, Ps3, "BLES02105", "LEGO® DIMENSIONS™"));
        assert!(offered(pad, Ps3, "BLUS31473", ""));
        // Not where the emulator doesn't fill the toy.
        assert!(!offered(Features::default(), Ps3, "BLES01844", "Disney Infinity"));
        assert!(!offered(pad, Ps3, "BLES01844", "Disney Infinity"));
        assert!(!offered(base, Ps3, "BLES02105", "LEGO® DIMENSIONS™"));
        // Not where the menu works and the reader doesn't know the game.
        assert!(!offered(base, Ps3, "BLUS31418", "Disney Infinity: Marvel Super Heroes"));
        assert!(!offered(base, Ps3, "BLUS31522", "Disney Infinity 3.0"));
        // Not where the menu isn't offered.
        assert!(!offered(pad, WiiU, "0005000010195D00", "LEGO® DIMENSIONS™"));
        assert!(!offered(base, WiiU, "0005000010136F00", "Disney Infinity"));
    }

    #[test]
    fn every_other_games_page_offers_its_pictures_exactly_where_it_always_did() {
        let titles = [
            "Skylanders: Spyro's Adventure",
            "Skylanders Giants",
            "Skylanders SWAP Force",
            "Skylanders Trap Team",
            "Skylanders SuperChargers",
            "Skylanders Imaginators",
            "Skylanders Battlecast",
            "LittleBigPlanet 3",
            "Infinity Runner",
            "LEGO Marvel Super Heroes",
            "",
        ];
        let every = Features { portal: true, infinity: true, toypad: true, ..Features::default() };
        for features in [Features::default(), Features { portal: true, ..Features::default() }, every] {
            for console in Console::ALL {
                for id in ["BLES02055", "0005000010142D00", "WUD0123456789ABC", "SX7P52", ""] {
                    for title in titles {
                        let before = figures::offers_portal_menu(features, console, title);
                        assert_eq!(offered(features, console, id, title), before, "{title} {id}");
                    }
                }
            }
        }
        // A Skylanders game stays one whatever its id, a toy's among them.
        assert!(offered(every, Ps3, "BLES01844", "Skylanders Giants"));
    }

    #[test]
    fn copies_are_made_for_the_games_they_always_were() {
        let copies = |console, title| copies_for(&game(console, "WUD0123456789ABC", title));
        for title in ["Skylanders SWAP Force", "Skylanders Trap Team", "Skylanders SuperChargers", "Skylanders Imaginators"] {
            assert!(copies(WiiU, title), "{title}");
            assert!(!copies(Wii, title), "{title}");
        }
        assert!(copies(Wii, "Skylanders: Spyro's Adventure"));
        assert!(!copies(WiiU, "Skylanders Giants"));
        assert!(!copies(Ps3, "Skylanders Giants"));
        // The reader doesn't know the Wii U's and the Wii's toy games, so no
        // copy of one is made.
        assert!(!copies(WiiU, "Disney Infinity"));
        assert!(!copies(WiiU, "LEGO Dimensions"));
        assert!(!copies(Wii, "Disney Infinity"));
        assert!(!copies(Ps3, "LittleBigPlanet 3"));
    }
}
