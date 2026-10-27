//! Reading a file out of a disc image's ISO 9660 file system where it lies,
//! without unpacking anything: the primary volume descriptor, then one
//! directory after another down the path.
//!
//! The layout is ECMA-119 (the ISO 9660 standard), read by byte position: the
//! volume descriptors start at logical sector 16, after the system area; each
//! begins with its type and "CD001"; the primary one (type 1) gives the
//! logical block size at bytes 128 to 131 and the root directory's record at
//! bytes 156 to 189; a set ends with a terminator (type 255). A directory
//! record gives its length at byte 0, where its extent starts at bytes 2 to
//! 5, its size at bytes 10 to 13, its flags at byte 25 (2 for a directory)
//! and its name's length and name from byte 32. Numbers are written both
//! ways round, and the little-endian half is read. A record never runs into
//! the next sector, so a zero length means the rest of the sector is empty.
//! A file's name ends ";1", its version, which is left out when names are
//! matched, and so is the dot of a name with no extension.

use std::io::{Read, Seek, SeekFrom};

/// What a disc image's sectors hold, and the size a block is unless the
/// volume says otherwise.
pub const SECTOR: u64 = 2048;

/// The most of a directory read, far past what a game's needs, so a broken
/// size never asks for gigabytes.
const LARGEST_DIRECTORY: u64 = 4 << 20;

/// How many volume descriptors are looked through for the primary one.
const DESCRIPTORS: u64 = 32;

/// A file or a directory in the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// Where it starts, in bytes from the start of the image.
    pub offset: u64,
    pub size: u64,
    pub directory: bool,
}

fn read_at(image: &mut (impl Read + Seek), offset: u64, size: usize) -> Option<Vec<u8>> {
    image.seek(SeekFrom::Start(offset)).ok()?;
    let mut bytes = vec![0; size];
    image.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// The root directory, from the primary volume descriptor. `None` for an
/// image without an ISO 9660 file system.
pub fn root(image: &mut (impl Read + Seek)) -> Option<(Entry, u64)> {
    for index in 16..16 + DESCRIPTORS {
        let descriptor = read_at(image, index * SECTOR, SECTOR as usize)?;
        if &descriptor[1..6] != b"CD001" {
            return None;
        }
        match descriptor[0] {
            1 => {
                let block = u64::from(u16::from_le_bytes([descriptor[128], descriptor[129]]));
                if !(512..=SECTOR).contains(&block) || !block.is_power_of_two() {
                    return None;
                }
                return Some((record(&descriptor[156..190], block)?, block));
            }
            255 => return None,
            _ => {}
        }
    }
    None
}

/// A directory record's entry, with `block` the volume's logical block size.
fn record(bytes: &[u8], block: u64) -> Option<Entry> {
    Some(Entry {
        offset: u64::from(u32_at(bytes, 2)?) * block,
        size: u64::from(u32_at(bytes, 10)?),
        directory: bytes.get(25)? & 2 != 0,
    })
}

/// A record's name as it is matched: without its version, and without the
/// dot of a name that has no extension.
fn plain(name: &[u8]) -> String {
    let name = String::from_utf8_lossy(name);
    let name = name.split(';').next().unwrap_or_default();
    name.strip_suffix('.').unwrap_or(name).to_string()
}

/// The entry called `name` in `directory`, whatever its case.
pub fn find(image: &mut (impl Read + Seek), directory: Entry, block: u64, name: &str) -> Option<Entry> {
    if !directory.directory || directory.size > LARGEST_DIRECTORY {
        return None;
    }
    let bytes = read_at(image, directory.offset, usize::try_from(directory.size).ok()?)?;
    let mut at = 0;
    while at < bytes.len() {
        let length = usize::from(bytes[at]);
        if length == 0 {
            // The rest of this sector is empty; the next record starts the
            // next one.
            at = (at / SECTOR as usize + 1) * SECTOR as usize;
            continue;
        }
        let entry = bytes.get(at..at + length)?;
        let name_length = usize::from(*entry.get(32)?);
        let own = entry.get(33..33 + name_length)?;
        // A name of one byte, 0 or 1, is the directory itself or its parent.
        if !(name_length == 1 && own[0] <= 1) && plain(own).eq_ignore_ascii_case(name) {
            return record(entry, block);
        }
        at += length;
    }
    None
}

/// The entry at `path`, one name for each directory down, from the root.
pub fn locate(image: &mut (impl Read + Seek), path: &[&str]) -> Option<Entry> {
    let (mut entry, block) = root(image)?;
    for name in path {
        entry = find(image, entry, block, name)?;
    }
    Some(entry)
}

/// A file's bytes, when it is no larger than `limit`.
pub fn read(image: &mut (impl Read + Seek), file: Entry, limit: u64) -> Option<Vec<u8>> {
    if file.directory || file.size > limit {
        return None;
    }
    read_at(image, file.offset, usize::try_from(file.size).ok()?)
}

/// A file's first `count` bytes, or all of it when it is shorter.
pub fn head(image: &mut (impl Read + Seek), file: Entry, count: usize) -> Option<Vec<u8>> {
    if file.directory {
        return None;
    }
    read_at(image, file.offset, count.min(usize::try_from(file.size).ok()?))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Cursor;

    /// A directory record as ECMA-119 lays one out, padded to an even length.
    fn dir_record(name: &[u8], sector: u32, size: u32, directory: bool) -> Vec<u8> {
        let mut bytes = vec![0u8; 33];
        bytes[2..6].copy_from_slice(&sector.to_le_bytes());
        bytes[6..10].copy_from_slice(&sector.to_be_bytes());
        bytes[10..14].copy_from_slice(&size.to_le_bytes());
        bytes[14..18].copy_from_slice(&size.to_be_bytes());
        bytes[25] = if directory { 2 } else { 0 };
        bytes[28] = 1;
        bytes[31] = 1;
        bytes[32] = name.len() as u8;
        bytes.extend_from_slice(name);
        if bytes.len() % 2 == 1 {
            bytes.push(0);
        }
        bytes[0] = bytes.len() as u8;
        bytes
    }

    /// A directory of these records in one sector of its own, after its "."
    /// and "..".
    fn directory(own: u32, parent: u32, records: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = [dir_record(&[0], own, SECTOR as u32, true), dir_record(&[1], parent, SECTOR as u32, true)].concat();
        for one in records {
            bytes.extend_from_slice(one);
        }
        bytes.resize(SECTOR as usize, 0);
        bytes
    }

    /// The folder a path is in, "" for the root.
    fn parent(path: &str) -> &str {
        path.rsplit_once('/').map_or("", |(folder, _)| folder)
    }

    /// An image with `files` (a path and its bytes) laid out as a disc's
    /// mastering tool would: the descriptors at 16 and 17, the root at 18,
    /// then every other folder in a sector of its own, then the files.
    pub(crate) fn image(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut folders: Vec<&str> = vec![""];
        for (path, _) in files {
            let mut folder = parent(path);
            while !folders.contains(&folder) {
                folders.push(folder);
                folder = parent(folder);
            }
        }
        let folder_at = |folder: &str| 18 + folders.iter().position(|known| *known == folder).unwrap() as u32;
        let mut file_at = Vec::new();
        let mut next = 18 + folders.len() as u32;
        for (_, bytes) in files {
            file_at.push(next);
            next += (bytes.len() as u64).div_ceil(SECTOR).max(1) as u32;
        }
        let mut sectors: Vec<Vec<u8>> = vec![vec![0; SECTOR as usize]; next as usize];
        for folder in &folders {
            let mut records = Vec::new();
            for sub in folders.iter().filter(|sub| !sub.is_empty() && parent(sub) == *folder) {
                let name = sub.rsplit('/').next().unwrap();
                records.push(dir_record(name.as_bytes(), folder_at(sub), SECTOR as u32, true));
            }
            for ((path, bytes), at) in files.iter().zip(&file_at) {
                if parent(path) == *folder {
                    let name = format!("{};1", path.rsplit('/').next().unwrap());
                    records.push(dir_record(name.as_bytes(), *at, bytes.len() as u32, false));
                }
            }
            let own = folder_at(folder);
            sectors[own as usize] = directory(own, folder_at(parent(folder)), &records);
        }
        let mut sectors = sectors.concat();
        for ((_, bytes), at) in files.iter().zip(&file_at) {
            let start = *at as usize * SECTOR as usize;
            sectors[start..start + bytes.len()].copy_from_slice(bytes);
        }
        let mut sectors: Vec<Vec<u8>> = sectors.chunks(SECTOR as usize).map(<[u8]>::to_vec).collect();
        let mut primary = vec![0u8; SECTOR as usize];
        primary[0] = 1;
        primary[1..6].copy_from_slice(b"CD001");
        primary[6] = 1;
        primary[8..40].copy_from_slice(&[b' '; 32]);
        primary[128..130].copy_from_slice(&(SECTOR as u16).to_le_bytes());
        primary[130..132].copy_from_slice(&(SECTOR as u16).to_be_bytes());
        primary[156..190].copy_from_slice(&dir_record(&[0], 18, SECTOR as u32, true));
        sectors[16] = primary;
        let mut terminator = vec![0u8; SECTOR as usize];
        terminator[0] = 255;
        terminator[1..6].copy_from_slice(b"CD001");
        terminator[6] = 1;
        sectors[17] = terminator;
        sectors.concat()
    }

    #[test]
    fn a_file_is_found_down_its_path_whatever_its_case_and_version() {
        let sfo = b"\0PSFfake";
        let mut disc = Cursor::new(image(&[("PS3_DISC.SFB", b".SFB"), ("PS3_GAME/PARAM.SFO", sfo)]));
        let file = locate(&mut disc, &["PS3_GAME", "PARAM.SFO"]).unwrap();
        assert_eq!(read(&mut disc, file, 1024).unwrap(), sfo);
        assert!(locate(&mut disc, &["ps3_game", "param.sfo"]).is_some());
        assert!(locate(&mut disc, &["PS3_DISC.SFB"]).is_some_and(|sfb| !sfb.directory));
        assert!(locate(&mut disc, &["PS3_GAME"]).is_some_and(|game| game.directory));
    }

    #[test]
    fn folders_inside_folders_and_files_over_a_sector_are_read() {
        let eboot = [b"SCE".as_slice(), &[7u8; 5000]].concat();
        let mut disc = Cursor::new(image(&[
            ("PS3_GAME/PARAM.SFO", b"sfo"),
            ("PS3_GAME/USRDIR/EBOOT.BIN", &eboot),
            ("PS3_GAME/LICDIR/LIC.DAT", b"PS3LICDA"),
        ]));
        let file = locate(&mut disc, &["PS3_GAME", "USRDIR", "EBOOT.BIN"]).unwrap();
        assert_eq!(read(&mut disc, file, 1 << 20).unwrap(), eboot);
        assert_eq!(head(&mut disc, file, 3).unwrap(), b"SCE");
        let lic = locate(&mut disc, &["PS3_GAME", "LICDIR", "LIC.DAT"]).unwrap();
        assert_eq!(head(&mut disc, lic, 64).unwrap(), b"PS3LICDA", "no more than the file holds");
    }

    #[test]
    fn what_isnt_there_or_is_too_big_is_none() {
        let mut disc = Cursor::new(image(&[("PS3_GAME/PARAM.SFO", b"0123456789")]));
        assert_eq!(locate(&mut disc, &["PS3_GAME", "ICON0.PNG"]), None);
        assert_eq!(locate(&mut disc, &["PS3_GAME", "PARAM.SFO", "MORE"]), None, "a file isn't a directory");
        let file = locate(&mut disc, &["PS3_GAME", "PARAM.SFO"]).unwrap();
        assert_eq!(read(&mut disc, file, 4), None);
        let folder = locate(&mut disc, &["PS3_GAME"]).unwrap();
        assert_eq!(read(&mut disc, folder, 1 << 20), None);
    }

    #[test]
    fn a_name_is_matched_without_its_version_or_a_lone_dot() {
        assert_eq!(plain(b"PARAM.SFO;1"), "PARAM.SFO");
        assert_eq!(plain(b"EBOOT.BIN"), "EBOOT.BIN");
        assert_eq!(plain(b"README.;1"), "README");
        assert_eq!(plain(b"PS3_GAME"), "PS3_GAME");
    }

    #[test]
    fn an_image_without_the_file_system_has_no_root() {
        assert_eq!(root(&mut Cursor::new(vec![0u8; 40 * SECTOR as usize])), None);
        assert_eq!(root(&mut Cursor::new(vec![0u8; 100])), None, "shorter than the system area");
        // A set with no primary descriptor before its terminator.
        let mut disc = image(&[("A", b"a")]);
        disc[16 * SECTOR as usize] = 2;
        disc[17 * SECTOR as usize] = 255;
        assert_eq!(root(&mut Cursor::new(disc)), None);
    }

    #[test]
    fn records_that_stop_short_of_a_sector_go_on_in_the_next() {
        // A directory two sectors long, whose second record starts the
        // second sector after the first sector's unused end.
        let mut disc = image(&[("PS3_GAME/PARAM.SFO", b"sfo")]);
        let root_at = 18 * SECTOR as usize;
        let moved = 30 * SECTOR as usize;
        disc.resize(32 * SECTOR as usize, 0);
        let first = disc[root_at..root_at + 68].to_vec();
        let game = dir_record(b"PS3_GAME", 19, SECTOR as u32, true);
        disc[moved..moved + 68].copy_from_slice(&first);
        disc[moved + SECTOR as usize..moved + SECTOR as usize + game.len()].copy_from_slice(&game);
        let mut root_record = dir_record(&[0], 30, 2 * SECTOR as u32, true);
        root_record.resize(34, 0);
        disc[16 * SECTOR as usize + 156..16 * SECTOR as usize + 190].copy_from_slice(&root_record);
        let mut disc = Cursor::new(disc);
        assert!(locate(&mut disc, &["PS3_GAME", "PARAM.SFO"]).is_some());
    }
}
