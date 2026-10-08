//! Where Dolphin keeps a Wii or GameCube game's saved games, for Omoio's
//! backups of them (crate::saves). Read in Dolphin's own source at the
//! release Omoio installs (Dolphin 2609a, read 8 October 2026); what was
//! found is in docs/what-we-verified.md.
//!
//! A Wii game saves into Dolphin's copy of the Wii's storage, in its title's
//! `data` folder, plain, as the game wrote it: `Wii/title/00010000/<the
//! first four characters of the game id, in hex>/data` (see
//! `disc::nand_title_folder`; Common/NandPaths.cpp, GetTitleDataPath). That
//! folder is backed up whole, kept under the title's upper half.
//!
//! A GameCube game saves to the memory card in slot A, which Dolphin keeps
//! by default as a folder of `.gci` files, one per save, shared by every
//! game of the same region: `GC/<USA, EUR or JAP>/Card A`
//! (Core/Config/MainSettings.cpp, MAIN_SLOT_A and GetGCIFolderPath). Dolphin
//! knows which saves are the game's by the game code in each file's first
//! four bytes, not by the file's name (Core/HW/GCMemcard/
//! GCMemcardDirectory.cpp, the GCMemcardDirectory constructor), and so does
//! Omoio. A backup keeps them under the region's folder name. A card kept
//! as one .raw file, which Dolphin uses only when its settings say so, is
//! not looked in.

use super::disc;
use crate::backends::SaveFolder;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The two kinds of Wii title a disc game's data can be under: a game, and
/// a game that also installs a channel (`disc::nand_title_folder`).
const WII_TITLE_KINDS: [&str; 2] = ["00010000", "00010004"];

/// The folders Dolphin keeps each region's GameCube memory cards in, by the
/// names it gives them unless told otherwise (Common/CommonPaths.h;
/// Core/Config/MainSettings.cpp, GetDirectoryForRegion). A Korean game uses
/// the Japanese one, and a disc of no region the one for the region
/// Dolphin falls back to, so every one is looked in.
const GAMECUBE_REGIONS: [&str; 3] = ["USA", "EUR", "JAP"];

/// Where a GameCube save names its game, its maker and itself: the game
/// code in its first four bytes, the maker's two after it, and its own name
/// in 32 bytes from 0x08 (Core/HW/GCMemcard/GCMemcard.h, DEntry).
const GAME_CODE: std::ops::Range<usize> = 0..4;
const MAKER_CODE: std::ops::Range<usize> = 4..6;
const SAVE_NAME: std::ops::Range<usize> = 8..0x28;

/// The Wii game's `data` folder, under each kind of title it is found as,
/// when the game has saved something into it. `title_id` is the library's,
/// which starts with the game id.
pub fn wii_folders(user: &Path, title_id: &str) -> Vec<SaveFolder> {
    WII_TITLE_KINDS
        .iter()
        .filter_map(|kind| {
            let title = wii_folder(user, title_id, kind)?;
            let data = title.join("data");
            // Dolphin makes the folder each time the game starts
            // (`ESDevice::DIVerify`, Core/IOS/ES/ES.cpp), so only one with
            // something in it holds a save.
            has_files(&data).then(|| SaveFolder { kept_as: kind.to_string(), path: title, saves: vec![data] })
        })
        .collect()
}

/// The title's folder under the kind of title `kept_as`, that a backup's
/// `data` goes back into.
pub fn wii_folder(user: &Path, title_id: &str, kept_as: &str) -> Option<PathBuf> {
    if !WII_TITLE_KINDS.contains(&kept_as) {
        return None;
    }
    let folder = disc::nand_title_folder(title_id)?;
    let (_, low) = folder.split_once('/')?;
    Some(user.join("Wii").join("title").join(kept_as).join(low))
}

/// The GameCube game's saves on each region's card in slot A.
pub fn gamecube_folders(user: &Path, title_id: &str) -> Vec<SaveFolder> {
    let Some(code) = title_id.get(GAME_CODE).filter(|code| code.bytes().all(|b| b.is_ascii_alphanumeric())) else {
        return Vec::new();
    };
    GAMECUBE_REGIONS
        .iter()
        .filter_map(|region| {
            let card = card_folder(user, region);
            let mut saves: Vec<PathBuf> = std::fs::read_dir(&card)
                .ok()?
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| is_save_file(path) && header(path).is_some_and(|h| h[GAME_CODE] == *code.as_bytes()))
                .collect();
            if saves.is_empty() {
                return None;
            }
            saves.sort();
            Some(SaveFolder { kept_as: region.to_string(), path: card, saves })
        })
        .collect()
}

/// The card folder of the region `kept_as`, that a backup's saves go back
/// into.
pub fn gamecube_folder(user: &Path, kept_as: &str) -> Option<PathBuf> {
    GAMECUBE_REGIONS.contains(&kept_as).then(|| card_folder(user, kept_as))
}

/// Whether two GameCube save files hold the same save: the same game code,
/// maker code and save name, the name read up to its first NUL, as Dolphin
/// compares them (Core/HW/GCMemcard/GCMemcardUtils.cpp, HasSameIdentity).
/// Dolphin loads only the first of two such files on a card, by file name,
/// so putting a backup back must not leave the newer one beside it.
pub fn same_gamecube_save(kept: &Path, saved: &Path) -> bool {
    match (identity(kept), identity(saved)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn card_folder(user: &Path, region: &str) -> PathBuf {
    user.join("GC").join(region).join("Card A")
}

/// Whether a file is one Dolphin loads from a card folder: one whose name
/// ends in `.gci`, in any case (Common/FileSearch.cpp, DoFileSearch). A
/// save the game deleted is renamed to end in `.gci.deleted`
/// (GCMemcardDirectory::FlushToFile) and is no longer one.
fn is_save_file(path: &Path) -> bool {
    path.is_file() && path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("gci"))
}

fn header(path: &Path) -> Option<[u8; 0x28]> {
    let mut header = [0; 0x28];
    std::fs::File::open(path).ok()?.read_exact(&mut header).ok()?;
    Some(header)
}

fn identity(path: &Path) -> Option<Vec<u8>> {
    if !is_save_file(path) {
        return None;
    }
    let header = header(path)?;
    let name = &header[SAVE_NAME];
    let name = &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())];
    Some([&header[GAME_CODE], &header[MAKER_CODE], name].concat())
}

fn has_files(dir: &Path) -> bool {
    std::fs::read_dir(dir).into_iter().flatten().flatten().any(|entry| {
        let path = entry.path();
        path.is_file() || (path.is_dir() && has_files(&path))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct User(PathBuf);

    impl User {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("omoio-dolphin-saves-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            User(root)
        }

        fn file(&self, at: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(at);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for User {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A save file as a card folder holds it: the 0x40-byte entry, then
    /// the save's blocks, here a few bytes standing in for them.
    fn gci(game: &str, maker: &str, name: &str) -> Vec<u8> {
        let mut bytes = vec![0; 0x40];
        bytes[GAME_CODE].copy_from_slice(game.as_bytes());
        bytes[MAKER_CODE].copy_from_slice(maker.as_bytes());
        bytes[6] = 0xFF;
        bytes[8..8 + name.len()].copy_from_slice(name.as_bytes());
        bytes.extend_from_slice(b"blocks");
        bytes
    }

    #[test]
    fn a_wii_games_save_is_its_titles_data_folder() {
        let user = User::new("wii");
        let title = user.0.join("Wii/title/00010000/53535050");
        std::fs::create_dir_all(title.join("data")).unwrap();
        assert!(wii_folders(&user.0, "SSPP52").is_empty(), "made at start, nothing saved yet");

        user.file("Wii/title/00010000/53535050/data/banner.bin", b"banner");
        user.file("Wii/title/00010000/53535050/content/title.tmd", b"not a save");
        user.file("Wii/title/00010000/534b5950/data/save.bin", b"another game's");

        let found = wii_folders(&user.0, "SSPP52");
        assert_eq!(
            found,
            [SaveFolder { kept_as: "00010000".to_string(), path: title.clone(), saves: vec![title.join("data")] }]
        );
        assert_eq!(wii_folders(&user.0, "SSPP52D2"), found, "a second disc saves with the first");
        assert_eq!(wii_folder(&user.0, "SSPP52", "00010000"), Some(title));
        assert_eq!(
            wii_folder(&user.0, "SSPP52", "00010004"),
            Some(user.0.join("Wii/title/00010004/53535050"))
        );
        assert_eq!(wii_folder(&user.0, "SSPP52", "EUR"), None);
        assert_eq!(wii_folder(&user.0, "SS-P52", "00010000"), None);
    }

    #[test]
    fn a_gamecube_games_saves_are_its_files_on_each_regions_card() {
        let user = User::new("gamecube");
        let eur = user.file("GC/EUR/Card A/01-GZLP-gczelda.gci", &gci("GZLP", "01", "gczelda"));
        let renamed = user.file("GC/EUR/Card A/Zelda 100%.GCI", &gci("GZLP", "01", "gczelda2"));
        let usa = user.file("GC/USA/Card A/01-GZLP-gczelda.gci", &gci("GZLP", "01", "gczelda"));
        user.file("GC/EUR/Card A/01-GALP-SuperSmashBros.gci", &gci("GALP", "01", "SuperSmashBros"));
        user.file("GC/EUR/Card A/01-GZLP-old.gci.deleted", &gci("GZLP", "01", "old"));
        user.file("GC/EUR/Card A/MC_SYSTEM_AREA", &[0; 0x40]);
        user.file("GC/EUR/Card A/short.gci", b"GZLP");

        let found = gamecube_folders(&user.0, "GZLP01");

        assert_eq!(
            found,
            [
                SaveFolder {
                    kept_as: "USA".to_string(),
                    path: user.0.join("GC/USA/Card A"),
                    saves: vec![usa],
                },
                SaveFolder {
                    kept_as: "EUR".to_string(),
                    path: user.0.join("GC/EUR/Card A"),
                    saves: vec![eur, renamed],
                },
            ]
        );
        assert!(gamecube_folders(&user.0, "GXXP01").is_empty());
        assert_eq!(gamecube_folder(&user.0, "JAP"), Some(user.0.join("GC/JAP/Card A")));
        assert_eq!(gamecube_folder(&user.0, "00010000"), None);
    }

    #[test]
    fn gamecube_saves_are_the_same_by_what_they_hold_not_their_file_names() {
        let user = User::new("identity");
        let kept = user.file("backup/zelda.gci", &gci("GZLP", "01", "gczelda"));
        let mut padded = gci("GZLP", "01", "gczelda");
        padded[0x20] = b'x'; // after the name's first NUL, which doesn't count
        let same = user.file("card/01-GZLP-gczelda.gci", &padded);
        let other_slot = user.file("card/01-GZLP-gczelda2.gci", &gci("GZLP", "01", "gczelda2"));
        let other_maker = user.file("card/8P-GZLP-gczelda.gci", &gci("GZLP", "8P", "gczelda"));
        let deleted = user.file("card/01-GZLP-gczelda.gci.deleted", &gci("GZLP", "01", "gczelda"));

        assert!(same_gamecube_save(&kept, &same));
        assert!(!same_gamecube_save(&kept, &other_slot));
        assert!(!same_gamecube_save(&kept, &other_maker));
        assert!(!same_gamecube_save(&kept, &deleted));
        assert!(!same_gamecube_save(&kept, &user.0.join("card/missing.gci")));
    }
}
