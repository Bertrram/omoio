//! GameCube and Wii disc images as Dolphin reads them: which game a disc
//! holds, and a GameCube game's banner.
//!
//! A Wii game lies in encrypted partitions, and nothing here reads them or
//! holds a key. A Wii disc is known by its first 0x440 bytes, which are never
//! encrypted, and by the headers of the file it is kept in. A GameCube disc
//! has no encryption at all, so its file table is read too, for the banner,
//! except from a WIA or RVZ image, which is read no further than its headers
//! (see `Wia`).
//!
//! Every layout here was read out of Dolphin's source at tag 2609a on 8
//! October 2026, and each names the file and function it comes from. The
//! images come from dumps we did not make, so no size or offset in them is
//! trusted beyond the file.

use crate::core::tga::{self, Picture};
use sha1::{Digest, Sha1};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const NOT_A_DISC: &str = "This file doesn't look like a GameCube or Wii disc image.";
const NOT_A_DISC_FOLDER: &str = "This folder doesn't look like a GameCube or Wii disc.";
const DAMAGED: &str =
    "This disc image looks damaged or incomplete, so it can't be read. Copying the disc again may help.";
const UNOPENED: &str = "Couldn't open this file. Check that it's still there and try again.";
const CHANNEL: &str = "This is a Wii channel (.wad), not a disc. Omoio plays GameCube and Wii discs.";

/// The plain header at the start of every disc, which is also as far as a
/// Wii disc is ever read. Dolphin 2609a, DiscIO/DiscUtils.h, DISCHEADER_SIZE.
const HEADER_SIZE: usize = 0x440;
/// The title is read from 0x20 for 0x60 bytes, up to its first zero byte.
/// Dolphin 2609a, DiscIO/VolumeDisc.cpp, VolumeDisc::GetInternalName.
const TITLE_AT: usize = 0x20;
const TITLE_END: usize = 0x80;
/// Dolphin tells the two consoles' discs apart by these words, the Wii's
/// first. Dolphin 2609a, DiscIO/Volume.cpp, TryCreateDisc, and
/// DiscIO/DiscUtils.h.
const WII_MAGIC_AT: usize = 0x18;
const WII_MAGIC: u32 = 0x5D1C_9EA3;
const GAMECUBE_MAGIC_AT: usize = 0x1C;
const GAMECUBE_MAGIC: u32 = 0xC233_9F3D;
/// Where a GameCube disc's header says its file table is and how long it is.
/// Dolphin 2609a, DiscIO/DiscUtils.cpp, GetFSTOffset and GetFSTSize.
const FST_OFFSET_AT: usize = 0x424;
const FST_SIZE_AT: usize = 0x428;
/// Dolphin refuses a larger file table, so a damaged one can't have it set
/// aside more memory than a Wii has. Dolphin 2609a,
/// DiscIO/FileSystemGCWii.cpp, FileSystemGCWii::FileSystemGCWii.
const MOST_FST: u64 = 128 * 1024 * 1024;
/// A GameCube disc's region, in bi2.bin at 0x18, which sits at 0x440 on the
/// disc. 0 is Japan. Dolphin 2609a, DiscIO/VolumeGC.cpp, VolumeGC::GetRegion,
/// and DiscIO/Enums.h, Region.
const REGION_AT: u64 = 0x458;
const REGION_IN_BI2: usize = 0x18;
const REGION_JAPAN: u32 = 0;
/// A GameCube banner is 96 by 32 pixels of RGB5A3 from 0x20 in
/// opening.bnr. Dolphin only takes the file at exactly the size of its kind:
/// BNR1 has one set of names, BNR2 six. Dolphin 2609a, DiscIO/VolumeGC.h,
/// GCBanner, and DiscIO/VolumeGC.cpp, VolumeGC::LoadBannerFile.
const BANNER_FILE: &[u8] = b"opening.bnr";
const BANNER_AT: usize = 0x20;
const BANNER_WIDTH: usize = 96;
const BANNER_HEIGHT: usize = 32;
const BNR1_SIZE: usize = 0x1960;
const BNR2_SIZE: usize = 0x1FA0;
/// A Wii save's banner.bin: a 0xA0-byte header, a 192 by 64 RGB5A3 banner,
/// then at least one 48 by 48 icon, which Dolphin requires before it reads
/// the banner. Dolphin 2609a, DiscIO/WiiSaveBanner.h and WiiSaveBanner.cpp,
/// WiiSaveBanner::WiiSaveBanner and GetBanner.
const SAVE_HEADER: usize = 0xA0;
const SAVE_BANNER_WIDTH: usize = 192;
const SAVE_BANNER_HEIGHT: usize = 64;
const SAVE_ICON_SIZE: usize = 48 * 48 * 2;

/// Which console a disc is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    GameCube,
    Wii,
}

/// What a disc's plain header says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disc {
    pub platform: Platform,
    /// The six characters at the start, such as `SSPP52`. Anything but a
    /// letter or digit becomes `-`, as Dolphin shows it, so the id is safe in
    /// a file name. Dolphin 2609a, DiscIO/Volume.cpp, Volume::FilterGameID.
    pub game_id: String,
    /// Counted from 0, so the second disc of a set is 1.
    pub disc_number: u8,
    pub revision: u8,
    pub title: String,
    /// How much data the disc holds, for working out room: the size the
    /// image records for the disc where it keeps one (WIA, RVZ, GCZ), how far
    /// its stored blocks reach where it keeps only those (WBFS, CISO), and
    /// the image less its own header otherwise. 0 for an extracted folder,
    /// which isn't measured here: its files are read where they are, so it
    /// needs no room, and measuring means walking every file in it.
    pub data_size: u64,
}

/// The disc images among the files Dolphin's game list looks for (Dolphin
/// 2609a, UICommon/GameFileCache.cpp, FindAllGamePaths), less two: .bin,
/// which many other files are named, a Wii save's own banner.bin among them,
/// and .nfs, a Wii game bought on the Wii U, which only the Wii U's keys
/// open. A .bin that is a disc is still read when picked by hand.
const DISC_EXTENSIONS: [&str; 8] = ["iso", "gcm", "tgc", "ciso", "gcz", "wbfs", "wia", "rvz"];

fn extension(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
}

/// Whether a file is a GameCube or Wii disc image, by its name alone.
pub fn is_disc_name(name: &str) -> bool {
    extension(name).is_some_and(|extension| DISC_EXTENSIONS.contains(&extension.as_str()))
        && !is_later_part(name)
}

/// Whether a file is the second or a later part of an image split into
/// `name.part0.iso`, `name.part1.iso` and on, which is read along with the
/// first and is no game of its own. Matched as exactly as Dolphin names the
/// parts. Dolphin 2609a, DiscIO/SplitFileBlob.cpp,
/// SplitPlainFileReader::Create.
fn is_later_part(name: &str) -> bool {
    let number = name.strip_suffix(".iso").and_then(|stem| stem.rsplit_once(".part"));
    number.is_some_and(|(_, number)| {
        !number.is_empty() && !number.starts_with('0') && number.bytes().all(|digit| digit.is_ascii_digit())
    })
}

/// Whether a file is a Wii channel, which Dolphin installs rather than plays
/// from where it is.
pub fn is_channel_name(name: &str) -> bool {
    extension(name).as_deref() == Some("wad")
}

/// Reads which game a disc image holds, from its plain headers only. A
/// folder is read as an extracted disc.
pub fn read(path: &Path) -> Result<Disc, String> {
    if path.is_dir() {
        return read_folder(path).ok_or_else(|| NOT_A_DISC_FOLDER.to_string());
    }
    if is_channel_name(&path.to_string_lossy()) {
        return Err(CHANNEL.to_string());
    }
    let mut image = Image::open(path).map_err(str::to_string)?;
    let header = read_header(&mut image).map_err(str::to_string)?;
    let platform = platform_of(&header).ok_or_else(|| NOT_A_DISC.to_string())?;
    // A Wii disc's region record lies past its header, so it is left unread.
    let region = match platform {
        Platform::GameCube => image.read_vec(REGION_AT, 4).map(|code| be32(&code, 0)),
        Platform::Wii => None,
    };
    Ok(disc_from(&header, platform, region, image.data_size()))
}

/// Reads an extracted disc, laid out as Dolphin's DirectoryBlob takes one
/// and DolphinTool's `extract` writes one: `sys/boot.bin`, `sys/main.dol`
/// and `files/` in the folder itself, or, for a whole Wii disc, in its DATA
/// folder beside the others. `None` when the folder isn't one. Only the
/// few files that hold the header are read; the folder isn't measured.
pub fn read_folder(dir: &Path) -> Option<Disc> {
    let root = partition_root(dir)?;
    let header = folder_header(&root)?;
    let platform = platform_of(&header)?;
    let region = match platform {
        Platform::GameCube => Some(folder_region(&root)),
        Platform::Wii => None,
    };
    Some(disc_from(&header, platform, region, 0))
}

/// A GameCube game's banner, as the bytes of a PNG file, from an image or an
/// extracted folder. `None` for a Wii disc, whose banner is inside its
/// encrypted partition, and for a GameCube game kept as WIA or RVZ, whose
/// banner is inside compressed data Omoio doesn't read (see `Wia`); the
/// library falls back to its own art or a RAWG cover.
pub fn gamecube_banner_png(path: &Path) -> Option<Vec<u8>> {
    let file = if path.is_dir() { banner_in_folder(path)? } else { banner_in_image(path)? };
    let picture = banner_picture(&file)?;
    tga::to_png(&picture)
}

/// A Wii save's banner, as the bytes of a PNG file. Dolphin keeps each save
/// in its NAND folder (see `nand_title_folder`) under `data/banner.bin`,
/// written there by the game, not encrypted.
pub fn save_banner_png(banner_bin: &Path) -> Option<Vec<u8>> {
    let mut file = File::open(banner_bin).ok()?;
    let banner_size = SAVE_BANNER_WIDTH * SAVE_BANNER_HEIGHT * 2;
    let length = file.metadata().ok()?.len();
    if length < (SAVE_HEADER + banner_size + SAVE_ICON_SIZE) as u64 {
        return None;
    }
    let pixels = read_file_at(&mut file, SAVE_HEADER as u64, banner_size)?;
    tga::to_png(&decode_rgb5a3(&pixels, SAVE_BANNER_WIDTH, SAVE_BANNER_HEIGHT)?)
}

/// Where a Wii disc game's data sits in Dolphin's NAND, under
/// `User/Wii/title/`: the title's upper half, then its lower half, each as
/// eight lower-case hex digits. A disc game's upper half is 00010000 and its
/// lower half is the first four characters of its game id. Dolphin 2609a,
/// Common/NandPaths.cpp, GetTitlePath; Core/IOS/ES/Formats.h, TitleType::Game;
/// Core/IOS/ES/Formats.cpp, TMDReader::GetGameID.
///
/// The few disc games that also install a channel, such as Wii Fit, use
/// 00010004 instead (TitleType::GameWithChannel), which only the encrypted
/// partition's ticket says, so a caller may try that folder too.
pub fn nand_title_folder(game_id: &str) -> Option<String> {
    let code = game_id.get(..4)?;
    if !code.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return None;
    }
    let hex: String = code.bytes().map(|byte| format!("{byte:02x}")).collect();
    Some(format!("00010000/{hex}"))
}

fn platform_of(header: &[u8]) -> Option<Platform> {
    if header.len() < GAMECUBE_MAGIC_AT + 4 {
        return None;
    }
    if be32(header, WII_MAGIC_AT) == WII_MAGIC {
        Some(Platform::Wii)
    } else if be32(header, GAMECUBE_MAGIC_AT) == GAMECUBE_MAGIC {
        Some(Platform::GameCube)
    } else {
        None
    }
}

/// The disc's header: all 0x440 bytes where the image gives them, else the
/// first 0x80, which hold everything `Disc` needs. A WIA or RVZ image keeps
/// only those 0x80 outside its compressed data.
fn read_header(image: &mut Image) -> Result<Vec<u8>, &'static str> {
    for length in [HEADER_SIZE, TITLE_END] {
        if let Some(header) = image.read_vec(0, length) {
            return Ok(header);
        }
    }
    let start = image.read_vec(0, GAMECUBE_MAGIC_AT + 4);
    if start.is_some_and(|start| platform_of(&start).is_some()) {
        Err(DAMAGED)
    } else {
        Err(NOT_A_DISC)
    }
}

/// Dolphin's game list reads the id, disc number, revision and title from
/// the plain header, never from inside a Wii partition. Dolphin 2609a,
/// UICommon/GameFile.cpp, GameFile::GameFile, and DiscIO/VolumeDisc.cpp.
fn disc_from(header: &[u8], platform: Platform, region: Option<u32>, data_size: u64) -> Disc {
    let game_id = header[..6]
        .iter()
        .map(|&byte| if byte.is_ascii_alphanumeric() { char::from(byte) } else { '-' })
        .collect();
    Disc {
        platform,
        game_id,
        disc_number: header[6],
        revision: header[7],
        title: decode_title(&header[TITLE_AT..TITLE_END], is_japanese(platform, region, header[3])),
        data_size,
    }
}

/// Whether Dolphin takes the disc as one from Japan or nearby, whose titles
/// are written in Shift-JIS rather than Windows-1252 (Dolphin 2609a,
/// DiscIO/Volume.cpp, Volume::DecodeString). A GameCube disc's bi2.bin says.
/// Where that can't be read, and for a Wii disc, whose own region record
/// lies past its header, the fourth character of the id is read as Dolphin's
/// DiscIO/Enums.cpp, CountryCodeToRegion, reads it: J and W are Japan or
/// Taiwan, and on a GameCube the Korean K, Q and T are counted with them.
fn is_japanese(platform: Platform, region: Option<u32>, country: u8) -> bool {
    match region {
        Some(code) => code == REGION_JAPAN,
        None => match country {
            b'J' | b'W' => true,
            b'K' | b'Q' | b'T' => platform == Platform::GameCube,
            _ => false,
        },
    }
}

fn decode_title(bytes: &[u8], japanese: bool) -> String {
    let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(bytes.len());
    let encoding = if japanese { encoding_rs::SHIFT_JIS } else { encoding_rs::WINDOWS_1252 };
    let (text, _) = encoding.decode_without_bom_handling(&bytes[..end]);
    text.trim().to_string()
}

fn be16(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([bytes[at], bytes[at + 1]])
}

fn be32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn be64(bytes: &[u8], at: usize) -> u64 {
    u64::from_be_bytes(bytes[at..at + 8].try_into().unwrap())
}

fn le32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn le64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

fn read_file_into(file: &mut File, offset: u64, out: &mut [u8]) -> Option<()> {
    // `?` on a Result or Option hands back failure at once, like an early
    // return; `.ok()` turns an error into a plain `None` first.
    file.seek(SeekFrom::Start(offset)).ok()?;
    file.read_exact(out).ok()
}

fn read_file_at(file: &mut File, offset: u64, length: usize) -> Option<Vec<u8>> {
    let mut bytes = vec![0; length];
    read_file_into(file, offset, &mut bytes)?;
    Some(bytes)
}

/// The disc inside one of the image files Dolphin reads. Dolphin picks the
/// kind by the first four bytes and takes anything else as a plain image.
/// Dolphin 2609a, DiscIO/Blob.cpp, CreateBlobReader. Each case carries its
/// own reader, like a tagged union, and `match` picks the case.
enum Image {
    Plain(Parts),
    Ciso(Ciso),
    Gcz(Gcz),
    Tgc(Tgc),
    Wbfs(Wbfs),
    Wia(Wia),
}

impl Image {
    fn open(path: &Path) -> Result<Image, &'static str> {
        let mut file = File::open(path).map_err(|_| UNOPENED)?;
        let size = file.metadata().map_err(|_| UNOPENED)?.len();
        let mut magic = [0; 4];
        if read_file_into(&mut file, 0, &mut magic).is_none() {
            return Err(NOT_A_DISC);
        }
        let image = match &magic {
            b"CISO" => Ciso::open(file).map(Image::Ciso),
            [0x01, 0xC0, 0x0B, 0xB1] => Gcz::open(file, size).map(Image::Gcz),
            [0xAE, 0x0F, 0x38, 0xA2] => Tgc::open(file, size).map(Image::Tgc),
            b"WBFS" => Wbfs::open(wbfs_parts(file, size, path)).map(Image::Wbfs),
            b"WIA\x01" => Wia::open(file, size, false).map(Image::Wia),
            b"RVZ\x01" => Wia::open(file, size, true).map(Image::Wia),
            _ => Some(Image::Plain(split_iso_parts(file, size, path))),
        };
        image.ok_or(DAMAGED)
    }

    /// How far into the disc the image can be read.
    fn limit(&self) -> u64 {
        match self {
            Image::Plain(parts) => parts.len(),
            Image::Ciso(ciso) => ciso.stored.len() as u64 * ciso.block_size,
            Image::Gcz(gcz) => gcz.disc_size,
            Image::Tgc(tgc) => tgc.size - tgc.header_size,
            Image::Wbfs(wbfs) => wbfs.table.len() as u64 * wbfs.block_size,
            Image::Wia(wia) => wia.disc_size,
        }
    }

    fn data_size(&self) -> u64 {
        match self {
            Image::Ciso(ciso) => ciso.data_size(),
            Image::Wbfs(wbfs) => wbfs.data_size(),
            _ => self.limit(),
        }
    }

    fn read_at(&mut self, offset: u64, out: &mut [u8]) -> Option<()> {
        match self {
            Image::Plain(parts) => parts.read_at(offset, out),
            Image::Ciso(ciso) => ciso.read_at(offset, out),
            Image::Gcz(gcz) => gcz.read_at(offset, out),
            Image::Tgc(tgc) => tgc.read_at(offset, out),
            Image::Wbfs(wbfs) => wbfs.read_at(offset, out),
            Image::Wia(wia) => wia.read_at(offset, out),
        }
    }

    /// `length` bytes of the disc from `offset`, or `None` when the image
    /// doesn't hold them. Checked against the disc first, so a damaged size
    /// never sets aside more memory than the disc could fill.
    fn read_vec(&mut self, offset: u64, length: usize) -> Option<Vec<u8>> {
        if offset.checked_add(length as u64)? > self.limit() {
            return None;
        }
        let mut bytes = vec![0; length];
        self.read_at(offset, &mut bytes)?;
        Some(bytes)
    }
}

/// One or more files read as if they were one, for an image kept in parts.
struct Parts {
    files: Vec<(File, u64)>,
}

impl Parts {
    fn one(file: File, size: u64) -> Self {
        Self { files: vec![(file, size)] }
    }

    fn len(&self) -> u64 {
        self.files.iter().map(|(_, size)| size).sum()
    }

    fn read_at(&mut self, mut offset: u64, out: &mut [u8]) -> Option<()> {
        let mut done = 0;
        for (file, size) in &mut self.files {
            if done == out.len() {
                break;
            }
            if offset >= *size {
                offset -= *size;
                continue;
            }
            let take = (*size - offset).min((out.len() - done) as u64) as usize;
            read_file_into(file, offset, &mut out[done..done + take])?;
            done += take;
            offset = 0;
        }
        // `Some(())` when every byte was there, `None` when the files ran out.
        (done == out.len()).then_some(())
    }
}

/// A plain image may be split into `name.part0.iso`, `name.part1.iso` and
/// on, read as one when there are at least two and none is empty. Dolphin
/// 2609a, DiscIO/SplitFileBlob.cpp, SplitPlainFileReader::Create.
fn split_iso_parts(first: File, size: u64, path: &Path) -> Parts {
    let mut parts = Parts::one(first, size);
    // `let ... else` takes the value out when there is one, and leaves by
    // the `else` when there isn't.
    let Some(base) = path.to_str().and_then(|text| text.strip_suffix(".part0.iso")) else {
        return parts;
    };
    for index in 1.. {
        let Ok(file) = File::open(format!("{base}.part{index}.iso")) else {
            break;
        };
        let size = file.metadata().map_or(0, |metadata| metadata.len());
        if size == 0 {
            parts.files.truncate(1);
            break;
        }
        parts.files.push((file, size));
    }
    parts
}

/// A WBFS image may go on in files named like the first with its last
/// character a digit from 1, `game.wbf1` after `game.wbfs`, up to ten files
/// in all. Dolphin 2609a, DiscIO/WbfsBlob.cpp,
/// WbfsFileReader::OpenAdditionalFiles.
fn wbfs_parts(first: File, size: u64, path: &Path) -> Parts {
    let mut parts = Parts::one(first, size);
    let Some(mut stem) = path.to_str().map(str::to_string) else {
        return parts;
    };
    stem.pop();
    for index in 1..10 {
        let Ok(file) = File::open(format!("{stem}{index}")) else {
            break;
        };
        let size = file.metadata().map_or(0, |metadata| metadata.len());
        parts.files.push((file, size));
    }
    parts
}

/// A 0x8000-byte header: "CISO", the block size in little-endian, then one
/// byte for each block of the disc, 1 when the block is stored. The stored
/// blocks follow in order, and one not stored reads as zeros. Dolphin
/// 2609a, DiscIO/CISOBlob.h and CISOBlob.cpp, CISOFileReader.
struct Ciso {
    file: File,
    block_size: u64,
    /// For each block of the disc, where it is among the stored ones.
    stored: Vec<Option<u64>>,
}

const CISO_HEADER: usize = 0x8000;

impl Ciso {
    fn open(mut file: File) -> Option<Self> {
        let header = read_file_at(&mut file, 0, CISO_HEADER)?;
        let block_size = u64::from(le32(&header, 4));
        if block_size == 0 {
            return None;
        }
        let mut count = 0;
        let stored = header[8..]
            .iter()
            .map(|&used| {
                (used == 1).then(|| {
                    count += 1;
                    count - 1
                })
            })
            .collect();
        Some(Self { file, block_size, stored })
    }

    /// Dolphin counts every block the map has room for, which comes to
    /// gigabytes more than any disc; how far the stored blocks reach is the
    /// size that matters for room.
    fn data_size(&self) -> u64 {
        let last = self.stored.iter().rposition(Option::is_some);
        last.map_or(0, |last| (last as u64 + 1) * self.block_size)
    }

    fn read_at(&mut self, mut offset: u64, out: &mut [u8]) -> Option<()> {
        let mut done = 0;
        while done < out.len() {
            let within = offset % self.block_size;
            let take = (self.block_size - within).min((out.len() - done) as u64) as usize;
            let piece = &mut out[done..done + take];
            match *self.stored.get((offset / self.block_size) as usize)? {
                Some(index) => {
                    let at = CISO_HEADER as u64 + index * self.block_size + within;
                    read_file_into(&mut self.file, at, piece)?;
                }
                None => piece.fill(0),
            }
            done += take;
            offset += take as u64;
        }
        Some(())
    }
}

/// Dolphin's own compressed GameCube format. A 32-byte header in
/// little-endian (magic, kind, size of the stored blocks, size of the disc,
/// block size, number of blocks), then where each block starts among the
/// stored ones, with the top bit set for a block kept as it is, then a
/// checksum for each block, then the blocks. Any other block is zlib.
/// Dolphin 2609a, DiscIO/CompressedBlob.h and CompressedBlob.cpp,
/// CompressedBlobReader::Initialize, ValidateBlockPointers and GetBlock.
struct Gcz {
    file: File,
    block_size: u64,
    disc_size: u64,
    starts: Vec<u64>,
    stored_size: u64,
    data_start: u64,
}

const GCZ_HEADER: u64 = 32;
const KEPT_AS_IT_IS: u64 = 1 << 63;
/// Dolphin writes blocks of 2 MiB at most (DolphinQt/ConvertDialog.cpp and
/// DolphinTool/ConvertCommand.cpp, PREFERRED_MAX_BLOCK_SIZE), and 32 KiB at
/// least, so even a dual-layer Wii disc's table of where they start comes to
/// a few megabytes. Far larger blocks or a far larger table mean a damaged
/// header.
const MOST_BLOCK: u64 = 64 * 1024 * 1024;
const MOST_TABLE: u64 = 64 * 1024 * 1024;

impl Gcz {
    fn open(mut file: File, size: u64) -> Option<Self> {
        let header = read_file_at(&mut file, 0, GCZ_HEADER as usize)?;
        let stored_size = le64(&header, 8);
        let disc_size = le64(&header, 16);
        let block_size = u64::from(le32(&header, 24));
        let blocks = u64::from(le32(&header, 28));
        let data_start = GCZ_HEADER + blocks * 12;
        let fits = data_start.checked_add(stored_size).is_some_and(|end| end <= size);
        if blocks == 0 || blocks * 8 > MOST_TABLE || block_size == 0 || block_size > MOST_BLOCK || !fits {
            return None;
        }
        let table = read_file_at(&mut file, GCZ_HEADER, blocks as usize * 8)?;
        let starts = table.chunks_exact(8).map(|start| le64(start, 0)).collect();
        let gcz = Self { file, block_size, disc_size, starts, stored_size, data_start };
        // A block kept as it is must be exactly one block long. A compressed
        // one is never longer than that, and Dolphin allows it 64 bytes
        // over to be safe.
        let all_fit = (0..gcz.starts.len()).all(|index| {
            let (start, end, kept) = gcz.span(index);
            end <= stored_size
                && start <= end
                && if kept { end - start == block_size } else { end - start <= block_size + 64 }
        });
        all_fit.then_some(gcz)
    }

    /// Where block `index` starts and ends among the stored ones, and whether
    /// it is kept as it is.
    fn span(&self, index: usize) -> (u64, u64, bool) {
        let start = self.starts[index];
        let end = self.starts.get(index + 1).map_or(self.stored_size, |next| next & !KEPT_AS_IT_IS);
        (start & !KEPT_AS_IT_IS, end, start & KEPT_AS_IT_IS != 0)
    }

    fn block(&mut self, index: usize) -> Option<Vec<u8>> {
        if index >= self.starts.len() {
            return None;
        }
        let (start, end, kept) = self.span(index);
        let stored = read_file_at(&mut self.file, self.data_start + start, (end - start) as usize)?;
        if kept {
            return Some(stored);
        }
        let mut block = Vec::with_capacity(self.block_size as usize);
        flate2::read::ZlibDecoder::new(stored.as_slice())
            .take(self.block_size + 1)
            .read_to_end(&mut block)
            .ok()?;
        (block.len() as u64 == self.block_size).then_some(block)
    }

    fn read_at(&mut self, mut offset: u64, out: &mut [u8]) -> Option<()> {
        let mut done = 0;
        while done < out.len() {
            let within = offset % self.block_size;
            let take = (self.block_size - within).min((out.len() - done) as u64) as usize;
            let block = self.block((offset / self.block_size) as usize)?;
            out[done..done + take].copy_from_slice(&block[within as usize..within as usize + take]);
            done += take;
            offset += take as u64;
        }
        Some(())
    }
}

/// A GameCube disc kept inside a demo disc's TGC file. The TGC header, in
/// big-endian, says where the disc starts in the file and where its DOL and
/// file table really are. The disc's own header points elsewhere, so Dolphin
/// puts those two places right as it reads, and moves every file in the
/// table by how far the files were shifted. Dolphin 2609a, DiscIO/TGCBlob.h
/// and TGCBlob.cpp, TGCFileReader.
struct Tgc {
    file: File,
    size: u64,
    header_size: u64,
    dol_at: u32,
    fst_at: u32,
    fst: Vec<u8>,
}

const TGC_HEADER: usize = 0x38;
const FST_ENTRY: usize = 12;

impl Tgc {
    fn open(mut file: File, size: u64) -> Option<Self> {
        let header = read_file_at(&mut file, 0, TGC_HEADER)?;
        let header_size = be32(&header, 0x08);
        let fst_real = be32(&header, 0x10);
        let fst_size = u64::from(be32(&header, 0x14));
        let dol_real = be32(&header, 0x1C);
        let files_real = be32(&header, 0x24);
        let files_virtual = be32(&header, 0x34);
        if u64::from(header_size) > size {
            return None;
        }
        // Dolphin carries on without the table when it can't be read.
        let mut fst = if fst_size <= MOST_FST {
            read_file_at(&mut file, u64::from(fst_real), fst_size as usize).unwrap_or_default()
        } else {
            Vec::new()
        };
        // Wrapping arithmetic, as Dolphin's 32-bit sums wrap: a shift below
        // zero comes out right when it is added back.
        let shift = files_real.wrapping_sub(files_virtual).wrapping_sub(header_size);
        if fst.len() >= FST_ENTRY {
            let entries = (be32(&fst, 8) as usize).min(fst.len() / FST_ENTRY);
            for entry in fst.chunks_exact_mut(FST_ENTRY).take(entries) {
                if entry[0] == 0 {
                    let moved = be32(entry, 4).wrapping_add(shift);
                    entry[4..8].copy_from_slice(&moved.to_be_bytes());
                }
            }
        }
        Some(Self {
            file,
            size,
            header_size: u64::from(header_size),
            dol_at: dol_real.wrapping_sub(header_size),
            fst_at: fst_real.wrapping_sub(header_size),
            fst,
        })
    }

    fn read_at(&mut self, offset: u64, out: &mut [u8]) -> Option<()> {
        read_file_into(&mut self.file, offset + self.header_size, out)?;
        overlay(out, offset, 0x420, &self.dol_at.to_be_bytes());
        overlay(out, offset, FST_OFFSET_AT as u64, &self.fst_at.to_be_bytes());
        overlay(out, offset, u64::from(self.fst_at), &self.fst);
        Some(())
    }
}

/// Copies whatever part of `bytes`, which belong at `at` on the disc, falls
/// within `out`, which was read from `offset`.
fn overlay(out: &mut [u8], offset: u64, at: u64, bytes: &[u8]) {
    let start = at.max(offset);
    let end = (at + bytes.len() as u64).min(offset + out.len() as u64);
    if end > start {
        out[(start - offset) as usize..(end - offset) as usize]
            .copy_from_slice(&bytes[(start - at) as usize..(end - at) as usize]);
    }
}

/// WBFS, the format Wii USB loaders keep games in. A header names its own
/// sector size and the size of the blocks a disc is stored in, as powers of
/// two; the files must come to exactly its sector count, and its first disc
/// slot must be in use. That disc's entry follows at the second sector: a
/// copy of the disc's first 256 bytes, then, for every block of a whole Wii
/// disc, the block of the file that holds it, 0 when it isn't stored.
/// Dolphin 2609a, DiscIO/WbfsBlob.cpp,
/// WbfsFileReader::ReadHeader, WbfsFileReader::WbfsFileReader and
/// SeekToCluster.
struct Wbfs {
    parts: Parts,
    block_size: u64,
    table: Vec<u16>,
}

const WII_SECTOR_SIZE: u64 = 0x8000;
const WII_SECTOR_COUNT: u64 = 143_432 * 2;
const WBFS_DISC_HEADER_COPY: u64 = 256;

impl Wbfs {
    fn open(mut parts: Parts) -> Option<Self> {
        let mut header = [0; 13];
        parts.read_at(0, &mut header)?;
        let sector_size = 1u64.checked_shl(u32::from(header[8]))?;
        let block_size = 1u64.checked_shl(u32::from(header[9]))?;
        let sectors = u64::from(be32(&header, 4));
        if sectors.checked_mul(sector_size)? != parts.len() || block_size < WII_SECTOR_SIZE || header[12] == 0 {
            return None;
        }
        let blocks = (WII_SECTOR_COUNT * WII_SECTOR_SIZE).div_ceil(block_size) as usize;
        let mut table = vec![0; blocks * 2];
        parts.read_at(sector_size + WBFS_DISC_HEADER_COPY, &mut table)?;
        let table = table.chunks_exact(2).map(|entry| be16(entry, 0)).collect();
        Some(Self { parts, block_size, table })
    }

    /// Dolphin counts every block a whole Wii disc has; how far the stored
    /// blocks reach is the size that matters for room.
    fn data_size(&self) -> u64 {
        let last = self.table.iter().rposition(|&block| block != 0);
        let reach = last.map_or(0, |last| (last as u64 + 1) * self.block_size);
        reach.min(WII_SECTOR_COUNT * WII_SECTOR_SIZE)
    }

    fn read_at(&mut self, mut offset: u64, out: &mut [u8]) -> Option<()> {
        let mut done = 0;
        while done < out.len() {
            let within = offset % self.block_size;
            let take = (self.block_size - within).min((out.len() - done) as u64) as usize;
            let block = u64::from(*self.table.get((offset / self.block_size) as usize)?);
            let at = block.checked_mul(self.block_size)? + within;
            self.parts.read_at(at, &mut out[done..done + take])?;
            done += take;
            offset += take as u64;
        }
        Some(())
    }
}

/// WIA, and RVZ, which Dolphin made from it. Two headers, both checked by
/// SHA-1, hold the disc's size and its first 0x80 bytes, which name the
/// game, and they are all Omoio reads. The rest of the disc is compressed in
/// groups found through tables of their own, and a Wii partition's entry
/// there also holds its key. Reading that would take a lot of code for one
/// 96 by 32 picture, so a GameCube game kept this way gets no banner from its
/// image, and the library uses its own art or a RAWG cover instead. Dolphin
/// 2609a, DiscIO/WIABlob.h and WIABlob.cpp, WIARVZFileReader::Initialize and
/// Read, and docs/WiaAndRvz.md, which describes both formats.
struct Wia {
    disc_size: u64,
    disc_start: Vec<u8>,
}

const WIA_HEADER_1: usize = 0x48;
const WIA_HEADER_2: usize = 0xDC;
/// The second header without its last seven bytes, which only LZMA uses.
const WIA_HEADER_2_LEAST: usize = WIA_HEADER_2 - 7;
const WIA_DISC_START: usize = 0x80;
/// The newest version Dolphin writes, and the oldest each format it reads.
const WIA_VERSION: u32 = 0x0100_0000;
const WIA_OLDEST: u32 = 0x0008_0000;
const RVZ_OLDEST: u32 = 0x0003_0000;
/// The compressions Dolphin reads run from 0, none, to LZMA2 for WIA and
/// zstd for RVZ, which has no purge.
const PURGE: u32 = 1;
const LZMA2: u32 = 4;
const ZSTD: u32 = 5;
/// WIA's groups are whole multiples of 2 MiB; RVZ's may also be a power of
/// two from 32 KiB.
const WIA_GROUP_STEP: u64 = 0x20_0000;
const RVZ_SMALLEST_GROUP: u64 = 0x8000;

impl Wia {
    /// Takes the file only when Dolphin would: a version it reads, both
    /// headers matching their SHA-1, the size the file says it has, and a
    /// group size and compression it knows.
    fn open(mut file: File, size: u64, rvz: bool) -> Option<Self> {
        let first = read_file_at(&mut file, 0, WIA_HEADER_1)?;
        let oldest = if rvz { RVZ_OLDEST } else { WIA_OLDEST };
        if be32(&first, 8) > WIA_VERSION || be32(&first, 4) < oldest {
            return None;
        }
        if Sha1::digest(&first[..0x34]).as_slice() != &first[0x34..0x48] || be64(&first, 0x2C) != size {
            return None;
        }
        let second_size = be32(&first, 0x0C) as usize;
        if second_size < WIA_HEADER_2_LEAST || (WIA_HEADER_1 + second_size) as u64 > size {
            return None;
        }
        let mut second = read_file_at(&mut file, WIA_HEADER_1 as u64, second_size)?;
        if Sha1::digest(&second).as_slice() != &first[0x10..0x24] {
            return None;
        }
        // Dolphin reads the header into its full size, short or long.
        second.resize(WIA_HEADER_2, 0);
        if usize::from(second[0xD4]) > 7 || second_size < WIA_HEADER_2_LEAST + usize::from(second[0xD4]) {
            return None;
        }
        let chunk_size = u64::from(be32(&second, 0x0C));
        let small_group = rvz && chunk_size >= RVZ_SMALLEST_GROUP && chunk_size.is_power_of_two();
        if chunk_size == 0 || (!small_group && chunk_size % WIA_GROUP_STEP != 0) {
            return None;
        }
        let compression = be32(&second, 0x04);
        if compression > (if rvz { ZSTD } else { LZMA2 }) || (rvz && compression == PURGE) {
            return None;
        }
        Some(Self {
            disc_size: be64(&first, 0x24),
            disc_start: second[0x10..0x90].to_vec(),
        })
    }

    /// Only the first 0x80 bytes, which the header holds; `None` past them.
    fn read_at(&self, offset: u64, out: &mut [u8]) -> Option<()> {
        let end = offset.checked_add(out.len() as u64)?;
        if end > WIA_DISC_START as u64 {
            return None;
        }
        out.copy_from_slice(&self.disc_start[offset as usize..end as usize]);
        Some(())
    }
}

/// The folder an extracted disc's `sys` and `files` are in: the one given,
/// or, for a whole Wii disc, its game partition's, named DATA or P0 the way
/// Dolphin names partition folders. Dolphin 2609a, DiscIO/DirectoryBlob.cpp,
/// IsValidDirectoryBlob and ParsePartitionDirectoryName, and
/// DolphinTool/ExtractCommand.cpp, HandleExtractPartition.
pub fn partition_root(dir: &Path) -> Option<PathBuf> {
    if is_partition_root(dir) {
        return Some(dir.to_path_buf());
    }
    std::fs::read_dir(dir).ok()?.flatten().map(|entry| entry.path()).find(|path| {
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        is_game_partition_name(&name) && is_partition_root(path)
    })
}

/// Dolphin takes a folder whose `sys/main.dol` is there and whose
/// `sys/boot.bin` holds at least the 0x20 bytes before the title.
fn is_partition_root(dir: &Path) -> bool {
    let sys = dir.join("sys");
    sys.join("main.dol").is_file()
        && std::fs::metadata(sys.join("boot.bin")).is_ok_and(|boot| boot.len() >= TITLE_AT as u64)
}

fn is_game_partition_name(name: &str) -> bool {
    if name.eq_ignore_ascii_case("DATA") {
        return true;
    }
    match name.strip_prefix(['P', 'p']) {
        Some(number) => !number.is_empty() && number.bytes().all(|digit| digit == b'0'),
        None => false,
    }
}

/// The disc header as Dolphin builds it for a folder: `sys/boot.bin`, with
/// zeros past its end. A Wii partition's folder may also have the plain
/// header from outside the partition in `disc/header.bin`, which Dolphin
/// puts first and reads the game from; the partition's own header fills in
/// what it lacks, apart from two flags at 0x60 that only the plain header
/// has. Dolphin 2609a, DiscIO/DirectoryBlob.cpp,
/// DirectoryBlobPartition::DirectoryBlobPartition and
/// DirectoryBlobReader::SetNonpartitionDiscHeader.
fn folder_header(root: &Path) -> Option<Vec<u8>> {
    let mut header = read_start(&root.join("sys").join("boot.bin"), HEADER_SIZE)?;
    header.resize(HEADER_SIZE, 0);
    if platform_of(&header) == Some(Platform::Wii) {
        if let Some(plain) = read_start(&root.join("disc").join("header.bin"), 0x100) {
            header[..plain.len()].copy_from_slice(&plain);
            for flag in [0x60, 0x61] {
                if plain.len() <= flag {
                    header[flag] = 0;
                }
            }
        }
    }
    Some(header)
}

/// A GameCube folder's region from `sys/bi2.bin`. Dolphin counts it as
/// unknown when the file is too short to hold it. Dolphin 2609a,
/// DiscIO/DirectoryBlob.cpp, DirectoryBlobPartition::SetBI2FromFile.
fn folder_region(root: &Path) -> u32 {
    const UNKNOWN: u32 = 0xFF;
    match read_start(&root.join("sys").join("bi2.bin"), REGION_IN_BI2 + 4) {
        Some(bi2) if bi2.len() >= REGION_IN_BI2 + 4 => be32(&bi2, REGION_IN_BI2),
        _ => UNKNOWN,
    }
}

/// Up to `most` bytes from the start of a file.
fn read_start(path: &Path, most: usize) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path).ok()?.take(most as u64).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// opening.bnr from a GameCube image, found through the disc's file table.
fn banner_in_image(path: &Path) -> Option<Vec<u8>> {
    let mut image = Image::open(path).ok()?;
    let header = image.read_vec(0, HEADER_SIZE)?;
    // A Wii disc's file table is inside its encrypted partition.
    if platform_of(&header)? != Platform::GameCube {
        return None;
    }
    let fst_size = u64::from(be32(&header, FST_SIZE_AT));
    if fst_size > MOST_FST {
        return None;
    }
    let fst = image.read_vec(u64::from(be32(&header, FST_OFFSET_AT)), fst_size as usize)?;
    let (offset, size) = find_in_root(&fst, BANNER_FILE)?;
    image.read_vec(offset, size.min(BNR2_SIZE as u64) as usize)
}

/// The offset and size of a file in the top folder of a GameCube file
/// table. The table is entries of 12 bytes: a byte that is not 0 for a
/// folder, three bytes giving where the name starts among the names after
/// the entries, then a file's offset and size, or a folder's parent and the
/// entry after its last. The first entry is the top folder, whose last word
/// is the number of entries. Names match without regard to case, since some
/// discs call the banner OPENING.BNR. Dolphin 2609a,
/// DiscIO/FileSystemGCWii.cpp, FileSystemGCWii::FileSystemGCWii,
/// FindFileInfo and FileInfoGCWii::NameCaseInsensitiveEquals.
fn find_in_root(fst: &[u8], wanted: &[u8]) -> Option<(u64, u64)> {
    if fst.len() < FST_ENTRY || fst[0] == 0 || fst.last() != Some(&0) {
        return None;
    }
    let count = be32(fst, 8) as usize;
    let names = count.checked_mul(FST_ENTRY).filter(|&names| names <= fst.len())?;
    let mut index = 1;
    while index < count {
        let entry = &fst[index * FST_ENTRY..(index + 1) * FST_ENTRY];
        let name_at = names + (be32(entry, 0) & 0x00FF_FFFF) as usize;
        let name = fst.get(name_at..)?;
        let name = &name[..name.iter().position(|&byte| byte == 0)?];
        if entry[0] != 0 {
            // A folder: its contents aren't in the top folder, so go past
            // them. A folder that ends before itself is a damaged table.
            let next = be32(entry, 8) as usize;
            if next <= index {
                return None;
            }
            index = next;
        } else if name.eq_ignore_ascii_case(wanted) {
            return Some((u64::from(be32(entry, 4)), u64::from(be32(entry, 8))));
        } else {
            index += 1;
        }
    }
    None
}

/// opening.bnr from an extracted GameCube folder, in its `files` folder.
fn banner_in_folder(dir: &Path) -> Option<Vec<u8>> {
    let root = partition_root(dir)?;
    if platform_of(&folder_header(&root)?)? != Platform::GameCube {
        return None;
    }
    let banner = std::fs::read_dir(root.join("files")).ok()?.flatten().find(|entry| {
        entry.file_name().to_string_lossy().as_bytes().eq_ignore_ascii_case(BANNER_FILE)
    })?;
    read_start(&banner.path(), BNR2_SIZE)
}

/// The picture in an opening.bnr, read only when the file is a size its kind
/// has.
fn banner_picture(file: &[u8]) -> Option<Picture> {
    let fits = match file.get(..4)? {
        b"BNR1" => file.len() == BNR1_SIZE,
        b"BNR2" => file.len() == BNR2_SIZE,
        _ => false,
    };
    if !fits {
        return None;
    }
    decode_rgb5a3(&file[BANNER_AT..], BANNER_WIDTH, BANNER_HEIGHT)
}

/// RGB5A3, the GameCube's and Wii's 16-bit picture format, stored in tiles
/// of 4 by 4 pixels, the tiles left to right and then down, each pixel
/// big-endian. With the top bit set a pixel is opaque, with five bits each
/// of red, green and blue; without it, three bits of alpha come first, then
/// four bits of each colour. The bits are widened to eight by repeating
/// them, as Dolphin's texture decoder does. Its game list instead lays a
/// banner over black; keeping the transparency lets the library choose.
/// Dolphin 2609a, VideoCommon/TextureDecoder_Generic.cpp,
/// DecodePixel_RGB5A3; VideoCommon/LookUpTables.h, Convert3To8, Convert4To8
/// and Convert5To8; Common/ColorUtil.cpp, Decode5A3Image, for the tiles.
fn decode_rgb5a3(data: &[u8], width: usize, height: usize) -> Option<Picture> {
    let data = data.get(..width * height * 2)?;
    let mut rgba = vec![0; width * height * 4];
    let mut pixels = data.chunks_exact(2).map(|pixel| be16(pixel, 0));
    for tile_y in (0..height).step_by(4) {
        for tile_x in (0..width).step_by(4) {
            for y in tile_y..tile_y + 4 {
                for x in tile_x..tile_x + 4 {
                    let at = (y * width + x) * 4;
                    rgba[at..at + 4].copy_from_slice(&rgb5a3_pixel(pixels.next()?));
                }
            }
        }
    }
    Some(Picture { width: width as u32, height: height as u32, rgba })
}

fn rgb5a3_pixel(pixel: u16) -> [u8; 4] {
    let five = |bits: u16| {
        let bits = (bits & 0x1F) as u8;
        (bits << 3) | (bits >> 2)
    };
    let four = |bits: u16| (bits & 0xF) as u8 * 0x11;
    let three = |bits: u16| {
        let bits = (bits & 0x7) as u8;
        (bits << 5) | (bits << 2) | (bits >> 1)
    };
    if pixel & 0x8000 != 0 {
        [five(pixel >> 10), five(pixel >> 5), five(pixel), 0xFF]
    } else {
        [four(pixel >> 8), four(pixel >> 4), four(pixel), three(pixel >> 12)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const GAMECUBE_ID: &[u8; 6] = b"GALE01";
    const GAMECUBE_TITLE: &str = "Super Smash Bros Melee";
    const WII_ID: &[u8; 6] = b"SSPP52";
    const WII_TITLE: &str = "Skylanders Spyro's Adventure";
    /// The test GameCube disc: its file table, its banner, a file in a
    /// folder, and some data at the end, in 64 KiB.
    const FST_AT: usize = 0x3000;
    const BANNER_ON_DISC: usize = 0x4000;
    const DISC_LEN: usize = 0x10000;

    /// A folder of its own under the system's temporary folder.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("omoio-disc-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn written(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn header(id: &[u8; 6], disc_number: u8, revision: u8, title: &[u8], platform: Platform) -> Vec<u8> {
        let mut header = vec![0; HEADER_SIZE];
        header[..6].copy_from_slice(id);
        header[6] = disc_number;
        header[7] = revision;
        match platform {
            Platform::Wii => header[WII_MAGIC_AT..WII_MAGIC_AT + 4].copy_from_slice(&WII_MAGIC.to_be_bytes()),
            Platform::GameCube => {
                header[GAMECUBE_MAGIC_AT..GAMECUBE_MAGIC_AT + 4].copy_from_slice(&GAMECUBE_MAGIC.to_be_bytes())
            }
        }
        header[TITLE_AT..TITLE_AT + title.len()].copy_from_slice(title);
        header
    }

    /// Pixels laid out in RGB5A3's 4 by 4 tiles.
    fn tiled(pixels: &[u16], width: usize, height: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for tile_y in (0..height).step_by(4) {
            for tile_x in (0..width).step_by(4) {
                for y in tile_y..tile_y + 4 {
                    for x in tile_x..tile_x + 4 {
                        out.extend_from_slice(&pixels[y * width + x].to_be_bytes());
                    }
                }
            }
        }
        out
    }

    /// An opening.bnr, every pixel opaque black but an opaque red one at
    /// (0, 0) and a see-through red one at (5, 1).
    fn banner(kind: &[u8; 4]) -> Vec<u8> {
        let mut file = vec![0; if kind == b"BNR1" { BNR1_SIZE } else { BNR2_SIZE }];
        file[..4].copy_from_slice(kind);
        let mut pixels = vec![0x8000; BANNER_WIDTH * BANNER_HEIGHT];
        pixels[0] = 0xFC00;
        pixels[BANNER_WIDTH + 5] = 0x3F00;
        let image = tiled(&pixels, BANNER_WIDTH, BANNER_HEIGHT);
        file[BANNER_AT..BANNER_AT + image.len()].copy_from_slice(&image);
        file
    }

    /// A file table: the top folder, then each entry as a name, whether it
    /// is a folder, and its two words.
    fn fst(entries: &[(&str, bool, u32, u32)]) -> Vec<u8> {
        let mut table = vec![1, 0, 0, 0, 0, 0, 0, 0];
        table.extend_from_slice(&(entries.len() as u32 + 1).to_be_bytes());
        let mut names = Vec::new();
        for &(name, folder, first, second) in entries {
            let at = names.len() as u32;
            names.extend_from_slice(name.as_bytes());
            names.push(0);
            table.extend_from_slice(&((u32::from(folder) << 24) | at).to_be_bytes());
            table.extend_from_slice(&first.to_be_bytes());
            table.extend_from_slice(&second.to_be_bytes());
        }
        table.extend_from_slice(&names);
        table
    }

    /// The test GameCube disc's file table. The banner is in the top folder
    /// in capitals, and a decoy of the same name sits in a folder before it.
    fn gamecube_fst(banner_len: usize) -> Vec<u8> {
        fst(&[
            ("audio", true, 0, 3),
            ("opening.bnr", false, 0x9000, 0x10),
            ("OPENING.BNR", false, BANNER_ON_DISC as u32, banner_len as u32),
        ])
    }

    fn gamecube_disc_with(title: &[u8], region: u32, banner: &[u8]) -> Vec<u8> {
        let mut disc = vec![0; DISC_LEN];
        disc[..HEADER_SIZE].copy_from_slice(&header(GAMECUBE_ID, 0, 1, title, Platform::GameCube));
        let table = gamecube_fst(banner.len());
        disc[0x420..0x424].copy_from_slice(&0x2800u32.to_be_bytes());
        disc[FST_OFFSET_AT..FST_OFFSET_AT + 4].copy_from_slice(&(FST_AT as u32).to_be_bytes());
        disc[FST_SIZE_AT..FST_SIZE_AT + 4].copy_from_slice(&(table.len() as u32).to_be_bytes());
        disc[REGION_AT as usize..REGION_AT as usize + 4].copy_from_slice(&region.to_be_bytes());
        disc[FST_AT..FST_AT + table.len()].copy_from_slice(&table);
        disc[BANNER_ON_DISC..BANNER_ON_DISC + banner.len()].copy_from_slice(banner);
        disc[0x9000..0x9010].fill(0x55);
        disc[0xF000..0xF010].fill(0xAA);
        disc
    }

    fn gamecube_disc() -> Vec<u8> {
        gamecube_disc_with(GAMECUBE_TITLE.as_bytes(), 1, &banner(b"BNR2"))
    }

    fn wii_disc(len: usize) -> Vec<u8> {
        let mut disc = vec![0; len];
        disc[..HEADER_SIZE].copy_from_slice(&header(WII_ID, 0, 2, WII_TITLE.as_bytes(), Platform::Wii));
        disc
    }

    fn expect_gamecube(disc: &Disc, data_size: u64) {
        assert_eq!(disc.platform, Platform::GameCube);
        assert_eq!(disc.game_id, "GALE01");
        assert_eq!((disc.disc_number, disc.revision), (0, 1));
        assert_eq!(disc.title, GAMECUBE_TITLE);
        assert_eq!(disc.data_size, data_size);
    }

    fn expect_wii(disc: &Disc, data_size: u64) {
        assert_eq!(disc.platform, Platform::Wii);
        assert_eq!(disc.game_id, "SSPP52");
        assert_eq!((disc.disc_number, disc.revision), (0, 2));
        assert_eq!(disc.title, WII_TITLE);
        assert_eq!(disc.data_size, data_size);
    }

    fn png_pixels(file: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut reader = png::Decoder::new(file).read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut pixels).unwrap();
        pixels.truncate(info.buffer_size());
        (info.width, info.height, pixels)
    }

    fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * width + x) * 4) as usize;
        pixels[at..at + 4].try_into().unwrap()
    }

    /// The test banner came through: opaque red, see-through red, black.
    fn expect_banner(png: Option<Vec<u8>>) {
        let (width, height, pixels) = png_pixels(&png.expect("a banner"));
        assert_eq!((width, height), (96, 32));
        assert_eq!(pixel(&pixels, width, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&pixels, width, 5, 1), [255, 0, 0, 109]);
        assert_eq!(pixel(&pixels, width, 95, 31), [0, 0, 0, 255]);
    }

    #[test]
    fn names_are_told_apart_without_opening_anything() {
        for name in ["a.iso", "b.GCM", "c.wbfs", "d.rvz", "e.wia", "f.gcz", "g.ciso", "h.tgc", "Game.part0.iso"] {
            assert!(is_disc_name(name), "{name}");
        }
        for name in ["a.wad", "b.dol", "c.elf", "d.dff", "e.m3u", "banner.bin", "f.nfs", "iso", "g.json"] {
            assert!(!is_disc_name(name), "{name}");
        }
        assert!(is_channel_name("Channel.WAD"));
        assert!(!is_channel_name("game.iso"));
    }

    #[test]
    fn only_the_first_part_of_a_split_image_is_a_game() {
        assert!(is_disc_name("Melee.part0.iso"));
        assert!(is_disc_name(r"D:\Games\Melee.part0.iso"));
        for later in ["Melee.part1.iso", "Melee.part12.iso", r"D:\Games\Melee.part2.iso"] {
            assert!(!is_disc_name(later), "{later}");
        }
        // Names Dolphin wouldn't read as a part are whole images.
        for whole in ["Melee.part.iso", "Melee.parts.iso", "Melee.part01.iso", "Melee.part1.gcm", "part1.iso"] {
            assert!(is_disc_name(whole), "{whole}");
        }
    }

    #[test]
    fn a_gamecube_iso_gives_its_game_and_banner() {
        let dir = scratch("gc-iso");
        let path = written(&dir, "melee.iso", &gamecube_disc());
        expect_gamecube(&read(&path).unwrap(), DISC_LEN as u64);
        expect_banner(gamecube_banner_png(&path));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_wii_iso_gives_its_game_but_no_gamecube_banner() {
        let dir = scratch("wii-iso");
        let path = written(&dir, "spyro.iso", &wii_disc(0x8000));
        expect_wii(&read(&path).unwrap(), 0x8000);
        assert_eq!(gamecube_banner_png(&path), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_second_disc_of_a_set_is_numbered_one() {
        let dir = scratch("disc-two");
        let mut disc = gamecube_disc();
        disc[6] = 1;
        let path = written(&dir, "two.gcm", &disc);
        assert_eq!(read(&path).unwrap().disc_number, 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn titles_are_shift_jis_only_for_japan() {
        let dir = scratch("titles");
        // "ゼルダ" in Shift-JIS.
        let zelda = [0x83, 0x5B, 0x83, 0x8B, 0x83, 0x5F];
        let japan = written(&dir, "japan.iso", &gamecube_disc_with(&zelda, REGION_JAPAN, &banner(b"BNR1")));
        assert_eq!(read(&japan).unwrap().title, "ゼルダ");
        // A Japanese BNR1 banner is read too.
        assert!(gamecube_banner_png(&japan).is_some());

        // "Pokémon" in Windows-1252, with padding around it.
        let pokemon = b"  Pok\xE9mon ";
        let europe = written(&dir, "europe.iso", &gamecube_disc_with(pokemon, 2, &banner(b"BNR2")));
        assert_eq!(read(&europe).unwrap().title, "Pokémon");

        // A Wii disc is taken as Japanese by the J in its id.
        let mut wii = wii_disc(0x1000);
        wii[..HEADER_SIZE].copy_from_slice(&header(b"RZDJ01", 0, 0, &zelda, Platform::Wii));
        assert_eq!(read(&written(&dir, "wii.iso", &wii)).unwrap().title, "ゼルダ");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn region_falls_back_to_the_country_letter_as_dolphin_reads_it() {
        assert!(is_japanese(Platform::Wii, None, b'J'));
        assert!(is_japanese(Platform::Wii, None, b'W'));
        assert!(!is_japanese(Platform::Wii, None, b'K'));
        assert!(is_japanese(Platform::GameCube, None, b'K'));
        assert!(!is_japanese(Platform::GameCube, None, b'E'));
        assert!(!is_japanese(Platform::GameCube, Some(1), b'J'), "bi2.bin wins");
        assert!(is_japanese(Platform::GameCube, Some(0), b'E'));
    }

    #[test]
    fn an_odd_character_in_the_id_becomes_a_dash() {
        let dir = scratch("odd-id");
        let mut disc = gamecube_disc();
        disc[..6].copy_from_slice(b"GA/E\x010");
        assert_eq!(read(&written(&dir, "odd.iso", &disc)).unwrap().game_id, "GA-E-0");
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn ciso(disc: &[u8], block: usize, left_out: &[usize]) -> Vec<u8> {
        let mut file = vec![0; CISO_HEADER];
        file[..4].copy_from_slice(b"CISO");
        file[4..8].copy_from_slice(&(block as u32).to_le_bytes());
        for (index, data) in disc.chunks(block).enumerate() {
            if !left_out.contains(&index) {
                file[8 + index] = 1;
                file.extend_from_slice(data);
            }
        }
        file
    }

    #[test]
    fn a_ciso_reads_through_its_block_map() {
        let dir = scratch("ciso");
        // Block 3, 0x6000 to 0x8000, is all zeros and left out.
        let path = written(&dir, "melee.ciso", &ciso(&gamecube_disc(), 0x2000, &[3]));
        expect_gamecube(&read(&path).unwrap(), DISC_LEN as u64);
        expect_banner(gamecube_banner_png(&path));
        let Image::Ciso(mut image) = Image::open(&path).unwrap() else { panic!("not a CISO") };
        let mut bytes = [1; 0x20];
        image.read_at(0x6FF0, &mut bytes).unwrap();
        assert_eq!(bytes, [0; 0x20]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn gcz(disc: &[u8], block: usize, kept: &[usize]) -> Vec<u8> {
        let (mut starts, mut data) = (Vec::new(), Vec::new());
        for (index, piece) in disc.chunks(block).enumerate() {
            let start = data.len() as u64;
            if kept.contains(&index) {
                starts.push(start | KEPT_AS_IT_IS);
                data.extend_from_slice(piece);
            } else {
                let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                encoder.write_all(piece).unwrap();
                data.extend(encoder.finish().unwrap());
                starts.push(start);
            }
        }
        let mut file = vec![0x01, 0xC0, 0x0B, 0xB1, 0, 0, 0, 0];
        file.extend_from_slice(&(data.len() as u64).to_le_bytes());
        file.extend_from_slice(&(disc.len() as u64).to_le_bytes());
        file.extend_from_slice(&(block as u32).to_le_bytes());
        file.extend_from_slice(&(starts.len() as u32).to_le_bytes());
        starts.iter().for_each(|start| file.extend_from_slice(&start.to_le_bytes()));
        file.extend(vec![0; starts.len() * 4]);
        file.extend(data);
        file
    }

    #[test]
    fn a_gcz_reads_zlib_blocks_and_kept_ones() {
        let dir = scratch("gcz");
        let path = written(&dir, "melee.gcz", &gcz(&gamecube_disc(), 0x4000, &[1]));
        expect_gamecube(&read(&path).unwrap(), DISC_LEN as u64);
        expect_banner(gamecube_banner_png(&path));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The test disc as a demo disc's TGC: a 0x8000-byte TGC header, the
    /// disc's own DOL and table offsets spoiled, and the files listed at a
    /// made-up place 0xC000 past where they are.
    fn tgc(disc: &[u8]) -> Vec<u8> {
        const AT: u32 = 0x8000;
        let mut disc = disc.to_vec();
        disc[0x420..0x428].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0xDE, 0xAD, 0xBE, 0xEF]);
        let table_len = be32(&disc, FST_SIZE_AT) as usize;
        for entry in disc[FST_AT..FST_AT + table_len].chunks_exact_mut(FST_ENTRY).skip(1).take(3) {
            if entry[0] == 0 {
                let listed = be32(entry, 4) + 0xC000;
                entry[4..8].copy_from_slice(&listed.to_be_bytes());
            }
        }
        let mut file = vec![0; AT as usize];
        let fields = [
            (0x08, AT),
            (0x10, AT + FST_AT as u32),
            (0x14, table_len as u32),
            (0x1C, AT + 0x2800),
            (0x24, AT + BANNER_ON_DISC as u32),
            (0x34, 0x10000),
        ];
        file[..4].copy_from_slice(&[0xAE, 0x0F, 0x38, 0xA2]);
        for (at, value) in fields {
            file[at..at + 4].copy_from_slice(&value.to_be_bytes());
        }
        file.extend(disc);
        file
    }

    #[test]
    fn a_tgc_puts_its_disc_header_and_file_table_right() {
        let dir = scratch("tgc");
        let path = written(&dir, "demo.tgc", &tgc(&gamecube_disc()));
        expect_gamecube(&read(&path).unwrap(), DISC_LEN as u64);
        expect_banner(gamecube_banner_png(&path));
        let mut image = Image::open(&path).unwrap();
        let offsets = image.read_vec(0x420, 8).unwrap();
        assert_eq!((be32(&offsets, 0), be32(&offsets, 4)), (0x2800, FST_AT as u32));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The disc in a WBFS of 0x8000-byte blocks: its first block in the
    /// file's block 18, its second left out, its third in block 19.
    fn wbfs(disc: &[u8]) -> Vec<u8> {
        const BLOCK: usize = 0x8000;
        let mut file = vec![0; 20 * BLOCK];
        file[..4].copy_from_slice(b"WBFS");
        file[4..8].copy_from_slice(&((20 * BLOCK / 512) as u32).to_be_bytes());
        file[8] = 9;
        file[9] = 15;
        file[12] = 1;
        file[512..512 + 256].copy_from_slice(&disc[..256]);
        let table = 512 + 256;
        file[table..table + 2].copy_from_slice(&18u16.to_be_bytes());
        file[table + 4..table + 6].copy_from_slice(&19u16.to_be_bytes());
        file[18 * BLOCK..19 * BLOCK].copy_from_slice(&disc[..BLOCK]);
        file[19 * BLOCK..20 * BLOCK].copy_from_slice(&disc[2 * BLOCK..3 * BLOCK]);
        file
    }

    #[test]
    fn a_wbfs_reads_through_its_block_table_even_split_in_two() {
        let dir = scratch("wbfs");
        let mut disc = wii_disc(0x18000);
        disc[0x10000..0x18000].fill(0x77);
        let file = wbfs(&disc);
        let whole = written(&dir, "spyro.wbfs", &file);
        expect_wii(&read(&whole).unwrap(), 0x18000);

        let split = dir.join("split");
        let first = written(&split, "spyro.wbfs", &file[..0x48000]);
        written(&split, "spyro.wbf1", &file[0x48000..]);
        expect_wii(&read(&first).unwrap(), 0x18000);
        let mut image = Image::open(&first).unwrap();
        assert_eq!(image.read_vec(0x10000, 4).unwrap(), [0x77; 4], "read across the split");

        // The files must come to the size the header gives.
        let short = written(&dir, "short.wbfs", &file[..file.len() - 512]);
        assert_eq!(read(&short).unwrap_err(), DAMAGED);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_plain_image_split_into_parts_counts_them_all() {
        let dir = scratch("parts");
        let disc = gamecube_disc();
        let first = written(&dir, "melee.part0.iso", &disc[..0x5000]);
        written(&dir, "melee.part1.iso", &disc[0x5000..0xA000]);
        written(&dir, "melee.part2.iso", &disc[0xA000..]);
        expect_gamecube(&read(&first).unwrap(), DISC_LEN as u64);
        // The banner runs from the first part into the second.
        expect_banner(gamecube_banner_png(&first));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A WIA or RVZ file of just its two headers, holding the first 0x80
    /// bytes of `disc` and saying the disc is `disc_size` long. Nothing
    /// past the headers is read, so nothing follows them.
    fn wia_file(rvz: bool, compression: u32, chunk: u32, disc: &[u8], disc_size: u64) -> Vec<u8> {
        let mut second = vec![0; WIA_HEADER_2];
        let gamecube = platform_of(disc) == Some(Platform::GameCube);
        second[0..4].copy_from_slice(&(if gamecube { 1u32 } else { 2 }).to_be_bytes());
        second[4..8].copy_from_slice(&compression.to_be_bytes());
        second[0x0C..0x10].copy_from_slice(&chunk.to_be_bytes());
        second[0x10..0x90].copy_from_slice(&disc[..0x80]);
        second[0xA0..0xB4].copy_from_slice(&Sha1::digest(b""));

        let mut first = vec![0; WIA_HEADER_1];
        first[..4].copy_from_slice(if rvz { b"RVZ\x01" } else { b"WIA\x01" });
        first[4..8].copy_from_slice(&WIA_VERSION.to_be_bytes());
        first[8..12].copy_from_slice(&(if rvz { RVZ_OLDEST } else { 0x0009_0000 }).to_be_bytes());
        first[0x0C..0x10].copy_from_slice(&(WIA_HEADER_2 as u32).to_be_bytes());
        first[0x10..0x24].copy_from_slice(&Sha1::digest(&second));
        first[0x24..0x2C].copy_from_slice(&disc_size.to_be_bytes());
        first[0x2C..0x34].copy_from_slice(&((WIA_HEADER_1 + WIA_HEADER_2) as u64).to_be_bytes());
        let hash = Sha1::digest(&first[..0x34]);
        first[0x34..0x48].copy_from_slice(&hash);
        [first, second].concat()
    }

    #[test]
    fn a_gamecube_wia_or_rvz_gives_its_game_but_no_banner() {
        let dir = scratch("gc-wia");
        let disc = gamecube_disc();
        let wia = wia_file(false, PURGE, 0x20_0000, &disc, DISC_LEN as u64);
        let path = written(&dir, "melee.wia", &wia);
        // The region in bi2.bin lies past the header, so the E in the id
        // says the title isn't Japanese.
        expect_gamecube(&read(&path).unwrap(), DISC_LEN as u64);
        assert_eq!(gamecube_banner_png(&path), None);
        let rvz = written(&dir, "melee.rvz", &wia_file(true, ZSTD, 0x20000, &disc, DISC_LEN as u64));
        expect_gamecube(&read(&rvz).unwrap(), DISC_LEN as u64);
        assert_eq!(gamecube_banner_png(&rvz), None);

        // A damaged first header is refused, as Dolphin refuses it.
        let mut spoiled = wia.clone();
        spoiled[0x24] ^= 1;
        assert_eq!(read(&written(&dir, "spoiled.wia", &spoiled)).unwrap_err(), DAMAGED);
        // So is a file that isn't the size it says.
        let mut longer = wia.clone();
        longer.push(0);
        assert_eq!(read(&written(&dir, "longer.wia", &longer)).unwrap_err(), DAMAGED);
        // And a compression RVZ doesn't have.
        let purged = written(&dir, "purged.rvz", &wia_file(true, PURGE, 0x20000, &disc, DISC_LEN as u64));
        assert_eq!(read(&purged).unwrap_err(), DAMAGED);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_wii_wia_or_rvz_is_known_from_its_header() {
        let dir = scratch("wii-wia");
        let disc = wii_disc(HEADER_SIZE);
        // LZMA, and the real size of a single-layer Wii disc.
        let wia = written(&dir, "spyro.wia", &wia_file(false, 3, 0x20_0000, &disc, 4_699_979_776));
        expect_wii(&read(&wia).unwrap(), 4_699_979_776);
        let rvz = written(&dir, "spyro.rvz", &wia_file(true, ZSTD, 0x20000, &disc, 4_699_979_776));
        expect_wii(&read(&rvz).unwrap(), 4_699_979_776);
        assert_eq!(gamecube_banner_png(&rvz), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_extracted_gamecube_folder_is_read_like_a_disc() {
        let dir = scratch("gc-folder");
        let disc = gamecube_disc();
        written(&dir, "sys/boot.bin", &disc[..HEADER_SIZE]);
        written(&dir, "sys/bi2.bin", &disc[HEADER_SIZE..HEADER_SIZE + 0x2000]);
        written(&dir, "sys/main.dol", &[0; 0x100]);
        written(&dir, "files/audio/a.dsp", &[0; 0x10]);
        written(&dir, "files/OPENING.BNR", &banner(b"BNR2"));
        // Read from its header alone, never measured.
        expect_gamecube(&read_folder(&dir).unwrap(), 0);
        expect_gamecube(&read(&dir).unwrap(), 0);
        expect_banner(gamecube_banner_png(&dir));

        // Without main.dol it is only a folder.
        std::fs::remove_file(dir.join("sys/main.dol")).unwrap();
        assert_eq!(read_folder(&dir), None);
        assert_eq!(read(&dir).unwrap_err(), NOT_A_DISC_FOLDER);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_extracted_wii_disc_is_read_from_its_data_partition() {
        let dir = scratch("wii-folder");
        // DolphinTool's layout: one folder per partition. The partition's
        // own header has another title, to show the plain one wins.
        written(&dir, "UPDATE/sys/boot.bin", &header(b"0000P1", 0, 0, b"Update", Platform::Wii));
        written(&dir, "UPDATE/sys/main.dol", &[0; 0x10]);
        written(&dir, "DATA/sys/boot.bin", &header(WII_ID, 0, 2, b"Partition", Platform::Wii));
        written(&dir, "DATA/sys/main.dol", &[0; 0x10]);
        written(&dir, "DATA/disc/header.bin", &wii_disc(HEADER_SIZE)[..0x100]);
        written(&dir, "DATA/files/opening.bnr", &[0; 0x20]);
        expect_wii(&read_folder(&dir).unwrap(), 0);
        // The partition's folder on its own reads the same game.
        let data = dir.join("DATA");
        assert_eq!(read_folder(&data).unwrap().game_id, "SSPP52");
        assert_eq!(gamecube_banner_png(&dir), None);
        // The picture reader is given the partition's folder either way.
        assert_eq!(partition_root(&dir), Some(data.clone()));
        assert_eq!(partition_root(&data), Some(data.clone()));
        assert_eq!(partition_root(&data.join("sys").join("boot.bin")), None, "a file");

        // P0 is the game partition's other name.
        std::fs::rename(&data, dir.join("P0")).unwrap();
        assert_eq!(read_folder(&dir).unwrap().title, WII_TITLE);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn damaged_or_cut_images_are_refused_in_plain_words() {
        let dir = scratch("damaged");
        let disc = gamecube_disc();
        // A disc header cut before its title ends.
        let short = written(&dir, "short.iso", &disc[..0x40]);
        assert_eq!(read(&short).unwrap_err(), DAMAGED);
        // A GCZ whose block table is cut off.
        let gcz_file = gcz(&disc, 0x4000, &[]);
        let cut = written(&dir, "cut.gcz", &gcz_file[..0x30]);
        assert_eq!(read(&cut).unwrap_err(), DAMAGED);
        // An RVZ cut short.
        let rvz = wia_file(true, ZSTD, 0x20000, &disc, DISC_LEN as u64);
        let cut = written(&dir, "cut.rvz", &rvz[..rvz.len() - 0x10]);
        let message = read(&cut).unwrap_err();
        assert_eq!(message, DAMAGED);
        assert!(!message.contains(&*dir.to_string_lossy()), "no path in a message");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn anything_else_is_not_a_disc() {
        let dir = scratch("not-disc");
        let text = written(&dir, "notes.iso", &b"Just some notes. ".repeat(100));
        assert_eq!(read(&text).unwrap_err(), NOT_A_DISC);
        let empty = written(&dir, "empty.iso", &[]);
        assert_eq!(read(&empty).unwrap_err(), NOT_A_DISC);
        assert_eq!(gamecube_banner_png(&text), None);
        assert_eq!(read(&dir.join("missing.iso")).unwrap_err(), UNOPENED);
        // A channel is told apart by its name before anything is opened.
        assert_eq!(read(&dir.join("channel.wad")).unwrap_err(), CHANNEL);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rgb5a3_pixels_widen_as_the_hardware_does() {
        // Opaque: 1 01010 01101 11111. Five bits widen by repeating their
        // top bits: 01010 to 01010010.
        assert_eq!(rgb5a3_pixel(0xA9BF), [82, 107, 255, 255]);
        // See-through: 0 101 1100 0011 0000. Alpha 101 widens to 10110110,
        // and each colour's four bits are repeated.
        assert_eq!(rgb5a3_pixel(0x5C30), [0xCC, 0x33, 0x00, 0xB6]);
        assert_eq!(rgb5a3_pixel(0x0000), [0, 0, 0, 0]);
        assert_eq!(rgb5a3_pixel(0x7FFF), [255, 255, 255, 255]);
        assert_eq!(rgb5a3_pixel(0x8000), [0, 0, 0, 255]);
    }

    #[test]
    fn rgb5a3_tiles_are_4_by_4_left_to_right_then_down() {
        // 8 by 8, numbered in the order stored: tile by tile, each row of a
        // tile in turn.
        let stored: Vec<u8> = (0..64u16).flat_map(|n| (0x8000 | n).to_be_bytes()).collect();
        let picture = decode_rgb5a3(&stored, 8, 8).unwrap();
        let blue = |x: usize, y: usize| picture.rgba[(y * 8 + x) * 4 + 2];
        let widened = |n: u8| (n << 3) | (n >> 2);
        assert_eq!(blue(0, 0), widened(0));
        assert_eq!(blue(3, 0), widened(3));
        assert_eq!(blue(0, 1), widened(4));
        assert_eq!(blue(4, 0), widened(16));
        assert_eq!(blue(0, 4), widened(32 % 32));
        assert_eq!(picture.rgba[(4 * 8) * 4 + 1], widened(1), "the third tile's first pixel");
        assert_eq!(blue(7, 7), widened(63 % 32));
        assert_eq!(decode_rgb5a3(&stored[..10], 8, 8), None);
    }

    #[test]
    fn a_save_banner_becomes_a_png() {
        let dir = scratch("save");
        let mut pixels = vec![0x8000; SAVE_BANNER_WIDTH * SAVE_BANNER_HEIGHT];
        pixels[0] = 0xFC00;
        pixels[SAVE_BANNER_WIDTH * 63 + 191] = 0x5C30;
        let mut file = b"WIBN".to_vec();
        file.resize(SAVE_HEADER, 0);
        file.extend(tiled(&pixels, SAVE_BANNER_WIDTH, SAVE_BANNER_HEIGHT));
        file.extend(vec![0; SAVE_ICON_SIZE]);
        let path = written(&dir, "banner.bin", &file);
        let (width, height, rgba) = png_pixels(&save_banner_png(&path).unwrap());
        assert_eq!((width, height), (192, 64));
        assert_eq!(pixel(&rgba, width, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&rgba, width, 191, 63), [0xCC, 0x33, 0x00, 0xB6]);
        // Without room for an icon, Dolphin doesn't take it.
        let short = written(&dir, "short.bin", &file[..file.len() - 1]);
        assert_eq!(save_banner_png(&short), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_wii_games_nand_folder_is_its_id_in_hex() {
        assert_eq!(nand_title_folder("SSPP52").as_deref(), Some("00010000/53535050"));
        assert_eq!(nand_title_folder("RZDJ01").as_deref(), Some("00010000/525a444a"));
        assert_eq!(nand_title_folder("SSP"), None);
        assert_eq!(nand_title_folder("SS-P52"), None);
    }

    /// Reads a real disc image, which is the only proof the reader works on
    /// one. Game files are not committed, so point this at one to run it:
    ///   set OMOIO_TEST_WII_DISC=D:\games\spyro.rvz
    ///   cargo test --lib real_disc -- --ignored --nocapture
    #[test]
    #[ignore = "needs OMOIO_TEST_WII_DISC pointing at a real disc image"]
    fn reads_a_real_disc() {
        let path = PathBuf::from(std::env::var("OMOIO_TEST_WII_DISC").expect("set OMOIO_TEST_WII_DISC"));
        let disc = read(&path).unwrap();
        println!("{disc:#?}");
        if disc.platform == Platform::Wii {
            println!("NAND folder: {:?}", nand_title_folder(&disc.game_id));
        } else {
            let banner = gamecube_banner_png(&path);
            println!("Banner: {:?} bytes of PNG", banner.map(|png| png.len()));
        }
    }
}
