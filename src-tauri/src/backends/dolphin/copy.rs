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
//! <percent>%` for each file on its error output.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

/// The percent in one of DolphinTool's progress lines.
fn percent(line: &str) -> Option<u32> {
    let (_, done) = line.strip_prefix("Extracting: ")?.rsplit_once(" | ")?;
    done.trim().strip_suffix('%')?.parse().ok()
}

/// Has DolphinTool copy the game's files out of `disc` into `into`, and
/// returns the folder the game's files are in.
pub fn extract(
    tool: &Path,
    disc: &Path,
    into: &Path,
    progress: &dyn Fn(u32),
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let mut child = super::install::command(tool)
        .arg("extract")
        .arg("-i")
        .arg(disc)
        .arg("-o")
        .arg(into)
        .args(["-g", "-s", "/"])
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
    let mut last = 0;
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Stopped. The pictures from before are kept.".to_string());
        }
        for done in heard.try_iter() {
            if done != last {
                last = done;
                progress(done);
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(_) => break None,
        }
    };
    let finished = status.is_some_and(|status| status.success());
    let files = into.join("DATA");
    if finished && files.join("files").is_dir() {
        Ok(files)
    } else {
        Err("Dolphin couldn't copy the game's files. Check the disc image is whole and try again.".to_string())
    }
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
}
