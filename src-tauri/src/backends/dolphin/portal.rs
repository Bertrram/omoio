//! Figures on Dolphin's Skylanders portal, put there from outside Dolphin.
//!
//! Dolphin keeps its portal in memory and fills it only from its Skylanders
//! Manager, a window of its own that opens from the main window's Tools
//! menu, under Emulated USB Devices (`MenuBar.cpp`, Dolphin 2609a). Started
//! with `--batch`, Dolphin never shows its main window and so never that
//! menu, so a Skylanders game starts with the main window, which is hidden
//! the moment it shows, and the manager is opened once behind it and kept
//! out of sight for the rest of the game, as RPCS3's is.
//!
//! The manager (`DolphinQt/SkylanderPortal/SkylanderPortalWindow.cpp`) has
//! sixteen slots, each a radio button and a box with the name of the figure
//! in it ("None" when empty), and buttons below that act on the slot whose
//! radio button is on: Load File asks for a figure's file ("Select Skylander
//! File"), Clear Slot takes the figure off, and Customize opens a small
//! window with ID and Variant boxes whose Create makes a new figure from
//! those two numbers alone, asks where to keep it ("Create Skylander File")
//! and puts it in the slot. Dolphin is Qt, like RPCS3, and is worked the
//! same way (backends/qt.rs).

use crate::backends::qt::{
    self, all_windows, arrives, class, gone, hide, key, name, open_menus, out_of_sight, owned_by, process_of,
    showing, soon, title, value, visible, Automation as Qt, WAIT,
};
use crate::core::figures::{self, Character};
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, UIA_ButtonControlTypeId, UIA_EditControlTypeId, UIA_RadioButtonControlTypeId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_ESCAPE, VK_LMENU,
    VK_RETURN,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsWindow, PostMessageW, SetForegroundWindow, WM_CLOSE};

/// How many figures Dolphin's portal holds (`MAX_SKYLANDERS` in
/// Core/IOS/USB/Emulated/Skylanders/Skylander.h).
pub const SLOTS: usize = 16;

/// What Qt's Windows platform is told, so Dolphin opens Qt's own file
/// windows, which answer UI Automation like the rest of it, rather than
/// Windows' own. Dolphin asks for its files through Qt's plain
/// `QFileDialog` calls (`QtUtils/DolphinFileDialog.cpp`).
pub const QT_PLATFORM: &str = "windows:dialogs=none";

/// Dolphin's windows, as `SkylanderPortalWindow.cpp` titles them.
const MANAGER: &str = "Skylanders Manager";
const OPEN_FIGURE: &str = "Select Skylander File";
const SAVE_FIGURE: &str = "Create Skylander File";

/// The way to the manager through the main window's menus (`MenuBar.cpp`),
/// as UI Automation names them, without the `&` that marks a shortcut.
const MENU: [&str; 3] = ["Tools", "Emulated USB Devices", "Skylanders Portal"];

/// The manager's buttons, by their words (`CreateMainWindow`).
const LOAD: &str = "Load File";
const CLEAR: &str = "Clear Slot";
const CUSTOMIZE: &str = "Customize";
/// The Customize window's button that makes the figure, which Dolphin
/// renames from OK.
const CREATE: &str = "Create";

/// Qt makes each widget's UI Automation id from where it sits; a file
/// window's name box and buttons end so.
const FILE_NAME: &str = "QFileDialog.fileNameEdit";

const LOOKS_DIFFERENT: &str = "Dolphin's portal looks different from what Omoio knows.";
const NOT_READY: &str = "Dolphin's portal isn't ready yet. Try again in a moment.";
const LOAD_FAILED: &str = "Dolphin couldn't put that figure on the portal. It may be on it already.";
const MAKE_FAILED: &str = "Dolphin couldn't make that figure. Try again.";

/// How long Dolphin may take to show its main window after it starts.
const START_WAIT: Duration = Duration::from_secs(30);

/// One errand in Dolphin's windows at a time: the manager opening while a
/// game starts and a figure the user picked meanwhile would each open
/// windows the other is waiting for.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn turn() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner)
}

/// UI Automation, with Dolphin's words for what goes wrong in its windows.
struct Automation(Qt);

impl Automation {
    fn new() -> Result<Self, String> {
        Qt::new().map(Self).ok_or_else(|| "Couldn't reach Dolphin's windows.".to_string())
    }

    fn window(&self, window: HWND) -> Result<IUIAutomationElement, String> {
        self.0.window(window).ok_or_else(|| LOOKS_DIFFERENT.to_string())
    }

    /// A button by its words, waited for.
    fn button(&self, within: &IUIAutomationElement, words: &str) -> Result<IUIAutomationElement, String> {
        soon(WAIT, || {
            self.0
                .of_kind(within, UIA_ButtonControlTypeId)
                .into_iter()
                .find(|button| name(button) == words)
        })
        .ok_or_else(|| LOOKS_DIFFERENT.to_string())
    }
}

/// The main window's title is Dolphin's name and version, "Dolphin 2609a"
/// (`GetScmRevStr`, Common/Version.cpp). The game's window starts as
/// "Dolphin" and becomes the same name and version followed by what runs
/// it, each part after " | ", with the game's name last (`UpdateTitle` in
/// Core/Core.cpp, `RenderWidget`'s constructor).
pub fn is_game_title(title: &str) -> bool {
    title == "Dolphin" || (title.starts_with("Dolphin ") && title.contains(" | "))
}

fn is_main_title(title: &str) -> bool {
    title.starts_with("Dolphin ") && !title.contains(" | ")
}

fn main_window(pid: u32) -> Option<HWND> {
    all_windows(pid)
        .into_iter()
        .find(|&window| !owned_by(pid, window) && is_main_title(&title(window)))
}

/// A window Dolphin put up by itself, such as an error about a figure's
/// file: one of its dialogs other than those Omoio opens. Qt's menus are
/// windows of their own kind.
fn is_message(pid: u32, window: HWND) -> bool {
    let title = title(window);
    owned_by(pid, window)
        && !title.is_empty()
        && !class(window).contains("Popup")
        && ![MANAGER, OPEN_FIGURE, SAVE_FIGURE].contains(&title.as_str())
}

/// Hands the keyboard back to the game's window when one of Dolphin's own
/// windows took it, as a file window does when it opens. Omoio switches
/// Dolphin's background input off, so its game hears the pad only while its
/// window is in front. Windows lets a program change which window is in
/// front only once it has had the last input, so a press of Alt, which no
/// window acts on alone, goes first, as backends/rpcs3 does.
pub fn give_back_to_game(pid: u32) {
    let front = unsafe { GetForegroundWindow() };
    // A message Dolphin put up is left in front, where it can be read.
    if process_of(front) != pid || is_message(pid, front) {
        return;
    }
    let game = showing(pid)
        .into_iter()
        .find(|&window| !owned_by(pid, window) && is_game_title(&title(window)));
    let Some(game) = game.filter(|&game| game != front) else {
        return;
    };
    if unsafe { SetForegroundWindow(game) }.as_bool() {
        return;
    }
    let alt = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_LMENU, dwFlags: flags, ..Default::default() } },
    };
    let presses = [alt(KEYBD_EVENT_FLAGS(0)), alt(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(&presses, std::mem::size_of::<INPUT>() as i32);
        let _ = SetForegroundWindow(game);
    }
}

/// Closes a message Dolphin put up, so it doesn't sit over the game, and
/// says whether there was one. Dolphin's own words name the file's whole
/// path, so Omoio says what happened in its own.
fn dismiss_message(pid: u32) -> bool {
    let Some(message) = showing(pid).into_iter().find(|&w| is_message(pid, w)) else {
        return false;
    };
    out_of_sight(message);
    key(message, VK_RETURN);
    gone(message);
    true
}

/// The Customize window: a small window of Dolphin's with no title of its
/// own, holding the ID and Variant boxes.
fn maker_window(automation: &Automation, pid: u32) -> Option<HWND> {
    showing(pid).into_iter().find(|&window| {
        title(window) != MANAGER
            && !is_game_title(&title(window))
            && !is_main_title(&title(window))
            && !class(window).contains("Popup")
            && automation
                .window(window)
                .is_ok_and(|found| automation.0.of_kind(&found, UIA_EditControlTypeId).len() == 2)
    })
}

/// Closes what a try that gave up left open: a message, a file window, the
/// Customize window. Each would stand in the way of the next errand.
fn put_away_leftovers(automation: &Automation, pid: u32) {
    for _ in 0..3 {
        if !dismiss_message(pid) {
            break;
        }
    }
    for leftover in [OPEN_FIGURE, SAVE_FIGURE] {
        for window in showing(pid).into_iter().filter(|&w| title(w) == leftover) {
            key(window, VK_ESCAPE);
            gone(window);
        }
    }
    if let Some(maker) = maker_window(automation, pid) {
        key(maker, VK_ESCAPE);
        gone(maker);
    }
}

/// The manager, opened through the main window's menus if it isn't open,
/// and kept out of sight.
fn manager(automation: &Automation, pid: u32) -> Result<HWND, String> {
    if let Some(window) = showing(pid).into_iter().find(|&w| title(w) == MANAGER) {
        out_of_sight(window);
        return Ok(window);
    }
    let main = main_window(pid).ok_or(NOT_READY)?;
    hide(main);
    let menus = automation.window(main)?;
    let opened = open_manager(automation, pid, &menus);
    if opened.is_err() {
        for menu in open_menus(pid) {
            key(menu, VK_ESCAPE);
        }
    }
    opened?;
    arrives(pid, MANAGER).ok_or_else(|| "Dolphin's portal didn't open. Try again.".to_string())
}

/// Tools, then Emulated USB Devices, then Skylanders Portal. Each menu goes
/// out of sight as soon as it opens, and they all close when the item is
/// chosen.
fn open_manager(automation: &Automation, pid: u32, menus: &IUIAutomationElement) -> Result<(), String> {
    for label in &MENU[..2] {
        let item = find_menu_item(automation, pid, menus, label).ok_or(LOOKS_DIFFERENT)?;
        qt::expand(&item).ok_or(LOOKS_DIFFERENT)?;
        for menu in open_menus(pid) {
            out_of_sight(menu);
        }
    }
    qt::invoke(&find_menu_item(automation, pid, menus, MENU[2]).ok_or(LOOKS_DIFFERENT)?).ok_or_else(|| LOOKS_DIFFERENT.to_string())
}

/// A menu item, in the main window or in one of the menus open over it.
fn find_menu_item(automation: &Automation, pid: u32, menus: &IUIAutomationElement, label: &str) -> Option<IUIAutomationElement> {
    soon(WAIT, || {
        automation.0.menu_item(menus, label).or_else(|| {
            open_menus(pid)
                .into_iter()
                .find_map(|menu| automation.window(menu).ok().and_then(|menu| automation.0.menu_item(&menu, label)))
        })
    })
}

/// Gets the portal ready while a Skylanders game starts: Dolphin's main
/// window is hidden the moment it shows, and the manager opened behind it,
/// so the first figure goes on as quickly as any other.
pub fn prepare(pid: u32) {
    std::thread::spawn(move || {
        let Some(main) = soon(START_WAIT, || main_window(pid).filter(|&window| visible(window))) else {
            return;
        };
        hide(main);
        let _turn = turn();
        if let Ok(automation) = Automation::new() {
            let _ = manager(&automation, pid);
        }
        give_back_to_game(pid);
    });
}

/// Keeps Dolphin's main window hidden while the game runs, and closes
/// Dolphin once the game's window `game` has gone: with its main window
/// there, Dolphin would otherwise carry on, out of sight, after the game.
pub fn tidy(pid: u32, game: isize) {
    let Some(main) = main_window(pid) else {
        return;
    };
    if !unsafe { IsWindow(Some(HWND(game as *mut _))) }.as_bool() {
        let _ = unsafe { PostMessageW(Some(main), WM_CLOSE, WPARAM(0), LPARAM(0)) };
    } else if visible(main) {
        hide(main);
    }
}

fn check(slot: usize) -> Result<(), String> {
    if slot < SLOTS {
        Ok(())
    } else {
        Err("The portal has no slot there.".to_string())
    }
}

/// A slot's name as the menu knows it: empty for none, and for one of the
/// figures whose variant Dolphin's list has wrong, or one no list has, the
/// figure's name where Dolphin says "Unknown (Id:212 Var:12302)"
/// (`UpdateSlotNames`).
fn shown(name: &str) -> String {
    if name == "None" || name.is_empty() {
        return String::new();
    }
    unknown(name)
        .and_then(|(id, variant)| figures::unlisted_name(id, variant))
        .unwrap_or_else(|| name.to_string())
}

fn unknown(name: &str) -> Option<(u16, u16)> {
    let (id, variant) = name.strip_prefix("Unknown (Id:")?.strip_suffix(')')?.split_once(" Var:")?;
    Some((id.parse().ok()?, variant.parse().ok()?))
}

/// The slots' name boxes and radio buttons, in order. The name boxes are the
/// manager's only boxes that take no typing; its collection path and search
/// boxes do. The slots' radio buttons are its only ones without words: those
/// that sort its list by element and kind have theirs.
fn slots(automation: &Automation, manager: &IUIAutomationElement) -> Result<(Vec<IUIAutomationElement>, Vec<IUIAutomationElement>), String> {
    let names: Vec<IUIAutomationElement> = automation
        .0
        .of_kind(manager, UIA_EditControlTypeId)
        .into_iter()
        .filter(|edit| !qt::enabled(edit))
        .collect();
    let radios: Vec<IUIAutomationElement> = automation
        .0
        .of_kind(manager, UIA_RadioButtonControlTypeId)
        .into_iter()
        .filter(|radio| name(radio).is_empty())
        .collect();
    if names.len() == SLOTS && radios.len() == SLOTS {
        Ok((names, radios))
    } else {
        Err(LOOKS_DIFFERENT.to_string())
    }
}

/// What each slot holds, empty where it holds nothing.
fn read(automation: &Automation, window: HWND) -> Result<Vec<String>, String> {
    let manager = automation.window(window)?;
    let (names, _) = slots(automation, &manager)?;
    Ok(names.iter().map(|slot| shown(&value(slot))).collect())
}

/// What the portal holds once Dolphin has finished, or `failed` when it put
/// up a message instead.
fn settled(automation: &Automation, pid: u32, window: HWND, failed: &str) -> Result<Vec<String>, String> {
    let names = read(automation, window)?;
    if dismiss_message(pid) {
        return Err(failed.to_string());
    }
    Ok(names)
}

/// Turns on the radio button of `slot`, which the buttons below act on.
fn pick_slot(automation: &Automation, window: HWND, slot: usize) -> Result<IUIAutomationElement, String> {
    let manager = automation.window(window)?;
    let (names, radios) = slots(automation, &manager)?;
    let radio = &radios[slot];
    if !qt::is_selected(radio) {
        qt::press(radio, window).ok_or(LOOKS_DIFFERENT)?;
        soon(WAIT, || qt::is_selected(radio).then_some(())).ok_or(LOOKS_DIFFERENT)?;
    }
    Ok(names[slot].clone())
}

/// Puts a figure's file in one of Qt's file windows and presses Enter, which
/// presses its Open or Save button, then waits for it to close.
fn choose_file(automation: &Automation, picker: HWND, file: &Path) -> Result<(), String> {
    let window = automation.window(picker)?;
    let name_box = soon(WAIT, || {
        automation
            .0
            .of_kind(&window, UIA_EditControlTypeId)
            .into_iter()
            .find(|edit| automation_id(edit).ends_with(FILE_NAME))
    })
    .ok_or(LOOKS_DIFFERENT)?;
    qt::set_value(&name_box, &file.to_string_lossy()).ok_or(LOOKS_DIFFERENT)?;
    key(picker, VK_RETURN);
    if gone(picker) {
        Ok(())
    } else {
        key(picker, VK_ESCAPE);
        Err("Dolphin didn't take the figure's file. Try again.".to_string())
    }
}

fn automation_id(element: &IUIAutomationElement) -> String {
    unsafe { element.CurrentAutomationId() }.map(|id| id.to_string()).unwrap_or_default()
}

/// The figures on the portal, by slot.
pub fn figures(pid: u32) -> Result<Vec<String>, String> {
    let _turn = turn();
    let automation = Automation::new()?;
    let window = manager(&automation, pid)?;
    read(&automation, window)
}

/// Gets the manager open, out of sight, without doing anything in it, so
/// the first errand from the portal menu doesn't wait for it.
pub fn ready(pid: u32) {
    let _turn = turn();
    if let Ok(automation) = Automation::new() {
        let _ = manager(&automation, pid);
    }
}

/// Puts the figure in `file` on the portal in `slot`, counted from 0, and
/// returns what the portal holds afterwards.
pub fn load(pid: u32, slot: usize, file: &Path) -> Result<Vec<String>, String> {
    check(slot)?;
    let _turn = turn();
    let automation = Automation::new()?;
    let window = manager(&automation, pid)?;
    put_away_leftovers(&automation, pid);
    pick_slot(&automation, window, slot)?;
    let buttons = automation.window(window)?;
    qt::press(&automation.button(&buttons, LOAD)?, window).ok_or(LOOKS_DIFFERENT)?;
    let picker = arrives(pid, OPEN_FIGURE).ok_or("Dolphin didn't ask for the figure. Try again.")?;
    choose_file(&automation, picker, file)?;
    settled(&automation, pid, window, LOAD_FAILED)
}

/// Takes the figure in `slot` off the portal, and returns what is left.
pub fn clear(pid: u32, slot: usize) -> Result<Vec<String>, String> {
    check(slot)?;
    let _turn = turn();
    let automation = Automation::new()?;
    let window = manager(&automation, pid)?;
    put_away_leftovers(&automation, pid);
    let held = pick_slot(&automation, window, slot)?;
    if !shown(&value(&held)).is_empty() {
        let buttons = automation.window(window)?;
        qt::press(&automation.button(&buttons, CLEAR)?, window).ok_or(LOOKS_DIFFERENT)?;
        // The press is posted, and Dolphin may answer a read before it gets
        // to it.
        let _ = soon(WAIT, || shown(&value(&held)).is_empty().then_some(()));
    }
    settled(&automation, pid, window, "Dolphin couldn't take that figure off the portal. Try again.")
}

/// Has Dolphin's figure maker make a figure of `character` into `file`.
/// Dolphin then puts it on the portal in `slot` itself. Returns what the
/// portal holds afterwards.
pub fn create(pid: u32, slot: usize, character: &Character, file: &Path) -> Result<Vec<String>, String> {
    check(slot)?;
    let _turn = turn();
    let automation = Automation::new()?;
    let window = manager(&automation, pid)?;
    put_away_leftovers(&automation, pid);
    pick_slot(&automation, window, slot)?;
    let buttons = automation.window(window)?;
    qt::press(&automation.button(&buttons, CUSTOMIZE)?, window).ok_or(LOOKS_DIFFERENT)?;
    let maker = soon(WAIT, || maker_window(&automation, pid)).ok_or("Dolphin's figure maker didn't open. Try again.")?;
    out_of_sight(maker);
    if let Err(problem) = make(&automation, pid, maker, character, file) {
        put_away_leftovers(&automation, pid);
        return Err(problem);
    }
    settled(&automation, pid, window, MAKE_FAILED)
}

/// Fills in the Customize window and has it save the figure. Dolphin makes
/// the figure from its ID and Variant boxes alone (`CreateSkylanderAdvanced`).
fn make(automation: &Automation, pid: u32, maker: HWND, character: &Character, file: &Path) -> Result<(), String> {
    let window = automation.window(maker)?;
    let boxes = automation.0.of_kind(&window, UIA_EditControlTypeId);
    let [id_box, variant_box] = boxes.as_slice() else {
        return Err(LOOKS_DIFFERENT.to_string());
    };
    qt::set_value(id_box, &character.id.to_string()).ok_or(LOOKS_DIFFERENT)?;
    qt::set_value(variant_box, &character.variant.to_string()).ok_or(LOOKS_DIFFERENT)?;
    qt::press(&automation.button(&window, CREATE)?, maker).ok_or(LOOKS_DIFFERENT)?;
    let saver = arrives(pid, SAVE_FIGURE).ok_or("Dolphin didn't ask where to keep the figure. Try again.")?;
    choose_file(automation, saver, file)?;
    // Dolphin writes the figure, puts it on the portal and closes the
    // Customize window, or says why it couldn't.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_window_is_told_from_the_main_window_by_its_title() {
        assert!(is_game_title("Dolphin"), "as the window opens");
        assert!(is_game_title("Dolphin 2609a | JIT64 DC | Direct3D 11 | HLE | Skylanders Spyro's Adventure"));
        assert!(!is_game_title("Dolphin 2609a"), "the main window");
        assert!(!is_game_title("Skylanders Manager"));
        assert!(is_main_title("Dolphin 2609a"));
        assert!(!is_main_title("Dolphin 2609a | JIT64 DC | Direct3D 11 | HLE"));
    }

    #[test]
    fn a_slot_reads_as_the_menu_names_it() {
        assert_eq!(shown("None"), "");
        assert_eq!(shown("Spyro"), "Spyro");
        assert_eq!(shown("Unknown (Id:212 Var:12302)"), "Tempest Timer");
        assert_eq!(shown("Unknown (Id:999 Var:0)"), "Unknown (Id:999 Var:0)");
        assert_eq!(unknown("Unknown (Id:16 Var:6145)"), Some((16, 0x1801)));
    }

    /// Needs a Dolphin running with its main window there, started with
    /// `QT_QPA_PLATFORM` set as `launch` sets it, and a figure file, so it
    /// runs only by hand:
    /// `OMOIO_DOLPHIN_PID=<pid> OMOIO_FIGURE=<file> cargo test portal_on_a_running_dolphin -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn portal_on_a_running_dolphin() {
        use std::time::Instant;
        let pid: u32 = std::env::var("OMOIO_DOLPHIN_PID").unwrap().parse().unwrap();
        let figure = std::env::var("OMOIO_FIGURE").unwrap();
        let started = Instant::now();
        let before = figures(pid).unwrap();
        println!("opened and read in {:?}: {before:?}", started.elapsed());
        assert_eq!(before.len(), SLOTS);
        let started = Instant::now();
        let loaded = load(pid, 0, Path::new(&figure)).unwrap();
        println!("load took {:?}: {loaded:?}", started.elapsed());
        assert!(!loaded[0].is_empty(), "slot 1 holds the figure: {loaded:?}");
        let started = Instant::now();
        let cleared = clear(pid, 0).unwrap();
        println!("clear took {:?}", started.elapsed());
        assert!(cleared[0].is_empty(), "slot 1 is empty again: {cleared:?}");
        let made = Path::new(&figure).with_file_name(format!("omoio-made-{}.sky", std::process::id()));
        let started = Instant::now();
        let after = create(pid, 1, &Character { name: "Spyro".into(), id: 16, variant: 0 }, &made).unwrap();
        println!("create took {:?}: {after:?}", started.elapsed());
        assert_eq!(after[1], "Spyro");
        clear(pid, 1).unwrap();
        let _ = std::fs::remove_file(made);
    }
}
