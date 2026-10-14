//! The game currently being played.
//!
//! RPCS3 runs as its own process. Omoio keeps hold of it so the picture can be
//! placed inside our window, so Stop actually stops it, and so a game never
//! outlives the app that started it.

use crate::backends::rpcs3::overlay;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

/// Where the game picture sits inside our window: right of the sidebar, and
/// below the title bar and the top bar, which keeps showing what is running
/// and the way to stop it. Same units the interface is laid out in.
const SIDEBAR: f64 = 212.0;
const CONTENT_TOP: f64 = 38.0 + 52.0;

#[derive(Clone, Serialize)]
pub struct Playing {
    pub title_id: String,
    pub title: String,
    /// Which console, so the right emulator's log is kept afterwards.
    pub console: crate::core::console::Console,
}

#[derive(Default)]
pub struct Session {
    inner: Mutex<Option<Running>>,
}

struct Running {
    pid: u32,
    playing: Playing,
    window: Option<isize>,
    fullscreen: bool,
    /// Taken off the screen for Big Picture while the game carries on.
    hidden: bool,
    started: std::time::Instant,
    /// Set by Stop, so a session we ended is never reported as a crash.
    stopped_by_us: bool,
}

impl Session {
    pub fn begin(&self, pid: u32, playing: Playing) {
        *self.inner.lock().unwrap() = Some(Running {
            pid,
            playing,
            window: None,
            fullscreen: false,
            hidden: false,
            started: std::time::Instant::now(),
            stopped_by_us: false,
        });
    }

    pub fn is_hidden(&self) -> bool {
        self.inner.lock().unwrap().as_ref().is_some_and(|r| r.hidden)
    }

    /// Takes the game picture off the screen and leaves the game running.
    /// A game still starting has no window yet; it is hidden the moment it
    /// gets one. Says whether a game was running.
    pub fn hide_game(&self) -> bool {
        let window = {
            let mut inner = self.inner.lock().unwrap();
            let Some(running) = inner.as_mut() else {
                return false;
            };
            running.hidden = true;
            running.window
        };
        // Outside the lock: the call waits on the emulator's window.
        if let Some(window) = window {
            overlay::hide(window);
        }
        true
    }

    /// Hands the keyboard and pad back to the game after Omoio moved it,
    /// unless it is hidden behind Big Picture, and says whether the game is
    /// in front afterwards. Only from Omoio, the emulator or nobody: a
    /// program the user went to keeps the front. Once, at once; see
    /// `give_game_front` for the hand-over that also makes sure it held.
    pub fn focus_game(&self) -> bool {
        let Some((game, pid)) = self.shown() else {
            return false;
        };
        match first_step(front_now(game, pid, 0)) {
            Step::Wait => true,
            Step::Take => overlay::bring_to_front(game),
            Step::Stop => false,
        }
    }

    /// The game's window and its emulator, while the window is on the
    /// screen.
    fn shown(&self) -> Option<(isize, u32)> {
        let inner = self.inner.lock().unwrap();
        let running = inner.as_ref().filter(|r| !r.hidden)?;
        Some((running.window?, running.pid))
    }

    /// Puts the game picture back, and with `activate` gives it the keyboard.
    pub fn show_game(&self, activate: bool) {
        let window = {
            let mut inner = self.inner.lock().unwrap();
            let Some(running) = inner.as_mut() else {
                return;
            };
            running.hidden = false;
            running.window
        };
        if let Some(window) = window {
            overlay::show(window, activate);
        }
    }

    fn started_at(&self) -> Option<std::time::Instant> {
        self.inner.lock().unwrap().as_ref().map(|r| r.started)
    }

    fn was_stopped_by_us(&self) -> bool {
        self.inner.lock().unwrap().as_ref().is_some_and(|r| r.stopped_by_us)
    }

    pub fn playing(&self) -> Option<Playing> {
        self.inner.lock().unwrap().as_ref().map(|r| r.playing.clone())
    }

    pub fn is_fullscreen(&self) -> bool {
        self.inner.lock().unwrap().as_ref().is_some_and(|r| r.fullscreen)
    }

    pub fn set_fullscreen(&self, on: bool) {
        if let Some(running) = self.inner.lock().unwrap().as_mut() {
            running.fullscreen = on;
        }
    }

    pub fn pid(&self) -> Option<u32> {
        self.inner.lock().unwrap().as_ref().map(|r| r.pid)
    }

    fn window(&self) -> Option<isize> {
        self.inner.lock().unwrap().as_ref().and_then(|r| r.window)
    }

    fn adopt_window(&self, window: isize) {
        if let Some(running) = self.inner.lock().unwrap().as_mut() {
            running.window = Some(window);
        }
    }

    fn end(&self) -> Option<Playing> {
        self.inner.lock().unwrap().take().map(|r| r.playing)
    }

    /// Ends the game if one is running. An emulator that writes out what it
    /// holds only as it closes, as Dolphin does, is asked to close the way
    /// its own Stop does, and killed only if it hasn't within a few seconds
    /// (`backends::close`), which this waits for. Any other is killed at
    /// once: that is how RPCS3 is stopped from outside, and it keeps nothing
    /// of ours that a clean exit would save.
    pub fn stop(&self) -> bool {
        let Some(pid) = self.pid() else {
            return false;
        };
        let backend = self.playing().and_then(|playing| crate::backends::for_console(playing.console));
        if let Some(running) = self.inner.lock().unwrap().as_mut() {
            running.stopped_by_us = true;
        }
        if !backend.is_some_and(|backend| crate::backends::close(backend, pid)) {
            kill(pid);
        }
        true
    }

    /// Whether `stop` would wait for the game's emulator to close, which
    /// takes seconds, too long for the window's own thread.
    pub fn stops_slowly(&self) -> bool {
        self.playing()
            .and_then(|playing| crate::backends::for_console(playing.console))
            .is_some_and(|backend| backend.closes_when_asked())
    }
}

#[cfg(windows)]
fn kill(pid: u32) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

#[cfg(not(windows))]
fn kill(_pid: u32) {}

fn still_running(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let Ok(out) = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        else {
            return false;
        };
        String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        false
    }
}

/// Keeps the game picture lined up with our window, and notices when the game
/// ends. Runs until the game is gone, then tells the interface.
pub fn watch(app: AppHandle, pid: u32) {
    tauri::async_runtime::spawn(async move {
        let mut attached = false;
        // When the game's window appeared, until its pads have been asked
        // about.
        let mut pads_unasked: Option<std::time::Instant> = None;
        // The key state is remembered until read, so clear anything left over
        // from before the game started. Otherwise an F11 pressed elsewhere
        // minutes ago throws the game to fullscreen the moment it appears.
        overlay::fullscreen_key_pressed();

        loop {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;

            let session = app.state::<Session>();
            // A stop or a second launch means this watcher is finished.
            if session.pid() != Some(pid) {
                return;
            }

            if !still_running(pid) {
                // Read the log before anything else can overwrite it.
                let stopped_by_us = session.was_stopped_by_us();
                let seconds = session
                    .started_at()
                    .map(|s| s.elapsed().as_secs())
                    .unwrap_or(0);
                let playing = session.end();
                if let Some(playing) = playing {
                    keep_session_log(&app, &playing, seconds, stopped_by_us);
                    let _ = app.emit("game-stopped", playing);
                }
                return;
            }

            let Some(window) = app.get_webview_window("main") else {
                continue;
            };
            let host = window.hwnd().map(|h| h.0 as isize).unwrap_or(0);

            // The game holds the keyboard, so this is the only way back out of
            // a picture that covers the screen. Read it even when we are not
            // going to act on it, so a press meant for another window is
            // consumed rather than saved up for later. Big Picture always
            // fills the screen, so there it changes nothing.
            let pressed = overlay::fullscreen_key_pressed();
            if let (true, Some(game)) = (pressed, session.window()) {
                if !crate::big_picture::is_on() && overlay::ours_has_focus(game, host) {
                    let now = !session.is_fullscreen();
                    session.set_fullscreen(now);
                    let _ = app.emit("game-fullscreen", now);
                    give_game_front(&app);
                }
            }

            if !attached {
                // The window only exists once RPCS3 has something to draw, so
                // this keeps looking while the game boots.
                let backend = session.playing().and_then(|playing| crate::backends::for_console(playing.console));
                let is_game = |title: &str| backend.is_none_or(|backend| backend.is_game_window(title));
                if let Some(game) = overlay::find_window(pid, &is_game) {
                    overlay::attach(game, host);
                    session.adopt_window(game);
                    // Big Picture was asked for while the game was starting.
                    if session.is_hidden() {
                        overlay::hide(game);
                    } else if overlay::front_belongs_to(&[std::process::id(), pid]) {
                        overlay::focus(game);
                    }
                    attached = true;
                    pads_unasked = Some(std::time::Instant::now());
                    let _ = app.emit("game-started", session.playing());
                }
            }

            if attached {
                if let Some(game) = session.window().filter(|&game| overlay::exists(game)) {
                    overlay::keep(game, host);
                }
                place(&window, &session);
                let backend = session.playing().and_then(|playing| crate::backends::for_console(playing.console));
                if let (Some(backend), Some(game)) = (backend, session.window()) {
                    backend.tidy_window(pid, game);
                }
            }

            if pads_unasked.is_some_and(|since| since.elapsed() >= PADS_SET_UP) {
                pads_unasked = None;
                ask_about_pads(&app);
            }
        }
    });
}

/// How long after the game's window appears to ask whether the emulator
/// found every pad. RPCS3 sets its pads up a fraction of a second after it
/// opens the window (its log, 8 October 2026); the rest is room for that to
/// reach its log file.
const PADS_SET_UP: std::time::Duration = std::time::Duration::from_secs(5);

/// Tells the interface when the emulator couldn't find a player's pad, which
/// would otherwise show only as a game that ignores it. Off the watcher's
/// task, since the emulator's log can be large.
fn ask_about_pads(app: &AppHandle) {
    let Some(playing) = app.state::<Session>().playing() else {
        return;
    };
    let Some(backend) = crate::backends::for_console(playing.console) else {
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        if let Some(message) = backend.unfound_pad(&app, &playing.title_id) {
            let _ = app.emit("pad-not-found", message);
        }
    });
}

/// Keeps the running game from hearing the pad while one of Omoio's menus is
/// over it: the portal menu, or Big Picture with the game hidden behind it.
/// Worked out afresh from what is showing, one change at a time, so a menu
/// closing just as another opens never leaves the game listening. Omoio's
/// window takes the keyboard back afterwards: the emulator's own window for
/// this can take it, and keys typed there would change its settings.
pub fn quiet_game(app: &AppHandle) {
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    let app = app.clone();
    std::thread::spawn(move || {
        let _turn = ONE_AT_A_TIME.lock().unwrap();
        let session = app.state::<Session>();
        let (Some(pid), Some(playing)) = (session.pid(), session.playing()) else {
            return;
        };
        let Some(backend) = crate::backends::for_console(playing.console) else {
            return;
        };
        let portal = crate::portal_menu::showing(&app);
        let over = portal || session.is_hidden();
        if portal {
            backend.ready_portal(pid);
        }
        if backend.hush(pid, over).is_ok() && over {
            if let Some(window) = app.get_webview_window(if portal { "portal" } else { "main" }) {
                let _ = window.set_focus();
            }
        }
    });
}

/// What is in front, seen from a game Omoio is handing the front to.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Front {
    Game,
    /// No window, which Windows allows "in certain circumstances, such as
    /// when a window is losing activation" ("GetForegroundWindow function",
    /// Microsoft Learn, read 14 October 2026).
    Nobody,
    /// Omoio's own window, with the interface or Big Picture in it.
    OmoioMain,
    /// Another of Omoio's windows, such as the portal menu.
    OmoioOther,
    /// One of the emulator's windows that nothing owns, such as its main
    /// window.
    Emulator,
    /// A window the emulator put up over one of its own: a dialog or a
    /// message, which may be there to be read.
    EmulatorDialog,
    /// Any other program: the user went there, and stays there.
    Other,
}

/// What to do about it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    /// Nothing for now: the game is in front, or may be about to be.
    Wait,
    /// Give the game the front.
    Take,
    /// Leave the front where it is, and stop looking.
    Stop,
}

/// At the moment Omoio hands over: the front is taken from Omoio's own
/// windows, the emulator's and nobody, never from another program.
fn first_step(front: Front) -> Step {
    match front {
        Front::Game => Step::Wait,
        Front::Nobody | Front::OmoioMain | Front::OmoioOther | Front::Emulator => Step::Take,
        Front::EmulatorDialog | Front::Other => Step::Stop,
    }
}

/// Looking again after a hand-over, in case something took the front back.
/// Omoio's window is taken from only while the game covers the screen; in
/// a window the user may have gone to Omoio's on purpose. Nobody in front
/// counts only when it was so at the look before too, so a switch to
/// another program, which passes through nobody, is never cut short.
fn next_step(front: Front, covers_screen: bool, nobody_before: bool) -> Step {
    match front {
        Front::Game => Step::Wait,
        Front::Nobody if nobody_before => Step::Take,
        Front::Nobody => Step::Wait,
        Front::OmoioMain if covers_screen => Step::Take,
        Front::Emulator => Step::Take,
        Front::OmoioMain | Front::OmoioOther | Front::EmulatorDialog | Front::Other => Step::Stop,
    }
}

/// What is in front now. `main` is Omoio's own window, 0 when it doesn't
/// matter which of Omoio's windows it is.
fn front_now(game: isize, emulator: u32, main: isize) -> Front {
    let front = overlay::front();
    if front == 0 {
        return Front::Nobody;
    }
    if front == game {
        return Front::Game;
    }
    let owner = overlay::process_of(front);
    if owner == std::process::id() {
        if front == main {
            Front::OmoioMain
        } else {
            Front::OmoioOther
        }
    } else if owner == emulator {
        if overlay::has_owner(front) {
            Front::EmulatorDialog
        } else {
            Front::Emulator
        }
    } else {
        Front::Other
    }
}

/// Whether the game's picture fills the screen, so Omoio's own window has
/// nothing the user can see or use while the game is on it.
fn covers_screen(session: &Session) -> bool {
    crate::big_picture::is_on() || session.is_fullscreen()
}

/// How long, and how often, Omoio looks again after handing the game the
/// front, and how many times at most it takes it back meanwhile. Long enough
/// for what the hand-over set off in Omoio's own window and in the
/// emulator's to have happened, short enough that nothing the user does
/// later is answered.
const KEEP_WATCH: std::time::Duration = std::time::Duration::from_millis(1500);
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_millis(50);
const TAKES: u32 = 3;

/// Which hand-over is being watched. A newer one ends the watch of the one
/// before, so two never look at once.
static HANDOVER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Gives the game the front and makes sure it keeps it: after the portal
/// menu closes, after Resume, after leaving Big Picture into a game that
/// fills the screen, and when Omoio's own window comes to the front over
/// such a game.
///
/// Dolphin and RPCS3 hear the pad only while the game's window is the
/// window in front: Dolphin compares `GetForegroundWindow()` with its render
/// window when background input is off (`Host::GetRenderFocus` in
/// DolphinQt/Host.cpp and `UpdateInputGate` in Core/Core.cpp, Dolphin
/// 2609a), and RPCS3 follows Qt's word on whether its game window is active
/// (`activeChanged` in rpcs3qt/gs_frame.cpp, master of 14 October 2026).
/// Up to 0.3.1 the hand-over was one SetForegroundWindow whose answer was
/// never read, and in Big Picture the game stayed deaf after every close of
/// the menu (Discord, ticket #4, 9 and 10 October 2026). There Windows may
/// refuse Omoio the front once Omoio no longer has it, since a pad gives
/// Omoio no keyboard or mouse input (overlay::bring_to_front), and hiding
/// the portal menu while it was in front let Windows give the front to a
/// window of its own choosing first
/// ("ShowWindow function", SW_HIDE: "Hides the window and activates another
/// window"; Window Features: the next window "as determined by the ALT+ESC
/// key combination"; both Microsoft Learn, read 14 October 2026). So the
/// menu hands over before it hides (portal_menu::close), the hand-over
/// presses Alt when Windows refuses, and this looks again for a moment in
/// case something takes the front straight back. Never from another
/// program, and off the calling thread after the first try.
pub fn give_game_front(app: &AppHandle) {
    let session = app.state::<Session>();
    session.focus_game();
    let Some((game, pid)) = session.shown() else {
        return;
    };
    let turn = HANDOVER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let app = app.clone();
    std::thread::spawn(move || keep_front(&app, game, pid, turn));
}

fn keep_front(app: &AppHandle, game: isize, pid: u32, turn: u64) {
    use std::sync::atomic::Ordering;
    let main = app
        .get_webview_window("main")
        .and_then(|window| window.hwnd().ok())
        .map(|hwnd| hwnd.0 as isize)
        .unwrap_or(0);
    let until = std::time::Instant::now() + KEEP_WATCH;
    let (mut takes, mut nobody_before) = (0, false);
    while std::time::Instant::now() < until {
        std::thread::sleep(LOOK_EVERY);
        let session = app.state::<Session>();
        // A newer hand-over, another game, the game taken off the screen,
        // or a menu over it: none of that is this watch's business.
        if HANDOVER.load(Ordering::Relaxed) != turn
            || session.shown() != Some((game, pid))
            || crate::portal_menu::showing(app)
        {
            return;
        }
        let front = front_now(game, pid, main);
        match next_step(front, covers_screen(&session), nobody_before) {
            Step::Wait => {}
            Step::Stop => return,
            Step::Take if takes == TAKES => return,
            Step::Take => {
                takes += 1;
                overlay::bring_to_front(game);
            }
        }
        nobody_before = front == Front::Nobody;
    }
}

/// Omoio's own window came to the front. Over a game that fills the screen,
/// in Big Picture or in Omoio's fullscreen, there is nothing of Omoio's to
/// use, so the game gets the front. Alt+Tab back to Omoio brings Omoio's
/// own window, not the game's, which Omoio owns and so keeps off the
/// taskbar (attach() in overlay.rs). With the portal menu up, or the game
/// in a window or hidden behind Big Picture, Omoio keeps the front.
pub fn omoio_activated(app: &AppHandle) {
    let session = app.state::<Session>();
    if session.shown().is_some() && covers_screen(&session) && !crate::portal_menu::showing(app) {
        give_game_front(app);
    }
}

/// Fills in a game's version from the emulator's log when the game's own
/// files couldn't give one, as with an encrypted Wii U disc image.
fn learn_version(app: &AppHandle, title_id: &str, version: String) {
    let Ok(data_dir) = app.path().data_dir() else {
        return;
    };
    let file = data_dir.join("Omoio").join("library.json");
    let mut library = crate::core::library::Library::load(&file);
    if let Some(game) = library.get_mut(title_id).filter(|game| game.version.is_none()) {
        game.version = Some(version);
        let _ = library.save(&file);
    }
}

/// Copies what the emulator said about this session somewhere it will survive,
/// and records how it ended. Emulators overwrite their own log on the next
/// launch, so this is the only chance to keep it, and to learn from it what
/// the game's own files couldn't say.
fn keep_session_log(app: &AppHandle, playing: &Playing, seconds: u64, stopped_by_us: bool) {
    use crate::core::playlog::{self, Ending, Session as LoggedSession};

    let backend = crate::backends::for_console(playing.console);
    let log = backend
        .as_ref()
        .and_then(|backend| backend.log_file(app))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    if let Some(version) = backend.as_ref().and_then(|backend| backend.version_from_log(&log)) {
        learn_version(app, &playing.title_id, version);
    }

    let ending = if stopped_by_us {
        Ending::Stopped
    } else if playlog::has_fatal(&log) {
        Ending::Crashed
    } else {
        Ending::Closed
    };

    let Ok(data_dir) = app.path().data_dir() else {
        return;
    };
    let logs = data_dir.join("Omoio").join("logs");
    if std::fs::create_dir_all(&logs).is_err() {
        return;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let log_file = logs.join(format!("{}-{stamp}.log", playing.title_id));
    let _ = std::fs::write(&log_file, &log);

    let session = LoggedSession {
        console: playing.console,
        title_id: playing.title_id.clone(),
        title: playing.title.clone(),
        started: stamp.to_string(),
        seconds,
        ending,
        machine: playlog::read_machine(&log),
        problems: playlog::read_problems(&log, 40),
        log_file: log_file.to_string_lossy().into_owned(),
    };

    let index = logs.join("sessions.json");
    let mut all: Vec<LoggedSession> = std::fs::read_to_string(&index)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    all.insert(0, session);

    // Keep as many as the user asked for and take the old logs with them, so
    // this never grows without bound on someone's disk.
    let keep = crate::core::settings::Settings::load(&data_dir.join("Omoio").join("settings.json"))
        .keep_sessions;
    for old in all.iter().skip(keep) {
        let _ = std::fs::remove_file(&old.log_file);
    }
    all.truncate(keep);
    if let Ok(text) = serde_json::to_string_pretty(&all) {
        let _ = std::fs::write(&index, text);
    }
}

/// Fills the content area, or the whole screen when the user asked for that
/// or Big Picture is on.
pub fn place(window: &WebviewWindow, session: &Session) {
    let Some(game) = session.window() else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);

    if session.is_fullscreen() || crate::big_picture::is_on() {
        if let Ok(Some(monitor)) = window.current_monitor() {
            let pos = monitor.position();
            let size = monitor.size();
            overlay::place(game, pos.x, pos.y, size.width as i32, size.height as i32);
        }
        return;
    }

    let (Ok(pos), Ok(size)) = (window.inner_position(), window.inner_size()) else {
        return;
    };
    let left = (SIDEBAR * scale).round() as i32;
    let top = (CONTENT_TOP * scale).round() as i32;
    overlay::place(
        game,
        pos.x + left,
        pos.y + top,
        size.width as i32 - left,
        size.height as i32 - top,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_front_is_never_taken_from_another_program() {
        assert_eq!(first_step(Front::Other), Step::Stop);
        for covers in [false, true] {
            for nobody_before in [false, true] {
                assert_eq!(next_step(Front::Other, covers, nobody_before), Step::Stop);
            }
        }
    }

    #[test]
    fn a_hand_over_takes_the_front_from_omoio_the_emulator_and_nobody() {
        assert_eq!(first_step(Front::OmoioMain), Step::Take);
        assert_eq!(first_step(Front::OmoioOther), Step::Take);
        assert_eq!(first_step(Front::Emulator), Step::Take);
        assert_eq!(first_step(Front::Nobody), Step::Take);
        assert_eq!(first_step(Front::Game), Step::Wait);
        // A dialog or message of the emulator's is left to be read.
        assert_eq!(first_step(Front::EmulatorDialog), Step::Stop);
    }

    #[test]
    fn omoios_window_is_taken_back_only_over_a_game_that_fills_the_screen() {
        assert_eq!(next_step(Front::OmoioMain, true, false), Step::Take);
        assert_eq!(next_step(Front::OmoioMain, false, false), Step::Stop);
        // The portal menu, opened again, keeps the front.
        assert_eq!(next_step(Front::OmoioOther, true, false), Step::Stop);
    }

    #[test]
    fn nobody_in_front_waits_one_look_before_it_counts() {
        assert_eq!(next_step(Front::Nobody, true, false), Step::Wait);
        assert_eq!(next_step(Front::Nobody, true, true), Step::Take);
        assert_eq!(next_step(Front::Game, true, true), Step::Wait);
        assert_eq!(next_step(Front::Emulator, false, false), Step::Take);
        assert_eq!(next_step(Front::EmulatorDialog, true, true), Step::Stop);
    }
}
