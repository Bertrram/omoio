//! Copies of a game's saved games, kept outside the emulator.
//!
//! Each emulator says where a game's saves sit in its storage
//! (`EmulatorBackend::save_folders`); taking, listing and putting back copies
//! is the same for all of them and lives here.
//!
//! Backups live in Omoio's own folder, not inside the emulator, so
//! reinstalling the emulator or clearing its storage does not take someone's
//! saves with it. One backup is a folder named for when it was taken,
//! holding a folder for each place the saves came from, by the name the
//! emulator keeps it under, and the saves in that as they were:
//! `saves/<title id>/<taken>/<kept as>/<save>`. RPCS3's backups have been
//! laid out that way from the start, with the RPCS3 user as the name.

use crate::backends::{EmulatorBackend, SaveFolder};
use crate::core::library::Game;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Backup {
    /// Seconds since the epoch, as the session log names use.
    pub made: u64,
    pub bytes: u64,
    /// How many saves it holds, which is not always one: a game can keep
    /// several folders, or several files on a GameCube memory card.
    pub saves: usize,
}

fn backups_dir(app: &AppHandle, title_id: &str) -> Result<PathBuf, String> {
    let data = app.path().data_dir().map_err(|e| e.to_string())?;
    Ok(data.join("Omoio").join("saves").join(title_id))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn has_saves(app: &AppHandle, backend: &dyn EmulatorBackend, game: &Game) -> bool {
    backend.save_folders(app, game).iter().any(|folder| !folder.saves.is_empty())
}

/// Takes a copy of this game's saves. Returns `None` when there is nothing to
/// copy, which is not a failure: a game that has never been played has no
/// saves, and saying so is better than writing an empty backup.
pub fn back_up(app: &AppHandle, backend: &dyn EmulatorBackend, game: &Game) -> Result<Option<Backup>, String> {
    take(&backend.save_folders(app, game), &backups_dir(app, &game.title_id)?)
}

/// What we have kept for this game, newest first.
pub fn list(app: &AppHandle, title_id: &str) -> Vec<Backup> {
    backups_dir(app, title_id).map(|dir| list_in(&dir)).unwrap_or_default()
}

/// Puts a backup back, replacing what the game has saved under the same
/// names, and what the emulator says is the same save under another name.
/// Saves the game made since that the backup doesn't hold are left.
///
/// The current saves are copied aside first. Restoring is the one action here
/// that destroys something, and doing it without a way back would make a
/// mis-click unrecoverable.
pub fn restore(app: &AppHandle, backend: &dyn EmulatorBackend, game: &Game, made: u64) -> Result<(), String> {
    restore_in(
        &backups_dir(app, &game.title_id)?,
        made,
        &backend.save_folders(app, game),
        &|kept_as| backend.save_folder(app, game, kept_as),
        &|kept, saved| backend.is_same_save(kept, saved),
    )
}

pub fn forget(app: &AppHandle, title_id: &str, made: u64) -> Result<(), String> {
    let dir = backups_dir(app, title_id)?.join(made.to_string());
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Copies the saves in `folders` into a new backup among `backups`.
fn take(folders: &[SaveFolder], backups: &Path) -> Result<Option<Backup>, String> {
    let saves: usize = folders.iter().map(|folder| folder.saves.len()).sum();
    if saves == 0 {
        return Ok(None);
    }

    // Two copies in the same second, as putting a backup back straight after
    // taking one makes, would share a folder and the second would write
    // into the first.
    let mut made = now();
    while backups.join(made.to_string()).exists() {
        made += 1;
    }
    let into = backups.join(made.to_string());
    std::fs::create_dir_all(&into).map_err(|e| e.to_string())?;

    let mut bytes = 0;
    for folder in folders {
        for save in &folder.saves {
            let name = save.file_name().unwrap_or_default();
            bytes += copy(save, &into.join(&folder.kept_as).join(name))?;
        }
    }
    Ok(Some(Backup { made, bytes, saves }))
}

fn list_in(backups: &Path) -> Vec<Backup> {
    let Ok(entries) = std::fs::read_dir(backups) else {
        return Vec::new();
    };

    let mut found: Vec<Backup> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let made = e.file_name().to_string_lossy().parse().ok()?;
            let (bytes, saves) = measure(&e.path());
            Some(Backup { made, bytes, saves })
        })
        .collect();
    found.sort_by(|a, b| b.made.cmp(&a.made));
    found
}

/// `now` is what the game has saved at the moment, `folder_for` where a
/// backup's part goes back, and `same` the emulator's `is_same_save`.
fn restore_in(
    backups: &Path,
    made: u64,
    now: &[SaveFolder],
    folder_for: &dyn Fn(&str) -> Option<PathBuf>,
    same: &dyn Fn(&Path, &Path) -> bool,
) -> Result<(), String> {
    let from = backups.join(made.to_string());
    if !from.is_dir() {
        return Err("That backup isn't there any more.".into());
    }

    // Every part has to have somewhere to go before anything is touched, so
    // a backup is never put back only in part.
    let mut parts = Vec::new();
    for part in std::fs::read_dir(&from).map_err(|e| e.to_string())?.flatten() {
        if !part.path().is_dir() {
            continue;
        }
        let kept_as = part.file_name().to_string_lossy().into_owned();
        let folder = folder_for(&kept_as).ok_or("Omoio can't tell where this backup's saves go back.")?;
        parts.push((part.path(), kept_as, folder));
    }

    take(now, backups)?;

    for (part, kept_as, folder) in parts {
        let saved: Vec<&PathBuf> = now
            .iter()
            .filter(|place| place.kept_as == kept_as)
            .flat_map(|place| &place.saves)
            .collect();
        for save in std::fs::read_dir(&part).map_err(|e| e.to_string())?.flatten() {
            let kept = save.path();
            // The same save under another name would leave the emulator two
            // of it, and it would go on loading the one it has now.
            for old in saved.iter().filter(|old| old.exists() && same(&kept, old)) {
                remove(old)?;
            }
            let target = folder.join(save.file_name());
            // Replaced whole, so a save that shrank does not keep stale files.
            if target.exists() {
                remove(&target)?;
            }
            copy(&kept, &target)?;
        }
    }
    Ok(())
}

/// Copies a save, a file or a folder with everything under it, returning
/// the bytes copied.
fn copy(from: &Path, to: &Path) -> Result<u64, String> {
    if from.is_dir() {
        return copy_tree(from, to);
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(from, to).map_err(|e| e.to_string())
}

fn copy_tree(from: &Path, to: &Path) -> Result<u64, String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let mut bytes = 0;

    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let target = to.join(entry.file_name());
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => bytes += copy_tree(&entry.path(), &target)?,
            Ok(kind) if kind.is_file() => {
                bytes += std::fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
            }
            _ => {}
        }
    }
    Ok(bytes)
}

fn remove(path: &Path) -> Result<(), String> {
    let removed = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
    removed.map_err(|e| e.to_string())
}

/// Size on disk and how many saves a backup holds.
fn measure(dir: &Path) -> (u64, usize) {
    let mut bytes = 0;
    let mut saves = 0;

    let Ok(parts) = std::fs::read_dir(dir) else {
        return (0, 0);
    };
    for part in parts.flatten() {
        let Ok(inside) = std::fs::read_dir(part.path()) else {
            continue;
        };
        for save in inside.flatten() {
            saves += 1;
            bytes += size(&save.path());
        }
    }
    (bytes, saves)
}

fn size(path: &Path) -> u64 {
    if path.is_file() {
        return std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    }
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(entry.path()),
                Ok(kind) if kind.is_file() => {
                    total += entry.metadata().map(|m| m.len()).unwrap_or(0);
                }
                _ => {}
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tree(PathBuf);

    impl Tree {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("omoio-saves-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Tree(root)
        }

        fn file(&self, at: &str, bytes: &[u8]) {
            let path = self.0.join(at);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }

        fn read(&self, at: &str) -> Vec<u8> {
            std::fs::read(self.0.join(at)).unwrap()
        }

        fn has(&self, at: &str) -> bool {
            self.0.join(at).exists()
        }

        fn folder(&self, kept_as: &str, at: &str, saves: &[&str]) -> SaveFolder {
            SaveFolder {
                kept_as: kept_as.to_string(),
                path: self.0.join(at),
                saves: saves.iter().map(|save| self.0.join(at).join(save)).collect(),
            }
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn by_name(_: &Path, _: &Path) -> bool {
        false
    }

    #[test]
    fn copies_a_whole_save_folder() {
        let tree = Tree::new("copy");
        tree.file("from/PARAM.SFO", &[0; 100]);
        tree.file("from/deep/inner/DATA.BIN", &[0; 250]);

        let bytes = copy(&tree.0.join("from"), &tree.0.join("to")).unwrap();

        assert_eq!(bytes, 350);
        assert!(tree.has("to/PARAM.SFO"));
        assert!(tree.has("to/deep/inner/DATA.BIN"));
    }

    #[test]
    fn measures_a_backup_by_its_saves() {
        // The shape a backup is written in: <kept as>/<save>, a save being a
        // folder (RPCS3) or a file (a GameCube memory card's).
        let tree = Tree::new("measure");
        tree.file("00000001/BCES00141/DATA.BIN", &[0; 400]);
        tree.file("00000001/BCES00141-AUTO/DATA.BIN", &[0; 600]);
        tree.file("EUR/01-GALP-SuperSmashBros.gci", &[0; 64]);

        let (bytes, saves) = measure(&tree.0);
        assert_eq!(bytes, 1064);
        assert_eq!(saves, 3, "each save counts, not each place");
    }

    #[test]
    fn an_empty_backup_measures_as_nothing() {
        let tree = Tree::new("empty");
        assert_eq!(measure(&tree.0), (0, 0));
        assert_eq!(size(&tree.0.join("no-such-folder")), 0);
    }

    #[test]
    fn a_backup_keeps_each_save_under_the_name_of_its_place() {
        let tree = Tree::new("take");
        tree.file("hdd/home/00000001/savedata/BLES01234/DATA.BIN", &[1; 300]);
        tree.file("hdd/home/00000001/savedata/BLES01234-AUTO/DATA.BIN", &[2; 200]);
        tree.file("card/EUR/Card A/01-GALP-x.gci", &[3; 64]);
        let folders = [
            tree.folder("00000001", "hdd/home/00000001/savedata", &["BLES01234", "BLES01234-AUTO"]),
            tree.folder("EUR", "card/EUR/Card A", &["01-GALP-x.gci"]),
        ];

        let backups = tree.0.join("backups");
        let made = take(&folders, &backups).unwrap().unwrap();

        assert_eq!((made.bytes, made.saves), (564, 3));
        let kept = backups.join(made.made.to_string());
        assert!(kept.join("00000001/BLES01234/DATA.BIN").is_file());
        assert!(kept.join("00000001/BLES01234-AUTO/DATA.BIN").is_file());
        assert!(kept.join("EUR/01-GALP-x.gci").is_file());
        assert_eq!(list_in(&backups), [made]);
    }

    #[test]
    fn nothing_to_copy_takes_no_backup() {
        let tree = Tree::new("nothing");
        let backups = tree.0.join("backups");
        assert_eq!(take(&[], &backups).unwrap(), None);
        assert_eq!(take(&[tree.folder("EUR", "card", &[])], &backups).unwrap(), None);
        assert!(!backups.exists());
    }

    #[test]
    fn two_backups_in_one_second_are_kept_apart() {
        let tree = Tree::new("same-second");
        tree.file("saves/data/one", &[1; 10]);
        let folders = [tree.folder("00010000", "saves", &["data"])];
        let backups = tree.0.join("backups");

        let first = take(&folders, &backups).unwrap().unwrap();
        let second = take(&folders, &backups).unwrap().unwrap();

        assert_ne!(first.made, second.made);
        assert_eq!(list_in(&backups).len(), 2);
    }

    #[test]
    fn putting_back_replaces_saves_of_the_same_name_and_leaves_the_rest() {
        let tree = Tree::new("restore");
        let savedata = "home/00000001/savedata";
        tree.file("backups/100/00000001/BLES01234/DATA.BIN", b"old");
        tree.file(&format!("{savedata}/BLES01234/DATA.BIN"), b"new and longer");
        tree.file(&format!("{savedata}/BLES01234/EXTRA.BIN"), b"stale");
        tree.file(&format!("{savedata}/BLES01234-AUTO/DATA.BIN"), b"made since");
        let now = [tree.folder("00000001", savedata, &["BLES01234", "BLES01234-AUTO"])];
        let home = tree.0.join(savedata);

        restore_in(&tree.0.join("backups"), 100, &now, &|_| Some(home.clone()), &by_name).unwrap();

        assert_eq!(tree.read(&format!("{savedata}/BLES01234/DATA.BIN")), b"old");
        assert!(!tree.has(&format!("{savedata}/BLES01234/EXTRA.BIN")), "replaced whole");
        assert_eq!(tree.read(&format!("{savedata}/BLES01234-AUTO/DATA.BIN")), b"made since");
        // What was there went into a backup of its own first.
        let backups = list_in(&tree.0.join("backups"));
        assert_eq!(backups.len(), 2);
        assert_eq!(backups[0].saves, 2);
    }

    #[test]
    fn putting_back_takes_out_the_same_save_under_another_name() {
        let tree = Tree::new("restore-same");
        tree.file("backups/100/EUR/zelda.gci", b"kept");
        tree.file("card/01-GZLP-zelda.gci", b"now");
        tree.file("card/01-GALP-other.gci", b"another save");
        let now = [tree.folder("EUR", "card", &["01-GZLP-zelda.gci"])];
        let card = tree.0.join("card");
        let same = |kept: &Path, saved: &Path| {
            kept.ends_with("zelda.gci") && saved.ends_with("01-GZLP-zelda.gci")
        };

        restore_in(&tree.0.join("backups"), 100, &now, &|_| Some(card.clone()), &same).unwrap();

        assert_eq!(tree.read("card/zelda.gci"), b"kept");
        assert!(!tree.has("card/01-GZLP-zelda.gci"));
        assert_eq!(tree.read("card/01-GALP-other.gci"), b"another save");
    }

    #[test]
    fn a_backup_goes_back_where_nothing_is_saved_any_more() {
        let tree = Tree::new("restore-empty");
        tree.file("backups/100/00010000/data/banner.bin", b"banner");
        let title = tree.0.join("nand/title/00010000/53535050");

        restore_in(&tree.0.join("backups"), 100, &[], &|_| Some(title.clone()), &by_name).unwrap();

        assert_eq!(tree.read("nand/title/00010000/53535050/data/banner.bin"), b"banner");
        assert_eq!(list_in(&tree.0.join("backups")).len(), 1, "nothing to copy aside");
    }

    #[test]
    fn a_backup_with_a_part_that_has_nowhere_to_go_changes_nothing() {
        let tree = Tree::new("restore-nowhere");
        tree.file("backups/100/EUR/a.gci", b"a");
        tree.file("backups/100/XYZ/b.gci", b"b");
        tree.file("card/a.gci", b"now");
        let now = [tree.folder("EUR", "card", &["a.gci"])];
        let card = tree.0.join("card");
        let folder_for = |kept_as: &str| (kept_as == "EUR").then(|| card.clone());

        assert!(restore_in(&tree.0.join("backups"), 100, &now, &folder_for, &by_name).is_err());
        assert_eq!(tree.read("card/a.gci"), b"now");
    }

    #[test]
    fn a_backup_that_is_gone_is_said_so() {
        let tree = Tree::new("restore-gone");
        let err = restore_in(&tree.0.join("backups"), 100, &[], &|_| None, &by_name).unwrap_err();
        assert_eq!(err, "That backup isn't there any more.");
    }
}
