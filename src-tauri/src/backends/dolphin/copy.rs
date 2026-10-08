//! A copy of a Wii game's files that Omoio can read, made by Dolphin's own
//! DolphinTool, for the figures' pictures.
//!
//! A Wii disc's files are encrypted, and Omoio never decrypts anything. The
//! user allowed DolphinTool to make a readable copy for the pictures (8
//! October 2026), as Cemu makes a .wua of a Wii U disc: DolphinTool does the
//! decrypting, Omoio reads the pictures and deletes the copy.
//!
//! `DolphinTool extract` (Source/Core/DolphinTool/ExtractCommand.cpp, Dolphin
//! 2609a): `-i` the disc image, `-o` the folder, `-g` the game's own
//! partition only, and `-s` one file or folder of it. Given `/`, the whole
//! of the game's files go into `<folder>/DATA/files/` and nothing else:
//! without `-s` it would write the partition's system files too, its ticket
//! among them, which Omoio has no use for. It prints `Extracting: <file> |
//! <percent>%` for each file of a folder on its error output.
//!
//! For a game whose pictures the reader finds in a few files, only those are
//! copied, one run for each, since `-s` takes one: a file goes to
//! `<folder>/DATA/files/<file>` too, and DolphinTool prints nothing for it
//! (`ExtractFile`), so progress then goes by the files done.

use crate::core::figures;
use std::cell::Cell;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

/// The files of a game the picture reader reads, where those are all it
/// needs.
struct ReaderFiles {
    game: figures::Game,
    files: &'static [&'static str],
    /// The room they take, with some to spare.
    room: u64,
}

/// Spyro's Adventure keeps every figure's picture in its versus screen, the
/// element symbols in the screen kept for every level, and each magic item's
/// and adventure pack's sprite in that toy's own archive (omoio-portraits,
/// "Read Spyro's Adventure's pictures from the Wii game" and "Read Spyro's
/// Adventure's magic items and adventure packs", 8 October 2026). Together
/// they are 30,969,088 bytes on the PAL disc, against 4.4 GB for the game;
/// `room` allows another region's to be a little larger.
const READER_FILES: [ReaderFiles; 1] = [ReaderFiles {
    game: figures::Game::Spyro,
    files: &[
        "/misc/PvP_MainControl.arc",
        "/permanent/global.bld",
        "/item/Item_Anvil.bld",
        "/item/Item_SecretStash.bld",
        "/item/Item_Regeneration.bld",
        "/item/Item_Pirates.bld",
        "/item/Item_Hourglass.bld",
        "/item/Item_Shield.bld",
        "/item/Item_Potion.bld",
        "/item/Item_Zapper.bld",
        "/item/Item_Location_Dragon.bld",
        "/item/Item_Location_Ice.bld",
        "/item/Item_Location_Pirate.bld",
        "/item/Item_Location_Undead.bld",
    ],
    room: 40 << 20,
}];

/// Every one of a game's files, for a game not in `READER_FILES`.
const WHOLE: &[&str] = &["/"];

fn reader_files(game: Option<figures::Game>) -> Option<&'static ReaderFiles> {
    READER_FILES.iter().find(|known| Some(known.game) == game)
}

/// What to copy of a game, by which Skylanders game it is: the files the
/// picture reader needs, or all of them.
pub fn files_for(game: Option<figures::Game>) -> &'static [&'static str] {
    reader_files(game).map_or(WHOLE, |known| known.files)
}

/// The room a copy of only the files the reader needs takes, for a game
/// that has such a list.
pub fn room_for(game: Option<figures::Game>) -> Option<u64> {
    reader_files(game).map(|known| known.room)
}

/// The percent in one of DolphinTool's progress lines.
fn percent(line: &str) -> Option<u32> {
    let (_, done) = line.strip_prefix("Extracting: ")?.rsplit_once(" | ")?;
    done.trim().strip_suffix('%')?.parse().ok()
}

/// How far the whole copy is, `runs` runs of DolphinTool in all, `done` of
/// them finished and the current one at `percent`.
fn overall(done: usize, runs: usize, percent: u32) -> u32 {
    let runs = runs.max(1) as u32;
    (done as u32 * 100 + percent.min(100)) / runs
}

/// Whether a run left what it was asked for in the copy: the folder of the
/// game's files, or the one file. DolphinTool says it finished even when a
/// file couldn't be written (`ExportFile`'s answer is not looked at).
fn copied(copy: &Path, file: &str) -> bool {
    let inside = file.trim_start_matches('/');
    if inside.is_empty() {
        copy.join("files").is_dir()
    } else {
        copy.join("files").join(inside).is_file()
    }
}

const STOPPED: &str = "Stopped. The pictures from before are kept.";

/// One run of DolphinTool, copying `file` out of `disc` into `into`, and
/// whether it finished.
fn run(
    tool: &Path,
    disc: &Path,
    into: &Path,
    file: &str,
    progress: &dyn Fn(u32),
    cancel: &AtomicBool,
) -> Result<bool, String> {
    let mut child = super::install::command(tool)
        .arg("extract")
        .arg("-i")
        .arg(disc)
        .arg("-o")
        .arg(into)
        .args(["-g", "-s", file])
        // DolphinTool looks for a keys file in the folder it runs in before
        // using the keys built into it (`IOSC::LoadDefaultEntries`), so it
        // runs in the folder made for the copy, empty but for the copy.
        .current_dir(into)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "Couldn't start Dolphin's own tool to copy the game.".to_string())?;
    // Read on a thread of its own, so a stop is heard while one large file
    // is being copied and nothing is printed.
    let (tell, heard) = std::sync::mpsc::channel();
    if let Some(said) = child.stderr.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(said).lines().map_while(Result::ok) {
                if let Some(done) = percent(&line) {
                    let _ = tell.send(done);
                }
            }
        });
    }
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(STOPPED.to_string());
        }
        for done in heard.try_iter() {
            progress(done);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(_) => break None,
        }
    };
    Ok(status.is_some_and(|status| status.success()))
}

/// Has DolphinTool copy `files` of the game out of `disc` into `into`, one
/// run each, and returns the folder the game's files are in.
pub fn extract(
    tool: &Path,
    disc: &Path,
    into: &Path,
    files: &[&str],
    progress: &dyn Fn(u32),
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let failed = || "Dolphin couldn't copy the game's files. Check the disc image is whole and try again.".to_string();
    if files.is_empty() {
        return Err(failed());
    }
    std::fs::create_dir_all(into).map_err(|_| "Couldn't make room for a copy of the game.".to_string())?;
    let copy = into.join("DATA");
    let last = Cell::new(0);
    let tell = |done: u32| {
        if done != last.get() {
            last.set(done);
            progress(done);
        }
    };
    for (done, file) in files.iter().enumerate() {
        let finished = run(tool, disc, into, file, &|percent| tell(overall(done, files.len(), percent)), cancel)?;
        if !finished || !copied(&copy, file) {
            return Err(failed());
        }
        tell(overall(done + 1, files.len(), 0));
    }
    Ok(copy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_is_read_from_dolphintools_lines() {
        assert_eq!(percent("Extracting: /uigameimage.str | 42%"), Some(42));
        assert_eq!(percent("Extracting: /a | b/c.bin | 100%"), Some(100), "a name with the separator in it");
        assert_eq!(percent("Error: Unable to open volume"), None);
    }

    #[test]
    fn spyros_adventure_copies_only_the_files_the_reader_reads() {
        let spyro = Some(figures::Game::Spyro);
        let files = files_for(spyro);
        assert_eq!(files[..2], ["/misc/PvP_MainControl.arc", "/permanent/global.bld"]);
        // The eight magic items and four adventure packs, one archive each.
        assert_eq!(files.iter().filter(|file| file.starts_with("/item/Item_") && file.ends_with(".bld")).count(), 12);
        let room = room_for(spyro).unwrap();
        assert!(room >= 30_969_088 && room < 100 << 20, "the files with a little to spare, not the game");
        // Every other game, and one Omoio can't tell, is copied whole.
        for other in [Some(figures::Game::Giants), Some(figures::Game::TrapTeam), None] {
            assert_eq!(files_for(other), ["/"]);
            assert_eq!(room_for(other), None);
        }
    }

    #[test]
    fn progress_runs_over_every_run_once() {
        // One run: DolphinTool's own percent.
        assert_eq!(overall(0, 1, 42), 42);
        assert_eq!(overall(1, 1, 0), 100);
        // Two runs of one file each, which print nothing: half, then all.
        assert_eq!(overall(0, 2, 0), 0);
        assert_eq!(overall(1, 2, 0), 50);
        assert_eq!(overall(1, 2, 50), 75);
        assert_eq!(overall(2, 2, 0), 100);
        assert_eq!(overall(0, 0, 0), 0, "no runs");
    }

    #[test]
    fn a_run_counts_only_once_its_file_is_there() {
        let into = std::env::temp_dir().join(format!("omoio-dolphin-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&into);
        let copy = into.join("DATA");
        assert!(!copied(&copy, "/"));
        assert!(!copied(&copy, "/misc/PvP_MainControl.arc"));
        std::fs::create_dir_all(copy.join("files").join("misc")).unwrap();
        std::fs::write(copy.join("files").join("misc").join("PvP_MainControl.arc"), b"made up").unwrap();
        assert!(copied(&copy, "/"));
        assert!(copied(&copy, "/misc/PvP_MainControl.arc"));
        assert!(!copied(&copy, "/permanent/global.bld"));
        assert!(!copied(&copy, "/misc"), "a folder isn't the file asked for");
        let _ = std::fs::remove_dir_all(&into);
    }
}
