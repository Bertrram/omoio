//! Bringing over what the user's own Cemu has: their keys and their saves.
//!
//! Omoio runs a Cemu of its own, kept in its folder, so a Cemu the user already
//! had goes on as before and nothing done in one shows in the other. People
//! took that the wrong way round: one changed his own Cemu and expected Omoio
//! to follow, another had his keys in his own Cemu and none in Omoio's. This
//! copies the keys and saved games across once, when the user asks.
//!
//! The user's Cemu is only ever read. Nothing in Omoio's is overwritten unless
//! the user says so for that one game, and then what was there is moved aside
//! first, as restoring an RPCS3 backup does.

use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// Where a Cemu keeps the two things brought over.
#[derive(Debug)]
pub(super) struct Places {
    /// The folder the rest were found in, shown to the user.
    root: PathBuf,
    keys: PathBuf,
    /// mlc01/usr/save, which holds one folder per game.
    saves: PathBuf,
}

/// Where a Cemu keeps its user data, from the folder the user picked.
///
/// Cemu 2 keeps it in `portable` beside Cemu.exe when that folder is there,
/// and otherwise in %APPDATA%\Cemu, while Cemu 1 kept everything beside
/// Cemu.exe (`ActiveSettings.cpp`, v2.6). People pick either the folder
/// Cemu.exe is in or the data folder itself, so both are understood.
pub(super) fn places(picked: &Path, appdata_cemu: Option<&Path>) -> Result<Places, String> {
    let holds_data =
        |dir: &Path| dir.join("keys.txt").is_file() || dir.join("settings.xml").is_file() || dir.join("mlc01").is_dir();
    let root = if picked.join("portable").is_dir() {
        picked.join("portable")
    } else if holds_data(picked) {
        picked.to_path_buf()
    } else if picked.join("Cemu.exe").is_file() {
        appdata_cemu
            .filter(|dir| holds_data(dir))
            .map(Path::to_path_buf)
            .ok_or("This Cemu hasn't saved anything yet, so there is nothing to bring over.")?
    } else {
        return Err("This doesn't look like a Cemu folder. Choose the folder Cemu.exe is in.".to_string());
    };
    let mut mlc = super::mlc_folder(&root);
    if mlc.is_relative() {
        mlc = root.join(mlc);
    }
    Ok(Places {
        keys: root.join("keys.txt"),
        saves: mlc.join("usr").join("save"),
        root,
    })
}

/// Where Omoio's own Cemu keeps them.
fn omoio_places(app: &AppHandle) -> Result<Places, String> {
    let dir = super::install_dir(app)?;
    if !dir.join("Cemu.exe").is_file() {
        return Err("Install Cemu first, then bring yours over.".to_string());
    }
    let root = dir.join("portable");
    let mut mlc = super::mlc_folder(&root);
    if mlc.is_relative() {
        mlc = root.join(mlc);
    }
    Ok(Places {
        keys: root.join("keys.txt"),
        saves: mlc.join("usr").join("save"),
        root,
    })
}

/// Where a save Omoio's Cemu had is moved before the user's replaces it.
fn set_aside_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(super::install_dir(app)?.join("replaced-saves"))
}

fn user_places(app: &AppHandle, picked: &str) -> Result<Places, String> {
    let appdata = app.path().config_dir().ok().map(|dir| dir.join("Cemu"));
    let from = places(Path::new(picked), appdata.as_deref())?;
    let to = omoio_places(app)?;
    if same_folder(&from.root, &to.root) || same_folder(&from.saves, &to.saves) {
        return Err("That's Omoio's own Cemu. Choose the one you had before.".to_string());
    }
    Ok(from)
}

fn same_folder(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[derive(Serialize, Debug, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum SaveState {
    /// Omoio's Cemu has nothing for this game.
    New,
    /// Omoio's Cemu has the same files already.
    Same,
    /// Omoio's Cemu has a save of its own for this game.
    Differs,
}

#[derive(Serialize, Debug)]
pub struct Save {
    /// The game's title id, 16 hex characters, as Cemu files the save.
    pub title_id: String,
    /// The game's name, when the save says it.
    pub name: Option<String>,
    pub state: SaveState,
}

/// What the user's Cemu has that Omoio's doesn't, before anything is copied.
#[derive(Serialize, Debug)]
pub struct Found {
    pub folder: String,
    /// Keys Omoio's Cemu doesn't have yet.
    pub keys: usize,
    /// Why the keys can't be brought, when they can't.
    pub keys_problem: Option<String>,
    pub saves: Vec<Save>,
}

/// What was copied.
#[derive(Serialize, Debug, Default, PartialEq, Eq)]
pub struct Brought {
    pub keys: usize,
    pub saves: usize,
}

/// Each game's save folder: usr/save, then the title id's two halves. Other
/// folders there, such as Cemu's `system`, are not a game's and are left.
fn game_saves(saves: &Path) -> Vec<(String, PathBuf)> {
    let is_half = |name: &str| name.len() == 8 && name.chars().all(|c| c.is_ascii_hexdigit());
    let mut found = Vec::new();
    for high in std::fs::read_dir(saves).into_iter().flatten().flatten() {
        let high_name = high.file_name().to_string_lossy().to_ascii_lowercase();
        if !is_half(&high_name) || !high.path().is_dir() {
            continue;
        }
        for low in std::fs::read_dir(high.path()).into_iter().flatten().flatten() {
            let low_name = low.file_name().to_string_lossy().to_ascii_lowercase();
            if is_half(&low_name) && low.path().is_dir() && has_files(&low.path()) {
                found.push((format!("{high_name}{low_name}"), low.path()));
            }
        }
    }
    found.sort();
    found
}

fn save_folder(saves: &Path, title_id: &str) -> PathBuf {
    saves.join(&title_id[..8]).join(&title_id[8..])
}

/// Every file under a folder, by its path inside it.
fn files(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, inside: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = inside.join(entry.file_name());
            if path.is_dir() {
                walk(&path, &name, out);
            } else if path.is_file() {
                out.push(name);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, Path::new(""), &mut out);
    out.sort();
    out
}

pub(super) fn has_files(dir: &Path) -> bool {
    !files(dir).is_empty()
}

/// Whether two save folders hold the same files with the same contents.
fn same_files(a: &Path, b: &Path) -> bool {
    let names = files(a);
    if names != files(b) {
        return false;
    }
    names.iter().all(|name| {
        let (x, y) = (a.join(name), b.join(name));
        let sizes = |p: &Path| std::fs::metadata(p).map(|m| m.len()).ok();
        sizes(&x).is_some() && sizes(&x) == sizes(&y) && std::fs::read(&x).ok() == std::fs::read(&y).ok()
    })
}

/// The game's name from the copy of its meta.xml kept beside the save.
fn save_name(folder: &Path) -> Option<String> {
    let xml = std::fs::read_to_string(folder.join("meta").join("meta.xml")).ok()?;
    let name = super::element(&xml, "longname_en")?;
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    (!name.is_empty()).then_some(name)
}

fn state(from: &Path, to_saves: &Path, title_id: &str) -> SaveState {
    let there = save_folder(to_saves, title_id);
    if !has_files(&there) {
        SaveState::New
    } else if same_files(from, &there) {
        SaveState::Same
    } else {
        SaveState::Differs
    }
}

pub(super) fn survey(from: &Places, to: &Places) -> Found {
    let (keys, keys_problem) = match std::fs::read_to_string(&from.keys) {
        Err(_) => (0, None),
        Ok(text) => match super::keys::read(&text) {
            (_, Some(line)) => (
                0,
                Some(format!(
                    "Line {line} of your keys.txt isn't a key, so your keys were left out."
                )),
            ),
            (new, None) => {
                let have = std::fs::read_to_string(&to.keys)
                    .map(|t| super::keys::read(&t).0)
                    .unwrap_or_default();
                (new.iter().filter(|key| !have.contains(key)).count(), None)
            }
        },
    };
    let saves = game_saves(&from.saves)
        .into_iter()
        .map(|(title_id, folder)| Save {
            state: state(&folder, &to.saves, &title_id),
            name: save_name(&folder),
            title_id,
        })
        .collect();
    Found {
        folder: from.root.to_string_lossy().into_owned(),
        keys,
        keys_problem,
        saves,
    }
}

/// Copies a folder's files, returning how many. Only reads `from`.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<usize> {
    let mut copied = 0;
    for name in files(from) {
        let target = to.join(&name);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(from.join(&name), &target)?;
        copied += 1;
    }
    Ok(copied)
}

/// Copies a save in beside where it goes and then moves it into place, so a
/// copy that stops part way never leaves half a save for Cemu to load.
fn put_in_place(from: &Path, there: &Path) -> Result<(), String> {
    let couldnt = || "Couldn't copy the saves. Nothing of yours was changed.".to_string();
    let part = there.with_extension("omoio-part");
    let _ = std::fs::remove_dir_all(&part);
    if copy_tree(from, &part).is_err() {
        let _ = std::fs::remove_dir_all(&part);
        return Err(couldnt());
    }
    if there.exists() {
        // An empty folder Cemu left. A save with files in it never gets here.
        let _ = std::fs::remove_dir_all(there);
    }
    std::fs::rename(&part, there).map_err(|_| {
        let _ = std::fs::remove_dir_all(&part);
        couldnt()
    })
}

/// Adds the keys Omoio's Cemu lacks and copies the saves of games it has no
/// save for. A game Omoio's Cemu has a save for is left alone, whatever it
/// holds: replacing one is asked for one game at a time.
pub(super) fn bring(from: &Places, to: &Places) -> Result<Brought, String> {
    let mut brought = Brought::default();
    if let Ok(text) = std::fs::read_to_string(&from.keys) {
        let (keys, bad) = super::keys::read(&text);
        if bad.is_none() && !keys.is_empty() {
            brought.keys = super::keys::merged(&to.keys, &text)?;
        }
    }
    for (title_id, folder) in game_saves(&from.saves) {
        if state(&folder, &to.saves, &title_id) != SaveState::New {
            continue;
        }
        let there = save_folder(&to.saves, &title_id);
        if let Some(parent) = there.parent() {
            std::fs::create_dir_all(parent).map_err(|_| "Couldn't copy the saves.".to_string())?;
        }
        put_in_place(&folder, &there)?;
        brought.saves += 1;
    }
    Ok(brought)
}

/// Puts the user's save for one game in place of the one Omoio's Cemu has,
/// which is moved into `aside` first, under the time and the title id.
pub(super) fn replace(from: &Places, to: &Places, title_id: &str, aside: &Path, now: u64) -> Result<(), String> {
    let title_id = title_id.to_ascii_lowercase();
    let ours = game_saves(&from.saves)
        .into_iter()
        .find(|(id, _)| *id == title_id)
        .map(|(_, folder)| folder)
        .ok_or("Your Cemu has no save for that game any more.")?;
    let there = save_folder(&to.saves, &title_id);
    if has_files(&there) {
        let couldnt = || "Couldn't move Omoio's save aside, so it was left as it was.".to_string();
        let held = aside.join(now.to_string());
        let kept = held.join(&title_id);
        std::fs::create_dir_all(&held).map_err(|_| couldnt())?;
        // Renaming keeps it whole. Should the two be on different drives, it
        // is copied and only then removed.
        if std::fs::rename(&there, &kept).is_err() {
            copy_tree(&there, &kept).map_err(|_| couldnt())?;
            std::fs::remove_dir_all(&there).map_err(|_| couldnt())?;
        }
    }
    put_in_place(&ours, &there)
}

/// What the Cemu in the folder the user picked has to bring over.
pub fn look(app: &AppHandle, picked: &str) -> Result<Found, String> {
    let from = user_places(app, picked)?;
    Ok(survey(&from, &omoio_places(app)?))
}

pub fn bring_over(app: &AppHandle, picked: &str) -> Result<Brought, String> {
    let from = user_places(app, picked)?;
    bring(&from, &omoio_places(app)?)
}

pub fn replace_save(app: &AppHandle, picked: &str, title_id: &str) -> Result<(), String> {
    let from = user_places(app, picked)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    replace(&from, &omoio_places(app)?, title_id, &set_aside_dir(app)?, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "00112233445566778899aabbccddeeff";
    const OTHER_KEY: &str = "ffeeddccbbaa99887766554433221100";
    const GAME: &str = "0005000010172700";
    const OTHER_GAME: &str = "00050000101c4d00";

    /// A folder of its own for each test, gone when it ends.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("omoio-own-cemu-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn save(mlc: &Path, title_id: &str, text: &str) {
        let folder = save_folder(&mlc.join("usr").join("save"), title_id);
        write(&folder.join("user").join("80000001").join("game.dat"), text);
        write(
            &folder.join("meta").join("meta.xml"),
            "<?xml version=\"1.0\"?><menu><longname_en type=\"string\">Skylanders\nTrap Team</longname_en></menu>",
        );
    }

    fn read_save(mlc: &Path, title_id: &str) -> String {
        let folder = save_folder(&mlc.join("usr").join("save"), title_id);
        std::fs::read_to_string(folder.join("user").join("80000001").join("game.dat")).unwrap()
    }

    /// Every file under a folder with its contents, to show nothing changed.
    fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        files(dir)
            .into_iter()
            .map(|name| (name.clone(), std::fs::read(dir.join(&name)).unwrap()))
            .collect()
    }

    /// A portable Cemu 2 for the user, with keys and two saves, and Omoio's
    /// Cemu with nothing yet.
    fn two_cemus(scratch: &Scratch) -> (PathBuf, Places, Places) {
        let theirs = scratch.0.join("Cemu");
        write(&theirs.join("Cemu.exe"), "");
        let data = theirs.join("portable");
        write(&data.join("keys.txt"), &format!("# my keys\r\n{KEY} # a disc\r\n"));
        save(&data.join("mlc01"), GAME, "theirs");
        save(&data.join("mlc01"), OTHER_GAME, "theirs too");
        write(
            &data.join("mlc01").join("usr").join("save").join("system").join("x.dat"),
            "not a game",
        );
        let from = places(&theirs, None).unwrap();
        let omoio = scratch.0.join("Omoio").join("portable");
        std::fs::create_dir_all(&omoio).unwrap();
        let to = Places {
            keys: omoio.join("keys.txt"),
            saves: omoio.join("mlc01").join("usr").join("save"),
            root: omoio,
        };
        (theirs, from, to)
    }

    #[test]
    fn each_kind_of_cemu_folder_is_understood() {
        let scratch = Scratch::new("places");

        let portable = scratch.0.join("portable-cemu");
        write(&portable.join("Cemu.exe"), "");
        std::fs::create_dir_all(portable.join("portable")).unwrap();
        assert_eq!(places(&portable, None).unwrap().root, portable.join("portable"));

        let appdata = scratch.0.join("AppData").join("Cemu");
        write(&appdata.join("settings.xml"), "<content></content>");
        let installed = scratch.0.join("installed-cemu");
        write(&installed.join("Cemu.exe"), "");
        let found = places(&installed, Some(&appdata)).unwrap();
        assert_eq!(
            found.root, appdata,
            "Cemu 2 without portable keeps its data in %APPDATA%"
        );
        assert_eq!(found.saves, appdata.join("mlc01").join("usr").join("save"));
        assert!(places(&installed, None).is_err(), "nothing saved yet");
        assert_eq!(
            places(&appdata, None).unwrap().root,
            appdata,
            "the data folder picked itself"
        );

        let old = scratch.0.join("cemu_1.26");
        write(&old.join("Cemu.exe"), "");
        write(&old.join("keys.txt"), KEY);
        assert_eq!(
            places(&old, None).unwrap().root,
            old,
            "Cemu 1 keeps everything beside Cemu.exe"
        );

        let elsewhere = scratch.0.join("D").join("WiiU").join("mlc01");
        write(
            &appdata.join("settings.xml"),
            &format!("<content><mlc_path>{}</mlc_path></content>", elsewhere.display()),
        );
        assert_eq!(
            places(&appdata, None).unwrap().saves,
            elsewhere.join("usr").join("save"),
            "the mlc folder set in Cemu"
        );

        let empty = scratch.0.join("Documents");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(places(&empty, None)
            .unwrap_err()
            .contains("doesn't look like a Cemu folder"));
    }

    #[test]
    fn keys_and_saves_are_copied_and_the_users_cemu_is_left_as_it_was() {
        let scratch = Scratch::new("copy");
        let (theirs, from, to) = two_cemus(&scratch);
        let before = snapshot(&theirs);

        let found = survey(&from, &to);
        assert_eq!(found.keys, 1);
        assert_eq!(found.keys_problem, None);
        let states: Vec<_> = found.saves.iter().map(|s| (s.title_id.as_str(), s.state)).collect();
        assert_eq!(
            states,
            [(GAME, SaveState::New), (OTHER_GAME, SaveState::New)],
            "Cemu's system folder is no game's"
        );
        assert_eq!(found.saves[0].name.as_deref(), Some("Skylanders Trap Team"));

        assert_eq!(bring(&from, &to).unwrap(), Brought { keys: 1, saves: 2 });
        let omoio_mlc = to.saves.parent().unwrap().parent().unwrap();
        assert_eq!(read_save(omoio_mlc, GAME), "theirs");
        assert_eq!(read_save(omoio_mlc, OTHER_GAME), "theirs too");
        assert!(std::fs::read_to_string(&to.keys).unwrap().contains(KEY));
        assert!(!to.saves.join("system").exists(), "only games' saves are copied");
        assert!(!files(&to.saves)
            .iter()
            .any(|f| f.to_string_lossy().contains("omoio-part")));

        assert_eq!(snapshot(&theirs), before, "the user's Cemu is only read");

        let again = survey(&from, &to);
        assert_eq!(again.keys, 0);
        assert!(again.saves.iter().all(|s| s.state == SaveState::Same));
        assert_eq!(
            bring(&from, &to).unwrap(),
            Brought::default(),
            "a second time brings nothing"
        );
    }

    #[test]
    fn a_save_omoio_already_has_is_kept_unless_the_user_replaces_it() {
        let scratch = Scratch::new("conflict");
        let (theirs, from, to) = two_cemus(&scratch);
        let omoio_mlc = to.saves.parent().unwrap().parent().unwrap().to_path_buf();
        save(&omoio_mlc, GAME, "omoio's");
        write(&to.keys, &format!("{OTHER_KEY}\r\n"));
        let before = snapshot(&theirs);

        let found = survey(&from, &to);
        let state_of = |id: &str| found.saves.iter().find(|s| s.title_id == id).unwrap().state;
        assert_eq!(state_of(GAME), SaveState::Differs);
        assert_eq!(state_of(OTHER_GAME), SaveState::New);

        assert_eq!(bring(&from, &to).unwrap(), Brought { keys: 1, saves: 1 });
        assert_eq!(
            read_save(&omoio_mlc, GAME),
            "omoio's",
            "never overwritten without asking"
        );
        let keys = std::fs::read_to_string(&to.keys).unwrap();
        assert!(
            keys.contains(OTHER_KEY) && keys.contains(KEY),
            "keys are added, none taken away"
        );

        let aside = scratch.0.join("Omoio").join("replaced-saves");
        replace(&from, &to, GAME, &aside, 1_700_000_000).unwrap();
        assert_eq!(read_save(&omoio_mlc, GAME), "theirs");
        let kept = aside.join("1700000000").join(GAME);
        assert_eq!(
            std::fs::read_to_string(kept.join("user").join("80000001").join("game.dat")).unwrap(),
            "omoio's",
            "Omoio's save is moved aside, not lost"
        );
        assert_eq!(snapshot(&theirs), before, "the user's Cemu is only read");
        assert!(
            replace(&from, &to, "0005000010000000", &aside, 1).is_err(),
            "no such save of theirs"
        );
    }

    #[test]
    fn a_keys_file_cemu_would_refuse_is_left_out() {
        let scratch = Scratch::new("bad-keys");
        let (_, from, to) = two_cemus(&scratch);
        write(&from.keys, &format!("{KEY}\r\nnot a key\r\n"));
        let found = survey(&from, &to);
        assert_eq!(found.keys, 0);
        assert!(found.keys_problem.unwrap().contains("Line 2"));
        assert_eq!(bring(&from, &to).unwrap().keys, 0);
        assert!(!to.keys.exists());
        assert_eq!(bring(&from, &to).unwrap().saves, 0, "saves came across the first time");
    }

    #[test]
    fn an_empty_save_folder_omoio_has_counts_as_none() {
        let scratch = Scratch::new("empty");
        let (_, from, to) = two_cemus(&scratch);
        std::fs::create_dir_all(save_folder(&to.saves, GAME).join("user")).unwrap();
        assert_eq!(survey(&from, &to).saves[0].state, SaveState::New);
        assert_eq!(bring(&from, &to).unwrap().saves, 2);
        assert_eq!(read_save(to.saves.parent().unwrap().parent().unwrap(), GAME), "theirs");
    }
}
