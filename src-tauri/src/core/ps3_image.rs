//! A PS3 disc image (.iso), told apart from any other ISO by the rules
//! RPCS3 boots one by, so Omoio doesn't import an image RPCS3 then refuses
//! for its shape, and read where it lies through its ISO 9660 file system
//! (core/iso9660.rs).
//!
//! RPCS3's rules, from Loader/ISO.cpp (RPCS3/rpcs3 c365ea7, the source of
//! build 0.0.43-20410, read 27 October 2026): "CD001" right after the first
//! volume descriptor's type (`is_iso_file`); the image's first four bytes, a
//! big-endian count of plain regions from 1 to 127, or "It's non-PS3ISO"
//! (`iso_file_decryption::init`), the regions alternating plain and
//! encrypted; and a disc boots only with a PS3_DISC.SFB that begins ".SFB"
//! and PS3_GAME's PARAM.SFO (Emu/System.cpp, `Emulator::Load` and
//! `IsValidSfb`). The file system lies in the first, plain region, as
//! RPCS3 reads it before any key is set up (`iso_archive`, ISO.cpp lines
//! 1328 to 1366). PARAM.SFO most likely lies in a plain region too, as two
//! tools read it from encrypted images without a key (LibIRD, IRD.cs lines
//! 650 to 680, Deterous/LibIRD 1f103eb; opticaldiscs-rs, gameid.rs lines
//! 238 to 242, danifunker/opticaldiscs-rs 3e4f414). An image where it
//! doesn't is refused as unreadable.
//! An image with more than one region still needs its key when LIC.DAT or
//! EBOOT.BIN doesn't begin as it does in the clear, "PS3LICDA" and "SCE"
//! (`read_magic_blocks`, `is_any_block_encrypted`,
//! `iso_file_decryption::get_key_status`). Only those few leading bytes are
//! compared: Omoio never decrypts anything.

use crate::core::iso9660;
use std::io::{Read, Seek, SeekFrom};

/// The most of PARAM.SFO read, far past any real one's few hundred bytes.
const LARGEST_SFO: u64 = 64 << 10;

/// The most of ICON0.PNG read. The PS3 draws it at 320 by 176.
const LARGEST_ICON: u64 = 4 << 20;

/// The files whose first bytes say whether the game's files are readable,
/// with those bytes as they are in the clear (`read_magic_blocks`).
const IN_THE_CLEAR: [(&[&str], &[u8]); 2] = [
    (&["PS3_GAME", "LICDIR", "LIC.DAT"], b"PS3LICDA"),
    (&["PS3_GAME", "USRDIR", "EBOOT.BIN"], b"SCE"),
];

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

/// How many plain regions the image's first sector says it has.
fn regions(image: &mut (impl Read + Seek)) -> Option<u32> {
    let mut count = [0u8; 4];
    image.seek(SeekFrom::Start(0)).ok()?;
    image.read_exact(&mut count).ok()?;
    Some(u32::from_be_bytes(count))
}

/// Whether this is a PS3 disc image RPCS3 boots. Reads a few sectors, so
/// it is cheap enough to ask of every file picked.
pub fn is_ps3(image: &mut (impl Read + Seek)) -> bool {
    regions(image).is_some_and(|count| (1..=127).contains(&count))
        && iso9660::locate(image, &["PS3_DISC.SFB"])
            .and_then(|sfb| iso9660::head(image, sfb, 4))
            .is_some_and(|start| start == b".SFB")
        && iso9660::locate(image, &["PS3_GAME", "PARAM.SFO"]).is_some_and(|sfo| !sfo.directory)
}

/// Whether an ISO holds a PS3 game's files, PS3_DISC.SFB or PS3_GAME's
/// PARAM.SFO, whether or not RPCS3 would boot it (`is_ps3`).
pub fn has_ps3_files(image: &mut (impl Read + Seek)) -> bool {
    iso9660::locate(image, &["PS3_DISC.SFB"]).is_some() || iso9660::locate(image, &["PS3_GAME", "PARAM.SFO"]).is_some()
}

/// Whether the game's files still need the disc's key to be read: an image
/// with an encrypted region, and one of LIC.DAT and EBOOT.BIN in it that
/// doesn't begin as it does in the clear. An image with neither needs none,
/// as RPCS3 finds, and nor does one whose only region is plain.
pub fn encrypted(image: &mut (impl Read + Seek)) -> bool {
    regions(image).is_some_and(|count| count > 1)
        && IN_THE_CLEAR.iter().any(|(path, clear)| {
            iso9660::locate(image, path)
                .and_then(|file| iso9660::head(image, file, clear.len()))
                .is_some_and(|start| start != *clear)
        })
}

/// Whether a 3k3y drive wrote the image encrypted, with the disc's key data
/// inside it: "Encrypted 3K BLD" at 0xF70 (`iso_file_decryption::init`,
/// Loader/ISO.cpp, RPCS3/rpcs3 c365ea7). Such a dump came with no key file.
pub fn encrypted_by_3k3y(image: &mut (impl Read + Seek)) -> bool {
    let mut mark = [0u8; 16];
    image.seek(SeekFrom::Start(0xF70)).is_ok() && image.read_exact(&mut mark).is_ok() && &mark == b"Encrypted 3K BLD"
}

/// The game's PARAM.SFO.
pub fn param_sfo(image: &mut (impl Read + Seek)) -> Option<Vec<u8>> {
    let file = iso9660::locate(image, &["PS3_GAME", "PARAM.SFO"])?;
    iso9660::read(image, file, LARGEST_SFO)
}

/// The game's ICON0.PNG, the library tile's picture, when it reads as a
/// PNG. In an image whose files are encrypted it may not.
pub fn icon(image: &mut (impl Read + Seek)) -> Option<Vec<u8>> {
    let file = iso9660::locate(image, &["PS3_GAME", "ICON0.PNG"])?;
    iso9660::read(image, file, LARGEST_ICON).filter(|bytes| bytes.starts_with(PNG))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::core::iso9660::tests::image;
    use std::io::Cursor;

    /// A PS3 disc image as a dumping tool writes one: `regions` plain
    /// regions in its first sector, then the file system with the disc's
    /// PS3_DISC.SFB and these files.
    fn with_regions(regions: u32, files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut all = vec![("PS3_DISC.SFB", b".SFB\0\0\0\x01".as_slice())];
        all.extend_from_slice(files);
        let mut bytes = image(&all);
        bytes[..4].copy_from_slice(&regions.to_be_bytes());
        bytes
    }

    /// A retail disc's image: plain, encrypted and plain again.
    pub(crate) fn disc(files: &[(&str, &[u8])]) -> Vec<u8> {
        with_regions(2, files)
    }

    /// Any PARAM.SFO's bytes: this module only finds and hands them over.
    fn sfo() -> Vec<u8> {
        b"\0PSF\x01\x01\0\0 BLUS30779".to_vec()
    }

    #[test]
    fn a_ps3_disc_is_told_by_its_regions_its_sfb_and_its_param_sfo() {
        let sfo = sfo();
        let mut ps3 = Cursor::new(disc(&[("PS3_GAME/PARAM.SFO", &sfo)]));
        assert!(is_ps3(&mut ps3));
        assert_eq!(param_sfo(&mut ps3).unwrap(), sfo);

        // An ISO a PC tool made, with no region count, isn't one RPCS3 boots.
        let mut made_on_a_pc = Cursor::new(image(&[("PS3_DISC.SFB", b".SFB"), ("PS3_GAME/PARAM.SFO", &sfo)]));
        assert!(!is_ps3(&mut made_on_a_pc));
        assert!(!is_ps3(&mut Cursor::new(with_regions(128, &[("PS3_GAME/PARAM.SFO", &sfo)]))));
        // Nor is one without a PS3_DISC.SFB that reads as one.
        let mut no_sfb = image(&[("PS3_GAME/PARAM.SFO", &sfo)]);
        no_sfb[..4].copy_from_slice(&1u32.to_be_bytes());
        assert!(!is_ps3(&mut Cursor::new(no_sfb)));
        let mut bad_sfb = image(&[("PS3_DISC.SFB", b"SFB."), ("PS3_GAME/PARAM.SFO", &sfo)]);
        bad_sfb[..4].copy_from_slice(&1u32.to_be_bytes());
        assert!(!is_ps3(&mut Cursor::new(bad_sfb)));
        // Nor a disc with regions and no PS3_GAME.
        assert!(!is_ps3(&mut Cursor::new(disc(&[("SYSTEM.CNF", b"BOOT2 = cdrom0:\\SLUS_200.62;1")]))));
        assert!(!is_ps3(&mut Cursor::new(vec![0u8; 64])));
    }

    #[test]
    fn a_disc_whose_files_dont_begin_as_in_the_clear_is_encrypted() {
        let sfo = sfo();
        let game = |lic: &[u8], eboot: &[u8]| {
            Cursor::new(disc(&[
                ("PS3_GAME/PARAM.SFO", &sfo),
                ("PS3_GAME/LICDIR/LIC.DAT", lic),
                ("PS3_GAME/USRDIR/EBOOT.BIN", eboot),
            ]))
        };
        assert!(!encrypted(&mut game(b"PS3LICDA and the rest", b"SCE\0\0\0\x02")));
        assert!(encrypted(&mut game(b"\x9a\x13 scrambled", b"SCE\0\0\0\x02")));
        assert!(encrypted(&mut game(b"PS3LICDA and the rest", b"\x51\x07\xee")));
        // An install disc has no EBOOT.BIN, and LIC.DAT alone says it.
        let install = disc(&[("PS3_GAME/PARAM.SFO", &sfo), ("PS3_GAME/LICDIR/LIC.DAT", b"PS3LICDA")]);
        assert!(!encrypted(&mut Cursor::new(install)));
        // With neither file there is nothing to read with a key.
        assert!(!encrypted(&mut Cursor::new(disc(&[("PS3_GAME/PARAM.SFO", &sfo)]))));
        // An image whose only region is plain has nothing encrypted, whatever
        // its files begin with.
        let one_region = with_regions(1, &[("PS3_GAME/PARAM.SFO", &sfo), ("PS3_GAME/USRDIR/EBOOT.BIN", b"\x7fELF")]);
        assert!(!encrypted(&mut Cursor::new(one_region)));
    }

    #[test]
    fn the_icon_is_taken_only_when_it_reads_as_a_png() {
        let png = [PNG, b"rest of the picture"].concat();
        let mut clear = Cursor::new(disc(&[("PS3_GAME/PARAM.SFO", &sfo()), ("PS3_GAME/ICON0.PNG", &png)]));
        assert_eq!(icon(&mut clear).unwrap(), png);
        let mut scrambled = Cursor::new(disc(&[("PS3_GAME/PARAM.SFO", &sfo()), ("PS3_GAME/ICON0.PNG", b"\x13\x37")]));
        assert_eq!(icon(&mut scrambled), None);
        assert_eq!(icon(&mut Cursor::new(disc(&[("PS3_GAME/PARAM.SFO", &sfo())]))), None);
    }
}
