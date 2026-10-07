//! Unpacking a compressed dump.
//!
//! These archives run to 19 GB, so extraction reports progress as it goes and
//! stops when asked. Stopping leaves nothing behind: a half-unpacked game is
//! not something the user should have to clean up.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    SevenZip,
    Zip,
}

/// Decided by what the file actually starts with, not by its extension, so a
/// mislabelled file is caught before we start unpacking 19 GB of it.
pub fn detect_kind(path: &Path) -> Option<Kind> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut magic = [0u8; 6];
    file.read_exact(&mut magic).ok()?;
    if magic == [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C] {
        Some(Kind::SevenZip)
    } else if magic[..2] == [0x50, 0x4B] {
        Some(Kind::Zip)
    } else {
        None
    }
}

/// Total unpacked size, read from the archive's index before extracting, so
/// progress is a real percentage and we can check the disk first.
pub fn unpacked_size(path: &Path, kind: Kind) -> Result<u64, String> {
    match kind {
        Kind::SevenZip => {
            let reader = open_7z(path)?;
            Ok(reader.archive().files.iter().map(|f| f.size).sum())
        }
        Kind::Zip => {
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            let mut total = 0;
            for i in 0..archive.len() {
                total += archive.by_index(i).map_err(|e| e.to_string())?.size();
            }
            Ok(total)
        }
    }
}

/// The path of every file inside, read from the archive's index, so what an
/// archive holds can be judged before a byte of it is unpacked.
pub fn names(path: &Path, kind: Kind) -> Result<Vec<String>, String> {
    match kind {
        Kind::SevenZip => {
            let reader = open_7z(path)?;
            Ok(reader.archive().files.iter().map(|f| f.name.clone()).collect())
        }
        Kind::Zip => {
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            Ok(archive.file_names().map(str::to_string).collect())
        }
    }
}

/// One small file out of the archive, such as the PARAM.SFO that says which
/// game it holds, when that takes a moment. `None` when getting it would mean
/// unpacking much of the archive first, as it can in a solid 7z, where a file
/// is reached only by decoding everything packed before it.
pub fn read_small(path: &Path, kind: Kind, name: &str) -> Option<Vec<u8>> {
    // Decoded in a second or two.
    read_within(path, kind, name, 64 * 1024 * 1024)
}

/// `read_small`, with how much may be decoded ahead of the file to get it.
fn read_within(path: &Path, kind: Kind, name: &str, most_ahead: u64) -> Option<Vec<u8>> {
    // A PARAM.SFO is a couple of kilobytes and a meta.xml a few dozen.
    const LARGEST: u64 = 1024 * 1024;

    match kind {
        Kind::Zip => {
            let file = std::fs::File::open(path).ok()?;
            let mut archive = zip::ZipArchive::new(file).ok()?;
            let mut entry = archive.by_name(name).ok()?;
            if entry.size() > LARGEST {
                return None;
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).ok()?;
            Some(bytes)
        }
        Kind::SevenZip => {
            let mut reader = open_7z(path).ok()?;
            let archive = reader.archive();
            let at = archive.files.iter().position(|file| file.name == name)?;
            if archive.files[at].size > LARGEST {
                return None;
            }
            if archive.is_solid {
                let block = archive.stream_map.file_block_index.get(at).copied().flatten()?;
                let first = *archive.stream_map.block_first_file_index.get(block)?;
                let ahead: u64 = (first..at)
                    .filter(|&i| archive.stream_map.file_block_index[i] == Some(block))
                    .map(|i| archive.files[i].size)
                    .sum();
                if ahead > most_ahead {
                    return None;
                }
            }
            reader.read_file(name).ok()
        }
    }
}

#[derive(Debug)]
pub struct Cancelled;

/// Unpacks into `dest`, calling `progress` with bytes written so far.
pub fn extract(
    path: &Path,
    kind: Kind,
    dest: &Path,
    cancel: &Arc<AtomicBool>,
    progress: &mut dyn FnMut(u64),
) -> Result<Result<(), Cancelled>, String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;

    // Game dumps nest deeply enough to pass Windows' 260-character path limit:
    // one real archive reaches 283. Writing through the extended-length form of
    // the destination lifts that limit; the plain path is what we hand back.
    let write_root = extended_length(dest);
    let result = match kind {
        Kind::SevenZip => extract_7z(path, &write_root, cancel, progress),
        Kind::Zip => extract_zip(path, &write_root, cancel, progress),
    };

    // Whether the user stopped it or it failed, what is on disk is a partial
    // game. Take it away rather than leave it looking importable.
    match &result {
        Ok(Ok(())) => {}
        _ => {
            let _ = std::fs::remove_dir_all(dest);
        }
    }
    result
}

/// The decoder pulls from the archive in small reads. Straight off a file
/// handle that is one syscall each, which dominates the time on an archive
/// this size, so it reads through a large buffer instead.
fn open_7z(
    path: &Path,
) -> Result<sevenz_rust2::ArchiveReader<std::io::BufReader<std::fs::File>>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let buffered = std::io::BufReader::with_capacity(4 * 1024 * 1024, file);
    sevenz_rust2::ArchiveReader::new(buffered, sevenz_rust2::Password::empty())
        .map_err(|e| e.to_string())
}

fn extract_7z(
    path: &Path,
    dest: &Path,
    cancel: &Arc<AtomicBool>,
    progress: &mut dyn FnMut(u64),
) -> Result<Result<(), Cancelled>, String> {
    let mut reader = open_7z(path)?;

    let mut written: u64 = 0;
    let mut stopped = false;
    // Reported after the walk rather than raised inside it, so we never have to
    // build one of the archive crate's own error values just to stop.
    let mut failure: Option<String> = None;

    let walked = reader.for_each_entries(|entry, data| {
        if cancel.load(Ordering::Relaxed) {
            stopped = true;
            return Ok(false);
        }
        let Some(target) = safe_join(dest, entry.name()) else {
            return Ok(true);
        };
        if entry.is_directory() {
            let _ = std::fs::create_dir_all(&target);
            return Ok(true);
        }
        if let Some(parent) = target.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                failure = Some(e.to_string());
                return Ok(false);
            }
        }

        let mut file = match std::fs::File::create(&target) {
            Ok(file) => file,
            Err(e) => {
                failure = Some(e.to_string());
                return Ok(false);
            }
        };
        match copy_watching_cancel(data, &mut file, cancel, &mut written, progress) {
            Ok(Completed::Whole) => Ok(true),
            Ok(Completed::Stopped) => {
                stopped = true;
                Ok(false)
            }
            Err(e) => {
                failure = Some(e.to_string());
                Ok(false)
            }
        }
    });

    // Stopping on request outranks whatever the walk reported on its way out.
    if stopped {
        return Ok(Err(Cancelled));
    }
    if let Some(message) = failure {
        return Err(message);
    }
    walked.map_err(|e| e.to_string())?;
    Ok(Ok(()))
}

fn extract_zip(
    path: &Path,
    dest: &Path,
    cancel: &Arc<AtomicBool>,
    progress: &mut dyn FnMut(u64),
) -> Result<Result<(), Cancelled>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut written: u64 = 0;

    for i in 0..archive.len() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(Err(Cancelled));
        }
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        // enclosed_name already refuses anything that climbs out of the folder.
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let Some(target) = safe_join(dest, &name.to_string_lossy()) else {
            continue;
        };
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&target).map_err(|e| e.to_string())?;
        match copy_watching_cancel(&mut entry, &mut out, cancel, &mut written, progress) {
            Ok(Completed::Whole) => {}
            Ok(Completed::Stopped) => return Ok(Err(Cancelled)),
            Err(e) => return Err(e.to_string()),
        }
    }

    Ok(Ok(()))
}

enum Completed {
    Whole,
    Stopped,
}

/// Copies in chunks so a single huge file inside the archive still reports
/// progress and can still be stopped partway through it.
fn copy_watching_cancel(
    from: &mut dyn Read,
    to: &mut dyn std::io::Write,
    cancel: &Arc<AtomicBool>,
    written: &mut u64,
    progress: &mut dyn FnMut(u64),
) -> std::io::Result<Completed> {
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(Completed::Stopped);
        }
        let read = from.read(&mut buffer)?;
        if read == 0 {
            return Ok(Completed::Whole);
        }
        to.write_all(&buffer[..read])?;
        *written += read as u64;
        progress(*written);
    }
}

/// The `\\?\` form of a path, which Windows exempts from its 260-character
/// limit. Canonicalising produces it; anything joined onto the result keeps it.
pub fn extended_length(path: &Path) -> PathBuf {
    if cfg!(windows) {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}

/// A folder in `parent` to unpack an archive called `name` into that nothing
/// uses yet: the name itself, then "name (2)", "name (3)" and so on. One
/// game's versions for two consoles often come in archives named alike.
pub fn free_folder(parent: &Path, name: &str) -> PathBuf {
    let first = parent.join(name);
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| parent.join(format!("{name} ({n})")))
        .find(|folder| !folder.exists())
        .unwrap_or(first)
}

/// Keeps an archive from writing outside the folder we chose for it, however
/// its entries are named.
fn safe_join(dest: &Path, name: &str) -> Option<PathBuf> {
    let mut out = dest.to_path_buf();
    for part in name.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => return None,
            _ if part.contains(':') => return None,
            _ => out.push(part),
        }
    }
    (out != dest).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_kind_from_the_file_itself() {
        let dir = std::env::temp_dir().join(format!("omoio-arc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let seven = dir.join("a.7z");
        std::fs::write(&seven, [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0, 0]).unwrap();
        assert_eq!(detect_kind(&seven), Some(Kind::SevenZip));

        // Named .7z but actually a zip: believe the bytes.
        let liar = dir.join("b.7z");
        std::fs::write(&liar, b"PK\x03\x04rest").unwrap();
        assert_eq!(detect_kind(&liar), Some(Kind::Zip));

        let neither = dir.join("c.7z");
        std::fs::write(&neither, b"just text here").unwrap();
        assert_eq!(detect_kind(&neither), None);

        let tiny = dir.join("d.7z");
        std::fs::write(&tiny, b"PK").unwrap();
        assert_eq!(detect_kind(&tiny), None);

        assert_eq!(detect_kind(Path::new("no-such-archive.7z")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_taken_folder_name_gets_a_number() {
        let dir = std::env::temp_dir().join(format!("omoio-free-{}", std::process::id()));
        let name = "Skylanders - Swap Force (Europe)";
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(free_folder(&dir, name), dir.join(name));

        std::fs::create_dir(dir.join(name)).unwrap();
        assert_eq!(free_folder(&dir, name), dir.join(format!("{name} (2)")));

        std::fs::create_dir(dir.join(format!("{name} (2)"))).unwrap();
        assert_eq!(free_folder(&dir, name), dir.join(format!("{name} (3)")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reads a real archive's index. Archives are not committed, so point this
    /// at one to run it:
    ///   set OMOIO_ARCHIVE=D:\dumps\game.7z
    ///   cargo test real_archive -- --ignored --nocapture
    #[test]
    #[ignore = "needs OMOIO_ARCHIVE pointing at a real archive"]
    fn reads_a_real_archive_index() {
        let path = std::env::var("OMOIO_ARCHIVE").expect("set OMOIO_ARCHIVE");
        let path = Path::new(&path);
        let kind = detect_kind(path).expect("should recognise the archive");
        let size = unpacked_size(path, kind).expect("should read the index");
        println!("kind:     {kind:?}");
        println!("unpacked: {size} bytes ({:.1} GB)", size as f64 / 1024f64.powi(3));
        assert!(size > 0);
    }

    /// Unpacks a real archive far enough to prove progress is reported, then
    /// stops it and checks nothing is left behind. Same setup as above:
    ///   cargo test stops_and_cleans_up -- --ignored --nocapture
    #[test]
    #[ignore = "needs OMOIO_ARCHIVE pointing at a real archive"]
    fn stops_and_cleans_up_when_cancelled() {
        let path = std::env::var("OMOIO_ARCHIVE").expect("set OMOIO_ARCHIVE");
        let path = Path::new(&path);
        let kind = detect_kind(path).unwrap();
        let dest = std::env::temp_dir().join(format!("omoio-cancel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dest);

        let stop_after: u64 = std::env::var("OMOIO_STOP_AFTER_MB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(200)
            * 1024
            * 1024;

        let cancel = Arc::new(AtomicBool::new(false));
        let stopper = cancel.clone();
        let mut furthest = 0u64;
        let mut updates = 0u32;
        let started = std::time::Instant::now();

        let outcome = extract(path, kind, &dest, &cancel, &mut |done| {
            furthest = done;
            updates += 1;
            if done > stop_after {
                stopper.store(true, Ordering::Relaxed);
            }
        })
        .expect("extraction itself should not fail");

        let elapsed = started.elapsed().as_secs_f64();
        println!(
            "progress updates: {updates}, furthest: {furthest} bytes, {:.1}s, {:.1} MB/s",
            elapsed,
            furthest as f64 / 1024.0 / 1024.0 / elapsed,
        );
        assert!(outcome.is_err(), "should report that it was stopped");
        assert!(updates > 1, "should report progress as it goes");
        assert!(furthest > stop_after, "should have unpacked something first");
        assert!(!dest.exists(), "a stopped extraction should leave nothing behind");
    }

    /// Unpacks a real archive all the way and checks a game comes out of it.
    /// Slow and needs room on disk, so it only runs when asked:
    ///   set OMOIO_ARCHIVE=D:\dumps\game.7z
    ///   set OMOIO_EXTRACT_TO=D:\PS3 Games
    ///   cargo test --release unpacks_a_real_archive -- --ignored --nocapture
    #[test]
    #[ignore = "needs OMOIO_ARCHIVE and OMOIO_EXTRACT_TO"]
    fn unpacks_a_real_archive_completely() {
        let archive = std::env::var("OMOIO_ARCHIVE").expect("set OMOIO_ARCHIVE");
        let into = std::env::var("OMOIO_EXTRACT_TO").expect("set OMOIO_EXTRACT_TO");
        let archive = Path::new(&archive);
        let kind = detect_kind(archive).unwrap();
        let expected = unpacked_size(archive, kind).unwrap();

        let dest = Path::new(&into).join(archive.file_stem().unwrap());
        let _ = std::fs::remove_dir_all(&dest);

        let cancel = Arc::new(AtomicBool::new(false));
        let mut written = 0u64;
        let started = std::time::Instant::now();
        extract(archive, kind, &dest, &cancel, &mut |done| written = done)
            .expect("extraction should succeed")
            .expect("should not report being cancelled");
        let elapsed = started.elapsed().as_secs_f64();

        println!(
            "unpacked {written} of {expected} bytes in {:.0}s ({:.1} MB/s)",
            elapsed,
            written as f64 / 1024.0 / 1024.0 / elapsed
        );
        assert_eq!(written, expected, "should unpack every byte the index promised");

        let game = crate::import::identify(&dest).expect("a game should come out of it");
        println!("game: {} {} v{}", game.title_id, game.title, game.version.as_deref().unwrap_or("-"));
        println!("root: {}", game.path.display());
        println!("beside the dump: {} bytes", expected - game.size_bytes);

        // The game measures a little under the archive as a whole: readmes and
        // the like sit beside the dump rather than inside it, and the library
        // should report the game's size, not the download's.
        assert!(game.size_bytes > 0, "the game should have a size");
        assert!(
            game.size_bytes <= expected,
            "the dump cannot be larger than the archive it came out of"
        );
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("omoio-small-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_small_file_comes_out_of_a_zip_without_unpacking_the_rest() {
        use std::io::Write;
        let dir = scratch("zip");
        let path = dir.join("game.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("Game/PS3_GAME/USRDIR/EBOOT.BIN", options).unwrap();
        zip.write_all(&[0; 4096]).unwrap();
        zip.start_file("Game/PS3_GAME/PARAM.SFO", options).unwrap();
        zip.write_all(b"sfo").unwrap();
        zip.finish().unwrap();

        assert_eq!(read_small(&path, Kind::Zip, "Game/PS3_GAME/PARAM.SFO").as_deref(), Some(&b"sfo"[..]));
        assert_eq!(read_small(&path, Kind::Zip, "Game/PS3_GAME/ICON0.PNG"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// In a solid 7z a file is reached by decoding everything packed before
    /// it, so one sitting behind the game's data is not worth the wait.
    #[test]
    fn a_small_file_comes_out_of_a_solid_7z_only_when_little_is_ahead_of_it() {
        let dir = scratch("7z");
        let path = dir.join("game.7z");
        let mut writer = sevenz_rust2::ArchiveWriter::create(&path).unwrap();
        writer
            .push_archive_entries(
                vec![
                    sevenz_rust2::ArchiveEntry::new_file("Game/PS3_GAME/PARAM.SFO"),
                    sevenz_rust2::ArchiveEntry::new_file("Game/PS3_GAME/USRDIR/EBOOT.BIN"),
                    sevenz_rust2::ArchiveEntry::new_file("Game/meta/meta.xml"),
                ],
                [&b"sfo"[..], &[7u8; 4096][..], &b"<menu/>"[..]]
                    .into_iter()
                    .map(sevenz_rust2::SourceReader::new)
                    .collect(),
            )
            .unwrap();
        writer.finish().unwrap();
        assert!(open_7z(&path).unwrap().archive().is_solid);

        assert_eq!(read_within(&path, Kind::SevenZip, "Game/PS3_GAME/PARAM.SFO", 1024).as_deref(), Some(&b"sfo"[..]));
        assert_eq!(read_within(&path, Kind::SevenZip, "Game/meta/meta.xml", 1024), None, "4 KB ahead of it");
        assert_eq!(read_within(&path, Kind::SevenZip, "Game/meta/meta.xml", 8192).as_deref(), Some(&b"<menu/>"[..]));
        assert_eq!(read_small(&path, Kind::SevenZip, "nothing.txt"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_entries_that_escape_the_destination() {
        let dest = Path::new("C:\\games\\out");
        assert_eq!(safe_join(dest, "..\\..\\evil.exe"), None);
        assert_eq!(safe_join(dest, "a/../../evil.exe"), None);
        assert_eq!(safe_join(dest, "C:\\Windows\\evil.exe"), None);
        assert_eq!(safe_join(dest, ""), None);
    }

    #[test]
    fn joins_ordinary_entries_below_the_destination() {
        let dest = Path::new("C:\\games\\out");
        assert_eq!(
            safe_join(dest, "PS3_GAME/USRDIR/EBOOT.BIN"),
            Some(dest.join("PS3_GAME").join("USRDIR").join("EBOOT.BIN"))
        );
        assert_eq!(
            safe_join(dest, "PS3_GAME\\PARAM.SFO"),
            Some(dest.join("PS3_GAME").join("PARAM.SFO"))
        );
        assert_eq!(safe_join(dest, "./PS3_DISC.SFB"), Some(dest.join("PS3_DISC.SFB")));
    }
}
