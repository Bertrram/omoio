//! Reads the keyboard and mouse while another program, Cemu, is in front,
//! for the pad Omoio plays a keyboard-and-mouse player through (dsu.rs).
//!
//! Windows' Raw Input hands a window every key and mouse report, and with
//! `RIDEV_INPUTSINK` it does so "even when the caller is not in the
//! foreground", as long as the window is named as the target
//! ("RAWINPUTDEVICE structure", Microsoft Learn, read 20 October 2026). No
//! driver and no administrator rights. The window is a message-only one on
//! a thread of its own, as SDL's raw input thread has it, which registers
//! its keyboard with `RIDEV_INPUTSINK` for the same reason
//! (`WIN_RawInputThread`, src/video/windows/SDL_windowsrawinput.c, SDL
//! commit 089b862, read 20 October 2026).
//!
//! Keys come by scan code, which is where the key sits, so the key the
//! player chose on the Controller screen is the one that works on any
//! keyboard layout (core/keys.rs). Nothing is kept but what is held now and
//! how far the mouse moved, and the reader runs only while a game is served.
//!
//! Windows lets one window in a process have a kind of device's raw input,
//! the last one signed up ("RegisterRawInputDevices function", Remarks),
//! and the window toolkit under Tauri signs its own up as it starts
//! (`register_all_mice_and_keyboards_for_raw_input`, tao 0.35.3,
//! platform_impl/windows/event_loop.rs line 204), for device events Tauri
//! never passes on (tauri-runtime-wry 2.11.4 has no handler for them). So
//! the reader takes it over while it runs and gives it up after, and Omoio
//! loses nothing.
//!
//! The same thread keeps a window a few pixels big for the cursor to rest on
//! while the player aims (`rest_at`, used by dsu.rs), since a window's
//! messages go to the thread that made it.

use crate::keyboard_pad::Snapshot;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, MAPVK_VK_TO_VSC_EX};
use windows::Win32::UI::Input::{
    GetRawInputData, RegisterRawInputDevices, HRAWINPUT, MOUSE_MOVE_ABSOLUTE, MOUSE_VIRTUAL_DESKTOP, RAWINPUT,
    RAWINPUTDEVICE, RAWINPUTHEADER, RAWKEYBOARD, RAWMOUSE, RIDEV_INPUTSINK, RIDEV_REMOVE, RID_INPUT,
    RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, GetSystemMetrics,
    PostThreadMessageW, RegisterClassW, SetCursor, SetLayeredWindowAttributes, SetWindowPos, WindowFromPoint,
    HWND_MESSAGE, HWND_TOPMOST, LWA_ALPHA, MA_NOACTIVATE, MSG, RI_KEY_BREAK, RI_KEY_E0, RI_KEY_E1,
    RI_MOUSE_BUTTON_1_DOWN, RI_MOUSE_BUTTON_1_UP, RI_MOUSE_BUTTON_2_DOWN, RI_MOUSE_BUTTON_2_UP, RI_MOUSE_BUTTON_3_DOWN,
    RI_MOUSE_BUTTON_3_UP, RI_MOUSE_BUTTON_4_DOWN, RI_MOUSE_BUTTON_4_UP, RI_MOUSE_BUTTON_5_DOWN, RI_MOUSE_BUTTON_5_UP,
    RI_MOUSE_WHEEL, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN, SWP_ASYNCWINDOWPOS,
    SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_INPUT, WM_MOUSEACTIVATE, WM_QUIT, WM_SETCURSOR, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

/// The generic desktop page's mouse and keyboard ("Using Raw Input",
/// Microsoft Learn: usage page 1, usage 2 a mouse, 6 a keyboard).
const MOUSE: (u16, u16) = (1, 2);
const KEYBOARD: (u16, u16) = (1, 6);

/// One notch of a wheel (`WHEEL_DELTA`, "WM_MOUSEWHEEL message").
const NOTCH: i32 = 120;

#[derive(Default)]
struct State {
    keys: HashSet<u16>,
    buttons: u8,
    wheel_up: u32,
    wheel_down: u32,
    /// What finer wheels sent short of a notch.
    wheel_part: i32,
    dx: i32,
    dy: i32,
    /// The last place a mouse that reports where it is rather than how far
    /// it moved said it was, in pixels.
    at: Option<(i32, i32)>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

/// What is held now, and how far the mouse moved since the last time.
pub fn take() -> Snapshot {
    let mut state = STATE.lock().unwrap();
    let Some(state) = state.as_mut() else {
        return Snapshot::default();
    };
    Snapshot {
        keys: state.keys.clone(),
        buttons: state.buttons,
        wheel_up: state.wheel_up,
        wheel_down: state.wheel_down,
        dx: std::mem::take(&mut state.dx),
        dy: std::mem::take(&mut state.dy),
    }
}

/// The mouse buttons held now, bit 0 the left one as the mouse reports it,
/// 0 while nothing is read.
pub fn buttons() -> u8 {
    STATE.lock().unwrap().as_ref().map_or(0, |state| state.buttons)
}

/// The reader's thread, to be stopped.
pub struct Reader {
    thread_id: u32,
    thread: std::thread::JoinHandle<()>,
}

impl Reader {
    /// Stops reading, and lets Omoio's process take raw input no more.
    pub fn stop(self) {
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        let _ = self.thread.join();
        *STATE.lock().unwrap() = None;
    }
}

/// Starts reading the keyboard and mouse.
pub fn start() -> Result<Reader, String> {
    *STATE.lock().unwrap() = Some(State::default());
    let (ready, started) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        let window = match listen() {
            Ok(window) => window,
            Err(err) => {
                let _ = ready.send(Err(err));
                return;
            }
        };
        let rest = make_rest();
        REST.store(rest.map_or(0, |rest| rest.0 as isize), Ordering::Relaxed);
        RESTING_SHOWN.store(false, Ordering::Relaxed);
        let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));
        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            if msg.message == WM_INPUT {
                read(HRAWINPUT(msg.lParam.0 as *mut _));
            }
            // Raw input also needs the window's own handling to clean up
            // after it ("WM_INPUT message", Remarks).
            unsafe { DispatchMessageW(&msg) };
        }
        let gone = [MOUSE, KEYBOARD].map(|(page, usage)| RAWINPUTDEVICE {
            usUsagePage: page,
            usUsage: usage,
            dwFlags: RIDEV_REMOVE,
            hwndTarget: HWND::default(),
        });
        let _ = unsafe { RegisterRawInputDevices(&gone, size_of::<RAWINPUTDEVICE>() as u32) };
        REST.store(0, Ordering::Relaxed);
        if let Some(rest) = rest {
            let _ = unsafe { DestroyWindow(rest) };
        }
        let _ = unsafe { DestroyWindow(window) };
    });
    match started.recv() {
        Ok(Ok(thread_id)) => Ok(Reader { thread_id, thread }),
        Ok(Err(err)) => {
            let _ = thread.join();
            *STATE.lock().unwrap() = None;
            Err(err)
        }
        Err(_) => Err("The keyboard and mouse reader stopped as it started.".to_string()),
    }
}

/// The message-only window, signed up for the keyboard and mouse.
fn listen() -> Result<HWND, String> {
    let window = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("Message"),
            None,
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        )
    }
    .map_err(|err| format!("Couldn't make a window to read the keyboard and mouse: {err}"))?;
    let devices = [MOUSE, KEYBOARD].map(|(page, usage)| RAWINPUTDEVICE {
        usUsagePage: page,
        usUsage: usage,
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: window,
    });
    if let Err(err) = unsafe { RegisterRawInputDevices(&devices, size_of::<RAWINPUTDEVICE>() as u32) } {
        let _ = unsafe { DestroyWindow(window) };
        return Err(format!("Couldn't read the keyboard and mouse: {err}"));
    }
    Ok(window)
}

/// The window the cursor rests on, 0 while there is none.
static REST: AtomicIsize = AtomicIsize::new(0);
/// Whether it was last asked to show.
static RESTING_SHOWN: AtomicBool = AtomicBool::new(false);

/// Hides the cursor over the window and keeps a click on it from making it
/// the window in front. Windows asks the window under the cursor which
/// cursor to show ("WM_SETCURSOR message"), and given none "the cursor is
/// removed from the screen" ("SetCursor function"). A window can turn the
/// front down when clicked (`MA_NOACTIVATE`, "WM_MOUSEACTIVATE message"),
/// as `WS_EX_NOACTIVATE` already asks: such a window "does not become the
/// foreground window when the user clicks it" ("Extended Window Styles";
/// all Microsoft Learn, read 21 October 2026). Anything else is left to
/// Windows.
unsafe extern "system" fn rest_messages(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_SETCURSOR => {
            unsafe { SetCursor(None) };
            LRESULT(1)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// The window the cursor rests on, made hidden. Above every window that
/// isn't itself always on top, so above the game; a tool window, so neither
/// the taskbar nor Alt+Tab lists it; and drawn at an opacity of 1 in 255,
/// so it can't be seen while it still takes the clicks on it: only where a
/// layered window's "alpha value is zero" do clicks go through ("Layered
/// Windows", Window Features, Microsoft Learn, read 21 October 2026).
fn make_rest() -> Option<HWND> {
    let instance = unsafe { GetModuleHandleW(None) }.ok()?.into();
    let class = WNDCLASSW {
        lpfnWndProc: Some(rest_messages),
        hInstance: instance,
        lpszClassName: w!("OmoioMouseRest"),
        ..Default::default()
    };
    // Asked again for each game. Windows keeps the class from the first
    // time and turns the second down, which changes nothing.
    unsafe { RegisterClassW(&class) };
    let window = unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED,
            w!("OmoioMouseRest"),
            None,
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(instance),
            None,
        )
    }
    .ok()?;
    if unsafe { SetLayeredWindowAttributes(window, COLORREF(0), 1, LWA_ALPHA) }.is_err() {
        let _ = unsafe { DestroyWindow(window) };
        return None;
    }
    Some(window)
}

/// Shows the resting window around the pixel at `at` and says whether it is
/// the window there now, or hides it for `None`. Three pixels square, so a
/// cursor rounded a pixel off still rests on it. Called from another thread,
/// so Windows is asked not to wait for this one (`SWP_ASYNCWINDOWPOS`,
/// "SetWindowPos function"): a window shown that way may be there only by
/// the next call. False while the reader isn't running.
pub fn rest_at(at: Option<POINT>) -> bool {
    let window = REST.load(Ordering::Relaxed);
    if window == 0 {
        return false;
    }
    let window = HWND(window as *mut _);
    match at {
        Some(at) => {
            RESTING_SHOWN.store(true, Ordering::Relaxed);
            // Again at every call: the game moves with Omoio's window, and
            // another window always on top may have come over this one.
            let _ = unsafe {
                SetWindowPos(
                    window,
                    Some(HWND_TOPMOST),
                    at.x - 1,
                    at.y - 1,
                    3,
                    3,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_ASYNCWINDOWPOS,
                )
            };
            let there = unsafe { WindowFromPoint(at) };
            there == window
        }
        None => {
            if RESTING_SHOWN.swap(false, Ordering::Relaxed) {
                let flags = SWP_HIDEWINDOW | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
                let _ = unsafe { SetWindowPos(window, None, 0, 0, 0, 0, flags | SWP_ASYNCWINDOWPOS) };
            }
            false
        }
    }
}

fn read(handle: HRAWINPUT) {
    let mut input = RAWINPUT::default();
    let mut size = size_of::<RAWINPUT>() as u32;
    let read = unsafe {
        GetRawInputData(
            handle,
            RID_INPUT,
            Some(&mut input as *mut RAWINPUT as *mut _),
            &mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if read == u32::MAX || read == 0 {
        return;
    }
    let mut state = STATE.lock().unwrap();
    let Some(state) = state.as_mut() else {
        return;
    };
    match input.header.dwType {
        kind if kind == RIM_TYPEKEYBOARD.0 => key(state, unsafe { &input.data.keyboard }),
        kind if kind == RIM_TYPEMOUSE.0 => mouse(state, unsafe { &input.data.mouse }),
        _ => {}
    }
}

/// A key's scan code as core/keys.rs keeps it: 0xE0 in the high byte for
/// an extended key. A report with no scan code, as some programs that type
/// for the user send, is looked up from its virtual-key code. Pause comes
/// with E1 and is no key a player can have.
fn scan_code(make: u16, flags: u16, virtual_key: u16) -> Option<u16> {
    if u32::from(flags) & RI_KEY_E1 != 0 {
        return None;
    }
    if make == 0 {
        let looked_up = unsafe { MapVirtualKeyW(u32::from(virtual_key), MAPVK_VK_TO_VSC_EX) };
        return u16::try_from(looked_up).ok().filter(|&scan| scan != 0);
    }
    let extended = u32::from(flags) & RI_KEY_E0 != 0;
    Some(if extended { 0xE000 | make } else { make })
}

fn key(state: &mut State, report: &RAWKEYBOARD) {
    let Some(scan) = scan_code(report.MakeCode, report.Flags, report.VKey) else {
        return;
    };
    if u32::from(report.Flags) & RI_KEY_BREAK != 0 {
        state.keys.remove(&scan);
    } else {
        state.keys.insert(scan);
    }
}

fn mouse(state: &mut State, report: &RAWMOUSE) {
    if report.usFlags.0 & MOUSE_MOVE_ABSOLUTE.0 != 0 {
        // A tablet, a remote desktop or a streamed one: where it is, from 0
        // to 65535 across the screen ("RAWMOUSE structure", Remarks).
        let whole = report.usFlags.0 & MOUSE_VIRTUAL_DESKTOP.0 != 0;
        let (width, height) = unsafe {
            if whole {
                (GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN))
            } else {
                (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
            }
        };
        let at = (
            (i64::from(report.lLastX) * i64::from(width) / 65535) as i32,
            (i64::from(report.lLastY) * i64::from(height) / 65535) as i32,
        );
        if let Some(before) = state.at.replace(at) {
            state.dx += at.0 - before.0;
            state.dy += at.1 - before.1;
        }
    } else {
        state.dx += report.lLastX;
        state.dy += report.lLastY;
    }
    let flags = u32::from(unsafe { report.Anonymous.Anonymous.usButtonFlags });
    let pairs = [
        (RI_MOUSE_BUTTON_1_DOWN, RI_MOUSE_BUTTON_1_UP),
        (RI_MOUSE_BUTTON_2_DOWN, RI_MOUSE_BUTTON_2_UP),
        (RI_MOUSE_BUTTON_3_DOWN, RI_MOUSE_BUTTON_3_UP),
        (RI_MOUSE_BUTTON_4_DOWN, RI_MOUSE_BUTTON_4_UP),
        (RI_MOUSE_BUTTON_5_DOWN, RI_MOUSE_BUTTON_5_UP),
    ];
    for (bit, (down, up)) in pairs.into_iter().enumerate() {
        if flags & down != 0 {
            state.buttons |= 1 << bit;
        }
        if flags & up != 0 {
            state.buttons &= !(1 << bit);
        }
    }
    if flags & RI_MOUSE_WHEEL != 0 {
        // Signed, away from the user positive.
        let turned = unsafe { report.Anonymous.Anonymous.usButtonData } as i16;
        state.wheel_part += i32::from(turned);
        while state.wheel_part >= NOTCH {
            state.wheel_part -= NOTCH;
            state.wheel_up += 1;
        }
        while state.wheel_part <= -NOTCH {
            state.wheel_part += NOTCH;
            state.wheel_down += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_kept_as_core_keeps_them() {
        assert_eq!(scan_code(0x11, 0, 0), crate::core::keys::scan_code("KeyW"));
        assert_eq!(scan_code(0x48, RI_KEY_E0 as u16, 0), crate::core::keys::scan_code("ArrowUp"));
        assert_eq!(scan_code(0x48, 0, 0), crate::core::keys::scan_code("Numpad8"));
        assert_eq!(scan_code(0x1D, RI_KEY_E1 as u16, 0), None, "Pause");
        assert_eq!(scan_code(0, 0, 0x20), crate::core::keys::scan_code("Space"), "a report with no scan code");
    }

    fn report(flags: u32, data: i16, dx: i32, dy: i32) -> RAWMOUSE {
        let mut report = RAWMOUSE::default();
        report.Anonymous.Anonymous.usButtonFlags = flags as u16;
        report.Anonymous.Anonymous.usButtonData = data as u16;
        report.lLastX = dx;
        report.lLastY = dy;
        report
    }

    #[test]
    fn the_mouse_buttons_wheel_and_movement_are_kept() {
        let mut state = State::default();
        mouse(&mut state, &report(RI_MOUSE_BUTTON_1_DOWN | RI_MOUSE_BUTTON_4_DOWN, 0, 3, -2));
        mouse(&mut state, &report(0, 0, 4, 1));
        assert_eq!(state.buttons, 0b1001, "left and back");
        assert_eq!((state.dx, state.dy), (7, -1));
        mouse(&mut state, &report(RI_MOUSE_BUTTON_1_UP, 0, 0, 0));
        assert_eq!(state.buttons, 0b1000);
        mouse(&mut state, &report(RI_MOUSE_WHEEL, 60, 0, 0));
        assert_eq!(state.wheel_up, 0, "half a notch");
        mouse(&mut state, &report(RI_MOUSE_WHEEL, 60, 0, 0));
        mouse(&mut state, &report(RI_MOUSE_WHEEL, -240, 0, 0));
        assert_eq!((state.wheel_up, state.wheel_down), (1, 2));
    }
}
