//! A Wii U disc image as Cemu reads it: which game it holds, by its file's
//! name and, for a Skylanders game, by the product code at the start of its
//! sectors.
//!
//! Omoio decrypts nothing here. The title id of a disc can't be read without
//! the keys, so a disc is known by a hash of its file's name, and its game by
//! the product code, which lies in the clear.

use super::file_name;
use crate::core::console::Console;
use crate::core::library::Game;
use std::path::Path;

/// A Wii U disc image, which Cemu reads with the user's own keys.
pub(super) fn is_disc_image(name: &str) -> bool {
    let file = file_name(name);
    file.ends_with(".wud") || file.ends_with(".wux")
}

/// A game's name from its image's file name, without the region and language
/// notes such names carry: "Game (Europe) (En,Fr)" is "Game".
pub(super) fn title_from_file(stem: &str) -> String {
    let mut title = String::new();
    let mut depth = 0;
    for c in stem.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth as i32 - 1).max(0) as usize,
            _ if depth == 0 => title.push(c),
            _ => {}
        }
    }
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        stem.to_string()
    } else {
        title
    }
}

/// An id for a disc image, which cannot be read for its real one without
/// decrypting it. Made from the file's name so it is the same every time.
pub(super) fn disc_id(file: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in file.to_lowercase().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("WUD{:013X}", hash >> 12)
}

/// The product code at the start of a Wii U disc, "WUP-P-AS7J-00-...", as
/// the four letters after "WUP-P-". It is in the clear: Cemu reads it as it
/// lies, with no key (`DiscHeaderA`, src/Cafe/Filesystem/FST/FST.cpp lines
/// 121 to 125 and 249 to 252), and decrypts only from the partition table at
/// 0x18000 on (lines 137 to 140, 196 to 209 and 260 to 265). It tells a .wud
/// by its "WUP-" (Cafe/TitleList/TitleInfo.cpp lines 38 to 56; Bertrram/Cemu
/// v2.6-1). A .wux keeps the disc's sectors behind an index table: a 32
/// byte header of "WUX0" and 0x1099D02E, the sector size and the disc's
/// size, one little-endian sector number for each of the disc's sectors,
/// then the sectors from the next whole sector on (Cafe/Filesystem/WUD/wud.h
/// lines 3 to 26, wud.cpp lines 19 to 54 and 99 to 109). Read off Bertram's
/// four .wux files on 28 October 2026: "WUP-P-ASFP-00-301EUR-0" for SWAP
/// Force. Nothing a key is needed for is read.
pub(super) fn product_code(image: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(image).ok()?;
    let mut header = [0u8; 32];
    file.read_exact(&mut header).ok()?;
    let mut code = [0u8; 10];
    if header[..8] == [b'W', b'U', b'X', b'0', 0x2E, 0xD0, 0x99, 0x10] {
        let sector = u64::from(u32::from_le_bytes(header[8..12].try_into().ok()?));
        let size = u64::from_le_bytes(header[16..24].try_into().ok()?);
        // Cemu's own bounds for a sector (wud.cpp line 33).
        if !(0x100..0x1000_0000).contains(&sector) {
            return None;
        }
        let mut first = [0u8; 4];
        file.read_exact(&mut first).ok()?;
        let sectors = (32 + size.div_ceil(sector) * 4).div_ceil(sector) * sector;
        file.seek(SeekFrom::Start(sectors + u64::from(u32::from_le_bytes(first)) * sector)).ok()?;
        file.read_exact(&mut code).ok()?;
    } else {
        code.copy_from_slice(&header[..10]);
    }
    let game = std::str::from_utf8(&code).ok()?.strip_prefix("WUP-P-")?;
    game.bytes().all(|b| b.is_ascii_alphanumeric()).then(|| game.to_ascii_uppercase())
}

/// The game's English name, where the disc's product code says which
/// Skylanders game it is and the name it was given doesn't
/// (`figures::disc_english_name`).
pub(super) fn named_by_disc(image: &Path, title: &str) -> Option<&'static str> {
    crate::core::figures::disc_english_name(&product_code(image)?, title)
}

pub(super) fn identify_disc(image: &Path) -> Game {
    let stem = image.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let file = image.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let title = title_from_file(&stem);
    Game {
        console: Console::WiiU,
        title_id: disc_id(&file),
        title: named_by_disc(image, &title).map_or(title, str::to_string),
        version: None,
        update_version: None,
        size_bytes: std::fs::metadata(image).map(|m| m.len()).unwrap_or(0),
        path: image.to_path_buf(),
    }
}
