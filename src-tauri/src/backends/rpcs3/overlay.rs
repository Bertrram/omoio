//! Putting the game picture inside Omoio's window.
//!
//! RPCS3 stays its own process, as it must. What changes is where its window
//! sits: stripped of its frame, owned by Omoio, and parked exactly over the
//! content area, so playing a game looks like part of the app rather than a
//! second program appearing.
//!
//! Reparenting the window into ours was tried first and does not work: Tauri
//! draws through WebView2, which composites over native child windows whatever
//! their z-order, so the game ran but stayed invisible. Owning the window
//! instead keeps it above the webview and out of the taskbar.

use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_LMENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindow, GetWindowLongPtrW, GetWindowThreadProcessId, IsWindow,
    IsWindowVisible, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWLP_HWNDPARENT, GWL_STYLE,
    GW_OWNER, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_SHOW, SW_SHOWNA,
    WS_CAPTION, WS_POPUP, WS_SYSMENU, WS_THICKFRAME, WS_VISIBLE,
};

struct Search<'a> {
    pid: u32,
    is_game: &'a dyn Fn(&str) -> bool,
    found: Option<HWND>,
}

unsafe extern "system" fn visit(window: HWND, state: LPARAM) -> BOOL {
    let search = unsafe { &mut *(state.0 as *mut Search) };
    let mut owner = 0u32;
    unsafe { GetWindowThreadProcessId(window, Some(&mut owner)) };

    if owner == search.pid && unsafe { IsWindowVisible(window) }.as_bool() && is_game_window(window, search.is_game) {
        search.found = Some(window);
        return BOOL(0); // stop at the first one
    }
    BOOL(1)
}

/// Whether a window can be the game's. An emulator's dialogs have an owner;
/// `is_game` tells its game window from its other windows without one. With
/// RPCS3's interface showing, as for a Skylanders game's portal, those are
/// its main window, shown for a moment before it is hidden, and on a game's
/// first start a progress window that comes and goes before the game's.
/// Taking that one for the game, Omoio saw it close and quit RPCS3 under the
/// game.
fn is_game_window(window: HWND, is_game: &dyn Fn(&str) -> bool) -> bool {
    let owned = unsafe { GetWindow(window, GW_OWNER) }.is_ok();
    !owned && is_game(&crate::backends::qt::title(window))
}

/// The emulator's game window for a given process. Called on a timer while
/// the game boots, because the window only appears once it has something to
/// show.
pub fn find_window(pid: u32, is_game: &dyn Fn(&str) -> bool) -> Option<isize> {
    let mut search = Search { pid, is_game, found: None };
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize)) };
    search.found.map(|hwnd| hwnd.0 as isize)
}

/// Takes the frame off the game window and makes Omoio its owner, so it rides
/// above Omoio, minimises with it, and never gets its own taskbar button.
pub fn attach(game: isize, host: isize) {
    let game = HWND(game as *mut _);
    unsafe {
        let style = GetWindowLongPtrW(game, GWL_STYLE);
        let stripped = style
            & !((WS_CAPTION.0 | WS_THICKFRAME.0 | WS_SYSMENU.0) as isize)
            | ((WS_POPUP.0 | WS_VISIBLE.0) as isize);
        SetWindowLongPtrW(game, GWL_STYLE, stripped);
        SetWindowLongPtrW(game, GWLP_HWNDPARENT, host);
    }
}

/// Takes the game window back if it slipped out of Omoio's: RPCS3 puts its
/// frame back when the game leaves RPCS3's own fullscreen, and the game then
/// sat in a window of its own over Omoio. Left as it is when it is still in.
pub fn keep(game: isize, host: isize) {
    let window = HWND(game as *mut _);
    let frame = (WS_CAPTION.0 | WS_THICKFRAME.0 | WS_SYSMENU.0) as isize;
    unsafe {
        let style = GetWindowLongPtrW(window, GWL_STYLE);
        if style & frame != 0 {
            SetWindowLongPtrW(window, GWL_STYLE, style & !frame | WS_POPUP.0 as isize);
            let _ = SetWindowPos(
                window,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        if GetWindowLongPtrW(window, GWLP_HWNDPARENT) != host {
            SetWindowLongPtrW(window, GWLP_HWNDPARENT, host);
        }
    }
}

/// Whether the window is still there.
pub fn exists(game: isize) -> bool {
    unsafe { IsWindow(Some(HWND(game as *mut _))) }.as_bool()
}

/// Asks whether F11 went down since the last time we asked, which is the low
/// bit of GetAsyncKeyState.
///
/// The obvious reading, "is the key down right now", does not work here. We
/// look every 400ms and a tap lasts around a hundred, so most presses fall
/// between two looks and are never seen. That is not a rare miss: it means the
/// only way out of a picture covering the screen is to hold the key down and
/// hope, which is exactly how it behaved. The low bit is remembered by Windows
/// until read, so a tap between polls still counts.
///
/// The bit is per-process and nothing else here reads this key, so one press
/// gives exactly one event.
pub fn fullscreen_key_pressed() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_F11};
    unsafe { GetAsyncKeyState(VK_F11.0 as i32) as u16 & 0x0001 != 0 }
}

/// Whether the game or Omoio is the window being used.
///
/// The key is read globally, because the game holds the keyboard and Omoio
/// never sees a key of its own. Without this check F11 in a browser would drag
/// the game to fullscreen behind it, which is worse than not having the
/// shortcut at all.
pub fn ours_has_focus(game: isize, host: isize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, GetForegroundWindow, GA_ROOT};

    let front = unsafe { GetForegroundWindow() };
    if front.0.is_null() {
        return false;
    }
    let root = unsafe { GetAncestor(front, GA_ROOT) };
    let front = front.0 as isize;
    let root = root.0 as isize;
    front == game || front == host || root == game || root == host
}

/// Takes the game picture off the screen without touching the game. Hiding
/// the window in front hands the keyboard on to another window, which
/// Windows picks ("ShowWindow function", SW_HIDE: "Hides the window and
/// activates another window"). Big Picture gives its own window the front
/// straight after.
pub fn hide(game: isize) {
    unsafe {
        let _ = ShowWindow(HWND(game as *mut _), SW_HIDE);
    }
}

/// Puts the game picture back. With `activate` it takes the keyboard as
/// well, which Omoio may hand over because its own window is the one in
/// front at that moment.
pub fn show(game: isize, activate: bool) {
    unsafe {
        let _ = ShowWindow(HWND(game as *mut _), if activate { SW_SHOW } else { SW_SHOWNA });
    }
    if activate {
        focus(game);
    }
}

/// Gives the game the keyboard, and with it the pad: RPCS3 reads the pad
/// only while its window is the one in front. Windows allows this only from
/// the program in front, so it is asked for only when that is Omoio.
pub fn focus(game: isize) {
    bring_to_front(game);
}

/// Puts `window` in front, and says whether it is in front afterwards.
///
/// SetForegroundWindow works only for the program in front, or one that
/// had the last keyboard or mouse input, among a few others, and says so
/// when it is refused ("SetForegroundWindow function", Remarks, Microsoft
/// Learn, read 14 October 2026). A game played with a pad alone, as in Big
/// Picture, gives Omoio no such input: Omoio reads the pad itself, and a
/// pad's state is asked for (XInputGetState), not sent to any window. So
/// once something else is in front, a plain call fails, and before
/// 14 October 2026 that went unnoticed. "The system automatically enables
/// calls to SetForegroundWindow if the user presses the ALT key"
/// ("LockSetForegroundWindow function", Remarks), so a press of Alt goes
/// first when the plain call is refused. It goes to the window in front at
/// that moment, which callers allow to be only Omoio's, the emulator's own
/// main window or none; one with a menu bar may mark its menu for that
/// moment (on the test list of 14 October 2026). Tao
/// does the same to bring Tauri's windows forward (`force_window_active`,
/// platform_impl/windows/window.rs, tao 0.35.3), and so do the emulators'
/// own hand-backs. Callers decide first whether the front may be taken at
/// all: never from another program.
pub fn bring_to_front(window: isize) -> bool {
    let hwnd = HWND(window as *mut _);
    if unsafe { SetForegroundWindow(hwnd) }.as_bool() && front() == window {
        return true;
    }
    if !may_press_alt(&mut PRESSED.lock().unwrap_or_else(std::sync::PoisonError::into_inner), Instant::now()) {
        return false;
    }
    let alt = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_LMENU, dwFlags: flags, ..Default::default() } },
    };
    let presses = [alt(KEYBD_EVENT_FLAGS(0)), alt(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(&presses, std::mem::size_of::<INPUT>() as i32);
        let _ = SetForegroundWindow(hwnd);
    }
    front() == window
}

/// At most this many presses of Alt in `PRESS_SPELL`, whoever asks. Each
/// hand-over watches only briefly, but Omoio's window coming to the front
/// starts one too, so something that kept taking the front from the game
/// would otherwise be answered with Alt after Alt for as long as it went on.
const PRESSES: usize = 6;
const PRESS_SPELL: Duration = Duration::from_secs(10);

static PRESSED: Mutex<Vec<Instant>> = Mutex::new(Vec::new());

/// Whether another press of Alt fits in the spell, and if so counts it.
fn may_press_alt(pressed: &mut Vec<Instant>, now: Instant) -> bool {
    pressed.retain(|&at| now.duration_since(at) < PRESS_SPELL);
    if pressed.len() >= PRESSES {
        return false;
    }
    pressed.push(now);
    true
}

/// The window in front, or 0 for none. Windows has none for a moment while
/// one window loses the front to another ("GetForegroundWindow function",
/// Return value, Microsoft Learn).
pub fn front() -> isize {
    unsafe { GetForegroundWindow() }.0 as isize
}

/// The process a window belongs to, 0 for none.
pub fn process_of(window: isize) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(HWND(window as *mut _), Some(&mut pid)) };
    pid
}

/// Whether another window owns this one, as a program owns its dialogs and
/// messages.
pub fn has_owner(window: isize) -> bool {
    unsafe { GetWindow(HWND(window as *mut _), GW_OWNER) }.is_ok()
}

/// Whether the window in front belongs to one of these processes. Asked by
/// process rather than by window, so the portal menu and the emulator's own
/// dialogs count as Omoio's too.
pub fn front_belongs_to(processes: &[u32]) -> bool {
    let front = unsafe { GetForegroundWindow() };
    if front.0.is_null() {
        return false;
    }
    let mut owner = 0u32;
    unsafe { GetWindowThreadProcessId(front, Some(&mut owner)) };
    processes.contains(&owner)
}

pub fn place(game: isize, x: i32, y: i32, width: i32, height: i32) {
    if width <= 0 || height <= 0 {
        return;
    }
    unsafe {
        // Not activated and not reordered: moving the picture should never
        // steal focus from whatever the user is doing.
        let _ = SetWindowPos(
            HWND(game as *mut _),
            None,
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alt_is_pressed_at_most_six_times_in_ten_seconds() {
        let start = Instant::now();
        let mut pressed = Vec::new();
        for _ in 0..PRESSES {
            assert!(may_press_alt(&mut pressed, start));
        }
        assert!(!may_press_alt(&mut pressed, start + Duration::from_secs(9)));
        // Once the first ones are ten seconds old there is room again.
        assert!(may_press_alt(&mut pressed, start + PRESS_SPELL));
        assert_eq!(pressed.len(), 1);
    }
}
