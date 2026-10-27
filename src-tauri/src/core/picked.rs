//! What a picked file is when Omoio can't take it, so the import can say
//! what it saw rather than only that it isn't a game Omoio plays: an
//! archive Omoio doesn't open, or a game for a console Omoio doesn't run.
//!
//! Only what is certain is told: a marker in the file's own bytes, or an
//! extension no other console uses. Anything less is left to the message
//! that names the consoles Omoio takes. Each marker names the public source
//! it was read from, all read on 27 October 2026. The files come from
//! anywhere, so nothing here reads past the bytes it is given.

use crate::core::iso9660;
use std::io::{Read, Seek, SeekFrom};

/// How much of a file's start `other_archive` and `other_console` look at.
/// The furthest marker, an XCI's header after the card's key area, ends at
/// 0x110C. Fewer bytes are fine: a marker past them counts as not there.
pub const HEAD: usize = 0x2000;

/// What Omoio opens itself, by the bytes `archive::detect_kind` goes by: a
/// 7z's signature and a zip's "PK".
const SEVEN_ZIP: &[u8] = &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];
const ZIP: &[u8] = b"PK";

/// Archives Omoio doesn't open, by the bytes each starts with.
const ARCHIVES: &[&[u8]] = &[
    // RAR 1.5 to 4, then RAR 5 (RAR's technical note,
    // rarlab.com/technote.htm, "RAR 5.0 signature").
    b"Rar!\x1A\x07\x00",
    b"Rar!\x1A\x07\x01\x00",
    // gzip's two identifying bytes and 8, deflate, the one method it
    // defines (RFC 1952, 2.3.1). A .tgz is a tar inside gzip.
    &[0x1F, 0x8B, 0x08],
    // xz's header magic (The .xz File Format 1.2.1, 2.1.1.1).
    &[0xFD, b'7', b'z', b'X', b'Z', 0x00],
    // A Zstandard frame's magic number, 0xFD2FB528 little-endian (RFC 8878,
    // 3.1.1).
    &[0x28, 0xB5, 0x2F, 0xFD],
    // An LZ4 frame's, 0x184D2204 little-endian (LZ4 Frame Format
    // Description, github.com/lz4/lz4, doc/lz4_Frame_format.md).
    &[0x04, 0x22, 0x4D, 0x18],
    // A cabinet's "MSCF", then its first reserved field, which is set to
    // zero (Microsoft Cabinet Format, CFHEADER,
    // learn.microsoft.com/previous-versions/bb417343).
    b"MSCF\0\0\0\0",
];

/// A tar header's magic at byte 257: "ustar" and a NUL in POSIX's
/// (POSIX.1-2017, pax, "ustar Interchange Format"), "ustar" and two spaces
/// in GNU tar's older one (GNU tar, src/tar.h, OLDGNU_MAGIC). The five
/// letters are what both share.
const TAR_MAGIC_AT: usize = 257;
const TAR_MAGIC: &[u8] = b"ustar";

/// bzip2 starts "BZh" and a block size from '1' to '9', then the first
/// block's magic, pi's digits, or for an empty stream the end's, those of
/// its square root. No specification exists; this is the layout the
/// forensics wiki gives (forensics.wiki/bzip2).
const BZIP2_BLOCK: &[u8] = &[0x31, 0x41, 0x59, 0x26, 0x53, 0x59];
const BZIP2_END: &[u8] = &[0x17, 0x72, 0x45, 0x38, 0x50, 0x90];

/// Markers that alone make a file's console certain: where each sits, what
/// it is, and the console.
const MARKERS: &[(usize, &[u8], &str)] = &[
    // A Switch package (.nsp, or .nsz compressed) is a PartitionFS, which
    // starts "PFS0" (switchbrew, NCA, PFS0); linkle's documentation names
    // the two together, "PFS0/NSP" (docs.rs/crate/linkle). A homebrew
    // program (.nro) has "NRO0" at 0x10 (switchbrew, NRO).
    (0, b"PFS0", "Switch"),
    (0x10, b"NRO0", "Switch"),
    // A 3DS card image (.3ds, .cci) is an NCSD and a program (.cxi) an
    // NCCH, each named at 0x100, after its header's signature (3dbrew, NCSD
    // and NCCH). A homebrew program starts "3DSX" (3dbrew, 3DSX Format).
    (0x100, b"NCSD", "3DS"),
    (0x100, b"NCCH", "3DS"),
    (0, b"3DSX", "3DS"),
    // An N64 image's first word, 0x80371240 in every commercial game, which
    // emulators read to tell a dump's byte order (n64brew, ROM Header): as
    // it is, swapped in pairs, and little-endian.
    (0, &[0x80, 0x37, 0x12, 0x40], "N64"),
    (0, &[0x37, 0x80, 0x40, 0x12], "N64"),
    (0, &[0x40, 0x12, 0x37, 0x80], "N64"),
    // "NES" and MS-DOS's end of file (NESdev wiki, iNES).
    (0, b"NES\x1A", "NES"),
    // A PS1 program's header (DuckStation, src/core/bios.h, PSEXEHeader).
    (0, b"PS-X EXE", "PS1"),
    // A PSP program's package, EBOOT.PBP (PPSSPP, Core/Loaders.cpp,
    // Identify_File).
    (0, b"\0PBP", "PSP"),
    // An Xbox program (xboxdevwiki, Xbe) and an Xbox 360 one (free60, XEX).
    (0, b"XBEH", "Xbox"),
    (0, b"XEX2", "Xbox 360"),
];

/// The 4 KiB key area a Switch card keeps before its header, which a dump
/// may carry along (switchbrew, XCI: CardKeyArea at 0, CardHeader at
/// 0x1000).
const KEY_AREA: usize = 0x1000;

/// The most of a SYSTEM.CNF read, far past the few lines one holds, so a
/// broken size never asks for more.
const LARGEST_SYSTEM_CNF: u64 = 4096;

/// An Xbox disc's own file system has its descriptor 64 KiB into the game
/// partition, at sector 32, starting and ending "MICROSOFT*XBOX*MEDIA"
/// (xboxdevwiki, XDVDFS; extract-xiso.c, verify_xiso).
const XBOX_DESCRIPTOR: u64 = 0x10000;
const XBOX_MEDIA: &[u8] = b"MICROSOFT*XBOX*MEDIA";

/// Where the game partition starts in an image of a whole disc, and whose
/// disc that is (extract-xiso.c): 0x18300000 on the Xbox's own discs
/// (XGD1_LSEEK_OFFSET), 0x0FD90000 on the Xbox 360's (GLOBAL_LSEEK_OFFSET,
/// added with "XBox 360 iso extraction") and 0x02080000 on the 360's later
/// XGD3 discs (XGD3_LSEEK_OFFSET). An XISO, the game partition alone, has
/// it at 0.
const XBOX_PARTITIONS: [(u64, &str); 3] = [(0x1830_0000, "Xbox"), (0x0FD9_0000, "Xbox 360"), (0x0208_0000, "Xbox 360")];

/// The most of an XISO's root directory read, so a broken size never asks
/// for gigabytes.
const LARGEST_XBOX_ROOT: u64 = 1 << 20;

/// Whether a picked file is an archive Omoio can't open. Omoio opens .zip
/// and .7z, and says so when this is true.
pub fn other_archive(name: &str, head: &[u8]) -> bool {
    match extension(name).as_str() {
        // Cemu's .wua keeps a Wii U game's files in zstd blocks, so it most
        // often starts as zstd does (backends/cemu/wua.rs). Omoio plays it.
        "wua" => false,
        // WinZip names a zip .zipx when it has used one of its advanced
        // methods, such as LZMA, PPMd or WavPack, and says a compatible tool
        // is needed to open it ("What is a Zipx File?", kb.corel.com/125951).
        // Its bytes are a zip's, so its name tells it.
        "zipx" => true,
        _ if head.starts_with(SEVEN_ZIP) || head.starts_with(ZIP) => false,
        _ => ARCHIVES.iter().any(|magic| head.starts_with(magic)) || at(head, TAR_MAGIC_AT, TAR_MAGIC) || is_bzip2(head),
    }
}

/// The console a single picked file is a game for, when it isn't one Omoio
/// runs and the file's own bytes, or an extension no other console uses,
/// make it certain. A .iso, .bin or .cue is never told by its name, since
/// many consoles' discs are kept so, a PS3's, a Wii's and a GameCube's
/// among them; `console_in_image` reads a .iso's file system instead. A
/// package that starts "\x7FPKG" is the PS3's, or a PSP's or PS Vita's,
/// which its header gives one type for (psdevwiki, PS3, PKG files), so it
/// is left unnamed.
pub fn other_console(name: &str, head: &[u8]) -> Option<&'static str> {
    by_marker(head).or_else(|| by_extension(&extension(name), head))
}

/// The console a disc image (.iso) is for, when it isn't one Omoio runs:
/// read from a PlayStation disc's ISO 9660 file system, or from an Xbox
/// disc's own. A PS3 disc, with PS3_DISC.SFB or PS3_GAME/PARAM.SFO at its
/// root, is `None`, as is anything not told.
pub fn console_in_image(image: &mut (impl Read + Seek)) -> Option<&'static str> {
    if iso9660::root(image).is_some() {
        if iso9660::locate(image, &["PS3_DISC.SFB"]).is_some() || iso9660::locate(image, &["PS3_GAME", "PARAM.SFO"]).is_some() {
            return None;
        }
        if let Some(console) = playstation(image) {
            return Some(console);
        }
    }
    xbox(image)
}

/// A file's extension, lower case, from the last part of its name.
fn extension(name: &str) -> String {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    file.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Whether `head` holds `marker` at `offset`.
fn at(head: &[u8], offset: usize, marker: &[u8]) -> bool {
    head.get(offset..offset + marker.len()) == Some(marker)
}

fn is_bzip2(head: &[u8]) -> bool {
    match head {
        [b'B', b'Z', b'h', b'1'..=b'9', rest @ ..] => rest.starts_with(BZIP2_BLOCK) || rest.starts_with(BZIP2_END),
        _ => false,
    }
}

fn by_marker(head: &[u8]) -> Option<&'static str> {
    MARKERS
        .iter()
        .find(|(offset, marker, _)| at(head, *offset, marker))
        .map(|(_, _, console)| *console)
        .or_else(|| is_xci(head).then_some("Switch"))
        .or_else(|| is_ps4(head).then_some("PS4"))
        .or_else(|| is_gba(head).then_some("Game Boy Advance"))
        .or_else(|| ds(head))
}

/// Extensions that name one console's games and nothing else a player
/// would pick.
fn by_extension(extension: &str, head: &[u8]) -> Option<&'static str> {
    match extension {
        // A CIA has no fixed bytes, as its header's sizes are only the usual
        // ones, but its extension is the format's name, CTR Importable
        // Archive (3dbrew, CIA).
        "cia" => Some("3DS"),
        // A Game Boy cartridge's one marker is Nintendo's logo, which Omoio
        // doesn't copy, so its extensions tell it (libretro docs, Gambatte).
        // Gambatte's .dmg is left out, as a Mac disk image is named so too.
        "gb" => Some("Game Boy"),
        "gbc" => Some("Game Boy Color"),
        // A SNES cartridge has no marker, so its two usual extensions tell
        // it (libretro docs, Snes9x).
        "sfc" | "smc" => Some("SNES"),
        // A PS Vita package is a plain zip (docs.fileformat.com, VPK), while
        // Valve's .vpk is a format of its own.
        "vpk" if head.starts_with(ZIP) => Some("PS Vita"),
        _ => None,
    }
}

/// A Switch card image (.xci, or .xcz compressed): "HEAD" at 0x100 of its
/// header and, at 0x108, the backup area's start, which is always
/// FFFFFFFFh (switchbrew, XCI).
fn is_xci(head: &[u8]) -> bool {
    [0, KEY_AREA]
        .iter()
        .any(|&start| at(head, start + 0x100, b"HEAD") && at(head, start + 0x108, &[0xFF; 4]))
}

/// A PS4 package: "\x7FCNT", then at 0x40 its content ID (orbis-pkg,
/// src/header.rs), whose title ID, from the ID's eighth character, is a
/// PS4's CUSA (ConsoleMods wiki, PS4 Files and Directories). A PS5 package
/// starts "\x7FCNT" too (psdevwiki, PS5, Package Files), so the title ID is
/// what makes it the PS4's.
fn is_ps4(head: &[u8]) -> bool {
    head.starts_with(b"\x7FCNT") && at(head, 0x47, b"CUSA")
}

/// A GBA cartridge's header: an ARM branch to start from, "B rom_start" in
/// GBATEK's words, which is EAh in its top byte; 96h at 0xB2, which the
/// hardware requires; and at 0xBD the complement of the bytes from 0xA0 to
/// 0xBC (GBATEK, GBA Cartridge Header).
fn is_gba(head: &[u8]) -> bool {
    let Some(header) = head.get(..0xBE) else {
        return false;
    };
    let complement = header[0xA0..=0xBC]
        .iter()
        .fold(0u8, |sum, &byte| sum.wrapping_sub(byte))
        .wrapping_sub(0x19);
    header[3] == 0xEA && header[0xB2] == 0x96 && header[0xBD] == complement
}

/// A DS cartridge's header: its logo's checksum at 0x15C, which is fixed at
/// CF56h, and its own at 0x15E, a CRC-16 of everything before, with the
/// unit code at 0x12 saying whether it is for the DSi alone (GBATEK, DS
/// Cartridge Header).
fn ds(head: &[u8]) -> Option<&'static str> {
    let header = head.get(..0x160)?;
    let logo = u16::from_le_bytes([header[0x15C], header[0x15D]]);
    let own = u16::from_le_bytes([header[0x15E], header[0x15F]]);
    if logo != 0xCF56 || own != crc16(0xFFFF, &header[..0x15E]) {
        return None;
    }
    Some(if header[0x12] == 3 { "DSi" } else { "DS" })
}

/// CRC-16 as the DS's BIOS reckons it, from `start`, FFFFh for a header
/// (GBATEK, BIOS Misc Functions, GetCRC16). The BIOS's table, C0C1h to
/// A001h, is what A001h taken a bit at a time from the low end gives for
/// each single bit, so this is the same sum.
fn crc16(start: u16, bytes: &[u8]) -> u16 {
    let mut crc = start;
    for &byte in bytes {
        crc ^= u16::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xA001 } else { crc >> 1 };
        }
    }
    crc
}

/// A PS2 or PS1 disc by its SYSTEM.CNF, a BOOT2 line for the PS2 and a BOOT
/// line for the PS1, as PCSX2 tells them apart (pcsx2/CDVD/CDVD.cpp,
/// GetPS2ElfName). With none, a PS1 starts PSX.EXE (DuckStation,
/// src/core/system.cpp, GetExecutableNameForImage). A PSP's UMD keeps
/// PSP_GAME/PARAM.SFO and UMD_DATA.BIN at its root (PPSSPP,
/// Core/PSPLoaders.cpp, LoadParamSFOFromDisc and InitMemorySizeForGame).
fn playstation(image: &mut (impl Read + Seek)) -> Option<&'static str> {
    if let Some(cnf) = iso9660::locate(image, &["SYSTEM.CNF"]) {
        return boot_line(&iso9660::read(image, cnf, LARGEST_SYSTEM_CNF)?);
    }
    if iso9660::locate(image, &["PSX.EXE"]).is_some_and(|exe| !exe.directory) {
        return Some("PS1");
    }
    let umd = iso9660::locate(image, &["PSP_GAME", "PARAM.SFO"]).is_some() || iso9660::locate(image, &["UMD_DATA.BIN"]).is_some();
    umd.then_some("PSP")
}

/// "PS2" for a SYSTEM.CNF with a BOOT2 line, "PS1" for one with a BOOT line.
/// A line is a key, "=" and a value, with spaces around them. DuckStation
/// matches the key whatever its case, and so does this.
fn boot_line(cnf: &[u8]) -> Option<&'static str> {
    let text = String::from_utf8_lossy(cnf);
    let keys: Vec<String> = text
        .split(['\r', '\n'])
        .filter_map(|line| line.split_once('='))
        .map(|(key, _)| key.trim().to_ascii_uppercase())
        .collect();
    if keys.iter().any(|key| key == "BOOT2") {
        Some("PS2")
    } else if keys.iter().any(|key| key == "BOOT") {
        Some("PS1")
    } else {
        None
    }
}

fn read_at(image: &mut (impl Read + Seek), offset: u64, size: usize) -> Option<Vec<u8>> {
    image.seek(SeekFrom::Start(offset)).ok()?;
    let mut bytes = vec![0; size];
    image.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

/// An Xbox or Xbox 360 disc image. An image of a whole disc is told by
/// where its game partition starts, an XISO by the program at its root.
fn xbox(image: &mut (impl Read + Seek)) -> Option<&'static str> {
    if let Some(descriptor) = xbox_descriptor(image, 0) {
        return xbox_program(image, &descriptor);
    }
    XBOX_PARTITIONS
        .iter()
        .find(|(start, _)| xbox_descriptor(image, *start).is_some())
        .map(|(_, console)| *console)
}

fn xbox_descriptor(image: &mut (impl Read + Seek), partition: u64) -> Option<Vec<u8>> {
    let descriptor = read_at(image, partition + XBOX_DESCRIPTOR, iso9660::SECTOR as usize)?;
    (descriptor.starts_with(XBOX_MEDIA) && descriptor.ends_with(XBOX_MEDIA)).then_some(descriptor)
}

/// The console an XISO is for, by the program at its root, whose place the
/// descriptor gives: the root directory's sector at 0x14 and its size at
/// 0x18 (xboxdevwiki, XDVDFS). The Xbox starts a disc's default.xbe, which
/// sits at the root (github.com/7oxicshadow/xbox_xbe_launcher, README), and
/// the 360 its default.xex (Xenia, src/xenia/emulator.cc, FindLaunchModule).
fn xbox_program(image: &mut (impl Read + Seek), descriptor: &[u8]) -> Option<&'static str> {
    let sector = u64::from(u32::from_le_bytes(descriptor.get(0x14..0x18)?.try_into().ok()?));
    let size = u64::from(u32::from_le_bytes(descriptor.get(0x18..0x1C)?.try_into().ok()?));
    if size > LARGEST_XBOX_ROOT {
        return None;
    }
    let root = read_at(image, sector * iso9660::SECTOR, usize::try_from(size).ok()?)?;
    let names = xbox_names(&root);
    let holds = |program: &str| names.iter().any(|name| name.eq_ignore_ascii_case(program));
    match (holds("default.xbe"), holds("default.xex")) {
        (true, false) => Some("Xbox"),
        (false, true) => Some("Xbox 360"),
        _ => None,
    }
}

/// The names in an Xbox directory, a binary tree whose every node gives its
/// left and right child's place in four-byte steps, 0 for none, then a
/// sector, a size and attributes, and its name's length at byte 13 and the
/// name from 14 (extract-xiso.c, traverse_xiso and XISO_FILENAME_OFFSET).
/// Every node is visited, so the tree's order needn't be trusted.
fn xbox_names(directory: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut next = vec![0];
    // A damaged tree could lead round in a circle, and no node is shorter
    // than its fourteen fixed bytes.
    let most = directory.len() / 14;
    while let Some(start) = next.pop() {
        if names.len() >= most {
            break;
        }
        let Some(node) = directory.get(start..start + 14) else {
            continue;
        };
        let Some(name) = directory.get(start + 14..start + 14 + usize::from(node[13])) else {
            continue;
        };
        names.push(String::from_utf8_lossy(name).into_owned());
        for child in [u16::from_le_bytes([node[0], node[1]]), u16::from_le_bytes([node[2], node[3]])] {
            if child != 0 {
                next.push(usize::from(child) * 4);
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::console::Console;
    use crate::core::iso9660::tests::image;
    use std::io::Cursor;

    /// `bytes` at `offset` in an otherwise empty head.
    fn placed(offset: usize, bytes: &[u8]) -> Vec<u8> {
        let mut head = vec![0; HEAD];
        head[offset..offset + bytes.len()].copy_from_slice(bytes);
        head
    }

    fn ps3_disc() -> Vec<u8> {
        image(&[("PS3_DISC.SFB", b".SFB"), ("PS3_GAME/PARAM.SFO", b"\0PSF")])
    }

    #[test]
    fn a_rar_archive_is_told_by_either_signature() {
        assert!(other_archive("game.rar", b"Rar!\x1A\x07\x00\xCF\x90\x73"));
        assert!(other_archive("game.part1.rar", b"Rar!\x1A\x07\x01\x00\x33\x92"));
        assert!(!other_archive("game.rar", b"Rar!\x1A\x07\x02\x00"));
        assert!(!other_archive("game.rar", b"Rar!"));
    }

    #[test]
    fn a_zipx_is_told_by_its_name() {
        let zip = b"PK\x03\x04\x14\x00";
        assert!(other_archive("game.zipx", zip));
        assert!(other_archive(r"D:\Games\GAME.ZIPX", zip));
        assert!(!other_archive("game.zip", zip));
    }

    #[test]
    fn a_tar_is_told_by_its_magic_at_byte_257() {
        assert!(other_archive("game.tar", &placed(257, b"ustar\x0000")));
        assert!(other_archive("game.tar", &placed(257, b"ustar  \0")), "GNU tar's older header");
        assert!(!other_archive("game.tar", &placed(256, b"ustar\x0000")));
        assert!(!other_archive("game.tar", &placed(257, b"ustar")[..261]), "cut short of the magic");
    }

    #[test]
    fn gzip_and_a_tgz_are_told_by_gzips_bytes() {
        let gzip = [0x1F, 0x8B, 0x08, 0x00];
        assert!(other_archive("game.iso.gz", &gzip));
        assert!(other_archive("game.tgz", &gzip));
        assert!(!other_archive("game.gz", &[0x1F, 0x8B, 0x07]), "a method gzip reserves");
        assert!(!other_archive("game.gz", &[0x1F]));
    }

    #[test]
    fn bzip2_is_told_by_its_header_and_first_block() {
        assert!(other_archive("game.tar.bz2", b"BZh91AY&SY\x00"));
        assert!(other_archive("empty.bz2", b"BZh9\x17\x72\x45\x38\x50\x90"));
        assert!(!other_archive("game.bz2", b"BZh01AY&SY"), "no block size 0");
        assert!(!other_archive("notes.txt", b"BZh9 is how this note starts"));
    }

    #[test]
    fn xz_is_told_by_its_header_magic() {
        assert!(other_archive("game.tar.xz", &[0xFD, b'7', b'z', b'X', b'Z', 0x00, 0x00, 0x04]));
        assert!(!other_archive("game.xz", &[0xFD, b'7', b'z', b'X', b'Z', 0x01]));
    }

    #[test]
    fn zstandard_and_lz4_are_told_by_their_frames() {
        assert!(other_archive("game.tar.zst", &[0x28, 0xB5, 0x2F, 0xFD, 0x04]));
        assert!(!other_archive("game.zst", &[0x28, 0xB5, 0x2F, 0xFC]));
        assert!(other_archive("game.lz4", &[0x04, 0x22, 0x4D, 0x18, 0x64]));
        assert!(!other_archive("game.lz4", &[0x04, 0x22, 0x4D, 0x19]));
    }

    #[test]
    fn a_cabinet_is_told_by_mscf_and_its_zero() {
        assert!(other_archive("game.cab", b"MSCF\0\0\0\0\x10\x27"));
        assert!(!other_archive("game.cab", b"MSCF\x01\0\0\0"), "its reserved field is zero");
    }

    #[test]
    fn a_zip_or_a_7z_is_never_another_archive() {
        let seven = [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x04];
        assert!(!other_archive("game.zip", b"PK\x03\x04"));
        assert!(!other_archive("game.7z", &seven));
        // As `detect_kind` does, the bytes are believed over the name.
        assert!(!other_archive("game.rar", b"PK\x03\x04"));
        assert!(!other_archive("game.tgz", &seven));
        let mut zip = placed(257, b"ustar");
        zip[..4].copy_from_slice(b"PK\x03\x04");
        assert!(!other_archive("game.zip", &zip), "a zip, whatever lies at byte 257");
    }

    #[test]
    fn a_disc_image_or_a_game_file_is_never_another_archive() {
        assert!(!other_archive("game.iso", &ps3_disc()[..HEAD]));
        assert!(!other_archive("game.wua", &[0x28, 0xB5, 0x2F, 0xFD, 0x24, 0x00]), "a .wua's first zstd block");
        assert!(!other_archive("game.pkg", b"\x7FPKG\x80\x00\x00\x01"));
        assert!(!other_archive("game.nsp", b"PFS0\x04\x00\x00\x00"));
        assert!(!other_archive("game.wux", b"WUX0"));
        assert!(!other_archive("game.rvz", b"RVZ\x01"));
        assert!(!other_archive("game.iso", &[]));
    }

    #[test]
    fn a_switch_package_card_or_program_is_told() {
        assert_eq!(other_console("game.nsp", b"PFS0\x04\x00\x00\x00"), Some("Switch"));
        assert_eq!(other_console("game.nsz", b"PFS0\x05\x00\x00\x00"), Some("Switch"));
        assert_eq!(other_console("game.nsp", b"PFS1"), None, "the name alone isn't enough");
        let mut card = placed(0x100, b"HEAD");
        card[0x108..0x10C].fill(0xFF);
        assert_eq!(other_console("game.xci", &card), Some("Switch"));
        let mut keyed = placed(KEY_AREA + 0x100, b"HEAD");
        keyed[KEY_AREA + 0x108..KEY_AREA + 0x10C].fill(0xFF);
        assert_eq!(other_console("game.xci", &keyed), Some("Switch"), "with the card's key area first");
        assert_eq!(other_console("game.xci", &placed(0x100, b"HEAD")), None, "no backup area address");
        assert_eq!(other_console("hbmenu.nro", &placed(0x10, b"NRO0")), Some("Switch"));
    }

    #[test]
    fn a_3ds_card_program_or_cia_is_told() {
        assert_eq!(other_console("game.3ds", &placed(0x100, b"NCSD")), Some("3DS"));
        assert_eq!(other_console("game.cci", &placed(0x100, b"NCSD")), Some("3DS"));
        assert_eq!(other_console("game.cxi", &placed(0x100, b"NCCH")), Some("3DS"));
        assert_eq!(other_console("game.3dsx", b"3DSX\x2C\x00"), Some("3DS"));
        assert_eq!(other_console("game.cia", &[0x20, 0x20, 0, 0]), Some("3DS"));
        assert_eq!(other_console("model.3ds", &placed(0x100, b"NCSE")), None, "a .3ds isn't taken on its name");
    }

    /// A DS cartridge's header for `unit`, with both its checksums.
    fn ds_header(unit: u8) -> Vec<u8> {
        let mut header = vec![0u8; 0x200];
        header[..10].copy_from_slice(b"OMOIO TEST");
        header[0x12] = unit;
        header[0x15C..0x15E].copy_from_slice(&0xCF56u16.to_le_bytes());
        let own = crc16(0xFFFF, &header[..0x15E]);
        header[0x15E..0x160].copy_from_slice(&own.to_le_bytes());
        header
    }

    #[test]
    fn a_ds_cartridge_is_told_by_its_header_checksums() {
        assert_eq!(other_console("game.nds", &ds_header(0)), Some("DS"));
        assert_eq!(other_console("game.nds", &ds_header(2)), Some("DS"), "a DS game the DSi does more with");
        assert_eq!(other_console("game.nds", &ds_header(3)), Some("DSi"));
        let mut broken = ds_header(0);
        broken[0] ^= 1;
        assert_eq!(other_console("game.nds", &broken), None, "the header's own checksum no longer matches");
        assert_eq!(other_console("game.nds", &ds_header(0)[..0x15F]), None, "cut short");
    }

    #[test]
    fn the_crc_is_the_one_the_ds_bios_tabulates() {
        let table = [0xC0C1, 0xC181, 0xC301, 0xC601, 0xCC01, 0xD801, 0xF001, 0xA001];
        for (bit, value) in table.into_iter().enumerate() {
            assert_eq!(crc16(0, &[1 << bit]), value, "bit {bit}");
        }
    }

    /// A GBA cartridge's header with `fixed` at 0xB2 and its complement.
    fn gba_header(fixed: u8) -> Vec<u8> {
        let mut header = vec![0u8; 0xC0];
        header[..4].copy_from_slice(&[0x2E, 0x00, 0x00, 0xEA]);
        header[0xA0..0xAA].copy_from_slice(b"OMOIO TEST");
        header[0xB2] = fixed;
        header[0xBD] = header[0xA0..=0xBC]
            .iter()
            .fold(0u8, |sum, &byte| sum.wrapping_sub(byte))
            .wrapping_sub(0x19);
        header
    }

    #[test]
    fn a_gba_cartridge_is_told_by_its_header() {
        assert_eq!(other_console("game.gba", &gba_header(0x96)), Some("Game Boy Advance"));
        assert_eq!(other_console("game.bin", &gba_header(0x96)), Some("Game Boy Advance"), "by its bytes");
        assert_eq!(other_console("game.gba", &gba_header(0x95)), None, "not the fixed byte");
        let mut wrong = gba_header(0x96);
        wrong[0xBD] ^= 0xFF;
        assert_eq!(other_console("game.gba", &wrong), None, "not the complement");
        let mut unbranched = gba_header(0x96);
        unbranched[3] = 0;
        assert_eq!(other_console("game.gba", &unbranched), None, "no branch to start from");
    }

    #[test]
    fn game_boy_and_snes_cartridges_are_told_by_their_names() {
        assert_eq!(other_console("Game.gb", &[0; 0x150]), Some("Game Boy"));
        assert_eq!(other_console("Game.GBC", &[0; 0x150]), Some("Game Boy Color"));
        assert_eq!(other_console("Game.sfc", &[0; 0x200]), Some("SNES"));
        assert_eq!(other_console("Game.smc", &[]), Some("SNES"));
        assert_eq!(other_console("Disk.dmg", &[0; 0x200]), None, "a Mac disk image is named so too");
        assert_eq!(other_console("game.gb.txt", b"notes"), None);
    }

    #[test]
    fn an_n64_image_is_told_in_all_three_byte_orders() {
        assert_eq!(other_console("game.z64", &[0x80, 0x37, 0x12, 0x40, 0x00]), Some("N64"));
        assert_eq!(other_console("game.v64", &[0x37, 0x80, 0x40, 0x12, 0x00]), Some("N64"));
        assert_eq!(other_console("game.n64", &[0x40, 0x12, 0x37, 0x80, 0x00]), Some("N64"));
        assert_eq!(other_console("game.z64", &[0x80, 0x37, 0x12, 0x41]), None);
    }

    #[test]
    fn an_nes_image_is_told_by_its_ines_header() {
        assert_eq!(other_console("game.nes", b"NES\x1A\x02\x01"), Some("NES"));
        assert_eq!(other_console("game.nes", b"NES\x1B"), None);
    }

    /// A PS4 package's first bytes, whose content ID has a title ID starting
    /// `title`.
    fn package(title: &[u8; 4]) -> Vec<u8> {
        let mut head = placed(0, b"\x7FCNT");
        let id = [b"UP0000-".as_slice(), title, b"00000_00-OMOIOTEST0000000"].concat();
        head[0x40..0x40 + id.len()].copy_from_slice(&id);
        head
    }

    #[test]
    fn playstation_programs_and_packages_are_told() {
        assert_eq!(other_console("SLUS_000.01", b"PS-X EXE\0\0\0\0"), Some("PS1"));
        assert_eq!(other_console("EBOOT.PBP", b"\0PBP\x00\x00\x01\x00"), Some("PSP"));
        assert_eq!(other_console("game.vpk", b"PK\x03\x04\x14\x00"), Some("PS Vita"));
        assert_eq!(other_console("pak01_dir.vpk", &[0x34, 0x12, 0xAA, 0x55]), None, "Valve's own .vpk");
        assert_eq!(other_console("game.pkg", &package(b"CUSA")), Some("PS4"));
        assert_eq!(other_console("game.pkg", &package(b"PPSA")), None, "a PS5 package starts the same");
        assert_eq!(other_console("game.pkg", b"\x7FCNT"), None, "cut short");
    }

    #[test]
    fn a_ps3_package_is_left_unnamed() {
        assert_eq!(other_console("game.pkg", b"\x7FPKG\x80\x00\x00\x01"), None);
        assert_eq!(other_console("game.pkg", b"\x7FPKG\x80\x00\x00\x02"), None, "a PSP's or PS Vita's");
    }

    #[test]
    fn xbox_programs_are_told_by_their_magic() {
        assert_eq!(other_console("default.xbe", b"XBEH\x00\x00"), Some("Xbox"));
        assert_eq!(other_console("default.xex", b"XEX2\x00\x00"), Some("Xbox 360"));
        assert_eq!(other_console("default.xex", b"XEX1"), None);
    }

    #[test]
    fn a_disc_image_is_never_told_by_its_name() {
        for name in ["game.iso", "game.bin", "game.cue", "GAME.ISO"] {
            assert_eq!(other_console(name, &[0; HEAD]), None, "{name}");
        }
        assert_eq!(other_console("game.cue", b"FILE \"game.bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n"), None);
        assert_eq!(other_console("game.iso", &ps3_disc()[..HEAD]), None);
    }

    #[test]
    fn the_files_of_omoios_own_consoles_get_no_name() {
        // A GameCube disc's and a Wii disc's headers: the game's ID, then
        // the console's magic word (backends/dolphin/disc.rs).
        let mut gamecube = placed(0, b"GALE01");
        gamecube[0x1C..0x20].copy_from_slice(&0xC233_9F3Du32.to_be_bytes());
        let mut wii = placed(0, b"SSPP52");
        wii[0x18..0x1C].copy_from_slice(&0x5D1C_9EA3u32.to_be_bytes());
        let files: [(&str, &[u8]); 8] = [
            ("game.iso", &gamecube),
            ("game.iso", &wii),
            ("game.iso", &ps3_disc()[..HEAD]),
            ("game.pkg", b"\x7FPKG\x80\x00\x00\x01"),
            ("game.wua", &[0x28, 0xB5, 0x2F, 0xFD]),
            ("game.wux", b"WUX0"),
            ("EBOOT.BIN", b"SCE\0"),
            ("game.rpx", b"\x7FELF"),
        ];
        for (name, head) in files {
            assert_eq!(other_console(name, head), None, "{name}");
        }
        assert_eq!(console_in_image(&mut Cursor::new(gamecube)), None);
        assert_eq!(console_in_image(&mut Cursor::new(wii)), None);
    }

    #[test]
    fn a_ps2_disc_is_told_by_its_boot2_line() {
        let cnf = b"BOOT2 = cdrom0:\\SLES_123.45;1\r\nVER = 1.00\r\nVMODE = PAL\r\n";
        let mut disc = Cursor::new(image(&[("SYSTEM.CNF", cnf), ("SLES_123.45", b"\x7FELF")]));
        assert_eq!(console_in_image(&mut disc), Some("PS2"));
        let tight = image(&[("SYSTEM.CNF", b"boot2=cdrom0:\\GAME.ELF;1\n"), ("PSX.EXE", b"PS-X EXE")]);
        assert_eq!(console_in_image(&mut Cursor::new(tight)), Some("PS2"), "its SYSTEM.CNF says what starts");
    }

    #[test]
    fn a_ps1_disc_is_told_by_its_boot_line_or_its_psx_exe() {
        let cnf = b"BOOT = cdrom:\\SLUS_000.01;1\r\nTCB = 4\r\nEVENT = 10\r\nSTACK = 801FFF00\r\n";
        assert_eq!(console_in_image(&mut Cursor::new(image(&[("SYSTEM.CNF", cnf)]))), Some("PS1"));
        assert_eq!(console_in_image(&mut Cursor::new(image(&[("PSX.EXE", b"PS-X EXE")]))), Some("PS1"));
        let silent = image(&[("SYSTEM.CNF", b"VMODE = NTSC\r\n"), ("PSX.EXE", b"PS-X EXE")]);
        assert_eq!(console_in_image(&mut Cursor::new(silent)), None, "a SYSTEM.CNF with no boot line");
    }

    #[test]
    fn a_large_system_cnf_is_not_read() {
        let mut disc = image(&[("SYSTEM.CNF", b"BOOT2 = cdrom0:\\GAME.ELF;1")]);
        // Its directory record's size, at byte 10 of a record whose name
        // starts at byte 33 (ECMA-119), made larger than is read.
        let name = disc.windows(12).position(|bytes| bytes == b"SYSTEM.CNF;1").unwrap();
        let size_at = name - 33 + 10;
        disc[size_at..size_at + 4].copy_from_slice(&(LARGEST_SYSTEM_CNF as u32 + 1).to_le_bytes());
        assert_eq!(console_in_image(&mut Cursor::new(disc)), None);
    }

    #[test]
    fn a_psp_disc_is_told_by_its_game_folder_or_umd_data() {
        assert_eq!(console_in_image(&mut Cursor::new(image(&[("PSP_GAME/PARAM.SFO", b"\0PSF")]))), Some("PSP"));
        assert_eq!(console_in_image(&mut Cursor::new(image(&[("UMD_DATA.BIN", b"ULES00000")]))), Some("PSP"));
        assert_eq!(console_in_image(&mut Cursor::new(image(&[("PSP_GAME/ICON0.PNG", b"\x89PNG")]))), None);
    }

    #[test]
    fn a_ps3_disc_is_never_named() {
        assert_eq!(console_in_image(&mut Cursor::new(ps3_disc())), None);
        let sfb = image(&[("PS3_DISC.SFB", b".SFB"), ("SYSTEM.CNF", b"BOOT2 = cdrom0:\\GAME.ELF;1")]);
        assert_eq!(console_in_image(&mut Cursor::new(sfb)), None);
        let sfo = image(&[("PS3_GAME/PARAM.SFO", b"\0PSF"), ("UMD_DATA.BIN", b"ULES00000")]);
        assert_eq!(console_in_image(&mut Cursor::new(sfo)), None);
    }

    #[test]
    fn an_image_neither_file_system_tells_is_none() {
        assert_eq!(console_in_image(&mut Cursor::new(vec![0u8; 0x20000])), None);
        assert_eq!(console_in_image(&mut Cursor::new(Vec::new())), None);
        assert_eq!(console_in_image(&mut Cursor::new(image(&[("README.TXT", b"notes")]))), None);
    }

    /// An Xbox file system's descriptor, giving its root directory's place.
    fn xbox_descriptor_at(root_sector: u32, root_size: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; iso9660::SECTOR as usize];
        bytes[..20].copy_from_slice(XBOX_MEDIA);
        bytes[0x14..0x18].copy_from_slice(&root_sector.to_le_bytes());
        bytes[0x18..0x1C].copy_from_slice(&root_size.to_le_bytes());
        bytes[0x7EC..].copy_from_slice(XBOX_MEDIA);
        bytes
    }

    /// An Xbox directory of one sector holding `names`, each node the right
    /// child of the one before, the rest of the sector padded with FFh.
    fn xbox_directory(names: &[&str]) -> Vec<u8> {
        let mut table = Vec::new();
        for (index, name) in names.iter().enumerate() {
            let start = table.len();
            let length = (14 + name.len() + 3) & !3;
            let right = if index + 1 < names.len() { (start + length) / 4 } else { 0 };
            table.extend_from_slice(&0u16.to_le_bytes());
            table.extend_from_slice(&(right as u16).to_le_bytes());
            table.extend_from_slice(&[0; 8]);
            table.push(0);
            table.push(name.len() as u8);
            table.extend_from_slice(name.as_bytes());
            table.resize(start + length, 0xFF);
        }
        table.resize(iso9660::SECTOR as usize, 0xFF);
        table
    }

    /// An XISO, the game partition alone, with `names` at its root.
    fn xiso(names: &[&str]) -> Vec<u8> {
        let mut disc = vec![0u8; XBOX_DESCRIPTOR as usize];
        disc.extend(xbox_descriptor_at(33, iso9660::SECTOR as u32));
        disc.extend(xbox_directory(names));
        disc
    }

    #[test]
    fn an_xiso_is_told_by_the_program_at_its_root() {
        assert_eq!(console_in_image(&mut Cursor::new(xiso(&["default.xbe", "media"]))), Some("Xbox"));
        assert_eq!(console_in_image(&mut Cursor::new(xiso(&["media", "default.xex"]))), Some("Xbox 360"));
        assert_eq!(console_in_image(&mut Cursor::new(xiso(&["DEFAULT.XBE"]))), Some("Xbox"), "whatever its case");
        assert_eq!(console_in_image(&mut Cursor::new(xiso(&["readme.txt"]))), None);
        let mut half = xiso(&["default.xbe"]);
        half[XBOX_DESCRIPTOR as usize + 0x7EC] = 0;
        assert_eq!(console_in_image(&mut Cursor::new(half)), None, "a descriptor must end as it starts");
    }

    #[test]
    fn a_looping_xbox_directory_still_ends() {
        let mut table = xbox_directory(&["a", "b"]);
        // The second node, 16 bytes in, made its own left child.
        table[16..18].copy_from_slice(&4u16.to_le_bytes());
        let names = xbox_names(&table);
        assert!(names.iter().any(|name| name == "a") && names.iter().any(|name| name == "b"));
    }

    /// A disc image of several gigabytes that is empty but for a few
    /// pieces, so a whole disc's layout can be tried without the memory.
    struct Sparse {
        size: u64,
        pieces: Vec<(u64, Vec<u8>)>,
        at: u64,
    }

    impl Read for Sparse {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let left = usize::try_from(self.size.saturating_sub(self.at)).unwrap_or(usize::MAX);
            let count = buf.len().min(left);
            let wanted = &mut buf[..count];
            wanted.fill(0);
            for (start, bytes) in &self.pieces {
                let from = self.at.max(*start);
                let to = (self.at + count as u64).min(start + bytes.len() as u64);
                if from < to {
                    wanted[(from - self.at) as usize..(to - self.at) as usize]
                        .copy_from_slice(&bytes[(from - start) as usize..(to - start) as usize]);
                }
            }
            self.at += count as u64;
            Ok(count)
        }
    }

    impl Seek for Sparse {
        fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
            self.at = match to {
                SeekFrom::Start(at) => at,
                SeekFrom::End(by) => self.size.saturating_add_signed(by),
                SeekFrom::Current(by) => self.at.saturating_add_signed(by),
            };
            Ok(self.at)
        }
    }

    #[test]
    fn a_whole_xbox_disc_is_told_by_where_its_game_partition_starts() {
        for (start, console) in XBOX_PARTITIONS {
            let descriptor = start + XBOX_DESCRIPTOR;
            let mut disc = Sparse {
                size: descriptor + 0x10000,
                pieces: vec![(descriptor, xbox_descriptor_at(33, iso9660::SECTOR as u32))],
                at: 0,
            };
            assert_eq!(console_in_image(&mut disc), Some(console), "{start:#x}");
        }
        let mut elsewhere = Sparse {
            size: 0x2000_0000,
            pieces: vec![(0x0300_0000 + XBOX_DESCRIPTOR, xbox_descriptor_at(33, iso9660::SECTOR as u32))],
            at: 0,
        };
        assert_eq!(console_in_image(&mut elsewhere), None, "not where any disc has its game partition");
    }

    #[test]
    fn no_console_omoio_runs_is_ever_named() {
        let omoio: Vec<&str> = Console::ALL.iter().map(|console| console.short()).collect();
        let mut card = placed(0x100, b"HEAD");
        card[0x108..0x10C].fill(0xFF);
        let named = [
            other_console("game.nsp", b"PFS0"),
            other_console("game.xci", &card),
            other_console("game.3ds", &placed(0x100, b"NCSD")),
            other_console("game.cia", &[]),
            other_console("game.nds", &ds_header(0)),
            other_console("game.nds", &ds_header(3)),
            other_console("game.gba", &gba_header(0x96)),
            other_console("game.gb", &[]),
            other_console("game.gbc", &[]),
            other_console("game.z64", &[0x80, 0x37, 0x12, 0x40]),
            other_console("game.sfc", &[]),
            other_console("game.nes", b"NES\x1A"),
            other_console("SLUS_000.01", b"PS-X EXE"),
            other_console("EBOOT.PBP", b"\0PBP"),
            other_console("game.vpk", b"PK\x03\x04"),
            other_console("game.pkg", &package(b"CUSA")),
            other_console("default.xbe", b"XBEH"),
            other_console("default.xex", b"XEX2"),
            console_in_image(&mut Cursor::new(image(&[("SYSTEM.CNF", b"BOOT2 = cdrom0:\\GAME.ELF;1")]))),
            console_in_image(&mut Cursor::new(image(&[("PSX.EXE", b"PS-X EXE")]))),
            console_in_image(&mut Cursor::new(image(&[("UMD_DATA.BIN", b"ULES00000")]))),
            console_in_image(&mut Cursor::new(xiso(&["default.xbe"]))),
            console_in_image(&mut Cursor::new(xiso(&["default.xex"]))),
        ];
        assert!(named.iter().all(Option::is_some), "every case is told");
        for console in named.into_iter().flatten() {
            assert!(!omoio.contains(&console), "{console}");
        }
    }
}
