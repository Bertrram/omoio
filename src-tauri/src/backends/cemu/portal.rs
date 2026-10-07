//! Figures on Cemu's Skylanders portal, put there from outside Cemu.
//!
//! Cemu keeps its portal in memory and fills it only from its own Emulated
//! USB Devices window; there is no file or command-line option for it. Omoio
//! opens that window through its menu item, presses its buttons with window
//! messages and reads what each slot holds. The window is left open for the
//! rest of the game, so a figure goes on without waiting for it to open
//! again. Nothing moves the mouse, and each window is put out of sight as it
//! opens so it never covers the game. How this was proven is in
//! docs/what-we-verified.md, "Skylanders".
//!
//! The same way, through Cemu's input settings window, it keeps the game from
//! hearing the pad while the portal menu or Big Picture is over it.

use crate::core::figures::{self, Character};
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use windows::core::BOOL;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{
    GetThreadPriority, OpenThread, SetThreadPriority, THREAD_PRIORITY, THREAD_PRIORITY_HIGHEST,
    THREAD_QUERY_LIMITED_INFORMATION, THREAD_SET_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GetClassNameW, GetDlgCtrlID, GetDlgItem, GetMenu, GetMenuItemCount, GetMenuItemID,
    GetMenuStringW, GetParent, GetSubMenu, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
    SendMessageTimeoutW, SetMenu, SetWindowPos, HMENU, MF_BYPOSITION, SMTO_ABORTIFHUNG, SWP_NOACTIVATE, SWP_NOSIZE,
    SWP_NOZORDER, WM_CLOSE, WM_COMMAND, WM_GETTEXT, WM_SETTEXT,
};

/// How many figures Cemu's portal holds (`MAX_SKYLANDERS`).
pub const SLOTS: usize = 16;

/// The window, its menu item and its file window, as Cemu 2.6 titles them.
const WINDOW: &str = "Emulated USB Devices";
const OPEN_FIGURE: &str = "Open Skylander dump";

/// Cemu's input settings window and its menu item, both titled this
/// (`InputSettings2.cpp`, `MainWindow.cpp`, v2.6).
const INPUT_SETTINGS: &str = "Input settings";

/// Cemu's figure maker and the save window it opens
/// (`EmulatedUSBDeviceFrame.cpp`, `CreateSkylanderDialog`).
const CREATOR: &str = "Skylander Figure Creator";
const SAVE_FIGURE: &str = "Create Skylander file";

/// Asking a list box how many items it has, one item's text and its length,
/// and the number kept with it. Windows carries these across processes.
const CB_GETCOUNT: u32 = 0x0146;
const CB_GETLBTEXT: u32 = 0x0148;
const CB_GETLBTEXTLEN: u32 = 0x0149;
const CB_GETITEMDATA: u32 = 0x0150;

const LOOKS_DIFFERENT: &str = "Cemu's portal looks different from what Omoio knows.";

/// What a button tells its window when it is clicked, in the high half of a
/// `WM_COMMAND`'s first number.
const BN_CLICKED: usize = 0;
/// The file name box in a Windows file window: in an open window an Edit
/// inside the list numbered `FILE_NAME_LIST`, in a save window an Edit
/// numbered `FILE_NAME_EDIT` itself (Cemu's "Create Skylander file", seen 13
/// September 2026). Any other Edit, such as the search box, would take the
/// name and be ignored, and Cemu would save under its own name somewhere else.
const FILE_NAME_LIST: i32 = 0x047C;
const FILE_NAME_EDIT: i32 = 1001;
/// The Open or Save button.
const OPEN_BUTTON: i32 = 1;

const WAIT: Duration = Duration::from_secs(5);
/// How long Cemu may take to show a figure going on or off in its slot.
const SETTLE: Duration = Duration::from_secs(2);
const SAVE_WAIT: Duration = Duration::from_secs(15);
const MAKER_WAIT: Duration = Duration::from_secs(20);
/// How long Cemu may take to show its menu bar or open one of its windows.
/// In the first minute of a game it is still loading and building shaders,
/// and a window took longer than five seconds: a figure failed twice and
/// went on at the third try (2 October 2026).
const START_WAIT: Duration = Duration::from_secs(20);
/// How long the portal window may take to show before its menu command is
/// sent again, in case Cemu let it go unanswered. Once Cemu answers, the
/// window shows within a fifth of a second (6 October 2026).
const RESEND: Duration = Duration::from_secs(1);
/// How long the game may go on hearing the pad while the portal window
/// opens ahead of the input settings window.
const READY_WAIT: Duration = Duration::from_secs(3);

/// One errand in Cemu's portal window at a time. A second one would press
/// buttons in the window the first is working in.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn turn() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A command from Cemu's menu bar, waiting for the bar to be read if Cemu
/// hasn't shown it yet.
fn command_when_ready(pid: u32, label: &str) -> Option<u32> {
    let until = Instant::now() + START_WAIT;
    loop {
        tidy(pid);
        if let Some(found) = command(pid, label) {
            return Some(found);
        }
        if Instant::now() >= until {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Waits up to `limit` for Cemu's window titled `wanted` and puts it out of
/// sight. Looked for often, so it is gone before it can be seen over the game.
pub(super) fn arrives(pid: u32, wanted: &str, limit: Duration) -> Option<HWND> {
    let until = Instant::now() + limit;
    while Instant::now() < until {
        if let Some(window) = windows_of(pid).into_iter().find(|&w| title(w) == wanted) {
            out_of_sight(window);
            return Some(window);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

pub(super) fn text(window: HWND) -> String {
    let mut buffer = [0u16; 512];
    let mut copied = 0usize;
    // Windows carries the text across to Cemu's process and back. A window
    // that has stopped answering is given up on rather than waited for.
    unsafe {
        SendMessageTimeoutW(
            window,
            WM_GETTEXT,
            WPARAM(buffer.len()),
            LPARAM(buffer.as_mut_ptr() as isize),
            SMTO_ABORTIFHUNG,
            2000,
            Some(&mut copied),
        )
    };
    String::from_utf16_lossy(&buffer[..copied.min(buffer.len())])
}

/// A window's title as Windows keeps it, asked without sending Cemu
/// anything, so a busy Cemu never holds up a look at its windows. Only for
/// Cemu's windows themselves; a box or button inside one answers `text`.
pub(super) fn title(window: HWND) -> String {
    let mut buffer = [0u16; 256];
    let length = unsafe { GetWindowTextW(window, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

/// Cemu's window thread at a higher priority for as long as this is kept,
/// and back as it was when dropped, however the wait ends. Cemu's figure
/// maker fills its list on that thread in a way that grows with the square
/// of the list (`CreateSkylanderDialog`, wxWidgets' `wxChoice`), and while a
/// game runs the thread shares the processor with it: 5.5 s to open against
/// 0.4 s with no game.
struct Hurried {
    thread: HANDLE,
    was: i32,
}

impl Hurried {
    fn new(window: HWND) -> Option<Self> {
        let id = unsafe { GetWindowThreadProcessId(window, None) };
        let thread =
            unsafe { OpenThread(THREAD_SET_LIMITED_INFORMATION | THREAD_QUERY_LIMITED_INFORMATION, false, id) }.ok()?;
        let was = unsafe { GetThreadPriority(thread) };
        if unsafe { SetThreadPriority(thread, THREAD_PRIORITY_HIGHEST) }.is_err() {
            let _ = unsafe { CloseHandle(thread) };
            return None;
        }
        Some(Self { thread, was })
    }
}

impl Drop for Hurried {
    fn drop(&mut self) {
        unsafe {
            let _ = SetThreadPriority(self.thread, THREAD_PRIORITY(self.was));
            let _ = CloseHandle(self.thread);
        }
    }
}

fn set_text(window: HWND, value: &str) {
    let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        SendMessageTimeoutW(
            window,
            WM_SETTEXT,
            WPARAM(0),
            LPARAM(wide.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            2000,
            None,
        )
    };
}

/// Selecting all of a box's text, and one typed character.
const EM_SETSEL: u32 = 0x00B1;
const WM_CHAR: u32 = 0x0102;

/// Types `value` into a box as keys would, replacing what it held. Setting
/// the text outright shows it, but a save window never hears of the change
/// and saves under the name it had before (seen 13 September 2026). The
/// characters are posted, so they arrive in order before anything posted
/// after them.
fn type_into(field: HWND, value: &str) {
    let _ = unsafe { PostMessageW(Some(field), EM_SETSEL, WPARAM(0), LPARAM(-1)) };
    for unit in value.encode_utf16() {
        let _ = unsafe { PostMessageW(Some(field), WM_CHAR, WPARAM(usize::from(unit)), LPARAM(1)) };
    }
}

pub(super) fn class(window: HWND) -> String {
    let mut buffer = [0u16; 128];
    let length = unsafe { GetClassNameW(window, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

/// Presses a button by telling its window the button was clicked, which is
/// what the button itself does after a real click. A simulated mouse click
/// (`BM_CLICK`) only counts in the window in front, and Cemu's windows are
/// kept out of sight behind the game: in the figure maker it did nothing.
pub(super) fn press(button: HWND) {
    let Ok(parent) = (unsafe { GetParent(button) }) else {
        return;
    };
    let id = unsafe { GetDlgCtrlID(button) } as usize & 0xFFFF;
    // Posted rather than sent: the press can open a window that waits for an
    // answer, and waiting on it here would wait forever.
    let _ = unsafe { PostMessageW(Some(parent), WM_COMMAND, WPARAM((BN_CLICKED << 16) | id), LPARAM(button.0 as isize)) };
}

pub(super) fn close(window: HWND) {
    let _ = unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) };
}

/// Far off the screen, where it still works but nobody sees it.
pub(super) fn out_of_sight(window: HWND) {
    let _ = unsafe {
        SetWindowPos(
            window,
            None,
            -32000,
            -32000,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
}

unsafe extern "system" fn collect(window: HWND, list: LPARAM) -> BOOL {
    let list = unsafe { &mut *(list.0 as *mut Vec<HWND>) };
    list.push(window);
    BOOL(1)
}

pub(super) fn children(window: HWND) -> Vec<HWND> {
    let mut list: Vec<HWND> = Vec::new();
    let _ = unsafe { EnumChildWindows(Some(window), Some(collect), LPARAM(&mut list as *mut Vec<HWND> as isize)) };
    list
}

/// Cemu's windows that are showing, in the order Windows keeps them.
pub(super) fn windows_of(pid: u32) -> Vec<HWND> {
    let mut all: Vec<HWND> = Vec::new();
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut all as *mut Vec<HWND> as isize)) };
    all.into_iter()
        .filter(|&window| {
            let mut owner = 0u32;
            unsafe { GetWindowThreadProcessId(window, Some(&mut owner)) };
            owner == pid && unsafe { IsWindowVisible(window) }.as_bool()
        })
        .collect()
}

pub(super) fn wait_up_to(limit: Duration, pid: u32, found: impl Fn(HWND) -> bool) -> Option<HWND> {
    let until = Instant::now() + limit;
    while Instant::now() < until {
        if let Some(window) = windows_of(pid).into_iter().find(|&w| found(w)) {
            return Some(window);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

/// Asks `done` every 20 ms until it says yes or `limit` is up, so a step
/// takes as long as Cemu needs for it rather than a fixed pause.
pub(super) fn until(limit: Duration, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + limit;
    while !done() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Whether Cemu has put up a message of its own.
fn message_up(pid: u32, expected: &[&str]) -> bool {
    windows_of(pid).into_iter().any(|w| is_message(w, expected))
}

/// What `slot` holds now, empty for nothing.
fn slot_now(window: HWND, slot: usize) -> String {
    read(window).map(|names| names[slot].clone()).unwrap_or_default()
}

/// The command a menu item sends, found by its text so it does not matter
/// what number wxWidgets gave it this time.
pub(super) fn menu_command(menu: HMENU, wanted: &str) -> Option<u32> {
    let count = unsafe { GetMenuItemCount(Some(menu)) };
    for at in 0..count.max(0) {
        let sub = unsafe { GetSubMenu(menu, at) };
        if !sub.is_invalid() {
            if let Some(found) = menu_command(sub, wanted) {
                return Some(found);
            }
            continue;
        }
        let mut buffer = [0u16; 128];
        let length = unsafe { GetMenuStringW(menu, at as u32, Some(&mut buffer), MF_BYPOSITION) };
        let label = String::from_utf16_lossy(&buffer[..length.max(0) as usize]).replace('&', "");
        if label.starts_with(wanted) {
            return Some(unsafe { GetMenuItemID(menu, at) });
        }
    }
    None
}

/// The menu commands Omoio uses of each running Cemu, by process and menu
/// text. Their numbers are read from the menu while the bar is on the window,
/// and kept: Omoio takes the bar off the game picture, and Cemu's own
/// fullscreen takes it off too, but either way the commands still work
/// (`MainWindow.cpp`, `SetFullScreen`, v2.6).
static COMMANDS: Mutex<Vec<(u32, &'static str, u32)>> = Mutex::new(Vec::new());

fn command(pid: u32, label: &str) -> Option<u32> {
    COMMANDS
        .lock()
        .unwrap()
        .iter()
        .find(|&&(known, item, _)| known == pid && item == label)
        .map(|&(_, _, command)| command)
}

/// Cemu's main window, found whether or not it is showing: Big Picture hides
/// the game's window while it is up.
pub(super) fn main_window(pid: u32) -> Option<HWND> {
    let mut all: Vec<HWND> = Vec::new();
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut all as *mut Vec<HWND> as isize)) };
    all.into_iter().find(|&window| {
        let mut owner = 0u32;
        unsafe { GetWindowThreadProcessId(window, Some(&mut owner)) };
        owner == pid && (!unsafe { GetMenu(window) }.is_invalid() || title(window).starts_with("Cemu"))
    })
}

/// Learns the commands Omoio uses from Cemu's menu bar, then takes the bar off
/// the window so it never shows over the game. Called while the game runs,
/// since Cemu puts the bar back each time it leaves its own fullscreen.
pub fn tidy(pid: u32) {
    let Some(main) = main_window(pid) else {
        return;
    };
    let menu = unsafe { GetMenu(main) };
    if menu.is_invalid() {
        return;
    }
    let found: Vec<(&'static str, u32)> = [WINDOW, INPUT_SETTINGS]
        .into_iter()
        .filter_map(|label| menu_command(menu, label).map(|command| (label, command)))
        .collect();
    if found.is_empty() {
        return;
    }
    let mut known = COMMANDS.lock().unwrap();
    known.retain(|&(other, _, _)| other != pid);
    known.extend(found.into_iter().map(|(label, command)| (pid, label, command)));
    drop(known);
    let _ = unsafe { SetMenu(main, None) };
}

/// Stops the game hearing the pad, or lets it hear again.
///
/// Cemu reads no game input while its own input settings window exists:
/// `g_inputConfigWindowHasFocus` is set in `InputSettings2`'s constructor and
/// cleared in its destructor, and `vpad.cpp` and `padscore.cpp` skip every
/// read while it is set (v2.6). So the window is opened out of sight while an
/// Omoio menu is over the game, and closed after. It is modal and disables
/// Cemu's main window, but a posted menu command still opens Emulated USB
/// Devices, and a figure loads and clears with it open (tried on a running
/// game, 27 September 2026). It is a dialog of the same kind as Cemu's
/// messages, so `is_message` has to leave it out.
pub fn hush(pid: u32, hushed: bool) -> Result<(), String> {
    let open_now = windows_of(pid).into_iter().find(|&w| title(w) == INPUT_SETTINGS);
    if !hushed {
        if let Some(window) = open_now {
            close(window);
        }
        return Ok(());
    }
    if let Some(window) = open_now {
        out_of_sight(window);
        return Ok(());
    }
    let main = main_window(pid).ok_or("Cemu isn't answering.")?;
    let input = command_when_ready(pid, INPUT_SETTINGS).ok_or("Cemu isn't ready yet.")?;
    let _ = unsafe { PostMessageW(Some(main), WM_COMMAND, WPARAM(input as usize), LPARAM(0)) };
    arrives(pid, INPUT_SETTINGS, START_WAIT)
        .map(|_| ())
        .ok_or("Cemu's input settings didn't open.".to_string())
}

/// The Emulated USB Devices window, opened if it is not already, and put out
/// of sight. Its menu command is sent again each `RESEND` until it shows;
/// Cemu only shows the window again when it is open already.
fn open(pid: u32) -> Result<HWND, String> {
    if let Some(window) = windows_of(pid).into_iter().find(|&w| title(w) == WINDOW) {
        out_of_sight(window);
        return Ok(window);
    }
    let main = main_window(pid).ok_or("Cemu isn't answering. Try again once the game has started.")?;
    let devices = command_when_ready(pid, WINDOW).ok_or("Cemu's portal isn't ready yet. Try again in a moment.")?;
    show_devices(pid, main, devices, START_WAIT).ok_or("Cemu's portal didn't open. Try again.".to_string())
}

fn show_devices(pid: u32, main: HWND, devices: u32, limit: Duration) -> Option<HWND> {
    let until = Instant::now() + limit;
    while Instant::now() < until {
        let _ = unsafe { PostMessageW(Some(main), WM_COMMAND, WPARAM(devices as usize), LPARAM(0)) };
        if let Some(window) = arrives(pid, WINDOW, RESEND) {
            return Some(window);
        }
    }
    None
}

/// Opens the portal window as the portal menu opens, before Cemu's input
/// settings window: for a good while after that window opens, Cemu answers
/// no menu command at all, and the portal menu's first read waited twenty
/// seconds and failed (6 October 2026). The menu covers the screen as it
/// opens, and the window stays open after.
pub fn ready(pid: u32) {
    let _turn = turn();
    let open_now = windows_of(pid).into_iter().map(title).collect::<Vec<_>>();
    if open_now.iter().any(|title| title == WINDOW || title == INPUT_SETTINGS) {
        return;
    }
    tidy(pid);
    if let (Some(main), Some(devices)) = (main_window(pid), command(pid, WINDOW)) {
        let _ = show_devices(pid, main, devices, READY_WAIT);
    }
}

/// Controls of one class and text, in the order Cemu made them. The
/// Skylanders page is made first, so its sixteen rows come before the other
/// toys' pages.
pub(super) fn controls(window: HWND, class_has: &str, label: Option<&str>) -> Vec<HWND> {
    children(window)
        .into_iter()
        .filter(|&c| class(c).contains(class_has) && label.map_or(true, |l| text(c) == l))
        .collect()
}

/// What each slot holds, empty where it holds nothing. A window that closed
/// while being read has no slots left, and is not taken for an empty portal.
fn read(window: HWND) -> Result<Vec<String>, String> {
    let names: Vec<String> = controls(window, "Edit", None)
        .into_iter()
        .take(SLOTS)
        .map(|slot| {
            let name = text(slot);
            if name == "None" {
                String::new()
            } else {
                shown(&name)
            }
        })
        .collect();
    if names.len() == SLOTS {
        Ok(names)
    } else {
        Err(LOOKS_DIFFERENT.to_string())
    }
}

/// A slot's name as the menu lists the character, through the same
/// `figures::named`, so the two match. Cemu calls a figure its list doesn't
/// have "Unknown (212 12302)" (`FindSkylander`, v2.6), which is how each of
/// the seven traps made with Trap Team's own variant shows.
fn shown(name: &str) -> String {
    unknown(name)
        .and_then(|(id, variant)| figures::trap_named(id, variant))
        .map_or_else(|| figures::named(name), str::to_string)
}

fn unknown(name: &str) -> Option<(u16, u16)> {
    let (id, variant) = name.strip_prefix("Unknown (")?.strip_suffix(')')?.split_once(' ')?;
    Some((id.parse().ok()?, variant.parse().ok()?))
}

fn check(slot: usize) -> Result<(), String> {
    if slot < SLOTS {
        Ok(())
    } else {
        Err("The portal has no slot there.".to_string())
    }
}

/// Whether a window is a dialog Cemu put up, other than the ones Omoio opened
/// on purpose. The input settings window is a dialog of the same kind, and
/// while a menu is over the game it is Omoio's: taken for a message, it made
/// every figure look as if it had failed.
pub(super) fn is_message(window: HWND, expected: &[&str]) -> bool {
    let name = title(window);
    class(window) == "#32770" && name != INPUT_SETTINGS && !expected.contains(&name.as_str())
}

/// Clicks OK on a message Cemu put up, so it does not sit over the game, and
/// hands on what it said.
pub(super) fn dismiss_message(pid: u32, expected: &[&str]) -> Option<String> {
    let message = windows_of(pid).into_iter().find(|&w| is_message(w, expected))?;
    let said = children(message)
        .into_iter()
        .filter(|&c| class(c) == "Static")
        .map(text)
        .filter(|t| !t.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if let Ok(ok) = unsafe { GetDlgItem(Some(message), OPEN_BUTTON) } {
        press(ok);
    }
    Some(said)
}

/// The figures on the portal, by slot.
pub fn figures(pid: u32) -> Result<Vec<String>, String> {
    let _turn = turn();
    read(open(pid)?)
}

/// Puts the figure in `file` on the portal in `slot`, counted from 0, and
/// returns what the portal holds afterwards.
pub fn load(pid: u32, slot: usize, file: &Path) -> Result<Vec<String>, String> {
    check(slot)?;
    let _turn = turn();
    let window = open(pid)?;
    let load = controls(window, "Button", Some("Load"))
        .into_iter()
        .nth(slot)
        .ok_or("Cemu's portal looks different from what Omoio knows.")?;
    press(load);

    let picker = wait_up_to(SAVE_WAIT, pid, |w| class(w) == "#32770" && title(w) == OPEN_FIGURE)
        .ok_or("Cemu didn't ask for the figure. Try again.")?;
    finish_file_window(pid, picker, file)?;
    // Cemu reads the file once its window has closed, then names it in the
    // slot or says why it couldn't.
    until(SETTLE, || !slot_now(window, slot).is_empty() || message_up(pid, &[OPEN_FIGURE]));
    if let Some(said) = dismiss_message(pid, &[OPEN_FIGURE]) {
        return Err(if said.is_empty() {
            "Cemu couldn't put that figure on the portal.".to_string()
        } else {
            format!("Cemu couldn't put that figure on the portal: {said}")
        });
    }
    read(window)
}

/// Takes the figure in `slot` off the portal, and returns what is left.
pub fn clear(pid: u32, slot: usize) -> Result<Vec<String>, String> {
    check(slot)?;
    let _turn = turn();
    let window = open(pid)?;
    let button = controls(window, "Button", Some("Clear"))
        .into_iter()
        .nth(slot)
        .ok_or("Cemu's portal looks different from what Omoio knows.")?;
    press(button);
    until(SETTLE, || slot_now(window, slot).is_empty());
    read(window)
}

/// Fills in a Windows file window Cemu opened, out of sight, presses its
/// Open or Save button, and waits for it to close. The button is pressed
/// once the name box shows the whole name, which the typed characters take
/// a moment to fill in.
pub(super) fn finish_file_window(pid: u32, picker: HWND, file: &Path) -> Result<(), String> {
    const DIFFERENT: &str = "Cemu's file window looks different from what Omoio knows.";
    out_of_sight(picker);
    let name_box = children(picker)
        .into_iter()
        .filter(|&c| class(c) == "Edit")
        .find(|&c| {
            let own = unsafe { GetDlgCtrlID(c) };
            let inside = unsafe { GetParent(c) }.map_or(0, |list| unsafe { GetDlgCtrlID(list) });
            own == FILE_NAME_EDIT || inside == FILE_NAME_LIST
        })
        .ok_or(DIFFERENT)?;
    let name = file.to_string_lossy();
    type_into(name_box, &name);
    // Starts with: the open window's suggestions can add to what was typed.
    until(Duration::from_secs(2), || text(name_box).starts_with(name.as_ref()));
    let button = unsafe { GetDlgItem(Some(picker), OPEN_BUTTON) }.map_err(|_| DIFFERENT.to_string())?;
    press(button);
    until(WAIT, || !windows_of(pid).contains(&picker));
    Ok(())
}

fn send(window: HWND, message: u32, wparam: usize, lparam: isize) -> usize {
    let mut result = 0usize;
    unsafe {
        SendMessageTimeoutW(
            window,
            message,
            WPARAM(wparam),
            LPARAM(lparam),
            SMTO_ABORTIFHUNG,
            2000,
            Some(&mut result),
        )
    };
    result
}

/// Cemu's figure maker, opened from the Create button of `slot` and put
/// out of sight. While a game runs it takes over five seconds to appear
/// (5.4 s in Swap Force, against 0.4 s with no game), so Cemu's window
/// thread is hurried while it opens, it gets a longer wait than other
/// windows, and one still open from a try that gave up is used rather than
/// a second opened over it.
fn open_creator(pid: u32, window: HWND, slot: usize) -> Result<HWND, String> {
    if let Some(creator) = windows_of(pid).into_iter().find(|&w| title(w) == CREATOR) {
        out_of_sight(creator);
        return Ok(creator);
    }
    let create = controls(window, "Button", Some("Create"))
        .into_iter()
        .nth(slot)
        .ok_or(LOOKS_DIFFERENT)?;
    let _hurried = Hurried::new(window);
    press(create);
    let creator =
        wait_up_to(MAKER_WAIT, pid, |w| title(w) == CREATOR).ok_or("Cemu's figure maker didn't open. Try again.")?;
    out_of_sight(creator);
    Ok(creator)
}

fn cancel_creator(creator: HWND) {
    if let Some(cancel) = controls(creator, "Button", Some("Cancel")).into_iter().next() {
        press(cancel);
    }
}

/// Every character Cemu's figure maker offers, read from its list: the name
/// each item shows and the id and variant it carries. The maker is closed
/// again without making anything.
pub fn characters(pid: u32) -> Result<Vec<Character>, String> {
    let _turn = turn();
    let window = open(pid)?;
    let creator = open_creator(pid, window, 0)?;
    let found: Vec<Character> = children(creator)
        .into_iter()
        .find(|&c| class(c) == "ComboBox")
        .map(|list| {
            let count = send(list, CB_GETCOUNT, 0, 0).min(4096);
            (0..count)
                .filter_map(|at| {
                    // A failed ask comes back as -1, so anything outlandish is skipped.
                    let length = send(list, CB_GETLBTEXTLEN, at, 0);
                    if length == 0 || length > 256 {
                        return None;
                    }
                    let mut buffer = vec![0u16; length + 1];
                    send(list, CB_GETLBTEXT, at, buffer.as_mut_ptr() as isize);
                    let name = String::from_utf16_lossy(&buffer[..length]);
                    Character::from_item(&name, send(list, CB_GETITEMDATA, at, 0) as u64)
                })
                .collect()
        })
        .unwrap_or_default();
    cancel_creator(creator);
    std::thread::sleep(Duration::from_millis(300));
    if found.is_empty() {
        Err("Couldn't read Cemu's list of characters.".to_string())
    } else {
        Ok(found)
    }
}

/// Has Cemu's figure maker make a figure of `character` into `file`. Cemu
/// then puts it on the portal in `slot` itself. Returns what the portal
/// holds afterwards.
pub fn create(pid: u32, slot: usize, character: &Character, file: &Path) -> Result<Vec<String>, String> {
    check(slot)?;
    let _turn = turn();
    let window = open(pid)?;
    let creator = open_creator(pid, window, slot)?;
    // The id and variant boxes, not the typing box inside the list above them.
    let boxes: Vec<HWND> = children(creator)
        .into_iter()
        .filter(|&c| class(c) == "Edit" && unsafe { GetParent(c) }.ok() == Some(creator))
        .collect();
    let (Some(&id_box), Some(&variant_box), 2) = (boxes.first(), boxes.get(1), boxes.len()) else {
        cancel_creator(creator);
        return Err(LOOKS_DIFFERENT.to_string());
    };
    set_text(id_box, &character.id.to_string());
    set_text(variant_box, &character.variant.to_string());
    let Some(make) = controls(creator, "Button", Some("Create")).into_iter().next() else {
        cancel_creator(creator);
        return Err(LOOKS_DIFFERENT.to_string());
    };
    press(make);

    // Windows' save window can take several seconds the first time a program
    // opens one. Anything else Cemu puts up instead is an error of its own,
    // and the figure maker is closed either way: left open it is modal, and
    // every later try would find the portal window unable to answer.
    let saver = wait_up_to(SAVE_WAIT, pid, |w| is_message(w, &[CREATOR, WINDOW]));
    let Some(saver) = saver.filter(|&w| title(w) == SAVE_FIGURE) else {
        let said = dismiss_message(pid, &[CREATOR, WINDOW, SAVE_FIGURE]);
        std::thread::sleep(Duration::from_millis(300));
        cancel_creator(creator);
        std::thread::sleep(Duration::from_millis(300));
        return Err(match said {
            Some(said) if !said.is_empty() => format!("Cemu couldn't make that figure: {said}"),
            _ => "Cemu didn't ask where to keep the figure. Try again.".to_string(),
        });
    };
    finish_file_window(pid, saver, file)?;
    // Cemu writes the figure, closes its maker and puts the figure in the
    // slot, or says why it couldn't.
    until(WAIT, || !windows_of(pid).contains(&creator));
    until(SETTLE, || !slot_now(window, slot).is_empty() || message_up(pid, &[OPEN_FIGURE, SAVE_FIGURE, CREATOR]));
    if let Some(said) = dismiss_message(pid, &[OPEN_FIGURE, SAVE_FIGURE, CREATOR]) {
        return Err(if said.is_empty() {
            "Cemu couldn't make that figure.".to_string()
        } else {
            format!("Cemu couldn't make that figure: {said}")
        });
    }
    read(window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_reads_as_the_menu_names_it() {
        assert_eq!(shown("Spyro"), "Spyro");
        assert_eq!(shown("Dragonâ€™s Peak"), "Dragon’s Peak");
        assert_eq!(shown("Hammer Slam Bowser (Nintendo Only)"), "Hammer Slam Bowser");
        assert_eq!(shown("Dark Clown Cruiser (Nintendo Only)"), "Dark Clown Cruiser");
        // Tempest Timer made with Trap Team's own variant, 0x300E.
        assert_eq!(shown("Unknown (212 12302)"), "Tempest Timer");
        assert_eq!(shown("Unknown (219 12309)"), "Shining Ship");
        // A figure Cemu doesn't know stays as Cemu put it.
        assert_eq!(shown("Unknown (999 0)"), "Unknown (999 0)");
        assert_eq!(unknown("Unknown Spyro"), None);
    }

    /// Needs a Cemu running and a figure file, so it runs only by hand:
    /// `OMOIO_CEMU_PID=<pid> OMOIO_FIGURE=<file> cargo test portal_on_a_running_cemu -- --ignored`
    #[test]
    #[ignore]
    fn portal_on_a_running_cemu() {
        let pid: u32 = std::env::var("OMOIO_CEMU_PID").unwrap().parse().unwrap();
        let figure = std::env::var("OMOIO_FIGURE").unwrap();
        let before = figures(pid).unwrap();
        assert_eq!(before.len(), SLOTS);
        let loaded = load(pid, 0, Path::new(&figure)).unwrap();
        assert!(!loaded[0].is_empty(), "slot 1 holds the figure: {loaded:?}");
        let cleared = clear(pid, 0).unwrap();
        assert!(cleared[0].is_empty(), "slot 1 is empty again: {cleared:?}");
    }
}
