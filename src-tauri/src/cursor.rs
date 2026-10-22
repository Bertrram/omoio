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
//!
//! A player on the pad Omoio serves Cemu (dsu.rs) aims through Raw Input,
//! which doesn't need the cursor at all, so for them the cursor is kept on
//! one pixel in the middle of the game instead, on a window of Omoio's,
//! where it is hidden and a click reaches neither Cemu nor the front
//! (`dsu::rest`).
//!
//! Some emulators also need to hold the mouse themselves for aiming to go
//! on past the window's edge (`EmulatorBackend::hold_mouse`). That is asked
//! once each time the game's window has been shown and has stayed in front
//! a moment, since showing it again is what makes such an emulator let go.

use crate::backends::rpcs3::overlay;
use crate::session::Session;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::WindowsAndMessaging::{ClipCursor, GetClientRect, IsWindowVisible};

/// How often the front is looked at. Short, so a switch away frees the
/// mouse before the user reaches for it.
const LOOK_EVERY: Duration = Duration::from_millis(50);

/// How long the game's window stays shown and in front before the emulator
/// is asked to hold the mouse: long enough for the window to have settled
/// after Omoio places and shows it, so the emulator doesn't let go again
/// straight after.
const SETTLED: Duration = Duration::from_secs(1);

/// Keeps the cursor inside the game started as `pid` while its window is
/// in front, until the game ends. Started only when someone aims with the
/// mouse in this game (`Features::mouse_aims`).
pub fn watch(app: AppHandle, pid: u32) {
    std::thread::spawn(move || {
        let backend = app
            .state::<Session>()
            .playing()
            .and_then(|playing| crate::backends::for_console(playing.console));
        let mut holding = false;
        // Whether the emulator holds the mouse, as far as Omoio knows, and
        // since when the window has been in front without a break.
        let mut held_by_emulator = false;
        let mut in_front_since: Option<Instant> = None;
        loop {
            std::thread::sleep(LOOK_EVERY);
            let session = app.state::<Session>();
            if session.pid() != Some(pid) {
                crate::dsu::rest(pid, None);
                if holding {
                    let_go();
                }
                return;
            }
            let window = session.shown().filter(|&(_, shown)| shown == pid).map(|(game, _)| game);
            if !window.is_some_and(|game| unsafe { IsWindowVisible(HWND(game as *mut _)) }.as_bool()) {
                held_by_emulator = false;
            }
            let game = window
                .filter(|&game| overlay::front() == game)
                .filter(|_| !crate::portal_menu::showing(&app))
                .and_then(|game| inside(game).map(|rect| (game, rect)));
            // The pad Omoio serves Cemu for the keyboard and mouse is heard
            // exactly while the mouse is kept here.
            crate::dsu::hear(game.is_some());
            // On that pad the cursor rests on one pixel of Omoio's instead,
            // where a click can't reach Cemu; for every other player there
            // is no resting place and nothing changes.
            let rest = crate::dsu::rest(pid, game.map(|(_, rect)| rect));
            match game {
                Some((game_window, rect)) => {
                    // Again at every look: the window moves and changes size
                    // when Omoio's window does, and Windows itself may have
                    // let the cursor go.
                    holding = unsafe { ClipCursor(Some(&rest.unwrap_or(rect))) }.is_ok();
                    let since = *in_front_since.get_or_insert_with(Instant::now);
                    if !held_by_emulator && since.elapsed() >= SETTLED {
                        held_by_emulator = backend.is_none_or(|backend| backend.hold_mouse(game_window));
                    }
                }
                None => {
                    in_front_since = None;
                    if holding {
                        let_go();
                        holding = false;
                    }
                }
            }
        }
    });
}

fn let_go() {
    let _ = unsafe { ClipCursor(None) };
}

/// Lets the cursor go if Omoio fails. A release build ends at once when it
/// does (`panic = "abort"`, Cargo.toml), before the next look could let go,
/// and the cursor would stay held, on one pixel for a Wii U player, until
/// something else came to the front.
pub fn let_go_if_omoio_fails() {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let_go();
        before(info);
    }));
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
