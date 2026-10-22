//! What Cemu says when it won't start a game.
//!
//! Cemu says why only in a message of its own, never in its log, and starts
//! no log at all for a game it refused: given a disc image whose key it
//! lacked, it showed "Unable to launch game", the path and "Could not
//! decrypt title", and wrote no log.txt (`MainWindow::FileLoad`, lines 471
//! to 556, Cemu v2.6; tried 20 October 2026). The message is centred on
//! Cemu's main window, which Omoio opens off the screen (`OFF_SCREEN`), so
//! nobody saw it and the game seemed to close at once (Discord,
//! support-0025, 18 October 2026). So while a game starts, Omoio reads each
//! message Cemu puts up and closes it. One that says Cemu won't start the
//! game ends Cemu, and the person is told why: in Omoio's words where Omoio
//! knows the case, in Cemu's otherwise, and never with a path, which goes in
//! the session's log only.

use super::portal::{self, children, class, close, is_message, main_window, out_of_sight, title, until, windows_of};
use crate::backends::qt::{self, Automation};
use std::process::Child;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Accessibility::UIA_TextControlTypeId;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, IDNO, WM_USER};

/// How long after Cemu starts its messages are looked for. Cemu refuses a
/// game as it starts (`FileLoad`, posted from `MainWindow`'s constructor,
/// `MainWindow.cpp` lines 309 to 313), and the other messages of a start
/// come while it loads the game: the renderer (`VulkanCanvas.cpp` lines 39
/// to 45), the game's program (`CafeSystem.cpp` lines 172 to 178) and the
/// graphic packs' patches (`GraphicPack2Patches.cpp` lines 41 to 67; all
/// Cemu v2.6). Cemu has taken twenty seconds to open a window in a game's
/// first minute (`portal::START_WAIT`), so a minute leaves room for a slow PC.
const WATCH: Duration = Duration::from_secs(60);
const LOOK_EVERY: Duration = Duration::from_millis(250);
/// How long Cemu may take to close once asked, before it is ended.
const QUIT_WAIT: Duration = Duration::from_secs(5);
/// How soon Cemu has to end after a message for the message to be why. The
/// renderer's and the damaged program's messages end Cemu as they are
/// answered (`exit(0)`, the same lines), in whatever language they are in.
const LAST_WORD: Duration = Duration::from_secs(10);

/// Cemu refusing a game without naming it, so the path doesn't tell
/// (`MainWindow.cpp` line 482, Cemu v2.6).
const REFUSALS: [&str; 1] = ["Unable to launch game because the base files were not found"];

/// The disc image's key isn't in Cemu's keys.txt (`NO_DISC_KEY`,
/// `MainWindow.cpp` lines 536 to 540, Cemu v2.6). Every translation Cemu
/// 2.6 ships keeps the name keys.txt in that sentence (its
/// resources/<language>/cemu.mo, read 20 October 2026), and only a refusal
/// names the game, so the two together tell the case in any language.
const KEYS_FILE: &str = "keys.txt";
/// Worded as the Emulators screen and the import sheet put it
/// (`refusal` in mod.rs). Never where a key comes from.
const NO_KEY: &str = "Cemu couldn't read this disc image because its key isn't among the keys you added. \
                      Add this disc's key under Emulators, Cemu, then start the game again.";
const UNREAD: &str = "Cemu couldn't start the game.";

/// Cemu's one question while a game starts, asked when the game has a
/// shader cache only from before Cemu 1.25.0 and none newer
/// (`LatteShaderCache_handleDeprecatedCacheFiles`, `LatteShaderCache.cpp`
/// lines 906 to 928, called at line 366, Cemu v2.6). Cemu waits for the
/// answer before the game goes on, and its window is off the screen, so
/// nobody could give one and the start hung. Omoio answers No, labelled
/// "Keep outdated cache file" (line 921): Yes, which Cemu recommends,
/// deletes the old files (lines 923 to 927), and they are the player's.
/// Either answer lets the game go on, and the question doesn't come back,
/// since Cemu then makes the newer cache it looks for (lines 369 to 371).
/// Known by its caption in English, the language Omoio starts Cemu in
/// (`ENGLISH` in mod.rs), or in any other language by the version its words
/// name: every translation Cemu 2.6 ships that has the question keeps
/// "1.25.0" (resources/<language>/cemu.mo, read 21 October 2026), and no
/// other message of Cemu's names it.
const CACHE_QUESTION: &str = "Outdated shader cache";
const CACHE_VERSION: &str = "1.25.0";
const CACHE_ANSWER: &str = "Keep outdated cache file";

/// Clicks a task dialog's button by its id ("TDM_CLICK_BUTTON message",
/// Microsoft Learn, read 21 October 2026; `WM_USER + 102` in CommCtrl.h).
/// wxWidgets gives a question's No button the id IDNO even under a label
/// of Cemu's own (`AddTaskDialogButton`, `msgdlg.cpp` lines 761 to 783,
/// wxWidgets 3.2.5). Only a number travels, so it works across processes
/// and with the dialog off the screen; the buttons' own control ids are 0,
/// so pressing one as `portal::press` does finds nothing (both seen with a
/// task dialog built as wxWidgets builds this one, 21 October 2026).
const TDM_CLICK_BUTTON: u32 = WM_USER + 102;

/// Why Cemu didn't start a game.
pub struct Refusal {
    /// What the person playing is told, without any path.
    pub shown: String,
    /// All Cemu said, path and all, for the session's log.
    pub said: String,
}

/// A message Cemu put up, and whether it said the game won't start.
struct Heard {
    pid: u32,
    at: Instant,
    ends_game: bool,
    /// The answer Omoio gave, for a question; never why a game didn't start.
    answer: Option<&'static str>,
    refusal: Refusal,
}

static HEARD: Mutex<Vec<Heard>> = Mutex::new(Vec::new());

fn heard() -> std::sync::MutexGuard<'static, Vec<Heard>> {
    HEARD.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Why the Cemu that was `pid` didn't start its game, asked once it has
/// gone: a message that refused the game, or the last one it put up right
/// before it ended by itself. Any other message it put up stays for
/// `notes`.
pub fn take(pid: u32) -> Option<Refusal> {
    let mut all = heard();
    let at = all
        .iter()
        .rposition(|said| said.pid == pid && said.answer.is_none() && counts(said.ends_game, said.at.elapsed()))?;
    Some(all.remove(at).refusal)
}

/// Every other message the Cemu that was `pid` put up and Omoio closed, such
/// as a graphic pack that couldn't be applied, and every question Omoio
/// answered, for the log kept of the game, so what was closed unseen is
/// still there to read. Forgets them.
pub fn notes(pid: u32) -> Vec<String> {
    let mut all = heard();
    let (mine, others): (Vec<Heard>, Vec<Heard>) = all.drain(..).partition(|said| said.pid == pid);
    *all = others;
    mine.into_iter()
        .map(|said| {
            let words = said.refusal.said.replace('\n', " ");
            match said.answer {
                Some(answer) => format!("Omoio answered a question from Cemu with \"{answer}\": {words}"),
                None => format!("Omoio closed a message from Cemu: {words}"),
            }
        })
        .collect()
}

/// Whether a message is why Cemu ended: one that refused the game always,
/// any other only when Cemu ended straight after it.
fn counts(ends_game: bool, since: Duration) -> bool {
    ends_game || since <= LAST_WORD
}

/// Watches the newly started Cemu for `game`, on a thread of its own, for
/// as long as the game is starting.
///
/// Every message of Cemu's is a task dialog, the kind Windows draws itself:
/// wxWidgets 3.2.5, which Cemu 2.6 is built with, shows one whenever
/// comctl32 6 is there, which Cemu's manifest asks for (`msgdlg.cpp` lines
/// 539 to 577). Its text is no window of its own; UI Automation reads it as
/// a text whose id is ContentText (seen 20 October 2026). A message with
/// only OK really has one Cancel button labelled OK, so closing the window
/// answers it as OK (lines 735 to 748, and `ShowModal`, lines 563 to 571).
/// A question has two buttons and can't be closed that way. Cemu's one
/// question while a game starts, whether to delete an outdated shader cache,
/// is not an error and is answered as Cemu recommends (`CACHE_QUESTION`);
/// any other question is left as it is. The portal menu's errands read
/// Cemu's messages for themselves (`portal::dismiss_message`), so while one
/// is under way nothing is touched here.
pub fn watch(mut cemu: Child, game: String) {
    std::thread::spawn(move || {
        let pid = cemu.id();
        heard().retain(|said| said.pid != pid);
        let automation = Automation::new();
        let end = Instant::now() + WATCH;
        let mut seen: Vec<isize> = Vec::new();
        while Instant::now() < end && running(&mut cemu) {
            std::thread::sleep(LOOK_EVERY);
            if portal::busy() {
                continue;
            }
            let Some(message) = windows_of(pid)
                .into_iter()
                .find(|&window| !seen.contains(&(window.0 as isize)) && is_box(window))
            else {
                continue;
            };
            // A message newly shown may not have its buttons or its words yet,
            // so it is looked at again until it does.
            let question = match buttons(message) {
                0 => continue,
                1 => false,
                2 => true,
                _ => {
                    seen.push(message.0 as isize);
                    continue;
                }
            };
            let said = read(automation.as_ref(), message);
            if question {
                let caption = title(message);
                if !is_cache_question(&caption, &said, &game) {
                    // Its words may not be there yet; it is looked at again
                    // until they are.
                    if !said.trim().is_empty() {
                        seen.push(message.0 as isize);
                    }
                    continue;
                }
                seen.push(message.0 as isize);
                out_of_sight(message);
                let _ = unsafe { PostMessageW(Some(message), TDM_CLICK_BUTTON, WPARAM(IDNO.0 as usize), LPARAM(0)) };
                heard().push(Heard {
                    pid,
                    at: Instant::now(),
                    ends_game: false,
                    answer: Some(CACHE_ANSWER),
                    refusal: Refusal {
                        shown: String::new(),
                        said: if said.trim().is_empty() { caption } else { said },
                    },
                });
                continue;
            }
            if said.trim().is_empty() {
                continue;
            }
            seen.push(message.0 as isize);
            out_of_sight(message);
            close(message);
            let understood = understand(&said, &game);
            let ends_game = understood.ends_game;
            heard().push(Heard {
                pid,
                at: Instant::now(),
                ends_game,
                answer: None,
                refusal: Refusal { shown: understood.shown, said },
            });
            if ends_game {
                quit(&mut cemu);
                return;
            }
        }
    });
}

fn running(cemu: &mut Child) -> bool {
    cemu.try_wait().is_ok_and(|status| status.is_none())
}

/// Asks Cemu to close, as its own window's close button does, and ends it
/// if it hasn't within `QUIT_WAIT`. With no game started it closes at once
/// (tried 20 October 2026).
fn quit(cemu: &mut Child) {
    if let Some(main) = main_window(cemu.id()) {
        close(main);
    }
    until(QUIT_WAIT, || !running(cemu));
    if running(cemu) {
        let _ = cemu.kill();
        let _ = cemu.wait();
    }
}

/// Whether a window is a message: a task dialog, or the plain message box
/// Windows shows without one, which holds only its text and buttons. Not
/// one of the windows the portal opens, nor a window of Cemu's own made
/// with wxWidgets.
fn is_box(window: HWND) -> bool {
    if !is_message(window, &portal::OWN_WINDOWS) {
        return false;
    }
    let inside: Vec<String> = children(window).into_iter().map(class).collect();
    inside.iter().any(|kind| kind == "DirectUIHWND")
        || (!inside.is_empty() && inside.iter().all(|kind| kind == "Static" || kind == "Button"))
}

/// A task dialog's buttons are windows of their own inside it (seen 20
/// October 2026), so they are counted without reading their words.
fn buttons(message: HWND) -> usize {
    children(message).into_iter().filter(|&child| class(child) == "Button").count()
}

/// Whether a question with this caption and these words is Cemu's question
/// about an outdated shader cache. Paths are left out of the words first: a
/// folder named after a Cemu version must not make another question look
/// like this one.
fn is_cache_question(caption: &str, said: &str, game: &str) -> bool {
    caption == CACHE_QUESTION || plain_words(said, game).contains(CACHE_VERSION)
}

/// All a message says. A plain message box keeps its text in a Static.
fn read(automation: Option<&Automation>, message: HWND) -> String {
    let texts: Vec<String> = match automation.and_then(|automation| Some((automation, automation.window(message)?))) {
        Some((automation, element)) => automation
            .of_kind(&element, UIA_TextControlTypeId)
            .iter()
            .map(qt::name)
            .collect(),
        None => Vec::new(),
    };
    let texts: Vec<String> = if texts.iter().any(|text| !text.trim().is_empty()) {
        texts
    } else {
        children(message)
            .into_iter()
            .filter(|&child| class(child) == "Static")
            .map(portal::text)
            .collect()
    };
    texts.into_iter().filter(|text| !text.trim().is_empty()).collect::<Vec<_>>().join("\n\n")
}

struct Understood {
    ends_game: bool,
    shown: String,
}

/// What a message of Cemu's means for the game at `game`.
fn understand(said: &str, game: &str) -> Understood {
    let names_game = mentions(said, game);
    let ends_game = names_game || REFUSALS.iter().any(|refusal| said.trim_start().starts_with(refusal));
    let words = plain_words(said, game);
    let shown = if names_game && said.contains(KEYS_FILE) {
        NO_KEY.to_string()
    } else if words.is_empty() {
        UNREAD.to_string()
    } else {
        format!("Cemu couldn't start the game: {words}")
    };
    Understood { ends_game, shown }
}

/// Paths compared the way Windows does, whichever slash Cemu writes. It
/// writes forward ones (seen 20 October 2026).
fn comparable(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn mentions(said: &str, game: &str) -> bool {
    !game.trim().is_empty() && comparable(said).contains(&comparable(game))
}

/// Cemu's words without any path, as sentences on one line. A line that
/// held only a path goes, with the label before it, such as "Path:" or
/// "Pfad:", which is how Cemu names a path in every language it speaks.
fn plain_words(said: &str, game: &str) -> String {
    let mut kept: Vec<String> = Vec::new();
    for line in said.lines() {
        let (rest, had_path) = without_paths(line, game);
        let rest = rest.trim();
        if had_path && (rest.is_empty() || is_label(rest)) {
            if rest.is_empty() && kept.last().is_some_and(|before| is_label(before)) {
                kept.pop();
            }
            if let Some(before) = rest.rfind(['.', '!', '?']) {
                kept.push(rest[..=before].to_string());
            }
            continue;
        }
        if !rest.is_empty() {
            kept.push(rest.to_string());
        }
    }
    kept.into_iter()
        .map(|line| if line.ends_with(['.', '!', '?', ':', '。', '！', '？', '：']) { line } else { line + "." })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_label(line: &str) -> bool {
    line.ends_with([':', '：'])
}

/// The line up to the first path in it, and whether it had one: the game's
/// own, wherever it is, or any other that starts with a drive or a share.
/// Cemu puts a path last on its line (`MainWindow.cpp`, `CemuApp.cpp`,
/// v2.6).
fn without_paths(line: &str, game: &str) -> (String, bool) {
    let mut rest = line.to_string();
    let mut had_path = false;
    if !game.trim().is_empty() {
        let wanted = comparable(game);
        while let Some(at) = comparable(&rest).find(&wanted) {
            rest.replace_range(at..at + wanted.len(), "");
            had_path = true;
        }
    }
    if let Some(at) = path_start(&rest) {
        rest.truncate(at);
        had_path = true;
    }
    (rest, had_path)
}

/// Where a path like `C:/…`, `C:\…` or `//server/…` starts, at the start of
/// the line or after a space or a quote.
fn path_start(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let starts_word = |at: usize| at == 0 || matches!(bytes[at - 1], b' ' | b'\t' | b'"' | b'\'' | b'(');
    let slash = |byte: u8| byte == b'/' || byte == b'\\';
    (0..bytes.len()).find(|&at| {
        starts_word(at)
            && ((bytes[at].is_ascii_alphabetic()
                && bytes.get(at + 1) == Some(&b':')
                && bytes.get(at + 2).is_some_and(|&byte| slash(byte)))
                || (slash(bytes[at]) && bytes.get(at + 1).is_some_and(|&byte| slash(byte))))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAME: &str = "D:\\Wii U\\Skylanders Giants [ABCD41]\\game.wud";

    /// What Cemu 2.6 showed for a disc image it had no key for, as UI
    /// Automation read it (20 October 2026), with this game's path.
    fn no_key_said() -> String {
        "Unable to launch game\nPath:\nD:/Wii U/Skylanders Giants [ABCD41]/game.wud\n\n\
         Could not decrypt title. Make sure that keys.txt contains the correct disc key for this title."
            .to_string()
    }

    #[test]
    fn a_missing_key_is_told_in_omoios_words() {
        let understood = understand(&no_key_said(), GAME);
        assert!(understood.ends_game);
        assert_eq!(understood.shown, NO_KEY);
        assert!(!understood.shown.contains("D:"));
    }

    #[test]
    fn a_missing_key_is_known_in_cemus_other_languages() {
        // Cemu 2.6's German, from resources/de/cemu.mo.
        let said = "Unfähig Spiel zu starten\nPfad:\nD:/Wii U/Skylanders Giants [ABCD41]/game.wud\n\n\
                    Konnte Titel nicht entschlüsseln. Stelle sicher, dass \"keys.txt\" den korrekten Schlüssel \
                    für diesen Titel beinhaltet.";
        let understood = understand(said, GAME);
        assert!(understood.ends_game);
        assert_eq!(understood.shown, NO_KEY);
    }

    #[test]
    fn other_refusals_are_told_in_cemus_words_without_the_path() {
        let said = "Unable to launch game\nPath:\nD:/Wii U/Skylanders Giants [ABCD41]/game.wud\n\n\
                    Could not decrypt title because title.tik is missing.";
        let understood = understand(said, GAME);
        assert!(understood.ends_game);
        assert_eq!(
            understood.shown,
            "Cemu couldn't start the game: Unable to launch game. Could not decrypt title because title.tik is missing."
        );

        // The path straight after the words, with no space (`MainWindow.cpp`
        // line 502).
        let said = "Failed to launch game.D:/Wii U/Skylanders Giants [ABCD41]/game.wud";
        assert_eq!(understand(said, GAME).shown, "Cemu couldn't start the game: Failed to launch game.");

        let said = "Failed to launch executable. Path: D:/Wii U/Skylanders Giants [ABCD41]/game.wud";
        assert_eq!(understand(said, GAME).shown, "Cemu couldn't start the game: Failed to launch executable.");

        let said = "Unable to mount title.\n\
                    Make sure the configured game paths are still valid and refresh the game list.\n\n\
                    File which failed to load:\nD:\\Wii U\\Skylanders Giants [ABCD41]\\game.wud";
        let understood = understand(said, GAME);
        assert!(understood.ends_game);
        assert_eq!(
            understood.shown,
            "Cemu couldn't start the game: Unable to mount title. \
             Make sure the configured game paths are still valid and refresh the game list."
        );
    }

    #[test]
    fn a_refusal_that_doesnt_name_the_game_still_ends_it() {
        let understood = understand("Unable to launch game because the base files were not found.", GAME);
        assert!(understood.ends_game);
        assert_eq!(
            understood.shown,
            "Cemu couldn't start the game: Unable to launch game because the base files were not found."
        );
    }

    #[test]
    fn a_message_that_lets_the_game_go_on_doesnt_end_it() {
        let understood = understand("Failed to apply patches for graphic pack 'Portal Stability Fix'", GAME);
        assert!(!understood.ends_game);
        assert_eq!(
            understood.shown,
            "Cemu couldn't start the game: Failed to apply patches for graphic pack 'Portal Stability Fix'."
        );
        let bad_line = understand("Error in keys.txt at line 3", GAME);
        assert!(!bad_line.ends_game, "names no game, so not a missing key");
        assert_eq!(bad_line.shown, "Cemu couldn't start the game: Error in keys.txt at line 3.");
    }

    #[test]
    fn any_path_is_left_out() {
        let said = "Cemu failed to write to the mlc directory.\nThe path is:\n\
                    C:/Users/Someone/AppData/Local/Omoio/cemu/portable/mlc01";
        assert_eq!(
            understand(said, GAME).shown,
            "Cemu couldn't start the game: Cemu failed to write to the mlc directory."
        );
        let said = "Unable to launch game\nPath:\n//nas/games/Giants.wud";
        assert_eq!(understand(said, "").shown, "Cemu couldn't start the game: Unable to launch game.");
        // Words that only look a little like a path stay.
        assert_eq!(path_start("See https://cemu.info or keys.txt"), None);
    }

    #[test]
    fn a_message_with_nothing_left_says_so_plainly() {
        let said = "D:/Wii U/Skylanders Giants [ABCD41]/game.wud";
        assert_eq!(understand(said, GAME).shown, UNREAD);
    }

    #[test]
    fn a_refusal_is_why_whenever_cemu_ends_and_another_message_only_just_before() {
        assert!(counts(true, Duration::from_secs(3600)));
        assert!(counts(false, Duration::from_secs(1)));
        assert!(!counts(false, LAST_WORD + Duration::from_secs(1)));
    }

    /// Needs a Cemu 2.6 of its own, never the one Omoio keeps, and a disc
    /// image it has no key for (a file of zeros named .wud will do), so it
    /// runs only by hand:
    /// `OMOIO_CEMU=<Cemu.exe> OMOIO_DISC=<file.wud> cargo test a_refusal_is_read_from_a_real_cemu -- --ignored`
    #[test]
    #[ignore]
    fn a_refusal_is_read_from_a_real_cemu() {
        let exe = std::env::var("OMOIO_CEMU").unwrap();
        let disc = std::env::var("OMOIO_DISC").unwrap();
        let cemu = super::super::command(std::path::Path::new(&exe)).arg("-g").arg(&disc).spawn().unwrap();
        let pid = cemu.id();
        watch(cemu, disc);
        until(WATCH, || heard().iter().any(|said| said.pid == pid));
        until(QUIT_WAIT + Duration::from_secs(1), || main_window(pid).is_none());
        assert!(main_window(pid).is_none(), "Cemu was closed");
        let refusal = take(pid).expect("Cemu's message was read");
        assert_eq!(refusal.shown, NO_KEY);
        assert!(refusal.said.contains("Could not decrypt title"), "{}", refusal.said);
    }

    #[test]
    fn the_shader_cache_question_is_known_in_any_language_and_no_other_is() {
        // Cemu 2.6's English, and its German from resources/de/cemu.mo, as
        // the task dialog shows them (wxWidgets puts the whole message in
        // its content, as it has no blank line after the first).
        let english = "Cemu detected that the shader cache for this game is outdated.\n\
                       Only shader caches generated with Cemu 1.25.0 or above are supported.\n\n\
                       We recommend deleting the outdated cache file as it will no longer be used by Cemu.";
        assert!(is_cache_question("Outdated shader cache", english, GAME));
        assert!(is_cache_question("Outdated shader cache", "", GAME), "before its words are there");
        let german = "Cemu erkannte dass der Shader-Zwischenspeicher für dieses Spiel veraltet ist.\n\
                      Nur Shader-Zwischenspeicher die mit Cemu 1.25.0 oder höher generiert wurden sind unterstützt.";
        assert!(is_cache_question("Shader-Zwischenspeicher veraltet", german, GAME));
        // Cemu's question when its mlc folder can't be used (`CemuApp.cpp`
        // lines 229 to 233, v2.6).
        let mlc = "Cemu failed to write to the custom mlc directory.\n\nThe path is:\nD:/Cemu 1.25.0/mlc01\n\n\
                   Cemu cannot start without a valid mlc path. Do you want to reset the path? \
                   You can later change it again in the General Settings.";
        assert!(!is_cache_question("Error", mlc, GAME), "1.25.0 only in a path");
        assert!(!is_cache_question("Error", "", GAME));
    }

    #[test]
    fn an_answered_question_goes_in_the_log_and_is_never_why_cemu_ended() {
        heard().push(Heard {
            pid: 900_003,
            at: Instant::now(),
            ends_game: false,
            answer: Some(CACHE_ANSWER),
            refusal: Refusal { shown: String::new(), said: "Cemu detected that\nthe cache".to_string() },
        });
        assert!(take(900_003).is_none(), "even when Cemu ends straight after");
        assert_eq!(
            notes(900_003),
            ["Omoio answered a question from Cemu with \"Keep outdated cache file\": Cemu detected that the cache"]
        );
    }

    #[test]
    fn what_was_heard_is_taken_once_and_only_for_its_cemu() {
        let note = |pid: u32, ends_game: bool, shown: &str| Heard {
            pid,
            at: Instant::now(),
            ends_game,
            answer: None,
            refusal: Refusal { shown: shown.to_string(), said: String::new() },
        };
        heard().extend([note(900_001, true, "first"), note(900_002, false, "other"), note(900_001, false, "later")]);
        assert_eq!(take(900_001).map(|refusal| refusal.shown).as_deref(), Some("later"));
        assert_eq!(notes(900_001).len(), 1, "the other message stays for the log");
        assert!(take(900_001).is_none());
        assert!(notes(900_001).is_empty());
        assert_eq!(take(900_002).map(|refusal| refusal.shown).as_deref(), Some("other"));
    }
}
