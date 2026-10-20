//! Keeps the mouse inside the game while someone aims with it.
//!
//! On the keyboard and mouse, moving the mouse turns the game's camera or
//! aims, the way a stick does. The game's window sits inside Omoio's, with
//! Omoio's sidebar and bar around it and other programs beyond, so a cursor
//! free to wander would leave the game mid-turn, and a click meant as a
//! shot would land in Omoio or another program. So while the game's window
//! is in front, the cursor is kept inside it with ClipCursor, and the moment
//! anything else is in front, the portal menu, Big Picture or another
//! program, it is let go. Omoio never brings anything to the front for this:
//! it only follows what is there.
//!
//! The cursor is shared by every program, and Microsoft asks that a program
//! that confines it "must release the cursor by using ClipCursor before
//! relinquishing control to another application" ("ClipCursor function",
//! Remarks, Microsoft Learn, read 20 October 2026). Windows changes the
//! front without asking Omoio, as with Alt+Tab or the Windows key, so this
//! looks often and lets go as soon as the game is no longer in front.

use crate::backends::rpcs3::overlay;
use crate::session::Session;
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::WindowsAndMessaging::{ClipCursor, GetClientRect};

/// How often the front is looked at. Short, so a switch away frees the
/// mouse before the user reaches for it.
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_millis(50);

/// Keeps the cursor inside the game started as `pid` while its window is
/// in front, until the game ends. Started only when someone aims with the
/// mouse in this game (`Features::mouse_aims`).
pub fn watch(app: AppHandle, pid: u32) {
    std::thread::spawn(move || {
        let mut holding = false;
        loop {
            std::thread::sleep(LOOK_EVERY);
            let session = app.state::<Session>();
            if session.pid() != Some(pid) {
                if holding {
                    let_go();
                }
                return;
            }
            let game = session
                .shown()
                .filter(|&(game, shown)| shown == pid && overlay::front() == game)
                .filter(|_| !crate::portal_menu::showing(&app))
                .and_then(|(game, _)| inside(game));
            match game {
                Some(rect) => {
                    // Again at every look: the window moves and changes size
                    // when Omoio's window does, and Windows itself may have
                    // let the cursor go.
                    holding = unsafe { ClipCursor(Some(&rect)) }.is_ok();
                }
                None if holding => {
                    let_go();
                    holding = false;
                }
                None => {}
            }
        }
    });
}

fn let_go() {
    let _ = unsafe { ClipCursor(None) };
}

/// The inside of a window, without its frame, on the screen.
fn inside(window: isize) -> Option<RECT> {
    let window = HWND(window as *mut _);
    let mut rect = RECT::default();
    unsafe { GetClientRect(window, &mut rect) }.ok()?;
    let mut corner = POINT { x: rect.left, y: rect.top };
    if !unsafe { ClientToScreen(window, &mut corner) }.as_bool() {
        return None;
    }
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    (width > 0 && height > 0).then(|| RECT {
        left: corner.x,
        top: corner.y,
        right: corner.x + width,
        bottom: corner.y + height,
    })
}
