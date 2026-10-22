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
//!
//! The build of Cemu made for Omoio (github.com/Bertrram/Cemu, branch
//! omoio-portal-ipc) also has a portal server like RPCS3's
//! (backends/portal_ipc.rs), which puts figures on, takes them off, makes
//! them and keeps the game deaf at once, with no window. When it answers as
//! the game starts, all of that goes through it; the figure maker's list,
//! which the server doesn't give, is still read from Cemu's window. Cemu 2.6
//! has no server, and everything goes through its windows as before.

use crate::backends::portal_ipc::{free_port, Failed, Server, Slot};
use crate::core::figures::{self, Character};
use std::path::Path;
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};
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

/// The windows Omoio opens in Cemu for the portal, none of them a message
/// (`start_errors`).
pub(super) const OWN_WINDOWS: [&str; 4] = [WINDOW, OPEN_FIGURE, CREATOR, SAVE_FIGURE];

/// Asking a list box how many items it has, one item's text and its length,
/// and the number kept with it. Windows carries these across processes.
const CB_GETCOUNT: u32 = 0x0146;
const CB_GETLBTEXT: u32 = 0x0148;
const CB_GETLBTEXTLEN: u32 = 0x0149;
const CB_GETITEMDATA: u32 = 0x0150;

const LOOKS_DIFFERENT: &str = "Cemu's portal looks different from what Omoio knows.";
const LOAD_FAILED: &str = "Cemu couldn't put that figure on the portal.";
const MAKE_FAILED: &str = "Cemu couldn't make that figure.";
const NOT_ANSWERING: &str = "Cemu's portal isn't answering. Try again.";

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

/// Whether an errand in Cemu's portal window is under way. Each
/// reads the messages Cemu puts up for it (`dismiss_message`), so they are
/// left to it meanwhile (`start_errors`).
pub(super) fn busy() -> bool {
    making() || matches!(ONE_AT_A_TIME.try_lock(), Err(TryLockError::WouldBlock))
}

/// How many errands with Cemu's figure maker are under way or waiting their
/// turn, so the portal menu closing meanwhile leaves Cemu's windows alone
/// (`making`).
static MAKING: AtomicUsize = AtomicUsize::new(0);

/// Counts one errand with the figure maker for as long as it is kept.
struct Making;

impl Making {
    fn new() -> Self {
        MAKING.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for Making {
    fn drop(&mut self) {
        MAKING.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Whether Omoio is having Cemu's figure maker make a figure or list its
/// characters, or is about to. Closing the input settings window `hush`
/// keeps open meanwhile can't end well.
///
/// That window is modal, and the figure maker opened over it is modal too,
/// with a modal loop of its own inside the first one (`MainWindow.cpp`,
/// `OnOptionsInput`, and `EmulatedUSBDeviceFrame.cpp`, `CreateSkylander`,
/// both `ShowModal`, v2.6). wxWidgets ends a modal loop only while it is
/// the innermost one: closing the outer window ends in `Exit`, which checks
/// `IsRunning()`, true only for the loop that is running now, and returns
/// without ending anything (wxWidgets 3.2.5, `evtloopcmn.cpp`,
/// `wxEventLoopBase::Exit`; `evtloop.h`, `IsRunning`; `dialog.cpp`,
/// `wxDialog::Show` and `EndModal`). The window is hidden all the same, so
/// its loop goes on with nothing left to end it: the game stays deaf
/// (`g_inputConfigWindowHasFocus` is cleared only when the window is
/// deleted, `InputSettings2.cpp`), Cemu's main window stays turned off, and
/// Cemu writes "Encountered wxWidgets assert!" to its log
/// (`CemuApp::OnAssertFailure`; all read 14 October 2026). That is what
/// closing the menu while a Skylander was being made ran into (GitHub issue
/// 24, 10 October 2026). So the menu leaves the window as it is until the
/// maker is done, and Omoio lets the game hear then (`portal_menu::close`,
/// `take_front`).
pub fn making() -> bool {
    MAKING.load(Ordering::SeqCst) > 0
}

/// Waits, before the figure maker opens, for an input settings window that
/// is closing because the menu closed just before: opened over it, the
/// maker would have that window's loop under its own, as above. Once Cemu
/// has hidden the window, its loop has ended. A window a menu still wants
/// open is left, as is one still there after `GONE_WAIT`.
fn input_settings_gone(pid: u32) {
    until(GONE_WAIT, || {
        quiet().deaf || !windows_of(pid).into_iter().any(|window| title(window) == INPUT_SETTINGS)
    });
}

/// The port asked of the portal server: its own in the build made for
/// Omoio, one above RPCS3's 28013 so the two never ask for the same one.
const DEFAULT_PORT: u16 = 28014;

/// The port written for the game now starting.
static PORT: AtomicU16 = AtomicU16::new(DEFAULT_PORT);

/// What the server answers `hello` with: its name and the version of its
/// commands. Any other answer, or none, is a Cemu without it.
const HELLO: &str = "ok cemu-portal 1";

/// How long after Cemu starts its server is asked for. The server starts
/// only with the game, once Cemu has put its portal in, and stops with it
/// (`cemu_initForGame` and `ShutdownTitle`, the build's CafeSystem.cpp lines
/// 382 to 384 and 993, commit 4607241, read 22 October 2026). In the first
/// minute of a game Cemu is still loading and building shaders
/// (`START_WAIT`), so it is given that long.
const SERVER_WAIT: Duration = Duration::from_secs(60);
/// Between tries at the server while the game starts.
const SERVER_RETRY: Duration = Duration::from_millis(250);

/// The port the portal server is to listen on for the game now starting,
/// for settings.xml (`tune_settings`): its own unless another program has
/// it, when Windows picks a free one, as for RPCS3's.
pub fn server_port(on: bool) -> u16 {
    let port = if on { free_port(DEFAULT_PORT) } else { DEFAULT_PORT };
    PORT.store(port, Ordering::Relaxed);
    port
}

fn ours() -> Server {
    Server::new(PORT.load(Ordering::Relaxed), SLOTS)
}

/// The Cemu whose start `prepare` saw, and whether its portal server
/// answered.
static ANSWERED: Mutex<Option<(u32, bool)>> = Mutex::new(None);

/// Cemu's characters with each figure's id and variant, as its figure maker
/// lists them. The server gives only the numbers, and the menu knows a
/// figure by the name Cemu's window would show for it.
static NAMES: Mutex<Vec<Character>> = Mutex::new(Vec::new());

/// Asks the Cemu started as `pid` for its portal server while its game
/// starts, on a thread of its own, until it answers or `SERVER_WAIT` is up.
///
/// Until it answers, the portal goes through Cemu's windows. That way round
/// the two agree: each row of the window only points at the portal slot
/// Cemu chose for the figure (`EmulatedUSBDeviceFrame.cpp` lines 283 to
/// 308, `m_skySlots`; `LoadSkylander`, `Skylander.cpp` lines 796 to 837), so
/// a figure put on through the window is on the portal the server reports,
/// if maybe in another slot. The window, though, would show none of the
/// figures the server put on, so once the server has answered it does every
/// errand it can for the rest of the game.
pub fn prepare(pid: u32) {
    *ANSWERED.lock().unwrap_or_else(PoisonError::into_inner) = Some((pid, false));
    std::thread::spawn(move || {
        let until = Instant::now() + SERVER_WAIT;
        let server = ours();
        loop {
            // Another Cemu started since has its own asking.
            if *ANSWERED.lock().unwrap_or_else(PoisonError::into_inner) != Some((pid, false)) {
                return;
            }
            if says_hello(&server) {
                let mut answered = ANSWERED.lock().unwrap_or_else(PoisonError::into_inner);
                if *answered == Some((pid, false)) {
                    *answered = Some((pid, true));
                }
                return;
            }
            if Instant::now() >= until {
                return;
            }
            std::thread::sleep(SERVER_RETRY);
        }
    });
}

fn says_hello(server: &Server) -> bool {
    server.ask("hello").is_ok_and(|lines| lines == [HELLO])
}

/// The portal server of `pid`, when it answered.
fn answered(pid: u32) -> Option<Server> {
    (*ANSWERED.lock().unwrap_or_else(PoisonError::into_inner) == Some((pid, true))).then(ours)
}

/// The server, when it answered and the figures it reports can be named.
/// Otherwise the window does the errand.
fn server(pid: u32) -> Option<Server> {
    answered(pid).filter(|_| !NAMES.lock().unwrap_or_else(PoisonError::into_inner).is_empty())
}

/// Which way the portal of `pid` was filled, for the log kept of the game,
/// or `None` when `prepare` never saw it start, as for a game with no
/// portal.
pub fn way_used(pid: u32) -> Option<&'static str> {
    match *ANSWERED.lock().unwrap_or_else(PoisonError::into_inner) {
        Some((seen, true)) if seen == pid => Some("Omoio: the portal menu used Cemu's portal server."),
        Some((seen, false)) if seen == pid => Some("Omoio: the portal menu used Cemu's Emulated USB Devices window."),
        _ => None,
    }
}

fn know_names(characters: Vec<Character>) {
    if !characters.is_empty() {
        *NAMES.lock().unwrap_or_else(PoisonError::into_inner) = characters;
    }
}

/// Takes Cemu's characters from the list the portal menu keeps of them
/// (`portal_menu::characters`, `{ "version", "characters" }`). A list kept
/// from another Cemu names the figures all the same, and the menu reads the
/// list again from a newer one the first time it opens.
pub fn know_kept_names(file: &Path) {
    #[derive(serde::Deserialize)]
    struct Kept {
        characters: Vec<Character>,
    }
    if let Some(kept) = std::fs::read_to_string(file).ok().and_then(|text| serde_json::from_str::<Kept>(&text).ok()) {
        know_names(kept.characters);
    }
}

/// Each slot's name as Cemu's window would show it, and then as the menu
/// knows it (`shown`). The window shows the list's name for the figure's id
/// and variant, else "Unknown (<id> <variant>)" (`FindSkylander`,
/// `Skylander.cpp` lines 984 to 994; `UpdateSkylanderEdits`,
/// `EmulatedUSBDeviceFrame.cpp` lines 439 to 456; v2.6), and the list is
/// the one the figure maker offers (lines 340 to 346, `GetListSkylanders`).
fn named(slots: &[Slot], names: &[Character]) -> Vec<String> {
    slots.iter().map(|slot| slot.map_or_else(String::new, |(id, variant)| name_of(names, id, variant))).collect()
}

fn name_of(names: &[Character], id: u16, variant: u16) -> String {
    let listed = names
        .iter()
        .find(|character| character.id == id && character.variant == variant)
        .map_or_else(|| format!("Unknown ({id} {variant})"), |character| character.name.clone());
    shown(&listed)
}

/// What the portal holds, as the server reports it.
fn on_portal(server: &Server) -> Result<Vec<String>, String> {
    let slots = server.status().map_err(|_| NOT_ANSWERING.to_string())?;
    Ok(named(&slots, &NAMES.lock().unwrap_or_else(PoisonError::into_inner)))
}

/// Puts the figure in `file` on in `slot` through the server. The window's
/// Load takes off what its row held first (`EmulatedUSBDeviceFrame.cpp`
/// lines 283 to 300), and the server's `load` wants the slot empty, as
/// RPCS3's does ("no free slot", the build's `SkylanderPortalIPC.cpp` lines
/// 136 to 138), so a full slot is emptied and the figure tried again.
fn load_through(server: &Server, slot: usize, file: &Path) -> Result<(), String> {
    let path = file.to_str().ok_or(LOAD_FAILED)?;
    let loaded = match server.load(slot, path) {
        Err(Failed::Refused(why)) if why == "no free slot" => server.remove(slot).and_then(|()| server.load(slot, path)),
        other => other,
    };
    match loaded {
        Ok(_) => Ok(()),
        Err(Failed::Refused(_)) => Err(LOAD_FAILED.to_string()),
        Err(_) => Err(NOT_ANSWERING.to_string()),
    }
}

/// Takes the figure in `slot` off through the server; a slot already empty
/// is as good.
fn clear_through(server: &Server, slot: usize) -> Result<(), String> {
    match server.remove(slot) {
        Ok(()) => Ok(()),
        Err(Failed::Refused(why)) if why == "slot empty" => Ok(()),
        Err(Failed::Refused(_)) => Err("Cemu couldn't take that figure off the portal.".to_string()),
        Err(_) => Err(NOT_ANSWERING.to_string()),
    }
}

/// Has the server make a figure of `character` into `file`, as the figure
/// maker would (`CreateSkylander`, `Skylander.cpp` lines 857 to 898), and
/// puts it on in `slot`, which the maker's window does itself once it has
/// made one (`EmulatedUSBDeviceFrame.cpp` lines 311 to 319). The server
/// won't make it over a file that is there (the build's
/// `SkylanderPortalIPC.cpp` lines 166 to 170), and none is: the menu picks
/// a name not taken (`portal_menu::new_figure`).
fn create_through(server: &Server, slot: usize, character: &Character, file: &Path) -> Result<(), String> {
    let path = file.to_str().ok_or(MAKE_FAILED)?;
    match server.ask(&format!("create {} {} {path}", character.id, character.variant)) {
        Ok(_) => load_through(server, slot, file),
        Err(Failed::Refused(_)) => Err(MAKE_FAILED.to_string()),
        Err(_) => Err(NOT_ANSWERING.to_string()),
    }
}

/// Whether the server has the game deaf, or hearing again, as asked.
fn hush_through(server: &Server, hushed: bool) -> bool {
    server.ask(if hushed { "hush on" } else { "hush off" }).is_ok()
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
///
/// Cemu opens the window whenever it gets to the menu command, however
/// late: a posted command reaches the frame with or without the menu bar on
/// the window (wxWidgets 3.2.5, which Cemu 2.6 is built with, `frame.cpp`,
/// `wxFrame::HandleCommand`, then `framecmn.cpp`, `ProcessCommand`), and each
/// one opens a window of its own (`MainWindow.cpp`, `OnOptionsInput`, v2.6;
/// both read 8 October 2026). One that opened after the menu had closed kept
/// the game deaf until the menu was opened and closed again (reported October
/// 2026). So the command is sent only when none is still on its way, and from
/// the first one on, Cemu's input settings windows are watched for as long as
/// it runs and kept as the menus want them. `ProcessCommand` drops a command
/// for a menu item that is turned off, so one that hasn't shown after
/// `ASK_AGAIN` is taken as lost and sent again.
///
/// A portal server that answered keeps the game deaf itself, with no
/// window: the build's `VPADRead` and `_KPADRead` skip every read while it
/// is hushed, as while the window is open (vpad.cpp line 267, padscore.cpp
/// line 474, commit 4607241). The window is used only if the server doesn't
/// do as asked. The two are apart, and `hush off` doesn't let the game hear
/// while the window is open, so letting it hear also closes any input
/// settings window, which there is when the game was made deaf before the
/// server answered.
pub fn hush(pid: u32, hushed: bool) -> Result<(), String> {
    let done = answered(pid).map(|server| hush_through(&server, hushed));
    if !hushed {
        let mut quiet = quiet();
        quiet.deaf = false;
        close_shown(pid, &mut quiet);
        return if done == Some(false) { Err(NOT_ANSWERING.to_string()) } else { Ok(()) };
    }
    if done == Some(true) {
        return Ok(());
    }
    quiet().deaf = true;
    // A window told to close lets the game hear only once Cemu deletes it,
    // the next time it has nothing else to do, even if a new one has opened
    // by then (`InputSettings2.cpp`, `~InputSettings2`, v2.6; wxWidgets
    // 3.2.5, `toplvcmn.cpp`, `wxTopLevelWindowBase::Destroy`; read 8 October
    // 2026). So a new one is asked for once the old one has gone.
    until(GONE_WAIT, || quiet().settled(Instant::now(), exists));
    let ask = {
        let quiet = quiet();
        open_input(pid, &quiet).is_none() && quiet.may_ask(pid, Instant::now())
    };
    if ask {
        let main = main_window(pid).ok_or("Cemu isn't answering.")?;
        let input = command_when_ready(pid, INPUT_SETTINGS).ok_or("Cemu isn't ready yet.")?;
        let mut quiet = quiet();
        if !quiet.deaf {
            return Ok(());
        }
        unsafe { PostMessageW(Some(main), WM_COMMAND, WPARAM(input as usize), LPARAM(0)) }
            .map_err(|_| "Cemu isn't answering.".to_string())?;
        quiet.asked = Some((pid, Instant::now()));
        watch(pid, &mut quiet);
    }
    keep_when_shown(pid, START_WAIT)
}

/// How long a window told to close is waited for, at most, before a new
/// one is asked for.
const GONE_WAIT: Duration = Duration::from_secs(2);
/// How long a command whose window hasn't shown is waited for before it is
/// sent again: three times `START_WAIT`, the longest Cemu has been seen to
/// take.
const ASK_AGAIN: Duration = Duration::from_secs(60);
/// How often Cemu's input settings windows are looked at: soon enough that
/// one shown late is barely seen, seldom enough to cost nothing while the
/// game runs.
const LATE_LOOK: Duration = Duration::from_millis(250);

/// What the menus want of Cemu's input settings window, kept between one
/// call to `hush` and the next.
struct Quiet {
    /// Whether the game is to be deaf, as last asked.
    deaf: bool,
    /// The Cemu sent the command that opens the window, and when, until the
    /// window shows.
    asked: Option<(u32, Instant)>,
    /// Windows told to close, and when, until they have gone.
    closing: Vec<(isize, Instant)>,
    /// The Cemu whose windows `watch_windows` looks after.
    watching: Option<u32>,
}

impl Quiet {
    /// Whether the command may be sent to `pid`. Not while an earlier one
    /// is on its way, or a second window would open over the first, unless
    /// that one is taken as lost.
    fn may_ask(&self, pid: u32, now: Instant) -> bool {
        match self.asked {
            Some((asked, when)) if asked == pid => now.duration_since(when) >= ASK_AGAIN,
            _ => true,
        }
    }

    fn answered(&mut self, pid: u32) {
        if self.asked.is_some_and(|(asked, _)| asked == pid) {
            self.asked = None;
        }
    }

    fn told_to_close(&self, window: isize) -> bool {
        self.closing.iter().any(|&(told, _)| told == window)
    }

    /// Whether every window told to close in the last `GONE_WAIT` has gone,
    /// forgetting those that have. One still there after that isn't waited
    /// for again.
    fn settled(&mut self, now: Instant, exists: impl Fn(isize) -> bool) -> bool {
        self.closing.retain(|&(window, _)| exists(window));
        self.closing.iter().all(|&(_, when)| now.duration_since(when) >= GONE_WAIT)
    }
}

static QUIET: Mutex<Quiet> = Mutex::new(Quiet {
    deaf: false,
    asked: None,
    closing: Vec::new(),
    watching: None,
});

fn quiet() -> MutexGuard<'static, Quiet> {
    QUIET.lock().unwrap_or_else(PoisonError::into_inner)
}

fn exists(window: isize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::IsWindow;
    unsafe { IsWindow(Some(HWND(window as *mut _))) }.as_bool()
}

/// The input settings window showing, unless it was told to close.
fn open_input(pid: u32, quiet: &Quiet) -> Option<HWND> {
    windows_of(pid)
        .into_iter()
        .find(|&w| title(w) == INPUT_SETTINGS && !quiet.told_to_close(w.0 as isize))
}

/// Waits up to `limit` for the input settings window and puts it out of
/// sight, as `arrives` does. Stops waiting if the game may hear again
/// meanwhile; `watch_windows` then sees to the window.
fn keep_when_shown(pid: u32, limit: Duration) -> Result<(), String> {
    let until = Instant::now() + limit;
    loop {
        let mut quiet = quiet();
        if !quiet.deaf {
            return Ok(());
        }
        if let Some(window) = open_input(pid, &quiet) {
            quiet.answered(pid);
            drop(quiet);
            out_of_sight(window);
            return Ok(());
        }
        drop(quiet);
        if Instant::now() >= until {
            return Err("Cemu's input settings didn't open.".to_string());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Tells each input settings window showing to close, should there be more
/// than one.
fn close_shown(pid: u32, quiet: &mut Quiet) {
    let now = Instant::now();
    while let Some(window) = open_input(pid, quiet) {
        close(window);
        quiet.closing.push((window.0 as isize, now));
        quiet.answered(pid);
    }
}

/// Has a thread look after `pid`'s input settings windows, once per Cemu.
fn watch(pid: u32, quiet: &mut Quiet) {
    if quiet.watching != Some(pid) {
        quiet.watching = Some(pid);
        std::thread::spawn(move || watch_windows(pid));
    }
}

/// Puts an input settings window out of sight while a menu wants the game
/// deaf, and closes it if not, whenever Cemu shows one, until Cemu has gone
/// or another Cemu is being looked after.
fn watch_windows(pid: u32) {
    loop {
        std::thread::sleep(LATE_LOOK);
        let mut quiet = quiet();
        if quiet.watching != Some(pid) {
            return;
        }
        if main_window(pid).is_none() {
            quiet.answered(pid);
            quiet.watching = None;
            return;
        }
        let Some(window) = open_input(pid, &quiet) else {
            continue;
        };
        if quiet.deaf {
            quiet.answered(pid);
            drop(quiet);
            out_of_sight(window);
        } else {
            close_shown(pid, &mut quiet);
        }
    }
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

/// Sends the command at least once, however little of `limit` is left.
fn show_devices(pid: u32, main: HWND, devices: u32, limit: Duration) -> Option<HWND> {
    let until = Instant::now() + limit;
    loop {
        let _ = unsafe { PostMessageW(Some(main), WM_COMMAND, WPARAM(devices as usize), LPARAM(0)) };
        if let Some(window) = arrives(pid, WINDOW, RESEND) {
            return Some(window);
        }
        if Instant::now() >= until {
            return None;
        }
    }
}

/// Opens the portal window as the portal menu opens, before Cemu's input
/// settings window: for a good while after that window opens, Cemu answers
/// no menu command at all, and the portal menu's first read waited twenty
/// seconds and failed (6 October 2026). The menu covers the screen as it
/// opens, and the window stays open after.
///
/// Cemu answers the commands it is sent in the order they came: they are
/// all posted to its main window, and a thread's posted messages wait in one
/// first-in, first-out queue (Microsoft's "About Messages and Message
/// Queues"). So one sent here, even if it isn't answered within
/// `READY_WAIT`, is answered before the input settings window that `hush`
/// asks for next. It is sent whenever the portal window isn't showing,
/// also while an input settings window still is, such as one Big Picture
/// asked for that is closing, and the command is waited for when Omoio
/// hasn't read it from the menu bar yet. Neither was so until now, and then
/// the first read sent it only after the input settings window, the order
/// that failed (reported on Discord, 11 October 2026: "Cemu's portal didn't
/// open", the first time only). Nothing in Cemu 2.6 or wxWidgets 3.2.5 drops
/// a menu command while that window is up: its modal loop hands on every
/// message (`evtloop.cpp`, `wxGUIEventLoop::PreProcessMessage`, which only
/// eats them around a `wxSafeYield`), and the frame drops one only for a
/// menu item that is turned off (`framecmn.cpp`,
/// `wxFrameBase::ProcessCommand`), which Emulated USB Devices never is
/// (`MainWindow.cpp`, `RecreateMenu`, v2.6; all read 14 October 2026). So a
/// command sent then waits rather than being lost, and what counts is that
/// the portal window's is ahead of it.
///
/// With the portal server doing the errands, there is no window to open.
pub fn ready(pid: u32) {
    if server(pid).is_some() {
        return;
    }
    let _turn = turn();
    if windows_of(pid).into_iter().any(|w| title(w) == WINDOW) {
        return;
    }
    let until = Instant::now() + READY_WAIT;
    let devices = loop {
        tidy(pid);
        if let Some(found) = command(pid, WINDOW) {
            break Some(found);
        }
        if Instant::now() >= until {
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if let (Some(main), Some(devices)) = (main_window(pid), devices) {
        let _ = show_devices(pid, main, devices, until.saturating_duration_since(Instant::now()));
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
/// have "Unknown (212 12302)" (`FindSkylander`, v2.6), which is how each
/// figure made with the game's own variant in place of Cemu's wrong one
/// shows, and each Sensei and Creation Crystal Omoio had made.
fn shown(name: &str) -> String {
    unknown(name)
        .and_then(|(id, variant)| figures::unlisted_name(id, variant))
        .unwrap_or_else(|| figures::named(name))
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
    if let Some(server) = server(pid) {
        return on_portal(&server);
    }
    read(open(pid)?)
}

/// Puts the figure in `file` on the portal in `slot`, counted from 0, and
/// returns what the portal holds afterwards.
pub fn load(pid: u32, slot: usize, file: &Path) -> Result<Vec<String>, String> {
    check(slot)?;
    let _turn = turn();
    if let Some(server) = server(pid) {
        load_through(&server, slot, file)?;
        return on_portal(&server);
    }
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
            LOAD_FAILED.to_string()
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
    if let Some(server) = server(pid) {
        clear_through(&server, slot)?;
        return on_portal(&server);
    }
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
    input_settings_gone(pid);
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
/// again without making anything. The portal server has no list, so this
/// is read from the window with or without it, and names the figures the
/// server reports from then on.
pub fn characters(pid: u32) -> Result<Vec<Character>, String> {
    let _making = Making::new();
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
        know_names(found.clone());
        Ok(found)
    }
}

/// Has Cemu's figure maker make a figure of `character` into `file`. Cemu
/// then puts it on the portal in `slot` itself. Returns what the portal
/// holds afterwards.
pub fn create(pid: u32, slot: usize, character: &Character, file: &Path) -> Result<Vec<String>, String> {
    check(slot)?;
    if let Some(server) = server(pid) {
        let _turn = turn();
        create_through(&server, slot, character, file)?;
        return on_portal(&server);
    }
    let _making = Making::new();
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
            MAKE_FAILED.to_string()
        } else {
            format!("Cemu couldn't make that figure: {said}")
        });
    }
    read(window)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// A few lines of Cemu's list (`Skylander.cpp` lines 64, 65, 180, 229,
    /// 308 and 484, v2.6), as its figure maker shows them.
    fn list() -> Vec<Character> {
        [
            ("Spyro", 16, 0x0000),
            ("Series 2 Spyro", 16, 0x1801),
            ("Tempest Timer", 212, 0x300D),
            ("Dragonâ€™s Peak", 300, 0x0000),
            ("King Pen", 601, 0x0000),
            ("Hammer Slam Bowser (Nintendo Only)", 3424, 0x0000),
        ]
        .map(|(name, id, variant)| Character { name: name.to_string(), id, variant })
        .to_vec()
    }

    #[test]
    fn a_figure_the_server_reports_is_named_as_cemus_window_names_it() {
        let list = list();
        assert_eq!(name_of(&list, 16, 0), "Spyro");
        assert_eq!(name_of(&list, 16, 0x1801), "Series 2 Spyro");
        assert_eq!(name_of(&list, 300, 0), "Dragon’s Peak");
        assert_eq!(name_of(&list, 3424, 0), "Hammer Slam Bowser");
        // Made with the game's own variants, which the list doesn't have:
        // the window says "Unknown (212 12302)" and the menu "Tempest Timer".
        assert_eq!(name_of(&list, 212, 0x300E), "Tempest Timer");
        assert_eq!(name_of(&list, 601, 0x5000), "King Pen");
        assert_eq!(name_of(&list, 680, 21000), "Magic Lantern Crystal");
        assert_eq!(name_of(&list, 999, 0), "Unknown (999 0)");
        assert_eq!(named(&[None, Some((16, 0x1801))], &list), ["", "Series 2 Spyro"]);
    }

    #[test]
    fn the_names_come_from_the_list_the_menu_kept() {
        let file = std::env::temp_dir().join(format!("omoio-characters-wiiu-{}.json", std::process::id()));
        let kept = r#"{"version":"2.6","characters":[{"name":"Spyro","id":16,"variant":0}]}"#;
        std::fs::write(&file, kept).unwrap();
        know_kept_names(&file);
        let _ = std::fs::remove_file(&file);
        let names = NAMES.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(named(&[Some((16, 0)), None], &names), ["Spyro", ""]);
    }

    /// A server that reads one line, gives `answer` and closes.
    fn answering(answer: &'static str) -> Server {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            BufReader::new(&stream).read_line(&mut String::new()).unwrap();
            (&stream).write_all(answer.as_bytes()).unwrap();
        });
        Server::new(port, SLOTS)
    }

    fn nobody() -> Server {
        Server::new(TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port(), SLOTS)
    }

    #[test]
    fn only_the_server_of_omoios_build_answers_hello() {
        assert!(says_hello(&answering("ok cemu-portal 1\n")));
        assert!(!says_hello(&answering("ok cemu-portal 2\n")));
        assert!(!says_hello(&answering("ok\n")));
        // RPCS3's server, which has no `hello` (`SkylanderPortalIPC.cpp` line 182).
        assert!(!says_hello(&answering("error unknown command\n")));
        // Cemu 2.6, which has no server at all.
        assert!(!says_hello(&nobody()));
    }

    /// A server that answers as the one in the build of Cemu made for Omoio
    /// does (`ProcessCommand`, its `SkylanderPortalIPC.cpp` lines 189 to
    /// 252, commit 4607241), on a portal of sixteen slots, with figures made
    /// and read where Cemu makes and reads a figure's id and variant
    /// (`CreateSkylander`, `Skylander.cpp` lines 857 to 898;
    /// `LoadSkylanderPath`, `EmulatedUSBDeviceFrame.cpp` lines 302 and 303).
    /// It takes `connections` commands, one to a connection, and each line it
    /// read comes back through `heard`.
    fn fork_like(connections: usize) -> (Server, mpsc::Receiver<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tell, heard) = mpsc::channel();
        std::thread::spawn(move || {
            let mut portal: [Slot; SLOTS] = [None; SLOTS];
            for _ in 0..connections {
                let (stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let line = line.trim_end().to_string();
                let answer = fork_answer(&mut portal, &line);
                let _ = tell.send(line);
                (&stream).write_all(answer.as_bytes()).unwrap();
            }
        });
        (Server::new(port, SLOTS), heard)
    }

    fn fork_answer(portal: &mut [Slot; SLOTS], line: &str) -> String {
        let (command, rest) = line.split_once(' ').unwrap_or((line, ""));
        match command {
            "hello" => "ok cemu-portal 1\n".to_string(),
            "status" => {
                let mut text: String = portal
                    .iter()
                    .enumerate()
                    .map(|(number, slot)| match slot {
                        Some((id, variant)) => format!("slot{number} loaded {id} {variant}\n"),
                        None => format!("slot{number} empty\n"),
                    })
                    .collect();
                text.push_str("ok\n");
                text
            }
            "load" => {
                let (slot, path) = rest.split_once(' ').unwrap();
                let Ok(data) = std::fs::read(path) else {
                    return "error cannot open file\n".to_string();
                };
                if data.len() != 0x400 {
                    return "error not a skylander file\n".to_string();
                }
                let id = u16::from_le_bytes([data[0x10], data[0x11]]);
                let variant = u16::from_le_bytes([data[0x1C], data[0x1D]]);
                let free = match slot {
                    "-1" => portal.iter().position(Option::is_none),
                    slot => Some(slot.parse::<usize>().unwrap()).filter(|&slot| portal[slot].is_none()),
                };
                let Some(slot) = free else {
                    return "error no free slot\n".to_string();
                };
                portal[slot] = Some((id, variant));
                format!("ok {slot}\n")
            }
            "remove" => match portal.get_mut(rest.parse::<usize>().unwrap()).map(Option::take) {
                Some(Some(_)) => "ok\n".to_string(),
                Some(None) => "error slot empty\n".to_string(),
                None => "error invalid slot\n".to_string(),
            },
            "create" => {
                let words: Vec<&str> = rest.splitn(3, ' ').collect();
                let mut data = vec![0u8; 0x400];
                data[0x10..0x12].copy_from_slice(&words[0].parse::<u16>().unwrap().to_le_bytes());
                data[0x1C..0x1E].copy_from_slice(&words[1].parse::<u16>().unwrap().to_le_bytes());
                std::fs::write(words[2], data).unwrap();
                "ok\n".to_string()
            }
            "hush" if rest == "on" || rest == "off" => "ok\n".to_string(),
            _ => "error unknown command\n".to_string(),
        }
    }

    #[test]
    fn figures_go_on_and_off_through_a_server_like_the_forks() {
        let dir = std::env::temp_dir().join(format!("omoio-cemu-portal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (spyro, king_pen) = (dir.join("Spyro.sky"), dir.join("King Pen.sky"));
        let (server, heard) = fork_like(12);
        assert!(says_hello(&server));
        create_through(&server, 3, &Character { name: "Spyro".into(), id: 16, variant: 0 }, &spyro).unwrap();
        // Made where Spyro is, who comes off first, as the window's Load
        // takes off what its row held.
        let king = Character { name: "King Pen".into(), id: 601, variant: 0x5000 };
        create_through(&server, 3, &king, &king_pen).unwrap();
        let slots = server.status().unwrap();
        assert_eq!(slots.len(), 16);
        assert_eq!(slots[3], Some((601, 0x5000)));
        assert_eq!(slots.iter().filter(|slot| slot.is_some()).count(), 1);
        assert_eq!(named(&slots, &list())[3], "King Pen");
        clear_through(&server, 3).unwrap();
        clear_through(&server, 3).unwrap();
        assert!(hush_through(&server, true));
        assert!(hush_through(&server, false));

        let heard: Vec<String> = heard.iter().take(12).collect();
        let (spyro, king_pen) = (spyro.to_str().unwrap(), king_pen.to_str().unwrap());
        let said = [
            "hello".to_string(),
            format!("create 16 0 {spyro}"),
            format!("load 3 {spyro}"),
            format!("create 601 20480 {king_pen}"),
            format!("load 3 {king_pen}"),
            "remove 3".to_string(),
            format!("load 3 {king_pen}"),
            "status".to_string(),
            "remove 3".to_string(),
            "remove 3".to_string(),
            "hush on".to_string(),
            "hush off".to_string(),
        ];
        assert_eq!(heard, said);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_the_server_turns_down_or_doesnt_answer_is_said_plainly() {
        let dir = std::env::temp_dir().join(format!("omoio-cemu-refused-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let notes = dir.join("notes.txt");
        std::fs::write(&notes, "not a figure").unwrap();
        let (server, _) = fork_like(2);
        assert_eq!(load_through(&server, 0, &notes), Err(LOAD_FAILED.to_string()));
        assert_eq!(clear_through(&server, 16), Err("Cemu couldn't take that figure off the portal.".to_string()));
        let gone = nobody();
        assert_eq!(load_through(&gone, 0, &notes), Err(NOT_ANSWERING.to_string()));
        assert_eq!(clear_through(&gone, 0), Err(NOT_ANSWERING.to_string()));
        assert!(!hush_through(&gone, true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_slot_reads_as_the_menu_names_it() {
        assert_eq!(shown("Spyro"), "Spyro");
        assert_eq!(shown("Dragonâ€™s Peak"), "Dragon’s Peak");
        assert_eq!(shown("Hammer Slam Bowser (Nintendo Only)"), "Hammer Slam Bowser");
        assert_eq!(shown("Dark Clown Cruiser (Nintendo Only)"), "Dark Clown Cruiser");
        // Tempest Timer made with Trap Team's own variant, 0x300E.
        assert_eq!(shown("Unknown (212 12302)"), "Tempest Timer");
        assert_eq!(shown("Unknown (219 12309)"), "Shining Ship");
        // King Pen made with a real Sensei's variant, 0x5000, and a crystal.
        assert_eq!(shown("Unknown (601 20480)"), "King Pen");
        assert_eq!(shown("Unknown (680 21000)"), "Magic Lantern Crystal");
        assert_eq!(shown("King Pen"), "King Pen");
        // A figure Cemu doesn't know stays as Cemu put it.
        assert_eq!(shown("Unknown (999 0)"), "Unknown (999 0)");
        assert_eq!(unknown("Unknown Spyro"), None);
    }

    #[test]
    fn the_figure_maker_counts_as_at_work_until_its_errand_ends() {
        let before = making();
        {
            let _one = Making::new();
            let _two = Making::new();
            assert!(making());
        }
        assert_eq!(making(), before);
    }

    fn quiet_with(closing: Vec<(isize, Instant)>) -> Quiet {
        Quiet { deaf: true, asked: None, closing, watching: None }
    }

    #[test]
    fn the_input_settings_command_goes_once_until_its_window_shows() {
        let now = Instant::now();
        let mut quiet = quiet_with(Vec::new());
        assert!(quiet.may_ask(7, now));
        quiet.asked = Some((7, now));
        assert!(!quiet.may_ask(7, now));
        // A Cemu started since is asked afresh, and its window doesn't
        // answer for the other one.
        assert!(quiet.may_ask(8, now));
        quiet.answered(8);
        assert!(!quiet.may_ask(7, now));
        quiet.answered(7);
        assert!(quiet.may_ask(7, now));
    }

    #[test]
    fn a_command_whose_window_never_showed_is_sent_again_in_the_end() {
        let sent = Instant::now();
        let mut quiet = quiet_with(Vec::new());
        quiet.asked = Some((7, sent));
        assert!(!quiet.may_ask(7, sent + START_WAIT));
        assert!(quiet.may_ask(7, sent + ASK_AGAIN), "taken as lost, so the game can be hushed again");
    }

    #[test]
    fn a_window_told_to_close_is_waited_for_a_while_and_never_kept() {
        let told = Instant::now();
        let mut quiet = quiet_with(vec![(1, told)]);
        assert!(!quiet.settled(told, |_| true));
        // Still there after its time: not waited for again, nor kept.
        assert!(quiet.settled(told + GONE_WAIT, |_| true));
        assert!(quiet.told_to_close(1));
        assert!(!quiet.told_to_close(2));
        // Gone, and forgotten.
        assert!(quiet.settled(told, |_| false));
        assert!(!quiet.told_to_close(1));
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
