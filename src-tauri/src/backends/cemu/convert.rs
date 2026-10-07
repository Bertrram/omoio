//! A copy of a Wii U disc image that Omoio can read, made by Cemu.
//!
//! A disc image is encrypted, and Omoio never decrypts. Cemu can turn one into
//! a compressed Wii U archive (.wua) with the user's own keys, but only from
//! its Title Manager: v2.6 has no command-line option for it
//! (`wxTitleManagerList::OnConvertToCompressedFormat`, `LaunchSettings.cpp`).
//! So Omoio starts Cemu with no game and the disc image's folder as its only
//! game path, opens the Title Manager, picks the disc image's row and has
//! Cemu convert it, pressing Cemu's own buttons the way it fills the portal
//! (portal.rs). The copy is only for reading the figures' pictures, and
//! figure_pictures.rs deletes it again straight after.

use super::portal::{
    arrives, children, class, close, finish_file_window, main_window, menu_command, out_of_sight, press, title,
    until, wait_up_to, windows_of,
};
use std::path::Path;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::{CUIAutomation8, IUIAutomation, IUIAutomationGridPattern, UIA_GridPatternId};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_DOWN, VK_HOME, VK_RETURN};
use windows::Win32::UI::WindowsAndMessaging::{
    GetDlgItem, GetMenu, GetMenuItemCount, GetMenuStringW, PostMessageW, SendMessageTimeoutW, HMENU, MF_BYPOSITION,
    SMTO_ABORTIFHUNG, WM_COMMAND, WM_KEYDOWN,
};

/// The Tools menu item that opens it, and its window's title (`MainWindow.cpp`,
/// `TitleManager.cpp`, v2.6).
const TITLE_MANAGER: &str = "Title Manager";
/// The windows conversion goes through, as `OnConvertToCompressedFormat`
/// titles them in v2.6: the item in a row's right-click menu, Cemu's question
/// whether to go on, the save window, the progress window, and the last word.
const CONVERT: &str = "Convert to compressed Wii U archive (.wua)";
const CONFIRM: &str = "Confirmation";
const SAVE: &str = "Save Wii U game archive file";
const CONVERTING: &str = "Converting to .wua";
const COMPLETE: &str = "Complete";
const FAILED: &str = "Error";

/// The Title Manager's columns (`wxTitleManagerList.h`) and what a disc
/// image's own row says in them (`GetTitleEntryText`).
const NAME: usize = 1;
const KIND: usize = 2;
const FORMAT: usize = 5;
const BASE: &str = "base";
const DISC: &str = "WUD";

const WM_CONTEXTMENU: u32 = 0x007B;
/// What an open menu's window answers: its menu, and which item to highlight.
const MN_GETHMENU: u32 = 0x01E1;
const MN_SELECTITEM: u32 = 0x01E5;
/// Which row of a list is selected, and where a progress bar stands.
const LVM_GETNEXTITEM: u32 = 0x100C;
const LVNI_SELECTED: isize = 0x0002;
const PBM_GETPOS: u32 = 0x0408;
const OK_BUTTON: i32 = 1;
const TDM_CLICK_BUTTON: u32 = 0x0466;

const START_WAIT: Duration = Duration::from_secs(30);
const WAIT: Duration = Duration::from_secs(10);
/// How long Cemu may take to find the disc image and read its title: it
/// decrypts the image's meta.xml with the key first.
const LIST_WAIT: Duration = Duration::from_secs(60);
/// How long Cemu may take to close once asked.
const QUIT_WAIT: Duration = Duration::from_secs(20);

const STOPPED: &str = "Stopped. Nothing was kept.";
const LOOKS_DIFFERENT: &str = "Cemu's Title Manager looks different from what Omoio knows.";

/// Has the Cemu at `exe`, whose settings are in `settings`, make a .wua of the
/// disc image `disc`, the game called `title` in the library, at `into`.
/// `progress` hears how far it is, out of 100. Cemu is closed again
/// afterwards, and its settings are put back as they were.
pub fn make_wua(
    exe: &Path,
    settings: &Path,
    disc: &Path,
    title: &str,
    into: &Path,
    progress: &dyn Fn(u32),
    cancel: &AtomicBool,
) -> Result<(), String> {
    let folder = disc.parent().ok_or("This game isn't where it was. Reconnect the drive it's on.")?;
    let before = std::fs::read_to_string(settings).map_err(|_| "Couldn't read Cemu's settings.".to_string())?;
    let ours = format!("<GamePaths>\n        <Entry>{}</Entry>\n    </GamePaths>", escaped(&folder.to_string_lossy()));
    let mut text = with_game_paths(&before, &ours);
    text = super::set_setting(&text, &["window_position"], "x", super::OFF_SCREEN);
    text = super::set_setting(&text, &["window_position"], "y", super::OFF_SCREEN);
    std::fs::write(settings, text).map_err(|_| "Couldn't change Cemu's settings.".to_string())?;
    let started = super::command(exe).spawn().map_err(|_| "Couldn't start Cemu.".to_string());
    let result = started.and_then(|mut cemu| {
        let made = convert(cemu.id(), title, into, progress, cancel);
        quit(&mut cemu);
        made
    });
    // Cemu writes all its settings again as it closes, game path and all, in
    // a layout of its own (seen 7 October 2026), so the file Omoio keeps
    // goes back whole.
    let _ = std::fs::write(settings, &before);
    result
}

fn convert(pid: u32, wanted: &str, into: &Path, progress: &dyn Fn(u32), cancel: &AtomicBool) -> Result<(), String> {
    let stopped = || cancel.load(Ordering::Relaxed);
    // Cemu starts off the screen, as the settings ask; its menu bar has the
    // Title Manager's command once the window is up.
    let main = soon(START_WAIT, || main_window(pid)).ok_or("Cemu didn't start.")?;
    let open_manager = soon(START_WAIT, || {
        let menu = unsafe { GetMenu(main) };
        (!menu.is_invalid()).then(|| menu_command(menu, TITLE_MANAGER)).flatten()
    })
    .ok_or(LOOKS_DIFFERENT)?;
    let _ = unsafe { PostMessageW(Some(main), WM_COMMAND, WPARAM(open_manager as usize), LPARAM(0)) };
    let manager = arrives(pid, TITLE_MANAGER, START_WAIT).ok_or(LOOKS_DIFFERENT)?;
    let list = soon(WAIT, || children(manager).into_iter().find(|&c| class(c) == "SysListView32")).ok_or(LOOKS_DIFFERENT)?;

    let ui = Ui::new()?;
    let row = soon(LIST_WAIT, || {
        if stopped() {
            return Some(Err(STOPPED.to_string()));
        }
        disc_row(&ui.rows(list), wanted).map(Ok)
    })
    .unwrap_or(Err("Cemu couldn't open the game. Play it once in Omoio, then try again.".to_string()))?;
    pick(list, row)?;

    // The row's own menu, out of sight the moment it shows, and its item.
    let _ = unsafe { PostMessageW(Some(list), WM_CONTEXTMENU, WPARAM(list.0 as usize), LPARAM(-1)) };
    let menu = wait_up_to(WAIT, pid, |w| class(w) == "#32768").ok_or(LOOKS_DIFFERENT)?;
    out_of_sight(menu);
    choose(menu, CONVERT)?;

    let confirm = arrives(pid, CONFIRM, WAIT).ok_or(LOOKS_DIFFERENT)?;
    ok(confirm);
    let saver = arrives(pid, SAVE, WAIT).ok_or(LOOKS_DIFFERENT)?;
    finish_file_window(pid, saver, into)?;

    let dialog = arrives(pid, CONVERTING, WAIT).ok_or(LOOKS_DIFFERENT)?;
    let gauge = children(dialog).into_iter().find(|&c| class(c) == "msctls_progress32");
    let mut asked_to_stop = false;
    loop {
        if let Some(done) = windows_of(pid).into_iter().find(|&w| title(w) == COMPLETE) {
            ok(done);
            progress(100);
            return Ok(());
        }
        if let Some(failed) = windows_of(pid).into_iter().find(|&w| title(w) == FAILED) {
            ok(failed);
            return Err("Cemu couldn't make the copy. There may not be room on the drive.".to_string());
        }
        if !windows_of(pid).contains(&dialog) {
            // Stopped, or the last checks before Cemu says it's complete.
            if asked_to_stop {
                return Err(STOPPED.to_string());
            }
        } else if stopped() && !asked_to_stop {
            asked_to_stop = true;
            if let Some(cancel) = children(dialog).into_iter().find(|&c| class(c) == "Button") {
                press(cancel);
            }
        } else if let Some(gauge) = gauge {
            progress(send(gauge, PBM_GETPOS, 0, 0) as u32);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The disc image's row: the one base title in disc image format, or when
/// the folder holds several, the one named as the game is in the library.
fn disc_row(rows: &[Vec<String>], wanted: &str) -> Option<usize> {
    let is_disc = |row: &Vec<String>| {
        row.get(KIND).is_some_and(|kind| kind == BASE) && row.get(FORMAT).is_some_and(|format| format == DISC)
    };
    let discs: Vec<usize> = (0..rows.len()).filter(|&at| is_disc(&rows[at])).collect();
    match discs.as_slice() {
        [only] => Some(*only),
        _ => discs.into_iter().find(|&at| rows[at].get(NAME).is_some_and(|name| plain(name) == plain(wanted))),
    }
}

/// Letters and digits only, in lower case, so "Skylanders - SuperChargers"
/// and "Skylanders SuperChargers" are the same.
fn plain(name: &str) -> String {
    name.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect()
}

/// Selects `row` the way the arrow keys do: Cemu's right-click menu acts on
/// the selected row.
fn pick(list: HWND, row: usize) -> Result<(), String> {
    let _ = unsafe { PostMessageW(Some(list), WM_KEYDOWN, WPARAM(usize::from(VK_HOME.0)), LPARAM(0)) };
    for _ in 0..row {
        let _ = unsafe { PostMessageW(Some(list), WM_KEYDOWN, WPARAM(usize::from(VK_DOWN.0)), LPARAM(0)) };
    }
    soon(WAIT, || (send(list, LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED) == row).then_some(()))
        .ok_or_else(|| LOOKS_DIFFERENT.to_string())
}

/// Presses OK in one of Cemu's messages. wxWidgets shows them as task
/// dialogs, whose buttons are no windows of their own, so the dialog is told
/// which to press (`TDM_CLICK_BUTTON`); a plain message box has an OK button.
fn ok(message: HWND) {
    out_of_sight(message);
    match unsafe { GetDlgItem(Some(message), OK_BUTTON) } {
        Ok(button) => press(button),
        Err(_) => {
            let _ = unsafe { PostMessageW(Some(message), TDM_CLICK_BUTTON, WPARAM(OK_BUTTON as usize), LPARAM(0)) };
        }
    }
}

/// Chooses the item labelled `label` in the open menu `menu`, as the arrow
/// keys and Enter would: highlighted by its place, which is found by its text,
/// then taken. Clicking it through UI Automation closed Cemu's menu without
/// doing anything (seen 7 October 2026).
fn choose(menu: HWND, label: &str) -> Result<(), String> {
    let items = HMENU(send(menu, MN_GETHMENU, 0, 0) as *mut core::ffi::c_void);
    let count = unsafe { GetMenuItemCount(Some(items)) };
    let at = (0..count.max(0))
        .find(|&at| {
            let mut buffer = [0u16; 128];
            let length = unsafe { GetMenuStringW(items, at as u32, Some(&mut buffer), MF_BYPOSITION) };
            String::from_utf16_lossy(&buffer[..length.max(0) as usize]).replace('&', "") == label
        })
        .ok_or(LOOKS_DIFFERENT)?;
    send(menu, MN_SELECTITEM, at as usize, 0);
    let _ = unsafe { PostMessageW(Some(menu), WM_KEYDOWN, WPARAM(usize::from(VK_RETURN.0)), LPARAM(0)) };
    Ok(())
}

/// Asks Cemu to close, and stops it if it won't.
fn quit(cemu: &mut Child) {
    let pid = cemu.id();
    for window in windows_of(pid).into_iter().filter(|&w| title(w) == TITLE_MANAGER) {
        close(window);
    }
    if let Some(main) = main_window(pid) {
        close(main);
    }
    until(QUIT_WAIT, || cemu.try_wait().is_ok_and(|status| status.is_some()));
    if cemu.try_wait().is_ok_and(|status| status.is_none()) {
        let _ = cemu.kill();
        let _ = cemu.wait();
    }
}

fn send(window: HWND, message: u32, wparam: usize, lparam: isize) -> usize {
    let mut result = 0usize;
    unsafe {
        SendMessageTimeoutW(window, message, WPARAM(wparam), LPARAM(lparam), SMTO_ABORTIFHUNG, 2000, Some(&mut result))
    };
    result
}

/// Asks `ready` until it gives something or `limit` is up.
fn soon<T>(limit: Duration, mut ready: impl FnMut() -> Option<T>) -> Option<T> {
    let end = Instant::now() + limit;
    loop {
        if let Some(found) = ready() {
            return Some(found);
        }
        if Instant::now() >= end {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The settings with `GamePaths` as `paths`, in place of the one there, or
/// added when there is none.
fn with_game_paths(text: &str, paths: &str) -> String {
    match block(text) {
        Some((start, end)) => format!("{}{paths}{}", &text[..start], &text[end..]),
        None => match text.rfind("</content>") {
            Some(at) => format!("{}    {paths}\n{}", &text[..at], &text[at..]),
            None => text.to_string(),
        },
    }
}

/// Where the `GamePaths` element starts and ends, empty or not.
fn block(text: &str) -> Option<(usize, usize)> {
    if let Some(start) = text.find("<GamePaths/>") {
        return Some((start, start + "<GamePaths/>".len()));
    }
    let start = text.find("<GamePaths>")?;
    let end = start + text[start..].find("</GamePaths>")? + "</GamePaths>".len();
    Some((start, end))
}

fn escaped(path: &str) -> String {
    path.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// UI Automation, for what Cemu's list says: a list in report view gives
/// each cell's text through it, where window messages would have to reach
/// into Cemu's own memory.
struct Ui(IUIAutomation);

impl Ui {
    fn new() -> Result<Self, String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                .map(Self)
                .map_err(|_| "Couldn't reach Cemu's windows.".to_string())
        }
    }

    /// Every row of a list, each cell's text in order.
    fn rows(&self, list: HWND) -> Vec<Vec<String>> {
        let grid = unsafe { self.0.ElementFromHandle(list) }
            .and_then(|element| unsafe { element.GetCurrentPatternAs::<IUIAutomationGridPattern>(UIA_GridPatternId) });
        let Ok(grid) = grid else {
            return Vec::new();
        };
        let (rows, columns) = unsafe { (grid.CurrentRowCount().unwrap_or(0), grid.CurrentColumnCount().unwrap_or(0)) };
        (0..rows)
            .map(|row| {
                (0..columns)
                    .map(|column| {
                        unsafe { grid.GetItem(row, column) }
                            .and_then(|cell| unsafe { cell.CurrentName() })
                            .map(|name| name.to_string())
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_path_is_swapped_in_and_put_back() {
        let empty = "<content>\n    <mlc_path></mlc_path>\n    <GamePaths/>\n    <GameCache/>\n</content>\n";
        let ours = "<GamePaths>\n        <Entry>D:\\Games\\Game</Entry>\n    </GamePaths>";
        let with = with_game_paths(empty, ours);
        assert!(with.contains("<Entry>D:\\Games\\Game</Entry>") && !with.contains("<GamePaths/>"));
        assert_eq!(with.matches("GamePaths>").count(), 2, "one element, opened and closed");
        assert_eq!(with_game_paths(&with, "<GamePaths/>"), empty);

        let paths = "<GamePaths>\n        <Entry>E:\\Wii U</Entry>\n    </GamePaths>";
        let theirs = format!("<content>\n    {paths}\n</content>\n");
        assert_eq!(with_game_paths(&with_game_paths(&theirs, ours), paths), theirs);

        let none = "<content>\n</content>\n";
        assert!(with_game_paths(none, ours).contains("<Entry>D:\\Games\\Game</Entry>"));
        assert_eq!(escaped("D:\\Tom & Jerry"), "D:\\Tom &amp; Jerry");
    }

    #[test]
    fn the_disc_images_row_is_found() {
        let row = |name: &str, kind: &str, format: &str| -> Vec<String> {
            ["00050000-101bfc00", name, kind, "0", "EUR", format, "Game Paths"].map(String::from).to_vec()
        };
        let save = row("Skylanders SWAP Force", "save", "Save folder");
        let update = row("Skylanders SuperChargers", "update", "Folder");
        let disc = row("Skylanders SuperChargers", "base", "WUD");
        assert_eq!(disc_row(&[save.clone(), update.clone(), disc.clone()], "anything"), Some(2));
        let other = row("Skylanders Trap Team", "base", "WUD");
        assert_eq!(disc_row(&[other.clone(), disc.clone()], "Skylanders - SuperChargers"), Some(1));
        assert_eq!(disc_row(&[other, disc], "Some Other Game"), None);
        assert_eq!(disc_row(&[save, update], "Skylanders SuperChargers"), None);
    }

    /// Needs Cemu, a disc image with its key in Cemu's keys.txt, and room for
    /// the copy, so it runs only by hand:
    /// `OMOIO_CEMU=<Cemu.exe> OMOIO_DISC=<image> OMOIO_TITLE=<name> OMOIO_INTO=<file.wua> cargo test make_a_wua_with_cemu -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn make_a_wua_with_cemu() {
        let exe = std::path::PathBuf::from(std::env::var("OMOIO_CEMU").unwrap());
        let settings = exe.parent().unwrap().join("portable").join("settings.xml");
        let disc = std::path::PathBuf::from(std::env::var("OMOIO_DISC").unwrap());
        let into = std::path::PathBuf::from(std::env::var("OMOIO_INTO").unwrap());
        let title = std::env::var("OMOIO_TITLE").unwrap();
        let before = std::fs::read_to_string(&settings).unwrap();
        let made = make_wua(&exe, &settings, &disc, &title, &into, &|pct| println!("{pct}%"), &AtomicBool::new(false));
        assert_eq!(made, Ok(()));
        assert!(into.is_file());
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), before, "Cemu's settings as they were");
    }
}
