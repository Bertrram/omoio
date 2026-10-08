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

const LABEL: &str = "portal";

/// What the emulators take as a figure file: Cemu's and RPCS3's own filters.
const EXTENSIONS: [&str; 4] = ["sky", "bin", "dump", "dmp"];

/// How many recently used figures are remembered, to list them first.
const RECENT: usize = 30;

/// The kind of pad that opened the menu, so its buttons are named as printed
/// on that pad.
static FAMILY: Mutex<String> = Mutex::new(String::new());

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

/// The characters an emulator's figure maker offered, kept with the
/// emulator's version so a newer emulator is asked again.
#[derive(serde::Serialize, serde::Deserialize)]
struct Kept {
    version: String,
    characters: Vec<Character>,
}

/// Every character the running game's emulator can make a figure of. Read
/// from the emulator the first time and kept, since the list only changes
/// with a new emulator.
pub fn characters(
    app: &AppHandle,
    backend: &dyn EmulatorBackend,
    console: Console,
    pid: u32,
) -> Result<Vec<Character>, String> {
    let version = backend.detect_version(app).unwrap_or_default();
    let key = serde_json::to_string(&console).unwrap_or_default().replace('"', "");
    let file = data_dir(app)?.join(format!("characters-{key}.json"));
    let kept = std::fs::read_to_string(&file)
        .ok()
        .and_then(|text| serde_json::from_str::<Kept>(&text).ok())
        .filter(|kept| kept.version == version && !kept.characters.is_empty());
    if let Some(kept) = kept {
        return Ok(kept.characters);
    }
    let characters = backend.portal_characters(pid)?;
    if let Ok(text) = serde_json::to_string(&Kept { version, characters: characters.clone() }) {
        let _ = std::fs::write(&file, text);
    }
    Ok(characters)
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
/// an RPCS3 game hears it only while its window is in front.
pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
        tell(app, false);
        crate::session::quiet_game(app);
        app.state::<Session>().focus_game();
    }
}

/// Puts the menu back in front after the emulator's own windows took it for
/// a figure, so the game stays deaf to the pad until the menu closes, and
/// Omoio may hand the game the keyboard then. With the menu closed while
/// the figure went on, the game is let hear again instead: those windows
/// may have taken the keyboard from it after the menu handed it over.
pub fn take_front(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL).filter(|_| showing(app)) {
        let _ = window.set_focus();
    } else {
        crate::session::quiet_game(app);
    }
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
/// closes the menu. Read twenty times a second, since a press lasts about a
/// tenth of one, and the choice is read again every two seconds so one made
/// while playing counts. It ends with the game and takes the menu with it.
pub fn watch(app: AppHandle, pid: u32) {
    std::thread::spawn(move || {
        let mut was = false;
        let mut wanted = button(&app);
        let mut ticks = 0u32;
        loop {
            std::thread::sleep(Duration::from_millis(50));
            ticks = ticks.wrapping_add(1);
            if ticks % 40 == 0 {
                wanted = button(&app);
            }
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
            if now && !was {
                if let Some(pad) = pressing {
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
}
