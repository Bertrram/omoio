//! Telling RPCS3 about a game and starting it.

use crate::core::library::Game;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

/// Where a dump keeps the binary RPCS3 boots. A disc dump puts it under
/// PS3_GAME; an installed PSN title keeps USRDIR at the top.
fn eboot_path(root: &Path) -> Option<PathBuf> {
    let candidates = [
        root.join("PS3_GAME").join("USRDIR").join("EBOOT.BIN"),
        root.join("USRDIR").join("EBOOT.BIN"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// RPCS3 keeps its own list of games as `TITLE_ID: path/`, one per line, with
/// forward slashes and a trailing slash. Read off a working install rather than
/// guessed at, and written back the same way so RPCS3 recognises its own file.
fn games_list_line(title_id: &str, root: &Path) -> String {
    let mut path = root.to_string_lossy().replace('\\', "/");
    if !path.ends_with('/') {
        path.push('/');
    }
    // Windows forbids quotes and backslashes in names, so the quoted form needs
    // no escaping, and it survives a folder called something like "Game #1"
    // which unquoted YAML would read as a comment.
    format!("{title_id}: \"{path}\"")
}

/// Replaces this game's line and leaves every other one untouched. The file
/// belongs to RPCS3 and may list games Omoio knows nothing about, so it is
/// edited rather than rewritten.
fn merged_list(existing: &str, title_id: &str, root: &Path) -> String {
    let prefix = format!("{title_id}:");
    let mut lines: Vec<&str> = existing
        .lines()
        .filter(|line| !line.trim_start().starts_with(&prefix))
        .filter(|line| !line.trim().is_empty())
        .collect();
    let ours = games_list_line(title_id, root);
    lines.push(&ours);

    let mut out = lines.join("\n");
    out.push('\n');
    out
}

pub fn register(app: &AppHandle, game: &Game) -> Result<(), String> {
    let config_dir = super::install_dir(app)?.join("config");
    std::fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
    let list = config_dir.join("games.yml");

    let existing = std::fs::read_to_string(&list).unwrap_or_default();
    let merged = merged_list(&existing, &game.title_id, &game.path);
    std::fs::write(&list, merged).map_err(|e| e.to_string())
}

/// RPCS3 greets a fresh install with a window about itself, its funding and
/// its piracy policy. The user asked for their game, so the greeting is turned
/// off the same way clicking its checkbox would, before the emulator ever runs.
fn with_welcome_disabled(existing: &str) -> String {
    const SECTION: &str = "[main_window]";
    const KEY: &str = "infoBoxEnabledWelcome";

    let mut lines: Vec<String> = Vec::new();
    let mut in_section = false;
    let mut written = false;

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            // Leaving the section without having seen the key: add it here.
            if in_section && !written {
                lines.push(format!("{KEY}=false"));
                written = true;
            }
            in_section = trimmed == SECTION;
        } else if in_section && trimmed.starts_with(KEY) {
            lines.push(format!("{KEY}=false"));
            written = true;
            continue;
        }
        lines.push(line.to_string());
    }

    if !written {
        if !in_section {
            lines.push(String::new());
            lines.push(SECTION.to_string());
        }
        lines.push(format!("{KEY}=false"));
    }

    let mut out = lines.join("\n");
    out.push('\n');
    out
}

fn disable_welcome_screen(app: &AppHandle) -> Result<(), String> {
    let dir = super::install_dir(app)?.join("GuiConfigs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join("CurrentSettings.ini");
    let existing = std::fs::read_to_string(&file).unwrap_or_default();
    if existing.contains("infoBoxEnabledWelcome=false") {
        return Ok(());
    }
    std::fs::write(&file, with_welcome_disabled(&existing)).map_err(|e| e.to_string())
}

/// RPCS3 reads the pad even while its window is not the one in front:
/// `IO / Background input enabled` ships on (`Emu/system_config.h`), and the
/// pad thread only asks `is_input_allowed()`, which is that setting or the
/// game window having focus (`Emu/RSX/GSFrameBase.cpp`). Switched off, a game
/// waiting behind Big Picture doesn't take the presses meant for Omoio. A
/// value chosen for one game in its own settings still wins over this one.
fn keep_pad_to_the_game(app: &AppHandle) {
    switch_off(app, "Background input enabled");
}

/// Omoio sizes and places the game window itself. Started in RPCS3's own
/// fullscreen, the window left it at a press of Esc in the game and came back
/// with its frame, out of Omoio's window.
fn leave_fullscreen_to_omoio(app: &AppHandle) {
    switch_off(app, "Start games in fullscreen mode");
}

/// One of RPCS3's settings in its config.yml, switched off before a game
/// starts.
fn switch_off(app: &AppHandle, key: &str) {
    let Ok(dir) = super::install_dir(app) else {
        return;
    };
    let path = dir.join("config").join("config.yml");
    let Ok(config) = std::fs::read_to_string(&path) else {
        return;
    };
    let updated = super::account::replace_setting(&config, key, "false");
    if updated != config {
        let _ = std::fs::write(&path, updated);
    }
}

pub fn launch(app: &AppHandle, game: &Game) -> Result<u32, String> {
    let exe = super::exe_path(app)?;
    if !exe.exists() {
        return Err("Install RPCS3 first, then you can play.".to_string());
    }
    if super::firmware::detect_version(app).is_none() {
        return Err("Add PS3 firmware first, then you can play.".to_string());
    }
    if !game.path.is_dir() {
        return Err("This game's folder isn't there. Reconnect the drive it's on.".to_string());
    }
    let eboot = eboot_path(&game.path)
        .ok_or("Couldn't find the game's program file. This folder may be incomplete.")?;

    // Before RPCS3's files are written, so one on its way out doesn't save
    // its own over them.
    super::end_running(&exe);
    register(app, game)?;
    disable_welcome_screen(app)?;
    keep_pad_to_the_game(app);
    leave_fullscreen_to_omoio(app);

    // --no-gui keeps RPCS3's own window out of the way: the user asked to play
    // a game, not to meet the emulator. A Skylanders game needs that window
    // for its portal, so it starts with it and Omoio hides it; see portal.rs.
    // Spawned rather than waited on, so Omoio stays usable while the game runs.
    let skylanders = crate::core::figures::is_skylanders(&game.title);
    let mut command = super::command(&exe);
    if skylanders {
        command.env("QT_QPA_PLATFORM", super::portal::QT_PLATFORM);
    } else {
        command.arg("--no-gui");
    }
    let child = command.arg(&eboot).spawn().map_err(|e| e.to_string())?;
    if skylanders {
        super::portal::prepare(child.id());
    }
    Ok(child.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RPCS3 reports a crash on the way out of a headless run even when the
    /// package went in, so success is read from the log. RPCS3 starts the log
    /// afresh on every run, so all of it counts, but only when it was written
    /// after the run began: an older log is a previous run's.
    #[test]
    fn reads_success_only_from_a_log_this_run_wrote() {
        let log = std::env::temp_dir().join(format!("omoio-pkg-log-{}.txt", std::process::id()));
        let success = "\u{b7}S GUI: Successfully installed D:/updates/GAME-A0133.pkg (version=01.33).\n";

        // A log left by an earlier run says nothing about this one.
        std::fs::write(&log, format!("{success}{}", "\u{b7}! an earlier, longer run\n".repeat(50))).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let started = std::time::SystemTime::now();
        assert_eq!(this_run(&log, started), "", "a previous run's log must not count");

        // The new run rewrites the log from the start, shorter than the last
        // one, which is what the old offset-based reading missed.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&log, success).unwrap();
        assert!(this_run(&log, started).contains("Successfully installed"));

        let _ = std::fs::remove_file(&log);
    }

    /// Updates follow on from each other, and RPCS3 names the one it wanted.
    /// Passing that on turns a dead end into an instruction.
    #[test]
    fn says_which_version_an_update_follows_on_from() {
        // Both wordings RPCS3 uses, taken from real refusals.
        let mismatch = "\u{b7}E {PKG Installer} PKG: The installed app version (01.33) does not match the target app version (01.32)\n";
        assert_eq!(target_version(mismatch).as_deref(), Some("01.32"));

        let nothing_installed = "\u{b7}E {PKG Installer} PKG: A target app version is required (01.32), but no PARAM.SFO was found for BCES00850.\n";
        assert_eq!(target_version(nothing_installed).as_deref(), Some("01.32"));

        assert_eq!(target_version("\u{b7}! nothing of the sort\n"), None);
    }

    #[test]
    fn writes_the_line_the_way_rpcs3_does() {
        let line = games_list_line(
            "BCES00850",
            Path::new("C:\\Games\\PS3\\LittleBigPlanet 2"),
        );
        assert_eq!(
            line,
            "BCES00850: \"C:/Games/PS3/LittleBigPlanet 2/\""
        );
    }

    #[test]
    fn does_not_double_the_trailing_slash() {
        let line = games_list_line("BCES00141", Path::new("D:/games/LBP/"));
        assert_eq!(line, "BCES00141: \"D:/games/LBP/\"");
    }

    #[test]
    fn turns_off_the_welcome_screen_in_an_existing_config() {
        let existing = "[GSFrame]\nvisibility=Windowed\n\n[main_window]\ninfoBoxEnabledWelcome=true\n\n[Meta]\nattachCommandLine=false\n";
        let out = with_welcome_disabled(existing);

        assert!(out.contains("infoBoxEnabledWelcome=false"));
        assert!(!out.contains("infoBoxEnabledWelcome=true"));
        // Everything else RPCS3 keeps in there survives.
        assert!(out.contains("[GSFrame]"));
        assert!(out.contains("visibility=Windowed"));
        assert!(out.contains("attachCommandLine=false"));
    }

    #[test]
    fn adds_the_setting_when_the_section_has_no_such_key() {
        let out = with_welcome_disabled("[main_window]\nsomethingElse=1\n\n[Meta]\nx=2\n");
        assert!(out.contains("infoBoxEnabledWelcome=false"));
        assert!(out.contains("somethingElse=1"));
        assert!(out.contains("[Meta]"));
        // It has to land inside main_window, not after Meta.
        let welcome = out.find("infoBoxEnabledWelcome").unwrap();
        assert!(welcome < out.find("[Meta]").unwrap());
    }

    #[test]
    fn writes_a_whole_config_when_there_is_none() {
        let out = with_welcome_disabled("");
        assert!(out.contains("[main_window]"));
        assert!(out.contains("infoBoxEnabledWelcome=false"));
    }

    #[test]
    fn keeps_games_it_did_not_put_there() {
        let existing = "BCES00141: C:/games/LBP/\nNPEA00243: C:/games/Sackboy/\n";
        let merged = merged_list(&existing, "BCES00850", Path::new("D:/games/LBP2"));

        assert!(merged.contains("BCES00141: C:/games/LBP/"));
        assert!(merged.contains("NPEA00243: C:/games/Sackboy/"));
        assert!(merged.contains("BCES00850: \"D:/games/LBP2/\""));
        assert_eq!(merged.lines().count(), 3);
    }

    #[test]
    fn replaces_a_game_instead_of_listing_it_twice() {
        let existing = "BCES00850: C:/old/place/\nBCES00141: C:/games/LBP/\n";
        let merged = merged_list(&existing, "BCES00850", Path::new("D:/new/place"));

        assert!(!merged.contains("C:/old/place"));
        assert!(merged.contains("BCES00850: \"D:/new/place/\""));
        assert_eq!(merged.lines().count(), 2);
    }

    #[test]
    fn writes_a_usable_list_from_nothing() {
        let merged = merged_list("", "BCES00850", Path::new("D:/games/LBP2"));
        assert_eq!(merged, "BCES00850: \"D:/games/LBP2/\"\n");
    }

    #[test]
    fn finds_the_binary_in_both_dump_shapes() {
        let root = std::env::temp_dir().join(format!("omoio-eboot-{}", std::process::id()));
        let disc = root.join("disc");
        let hdd = root.join("hdd");
        std::fs::create_dir_all(disc.join("PS3_GAME").join("USRDIR")).unwrap();
        std::fs::create_dir_all(hdd.join("USRDIR")).unwrap();
        std::fs::write(disc.join("PS3_GAME").join("USRDIR").join("EBOOT.BIN"), b"x").unwrap();
        std::fs::write(hdd.join("USRDIR").join("EBOOT.BIN"), b"x").unwrap();

        assert_eq!(
            eboot_path(&disc),
            Some(disc.join("PS3_GAME").join("USRDIR").join("EBOOT.BIN"))
        );
        assert_eq!(eboot_path(&hdd), Some(hdd.join("USRDIR").join("EBOOT.BIN")));
        assert_eq!(eboot_path(&root.join("nothing")), None);

        let _ = std::fs::remove_dir_all(&root);
    }
}

/// Hands a downloaded update to RPCS3 to install.
///
/// `--installpkg` is RPCS3's own flag, confirmed against the installed build's
/// help output. It is paired with `--headless` rather than `--no-gui`, which
/// was tried first and never returns: without a window there is still an event
/// loop, and it sits there once the work is done. Headless installs and exits
/// in a few seconds.
///
/// The exit code is not the answer, though. RPCS3 falls over on the way out of
/// a headless run and reports a crash even when the package went in perfectly,
/// so success is read from what it wrote in its own log instead.
pub fn install_package(app: &AppHandle, package: &std::path::Path) -> Result<(), String> {
    let exe = super::exe_path(app)?;
    if !exe.exists() {
        return Err("Install RPCS3 first, then packages can be installed.".to_string());
    }
    super::refuse_while_running(app)?;

    let log = super::install_dir(app)?.join("log").join("RPCS3.log");
    let started = std::time::SystemTime::now();

    super::command(&exe)
        .arg("--headless")
        .arg("--installpkg")
        .arg(package)
        .status()
        .map_err(|e| e.to_string())?;

    let written = this_run(&log, started);
    let name = package.file_name().unwrap_or_default().to_string_lossy();
    if written
        .lines()
        .any(|line| line.contains("Successfully installed") && line.contains(name.as_ref()))
    {
        return Ok(());
    }

    // Updates are sequential: each package expects the one before it. RPCS3
    // says exactly which version it wanted, and passing that on saves the user
    // guessing why a perfectly good download was refused.
    if let Some(wanted) = target_version(&written) {
        return Err(format!(
            "This update follows on from version {wanted}. Install that one first."
        ));
    }
    Err("RPCS3 couldn't install that package.".to_string())
}

/// The version a refused package was expecting.
///
/// RPCS3 says it two ways, depending on whether the game has an update
/// installed already:
///
/// - "The installed app version (01.33) does not match the target app version
///   (01.32)"
/// - "A target app version is required (01.32), but no PARAM.SFO was found"
///
/// Both name the wanted version in brackets right after "target app version",
/// which is what this reads.
fn target_version(log: &str) -> Option<String> {
    let line = log
        .lines()
        .find(|line| line.contains("target app version"))?;
    let after = line.rsplit_once("target app version")?.1;
    let start = after.find('(')? + 1;
    let end = after[start..].find(')')? + start;
    let version = &after[start..end];
    // Only a version, never a stray sentence from some other message.
    if version.is_empty() || !version.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    Some(version.to_string())
}

/// What RPCS3 wrote during the run that began at `started`.
///
/// RPCS3 starts its log afresh on every run, so the whole file is this run's.
/// Reading on from where the previous log ended, as this once did, skipped
/// straight past a shorter new log and reported a package that went in
/// perfectly as refused. A log older than `started` is from before: RPCS3
/// never got as far as writing one, and it says nothing about this run.
fn this_run(log: &std::path::Path, started: std::time::SystemTime) -> String {
    let fresh = std::fs::metadata(log)
        .and_then(|meta| meta.modified())
        .is_ok_and(|modified| modified >= started);
    if !fresh {
        return String::new();
    }
    String::from_utf8_lossy(&std::fs::read(log).unwrap_or_default()).into_owned()
}
