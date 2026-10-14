//! Big Picture: Omoio across the whole screen, used from the sofa with a pad.
//!
//! It is the same window, filling the screen and drawing the interface made
//! for a controller. A game started from it fills the screen too. Pressing
//! View and Menu together during a game takes the game off the screen and
//! brings Big Picture up, with the game still running behind it, the way
//! Steam's own button brings up Steam. The same two again, or Resume, put the
//! game back.

use crate::backends::rpcs3::overlay;
use crate::core::settings::Settings;
use crate::session::Session;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

static ON: AtomicBool = AtomicBool::new(false);

/// View and Menu on an Xbox pad, the two small buttons in the middle of
/// nearly every other pad. Both held on the same pad.
const CHORD: [&str; 2] = ["Back", "Start"];

pub fn is_on() -> bool {
    ON.load(Ordering::Relaxed)
}

#[derive(Clone, serde::Serialize)]
pub struct State {
    pub on: bool,
    /// A game is running with its picture taken off the screen.
    pub suspended: bool,
}

pub fn state(app: &AppHandle) -> State {
    State {
        on: is_on(),
        suspended: app.state::<Session>().is_hidden(),
    }
}

fn tell(app: &AppHandle) {
    let _ = app.emit("big-picture", state(app));
}

/// Switches Big Picture on or off. Coming in with a game on the screen takes
/// the game off it, since whoever asked for Big Picture wants to see it.
/// Going out puts a waiting game back where the desktop keeps it, without
/// taking the keyboard from Omoio, unless the desktop keeps it over the
/// whole screen: then nothing of Omoio's is left to use, and the game gets
/// the keyboard and the pad.
pub fn set(app: &AppHandle, on: bool) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("Omoio's window isn't there.")?;
    let session = app.state::<Session>();
    if on && session.playing().is_some() {
        session.hide_game();
        crate::portal_menu::close(app);
    }
    ON.store(on, Ordering::Relaxed);
    window.set_fullscreen(on).map_err(|e| e.to_string())?;
    if on {
        let _ = window.set_focus();
    } else if session.is_hidden() {
        crate::session::place(&window, &session);
        session.show_game(false);
        if session.is_fullscreen() {
            crate::session::give_game_front(app);
        }
    }
    crate::session::quiet_game(app);
    tell(app);
    Ok(())
}

/// Puts the game waiting behind Big Picture back on the screen, and gives it
/// the keyboard and the pad, making sure it keeps them
/// (`session::give_game_front`).
pub fn resume(app: &AppHandle) {
    let session = app.state::<Session>();
    if session.playing().is_none() {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        crate::session::place(&window, &session);
    }
    session.show_game(true);
    crate::session::give_game_front(app);
    crate::session::quiet_game(app);
    tell(app);
}

/// Opens in Big Picture when the user asked for that, and starts listening
/// for the two buttons.
pub fn start(app: &AppHandle) {
    let asked = app
        .path()
        .data_dir()
        .map(|dir| Settings::load(&dir.join("Omoio").join("settings.json")).start_in_big_picture)
        .unwrap_or(false);
    if asked {
        let _ = set(app, true);
    }
    watch(app.clone());
}

fn is_chord(held: &[&str]) -> bool {
    CHORD.iter().all(|button| held.contains(button))
}

/// What the two buttons do: put back a game waiting behind Big Picture, bring
/// Big Picture up over a game on the screen, or with no game, open it.
fn pressed(app: &AppHandle) {
    let session = app.state::<Session>();
    let playing = session.playing().is_some();
    if playing && session.is_hidden() {
        resume(app);
    } else if playing || !is_on() {
        let _ = set(app, true);
    }
}

/// Reads every pad twenty times a second, since a press lasts about a tenth
/// of one. Only while Omoio or its game is the window in front, so the
/// buttons never reach into another program, and nothing is read at all
/// while Omoio sits in the background.
fn watch(app: AppHandle) {
    std::thread::spawn(move || {
        let mut was = false;
        loop {
            std::thread::sleep(Duration::from_millis(50));
            let mut ours = vec![std::process::id()];
            ours.extend(app.state::<Session>().pid());
            let now = overlay::front_belongs_to(&ours)
                && crate::pads::connected()
                    .iter()
                    .any(|pad| crate::pads::held(&pad.device).is_some_and(|held| is_chord(&held)));
            if now && !was {
                pressed(&app);
            }
            was = now;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_buttons_have_to_be_held() {
        assert!(is_chord(&["Back", "Start"]));
        assert!(is_chord(&["Start", "South", "Back"]));
        assert!(!is_chord(&["Start"]));
        assert!(!is_chord(&["Back", "Guide"]));
        assert!(!is_chord(&[]));
    }
}
