//! Turning a folder or a disc image the user picked into a library entry.
//!
//! Games are referenced where they already are. An 8 GB dump is not copied,
//! so importing is immediate and a game on an external drive stays on it.

use crate::core::console::Console;
use crate::core::import_warning::Imported;
use crate::core::library::Game;
use crate::core::ps3_image;
use crate::core::sfo::{Category, Sfo};
use std::path::{Path, PathBuf};

/// What went wrong, in terms the interface can say out loud without showing a
/// path or an exception.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    NotAGameDump,
    UnreadableMetadata,
    IsAnUpdate,
    NoTitleId,
    /// A disc image whose PARAM.SFO couldn't be read.
    UnreadableImage,
    /// A disc image whose game files are still encrypted, with no key file
    /// beside it for RPCS3.
    EncryptedImage,
    /// A disc image a 3k3y drive wrote encrypted, with no key file beside it.
    EncryptedBy3k3y,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAGameDump => {
                write!(f, "This folder doesn't look like a PS3 game.")
            }
            Self::UnreadableMetadata => {
                write!(f, "Couldn't read PARAM.SFO. This folder doesn't look like a PS3 game dump.")
            }
            Self::IsAnUpdate => {
                write!(f, "This is a game update, not a game. Import the game itself first.")
            }
            Self::NoTitleId => {
                write!(f, "This dump is missing its title ID, so it can't be identified.")
            }
            Self::UnreadableImage => {
                write!(f, "Couldn't read PARAM.SFO in this disc image. The image may be incomplete.")
            }
            Self::EncryptedImage => {
                write!(
                    f,
                    "This disc image is encrypted. RPCS3 needs the key file that came with the dump, \
                     named like the image and ending .dkey, in the same folder."
                )
            }
            Self::EncryptedBy3k3y => {
                write!(
                    f,
                    "This disc image is encrypted the way a 3k3y drive writes it, which Omoio doesn't take. \
                     Import a decrypted image or the game's folder."
                )
            }
        }
    }
}

/// Where a dump keeps its PARAM.SFO. A disc dump puts it under PS3_GAME; an
/// installed PSN title keeps it at the top.
fn sfo_path(root: &Path) -> Option<PathBuf> {
    // A PS4 game keeps a param.sfo in sce_sys, which Windows also finds as
    // PARAM.SFO. No PS3 dump has that folder.
    if root.file_name().is_some_and(|name| name.eq_ignore_ascii_case("sce_sys")) {
        return None;
    }
    let disc = root.join("PS3_GAME").join("PARAM.SFO");
    if disc.is_file() {
        return Some(disc);
    }
    let hdd = root.join("PARAM.SFO");
    if hdd.is_file() {
        return Some(hdd);
    }
    None
}

/// The tile art shipped inside the dump itself. Using it costs nothing and
/// needs no metadata service, which is why it is the default before one exists.
fn icon_path(root: &Path) -> Option<PathBuf> {
    let candidates = [
        root.join("PS3_GAME").join("ICON0.PNG"),
        root.join("ICON0.PNG"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// The tile art of a dump or a disc image, read out of the image where it
/// lies.
pub fn icon(game: &Path) -> Option<Vec<u8>> {
    if is_image(game) {
        return ps3_image::icon(&mut std::fs::File::open(game).ok()?);
    }
    std::fs::read(icon_path(game)?).ok()
}

/// Whether a game's path is a disc image, by its name, the way Cemu and
/// Dolphin tell theirs, so one on a drive that isn't plugged in is still
/// known for one.
pub fn is_image_name(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("iso"))
}

/// Whether a game's path is a disc image: named as one and not a folder, as
/// a game's folder can be, say "Game.iso" unpacked from "Game.iso.7z". One on
/// a drive that isn't plugged in is still told by its name.
pub fn is_image(path: &Path) -> bool {
    is_image_name(path) && !path.is_dir()
}

/// Whether a file is a PS3 disc image RPCS3 boots (core/ps3_image.rs). A
/// Wii or GameCube .iso isn't, and is left to Dolphin.
fn is_ps3_image(path: &Path) -> bool {
    is_image_name(path) && path.is_file() && std::fs::File::open(path).is_ok_and(|mut file| ps3_image::is_ps3(&mut file))
}

/// The PS3 disc image picked, or the one in the folder picked, at the top or
/// one level down, which is where an archive unpacks it. Two or more is not
/// clear, so none, as Dolphin does with its images.
fn find_image(picked: &Path) -> Option<PathBuf> {
    if picked.is_file() {
        return is_ps3_image(picked).then(|| picked.to_path_buf());
    }
    let mut found = Vec::new();
    for entry in std::fs::read_dir(picked).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Ok(inner) = std::fs::read_dir(&path) {
                found.extend(inner.flatten().map(|inside| inside.path()).filter(|inside| is_ps3_image(inside)));
            }
        } else if is_ps3_image(&path) {
            found.push(path);
        }
    }
    (found.len() == 1).then(|| found.remove(0))
}

/// Whether the key file RPCS3 decrypts an image with is beside it: the
/// image's own name ending .dkey, or .key, in its folder (Loader/ISO.cpp,
/// `iso_file_decryption::check_type`, lines 458 to 493, RPCS3/rpcs3
/// c365ea7, read 27 October 2026). RPCS3 also looks in a redump folder in
/// its own data folder, which Omoio doesn't: the key file that came with a
/// dump is kept beside it. Only its name is looked at. Omoio never opens
/// it: RPCS3 does any decrypting.
pub(crate) fn has_key_file(image: &Path) -> bool {
    ["dkey", "key"].iter().any(|extension| image.with_extension(extension).is_file())
}

/// Archives usually extract into a folder of their own name, so the dump often
/// sits one level below whatever the user picked. Looking one level down costs
/// nothing and saves them from having to find the real root themselves.
fn find_dump_root(picked: &Path) -> Option<PathBuf> {
    if sfo_path(picked).is_some() {
        return Some(picked.to_path_buf());
    }
    let mut candidate = None;
    for entry in std::fs::read_dir(picked).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() && sfo_path(&path).is_some() {
            // Only unambiguous when there is exactly one dump inside.
            if candidate.is_some() {
                return None;
            }
            candidate = Some(path);
        }
    }
    candidate
}

/// Whether a folder is a PS3 dump, or a PS3 disc image, without measuring
/// it: an image is read only as far as a few sectors. Every emulator is
/// asked about every import, so this has to be cheap.
pub fn recognises(picked: &Path) -> bool {
    find_dump_root(picked).is_some() || find_image(picked).is_some()
}

/// The title id, name and version a PARAM.SFO gives.
fn read_sfo(bytes: &[u8]) -> Result<(String, String, Option<String>), Error> {
    let sfo = Sfo::parse(bytes).map_err(|_| Error::UnreadableMetadata)?;

    // A GD entry is an update or add-on. Letting one into the library would
    // put a second "LittleBigPlanet" beside the real one.
    if sfo.category() == Some(Category::GameData) {
        return Err(Error::IsAnUpdate);
    }

    let title_id = sfo.title_id().ok_or(Error::NoTitleId)?.to_string();
    let title = sfo
        .title()
        .map(str::to_string)
        .unwrap_or_else(|| title_id.clone());
    Ok((title_id, title, sfo.app_version().map(str::to_string)))
}

pub fn identify(picked: &Path) -> Result<Game, Error> {
    let Some(root) = find_dump_root(picked) else {
        return identify_image(&find_image(picked).ok_or(Error::NotAGameDump)?);
    };
    let sfo_file = sfo_path(&root).ok_or(Error::NotAGameDump)?;

    let bytes = std::fs::read(&sfo_file).map_err(|_| Error::UnreadableMetadata)?;
    let (title_id, title, version) = read_sfo(&bytes)?;

    Ok(Game {
        console: Console::Ps3,
        title_id,
        title,
        version,
        update_version: None,
        size_bytes: directory_size(&root),
        path: root,
    })
}

/// A PS3 disc image, kept where it is, as Wii and Wii U images are. One
/// whose game files are still encrypted is taken only with its key file
/// beside it, since without one RPCS3 refuses to boot it (Emu/System.cpp,
/// `check_disc_key`, lines 1179 to 1197, RPCS3/rpcs3 c365ea7, read 27
/// October 2026). A 3k3y image RPCS3 boots without one, by a key inside the
/// image, but what Bertram allowed is an image with its key file (27
/// October 2026), so Omoio doesn't take it. Whether a key file is the right
/// one only RPCS3 can say, by decrypting with it, which Omoio never does.
fn identify_image(image: &Path) -> Result<Game, Error> {
    let mut file = std::fs::File::open(image).map_err(|_| Error::UnreadableImage)?;
    if ps3_image::encrypted(&mut file) && !has_key_file(image) {
        // A 3k3y dump came with no key file, so it isn't asked for one.
        if ps3_image::encrypted_by_3k3y(&mut file) {
            return Err(Error::EncryptedBy3k3y);
        }
        return Err(Error::EncryptedImage);
    }
    let bytes = ps3_image::param_sfo(&mut file).ok_or(Error::UnreadableImage)?;
    let (title_id, title, version) = read_sfo(&bytes).map_err(|error| match error {
        Error::UnreadableMetadata => Error::UnreadableImage,
        other => other,
    })?;
    Ok(Game {
        console: Console::Ps3,
        title_id,
        title,
        version,
        update_version: None,
        size_bytes: file.metadata().map(|meta| meta.len()).unwrap_or(0),
        path: image.to_path_buf(),
    })
}

/// Where an archive keeps the dump's PARAM.SFO, by the names inside it. The
/// places `find_dump_root` and `sfo_path` look: at the top or one folder
/// down, under PS3_GAME for a disc or at the dump's own top for a PSN title,
/// and only when there is one dump to choose.
fn packed_sfo(names: &[String]) -> Option<&str> {
    let is = |part: &str, want: &str| part.eq_ignore_ascii_case(want);
    // The dump's folder, whether it is a disc, and the name to read.
    let mut found: Vec<(&str, bool, &str)> = Vec::new();
    for name in names {
        let parts: Vec<&str> = name.split(['/', '\\']).filter(|part| !part.is_empty()).collect();
        let (root, disc) = match parts.as_slice() {
            [game, sfo] if is(game, "PS3_GAME") && is(sfo, "PARAM.SFO") => ("", true),
            [root, game, sfo] if is(game, "PS3_GAME") && is(sfo, "PARAM.SFO") => (*root, true),
            [sfo] if is(sfo, "PARAM.SFO") => ("", false),
            // A PS4 game keeps a param.sfo in sce_sys, as `sfo_path` says.
            [root, sfo] if is(sfo, "PARAM.SFO") && !is(root, "sce_sys") => (*root, false),
            _ => continue,
        };
        found.push((root, disc, name));
    }

    // The top is looked at first, as `find_dump_root` does.
    let at_top = found.iter().any(|(root, ..)| root.is_empty());
    found.retain(|(root, ..)| !at_top || root.is_empty());
    let first = found.first()?.0;
    if found.iter().any(|(root, ..)| *root != first) {
        return None;
    }
    found
        .iter()
        .find(|(_, disc, _)| *disc)
        .or(found.first())
        .map(|(_, _, name)| *name)
}

/// Which game an archive holds, from its PARAM.SFO, before anything is
/// unpacked. `read` gives one file inside by its name.
pub fn identify_packed(names: &[String], read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Option<Imported> {
    let (title_id, title, _) = read_sfo(&read(packed_sfo(names)?)?).ok()?;
    Some(Imported { console: Console::Ps3, title_id, title })
}

/// Sums what it can and ignores what it cannot read, because a size shown in
/// the interface is never worth failing an import over.
pub(crate) fn directory_size(root: &Path) -> u64 {
    let mut total = 0;
    // Dumps nest past Windows' 260-character path limit, and this walk would
    // quietly skip whatever sat beyond it and report a size that was too small.
    let mut stack = vec![crate::archive::extended_length(root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
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

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("omoio-import-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, relative: &str, bytes: &[u8]) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A PARAM.SFO with null-terminated text entries, matching real dumps.
    fn sfo(entries: &[(&str, &str)]) -> Vec<u8> {
        const HEADER: usize = 20;
        let (mut keys, mut data, mut index) = (Vec::new(), Vec::new(), Vec::new());
        for (key, value) in entries {
            let key_offset = keys.len();
            keys.extend_from_slice(key.as_bytes());
            keys.push(0);

            let data_offset = data.len();
            data.extend_from_slice(value.as_bytes());
            data.push(0);
            let len = value.len() + 1;

            index.extend_from_slice(&(key_offset as u16).to_le_bytes());
            index.extend_from_slice(&0x0204u16.to_le_bytes());
            index.extend_from_slice(&(len as u32).to_le_bytes());
            index.extend_from_slice(&(len as u32).to_le_bytes());
            index.extend_from_slice(&(data_offset as u32).to_le_bytes());
        }
        while keys.len() % 4 != 0 {
            keys.push(0);
        }

        let key_table = HEADER + index.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"\0PSF");
        out.extend_from_slice(&0x0101u32.to_le_bytes());
        out.extend_from_slice(&(key_table as u32).to_le_bytes());
        out.extend_from_slice(&((key_table + keys.len()) as u32).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        out.extend_from_slice(&index);
        out.extend_from_slice(&keys);
        out.extend_from_slice(&data);
        out
    }

    fn disc_game() -> Vec<(&'static str, &'static str)> {
        vec![
            ("CATEGORY", "DG"),
            ("TITLE", "LittleBigPlanet"),
            ("TITLE_ID", "BCES00141"),
            ("VERSION", "01.01"),
        ]
    }

    #[test]
    fn imports_a_disc_dump() {
        let dir = TempDir::new("disc");
        dir.write("PS3_DISC.SFB", &[0; 1536]);
        dir.write("PS3_GAME/PARAM.SFO", &sfo(&disc_game()));
        dir.write("PS3_GAME/USRDIR/EBOOT.BIN", &[0; 2048]);

        let game = identify(&dir.0).unwrap();
        assert_eq!(game.title_id, "BCES00141");
        assert_eq!(game.title, "LittleBigPlanet");
        assert_eq!(game.version.as_deref(), Some("01.01"));
        assert_eq!(game.path, dir.0);
        assert_eq!(game.size_bytes, 1536 + 2048 + sfo(&disc_game()).len() as u64);
    }

    #[test]
    fn imports_an_installed_psn_title() {
        let dir = TempDir::new("hdd");
        dir.write(
            "PARAM.SFO",
            &sfo(&[
                ("CATEGORY", "HG"),
                ("TITLE", "Sackboy's Prehistoric Moves"),
                ("TITLE_ID", "NPEA00243"),
                ("APP_VER", "01.00"),
            ]),
        );
        dir.write("USRDIR/EBOOT.BIN", &[0; 16]);

        let game = identify(&dir.0).unwrap();
        assert_eq!(game.title_id, "NPEA00243");
        assert_eq!(game.version.as_deref(), Some("01.00"));
    }

    #[test]
    fn looks_one_level_down_for_the_dump() {
        // What extracting an archive normally leaves behind.
        let dir = TempDir::new("nested");
        dir.write("LittleBigPlanet (Europe)/PS3_DISC.SFB", &[0; 16]);
        dir.write("LittleBigPlanet (Europe)/PS3_GAME/PARAM.SFO", &sfo(&disc_game()));

        let game = identify(&dir.0).unwrap();
        assert_eq!(game.title_id, "BCES00141");
        assert_eq!(game.path, dir.0.join("LittleBigPlanet (Europe)"));
    }

    #[test]
    fn refuses_to_guess_between_two_nested_dumps() {
        let dir = TempDir::new("ambiguous");
        dir.write("one/PS3_GAME/PARAM.SFO", &sfo(&disc_game()));
        dir.write("two/PS3_GAME/PARAM.SFO", &sfo(&disc_game()));
        assert_eq!(identify(&dir.0).unwrap_err(), Error::NotAGameDump);
    }

    #[test]
    fn finds_the_tile_art_in_both_dump_shapes() {
        let dir = TempDir::new("icons");
        dir.write("disc/PS3_GAME/ICON0.PNG", b"png");
        dir.write("hdd/ICON0.PNG", b"png");

        assert_eq!(
            icon_path(&dir.0.join("disc")),
            Some(dir.0.join("disc").join("PS3_GAME").join("ICON0.PNG"))
        );
        assert_eq!(
            icon_path(&dir.0.join("hdd")),
            Some(dir.0.join("hdd").join("ICON0.PNG"))
        );
        assert_eq!(icon_path(&dir.0.join("nothing")), None);
    }

    #[test]
    fn tolerates_extra_files_beside_the_dump() {
        let dir = TempDir::new("extras");
        dir.write("PS3_DISC.SFB", &[0; 16]);
        dir.write("PS3_GAME/PARAM.SFO", &sfo(&disc_game()));
        dir.write("PS3_EXTRA/PARAM.SFO", &sfo(&disc_game()));
        dir.write("notes.txt", b"whatever");

        assert_eq!(identify(&dir.0).unwrap().title_id, "BCES00141");
    }

    #[test]
    fn rejects_an_update_rather_than_listing_it_as_a_game() {
        let dir = TempDir::new("update");
        dir.write(
            "PARAM.SFO",
            &sfo(&[
                ("CATEGORY", "GD"),
                ("TITLE", "LittleBigPlanet"),
                ("TITLE_ID", "BCES00141"),
                ("APP_VER", "02.00"),
            ]),
        );
        assert_eq!(identify(&dir.0).unwrap_err(), Error::IsAnUpdate);
    }

    #[test]
    fn rejects_a_dump_with_no_title_id() {
        let dir = TempDir::new("no-id");
        dir.write("PARAM.SFO", &sfo(&[("CATEGORY", "HG"), ("TITLE", "Save data")]));
        assert_eq!(identify(&dir.0).unwrap_err(), Error::NoTitleId);
    }

    #[test]
    fn rejects_a_folder_that_is_not_a_game() {
        let dir = TempDir::new("empty");
        dir.write("holiday.jpg", &[0; 8]);
        assert_eq!(identify(&dir.0).unwrap_err(), Error::NotAGameDump);
        assert_eq!(identify(Path::new("no-such-folder")).unwrap_err(), Error::NotAGameDump);
    }

    /// Checks identify() against a real dump, which is the only thing that
    /// proves it. Game files are not committed, so point this at one to run it:
    ///   set OMOIO_DUMP=D:\dumps\some-game
    ///   cargo test real_dump -- --ignored --nocapture
    #[test]
    #[ignore = "needs OMOIO_DUMP pointing at a real dump"]
    fn identifies_a_real_dump() {
        let picked = std::env::var("OMOIO_DUMP").expect("set OMOIO_DUMP");
        let game = identify(Path::new(&picked)).expect("should identify the dump");
        println!("picked: {picked}");
        println!("root:   {}", game.path.display());
        println!(
            "game:   {} {} v{} ({:.1} GB)",
            game.title_id,
            game.title,
            game.version.as_deref().unwrap_or("-"),
            game.size_bytes as f64 / 1024f64.powi(3),
        );
        assert!(!game.title_id.is_empty());
        assert!(game.size_bytes > 0);
    }

    #[test]
    fn reports_unreadable_metadata_separately_from_a_wrong_folder() {
        let dir = TempDir::new("bad-sfo");
        dir.write("PARAM.SFO", b"this is not a PARAM.SFO");
        assert_eq!(identify(&dir.0).unwrap_err(), Error::UnreadableMetadata);
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn an_archives_game_is_told_from_its_param_sfo_before_unpacking() {
        let bytes = sfo(&[("CATEGORY", "DG"), ("TITLE", "Skylanders Trap Team"), ("TITLE_ID", "BLES02055")]);
        let wanted = "Skylanders Trap Team (Europe)/PS3_GAME/PARAM.SFO";
        let read = |name: &str| (name == wanted).then(|| bytes.clone());
        let inside = names(&[
            "Skylanders Trap Team (Europe)/PS3_DISC.SFB",
            "Skylanders Trap Team (Europe)/PS3_EXTRA/PARAM.SFO",
            wanted,
            "Skylanders Trap Team (Europe)/PS3_GAME/USRDIR/EBOOT.BIN",
        ]);
        assert_eq!(
            identify_packed(&inside, &read),
            Some(Imported {
                console: Console::Ps3,
                title_id: "BLES02055".to_string(),
                title: "Skylanders Trap Team".to_string(),
            })
        );

        // A PSN title at the top, with Windows separators.
        let read = |name: &str| (name == "PARAM.SFO").then(|| bytes.clone());
        assert!(identify_packed(&names(&["USRDIR\\EBOOT.BIN", "PARAM.SFO"]), &read).is_some());
    }

    #[test]
    fn an_archive_is_not_guessed_at() {
        let game = sfo(&disc_game());
        let update = sfo(&[("CATEGORY", "GD"), ("TITLE", "LittleBigPlanet"), ("TITLE_ID", "BCES00141")]);
        let any = |bytes: &Vec<u8>| {
            let bytes = bytes.clone();
            move |_: &str| Some(bytes.clone())
        };
        let two = names(&["One/PS3_GAME/PARAM.SFO", "Two/PS3_GAME/PARAM.SFO"]);
        assert_eq!(identify_packed(&two, &any(&game)), None, "two dumps");
        assert_eq!(identify_packed(&names(&["PS3_GAME/PARAM.SFO"]), &any(&update)), None, "an update");
        assert_eq!(identify_packed(&names(&["Game/sce_sys/param.sfo"]), &any(&game)), None, "a PS4 game");
        let unreadable = |_: &str| -> Option<Vec<u8>> { None };
        assert_eq!(identify_packed(&names(&["Game/PS3_GAME/PARAM.SFO"]), &unreadable), None, "too deep to read");
    }

    #[test]
    fn a_ps4_game_is_not_taken_for_a_ps3_dump() {
        let dir = TempDir::new("ps4");
        dir.write("eboot.bin", b"x");
        dir.write("sce_sys/param.sfo", &sfo(&[("TITLE_ID", "CUSA02180"), ("CATEGORY", "gd")]));
        assert!(!recognises(&dir.0));
        assert!(!recognises(&dir.0.join("sce_sys")));
    }

    /// Spyro's Adventure as a decrypted disc image.
    fn spyro_image(lic: &[u8]) -> Vec<u8> {
        let sfo = sfo(&[
            ("CATEGORY", "DG"),
            ("TITLE", "Skylanders Spyro's Adventure"),
            ("TITLE_ID", "BLUS30779"),
            ("APP_VER", "01.00"),
        ]);
        let icon = b"\x89PNG\r\n\x1a\nthe tile".to_vec();
        crate::core::ps3_image::tests::disc(&[
            ("PS3_GAME/PARAM.SFO", &sfo),
            ("PS3_GAME/ICON0.PNG", &icon),
            ("PS3_GAME/LICDIR/LIC.DAT", lic),
            ("PS3_GAME/USRDIR/EBOOT.BIN", b"SCE\0\0\0\x02"),
        ])
    }

    #[test]
    fn imports_a_disc_image_where_it_is() {
        let dir = TempDir::new("image");
        dir.write("Spyro.iso", &spyro_image(b"PS3LICDA"));
        let image = dir.0.join("Spyro.iso");
        assert!(recognises(&image));
        let game = identify(&image).unwrap();
        assert_eq!((game.title_id.as_str(), game.title.as_str()), ("BLUS30779", "Skylanders Spyro's Adventure"));
        assert_eq!(game.version.as_deref(), Some("01.00"));
        assert_eq!(game.path, image);
        assert_eq!(game.size_bytes, std::fs::metadata(&image).unwrap().len());
        assert_eq!(icon(&image).unwrap(), b"\x89PNG\r\n\x1a\nthe tile");
    }

    #[test]
    fn finds_the_one_disc_image_an_archive_unpacked() {
        let dir = TempDir::new("image-unpacked");
        dir.write("Spyro (USA)/Spyro (USA).iso", &spyro_image(b"PS3LICDA"));
        dir.write("Spyro (USA)/Spyro (USA).dkey", b"not read");
        dir.write("Spyro (USA)/readme.txt", b"hello");
        assert_eq!(identify(&dir.0).unwrap().path, dir.0.join("Spyro (USA)").join("Spyro (USA).iso"));
        // Two images are not clear, so neither is taken.
        dir.write("Another.iso", &spyro_image(b"PS3LICDA"));
        assert!(!recognises(&dir.0));
    }

    #[test]
    fn an_iso_that_isnt_a_ps3_disc_is_left_to_the_others() {
        let dir = TempDir::new("image-other");
        dir.write("wii.iso", &[0u8; 64 * 1024]);
        dir.write("pc.iso", &crate::core::iso9660::tests::image(&[("SETUP.EXE", b"MZ")]));
        assert!(!recognises(&dir.0.join("wii.iso")));
        assert!(!recognises(&dir.0.join("pc.iso")));
        assert!(!recognises(&dir.0));
    }

    #[test]
    fn an_encrypted_image_is_taken_only_with_its_key_file_beside_it() {
        let dir = TempDir::new("image-encrypted");
        dir.write("Spyro.iso", &spyro_image(b"\x9a\x13 scrambled"));
        let image = dir.0.join("Spyro.iso");
        assert!(recognises(&image), "it is a PS3 disc all the same");
        let refused = identify(&image).unwrap_err();
        assert_eq!(refused, Error::EncryptedImage);
        assert_eq!(
            refused.to_string(),
            "This disc image is encrypted. RPCS3 needs the key file that came with the dump, named like the image and ending .dkey, in the same folder."
        );
        dir.write("Spyro.dkey", b"never opened by Omoio");
        assert_eq!(identify(&image).unwrap().title_id, "BLUS30779");
        std::fs::remove_file(dir.0.join("Spyro.dkey")).unwrap();
        dir.write("Spyro.key", b"never opened by Omoio");
        assert!(identify(&image).is_ok(), "RPCS3 takes .key too");
        std::fs::remove_file(dir.0.join("Spyro.key")).unwrap();
        dir.write("Other.dkey", b"another disc's");
        assert_eq!(identify(&image).unwrap_err(), Error::EncryptedImage, "named like another image");
    }

    #[test]
    fn a_3k3y_image_is_refused_without_asking_for_a_key_file_it_never_had() {
        let dir = TempDir::new("image-3k3y");
        let mut bytes = spyro_image(b"\x9a\x13 scrambled");
        bytes[0xF70..0xF80].copy_from_slice(b"Encrypted 3K BLD");
        dir.write("Spyro.iso", &bytes);
        let image = dir.0.join("Spyro.iso");
        assert_eq!(identify(&image).unwrap_err(), Error::EncryptedBy3k3y);
        // Its key file beside it serves as well, as RPCS3 tries that first.
        dir.write("Spyro.dkey", b"never opened by Omoio");
        assert!(identify(&image).is_ok());
    }

    #[test]
    fn a_folder_named_like_an_image_is_still_a_folder() {
        let dir = TempDir::new("image-folder");
        assert!(is_image(Path::new("E:\\Unplugged\\Spyro.iso")), "told by its name when it isn't there");
        std::fs::create_dir_all(dir.0.join("Game.iso")).unwrap();
        assert!(!is_image(&dir.0.join("Game.iso")));
    }
}

/// How deep to look for games under a chosen folder.
///
/// A games drive is folders of games, sometimes grouped a level or two further
/// by region or letter. It is not an arbitrary tree, and walking a whole
/// multi-terabyte drive to the bottom to find PARAM.SFO files would take
/// minutes and turn up nothing extra.
const SCAN_DEPTH: usize = 4;

/// Every dump under a folder, found by walking rather than by asking the user
/// to point at each one: PS3 dumps, and files an emulator takes as a whole
/// game, such as a Wii U .wua.
///
/// A folder that is itself a dump is not descended into: a game contains no
/// other game, and `PS3_GAME` sitting inside would otherwise be offered twice.
pub fn find_dumps(
    root: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    found: &mut dyn FnMut(&Path),
) -> Vec<PathBuf> {
    let mut dumps = Vec::new();
    // Depth alongside each folder, so the walk stays iterative.
    let mut stack = vec![(crate::archive::extended_length(root), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        if sfo_path(&dir).is_some() {
            found(&dir);
            dumps.push(dir);
            continue;
        }
        if depth >= SCAN_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push((entry.path(), depth + 1)),
                Ok(kind) if kind.is_file() && crate::backends::is_game_file(&entry.file_name().to_string_lossy()) => {
                    found(&entry.path());
                    dumps.push(entry.path());
                }
                _ => {}
            }
        }
    }

    // read_dir gives no order worth relying on, and a list that reshuffles
    // between scans reads as though something changed.
    dumps.sort();
    dumps.into_iter().map(plain).collect()
}

/// Undoes the extended-length prefix.
///
/// The walk needs it to see past Windows' 260-character limit, but every path
/// found underneath inherits it, and `\\?\C:\...` would then be what we store
/// in the library, hand to the emulator, and show on screen.
fn plain(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|p| p.strip_prefix(r"\\?\")) {
        Some(stripped) => PathBuf::from(stripped),
        None => path,
    }
}

#[cfg(test)]
mod scan_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// A throwaway tree under the system temp folder, removed when dropped so
    /// a failing test does not leave litter behind.
    struct Tree(PathBuf);

    impl Tree {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("omoio-scan-{name}"));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Tree(root)
        }

        /// A folder that looks like a disc dump, as far as the walk is
        /// concerned: it is the PARAM.SFO that marks one.
        fn disc_dump(&self, at: &str) -> PathBuf {
            let root = self.0.join(at);
            std::fs::create_dir_all(root.join("PS3_GAME")).unwrap();
            std::fs::write(root.join("PS3_GAME").join("PARAM.SFO"), b"not a real sfo").unwrap();
            root
        }

        fn folder(&self, at: &str) {
            std::fs::create_dir_all(self.0.join(at)).unwrap();
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scan(root: &Path) -> Vec<PathBuf> {
        find_dumps(root, &AtomicBool::new(false), &mut |_| {})
    }

    #[test]
    fn finds_games_sitting_side_by_side() {
        let tree = Tree::new("side-by-side");
        tree.disc_dump("Game A");
        tree.disc_dump("Game B");
        tree.folder("Not a game/random");

        let found = scan(&tree.0);
        assert_eq!(found.len(), 2, "expected both games, got {found:?}");
    }

    #[test]
    fn finds_games_grouped_a_few_folders_deep() {
        // A real drive is often sorted by region or letter.
        let tree = Tree::new("grouped");
        tree.disc_dump("Games/Europe/Game A");
        tree.disc_dump("Games/Japan/Game B");

        assert_eq!(scan(&tree.0).len(), 2);
    }

    #[test]
    fn does_not_look_inside_a_game_it_has_already_found() {
        // PS3_GAME holds its own PARAM.SFO; descending would offer the same
        // game a second time.
        let tree = Tree::new("no-nesting");
        let dump = tree.disc_dump("Game A");
        std::fs::write(dump.join("PARAM.SFO"), b"not a real sfo").unwrap();

        let found = scan(&tree.0);
        assert_eq!(found.len(), 1, "the same game was found twice: {found:?}");
    }

    #[test]
    fn finds_a_wii_u_wua_beside_the_ps3_games() {
        let tree = Tree::new("wua");
        tree.disc_dump("PS3/Game A");
        tree.folder("Wii U/Game C (Europe)");
        std::fs::write(tree.0.join("Wii U").join("Game B.WUA"), b"x").unwrap();
        std::fs::write(tree.0.join("Wii U").join("notes.txt"), b"x").unwrap();
        std::fs::write(tree.0.join("Wii U/Game C (Europe)/Game C.wua"), b"x").unwrap();

        let found = scan(&tree.0);
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(found.iter().any(|path| path.ends_with("Game B.WUA")));
        assert!(found.iter().any(|path| path.ends_with("Game C.wua")));
    }

    #[test]
    fn stops_when_asked_to() {
        let tree = Tree::new("cancelled");
        tree.disc_dump("Game A");
        tree.disc_dump("Game B");

        let cancel = AtomicBool::new(true);
        assert!(find_dumps(&tree.0, &cancel, &mut |_| {}).is_empty());
    }

    #[test]
    fn hands_back_paths_a_person_could_read() {
        // The walk goes through \? to see past the path limit; that must not
        // end up in the library or on screen.
        let tree = Tree::new("plain-paths");
        tree.disc_dump("Game A");

        for found in scan(&tree.0) {
            assert!(
                !found.to_string_lossy().starts_with(r"\?\"),
                "{found:?} still carries the extended-length prefix"
            );
        }
    }

    #[test]
    fn gives_back_the_same_order_every_time() {
        let tree = Tree::new("stable-order");
        tree.disc_dump("Game C");
        tree.disc_dump("Game A");
        tree.disc_dump("Game B");

        assert_eq!(scan(&tree.0), scan(&tree.0));
    }
}
