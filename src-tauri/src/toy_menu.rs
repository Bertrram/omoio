//! The toys' side of the menu over a game played with a toy that isn't a
//! Skylander: Disney Infinity's base, and LEGO Dimensions' Toy Pad. The
//! window, its keys, its pad button, the hush and the hand-back to the game
//! are the portal menu's (portal_menu.rs); this keeps each toy's own files,
//! apart from the Skylanders ones, so neither menu lists the other's.
//!
//! Each toy's figures live in a folder of their own inside the figures
//! folder, with their own backups, their own record of which figure each
//! file is and their own recently used list. Omoio never reads which figure
//! a file is out of the file: a Disney Infinity figure's number is encrypted
//! (Cemu's `InfinityUSB::LoadFigure`, Infinity.cpp lines 856 to 878, v2.6-2,
//! read 4 November 2026). It knows a figure it had the emulator make from
//! its own record of what it asked for, and one the player brought only
//! once the emulator has named it on the base.

use crate::backends::EmulatorBackend;
use crate::core::console::Console;
use crate::core::figures;
use crate::core::infinity::{self, InfinityGame, Piece};
use crate::core::settings::Settings;
use crate::core::toys::{self, Toy, ToyFigure};
use crate::session::{Playing, Session};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// The toys with a menu of their own beside the portal menu.
const TOYS: [Toy; 2] = [Toy::Infinity, Toy::ToyPad];

/// How many of a toy's figures are remembered as used lately, to list them
/// first: as many as the portal menu remembers of Skylanders
/// (portal_menu.rs, `RECENT`).
const RECENT: usize = 30;

/// The only files the emulators' Infinity and Toy Pad windows open and save:
/// Cemu's "BIN files (*.bin)" (gui/EmulatedUSBDevices/EmulatedUSBDeviceFrame.cpp
/// lines 461, 567 and 612, v2.6-2), RPCS3's "Infinity Figure (*.bin)"
/// (rpcs3qt/infinity_dialog.cpp lines 582 and 850, master b2e52d6) and
/// Dolphin's (DolphinQt/InfinityBase/InfinityBaseWindow.cpp lines 148 and
/// 296, 2609a). Read 4 November 2026.
const EXTENSION: &str = "bin";

const NO_MENU: &str = "This menu doesn't work in this game.";

/// What the toy's menu is called in a sentence: "The base menu doesn't work
/// in this game." Skylanders' keeps the words it has always had.
pub fn menu_name(toy: Toy) -> &'static str {
    match toy {
        Toy::Skylanders => "portal menu",
        Toy::Infinity => "base menu",
        Toy::ToyPad => "Toy Pad menu",
    }
}

/// The running game as its toy's menu needs it.
#[derive(Clone, Serialize)]
pub struct ToyGame {
    pub toy: Toy,
    pub console: Console,
    /// Disney Infinity 1.0, 2.0 or 3.0, as "one", "two" or "three", where
    /// the game's id or title says which.
    pub game: Option<&'static str>,
    /// That game's series, 1, 2 or 3, which decides the figures it takes.
    pub series: Option<u8>,
    #[serde(skip)]
    infinity: Option<InfinityGame>,
}

fn word(game: InfinityGame) -> &'static str {
    match game {
        InfinityGame::One => "one",
        InfinityGame::Two => "two",
        InfinityGame::Three => "three",
    }
}

/// The toy a game is played with, and for Disney Infinity which game it is.
/// `None` for a game played without one.
pub fn game_of(playing: &Playing) -> Option<ToyGame> {
    let toy = toys::toy_of(playing.console, &playing.title_id, &playing.title)?;
    let infinity = match toy {
        Toy::Infinity => InfinityGame::of(playing.console, &playing.title_id, &playing.title),
        Toy::Skylanders | Toy::ToyPad => None,
    };
    Some(ToyGame {
        toy,
        console: playing.console,
        game: infinity.map(word),
        series: infinity.map(InfinityGame::series),
        infinity,
    })
}

/// The running game's toy. `None` while nothing runs.
pub fn game(app: &AppHandle) -> Option<ToyGame> {
    game_of(&app.state::<Session>().playing()?)
}

/// The running game's emulator and process, for an errand on its toy.
pub struct Running {
    pub backend: &'static dyn EmulatorBackend,
    pub pid: u32,
    pub game: ToyGame,
}

/// The running game, when its toy has a menu here. A Skylanders game has
/// the portal menu's own errands instead.
pub fn running(app: &AppHandle) -> Result<Running, String> {
    let session = app.state::<Session>();
    let (Some(playing), Some(pid)) = (session.playing(), session.pid()) else {
        return Err("Start a game first.".to_string());
    };
    let backend = crate::backends::for_console(playing.console).ok_or("Start a game first.")?;
    let game = game_of(&playing).filter(|game| game.toy != Toy::Skylanders).ok_or(NO_MENU)?;
    Ok(Running { backend, pid, game })
}

/// Where a toy's figure files are kept: a folder of its own inside the
/// figures folder, so Settings' "Open folder" shows it, and the Skylanders
/// menu, which reads the figures folder's own files only
/// (`portal_menu::list`), never lists them.
pub fn folder(app: &AppHandle, toy: Toy) -> Result<PathBuf, String> {
    let name = toy.folder().ok_or(NO_MENU)?;
    Ok(crate::portal_menu::folder(app)?.join(name))
}

fn is_bin(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
}

/// The toy a figure file the player adds in Settings is for, told by its
/// size alone, since what it holds is encrypted: a .bin of exactly a Disney
/// Infinity figure's 320 bytes (`INF_FIGURE_SIZE`, Cemu's
/// nsyshid/Infinity.h lines 44 to 46, v2.6-2) or a LEGO Dimensions tag's 180
/// (the 0x2D pages of 4 bytes in nsyshid/Dimensions.h line 50). `None` for
/// every other file, which goes in the figures folder as before. No
/// emulator ever took a file of either size for a Skylander, which is 1024
/// bytes: Cemu's window turns a shorter file away
/// (EmulatedUSBDeviceFrame.cpp lines 293 to 296). Read 4 November 2026.
pub fn toy_of_file(path: &Path) -> Option<Toy> {
    sorted_into(path, path.metadata().ok()?.len())
}

fn sorted_into(path: &Path, size: u64) -> Option<Toy> {
    if !is_bin(path) {
        return None;
    }
    TOYS.into_iter().find(|toy| toy.file_size() == Some(size))
}

/// A file name for a new figure that isn't taken yet, as a Skylander's is
/// found (`figures::free_name`, which leaves out what Windows doesn't allow
/// in a file name), but ending in .bin, the only files the emulators' toy
/// windows open (`EXTENSION`).
pub fn free_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let bin = |sky: &str| format!("{}.{EXTENSION}", sky.strip_suffix(".sky").unwrap_or(sky));
    bin(&figures::free_name(name, |sky| taken(&bin(sky))))
}

/// Where a new figure of `name` is kept, in its toy's folder.
pub fn new_figure(app: &AppHandle, toy: Toy, name: &str) -> Result<PathBuf, String> {
    let dir = folder(app, toy)?;
    std::fs::create_dir_all(&dir).map_err(|_| "Couldn't make the figures folder.".to_string())?;
    Ok(dir.join(free_name(name, |file| dir.join(file).exists())))
}

/// `toy` and `console` as Omoio's files name them: "infinity", "wiiu".
fn key<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default().replace('"', "")
}

/// Which figure each of a toy's files is, by file name.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct Known {
    /// The figures Omoio had the emulator's figure maker make: the number
    /// and series it asked for.
    #[serde(default)]
    made: BTreeMap<String, ToyFigure>,
    /// Files the player brought, as the emulator named them once they were
    /// on the base, with the number and series its maker's list gives that
    /// name (`seen`).
    #[serde(default)]
    seen: BTreeMap<String, ToyFigure>,
}

impl Known {
    fn figure(&self, file: &str) -> Option<&ToyFigure> {
        self.made.get(file).or_else(|| self.seen.get(file))
    }

    fn forget(&mut self, file: &str) -> bool {
        let made = self.made.remove(file).is_some();
        self.seen.remove(file).is_some() || made
    }
}

/// Not `made-figures.json`, which is the Skylanders menu's.
fn known_file(app: &AppHandle, toy: Toy) -> Result<PathBuf, String> {
    Ok(crate::portal_menu::data_dir(app)?.join(format!("known-{}.json", key(&toy))))
}

fn read_known(file: &Path) -> Known {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_known(file: &Path, known: &Known) {
    if let Ok(text) = serde_json::to_string(known) {
        let _ = std::fs::write(file, text);
    }
}

fn known(app: &AppHandle, toy: Toy) -> Known {
    known_file(app, toy).map(|file| read_known(&file)).unwrap_or_default()
}

/// Remembers which figure a new file is, so the menu knows it and puts it
/// only where it goes.
pub fn made(app: &AppHandle, toy: Toy, file: &Path, figure: &ToyFigure) {
    let (Ok(record), Some(name)) = (known_file(app, toy), file.file_name()) else {
        return;
    };
    let mut known = read_known(&record);
    known.made.insert(name.to_string_lossy().into_owned(), figure.clone());
    write_known(&record, &known);
}

/// The figures an emulator's maker offered for a toy, kept with the
/// emulator's version so a newer emulator is asked again.
#[derive(Serialize, Deserialize)]
struct Kept {
    version: String,
    figures: Vec<ToyFigure>,
}

/// Not `characters-<console>.json`: the Skylanders menu merges every file
/// named so into its own offers (`portal_menu::from_the_others`).
fn kept_file(app: &AppHandle, toy: Toy, console: Console) -> Result<PathBuf, String> {
    Ok(crate::portal_menu::data_dir(app)?.join(format!("{}-figures-{}.json", key(&toy), key(&console))))
}

fn read_kept(file: &Path) -> Option<Kept> {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str::<Kept>(&text).ok())
        .filter(|kept| !kept.figures.is_empty())
}

/// The kept list alone, without asking the emulator.
fn kept_figures(app: &AppHandle, game: &ToyGame) -> Vec<ToyFigure> {
    kept_file(app, game.toy, game.console)
        .ok()
        .and_then(|file| read_kept(&file))
        .map(|kept| kept.figures)
        .unwrap_or_default()
}

/// Every figure the running game's emulator makes for its toy. Read from
/// the emulator the first time and kept, since the list only changes with a
/// new emulator, as the Skylanders characters are (`portal_menu::characters`).
fn listed(app: &AppHandle, backend: &dyn EmulatorBackend, pid: u32, game: &ToyGame) -> Result<Vec<ToyFigure>, String> {
    let file = kept_file(app, game.toy, game.console)?;
    let version = backend.detect_version(app).unwrap_or_default();
    match read_kept(&file) {
        Some(kept) if kept.version == version => Ok(kept.figures),
        kept => backend
            .toy_list(app, pid, game.toy)
            .inspect(|figures| {
                if figures.is_empty() {
                    return;
                }
                if let Ok(text) = serde_json::to_string(&Kept { version, figures: figures.clone() }) {
                    let _ = std::fs::write(&file, text);
                }
            })
            // The list kept from the emulator before names the same
            // figures, if maybe not its newest, so the menu offers those
            // until the new list can be read, as for Skylanders.
            .or_else(|why| kept.map(|kept| kept.figures).ok_or(why)),
    }
}

/// What a known figure is, and where it may go in the running game.
struct Placed {
    kind: Option<&'static str>,
    places: Vec<usize>,
}

/// Every place of the toy, for a figure that may go on any.
fn every(toy: Toy) -> Vec<usize> {
    (0..toy.places()).collect()
}

/// Where `figure` may go in the running game, and what kind of piece it is.
/// `None` for a figure the game doesn't take.
fn placed(game: &ToyGame, figure: &ToyFigure) -> Option<Placed> {
    match game.toy {
        Toy::Infinity => {
            let piece = Piece::of_number(figure.number)?;
            let places = match game.infinity {
                Some(infinity) => infinity::places_for(infinity, figure),
                // A Disney Infinity game whose id and title don't say which:
                // every figure, where its kind goes, as the portal menu offers
                // every Skylander over a game it can't tell (`figures::menu_note`).
                None => (0..infinity::PLACES).filter(|&place| infinity::fits(place, piece)).collect(),
            };
            (!places.is_empty()).then(|| Placed { kind: Some(piece.kind()), places })
        }
        // Any of the Toy Pad's places takes any tag: Cemu's figure maker
        // offers the same list whichever place it was opened for
        // (`CreateMinifig` and `CreateDimensionFigureDialog`,
        // EmulatedUSBDeviceFrame.cpp lines 653 to 661 and 688 to 704,
        // v2.6-2, read 4 November 2026).
        Toy::ToyPad => Some(Placed { kind: None, places: every(Toy::ToyPad) }),
        Toy::Skylanders => None,
    }
}

/// A figure the emulator's maker can make that the running game takes, with
/// the places it may go in.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToyOffer {
    pub name: String,
    pub number: u32,
    pub series: Option<u8>,
    /// For Disney Infinity "character", "play_set", "round_disc" or
    /// "hex_disc"; empty for a toy Omoio doesn't sort into kinds.
    pub kind: String,
    pub places: Vec<usize>,
}

fn offered(figures: Vec<ToyFigure>, game: &ToyGame) -> Vec<ToyOffer> {
    figures
        .into_iter()
        .filter_map(|figure| {
            let placed = placed(game, &figure)?;
            Some(ToyOffer {
                kind: placed.kind.unwrap_or_default().to_string(),
                places: placed.places,
                name: figure.name,
                number: figure.number,
                series: figure.series,
            })
        })
        .collect()
}

/// Every figure the running game's emulator can make that the game takes,
/// each with the places it may go in, in the order of the maker's list.
pub fn offers(
    app: &AppHandle,
    backend: &dyn EmulatorBackend,
    pid: u32,
    game: &ToyGame,
) -> Result<Vec<ToyOffer>, String> {
    Ok(offered(listed(app, backend, pid, game)?, game))
}

/// One of the player's figure files for the running game's toy.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToyFile {
    pub path: String,
    /// The file's name without its extension.
    pub name: String,
    /// Which figure it is, for a file Omoio had made or the emulator named.
    pub figure: Option<ToyFigure>,
    pub kind: Option<String>,
    /// Where it may go: every place when Omoio doesn't know what it is.
    pub places: Vec<usize>,
    /// Used lately, so listed first.
    pub recent: bool,
}

fn files_in(dir: &Path, known: &Known, recent: &[String], game: &ToyGame) -> Vec<ToyFile> {
    let size = game.toy.file_size();
    let mut found: Vec<(usize, String, ToyFile)> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                // A file of another size isn't a figure of this toy, and its
                // emulator wouldn't take it.
                .filter(|path| {
                    is_bin(path)
                        && path
                            .metadata()
                            .is_ok_and(|meta| meta.is_file() && size.is_none_or(|size| meta.len() == size))
                })
                .filter_map(|path| {
                    let file = path.file_name()?.to_string_lossy().into_owned();
                    let figure = known.figure(&file).cloned();
                    let (kind, places) = match &figure {
                        // A figure from a game later than the running one is
                        // left out, as the portal menu leaves out a Skylander
                        // the game doesn't read.
                        Some(figure) => {
                            let placed = placed(game, figure)?;
                            (placed.kind.map(str::to_string), placed.places)
                        }
                        // Until the emulator names a file the player brought,
                        // Omoio doesn't know what it is, so it may go anywhere.
                        None => (None, every(game.toy)),
                    };
                    let rank = recent.iter().position(|used| *used == file);
                    let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                    Some((
                        rank.unwrap_or(usize::MAX),
                        name.to_lowercase(),
                        ToyFile {
                            path: path.to_string_lossy().into_owned(),
                            name,
                            figure,
                            kind,
                            places,
                            recent: rank.is_some(),
                        },
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    found.into_iter().map(|(_, _, file)| file).collect()
}

/// The player's figure files for the running game's toy that the game
/// takes, the ones used lately first, the rest by name. Nothing while no
/// game with a toy menu runs.
pub fn files(app: &AppHandle) -> Vec<ToyFile> {
    let Some(game) = game(app).filter(|game| game.toy != Toy::Skylanders) else {
        return Vec::new();
    };
    let Ok(dir) = folder(app, game.toy) else {
        return Vec::new();
    };
    files_in(&dir, &known(app, game.toy), &recent(app, game.toy), &game)
}

/// The figure named `name` in a maker's list: by the name as it is, or
/// else whatever its case, and only where one figure has it.
fn named<'a>(list: &'a [ToyFigure], name: &str) -> Option<&'a ToyFigure> {
    let only = |matches: Vec<&'a ToyFigure>| match matches.as_slice() {
        [one] => Some(*one),
        _ => None,
    };
    let name = name.trim();
    only(list.iter().filter(|figure| figure.name == name).collect()).or_else(|| {
        only(list.iter().filter(|figure| figure.name.trim().eq_ignore_ascii_case(name)).collect())
    })
}

/// Remembers which figure a file the player brought is, once the emulator
/// has named it on the base: the name it shows for the place, looked up in
/// the list its figure maker gave (`listed`). Nothing is read from the file.
/// A file Omoio made is known already, and a name the list doesn't hold,
/// such as an emulator's "Unknown Figure", teaches nothing.
fn seen(app: &AppHandle, game: &ToyGame, figure: &Path, name: &str) {
    let (Ok(record), Some(file)) = (known_file(app, game.toy), figure.file_name()) else {
        return;
    };
    let file = file.to_string_lossy().into_owned();
    let mut known = read_known(&record);
    if known.made.contains_key(&file) {
        return;
    }
    let list = kept_figures(app, game);
    let Some(figure) = named(&list, name) else {
        return;
    };
    if known.seen.get(&file) != Some(figure) {
        known.seen.insert(file, figure.clone());
        write_known(&record, &known);
    }
}

/// The file name of one of the player's figures for `toy`, checked to be a
/// figure file in the toy's own folder.
fn saved(app: &AppHandle, toy: Toy, figure: &Path) -> Result<String, String> {
    let dir = folder(app, toy)?;
    let in_folder = figure.parent().is_some_and(|parent| parent == dir);
    match figure.file_name() {
        Some(name) if in_folder && is_bin(figure) && figure.is_file() => Ok(name.to_string_lossy().into_owned()),
        _ => Err("That isn't one of your saved figures.".to_string()),
    }
}

/// Whether a figure may go in `place`. The emulators put any file in any
/// place (Cemu's `LoadFigurePath`, EmulatedUSBDeviceFrame.cpp lines 473 to
/// 494, v2.6-2), so the menu keeps a character off the hexagon. A file
/// Omoio doesn't know may go anywhere.
fn goes(game: &ToyGame, figure: Option<&ToyFigure>, place: usize) -> Result<(), String> {
    if place >= game.toy.places() {
        return Err("That figure doesn't go there.".to_string());
    }
    let Some(figure) = figure else {
        return Ok(());
    };
    match placed(game, figure) {
        None => Err("This game doesn't take that figure.".to_string()),
        Some(placed) if !placed.places.contains(&place) => Err("That figure doesn't go there.".to_string()),
        Some(_) => Ok(()),
    }
}

/// Keeps a copy of a figure's file before it goes on, so a figure the game
/// later calls broken can be put back as it was, as a Skylander's is
/// (`portal_menu::back_up`). While a figure is on, its emulator writes each
/// block the game sends straight into the file (Cemu's
/// `InfinityUSB::WriteBlock` and `InfinityFigure::Save`, Infinity.cpp lines
/// 761 to 781 and 819 to 826, v2.6-2; RPCS3's `infinity_base::write_block`
/// and `infinity_figure::save`, Emu/Io/Infinity.cpp lines 223 to 241 and 17
/// to 26, master b2e52d6; Dolphin's `InfinityBase::WriteBlock` and
/// `InfinityFigure::Save`, Core/IOS/USB/Emulated/Infinity.cpp lines 513 to
/// 539 and 944, 2609a; read 4 November 2026). Each toy keeps its copies in
/// a Backups folder of its own, so a Skylander and a Disney Infinity figure
/// of the same file name keep theirs apart. A figure that can't be copied
/// still goes on.
pub fn back_up(app: &AppHandle, toy: Toy, figure: &Path) {
    let taken = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    if let Ok(dir) = folder(app, toy) {
        let _ = crate::portal_menu::back_up_into(figure, &dir.join("Backups"), taken);
    }
}

/// Before one of the player's figures goes in `place`: that it is one of
/// their figures for this toy and goes there, and a copy of it kept.
pub fn before_load(app: &AppHandle, game: &ToyGame, place: usize, figure: &Path) -> Result<(), String> {
    let file = saved(app, game.toy, figure)?;
    goes(game, known(app, game.toy).figure(&file), place)?;
    back_up(app, game.toy, figure);
    Ok(())
}

/// After a figure went in `place`: remembered as used, and, for a file the
/// player brought, as the figure the emulator now names there.
pub fn after_load(app: &AppHandle, game: &ToyGame, figure: &Path, place: usize, places: &[String]) {
    used(app, game.toy, figure);
    if let Some(name) = places.get(place).filter(|name| !name.trim().is_empty()) {
        seen(app, game, figure, name);
    }
}

/// Before a new figure is made for `place`: the figure the maker's list
/// gives for `number` and `series`, checked to go there, and the file it
/// goes in.
pub fn before_create(
    app: &AppHandle,
    game: &ToyGame,
    place: usize,
    number: u32,
    series: Option<u8>,
) -> Result<(ToyFigure, PathBuf), String> {
    let figure = kept_figures(app, game)
        .into_iter()
        .find(|figure| figure.number == number && figure.series == series)
        .ok_or("Omoio doesn't know that figure. Close the menu and open it again.")?;
    goes(game, Some(&figure), place)?;
    let file = new_figure(app, game.toy, &figure.name)?;
    Ok((figure, file))
}

/// A new figure on its toy: the file, and what each place holds now.
#[derive(Serialize)]
pub struct ToyMade {
    pub path: String,
    pub places: Vec<String>,
}

fn recent_of(settings: &mut Settings, toy: Toy) -> Option<&mut Vec<String>> {
    match toy {
        Toy::Infinity => Some(&mut settings.recent_infinity),
        Toy::ToyPad => Some(&mut settings.recent_toypad),
        Toy::Skylanders => None,
    }
}

fn recent(app: &AppHandle, toy: Toy) -> Vec<String> {
    let Ok(dir) = crate::portal_menu::data_dir(app) else {
        return Vec::new();
    };
    let mut settings = Settings::load(&dir.join("settings.json"));
    recent_of(&mut settings, toy).map(std::mem::take).unwrap_or_default()
}

fn remember(list: &mut Vec<String>, name: String) {
    list.retain(|known| *known != name);
    list.insert(0, name);
    list.truncate(RECENT);
}

/// Remembers a figure as the one used last, so the menu lists it first.
/// Kept apart from the Skylanders menu's list, so neither pushes the
/// other's figures out.
pub fn used(app: &AppHandle, toy: Toy, figure: &Path) {
    let (Ok(dir), Some(name)) = (crate::portal_menu::data_dir(app), figure.file_name()) else {
        return;
    };
    let file = dir.join("settings.json");
    let mut settings = Settings::load(&file);
    let Some(list) = recent_of(&mut settings, toy) else {
        return;
    };
    remember(list, name.to_string_lossy().into_owned());
    let _ = settings.save(&file);
}

/// Moves one of the player's toy figures to the Recycle Bin, as the
/// Skylanders menu's delete does (`portal_menu::delete`), and forgets which
/// figure it was and that it was used. Only a figure file in one of the
/// toys' own folders is taken.
pub fn delete(app: &AppHandle, figure: &str) -> Result<(), String> {
    let path = Path::new(figure);
    let (toy, name) = TOYS
        .into_iter()
        .find_map(|toy| Some((toy, saved(app, toy, path).ok()?)))
        .ok_or("That isn't one of your saved figures.")?;
    crate::portal_menu::to_recycle_bin(path)?;
    if let Ok(record) = known_file(app, toy) {
        let mut known = read_known(&record);
        if known.forget(&name) {
            write_known(&record, &known);
        }
    }
    if let Ok(dir) = crate::portal_menu::data_dir(app) {
        let file = dir.join("settings.json");
        let mut settings = Settings::load(&file);
        if let Some(list) = recent_of(&mut settings, toy) {
            list.retain(|used| *used != name);
            let _ = settings.save(&file);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn infinity(console: Console, title_id: &str, title: &str) -> ToyGame {
        game_of(&Playing { title_id: title_id.to_string(), title: title.to_string(), console }).unwrap()
    }

    fn one() -> ToyGame {
        infinity(Console::Wii, "SQIE4Q", "Disney Infinity")
    }

    fn three() -> ToyGame {
        infinity(Console::Ps3, "BLUS31522", "Disney Infinity 3.0")
    }

    /// The same game whose id and title don't say which it is.
    fn unknown() -> ToyGame {
        ToyGame { game: None, series: None, infinity: None, ..one() }
    }

    fn figure(name: &str, number: u32, series: u8) -> ToyFigure {
        ToyFigure { name: name.to_string(), number, series: Some(series) }
    }

    /// One of each kind from each game, from Cemu's list (nsyshid/Infinity.cpp
    /// lines 21 to 343, v2.6-2).
    fn list() -> Vec<ToyFigure> {
        vec![
            figure("Mr. Incredible", 1000001, 1),
            figure("Lone Ranger Play Set", 2000002, 1),
            figure("Bolt's Super Strength", 3000003, 1),
            figure("Mickey's Car", 4000018, 1),
            figure("Captain America", 1000100, 2),
            figure("Marvel's The Avengers Play Set", 2000100, 2),
            figure("Assault on Asgard", 2000103, 2),
            figure("Sam Flynn", 1000150, 3),
            figure("Inside Out Play Set", 2000200, 3),
        ]
    }

    /// A folder of its own for one test, emptied first.
    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("omoio-toy-menu-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn a_new_figure_is_a_bin_named_for_it_that_isnt_taken() {
        assert_eq!(free_name("Mr. Incredible", |_| false), "Mr. Incredible.bin");
        assert_eq!(free_name("Mr. Incredible", |file| file == "Mr. Incredible.bin"), "Mr. Incredible 2.bin");
        let taken = ["Jack Sparrow.bin", "Jack Sparrow 2.bin"];
        assert_eq!(free_name("Jack Sparrow", |file| taken.contains(&file)), "Jack Sparrow 3.bin");
        // A Skylander's file of the same name doesn't count: only .bin is asked.
        assert_eq!(free_name("Jack Sparrow", |file| file == "Jack Sparrow.sky"), "Jack Sparrow.bin");
        // What Windows doesn't allow in a file name is left out.
        assert_eq!(free_name("Star Wars: Rise Against the Empire", |_| false), "Star Wars Rise Against the Empire.bin");
        assert_eq!(free_name("Who? <Me>|*", |_| false), "Who Me.bin");
        assert_eq!(free_name(":::", |_| false), "Figure.bin");
    }

    #[test]
    fn a_figure_file_goes_in_its_toys_folder_by_its_size() {
        assert_eq!(sorted_into(Path::new("Jack Sparrow.bin"), 320), Some(Toy::Infinity));
        assert_eq!(sorted_into(Path::new("JACK SPARROW.BIN"), 320), Some(Toy::Infinity));
        assert_eq!(sorted_into(Path::new("Batman.bin"), 180), Some(Toy::ToyPad));
        // A Skylander, whatever its extension, and anything else, stays
        // where it went before.
        assert_eq!(sorted_into(Path::new("Spyro.bin"), 1024), None);
        assert_eq!(sorted_into(Path::new("Spyro.sky"), 1024), None);
        assert_eq!(sorted_into(Path::new("Odd.sky"), 320), None);
        assert_eq!(sorted_into(Path::new("Odd.dump"), 180), None);
        assert_eq!(sorted_into(Path::new("Short.bin"), 319), None);
        assert_eq!(sorted_into(Path::new("no extension"), 320), None);
    }

    #[test]
    fn which_figure_each_file_is_is_kept_between_runs() {
        let root = scratch("known");
        let record = root.join("known-infinity.json");
        assert_eq!(read_known(&record), Known::default(), "nothing kept yet");
        let mut known = Known::default();
        known.made.insert("Mr. Incredible.bin".to_string(), figure("Mr. Incredible", 1000001, 1));
        known.seen.insert("from a friend.bin".to_string(), figure("Captain America", 1000100, 2));
        write_known(&record, &known);
        let read = read_known(&record);
        assert_eq!(read, known);
        assert_eq!(read.figure("from a friend.bin").map(|f| f.number), Some(1000100));
        assert_eq!(read.figure("never seen.bin"), None);
        // What Omoio made wins over a name the emulator showed.
        let mut both = read;
        both.seen.insert("Mr. Incredible.bin".to_string(), figure("Captain America", 1000100, 2));
        assert_eq!(both.figure("Mr. Incredible.bin").map(|f| f.number), Some(1000001));
        assert!(both.forget("Mr. Incredible.bin"));
        assert_eq!(both.figure("Mr. Incredible.bin"), None);
        assert!(!both.forget("Mr. Incredible.bin"));
        // A record from an older Omoio with only one half still reads.
        let older = r#"{"made":{"Jack Sparrow.bin":{"name":"Jack Sparrow","number":1000003,"series":1}}}"#;
        std::fs::write(&record, older).unwrap();
        assert_eq!(read_known(&record).made.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_game_is_offered_what_it_takes_where_it_goes() {
        let at = |game: &ToyGame, number: u32| {
            offered(list(), game).into_iter().find(|offer| offer.number == number).map(|offer| offer.places)
        };
        let one = one();
        assert_eq!(at(&one, 1000001), Some(vec![3, 6]), "a character on Player One or Two");
        assert_eq!(at(&one, 2000002), Some(vec![0]), "a play set on the hexagon");
        assert_eq!(at(&one, 3000003), Some(vec![4, 5, 7, 8]), "a round disc under a character");
        assert_eq!(at(&one, 4000018), Some(vec![0, 1, 2]), "a hexagonal disc on the hexagon");
        assert_eq!(at(&one, 1000100), None, "2.0's characters aren't in 1.0");
        assert_eq!(at(&one, 1000150), None);
        let three = three();
        assert_eq!(at(&three, 1000001), Some(vec![3, 6]), "1.0's characters go in 3.0's Toy Box");
        assert_eq!(at(&three, 1000100), Some(vec![3, 6]));
        assert_eq!(at(&three, 3000003), Some(vec![4, 5, 7, 8]));
        assert_eq!(at(&three, 2000002), None, "a play set works in its own game only");
        assert_eq!(at(&three, 2000103), None, "so does a Toy Box Game");
        assert_eq!(at(&three, 2000200), Some(vec![0]));
        let kinds: Vec<String> = offered(list(), &one).into_iter().map(|offer| offer.kind).collect();
        assert_eq!(kinds, ["character", "play_set", "round_disc", "hex_disc"]);
        // Over a game Omoio can't tell, everything, where its kind goes.
        assert_eq!(offered(list(), &unknown()).len(), list().len());
        assert_eq!(at(&unknown(), 1000150), Some(vec![3, 6]));
    }

    #[test]
    fn a_figure_goes_only_where_it_fits() {
        let one = one();
        let jack = figure("Jack Sparrow", 1000003, 1);
        assert!(goes(&one, Some(&jack), 3).is_ok());
        assert!(goes(&one, Some(&jack), 0).is_err(), "a character isn't put on the hexagon");
        assert!(goes(&one, Some(&jack), 4).is_err(), "nor under one");
        assert!(goes(&one, Some(&figure("Sam Flynn", 1000150, 3)), 3).is_err(), "nor one the game doesn't take");
        // A file the player brought that the emulator hasn't named yet.
        assert!(goes(&one, None, 0).is_ok());
        assert!(goes(&one, None, 8).is_ok());
        assert!(goes(&one, None, 9).is_err(), "the base has nine places");
    }

    #[test]
    fn a_name_the_emulator_shows_is_found_in_its_makers_list() {
        let list = list();
        assert_eq!(named(&list, "Captain America").map(|f| f.number), Some(1000100));
        assert_eq!(named(&list, " captain america ").map(|f| f.number), Some(1000100));
        assert_eq!(named(&list, "Unknown Figure"), None);
        assert_eq!(named(&list, ""), None);
        let twice = [figure("Captain America", 1000100, 2), figure("Captain America", 1000999, 3)];
        assert_eq!(named(&twice, "Captain America"), None, "a name two figures share says nothing");
    }

    #[test]
    fn the_players_files_are_listed_used_lately_first() {
        let dir = scratch("files");
        let game = one();
        let write = |name: &str, size: usize| std::fs::write(dir.join(name), vec![0u8; size]).unwrap();
        write("Mr. Incredible.bin", 320);
        write("Jack Sparrow.bin", 320);
        write("from a friend.bin", 320);
        write("Sam Flynn.bin", 320);
        write("Spyro.bin", 1024);
        write("notes.txt", 320);
        std::fs::create_dir_all(dir.join("Backups")).unwrap();
        let mut known = Known::default();
        known.made.insert("Mr. Incredible.bin".to_string(), figure("Mr. Incredible", 1000001, 1));
        known.made.insert("Sam Flynn.bin".to_string(), figure("Sam Flynn", 1000150, 3));
        known.seen.insert("Jack Sparrow.bin".to_string(), figure("Jack Sparrow", 1000003, 1));
        let files = files_in(&dir, &known, &["Mr. Incredible.bin".to_string()], &game);
        let names: Vec<&str> = files.iter().map(|file| file.name.as_str()).collect();
        // Sam Flynn is 3.0's, a .bin of a Skylander's size isn't this
        // toy's, and neither are other files or the backups.
        assert_eq!(names, ["Mr. Incredible", "from a friend", "Jack Sparrow"]);
        assert!(files[0].recent && !files[1].recent);
        assert_eq!(files[0].places, [3, 6]);
        assert_eq!(files[0].kind.as_deref(), Some("character"));
        assert_eq!(files[1].figure, None);
        assert_eq!(files[1].places, (0..9).collect::<Vec<_>>(), "a file Omoio doesn't know may go anywhere");
        assert_eq!(files[2].places, [3, 6], "one the emulator named is known");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_recent_list_keeps_the_newest_first_and_no_more_than_it_should() {
        let mut list = Vec::new();
        for n in 0..RECENT + 5 {
            remember(&mut list, format!("{n}.bin"));
        }
        assert_eq!(list.len(), RECENT);
        assert_eq!(list[0], format!("{}.bin", RECENT + 4));
        remember(&mut list, "20.bin".to_string());
        assert_eq!(list[0], "20.bin");
        assert_eq!(list.iter().filter(|name| *name == "20.bin").count(), 1);
    }
}
