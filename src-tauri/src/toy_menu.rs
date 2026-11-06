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
//! read 4 November 2026), and so is a LEGO Dimensions minifigure's
//! (`DimensionsUSB::GetFigureId`, nsyshid/Dimensions.cpp lines 1001 to
//! 1017, v2.6-2, read 4 November 2026). It knows a figure it had the
//! emulator make from its own record of what it asked for, and one the
//! player brought only once the emulator has named it on the toy.

use crate::backends::{self, EmulatorBackend, Listed};
use crate::core::console::Console;
use crate::core::dimensions;
use crate::core::figures;
use crate::core::infinity::{self, InfinityGame, Piece};
use crate::core::settings::Settings;
use crate::core::toys::{self, Toy, ToyFigure};
use crate::session::{Playing, Session};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
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
    /// Files the player named themselves in the menu (`rename`).
    #[serde(default)]
    named: std::collections::BTreeSet<String>,
}

impl Known {
    fn figure(&self, toy: Toy, file: &str) -> Option<&ToyFigure> {
        match self.made.get(file) {
            // A Blank Tag Omoio made is whatever the game has written to it
            // since, once the emulator has named that.
            Some(made) if blank(toy, made) => self.seen.get(file).or(Some(made)),
            Some(made) => Some(made),
            None => self.seen.get(file),
        }
    }

    fn forget(&mut self, file: &str) -> bool {
        let made = self.made.remove(file).is_some();
        let named = self.named.remove(file);
        self.seen.remove(file).is_some() || made || named
    }

    /// What is known of `from`, known of `to` instead.
    /// Anything still known under `to` is of a file deleted by hand before,
    /// since no file had that name, so it goes.
    fn rename(&mut self, from: &str, to: &str) {
        for record in [&mut self.made, &mut self.seen] {
            let figure = record.remove(from);
            record.remove(to);
            if let Some(figure) = figure {
                record.insert(to.to_string(), figure);
            }
        }
        self.named.remove(from);
        self.named.insert(to.to_string());
    }
}

/// A LEGO Dimensions Blank Tag, the one tag that becomes another while it
/// is on the Toy Pad: the figure maker leaves its pages for the game to
/// fill (Cemu's `DimensionsUSB::CreateFigure`, nsyshid/Dimensions.cpp lines
/// 707 to 749, v2.6-2; RPCS3's `dimensions_toypad::create_blank_character`,
/// rpcs3/Emu/Io/Dimensions.cpp lines 496 to 529, master b2e52d6), and both
/// emulators take the id the game then writes to its page 36 as the tag's
/// (Cemu's `DimensionsUSB::WriteBlock`, lines 1050 to 1078; RPCS3's
/// `dimensions_toypad::write_block`, lines 311 to 338). Read 4 November 2026.
fn blank(toy: Toy, figure: &ToyFigure) -> bool {
    toy == Toy::ToyPad && matches!(dimensions::Piece::of_id(figure.number), dimensions::Piece::BlankTag)
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
    /// Whether the read reached both ends of the maker's list
    /// (`backends::kept_or_read`). A list kept by Omoio 0.3.5 or before
    /// doesn't say, so it is read once more.
    #[serde(default)]
    whole: bool,
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
    let kept = read_kept(&file).map(|kept| (kept.version, Listed { items: kept.figures, whole: kept.whole }));
    let read = || backend.toy_list(app, pid, game.toy);
    let (offered, keep) = backends::kept_or_read(kept, &version, read, |a, b| a.number == b.number);
    if let Some(keep) = keep.filter(|keep| !keep.items.is_empty()) {
        if let Ok(text) = serde_json::to_string(&Kept { version, whole: keep.whole, figures: keep.items }) {
            let _ = std::fs::write(&file, text);
        }
    }
    offered
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
        Toy::ToyPad => Some(Placed {
            kind: Some(dimensions::Piece::of_id(figure.number).kind()),
            places: every(Toy::ToyPad),
        }),
        Toy::Skylanders => None,
    }
}

/// Whether Omoio has the emulator's figure maker make `figure`. Never a
/// LEGO Dimensions vehicle or gadget: both makers encrypt any id they are
/// given as a minifigure's, where the game keeps a vehicle's in plain, so
/// such a tag reads as no vehicle at all (Cemu's
/// `DimensionsUSB::CreateFigure` and `GetFigureId`, nsyshid/Dimensions.cpp
/// lines 707 to 749 and 1001 to 1017, v2.6-2; RPCS3's
/// `create_blank_character`, rpcs3/Emu/Io/Dimensions.cpp lines 496 to 529,
/// master b2e52d6). Both makers' own windows offer the minifigures and the
/// Blank Tag only (Cemu's EmulatedUSBDeviceFrame.cpp lines 698 to 703;
/// RPCS3's rpcs3qt/dimensions_dialog.cpp lines 389 to 402), and the game
/// writes a vehicle or gadget to a Blank Tag as it is built. Read 4
/// November 2026. Vehicles stay in the maker's list all the same, to name a
/// tag the game wrote.
fn makes(game: &ToyGame, figure: &ToyFigure) -> bool {
    game.toy != Toy::ToyPad || !matches!(dimensions::Piece::of_id(figure.number), dimensions::Piece::Vehicle)
}

/// A figure the emulator's maker can make that the running game takes, with
/// the places it may go in.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToyOffer {
    pub name: String,
    pub number: u32,
    pub series: Option<u8>,
    /// For Disney Infinity "character", "play_set", "round_disc" or
    /// "hex_disc"; for the Toy Pad "minifigure" or "blank_tag".
    pub kind: String,
    pub places: Vec<usize>,
}

fn offered(figures: Vec<ToyFigure>, game: &ToyGame) -> Vec<ToyOffer> {
    figures
        .into_iter()
        .filter(|figure| makes(game, figure))
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
    /// As `ToyOffer::kind`, and for the Toy Pad also "vehicle", for a tag
    /// the game wrote a vehicle or gadget to.
    pub kind: Option<String>,
    /// Where it may go: every place when Omoio doesn't know what it is.
    pub places: Vec<usize>,
    /// Used lately, so listed first.
    pub recent: bool,
    /// Named by the player in the menu, so its name is the one to show.
    pub named: bool,
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
                    let figure = known.figure(game.toy, &file).cloned();
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
                            named: known.named.contains(&file),
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

/// The figure named `name` in a maker's list: by the name exactly as the
/// emulator shows it, then without the spaces at its ends, then whatever its
/// case, and only where one figure has it. Exactly first, since a space can
/// be all that tells two apart: both Toy Pad lists have 1054 "Gyrosphere"
/// and 1056 " Gyrosphere" (Cemu's nsyshid/Dimensions.cpp lines 169 and 171,
/// v2.6-2; RPCS3's rpcs3qt/dimensions_dialog.cpp lines 167 and 169, master
/// b2e52d6; read 4 November 2026).
fn named<'a>(list: &'a [ToyFigure], name: &str) -> Option<&'a ToyFigure> {
    let only = |matches: Vec<&'a ToyFigure>| match matches.as_slice() {
        [one] => Some(*one),
        _ => None,
    };
    if name.trim().is_empty() {
        return None;
    }
    only(list.iter().filter(|figure| figure.name == name).collect())
        .or_else(|| only(list.iter().filter(|figure| figure.name == name.trim()).collect()))
        .or_else(|| {
            only(list.iter().filter(|figure| figure.name.trim().eq_ignore_ascii_case(name.trim())).collect())
        })
}

/// Learns which figure each of `names`'s files is, from the name the
/// emulator shows for the place it is in, looked up in `list`, the one its
/// figure maker gave (`listed`). Nothing is read from the file. A file Omoio
/// made is known already, apart from a Blank Tag (`blank`), and a name the
/// list doesn't hold, such as an emulator's "Unknown Figure", teaches
/// nothing. Whether anything was learned.
fn learned(known: &mut Known, toy: Toy, list: &[ToyFigure], names: &[(String, String)]) -> bool {
    let mut changed = false;
    for (file, name) in names {
        if known.made.get(file).is_some_and(|made| !blank(toy, made)) {
            continue;
        }
        let Some(figure) = named(list, name) else {
            continue;
        };
        if known.seen.get(file) != Some(figure) {
            known.seen.insert(file.clone(), figure.clone());
            changed = true;
        }
    }
    changed
}

/// Remembers which figure each of `names`'s files is, by file name, from
/// the name the emulator shows for it on the toy (`learned`).
fn seen(app: &AppHandle, game: &ToyGame, names: &[(String, String)]) {
    if names.is_empty() {
        return;
    }
    let Ok(record) = known_file(app, game.toy) else {
        return;
    };
    let mut known = read_known(&record);
    if learned(&mut known, game.toy, &kept_figures(app, game), names) {
        write_known(&record, &known);
    }
}

/// Which of the player's files Omoio put in each place of the running
/// game's toy, by file name, while the emulator whose process is `pid`
/// runs. A figure moved on the Toy Pad takes its open file along: both
/// emulators give the place's file to the new place (Cemu's
/// `DimensionsUSB::MoveFigure`, nsyshid/Dimensions.cpp lines 751 to 773,
/// v2.6-2; RPCS3's `dimensions_toypad::move_figure`,
/// rpcs3/Emu/Io/Dimensions.cpp lines 470 to 494, master b2e52d6; read 4
/// November 2026), so the record follows it here too.
struct OnToy {
    pid: u32,
    files: BTreeMap<usize, String>,
}

static ON_TOY: Mutex<Option<OnToy>> = Mutex::new(None);

/// The places of the game whose emulator is `pid`, begun empty for each new
/// game.
fn on_toy<T>(pid: u32, change: impl FnOnce(&mut BTreeMap<usize, String>) -> T) -> T {
    let mut on = ON_TOY.lock().unwrap_or_else(PoisonError::into_inner);
    if on.as_ref().is_some_and(|on| on.pid != pid) {
        *on = None;
    }
    change(&mut on.get_or_insert_with(|| OnToy { pid, files: BTreeMap::new() }).files)
}

/// `file` is in `place` now, and nowhere else: neither emulator holds one
/// file in two places of the Toy Pad (RPCS3 opens it locked,
/// rpcs3qt/dimensions_dialog.cpp line 743, master b2e52d6), and Omoio's Cemu
/// refuses a tag that is on already.
fn put(files: &mut BTreeMap<usize, String>, place: usize, file: String) {
    files.retain(|_, on| *on != file);
    files.insert(place, file);
}

/// The figure in `from` went to `to`. Whatever was in `to` came off first,
/// as both emulators take it off (`OnToy`).
fn moved(files: &mut BTreeMap<usize, String>, from: usize, to: usize) {
    let file = files.remove(&from);
    files.remove(&to);
    if let Some(file) = file {
        files.insert(to, file);
    }
}

/// What the places say now of the files Omoio put in them: a place the
/// emulator shows as empty holds no file any more, and every other one's
/// file is given with the name the emulator shows for it, as it shows it
/// (`named`).
fn read(files: &mut BTreeMap<usize, String>, places: &[String]) -> Vec<(String, String)> {
    files.retain(|&place, _| places.get(place).is_some_and(|name| !name.trim().is_empty()));
    files.iter().map(|(&place, file)| (file.clone(), places[place].clone())).collect()
}

/// Learns from what the places hold now which figure each file Omoio put on
/// them is. Only the Toy Pad's are learned this way: a tag becomes another
/// while it is on, as the game writes a vehicle or gadget to a Blank Tag
/// (`blank`), where a Disney Infinity figure stays the one it was named
/// when it went on (`after_load`).
fn learn(app: &AppHandle, game: &ToyGame, pid: u32, places: &[String]) {
    let names = on_toy(pid, |files| read(files, places));
    if game.toy == Toy::ToyPad {
        seen(app, game, &names);
    }
}

fn file_name(figure: &Path) -> Option<String> {
    figure.file_name().map(|name| name.to_string_lossy().into_owned())
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
    goes(game, known(app, game.toy).figure(game.toy, &file), place)?;
    back_up(app, game.toy, figure);
    Ok(())
}

/// After a figure went in `place` of the game whose emulator is `pid`:
/// remembered as used and as being there, and, for a file the player
/// brought, as the figure the emulator now names there.
pub fn after_load(app: &AppHandle, game: &ToyGame, pid: u32, figure: &Path, place: usize, places: &[String]) {
    used(app, game.toy, figure);
    let Some(file) = file_name(figure) else {
        return;
    };
    on_toy(pid, |files| put(files, place, file.clone()));
    if game.toy == Toy::ToyPad {
        learn(app, game, pid, places);
    } else if let Some(name) = places.get(place).filter(|name| !name.trim().is_empty()) {
        seen(app, game, &[(file, name.clone())]);
    }
}

/// Before a new figure is made for `place`: the figure the maker's list
/// gives for `number` and `series`, checked to be one Omoio makes and to go
/// there, and the file it goes in. A Blank Tag's file is "Blank Tag.bin",
/// or the first of "Blank Tag 2.bin" and on that is free.
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
    if !makes(game, &figure) {
        return Err("Vehicles and gadgets go on a Blank Tag. Put one on the centre pad and build the vehicle in the game.".to_string());
    }
    goes(game, Some(&figure), place)?;
    let file = new_figure(app, game.toy, &figure.name)?;
    Ok((figure, file))
}

/// After a new figure went in `place`: remembered as used and as being
/// there.
pub fn after_create(app: &AppHandle, game: &ToyGame, pid: u32, file: &Path, place: usize, places: &[String]) {
    used(app, game.toy, file);
    if let Some(name) = file_name(file) {
        on_toy(pid, |files| put(files, place, name));
    }
    learn(app, game, pid, places);
}

/// After the places were read: what they hold now, learned for the files
/// Omoio put on them.
pub fn after_read(app: &AppHandle, game: &ToyGame, pid: u32, places: &[String]) {
    learn(app, game, pid, places);
}

/// After the figure in `place` came off.
pub fn after_remove(app: &AppHandle, game: &ToyGame, pid: u32, place: usize, places: &[String]) {
    on_toy(pid, |files| files.remove(&place));
    learn(app, game, pid, places);
}

/// Before a figure on the Toy Pad moves from `from` to `to`: that the game
/// is played with the Toy Pad, and both are its places.
pub fn before_move(game: &ToyGame, from: usize, to: usize) -> Result<(), String> {
    if game.toy != Toy::ToyPad {
        return Err(NO_MENU.to_string());
    }
    if from.max(to) >= game.toy.places() {
        return Err("That figure doesn't go there.".to_string());
    }
    Ok(())
}

/// After the figure in `from` moved to `to`: its file is recorded there.
pub fn after_move(app: &AppHandle, game: &ToyGame, pid: u32, from: usize, to: usize, places: &[String]) {
    on_toy(pid, |files| moved(files, from, to));
    learn(app, game, pid, places);
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

/// What a toy calls one of the player's figure files, and where they go on.
fn words(toy: Toy) -> (&'static str, &'static str) {
    match toy {
        Toy::ToyPad => ("tag", "the Toy Pad"),
        Toy::Infinity => ("figure", "the base"),
        Toy::Skylanders => ("figure", "the portal"),
    }
}

/// Whether the file of this name is on the toy of the running game, as
/// Omoio put it there.
fn on_the_toy(file: &str) -> bool {
    let on = ON_TOY.lock().unwrap_or_else(PoisonError::into_inner);
    on.as_ref().is_some_and(|on| on.files.values().any(|on| on == file))
}

/// Gives one of the player's toy figures the name they typed. The file
/// takes it by `figures::free_name`'s rules, without a number added: a name
/// another file has already is refused, and no file is ever replaced. What
/// Omoio keeps under the file's name moves with it: which figure it is
/// (`Known`), its place among the ones used lately, and its backups. One on
/// the toy is refused, as deleting it is in the menu, since the emulator
/// holds its file open and writes to it (`delete` in src/toypad.ts).
/// Returns the file's new path.
pub fn rename(app: &AppHandle, figure: &str, name: &str) -> Result<String, String> {
    let path = Path::new(figure);
    let (toy, old) = TOYS
        .into_iter()
        .find_map(|toy| Some((toy, saved(app, toy, path).ok()?)))
        .ok_or("That isn't one of your saved figures.")?;
    let (noun, on) = words(toy);
    if on_the_toy(&old) {
        return Err(format!("Take it off {on} first, then rename it."));
    }
    let clean = figures::file_safe(name);
    if clean.is_empty() {
        return Err(format!("Type a name for the {noun}."));
    }
    let new = format!("{clean}.{EXTENSION}");
    if new == old {
        return Ok(figure.to_string());
    }
    let dir = path.parent().ok_or("That isn't one of your saved figures.")?;
    let target = dir.join(&new);
    // Only the case changes: Windows sees one file, and renames it.
    let same_file = new.to_lowercase() == old.to_lowercase();
    if !same_file && target.exists() {
        return Err(format!("You have a {noun} called {clean} already."));
    }
    move_without_replacing(path, &target).map_err(|_| format!("Couldn't rename that {noun}."))?;
    if let Ok(record) = known_file(app, toy) {
        let mut known = read_known(&record);
        known.rename(&old, &new);
        write_known(&record, &known);
    }
    if let Ok(data) = crate::portal_menu::data_dir(app) {
        let file = data.join("settings.json");
        let mut settings = Settings::load(&file);
        if let Some(list) = recent_of(&mut settings, toy) {
            if list.contains(&old) {
                list.retain(|used| *used != new);
                if let Some(used) = list.iter_mut().find(|used| **used == old) {
                    *used = new.clone();
                }
                let _ = settings.save(&file);
            }
        }
    }
    move_backups(&dir.join("Backups"), &old, &new, same_file);
    Ok(target.to_string_lossy().into_owned())
}

/// Renames a file, failing rather than replacing one already there:
/// MoveFileExW without MOVEFILE_REPLACE_EXISTING (Microsoft Learn), where
/// `std::fs::rename` replaces.
fn move_without_replacing(from: &Path, to: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVE_FILE_FLAGS};
    let wide = |path: &Path| path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let (from, to) = (wide(from), wide(to));
    unsafe { MoveFileExW(PCWSTR(from.as_ptr()), PCWSTR(to.as_ptr()), MOVE_FILE_FLAGS(0)) }.map_err(|err| err.to_string())
}

/// A renamed figure's backups (`back_up`) go with it, so putting it back as
/// it was still finds them. Where copies are kept under the new name
/// already, from a figure of that name deleted before, they stay, and these
/// join them under names of their own. Where only the case changes, Windows
/// sees one folder, which is renamed.
fn move_backups(backups: &Path, old: &str, new: &str, same_file: bool) {
    let (from, to) = (backups.join(old), backups.join(new));
    if !from.is_dir() {
        return;
    }
    if same_file || !to.exists() {
        let _ = move_without_replacing(&from, &to);
        return;
    }
    for copy in std::fs::read_dir(&from).into_iter().flatten().flatten() {
        let copy = copy.path();
        let (Some(second), Some(extension)) =
            (copy.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<u64>().ok()), copy.extension())
        else {
            continue;
        };
        let mut second = second;
        let mut into = to.join(second.to_string()).with_extension(extension);
        while into.exists() {
            second += 1;
            into = to.join(second.to_string()).with_extension(extension);
        }
        let _ = move_without_replacing(&copy, &into);
    }
    let _ = std::fs::remove_dir(&from);
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
    fn a_renamed_tag_keeps_what_omoio_knows_of_it() {
        let blank = ToyFigure { name: "Blank Tag".to_string(), number: 0, series: None };
        let car = ToyFigure { name: "Police Car".to_string(), number: 1000, series: None };
        let mut known = Known::default();
        known.made.insert("Blank Tag 2.bin".to_string(), blank.clone());
        known.seen.insert("Blank Tag 2.bin".to_string(), car.clone());
        // A record left from a file of the new name deleted by hand goes.
        known.seen.insert("My car.bin".to_string(), blank.clone());
        known.rename("Blank Tag 2.bin", "My car.bin");
        assert_eq!(known.figure(Toy::ToyPad, "My car.bin"), Some(&car));
        assert_eq!(known.figure(Toy::ToyPad, "Blank Tag 2.bin"), None);
        assert!(known.named.contains("My car.bin") && !known.named.contains("Blank Tag 2.bin"));
        assert!(known.forget("My car.bin"));
        assert!(known.named.is_empty());
    }

    #[test]
    fn a_rename_never_replaces_a_file_and_takes_its_backups_along() {
        let dir = std::env::temp_dir().join(format!("omoio-rename-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Backups").join("Blank Tag.bin")).unwrap();
        std::fs::create_dir_all(dir.join("Backups").join("Batman.bin")).unwrap();
        std::fs::write(dir.join("Blank Tag.bin"), b"tag").unwrap();
        std::fs::write(dir.join("Batman.bin"), b"batman").unwrap();
        std::fs::write(dir.join("Backups").join("Blank Tag.bin").join("100.bin"), b"old tag").unwrap();
        std::fs::write(dir.join("Backups").join("Batman.bin").join("100.bin"), b"old batman").unwrap();

        // Onto a name another file has: refused, and both files as they were.
        assert!(move_without_replacing(&dir.join("Blank Tag.bin"), &dir.join("Batman.bin")).is_err());
        assert_eq!(std::fs::read(dir.join("Batman.bin")).unwrap(), b"batman");
        assert_eq!(std::fs::read(dir.join("Blank Tag.bin")).unwrap(), b"tag");
        // Only the case changing is the same file, renamed.
        move_without_replacing(&dir.join("Blank Tag.bin"), &dir.join("blank tag.bin")).unwrap();
        let names: Vec<String> =
            std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(names.contains(&"blank tag.bin".to_string()), "{names:?}");

        // Backups go along; where the new name has some already, both stay.
        move_backups(&dir.join("Backups"), "Blank Tag.bin", "Police Car.bin", false);
        assert_eq!(std::fs::read(dir.join("Backups").join("Police Car.bin").join("100.bin")).unwrap(), b"old tag");
        std::fs::create_dir_all(dir.join("Backups").join("Bat.bin")).unwrap();
        std::fs::write(dir.join("Backups").join("Bat.bin").join("100.bin"), b"older").unwrap();
        move_backups(&dir.join("Backups"), "Batman.bin", "Bat.bin", false);
        // Only the case changing: the folder itself is renamed.
        move_backups(&dir.join("Backups"), "Bat.bin", "BAT.bin", true);
        let folders: Vec<String> = std::fs::read_dir(dir.join("Backups"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(folders.contains(&"BAT.bin".to_string()), "{folders:?}");
        assert_eq!(std::fs::read(dir.join("Backups").join("BAT.bin").join("101.bin")).unwrap(), b"old batman");
        assert_eq!(std::fs::read(dir.join("Backups").join("Bat.bin").join("100.bin")).unwrap(), b"older");
        assert_eq!(std::fs::read(dir.join("Backups").join("Bat.bin").join("101.bin")).unwrap(), b"old batman");
        assert!(!dir.join("Backups").join("Batman.bin").exists());
        let _ = std::fs::remove_dir_all(&dir);
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
        assert_eq!(read.figure(Toy::Infinity, "from a friend.bin").map(|f| f.number), Some(1000100));
        assert_eq!(read.figure(Toy::Infinity, "never seen.bin"), None);
        // What Omoio made wins over a name the emulator showed.
        let mut both = read;
        both.seen.insert("Mr. Incredible.bin".to_string(), figure("Captain America", 1000100, 2));
        assert_eq!(both.figure(Toy::Infinity, "Mr. Incredible.bin").map(|f| f.number), Some(1000001));
        assert!(both.forget("Mr. Incredible.bin"));
        assert_eq!(both.figure(Toy::Infinity, "Mr. Incredible.bin"), None);
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
        // Two of the Toy Pad's vehicles are told apart by a space alone.
        let gyrospheres = [tag("Gyrosphere", 1054), tag("Sonic Beam Gyrosphere", 1055), tag(" Gyrosphere", 1056)];
        assert_eq!(named(&gyrospheres, "Gyrosphere").map(|f| f.number), Some(1054));
        assert_eq!(named(&gyrospheres, " Gyrosphere").map(|f| f.number), Some(1056));
        assert_eq!(named(&gyrospheres, "gyrosphere"), None, "whatever its case, two have it");
        assert_eq!(named(&gyrospheres, "sonic beam gyrosphere ").map(|f| f.number), Some(1055));
        assert_eq!(named(&gyrospheres, "   "), None);
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

    fn toypad() -> ToyGame {
        ToyGame { toy: Toy::ToyPad, console: Console::WiiU, game: None, series: None, infinity: None }
    }

    fn tag(name: &str, number: u32) -> ToyFigure {
        ToyFigure { name: name.to_string(), number, series: None }
    }

    /// Some of each kind from the makers' lists, the same in both emulators
    /// (Cemu's `s_listMinis` and `s_listTokens`, nsyshid/Dimensions.cpp
    /// lines 25 to 380, v2.6-2; RPCS3's rpcs3qt/dimensions_dialog.cpp lines
    /// 23 to 378, master b2e52d6).
    fn tags() -> Vec<ToyFigure> {
        vec![
            tag("Blank Tag", 0),
            tag("Batman", 1),
            tag("Supergirl Red Lantern", 769),
            tag("Police Car", 1000),
            tag("Batmobile", 1006),
            tag("Batblaster", 1007),
            tag("Unknown", 1100),
            tag("Unknown", 1101),
        ]
    }

    #[test]
    fn a_blank_tag_is_a_bin_named_for_it_that_isnt_taken() {
        assert_eq!(free_name("Blank Tag", |_| false), "Blank Tag.bin");
        let taken = ["Blank Tag.bin", "Blank Tag 2.bin"];
        assert_eq!(free_name("Blank Tag", |file| taken.contains(&file)), "Blank Tag 3.bin");
    }

    #[test]
    fn the_toy_pad_is_offered_its_minifigures_and_the_blank_tag_on_any_place() {
        let offers = offered(tags(), &toypad());
        let names: Vec<&str> = offers.iter().map(|offer| offer.name.as_str()).collect();
        assert_eq!(names, ["Blank Tag", "Batman", "Supergirl Red Lantern"], "no vehicle is made from its id");
        let kinds: Vec<&str> = offers.iter().map(|offer| offer.kind.as_str()).collect();
        assert_eq!(kinds, ["blank_tag", "minifigure", "minifigure"]);
        assert!(offers.iter().all(|offer| offer.places == (0..7).collect::<Vec<_>>() && offer.series.is_none()));
    }

    #[test]
    fn a_tag_goes_in_any_of_the_toy_pads_seven_places() {
        let game = toypad();
        assert!(goes(&game, Some(&tag("Batman", 1)), 0).is_ok());
        assert!(goes(&game, Some(&tag("Batmobile", 1006)), 6).is_ok(), "a tag the game wrote a vehicle to");
        assert!(goes(&game, None, 6).is_ok());
        assert!(goes(&game, None, 7).is_err(), "the Toy Pad has seven places");
        assert!(makes(&game, &tag("Blank Tag", 0)) && makes(&game, &tag("Batman", 1)));
        assert!(!makes(&game, &tag("Batmobile", 1006)));
        assert!(makes(&one(), &figure("Mickey's Car", 4000018, 1)), "Disney Infinity's pieces are all made");
    }

    #[test]
    fn a_blank_tag_becomes_the_vehicle_the_game_wrote_to_it() {
        let list = tags();
        let mut known = Known::default();
        known.made.insert("Blank Tag.bin".to_string(), tag("Blank Tag", 0));
        known.made.insert("Batman.bin".to_string(), tag("Batman", 1));
        let says = |file: &str, name: &str| [(file.to_string(), name.to_string())];
        assert_eq!(known.figure(Toy::ToyPad, "Blank Tag.bin").map(|f| f.number), Some(0));
        assert!(learned(&mut known, Toy::ToyPad, &list, &says("Blank Tag.bin", "Batmobile")));
        assert_eq!(known.figure(Toy::ToyPad, "Blank Tag.bin").map(|f| f.number), Some(1006));
        assert!(!learned(&mut known, Toy::ToyPad, &list, &says("Blank Tag.bin", "Batmobile")), "nothing new");
        // Rebuilt in the game.
        assert!(learned(&mut known, Toy::ToyPad, &list, &says("Blank Tag.bin", "Batblaster")));
        assert_eq!(known.figure(Toy::ToyPad, "Blank Tag.bin").map(|f| f.number), Some(1007));
        // A minifigure Omoio made stays the one it asked for.
        assert!(!learned(&mut known, Toy::ToyPad, &list, &says("Batman.bin", "Police Car")));
        assert_eq!(known.figure(Toy::ToyPad, "Batman.bin").map(|f| f.number), Some(1));
        // A tag the player brought is what the emulator names it.
        assert!(learned(&mut known, Toy::ToyPad, &list, &says("from a friend.bin", "Supergirl Red Lantern")));
        assert_eq!(known.figure(Toy::ToyPad, "from a friend.bin").map(|f| f.number), Some(769));
        // A name the list doesn't hold, or holds twice, says nothing.
        assert!(!learned(&mut known, Toy::ToyPad, &list, &says("odd.bin", "Unknown")));
        assert!(!learned(&mut known, Toy::ToyPad, &list, &says("odd.bin", "Unknown (4000)")));
        // Only a Toy Pad tag is a Blank Tag.
        let mut base = Known::default();
        base.made.insert("odd.bin".to_string(), ToyFigure { name: "Blank Tag".into(), number: 0, series: None });
        assert!(!learned(&mut base, Toy::Infinity, &list, &says("odd.bin", "Batmobile")));
    }

    #[test]
    fn the_record_follows_a_tag_from_place_to_place() {
        let mut files = BTreeMap::new();
        put(&mut files, 1, "Batman.bin".to_string());
        put(&mut files, 0, "Blank Tag.bin".to_string());
        moved(&mut files, 1, 3);
        assert_eq!(files, BTreeMap::from([(0, "Blank Tag.bin".to_string()), (3, "Batman.bin".to_string())]));
        moved(&mut files, 3, 3);
        assert_eq!(files.get(&3).map(String::as_str), Some("Batman.bin"), "lifted and put back");
        moved(&mut files, 3, 0);
        assert_eq!(files, BTreeMap::from([(0, "Batman.bin".to_string())]), "what was there came off");
        put(&mut files, 2, "Batman.bin".to_string());
        assert_eq!(files, BTreeMap::from([(2, "Batman.bin".to_string())]), "one file is in one place");
        put(&mut files, 5, "Blank Tag.bin".to_string());
        moved(&mut files, 4, 5);
        assert_eq!(files.get(&5), None, "a figure Omoio didn't put on took the place");
        put(&mut files, 5, "Blank Tag.bin".to_string());
        let mut places = vec![String::new(); 7];
        places[2] = "Batman".to_string();
        places[5] = " Gyrosphere".to_string();
        assert_eq!(
            read(&mut files, &places),
            [("Batman.bin".to_string(), "Batman".to_string()), ("Blank Tag.bin".to_string(), " Gyrosphere".to_string())],
            "each name as the emulator shows it"
        );
        places[5] = " ".to_string();
        assert_eq!(read(&mut files, &places).len(), 1, "a place of spaces is empty");
        put(&mut files, 5, "Blank Tag.bin".to_string());
        places[5] = "Batmobile".to_string();
        places[2].clear();
        assert_eq!(read(&mut files, &places).len(), 1, "a place that reads empty holds no file");
        assert_eq!(files.get(&2), None);
        assert!(read(&mut files, &[]).is_empty() && files.is_empty());
    }

    #[test]
    fn each_game_begins_with_nothing_on_its_toy() {
        let (first, second) = (u32::MAX - 1, u32::MAX - 2);
        on_toy(first, |files| put(files, 1, "Batman.bin".to_string()));
        assert_eq!(on_toy(first, |files| files.len()), 1);
        assert_eq!(on_toy(second, |files| files.len()), 0);
    }

    #[test]
    fn a_figure_moves_only_between_the_toy_pads_places() {
        assert!(before_move(&toypad(), 0, 6).is_ok());
        assert!(before_move(&toypad(), 1, 1).is_ok(), "lifted and put back");
        assert!(before_move(&toypad(), 0, 7).is_err());
        assert!(before_move(&toypad(), 7, 0).is_err());
        assert!(before_move(&one(), 3, 6).is_err(), "nothing moves on the Infinity Base");
    }

    #[test]
    fn the_players_tags_are_listed_with_their_kinds() {
        let dir = scratch("tags");
        let write = |name: &str, size: usize| std::fs::write(dir.join(name), vec![0u8; size]).unwrap();
        write("Batman.bin", 180);
        write("Blank Tag.bin", 180);
        write("from a friend.bin", 180);
        write("Stitch.bin", 320);
        let mut known = Known::default();
        known.made.insert("Batman.bin".to_string(), tag("Batman", 1));
        known.made.insert("Blank Tag.bin".to_string(), tag("Blank Tag", 0));
        known.seen.insert("Blank Tag.bin".to_string(), tag("Batmobile", 1006));
        let files = files_in(&dir, &known, &[], &toypad());
        let listed: Vec<(&str, Option<&str>)> =
            files.iter().map(|file| (file.name.as_str(), file.kind.as_deref())).collect();
        // A Disney Infinity figure's size isn't a tag's.
        assert_eq!(
            listed,
            [("Batman", Some("minifigure")), ("Blank Tag", Some("vehicle")), ("from a friend", None)]
        );
        assert!(files.iter().all(|file| file.places == (0..7).collect::<Vec<_>>()));
        assert_eq!(files[1].figure.as_ref().map(|f| f.name.as_str()), Some("Batmobile"));
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
