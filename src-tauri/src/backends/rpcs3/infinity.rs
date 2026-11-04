//! Figures on RPCS3's Infinity Base, put there from outside RPCS3.
//!
//! RPCS3 always emulates one Infinity Base (`get_num_emu_devices`,
//! Emu/Io/Infinity.cpp lines 380 to 383) and fills it only from its
//! Infinity Manager, which opens from the main window's Manage menu, then
//! Portals and Gates, then Infinity Base (rpcs3qt/main_window.ui lines 298,
//! 330 to 337 and 1327 to 1330; main_window.cpp lines 3211 to 3215). Its
//! portal server knows Skylanders only (Emu/Io/SkylanderPortalIPC.cpp lines
//! 151 to 182). So a Disney Infinity game starts with RPCS3's main window,
//! hidden, as a Skylanders game does (launch.rs), and the manager is opened
//! once behind it, kept out of sight for the rest of the game and worked by
//! UI Automation the way portal.rs works the Skylanders Manager, which RPCS3
//! builds the same way: a group box of rows, each a name box and the
//! buttons Clear, Create and Load (infinity_dialog.cpp lines 715 to 823;
//! skylander_dialog.cpp lines 650 to 703). All RPCS3/rpcs3 b2e52d6, read 4
//! November 2026.
//!
//! Omoio reads nothing out of a figure's file. RPCS3 names a figure in the
//! manager once it has put it on the base (lines 877 to 889), and its
//! figure maker's list gives each figure's number and series.

use super::portal::{
    dismiss_message, free_turn, give_back_to_game, is_message, main_window, put_away, turn, END_WAIT, FILE_BUTTON,
    FILE_NAME, START_WAIT, STEP_WAIT,
};
use crate::backends::qt::{
    self, arrives, gone, hide, key, name, out_of_sight, showing, soon, title, value, visible, Automation as Qt, WAIT,
};
use crate::core::infinity::PLACES;
use crate::core::toys::ToyFigure;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::IUIAutomationElement;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_RETURN};

/// RPCS3's windows for the base, as `rpcs3qt/infinity_dialog.cpp` titles
/// them (lines 718, 409, 850 and 582).
const MANAGER: &str = "Infinity Manager";
const CREATOR: &str = "Figure Creator";
const OPEN_FIGURE: &str = "Select Infinity File";
const SAVE_FIGURE: &str = "Create Figure File";

/// The windows above, which portal.rs never takes for one of RPCS3's
/// messages.
pub(super) const WINDOWS: [&str; 4] = [MANAGER, CREATOR, OPEN_FIGURE, SAVE_FIGURE];

/// What a try that gave up may leave open, a file window before the figure
/// maker it opened over.
const LEFTOVERS: [&str; 3] = [OPEN_FIGURE, SAVE_FIGURE, CREATOR];

/// The way to the manager through the main window's menus.
const MENU: [&str; 3] = ["Manage", "Portals and Gates", "Infinity Base"];

/// Qt makes each widget's UI Automation id from the object names of the
/// widgets it sits in: the manager is `infinity_manager` and its figure
/// maker `figure_creator` (infinity_dialog.cpp lines 719 and 410), built as
/// the Skylanders windows whose ids were proven (docs/what-we-verified.md,
/// "Driving RPCS3's portal from outside works"). The manager's nine name
/// boxes and 27 buttons are its group box's own, in place order (lines 733
/// to 757 and 772 to 823). The maker's two boxes are Figure Number then
/// Series (lines 506 to 516); its list's typing box sits inside the list,
/// so it has an id of its own.
const PLACE_NAME: &str = "gui_application.infinity_manager.QGroupBox.QLineEdit";
const PLACE_BUTTON: &str = "gui_application.infinity_manager.QGroupBox.QPushButton";
const MAKER_BOX: &str = "gui_application.figure_creator.QLineEdit";
const MAKER_BUTTON: &str = "gui_application.figure_creator.QPushButton";
const MAKER_LIST: &str = "gui_application.figure_creator.QComboBox";
const MAKER_TYPING: &str = "gui_application.figure_creator.QComboBox.QLineEdit";

/// What a name box says when its place holds nothing, and for a figure
/// RPCS3's list doesn't have (infinity_dialog.cpp lines 795, 800, 833 and
/// 887).
const NONE: &str = "None";
const UNKNOWN: &str = "Unknown Figure";

/// The places whose figure maker between them offers every figure once: 0
/// the play sets and hexagonal discs, 3 the characters, 4 the round discs.
/// The maker offers only what fits the place it was opened from
/// (infinity_dialog.cpp lines 456 to 462).
const MAKER_PLACES: [usize; 3] = [0, 3, 4];

/// The figure numbers RPCS3's maker can write: three bytes, the first of
/// them not zero, or it makes an empty file and says it failed
/// (`create_blank_figure`, infinity_dialog.cpp lines 603, 638 to 640 and
/// 668 to 669).
const MAKES: std::ops::RangeInclusive<u32> = 0x01_0000..=0xFF_FFFF;

/// Where the maker's list is kept, with the RPCS3 it was read from, in
/// Omoio's data folder. Not named like the Skylanders lists
/// (`characters-<console>.json`), which the portal menu reads together.
const KEPT: &str = "infinity-ps3.json";

const UNREACHABLE: &str = "Couldn't reach RPCS3's windows.";
const LOOKS_DIFFERENT: &str = "RPCS3's Infinity Base looks different from what Omoio knows.";
const NOT_READY: &str = "RPCS3's Infinity Base isn't ready yet. Try again in a moment.";
const DIDNT_OPEN: &str = "RPCS3's Infinity Base didn't open. Try again.";
const NO_PLACE: &str = "The base has no place there.";
const LOAD_FAILED: &str = "RPCS3 couldn't put that figure on the base. It may be on it already.";
const MAKE_FAILED: &str = "RPCS3 couldn't make that figure. Try again.";
const CANT_MAKE: &str = "RPCS3's figure maker can't make that figure.";
const MAKER_DIDNT_OPEN: &str = "RPCS3's figure maker didn't open. Try again.";
const NO_LIST: &str = "Couldn't read RPCS3's list of figures.";

/// The places Omoio has put a figure on, in the RPCS3 started as the pid
/// kept here. RPCS3 keeps each place's figure number in a list that
/// outlives its manager, and a place it clears gets the number 0 rather
/// than none, so a manager opened again says "Unknown Figure" for every
/// place that was cleared (`figure_slots`, infinity_dialog.cpp lines 20,
/// 786 to 801 and 825 to 835). Omoio keeps the manager open for the whole
/// game, but if it ever opens it again, only a place it filled holds a
/// figure.
static FILLED: Mutex<(u32, [bool; PLACES])> = Mutex::new((0, [false; PLACES]));

fn filled(pid: u32) -> [bool; PLACES] {
    let kept = FILLED.lock().unwrap_or_else(PoisonError::into_inner);
    if kept.0 == pid {
        kept.1
    } else {
        [false; PLACES]
    }
}

fn fill(pid: u32, place: usize, on: bool) {
    let mut kept = FILLED.lock().unwrap_or_else(PoisonError::into_inner);
    if kept.0 != pid {
        *kept = (pid, [false; PLACES]);
    }
    kept.1[place] = on;
}

fn automation() -> Result<Qt, String> {
    Qt::new().ok_or_else(|| UNREACHABLE.to_string())
}

fn check(place: usize) -> Result<(), String> {
    if place < PLACES {
        Ok(())
    } else {
        Err(NO_PLACE.to_string())
    }
}

/// Gets the base ready while a Disney Infinity game starts: RPCS3's main
/// window is hidden the moment it shows, and the manager is opened behind
/// it, so the first figure goes on as quickly as any other.
pub fn prepare(pid: u32) {
    std::thread::spawn(move || {
        let Some(main) = soon(START_WAIT, || main_window(pid).filter(|&window| visible(window))) else {
            return;
        };
        hide(main);
        let _turn = turn();
        let Ok(automation) = automation() else {
            return;
        };
        if let Ok(window) = manager(&automation, pid) {
            warm_up(&automation, pid, window);
        }
    });
}

/// Opens the manager as the base menu opens, if the game's start didn't,
/// so the menu's first errand doesn't wait for it. Nothing when an errand
/// already has the turn, which opens it anyway.
pub fn ready(pid: u32) {
    let Some(_turn) = free_turn() else {
        return;
    };
    if showing(pid).into_iter().any(|window| title(window) == MANAGER) {
        return;
    }
    if let Ok(automation) = automation() {
        let _ = manager(&automation, pid);
    }
}

/// The manager, opened through the main window's menus if it isn't open,
/// and kept out of sight.
fn manager(automation: &Qt, pid: u32) -> Result<HWND, String> {
    if let Some(window) = showing(pid).into_iter().find(|&w| title(w) == MANAGER) {
        out_of_sight(window);
        return Ok(window);
    }
    let main = main_window(pid).ok_or(NOT_READY)?;
    hide(main);
    let menus = automation.window(main).ok_or(LOOKS_DIFFERENT)?;
    if !qt::choose_from_menus(automation, pid, &menus, &MENU) {
        return Err(LOOKS_DIFFERENT.to_string());
    }
    arrives(pid, MANAGER).ok_or_else(|| DIDNT_OPEN.to_string())
}

/// Opens one of Qt's file windows and closes it again, unseen, while the
/// game starts, as portal.rs does for the Skylanders Manager: Qt sets its
/// file windows up the first time one opens. Pressed with UI Automation's
/// own press, which leaves the keyboard alone. Cancel in Load's file window
/// leaves the place as it was (infinity_dialog.cpp lines 847 to 859).
fn warm_up(automation: &Qt, pid: u32, window: HWND) {
    let Some(manager) = automation.window(window) else {
        return;
    };
    if place_button(automation, &manager, "Load", 0).ok().and_then(|load| qt::invoke(&load)).is_none() {
        return;
    }
    let Some(picker) = arrives(pid, OPEN_FIGURE) else {
        return;
    };
    let cancel = automation
        .window(picker)
        .and_then(|dialog| automation.all(&dialog, FILE_BUTTON).into_iter().find(|button| name(button) == "Cancel"));
    if cancel.is_some_and(|cancel| qt::invoke(&cancel).is_some()) {
        gone(picker);
    }
    give_back_to_game(pid);
}

fn place_button(automation: &Qt, manager: &IUIAutomationElement, label: &str, place: usize) -> Result<IUIAutomationElement, String> {
    automation
        .all(manager, PLACE_BUTTON)
        .into_iter()
        .filter(|button| name(button) == label)
        .nth(place)
        .ok_or_else(|| LOOKS_DIFFERENT.to_string())
}

/// Presses a button as the keyboard does (qt.rs, `press`), which Qt answers
/// at once, where UI Automation's own press lands a tenth of a second late.
fn press(button: &IUIAutomationElement, window: HWND) -> Result<(), String> {
    qt::press(button, window).ok_or_else(|| LOOKS_DIFFERENT.to_string())
}

/// What each name box says, in place order, as RPCS3 put it.
fn boxes(automation: &Qt, window: HWND) -> Result<Vec<String>, String> {
    let manager = automation.window(window).ok_or(LOOKS_DIFFERENT)?;
    let said: Vec<String> = automation.all(&manager, PLACE_NAME).iter().map(value).collect();
    if said.len() == PLACES {
        Ok(said)
    } else {
        Err(LOOKS_DIFFERENT.to_string())
    }
}

/// Each place's figure as the menu knows it, empty where there is none.
fn named(pid: u32, said: &[String]) -> Vec<String> {
    said.iter().zip(filled(pid)).map(|(said, filled)| shown(said, filled)).collect()
}

/// A place's name: empty for none, and for RPCS3's "Unknown Figure" in a
/// place Omoio never filled, which is a place cleared before the manager
/// was opened again (`FILLED`). In a place Omoio filled it is a figure
/// RPCS3's list doesn't have, and stays as RPCS3 says it.
fn shown(said: &str, filled: bool) -> String {
    if said == NONE || (said == UNKNOWN && !filled) {
        String::new()
    } else {
        said.to_string()
    }
}

/// What the base holds once RPCS3 has put a figure on `place`, or `failed`
/// when it put up a message instead. RPCS3's own words name the file's
/// whole path, so they stay behind. The read comes first: RPCS3 answers it
/// only once it has finished with the figure, as it does for the Skylanders
/// Manager (portal.rs, `settled`).
fn settled(automation: &Qt, pid: u32, window: HWND, place: usize, failed: &str) -> Result<Vec<String>, String> {
    let said = boxes(automation, window)?;
    if dismiss_message(pid) {
        return Err(failed.to_string());
    }
    fill(pid, place, true);
    Ok(named(pid, &said))
}

/// Puts a figure's file in one of Qt's file windows and presses Enter, which
/// presses its Open or Save button, then waits for it to close.
fn choose_file(automation: &Qt, picker: HWND, file: &Path) -> Result<(), String> {
    let window = automation.window(picker).ok_or(LOOKS_DIFFERENT)?;
    let name_box = automation.first(&window, FILE_NAME).ok_or(LOOKS_DIFFERENT)?;
    qt::set_value(&name_box, &file.to_string_lossy()).ok_or(LOOKS_DIFFERENT)?;
    key(picker, VK_RETURN);
    if gone(picker) {
        Ok(())
    } else {
        key(picker, VK_ESCAPE);
        Err("RPCS3 didn't take the figure's file. Try again.".to_string())
    }
}

/// The figures on the base, by place.
pub fn places(pid: u32) -> Result<Vec<String>, String> {
    let _turn = turn();
    let automation = automation()?;
    let window = manager(&automation, pid)?;
    Ok(named(pid, &boxes(&automation, window)?))
}

/// Puts the figure in `file` on the base in `place`, counted from 0 in the
/// emulators' order (core/infinity.rs), and returns what the base holds
/// afterwards. RPCS3 takes off what the place held first, and only once it
/// has read the new file (`load_figure_path`, infinity_dialog.cpp lines 861
/// to 890), so a file it can't open leaves the place as it was. It holds a
/// figure's file open against writers while it is on the base (`fs::lock`,
/// line 863), so a file already on the base is turned down.
pub fn load(pid: u32, place: usize, file: &Path) -> Result<Vec<String>, String> {
    check(place)?;
    let _turn = turn();
    let automation = automation()?;
    let window = manager(&automation, pid)?;
    put_away(pid, &LEFTOVERS);
    let buttons = automation.window(window).ok_or(LOOKS_DIFFERENT)?;
    press(&place_button(&automation, &buttons, "Load", place)?, window)?;
    let picker = arrives(pid, OPEN_FIGURE).ok_or("RPCS3 didn't ask for the figure. Try again.")?;
    choose_file(&automation, picker, file)?;
    settled(&automation, pid, window, place, LOAD_FAILED)
}

/// Takes the figure in `place` off the base, and returns what is left.
/// RPCS3's Clear saves the figure to its file as it goes (`remove_figure`,
/// Emu/Io/Infinity.cpp lines 273 to 298).
pub fn remove(pid: u32, place: usize) -> Result<Vec<String>, String> {
    check(place)?;
    let _turn = turn();
    let automation = automation()?;
    let window = manager(&automation, pid)?;
    let manager = automation.window(window).ok_or(LOOKS_DIFFERENT)?;
    let held = automation.all(&manager, PLACE_NAME).into_iter().nth(place).ok_or(LOOKS_DIFFERENT)?;
    if value(&held) != NONE {
        press(&place_button(&automation, &manager, "Clear", place)?, window)?;
        // The press is posted, and RPCS3 may answer a read before it gets to it.
        let _ = soon(WAIT, || (value(&held) == NONE).then_some(()));
    }
    let said = boxes(&automation, window)?;
    if said[place] == NONE {
        fill(pid, place, false);
    }
    Ok(named(pid, &said))
}

/// Has RPCS3's figure maker make `figure` into `file`, a file that isn't
/// there yet. RPCS3 then puts it on the base in `place` itself
/// (`create_figure`, infinity_dialog.cpp lines 837 to 845). Returns what
/// the base holds afterwards.
pub fn create(pid: u32, place: usize, figure: &ToyFigure, file: &Path) -> Result<Vec<String>, String> {
    check(place)?;
    // The maker takes a series of 1 to 3 (line 513, and Create's check at
    // lines 566 to 571).
    let series = figure.series.filter(|series| (1..=3).contains(series)).ok_or(CANT_MAKE)?;
    if !MAKES.contains(&figure.number) {
        return Err(CANT_MAKE.to_string());
    }
    // Qt's save window would ask before replacing it.
    if file.exists() {
        return Err("There's a figure file of that name already. Try again.".to_string());
    }
    // RPCS3 makes the file but not the folder it goes in (line 603).
    if let Some(folder) = file.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let _turn = turn();
    let automation = automation()?;
    let window = manager(&automation, pid)?;
    put_away(pid, &LEFTOVERS);
    let buttons = automation.window(window).ok_or(LOOKS_DIFFERENT)?;
    press(&place_button(&automation, &buttons, "Create", place)?, window)?;
    let maker = arrives(pid, CREATOR).ok_or(MAKER_DIDNT_OPEN)?;
    if let Err(problem) = make(&automation, pid, maker, figure.number, series, file) {
        put_away(pid, &LEFTOVERS);
        return Err(problem);
    }
    // Unlike the Skylanders Manager, this one names the new figure as it
    // puts it on (lines 880 to 888), so it can be read at once.
    settled(&automation, pid, window, place, MAKE_FAILED)
}

/// Fills in the figure maker and has it save the figure. Its Figure Number
/// and Series boxes are what it makes the figure from; its list only fills
/// them in (infinity_dialog.cpp lines 544 to 595).
fn make(automation: &Qt, pid: u32, maker: HWND, number: u32, series: u8, file: &Path) -> Result<(), String> {
    let window = automation.window(maker).ok_or(LOOKS_DIFFERENT)?;
    let boxes = soon(WAIT, || Some(automation.all(&window, MAKER_BOX)).filter(|found| found.len() == 2))
        .ok_or(LOOKS_DIFFERENT)?;
    qt::set_value(&boxes[0], &number.to_string()).ok_or(LOOKS_DIFFERENT)?;
    qt::set_value(&boxes[1], &series.to_string()).ok_or(LOOKS_DIFFERENT)?;
    // Among the series filters All, 1.0, 2.0 and 3.0, and Cancel (lines 418
    // to 421 and 523 to 524).
    let make = automation
        .all(&window, MAKER_BUTTON)
        .into_iter()
        .find(|button| name(button) == "Create")
        .ok_or(LOOKS_DIFFERENT)?;
    press(&make, maker)?;
    let saver = arrives(pid, SAVE_FIGURE).ok_or("RPCS3 didn't ask where to keep the figure. Try again.")?;
    choose_file(automation, saver, file)?;
    // RPCS3 writes the figure and closes the maker, or says why it couldn't
    // and leaves the maker open.
    let done = soon(WAIT, || {
        if !visible(maker) {
            Some(true)
        } else {
            showing(pid).into_iter().any(|window| is_message(pid, window)).then_some(false)
        }
    });
    if done == Some(true) {
        Ok(())
    } else {
        Err(MAKE_FAILED.to_string())
    }
}

/// The list of figures RPCS3 makes, kept with the RPCS3 it came from.
#[derive(Serialize, Deserialize)]
struct Kept {
    version: String,
    figures: Vec<ToyFigure>,
}

/// Every figure RPCS3's maker makes, with its number and series. Read from
/// the maker once for each version of RPCS3 and kept in Omoio's data
/// folder, as the portal menu keeps RPCS3's Skylanders (portal_menu.rs,
/// `characters`), since reading it takes a few seconds.
pub fn list(app: &AppHandle, pid: u32) -> Result<Vec<ToyFigure>, String> {
    let version = super::detect_version(app).unwrap_or_default();
    let file = app.path().data_dir().ok().map(|data| data.join("Omoio").join(KEPT));
    kept_or_read(file.as_deref(), &version, || from_maker(pid))
}

/// The list kept for `version`, else the one `read` gives, kept in its
/// place. When it can't be read, a list kept from another RPCS3 names the
/// same figures, if maybe not its newest, so the menu offers those.
fn kept_or_read(
    file: Option<&Path>,
    version: &str,
    read: impl FnOnce() -> Result<Vec<ToyFigure>, String>,
) -> Result<Vec<ToyFigure>, String> {
    let kept = file
        .and_then(|file| std::fs::read_to_string(file).ok())
        .and_then(|text| serde_json::from_str::<Kept>(&text).ok())
        .filter(|kept| !kept.figures.is_empty());
    match kept {
        Some(kept) if kept.version == version => Ok(kept.figures),
        kept => match read() {
            Ok(figures) => {
                let fresh = Kept { version: version.to_string(), figures };
                if let (Some(file), Ok(text)) = (file, serde_json::to_string(&fresh)) {
                    let _ = std::fs::write(file, text);
                }
                Ok(fresh.figures)
            }
            Err(why) => kept.map(|kept| kept.figures).ok_or(why),
        },
    }
}

/// The figure maker's list, read from the makers of `MAKER_PLACES`. Each is
/// closed again without making anything.
fn from_maker(pid: u32) -> Result<Vec<ToyFigure>, String> {
    let _turn = turn();
    let automation = automation()?;
    let window = manager(&automation, pid)?;
    put_away(pid, &LEFTOVERS);
    let mut found = Vec::new();
    for place in MAKER_PLACES {
        let buttons = automation.window(window).ok_or(LOOKS_DIFFERENT)?;
        press(&place_button(&automation, &buttons, "Create", place)?, window)?;
        let maker = arrives(pid, CREATOR).ok_or(MAKER_DIDNT_OPEN)?;
        let steps = read_maker(&automation, maker);
        key(maker, VK_ESCAPE);
        if !gone(maker) {
            put_away(pid, &LEFTOVERS);
            return Err(NO_LIST.to_string());
        }
        for step in steps? {
            keep(&mut found, &step);
        }
    }
    if found.is_empty() {
        Err(NO_LIST.to_string())
    } else {
        Ok(found)
    }
}

/// Each step through the maker's list as (the list's text, Figure Number,
/// Series). Each step fills both boxes for the figure it lands on (lines
/// 544 to 555). The list is sorted by name, with "--Unknown--" among them,
/// and its boxes open on the place's first figure by number, in series 1
/// (lines 441 to 498 and 509 to 511).
fn read_maker(automation: &Qt, maker: HWND) -> Result<Vec<(String, String, String)>, String> {
    let window = automation.window(maker).ok_or(LOOKS_DIFFERENT)?;
    let list = automation.first(&window, MAKER_LIST).ok_or(LOOKS_DIFFERENT)?;
    let boxes = soon(WAIT, || Some(automation.all(&window, MAKER_BOX)).filter(|found| found.len() == 2))
        .ok_or(LOOKS_DIFFERENT)?;
    let typing = automation.first(&window, MAKER_TYPING).ok_or(LOOKS_DIFFERENT)?;
    let read = || (value(&list), value(&boxes[0]), value(&boxes[1]));
    qt::step_through(maker, &typing, STEP_WAIT, END_WAIT, read).ok_or_else(|| LOOKS_DIFFERENT.to_string())
}

/// Adds what one step showed, once for each number. "--Unknown--" leaves
/// the boxes as the figure before it had them (lines 477 and 547), so it is
/// left out.
fn keep(found: &mut Vec<ToyFigure>, (text, number, series): &(String, String, String)) {
    let (Ok(number), Ok(series)) = (number.parse::<u32>(), series.parse::<u8>()) else {
        return;
    };
    if text.is_empty() || text.starts_with("--") || !(1..=3).contains(&series) {
        return;
    }
    if found.iter().any(|known| known.number == number) {
        return;
    }
    found.push(ToyFigure { name: without_series(text).to_string(), number, series: Some(series) });
}

/// A figure's name as the manager shows it, without the " (2.0)" the
/// maker's list puts after it (lines 466 to 467; RPCS3 pull request 17529,
/// 28 September 2025). A name of its own in brackets, such as "Hulk
/// (Crystal)", stays.
fn without_series(text: &str) -> &str {
    match text.rsplit_once(" (") {
        Some((name, tail)) if tail.len() == 4 && tail.ends_with(".0)") && tail.as_bytes()[0].is_ascii_digit() => name,
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(text: &str, number: &str, series: &str) -> (String, String, String) {
        (text.to_string(), number.to_string(), series.to_string())
    }

    fn figure(name: &str, number: u32, series: u8) -> ToyFigure {
        ToyFigure { name: name.to_string(), number, series: Some(series) }
    }

    #[test]
    fn a_place_reads_as_the_menu_names_it() {
        assert_eq!(shown("None", false), "");
        assert_eq!(shown("None", true), "");
        assert_eq!(shown("Mr. Incredible", true), "Mr. Incredible");
        assert_eq!(shown("Mr. Incredible", false), "Mr. Incredible");
        // A place cleared before the manager was opened again.
        assert_eq!(shown("Unknown Figure", false), "");
        // A figure Omoio put on that RPCS3's list doesn't have.
        assert_eq!(shown("Unknown Figure", true), "Unknown Figure");
    }

    #[test]
    fn only_a_place_omoio_filled_in_this_rpcs3_holds_an_unknown_figure() {
        let pid = u32::MAX - std::process::id();
        let said: Vec<String> = ["Unknown Figure", "None", "Stitch", "Unknown Figure", "", "", "", "", ""]
            .map(String::from)
            .to_vec();
        fill(pid, 3, true);
        assert_eq!(named(pid, &said)[..4], ["", "", "Stitch", "Unknown Figure"]);
        fill(pid, 3, false);
        assert_eq!(named(pid, &said)[3], "");
        // Another RPCS3 starts with nothing filled.
        fill(pid, 0, true);
        assert_eq!(named(pid - 1, &said)[0], "");
    }

    #[test]
    fn the_series_comes_off_a_name_and_the_names_own_brackets_stay() {
        assert_eq!(without_series("Mr. Incredible (1.0)"), "Mr. Incredible");
        assert_eq!(without_series("Mr. Incredible (Crystal) (1.0)"), "Mr. Incredible (Crystal)");
        assert_eq!(without_series("Anakin Skywalker (Light FX) (3.0)"), "Anakin Skywalker (Light FX)");
        // As an RPCS3 from before 28 September 2025 lists it.
        assert_eq!(without_series("Mr. Incredible (Crystal)"), "Mr. Incredible (Crystal)");
        assert_eq!(without_series("Stitch"), "Stitch");
    }

    #[test]
    fn each_figure_is_kept_once_and_the_unknown_line_not_at_all() {
        let mut found = Vec::new();
        // Lines 32, 54 and 128 of RPCS3's list, as the maker shows them.
        keep(&mut found, &step("Mr. Incredible (1.0)", "1000001", "1"));
        keep(&mut found, &step("Mr. Incredible (Crystal) (1.0)", "1000030", "1"));
        keep(&mut found, &step("--Unknown--", "1000030", "1"));
        keep(&mut found, &step("Mr. Incredible (1.0)", "1000001", "1"));
        keep(&mut found, &step("Stitch (2.0)", "1000118", "2"));
        keep(&mut found, &step("", "1000200", "3"));
        keep(&mut found, &step("Nobody (4.0)", "1000999", "4"));
        keep(&mut found, &step("Nobody", "", "1"));
        assert_eq!(
            found,
            [figure("Mr. Incredible", 1000001, 1), figure("Mr. Incredible (Crystal)", 1000030, 1), figure("Stitch", 1000118, 2)]
        );
    }

    #[test]
    fn the_list_kept_for_this_rpcs3_is_used_and_another_read_again() {
        let path = std::env::temp_dir().join(format!("omoio-infinity-ps3-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let file = Some(path.as_path());
        let old = [figure("Mr. Incredible", 1000001, 1)].to_vec();
        let new = [figure("Mr. Incredible", 1000001, 1), figure("Stitch", 1000118, 2)].to_vec();

        // Nothing kept: read, and kept.
        let read = kept_or_read(file, "0.0.43-20400", || Ok(old.clone()));
        assert_eq!(read, Ok(old.clone()));
        // Kept for this RPCS3: not read again.
        let kept = kept_or_read(file, "0.0.43-20400", || Err("read".to_string()));
        assert_eq!(kept, Ok(old.clone()));
        // A newer RPCS3 that can't be read yet: the list kept before.
        let fallback = kept_or_read(file, "0.0.43-20500", || Err("busy".to_string()));
        assert_eq!(fallback, Ok(old.clone()));
        // A newer RPCS3: read again, and kept in its place.
        assert_eq!(kept_or_read(file, "0.0.43-20500", || Ok(new.clone())), Ok(new.clone()));
        assert_eq!(kept_or_read(file, "0.0.43-20500", || Err("read".to_string())), Ok(new.clone()));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(r#""version":"0.0.43-20500""#), "{text}");
        assert!(text.contains(r#"{"name":"Stitch","number":1000118,"series":2}"#), "{text}");

        // An empty list is never kept as the answer.
        std::fs::write(&path, r#"{"version":"0.0.43-20500","figures":[]}"#).unwrap();
        assert_eq!(kept_or_read(file, "0.0.43-20500", || Err("busy".to_string())), Err("busy".to_string()));
        let _ = std::fs::remove_file(&path);

        // No data folder: read each time.
        assert_eq!(kept_or_read(None, "0.0.43-20500", || Ok(new.clone())), Ok(new));
    }

    #[test]
    fn only_a_number_and_series_the_maker_can_write_are_made() {
        assert!(!MAKES.contains(&0xFFFF));
        assert!(MAKES.contains(&1_000_001));
        assert!(MAKES.contains(&4_000_229));
        assert!(!MAKES.contains(&0x0100_0000));
        let file = Path::new("C:\\nowhere\\Stitch.bin");
        let without = ToyFigure { name: "Stitch".to_string(), number: 1_000_118, series: None };
        assert_eq!(create(0, 3, &without, file), Err(CANT_MAKE.to_string()));
        let fourth = ToyFigure { series: Some(4), ..without.clone() };
        assert_eq!(create(0, 3, &fourth, file), Err(CANT_MAKE.to_string()));
        let short = ToyFigure { number: 0xFFFF, series: Some(2), ..without.clone() };
        assert_eq!(create(0, 3, &short, file), Err(CANT_MAKE.to_string()));
        assert_eq!(create(0, PLACES, &figure("Stitch", 1_000_118, 2), file), Err(NO_PLACE.to_string()));
    }

    /// Needs an RPCS3 running a Disney Infinity game with its main window
    /// (started by Omoio, or without `--no-gui`), a figure file and a folder
    /// to make one in, so it runs only by hand:
    /// `OMOIO_RPCS3_PID=<pid> OMOIO_INFINITY_FIGURE=<file> cargo test infinity_base_on_a_running_rpcs3 -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn infinity_base_on_a_running_rpcs3() {
        use std::time::Instant;
        let pid: u32 = std::env::var("OMOIO_RPCS3_PID").unwrap().parse().unwrap();
        let figure_file = std::env::var("OMOIO_INFINITY_FIGURE").unwrap();
        let before = places(pid).unwrap();
        assert_eq!(before.len(), PLACES);
        let started = Instant::now();
        let loaded = load(pid, 3, Path::new(&figure_file)).unwrap();
        println!("load took {:?}: {loaded:?}", started.elapsed());
        assert!(!loaded[3].is_empty(), "Player One holds the figure: {loaded:?}");
        let started = Instant::now();
        let cleared = remove(pid, 3).unwrap();
        println!("remove took {:?}", started.elapsed());
        assert!(cleared[3].is_empty(), "Player One is empty again: {cleared:?}");
        let started = Instant::now();
        let listed = from_maker(pid).unwrap();
        println!("{} figures in {:?}", listed.len(), started.elapsed());
        assert!(listed.contains(&figure("Mr. Incredible", 1_000_001, 1)), "{listed:?}");
        assert!(listed.iter().any(|f| f.number == 2_000_001), "a play set");
        assert!(listed.iter().any(|f| (3_000_001..4_000_000).contains(&f.number)), "a round disc");
        assert!(listed.iter().any(|f| (4_000_001..5_000_000).contains(&f.number)), "a hexagonal disc");
        // Beside the figure, in a small folder like Omoio's own: Qt's save
        // window reads the folder it opens in.
        let made = Path::new(&figure_file).with_file_name(format!("omoio-made-{}.bin", std::process::id()));
        let started = Instant::now();
        let after = create(pid, 3, &figure("Mr. Incredible", 1_000_001, 1), &made).unwrap();
        println!("create took {:?}: {after:?}", started.elapsed());
        assert_eq!(after[3], "Mr. Incredible");
        assert_eq!(std::fs::metadata(&made).unwrap().len(), 320);
        remove(pid, 3).unwrap();
        let _ = std::fs::remove_file(made);
    }
}
