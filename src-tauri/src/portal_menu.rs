//! The Skylanders menu: an Omoio window over the running game, opened with
//! the Guide button and used with the pad alone. It shows the figures on the
//! portal, the saved ones, and every character the game reads sorted by
//! element, and puts one on or takes one off through the game's emulator.
//!
//! Figures are the user's own files, copied into Omoio's figures folder from
//! Settings, or new ones of any character, which the emulator's own figure
//! maker makes into the same folder. Omoio never writes figure data itself;
//! a Trap Team trap is read, for the villain it holds, and a file the user
//! brought only as far as its plain first blocks, to tell a Creation Crystal.
//! Before a figure goes on, a copy of its file is kept aside (`back_up`).

use crate::backends::EmulatorBackend;
use crate::core::console::Console;
use crate::core::figure_data::{self, Trapped};
use crate::core::figures::{self, Character, Class, Element, Kind, Movement};
use crate::core::imaginators::{self, BattleClass, Casing};
use crate::core::settings::Settings;
use crate::core::vehicles::{self, Terrain};
use crate::core::villains::{Villain, VILLAINS};
use crate::session::Session;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_INSERT, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_TAB,
};

const LABEL: &str = "portal";

/// What the emulators take as a figure file: Cemu's and RPCS3's own filters.
const EXTENSIONS: [&str; 4] = ["sky", "bin", "dump", "dmp"];

/// How many recently used figures are remembered, to list them first.
const RECENT: usize = 30;

/// The kind of pad that opened the menu, so its buttons are named as printed
/// on that pad.
static FAMILY: Mutex<String> = Mutex::new(String::new());

/// The key that opens and closes the menu as the pad button does, for a
/// player with no pad Omoio can read (a player tried F8 and other keys,
/// reported 8 to 14 October 2026). Insert, since none of the three
/// emulators uses it unless the user binds it:
/// - RPCS3's shortcuts are Ctrl, Alt or Shift with a letter, a number, plus,
///   minus, F5, F10 or F11, and Alt+Return, Esc, F11 and F12, and its
///   keyboard pad uses W A S D, Q E R T F G, Z X C V, the arrows, Return,
///   Space, Backspace, Delete, End, Page Down and Home
///   (`rpcs3qt/shortcut_settings.cpp`, `shortcut_settings::shortcut_settings`,
///   lines 64 to 97; `Input/keyboard_pad_handler.cpp`, `init_config`, lines
///   37 to 61; commit 6d7020d5). Its game window hears no other key
///   (`rpcs3qt/gs_frame.cpp`, `handle_shortcut`).
/// - Cemu 2.6 hears Escape, Alt+Return, F11 and F12 in its game window, and
///   Alt+F4 and Ctrl+Q (`gui/MainWindow.cpp`, `OnKeyUp` and `OnKeyDown`,
///   lines 1426 to 1453, tag v2.6), and gives a pad no key unless the user
///   does: its own layouts are for SDL and XInput pads only
///   (`input/emulated/VPADController.cpp`, `set_default_mapping`, line 509).
/// - Dolphin 2609a's hotkeys are Escape, Tab, F1 to F12 alone or with Shift
///   or Alt, Alt+Return, Alt+A, Ctrl with O, P or I, and for a Game Boy
///   Advance Ctrl+Shift with O, W or R, M, and the numeric keypad's 1 to 4,
///   plus and minus (`Core/HotkeyManager.cpp`, `HotkeyManager::LoadDefaults`,
///   lines 395 to 476). Its Wii Remote uses 1, 2, Q, E, Return and the
///   arrows, and its Nunchuk W A S D, left Ctrl and left Shift
///   (`Core/HW/WiimoteEmu/WiimoteEmu.cpp`, `Wiimote::LoadDefaults`, lines 689
///   to 798; `Extension/Nunchuk.cpp`, `Nunchuk::LoadDefaults`, line 177);
///   its GameCube pad X Z C S D, Return, T F G H, I J K L, 1 2 3, Q W, Ctrl,
///   Shift and the arrows (`Core/HW/GCPadEmu.cpp`, `GCPad::LoadDefaults`,
///   lines 190 to 262; tag 2609a).
/// - The menu itself uses the arrows, Page Up and Down, Enter, Delete,
///   Backspace, Escape, P and Y (`src/portal.ts`).
/// With Shift or Ctrl held it is left alone, since text boxes paste and copy
/// with those. All read 14 October 2026. The menu and the Portal button's
/// setting name it.
const MENU_KEY: VIRTUAL_KEY = VK_INSERT;

/// Tab opens and closes the menu too, for a keyboard with no Insert key,
/// such as a Surface's (Bertram, 21 October 2026): a keyboard player's
/// mouse is kept inside the game (cursor.rs), so the Portal button can't be
/// reached either. Tab is on every keyboard and is no key a player can be
/// given (core/keys.rs). It counts only with the game's window or the menu
/// in front, so it still moves between buttons in Omoio's own window, and
/// never with Ctrl, Alt or a Windows key held, nor with Alt or a Windows key
/// held at the look before, so Alt+Tab and Windows+Tab still switch
/// programs however quickly they are let go. Shift is let through: a
/// keyboard player holds it for the left stick's press (core/keys.rs).
///
/// The emulators still hear Tab, since Omoio can't keep a key from them.
/// RPCS3 and Cemu give it no job (rpcs3qt/shortcut_settings.cpp lines 73 to
/// 94, build 0.0.43-20344; MainWindow::OnKeyUp, Cemu v2.6), though RPCS3
/// hears it as another key, Backtab, while Shift is held (Qt's
/// qwindowskeymapper.cpp lines 1219 to 1220, Qt 6.12.0), which no layout
/// uses. Dolphin turns its speed limit off while Tab is held
/// (HotkeyManager.cpp line 433, `HK_TOGGLE_THROTTLE`, 2609a), which the
/// Hotkeys.ini Omoio writes leaves out (backends/dolphin/settings.rs,
/// `quiet_hotkeys`) unless the player has set hotkeys up in Dolphin.
const SECOND_MENU_KEY: VIRTUAL_KEY = VK_TAB;

/// Whether the menu key went down since the last look, from whether it was
/// down then (`was`), is down now (`down`), and Windows' note that it went
/// down since the last ask (`tapped`). Looked at every 50 ms, a tap of about
/// a tenth of a second is seen down; one shorter than a look is caught by
/// the note. The note isn't relied on alone, since another program asking
/// can take it (`GetAsyncKeyState`, Microsoft's documentation, Remarks). Once
/// the key has been seen down, nothing counts again until it has been seen
/// up, so holding it never opens and closes the menu over and over.
fn key_went_down(was: bool, down: bool, tapped: bool) -> bool {
    !was && (down || tapped)
}

/// Reads `key`, remembering in `was` whether it was down.
fn menu_key_pressed(key: VIRTUAL_KEY, was: &mut bool) -> bool {
    let state = unsafe { GetAsyncKeyState(i32::from(key.0)) } as u16;
    let down = state & 0x8000 != 0;
    let pressed = key_went_down(*was, down, state & 0x0001 != 0);
    *was = down;
    pressed
}

/// Whether one of Omoio's windows or the game's own window is in front, so
/// a key pressed in another program, or typed in one of the emulator's
/// other windows such as its settings, does nothing here. The key is read
/// from anywhere, as F11 is (`session::watch`).
fn omoio_or_game_in_front(game: Option<isize>) -> bool {
    use crate::backends::rpcs3::overlay;
    let front = overlay::front();
    front != 0 && (Some(front) == game || overlay::process_of(front) == std::process::id())
}

fn key_held(key: VIRTUAL_KEY) -> bool {
    let state = unsafe { GetAsyncKeyState(i32::from(key.0)) } as u16;
    state & 0x8000 != 0
}

/// Whether Shift or Ctrl is held with the menu key.
fn with_shift_or_ctrl() -> bool {
    key_held(VK_SHIFT) || key_held(VK_CONTROL)
}

/// Whether the game's window or the menu is in front, for `SECOND_MENU_KEY`.
fn game_or_menu_in_front(app: &AppHandle, game: Option<isize>) -> bool {
    use crate::backends::rpcs3::overlay;
    let front = overlay::front();
    let menu = app
        .get_webview_window(LABEL)
        .filter(|_| showing(app))
        .and_then(|window| window.hwnd().ok())
        .map(|hwnd| hwnd.0 as isize);
    front != 0 && (Some(front) == game || Some(front) == menu)
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().data_dir().map_err(|e| e.to_string())?.join("Omoio"))
}

/// Where the user's figure files are kept.
pub fn folder(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("figures"))
}

fn is_figure(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|known| known.eq_ignore_ascii_case(e)))
}

#[derive(serde::Serialize)]
pub struct Figure {
    /// The file's name without its extension.
    pub name: String,
    pub path: String,
    /// The character, for a figure Omoio had the emulator make, or for a
    /// Creation Crystal the user brought, told from its file so the menu can
    /// keep it with the crystals. Any other file the user brought has none,
    /// and is listed in every game as before.
    pub id: Option<u16>,
    pub variant: Option<u16>,
    pub element: Option<Element>,
    pub kind: Option<Kind>,
    pub series: Option<u8>,
    /// How a swapper's bottom half moves.
    pub movement: Option<Movement>,
    pub class: Option<Class>,
    /// A vehicle's terrain, or a trophy's.
    pub terrain: Option<Terrain>,
    /// A vehicle's own SuperCharger, or a SuperCharger's own vehicle.
    pub partner: Option<u16>,
    /// A Sensei's battle class.
    pub battle_class: Option<BattleClass>,
    /// A Creation Crystal's casing.
    pub casing: Option<Casing>,
    /// The villain a trap holds, read from the data the game wrote to it.
    pub holds: Option<Trapped>,
}

/// The villain in a trap file. Only a trap is opened, and only read.
fn held(path: &Path, id: Option<u16>) -> Option<Trapped> {
    if !id.is_some_and(figure_data::is_trap) {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    figure_data::trapped(&<[u8; figure_data::SIZE]>::try_from(bytes.as_slice()).ok()?)
}

/// The id and variant of a Creation Crystal in a file the user brought. A
/// file that isn't a figure's size isn't opened.
fn brought_crystal(path: &Path) -> Option<[u16; 2]> {
    if path.metadata().ok()?.len() != figure_data::SIZE as u64 {
        return None;
    }
    crystal_in(&std::fs::read(path).ok()?)
}

/// A crystal's id and variant, read from the figure's first two blocks,
/// which no game encrypts. Nothing is decrypted, and a file whose own number
/// doesn't check out isn't taken for a figure.
fn crystal_in(bytes: &[u8]) -> Option<[u16; 2]> {
    let figure = <[u8; figure_data::SIZE]>::try_from(bytes).ok()?;
    let id = figure_data::id(&figure);
    (imaginators::is_crystal(id) && figure_data::number_ok(&figure)).then(|| [id, figure_data::variant(&figure)])
}

/// Which character each figure Omoio had made is, by file name.
fn made_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("made-figures.json"))
}

fn made_list(app: &AppHandle) -> BTreeMap<String, [u16; 2]> {
    made_file(app)
        .ok()
        .and_then(|file| std::fs::read_to_string(file).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Remembers which character a new figure is, so the menu files it under
/// its element and uses it again rather than making another.
pub fn made(app: &AppHandle, figure: &Path, character: &Character) {
    let (Ok(file), Some(name)) = (made_file(app), figure.file_name()) else {
        return;
    };
    let mut list = made_list(app);
    list.insert(name.to_string_lossy().into_owned(), [character.id, character.variant]);
    if let Ok(text) = serde_json::to_string(&list) {
        let _ = std::fs::write(file, text);
    }
}

/// The user's figure files, the ones used lately first, the rest by name.
pub fn list(app: &AppHandle) -> Vec<Figure> {
    let Ok(dir) = folder(app) else {
        return Vec::new();
    };
    let recent = data_dir(app)
        .map(|d| Settings::load(&d.join("settings.json")).recent_figures)
        .unwrap_or_default();
    let made = made_list(app);
    let mut found: Vec<Figure> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_file() && is_figure(path))
                .map(|path| {
                    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let character = made.get(&file).copied().or_else(|| brought_crystal(&path));
                    let holds = held(&path, character.map(|[id, _]| id));
                    Figure {
                        name: path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                        path: path.to_string_lossy().into_owned(),
                        id: character.map(|[id, _]| id),
                        variant: character.map(|[_, variant]| variant),
                        element: character.and_then(|[id, _]| figures::element(id)),
                        kind: character.map(|[id, _]| figures::kind(id)),
                        series: character.and_then(|[_, variant]| figures::series(variant)),
                        movement: character.and_then(|[id, _]| figures::movement(id)),
                        class: character.and_then(|[id, _]| figures::class(id)),
                        terrain: character.and_then(|[id, _]| vehicles::terrain(id)),
                        partner: character.and_then(|[id, _]| vehicles::partner(id)),
                        battle_class: character.and_then(|[id, _]| imaginators::battle_class(id)),
                        casing: character
                            .and_then(|[id, variant]| imaginators::crystal(id, variant))
                            .map(|crystal| crystal.casing),
                        holds,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort_by_cached_key(|figure| {
        let file = Path::new(&figure.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        (
            recent.iter().position(|used| *used == file).unwrap_or(usize::MAX),
            figure.name.to_lowercase(),
        )
    });
    found
}

/// A villain as the menu's villains tab shows it.
#[derive(serde::Serialize)]
pub struct VillainState {
    #[serde(flatten)]
    pub villain: Villain,
    /// Seen in one of the user's traps, now or before.
    pub caught: bool,
    /// The saved trap that holds it now.
    pub trap: Option<HeldIn>,
}

#[derive(serde::Serialize)]
pub struct HeldIn {
    pub name: String,
    pub path: String,
    pub id: u16,
    pub variant: u16,
    /// The trap's own element, `None` for the Kaos trap.
    pub element: Option<Element>,
    /// The villain's variant form, such as Outlaw Brawl and Chain.
    pub variant_form: bool,
    pub evolved: bool,
}

/// Which villains Omoio has seen in a trap. Kept, so a villain stays caught
/// once its trap has taken another.
fn caught_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("caught-villains.json"))
}

/// Every Trap Team villain: which ones the user has caught and which saved
/// trap holds each now.
pub fn villains(app: &AppHandle) -> Vec<VillainState> {
    let traps: Vec<Figure> = list(app).into_iter().filter(|figure| figure.holds.is_some()).collect();
    let file = caught_file(app).ok();
    let mut caught: std::collections::BTreeSet<u16> = file
        .as_ref()
        .and_then(|file| std::fs::read_to_string(file).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let known = caught.len();
    caught.extend(traps.iter().filter_map(|trap| trap.holds.map(|holds| holds.villain)));
    if caught.len() != known {
        if let (Some(file), Ok(text)) = (file, serde_json::to_string(&caught)) {
            let _ = std::fs::write(file, text);
        }
    }
    VILLAINS
        .iter()
        .map(|&villain| {
            let trap = traps.iter().find_map(|trap| {
                let holds = trap.holds.filter(|holds| holds.villain == villain.id)?;
                Some(HeldIn {
                    name: trap.name.clone(),
                    path: trap.path.clone(),
                    id: trap.id?,
                    variant: trap.variant?,
                    element: trap.element,
                    variant_form: holds.variant,
                    evolved: holds.evolved,
                })
            });
            VillainState { villain, caught: caught.contains(&villain.id), trap }
        })
        .collect()
}

/// Moves one of the user's saved figures to the Recycle Bin, and forgets
/// which character it was and that it was used. Only a figure file in the
/// figures folder is taken. The Recycle Bin rather than gone for good, so a
/// figure deleted by mistake, with everything its game saved on it, can be
/// put back.
pub fn delete(app: &AppHandle, figure: &str) -> Result<(), String> {
    let dir = folder(app)?;
    let path = Path::new(figure);
    let in_folder = path.parent().is_some_and(|parent| parent == dir);
    if !in_folder || !is_figure(path) || !path.is_file() {
        return Err("That isn't one of your saved figures.".to_string());
    }
    to_recycle_bin(path)?;
    let Some(name) = path.file_name().map(|name| name.to_string_lossy().into_owned()) else {
        return Ok(());
    };
    let mut list = made_list(app);
    if list.remove(&name).is_some() {
        if let (Ok(file), Ok(text)) = (made_file(app), serde_json::to_string(&list)) {
            let _ = std::fs::write(file, text);
        }
    }
    if let Ok(dir) = data_dir(app) {
        let file = dir.join("settings.json");
        let mut settings = Settings::load(&file);
        settings.recent_figures.retain(|used| *used != name);
        let _ = settings.save(&file);
    }
    Ok(())
}

/// Moves a file to the Recycle Bin as Explorer's Delete does, without
/// asking or showing anything: Windows' own file operation, with undo
/// allowed.
fn to_recycle_bin(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW,
    };
    // The list of files ends with an empty one, so two nulls.
    let from: Vec<u16> = path.as_os_str().encode_wide().chain([0, 0]).collect();
    let mut operation = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: windows::core::PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT).0 as u16,
        ..Default::default()
    };
    let failed = unsafe { SHFileOperationW(&mut operation) } != 0 || operation.fAnyOperationsAborted.as_bool();
    if failed || path.exists() {
        return Err("Couldn't move that figure to the Recycle Bin.".to_string());
    }
    Ok(())
}

/// Copies figure files the user picked into the figures folder. A file of the
/// same name already there is kept. Returns how many were added.
pub fn add(app: &AppHandle, paths: &[String]) -> Result<usize, String> {
    let dir = folder(app)?;
    std::fs::create_dir_all(&dir).map_err(|_| "Couldn't make the figures folder.".to_string())?;
    let mut added = 0;
    for path in paths {
        let from = Path::new(path);
        let Some(name) = from.file_name() else {
            continue;
        };
        let to = dir.join(name);
        if !is_figure(from) || to.exists() {
            continue;
        }
        std::fs::copy(from, &to).map_err(|_| "Couldn't copy that figure file.".to_string())?;
        added += 1;
    }
    Ok(added)
}

/// How many copies of one figure are kept, the newest. A copy is 1 KB.
const BACKUPS_KEPT: usize = 20;

/// Where the copies are kept, inside the figures folder so a player who
/// opens it from Settings finds them: a folder for each figure by its file
/// name, and in it the file as it was each time it went on, when it had
/// changed since, named for the second it was taken, as save backups are
/// (saves.rs).
fn backups(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(folder(app)?.join("Backups"))
}

/// Keeps a copy of a figure's file before it goes on the portal, so a
/// figure the game later calls broken can be put back as it was. While a
/// figure is on, its emulator writes each block the game sends straight into
/// the file (Cemu's `SkylanderUSB::WriteBlock` and `Skylander::Save`,
/// Skylander.cpp lines 919 to 939 and 1001 to 1008, v2.6; RPCS3's
/// `sky_portal::write_block` and `skylander::save`, Emu/Io/Skylander.cpp
/// lines 35 to 47 and 144 to 163, master 599ab4f; read 22 October 2026), so
/// a write cut off by the figure coming off or the emulator closing stays
/// as far as the game got. The copy is the file's bytes as they are; nothing
/// in them is decrypted or changed. A figure that can't be copied still goes
/// on.
pub fn back_up(app: &AppHandle, figure: &Path) {
    let taken = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    if let Ok(backups) = backups(app) {
        let _ = back_up_into(figure, &backups, taken);
    }
}

/// Copies `figure` into its folder among `backups`, unless the newest copy
/// there already holds the same bytes, and lets the oldest copies go past
/// `BACKUPS_KEPT`. Returns the copy made, if one was.
fn back_up_into(figure: &Path, backups: &Path, taken: u64) -> std::io::Result<Option<PathBuf>> {
    let (Some(name), Some(extension)) = (figure.file_name(), figure.extension()) else {
        return Ok(None);
    };
    let bytes = std::fs::read(figure)?;
    let kept = backups.join(name);
    let copies = copies_in(&kept);
    if copies.last().is_some_and(|newest| std::fs::read(newest).is_ok_and(|held| held == bytes)) {
        return Ok(None);
    }
    std::fs::create_dir_all(&kept)?;
    // Two in the same second, as a figure put on, taken off and put on
    // again can make, would share a name and the second would replace the
    // first.
    let mut second = taken;
    let mut copy = kept.join(second.to_string()).with_extension(extension);
    while copy.exists() {
        second += 1;
        copy = kept.join(second.to_string()).with_extension(extension);
    }
    std::fs::write(&copy, &bytes)?;
    let copies = copies_in(&kept);
    for old in &copies[..copies.len().saturating_sub(BACKUPS_KEPT)] {
        let _ = std::fs::remove_file(old);
    }
    Ok(Some(copy))
}

/// The copies kept of one figure, oldest first, by the second in each name.
fn copies_in(kept: &Path) -> Vec<PathBuf> {
    let mut copies: Vec<(u64, PathBuf)> = std::fs::read_dir(kept)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_file())
                .filter_map(|path| Some((path.file_stem()?.to_str()?.parse().ok()?, path)))
                .collect()
        })
        .unwrap_or_default();
    copies.sort();
    copies.into_iter().map(|(_, path)| path).collect()
}

/// The characters an emulator's figure maker offered, kept with the
/// emulator's version so a newer emulator is asked again.
#[derive(serde::Serialize, serde::Deserialize)]
struct Kept {
    version: String,
    characters: Vec<Character>,
}

/// Every character the running game's emulator can make a figure of. Read
/// from the emulator the first time and kept, since the list only changes
/// with a new emulator. An emulator whose maker doesn't list them with
/// their numbers gets those Omoio read from the others.
pub fn characters(
    app: &AppHandle,
    backend: &dyn EmulatorBackend,
    console: Console,
    pid: u32,
) -> Result<Vec<Character>, String> {
    let own = figures::characters_file(console);
    let version = backend.detect_version(app).unwrap_or_default();
    let file = data_dir(app)?.join(&own);
    let kept = std::fs::read_to_string(&file)
        .ok()
        .and_then(|text| serde_json::from_str::<Kept>(&text).ok())
        .filter(|kept| kept.version == version && !kept.characters.is_empty());
    let listed = match kept {
        Some(kept) => Ok(kept.characters),
        None => backend.portal_characters(pid).inspect(|characters| {
            if let Ok(text) = serde_json::to_string(&Kept { version, characters: characters.clone() }) {
                let _ = std::fs::write(&file, text);
            }
        }),
    };
    if backend.lists_characters() {
        return listed;
    }
    // A maker that names only some of its figures with their numbers, as
    // Dolphin's does (backends/dolphin/portal.rs, `characters`): those, and
    // the ones read from the other emulators' makers beside them.
    let listed = listed.unwrap_or_default();
    match from_the_others(app, &own) {
        Ok(others) => Ok(merge([listed, others].into_iter())),
        Err(_) if !listed.is_empty() => Ok(listed),
        Err(why) => Err(why),
    }
}

/// The characters kept from every other emulator's maker, each once by its
/// number, in the order of the files' names.
fn from_the_others(app: &AppHandle, own: &str) -> Result<Vec<Character>, String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(data_dir(app)?)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    files.retain(|file| {
        file.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("characters-") && name.ends_with(".json") && name != own)
    });
    files.sort();
    let lists = files
        .iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .filter_map(|text| serde_json::from_str::<Kept>(&text).ok())
        .map(|kept| kept.characters);
    let merged = merge(lists);
    if merged.is_empty() {
        Err("Omoio learns which figures can be made from Cemu's or RPCS3's figure maker. \
             Make one figure in a Skylanders game there once, and they show here too."
            .to_string())
    } else {
        Ok(merged)
    }
}

fn merge(lists: impl Iterator<Item = Vec<Character>>) -> Vec<Character> {
    let mut merged: Vec<Character> = Vec::new();
    for character in lists.flatten() {
        if !merged.iter().any(|known| known.id == character.id && known.variant == character.variant) {
            merged.push(character);
        }
    }
    merged
}

/// Where a new figure of a character is kept: the figures folder, under the
/// character's name.
pub fn new_figure(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let dir = folder(app)?;
    std::fs::create_dir_all(&dir).map_err(|_| "Couldn't make the figures folder.".to_string())?;
    Ok(dir.join(figures::free_name(name, |file| dir.join(file).exists())))
}

/// Remembers a figure as just used, so the menu lists it first.
pub fn used(app: &AppHandle, figure: &str) {
    let (Ok(dir), Some(name)) = (data_dir(app), Path::new(figure).file_name()) else {
        return;
    };
    let name = name.to_string_lossy().into_owned();
    let file = dir.join("settings.json");
    let mut settings = Settings::load(&file);
    settings.recent_figures.retain(|known| *known != name);
    settings.recent_figures.insert(0, name);
    settings.recent_figures.truncate(RECENT);
    let _ = settings.save(&file);
}

#[derive(Clone, serde::Serialize)]
struct MenuState {
    open: bool,
    family: String,
}

pub fn family() -> String {
    let family = FAMILY.lock().unwrap().clone();
    if family.is_empty() {
        "generic".to_string()
    } else {
        family
    }
}

fn tell(app: &AppHandle, open: bool) {
    let _ = app.emit_to(LABEL, "portal-menu", MenuState { open, family: family() });
}

/// Whether the menu is on the screen.
pub fn showing(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

/// Shows the menu over the whole screen Omoio is on. Made the first time and
/// kept, hidden, after that, so opening it again is instant. The game stops
/// hearing the pad while it is up.
fn open(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LABEL) {
        window.show().map_err(|e| e.to_string())?;
        let _ = window.set_focus();
        tell(app, true);
        crate::session::quiet_game(app);
        return Ok(());
    }
    let main = app.get_webview_window("main").ok_or("Omoio's window isn't there.")?;
    let monitor = main
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("No screen found for the menu.")?;
    let scale = monitor.scale_factor();
    let (position, size) = (monitor.position(), monitor.size());
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(PathBuf::from("portal.html")))
        .title("Portal")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(true)
        .position(position.x as f64 / scale, position.y as f64 / scale)
        .inner_size(size.width as f64 / scale, size.height as f64 / scale)
        .build()
        .map_err(|e| e.to_string())?;
    crate::session::quiet_game(app);
    Ok(())
}

/// Hides the menu. The game underneath carries on, and hears the pad again:
/// an RPCS3 game hears it only while its window is in front. While Cemu's
/// figure maker is at work, the game is let hear only once it is done
/// (`take_front`, which every portal command calls at its end): letting it
/// hear closes a window of Cemu's that the maker's own window sits over,
/// which Cemu can't do then, and the game stayed deaf
/// (`backends::cemu::portal::making`). The figure is made and kept all the
/// same, and is on the portal when the menu opens again.
pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        // The game takes the front while the menu still has it: hiding the
        // window in front lets Windows choose who gets the front next, and
        // in Big Picture Omoio may then be refused it
        // (session::give_game_front).
        app.state::<Session>().focus_game();
        let _ = window.hide();
        tell(app, false);
        if !crate::backends::cemu::portal::making() {
            crate::session::quiet_game(app);
        }
        crate::session::give_game_front(app);
    }
}

/// Puts the menu back in front after the emulator's own windows took it for
/// a figure, so the game stays deaf to the pad until the menu closes, and
/// Omoio may hand the game the keyboard then. With the menu closed while
/// the figure went on, the game is let hear again instead: those windows
/// may have taken the keyboard from it after the menu handed it over, and
/// with Cemu's figure maker at work `close` left the game deaf until now.
pub fn take_front(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL).filter(|_| showing(app)) {
        let _ = window.set_focus();
    } else {
        crate::session::quiet_game(app);
    }
}

/// What the menu shows instead of itself over a Skylanders game it isn't
/// offered in: the game page's own line (`figures::menu_note`). Its key and
/// its pad button open it there too, and it closes as the menu does, but
/// nothing is read from the portal or put on it, and the emulator's portal
/// window isn't opened (`session::quiet_game`). `None` where the menu works,
/// and while nothing runs.
pub fn note(app: &AppHandle) -> Option<String> {
    let playing = app.state::<Session>().playing()?;
    let backend = crate::backends::for_console(playing.console)?;
    figures::menu_note(backend.features(), playing.console, &playing.title)
}

/// Opens the menu from the Portal button over the running game, for a
/// player whose pad Omoio can't read. Only in a game the menu works in, as
/// the button shows only there. The window is made on the main thread, as
/// the pad's press makes it, and this waits until it is up.
pub fn open_by_button(app: &AppHandle) -> Result<(), String> {
    let playing = app.state::<Session>().playing().ok_or("Start a game first.")?;
    let works = crate::backends::for_console(playing.console)
        .is_some_and(|backend| figures::offers_portal_menu(backend.features(), playing.console, &playing.title));
    if !works {
        return Err("The portal menu doesn't work in this game.".to_string());
    }
    let (done, opened) = std::sync::mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = done.send(if showing(&handle) { Ok(()) } else { open(&handle) });
    })
    .map_err(|_| "Couldn't open the portal menu.".to_string())?;
    opened
        .recv_timeout(Duration::from_secs(10))
        .map_err(|_| "Couldn't open the portal menu.".to_string())?
        .map_err(|_| "Couldn't open the portal menu.".to_string())
}

/// The pad button that opens the menu, as a place on the pad.
pub fn button(app: &AppHandle) -> String {
    data_dir(app)
        .map(|dir| Settings::load(&dir.join("settings.json")).portal_button)
        .unwrap_or_else(|_| "Guide".to_string())
}

/// Makes `place` the button that opens the menu.
pub fn set_button(app: &AppHandle, place: &str) -> Result<(), String> {
    if !crate::core::pad_layout::INPUTS.contains(&place) {
        return Err("That isn't a button Omoio knows.".to_string());
    }
    let file = data_dir(app)?.join("settings.json");
    let mut settings = Settings::load(&file);
    settings.portal_button = place.to_string();
    settings.save(&file)
}

/// While a Skylanders game runs, the chosen button on any pad opens or
/// closes the menu, and so does the menu key while Omoio or the game is in
/// front. In one the menu isn't offered in, the same window shows only why
/// (`note`). Read twenty times a second, since a press lasts about a tenth
/// of one, and the choice is read again every two seconds so one made while
/// playing counts. It ends with the game and takes the menu with it.
pub fn watch(app: AppHandle, pid: u32) {
    std::thread::spawn(move || {
        let mut was = false;
        let mut wanted = button(&app);
        let mut ticks = 0u32;
        // Windows keeps its note of a press until asked, so one from before
        // the game started is cleared rather than taken for a press now.
        let (mut key_was, mut second_was, mut switching_was) = (false, false, false);
        menu_key_pressed(MENU_KEY, &mut key_was);
        menu_key_pressed(SECOND_MENU_KEY, &mut second_was);
        loop {
            std::thread::sleep(Duration::from_millis(50));
            ticks = ticks.wrapping_add(1);
            if ticks % 40 == 0 {
                wanted = button(&app);
            }
            // Read every time, in front or not, so a press meant for another
            // program is used up rather than kept for later.
            let key = menu_key_pressed(MENU_KEY, &mut key_was);
            let second = menu_key_pressed(SECOND_MENU_KEY, &mut second_was);
            // Alt+Tab let go between two looks reads as Tab alone.
            let switching = key_held(VK_MENU) || key_held(VK_LWIN) || key_held(VK_RWIN);
            let switched = std::mem::replace(&mut switching_was, switching) || switching;
            let session = app.state::<Session>();
            if session.pid() != Some(pid) {
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || {
                    if let Some(window) = handle.get_webview_window(LABEL) {
                        let _ = window.close();
                    }
                });
                return;
            }
            let Some(playing) = session.playing() else {
                continue;
            };
            if !figures::is_skylanders(&playing.title) {
                return;
            }
            let pressing = crate::pads::connected()
                .into_iter()
                .find(|pad| crate::pads::held(&pad.device).is_some_and(|held| held.iter().any(|h| *h == wanted)));
            let now = pressing.is_some();
            let by_pad = now && !was;
            let by_key = (key && !with_shift_or_ctrl() && omoio_or_game_in_front(session.window()))
                || (second && !key_held(VK_CONTROL) && !switched && game_or_menu_in_front(&app, session.window()));
            if by_pad || by_key {
                if let Some(pad) = pressing.filter(|_| by_pad) {
                    *FAMILY.lock().unwrap() = pad.family;
                }
                let handle = app.clone();
                // Windows are made and shown on the main thread.
                let _ = app.run_on_main_thread(move || {
                    if showing(&handle) {
                        close(&handle);
                    } else {
                        let _ = open(&handle);
                    }
                });
            }
            was = now;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_key_counts_once_for_each_press() {
        // Seen down at a look: a press.
        assert!(key_went_down(false, true, true));
        assert!(key_went_down(false, true, false), "Windows' note was taken by another program");
        // Down and up between two looks: only the note says so.
        assert!(key_went_down(false, false, true));
        // Held: not again, whatever the note says.
        assert!(!key_went_down(true, true, true));
        assert!(!key_went_down(true, true, false));
        // Let go: nothing, even with a note still there.
        assert!(!key_went_down(true, false, true));
        assert!(!key_went_down(false, false, false));
    }

    #[test]
    fn figure_files_are_told_by_the_emulators_own_extensions() {
        assert!(is_figure(Path::new("Whirlwind.sky")));
        assert!(is_figure(Path::new("Spyro.BIN")));
        assert!(is_figure(Path::new("x.dump")));
        assert!(!is_figure(Path::new("notes.txt")));
        assert!(!is_figure(Path::new("no extension")));
    }

    /// A figure as an emulator's figure maker makes it, with its number and
    /// checksum, as figure_data's own tests make one.
    fn made_figure(id: u16, variant: u16) -> Vec<u8> {
        let mut figure = vec![0u8; figure_data::SIZE];
        figure[..4].copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
        figure[0x10..0x12].copy_from_slice(&id.to_le_bytes());
        figure[0x1C..0x1E].copy_from_slice(&variant.to_le_bytes());
        let crc = figure_data::crc16(&figure[..0x1E]);
        figure[0x1E..0x20].copy_from_slice(&crc.to_le_bytes());
        figure
    }

    #[test]
    fn a_crystal_the_user_brought_is_told_by_its_plain_first_blocks() {
        assert_eq!(crystal_in(&made_figure(680, 0x5208)), Some([680, 0x5208]));
        // One whose design Omoio has no name for is still a crystal.
        assert_eq!(crystal_in(&made_figure(682, 0x5212)), Some([682, 0x5212]));
        // Every other figure stays unknown, as it was.
        assert_eq!(crystal_in(&made_figure(217, 0x3003)), None); // a trap
        assert_eq!(crystal_in(&made_figure(601, 0x5000)), None); // King Pen
        assert_eq!(crystal_in(&made_figure(16, 0x0000)), None); // Spyro
        // A file whose number doesn't check out, or isn't a figure's size.
        let mut broken = made_figure(680, 0x5208);
        broken[0x1E] ^= 0xFF;
        assert_eq!(crystal_in(&broken), None);
        assert_eq!(crystal_in(&made_figure(680, 0x5208)[..512]), None);
        assert_eq!(crystal_in(&[made_figure(680, 0x5208), vec![0]].concat()), None);
    }

    /// A folder of its own for one test, emptied first.
    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("omoio-figure-backups-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// King Pen as the game might have left him, `mark` standing in for
    /// what it wrote.
    fn played(mark: u8) -> Vec<u8> {
        let mut figure = made_figure(601, 0x5000);
        figure[0x80] = mark;
        figure
    }

    #[test]
    fn a_figure_is_copied_as_it_is_before_it_goes_on() {
        let root = scratch("copied");
        let figure = root.join("King Pen.sky");
        let backups = root.join("Backups");
        std::fs::write(&figure, played(1)).unwrap();
        let copy = back_up_into(&figure, &backups, 1_000).unwrap().unwrap();
        assert_eq!(copy, backups.join("King Pen.sky").join("1000.sky"));
        assert_eq!(std::fs::read(&copy).unwrap(), played(1));
        assert_eq!(std::fs::read(&figure).unwrap(), played(1), "the figure itself is left as it was");
        // On again, unchanged: nothing new.
        assert_eq!(back_up_into(&figure, &backups, 2_000).unwrap(), None);
        // Changed by the game since: a new copy beside the old one.
        std::fs::write(&figure, played(2)).unwrap();
        let newer = back_up_into(&figure, &backups, 3_000).unwrap().unwrap();
        assert_eq!(std::fs::read(&newer).unwrap(), played(2));
        assert_eq!(copies_in(&backups.join("King Pen.sky")), [copy, newer]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copies_taken_in_the_same_second_each_keep_their_own() {
        let root = scratch("second");
        let figure = root.join("King Pen.sky");
        let backups = root.join("Backups");
        std::fs::write(&figure, played(1)).unwrap();
        back_up_into(&figure, &backups, 500).unwrap();
        std::fs::write(&figure, played(2)).unwrap();
        let second = back_up_into(&figure, &backups, 500).unwrap().unwrap();
        assert_eq!(second, backups.join("King Pen.sky").join("501.sky"));
        assert_eq!(std::fs::read(backups.join("King Pen.sky").join("500.sky")).unwrap(), played(1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_the_newest_copies_are_kept() {
        let root = scratch("newest");
        let figure = root.join("King Pen.sky");
        let backups = root.join("Backups");
        let made = BACKUPS_KEPT as u64 + 3;
        // From 90, so the names go from two digits to three and are told
        // apart by number rather than as text.
        for n in 0..made {
            std::fs::write(&figure, played(n as u8)).unwrap();
            back_up_into(&figure, &backups, 90 + n).unwrap();
        }
        let copies = copies_in(&backups.join("King Pen.sky"));
        assert_eq!(copies.len(), BACKUPS_KEPT);
        assert_eq!(copies[0], backups.join("King Pen.sky").join("93.sky"));
        assert_eq!(std::fs::read(&copies[BACKUPS_KEPT - 1]).unwrap(), played(made as u8 - 1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn each_figure_file_has_copies_of_its_own() {
        let root = scratch("own");
        let backups = root.join("Backups");
        for name in ["King Pen.sky", "King Pen.bin", "King Pen 2.sky"] {
            std::fs::write(root.join(name), played(7)).unwrap();
            assert!(back_up_into(&root.join(name), &backups, 10).unwrap().is_some(), "{name}");
        }
        assert_eq!(copies_in(&backups.join("King Pen.bin")), [backups.join("King Pen.bin").join("10.bin")]);
        // A figure that isn't there anymore is left to the emulator to report.
        assert!(back_up_into(&root.join("Gone.sky"), &backups, 10).is_err());
        assert!(!backups.join("Gone.sky").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
